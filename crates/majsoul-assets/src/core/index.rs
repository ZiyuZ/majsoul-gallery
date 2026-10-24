use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Configure the embedding application's private index and cache directory.
pub fn set_data_dir(path: PathBuf) -> Result<()> {
    if let Some(existing) = DATA_DIR.get() {
        anyhow::ensure!(existing == &path, "解析器数据目录已经配置");
        return Ok(());
    }
    DATA_DIR
        .set(path)
        .map_err(|_| anyhow::anyhow!("解析器数据目录已经配置"))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset {
    pub path: String,
    pub file: usize,
    pub id: i64,
    pub class: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bundle {
    pub name: String,
    pub size: u64,
    pub modified: u64,
    pub assets: Vec<Asset>,
    pub error: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Index {
    pub schema: u32,
    pub manifest_digest: String,
    pub source: PathBuf,
    pub unity: String,
    pub bundles: BTreeMap<String, Bundle>,
}
#[derive(Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub input: String,
    #[serde(default)]
    pub output: String,
}

pub fn data_dir() -> PathBuf {
    if let Some(path) = DATA_DIR.get() {
        return path.clone();
    }
    if let Some(path) = std::env::var_os("MAJSOUL_VIEWER_DATA_DIR") {
        return PathBuf::from(path);
    }
    std::env::current_exe()
        .expect("application location")
        .parent()
        .expect("application directory")
        .join("majsoul-assetviewer-data")
}
pub fn cache_dir() -> PathBuf {
    data_dir().join("cache")
}
pub fn settings() -> Settings {
    fs::read(data_dir().join("settings.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
pub fn digest(bytes: impl AsRef<[u8]>) -> String {
    format!("{:x}", Sha256::digest(bytes.as_ref()))
}
pub fn fingerprint(path: &Path) -> Result<(u64, u64)> {
    let m = fs::metadata(path)?;
    Ok((
        m.len(),
        m.modified()?
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos() as u64,
    ))
}
pub fn save_settings(settings: &Settings) -> Result<()> {
    fs::create_dir_all(data_dir())?;
    let mut file = tempfile::NamedTempFile::new_in(data_dir())?;
    serde_json::to_writer(file.as_file_mut(), settings)?;
    file.persist(data_dir().join("settings.json"))?;
    Ok(())
}
pub fn source_dir(input: &str) -> Result<PathBuf> {
    let selected = PathBuf::from(input.trim().trim_matches('"'));
    let p = if selected.is_file() {
        selected.parent().context("程序文件缺少父目录")?.to_owned()
    } else {
        selected
    };
    for candidate in [
        p.join("Jantama_MahjongSoul_Data/StreamingAssets/StandaloneWindows"),
        p.join("StreamingAssets/StandaloneWindows"),
        p.join("StandaloneWindows"),
        p.clone(),
    ] {
        if candidate.join("AssetBundleConfig.json").is_file()
            && candidate.is_dir()
            && fs::read_dir(&candidate)?
                .filter_map(Result::ok)
                .any(|e| e.path().extension().is_some_and(|x| x == "majset"))
        {
            return Ok(candidate.canonicalize()?);
        }
    }
    bail!("未找到 .majset 文件，请选择游戏安装目录或 StandaloneWindows 资源目录")
}

pub struct Library {
    pub index: Index,
    pub index_path: PathBuf,
}
impl Library {
    pub fn open(input: Option<&str>) -> Result<Self> {
        let settings = settings();
        let input = input
            .filter(|s| !s.trim().is_empty())
            .or_else(|| (!settings.input.is_empty()).then_some(settings.input.as_str()));
        let input = input.context("尚未配置资源库，请先选择雀魂安装目录、程序或资源目录")?;
        let source = source_dir(input)?;
        let index_path = data_dir().join("indexes").join(format!(
            "{}.json",
            digest(source.to_string_lossy().as_bytes())
        ));
        let bytes = fs::read(&index_path).context("此目录尚未初始化，请先建立本地资源索引")?;
        let index: Index = serde_json::from_slice(&bytes).context("索引文件无法读取")?;
        if index.schema != 2 || index.source != source {
            bail!("索引版本或资源来源不匹配，请重新建立索引");
        }
        for (name, bundle) in &index.bundles {
            if Path::new(name).components().count() != 1
                || Path::new(name).file_name().is_none()
                || bundle.name != *name
            {
                bail!("索引包含无效的资源包路径");
            }
        }
        Ok(Self { index, index_path })
    }
}

pub fn initialize(input: &str, root: &Path, rebuild: bool) -> Result<PathBuf> {
    initialize_with_progress(input, root, rebuild, &|_, _, _, _| {})
}
pub fn initialize_with_progress(
    input: &str,
    root: &Path,
    rebuild: bool,
    progress: &dyn Fn(&str, usize, usize, &str),
) -> Result<PathBuf> {
    progress("定位资源目录", 0, 0, "");
    let source = source_dir(input)?;
    let path = root.join("indexes").join(format!(
        "{}.json",
        digest(source.to_string_lossy().as_bytes())
    ));
    if !rebuild && path.is_file() {
        let existing: Index =
            serde_json::from_slice(&fs::read(&path)?).context("索引损坏，请勾选重新建立索引")?;
        anyhow::ensure!(
            existing.schema == 2 && existing.source == source,
            "索引版本或来源不匹配，请勾选重新建立索引"
        );
        return Ok(source);
    }
    progress("读取游戏资源清单", 0, 0, "");
    let assets = super::names::asset_manifest(&source)?;
    let data = source
        .parent()
        .and_then(Path::parent)
        .context("找不到游戏 Data 目录")?;
    let studio = unity_rs_core::studio::Studio::open(data.join("resources.assets"))
        .context("无法从 resources.assets 识别 Unity 版本；请选择完整的游戏安装目录")?;
    let unity = studio
        .files()
        .next()
        .context("缺少 Unity 版本")?
        .unity_version()
        .to_owned();
    let mut bundles = BTreeMap::new();
    let count = assets
        .values()
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    for name in assets.values() {
        if bundles.contains_key(name) {
            continue;
        }
        if bundles.len() % 64 == 0 {
            progress("记录资源包指纹", bundles.len(), count, name);
        }
        let bundle_path = super::names::safe_bundle(&source, name)?;
        let (size, modified) =
            fingerprint(&bundle_path).with_context(|| format!("资源包缺失：{name}"))?;
        bundles.insert(
            name.clone(),
            Bundle {
                name: name.clone(),
                size,
                modified,
                assets: vec![],
                error: None,
            },
        );
    }
    for (logical, name) in assets {
        let extension = logical.rsplit('.').next().unwrap_or("").to_owned();
        if !["png", "jpg", "jpeg", "ttf", "otf"].contains(&extension.as_str()) {
            continue;
        }
        bundles.get_mut(&name).unwrap().assets.push(Asset {
            path: logical,
            file: 0,
            id: 0,
            class: if ["ttf", "otf"].contains(&extension.as_str()) {
                128
            } else {
                28
            },
        });
    }
    let index = Index {
        schema: 2,
        manifest_digest: digest(fs::read(source.join("AssetBundleConfig.json"))?),
        source: source.clone(),
        unity,
        bundles,
    };
    progress("保存索引", count, count, "");
    fs::create_dir_all(path.parent().unwrap())?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    serde_json::to_writer(temp.as_file_mut(), &index)?;
    temp.persist(path)?;
    Ok(source)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_install_executable_data_and_resource_paths() -> Result<()> {
        let root = tempfile::tempdir()?;
        let data = root.path().join("Jantama_MahjongSoul_Data");
        let streaming = data.join("StreamingAssets");
        let source = streaming.join("StandaloneWindows");
        fs::create_dir_all(&source)?;
        fs::write(source.join("AssetBundleConfig.json"), b"fixture")?;
        fs::write(source.join("example.majset"), b"fixture")?;
        let exe = root.path().join("Jantama_MahjongSoul.exe");
        fs::write(&exe, b"fixture")?;
        for selected in [root.path(), &exe, &data, &streaming, &source] {
            assert_eq!(
                source_dir(&selected.to_string_lossy())?,
                source.canonicalize()?
            );
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires local installation"]
    fn initializes_metadata_only_and_reuses_index() -> Result<()> {
        let root = tempfile::tempdir()?;
        let input = r"D:\Program Files (x86)\Steam\steamapps\common\MahjongSoul";
        let source = initialize(input, root.path(), false)?;
        let path = root.path().join("indexes").join(format!(
            "{}.json",
            digest(source.to_string_lossy().as_bytes())
        ));
        let stamp = fingerprint(&path)?;
        let index: Index = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(index.schema, 2);
        assert!(index.bundles.len() > 1000);
        assert!(
            index
                .bundles
                .values()
                .flat_map(|b| &b.assets)
                .any(|a| a.path.contains("/character/"))
        );
        initialize(input, root.path(), false)?;
        assert_eq!(stamp, fingerprint(&path)?);
        assert_eq!(fs::read_dir(root.path())?.count(), 1);
        assert_eq!(fs::read_dir(path.parent().unwrap())?.count(), 1);
        Ok(())
    }
}

#[derive(Serialize)]
pub struct UpdateCheck {
    pub changed: bool,
    pub manifest_changed: bool,
    pub modified: usize,
    pub missing: usize,
    pub added: usize,
    pub removed: usize,
}
pub fn check_updates(
    library: &Library,
    progress: &dyn Fn(&str, usize, usize, &str),
) -> Result<UpdateCheck> {
    let index = &library.index;
    let digest_now = digest(fs::read(index.source.join("AssetBundleConfig.json"))?);
    let manifest_changed = index.manifest_digest != digest_now;
    let current = super::names::asset_manifest(&index.source)?
        .into_values()
        .collect::<std::collections::BTreeSet<_>>();
    let mut result = UpdateCheck {
        changed: manifest_changed,
        manifest_changed,
        modified: 0,
        missing: 0,
        added: current
            .iter()
            .filter(|n| !index.bundles.contains_key(*n))
            .count(),
        removed: index
            .bundles
            .keys()
            .filter(|n| !current.contains(*n))
            .count(),
    };
    for (i, (name, bundle)) in index.bundles.iter().enumerate() {
        if i % 64 == 0 {
            progress("检查资源更新", i, index.bundles.len(), name);
        }
        match fingerprint(&super::names::safe_bundle(&index.source, name)?) {
            Ok(stamp) if stamp != (bundle.size, bundle.modified) => result.modified += 1,
            Err(_) => result.missing += 1,
            _ => {}
        }
    }
    result.changed |= result.modified + result.missing + result.added + result.removed > 0;
    progress("更新检查完成", index.bundles.len(), index.bundles.len(), "");
    Ok(result)
}
