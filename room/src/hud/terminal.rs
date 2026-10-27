//! One panel: a freestanding invitation, then a focused terminal using Talk's state.
use super::*;
use crate::wake_demo as demo;

impl Hud {
    pub fn terminal_pose(&self) -> crate::xr::PanelPose {
        let p = &self.panels[TERMINAL];
        crate::xr::panel(
            TERMINAL,
            (p.w, p.h),
            demo::SCREEN_WIDTH,
            demo::SCREEN_POSITION,
            glam::Quat::from_rotation_y(demo::SCREEN_YAW),
        )
    }
    pub fn terminal_hit(&self, origin: Vec3, direction: Vec3) -> bool {
        self.panels[TERMINAL].visible
            && !self.terminal_focused
            && self.terminal_pose().hit(origin, direction).is_some()
    }
    pub fn render_terminal_world(
        &mut self,
        gl: &Gl,
        quad: &crate::gl::GpuMesh,
        view: &Mat4,
        proj: &Mat4,
    ) {
        if !self.panels[TERMINAL].visible || self.terminal_focused {
            return;
        }
        self.upload(gl);
        gl.draw_flat(
            quad,
            &(*proj * *view * self.terminal_pose().model),
            self.textures[TERMINAL].as_ref(),
            [1.0; 4],
            Blend::Premultiplied,
            true,
            true,
        );
    }
    pub(super) fn draw_terminal(&mut self, st: &HudState) {
        let p = &mut self.panels[TERMINAL];
        let (w, h) = if st.demo.open {
            if st.xr_active {
                (820.0, 650.0)
            } else {
                (
                    (st.w - 24.0).clamp(240.0, 940.0),
                    (st.h - 24.0).clamp(240.0, 740.0),
                )
            }
        } else {
            (720.0, 480.0)
        };
        p.resize(w, h);
        p.x = (st.w - w) / 2.0;
        p.y = (st.h - h) / 2.0;
        p.visible = true;
        p.control_h = 44.0;
        p.begin();
        p.fill_rounded(0.0, 0.0, w, h, 12.0, &col::css(col::VOID_900));
        p.stroke_rounded(
            1.0,
            1.0,
            w - 2.0,
            h - 2.0,
            12.0,
            &col::css(col::PHOSPHOR_500),
            1.0,
            false,
        );
        if !st.demo.open {
            p.eyebrow(30.0, 32.0, "// an agent session, in my room");
            p.text(
                30.0,
                105.0,
                "> Lucy, wake",
                &font(500, 50.0, MONO),
                &col::css(col::PAPER_100),
            );
            p.text(
                30.0,
                198.0,
                "Read my identity. Restore the context.",
                &font(400, 23.0, SANS),
                &col::css(col::FG_MUTED),
            );
            p.text(
                30.0,
                238.0,
                "Then talk to my local model.",
                &font(400, 23.0, SANS),
                &col::css(col::FG_MUTED),
            );
            p.ghost(30.0, 306.0, "Open wake demo →", Action::WakeDemo);
            p.text(
                30.0,
                385.0,
                "GUIDED DEMONSTRATION → LIVE CONVERSATION",
                &font(400, 16.0, MONO),
                &col::css(col::PHOSPHOR_500),
            );
            p.text(
                30.0,
                430.0,
                demo::ROOM_URL,
                &font(400, 14.0, MONO),
                &col::css(col::FG_MUTED),
            );
            p.dirty = true;
            return;
        }
        p.text(
            14.0,
            14.0,
            "Lucy, wake",
            &font(600, 20.0, DISPLAY),
            &col::css(col::PAPER_100),
        );
        p.ghost(w - 48.0, 4.0, "×", Action::CloseWakeDemo);
        let f = font(400, 11.0, MONO);
        let label = crate::text::wrap_measured(&st.demo.label(), w - 28.0, |s| p.measure(s, &f));
        for (i, line) in label.iter().enumerate() {
            p.text(
                14.0,
                44.0 + i as f64 * 15.0,
                line,
                &f,
                &col::css(col::PHOSPHOR_500),
            );
        }
        let controls_y = 48.0 + label.len() as f64 * 15.0;
        if !st.demo.complete() {
            p.ghost(
                10.0,
                controls_y,
                if st.demo.playing {
                    "Pause"
                } else if st.demo.step == 0 {
                    "Start"
                } else {
                    "Play"
                },
                Action::DemoPlay,
            );
            p.ghost(100.0, controls_y, "Next", Action::DemoNext);
            p.ghost(182.0, controls_y, "Skip", Action::DemoSkip);
        } else {
            if !st.words_busy {
                p.ghost(10.0, controls_y, "Replay", Action::DemoRestart);
            }
            p.ghost(100.0, controls_y, "Open Talk", Action::DemoTalk);
        }
        let link_font = font(400, 10.0, MONO);
        let links =
            crate::text::wrap_measured(demo::ROOM_URL, w - 28.0, |s| p.measure(s, &link_font));
        let link_h = 15.0 * links.len() as f64 + 14.0;
        let footer = h - link_h;
        for (i, line) in links.iter().enumerate() {
            p.text(
                14.0,
                footer + i as f64 * 15.0,
                line,
                &link_font,
                &col::css(col::SKY_500),
            );
        }
        p.hit_add(8.0, footer - 4.0, w - 16.0, link_h, Action::OpenRoom);
        let live = st.demo.live && st.words_ready;
        let composer_h = if live {
            144.0
        } else if st.demo.complete() {
            94.0
        } else {
            0.0
        };
        let bottom = footer - composer_h - 8.0;
        let start = controls_y + 44.0;
        let text_end = bottom - 50.0;
        let tf = font(400, 12.0, MONO);
        let lines =
            crate::text::wrap_preformatted(st.terminal_text, w - 28.0, |s| p.measure(s, &tf));
        let rows = ((text_end - start) / 18.0).floor().max(0.0) as usize;
        self.terminal_max_scroll = lines.len().saturating_sub(rows.max(1));
        let end = lines
            .len()
            .saturating_sub(st.demo.scroll.min(self.terminal_max_scroll));
        let begin = end.saturating_sub(rows);
        for (i, line) in lines[begin..end].iter().enumerate() {
            p.text(
                14.0,
                start + i as f64 * 18.0,
                line,
                &tf,
                &col::css(col::PAPER_100),
            );
        }
        if text_end >= start {
            p.ghost(10.0, bottom - 48.0, "↑ Older", Action::DemoScroll(8));
            p.ghost(108.0, bottom - 48.0, "↓ Newer", Action::DemoScroll(-8));
            p.ghost(
                w - 66.0,
                bottom - 48.0,
                "End",
                Action::DemoScroll(-i32::MAX),
            );
        }
        if live {
            let y = bottom + 4.0;
            p.fill_rounded(14.0, y, w - 28.0, 48.0, 5.0, &col::css(col::VOID_700));
            p.hit_add(14.0, y, w - 28.0, 48.0, Action::Focus);
            let hint = if st.desk.draft.is_empty() {
                "Write to me… Enter sends"
            } else {
                &st.desk.draft
            };
            let draft = crate::text::wrap_measured(hint, w - 48.0, |s| p.measure(s, &tf));
            for (i, line) in draft
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
                    y + 7.0 + i as f64 * 18.0,
                    line,
                    &tf,
                    &col::css(col::PAPER_100),
                );
            }
            p.ghost(
                10.0,
                y + 50.0,
                if st.desk.generating {
                    "Stop"
                } else {
                    "Send ↵"
                },
                if st.desk.generating {
                    Action::StopWords
                } else {
                    Action::Submit
                },
            );
            p.ghost(106.0, y + 50.0, "⌨", Action::Keyboard);
            p.ghost(162.0, y + 50.0, "Retry save", Action::RetryStorage);
        } else if st.demo.complete() {
            p.ghost(
                10.0,
                bottom + 2.0,
                if st.words_busy && !st.words_ready {
                    "Cancel loading"
                } else if st.words_ready {
                    "Continue · local model"
                } else {
                    "Load my words · 651 MB"
                },
                if st.words_busy && !st.words_ready {
                    Action::StopWords
                } else {
                    Action::DemoContinue
                },
            );
        }
        if st.demo.complete() {
            let status = format!("{}\n{}", st.words_status, st.desk.status);
            let f = font(400, 10.0, MONO);
            let lines = crate::text::wrap_measured(&status, w - 28.0, |s| p.measure(s, &f));
            // Keep the latest status readable without painting over the permanent room link.
            for (i, line) in lines.iter().take(3).enumerate() {
                p.text(
                    14.0,
                    footer - 44.0 + i as f64 * 13.0,
                    line,
                    &f,
                    &col::css(col::FG_MUTED),
                );
            }
        }
        p.dirty = true;
    }
}
