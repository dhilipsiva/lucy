//! A small WebGL2 renderer through web-sys: four programs (lit meshes, flat textured quads,
//! lines, points), meshes uploaded once, textures from 2D canvases. Linear light inside, ACES
//! tone mapping and sRGB out, a linear fog, as the hand-off's three.js scene was set up.

use glam::{Mat3, Mat4, Vec3};
use wasm_bindgen::JsCast;
use web_sys::{
    HtmlCanvasElement, WebGl2RenderingContext as G, WebGlBuffer, WebGlProgram, WebGlShader,
    WebGlTexture, WebGlUniformLocation, WebGlVertexArrayObject,
};

use crate::geom::Mesh;

pub struct Program {
    prog: WebGlProgram,
    uniforms: std::collections::HashMap<String, WebGlUniformLocation>,
}
impl Program {
    fn u(&self, name: &str) -> Option<&WebGlUniformLocation> {
        self.uniforms.get(name)
    }
}

/// A mesh on the GPU: one vertex array, drawn with `mode`.
pub struct GpuMesh {
    vao: WebGlVertexArrayObject,
    count: i32,
    mode: u32,
    indexed: bool,
    _buffers: Vec<WebGlBuffer>,
}

pub struct Texture {
    pub tex: WebGlTexture,
    pub width: u32,
    pub height: u32,
}

/// The lights of one frame, in world space and linear light.
#[derive(Clone, Debug)]
pub struct Lights {
    pub hemi_sky: [f32; 3],
    pub hemi_ground: [f32; 3],
    pub hemi_intensity: f32,
    pub dir_color: [f32; 3],
    pub dir_dir: Vec3,
    pub dir_intensity: f32,
    pub points: Vec<PointLight>,
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
}
#[derive(Clone, Copy, Debug)]
pub struct PointLight {
    pub pos: Vec3,
    pub color: [f32; 3],
    pub intensity: f32,
    pub distance: f32,
    pub decay: f32,
}

/// What one lit draw needs besides the mesh.
pub struct LitParams<'a> {
    pub model: Mat4,
    pub color: [f32; 3],
    pub emissive: [f32; 3],
    pub emissive_intensity: f32,
    pub roughness: f32,
    pub opacity: f32,
    pub map: Option<&'a Texture>,
    pub double_sided: bool,
    pub depth_write: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Blend {
    None,
    Alpha,
    Premultiplied,
    Additive,
}

pub struct Gl {
    pub ctx: G,
    lit: Program,
    flat: Program,
    lines: Program,
    points: Program,
    pub width: u32,
    pub height: u32,
    pub exposure: f32,
}

const LIT_VS: &str = r#"#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos; layout(location=1) in vec3 a_nrm; layout(location=2) in vec2 a_uv;
uniform mat4 u_model, u_view, u_proj; uniform mat3 u_nrm;
out vec3 v_pos; out vec3 v_nrm; out vec2 v_uv; out float v_depth;
void main(){ vec4 w = u_model * vec4(a_pos,1.0); v_pos = w.xyz; v_nrm = normalize(u_nrm * a_nrm); v_uv = a_uv;
  vec4 vp = u_view * w; v_depth = -vp.z; gl_Position = u_proj * vp; }"#;
const LIT_FS: &str = r#"#version 300 es
precision highp float;
in vec3 v_pos; in vec3 v_nrm; in vec2 v_uv; in float v_depth; out vec4 o;
uniform vec3 u_color, u_emissive; uniform float u_emissiveI, u_rough, u_opacity; uniform int u_useMap; uniform sampler2D u_map;
uniform vec3 u_hemiSky, u_hemiGround; uniform float u_hemiI; uniform vec3 u_dirColor, u_dirDir; uniform float u_dirI;
uniform int u_nPoints; uniform vec3 u_pPos[4]; uniform vec3 u_pColor[4]; uniform float u_pI[4]; uniform float u_pDist[4]; uniform float u_pDecay[4];
uniform vec3 u_fog; uniform float u_fogNear, u_fogFar, u_exposure; uniform vec3 u_cam; uniform int u_doubleSided;
vec3 aces(vec3 x){ x *= 0.6; const float a=2.51,b=0.03,c=2.43,d=0.59,e=0.14; return clamp((x*(a*x+b))/(x*(c*x+d)+e),0.0,1.0); }
vec3 toSrgb(vec3 c){ return mix(12.92*c, 1.055*pow(c, vec3(1.0/2.4))-0.055, step(0.0031308, c)); }
void main(){
  vec3 n = normalize(v_nrm); if (u_doubleSided==1 && !gl_FrontFacing) n = -n;
  vec3 albedo = u_color; if (u_useMap==1) albedo *= texture(u_map, v_uv).rgb;
  vec3 light = mix(u_hemiGround, u_hemiSky, 0.5 + 0.5*n.y) * u_hemiI;
  vec3 v = normalize(u_cam - v_pos);
  float ndl = max(dot(n, normalize(u_dirDir)), 0.0); light += u_dirColor * u_dirI * ndl;
  vec3 h = normalize(normalize(u_dirDir) + v); float spec = pow(max(dot(n,h),0.0), mix(64.0, 4.0, u_rough)) * (1.0-u_rough) * 0.25 * u_dirI;
  for (int i=0;i<4;i++){ if (i>=u_nPoints) break; vec3 L = u_pPos[i]-v_pos; float d = length(L); L/=d;
    float att = u_pDist[i] > 0.0 ? pow(clamp(1.0 - pow(d/u_pDist[i], 4.0), 0.0, 1.0), 2.0) / pow(max(d,0.05), u_pDecay[i]) : 1.0/pow(max(d,0.05), u_pDecay[i]);
    light += u_pColor[i] * u_pI[i] * att * max(dot(n,L),0.0); }
  vec3 c = albedo * light + spec * u_dirColor + u_emissive * u_emissiveI;
  c = aces(c * u_exposure);
  float f = clamp((u_fogFar - v_depth) / (u_fogFar - u_fogNear), 0.0, 1.0);
  c = mix(u_fog, c, f);
  o = vec4(toSrgb(c), u_opacity);
}"#;
const FLAT_VS: &str = r#"#version 300 es
precision highp float;
layout(location=0) in vec3 a_pos; layout(location=1) in vec3 a_nrm; layout(location=2) in vec2 a_uv;
uniform mat4 u_mvp; out vec2 v_uv; void main(){ v_uv = a_uv; gl_Position = u_mvp * vec4(a_pos,1.0); }"#;
const FLAT_FS: &str = r#"#version 300 es
precision highp float; in vec2 v_uv; out vec4 o;
uniform sampler2D u_map; uniform int u_useMap; uniform vec4 u_tint; uniform int u_flipY; uniform int u_premul;
void main(){ vec2 uv = v_uv; if (u_flipY==1) uv.y = 1.0-uv.y; vec4 t = u_useMap==1 ? texture(u_map, uv) : vec4(1.0);
  if (u_premul==1) { o = vec4(t.rgb * u_tint.rgb, t.a) * u_tint.a; } else { o = t * u_tint; } }"#;
const LINE_VS: &str = r#"#version 300 es
precision highp float; layout(location=0) in vec3 a_pos; uniform mat4 u_model, u_view, u_proj; out float v_depth;
void main(){ vec4 vp = u_view * u_model * vec4(a_pos,1.0); v_depth = -vp.z; gl_Position = u_proj * vp; }"#;
const LINE_FS: &str = r#"#version 300 es
precision highp float; in float v_depth; out vec4 o; uniform vec3 u_color; uniform float u_opacity; uniform vec3 u_fog; uniform float u_fogNear, u_fogFar;
vec3 toSrgb(vec3 c){ return mix(12.92*c, 1.055*pow(c, vec3(1.0/2.4))-0.055, step(0.0031308, c)); }
void main(){ float f = clamp((u_fogFar - v_depth) / (u_fogFar - u_fogNear), 0.0, 1.0); o = vec4(toSrgb(mix(u_fog, u_color, f)), u_opacity); }"#;
const POINT_VS: &str = r#"#version 300 es
precision highp float; layout(location=0) in vec3 a_pos; uniform mat4 u_model, u_view, u_proj; uniform float u_size, u_scale; out float v_depth;
void main(){ vec4 vp = u_view * u_model * vec4(a_pos,1.0); v_depth = -vp.z; gl_Position = u_proj * vp; gl_PointSize = max(1.0, u_size * u_scale / max(-vp.z, 0.1)); }"#;
const POINT_FS: &str = r#"#version 300 es
precision highp float; in float v_depth; out vec4 o; uniform vec3 u_color; uniform float u_opacity; uniform vec3 u_fog; uniform float u_fogNear, u_fogFar;
vec3 toSrgb(vec3 c){ return mix(12.92*c, 1.055*pow(c, vec3(1.0/2.4))-0.055, step(0.0031308, c)); }
void main(){ vec2 d = gl_PointCoord - 0.5; if (dot(d,d) > 0.25) discard; float f = clamp((u_fogFar - v_depth) / (u_fogFar - u_fogNear), 0.0, 1.0); o = vec4(toSrgb(mix(u_fog, u_color, f)), u_opacity); }"#;

impl Gl {
    pub fn new(canvas: &HtmlCanvasElement) -> Result<Gl, String> {
        let opts = js_sys::Object::new();
        for (k, v) in [
            ("antialias", true),
            ("alpha", false),
            ("premultipliedAlpha", true),
        ] {
            js_sys::Reflect::set(&opts, &k.into(), &v.into()).map_err(|_| "context options")?;
        }
        js_sys::Reflect::set(&opts, &"powerPreference".into(), &"high-performance".into())
            .map_err(|_| "context options")?;
        let ctx = canvas
            .get_context_with_context_options("webgl2", &opts)
            .map_err(|e| format!("getContext: {e:?}"))?
            .ok_or("this browser has no WebGL2")?
            .dyn_into::<G>()
            .map_err(|_| "not a WebGL2 context")?;
        let lit = compile(
            &ctx,
            LIT_VS,
            LIT_FS,
            &[
                "u_model",
                "u_view",
                "u_proj",
                "u_nrm",
                "u_color",
                "u_emissive",
                "u_emissiveI",
                "u_rough",
                "u_opacity",
                "u_useMap",
                "u_map",
                "u_hemiSky",
                "u_hemiGround",
                "u_hemiI",
                "u_dirColor",
                "u_dirDir",
                "u_dirI",
                "u_nPoints",
                "u_pPos",
                "u_pColor",
                "u_pI",
                "u_pDist",
                "u_pDecay",
                "u_fog",
                "u_fogNear",
                "u_fogFar",
                "u_exposure",
                "u_cam",
                "u_doubleSided",
            ],
        )?;
        let flat = compile(
            &ctx,
            FLAT_VS,
            FLAT_FS,
            &[
                "u_mvp", "u_map", "u_useMap", "u_tint", "u_flipY", "u_premul",
            ],
        )?;
        let lines = compile(
            &ctx,
            LINE_VS,
            LINE_FS,
            &[
                "u_model",
                "u_view",
                "u_proj",
                "u_color",
                "u_opacity",
                "u_fog",
                "u_fogNear",
                "u_fogFar",
            ],
        )?;
        let points = compile(
            &ctx,
            POINT_VS,
            POINT_FS,
            &[
                "u_model",
                "u_view",
                "u_proj",
                "u_size",
                "u_scale",
                "u_color",
                "u_opacity",
                "u_fog",
                "u_fogNear",
                "u_fogFar",
            ],
        )?;
        crate::panel::texture_limit(
            ctx.get_parameter(G::MAX_TEXTURE_SIZE)
                .ok()
                .and_then(|v| v.as_f64())
                .unwrap_or(2048.0)
                .min(4096.0),
        );
        ctx.enable(G::DEPTH_TEST);
        ctx.enable(G::BLEND);
        ctx.enable(G::CULL_FACE);
        ctx.cull_face(G::BACK);
        ctx.depth_func(G::LEQUAL);
        Ok(Gl {
            ctx,
            lit,
            flat,
            lines,
            points,
            width: 1,
            height: 1,
            exposure: 1.05,
        })
    }
    pub fn resize(&mut self, width: u32, height: u32) {
        self.width = width.max(1);
        self.height = height.max(1);
        self.ctx
            .viewport(0, 0, self.width as i32, self.height as i32);
    }
    pub fn viewport(&self, x: i32, y: i32, w: i32, h: i32) {
        self.ctx.viewport(x, y, w, h);
    }
    pub fn clear(&self, color: [f32; 3]) {
        // the clear colour is sRGB already (the void), no tone mapping
        self.ctx.clear_color(color[0], color[1], color[2], 1.0);
        self.ctx.clear(G::COLOR_BUFFER_BIT | G::DEPTH_BUFFER_BIT);
    }
    pub fn upload(&self, mesh: &Mesh) -> GpuMesh {
        let g = &self.ctx;
        let vao = g.create_vertex_array().expect("vao");
        g.bind_vertex_array(Some(&vao));
        let mut bufs = Vec::new();
        for (loc, data, size) in [
            (0u32, &mesh.positions, 3),
            (1, &mesh.normals, 3),
            (2, &mesh.uvs, 2),
        ] {
            let b = g.create_buffer().expect("buffer");
            g.bind_buffer(G::ARRAY_BUFFER, Some(&b));
            unsafe {
                let view = js_sys::Float32Array::view(data);
                g.buffer_data_with_array_buffer_view(G::ARRAY_BUFFER, &view, G::STATIC_DRAW);
            }
            g.enable_vertex_attrib_array(loc);
            g.vertex_attrib_pointer_with_i32(loc, size, G::FLOAT, false, 0, 0);
            bufs.push(b);
        }
        let ib = g.create_buffer().expect("index buffer");
        g.bind_buffer(G::ELEMENT_ARRAY_BUFFER, Some(&ib));
        unsafe {
            let view = js_sys::Uint32Array::view(&mesh.indices);
            g.buffer_data_with_array_buffer_view(G::ELEMENT_ARRAY_BUFFER, &view, G::STATIC_DRAW);
        }
        bufs.push(ib);
        g.bind_vertex_array(None);
        GpuMesh {
            vao,
            count: mesh.indices.len() as i32,
            mode: G::TRIANGLES,
            indexed: true,
            _buffers: bufs,
        }
    }
    /// Positions only, drawn as LINES or POINTS.
    pub fn upload_positions(&self, positions: &[f32], mode: u32) -> GpuMesh {
        let g = &self.ctx;
        let vao = g.create_vertex_array().expect("vao");
        g.bind_vertex_array(Some(&vao));
        let b = g.create_buffer().expect("buffer");
        g.bind_buffer(G::ARRAY_BUFFER, Some(&b));
        unsafe {
            let view = js_sys::Float32Array::view(positions);
            g.buffer_data_with_array_buffer_view(G::ARRAY_BUFFER, &view, G::STATIC_DRAW);
        }
        g.enable_vertex_attrib_array(0);
        g.vertex_attrib_pointer_with_i32(0, 3, G::FLOAT, false, 0, 0);
        g.bind_vertex_array(None);
        GpuMesh {
            vao,
            count: (positions.len() / 3) as i32,
            mode,
            indexed: false,
            _buffers: vec![b],
        }
    }
    pub fn texture_from_canvas(&self, canvas: &HtmlCanvasElement, premultiplied: bool) -> Texture {
        let g = &self.ctx;
        let tex = g.create_texture().expect("texture");
        let t = Texture {
            tex,
            width: canvas.width(),
            height: canvas.height(),
        };
        self.update_texture(&t, canvas, premultiplied);
        t
    }
    pub fn update_texture(&self, t: &Texture, canvas: &HtmlCanvasElement, premultiplied: bool) {
        let g = &self.ctx;
        g.bind_texture(G::TEXTURE_2D, Some(&t.tex));
        g.pixel_storei(
            G::UNPACK_PREMULTIPLY_ALPHA_WEBGL,
            if premultiplied { 1 } else { 0 },
        );
        g.pixel_storei(G::UNPACK_FLIP_Y_WEBGL, 0);
        let _ = g.tex_image_2d_with_u32_and_u32_and_html_canvas_element(
            G::TEXTURE_2D,
            0,
            G::RGBA as i32,
            G::RGBA,
            G::UNSIGNED_BYTE,
            canvas,
        );
        g.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_MIN_FILTER, G::LINEAR as i32);
        g.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_MAG_FILTER, G::LINEAR as i32);
        g.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_WRAP_S, G::CLAMP_TO_EDGE as i32);
        g.tex_parameteri(G::TEXTURE_2D, G::TEXTURE_WRAP_T, G::CLAMP_TO_EDGE as i32);
        g.bind_texture(G::TEXTURE_2D, None);
    }
    fn blend(&self, b: Blend) {
        let g = &self.ctx;
        match b {
            Blend::None => g.disable(G::BLEND),
            Blend::Alpha => {
                g.enable(G::BLEND);
                g.blend_func(G::SRC_ALPHA, G::ONE_MINUS_SRC_ALPHA);
            }
            Blend::Premultiplied => {
                g.enable(G::BLEND);
                g.blend_func(G::ONE, G::ONE_MINUS_SRC_ALPHA);
            }
            Blend::Additive => {
                g.enable(G::BLEND);
                g.blend_func(G::ONE, G::ONE);
            }
        }
    }
    fn set_lights(&self, p: &Program, lights: &Lights, cam: Vec3) {
        let g = &self.ctx;
        g.uniform3fv_with_f32_array(p.u("u_hemiSky"), &lights.hemi_sky);
        g.uniform3fv_with_f32_array(p.u("u_hemiGround"), &lights.hemi_ground);
        g.uniform1f(p.u("u_hemiI"), lights.hemi_intensity);
        g.uniform3fv_with_f32_array(p.u("u_dirColor"), &lights.dir_color);
        g.uniform3f(
            p.u("u_dirDir"),
            lights.dir_dir.x,
            lights.dir_dir.y,
            lights.dir_dir.z,
        );
        g.uniform1f(p.u("u_dirI"), lights.dir_intensity);
        let n = lights.points.len().min(4);
        g.uniform1i(p.u("u_nPoints"), n as i32);
        let mut pos = [0f32; 12];
        let mut col = [0f32; 12];
        let mut inten = [0f32; 4];
        let mut dist = [0f32; 4];
        let mut dec = [1f32; 4];
        for (i, l) in lights.points.iter().take(4).enumerate() {
            pos[i * 3..i * 3 + 3].copy_from_slice(&[l.pos.x, l.pos.y, l.pos.z]);
            col[i * 3..i * 3 + 3].copy_from_slice(&l.color);
            inten[i] = l.intensity;
            dist[i] = l.distance;
            dec[i] = l.decay;
        }
        g.uniform3fv_with_f32_array(p.u("u_pPos"), &pos);
        g.uniform3fv_with_f32_array(p.u("u_pColor"), &col);
        g.uniform1fv_with_f32_array(p.u("u_pI"), &inten);
        g.uniform1fv_with_f32_array(p.u("u_pDist"), &dist);
        g.uniform1fv_with_f32_array(p.u("u_pDecay"), &dec);
        g.uniform3fv_with_f32_array(p.u("u_fog"), &lights.fog_color);
        g.uniform1f(p.u("u_fogNear"), lights.fog_near);
        g.uniform1f(p.u("u_fogFar"), lights.fog_far);
        g.uniform1f(p.u("u_exposure"), self.exposure);
        g.uniform3f(p.u("u_cam"), cam.x, cam.y, cam.z);
    }
    pub fn draw_lit(
        &self,
        mesh: &GpuMesh,
        prm: &LitParams,
        view: &Mat4,
        proj: &Mat4,
        lights: &Lights,
        cam: Vec3,
    ) {
        let g = &self.ctx;
        let p = &self.lit;
        g.use_program(Some(&p.prog));
        g.uniform_matrix4fv_with_f32_array(p.u("u_model"), false, &prm.model.to_cols_array());
        g.uniform_matrix4fv_with_f32_array(p.u("u_view"), false, &view.to_cols_array());
        g.uniform_matrix4fv_with_f32_array(p.u("u_proj"), false, &proj.to_cols_array());
        let nm = Mat3::from_mat4(prm.model).inverse().transpose();
        g.uniform_matrix3fv_with_f32_array(p.u("u_nrm"), false, &nm.to_cols_array());
        g.uniform3fv_with_f32_array(p.u("u_color"), &prm.color);
        g.uniform3fv_with_f32_array(p.u("u_emissive"), &prm.emissive);
        g.uniform1f(p.u("u_emissiveI"), prm.emissive_intensity);
        g.uniform1f(p.u("u_rough"), prm.roughness);
        g.uniform1f(p.u("u_opacity"), prm.opacity);
        g.uniform1i(p.u("u_doubleSided"), if prm.double_sided { 1 } else { 0 });
        if let Some(t) = prm.map {
            g.active_texture(G::TEXTURE0);
            g.bind_texture(G::TEXTURE_2D, Some(&t.tex));
            g.uniform1i(p.u("u_map"), 0);
            g.uniform1i(p.u("u_useMap"), 1);
        } else {
            g.uniform1i(p.u("u_useMap"), 0);
        }
        self.set_lights(p, lights, cam);
        if prm.double_sided {
            g.disable(G::CULL_FACE);
        } else {
            g.enable(G::CULL_FACE);
        }
        g.depth_mask(prm.depth_write);
        self.blend(if prm.opacity < 1.0 {
            Blend::Alpha
        } else {
            Blend::None
        });
        self.draw(mesh);
        g.depth_mask(true);
        g.enable(G::CULL_FACE);
    }
    /// An unlit quad or billboard with a texture (panels, boards, halos).
    pub fn draw_flat(
        &self,
        mesh: &GpuMesh,
        mvp: &Mat4,
        tex: Option<&Texture>,
        tint: [f32; 4],
        blend: Blend,
        depth_test: bool,
        flip_y: bool,
    ) {
        let g = &self.ctx;
        let p = &self.flat;
        g.use_program(Some(&p.prog));
        g.uniform_matrix4fv_with_f32_array(p.u("u_mvp"), false, &mvp.to_cols_array());
        if let Some(t) = tex {
            g.active_texture(G::TEXTURE0);
            g.bind_texture(G::TEXTURE_2D, Some(&t.tex));
            g.uniform1i(p.u("u_map"), 0);
            g.uniform1i(p.u("u_useMap"), 1);
        } else {
            g.uniform1i(p.u("u_useMap"), 0);
        }
        g.uniform4fv_with_f32_array(p.u("u_tint"), &tint);
        g.uniform1i(p.u("u_flipY"), if flip_y { 1 } else { 0 });
        g.uniform1i(
            p.u("u_premul"),
            if blend == Blend::Premultiplied { 1 } else { 0 },
        );
        if depth_test {
            g.enable(G::DEPTH_TEST);
        } else {
            g.disable(G::DEPTH_TEST);
        }
        g.depth_mask(false);
        g.disable(G::CULL_FACE);
        self.blend(blend);
        self.draw(mesh);
        g.depth_mask(true);
        g.enable(G::DEPTH_TEST);
        g.enable(G::CULL_FACE);
    }
    pub fn draw_lines(
        &self,
        mesh: &GpuMesh,
        model: &Mat4,
        view: &Mat4,
        proj: &Mat4,
        color: [f32; 3],
        opacity: f32,
        lights: &Lights,
    ) {
        let g = &self.ctx;
        let p = &self.lines;
        g.use_program(Some(&p.prog));
        g.uniform_matrix4fv_with_f32_array(p.u("u_model"), false, &model.to_cols_array());
        g.uniform_matrix4fv_with_f32_array(p.u("u_view"), false, &view.to_cols_array());
        g.uniform_matrix4fv_with_f32_array(p.u("u_proj"), false, &proj.to_cols_array());
        g.uniform3fv_with_f32_array(p.u("u_color"), &color);
        g.uniform1f(p.u("u_opacity"), opacity);
        g.uniform3fv_with_f32_array(p.u("u_fog"), &lights.fog_color);
        g.uniform1f(p.u("u_fogNear"), lights.fog_near);
        g.uniform1f(p.u("u_fogFar"), lights.fog_far);
        g.depth_mask(false);
        self.blend(Blend::Alpha);
        self.draw(mesh);
        g.depth_mask(true);
    }
    pub fn draw_points(
        &self,
        mesh: &GpuMesh,
        model: &Mat4,
        view: &Mat4,
        proj: &Mat4,
        color: [f32; 3],
        size: f32,
        opacity: f32,
        lights: &Lights,
    ) {
        let g = &self.ctx;
        let p = &self.points;
        g.use_program(Some(&p.prog));
        g.uniform_matrix4fv_with_f32_array(p.u("u_model"), false, &model.to_cols_array());
        g.uniform_matrix4fv_with_f32_array(p.u("u_view"), false, &view.to_cols_array());
        g.uniform_matrix4fv_with_f32_array(p.u("u_proj"), false, &proj.to_cols_array());
        g.uniform1f(p.u("u_size"), size);
        g.uniform1f(p.u("u_scale"), self.height as f32 / 2.0);
        g.uniform3fv_with_f32_array(p.u("u_color"), &color);
        g.uniform1f(p.u("u_opacity"), opacity);
        g.uniform3fv_with_f32_array(p.u("u_fog"), &lights.fog_color);
        g.uniform1f(p.u("u_fogNear"), lights.fog_near);
        g.uniform1f(p.u("u_fogFar"), lights.fog_far);
        g.depth_mask(false);
        self.blend(Blend::Alpha);
        self.draw(mesh);
        g.depth_mask(true);
    }
    fn draw(&self, mesh: &GpuMesh) {
        let g = &self.ctx;
        g.bind_vertex_array(Some(&mesh.vao));
        if mesh.indexed {
            g.draw_elements_with_i32(mesh.mode, mesh.count, G::UNSIGNED_INT, 0);
        } else {
            g.draw_arrays(mesh.mode, 0, mesh.count);
        }
        g.bind_vertex_array(None);
    }
}

fn compile(g: &G, vs: &str, fs: &str, uniforms: &[&str]) -> Result<Program, String> {
    let v = shader(g, G::VERTEX_SHADER, vs)?;
    let f = shader(g, G::FRAGMENT_SHADER, fs)?;
    let prog = g.create_program().ok_or("createProgram")?;
    g.attach_shader(&prog, &v);
    g.attach_shader(&prog, &f);
    g.link_program(&prog);
    if !g
        .get_program_parameter(&prog, G::LINK_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        return Err(format!(
            "link: {}",
            g.get_program_info_log(&prog).unwrap_or_default()
        ));
    }
    let mut map = std::collections::HashMap::new();
    for u in uniforms {
        if let Some(loc) = g.get_uniform_location(&prog, u) {
            map.insert((*u).to_string(), loc);
        }
    }
    Ok(Program {
        prog,
        uniforms: map,
    })
}
fn shader(g: &G, kind: u32, src: &str) -> Result<WebGlShader, String> {
    let s = g.create_shader(kind).ok_or("createShader")?;
    g.shader_source(&s, src);
    g.compile_shader(&s);
    if !g
        .get_shader_parameter(&s, G::COMPILE_STATUS)
        .as_bool()
        .unwrap_or(false)
    {
        return Err(format!(
            "shader: {}",
            g.get_shader_info_log(&s).unwrap_or_default()
        ));
    }
    Ok(s)
}
