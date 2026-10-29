//! Diagnostic sidecars, never evidence or budget authority. Timings are nested.
use serde_json::{Value, json};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::Instant;

#[derive(Default)]
struct Data {
    stages: BTreeMap<&'static str, Vec<u64>>,
    counts: BTreeMap<&'static str, u64>,
}
thread_local! { static DATA: RefCell<Option<Data>> = const { RefCell::new(None) }; }

pub(crate) struct Span {
    stage: &'static str,
    start: Option<Instant>,
}
pub(crate) fn span(stage: &'static str) -> Span {
    Span {
        stage,
        start: DATA.with(|d| d.borrow().is_some().then(Instant::now)),
    }
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            DATA.with(|d| {
                if let Some(data) = d.borrow_mut().as_mut() {
                    data.stages
                        .entry(self.stage)
                        .or_default()
                        .push(start.elapsed().as_micros().min(u64::MAX as u128) as u64);
                }
            });
        }
    }
}
pub(crate) fn count(name: &'static str, value: u64) {
    DATA.with(|d| {
        if let Some(data) = d.borrow_mut().as_mut() {
            *data.counts.entry(name).or_default() += value;
        }
    });
}

pub(crate) fn capture<T>(
    root: &Path,
    name: &str,
    f: impl FnOnce() -> super::Result<T>,
) -> super::Result<T> {
    DATA.with(|d| *d.borrow_mut() = Some(Data::default()));
    let started = Instant::now();
    let result = f();
    let data = DATA.with(|d| d.borrow_mut().take()).unwrap_or_default();
    let stages: BTreeMap<_, Value> = data.stages.into_iter().map(|(name, mut samples)| {
        samples.sort_unstable();
        let n = samples.len();
        (name, json!({"calls":n,"total_us":samples.iter().map(|v| *v as u128).sum::<u128>(),
            "p50_us":samples[(n-1)/2],"p95_us":samples[((n*95).div_ceil(100)-1).min(n-1)],"max_us":samples[n-1]}))
    }).collect();
    let report = json!({"schema":1,"kind":"engineering_diagnostic","complete":result.is_ok(),
        "wall_ms":started.elapsed().as_millis(),"timings":"inclusive nested spans; do not sum or charge to budgets",
        "stages":stages,"counts":data.counts});
    crate::store::save(
        &root.join(name),
        &serde_json::to_vec(&report).map_err(|e| e.to_string())?,
    )?;
    result
}
