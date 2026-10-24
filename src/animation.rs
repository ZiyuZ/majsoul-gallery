mod gpu;
mod mesh;
// Spine preview rendered in a dedicated process. Only one bounded frame is queued.
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use rusty_spine::{controller::SkeletonController, *};
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Once, mpsc},
    time::{Duration, Instant},
};

pub enum Event {
    Frame(RgbaImage),
    Actions {
        names: Vec<String>,
        selected: String,
    },
}
pub struct Player {
    pub frames: mpsc::Receiver<Result<Event, String>>,
    input: Option<std::process::ChildStdin>,
    child: Option<std::process::Child>,
}
impl Player {
    pub fn start(root: PathBuf, scene: PathBuf, resolution: (u32, u32)) -> Self {
        use std::process::{Command, Stdio};
        let (tx, frames) = mpsc::sync_channel(1);
        crate::diagnostics::record(format!("animation start: {}", scene.display()));
        let result = (|| -> Result<_> {
            let mut command = Command::new(std::env::current_exe()?);
            command
                .arg("--animation-worker")
                .arg(root)
                .arg(scene)
                .arg("0")
                .arg(resolution.0.to_string())
                .arg(resolution.1.to_string())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::piped());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }
            Ok(command.spawn()?)
        })();
        match result {
            Ok(mut child) => {
                let input = child.stdin.take();
                let mut stdout = child.stdout.take().unwrap();
                let stderr = child.stderr.take().unwrap();
                std::thread::spawn(move || {
                    use std::io::BufRead;
                    for line in std::io::BufReader::new(stderr)
                        .lines()
                        .map_while(Result::ok)
                    {
                        crate::diagnostics::record(format!("animation worker: {line}"));
                    }
                });
                std::thread::spawn(move || {
                    use std::io::Read;
                    loop {
                        let read = (|| -> Result<Event> {
                            let mut header = [0u8; 8];
                            stdout.read_exact(&mut header)?;
                            let w = u32::from_le_bytes(header[..4].try_into().unwrap());
                            let h = u32::from_le_bytes(header[4..].try_into().unwrap());
                            if w == 0 {
                                ensure!(h <= 1_000_000, "动画动作列表过大");
                                let mut bytes = vec![0; h as usize];
                                stdout.read_exact(&mut bytes)?;
                                let info: Value = serde_json::from_slice(&bytes)?;
                                return Ok(Event::Actions {
                                    names: info["names"]
                                        .as_array()
                                        .context("缺少动作列表")?
                                        .iter()
                                        .filter_map(|s| s.as_str().map(str::to_owned))
                                        .collect(),
                                    selected: info["selected"].as_str().unwrap_or_default().into(),
                                });
                            }
                            ensure!(w > 0 && h > 0 && w <= 2048 && h <= 2048, "动画帧尺寸无效");
                            let mut bytes = vec![0; (w * h * 4) as usize];
                            stdout.read_exact(&mut bytes)?;
                            Ok(Event::Frame(
                                RgbaImage::from_raw(w, h, bytes).context("动画帧无效")?,
                            ))
                        })();
                        match read {
                            Ok(Event::Actions { names, selected }) => {
                                if tx.send(Ok(Event::Actions { names, selected })).is_err() {
                                    break;
                                }
                            }
                            Ok(frame) => {
                                if matches!(
                                    tx.try_send(Ok(frame)),
                                    Err(mpsc::TrySendError::Disconnected(_))
                                ) {
                                    break;
                                }
                            }
                            Err(error) => {
                                crate::diagnostics::record(format!(
                                    "animation stream ended: {error:#}"
                                ));
                                let _=tx.send(Err("动画播放器已退出，详情见 majsoul-gallery-data/gallery.log。可关闭动画后重试。".into()));
                                break;
                            }
                        }
                    }
                });
                Self {
                    frames,
                    child: Some(child),
                    input,
                }
            }
            Err(error) => {
                let _ = tx.send(Err(format!("无法启动动画播放器：{error:#}")));
                Self {
                    frames,
                    child: None,
                    input: None,
                }
            }
        }
    }
    fn send(&mut self, command: Value) -> Result<()> {
        use std::io::Write;
        let input = self.input.as_mut().context("动画播放器未连接")?;
        writeln!(input, "{command}")?;
        input.flush()?;
        Ok(())
    }
    pub fn set_animation(&mut self, name: &str) -> Result<()> {
        self.send(serde_json::json!({"animation":name}))
    }
    pub fn resize(&mut self, resolution: (u32, u32)) -> Result<()> {
        self.send(serde_json::json!({"width":resolution.0,"height":resolution.1}))
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            std::thread::spawn(move || {
                if let Ok(status) = child.wait() {
                    crate::diagnostics::record(format!("animation exit: {status}"));
                }
            });
        }
    }
}

pub fn worker(
    root: PathBuf,
    path: PathBuf,
    limit: Option<usize>,
    resolution: (u32, u32),
) -> Result<()> {
    std::thread::Builder::new()
        .name("spine-renderer".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(move || -> Result<()> {
            use std::io::Write;
            let mut scene = Scene::load(&root, &path)?;
            let mut renderer = gpu::Renderer::new()?;
            crate::diagnostics::record(format!("animation loaded: {}", path.display()));
            let mut output = std::io::BufWriter::new(std::io::stdout());
            let info = serde_json::to_vec(
                &serde_json::json!({"names":scene.animations,"selected":scene.selected}),
            )?;
            output.write_all(&0u32.to_le_bytes())?;
            output.write_all(&(info.len() as u32).to_le_bytes())?;
            output.write_all(&info)?;
            output.flush()?;
            let (commands, rx) = mpsc::channel();
            std::thread::spawn(move || {
                use std::io::BufRead;
                for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    if let Ok(value) = serde_json::from_str::<Value>(&line) {
                        let _ = commands.send(value);
                    }
                }
            });
            let mut resolution = resolution;
            let mut previous = Instant::now();
            for index in 0.. {
                for command in rx.try_iter() {
                    if let Some(name) = command["animation"].as_str() {
                        scene.set_animation(name);
                    }
                    if let (Some(w), Some(h)) =
                        (command["width"].as_u64(), command["height"].as_u64())
                    {
                        resolution = (w.clamp(1, 2048) as u32, h.clamp(1, 2048) as u32);
                    }
                }
                let begin = Instant::now();
                let frame = scene.gpu_frame(
                    &mut renderer,
                    begin.duration_since(previous).as_secs_f32().min(0.1),
                    resolution.0,
                    resolution.1,
                )?;
                previous = begin;
                output.write_all(&frame.width().to_le_bytes())?;
                output.write_all(&frame.height().to_le_bytes())?;
                output.write_all(frame.as_raw())?;
                output.flush()?;
                if limit.is_some_and(|limit| index + 1 >= limit) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(16).saturating_sub(begin.elapsed()));
            }
            Ok(())
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("动画线程发生 panic"))?
}

/// Deterministic offscreen rendering for file export, independent of preview zoom.
pub fn export_frames(
    root: &Path,
    path: &Path,
    action: &str,
    long_edge: u32,
    fps: u32,
    seconds: u32,
    mut write: impl FnMut(&RgbaImage, u32, u32) -> Result<()>,
) -> Result<()> {
    ensure!(
        (256..=2048).contains(&long_edge) && [24, 30, 60].contains(&fps),
        "无效的动画导出规格"
    );
    let mut scene = Scene::load(root, path)?;
    if !action.is_empty() {
        ensure!(
            scene.animations.iter().any(|name| name == action),
            "该皮肤没有所选动作"
        );
        scene.set_animation(action);
    }
    let duration = if seconds == 0 {
        scene
            .layers
            .iter()
            .filter_map(|layer| layer.animation_state.get_current(0))
            .map(|track| track.animation().duration())
            .fold(0.0_f32, f32::max)
            .max(1. / fps as f32)
    } else {
        seconds as f32
    };
    ensure!(
        duration.is_finite() && duration <= 60.,
        "动作超过一分钟，请选择固定导出时长"
    );
    let [minx, miny, maxx, maxy] = scene.bounds;
    let width = (maxx - minx).max(1.);
    let height = (maxy - miny).max(1.);
    let scale = long_edge as f32 / width.max(height);
    let dimensions = (
        (width * scale).round().max(1.) as u32,
        (height * scale).round().max(1.) as u32,
    );
    let count = (duration * fps as f32).ceil().max(1.) as u32;
    let mut renderer = gpu::Renderer::new()?;
    let mut render_time = std::time::Duration::ZERO;
    for index in 0..count {
        let tick = std::time::Instant::now();
        let frame = scene.gpu_frame(
            &mut renderer,
            if index == 0 { 0. } else { 1. / fps as f32 },
            dimensions.0,
            dimensions.1,
        )?;
        render_time += tick.elapsed();
        write(&frame, index, count)?;
    }
    crate::diagnostics::record(format!(
        "skin export render/readback: {render_time:?}, frames={count}, dimensions={dimensions:?}"
    ));
    Ok(())
}

struct Texture(Result<RgbaImage, String>);
fn init() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        extension::set_create_texture_cb(|page, path| {
            #[cfg(windows)]
            let path = path.replace('/', "\\");
            let texture = image::open(&path)
                .map(|i| i.to_rgba8())
                .map_err(|e| format!("{path}: {e}"));
            page.renderer_object().set(Texture(texture));
        });
        extension::set_dispose_texture_cb(|page| unsafe {
            // The create callback installs exactly one Texture, owned by this atlas page.
            page.renderer_object().dispose::<Texture>();
        });
    });
}
fn package_file(root: &Path, value: &Value, key: &str) -> Result<PathBuf> {
    let relative = value[key].as_str().context("缺少 Spine 文件路径")?;
    let path = root.join(relative).canonicalize()?;
    ensure!(
        path.starts_with(root) && path.is_file(),
        "Spine 文件路径超出素材包"
    );
    Ok(path)
}
struct Background {
    image: RgbaImage,
    rect: [f32; 4],
    uv: [f32; 4],
    color: [f32; 4],
}
pub struct Scene {
    layers: Vec<SkeletonController>,
    backgrounds: Vec<Background>,
    bounds: [f32; 4],
    animations: Vec<String>,
    selected: String,
}
impl Scene {
    pub fn load(root: &Path, path: &Path) -> Result<Self> {
        init();
        let root = root.canonicalize()?;
        let root = root.as_path();
        let json: Value = serde_json::from_slice(&fs::read(path)?)?;
        let mut layers = Vec::new();
        let mut primary = None;
        let mut animations = Vec::new();
        for layer in json["layers"].as_array().context("缺少 Spine 图层")? {
            ensure!(
                layer["version"]
                    .as_str()
                    .is_some_and(|v| v.starts_with("4.2.")),
                "当前播放器支持 Spine 4.2"
            );
            let atlas_path = package_file(root, layer, "atlas")?;
            // Validate atlas page references before the runtime invokes its texture loader.
            let atlas_text = fs::read_to_string(&atlas_path)?;
            let mut after_blank = true;
            for line in atlas_text.lines() {
                let line = line.trim();
                if line.is_empty() {
                    after_blank = true;
                    continue;
                }
                if after_blank {
                    let page = atlas_path.parent().unwrap().join(line).canonicalize()?;
                    ensure!(page.starts_with(root), "图集纹理超出素材包");
                }
                after_blank = false;
            }
            crate::diagnostics::record(format!("load atlas: {}", atlas_path.display()));
            let atlas = Arc::new(Atlas::new_from_file(&atlas_path)?);
            crate::diagnostics::record("atlas loaded");
            for page in atlas.pages() {
                // Renderer object is created above and remains alive with the controller's atlas.
                let mut object = page.renderer_object();
                let texture = unsafe { object.get::<Texture>() }.context("图集没有纹理")?;
                if let Err(error) = &texture.0 {
                    anyhow::bail!("无法读取图集纹理：{error}");
                }
            }
            let skeleton = package_file(root, layer, "skeleton")?;
            crate::diagnostics::record(format!("load skeleton: {}", skeleton.display()));
            let data = Arc::new(if layer["format"] == "json" {
                SkeletonJson::new(atlas).read_skeleton_data_file(&skeleton)?
            } else {
                SkeletonBinary::new(atlas).read_skeleton_data_file(&skeleton)?
            });
            crate::diagnostics::record("skeleton loaded");
            let names = data
                .animations()
                .map(|a| a.name().to_owned())
                .collect::<Vec<_>>();
            if layers.is_empty() {
                animations = names.clone();
            }
            let idle = names
                .iter()
                .find(|n| n.eq_ignore_ascii_case("idle"))
                .or_else(|| names.iter().find(|n| n.to_lowercase().contains("idle")))
                .or_else(|| names.first())
                .cloned();
            let animation = primary
                .as_ref()
                .filter(|n| names.contains(n))
                .cloned()
                .or(idle);
            if layers.is_empty() {
                primary = animation.clone();
            }
            let state = Arc::new(AnimationStateData::new(data.clone()));
            let mut controller = SkeletonController::new(data, state);
            controller.settings.premultiplied_alpha = layer["premultiplied_alpha"] == true;
            if let Some(name) = animation {
                controller
                    .animation_state
                    .set_animation_by_name(0, &name, true)?;
            }
            crate::diagnostics::record("controller created");
            controller.update(0., Physics::Update);
            crate::diagnostics::record("controller posed");
            layers.push(controller);
        }
        ensure!(!layers.is_empty(), "没有可播放的 Spine 图层");
        let mut backgrounds = Vec::new();
        if let Some(items) = json["backgrounds"].as_array() {
            for bg in items {
                let get = |k: &str| bg[k].as_f64().unwrap_or(0.) as f32;
                let mut uv = [1., 1., 0., 0.];
                let mut color = [1.; 4];
                for i in 0..4 {
                    uv[i] = bg["uv"][i].as_f64().unwrap_or(uv[i] as f64) as f32;
                    color[i] = bg["color"][i].as_f64().unwrap_or(1.) as f32;
                }
                backgrounds.push(Background {
                    image: image::open(package_file(root, bg, "path")?)?.to_rgba8(),
                    rect: [get("x"), get("y"), get("width"), get("height")],
                    uv,
                    color,
                });
            }
        }
        let mut scene = Self {
            layers,
            backgrounds,
            animations,
            selected: primary.unwrap_or_default(),
            bounds: [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ],
        };
        scene.measure_bounds();
        ensure!(
            scene.bounds.iter().all(|n| n.is_finite()),
            "Spine 没有可见几何体"
        );
        Ok(scene)
    }
    fn set_animation(&mut self, name: &str) {
        if !self.animations.iter().any(|s| s == name) {
            return;
        }
        self.selected = name.to_owned();
        for layer in &mut self.layers {
            let names = layer
                .skeleton
                .data()
                .animations()
                .map(|a| a.name().to_owned())
                .collect::<Vec<_>>();
            let choice = names
                .iter()
                .find(|s| s.as_str() == name)
                .or_else(|| names.iter().find(|s| s.eq_ignore_ascii_case("idle")));
            layer.animation_state.clear_tracks();
            layer.skeleton.set_to_setup_pose();
            if let Some(choice) = choice {
                let _ = layer.animation_state.set_animation_by_name(0, choice, true);
            }
            layer.update(0., Physics::Update);
        }
        self.measure_bounds();
    }
    fn measure_bounds(&mut self) {
        self.bounds = [
            f32::INFINITY,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        ];
        for layer in &mut self.layers {
            let current = layer
                .animation_state
                .get_current(0)
                .map(|t| (t.animation().name().to_owned(), t.animation().duration()));
            let step = current.as_ref().map(|(_, d)| d.min(4.) / 16.).unwrap_or(0.);
            for sample in 0..=16 {
                if sample > 0 {
                    layer.update(step, Physics::Update);
                }
                for mesh in mesh::collect(layer) {
                    for [x, y] in mesh.vertices {
                        extend(&mut self.bounds, x, y);
                    }
                }
            }
            layer.animation_state.clear_tracks();
            layer.skeleton.set_to_setup_pose();
            if let Some((name, _)) = current {
                let _ = layer.animation_state.set_animation_by_name(0, &name, true);
            }
            layer.update(0., Physics::Reset);
        }
        for bg in &self.backgrounds {
            let [x, y, w, h] = bg.rect;
            extend(&mut self.bounds, x, y);
            extend(&mut self.bounds, x + w, y + h);
        }
    }
    fn gpu_frame(
        &mut self,
        renderer: &mut gpu::Renderer,
        dt: f32,
        w: u32,
        h: u32,
    ) -> Result<RgbaImage> {
        let [minx, miny, maxx, maxy] = self.bounds;
        let scale = (w as f32 / (maxx - minx).max(1.)).min(h as f32 / (maxy - miny).max(1.)) * 0.94;
        let map = |[x, y]: [f32; 2]| {
            [
                (x - (minx + maxx) * 0.5) * scale * 2. / w as f32,
                (y - (miny + maxy) * 0.5) * scale * 2. / h as f32,
            ]
        };
        let mut draws = Vec::new();
        for bg in &self.backgrounds {
            let [x, y, bw, bh] = bg.rect;
            let [sx, sy, ox, oy] = bg.uv;
            let xy = [[x, y], [x + bw, y], [x + bw, y + bh], [x, y + bh]].map(map);
            let uv = [
                [ox, 1. - oy],
                [ox + sx, 1. - oy],
                [ox + sx, 1. - oy - sy],
                [ox, 1. - oy - sy],
            ];
            let vertices = [0, 1, 2, 2, 3, 0]
                .into_iter()
                .map(|i| gpu::Vertex {
                    position: xy[i],
                    uv: uv[i],
                    color: bg.color,
                    dark: [0.; 4],
                    pma: 0.,
                })
                .collect();
            draws.push(gpu::Draw {
                key: &bg.image as *const RgbaImage as usize,
                image: &bg.image,
                vertices,
                blend: BlendMode::Normal,
            });
        }
        for layer in &mut self.layers {
            layer.update(dt, Physics::Update);
            for mesh in mesh::collect(layer) {
                let Some(ptr) = mesh.attachment_renderer_object else {
                    continue;
                };
                // The atlas owns the texture for the entire renderer session.
                let texture = unsafe { &*ptr.cast::<Texture>() };
                let Ok(image) = &texture.0 else {
                    continue;
                };
                let color = [mesh.color.r, mesh.color.g, mesh.color.b, mesh.color.a];
                let dark = [
                    mesh.dark_color.r,
                    mesh.dark_color.g,
                    mesh.dark_color.b,
                    mesh.dark_color.a,
                ];
                let vertices = mesh
                    .indices
                    .iter()
                    .map(|&i| {
                        let i = i as usize;
                        gpu::Vertex {
                            position: map(mesh.vertices[i]),
                            uv: mesh.uvs[i],
                            color,
                            dark,
                            pma: if mesh.premultiplied_alpha { 1. } else { 0. },
                        }
                    })
                    .collect();
                draws.push(gpu::Draw {
                    key: ptr as usize,
                    image,
                    vertices,
                    blend: mesh.blend_mode,
                });
            }
        }
        renderer.render(&draws, w, h)
    }
    #[cfg(test)]
    pub fn frame(&mut self, dt: f32, w: u32, h: u32) -> RgbaImage {
        let mut pixels = vec![[0f32; 4]; (w * h) as usize];
        let [minx, miny, maxx, maxy] = self.bounds;
        let scale = (w as f32 / (maxx - minx).max(1.)).min(h as f32 / (maxy - miny).max(1.)) * 0.94;
        let map = |[x, y]: [f32; 2]| {
            [
                (x - (minx + maxx) * 0.5) * scale + w as f32 * 0.5,
                h as f32 * 0.5 - (y - (miny + maxy) * 0.5) * scale,
            ]
        };
        for bg in &self.backgrounds {
            let [x, y, bw, bh] = bg.rect;
            let [sx, sy, ox, oy] = bg.uv;
            let xy = [[x, y], [x + bw, y], [x + bw, y + bh], [x, y + bh]].map(map);
            let uv = [
                [ox, 1. - oy],
                [ox + sx, 1. - oy],
                [ox + sx, 1. - oy - sy],
                [ox, 1. - oy - sy],
            ];
            for ids in [[0, 1, 2], [0, 2, 3]] {
                triangle(
                    &mut pixels,
                    w,
                    h,
                    &bg.image,
                    ids.map(|i| xy[i]),
                    ids.map(|i| uv[i]),
                    bg.color,
                    [0.; 4],
                    false,
                    BlendMode::Normal,
                );
            }
        }
        for layer in &mut self.layers {
            layer.update(dt, Physics::Update);
            for mesh in mesh::collect(layer) {
                let Some(ptr) = mesh.attachment_renderer_object else {
                    continue;
                };
                // Atlas owns the Texture until this layer/controller is dropped.
                let texture = unsafe { &*ptr.cast::<Texture>() };
                let Ok(image) = &texture.0 else {
                    continue;
                };
                for ids in mesh.indices.as_chunks::<3>().0 {
                    let ids = [ids[0] as usize, ids[1] as usize, ids[2] as usize];
                    triangle(
                        &mut pixels,
                        w,
                        h,
                        image,
                        ids.map(|i| map(mesh.vertices[i])),
                        ids.map(|i| mesh.uvs[i]),
                        [mesh.color.r, mesh.color.g, mesh.color.b, mesh.color.a],
                        [
                            mesh.dark_color.r,
                            mesh.dark_color.g,
                            mesh.dark_color.b,
                            mesh.dark_color.a,
                        ],
                        mesh.premultiplied_alpha,
                        mesh.blend_mode,
                    );
                }
            }
        }
        // GPUI's RenderImage consumes straight BGRA frames.
        RgbaImage::from_raw(
            w,
            h,
            pixels
                .into_iter()
                .flat_map(|p| {
                    let a = p[3].clamp(0., 1.);
                    let c = |i: usize| {
                        if a > 0. {
                            (p[i] / a).clamp(0., 1.) * 255.
                        } else {
                            0.
                        }
                    };
                    [c(2) as u8, c(1) as u8, c(0) as u8, (a * 255.) as u8]
                })
                .collect(),
        )
        .unwrap()
    }
}

fn extend(b: &mut [f32; 4], x: f32, y: f32) {
    b[0] = b[0].min(x);
    b[1] = b[1].min(y);
    b[2] = b[2].max(x);
    b[3] = b[3].max(y);
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn triangle(
    dst: &mut [[f32; 4]],
    w: u32,
    h: u32,
    texture: &RgbaImage,
    mut xy: [[f32; 2]; 3],
    mut uv: [[f32; 2]; 3],
    color: [f32; 4],
    dark: [f32; 4],
    pma: bool,
    blend: BlendMode,
) {
    let cross = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
    };
    let mut area = cross(xy[0], xy[1], xy[2]);
    if area.abs() < 0.0001 {
        return;
    }
    if area < 0. {
        xy.swap(1, 2);
        uv.swap(1, 2);
        area = -area;
    }
    let minx = xy
        .iter()
        .map(|p| p[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.) as u32;
    let miny = xy
        .iter()
        .map(|p| p[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.) as u32;
    let maxx = xy
        .iter()
        .map(|p| p[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., w as f32) as u32;
    let maxy = xy
        .iter()
        .map(|p| p[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0., h as f32) as u32;
    let top_left = |a: [f32; 2], b: [f32; 2]| b[1] < a[1] || (b[1] == a[1] && b[0] > a[0]);
    let inclusive = [
        top_left(xy[1], xy[2]),
        top_left(xy[2], xy[0]),
        top_left(xy[0], xy[1]),
    ];
    for y in miny..maxy {
        for x in minx..maxx {
            let p = [x as f32 + 0.5, y as f32 + 0.5];
            let e = [
                cross(xy[1], xy[2], p),
                cross(xy[2], xy[0], p),
                cross(xy[0], xy[1], p),
            ];
            if (0..3).any(|i| e[i] < 0. || (e[i] == 0. && !inclusive[i])) {
                continue;
            }
            let weights = e.map(|n| n / area);
            let coord = |c: usize| (0..3).map(|i| uv[i][c] * weights[i]).sum::<f32>();
            let fx =
                (coord(0) * texture.width() as f32 - 0.5).clamp(0., (texture.width() - 1) as f32);
            let fy =
                (coord(1) * texture.height() as f32 - 0.5).clamp(0., (texture.height() - 1) as f32);
            let (tx, ty) = (fx.floor() as u32, fy.floor() as u32);
            let (nx, ny) = (
                (tx + 1).min(texture.width() - 1),
                (ty + 1).min(texture.height() - 1),
            );
            let (wx, wy) = (fx - tx as f32, fy - ty as f32);
            let samples = [
                texture.get_pixel(tx, ty).0,
                texture.get_pixel(nx, ty).0,
                texture.get_pixel(tx, ny).0,
                texture.get_pixel(nx, ny).0,
            ];
            let tex: [f32; 4] = std::array::from_fn(|c| {
                ((samples[0][c] as f32 * (1. - wx) + samples[1][c] as f32 * wx) * (1. - wy)
                    + (samples[2][c] as f32 * (1. - wx) + samples[3][c] as f32 * wx) * wy)
                    / 255.
            });
            let alpha = tex[3] * color[3];
            if alpha <= 0. {
                continue;
            }
            let src: [f32; 3] = std::array::from_fn(|i| {
                let c = ((tex[3] - 1.) * dark[3] + 1. - tex[i]) * dark[i] + tex[i] * color[i];
                c * if pma { 1. } else { alpha }
            });
            let d = &mut dst[(y * w + x) as usize];
            for i in 0..3 {
                d[i] = match blend {
                    BlendMode::Additive => src[i] + d[i],
                    BlendMode::Multiply => {
                        src[i] * d[i] + src[i] * (1. - d[3]) + d[i] * (1. - alpha)
                    }
                    BlendMode::Screen => src[i] + d[i] - src[i] * d[i],
                    _ => src[i] + d[i] * (1. - alpha),
                };
            }
            d[3] = alpha + d[3] * (1. - alpha);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_translucent_triangles_share_edge_without_double_blending() {
        let texture = RgbaImage::from_pixel(1, 1, image::Rgba([255, 255, 255, 128]));
        let mut dst = vec![[0.; 4]; 16];
        let xy = [[0., 0.], [4., 0.], [4., 4.], [0., 4.]];
        for ids in [[0, 1, 2], [0, 2, 3]] {
            triangle(
                &mut dst,
                4,
                4,
                &texture,
                ids.map(|i| xy[i]),
                [[0.; 2]; 3],
                [1.; 4],
                [0.; 4],
                false,
                BlendMode::Normal,
            );
        }
        assert!(dst.iter().all(|p| (p[3] - 128. / 255.).abs() < 0.001));
    }

    #[test]
    fn loads_weighted_mesh_that_previously_overran_runtime_buffers() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local/target/debug/majsoul-gallery-data/materials");
        if !root.is_dir() {
            return Ok(());
        }
        let mut scene = Scene::load(
            &root,
            &root.join("characters/20000107/skins/40010703/spine/scene.json"),
        )?;
        let frame = scene.frame(0.04, 96, 128);
        assert!(frame.pixels().any(|p| p[3] > 0));
        if let Some(name) = scene.animations.last().cloned() {
            scene.set_animation(&name);
            assert_eq!(scene.selected, name);
            let frame = scene.frame(0.04, 96, 128);
            assert!(frame.pixels().any(|p| p[3] > 0));
        }
        Ok(())
    }
    #[test]
    fn local_multilayer_scene_produces_changing_frames() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local/target/debug/majsoul-gallery-data/materials");
        if !root.is_dir() {
            return Ok(());
        }
        let root = root.canonicalize()?;
        let path = root.join("characters/20000108/skins/40010803/spine/scene.json");
        let start = Instant::now();
        let mut scene = Scene::load(&root, &path)?;
        assert_eq!(scene.layers.len(), 3);
        let loaded = start.elapsed();
        let first = scene.frame(0.04, 512, 640);
        let second = scene.frame(0.3, 512, 640);
        assert!(first.pixels().any(|p| p[3] > 0));
        assert_ne!(first.as_raw(), second.as_raw());
        eprintln!(
            "Spine load: {:?}; 2 frames: {:?}",
            loaded,
            start.elapsed() - loaded
        );
        Ok(())
    }
}
