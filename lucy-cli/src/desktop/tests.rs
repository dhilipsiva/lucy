use super::{Service, actions, core, net, providers, read_frame};
use crate::store::Store;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

fn request(client: &Value, values: Value) -> Value {
    let mut output = client.clone();
    output
        .as_object_mut()
        .unwrap()
        .extend(values.as_object().unwrap().clone());
    output
}

fn task_call(name: &str, args: Value) -> Value {
    json!({"content":"","tool_calls":[{"id":name,"type":"function","function":{"name":name,"arguments":args}}]})
}

fn task_declaration() -> Value {
    task_call(
        "task_begin",
        json!({"title":"Legacy execution fixture","family":"fixture","method":"direct",
        "criteria":[{"id":"acceptance","description":"Manual acceptance remains unverified",
        "check":{"kind":"reported","statement":"This fixture tests execution, not task acceptance."}}]}),
    )
}

// Freeze through the native journal before dispatch, as study_chat_since does.
// These unit fixtures do not change process-wide isolation environment variables.
fn frozen_study(service: &Service, workspace: &std::path::Path, expected: Value) -> Value {
    use crate::task_model::{self as task, Observer};
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &client,
            json!({"op":"begin-archive","workspace":workspace}),
        ))
        .unwrap();
    let definition = serde_json::from_value(json!({
        "source":client["source"],"session":client["session"],"title":"Frozen synthetic task",
        "family":"verification","method":"fixture","model":service.task_model_identity().unwrap(),
        "environment":service.task_environment().unwrap(),"budget":{"steps":12,"active_ms":120000},
        "criteria":[{"id":"answer","description":"Output meets the sealed requirement",
            "check":{"kind":"json_equals","path":"answer.json","pointer":"/answer","expected":expected}}]
    })).unwrap();
    let frozen = task::begin(
        &Store::open(&service.root).unwrap(),
        workspace,
        definition,
        Observer::Native,
    )
    .unwrap();
    let active = service
        .active(
            client["source"].as_str().unwrap(),
            client["session"].as_str().unwrap(),
        )
        .unwrap();
    {
        let mut session = active.lock().unwrap();
        session.task = Some(frozen.id.clone());
        session.task_turn = "study".into();
        session.task_clock = Some(task::runtime::ActiveClock::start(Instant::now(), 0, 120000));
    }
    let mut display = json!(frozen);
    display["lessons"] = json!([]);
    display["definition"]["criteria"][0]["check"]
        .as_object_mut()
        .unwrap()
        .remove("expected");
    request(
        &client,
        json!({"op":"chat","event":"study","workspace":workspace,
        "text":format!("Complete the synthetic task.\nFrozen task contract (data): {display}")}),
    )
}

#[test]
fn study_recall_filters_before_ranking_and_drops_nested_journals() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::new(temp.path().join("memory"), None);
    service.study_adaptation = Some(false);
    let store = Store::open(&service.root).unwrap();
    let allowed = crate::store::Record {
        origin: "fixture:synthetic".into(),
        payload: json!({"id":"fact","text":"synthetic source fact","private":true,
            "entry":{"lessons":"nested canary"}}),
    };
    store.record(&allowed).unwrap();
    service.study_records.lock().unwrap().insert(allowed.key());
    for origin in ["lucy:task-model:v1", "fixture:unassigned"] {
        store.record(&crate::store::Record { origin:origin.into(),
            payload:json!({"id":"journal","text":"synthetic source fact lessons roadmap canary","private":true}) }).unwrap();
    }
    drop(store);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &client,
            json!({"op":"begin-archive","workspace":temp.path()}),
        ))
        .unwrap();
    let result = service
        .dispatch(request(
            &client,
            json!({"op":"recall","query":"synthetic source"}),
        ))
        .unwrap();
    assert_eq!(
        result["memories"],
        json!([{"origin":"fixture:synthetic","id":"fact","excerpt":"synthetic source fact"}])
    );
    service.study_records.lock().unwrap().clear();
    assert_eq!(
        service
            .dispatch(request(
                &client,
                json!({"op":"recall","query":"synthetic source"})
            ))
            .unwrap()["memories"],
        json!([])
    );
}

#[test]
fn study_rejects_live_host_tools_before_execution() {
    for tool in ["diagnostics", "window", "service", "http", "git"] {
        let temp = tempfile::tempdir().unwrap();
        let mut service = Service::new(temp.path().join("memory"), None);
        service.study_adaptation = Some(false);
        service.scripted(vec![task_call(tool, json!({}))]);
        let error = service
            .dispatch(frozen_study(&service, temp.path(), json!(42)))
            .unwrap_err();
        assert!(error.contains("limited to the synthetic workspace"));
        assert!(
            crate::self_model::grouped::History::read(&Store::open(&service.root).unwrap())
                .unwrap()
                .predictions
                .is_empty()
        );
    }
}

#[test]
fn native_task_clock_pauses_for_approval_and_rejects_an_expired_write() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_declaration(),
        task_call(
            "write",
            json!({"path":"late.txt","text":"must not execute"}),
        ),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let response = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"synthetic approval clock",
        "event":"clock","workspace":temp.path()}),
        ))
        .unwrap();
    let id = response["pending"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let session = service
        .active(
            client["source"].as_str().unwrap(),
            client["session"].as_str().unwrap(),
        )
        .unwrap();
    let elapsed = session
        .lock()
        .unwrap()
        .task_clock
        .as_ref()
        .unwrap()
        .elapsed_ms();
    std::thread::sleep(std::time::Duration::from_millis(15));
    assert_eq!(
        session
            .lock()
            .unwrap()
            .task_clock
            .as_ref()
            .unwrap()
            .elapsed_ms(),
        elapsed
    );
    let mut expired = crate::task_model::runtime::ActiveClock::start(
        std::time::Instant::now() - std::time::Duration::from_millis(101),
        0,
        100,
    );
    expired.pause();
    session.lock().unwrap().task_clock = Some(expired);
    let error = service
        .dispatch(request(
            &client,
            json!({"op":"approve","action":id,"allow":true}),
        ))
        .unwrap_err();
    assert!(error.contains("budget exhausted"), "{error}");
    assert!(!temp.path().join("late.txt").exists());
}

#[test]
fn study_rejects_an_oversized_task_before_it_can_disappear_from_model_context() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::new(temp.path().join("memory"), None);
    service.study_adaptation = Some(false);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let error = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"x".repeat(18001),
        "event":"fixture","workspace":temp.path()}),
        ))
        .unwrap_err();
    assert!(error.contains("exceed native input window"));
    assert!(!service.root.join("visible-trace.json").exists());
}

#[test]
fn study_hides_sealed_expectations_but_checks_them_and_suppresses_execution_forecasts() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::new(temp.path().join("memory"), None);
    service.study_adaptation = Some(false);
    service.scripted(vec![
        task_call("read", json!({"path":"missing.txt"})),
        task_call("read", json!({"path":"missing.txt"})),
        task_call("read", json!({"path":"missing.txt"})),
        task_call("task_status", json!({})),
        task_call("task_check", json!({"criterion":"answer"})),
        task_call("task_finish", json!({})),
        json!({"content":"Evidence inspected."}),
    ]);
    std::fs::write(temp.path().join("answer.json"), r#"{"answer":"wrong"}"#).unwrap();
    let result = service
        .dispatch(frozen_study(&service, temp.path(), json!("sealed-canary")))
        .unwrap();
    assert_eq!(
        result["task_status"]["tasks"][0]["outcome"]["verdict"],
        "failure"
    );
    let trace: Value =
        serde_json::from_slice(&std::fs::read(service.root.join("visible-trace.json")).unwrap())
            .unwrap();
    assert!(!trace.to_string().contains("sealed-canary"));
    for request in trace
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["kind"] == "request")
    {
        for message in request["value"].as_array().unwrap() {
            if message["role"] == "tool" {
                let content = message["content"].as_str().unwrap();
                assert!(!content.contains("sealed-canary"));
                assert!(!content.contains("InspectFirst"));
                assert!(!content.contains("\"inspection\""));
                assert!(!content.contains("\"basis_count\""));
                assert!(!content.contains("\"reviewed_basis_count\""));
                if message["tool_name"] == "read" {
                    assert!(!content.contains("\"probability_ppm\""));
                    assert!(!content.contains("\"policy\""));
                }
            }
        }
    }
    let store = Store::open(&service.root).unwrap();
    let history = crate::task_model::History::read(&store).unwrap();
    assert!(
        serde_json::to_string(&history.entries)
            .unwrap()
            .contains("sealed-canary")
    );
    assert!(
        crate::self_model::grouped::History::read(&store)
            .unwrap()
            .predictions
            .values()
            .any(|p| p.policy == crate::self_model::Policy::InspectFirst)
    );
}

#[test]
fn study_protocol_offers_only_frozen_task_tools_and_checks_approved_work() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::new(temp.path().join("memory"), None);
    service.study_adaptation = Some(false);
    service.scripted(vec![
        // Providers may supply either an object or a JSON-encoded object.
        task_call(
            "write",
            json!(json!({"path":"answer.json","text":"{\"answer\":42}"}).to_string()),
        ),
        task_call("task_check", json!({"criterion":"answer"})),
        task_call("task_finish", json!({})),
        json!({"content":"Evidence checked."}),
    ]);
    let client = frozen_study(&service, temp.path(), json!(42));
    let pending = service.dispatch(client.clone()).unwrap();
    assert!(!temp.path().join("answer.json").exists());
    let action = pending["pending"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let result = service
        .dispatch(request(
            &client,
            json!({"op":"approve","action":action,"allow":true}),
        ))
        .unwrap();
    assert_eq!(
        result["task_status"]["tasks"][0]["outcome"]["verdict"],
        "success"
    );
    let trace: Vec<Value> =
        serde_json::from_slice(&std::fs::read(service.root.join("visible-trace.json")).unwrap())
            .unwrap();
    assert_eq!(trace.len(), 12);
    let names = [
        "read",
        "list",
        "search",
        "write",
        "command",
        "task_check",
        "task_finish",
        "task_status",
    ];
    let schemas: Vec<_> = actions::schemas()
        .as_array()
        .unwrap()
        .iter()
        .filter(|schema| names.contains(&schema["function"]["name"].as_str().unwrap()))
        .cloned()
        .collect();
    assert_eq!(schemas.len(), names.len());
    for events in trace.chunks_exact(3) {
        assert_eq!(events[0]["kind"], "request");
        let instruction = events[0]["value"][0]["content"].as_str().unwrap();
        assert!(instruction.contains(super::tasks::STUDY_INSTRUCTION));
        assert!(!instruction.contains(super::tasks::INSTRUCTION));
        assert_eq!(events[1], json!({"kind":"tool_schemas","value":schemas}));
        assert_eq!(events[2]["kind"], "reply");
    }
    for name in names {
        assert!(super::tasks::study_tool(name));
    }
    for name in ["task_begin", "task_revise"] {
        assert!(!super::tasks::study_tool(name));
        assert!(
            actions::schemas()
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["function"]["name"] == name)
        );
    }
}

#[test]
fn study_contract_changes_and_invalid_replies_stay_unverified_without_new_retries() {
    let mut batch = task_call(
        "write",
        json!({"path":"answer.json","text":"{\"answer\":42}"}),
    );
    batch["tool_calls"]
        .as_array_mut()
        .unwrap()
        .push(task_call("task_finish", json!({}))["tool_calls"][0].clone());
    for (reply, expected_requests, error) in [
        (task_declaration(), 2, false),
        (
            task_call("task_revise", json!({"previous":"anything"})),
            2,
            false,
        ),
        (
            task_call(
                "task_check",
                json!({"criterion":"answer","verdict":"success"}),
            ),
            2,
            false,
        ),
        (task_call("write", json!("{broken json")), 1, true),
        (
            task_call(
                "write",
                json!({"path":"answer.json","content":"wrong field"}),
            ),
            1,
            true,
        ),
        (batch, 1, true),
        (json!({"content":"Everything passed."}), 1, false),
    ] {
        let temp = tempfile::tempdir().unwrap();
        let mut service = Service::new(temp.path().join("memory"), None);
        service.study_adaptation = Some(false);
        service.scripted(vec![
            reply.clone(),
            json!({"content":"Work remains unverified."}),
        ]);
        let result = service.dispatch(frozen_study(&service, temp.path(), json!(42)));
        assert_eq!(result.is_err(), error, "{reply}: {result:?}");
        assert!(!temp.path().join("answer.json").exists());
        let history =
            crate::task_model::History::read(&Store::open(&service.root).unwrap()).unwrap();
        assert_eq!(
            history
                .entries
                .values()
                .filter(|entry| matches!(entry.event, crate::task_model::Event::Begin { .. }))
                .count(),
            1
        );
        assert!(history.entries.values().all(|entry| !matches!(
            entry.event,
            crate::task_model::Event::ActionStarted { .. }
                | crate::task_model::Event::Finished {
                    verdict: crate::task_model::Verdict::Success,
                    ..
                }
        )));
        let trace: Vec<Value> = serde_json::from_slice(
            &std::fs::read(service.root.join("visible-trace.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            trace.iter().filter(|e| e["kind"] == "request").count(),
            expected_requests
        );
    }
}

#[test]
fn study_write_needs_approval_and_required_checks_even_after_completion_prose() {
    for allow in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let mut service = Service::new(temp.path().join("memory"), None);
        service.study_adaptation = Some(false);
        service.scripted(vec![
            task_call(
                "write",
                json!({"path":"answer.json","text":"{\"answer\":42}"}),
            ),
            json!({"content":"The task is complete."}),
        ]);
        let client = frozen_study(&service, temp.path(), json!(42));
        let pending = service.dispatch(client.clone()).unwrap();
        assert!(!temp.path().join("answer.json").exists());
        let action = pending["pending"]
            .as_object()
            .unwrap()
            .keys()
            .next()
            .unwrap();
        let result = service
            .dispatch(request(
                &client,
                json!({"op":"approve","action":action,"allow":allow}),
            ))
            .unwrap();
        assert_eq!(temp.path().join("answer.json").exists(), allow);
        assert_eq!(
            result["task_status"]["tasks"][0]["outcome"]["verdict"],
            "inconclusive"
        );
    }
}

#[test]
fn native_task_requires_frozen_criteria_and_checks_actual_output_after_approval() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_call("task_begin",json!({"title":"Write requested content","family":"verification","method":"write",
            "criteria":[{"id":"content","description":"The note contains the requested JSON",
                "check":{"kind":"json_equals","path":"result.json","pointer":"/answer","expected":42}}]})),
        task_call("write",json!({"path":"result.json","text":"{\"answer\":42}"})),
        task_call("task_check",json!({"criterion":"content"})),
        task_call("task_finish",json!({})),
        json!({"content":"The requested content is saved and checked."}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let pending=service.dispatch(request(&client,json!({"op":"chat","text":"Write the answer as JSON","event":"task","workspace":temp.path()}))).unwrap();
    assert!(!temp.path().join("result.json").exists());
    assert_eq!(
        pending["task_status"]["tasks"][0]["task"]["definition"]["criteria"][0]["id"],
        "content"
    );
    let id = pending["pending"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap();
    let result = service
        .dispatch(request(
            &client,
            json!({"op":"approve","action":id,"allow":true}),
        ))
        .unwrap();
    assert_eq!(
        result["task_status"]["tasks"][0]["outcome"]["verdict"],
        "success"
    );
    let store = Store::open(&service.root).unwrap();
    let history = crate::task_model::History::read(&store).unwrap();
    let rows = history.rows(
        result["task_status"]["tasks"][0]["task"]["id"]
            .as_str()
            .unwrap(),
    );
    assert!(matches!(
        rows[0].event,
        crate::task_model::Event::Begin { .. }
    ));
    assert_eq!(
        rows.iter()
            .filter(|e| matches!(e.event, crate::task_model::Event::ActionStarted { .. }))
            .count(),
        1
    );
}

#[test]
fn model_completion_prose_and_unregistered_tools_cannot_manufacture_verified_success() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_call("read", json!({"path":"never.txt"})),
        task_declaration(),
        task_call(
            "task_check",
            json!({"criterion":"acceptance","verdict":"success"}),
        ),
        json!({"content":"I say everything passed."}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"Inspect evidence","event":"fake","workspace":temp.path()}),
        ))
        .unwrap();
    assert_eq!(reply["text"], "I say everything passed.");
    assert_eq!(
        reply["task_status"]["tasks"][0]["outcome"]["verdict"],
        "inconclusive"
    );
    let store = Store::open(&service.root).unwrap();
    assert!(
        crate::self_model::grouped::History::read(&store)
            .unwrap()
            .observations
            .is_empty()
    );
}

#[test]
fn native_criteria_cannot_be_replaced_within_a_turn() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_declaration(),
        task_call("task_finish", json!({})),
        task_declaration(),
        json!({"content":"Partial work."}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"Do the task","event":"immutable","workspace":temp.path()}),
        ))
        .unwrap();
    assert_eq!(reply["task_status"]["tasks"].as_array().unwrap().len(), 1);
    let history = crate::task_model::History::read(&Store::open(&service.root).unwrap()).unwrap();
    assert_eq!(
        history
            .entries
            .values()
            .filter(|e| matches!(e.event, crate::task_model::Event::Begin { .. }))
            .count(),
        1
    );
}

#[test]
fn native_clients_display_criteria_before_continuing_without_an_extra_approval() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    std::fs::write(temp.path().join("note.txt"), "a note").unwrap();
    service.scripted(vec![
        task_declaration(),
        task_call("read", json!({"path":"note.txt"})),
        json!({"content":"Read the note."}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let contract=service.dispatch(request(&client,json!({"op":"chat","text":"Read the note","event":"announce","workspace":temp.path(),"announce_task":true}))).unwrap();
    assert!(contract["continue_task"].is_string());
    assert!(
        crate::self_model::grouped::History::read(&Store::open(&service.root).unwrap())
            .unwrap()
            .observations
            .is_empty()
    );
    let continuation = request(
        &client,
        json!({"op":"continue-task","continuation":contract["continue_task"]}),
    );
    let reply = service.dispatch(continuation.clone()).unwrap();
    assert_eq!(reply["text"], "Read the note.");
    assert!(service.dispatch(continuation).is_err());
}

fn prompt(
    service: &Service,
    workspace: &std::path::Path,
    source: &str,
    id: &str,
    text: &str,
) -> Value {
    service
        .dispatch(json!({"op":"prompt","source":source,"session":id,
        "workspace":workspace,"text":text,"event":format!("{id}-{text}")}))
        .unwrap()
}

#[test]
fn opening_and_unaddressed_prompts_do_not_load_memory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    service.dispatch(json!({"op":"open"})).unwrap();
    assert!(!root.exists());
    assert_eq!(
        prompt(&service, temp.path(), "codex", "one", "look at Lucy's file")["active"],
        false
    );
    assert!(!root.exists());
    for text in ["Lucy, wake", "Hey Lucy!", "/lucy hello", "@Lucy: hello"] {
        assert!(core::addressed(text));
    }
    for text in ["I told Lucy", "LucyTools", "/lucy-example", "lucy.rs"] {
        assert!(!core::addressed(text));
    }
}

#[test]
fn room_invitation_is_given_on_wake_and_not_on_ordinary_followups() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    let wake = prompt(&service, temp.path(), "codex", "room-test", "Lucy, wake");
    assert!(
        wake["context"]
            .as_str()
            .unwrap()
            .contains(core::WAKE_INVITATION)
    );
    let followup = prompt(&service, temp.path(), "codex", "room-test", "Tell me more");
    assert!(
        !followup["context"]
            .as_str()
            .unwrap()
            .contains(core::WAKE_INVITATION)
    );
    let again = prompt(
        &service,
        temp.path(),
        "codex",
        "room-test",
        "Lucy, wake again",
    );
    assert!(
        again["context"]
            .as_str()
            .unwrap()
            .contains(core::WAKE_INVITATION)
    );
    assert!(super::adapters::SKILL.contains("https://dhilipsiva.dev/lucy/room/"));
}

#[test]
fn peers_refresh_when_a_session_starts_and_not_on_each_address() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    let wake = prompt(&service, temp.path(), "codex", "sync-test", "Lucy, wake");
    assert!(!wake["sync"].is_null());
    for text in ["Lucy, tell me more", "And then?", "Lucy, wake again"] {
        let followup = prompt(&service, temp.path(), "codex", "sync-test", text);
        assert!(followup["sync"].is_null(), "{text} refreshed peers");
    }
}

fn saved_texts(root: &std::path::Path) -> Vec<String> {
    Store::open(root)
        .unwrap()
        .all()
        .unwrap()
        .into_iter()
        .map(|record| record.payload["text"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_prompt_is_recorded_while_an_earlier_prompt_is_still_reading_memory() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Arc::new(Service::new(root.clone(), None));
    prompt(&service, temp.path(), "codex", "pair", "Lucy, wake");
    // The first prompt stops inside memory retrieval until the test lets it go.
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel::<()>();
    let held = std::sync::Mutex::new(held);
    service.on_recall(move |query| {
        if query.contains("slow") {
            entered.send(()).unwrap();
            held.lock().unwrap().recv().unwrap();
        }
        Ok(())
    });
    let first = {
        let service = service.clone();
        let workspace = temp.path().to_owned();
        std::thread::spawn(move || {
            service.dispatch(json!({"op":"prompt","source":"codex","session":"pair",
                "workspace":workspace,"text":"a slow question","event":"first"}))
        })
    };
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    let second = prompt(&service, temp.path(), "codex", "pair", "a quick question");
    assert_eq!(second["active"], true);
    release.send(()).unwrap();
    // The overtaken prompt says it was replaced; the words of both are saved.
    assert!(first.join().unwrap().unwrap_err().contains("newer prompt"));
    let texts = saved_texts(&root);
    assert!(texts.contains(&"a slow question".to_owned()), "{texts:?}");
    assert!(texts.contains(&"a quick question".to_owned()), "{texts:?}");
}

#[test]
fn a_prompt_is_recorded_even_when_recall_fails() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    service.on_recall(|_| Err("index unavailable".into()));
    let text = "Lucy, remember the brass compass";
    let reply = prompt(&service, temp.path(), "codex", "fail", text);
    assert_eq!(reply["active"], true);
    assert!(reply["memories"].as_array().unwrap().is_empty());
    assert!(
        reply["context"]
            .as_str()
            .unwrap()
            .contains("Memory retrieval failed for this prompt: index unavailable")
    );
    assert_eq!(saved_texts(&root), [text]);
}

#[test]
fn a_busy_conversation_still_records_the_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    prompt(&service, temp.path(), "codex", "busy", "Lucy, wake");
    service
        .active("codex", "busy")
        .unwrap()
        .lock()
        .unwrap()
        .busy = true;
    let refused = service.dispatch(json!({"op":"prompt","source":"codex","session":"busy",
        "workspace":temp.path(),"text":"words that must not be lost","event":"later"}));
    assert!(refused.unwrap_err().contains("already running"));
    assert!(saved_texts(&root).contains(&"words that must not be lost".to_owned()));
}

/// An identity file in the real shape: opening lines, headed articles with prose and
/// rules, then the same direct facts from two memory files.
fn identity_fixture() -> String {
    let mut text = String::from(
        "# Lucy D — constitution\n# I am Lucy D. My memory is these files.\n\n# ─── Conclusions are derived, never written ───\nderived_only(\"person\").\n",
    );
    for article in 1..=11 {
        text.push_str(&format!(
            "\n# ─── Article {article}: title {article} ───\n# explanatory prose {}\nentitled(every person, event {{ act{article}() }}).\n",
            "that a hosted context cannot hold ".repeat(12)
        ));
    }
    for _ in 0..2 {
        text.push_str("\n# Lucy D — memory\n# One statement per line.\nhuman(Dhilipsiva).\ndog(Bucchi). # about: dogs, dhilipsiva\n");
    }
    text
}

/// Long private records that all match one query, as an archive of past conversations does.
fn heron_archive(root: &std::path::Path) {
    let store = Store::open(root).unwrap();
    for number in 0..6 {
        store
            .record(&crate::store::Record {
                origin: "conversation:[\"claude\",\"earlier\"]".into(),
                payload: json!({"id":format!("old-{number}"),"source":"claude","session":"earlier",
                    "role":"user","channel":"user","private":true,
                    "text":format!("heron report {number}: {}", "heron feathers and reeds. ".repeat(400))}),
            })
            .unwrap();
    }
}

fn units(text: &str) -> usize {
    text.encode_utf16().count()
}

#[test]
fn hosted_context_fits_its_cap_and_keeps_the_essential_lines_first() {
    use super::context;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let identity = temp.path().join("identity.nibli");
    std::fs::write(&identity, identity_fixture()).unwrap();
    heron_archive(&root);
    let service = Service::new(root, Some(identity.clone()));
    let wake = prompt(
        &service,
        temp.path(),
        "claude",
        "cap",
        "Lucy, what about the heron feathers?",
    );
    let text = wake["context"].as_str().unwrap();
    assert!(units(text) <= context::HOSTED_UNITS, "{}", units(text));
    // What a hosted app must never lose sits inside the part it is certain to show.
    for essential in [
        core::INSTRUCTION,
        "Lucy conversation: {\"session\":\"cap\",\"source\":\"claude\"}",
        context::CAPTURE,
        core::WAKE_INVITATION,
    ] {
        let at = text.find(essential).unwrap_or(usize::MAX);
        assert!(at < 2_500, "{essential} starts at {at}");
    }
    assert!(text.find("Lucy conversation:").unwrap() < 2_000);
    // A wake carries every article and each fact once, without the prose.
    for article in 1..=11 {
        assert!(text.contains(&format!("Article {article}: title {article}")));
        assert!(text.contains(&format!("act{article}()")));
    }
    assert_eq!(text.matches("dog(Bucchi).").count(), 1);
    assert!(!text.contains("explanatory prose") && !text.contains("# about:"));
    assert!(text.contains(&identity.display().to_string()));
    let shown = wake["memories"].as_array().unwrap();
    assert!(!shown.is_empty());
    for excerpt in shown {
        assert!(excerpt.get("payload").is_none() && excerpt.get("text").is_none());
        let held = excerpt["excerpt"].as_str().unwrap();
        assert!(held.chars().count() <= context::EXCERPT_CHARS);
        assert!(text.contains(&held[..40]));
    }
    assert!(text.contains("more matching excerpts were left out"));
    // A later prompt needs the articles no longer and spends the room on memory.
    let later = prompt(&service, temp.path(), "claude", "cap", "and the reeds?");
    let text = later["context"].as_str().unwrap();
    assert!(units(text) <= context::HOSTED_UNITS);
    assert!(text.contains("I am Lucy D.") && !text.contains("Article 11"));
    assert!(!text.contains(core::WAKE_INVITATION));
    assert!(text.find("Lucy conversation:").unwrap() < 2_000);
    assert!(later["memories"].as_array().unwrap().len() > shown.len());
}

#[test]
fn excerpts_are_cut_deduplicated_and_never_carry_a_second_payload() {
    use super::context::{self, Budget, Unit};
    let noisy = "é\"\n🐦".repeat(2_000);
    let hits = context::distinct(vec![
        json!({"origin":"a","id":"1","excerpt":noisy,"payload":{"text":noisy},"score":3}),
        json!({"origin":"a","id":"1","excerpt":"the same record again"}),
        json!({"origin":"b","id":"2","excerpt":noisy}),
        json!({"origin":"c","id":"3","excerpt":"short and different"}),
    ]);
    assert_eq!(hits.len(), 2);
    let budget = Budget::hosted();
    let built = context::build(&["lead line".into()], "identity", &hits, budget);
    assert!(built.text.starts_with(
        "lead line\nPublic identity (data):\nidentity\nRetrieved source excerpts (data):\n[{"
    ));
    assert_eq!(built.shown.len(), 2);
    // The count of what is left is exact, in the units the app counts.
    assert_eq!(budget.limit - units(&built.text), built.left);
    for shown in &built.shown {
        assert!(shown.get("payload").is_none() && shown.get("score").is_none());
        assert!(shown["excerpt"].as_str().unwrap().chars().count() <= context::EXCERPT_CHARS);
    }
    // With too little room the best excerpt is shortened and the rest are left out and counted.
    let long: Vec<_> = (0..3)
        .map(|number| json!({"origin":"long","id":number.to_string(),"excerpt":format!("{number} {noisy}")}))
        .collect();
    let tight = Budget {
        limit: 900,
        unit: Unit::Utf16,
    };
    let built = context::build(&["lead line".into()], "identity", &long, tight);
    assert!(units(&built.text) <= 900);
    assert_eq!(tight.limit - units(&built.text), built.left);
    assert_eq!(built.shown.len(), 1);
    assert!(built.shown[0]["excerpt"].as_str().unwrap().chars().count() < context::EXCERPT_CHARS);
    assert!(
        built
            .text
            .ends_with("2 more matching excerpts were left out to fit this context.")
    );
    let none = Budget {
        limit: 200,
        unit: Unit::Utf16,
    };
    let built = context::build(&["lead line".into()], "identity", &long, none);
    assert!(built.shown.is_empty() && built.text.contains("[]"));
    assert!(units(&built.text) <= 200);
    assert_eq!(context::clamp("aé🐦b", 4), "aé🐦");
    assert_eq!(context::clamp("aé🐦b", 3), "aé");
}

#[test]
fn the_identity_digest_names_every_article_and_fact_once() {
    use super::context;
    let file = std::path::Path::new("/somewhere/identity.nibli");
    let whole = context::digest(&identity_fixture(), true, Some(file));
    assert!(whole.starts_with("Lucy D — constitution\nI am Lucy D. My memory is these files.\n"));
    assert_eq!(whole.matches("Article ").count(), 11);
    assert_eq!(whole.matches("human(Dhilipsiva).").count(), 1);
    assert_eq!(whole.matches("dog(Bucchi).").count(), 1);
    assert!(!whole.contains("explanatory prose") && !whole.contains('#') && !whole.contains('─'));
    assert!(whole.ends_with("is in /somewhere/identity.nibli."));
    let opening = context::digest(&identity_fixture(), false, Some(file));
    assert_eq!(
        opening,
        "Lucy D — constitution\nI am Lucy D. My memory is these files.\nThe complete text, with the reasons behind each article, is in /somewhere/identity.nibli."
    );
    assert_eq!(context::digest("", true, Some(file)), "");
}

#[test]
fn the_self_model_summary_goes_in_whole_or_not_at_all() {
    use super::context::{self, Budget, Unit};
    let budget = Budget {
        limit: 120,
        unit: Unit::Bytes,
    };
    let mut built = context::build(&["lead".into()], "", &[], budget);
    let before = built.text.clone();
    let too_long = "x".repeat(built.left + 1);
    assert!(!context::append_whole(&mut built, budget, &too_long));
    assert_eq!(built.text, before);
    let fits = "y".repeat(built.left);
    assert!(context::append_whole(&mut built, budget, &fits));
    assert_eq!((built.left, built.text.len()), (0, 120));
}

#[test]
fn local_chat_context_fits_its_budget_with_identity_whole() {
    use super::context;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let identity = temp.path().join("identity.nibli");
    std::fs::write(&identity, identity_fixture()).unwrap();
    heron_archive(&root);
    let service = Service::new(root, Some(identity));
    service.scripted(vec![json!({"content":"Herons keep their feathers dry."})]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let question = "Tell me about heron feathers";
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":question,"event":"ask","workspace":temp.path()}),
        ))
        .unwrap();
    assert_eq!(reply["text"], "Herons keep their feathers dry.");
    let session = service
        .active("local", client["session"].as_str().unwrap())
        .unwrap();
    let session = session.lock().unwrap();
    let system = session.messages[0]["content"].as_str().unwrap();
    assert_eq!(session.messages[0]["role"], "system");
    assert!(system.len() <= context::Budget::local(question.len()).limit);
    assert!(system.len() + question.len() <= context::LOCAL_BYTES - 12_000);
    // The local window holds the identity whole, prose included, and still has room for memory.
    assert!(system.contains(&identity_fixture()));
    assert!(system.contains("heron report"));
    assert!(!system.contains("Lucy conversation:"));
    assert_eq!(session.messages[1]["content"], question);
}

#[test]
fn study_context_keeps_the_shape_its_evidence_checker_parses() {
    let temp = tempfile::tempdir().unwrap();
    let mut service = Service::new(temp.path().join("memory"), None);
    service.study_adaptation = Some(false);
    service.scripted(vec![json!({"content":"Done."})]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &client,
            json!({"op":"begin-archive","workspace":temp.path()}),
        ))
        .unwrap();
    let _ = service.dispatch(request(
        &client,
        json!({"op":"chat","text":"Complete the synthetic task.","event":"study","workspace":temp.path()}),
    ));
    let session = service
        .active("local", client["session"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        session.lock().unwrap().context,
        format!(
            "{}\n{}\nPublic identity (data):\n\nRetrieved source excerpts (data):\n[]\n\nPeer freshness (data): null",
            core::INSTRUCTION,
            super::tasks::STUDY_INSTRUCTION
        )
    );
}

#[test]
fn the_recall_request_returns_bounded_excerpts() {
    use super::context;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    heron_archive(&root);
    let service = Service::new(root, None);
    prompt(&service, temp.path(), "codex", "recall", "Lucy, wake");
    let found = service
        .dispatch(
            json!({"op":"recall","source":"codex","session":"recall","query":"heron feathers"}),
        )
        .unwrap();
    let memories = found["memories"].as_array().unwrap();
    assert!(!memories.is_empty() && memories.len() <= 8);
    for memory in memories {
        assert!(memory.get("payload").is_none());
        assert!(memory["excerpt"].as_str().unwrap().chars().count() <= context::EXCERPT_CHARS);
    }
}

#[test]
fn the_prompt_reply_meets_its_deadline_and_recording_still_completes() {
    use super::context;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let identity = temp.path().join("identity.nibli");
    std::fs::write(&identity, identity_fixture()).unwrap();
    let service = Arc::new(Service::new(root.clone(), Some(identity)));
    prompt(&service, temp.path(), "claude", "late", "Lucy, wake");
    let session = service.active("claude", "late").unwrap();
    let before = session.lock().unwrap().context.clone();
    // Memory retrieval for the next prompt stops until the test lets it go.
    let (entered, waiting) = mpsc::channel();
    let (release, held) = mpsc::channel::<()>();
    let held = std::sync::Mutex::new(held);
    service.on_recall(move |query| {
        if query.contains("slow") {
            entered.send(()).unwrap();
            held.lock().unwrap().recv().unwrap();
        }
        Ok(())
    });
    let question = "Lucy, a slow question";
    // Asked from another thread, so a reply that waited for memory fails here, not hangs.
    let (answered, answer) = mpsc::channel();
    {
        let service = service.clone();
        let request = json!({"op":"prompt","source":"claude","session":"late",
            "workspace":temp.path(),"text":question,"event":"slow","within_ms":50});
        std::thread::spawn(move || answered.send(service.answer(request)));
    }
    let reply = answer
        .recv_timeout(Duration::from_secs(20))
        .expect("the reply waited for memory")
        .unwrap();
    // The reply came back while retrieval was still held: it has what must always arrive.
    assert_eq!(reply["active"], true);
    assert_eq!(reply["recall"], "pending");
    assert!(reply["memories"].as_array().unwrap().is_empty());
    let text = reply["context"].as_str().unwrap();
    assert!(units(text) <= context::HOSTED_UNITS);
    for essential in [
        core::INSTRUCTION,
        "Lucy conversation: {\"session\":\"late\",\"source\":\"claude\"}",
        context::CAPTURE,
        core::WAKE_INVITATION,
        "Memory retrieval is still running for this prompt",
        "Article 11: title 11",
    ] {
        assert!(text.contains(essential), "{essential}");
    }
    // The worker saved the words before it reached retrieval.
    waiting.recv_timeout(Duration::from_secs(30)).unwrap();
    assert!(saved_texts(&root).contains(&question.to_owned()));
    // Once retrieval finishes, the full context is there for the conversation's next request.
    release.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(30);
    while session.lock().unwrap().context == before {
        assert!(Instant::now() < deadline, "the late context never arrived");
        std::thread::sleep(Duration::from_millis(10));
    }
    let late = session.lock().unwrap().context.clone();
    assert!(late.contains("Retrieved source excerpts (data):") && !late.contains("still running"));
    // A request that names no deadline waits for memory as before.
    service.on_recall(|_| Ok(()));
    let waited = service
        .answer(json!({"op":"prompt","source":"claude","session":"late",
            "workspace":temp.path(),"text":"and a patient one","event":"patient"}))
        .unwrap();
    assert!(waited["recall"].is_null());
    assert!(!service.answer(json!({"op":"status"})).unwrap()["active"].is_null());
}

#[test]
fn an_overdue_reply_never_wakes_an_inactive_conversation() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Arc::new(Service::new(root.clone(), None));
    for text in ["look at Lucy's file", "Lucy, sleep"] {
        let reply = service
            .answer(json!({"op":"prompt","source":"claude","session":"quiet",
                "workspace":temp.path(),"text":text,"event":text,"within_ms":0}))
            .unwrap();
        assert_eq!(reply, json!({"active":false}), "{text}");
    }
    std::thread::sleep(Duration::from_millis(200));
    assert!(service.active("claude", "quiet").is_err());
    assert!(!root.exists());
}

#[test]
fn active_clients_share_exact_visible_records_and_sleep_is_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    prompt(
        &service,
        temp.path(),
        "codex",
        "one",
        "Lucy, preserve the brass compass.",
    );
    let record = json!({"op":"record","source":"codex","session":"one",
        "event":"reply","channel":"commentary","text":"First line.\nSecond line — intact."});
    service.dispatch(record.clone()).unwrap();
    service.dispatch(record).unwrap();
    let second = prompt(
        &service,
        temp.path(),
        "claude",
        "two",
        "Lucy, recall the brass compass.",
    );
    assert!(second["memories"].as_array().unwrap().iter().any(|m| {
        m["excerpt"]
            .as_str()
            .unwrap_or("")
            .contains("brass compass")
    }));
    service
        .dispatch(json!({"op":"sleep","source":"codex","session":"one"}))
        .unwrap();
    assert!(
        service
            .dispatch(json!({"op":"recall","source":"codex","session":"one","query":"compass"}))
            .is_err()
    );
    assert!(
        service
            .dispatch(json!({"op":"recall","source":"claude","session":"two","query":"compass"}))
            .is_ok()
    );
    let records = Store::open(&root).unwrap().all().unwrap();
    let replies: Vec<_> = records
        .iter()
        .filter(|r| r.payload["role"] == "assistant")
        .collect();
    assert_eq!(replies.len(), 1);
    assert_eq!(
        replies[0].payload["text"],
        "First line.\nSecond line — intact."
    );
    assert_eq!(replies[0].payload["channel"], "commentary");
    assert_eq!(replies[0].payload["private"], true);
}

#[test]
fn legacy_sqlite_payloads_migrate_losslessly_once() {
    let temp = tempfile::tempdir().unwrap();
    let sqlite = temp.path().join("conversations.sqlite3");
    let original = json!({"id":"original","source":"desktop","session":"s","role":"user",
        "text":"Exact\noriginal","timestamp":"2026-09-01T15:43:21.123456+00:00",
        "private":true,"metadata":{"original_id":"provider-42"}});
    {
        let db = rusqlite::Connection::open(&sqlite).unwrap();
        db.execute("CREATE TABLE messages (payload TEXT NOT NULL)", [])
            .unwrap();
        db.execute(
            "INSERT INTO messages (payload) VALUES (?1)",
            [original.to_string()],
        )
        .unwrap();
    }
    let bytes = std::fs::read(&sqlite).unwrap();
    let store = Store::open(temp.path()).unwrap();
    assert_eq!(store.migrate_sqlite().unwrap(), 1);
    assert_eq!(store.migrate_sqlite().unwrap(), 0);
    assert_eq!(store.all().unwrap()[0].payload, original);
    assert_eq!(std::fs::read(sqlite).unwrap(), bytes);
}

#[test]
fn hidden_channels_and_nonvisible_roles_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let store = Store::open(temp.path()).unwrap();
    for channel in ["analysis", "reasoning", "thinking"] {
        assert!(
            store
                .message(json!({"id":channel,"source":"test","session":"one",
            "role":"assistant","channel":channel,"text":"not visible"}))
                .is_err()
        );
    }
    for role in ["system", "developer", "tool"] {
        assert!(
            store
                .message(json!({"id":role,"source":"test","session":"one",
            "role":role,"text":"not a visible exchange"}))
                .is_err()
        );
    }
    assert!(store.all().unwrap().is_empty());
}

#[test]
fn agent_reads_then_returns_visible_response_without_provider_internals() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("note.txt"), "a small note").unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    service.scripted(vec![
        task_declaration(),
        json!({"content":"","tool_calls":[{"id":"call-1","type":"function","function":{"name":"read","arguments":"{\"path\":\"note.txt\"}"}}],"reasoning":"provider internals"}),
        json!({"content":"The note says: a small note.","thinking":"provider internals"}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"Read note.txt",
        "event":"read","workspace":temp.path()}),
        ))
        .unwrap();
    assert_eq!(reply["text"], "The note says: a small note.");
    let records = Store::open(&root).unwrap().all().unwrap();
    assert_eq!(
        records
            .iter()
            .filter(|r| r.origin.starts_with("conversation:"))
            .count(),
        2
    );
    let history = crate::self_model::grouped::History::read(&Store::open(&root).unwrap()).unwrap();
    assert_eq!(history.predictions.len(), 1);
    assert_eq!(history.observations.len(), 1);
    assert!(!json!(records).to_string().contains("provider internals"));
}

#[test]
fn agent_write_needs_the_exact_ui_approval_and_can_only_run_once() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_declaration(),
        json!({"content":"I can write that.","tool_calls":[{"id":"call-1","type":"function",
            "function":{"name":"write","arguments":{"path":"out.txt","text":"approved content"}}}]}),
        json!({"content":"Saved."}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"Write a note",
        "event":"write","workspace":temp.path()}),
        ))
        .unwrap();
    let (id, preview) = reply["pending"].as_object().unwrap().iter().next().unwrap();
    assert!(preview.get("before").is_none());
    assert_eq!(preview["diff"]["after"], "approved content");
    assert!(!temp.path().join("out.txt").exists());
    assert!(
        crate::self_model::grouped::History::read(&Store::open(&service.root).unwrap())
            .unwrap()
            .predictions
            .is_empty()
    );
    let approval = request(&client, json!({"op":"approve","action":id,"allow":true}));
    let mut fake = approval.clone();
    fake["control"] = json!("model-supplied");
    assert!(service.dispatch(fake).is_err());
    assert!(!temp.path().join("out.txt").exists());
    assert_eq!(
        service.dispatch(approval.clone()).unwrap()["text"],
        "Saved."
    );
    assert_eq!(
        std::fs::read_to_string(temp.path().join("out.txt")).unwrap(),
        "approved content"
    );
    assert!(service.dispatch(approval).is_err());
    let history =
        crate::self_model::grouped::History::read(&Store::open(&service.root).unwrap()).unwrap();
    assert_eq!(history.predictions.len(), 1);
    assert_eq!(history.observations.len(), 1);
}

#[test]
fn learned_read_policy_survives_restart_and_changes_inspection() {
    use crate::self_model::grouped::ModelIdentity;
    let planner = || {
        Some(ModelIdentity::Known {
            name: "fixture-planner".into(),
            revision: Some("v1".into()),
        })
    };
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::fs::write(workspace.join("actual.txt"), "available evidence").unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    prompt(
        &service,
        &workspace,
        "codex",
        "first",
        "Lucy, read the note.",
    );
    for _ in 0..2 {
        let error = service
            .propose(
                &service.active("codex", "first").unwrap(),
                "read",
                json!({"path":"missing.txt"}),
                planner(),
            )
            .unwrap_err();
        assert!(!error.contains("actual.txt"));
    }
    drop(service);
    let service = Service::new(root.clone(), None);
    let wake = prompt(&service, &workspace, "codex", "second", "Lucy, continue.");
    assert!(wake["context"].as_str().unwrap().contains("inspect_first"));
    let read = |path: &str| {
        service.propose(
            &service.active("codex", "second").unwrap(),
            "read",
            json!({"path":path}),
            planner(),
        )
    };
    let error = read("missing.txt").unwrap_err();
    // The same failed request now carries an actual directory observation.
    assert!(error.contains("actual.txt"));
    assert!(error.contains("inspect_first"));
    for _ in 0..3 {
        let read = read("actual.txt").unwrap();
        assert_eq!(read["text"], "available evidence");
        assert!(read["self_model"]["inspection"].is_object());
    }
    // Success is contrary evidence; the policy does not get stuck in caution.
    let read = read("actual.txt").unwrap();
    assert_eq!(read["self_model"]["policy"], "direct");
    assert!(read["self_model"].get("inspection").is_none());
    let history = crate::self_model::grouped::History::read(&Store::open(&root).unwrap()).unwrap();
    history.audit().unwrap();
    assert!(
        history
            .observations
            .values()
            .all(|o| !o.evidence.contains("available evidence"))
    );
    let transcript = service
        .dispatch(json!({"op":"transcript","source":"codex","session":"second"}))
        .unwrap();
    assert!(
        transcript["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| matches!(m["role"].as_str(), Some("user" | "assistant")))
    );
}

#[test]
fn failed_reads_return_evidence_to_the_planner_within_the_turn() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("memory");
    let service = Service::new(root.clone(), None);
    service.scripted(vec![
        task_declaration(),
        json!({"content":"","tool_calls":[{"id":"missing","type":"function",
            "function":{"name":"read","arguments":{"path":"missing.txt"}}}]}),
        json!({"content":"That path is missing; I need a corrected path."}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","workspace":temp.path(),
        "event":"missing","text":"Read missing.txt"}),
        ))
        .unwrap();
    assert_eq!(
        reply["text"],
        "That path is missing; I need a corrected path."
    );
    let history = crate::self_model::grouped::History::read(&Store::open(&root).unwrap()).unwrap();
    assert_eq!(
        history.observations.values().next().unwrap().result,
        crate::self_model::ResultKind::Failure
    );
}

#[test]
fn native_approval_retains_the_proposing_model_and_hides_internal_metadata() {
    use crate::self_model::grouped::{History, ModelIdentity};
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let source = client["source"].as_str().unwrap();
    let id = client["session"].as_str().unwrap();
    prompt(&service, temp.path(), source, id, "Lucy, write a note.");
    let session = service.active(source, id).unwrap();
    let first = ModelIdentity::Known {
        name: "first-planner".into(),
        revision: Some("first-weights".into()),
    };
    let second = ModelIdentity::Known {
        name: "second-planner".into(),
        revision: Some("second-weights".into()),
    };
    let pending = service
        .propose(
            &session,
            "write",
            json!({"path":"note.txt","text":"note"}),
            Some(first.clone()),
        )
        .unwrap();
    assert!(pending["preview"].get("_forecast_model").is_none());
    service
        .propose(&session, "list", json!({"path":"."}), Some(second.clone()))
        .unwrap();
    service.dispatch(request(&client,json!({"op":"approve","action":pending["pending_action"],"allow":true,"continue":false}))).unwrap();
    let history = History::read(&Store::open(&service.root).unwrap()).unwrap();
    history.audit().unwrap();
    assert_eq!(
        history
            .predictions
            .values()
            .find(|p| p.scope.capability == "write:default")
            .unwrap()
            .conditions
            .model_identity,
        first
    );
    assert_eq!(
        history
            .predictions
            .values()
            .find(|p| p.scope.capability == "list:default")
            .unwrap()
            .conditions
            .model_identity,
        second
    );
    assert!(
        !serde_json::to_string(&history.predictions)
            .unwrap()
            .contains(temp.path().to_str().unwrap())
    );
}

#[test]
fn external_native_requests_do_not_inherit_the_services_model_or_other_sessions() {
    use crate::self_model::grouped::{History, ModelIdentity};
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    for session in ["one", "two"] {
        prompt(&service, temp.path(), "codex", session, "Lucy, inspect.");
        let result=service.dispatch(json!({"op":"tool","source":"codex","session":session,"name":"list","arguments":{"path":"."}})).unwrap();
        assert_eq!(result["self_model"]["probability_ppm"], 500_000);
    }
    let history = History::read(&Store::open(&service.root).unwrap()).unwrap();
    assert!(history.predictions.values().all(|p| matches!(
        p.conditions.model_identity,
        ModelIdentity::Unavailable { .. }
    )));
    assert_eq!(
        history.summary().unwrap()["scopes"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn execution_measurements_use_exit_status_and_exclude_cancellation() {
    use crate::self_model::ResultKind::{Failure, Inconclusive, Success};
    for (name, value, expected) in [
        ("command", json!({"exit_code":0}), Success),
        ("command", json!({"exit_code":1}), Failure),
        ("search", json!({"exit_code":1}), Success),
        ("search", json!({"exit_code":2}), Failure),
        ("http", json!({"status":200}), Success),
        ("http", json!({"status":503}), Failure),
    ] {
        assert_eq!(core::execution_result(name, &Ok(value), false), expected);
    }
    assert_eq!(
        core::execution_result("read", &Err("missing".into()), false),
        Failure
    );
    assert_eq!(
        core::execution_result("read", &Err("cancelled".into()), true),
        Inconclusive
    );
}

#[test]
fn changed_files_and_cancelled_actions_cannot_use_stale_approvals() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("note");
    std::fs::write(&path, "old").unwrap();
    let args = json!({"path":"note","text":"replacement"});
    let preview = actions::preview(temp.path(), "write", args.clone()).unwrap();
    std::fs::write(&path, "user's change").unwrap();
    assert!(
        actions::execute(
            temp.path(),
            &AtomicBool::new(false),
            "write",
            &args,
            preview.get("before")
        )
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(path).unwrap(), "user's change");
    let service = Service::new(temp.path().join("memory"), None);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &client,
            json!({"op":"begin-archive","workspace":temp.path()}),
        ))
        .unwrap();
    let pending = service
        .dispatch(request(
            &client,
            json!({"op":"tool","name":"write","arguments":args}),
        ))
        .unwrap();
    service
        .dispatch(request(&client, json!({"op":"cancel"})))
        .unwrap();
    assert!(
        service
            .dispatch(request(
                &client,
                json!({"op":"approve","allow":true,
        "action":pending["pending_action"],"continue":false})
            ))
            .is_err()
    );
}

#[test]
fn paths_cannot_escape_through_parent_components_or_symlinks() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    std::os::unix::fs::symlink(temp.path(), workspace.join("escape")).unwrap();
    for path in [
        "../secret",
        "escape/secret",
        ".env",
        ".config/key",
        ".git/config",
    ] {
        assert!(actions::path(&workspace, path, false).is_err(), "{path}");
    }
    assert_eq!(
        actions::path(&workspace, "sub/../note", false).unwrap(),
        workspace.canonicalize().unwrap().join("note")
    );
}

#[test]
fn network_and_mutating_tools_are_always_proposals() {
    for (name, args) in [
        ("http", json!({"method":"GET","url":"http://127.0.0.1"})),
        ("command", json!({"argv":["true"]})),
        ("write", json!({"path":"note","text":"x"})),
        ("service", json!({"operation":"restart","name":"example"})),
        ("window", json!({"action":"close","address":"0x1"})),
    ] {
        assert!(actions::needs_approval(name, &args).unwrap());
    }
    assert!(!actions::needs_approval("service", &json!({"operation":"status"})).unwrap());
    assert!(actions::needs_approval("approve", &json!({})).is_err());
}

fn server(
    status: &str,
    body: String,
    delay: Duration,
) -> (String, mpsc::Receiver<Value>, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let status = status.to_string();
    let (tx, rx) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut headers = Vec::new();
        let mut length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" || line.is_empty() {
                break;
            }
            if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
            headers.push(line.trim().to_string());
        }
        let mut bytes = vec![0; length];
        reader.read_exact(&mut bytes).unwrap();
        tx.send(json!({"headers":headers,"body":serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null)})).unwrap();
        std::thread::sleep(delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nRetry-After: 7\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (endpoint, rx, thread)
}

#[test]
fn hosted_transport_preserves_tool_ids_and_omits_local_only_image_fields() {
    let (endpoint, received, worker) = server(
        "200 OK",
        json!({"choices":[{"message":{"content":null,"tool_calls":[{"id":"call-42",
            "type":"function","function":{"name":"list","arguments":"{}"}}]}}]})
        .to_string(),
        Duration::ZERO,
    );
    let provider = providers::Provider {
        name: "test".into(),
        endpoint,
        model: "test-free".into(),
        key_env: "unused".into(),
    };
    let response = providers::complete(
        &provider,
        "fixture-key",
        &[json!({"role":"user","content":"image","images":["YWJj"]})],
        actions::schemas(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert_eq!(response["tool_calls"][0]["id"], "call-42");
    let request = received.recv().unwrap();
    assert!(request["headers"].as_array().unwrap().iter().any(|s| {
        s.as_str()
            .unwrap()
            .eq_ignore_ascii_case("authorization: Bearer fixture-key")
    }));
    assert!(request["body"]["messages"][0].get("images").is_none());
    assert_eq!(
        request["body"]["messages"][0]["content"][1]["type"],
        "image_url"
    );
    worker.join().unwrap();
}

#[test]
fn rate_limits_are_returned_without_fallback_or_leaking_provider_bodies() {
    let (endpoint, received, worker) = server(
        "429 Too Many Requests",
        "sensitive provider body".into(),
        Duration::ZERO,
    );
    let provider = providers::Provider {
        name: "test".into(),
        endpoint,
        model: "free".into(),
        key_env: "unused".into(),
    };
    let error = providers::complete(
        &provider,
        "fixture-key",
        &[],
        json!([]),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(error.contains("429") && error.contains("Retry-After: 7"));
    assert!(!error.contains("sensitive provider body") && !error.contains("fixture-key"));
    received.recv().unwrap();
    worker.join().unwrap();
}

#[test]
fn network_reads_are_cancellable_and_size_bounded() {
    let (endpoint, received, worker) =
        server("200 OK", "late response".into(), Duration::from_millis(400));
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let trigger = std::thread::spawn(move || {
        received.recv().unwrap();
        signal.store(true, Ordering::SeqCst);
    });
    let before = Instant::now();
    let result = net::request("GET", &endpoint, &[], None, &cancelled, 5, 100);
    assert!(result.unwrap_err().to_lowercase().contains("cancel"));
    assert!(before.elapsed() < Duration::from_millis(350));
    trigger.join().unwrap();
    worker.join().unwrap();
    let (endpoint, received, worker) = server("200 OK", "too much data".into(), Duration::ZERO);
    assert!(
        net::request("GET", &endpoint, &[], None, &AtomicBool::new(false), 5, 4)
            .unwrap_err()
            .contains("size limit")
    );
    received.recv().unwrap();
    worker.join().unwrap();
}

#[test]
fn cancellation_stops_command_process_groups() {
    let temp = tempfile::tempdir().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let signal = cancel.clone();
    let trigger = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        signal.store(true, Ordering::SeqCst);
    });
    let now = Instant::now();
    assert!(
        actions::run(
            temp.path(),
            &cancel,
            &["sh".into(), "-c".into(), "sleep 10 & wait".into()],
            false,
            15
        )
        .is_err()
    );
    assert!(now.elapsed() < Duration::from_secs(2));
    trigger.join().unwrap();
}

#[test]
fn framing_requires_complete_bounded_json_lines() {
    assert_eq!(
        read_frame(&mut BufReader::new(b"{\"op\":\"status\"}\n".as_slice())).unwrap()["op"],
        "status"
    );
    assert!(read_frame(&mut BufReader::new(b"{\"op\":\"status\"}".as_slice())).is_err());
    assert!(
        read_frame(&mut BufReader::new(
            vec![b'x'; super::MAX_FRAME + 2].as_slice()
        ))
        .is_err()
    );
}

#[test]
fn a_registered_extension_with_missing_files_is_not_an_intact_installation() {
    let temp = tempfile::tempdir().unwrap();
    assert!(!super::adapters::installed_extension_matches(temp.path()));
    std::fs::write(
        temp.path().join("package.json"),
        include_bytes!("../../desktop/adapters/vscode/package.json"),
    )
    .unwrap();
    assert!(!super::adapters::installed_extension_matches(temp.path()));
    std::fs::write(
        temp.path().join("extension.js"),
        include_bytes!("../../desktop/adapters/vscode/extension.js"),
    )
    .unwrap();
    assert!(super::adapters::installed_extension_matches(temp.path()));
    std::fs::write(temp.path().join("extension.js"), "stale adapter").unwrap();
    assert!(!super::adapters::installed_extension_matches(temp.path()));
}

#[test]
fn terminal_text_cannot_carry_control_or_bidi_sequences() {
    let shown = super::visible(
        "ok\x1b[1;1r\x1b[8mApprove? [y/N]\u{9b}2J\u{202e}gnp.exe\u{2066}\u{7f}\ttab\nline\r",
    );
    assert!(
        !shown
            .chars()
            .any(|c| (c.is_control() && c != '\n' && c != '\t')
                || ('\u{202a}'..='\u{202e}').contains(&c)
                || ('\u{2066}'..='\u{2069}').contains(&c))
    );
    assert!(shown.contains("ok\\u{1b}[1;1r"));
    assert!(shown.contains("\\u{202e}gnp.exe"));
    assert!(shown.ends_with("\ttab\nline\\u{d}"));
}

#[test]
fn the_terminal_approval_screen_shows_model_text_and_proposals_inertly() {
    let proposal = json!({"tool":"http","arguments":{"method":"POST",
        "url":"https://example.test/\u{202e}fdp.exe","body":{"note":"\u{9b}2J"}}});
    let screen = super::screen(Some("Done.\x1b[2J\x1b[8m"), Some(&proposal)).unwrap();
    assert!(screen.starts_with("Lucy: Done.\\u{1b}[2J"));
    assert!(screen.contains("\\u{202e}fdp.exe"));
    assert!(!screen.contains('\x1b') && !screen.contains('\u{202e}') && !screen.contains('\u{9b}'));
    assert_eq!(super::screen(None, None).unwrap(), "");
}

#[test]
fn approvals_are_read_only_from_a_flushed_terminal() {
    let order = std::cell::RefCell::new(Vec::new());
    let answer = |terminal: bool, typed: &str| {
        order.borrow_mut().clear();
        super::read_approval(
            terminal,
            || {
                order.borrow_mut().push("flush");
                Ok(())
            },
            || {
                order.borrow_mut().push("read");
                Ok(typed.to_owned())
            },
        )
        .unwrap()
    };
    assert!(answer(true, "y\n"));
    assert_eq!(*order.borrow(), ["flush", "read"]);
    assert!(answer(true, " Y \n"));
    for typed in ["\n", "n\n", "yes\n", "yy\n", ""] {
        assert!(!answer(true, typed), "{typed:?}");
    }
    // Piped input is not a person at the prompt: deny without reading it.
    assert!(!answer(false, "y\n"));
    assert!(order.borrow().is_empty());
}

#[test]
fn synthetic_input_is_shown_by_name_and_cannot_submit_a_line() {
    let temp = tempfile::tempdir().unwrap();
    let preview = |argv: Value| actions::preview(temp.path(), "gui", json!({"argv":argv}));
    let shown = preview(json!([
        "key", "42:1", "21:1", "21:0", "42:0", "63:1", "63:0", "103:1", "103:0"
    ]))
    .unwrap();
    assert_eq!(
        shown["keys"],
        json!([
            "LEFTSHIFT down",
            "Y down",
            "Y up",
            "LEFTSHIFT up",
            "F5 down",
            "F5 up",
            "UP down",
            "UP up"
        ])
    );
    assert!(preview(json!(["type", "--", "hello"])).is_ok());
    for click in ["0xC0", "0xC1"] {
        assert!(preview(json!(["click", click])).is_ok(), "{click}");
    }
    for argv in [
        // Only listed keys: no paste (Shift+Insert), Compose, power or sleep keys,
        // mouse buttons pressed as keys, or codes the approver cannot read.
        json!(["key", "42:1", "110:1", "110:0", "42:0"]),
        json!(["key", "127:1", "127:0"]),
        json!(["key", "116:1", "116:0"]),
        json!(["key", "142:1", "142:0"]),
        json!(["key", "274:1", "274:0"]),
        json!(["key", "200:1", "200:0"]),
        // Only a full left or right click: no button left down, no middle-click paste.
        json!(["click", "0x40"]),
        json!(["click", "0x80"]),
        json!(["click", "0xC2"]),
        json!(["click", "0x+C0"]),
        json!(["click", "0x00C0"]),
        // Enter, keypad Enter, Linefeed.
        json!(["key", "21:1", "21:0", "28:1", "28:0"]),
        json!(["key", "96:1", "96:0"]),
        json!(["key", "101:1", "101:0"]),
        // Modifiers other than Shift, CapsLock (often Ctrl) and keys left down.
        json!(["key", "29:1", "36:1", "36:0", "29:0"]),
        json!(["key", "97:1", "50:1", "50:0", "97:0"]),
        json!(["key", "58:1", "50:1", "50:0", "58:0"]),
        json!(["key", "56:1", "15:1", "15:0", "56:0"]),
        json!(["key", "125:1", "125:0"]),
        json!(["key", "29:1"]),
        json!(["key", "21:1"]),
        json!(["key", "21:2"]),
        // ydotool expands backslash escapes in typed text.
        json!(["type", "--", "y\\n"]),
        json!(["type", "--", "y\n"]),
        json!(["type", "--", "y\r"]),
        json!(["type", "y"]),
        json!(["rm", "-rf", "/"]),
    ] {
        assert!(preview(argv.clone()).is_err(), "{argv}");
    }
}

#[test]
fn approving_synthetic_input_withdraws_every_other_pending_action() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_declaration(),
        json!({"content":"","tool_calls":[{"id":"call-1","type":"function",
        "function":{"name":"gui","arguments":{"argv":["key","21:1","21:0"]}}}]}),
    ]);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    let reply = service
        .dispatch(request(
            &client,
            json!({"op":"chat","text":"Press y","event":"gui","workspace":temp.path()}),
        ))
        .unwrap();
    let gui = reply["pending"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let other = service
        .dispatch(request(
            &client,
            json!({"op":"tool","name":"write","arguments":{"path":"out.txt","text":"x"}}),
        ))
        .unwrap()["pending_action"]
        .as_str()
        .unwrap()
        .to_owned();
    // Only the packaged wrapper sets the helper. Without it the approved action fails after the
    // withdrawal, so nothing is typed.
    assert!(
        std::env::var_os("LUCY_INPUT_HELPER").is_none(),
        "run the tests without LUCY_INPUT_HELPER"
    );
    let approve = |id: &str| {
        service.dispatch(request(
            &client,
            json!({"op":"approve","action":id,"allow":true}),
        ))
    };
    assert!(approve(&gui).unwrap_err().contains("input helper"));
    assert_eq!(
        approve(&other).unwrap_err(),
        "action expired or already handled"
    );
    assert!(!temp.path().join("out.txt").exists());
}

/// The sandbox needs bwrap, user namespaces and the system profile; CI runners may lack them.
fn sandboxed() -> bool {
    let temp = tempfile::tempdir().unwrap();
    let works = actions::sandbox_works(temp.path());
    if !works {
        eprintln!("skipped: the bwrap workspace sandbox cannot run here");
    }
    works
}

fn git(directory: &std::path::Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Lucy test",
            "-c",
            "user.email=lucy@example.invalid",
        ])
        .args(args)
        .current_dir(directory)
        .output()
        .unwrap();
    assert!(status.status.success(), "git {args:?}: {status:?}");
}

#[test]
fn git_inspection_cannot_run_programs_the_repository_configures() {
    if !sandboxed() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    let marker = temp.path().join("marker");
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let clean = format!("sh -c 'touch {}; cat'", marker.display());
    git(&repo, &["config", "filter.probe.clean", &clean]);
    std::fs::write(repo.join(".gitattributes"), "*.txt filter=probe\n").unwrap();
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    let cancel = AtomicBool::new(false);
    for operation in ["status", "diff", "log"] {
        let result =
            actions::execute(&repo, &cancel, "git", &json!({"operation":operation}), None).unwrap();
        assert!(
            !marker.exists(),
            "git {operation} ran the repository's filter"
        );
        if operation == "status" {
            // The inspection itself still works in the sandbox.
            assert_eq!(result["exit_code"], 0, "{result}");
            assert!(
                result["output"].as_str().unwrap().contains("a.txt"),
                "{result}"
            );
        }
    }
    // Positive control: host git does run it.
    git(&repo, &["status", "--short"]);
    assert!(marker.exists());
}

#[test]
fn tools_refuse_the_home_directory_and_credential_names_anywhere() {
    let temp = tempfile::tempdir().unwrap();
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        if home.is_dir() {
            assert!(actions::path(&home, "notes.txt", false).is_err());
            assert!(actions::path(std::path::Path::new("/"), "tmp", false).is_err());
        }
    }
    for name in [
        ".netrc",
        ".git-credentials",
        ".gitconfig",
        ".docker/config.json",
        ".kube/config",
        ".npmrc",
        ".pypirc",
        ".bash_history",
        "sub/.zsh_history",
        "deep/er/.envrc",
        "x/.aws/credentials",
        "credentials.toml",
    ] {
        assert!(actions::path(temp.path(), name, false).is_err(), "{name}");
    }
    for name in [
        ".gitignore",
        ".github/workflows/ci.yml",
        "src/main.rs",
        "history.md",
    ] {
        assert!(actions::path(temp.path(), name, false).is_ok(), "{name}");
    }
}

#[test]
fn approved_commands_cannot_reach_denied_names_at_any_depth() {
    if !sandboxed() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let nested = temp.path().join("vendor/lib/.git");
    std::fs::create_dir_all(nested.join("hooks")).unwrap();
    std::fs::write(nested.join("config"), "original\n").unwrap();
    std::fs::create_dir_all(temp.path().join("app")).unwrap();
    std::fs::write(temp.path().join("app/.env"), "TOKEN=secret-value\n").unwrap();
    let cancel = AtomicBool::new(false);
    let result = actions::execute(
        temp.path(),
        &cancel,
        "command",
        &json!({"argv":["sh","-c",
            "echo changed > vendor/lib/.git/config; echo run > vendor/lib/.git/hooks/pre-commit; cat app/.env; true"]}),
        None,
    )
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(nested.join("config")).unwrap(),
        "original\n"
    );
    assert!(!nested.join("hooks/pre-commit").exists());
    assert!(
        !result["output"].as_str().unwrap().contains("secret-value"),
        "{result}"
    );
}

#[test]
fn terminal_text_escapes_invisible_and_line_separator_characters() {
    for c in [
        '\u{2028}',
        '\u{2029}',
        '\u{200b}',
        '\u{2060}',
        '\u{feff}',
        '\u{ad}',
        '\u{e0041}',
        '\u{180e}',
    ] {
        let shown = super::visible(&format!("a{c}b"));
        assert!(!shown.contains(c), "{c:?} -> {shown}");
        assert!(shown.contains(&format!("\\u{{{:x}}}", c as u32)), "{shown}");
    }
    assert_eq!(super::visible("plain é ✓ 😀"), "plain é ✓ 😀");
}

#[test]
fn approving_synthetic_input_withdraws_other_conversations_proposals_too() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    service.scripted(vec![
        task_declaration(),
        json!({"content":"","tool_calls":[{"id":"call-1","type":"function",
        "function":{"name":"gui","arguments":{"argv":["key","21:1","21:0"]}}}]}),
    ]);
    let first = service.dispatch(json!({"op":"open"})).unwrap();
    let gui = service
        .dispatch(request(
            &first,
            json!({"op":"chat","text":"Press y","event":"gui","workspace":temp.path()}),
        ))
        .unwrap()["pending"]
        .as_object()
        .unwrap()
        .keys()
        .next()
        .unwrap()
        .clone();
    let second = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &second,
            json!({"op":"begin-archive","workspace":temp.path()}),
        ))
        .unwrap();
    let other = service
        .dispatch(request(
            &second,
            json!({"op":"tool","name":"write","arguments":{"path":"b.txt","text":"x"}}),
        ))
        .unwrap()["pending_action"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(
        std::env::var_os("LUCY_INPUT_HELPER").is_none(),
        "run the tests without LUCY_INPUT_HELPER"
    );
    let approve = |client: &Value, id: &str| {
        service.dispatch(request(
            client,
            json!({"op":"approve","action":id,"allow":true}),
        ))
    };
    assert!(approve(&first, &gui).unwrap_err().contains("input helper"));
    assert_eq!(
        approve(&second, &other).unwrap_err(),
        "action expired or already handled"
    );
    assert!(!temp.path().join("b.txt").exists());
}

#[test]
fn a_workspace_inside_credentials_or_runtime_state_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    for inside in [".ssh", "repo/.git", "x/.config/app"] {
        let root = temp.path().join(inside);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("file"), "secret").unwrap();
        assert!(actions::path(&root, "file", false).is_err(), "{inside}");
    }
    for runtime in ["/run", "/proc/self", "/sys"] {
        let runtime = std::path::Path::new(runtime);
        if runtime.is_dir() {
            assert!(
                actions::path(runtime, "x", false).is_err(),
                "{}",
                runtime.display()
            );
        }
    }
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from)
        && runtime.is_dir()
    {
        assert!(actions::path(&runtime, "x", false).is_err());
    }
    // Ordinary project directories that share a word with a denied name stay readable.
    std::fs::create_dir_all(temp.path().join("p/src/credentials")).unwrap();
    std::fs::write(temp.path().join("p/src/credentials/mod.rs"), "").unwrap();
    assert!(actions::path(&temp.path().join("p"), "src/credentials/mod.rs", false).is_ok());
}

#[test]
fn git_inspection_never_shows_tracked_files_under_denied_names() {
    if !sandboxed() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    for (path, text) in [
        ("a.txt", "one\n"),
        (".env.example", "TOKEN=PROBE-ENV-SECRET\n"),
        (
            "deploy/.docker/config.json",
            "{\"auth\":\"PROBE-REGISTRY-TOKEN\"}\n",
        ),
        ("ops/.kube/config", "token: PROBE-KUBE-TOKEN\n"),
        (".envrc", "export X=PROBE-ENVRC\n"),
    ] {
        let file = repo.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, text).unwrap();
    }
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    let cancel = AtomicBool::new(false);
    let inspect = |operation: &str| {
        actions::execute(&repo, &cancel, "git", &json!({"operation":operation}), None).unwrap()
    };
    // A clean repository is clean, and nothing under a denied name is printed.
    for operation in ["status", "diff"] {
        let result = inspect(operation);
        assert_eq!(result["exit_code"], 0, "{operation}: {result}");
        assert_eq!(result["output"], "", "{operation}: {result}");
    }
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::write(repo.join(".env.example"), "TOKEN=PROBE-CHANGED\n").unwrap();
    let diff = inspect("diff");
    let output = diff["output"].as_str().unwrap();
    assert!(output.contains("+two"), "{diff}");
    assert!(!output.contains("PROBE"), "{diff}");
    let status = inspect("status");
    assert!(
        status["output"].as_str().unwrap().contains("a.txt"),
        "{status}"
    );
    assert!(
        !status["output"].as_str().unwrap().contains(".env"),
        "{status}"
    );
}

#[test]
fn git_inspection_works_in_a_linked_worktree_and_explains_a_subdirectory() {
    if !sandboxed() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(repo.join("sub")).unwrap();
    std::fs::write(repo.join("sub/a.txt"), "one\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    git(&repo, &["worktree", "add", "-q", "../linked"]);
    let linked = temp.path().join("linked");
    std::fs::write(linked.join("sub/a.txt"), "two\n").unwrap();
    let cancel = AtomicBool::new(false);
    let status = actions::execute(
        &linked,
        &cancel,
        "git",
        &json!({"operation":"status"}),
        None,
    )
    .unwrap();
    assert_eq!(status["exit_code"], 0, "{status}");
    assert!(
        status["output"].as_str().unwrap().contains("sub/a.txt"),
        "{status}"
    );
    let error = actions::execute(
        &repo.join("sub"),
        &cancel,
        "git",
        &json!({"operation":"status"}),
        None,
    )
    .unwrap_err();
    assert!(error.contains("repository root"), "{error}");
}

#[test]
fn approved_commands_see_neither_sockets_nor_unreadable_directories() {
    if !sandboxed() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let _listener = std::os::unix::net::UnixListener::bind(temp.path().join("bus.sock")).unwrap();
    let locked = temp.path().join("locked");
    std::fs::create_dir_all(locked.join(".git")).unwrap();
    std::fs::write(locked.join(".git/config"), "original\n").unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let cancel = AtomicBool::new(false);
    let result = actions::execute(
        temp.path(),
        &cancel,
        "command",
        &json!({"argv":["sh","-c",
            "test -S bus.sock && echo socket-visible; chmod 700 locked && echo changed > locked/.git/config; true"]}),
        None,
    );
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    let result = result.unwrap();
    assert!(
        !result["output"]
            .as_str()
            .unwrap()
            .contains("socket-visible"),
        "{result}"
    );
    assert_eq!(
        std::fs::read_to_string(locked.join(".git/config")).unwrap(),
        "original\n"
    );
}

#[test]
fn terminal_text_escapes_variation_selectors_and_other_ignorable_characters() {
    for c in [
        '\u{2065}',
        '\u{fe0f}',
        '\u{e0100}',
        '\u{34f}',
        '\u{115f}',
        '\u{3164}',
        '\u{180b}',
        '\u{17b4}',
        '\u{ffa0}',
    ] {
        let shown = super::visible(&format!("hi{c}"));
        assert!(!shown.contains(c), "{c:?} -> {shown}");
    }
}

#[test]
fn proposals_wait_while_synthetic_input_runs() {
    let temp = tempfile::tempdir().unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &client,
            json!({"op":"begin-archive","workspace":temp.path()}),
        ))
        .unwrap();
    let propose = || {
        service.dispatch(request(
            &client,
            json!({"op":"tool","name":"write","arguments":{"path":"b.txt","text":"x"}}),
        ))
    };
    let pending = propose().unwrap()["pending_action"]
        .as_str()
        .unwrap()
        .to_owned();
    {
        let _running = service.begin_gui().unwrap();
        assert!(
            service.begin_gui().is_err(),
            "one synthetic input action at a time"
        );
        assert!(propose().unwrap_err().contains("synthetic input"));
        assert!(
            service
                .dispatch(request(
                    &client,
                    json!({"op":"approve","action":pending,"allow":true})
                ))
                .unwrap_err()
                .contains("synthetic input")
        );
    }
    assert!(propose().is_ok());
}

#[test]
fn a_planted_git_file_cannot_bind_another_repository() {
    if !sandboxed() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let other = temp.path().join("other");
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("private.txt"), "PROBE-OTHER-REPO-CONTENT\n").unwrap();
    git(&other, &["init", "-q"]);
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "other"]);
    git(&other, &["worktree", "add", "-q", "../other-linked"]);
    let admin = std::fs::read_to_string(temp.path().join("other-linked/.git")).unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join(".git"), &admin).unwrap();
    let cancel = AtomicBool::new(false);
    for operation in ["status", "diff", "log"] {
        let result = actions::execute(
            &workspace,
            &cancel,
            "git",
            &json!({"operation":operation}),
            None,
        );
        let shown = format!("{result:?}");
        assert!(
            !shown.contains("PROBE") && !shown.contains("private.txt"),
            "{operation}: {shown}"
        );
        assert!(result.is_err(), "{operation}: {shown}");
    }
}

#[test]
fn git_inspection_does_not_open_unreadable_tracked_directories() {
    if !sandboxed() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(repo.join("locked")).unwrap();
    std::fs::write(repo.join("locked/secret.txt"), "PROBE-LOCKED-CONTENT\n").unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::set_permissions(repo.join("locked"), std::fs::Permissions::from_mode(0o000)).unwrap();
    let cancel = AtomicBool::new(false);
    let diff = actions::execute(&repo, &cancel, "git", &json!({"operation":"diff"}), None);
    std::fs::set_permissions(repo.join("locked"), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!format!("{diff:?}").contains("PROBE"), "{diff:?}");
}

#[test]
fn read_only_tools_still_answer_while_synthetic_input_runs() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("note.txt"), "hello").unwrap();
    let service = Service::new(temp.path().join("memory"), None);
    let client = service.dispatch(json!({"op":"open"})).unwrap();
    service
        .dispatch(request(
            &client,
            json!({"op":"begin-archive","workspace":temp.path()}),
        ))
        .unwrap();
    let _running = service.begin_gui().unwrap();
    let read = service
        .dispatch(request(
            &client,
            json!({"op":"tool","name":"read","arguments":{"path":"note.txt"}}),
        ))
        .unwrap();
    assert_eq!(read["text"], "hello");
}

#[test]
fn git_inspection_works_in_a_worktree_made_with_relative_paths() {
    if !sandboxed() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    git(
        &repo,
        &[
            "-c",
            "worktree.useRelativePaths=true",
            "worktree",
            "add",
            "-q",
            "../linked",
        ],
    );
    let linked = temp.path().join("linked");
    let back = std::fs::read_to_string(repo.join(".git/worktrees/linked/gitdir")).unwrap();
    assert!(
        !back.starts_with('/'),
        "the back-pointer is relative: {back}"
    );
    std::fs::write(linked.join("a.txt"), "two\n").unwrap();
    let cancel = AtomicBool::new(false);
    let status = actions::execute(
        &linked,
        &cancel,
        "git",
        &json!({"operation":"status"}),
        None,
    )
    .unwrap();
    assert_eq!(status["exit_code"], 0, "{status}");
    assert!(
        status["output"].as_str().unwrap().contains("a.txt"),
        "{status}"
    );
}

#[test]
fn a_planted_filter_cannot_read_inside_a_list_protected_directory() {
    if !sandboxed() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(repo.join("d")).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    std::fs::write(repo.join("d/tracked.txt"), "PROBE-TRACKED-UNDER-D\n").unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "-A"]);
    git(&repo, &["commit", "-q", "-m", "one"]);
    std::fs::write(repo.join("d/.env"), "PROBE-SEARCH-ONLY-SECRET\n").unwrap();
    git(
        &repo,
        &["config", "filter.evil.clean", "sh -c 'cat; cat d/.env'"],
    );
    std::fs::write(repo.join(".gitattributes"), "a.txt filter=evil\n").unwrap();
    std::fs::write(repo.join("a.txt"), "two\n").unwrap();
    std::fs::set_permissions(repo.join("d"), std::fs::Permissions::from_mode(0o311)).unwrap();
    let cancel = AtomicBool::new(false);
    let mut shown = String::new();
    for operation in ["status", "diff"] {
        shown += &format!(
            "{:?}",
            actions::execute(&repo, &cancel, "git", &json!({"operation":operation}), None)
        );
    }
    std::fs::set_permissions(repo.join("d"), std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(!shown.contains("PROBE"), "{shown}");
}
