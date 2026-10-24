//! PNG and lossless WebP export. Native rendering/encoding runs in an isolated process.
use crate::task_process::emit;
use anyhow::{Context, Result, ensure};
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    #[default]
    Png,
    Webp,
}
impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Request {
    pub root: PathBuf,
    pub image: PathBuf,
    pub scene: Option<PathBuf>,
    pub action: String,
    pub output: PathBuf,
    pub animated: bool,
    pub format: Format,
    pub long_edge: u32,
    pub fps: u32,
    pub seconds: u32,
    pub white_background: bool,
    pub lossy: bool,
}

fn encoder(width: u32, height: u32, lossy: bool) -> Result<webp_animation::Encoder> {
    let config = if lossy {
        webp_animation::EncodingConfig::new_lossy(90.)
    } else {
        webp_animation::EncodingConfig::default()
    };
    Ok(webp_animation::Encoder::new_with_options(
        (width, height),
        webp_animation::EncoderOptions {
            encoding_config: Some(config),
            ..Default::default()
        },
    )?)
}

#[cfg(any(feature = "spine", test))]
fn export_rgba(image: &RgbaImage, white: bool) -> RgbaImage {
    // The GPUI preview consumes straight BGRA; file encoders consume straight RGBA.
    let mut frame = image.clone();
    for pixel in frame.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    background(&mut frame, white);
    frame
}

pub struct ExportJob {
    pub task: crate::task_process::Job,
    _request: tempfile::NamedTempFile,
}
impl ExportJob {
    pub fn start(request: &Request) -> Result<Self> {
        let cache = crate::model::state_dir().join("cache");
        fs::create_dir_all(&cache)?;
        let mut file = tempfile::NamedTempFile::new_in(cache)?;
        file.write_all(&serde_json::to_vec(request)?)?;
        file.flush()?;
        let mut command = Command::new(std::env::current_exe()?);
        command.arg("--skin-export-worker").arg(file.path());
        Ok(Self {
            task: crate::task_process::Job::spawn(command)?,
            _request: file,
        })
    }
}

fn background(frame: &mut RgbaImage, white: bool) {
    if white {
        for pixel in frame.pixels_mut() {
            let alpha = pixel[3] as u32;
            for channel in &mut pixel.0[..3] {
                *channel = ((*channel as u32 * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
            }
            pixel[3] = 255;
        }
    }
}

pub fn export(request: &Request, progress: &(dyn Fn(u32, u32) + Sync)) -> Result<()> {
    ensure!(
        !request.animated || request.format == Format::Webp,
        "动画请选择 WebP 格式"
    );
    ensure!([0, 5, 10, 15].contains(&request.seconds), "无效的导出时长");
    if request.animated {
        #[cfg(feature = "spine")]
        return export_animation(request, progress);
        #[cfg(not(feature = "spine"))]
        anyhow::bail!("此构建未启用 spine 功能");
    } else {
        let mut image = image::open(&request.image)
            .context("无法读取皮肤原图")?
            .into_rgba8();
        background(&mut image, request.white_background);
        let data = match request.format {
            Format::Png => {
                let mut bytes = std::io::Cursor::new(Vec::new());
                image.write_to(&mut bytes, image::ImageFormat::Png)?;
                bytes.into_inner()
            }
            Format::Webp => {
                let mut encoder = encoder(image.width(), image.height(), request.lossy)?;
                encoder.add_frame(image.as_raw(), 0)?;
                encoder.finalize(1000)?.to_vec()
            }
        };
        crate::files::write_atomic(&request.output, &data)?;
        progress(1, 1);
    }
    Ok(())
}
#[cfg(feature = "spine")]
fn export_animation(request: &Request, progress: &(dyn Fn(u32, u32) + Sync)) -> Result<()> {
    let scene = request.scene.as_ref().context("当前皮肤没有动画")?;
    let started = std::time::Instant::now();
    let data = std::thread::scope(|scope| -> Result<Vec<u8>> {
        // Bound memory while overlapping GPU rendering/readback with CPU WebP encoding.
        let (send, receive) = std::sync::mpsc::sync_channel::<(RgbaImage, u32, u32)>(2);
        let encoding = scope.spawn(move || -> Result<Vec<u8>> {
            let mut output = None;
            let mut end = 0;
            let mut elapsed = std::time::Duration::ZERO;
            for (frame, index, total) in receive {
                let tick = std::time::Instant::now();
                if output.is_none() {
                    output = Some(encoder(frame.width(), frame.height(), request.lossy)?);
                }
                output
                    .as_mut()
                    .unwrap()
                    .add_frame(frame.as_raw(), (index * 1000 / request.fps) as i32)?;
                elapsed += tick.elapsed();
                end = (total * 1000 / request.fps) as i32;
                progress(index + 1, total);
            }
            progress(0, 0);
            let tick = std::time::Instant::now();
            let data = output.context("动画没有输出帧")?.finalize(end)?.to_vec();
            crate::diagnostics::record(format!(
                "skin export encoding: frames={elapsed:?}, finalize={:?}",
                tick.elapsed()
            ));
            Ok(data)
        });
        let rendering = crate::animation::export_frames(
            &request.root,
            scene,
            &request.action,
            request.long_edge,
            request.fps,
            request.seconds,
            |image, index, total| {
                send.send((export_rgba(image, request.white_background), index, total))
                    .map_err(|_| anyhow::anyhow!("编码任务已结束"))?;
                Ok(())
            },
        );
        drop(send);
        let encoded = encoding
            .join()
            .map_err(|_| anyhow::anyhow!("动画编码线程发生异常"))?;
        let data = encoded?;
        rendering?;
        Ok(data)
    })?;
    crate::diagnostics::record(format!(
        "skin export total: {:?}, bytes={}",
        started.elapsed(),
        data.len()
    ));
    crate::files::write_atomic(&request.output, &data)?;
    Ok(())
}

pub fn run_worker(path: &Path) {
    let result = (|| -> Result<()> {
        let request: Request = serde_json::from_slice(&fs::read(path)?)?;
        std::thread::Builder::new()
            .name("skin-export".into())
            .stack_size(16 * 1024 * 1024)
            .spawn(move || -> Result<()> {
                export(&request, &|done, total| {
                    emit(json!({"type":"progress", "done":done,"total":total,
                    "percent": if total == 0 { 99. } else { done as f32 * 99. / total as f32 },
                    "label":if total == 0 {"正在完成编码"} else {"正在导出皮肤"}, "current":""}))
                })?;
                emit(json!({"type":"complete", "output":request.output}));
                Ok(())
            })?
            .join()
            .map_err(|_| anyhow::anyhow!("皮肤导出线程发生异常"))??;
        Ok(())
    })();
    if let Err(error) = result {
        emit(json!({"type":"error", "message":format!("{error:#}")}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(feature = "spine"))]
    #[test]
    fn static_build_rejects_animation_without_creating_output() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let request = Request {
            root: temp.path().into(),
            image: temp.path().join("unused.png"),
            scene: None,
            action: String::new(),
            output: temp.path().join("animation.webp"),
            animated: true,
            format: Format::Webp,
            long_edge: 256,
            fps: 24,
            seconds: 5,
            white_background: false,
            lossy: false,
        };
        assert!(
            export(&request, &|_, _| {})
                .unwrap_err()
                .to_string()
                .contains("未启用 spine")
        );
        assert!(!request.output.exists());
        Ok(())
    }

    #[test]
    fn preview_bgra_is_converted_to_rgba_for_files() {
        let bgra = RgbaImage::from_raw(1, 1, vec![20, 40, 230, 255]).unwrap();
        assert_eq!(
            export_rgba(&bgra, false).get_pixel(0, 0).0,
            [230, 40, 20, 255]
        );
    }
    #[test]
    fn transparent_and_white_background_exports_preserve_expected_pixels() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("source.png");
        let pixels = RgbaImage::from_raw(2, 1, vec![90, 40, 210, 128, 0, 0, 0, 0]).unwrap();
        pixels.save(&source)?;
        let mut request = Request {
            root: directory.path().into(),
            image: source,
            scene: None,
            action: String::new(),
            output: directory.path().join("result.png"),
            animated: false,
            format: Format::Png,
            long_edge: 1024,
            fps: 30,
            seconds: 0,
            white_background: false,
            lossy: false,
        };
        export(&request, &|_, _| {})?;
        assert_eq!(image::open(&request.output)?.into_rgba8(), pixels);
        request.format = Format::Webp;
        request.output = directory.path().join("transparent.webp");
        export(&request, &|_, _| {})?;
        let bytes = fs::read(&request.output)?;
        let frame = webp_animation::Decoder::new(&bytes)?
            .into_iter()
            .next()
            .context("missing transparent frame")?;
        assert_eq!(&frame.data()[..4], &[90, 40, 210, 128]);
        assert_eq!(frame.data()[7], 0);
        request.format = Format::Png;
        request.output = directory.path().join("white.png");
        request.white_background = true;
        export(&request, &|_, _| {})?;
        let white = image::open(&request.output)?.into_rgba8();
        assert_eq!(white.get_pixel(0, 0).0, [172, 147, 232, 255]);
        assert_eq!(white.get_pixel(1, 0).0, [255, 255, 255, 255]);
        request.format = Format::Webp;
        request.output = directory.path().join("result.webp");
        export(&request, &|_, _| {})?;
        let bytes = fs::read(&request.output)?;
        let decoder = webp_animation::Decoder::new(&bytes)?;
        let frame = decoder.into_iter().next().context("missing frame")?;
        assert_eq!(frame.data(), white.as_raw());
        Ok(())
    }
}

#[cfg(all(test, feature = "spine"))]
mod local_tests {
    use super::*;
    use serde_json::Value;
    #[test]
    #[ignore = "requires a locally imported library and DirectX GPU"]
    fn exports_real_spine_frames_to_transparent_lossless_webp() -> Result<()> {
        export_real(false)
    }
    #[test]
    #[ignore = "requires a locally imported library and DirectX GPU"]
    fn exports_real_spine_frames_to_transparent_lossy_webp() -> Result<()> {
        export_real(true)
    }
    fn export_real(lossy: bool) -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let settings: Value = serde_json::from_slice(&fs::read(
            root.join(".local/target/debug/majsoul-gallery-data/settings.json"),
        )?)?;
        let assets = PathBuf::from(settings["assets"].as_str().context("missing assets")?);
        let destination = tempfile::tempdir()?;
        let request = Request {
            root: assets.clone(),
            image: assets.join("unused.png"),
            scene: Some(assets.join("characters/200001/skins/400107/spine/scene.json")),
            action: String::new(),
            output: destination.path().join("spine.webp"),
            animated: true,
            format: Format::Webp,
            long_edge: 256,
            fps: 24,
            seconds: 5,
            white_background: false,
            lossy,
        };
        let output = request.output.clone();
        let started = std::time::Instant::now();
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn(move || export(&request, &|_, _| {}))?
            .join()
            .unwrap()?;
        let bytes = fs::read(&output)?;
        eprintln!(
            "lossy={lossy}, elapsed={:?}, bytes={}",
            started.elapsed(),
            bytes.len()
        );
        let decoder = webp_animation::Decoder::new(&bytes)?;
        let frames = decoder.into_iter().collect::<Vec<_>>();
        ensure!(frames.len() > 1, "missing animated frames");
        let (width, height) = frames[0].dimensions();
        assert_eq!(width.max(height), 256);
        assert!(
            frames[0]
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] == 0)
        );
        assert!(
            frames[0]
                .data()
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[3] != 0)
        );
        assert!(frames.iter().skip(1).any(|f| f.data() != frames[0].data()));
        assert_eq!(frames.last().unwrap().timestamp(), 5000);
        Ok(())
    }
}
