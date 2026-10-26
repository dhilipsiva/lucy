//! WebXR's stereo frame loop and shared panel geometry. Hardware acceptance is separate.
use glam::{Mat4, Quat, Vec3};
#[derive(Clone, Debug)]
pub struct PanelPose {
    pub index: usize,
    pub model: Mat4,
    pub pixels: (f64, f64),
}
impl PanelPose {
    /// Unit quad centered at the origin, +Y up. Canvas pixels have +Y down.
    pub fn hit(&self, origin: Vec3, direction: Vec3) -> Option<(f32, f64, f64)> {
        let inv = self.model.inverse();
        let o = inv.transform_point3(origin);
        let d = inv.transform_vector3(direction);
        if d.z.abs() < 1e-6 {
            return None;
        }
        let t = -o.z / d.z;
        if t <= 0.0 {
            return None;
        }
        let p = o + d * t;
        if p.x.abs() > 0.5 || p.y.abs() > 0.5 {
            return None;
        }
        Some((
            t,
            (p.x as f64 + 0.5) * self.pixels.0,
            (0.5 - p.y as f64) * self.pixels.1,
        ))
    }
}
pub fn panel(
    index: usize,
    pixels: (f64, f64),
    width: f32,
    position: Vec3,
    rotation: Quat,
) -> PanelPose {
    PanelPose {
        index,
        pixels,
        model: Mat4::from_scale_rotation_translation(
            Vec3::new(width, width * (pixels.1 / pixels.0) as f32, 1.0),
            rotation,
            position,
        ),
    }
}
pub fn bounded(mut pos: Vec3) -> Vec3 {
    let r = pos.x.hypot(pos.z);
    if r > 5.4 {
        pos.x *= 5.4 / r;
        pos.z *= 5.4 / r;
    }
    pos
}
#[derive(Default)]
pub struct Snap {
    last: f64,
    held: bool,
}
impl Snap {
    pub fn turn(&mut self, x: f32, now: f64) -> f32 {
        if x.abs() < 0.35 {
            self.held = false;
        }
        if x.abs() > 0.65 && !self.held && now - self.last >= 350.0 {
            self.last = now;
            self.held = true;
            -x.signum() * std::f32::consts::PI / 6.0
        } else {
            0.0
        }
    }
}
#[cfg(target_arch = "wasm32")]
mod browser {
    use super::*;
    use crate::{app::App, sources::js_error};
    use std::{
        cell::RefCell,
        rc::{Rc, Weak},
    };
    use wasm_bindgen::{prelude::*, JsCast};
    use wasm_bindgen_futures::{spawn_local, JsFuture};
    use web_sys::*;
    type Loop = Rc<RefCell<Option<Closure<dyn FnMut(f64, XrFrame)>>>>;
    pub struct Session {
        pub session: XrSession,
        pub layer: XrWebGlLayer,
        pub space: XrReferenceSpace,
        pub offset: f32,
        pub snap: Snap,
        pub stick_down: bool,
        pub head_yaw: f32,
    }
    pub fn probe(app: Weak<RefCell<App>>) {
        let Some(w) = window() else {
            return;
        };
        let xr = w.navigator().xr();
        if xr.is_undefined() || xr.is_null() {
            return;
        }
        spawn_local(async move {
            let supported = JsFuture::from(xr.is_session_supported(XrSessionMode::ImmersiveVr))
                .await
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if let Some(a) = app.upgrade() {
                a.borrow_mut().xr_support(supported);
            }
        });
    }
    /// requestSession is synchronous here, inside the activating pointer event (WebXR §3.2).
    pub fn start(app: Weak<RefCell<App>>, gl: WebGl2RenderingContext) {
        let Some(w) = window() else {
            return;
        };
        let xr = w.navigator().xr();
        let init = XrSessionInit::new();
        init.set_optional_features(&["local-floor".into(), "hand-tracking".into()]);
        let requested = xr.request_session_with_options(XrSessionMode::ImmersiveVr, &init);
        spawn_local(async move {
            let result = async {
                let session: XrSession = JsFuture::from(requested)
                    .await
                    .map_err(|e| js_error(&e))?
                    .unchecked_into();
                let setup = async {
                    JsFuture::from(gl.make_xr_compatible())
                        .await
                        .map_err(|e| js_error(&e))?;
                    let layer = XrWebGlLayer::new_with_web_gl2_rendering_context(&session, &gl)
                        .map_err(|e| js_error(&e))?;
                    let state = XrRenderStateInit::new();
                    state.set_base_layer(Some(&layer));
                    state.set_depth_near(0.05);
                    state.set_depth_far(30.0);
                    session.update_render_state_with_state(&state);
                    let (space, offset) = match JsFuture::from(
                        session.request_reference_space(XrReferenceSpaceType::LocalFloor),
                    )
                    .await
                    {
                        Ok(s) => (s, 0.0),
                        Err(_) => (
                            JsFuture::from(
                                session.request_reference_space(XrReferenceSpaceType::Local),
                            )
                            .await
                            .map_err(|e| js_error(&e))?,
                            crate::world::EYE,
                        ),
                    };
                    Ok::<_, String>((layer, space.unchecked_into::<XrReferenceSpace>(), offset))
                }
                .await;
                let (layer, space, offset) = match setup {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = session.end();
                        return Err(e);
                    }
                };
                let loop_: Loop = Rc::new(RefCell::new(None));
                let weak = app.clone();
                let frames = loop_.clone();
                let end = Closure::<dyn FnMut(Event)>::new(move |_| {
                    frames.borrow_mut().take();
                    let weak = weak.clone();
                    spawn_local(async move {
                        if let Some(a) = weak.upgrade() {
                            a.borrow_mut().xr_ended();
                        }
                    });
                });
                session.set_onend(Some(end.as_ref().unchecked_ref()));
                end.forget();
                let weak = app.clone();
                let select = Closure::<dyn FnMut(XrInputSourceEvent)>::new(move |e| {
                    if let Some(a) = weak.upgrade() {
                        a.borrow_mut().xr_select(&e);
                    }
                });
                session.set_onselectstart(Some(select.as_ref().unchecked_ref()));
                select.forget();
                let weak = app.clone();
                let frames = loop_.clone();
                let current = session.clone();
                *loop_.borrow_mut() = Some(Closure::new(move |now, frame| {
                    if let Some(a) = weak.upgrade() {
                        a.borrow_mut().xr_frame(now, &frame);
                    }
                    if let Some(cb) = frames.borrow().as_ref() {
                        current.request_animation_frame(cb.as_ref().unchecked_ref());
                    }
                }));
                if let Some(a) = app.upgrade() {
                    a.borrow_mut().xr_started(Session {
                        session: session.clone(),
                        layer,
                        space,
                        offset,
                        snap: Snap::default(),
                        stick_down: false,
                        head_yaw: 0.0,
                    });
                }
                if let Some(cb) = loop_.borrow().as_ref() {
                    session.request_animation_frame(cb.as_ref().unchecked_ref());
                }
                Ok::<_, String>(())
            }
            .await;
            if let Err(e) = result {
                if let Some(a) = app.upgrade() {
                    a.borrow_mut().xr_error(&e);
                }
            }
        });
    }
    pub fn matrix(v: &[f32]) -> Mat4 {
        let a: [f32; 16] = v.try_into().unwrap_or(Mat4::IDENTITY.to_cols_array());
        Mat4::from_cols_array(&a)
    }
    pub fn ray(
        frame: &XrFrame,
        source: &XrInputSource,
        space: &XrReferenceSpace,
        rig: Mat4,
    ) -> Option<(Vec3, Vec3)> {
        let pose = frame.get_pose(&source.target_ray_space(), space)?;
        let m = rig * matrix(&pose.transform().matrix());
        Some((
            m.w_axis.truncate(),
            m.transform_vector3(-Vec3::Z).normalize(),
        ))
    }
}
#[cfg(target_arch = "wasm32")]
pub use browser::*;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tilted_quad_center_and_corners_match_canvas() {
        for pitch in [-28.0f32, -42.0] {
            let q = Quat::from_rotation_x(pitch.to_radians());
            let p = panel(0, (800., 400.), 1.3, Vec3::new(0., 0.88, -0.92), q);
            for (x, y) in [(0., 0.), (-0.499, 0.499), (0.499, -0.499)] {
                let target = p.model.transform_point3(Vec3::new(x, y, 0.));
                let normal = q * Vec3::Z;
                let (_, px, py) = p.hit(target + normal, -normal).unwrap();
                assert!((px - (x as f64 + 0.5) * 800.).abs() < 0.001);
                assert!((py - (0.5 - y as f64) * 400.).abs() < 0.001);
                assert!(p.hit(target + normal, normal).is_none());
            }
        }
    }
    #[test]
    fn comfort_turn_requires_release_and_cooldown() {
        let mut s = Snap::default();
        assert_eq!(s.turn(1., 400.), -std::f32::consts::PI / 6.);
        assert_eq!(s.turn(1., 800.), 0.);
        s.turn(0., 900.);
        assert!(s.turn(-1., 1000.) > 0.);
        assert!(bounded(Vec3::new(20., 0., 20.)).length() < 5.5);
    }
}
