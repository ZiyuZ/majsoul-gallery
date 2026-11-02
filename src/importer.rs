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
        json!({"format":1,"inventory":1,"illustrations":2,"character_layout":1,"spine":cfg!(feature = "spine"),"source":source,"manifest":core::index::digest(fs::read(source.join("AssetBundleConfig.json"))?),"bundles":bundles}),
    )
}
fn read_resume(assets: &Path) -> Option<Value> {
    fs::read(assets.parent()?.join("import/resume.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
}
fn diagnostic_complete(assets: &Path) -> bool {
    assets
        .parent()
        .and_then(|data| fs::read(data.join("import/diagnostics.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .is_some_and(|report| report["status"] == "complete")
}
fn character_diagnostics(names: &core::names::Names, catalog: &core::Catalog) -> Value {
    let configured = names
        .rows("item_definition.character")
        .iter()
        .filter(|row| matches!(row["sex"].as_i64(), Some(1 | 2)))
        .filter_map(|row| {
            Some((
                row["id"].as_i64()?,
                row["name_chs"].as_str().unwrap_or_default(),
            ))
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    let linked = catalog
        .groups
        .iter()
        .filter(|group| group.category == "character" && !group.story)
        .filter_map(|group| group.character.as_ref())
        .filter(|character| matches!(character.sex, Some(1 | 2)))
        .map(|character| character.id)
        .collect::<std::collections::BTreeSet<_>>();
    let unmatched = configured
        .iter()
        .filter(|(id, _)| !linked.contains(id))
        .map(|(id, name)| json!({"id":id,"name":name}))
        .collect::<Vec<_>>();
    json!({"configured_characters":configured.len(),"configured_skins":names.rows("item_definition.skin").len(),"linked_characters":linked.len(),"unmatched_characters":unmatched})
}
pub fn has_updates(input: &Path, active: &Path) -> Result<bool> {
    let Some(baseline) = read_resume(active) else {
        return Ok(false);
    };
    let source = crate::discovery::validate(input)?;
    if baseline["source"] != json!(source) {
        return Ok(false);
    }
    Ok(!diagnostic_complete(active) || baseline != source_signature(&source)?)
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
        && diagnostic_complete(&target)
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
    let diagnostic_path = data.join("import/diagnostics.json");
    let mut diagnostic = character_diagnostics(names, &opened.catalog);
    diagnostic["application_version"] = json!(env!("CARGO_PKG_VERSION"));
    diagnostic["source"] = json!(source_path);
    diagnostic["bundles"] = json!(opened.library.index.bundles.len());
    diagnostic["status"] = json!("exporting");
    crate::files::write_json(&diagnostic_path, &diagnostic)?;
    crate::diagnostics::record(format!(
        "import: configured={} linked={} unmatched={}",
        diagnostic["configured_characters"],
        diagnostic["linked_characters"],
        diagnostic["unmatched_characters"]
            .as_array()
            .map_or(0, Vec::len)
    ));
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
            unlocked_gifts_only: false,
            ..Default::default()
        },
        &task,
    )?;
    diagnostic["export_errors"] = json!(report.errors);
    if report.errors != 0 {
        diagnostic["status"] = json!("failed");
        crate::files::write_json(&diagnostic_path, &diagnostic)?;
    }
    ensure!(
        report.errors == 0,
        "{} 项素材未能导入。已完成的内容已保留，请查看日志后重试。",
        report.errors
    );
    phase.store(3, Ordering::Relaxed);
    task.update("验证角色、皮肤和装扮", 0, 1, "");
    let library = crate::model::Library::load(Path::new(&report.directory))?;
    diagnostic["exported_characters"] = json!(library.characters.len());
    let exported = library
        .characters
        .iter()
        .map(|character| character.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let unexported = names
        .rows("item_definition.character")
        .iter()
        .filter(|row| matches!(row["sex"].as_i64(), Some(1 | 2)))
        .filter_map(|row| {
            let id = row["id"].as_i64()?;
            (!exported.contains(&id.to_string())).then(|| json!({"id":id,"name":row["name_chs"]}))
        })
        .collect::<Vec<_>>();
    diagnostic["unexported_characters"] = json!(unexported);
    let complete = !library.characters.is_empty()
        && library.status == "complete"
        && diagnostic["configured_characters"].as_u64() == Some(library.characters.len() as u64)
        && diagnostic["unmatched_characters"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && unexported.is_empty();
    diagnostic["status"] = json!(if complete { "complete" } else { "partial" });
    crate::files::write_json(&diagnostic_path, &diagnostic)?;
    ensure!(
        complete,
        "角色导入不完整：配置 {} 个，关联 {} 个，导入 {} 个。详细名单见 import/diagnostics.json。已导入素材已保留。",
        diagnostic["configured_characters"],
        diagnostic["linked_characters"],
        library.characters.len()
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
    fn incomplete_diagnostics_do_not_allow_reusing_a_completed_export() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let assets = temp.path().join("materials");
        fs::create_dir_all(temp.path().join("import"))?;
        let report = temp.path().join("import/diagnostics.json");
        assert!(!diagnostic_complete(&assets));
        crate::files::write_json(&report, &json!({"status":"partial"}))?;
        assert!(!diagnostic_complete(&assets));
        crate::files::write_json(&report, &json!({"status":"complete"}))?;
        assert!(diagnostic_complete(&assets));
        Ok(())
    }

    #[test]
    fn diagnostics_list_configured_players_missing_from_the_catalog() -> Result<()> {
        let names = serde_json::from_value(
            json!({"signature":"fixture","tables":{"item_definition.character":[
            {"id":200001,"name_chs":"一姬","sex":1},{"id":0,"name_chs":"默认","sex":0}
        ]},"icon_assets":{},"audio_assets":{},"spine_assets":{}}),
        )?;
        let catalog = core::Catalog {
            categories: vec![],
            groups: vec![],
            resources: Default::default(),
        };
        let report = character_diagnostics(&names, &catalog);
        assert_eq!(report["configured_characters"], 1);
        assert_eq!(report["linked_characters"], 0);
        assert_eq!(
            report["unmatched_characters"],
            json!([{"id":200001,"name":"一姬"}])
        );
        Ok(())
    }
    #[test]
    #[ignore = "updates the locally imported package using the installed game"]
    fn local_illustration_import_matches_configured_images() -> Result<()> {
        let data =
            Path::new(env!("CARGO_MANIFEST_DIR")).join(".local/target/debug/majsoul-gallery-data");
        let assets = data.join("materials");
        let baseline = read_resume(&assets).context("missing local baseline")?;
        let source = Path::new(baseline["source"].as_str().context("missing source")?);
        worker(source, &data, false)?;
        let library = crate::model::Library::load(&assets)?;
        ensure!(!library.illustrations.is_empty(), "missing illustrations");
        ensure!(
            library
                .illustrations
                .iter()
                .all(|item| item.image.is_some() && item.thumbnail.is_some()),
            "missing configured images"
        );
        let item = library
            .illustrations
            .iter()
            .find(|item| item.name == "守梦夜明-2026")
            .context("missing regression illustration")?;
        assert_eq!(
            image::image_dimensions(item.thumbnail.as_ref().unwrap())?,
            (600, 338)
        );
        assert_eq!(
            image::image_dimensions(item.image.as_ref().unwrap())?,
            (1920, 1080)
        );
        assert_eq!(item.year(), Some(2026));
        Ok(())
    }
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
