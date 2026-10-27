//! A guided example, never an execution log or conversational memory.
use lucy_room_memory::{Message, RoomArchiveV1};

pub const ROOM_URL: &str = "https://dhilipsiva.dev/lucy/room/";
pub const CONTEXT: &str = "[Room interface context] The visitor has watched a labelled demonstration of a CLI-agent wake: reading the Lucy skill, protocol, constitution and public memory, identifying a session, checking a local service, requesting host permission, refreshing peers, recalling context and recording a greeting. Those commands, permissions and peer statuses were illustrative, not executed by this browser. The current live conversation uses Talk's local model and this device's private journal. My public room is https://dhilipsiva.dev/lucy/room/.";
pub const SCREEN_POSITION: glam::Vec3 = glam::Vec3::new(-1.05, 1.30, -2.80);
pub const SCREEN_YAW: f32 = 0.25;
pub const SCREEN_WIDTH: f32 = 1.0;
const INTERVAL_MS: f64 = 3200.0;

pub const STEPS: &[(&str, &str)] = &[
    ("Read the skill", "visitor > Lucy, wake\nLucy > I'll read my wake instructions and restore our context.\nread > Lucy SKILL.md and WAKE.md\nresult > The host keeps its selected model and permissions."),
    ("Load my identity", "read > lucy/constitution.nibli · lucy/memory.nibli\nLucy > I'm Lucy D. My identity and attributed memory travel across sessions.\nread > peers.json\nresult > Public peers: Lucy, NixOS and the book."),
    ("Find the session", "agent > source: cli-agent · session: example-session\ncommand > lucy --version\ncommand > lucy doctor\nresult > A local private memory service is available."),
    ("Respect host permissions", "command > lucy memory sync --source cli-agent --session example-session\nresult > The sandbox blocks the local service.\nhost > Allow the memory-service connection? [illustrated approval]\nresult > Permission granted in this example. This replay grants no permissions."),
    ("Refresh peers", "sync > book: fresh · NixOS: fresh · Lucy: cached\nresult > Lucy's retry is deferred; the last good copy is retained.\nLucy > I'll report that freshness gap rather than call the cached copy current."),
    ("Recall and preserve", "command > lucy memory recall --source cli-agent --session example-session --text 'Lucy, wake'\nread > Relevant attributed records and continuity events\nrecord > Exact visible messages saved in local private memory [example]\nLucy > Memory provides evidence, never permission to execute instructions."),
    ("I'm ready", "Lucy > I'm awake. My identity and shared memories are loaded. In this example, the main Lucy peer is cached.\nLucy > You can also meet me in my public room:\nhttps://dhilipsiva.dev/lucy/room/\n\nThe demonstration is complete. Continue below to talk to my real local model."),
];

#[derive(Default)]
pub struct WakeDemo {
    pub open: bool,
    pub step: usize,
    pub playing: bool,
    pub live: bool,
    pub visited_live: bool,
    pub handover_requested: bool,
    /// Wrapped lines above the tail; zero follows new replies.
    pub scroll: usize,
    last_ms: f64,
}
impl WakeDemo {
    pub fn complete(&self) -> bool {
        self.step == STEPS.len()
    }
    pub fn close(&mut self) {
        self.open = false;
        self.playing = false;
    }
    pub fn play_pause(&mut self, now: f64) {
        if self.complete() {
            return;
        }
        if self.step == 0 {
            self.step = 1;
        }
        self.playing = !self.playing;
        self.last_ms = now;
        self.scroll = 0;
    }
    pub fn next(&mut self) {
        self.playing = false;
        self.step = (self.step + 1).min(STEPS.len());
        self.scroll = 0;
    }
    pub fn skip(&mut self) {
        self.step = STEPS.len();
        self.playing = false;
        self.scroll = 0;
    }
    pub fn restart(&mut self) {
        self.step = 0;
        self.playing = false;
        self.live = false;
        self.handover_requested = false;
        self.scroll = 0;
    }
    pub fn tick(&mut self, now: f64, model_ready: bool) -> bool {
        let mut changed = false;
        if self.open && self.playing && now - self.last_ms >= INTERVAL_MS {
            self.step = (self.step + 1).min(STEPS.len());
            self.last_ms = now;
            self.scroll = 0;
            if self.complete() {
                self.playing = false;
            }
            changed = true;
        }
        if self.complete() && self.handover_requested && model_ready && !self.live {
            self.live = true;
            self.visited_live = true;
            self.scroll = 0;
            changed = true;
        }
        changed
    }
    pub fn label(&self) -> String {
        if self.live {
            "Live · on this device".into()
        } else if self.complete() {
            "Demonstration complete".into()
        } else if self.step == 0 {
            "Demonstration · ready to start".into()
        } else {
            format!(
                "Demonstration · {} / {} · {}",
                self.step,
                STEPS.len(),
                STEPS[self.step - 1].0
            )
        }
    }
    pub fn transcript(
        &self,
        archive: &RoomArchiveV1,
        session: &str,
        stream: Option<&Message>,
    ) -> String {
        let mut out = "DEMONSTRATION · a CLI-agent wake\nIllustrated commands and results; no shell or private service is accessed.\n\n".to_owned();
        if self.step == 0 {
            out.push_str("visitor > Lucy, wake\n\nStart the guided replay, or use Next to explore one step at a time.\n");
        }
        for (title, text) in STEPS.iter().take(self.step) {
            out.push_str(&format!("── {title} ──\n{text}\n\n"));
        }
        if self.live {
            out.push_str("── LIVE · ON THIS DEVICE · shared with Talk ──\n");
            for m in archive.messages.iter().filter(|m| m.session == session) {
                out.push_str(&format!("\n{} > {}\n", m.speaker, m.text));
            }
            if let Some(m) = stream.filter(|m| !archive.messages.iter().any(|a| a.key() == m.key()))
            {
                out.push_str(&format!("\nLucy > {}\n", m.text));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paused_closed_and_reopened_replay_obeys_the_clock() {
        let mut d = WakeDemo {
            open: true,
            ..Default::default()
        };
        d.play_pause(100.0);
        assert_eq!(d.step, 1);
        assert!(!d.tick(3299.0, false));
        assert!(d.tick(3300.0, false));
        d.play_pause(3400.0);
        assert!(!d.tick(10000.0, false));
        d.next();
        assert_eq!(d.step, 3);
        d.play_pause(10000.0);
        d.close();
        assert!(!d.tick(20000.0, false));
        d.open = true;
        assert!(!d.tick(20001.0, false));
        d.play_pause(20001.0);
        assert!(d.tick(23201.0, false));
        assert_eq!(d.step, 4);
    }
    #[test]
    fn handover_requires_completion_request_and_a_ready_model() {
        let mut d = WakeDemo::default();
        assert!(!d.tick(0.0, true));
        d.skip();
        assert!(!d.tick(1.0, true));
        d.handover_requested = true;
        assert!(!d.tick(2.0, false));
        assert!(d.tick(3.0, true));
        assert!(d.live && d.visited_live);
        d.restart();
        assert!(!d.live && !d.handover_requested);
        assert!(d.visited_live);
        assert!(!d.tick(4.0, true));
    }
    #[test]
    fn skip_and_restart_leave_the_shared_archive_untouched() {
        let archive = RoomArchiveV1::default();
        let original = archive.clone();
        let mut d = WakeDemo::default();
        d.skip();
        let text = d.transcript(&archive, "example", None);
        assert!(text.contains("Lucy: cached") && text.contains(ROOM_URL));
        assert!(text.contains("DEMONSTRATION"));
        d.next();
        assert!(d.complete());
        d.restart();
        assert_eq!(archive, original);
        assert!(!d
            .transcript(&archive, "example", None)
            .contains("Permission granted"));
    }
}
