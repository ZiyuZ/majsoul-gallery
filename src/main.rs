#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod account;
#[cfg(feature = "spine")]
mod animation;
mod bond;
mod cli;
mod diagnostics;
mod discovery;
mod files;
mod importer;
mod model;
mod report;
mod search;
mod settings;
mod skin_export;
mod storage;
mod task_process;
mod text;
mod ui;
use gpui_kit::component::{Theme, ThemeMode};
use gpui_kit::*;
fn main() {
    diagnostics::init();
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|s| s == "--skin-export-worker") {
        if let Some(path) = args.get(1) {
            skin_export::run_worker(std::path::Path::new(path));
        }
        return;
    }
    if args.first().is_some_and(|s| s == "--import-worker") {
        if args.len() >= 3 {
            importer::run_worker(
                std::path::Path::new(&args[1]),
                std::path::Path::new(&args[2]),
                args.iter().any(|arg| arg == "--full"),
            );
        }
        return;
    }
    if args
        .first()
        .is_some_and(|s| s == "--discover-installations")
    {
        for path in discovery::find_installs(None) {
            println!("{}", model::display_path(&path));
        }
        return;
    }
    #[cfg(not(feature = "spine"))]
    if args.first().is_some_and(|s| s == "--animation-worker") {
        eprintln!("此构建未启用 spine 功能");
        std::process::exit(2);
    }
    #[cfg(feature = "spine")]
    if args.first().is_some_and(|s| s == "--animation-worker") {
        let result = if args.len() >= 3 {
            animation::worker(
                args[1].clone().into(),
                args[2].clone().into(),
                args.get(3)
                    .and_then(|s| s.to_str()?.parse::<usize>().ok())
                    .filter(|n| *n > 0),
                (
                    args.get(4)
                        .and_then(|s| s.to_str()?.parse().ok())
                        .unwrap_or(512),
                    args.get(5)
                        .and_then(|s| s.to_str()?.parse().ok())
                        .unwrap_or(640),
                ),
            )
        } else {
            Err(anyhow::anyhow!("缺少动画素材路径"))
        };
        if let Err(error) = result {
            diagnostics::record(format!("animation failed: {error:#}"));
            eprintln!("{error:#}");
            std::process::exit(1);
        }
        return;
    }
    diagnostics::record("application start");
    if let Some(path) = std::env::args_os()
        .skip(1)
        .collect::<Vec<_>>()
        .strip_prefix(&["--check-assets".into()])
        .and_then(|args| args.first())
        .cloned()
    {
        match model::Library::load(std::path::Path::new(&path)) {
            Ok(lib) => println!(
                "{} characters, {} skins, {} decorations",
                lib.characters.len(),
                lib.skin_count(),
                lib.decorations.len()
            ),
            Err(error) => {
                eprintln!("{error:#}");
                std::process::exit(1);
            }
        }
        return;
    }
    match cli::run_if_requested(&args) {
        Ok(true) => return,
        Err(error) => {
            eprintln!("导出失败：{error:#}");
            std::process::exit(1);
        }
        Ok(false) => {}
    }
    if let Err(error) = storage::clear_import_cache(&model::state_dir()) {
        diagnostics::record(format!("material storage preparation failed: {error:#}"));
    }
    gpui_kit::application()
        .with_assets(gpui_kit::assets::AllAssets)
        .run(|cx| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Light, None, cx);
            ui::configure_theme(cx, false);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1360.), px(900.)),
                    cx,
                ))),
                window_min_size: Some(size(px(1000.), px(680.))),
                titlebar: Some(TitlebarOptions {
                    title: Some(text::APP_NAME.into()),
                    ..gpui_kit::component::TitleBar::title_bar_options()
                }),
                ..gpui_kit::component::TitleBar::window_options()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::Gallery::new(window, cx))
            })
            .expect("无法打开雀魂藏馆窗口");
            cx.activate(true);
        });
}
