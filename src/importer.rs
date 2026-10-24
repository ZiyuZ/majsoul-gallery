use crate::task_process::emit;
use anyhow::{Context, Result, ensure};
use majsoul_assets::core::{self, export, progress::Jobs};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

pub fn start(input: &Path, data: &Path, full: bool) -> Result<crate::task_process::Job> {
    let mut command = Command::new(std::env::current_exe()?);
    command.arg("--import-worker").arg(input).arg(data);
    if full {
        command.arg("--full");
    }
    crate::task_process::Job::spawn(command)
}

fn progress_label(label: &str) -> String {
    match label {
        "按资源包导出图片" => "导入图片".into(),
        "规划图片导出" => "整理图片资源".into(),
        "清理所选分类的旧导出" => "准备新的素材库".into(),
        _ => label
            .strip_prefix("导出")
            .map(|rest| format!("导入{rest}"))
            .unwrap_or_else(|| label.into()),
    }
}

fn source_signature(source: &Path) -> Result<Value> {
    let mut bundles = std::collections::BTreeMap::new();
    for name in core::names::asset_manifest(source)?
        .into_values()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let stamp = core::index::fingerprint(&core::names::safe_bundle(source, &name)?)?;
        bundles.insert(name, json!([stamp.0, stamp.1]));
    }
    Ok(
        json!({"format":1,"spine":cfg!(feature = "spine"),"source":source,"manifest":core::index::digest(fs::read(source.join("AssetBundleConfig.json"))?),"bundles":bundles}),
    )
}
fn read_resume(assets: &Path) -> Option<Value> {
    fs::read(assets.parent()?.join("import/resume.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
pub fn has_updates(input: &Path, active: &Path) -> Result<bool> {
    let Some(baseline) = read_resume(active) else {
        return Ok(false);
    };
    let source = crate::discovery::validate(input)?;
    if baseline["source"] != json!(source) {
        return Ok(false);
    }
    Ok(baseline != source_signature(&source)?)
}
pub fn worker(input: &Path, data: &Path, full: bool) -> Result<()> {
    fs::create_dir_all(data)?;
    crate::storage::clear_import_cache(data)?;
    core::index::set_data_dir(data.join("import"))?;
    let phase = Arc::new(AtomicUsize::new(0));
    let marker_written = Arc::new(AtomicBool::new(false));
    let highest_percent = AtomicUsize::new(0);
    let marker = Arc::new(std::sync::Mutex::new(None::<(PathBuf, Value)>));
    let observer_phase = phase.clone();
    let observer_marker = marker.clone();
    let jobs = Jobs::with_observer(move |snapshot| {
        let phase = observer_phase.load(Ordering::Relaxed);
        let fraction = if snapshot.total == 0 || snapshot.label.contains("检查") {
            0.
        } else {
            (snapshot.done as f32 / snapshot.total as f32).clamp(0., 1.)
        };
        let (start, span) = [(0., 15.), (15., 15.), (30., 65.), (95., 5.)][phase.min(3)];
        let percent = ((start + span * fraction) * 100.) as usize;
        let percent = highest_percent
            .fetch_max(percent, Ordering::Relaxed)
            .max(percent) as f32
            / 100.;
        emit(
            json!({"type":"progress","label":progress_label(&snapshot.label),"current":snapshot.current,"done":snapshot.done,"total":snapshot.total,"percent":percent}),
        );
        // The first resource progress event follows completion of the exporter's cleanup.
        if phase == 2
            && snapshot.total > 0
            && !snapshot.label.contains("检查")
            && !snapshot.label.contains("清理")
            && !marker_written.swap(true, Ordering::Relaxed)
            && let Some((path, value)) = observer_marker.lock().unwrap().as_ref()
        {
            let _ = fs::write(path, value.to_string());
        }
    });
    let task = jobs.begin("export")?;
    let source_path = crate::discovery::validate(input)?;
    let signature = source_signature(&source_path)?;
    let target = data.join("materials");
    let baseline = read_resume(&target).filter(|s| s["source"] == signature["source"]);
    if !full
        && baseline.as_ref() == Some(&signature)
        && let Ok(library) = crate::model::Library::load(&target)
        && library.status == "complete"
    {
        emit(json!({"type":"complete","assets":target,"source":source_path}));
        return Ok(());
    }
    let input = source_path.to_string_lossy().into_owned();
    let source = core::index::initialize_with_progress(
        &input,
        &core::index::data_dir(),
        false,
        &|l, d, t, c| task.update(l, d, t, c),
    )?;
    let library = core::Library::open(Some(&source.to_string_lossy()))?;
    if core::index::check_updates(&library, &|l, d, t, c| task.update(l, d, t, c))?.changed {
        core::index::initialize_with_progress(
            &input,
            &core::index::data_dir(),
            true,
            &|l, d, t, c| task.update(l, d, t, c),
        )?;
    }
    phase.store(1, Ordering::Relaxed);
    let opened = core::session::load(Some(&input), &task)?;
    let names = opened.names.as_ref().context("无法解析本地名称和配置表")?;
    if let Some(warning) = &opened.name_warning {
        crate::diagnostics::record(warning);
    }
    fs::create_dir_all(&target)?;
    let resume = data.join("import/resume.json");
    let reusable = !full && baseline.is_some() && target.join("manifest.json").is_file();
    if reusable
        && baseline.as_ref() != Some(&signature)
        && let Some(baseline) = baseline.as_ref()
    {
        task.update("检查变化的素材", 0, 0, "");
        let stamps = baseline["bundles"]
            .as_object()
            .context("素材版本记录缺少资源包信息")?
            .iter()
            .filter_map(|(name, value)| {
                Some((name.clone(), (value[0].as_u64()?, value[1].as_u64()?)))
            })
            .collect();
        export::invalidate_changed(&opened.library, names, &opened.catalog, &target, &stamps)?;
    }
    *marker.lock().unwrap() = Some((resume.clone(), signature.clone()));
    phase.store(2, Ordering::Relaxed);
    let types = export::TYPES
        .iter()
        .filter(|kind| **kind != "spine" || cfg!(feature = "spine"))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();
    let report = export::run_with_options(
        &opened.library,
        names,
        &opened.catalog,
        &target.to_string_lossy(),
        &types,
        &export::Options {
            overwrite: !reusable,
            link_cached_files: true,
            direct_output: true,
            ..Default::default()
        },
        &task,
    )?;
    ensure!(
        report.errors == 0,
        "{} 项素材未能导入。已完成的内容已保留，请查看日志后重试。",
        report.errors
    );
    phase.store(3, Ordering::Relaxed);
    task.update("验证角色、皮肤和装扮", 0, 1, "");
    let library = crate::model::Library::load(Path::new(&report.directory))?;
    ensure!(
        !library.characters.is_empty() && library.status == "complete",
        "导入的素材包尚未完整生成"
    );
    fs::write(resume, signature.to_string())?;
    crate::storage::clear_import_cache(data)?;
    task.update("导入完成", 1, 1, "");
    emit(json!({"type":"complete","assets":library.root,"source":source}));
    Ok(())
}

pub fn run_worker(input: &Path, data: &Path, full: bool) {
    if let Err(error) = worker(input, data, full) {
        if let Err(cleanup) = crate::storage::clear_import_cache(data) {
            crate::diagnostics::record(format!("import scratch cleanup failed: {cleanup:#}"));
        }
        crate::diagnostics::record(format!("local import failed: {error:#}"));
        emit(json!({"type":"error","message":format!("{error:#}")}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires a locally imported package"]
    fn local_startup_check_uses_the_saved_baseline() -> Result<()> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local/target/debug/majsoul-gallery-data/materials");
        let baseline = read_resume(&root).context("missing local baseline")?;
        let source = Path::new(baseline["source"].as_str().context("missing source")?);
        assert!(!has_updates(source, &root)?);
        Ok(())
    }
}
