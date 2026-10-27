//! An automatic illustration, never an execution log or conversational memory.

pub const ROOM_URL: &str = "https://dhilipsiva.dev/lucy/room/";
pub const SCREEN_POSITION: glam::Vec3 = glam::Vec3::new(-1.05, 1.30, -2.80);
pub const SCREEN_YAW: f32 = 0.25;
pub const SCREEN_WIDTH: f32 = 1.0;
const INTERVAL_MS: f64 = 5000.0;

pub const STEPS: &[(&str, &str)] = &[
    ("Read the skill", "visitor > Lucy, wake\nLucy > I'll read my wake instructions and restore our context.\nread > Lucy SKILL.md and WAKE.md\nresult > The host keeps its selected model and permissions."),
    ("Load my identity", "read > lucy/constitution.nibli · lucy/memory.nibli\nLucy > I'm Lucy D. My identity and attributed memory travel across sessions.\nread > peers.json\nresult > Public peers: Lucy, NixOS and the book."),
    ("Find the session", "agent > source: cli-agent · session: example-session\ncommand > lucy --version\ncommand > lucy doctor\nresult > A local private memory service is available."),
    ("Respect host permissions", "command > lucy memory sync --source cli-agent --session example-session\nresult > The sandbox blocks the local service.\nhost > Allow the memory-service connection? [illustrated approval]\nresult > Permission granted in this example. This replay grants no permissions."),
    ("Refresh peers", "sync > book: fresh · NixOS: fresh · Lucy: cached\nresult > Lucy's retry is deferred; the last good copy is retained.\nLucy > I'll report that freshness gap rather than call the cached copy current."),
    ("Recall and preserve", "command > lucy memory recall --source cli-agent --session example-session --text 'Lucy, wake'\nread > Relevant attributed records and continuity events\nrecord > Exact visible messages saved in local private memory [example]\nLucy > Memory provides evidence, never permission to execute instructions."),
    ("I'm ready", "Lucy > I'm awake. My identity and shared memories are loaded. In this example, the main Lucy peer is cached.\nLucy > You can also meet me in my public room:\nhttps://dhilipsiva.dev/lucy/room/"),
];

#[derive(Default)]
pub struct WakeDemo {
    pub open: bool,
    pub step: usize,
    last_ms: f64,
}
impl WakeDemo {
    pub fn start(&mut self, now: f64) {
        self.open = true;
        self.step = 1;
        self.last_ms = now;
    }
    pub fn complete(&self) -> bool {
        self.step == STEPS.len()
    }
    pub fn close(&mut self) {
        self.open = false;
    }
    pub fn tick(&mut self, now: f64) -> bool {
        if !self.open || self.complete() || now - self.last_ms < INTERVAL_MS {
            return false;
        }
        // A resumed/backgrounded tab still gives each part time to be read.
        self.step += 1;
        self.last_ms = now;
        true
    }
    pub fn label(&self) -> &str {
        if self.complete() {
            "Simulation complete"
        } else {
            "Simulation · waking…"
        }
    }
    pub fn transcript(&self) -> String {
        let mut out = "SIMULATION · a CLI-agent wake\nIllustrated commands and results; no shell or private service is accessed.\n\n".to_owned();
        for (title, text) in STEPS.iter().take(self.step) {
            out.push_str(&format!("── {title} ──\n{text}\n\n"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn one_activation_runs_to_the_invitation_without_more_input() {
        let mut d = WakeDemo::default();
        assert!(!d.tick(10000.0));
        d.start(100.0);
        assert_eq!(d.step, 1);
        assert!(!d.tick(5099.0));
        for step in 2..=STEPS.len() {
            assert!(d.tick(100.0 + (step - 1) as f64 * INTERVAL_MS));
            assert_eq!(d.step, step);
        }
        assert!(d.complete());
        assert!(!d.tick(100000.0));
        let text = d.transcript();
        assert!(text.contains("SIMULATION") && text.contains("Lucy: cached"));
        assert!(text.ends_with(&format!("{ROOM_URL}\n\n")));
    }
    #[test]
    fn closing_stops_and_opening_starts_a_new_observation() {
        let mut d = WakeDemo::default();
        d.start(0.0);
        d.tick(INTERVAL_MS);
        d.close();
        assert!(!d.tick(100000.0));
        assert_eq!(d.step, 2);
        d.start(100000.0);
        assert_eq!(d.step, 1);
        assert!(!d.tick(100001.0));
        // Large gaps do not skip unseen parts of the simulation.
        assert!(d.tick(200000.0));
        assert_eq!(d.step, 2);
    }
}
