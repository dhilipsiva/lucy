use super::*;
use crate::store::Store;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    task: String,
}

pub fn command(args: &[String], stdin: &str) -> Result<Value> {
    let op = args.first().map(String::as_str).unwrap_or("status");
    let (mut root, mut workspace, mut use_stdin) = (None, None, false);
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--root" | "--workspace" => {
                let option = &args[i];
                i += 1;
                let value = PathBuf::from(args.get(i).ok_or("directory option requires a value")?);
                if option == "--root" {
                    root = Some(value);
                } else {
                    workspace = Some(value);
                }
            }
            "--stdin" if !use_stdin => use_stdin = true,
            _ => return Err("unknown task option".into()),
        }
        i += 1;
    }
    if op == "study-info" {
        #[cfg(unix)]
        return Ok(
            json!({"schema":2,"protocol":crate::store::digest(PROTOCOL.as_bytes()),"implementation":study::implementation()?,
            "model":crate::desktop::providers::identity()?,"policy":POLICY,"checker":crate::store::digest(SPEC.as_bytes()),"real_data":crate::store::default_root()?}),
        );
        #[cfg(not(unix))]
        return Err("native study execution requires Unix".into());
    }
    if !matches!(
        op,
        "status"
            | "audit"
            | "events"
            | "facts"
            | "begin"
            | "run"
            | "check"
            | "finish"
            | "revise"
            | "policy"
            | "study-score"
            | "study-trial"
    ) {
        return Err("use task begin, run, check, finish, revise, status, audit, events, facts, policy, study-score or study-trial".into());
    }
    if !matches!(op, "status" | "audit" | "events" | "facts") && !use_stdin {
        return Err("task operation requires --stdin JSON".into());
    }
    let root = root.map_or_else(crate::store::default_root, Ok)?;
    let workspace =
        workspace.map_or_else(|| std::env::current_dir().map_err(|e| e.to_string()), Ok)?;
    if op == "study-trial" {
        return study::trial(&root, &workspace, stdin);
    }
    if op == "study-score" {
        return study::score_command(&root, stdin);
    }
    if op == "run" {
        return run(&root, &workspace, stdin);
    }
    let store = Store::open(&root)?;
    match op {
        "begin" => {
            let definition: Definition = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
            Ok(json!(begin(&store, &workspace, definition, Observer::Cli)?))
        }
        "revise" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                task: String,
                definition: Definition,
            }
            let input: Input = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
            own(&store, &input.task)?;
            Ok(json!(revise(
                &store,
                &workspace,
                &input.task,
                input.definition,
                Observer::Cli
            )?))
        }
        "check" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                task: String,
                criterion: String,
            }
            let input: Input = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
            own(&store, &input.task)?;
            Ok(json!(check(
                &store,
                &workspace,
                &input.task,
                &input.criterion
            )?))
        }
        "finish" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Input {
                task: String,
                #[serde(default)]
                interrupted: bool,
            }
            let input: Input = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
            own(&store, &input.task)?;
            Ok(json!(finish(
                &store,
                &workspace,
                &input.task,
                input.interrupted
            )?))
        }
        "policy" => study::policy_command(&store, stdin),
        _ => {
            let history = History::read(&store)?;
            match op {
                "events" => {
                    Ok(json!({"schema":1,"events":history.entries.values().collect::<Vec<_>>()}))
                }
                "facts" => Ok(json!({"facts":history.facts()})),
                "audit" => Ok(json!({"schema":1,"integrity":"checked"})),
                _ => {
                    let target = if use_stdin {
                        Some(serde_json::from_str::<Target>(stdin).map_err(|e| e.to_string())?)
                    } else {
                        None
                    };
                    history.status(target.as_ref().map(|t| t.task.as_str()))
                }
            }
        }
    }
}

fn own(store: &Store, task: &str) -> Result<()> {
    if History::read(store)?.task(task)?.scope.observer != Observer::Cli {
        return Err("CLI cannot resolve or execute a native task".into());
    }
    Ok(())
}

fn run(root: &std::path::Path, workspace: &std::path::Path, stdin: &str) -> Result<Value> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        task: String,
        argv: Vec<String>,
    }
    let input: Input = serde_json::from_str(stdin).map_err(|e| e.to_string())?;
    if input.argv.is_empty()
        || input.argv.len() > 128
        || input
            .argv
            .iter()
            .any(|a| a.len() > 4096 || a.contains('\0'))
    {
        return Err("run requires bounded explicit argv".into());
    }
    #[cfg(not(unix))]
    {
        let _ = (root, workspace, input);
        Err("task execution requires Unix; inspection remains available".into())
    }
    #[cfg(unix)]
    {
        use std::sync::atomic::AtomicBool;
        use std::time::Instant;
        let (action, remaining) = {
            let store = Store::open(root)?;
            own(&store, &input.task)?;
            let history = History::read(&store)?;
            checks::same_workspace(history.task(&input.task)?, workspace)?;
            let remaining = history.available(&input.task)?;
            let action = reserve_action(
                &store,
                &input.task,
                "host_command",
                &json!({"argv":input.argv}),
            )?;
            (action, remaining)
        };
        // The invoking host authorizes this explicit argv. Native tools never
        // enter this host path: they retain their normal sandbox and approvals.
        let started = Instant::now();
        let result = crate::desktop::actions::run_ms(
            workspace,
            &AtomicBool::new(false),
            &input.argv,
            false,
            remaining.min(120_000),
        );
        let elapsed = started.elapsed().as_millis() as u64;
        let code = result.as_ref().ok().and_then(|v| v["exit_code"].as_i64());
        let outcome = match code {
            Some(0) => Verdict::Success,
            Some(_) => Verdict::Failure,
            None => Verdict::Inconclusive,
        };
        let saved = Store::open(root).and_then(|store| {
            complete_action(&store, &input.task, &action, outcome, elapsed, code)
        });
        Ok(
            json!({"schema":1,"execution":match result {Ok(v)=>v,Err(e)=>json!({"error":e})},
            "receipt":saved.as_ref().ok(),"recording_error":saved.err(),
            "meaning":"Execution result only. Check the task criteria separately; never rerun solely to repair memory."}),
        )
    }
}
