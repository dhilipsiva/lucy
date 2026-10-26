//! Panels: the interface drawn in the scene (roadmap/N8.v3.md). Each panel is a 2D canvas the
//! browser rasterizes with the self-hosted QUINE fonts, uploaded as a texture on a quad: on a
//! flat screen in screen space, in VR in the room. Hit regions are recorded as the panel is
//! drawn, so a click, a tap, a controller ray or a pad focus all land the same way.

use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement};

use crate::colors as col;
use crate::colors::Verdict;

#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Control(usize),
    Group(String),
    Mode(String),
    ToggleProof,
    ProofPage(usize),
    ClearAnswer,
    ToggleMin,
    Voice,
    Fullscreen,
    HideHud,
    ShowHud,
    Mind,
    MindTab(String),
    CloseMind,
    Find,
    Xr,
    Focus,
    ChipPage(i32),
    Submit,
    ClearDraft,
    Keyboard,
    Key(String),
    DrawerPage(i32),
    Export(bool),
    Restore,
    Forget,
    Persist,
    RetryStorage,
    Seat,
    Eye(String),
    Relay(String),
    LoadWords,
    StopWords,
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub action: Action,
}

thread_local! {static TEXTURE_LIMIT:std::cell::Cell<f64> = const {std::cell::Cell::new(4096.0)};}
pub fn texture_limit(limit: f64) {
    TEXTURE_LIMIT.with(|l| l.set(limit.max(1.0)));
}
pub const MONO: &str = "'IBM Plex Mono', ui-monospace, Menlo, monospace";
pub const SANS: &str = "'IBM Plex Sans', system-ui, sans-serif";
pub const DISPLAY: &str = "'Space Grotesk', 'IBM Plex Sans', sans-serif";
pub fn font(weight: u32, px: f64, family: &str) -> String {
    format!("{weight} {px}px {family}")
}

pub struct Panel {
    pub canvas: HtmlCanvasElement,
    ctx: CanvasRenderingContext2d,
    pub w: f64,
    pub h: f64,
    pub dpr: f64,
    requested_dpr: f64,
    pub control_h: f64,
    pub tex: Option<usize>,
    pub hits: Vec<Hit>,
    pub x: f64,
    pub y: f64,
    pub visible: bool,
    pub opacity: f32,
    pub dirty: bool,
}
impl Panel {
    pub fn new(w: f64, h: f64, dpr: f64) -> Panel {
        let doc = web_sys::window().unwrap().document().unwrap();
        let canvas: HtmlCanvasElement = doc.create_element("canvas").unwrap().dyn_into().unwrap();
        let ctx: CanvasRenderingContext2d = canvas
            .get_context("2d")
            .unwrap()
            .unwrap()
            .dyn_into()
            .unwrap();
        let mut p = Panel {
            canvas,
            ctx,
            w: 1.0,
            h: 1.0,
            dpr,
            requested_dpr: dpr,
            control_h: 34.0,
            tex: None,
            hits: Vec::new(),
            x: 0.0,
            y: 0.0,
            visible: true,
            opacity: 1.0,
            dirty: true,
        };
        p.resize(w, h);
        p
    }
    pub fn pixel_ratio(&mut self, dpr: f64) {
        self.requested_dpr = dpr;
        self.resize(self.w, self.h);
    }
    /// Size in CSS pixels; the canvas backs it at the device ratio.
    pub fn resize(&mut self, w: f64, h: f64) {
        self.w = w.max(1.0);
        self.h = h.max(1.0);
        self.dpr = TEXTURE_LIMIT.with(|l| {
            self.requested_dpr
                .min(l.get() / self.w)
                .min(l.get() / self.h)
        });
        self.canvas.set_width((self.w * self.dpr).ceil() as u32);
        self.canvas.set_height((self.h * self.dpr).ceil() as u32);
        self.ctx
            .set_transform(self.dpr, 0.0, 0.0, self.dpr, 0.0, 0.0)
            .ok();
        self.dirty = true;
    }
    pub fn begin(&mut self) {
        self.ctx
            .set_transform(self.dpr, 0.0, 0.0, self.dpr, 0.0, 0.0)
            .ok();
        self.ctx.clear_rect(0.0, 0.0, self.w, self.h);
        self.ctx.set_text_baseline("top");
        self.hits.clear();
    }
    pub fn hit_add(&mut self, x: f64, y: f64, w: f64, h: f64, action: Action) {
        self.hits.push(Hit { x, y, w, h, action });
    }
    /// The action under a point in panel coordinates.
    pub fn hit(&self, px: f64, py: f64) -> Option<Action> {
        self.hits
            .iter()
            .rev()
            .find(|h| px >= h.x && px <= h.x + h.w && py >= h.y && py <= h.y + h.h)
            .map(|h| h.action.clone())
    }
    pub fn contains(&self, sx: f64, sy: f64) -> bool {
        self.visible
            && sx >= self.x
            && sx <= self.x + self.w
            && sy >= self.y
            && sy <= self.y + self.h
    }

    // ── primitives ──
    fn path_rounded(&self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        let c = &self.ctx;
        let r = r.min(w / 2.0).min(h / 2.0);
        c.begin_path();
        c.move_to(x + r, y);
        c.arc_to(x + w, y, x + w, y + h, r).ok();
        c.arc_to(x + w, y + h, x, y + h, r).ok();
        c.arc_to(x, y + h, x, y, r).ok();
        c.arc_to(x, y, x + w, y, r).ok();
        c.close_path();
    }
    pub fn fill_rounded(&self, x: f64, y: f64, w: f64, h: f64, r: f64, fill: &str) {
        self.path_rounded(x, y, w, h, r);
        self.ctx.set_fill_style_str(fill);
        self.ctx.fill();
    }
    pub fn stroke_rounded(
        &self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        r: f64,
        stroke: &str,
        width: f64,
        dashed: bool,
    ) {
        self.path_rounded(x + width / 2.0, y + width / 2.0, w - width, h - width, r);
        if dashed {
            let arr = js_sys::Array::new();
            arr.push(&4.0.into());
            arr.push(&3.0.into());
            self.ctx.set_line_dash(&arr).ok();
        } else {
            self.ctx.set_line_dash(&js_sys::Array::new()).ok();
        }
        self.ctx.set_stroke_style_str(stroke);
        self.ctx.set_line_width(width);
        self.ctx.stroke();
        self.ctx.set_line_dash(&js_sys::Array::new()).ok();
    }
    /// The glass surface: the void at 84%, a hairline border, radius 10 (no blur on a texture).
    pub fn glass(&self, x: f64, y: f64, w: f64, h: f64, r: f64) {
        self.fill_rounded(x, y, w, h, r, &col::css_a(col::VOID_900, 0.86));
        self.stroke_rounded(x, y, w, h, r, &col::css(col::VOID_500), 1.0, false);
    }
    pub fn line(&self, x0: f64, y0: f64, x1: f64, y1: f64, color: &str, width: f64) {
        let c = &self.ctx;
        c.begin_path();
        c.move_to(x0, y0);
        c.line_to(x1, y1);
        c.set_stroke_style_str(color);
        c.set_line_width(width);
        c.stroke();
    }
    pub fn measure(&self, s: &str, f: &str) -> f64 {
        self.ctx.set_font(f);
        self.ctx.measure_text(s).map(|m| m.width()).unwrap_or(0.0)
    }
    pub fn text(&self, x: f64, y: f64, s: &str, f: &str, color: &str) -> f64 {
        self.ctx.set_font(f);
        self.ctx.set_fill_style_str(color);
        self.ctx.fill_text(s, x, y).ok();
        self.ctx.measure_text(s).map(|m| m.width()).unwrap_or(0.0)
    }
    /// Text wrapped to a width; returns the height used.
    pub fn text_wrapped(
        &self,
        x: f64,
        y: f64,
        s: &str,
        f: &str,
        color: &str,
        max_w: f64,
        line_h: f64,
        max_lines: usize,
    ) -> f64 {
        self.ctx.set_font(f);
        let lines = crate::text::wrap_measured(s, max_w, |t| {
            self.ctx.measure_text(t).map(|m| m.width()).unwrap_or(0.0)
        });
        let n = lines.len().min(max_lines);
        self.ctx.set_fill_style_str(color);
        for (i, l) in lines.iter().take(n).enumerate() {
            let shown = if i + 1 == n && lines.len() > n {
                format!("{}…", l.trim_end())
            } else {
                l.clone()
            };
            self.ctx.fill_text(&shown, x, y + i as f64 * line_h).ok();
        }
        n as f64 * line_h
    }
    pub fn dot(&self, x: f64, y: f64, r: f64, color: &str, glow: bool) {
        let c = &self.ctx;
        if glow {
            c.set_shadow_color(color);
            c.set_shadow_blur(6.0);
        }
        c.begin_path();
        c.arc(x, y, r, 0.0, std::f64::consts::TAU).ok();
        c.set_fill_style_str(color);
        c.fill();
        c.set_shadow_blur(0.0);
        c.set_shadow_color("transparent");
    }
    /// A verdict chip: 10.5px mono 500, letter-spaced caps, a 1px border in the verdict colour.
    pub fn vchip(&self, x: f64, y: f64, v: Verdict, label: Option<&str>) -> (f64, f64) {
        let f = font(500, 10.5, MONO);
        let label = label.unwrap_or(v.label()).to_uppercase();
        let spaced: String = label.chars().flat_map(|c| [c, '\u{200A}']).collect(); // a hair space for the .12em tracking
        let tw = self.measure(&spaced, &f);
        let dot = if v.dot() { 12.0 } else { 0.0 };
        let (w, h) = (tw + 14.0 + dot, 18.0);
        self.fill_rounded(x, y, w, h, 4.0, &col::css_a(col::VOID_900, 0.7));
        self.stroke_rounded(x, y, w, h, 4.0, &col::css(v.line()), 1.0, false);
        if v.dot() {
            self.dot(x + 10.0, y + h / 2.0, 3.0, &col::css(v.fg()), true);
        }
        self.text(x + 7.0 + dot, y + 4.0, &spaced, &f, &col::css(v.fg()));
        (w, h)
    }
    /// A chip (q-btn secondary): 34px high, 12px side padding, 13px mono 400.
    pub fn chip(
        &mut self,
        x: f64,
        y: f64,
        label: &str,
        style: ChipStyle,
        action: Action,
        focused: bool,
    ) -> f64 {
        let f = font(400, 13.0, MONO);
        let glyph_w = style
            .glyph
            .map(|g| self.measure(g, &f) + 4.0)
            .unwrap_or(0.0);
        let tw = self.measure(label, &f);
        let w = tw + glyph_w + 24.0;
        let h = self.control_h;
        if focused {
            self.stroke_rounded(
                x - 3.0,
                y - 3.0,
                w + 6.0,
                h + 6.0,
                8.0,
                &col::css(col::EMBER_500),
                2.0,
                false,
            );
        }
        self.fill_rounded(x, y, w, h, 6.0, &col::css_a(col::VOID_900, 0.35));
        self.stroke_rounded(x, y, w, h, 6.0, &col::css(style.border), 1.0, style.dashed);
        let mut tx = x + 12.0;
        if let Some(g) = style.glyph {
            tx += self.text(tx, y + 9.5, g, &f, &col::css(style.glyph_color)) + 4.0;
        }
        self.text(tx, y + 9.5, label, &f, &col::css(style.text));
        if style.disabled {
            self.fill_rounded(x, y, w, h, 6.0, &col::css_a(col::VOID_900, 0.45));
        }
        self.hit_add(x, y, w, h, action);
        w
    }
    /// A small ghost button (q-btn ghost sm): 28px, 12px mono.
    pub fn ghost(&mut self, x: f64, y: f64, label: &str, action: Action) -> f64 {
        let f = font(500, 12.0, MONO);
        let tw = self.measure(label, &f);
        let (w, h) = if self.control_h >= 44.0 {
            ((tw + 20.0).max(44.0), 44.0)
        } else {
            (tw + 20.0, 28.0)
        };
        self.text(x + 10.0, y + 7.0, label, &f, &col::css(col::FG_MUTED));
        self.hit_add(x, y, w, h, action);
        w
    }
    /// A 36px bordered square glyph button (icon-btn).
    pub fn icon_button(
        &mut self,
        x: f64,
        y: f64,
        glyph: &str,
        on: Option<u32>,
        action: Action,
    ) -> f64 {
        let s = self.control_h.max(36.0);
        let border = on.unwrap_or(col::VOID_500);
        self.stroke_rounded(x, y, s, s, 6.0, &col::css(border), 1.0, false);
        let f = font(600, 15.0, MONO);
        let tw = self.measure(glyph, &f);
        self.text(
            x + (s - tw) / 2.0,
            y + 10.0,
            glyph,
            &f,
            &col::css(on.unwrap_or(col::FG_MUTED)),
        );
        self.hit_add(x, y, s, s, action);
        s
    }
    /// A tab (q-tab): mono 500 14px, muted or strong, a 2px ember underline when selected.
    pub fn tab(
        &mut self,
        x: f64,
        y: f64,
        label: &str,
        count: Option<usize>,
        selected: bool,
        action: Action,
    ) -> f64 {
        let f = font(500, 13.0, MONO);
        let fc = font(500, 11.0, MONO);
        let tw = self.measure(label, &f);
        let cw = count
            .map(|n| self.measure(&n.to_string(), &fc) + 6.0)
            .unwrap_or(0.0);
        let w = tw + cw + 24.0;
        let h = self.control_h.max(36.0);
        self.text(
            x + 12.0,
            y + 11.0,
            label,
            &f,
            &col::css(if selected {
                col::PAPER_100
            } else {
                col::FG_MUTED
            }),
        );
        if let Some(n) = count {
            self.text(
                x + 12.0 + tw + 6.0,
                y + 13.0,
                &n.to_string(),
                &fc,
                &col::css(col::FG_FAINT),
            );
        }
        if selected {
            self.fill_rounded(x, y + h - 2.0, w, 2.0, 1.0, &col::css(col::EMBER_500));
        }
        self.hit_add(x, y, w, h, action);
        w
    }
    pub fn eyebrow(&self, x: f64, y: f64, s: &str) -> f64 {
        self.text(
            x,
            y,
            &s.to_uppercase(),
            &font(400, 12.0, MONO),
            &col::css(col::EMBER_500),
        )
    }
    pub fn ctx(&self) -> &CanvasRenderingContext2d {
        &self.ctx
    }
}

#[derive(Clone, Copy)]
pub struct ChipStyle {
    pub border: u32,
    pub text: u32,
    pub dashed: bool,
    pub glyph: Option<&'static str>,
    pub glyph_color: u32,
    pub disabled: bool,
}
impl ChipStyle {
    pub fn plain() -> ChipStyle {
        ChipStyle {
            border: col::VOID_400,
            text: col::PAPER_100,
            dashed: false,
            glyph: None,
            glyph_color: col::EMBER_500,
            disabled: false,
        }
    }
    pub fn recall() -> ChipStyle {
        ChipStyle {
            dashed: true,
            glyph: Some("#"),
            ..Self::plain()
        }
    }
    pub fn words() -> ChipStyle {
        ChipStyle {
            border: col::QUANTA_500,
            text: col::QUANTA_300,
            glyph_color: col::QUANTA_300,
            ..Self::plain()
        }
    }
    pub fn eyes() -> ChipStyle {
        ChipStyle {
            border: col::SKY_500,
            ..Self::plain()
        }
    }
    pub fn disabled(mut self) -> ChipStyle {
        self.disabled = true;
        self
    }
}
