//! Image loading and contain-fit drawing shared by cards and the detail preview.
use super::*;

pub(super) fn fitted_bounds(
    view: Bounds<Pixels>,
    width: f32,
    height: f32,
    zoom: f32,
    pan: Point<Pixels>,
) -> Bounds<Pixels> {
    let scale =
        (f32::from(view.size.width) / width).min(f32::from(view.size.height) / height) * zoom;
    let extent = size(px(width * scale), px(height * scale));
    Bounds::new(
        point(
            view.origin.x + (view.size.width - extent.width) * 0.5 + pan.x,
            view.origin.y + (view.size.height - extent.height) * 0.5 + pan.y,
        ),
        extent,
    )
}
pub(super) fn load(
    path: &std::path::Path,
    window: &mut Window,
    cx: &mut App,
) -> Option<Arc<RenderImage>> {
    window
        .use_asset::<ImageAssetLoader>(&Resource::from(path.to_owned()), cx)
        .and_then(Result::ok)
}

pub(super) fn paint(
    bounds: Bounds<Pixels>,
    image: Arc<RenderImage>,
    zoom: f32,
    pan: Point<Pixels>,
    window: &mut Window,
) {
    let extent = image.size(0);
    if extent.width.0 == 0 || extent.height.0 == 0 {
        return;
    }
    let destination = fitted_bounds(
        bounds,
        extent.width.0 as f32,
        extent.height.0 as f32,
        zoom,
        pan,
    );
    if let Err(error) = window.paint_image(bounds, destination, Corners::default(), image, 0, false)
    {
        crate::diagnostics::record(format!("image paint failed: {error:#}"));
    }
}

pub(super) fn fitted_picture(path: PathBuf) -> impl IntoElement {
    canvas(
        move |_, window, cx| load(&path, window, cx),
        move |bounds, image, window, _| {
            if let Some(image) = image {
                paint(bounds, image, 1., point(px(0.), px(0.)), window);
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full()
}

pub(super) fn app_icon(size: f32, scale_factor: f32) -> AnyElement {
    // Pre-filter at the actual display resolution instead of sampling the full-size artwork.
    static SOURCE: std::sync::OnceLock<image::DynamicImage> = std::sync::OnceLock::new();
    static ICONS: std::sync::OnceLock<
        std::sync::Mutex<std::collections::BTreeMap<u32, Arc<Image>>>,
    > = std::sync::OnceLock::new();
    let pixels = (size * scale_factor).round().clamp(16., 256.) as u32;
    let mut icons = ICONS
        .get_or_init(Default::default)
        .lock()
        .expect("图标缓存锁失效");
    let image = icons
        .entry(pixels)
        .or_insert_with(|| {
            let source = SOURCE.get_or_init(|| {
                image::load_from_memory(include_bytes!("../assets/app-icon.png"))
                    .expect("应用图标无效")
            });
            let resized =
                source.resize_exact(pixels, pixels, image::imageops::FilterType::Lanczos3);
            let mut bytes = std::io::Cursor::new(Vec::new());
            resized
                .write_to(&mut bytes, image::ImageFormat::Png)
                .expect("无法生成小尺寸图标");
            Arc::new(Image::from_bytes(ImageFormat::Png, bytes.into_inner()))
        })
        .clone();
    img(image)
        .size(px(size))
        .object_fit(ObjectFit::Contain)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::fitted_bounds;
    use gpui_kit::{Bounds, point, px, size};
    #[test]
    fn fit_keeps_tall_and_wide_images_inside_viewport() {
        for (view_w, view_h) in [(600., 400.), (258.4, 351.54), (130., 175.5)] {
            let view = Bounds::new(point(px(10.), px(20.)), size(px(view_w), px(view_h)));
            for (w, h) in [
                (1000., 3000.),
                (3000., 1000.),
                (866., 1357.),
                (463., 1346.),
                (906., 1495.),
            ] {
                let result = fitted_bounds(view, w, h, 1., point(px(0.), px(0.)));
                assert!(
                    result.origin.x + px(0.01) >= view.origin.x
                        && result.origin.y + px(0.01) >= view.origin.y
                );
                assert!(
                    result.origin.x + result.size.width
                        <= view.origin.x + view.size.width + px(0.01)
                );
                assert!(
                    result.origin.y + result.size.height
                        <= view.origin.y + view.size.height + px(0.01)
                );
            }
        }
    }
}
