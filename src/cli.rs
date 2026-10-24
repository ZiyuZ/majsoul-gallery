use crate::{
    account::AccountSnapshot,
    model::{self, Library},
    report::{self, GiftOptions, OwnedOptions},
    settings::Settings,
};
use anyhow::{Context, Result, ensure};
use clap::{Parser, ValueEnum};
use std::{ffi::OsString, path::PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "majsoul-gallery",
    version,
    about = "雀魂藏馆：图表与收藏清单导出",
    after_help = "不带导出参数时启动图形界面。输出路径使用 .html 生成可搜索网页，使用 .png 生成紧凑图片。"
)]
struct Args {
    #[arg(
        long,
        value_name = "HTML_OR_PNG",
        required_unless_present = "export_owned",
        conflicts_with = "export_owned",
        help = "导出角色喜好表"
    )]
    export_gifts: Option<PathBuf>,
    #[arg(long, value_name = "HTML_OR_PNG", help = "导出账号已拥有清单")]
    export_owned: Option<PathBuf>,
    #[arg(
        long,
        value_name = "DIR",
        help = "素材包路径；默认使用应用记录的素材库"
    )]
    assets: Option<PathBuf>,
    #[arg(
        long,
        value_enum,
        default_value = "all",
        help = "喜好表的樱花／竹林范围"
    )]
    pool: Pool,
    #[arg(
        long,
        value_enum,
        default_value = "all",
        help = "喜好表的常驻／限定范围"
    )]
    availability: Availability,
    #[arg(
        long,
        value_name = "JSON",
        conflicts_with = "demo_account",
        requires = "export_owned",
        help = "已拥有清单使用的账号记录；默认读取最近同步的账号"
    )]
    account: Option<PathBuf>,
    #[arg(long,hide=!cfg!(debug_assertions),requires="export_owned",help="使用示例账号导出已拥有清单（仅 Debug 构建）")]
    demo_account: bool,
    #[arg(
        long,
        value_enum,
        value_delimiter = ',',
        requires = "export_owned",
        help = "已拥有清单的类型；默认包含全部三类"
    )]
    types: Vec<OwnedType>,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Pool {
    All,
    Sakura,
    Bamboo,
}
#[derive(Clone, Copy, Debug, ValueEnum)]
enum Availability {
    All,
    Permanent,
    Limited,
}
#[derive(Clone, Copy, Debug, PartialEq, ValueEnum)]
enum OwnedType {
    Characters,
    Skins,
    Decorations,
}

pub fn run_if_requested(args: &[OsString]) -> Result<bool> {
    if !args.iter().any(|arg| {
        matches!(
            arg.to_string_lossy().split('=').next(),
            Some("--export-gifts" | "--export-owned" | "--help" | "-h" | "--version" | "-V")
        )
    }) {
        return Ok(false);
    }
    #[cfg(windows)]
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(
            windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS,
        );
    }
    let args = Args::parse_from(
        std::iter::once(OsString::from("majsoul-gallery")).chain(args.iter().cloned()),
    );
    let settings = Settings::load();
    let assets = args
        .assets
        .or(settings.assets)
        .unwrap_or_else(|| model::state_dir().join("materials"));
    println!("读取素材库：{}", model::display_path(&assets));
    let library = Library::load(&assets)?;
    let destination = if let Some(path) = args.export_gifts {
        let options = GiftOptions {
            sakura: !matches!(args.pool, Pool::Bamboo),
            bamboo: !matches!(args.pool, Pool::Sakura),
            permanent: !matches!(args.availability, Availability::Limited),
            limited: !matches!(args.availability, Availability::Permanent),
        };
        println!("生成角色喜好表…");
        report::write_gifts(&library, &options, &path)?;
        path
    } else {
        let account = if args.demo_account {
            ensure!(
                cfg!(debug_assertions),
                "--demo-account 仅在 Debug 构建中可用"
            );
            AccountSnapshot::demo(&library)
        } else {
            let path = match args.account {
                Some(path) => path,
                None => crate::account::default_snapshot_path(&model::state_dir())
                    .context("请先登录并同步账号，或通过 --account 指定账号记录")?,
            };
            crate::account::load_snapshot(&path)
                .with_context(|| format!("无法读取账号记录：{}", model::display_path(&path)))?
        };
        let included = |kind| args.types.is_empty() || args.types.contains(&kind);
        let options = OwnedOptions {
            characters: included(OwnedType::Characters),
            skins: included(OwnedType::Skins),
            decorations: included(OwnedType::Decorations),
        };
        let path = args.export_owned.context("请选择导出类型")?;
        println!("生成已拥有清单…");
        report::write_owned(&library, &account, &options, &path)?;
        path
    };
    println!("已导出：{}", model::display_path(&destination));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn export_modes_and_scope_arguments_are_validated() {
        assert!(
            Args::try_parse_from([
                "app",
                "--export-gifts",
                "a.html",
                "--export-owned",
                "b.html"
            ])
            .is_err()
        );
        assert!(
            Args::try_parse_from(["app", "--export-owned", "b.html", "--types", "wrong"]).is_err()
        );
        let args = Args::try_parse_from([
            "app",
            "--export-owned",
            "b.html",
            "--types",
            "characters,skins",
            "--demo-account",
        ])
        .unwrap();
        assert_eq!(args.types, vec![OwnedType::Characters, OwnedType::Skins]);
    }
}
