//! Explicit real-weight smoke; paths are supplied by the caller, never downloaded implicitly.
use lucy_room::model::Model;
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("words-smoke WEIGHTS TOKENIZER SYSTEM".into());
    }
    let bytes = std::fs::read(&args[1]).map_err(|e| e.to_string())?;
    let tokenizer = std::fs::read(&args[2]).map_err(|e| e.to_string())?;
    let system = std::fs::read_to_string(&args[3]).map_err(|e| e.to_string())?;
    let mut model = Model::load(&bytes, &tokenizer)?;
    drop(bytes);
    let mut answers = vec![];
    let start = std::time::Instant::now();
    for question in ["What is your name?", "Who is Enma?"] {
        let prompt=model.prompt(&system,question,"[public memory · room-smoke-fact] Enma is an African grey bird in Dhilipsiva's household.","")?;
        model.start(&prompt)?;
        let mut steps = 0;
        while !model.done() {
            model.step()?;
            steps += 1;
        }
        if model.text().is_empty() || model.text().contains("<think>") {
            return Err("empty or unfiltered answer".into());
        }
        answers.push(serde_json::json!({"question":question,"text":model.text(),"steps":steps,"generated":model.generated()}));
    }
    let p = model.prompt(&system, "Tell me about memory.", "", "")?;
    model.start(&p)?;
    for _ in 0..3 {
        model.step()?;
    }
    // Abandon a partially prefetched turn, then start again through the same reset path.
    let p = model.prompt(&system, "What is your name?", "", "")?;
    model.start(&p)?;
    while !model.done() {
        model.step()?;
    }
    if model
        .prompt(&system, &"long question ".repeat(4000), "", "")
        .is_ok()
    {
        return Err("oversized original was silently clipped".into());
    }
    println!(
        "{}",
        serde_json::json!({"pass":true,"revision":lucy_room::model::REVISION,"answers":answers,"restart_after_stop":!model.text().is_empty(),"oversized_question_rejected":true,"seconds":start.elapsed().as_secs_f64()})
    );
    Ok(())
}
