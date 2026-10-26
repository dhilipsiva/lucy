//! The HUD, drawn as panels in the scene (roadmap/N8.v3.md): the top bar, my bubble pinned to
//! my head, the dock with its groups, chips and answer board, the wake overlay, the restore
//! pill. On a flat screen the panels sit in screen space; the same panels stand in the room in
//! VR (N8e). Every panel is redrawn only when the state it shows has changed.

use glam::{Mat4, Vec3};

use crate::colors as col;
use crate::colors::Verdict;
use crate::gl::{Blend, Gl, Texture};
use crate::panel::{font, Action, ChipStyle, Panel, DISPLAY, MONO, SANS};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Wake,
    Ready,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ItemKind {
    Plain,
    Recall,
    Words,
    Eyes,
}
#[derive(Clone, Debug)]
pub struct Item {
    pub label: String,
    pub kind: ItemKind,
    pub query: Option<String>,
    pub disabled: bool,
}
#[derive(Clone, Debug)]
pub struct Group {
    pub id: String,
    pub label: String,
    pub items: Vec<Item>,
}
#[derive(Clone, Debug)]
pub struct Speech {
    pub id: u64,
    pub text: String,
    pub verdict: Verdict,
    pub label: Option<String>,
    pub at_ms: f64,
    pub streaming: bool,
}
impl Speech {
    /// Reading time, then I go quiet: 7 to 18 seconds by length; a streaming answer stays.
    pub fn ttl_ms(&self) -> f64 {
        (3200.0 + self.text.chars().count() as f64 * 60.0).clamp(7000.0, 18000.0)
    }
}
#[derive(Clone, Debug)]
pub struct AnswerCard {
    pub verdict: Verdict,
    pub label: Option<String>,
    pub meta: String,
    pub title: String,
    pub query: String,
    pub body: String,
    pub why: String,
    pub proof: String,
    pub proof_open: bool,
    pub proof_page: usize,
}

pub struct HudState<'a> {
    pub desk: &'a crate::desk::Desk,
    pub drawer_body: &'a str,
    pub phase: Phase,
    pub w: f64,
    pub h: f64,
    pub counts: Option<(usize, usize, f64)>,
    pub standing: &'a [(String, String)],
    pub speech: Option<&'a Speech>,
    pub speech_stale: bool,
    pub answer: Option<&'a AnswerCard>,
    pub groups: &'a [Group],
    pub group: &'a str,
    pub legend: &'a str,
    pub hud_hidden: bool,
    pub voice: bool,
    pub fullscreen: bool,
    pub dock_min: bool,
    pub wake_lines: &'a [String],
    pub wake_progress: f32,
    pub wake_animating: bool,
    pub focus: Option<usize>,
}

const TOP_L: usize = 0;
const TOP_R: usize = 1;
const BUBBLE: usize = 2;
const DOCK: usize = 3;
const WAKE: usize = 4;
const RESTORE: usize = 5;
const FINDER: usize = 6;
const MIND: usize = 7;
const N: usize = 8;

pub enum HitResult {
    Nothing,
    Panel(Option<Action>),
}

pub struct Hud {
    panels: Vec<Panel>,
    textures: Vec<Option<Texture>>,
    drawn_version: u64,
    drawn_size: (f64, f64),
    pub bubble_pos: Option<(f64, f64)>,
    pub bubble_fits: bool,
    pub dpr: f64,
    proof_page: Option<crate::text::ProofPage>,
}
impl Hud {
    pub fn new(dpr: f64) -> Hud {
        let panels = (0..N).map(|_| Panel::new(10.0, 10.0, dpr)).collect();
        Hud {
            panels,
            textures: (0..N).map(|_| None).collect(),
            drawn_version: u64::MAX,
            proof_page: None,
            drawn_size: (0.0, 0.0),
            bubble_pos: None,
            bubble_fits: true,
            dpr,
        }
    }
    pub fn dock_top(&self) -> Option<f64> {
        let d = &self.panels[DOCK];
        if d.visible {
            Some(d.y)
        } else {
            None
        }
    }
    pub fn proof_page_action(&self, forward: bool) -> Option<Action> {
        if !self.panels[DOCK].visible {
            return None;
        }
        let page = self.proof_page?;
        let target = if forward {
            page.next()
        } else {
            page.previous()
        };
        target.map(Action::ProofPage)
    }
    /// Redraw what changed; `version` is the app's counter of visible state.
    pub fn draw(&mut self, st: &HudState, version: u64) {
        let size_changed = self.drawn_size != (st.w, st.h);
        if version == self.drawn_version && !size_changed {
            return;
        }
        self.drawn_version = version;
        self.drawn_size = (st.w, st.h);
        for p in &mut self.panels {
            p.visible = false;
        }
        match st.phase {
            Phase::Wake => {
                self.draw_wake(st);
                if st.speech.is_some() {
                    self.draw_bubble(st);
                }
            }
            Phase::Failed => {
                self.draw_bubble(st);
            }
            Phase::Ready => {
                self.draw_bubble(st);
                if st.hud_hidden {
                    self.draw_restore(st);
                } else {
                    self.draw_top(st);
                    self.draw_dock(st);
                    self.draw_mind(st);
                }
            }
        }
    }
    fn draw_top(&mut self, st: &HudState) {
        // left group: dot · Lucy. alias Nell · standing dots · mind line
        let name_f = font(600, 19.0, DISPLAY);
        let alias_f = font(400, 11.0, MONO);
        let line_f = font(400, 11.5, MONO);
        let ok = !st.standing.is_empty() && st.standing.iter().all(|(_, s)| s == "TRUE");
        let line = match st.counts {
            Some((r, f, ms)) => format!(
                "mind loaded · {r} rules · {f} facts · {} ms · {}",
                ms.round() as i64,
                if ok {
                    "standing holds"
                } else {
                    "standing failed"
                }
            ),
            None => "waking…".to_string(),
        };
        let p = &mut self.panels[TOP_L];
        let (name_w, alias_w, line_w) = (
            p.measure("Lucy.", &name_f),
            p.measure("alias Nell", &alias_f),
            p.measure(&line, &line_f),
        );
        let compact = st.w < 760.0;
        let dots_w = if st.standing.is_empty() {
            0.0
        } else {
            st.standing.len() as f64 * 9.0 + 10.0
        };
        let right_part = if compact {
            0.0
        } else {
            14.0 + dots_w + line_w + 14.0
        };
        let w = 14.0 + 6.0 + 14.0 + name_w + 8.0 + alias_w + right_part;
        let h = 52.0;
        p.resize(w.min(st.w - 32.0).max(120.0), h);
        p.x = 16.0;
        p.y = 16.0;
        p.visible = true;
        p.begin();
        p.glass(0.0, 0.0, p.w, p.h, 10.0);
        p.dot(20.0, h / 2.0, 3.0, &col::css(col::PHOSPHOR_500), true);
        let mut x = 34.0;
        x += p.text(x, 15.0, "Lucy", &name_f, &col::css(col::PAPER_100));
        x += p.text(x, 15.0, ".", &name_f, &col::css(col::EMBER_500)) + 8.0;
        x += p.text(x, 22.0, "alias Nell", &alias_f, &col::css(col::FG_FAINT)) + 14.0;
        if !compact {
            p.line(x, 14.0, x, h - 14.0, &col::css(col::VOID_500), 1.0);
            x += 14.0;
            for (_, s) in st.standing.iter() {
                p.dot(
                    x + 3.0,
                    h / 2.0,
                    3.0,
                    &col::css(if s == "TRUE" {
                        col::PHOSPHOR_500
                    } else {
                        col::CRIMSON_500
                    }),
                    s == "TRUE",
                );
                x += 9.0;
            }
            if !st.standing.is_empty() {
                x += 10.0;
            }
            p.text(x, 20.0, &line, &line_f, &col::css(col::FG_MUTED));
        }
        p.hit_add(0.0, 0.0, p.w, p.h, Action::Find);
        p.dirty = true;
        // right group: voice ∿ · full screen ⤢ · hide H
        let r = &mut self.panels[TOP_R];
        let bw = 36.0 * 5.0 + 6.0 * 4.0 + 12.0;
        r.resize(bw, 48.0);
        r.x = st.w - 16.0 - bw;
        r.y = 16.0;
        r.visible = true;
        r.begin();
        r.glass(0.0, 0.0, r.w, r.h, 10.0);
        let mut x = 6.0;
        x += r.icon_button(x, 6.0, "M", None, Action::Mind) + 6.0;
        x += r.icon_button(x, 6.0, "VR", None, Action::Xr) + 6.0;
        x += r.icon_button(
            x,
            6.0,
            "∿",
            if st.voice {
                Some(col::PHOSPHOR_700)
            } else {
                None
            },
            Action::Voice,
        ) + 6.0;
        x += r.icon_button(
            x,
            6.0,
            if st.fullscreen { "⤡" } else { "⤢" },
            None,
            Action::Fullscreen,
        ) + 6.0;
        r.icon_button(x, 6.0, "H", None, Action::HideHud);
        r.dirty = true;
    }
    fn composer(p: &mut Panel, st: &HudState, w: f64, y: f64) {
        let mut x = 14.0;
        for (id, label) in [
            ("words", "Words"),
            ("recall", "Recall"),
            ("nibli", "Nibli"),
            ("ask", "Ask · soon"),
        ] {
            x += p.tab(
                x,
                y,
                label,
                None,
                st.desk.mode == id,
                Action::Mode(id.into()),
            );
        }
        let fy = y + 38.0;
        p.fill_rounded(14.0, fy, w - 28.0, 55.0, 6.0, &col::css(col::VOID_900));
        p.hit_add(14.0, fy, w - 28.0, 55.0, Action::Focus);
        let hint = if st.desk.draft.is_empty() {
            "Write a question…"
        } else {
            &st.desk.draft
        };
        let f = font(400, 14.0, SANS);
        let lines = crate::text::wrap_measured(hint, w - 55.0, |s| p.measure(s, &f));
        for (i, l) in lines
            .iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .enumerate()
        {
            p.text(
                22.0,
                fy + 8.0 + i as f64 * 19.0,
                l,
                &f,
                &col::css(col::PAPER_100),
            );
        }
        p.ghost(14.0, fy + 62.0, "Send ↵", Action::Submit);
        p.ghost(100.0, fy + 62.0, "×", Action::ClearDraft);
        p.ghost(140.0, fy + 62.0, "⌨", Action::Keyboard);
        p.text(
            14.0,
            fy + 96.0,
            &st.desk.status,
            &font(400, 10.5, MONO),
            &col::css(col::FG_MUTED),
        );
    }
    fn draw_mind(&mut self, st: &HudState) {
        let Some(tab) = &st.desk.drawer else { return };
        self.panels[DOCK].visible = false;
        self.panels[BUBBLE].visible = false;
        let p = &mut self.panels[MIND];
        let w = (st.w - 24.0).min(760.0);
        let h = (st.h - 100.0).max(240.0);
        p.resize(w, h);
        p.x = (st.w - w) / 2.0;
        p.y = 80.0;
        p.visible = true;
        p.begin();
        p.glass(0.0, 0.0, w, h, 12.0);
        p.ghost(w - 42.0, 8.0, "×", Action::CloseMind);
        let mut x = 12.0;
        for name in ["standing", "memory", "journal", "log", "about"] {
            x += p.tab(
                x,
                44.0,
                name,
                None,
                tab == name,
                Action::MindTab(name.into()),
            );
        }
        p.text(
            16.0,
            16.0,
            "My mind",
            &font(600, 18.0, DISPLAY),
            &col::css(col::PAPER_100),
        );
        let actions = if tab == "journal" {
            vec![
                ("Export all", Action::Export(false)),
                ("This visit", Action::Export(true)),
                ("Restore", Action::Restore),
                ("Keep storage", Action::Persist),
                ("Retry storage", Action::RetryStorage),
                (
                    if st.desk.forget_confirm {
                        "Confirm forget"
                    } else {
                        "Forget"
                    },
                    Action::Forget,
                ),
            ]
        } else if tab == "standing" {
            vec![("Verify standing", Action::Group("standing".into()))]
        } else {
            vec![]
        };
        let mut x = 12.0;
        let mut y = 88.0;
        for (label, a) in actions {
            let size = p.measure(label, &font(400, 12.0, MONO)) + 24.0;
            if x + size > w - 12.0 {
                x = 12.0;
                y += 32.0;
            }
            p.ghost(x, y, label, a);
            x += size;
        }
        if tab == "journal" {
            y += 34.0;
            p.ghost(
                12.0,
                y,
                if st.desk.seat {
                    "Seat: dhilipsiva (self-report)"
                } else {
                    "Seat: visitor · I am dhilipsiva"
                },
                Action::Seat,
            );
        }
        y += 40.0;
        let f = font(400, 13.0, MONO);
        let lines = crate::text::wrap_preformatted(st.drawer_body, w - 32.0, |s| p.measure(s, &f));
        let rows = ((h - y - 48.0) / 20.0).max(1.0) as usize;
        let page = crate::text::ProofPage::new(lines.len(), st.desk.page, rows);
        for (i, l) in lines.iter().skip(page.start).take(rows).enumerate() {
            p.text(16.0, y + i as f64 * 20.0, l, &f, &col::css(col::PAPER_100));
        }
        if page.previous().is_some() {
            p.ghost(16.0, h - 36.0, "‹ Previous", Action::DrawerPage(-1));
        }
        if page.next().is_some() {
            p.ghost(w - 110.0, h - 36.0, "Next ›", Action::DrawerPage(1));
        }
        p.text(
            w / 2.0 - 32.0,
            h - 28.0,
            &format!("{} / {}", page.index + 1, page.count),
            &f,
            &col::css(col::FG_MUTED),
        );
    }
    fn draw_bubble(&mut self, st: &HudState) {
        let Some(sp) = st.speech else {
            self.panels[BUBBLE].visible = false;
            return;
        };
        let p = &mut self.panels[BUBBLE];
        let w = 440f64.min(st.w - 32.0);
        let text_f = font(400, 15.0, SANS);
        let lh = 22.0;
        p.ctx().set_font(&text_f);
        let lines = crate::text::wrap_measured(&sp.text, w - 32.0, |t| p.measure(t, &text_f));
        let n = lines.len().min(8);
        let body_h = n as f64 * lh;
        let h = 12.0 + 18.0 + 6.0 + body_h + 13.0 + 9.0; // the tail below
        p.resize(w, h);
        p.visible = true;
        p.begin();
        let line_c = col::css(sp.verdict.line());
        p.fill_rounded(
            0.75,
            0.75,
            w - 1.5,
            h - 9.0 - 1.5,
            12.0,
            &col::css_a(col::VOID_700, 0.92),
        );
        p.stroke_rounded(
            0.0,
            0.0,
            w,
            h - 9.0,
            12.0,
            &line_c,
            1.5,
            sp.verdict == Verdict::Words,
        );
        // the tail: a small square turned on its corner, filled, with the two lower edges stroked
        let c = p.ctx();
        let cx = w / 2.0;
        let base = h - 9.0;
        c.begin_path();
        c.move_to(cx - 8.0, base - 1.0);
        c.line_to(cx, base + 7.5);
        c.line_to(cx + 8.0, base - 1.0);
        c.close_path();
        c.set_fill_style_str(&col::css_a(col::VOID_700, 0.92));
        c.fill();
        c.begin_path();
        c.move_to(cx - 8.0, base - 0.75);
        c.line_to(cx, base + 7.5);
        c.line_to(cx + 8.0, base - 0.75);
        c.set_stroke_style_str(&line_c);
        c.set_line_width(1.5);
        c.stroke();
        c.set_fill_style_str(&col::css_a(col::VOID_700, 0.92));
        c.fill_rect(cx - 7.0, base - 2.5, 14.0, 2.0);
        let (cw, _) = p.vchip(16.0, 12.0, sp.verdict, sp.label.as_deref());
        if sp.streaming {
            p.text(
                16.0 + cw + 8.0,
                12.0,
                "▌",
                &font(400, 12.0, MONO),
                &col::css(col::EMBER_500),
            );
        }
        for (i, l) in lines.iter().take(n).enumerate() {
            p.text(
                16.0,
                36.0 + i as f64 * lh,
                l,
                &text_f,
                &col::css(col::PAPER_100),
            );
        }
        p.opacity = if st.speech_stale { 0.0 } else { 1.0 };
        p.dirty = true;
        if let Some((x, y)) = self.bubble_pos {
            let p = &mut self.panels[BUBBLE];
            p.x = x - p.w / 2.0;
            p.y = y - p.h;
        }
    }
    /// Pin the bubble above her head, clamped inside the viewport and above the dock; eased.
    pub fn place_bubble(&mut self, hx: f64, hy: f64, visible: bool, w: f64, h: f64) {
        let dock_top = if self.panels[DOCK].visible {
            self.panels[DOCK].y - 14.0
        } else {
            h - 20.0
        };
        let p = &mut self.panels[BUBBLE];
        let floor = p.h + 84.0;
        let half = p.w / 2.0 + 12.0;
        let cx = hx.max(half).min(w - half);
        let cy = hy.max(floor).min(dock_top);
        let fits = dock_top >= floor && w - half > half;
        let (px, py) = match self.bubble_pos {
            Some((x, y)) => (x + (cx - x) * 0.35, y + (cy - y) * 0.35),
            None => (cx, cy),
        };
        self.bubble_pos = Some((px, py));
        p.x = px - p.w / 2.0;
        p.y = py - p.h;
        self.bubble_fits = visible && fits;
    }
    fn draw_dock(&mut self, st: &HudState) {
        self.proof_page = None;
        let two = st.answer.is_some() && st.w >= 1180.0 && !st.dock_min;
        let w = if two {
            1120f64.min(st.w - 32.0)
        } else {
            760f64.min(st.w - 32.0)
        };
        let left_w = if two { w * 1.3 / 2.3 } else { w };
        let p = &mut self.panels[DOCK];
        // measure the left column first
        let group = st
            .groups
            .iter()
            .find(|g| g.id == st.group)
            .or(st.groups.first());
        let chip_f = font(400, 13.0, MONO);
        let mut rows: Vec<Vec<(usize, f64)>> = Vec::new();
        if let Some(g) = group {
            let mut row: Vec<(usize, f64)> = Vec::new();
            let mut x = 0.0;
            for (i, it) in g.items.iter().enumerate() {
                let gw = match it.kind {
                    ItemKind::Recall => p.measure("#", &chip_f) + 4.0,
                    _ => 0.0,
                };
                let cw = p.measure(&it.label, &chip_f) + gw + 24.0;
                if x + cw > left_w - 28.0 && !row.is_empty() {
                    rows.push(std::mem::take(&mut row));
                    x = 0.0;
                }
                row.push((i, cw));
                x += cw + 8.0;
            }
            if !row.is_empty() {
                rows.push(row);
            }
        }
        let page_rows = if st.w < 600.0 { 1 } else { 2 };
        let pages = rows.len().div_ceil(page_rows).max(1);
        let page = st.desk.chip_page.min(pages - 1);
        let chip_rows = rows.len().saturating_sub(page * page_rows).min(page_rows);
        let composer_h = if st.group == "talk" { 156.0 } else { 0.0 };
        let paging_h = if pages > 1 { 28.0 } else { 0.0 };
        let left_h = if st.dock_min {
            44.0
        } else {
            10.0 + 36.0 + 8.0 + chip_rows as f64 * 42.0 + 12.0 + paging_h + composer_h
        };
        let foot_h = if st.dock_min { 0.0 } else { 30.0 };
        // the answer board's height
        let mut right_h = 0.0;
        let title_f = font(400, 15.0, SANS);
        let why_f = font(400, 13.0, MONO);
        let pre_f = font(400, 12.0, MONO);
        let right_w = if two { w - left_w } else { w };
        let mut proof_lines: Vec<String> = Vec::new();
        let mut title_lines: Vec<String> = Vec::new();
        let mut why_lines: Vec<String> = Vec::new();
        let mut body_lines: Vec<String> = Vec::new();
        if let Some(a) = st.answer {
            p.ctx().set_font(&title_f);
            title_lines = crate::text::wrap_measured(&a.title, right_w - 32.0 - 36.0, |t| {
                p.measure(t, &title_f)
            });
            title_lines.truncate(3);
            p.ctx().set_font(&why_f);
            why_lines = if a.why.is_empty() {
                Vec::new()
            } else {
                crate::text::wrap_measured(&a.why, right_w - 32.0, |t| p.measure(t, &why_f))
            };
            why_lines.truncate(4);
            body_lines = if a.body.is_empty() {
                Vec::new()
            } else {
                crate::text::wrap_measured(&a.body, right_w - 32.0, |t| p.measure(t, &title_f))
            };
            body_lines.truncate(10);
            if a.proof_open && !st.dock_min {
                proof_lines = crate::text::wrap_preformatted(&a.proof, right_w - 56.0, |t| {
                    p.measure(t, &pre_f)
                });
            }
            right_h = 12.0
                + 18.0
                + 8.0
                + title_lines.len() as f64 * 22.0
                + 6.0
                + (if a.query.is_empty() { 0.0 } else { 20.0 })
                + body_lines.len() as f64 * 22.0
                + (if body_lines.is_empty() { 0.0 } else { 8.0 })
                + why_lines.len() as f64 * 20.0
                + (if why_lines.is_empty() { 0.0 } else { 8.0 })
                + 22.0
                + 12.0;
            if a.proof_open && !st.dock_min {
                // Keep the proof below the top HUD, even when the controls and
                // answer stack. Paging exposes every line without growing the canvas.
                let stacked = if two { 0.0 } else { left_h };
                let available = st.h - 84.0 - 16.0 - foot_h - stacked - right_h - 56.0;
                let rows = (available / 19.2).floor().clamp(1.0, 14.0) as usize;
                let page = crate::text::ProofPage::new(proof_lines.len(), a.proof_page, rows);
                right_h += 56.0 + page.rows as f64 * 19.2;
                self.proof_page = Some(page);
            }
        }
        let h = if two {
            left_h.max(right_h) + foot_h
        } else if st.answer.is_some() && !st.dock_min {
            left_h + right_h + foot_h
        } else {
            left_h + foot_h
        };
        p.resize(w, h);
        p.x = (st.w - w) / 2.0;
        p.y = st.h - 16.0 - h;
        p.visible = true;
        p.begin();
        p.glass(0.0, 0.0, w, h, 10.0);
        // left column
        if st.dock_min {
            p.ghost(w - 110.0, 8.0, "controls ▴", Action::ToggleMin);
            p.text(
                14.0,
                14.0,
                "Lucy, her room: the controls are folded.",
                &font(400, 12.5, MONO),
                &col::css(col::FG_MUTED),
            );
        } else {
            let mut x = 14.0;
            for g in st.groups {
                let sel = g.id == st.group;
                x += p.tab(
                    x,
                    10.0,
                    &g.label,
                    if st.w < 600.0 {
                        None
                    } else {
                        Some(g.items.len())
                    },
                    sel,
                    Action::Group(g.id.clone()),
                );
            }
            p.line(
                14.0,
                46.0,
                left_w - 14.0,
                46.0,
                &col::css(col::VOID_500),
                1.0,
            );
            p.ghost(left_w - 42.0, 8.0, "▾", Action::ToggleMin);
            if let Some(g) = group {
                let mut y = 54.0;
                for row in rows.iter().skip(page * page_rows).take(chip_rows) {
                    let mut x = 14.0;
                    for (i, _) in row {
                        let it = &g.items[*i];
                        let style = match it.kind {
                            ItemKind::Plain => ChipStyle::plain(),
                            ItemKind::Recall => ChipStyle::recall(),
                            ItemKind::Words => ChipStyle::words(),
                            ItemKind::Eyes => ChipStyle::eyes(),
                        };
                        let style = if it.disabled { style.disabled() } else { style };
                        x += p.chip(
                            x,
                            y,
                            &it.label,
                            style,
                            Action::Control(*i),
                            st.focus == Some(*i),
                        ) + 8.0;
                    }
                    y += 42.0;
                }
            }
            let y = 54.0 + chip_rows as f64 * 42.0;
            if pages > 1 {
                if page > 0 {
                    p.ghost(14.0, y, "‹", Action::ChipPage(-1));
                }
                p.text(
                    60.0,
                    y + 4.0,
                    &format!("{} / {pages}", page + 1),
                    &font(400, 12.0, MONO),
                    &col::css(col::FG_MUTED),
                );
                if page + 1 < pages {
                    p.ghost(140.0, y, "›", Action::ChipPage(1));
                }
            }
            if st.group == "talk" {
                Self::composer(p, st, left_w, y + paging_h);
            }
            if two {
                p.line(
                    left_w,
                    0.0,
                    left_w,
                    h - foot_h,
                    &col::css(col::VOID_500),
                    1.0,
                );
            }
        }
        // the answer board
        if let Some(a) = st.answer {
            if !st.dock_min {
                let (ox, oy) = if two { (left_w, 0.0) } else { (0.0, left_h) };
                if !two {
                    p.line(14.0, oy, w - 14.0, oy, &col::css(col::VOID_500), 1.0);
                }
                let mut y = oy + 12.0;
                let x = ox + 16.0;
                p.ghost(ox + right_w - 44.0, oy + 6.0, "✕", Action::ClearAnswer);
                let (cw, _) = p.vchip(x, y, a.verdict, a.label.as_deref());
                p.text(
                    x + cw + 8.0,
                    y + 3.0,
                    &a.meta,
                    &font(400, 11.5, MONO),
                    &col::css(col::FG_FAINT),
                );
                y += 18.0 + 8.0;
                for l in &title_lines {
                    p.text(x, y, l, &title_f, &col::css(col::PAPER_100));
                    y += 22.0;
                }
                y += 6.0;
                if !a.query.is_empty() {
                    p.text(
                        x,
                        y,
                        &a.query,
                        &font(400, 12.5, MONO),
                        &col::css(col::EMBER_500),
                    );
                    y += 20.0;
                }
                for l in &body_lines {
                    p.text(x, y, l, &title_f, &col::css(col::PAPER_200));
                    y += 22.0;
                }
                if !body_lines.is_empty() {
                    y += 8.0;
                }
                for l in &why_lines {
                    p.text(x, y, l, &why_f, &col::css(col::FG_MUTED));
                    y += 20.0;
                }
                if !why_lines.is_empty() {
                    y += 8.0;
                }
                let summary = if a.proof_open {
                    "PROOF ▾"
                } else {
                    "PROOF ▸"
                };
                let sw = p.text(
                    x,
                    y + 4.0,
                    summary,
                    &font(500, 12.0, MONO),
                    &col::css(col::FG_MUTED),
                );
                p.hit_add(x - 4.0, y, sw + 24.0, 22.0, Action::ToggleProof);
                y += 22.0;
                if let Some(page) = self.proof_page {
                    let ph = page.rows as f64 * 19.2 + 46.0;
                    p.fill_rounded(
                        x,
                        y + 4.0,
                        right_w - 32.0,
                        ph,
                        6.0,
                        &col::css(col::VOID_1000),
                    );
                    p.stroke_rounded(
                        x,
                        y + 4.0,
                        right_w - 32.0,
                        ph,
                        6.0,
                        &col::css(col::VOID_500),
                        1.0,
                        false,
                    );
                    // A glyph wider than the available width must still never
                    // paint through the border. The source remains in the text mirror.
                    p.ctx().save();
                    p.ctx().begin_path();
                    p.ctx().rect(
                        x + 12.0,
                        y + 12.0,
                        right_w - 56.0,
                        page.rows as f64 * 19.2 + 2.0,
                    );
                    p.ctx().clip();
                    for (i, l) in proof_lines[page.start..page.end].iter().enumerate() {
                        p.text(
                            x + 12.0,
                            y + 14.0 + i as f64 * 19.2,
                            l,
                            &pre_f,
                            &col::css(col::PAPER_200),
                        );
                    }
                    p.ctx().restore();
                    let nav_y = y + 14.0 + page.rows as f64 * 19.2;
                    let range = format!(
                        "{}–{} / {} lines",
                        if proof_lines.is_empty() {
                            0
                        } else {
                            page.start + 1
                        },
                        page.end,
                        proof_lines.len()
                    );
                    p.text(
                        x + 12.0,
                        nav_y + 7.0,
                        &range,
                        &font(400, 10.5, MONO),
                        &col::css(col::FG_MUTED),
                    );
                    let mut nav_x = x + right_w - 44.0;
                    for (label, target) in [("Next ›", page.next()), ("‹ Prev", page.previous())]
                    {
                        if let Some(target) = target {
                            let width = p.measure(label, &font(500, 12.0, MONO)) + 20.0;
                            nav_x -= width;
                            p.ghost(nav_x, nav_y, label, Action::ProofPage(target));
                        }
                    }
                }
            }
        }
        // the foot: the legend and the note
        if !st.dock_min {
            let fy = h - foot_h;
            p.line(0.0, fy, w, fy, &col::css(col::VOID_500), 1.0);
            let note = "verdicts: the engine's · words: words · nothing leaves your tab";
            let nf = font(400, 11.0, MONO);
            let nw = p.measure(note, &nf);
            p.text(w - 14.0 - nw, fy + 9.0, note, &nf, &col::css(col::FG_FAINT));
            let legend =
                crate::text::ellipsize(st.legend, ((w - 14.0 - nw - 30.0) / 6.6).max(8.0) as usize);
            p.text(14.0, fy + 9.0, &legend, &nf, &col::css(col::FG_FAINT));
        }
        p.dirty = true;
    }
    fn draw_wake(&mut self, st: &HudState) {
        let p = &mut self.panels[WAKE];
        let w = 520f64.min(st.w - 48.0);
        let lines: Vec<&String> = st.wake_lines.iter().rev().take(9).rev().collect();
        let h = 24.0 + lines.len().max(1) as f64 * 19.2 + 18.0;
        p.resize(w, h);
        p.x = 24.0;
        p.y = st.h - 28.0 - h;
        p.visible = true;
        p.begin();
        p.eyebrow(0.0, 0.0, "// lucy · wake");
        let skip = if st.wake_animating {
            "any key or a click skips"
        } else {
            "loading my mind…"
        };
        let sf = font(400, 11.0, MONO);
        let sw = p.measure(skip, &sf);
        p.text(w - sw, 1.0, skip, &sf, &col::css(col::FG_FAINT));
        let lf = font(400, 12.0, MONO);
        for (i, l) in lines.iter().enumerate() {
            let last = i + 1 == lines.len();
            let shown = crate::text::ellipsize(l, (w / 7.2) as usize);
            p.text(
                0.0,
                24.0 + i as f64 * 19.2,
                &shown,
                &lf,
                &col::css(if last { col::PAPER_200 } else { col::FG_FAINT }),
            );
        }
        let by = h - 2.0;
        p.fill_rounded(0.0, by, w, 2.0, 1.0, &col::css(col::VOID_500));
        let pw = (w * st.wake_progress.clamp(0.0, 1.0) as f64).max(0.0);
        if pw > 0.0 {
            let c = p.ctx();
            c.set_shadow_color(&col::css(col::PHOSPHOR_500));
            c.set_shadow_blur(8.0);
            p.fill_rounded(0.0, by, pw, 2.0, 1.0, &col::css(col::PHOSPHOR_500));
            c.set_shadow_blur(0.0);
        }
        p.dirty = true;
    }
    fn draw_restore(&mut self, st: &HudState) {
        let p = &mut self.panels[RESTORE];
        let f = font(500, 12.0, MONO);
        let label = "show the HUD · H";
        let tw = p.measure(label, &f);
        let (w, h) = (tw + 24.0, 28.0);
        p.resize(w, h);
        p.x = st.w - 16.0 - w;
        p.y = st.h - 16.0 - h;
        p.visible = true;
        p.begin();
        p.fill_rounded(0.0, 0.0, w, h, 6.0, &col::css_a(col::VOID_900, 0.84));
        p.stroke_rounded(0.0, 0.0, w, h, 6.0, &col::css(col::VOID_400), 1.0, false);
        p.text(12.0, 7.0, label, &f, &col::css(col::PAPER_100));
        p.hit_add(0.0, 0.0, w, h, Action::ShowHud);
        p.dirty = true;
    }
    /// The finder pill when she is off-screen: at the edge she is behind, pointing the way.
    pub fn draw_finder(&mut self, st: &HudState, v: Vec3, on_screen: bool) {
        let dock_top = if self.panels[DOCK].visible {
            self.panels[DOCK].y - 36.0
        } else {
            st.h - 60.0
        };
        let p = &mut self.panels[FINDER];
        if on_screen || st.phase != Phase::Ready || st.hud_hidden {
            p.visible = false;
            return;
        }
        let behind = v.z > 1.0;
        let (mut nx, mut ny) = if behind { (-v.x, -v.y) } else { (v.x, v.y) };
        if nx.abs() < 0.15 && ny.abs() < 0.15 {
            ny = -1.0;
        }
        let m = nx.abs().max(ny.abs()).max(1e-3);
        nx /= m;
        ny /= m;
        let px = (st.w / 2.0 + nx as f64 * (st.w / 2.0 - 76.0)).round();
        let py = (st.h / 2.0 - ny as f64 * (st.h / 2.0 - 60.0))
            .clamp(84.0, dock_top)
            .round();
        let glyph = if nx.abs() > ny.abs() {
            if nx < 0.0 {
                "◂"
            } else {
                "▸"
            }
        } else if ny > 0.0 {
            "▴"
        } else {
            "▾"
        };
        let f = font(400, 12.5, MONO);
        let label = if nx < 0.0 {
            format!("{glyph}  Lucy")
        } else {
            format!("Lucy  {glyph}")
        };
        let tw = p.measure(&label, &f);
        let (w, h) = (tw + 28.0 + 16.0, 34.0);
        p.resize(w, h);
        p.x = px - w / 2.0;
        p.y = py - h / 2.0;
        p.visible = true;
        p.begin();
        p.fill_rounded(0.0, 0.0, w, h, 17.0, &col::css_a(col::VOID_900, 0.84));
        p.stroke_rounded(0.0, 0.0, w, h, 17.0, &col::css(col::VOID_500), 1.0, false);
        let dot_x = if nx < 0.0 {
            14.0 + p.measure(glyph, &f) + 10.0
        } else {
            14.0
        };
        p.dot(
            dot_x + 4.0,
            h / 2.0,
            4.0,
            &col::css(col::PHOSPHOR_500),
            true,
        );
        p.text(
            if nx < 0.0 { 14.0 } else { dot_x + 16.0 },
            9.0,
            &label,
            &f,
            &col::css(col::PAPER_100),
        );
        p.hit_add(0.0, 0.0, w, h, Action::Find);
        p.dirty = true;
    }
    /// Upload the changed panels and draw every visible one in screen space.
    pub fn render(&mut self, gl: &Gl, quad: &crate::gl::GpuMesh, w: f64, h: f64) {
        let ortho = Mat4::orthographic_rh_gl(0.0, w as f32, h as f32, 0.0, -1.0, 1.0);
        let order = [WAKE, DOCK, TOP_L, TOP_R, RESTORE, FINDER, BUBBLE, MIND];
        for &i in &order {
            let p = &mut self.panels[i];
            if !p.visible {
                continue;
            }
            if p.dirty || self.textures[i].is_none() {
                match &self.textures[i] {
                    Some(t) => gl.update_texture(t, &p.canvas, true),
                    None => self.textures[i] = Some(gl.texture_from_canvas(&p.canvas, true)),
                }
                p.dirty = false;
            }
            let opacity = if i == BUBBLE && !self.bubble_fits {
                0.0
            } else {
                p.opacity
            };
            if opacity <= 0.0 {
                continue;
            }
            let model = Mat4::from_translation(Vec3::new(
                (p.x + p.w / 2.0) as f32,
                (p.y + p.h / 2.0) as f32,
                0.0,
            )) * Mat4::from_scale(Vec3::new(p.w as f32, p.h as f32, 1.0));
            gl.draw_flat(
                quad,
                &(ortho * model),
                self.textures[i].as_ref(),
                [1.0, 1.0, 1.0, opacity],
                Blend::Premultiplied,
                false,
                false,
            );
        }
    }
    /// What is under a screen point: nothing, or a panel (with its action, if any).
    pub fn hit(&self, sx: f64, sy: f64) -> HitResult {
        for &i in &[MIND, RESTORE, FINDER, DOCK, TOP_R, TOP_L] {
            let p = &self.panels[i];
            if p.contains(sx, sy) {
                return HitResult::Panel(p.hit(sx - p.x, sy - p.y));
            }
        }
        HitResult::Nothing
    }
    pub fn over_panel(&self, sx: f64, sy: f64) -> bool {
        matches!(self.hit(sx, sy), HitResult::Panel(_))
    }
}
