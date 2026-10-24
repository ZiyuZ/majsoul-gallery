//! A portable, versioned gallery package. No Unity decoding is required by its consumer.
use super::items::locked_icon as locked;
use super::{Catalog, Library, Resource, names::Names, progress::Task};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};
pub const TYPES: &[&str] = &[
    "character",
    "spine",
    "item",
    "gift",
    "decoration",
    "illustration",
    "title",
    "rank",
];
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub players_only: bool,
    pub exclude_expired: bool,
    pub unlocked_gifts_only: bool,
    pub overwrite: bool,
    /// Share decoded scratch files with an application-owned library until scratch cleanup.
    #[serde(skip)]
    pub link_cached_files: bool,
    /// Write directly into an application-managed library directory.
    #[serde(skip)]
    pub direct_output: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            players_only: true,
            exclude_expired: true,
            unlocked_gifts_only: true,
            overwrite: false,
            link_cached_files: false,
            direct_output: false,
        }
    }
}
fn expired(row: &Value, now: i64) -> bool {
    if row["name_chs"]
        .as_str()
        .is_some_and(|s| s.contains("已过期"))
    {
        return true;
    }
    let Some(text) = row["item_expire"].as_str().filter(|s| !s.trim().is_empty()) else {
        return false;
    };
    let format = time::macros::format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    time::PrimitiveDateTime::parse(text, format)
        .ok()
        .map(|date| {
            date.assume_offset(time::UtcOffset::from_hms(8, 0, 0).expect("UTC+8"))
                .unix_timestamp()
                <= now
        })
        .unwrap_or(false)
}
fn player(group: &super::Group) -> bool {
    !group.story
        && group
            .character
            .as_ref()
            .is_some_and(|c| matches!(c.sex, Some(1 | 2)))
}
fn raw_export_resources<'a>(group: &'a super::Group, options: &Options) -> Vec<&'a Resource> {
    let decoration = super::catalog::matches_category("decoration", &group.category);
    let preferred = if group.resources.iter().any(|r| r.path.contains("/pic/chs/")) {
        "/pic/chs/"
    } else {
        "/pic/common/"
    };
    group
        .resources
        .iter()
        .filter(|r| {
            let path = &r.path;
            // The gallery skin contract consists of the full illustration and two avatars.
            // Client-only half/waitingroom/reverse references are not extra gallery images.
            if group.category == "character"
                && !["/full/", "/bighead/", "/smallhead/"]
                    .iter()
                    .any(|part| path.contains(part))
            {
                return false;
            }
            if path.contains("/3d/") || path.contains("/texture/") || path.contains("/face/") {
                return false;
            }
            if ["chs_t", "jp", "en", "kr"]
                .iter()
                .any(|l| path.contains(&format!("/{l}/")))
            {
                return false;
            }
            if options.unlocked_gifts_only
                && group.category == "gift"
                && r.item.as_ref().is_some_and(|i| locked(&i.icon))
            {
                return false;
            }
            if decoration {
                if group.category == "frame" {
                    return !path.contains("/pic/") || path.contains(preferred);
                }
                return path.contains(preferred);
            }
            true
        })
        .collect()
}
fn export_resources<'a>(group: &'a super::Group, options: &Options) -> Vec<&'a Resource> {
    let resources = raw_export_resources(group, options);
    let chs = resources
        .iter()
        .filter(|r| locale(&r.path) == "chs")
        .map(|r| kind(r))
        .collect::<std::collections::BTreeSet<_>>();
    resources
        .iter()
        .copied()
        .filter(|r| locale(&r.path) != "common" || !chs.contains(kind(r)))
        .collect()
}
#[derive(Serialize)]
pub struct Report {
    pub directory: String,
    pub assets: usize,
    pub scenes: usize,
    pub errors: usize,
    pub cancelled: bool,
    pub reused: usize,
    pub elapsed_ms: u128,
    pub bundle_cache_hits: u64,
    pub bundle_loads: u64,
}
fn logical_image_path(path: &str) -> String {
    let path = path.trim_start_matches('/').to_lowercase();
    let path = path.strip_prefix("myassets/").unwrap_or(&path);
    let Some((directory, file)) = path.rsplit_once('/') else {
        return path.into();
    };
    if let Some((parent, locale)) = directory.rsplit_once('/')
        && ["common", "chs", "chs_t", "jp", "en", "kr"].contains(&locale)
    {
        format!("{parent}/{file}")
    } else {
        path.into()
    }
}

fn illustration_images(row: &Value, files: &[String], assets: &BTreeMap<String, Value>) -> Value {
    let mut images = serde_json::Map::new();
    for (role, field) in [("full", "img_path"), ("thumbnail", "thumb_path")] {
        let Some(configured) = row[field].as_str().filter(|path| !path.is_empty()) else {
            continue;
        };
        let configured = logical_image_path(configured);
        if let Some(file) = files.iter().find(|file| {
            assets
                .get(*file)
                .and_then(|asset| asset["source_path"].as_str())
                .is_some_and(|path| logical_image_path(path) == configured)
        }) {
            images.insert(role.into(), json!(file));
        }
    }
    Value::Object(images)
}

fn safe_name(text: &str) -> String {
    let value = text
        .chars()
        .map(|c| {
            if c.is_control() || "<>:\"/\\|?*".contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let value = value.trim_matches([' ', '.']).to_owned();
    if value.is_empty() {
        "unknown".into()
    } else {
        value
    }
}
fn key(group: &super::Group) -> String {
    group
        .bindings
        .iter()
        .find(|b| {
            matches!(
                b.table.as_str(),
                "item_definition.skin"
                    | "spot.skin_spot"
                    | "item_definition.item"
                    | "level_definition.level_definition"
            )
        })
        .or_else(|| group.bindings.first())
        .map(|b| b.id.to_string())
        .unwrap_or_else(|| safe_name(group.path.rsplit('/').next().unwrap_or("unknown")))
}
fn owner(group: &super::Group) -> String {
    group
        .character
        .as_ref()
        .map(|c| c.id.to_string())
        .unwrap_or_else(|| "unassigned".into())
}
fn skin_base(group: &super::Group) -> String {
    format!("characters/{}/skins/{}", owner(group), key(group))
}
fn section(category: &str) -> &str {
    match category {
        "character" => "characters",
        "item" => "items",
        "gift" => "gifts",
        "loading" => "illustrations",
        "title" => "titles",
        "rank" => "ranks",
        _ => "decorations",
    }
}
fn asset_folder(category: &str) -> &str {
    match category {
        "hand" => "decorations/hands",
        "frame" => "decorations/avatar-frames",
        "stick" => "decorations/riichi-sticks",
        "table" => "decorations/tables",
        "tile-back" => "decorations/tile-backs",
        "tile-face" => "decorations/tile-faces",
        "win-effect" => "decorations/win-effects",
        "riichi-effect" => "decorations/riichi-effects",
        "call-effect" => "decorations/call-indicators",
        "background" | "background-icon" => "decorations/lobby-backgrounds",
        "bgm" | "riichi-bgm" | "music" => "decorations/music",
        other => section(other),
    }
}
fn numbered(stem: &str, n: usize) -> String {
    if n == 1 {
        format!("{stem}.png")
    } else {
        format!("{stem}-{n}.png")
    }
}
fn kind(resource: &Resource) -> &str {
    if resource.path.contains("/full/") {
        "full"
    } else if resource.path.contains("/bighead/") {
        "portrait"
    } else if resource.path.contains("/smallhead/") {
        "avatar"
    } else if resource.path.contains("/pic/")
        || resource.path.contains("/icon/")
        || resource.item.is_some()
    {
        "icon"
    } else {
        "image"
    }
}
fn locale(path: &str) -> &str {
    ["chs_t", "chs", "common", "jp", "en", "kr"]
        .into_iter()
        .find(|l| path.contains(&format!("/{l}/")))
        .unwrap_or("common")
}
pub fn decode(
    library: &Library,
    names: &Names,
    resource: &Resource,
) -> Result<super::preview::Preview> {
    if resource.class == "SpineTexture" {
        return super::spine::preview_texture(library, names, &resource.path);
    }
    if let Some(item) = &resource.item {
        let icon = super::item_icons::decode_items(library, names, std::slice::from_ref(item))
            .pop()
            .context("没有图标结果")?;
        let path = icon
            .path
            .with_context(|| icon.error.unwrap_or_else(|| "图标缺失".into()))?;
        let (width, height) = image::image_dimensions(&path)?;
        return Ok(super::preview::Preview {
            media: "image".into(),
            path,
            width,
            height,
            kind: "Sprite".into(),
            cached: icon.cached,
        });
    }
    super::preview::decode(library, resource)
}
fn copy(root: &Path, from: &str, relative: &str, overwrite: bool, link: bool) -> Result<()> {
    let target = root.join(relative);
    if !overwrite && target.is_file() {
        return Ok(());
    }
    let parent = target.parent().context("导出目录无效")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    if link {
        fs::remove_file(&temporary)?;
    }
    if !link || fs::hard_link(from, &temporary).is_err() {
        fs::copy(from, &temporary)?;
    }
    temporary.persist(target)?;
    Ok(())
}
fn read_json(path: &Path) -> Result<Value> {
    if !path.is_file() {
        return Ok(json!({}));
    }
    serde_json::from_slice(&fs::read(path)?).with_context(|| format!("无法读取 {}", path.display()))
}
fn write_json(root: &Path, relative: &str, value: &Value) -> Result<()> {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().context("导出目录无效")?)?;
    // Replace metadata atomically within the package directory.
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    serde_json::to_writer_pretty(temp.as_file_mut(), value)?;
    temp.persist(path)?;
    Ok(())
}
fn reusable_image(root: &Path, file: &str, overwrite: bool) -> Option<(u32, u32)> {
    if overwrite {
        None
    } else {
        image::image_dimensions(root.join(file)).ok()
    }
}
fn scene_ready(root: &Path, scene: &Value) -> bool {
    let exists = |v: &Value| {
        v.as_str().is_some_and(|p| {
            let p = Path::new(p);
            !p.is_absolute()
                && p.components()
                    .all(|c| matches!(c, std::path::Component::Normal(_)))
                && root.join(p).is_file()
        })
    };
    let Some(layers) = scene["layers"].as_array().filter(|a| !a.is_empty()) else {
        return false;
    };
    layers.iter().all(|l| {
        exists(&l["skeleton"])
            && exists(&l["atlas"])
            && l["pages"]
                .as_object()
                .is_some_and(|p| p.values().all(exists))
    }) && scene["backgrounds"]
        .as_array()
        .is_none_or(|b| b.iter().all(|b| exists(&b["path"])))
}
fn export_scene(
    lib: &Library,
    names: &Names,
    catalog: &Catalog,
    resource: &Resource,
    root: &Path,
    base: &str,
    options: &Options,
) -> Result<Value> {
    let path = root.join(format!("{base}/scene.json"));
    if !options.overwrite && path.is_file() {
        let old = read_json(&path)?;
        if scene_ready(root, &old) {
            return Ok(old);
        }
    }
    let scene = super::spine::prepare_scene(lib, names, catalog, resource, true)?;
    let mut value = serde_json::to_value(scene)?;
    for (i, layer) in value["layers"]
        .as_array_mut()
        .context("骨架图层缺失")?
        .iter_mut()
        .enumerate()
    {
        let dir = format!("{base}/layers/{}", i + 1);
        for (field, extension, stem) in [
            (
                "skeleton",
                layer["format"].as_str().unwrap_or("skel").to_owned(),
                "skeleton",
            ),
            ("atlas", "atlas".into(), "skeleton"),
        ] {
            let file = format!("{dir}/{stem}.{extension}");
            copy(
                root,
                layer[field].as_str().context("骨架路径缺失")?,
                &file,
                options.overwrite,
                options.link_cached_files,
            )?;
            layer[field] = json!(file);
        }
        for (name, path) in layer["pages"].as_object_mut().context("骨架图页缺失")? {
            ensure!(
                !name.contains(['/', '\\', ':']) && name != "..",
                "非法图页名"
            );
            let file = format!("{dir}/{name}");
            copy(
                root,
                path.as_str().context("纹理路径缺失")?,
                &file,
                options.overwrite,
                options.link_cached_files,
            )?;
            *path = json!(file);
        }
    }
    if let Some(backgrounds) = value["backgrounds"].as_array_mut() {
        for (i, bg) in backgrounds.iter_mut().enumerate() {
            let file = format!("{base}/backgrounds/{}.png", i + 1);
            copy(
                root,
                bg["path"].as_str().context("背景路径缺失")?,
                &file,
                options.overwrite,
                options.link_cached_files,
            )?;
            bg["path"] = json!(file);
        }
    }
    Ok(value)
}
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Table {
    definitions: BTreeMap<String, super::items::ItemDefinition>,
    entries: BTreeMap<String, Value>,
    assets: BTreeMap<String, Value>,
    item_assets: BTreeMap<String, String>,
    music: BTreeMap<String, Value>,
}
fn image_asset(
    lib: &Library,
    names: &Names,
    resource: &Resource,
    root: &Path,
    file: &str,
    options: &Options,
) -> Result<(Value, bool)> {
    let existing = reusable_image(root, file, options.overwrite);
    let (width, height) = if let Some(size) = existing {
        size
    } else {
        let preview = decode(lib, names, resource)?;
        ensure!(preview.media == "image", "此分类包含非图片资源");
        copy(root, &preview.path, file, true, options.link_cached_files)?;
        (preview.width, preview.height)
    };
    Ok((
        json!({"file":file,"width":width,"height":height,"role":resource.role,"kind":kind(resource),"locale":locale(&resource.path),"source_path":resource.path}),
        existing.is_some(),
    ))
}
fn image_jobs<'a>(
    group: &'a super::Group,
    options: &Options,
) -> Vec<(&'a Resource, String, String)> {
    let id = key(group);
    let is_character = group.category == "character";
    let base = skin_base(group);
    let mut role_counts = BTreeMap::<String, usize>::new();
    let mut resources = export_resources(group, options);
    resources.sort_by(|a, b| a.path.cmp(&b.path));
    // Assign filenames before scheduling; parallel execution cannot change numbering.
    let mut jobs = vec![];
    for resource in resources {
        let role = kind(resource).to_owned();
        let ordinal = role_counts
            .entry(if is_character {
                role.clone()
            } else {
                "image".into()
            })
            .or_default();
        *ordinal += 1;
        let file = if is_character {
            format!("{base}/{}", numbered(&role, *ordinal))
        } else {
            format!(
                "{}/{}",
                asset_folder(&group.category),
                numbered(&id, *ordinal)
            )
        };
        jobs.push((resource, role, file));
    }
    jobs
}
fn business_groups<'a>(groups: impl Iterator<Item = &'a super::Group>) -> Vec<&'a super::Group> {
    let mut result = BTreeMap::<String, &super::Group>::new();
    for group in groups {
        let id = format!("{}:{}", group.category, key(group));
        let replace = result.get(&id).is_none_or(|old| {
            let official = group.path.rsplit('/').next() == Some(key(group).as_str());
            let old_official = old.path.rsplit('/').next() == Some(key(old).as_str());
            (official && !old_official) || (official == old_official && group.path < old.path)
        });
        if replace {
            result.insert(id, group);
        }
    }
    result.into_values().collect()
}
pub fn run(
    lib: &Library,
    names: &Names,
    catalog: &Catalog,
    output: &str,
    types: &[String],
    task: &Task,
) -> Result<Report> {
    run_with_options(
        lib,
        names,
        catalog,
        output,
        types,
        &Options::default(),
        task,
    )
}
// All deletion targets are resolved beneath this application's export root.
fn reject_links(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    ensure!(
        !metadata.file_type().is_symlink(),
        "不能清理符号链接：{}",
        path.display()
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "不能清理链接或联接点：{}",
            path.display()
        );
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            reject_links(&entry?.path())?;
        }
    }
    Ok(())
}
fn remove_export_path(root: &Path, target: &Path) -> Result<()> {
    if !target.try_exists()? {
        return Ok(());
    }
    let root = root.canonicalize()?;
    let resolved = target.canonicalize()?;
    ensure!(
        resolved != root && resolved.starts_with(&root),
        "清理路径越出素材包：{}",
        target.display()
    );
    reject_links(target)?;
    if fs::metadata(target)?.is_dir() {
        fs::remove_dir_all(target)?;
    } else {
        fs::remove_file(target)?;
    }
    Ok(())
}
fn root_relative(root: &Path, path: &Path) -> Result<String> {
    Ok(path
        .strip_prefix(root)
        .context("路径不在素材包内")?
        .to_string_lossy()
        .replace('\\', "/"))
}
fn clean_selected(root: &Path, types: &[String], manifest: &mut Value) -> Result<()> {
    let selected = |name: &str| types.iter().any(|t| t == name);
    // Check every target first, before the first removal.
    let folders = types
        .iter()
        .filter_map(|t| match t.as_str() {
            "item" => Some("items"),
            "gift" => Some("gifts"),
            "decoration" => Some("decorations"),
            "illustration" => Some("illustrations"),
            "title" => Some("titles"),
            "rank" => Some("ranks"),
            _ => None,
        })
        .collect::<Vec<_>>();
    let absolute_root = root.canonicalize()?;
    for folder in folders
        .iter()
        .copied()
        .chain(if selected("character") || selected("spine") {
            Some("characters")
        } else {
            None
        })
    {
        let path = root.join(folder);
        if path.try_exists()? {
            let resolved = path.canonicalize()?;
            ensure!(
                resolved != absolute_root && resolved.starts_with(&absolute_root),
                "清理目录不在素材包内"
            );
            reject_links(&path)?;
        }
    }
    for folder in folders {
        remove_export_path(root, &root.join(folder))?;
        if let Some(categories) = manifest["categories"].as_object_mut() {
            categories.remove(folder);
        }
    }
    let characters = root.join("characters");
    if selected("character") && selected("spine") {
        remove_export_path(root, &characters)?;
        manifest["characters"] = json!({});
        return Ok(());
    }
    if (!selected("character") && !selected("spine")) || !characters.is_dir() {
        return Ok(());
    }
    let mut character_paths = serde_json::Map::new();
    for entry in fs::read_dir(&characters)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let dir = entry.path();
        let skins = dir.join("skins");
        let mut retained = serde_json::Map::new();
        if skins.is_dir() {
            for entry in fs::read_dir(&skins)? {
                let entry = entry?;
                if !entry.file_type()?.is_dir() {
                    continue;
                }
                let skin_dir = entry.path();
                let info = skin_dir.join("skin.json");
                let mut skin = read_json(&info)?;
                if selected("character") {
                    // Static full export removes image.png and every other old static file,
                    // while preserving the unselected animation subtree.
                    if !skin_dir.join("spine").is_dir() {
                        remove_export_path(root, &skin_dir)?;
                        continue;
                    }
                    for child in fs::read_dir(&skin_dir)? {
                        let child = child?;
                        if child.file_name() != "spine" {
                            remove_export_path(root, &child.path())?;
                        }
                    }
                    skin["images"] = json!({});
                    skin["spine"] = json!(root_relative(root, &skin_dir.join("spine/scene.json"))?);
                } else {
                    remove_export_path(root, &skin_dir.join("spine"))?;
                    if let Some(object) = skin.as_object_mut() {
                        object.remove("spine");
                    }
                    let has_images = skin["images"].as_object().is_some_and(|images| {
                        images
                            .values()
                            .any(|v| v.as_array().is_some_and(|a| !a.is_empty()))
                    });
                    if !has_images {
                        remove_export_path(root, &skin_dir)?;
                        continue;
                    }
                }
                let skin_id = skin["id"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| entry.file_name().to_string_lossy().into_owned());
                write_json(root, &root_relative(root, &info)?, &skin)?;
                retained.insert(skin_id, json!(root_relative(root, &info)?));
            }
        }
        if retained.is_empty() {
            remove_export_path(root, &dir)?;
            continue;
        }
        let info = dir.join("character.json");
        let mut character = read_json(&info)?;
        character["skins"] = json!(retained);
        write_json(root, &root_relative(root, &info)?, &character)?;
        character_paths.insert(
            entry.file_name().to_string_lossy().into_owned(),
            json!(root_relative(root, &info)?),
        );
    }
    manifest["characters"] = json!(character_paths);
    Ok(())
}
fn export_root(lib: &Library, output: &str, direct: bool) -> Result<std::path::PathBuf> {
    let destination = Path::new(output)
        .canonicalize()
        .context("请选择已经存在的输出文件夹")?;
    let game = lib
        .index
        .source
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .context("游戏目录无效")?;
    ensure!(!destination.starts_with(game), "不能向游戏安装目录导出");
    let root = if direct
        || destination
            .file_name()
            .is_some_and(|n| n == "majsoul-assets")
    {
        destination
    } else {
        destination.join("majsoul-assets")
    };
    if !root.exists() {
        return Ok(root);
    }
    let root_metadata = fs::symlink_metadata(&root)?;
    ensure!(
        !root_metadata.file_type().is_symlink(),
        "素材包根目录不能是链接"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            root_metadata.file_attributes() & 0x400 == 0,
            "素材包根目录不能是链接或联接点"
        );
    }
    ensure!(
        !root.canonicalize()?.starts_with(game),
        "不能向游戏安装目录导出"
    );
    Ok(root)
}
struct Selection<'a> {
    items: BTreeMap<i64, &'a Value>,
    selected: Vec<&'a super::Group>,
    spine_groups: Vec<&'a super::Group>,
    characters: BTreeMap<String, super::characters::Character>,
    aliases: BTreeMap<String, String>,
    referenced: BTreeMap<i64, super::characters::NamedItem>,
}
fn select<'a>(
    names: &'a Names,
    catalog: &'a Catalog,
    types: &[String],
    options: &Options,
    manifest: &Value,
) -> Result<Selection<'a>> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    let items = names
        .rows("item_definition.item")
        .iter()
        .filter_map(|r| Some((r["id"].as_i64()?, r)))
        .collect::<BTreeMap<_, _>>();
    let selected = business_groups(catalog.groups.iter().filter(|g| {
        g.category != "spine"
            && (g.category != "character" || !options.players_only || player(g))
            && (g.category != "item"
                || !options.exclude_expired
                || !g
                    .bindings
                    .iter()
                    .filter(|b| b.table == "item_definition.item")
                    .any(|b| items.get(&b.id).is_some_and(|r| expired(r, now))))
            && !export_resources(g, options).is_empty()
            && types
                .iter()
                .any(|t| t != "spine" && super::catalog::matches_category(t, &g.category))
    }));
    let spine_groups = business_groups(catalog.groups.iter().filter(|g| {
        g.category == "spine"
            && (!options.players_only || player(g))
            && types.iter().any(|t| t == "spine")
    }));
    let mut characters = selected
        .iter()
        .chain(spine_groups.iter())
        .filter_map(|g| g.character.as_ref())
        .map(|c| (c.id.to_string(), c.clone()))
        .collect::<BTreeMap<_, _>>();
    // Preserve icons used by unselected character/static or dynamic exports.
    if let Some(retained) = manifest["characters"].as_object() {
        for (id, profile) in super::characters::extract(names)? {
            if retained.contains_key(&id.to_string()) {
                characters.entry(id.to_string()).or_insert(profile);
            }
        }
    }
    let mut aliases = BTreeMap::new();
    let referenced = characters
        .values()
        .flat_map(|c| {
            c.gifts
                .iter()
                .chain(c.contract_materials.iter().flat_map(|m| m.items.iter()))
        })
        .filter_map(|item| {
            if options.unlocked_gifts_only && locked(&item.icon) {
                let replacement = items.values().find(|r| {
                    r["name_chs"].as_str() == Some(item.name.as_str())
                        && r["icon"]
                            .as_str()
                            .is_some_and(|s| !s.is_empty() && !locked(s))
                })?;
                let id = replacement["id"].as_i64()?;
                aliases.insert(item.id.to_string(), id.to_string());
                Some((
                    id,
                    super::characters::NamedItem {
                        id,
                        name: item.name.clone(),
                        icon: replacement["icon"].as_str()?.into(),
                    },
                ))
            } else {
                Some((item.id, item.clone()))
            }
        })
        .collect::<BTreeMap<_, _>>();
    Ok(Selection {
        items,
        selected,
        spine_groups,
        characters,
        aliases,
        referenced,
    })
}
#[derive(Default, Serialize)]
pub struct PlannedCategory {
    pub entries: usize,
    pub images: usize,
    pub reused_images: usize,
}
#[derive(Serialize)]
pub struct DryRun {
    pub directory: String,
    pub categories: BTreeMap<String, PlannedCategory>,
    pub images: usize,
    pub reused_images: usize,
    pub scenes: usize,
    pub reused_scenes: usize,
    pub dependency_icons: usize,
    pub reused_dependency_icons: usize,
    pub texture_bundles: usize,
    pub errors: Vec<String>,
}
/// Plan output without creating/cleaning the destination or decoding images.
/// Loading the library and resolving atlas maps may update application caches.
pub fn dry_run(
    lib: &Library,
    names: &Names,
    catalog: &Catalog,
    output: &str,
    types: &[String],
    options: &Options,
    task: &Task,
) -> Result<DryRun> {
    ensure!(
        !types.is_empty() && types.iter().all(|t| TYPES.contains(&t.as_str())),
        "请选择有效的导出分类"
    );
    let root = export_root(lib, output, options.direct_output)?;
    let mut manifest = read_json(&root.join("manifest.json"))?;
    ensure!(
        manifest["schema_version"].is_null() || manifest["schema_version"] == 2,
        "此目录的导出格式不兼容，请选择新位置"
    );
    task.update("检查资源更新", 0, 0, "");
    ensure!(
        !super::index::check_updates(lib, &|l, d, t, c| task.update(l, d, t, c))?.changed,
        "资源已更新，请先 import --rebuild"
    );
    if options.overwrite
        && types.iter().any(|t| t == "character")
        && types.iter().any(|t| t == "spine")
    {
        manifest["characters"] = json!({});
    }
    let selection = select(names, catalog, types, options, &manifest)?;
    let mut plan = DryRun {
        directory: root.to_string_lossy().into(),
        categories: BTreeMap::new(),
        images: 0,
        reused_images: 0,
        scenes: selection.spine_groups.len(),
        reused_scenes: 0,
        dependency_icons: 0,
        reused_dependency_icons: 0,
        texture_bundles: 0,
        errors: vec![],
    };
    let mut bundles = std::collections::BTreeSet::new();
    let mut exported_items = std::collections::BTreeSet::new();
    for (i, group) in selection.selected.iter().enumerate() {
        ensure!(!task.cancelled(), "已取消导出计划");
        task.update(
            "规划导出（不写出素材）",
            i,
            selection.selected.len(),
            &group.label,
        );
        let category = plan
            .categories
            .entry(section(&group.category).into())
            .or_default();
        category.entries += 1;
        for (resource, _, file) in image_jobs(group, options) {
            category.images += 1;
            plan.images += 1;
            if let Some(item) = &resource.item {
                exported_items.insert(item.id);
            }
            if reusable_image(&root, &file, options.overwrite).is_some() {
                category.reused_images += 1;
                plan.reused_images += 1;
            } else if let Some(item) = &resource.item {
                match super::item_icons::icon_bundles(lib, names, &item.icon) {
                    Ok((_, page)) => {
                        bundles.insert(page);
                    }
                    Err(e) => plan.errors.push(format!("{}: {e:#}", resource.path)),
                }
            } else {
                bundles.insert(resource.bundle.clone());
            }
        }
    }
    for group in &selection.spine_groups {
        let scene = root.join(format!("{}/spine/scene.json", skin_base(group)));
        if !options.overwrite && scene.is_file() && scene_ready(&root, &read_json(&scene)?) {
            plan.reused_scenes += 1;
        }
    }
    for (id, item) in &selection.referenced {
        if exported_items.contains(id) {
            continue;
        }
        let folder = if selection.items.get(id).is_some_and(|r| r["category"] == 2) {
            "gifts"
        } else {
            "items"
        };
        plan.dependency_icons += 1;
        if reusable_image(&root, &format!("{folder}/{id}.png"), options.overwrite).is_some() {
            plan.reused_dependency_icons += 1;
        } else {
            match super::item_icons::icon_bundles(lib, names, &item.icon) {
                Ok((_, page)) => {
                    bundles.insert(page);
                }
                Err(e) => plan.errors.push(format!("物品 {id}: {e:#}")),
            }
        }
    }
    plan.texture_bundles = bundles.len();
    ensure!(
        plan.images + plan.scenes + plan.dependency_icons > 0 || options.overwrite,
        "所选分类没有可导出的资源"
    );
    Ok(plan)
}
/// Invalidate changed sources before an incremental update.
pub fn invalidate_changed(
    lib: &Library,
    names: &Names,
    catalog: &Catalog,
    root: &Path,
    previous: &BTreeMap<String, (u64, u64)>,
) -> Result<()> {
    let changed = |name: &str| {
        lib.index
            .bundles
            .get(name)
            .is_none_or(|b| previous.get(name) != Some(&(b.size, b.modified)))
    };
    let types = TYPES.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let options = Options::default();
    let manifest = read_json(&root.join("manifest.json"))?;
    let selection = select(names, catalog, &types, &options, &manifest)?;
    let remove = |relative: &str| -> Result<()> {
        let path = Path::new(relative);
        ensure!(
            path.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "非法素材相对路径"
        );
        let path = root.join(path);
        if path.is_file() {
            fs::remove_file(path)?;
        }
        Ok(())
    };
    for group in selection.selected {
        for (resource, _, file) in image_jobs(group, &options) {
            let modified = if let Some(item) = &resource.item {
                super::item_icons::icon_bundles(lib, names, &item.icon)
                    .map(|(atlas, page)| changed(&atlas) || changed(&page))
                    .unwrap_or(true)
            } else {
                changed(&resource.bundle)
            };
            if modified {
                remove(&file)?;
            }
        }
    }
    let paths = super::names::asset_manifest(&lib.index.source)?;
    for group in selection.spine_groups {
        let Some(resource) = group.resources.first() else {
            continue;
        };
        let prefix = format!(
            "{}/",
            resource
                .path
                .split('/')
                .take(3)
                .collect::<Vec<_>>()
                .join("/")
        );
        if paths
            .iter()
            .any(|(path, bundle)| path.starts_with(&prefix) && changed(bundle))
        {
            let base = format!("{}/spine", skin_base(group));
            let scene = read_json(&root.join(format!("{base}/scene.json")))?;
            if let Some(layers) = scene["layers"].as_array() {
                for layer in layers {
                    for field in ["skeleton", "atlas"] {
                        if let Some(file) = layer[field].as_str() {
                            remove(file)?;
                        }
                    }
                    if let Some(pages) = layer["pages"].as_object() {
                        for file in pages.values().filter_map(Value::as_str) {
                            remove(file)?;
                        }
                    }
                }
            }
            if let Some(backgrounds) = scene["backgrounds"].as_array() {
                for file in backgrounds.iter().filter_map(|b| b["path"].as_str()) {
                    remove(file)?;
                }
            }
            remove(&format!("{base}/scene.json"))?;
        }
    }
    Ok(())
}

pub fn run_with_options(
    lib: &Library,
    names: &Names,
    catalog: &Catalog,
    output: &str,
    types: &[String],
    options: &Options,
    task: &Task,
) -> Result<Report> {
    let started = std::time::Instant::now();
    let cache_before = super::bundles::stats();
    ensure!(
        !types.is_empty() && types.iter().all(|t| TYPES.contains(&t.as_str())),
        "请选择有效的导出分类"
    );
    let root = export_root(lib, output, options.direct_output)?;
    fs::create_dir_all(&root)?;
    let mut manifest = read_json(&root.join("manifest.json"))?;
    ensure!(
        manifest["schema_version"].is_null() || manifest["schema_version"] == 2,
        "此目录的导出格式不兼容，请选择新位置"
    );
    task.update("检查资源更新", 0, 0, "");
    ensure!(
        !super::index::check_updates(lib, &|l, d, t, c| task.update(l, d, t, c))?.changed,
        "资源已更新，请先重建索引"
    );
    if options.overwrite {
        task.update("清理所选分类的旧导出", 0, 0, "");
        if task.cancelled() {
            anyhow::bail!("已取消导出");
        }
        manifest["schema"] = json!("majsoul-gallery-assets");
        manifest["schema_version"] = json!(2);
        manifest["status"] = json!("partial");
        write_json(&root, "manifest.json", &manifest)?;
        clean_selected(&root, types, &mut manifest)?;
        write_json(&root, "manifest.json", &manifest)?;
    } else {
        // Resume metadata may advance during export. An interrupted update must
        // not retain the previous "complete" marker and bypass the next retry.
        manifest["schema"] = json!("majsoul-gallery-assets");
        manifest["schema_version"] = json!(2);
        manifest["status"] = json!("partial");
        write_json(&root, "manifest.json", &manifest)?;
    }
    let Selection {
        items,
        selected,
        spine_groups,
        characters,
        aliases,
        referenced,
    } = select(names, catalog, types, options, &manifest)?;
    let total = selected
        .iter()
        .map(|g| export_resources(g, options).len())
        .sum::<usize>()
        + spine_groups.len()
        + referenced.len();
    ensure!(total > 0 || options.overwrite, "所选分类没有可导出的资源");
    let mut tables = BTreeMap::<String, Table>::new();
    let mut skin_files = BTreeMap::<String, Value>::new();
    let mut item_icons = BTreeMap::<String, String>::new();
    let mut errors = vec![];
    let (mut done, mut count, mut reused, mut scenes) = (0, 0, 0, 0);
    let mut files_by_category = manifest["categories"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut character_paths = manifest["characters"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    let mut ordered_groups = selected
        .iter()
        .copied()
        .chain(spine_groups.iter().copied())
        .collect::<Vec<_>>();
    ordered_groups.sort_by_key(|group| {
        let category = match group.category.as_str() {
            "character" => "character",
            "spine" => "spine",
            "item" => "item",
            "gift" => "gift",
            "loading" => "illustration",
            "title" => "title",
            "rank" => "rank",
            _ => "decoration",
        };
        (
            TYPES
                .iter()
                .position(|t| *t == category)
                .unwrap_or(TYPES.len()),
            key(group),
        )
    });
    let mut planned_section = String::new();
    let mut image_results = BTreeMap::<String, Result<(Value, bool)>>::new();
    for group in &ordered_groups {
        if group.category == "spine" {
            if task.cancelled() {
                break;
            }
            task.update("导出动态皮肤", done, total, &group.label);
            let base = skin_base(group);
            let file = format!("{base}/skin.json");
            let result = (|| -> Result<()> {
                let scene = export_scene(
                    lib,
                    names,
                    catalog,
                    group.resources.first().context("缺少动态素材")?,
                    &root,
                    &format!("{base}/spine"),
                    options,
                )?;
                let scene_file = format!("{base}/spine/scene.json");
                write_json(&root, &scene_file, &scene)?;
                if !skin_files.contains_key(&file) {
                    skin_files.insert(file.clone(), read_json(&root.join(&file))?);
                }
                let skin = skin_files.get_mut(&file).unwrap();
                skin["id"] = json!(key(group));
                skin["name"] = json!(group.label);
                skin["character_id"] = json!(owner(group));
                skin["spine"] = json!(scene_file);
                skin["bindings"] = json!(group.bindings);
                scenes += 1;
                Ok(())
            })();
            if let Err(e) = result {
                errors.push(json!({"skin":key(group),"error":format!("{e:#}")}));
            }
            done += 1;
            continue;
        }
        if task.cancelled() && image_results.is_empty() {
            break;
        }
        let category = section(&group.category).to_owned();
        let category_label = match category.as_str() {
            "characters" => "角色",
            "items" => "道具",
            "gifts" => "礼物",
            "decorations" => "装扮",
            "illustrations" => "插画",
            "titles" => "称号",
            "ranks" => "段位",
            _ => &category,
        };
        let export_label = format!("导出{category_label}图片");
        if planned_section != category {
            planned_section = category.clone();
            image_results.clear();
            task.update(&export_label, done, total, "规划资源包…");
            let mut planned = ordered_groups
                .iter()
                .filter(|g| g.category != "spine" && section(&g.category) == category)
                .flat_map(|g| image_jobs(g, options))
                .collect::<Vec<_>>();
            // Item resources are virtual: resolve the actual atlas page during planning.
            // Existing incremental targets need no dependency lookup at all.
            let mut bundle_keys = BTreeMap::new();
            for (resource, _, file) in &planned {
                if task.cancelled() {
                    break;
                }
                let bundle = if reusable_image(&root, file, options.overwrite).is_some() {
                    String::new()
                } else if let Some(item) = &resource.item {
                    super::item_icons::icon_bundles(lib, names, &item.icon)
                        .map(|(_, page)| page)
                        .unwrap_or_else(|_| resource.bundle.clone())
                } else {
                    resource.bundle.clone()
                };
                bundle_keys.insert(file.clone(), bundle);
            }
            planned.sort_by(|a, b| {
                bundle_keys
                    .get(&a.2)
                    .cmp(&bundle_keys.get(&b.2))
                    .then(a.2.cmp(&b.2))
            });
            let mut completed = 0;
            for batch in planned.chunks(2) {
                if task.cancelled() {
                    break;
                }
                task.update(&export_label, done + completed, total, &batch[0].0.path);
                let decode_one = |job: &(&Resource, String, String)| {
                    image_asset(lib, names, job.0, &root, &job.2, options)
                };
                let results = if batch.len() == 1 {
                    vec![decode_one(&batch[0])]
                } else {
                    std::thread::scope(|scope| {
                        let workers = batch
                            .iter()
                            .map(|job| scope.spawn(|| decode_one(job)))
                            .collect::<Vec<_>>();
                        workers
                            .into_iter()
                            .map(|worker| {
                                worker
                                    .join()
                                    .unwrap_or_else(|_| Err(anyhow::anyhow!("图片处理线程异常")))
                            })
                            .collect::<Vec<_>>()
                    })
                };
                for ((_, _, file), result) in batch.iter().zip(results) {
                    image_results.insert(file.clone(), result);
                    completed += 1;
                }
            }
        }
        let id = key(group);
        let is_character = group.category == "character";
        let base = skin_base(group);
        let mut image_paths = BTreeMap::<String, Vec<String>>::new();
        let jobs = image_jobs(group, options);
        let mut refs = vec![];
        for batch in jobs.chunks(2) {
            let results = batch
                .iter()
                .map(|(_, _, file)| image_results.remove(file))
                .collect::<Vec<_>>();
            for ((resource, role, file), result) in batch.iter().zip(results) {
                let Some(result) = result else { continue };
                match result {
                    Ok((asset, was_reused)) => {
                        count += 1;
                        reused += usize::from(was_reused);
                        refs.push(file.clone());
                        image_paths
                            .entry(role.clone())
                            .or_default()
                            .push(file.clone());
                        if let Some(item) = &resource.item {
                            item_icons.insert(item.id.to_string(), file.clone());
                        }
                        if !is_character {
                            tables
                                .entry(category.clone())
                                .or_default()
                                .assets
                                .insert(file.clone(), asset);
                        }
                    }
                    Err(e) => {
                        errors.push(json!({"resource":resource.path,"error":format!("{e:#}")}))
                    }
                }
                done += 1;
            }
        }
        let entry_id = format!("{}:{id}", group.category);
        if is_character {
            let file = format!("{base}/skin.json");
            let mut skin = read_json(&root.join(&file))?;
            skin["id"] = json!(id);
            skin["name"] = json!(group.label);
            skin["character_id"] = json!(owner(group));
            skin["bindings"] = json!(group.bindings);
            skin["images"] = json!(image_paths);
            skin_files.insert(file, skin);
        } else {
            let table = tables.entry(category).or_default();
            let mut entry = json!({"id":id,"name":group.label,"category":group.category,"bindings":group.bindings,"assets":refs});
            if group.category == "loading" {
                let path = format!("{}/", group.path);
                if let Some(row) = names
                    .rows("item_definition.loading_image")
                    .iter()
                    .find(|row| {
                        ["img_path", "thumb_path"].iter().any(|field| {
                            let raw = row[field]
                                .as_str()
                                .unwrap_or_default()
                                .trim_start_matches('/')
                                .to_lowercase();
                            let normalized = if raw.starts_with("myassets/") {
                                raw
                            } else {
                                format!("myassets/{raw}")
                            };
                            normalized.starts_with(&path)
                        })
                    })
                {
                    entry["loading_image_id"] = row["id"].clone();
                    entry["sort"] = row["sort"].clone();
                    entry["unlock_items"] = row["unlock_items"].clone();
                    entry["images"] = illustration_images(row, &refs, &table.assets);
                }
            }
            table.entries.insert(entry_id.clone(), entry);
            for binding in &group.bindings {
                if matches!(group.category.as_str(), "bgm" | "riichi-bgm") {
                    table.music.insert(format!("{}:{}",binding.table,binding.id),json!({"id":binding.id,"name":binding.name,"table":binding.table,"entry":entry_id,"assets":refs}));
                }
            }
        }
    }
    // Character dependencies use the same ordinary item/gift files as the category exports.
    for (id, item) in referenced {
        if task.cancelled() {
            break;
        }
        task.update("导出喜好与契约图标", done, total, &item.name);
        if let std::collections::btree_map::Entry::Vacant(entry) = item_icons.entry(id.to_string())
        {
            let folder = if items.get(&id).is_some_and(|r| r["category"] == 2) {
                "gifts"
            } else {
                "items"
            };
            let file = format!("{folder}/{id}.png");
            let result = (|| -> Result<()> {
                if reusable_image(&root, &file, options.overwrite).is_some() {
                    reused += 1;
                } else {
                    let icon =
                        super::item_icons::decode_items(lib, names, std::slice::from_ref(&item))
                            .pop()
                            .context("图标结果为空")?;
                    let path = icon
                        .path
                        .with_context(|| icon.error.unwrap_or_else(|| "图标未找到".into()))?;
                    copy(&root, &path, &file, true, options.link_cached_files)?;
                }
                entry.insert(file);
                count += 1;
                Ok(())
            })();
            if let Err(e) = result {
                errors.push(json!({"item_id":id,"error":format!("{e:#}")}));
            }
        }
        if let Some(file) = item_icons.get(&id.to_string()).cloned() {
            let section = if file.starts_with("gifts/") {
                "gifts"
            } else {
                "items"
            };
            let table = tables.entry(section.into()).or_default();
            if !table.assets.contains_key(&file) {
                let (width, height) = image::image_dimensions(root.join(&file))?;
                table.assets.insert(
                    file.clone(),
                    json!({"file":file,"width":width,"height":height,"kind":"icon"}),
                );
            }
            table
                .entries
                .entry(format!(
                    "{}:{id}",
                    if section == "gifts" { "gift" } else { "item" }
                ))
                .or_insert_with(|| json!({"id":id.to_string(),"name":item.name,"assets":[file]}));
        }
        done += 1;
    }
    for (alias, target) in aliases {
        if let Some(file) = item_icons.get(&target).cloned() {
            item_icons.insert(alias, file);
        }
    }
    task.update("保存角色与分类资料", done, total, "");
    let mut character_data = BTreeMap::<String, Value>::new();
    for (id, profile) in &characters {
        let file = format!("characters/{id}/character.json");
        let mut value = read_json(&root.join(&file))?;
        let old_skins = value["skins"].as_object().cloned().unwrap_or_default();
        value = serde_json::to_value(profile)?;
        value["skins"] = json!(old_skins);
        if let Some(gifts) = value["gifts"].as_array_mut() {
            if options.unlocked_gifts_only {
                gifts.retain(|item| !item["icon"].as_str().is_some_and(locked));
            }
            for item in gifts {
                remove_item_icon(item);
            }
        }
        if let Some(materials) = value["contract_materials"].as_array_mut() {
            for material in materials {
                if let Some(items) = material["items"].as_array_mut() {
                    for item in items {
                        remove_item_icon(item);
                    }
                }
            }
        }
        character_data.insert(id.clone(), value);
        character_paths.insert(id.clone(), json!(file));
    }
    for (file, skin) in &skin_files {
        write_json(&root, file, skin)?;
        let id = skin["character_id"]
            .as_str()
            .unwrap_or("unassigned")
            .to_owned();
        if !character_data.contains_key(&id) {
            let mut value = read_json(&root.join(format!("characters/{id}/character.json")))?;
            if value["skins"].is_null() {
                value["skins"] = json!({});
            }
            character_data.insert(id.clone(), value);
        }
        let value = character_data.get_mut(&id).unwrap();
        value["skins"][skin["id"].as_str().context("皮肤缺少 ID")?] = json!(file);
        character_paths.insert(id.clone(), json!(format!("characters/{id}/character.json")));
    }
    for (id, value) in character_data {
        write_json(&root, &format!("characters/{id}/character.json"), &value)?;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs() as i64;
    for (id, definition) in super::items::definitions(names) {
        let gift = definition.group == super::items::ItemGroup::Gifts;
        if !types
            .iter()
            .any(|kind| kind == if gift { "gift" } else { "item" })
            || (options.unlocked_gifts_only && definition.locked)
            || (options.exclude_expired
                && id
                    .parse::<i64>()
                    .ok()
                    .and_then(|id| items.get(&id))
                    .is_some_and(|row| expired(row, now)))
        {
            continue;
        }
        tables
            .entry(if gift { "gifts" } else { "items" }.into())
            .or_default()
            .definitions
            .insert(id, definition);
    }
    for (section, mut table) in tables {
        for (id, file) in &item_icons {
            if file.starts_with(&format!("{section}/")) {
                table.item_assets.insert(id.clone(), file.clone());
            }
        }
        let file = format!("{section}/{section}.json");
        write_json(&root, &file, &serde_json::to_value(table)?)?;
        files_by_category.insert(section, json!(file));
    }
    let cancelled = task.cancelled();
    if !cancelled {
        let update = super::index::check_updates(lib, &|_, _, _, _| {})?;
        if update.changed {
            errors.push(json!({"error":"导出期间资源变化，请重建索引"}));
        }
    }
    manifest = json!({"schema":"majsoul-gallery-assets","schema_version":2,"status":if cancelled{"cancelled"}else if errors.is_empty(){"complete"}else{"partial"},"mode":if options.overwrite{"full"}else{"incremental"},"unity":lib.index.unity,"categories":files_by_category,"characters":character_paths,"filters":options,"errors":errors,"limitations":["装扮仅导出图标，音乐不含音频","动态皮肤需要 Spine runtime，Unity shader/粒子未完整恢复"]});
    write_json(&root, "manifest.json", &manifest)?;
    task.update(
        if cancelled {
            "导出已取消"
        } else {
            "导出完成"
        },
        done,
        total,
        "",
    );
    let cache_after = super::bundles::stats();
    Ok(Report {
        elapsed_ms: started.elapsed().as_millis(),
        bundle_cache_hits: cache_after.hits.saturating_sub(cache_before.hits),
        bundle_loads: cache_after.loads.saturating_sub(cache_before.loads),
        directory: root.to_string_lossy().into(),
        assets: count,
        scenes,
        errors: errors.len(),
        cancelled,
        reused,
    })
}
fn remove_item_icon(item: &mut Value) {
    if let Some(item) = item.as_object_mut() {
        item.remove("icon");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn illustration_roles_match_configured_paths_instead_of_image_size() {
        let config = json!({"img_path":"deco/loading_cg/260901/main/260901.jpg","thumb_path":"deco/loading_cg/260901/thumb/260901_thumb.jpg"});
        let files = vec![
            "emoji.png".into(),
            "main.png".into(),
            "thumb.png".into(),
            "thumb-emoji.png".into(),
        ];
        let assets = BTreeMap::from([
            (
                "emoji.png".into(),
                json!({"source_path":"myassets/deco/loading_cg/260901/main/common/1001.png"}),
            ),
            (
                "main.png".into(),
                json!({"source_path":"myassets/deco/loading_cg/260901/main/chs/260901.jpg"}),
            ),
            (
                "thumb.png".into(),
                json!({"source_path":"myassets/deco/loading_cg/260901/thumb/common/260901_thumb.jpg"}),
            ),
            (
                "thumb-emoji.png".into(),
                json!({"source_path":"myassets/deco/loading_cg/260901/thumb/common/3001.png"}),
            ),
        ]);
        assert_eq!(
            illustration_images(&config, &files, &assets),
            json!({"full":"main.png","thumbnail":"thumb.png"})
        );
    }
    use crate::core::index::digest;
    #[test]
    fn managed_output_uses_exact_directory_and_rejects_game_directory() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let game = temp.path().join("game");
        let materials = temp.path().join("materials");
        fs::create_dir_all(&game)?;
        fs::create_dir_all(&materials)?;
        let game = game.canonicalize()?;
        let materials = materials.canonicalize()?;
        let lib = Library {
            index: super::super::index::Index {
                schema: 2,
                manifest_digest: String::new(),
                source: game.join("Data/StreamingAssets/StandaloneWindows"),
                unity: String::new(),
                bundles: BTreeMap::new(),
            },
            index_path: temp.path().join("index.json"),
        };
        assert_eq!(
            export_root(&lib, materials.to_str().unwrap(), true)?,
            materials
        );
        assert_eq!(
            export_root(&lib, materials.to_str().unwrap(), false)?,
            materials.join("majsoul-assets")
        );
        assert!(export_root(&lib, game.to_str().unwrap(), true).is_err());
        Ok(())
    }

    #[test]
    fn linked_import_survives_scratch_cleanup_and_replaces_atomically() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let cache = temp.path().join("cache");
        let root = temp.path().join("materials");
        fs::create_dir_all(&cache)?;
        let source = cache.join("decoded.png");
        fs::write(&source, b"original")?;
        copy(&root, source.to_str().unwrap(), "image.png", true, true)?;
        // Both names refer to the same data on the local test filesystem.
        fs::write(&source, b"shared")?;
        assert_eq!(fs::read(root.join("image.png"))?, b"shared");
        let replacement = cache.join("replacement.png");
        fs::write(&replacement, b"updated")?;
        copy(
            &root,
            replacement.to_str().unwrap(),
            "image.png",
            true,
            true,
        )?;
        assert_eq!(fs::read(&source)?, b"shared");
        assert_eq!(fs::read(root.join("image.png"))?, b"updated");
        fs::remove_dir_all(cache)?;
        assert_eq!(fs::read(root.join("image.png"))?, b"updated");
        Ok(())
    }
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn measure_local_bundle_schedule() -> Result<()> {
        let lib = Library::open(None)?;
        let names = Names::load(&lib)?;
        let mut catalog = Catalog::build(&lib);
        super::super::spine::supplement_catalog(&names, &mut catalog);
        names.apply(&mut catalog);
        names.apply_characters(&lib, &mut catalog)?;
        super::super::business::populate(&names, &mut catalog);
        let options = Options::default();
        let groups = business_groups(
            catalog
                .groups
                .iter()
                .filter(|g| g.category != "spine" && (g.category != "character" || player(g))),
        );
        let started = std::time::Instant::now();
        let mut sections = BTreeMap::<String, Vec<String>>::new();
        for group in groups {
            let mut jobs = image_jobs(group, &options);
            jobs.sort_by(|a, b| a.0.bundle.cmp(&b.0.bundle).then(a.2.cmp(&b.2)));
            for (resource, _, _) in jobs {
                let bundle = if let Some(item) = &resource.item {
                    super::super::item_icons::icon_bundles(&lib, &names, &item.icon)?.1
                } else {
                    resource.bundle.clone()
                };
                sections
                    .entry(section(&group.category).into())
                    .or_default()
                    .push(bundle);
            }
        }
        let mut before = 0;
        let mut after = 0;
        let mut jobs = 0;
        for bundles in sections.values() {
            jobs += bundles.len();
            before += usize::from(!bundles.is_empty())
                + bundles.windows(2).filter(|w| w[0] != w[1]).count();
            after += bundles
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len();
        }
        eprintln!(
            "planning: {jobs} images, bundle runs {before} -> {after}, {:.3}s",
            started.elapsed().as_secs_f64()
        );
        assert!(after <= before);
        Ok(())
    }
    #[test]
    fn expiry_respects_markers_and_utc8_deadlines() {
        assert!(expired(&json!({"name_chs":"旧道具(已过期)"}), 0));
        assert!(expired(&json!({"item_expire":"1970-01-01 08:00:00"}), 0));
        assert!(!expired(&json!({"item_expire":"1970-01-01 08:00:01"}), 0));
        assert!(!expired(&json!({"item_expire":"unknown"}), 0));
    }
    #[test]
    fn full_cleanup_removes_selected_files_and_preserves_unselected_skin_content() -> Result<()> {
        fn fixture(parent: &Path) -> Result<(std::path::PathBuf, Value)> {
            let root = parent.join("majsoul-assets");
            let base = "characters/1/skins/2";
            fs::create_dir_all(root.join(format!("{base}/spine")))?;
            fs::create_dir_all(root.join("items"))?;
            fs::create_dir_all(root.join("gifts"))?;
            for file in [
                format!("{base}/full.png"),
                format!("{base}/image.png"),
                format!("{base}/spine/page.png"),
                "items/old.png".into(),
                "gifts/keep.png".into(),
            ] {
                fs::write(root.join(file), b"fixture")?;
            }
            write_json(
                &root,
                &format!("{base}/spine/scene.json"),
                &json!({"layers":[]}),
            )?;
            write_json(
                &root,
                &format!("{base}/skin.json"),
                &json!({"id":"2","images":{"full":[format!("{base}/full.png")],"image":[format!("{base}/image.png")]},"spine":format!("{base}/spine/scene.json")}),
            )?;
            write_json(
                &root,
                "characters/1/character.json",
                &json!({"skins":{"2":format!("{base}/skin.json")}}),
            )?;
            Ok((
                root,
                json!({"categories":{"items":"items/items.json","gifts":"gifts/gifts.json"},"characters":{"1":"characters/1/character.json"}}),
            ))
        }
        let temp = tempfile::tempdir()?;
        let (root, mut manifest) = fixture(temp.path())?;
        clean_selected(&root, &["character".into(), "item".into()], &mut manifest)?;
        assert!(!root.join("characters/1/skins/2/image.png").exists());
        assert!(!root.join("characters/1/skins/2/full.png").exists());
        assert!(root.join("characters/1/skins/2/spine/page.png").exists());
        assert!(!root.join("items").exists());
        assert!(root.join("gifts/keep.png").exists());
        assert!(manifest["categories"].get("items").is_none());
        let temp = tempfile::tempdir()?;
        let (root, mut manifest) = fixture(temp.path())?;
        clean_selected(&root, &["spine".into()], &mut manifest)?;
        assert!(root.join("characters/1/skins/2/full.png").exists());
        assert!(!root.join("characters/1/skins/2/spine").exists());
        assert!(
            read_json(&root.join("characters/1/skins/2/skin.json"))?
                .get("spine")
                .is_none()
        );
        clean_selected(&root, &["character".into(), "spine".into()], &mut manifest)?;
        assert!(!root.join("characters").exists());
        assert!(root.join("gifts/keep.png").exists());
        assert!(remove_export_path(&root, temp.path()).is_err());
        Ok(())
    }
    #[test]
    #[ignore = "requires local installation"]
    fn business_layout_incremental_reuse_and_full_overwrite() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let source = super::super::index::initialize(
            r"D:\Program Files (x86)\Steam\steamapps\common\MahjongSoul",
            temp.path(),
            true,
        )?;
        let index_path = temp.path().join("indexes").join(format!(
            "{}.json",
            digest(source.to_string_lossy().as_bytes())
        ));
        let lib = Library {
            index: serde_json::from_slice(&fs::read(&index_path)?)?,
            index_path,
        };
        let names = Names::load(&lib)?;
        let mut catalog = Catalog::build(&lib);
        super::super::spine::supplement_catalog(&names, &mut catalog);
        names.apply(&mut catalog);
        names.apply_characters(&lib, &mut catalog)?;
        super::super::business::populate(&names, &mut catalog);
        let mut seen = std::collections::BTreeSet::new();
        catalog.groups.retain(|g| {
            if g.category == "spine" {
                return g.path == "myassets/spine/40010803";
            }
            if g.category == "character" {
                return g
                    .bindings
                    .iter()
                    .any(|b| b.table == "item_definition.skin" && b.id == 40010803);
            }
            ["item", "gift", "bgm", "title", "rank"].contains(&g.category.as_str())
                && seen.insert(g.category.clone())
        });
        let jobs = super::super::progress::Jobs::default();
        let task = jobs.begin("export")?;
        let types = TYPES.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let output = temp.path().to_string_lossy();
        let planned = dry_run(
            &lib,
            &names,
            &catalog,
            &output,
            &types,
            &Options::default(),
            &task,
        )?;
        assert!(planned.errors.is_empty());
        assert!(planned.images > 0 && planned.scenes == 1);
        assert!(
            !temp.path().join("majsoul-assets").exists(),
            "dry run must not create export root"
        );
        let first = run(&lib, &names, &catalog, &output, &types, &task)?;
        let root = Path::new(&first.directory);
        assert_eq!(root.file_name().unwrap(), "majsoul-assets");
        assert_eq!(first.errors, 0);
        let manifest = read_json(&root.join("manifest.json"))?;
        assert_eq!(manifest["schema_version"], 2);
        assert_eq!(manifest["status"], "complete");
        let character = read_json(
            &root.join(
                manifest["characters"]["20000108"]
                    .as_str()
                    .context("missing character")?,
            ),
        )?;
        let skin_file = character["skins"]["40010803"]
            .as_str()
            .context("missing skin")?;
        let skin = read_json(&root.join(skin_file))?;
        for files in skin["images"]
            .as_object()
            .context("missing images")?
            .values()
        {
            for file in files.as_array().unwrap() {
                let file = Path::new(file.as_str().unwrap());
                assert_eq!(file.parent(), Path::new(skin_file).parent());
                assert!(root.join(file).is_file());
            }
        }
        assert_eq!(
            skin["spine"],
            "characters/20000108/skins/40010803/spine/scene.json"
        );
        let scene = read_json(&root.join(skin["spine"].as_str().unwrap()))?;
        assert!(scene_ready(root, &scene));
        assert!(character.get("item_icons").is_none());
        for gift in character["gifts"].as_array().unwrap() {
            assert!(gift.get("icon").is_none());
        }
        let second = run(&lib, &names, &catalog, &output, &types, &task)?;
        assert_eq!(second.directory, first.directory);
        assert_eq!(second.assets, second.reused);
        assert_eq!(second.errors, 0);
        let mut stamps = lib
            .index
            .bundles
            .iter()
            .map(|(name, b)| (name.clone(), (b.size, b.modified)))
            .collect::<BTreeMap<_, _>>();
        let static_group = catalog
            .groups
            .iter()
            .find(|g| g.category == "character")
            .context("missing static skin")?;
        let (resource, _, static_file) = image_jobs(static_group, &Options::default())
            .into_iter()
            .next()
            .context("missing skin image")?;
        invalidate_changed(&lib, &names, &catalog, root, &stamps)?;
        assert!(
            root.join(&static_file).is_file(),
            "unchanged images should be retained"
        );
        stamps.remove(&resource.bundle);
        invalidate_changed(&lib, &names, &catalog, root, &stamps)?;
        assert!(
            !root.join(&static_file).exists(),
            "changed bundle image must be regenerated"
        );
        let updated = run(&lib, &names, &catalog, &output, &types, &task)?;
        assert_eq!(updated.errors, 0);
        assert!(updated.reused < updated.assets);
        assert!(root.join(&static_file).is_file());
        let mut stamps = lib
            .index
            .bundles
            .iter()
            .map(|(name, b)| (name.clone(), (b.size, b.modified)))
            .collect::<BTreeMap<_, _>>();
        let spine_bundle = names
            .spine_assets
            .iter()
            .find(|(path, _)| {
                path.starts_with("myassets/spine/40010803/") && path.ends_with(".atlas.txt")
            })
            .context("missing spine atlas")?
            .1;
        stamps.remove(spine_bundle);
        invalidate_changed(&lib, &names, &catalog, root, &stamps)?;
        assert!(!root.join(skin["spine"].as_str().unwrap()).exists());
        let updated = run(&lib, &names, &catalog, &output, &types, &task)?;
        assert_eq!(updated.errors, 0);
        assert!(scene_ready(
            root,
            &read_json(&root.join(skin["spine"].as_str().unwrap()))?
        ));
        let before_manifest = fs::read(root.join("manifest.json"))?;
        let sentinel = root.join("items/keep-during-dry-run.txt");
        fs::write(&sentinel, "do not remove")?;
        let incremental_plan = dry_run(
            &lib,
            &names,
            &catalog,
            &output,
            &types,
            &Options::default(),
            &task,
        )?;
        assert_eq!(incremental_plan.images, incremental_plan.reused_images);
        assert_eq!(incremental_plan.scenes, incremental_plan.reused_scenes);
        let full_plan = dry_run(
            &lib,
            &names,
            &catalog,
            &output,
            &types,
            &Options {
                overwrite: true,
                ..Default::default()
            },
            &task,
        )?;
        assert_eq!(
            full_plan.reused_images + full_plan.reused_scenes + full_plan.reused_dependency_icons,
            0
        );
        assert_eq!(fs::read(&sentinel)?, b"do not remove");
        assert_eq!(fs::read(root.join("manifest.json"))?, before_manifest);
        let full = run_with_options(
            &lib,
            &names,
            &catalog,
            &output,
            &types,
            &Options {
                overwrite: true,
                ..Default::default()
            },
            &task,
        )?;
        assert_eq!(full.reused, 0);
        assert_eq!(full.errors, 0);
        let locked_ids = names
            .rows("item_definition.item")
            .iter()
            .filter(|r| r["icon"].as_str().is_some_and(locked))
            .filter_map(|r| r["id"].as_i64())
            .collect::<std::collections::BTreeSet<_>>();
        assert!(
            character["gifts"]
                .as_array()
                .unwrap()
                .iter()
                .all(|g| !locked_ids.contains(&g["id"].as_i64().unwrap()))
        );
        let with_locked = run_with_options(
            &lib,
            &names,
            &catalog,
            &output,
            &types,
            &Options {
                unlocked_gifts_only: false,
                ..Default::default()
            },
            &task,
        )?;
        assert_eq!(with_locked.errors, 0);
        let character = read_json(&root.join("characters/20000108/character.json"))?;
        let gifts = read_json(&root.join("gifts/gifts.json"))?;
        for gift in character["gifts"].as_array().unwrap() {
            assert!(gift.get("icon").is_none());
            let id = gift["id"].as_i64().unwrap().to_string();
            let file = gifts["item_assets"][&id]
                .as_str()
                .context("missing gift image mapping")?;
            assert!(root.join(file).is_file());
            if locked_ids.contains(&gift["id"].as_i64().unwrap()) {
                assert_eq!(file, format!("gifts/{id}.png"));
            }
        }
        jobs.cancel_export();
        let cancelled = run(&lib, &names, &catalog, &output, &types, &task)?;
        assert!(cancelled.cancelled);
        assert_eq!(
            read_json(&root.join("manifest.json"))?["status"],
            "cancelled"
        );
        Ok(())
    }
}
