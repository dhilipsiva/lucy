//! Reuse only completed Nibli checks. Callers still read records, compare the
//! Rust graph, and validate every receipt before publishing a success marker.
use crate::task_model::profile;
use nibli_engine::NibliEngine;
use nibli_types::logic::QueryResult;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

// Activation requires the preregistered correctness AND performance gates.
const DEFAULT_ENABLED: bool = false;
const CAPACITY: usize = 64;
const QUERY: &str = "sl_invalid(History).";
type Key = [u8; 32];
type Cache = Arc<Mutex<VecDeque<Key>>>;
type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Copy)]
pub(crate) enum Domain {
    Task,
    Legacy,
    Grouped,
}
impl Domain {
    fn label(self) -> &'static str {
        match self {
            Self::Task => "task evidence check",
            Self::Legacy => "self-model evidence check",
            Self::Grouped => "v2 evidence check",
        }
    }
}

// These are the pinned engine's existing defaults, now explicit and keyed.
// The engine implementation/pin is fixed for this process; no result persists.
#[derive(Clone, Copy)]
struct Settings {
    depth: u32,
    materialization: bool,
    strict: bool,
    existential_import: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            depth: 10,
            materialization: true,
            strict: false,
            existential_import: false,
        }
    }
}
fn fingerprint(domain: Domain, spec: &str, facts: &str, query: &str, settings: Settings) -> Key {
    let mut hash = Sha256::new();
    for part in [
        domain.label().as_bytes(),
        spec.as_bytes(),
        facts.as_bytes(),
        query.as_bytes(),
        &settings.depth.to_le_bytes(),
        &[
            settings.materialization as u8,
            settings.strict as u8,
            settings.existential_import as u8,
        ],
    ] {
        hash.update((part.len() as u64).to_le_bytes());
        hash.update(part);
    }
    hash.finalize().into()
}

#[derive(Clone)]
struct Context {
    enabled: bool,
    cache: Cache,
}
fn context() -> Context {
    #[cfg(test)]
    if let Some(context) = TEST_CONTEXT.with(|c| c.borrow().clone()) {
        return context;
    }
    static CACHE: OnceLock<Cache> = OnceLock::new();
    Context {
        enabled: match std::env::var("LUCY_AUDIT_CACHE").as_deref() {
            Ok("0") => false,
            Ok("1") => true,
            _ => DEFAULT_ENABLED,
        },
        cache: CACHE
            .get_or_init(|| Arc::new(Mutex::new(VecDeque::new())))
            .clone(),
    }
}

/// A successful logical check is only a candidate for reuse until the caller
/// has also completed every Rust check. Dropping this token publishes nothing.
pub(crate) struct Checked {
    pub invalid: bool,
    pending: Option<(Cache, Key)>,
}
impl Checked {
    pub fn remember(self) {
        if let Some((cache, key)) = self.pending
            && let Ok(mut keys) = cache.try_lock()
            && !keys.contains(&key)
        {
            if keys.len() == CAPACITY {
                keys.pop_front();
                profile::count("audit_cache_evictions", 1);
            }
            keys.push_back(key);
        }
    }
}

pub(crate) fn check(domain: Domain, spec: &str, facts: &str) -> Result<Checked> {
    check_with(context(), domain, spec, facts, Settings::default())
}
fn check_with(
    context: Context,
    domain: Domain,
    spec: &str,
    facts: &str,
    settings: Settings,
) -> Result<Checked> {
    let key = context.enabled.then(|| {
        let _span = profile::span("audit_fingerprint");
        fingerprint(domain, spec, facts, QUERY, settings)
    });
    if let Some(key) = key {
        // Contention/poisoning falls back to full validation. No lock is held
        // across Nibli evaluation or caller-side Rust validation.
        if let Ok(mut keys) = context.cache.try_lock()
            && let Some(index) = keys.iter().position(|k| *k == key)
        {
            keys.remove(index);
            keys.push_back(key);
            profile::count("audit_cache_hits", 1);
            return Ok(Checked {
                invalid: false,
                pending: None,
            });
        }
        profile::count("audit_cache_misses", 1);
    } else {
        profile::count("audit_cache_bypasses", 1);
    }
    let _span = profile::span("nibli_audit");
    profile::count("nibli_evaluations", 1);
    let engine = NibliEngine::new();
    engine
        .set_max_chain_depth(settings.depth)
        .map_err(|e| e.to_string())?;
    engine.set_materialization(settings.materialization);
    engine.set_strict(settings.strict);
    engine
        .set_existential_import(settings.existential_import)
        .map_err(|e| e.to_string())?;
    engine
        .assert_text(&format!("{spec}\n{facts}"))
        .map_err(|e| e.to_string())?;
    let result = engine.query_holds(QUERY).map_err(|e| e.to_string())?;
    let (complete, refused) = engine.materialization_report().map_err(|e| e.to_string())?;
    checked_result(
        domain,
        result,
        &complete,
        &refused,
        key.map(|key| (context.cache, key)),
    )
}

fn checked_result(
    domain: Domain,
    result: QueryResult,
    complete: &[String],
    refused: &[(String, String)],
    pending: Option<(Cache, Key)>,
) -> Result<Checked> {
    if !complete.iter().any(|r| r == "sl_invalid") {
        return Err(match domain {
            Domain::Legacy => format!("{} incomplete: {refused:?}", domain.label()),
            _ => format!("{} incomplete", domain.label()),
        });
    }
    let invalid = match result {
        QueryResult::True => true,
        QueryResult::False => false,
        other => {
            return Err(match domain {
                Domain::Legacy => format!("{} inconclusive: {other:?}", domain.label()),
                _ => format!("{} inconclusive", domain.label()),
            });
        }
    };
    Ok(Checked {
        invalid,
        pending: pending.filter(|_| !invalid),
    })
}

#[cfg(test)]
thread_local! {
    static TEST_CONTEXT: std::cell::RefCell<Option<Context>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn with_test_cache<T>(enabled: bool, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<Context>);
    impl Drop for Restore {
        fn drop(&mut self) {
            TEST_CONTEXT.with(|c| *c.borrow_mut() = self.0.take());
        }
    }
    let _restore = Restore(TEST_CONTEXT.with(|c| {
        c.replace(Some(Context {
            enabled,
            cache: Arc::new(Mutex::new(VecDeque::new())),
        }))
    }));
    f()
}

#[cfg(test)]
pub(crate) mod tests;
