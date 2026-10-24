use crate::search::SearchIndex;
use anyhow::{Context, Result, ensure};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Item {
    pub id: String,
    pub name: String,
    pub icon: Option<PathBuf>,
}
#[derive(Clone, Debug)]
pub struct Requirement {
    pub quantity: u64,
    pub items: Vec<Item>,
}
impl Requirement {
    pub fn display_item(&self) -> Option<&Item> {
        self.items
            .iter()
            .find(|item| item.icon.is_some())
            .or_else(|| self.items.first())
    }
}
#[derive(Clone, Debug)]
pub struct Skin {
    pub search: SearchIndex,
    pub id: String,
    pub name: String,
    pub full: Option<PathBuf>,
    pub portrait: Option<PathBuf>,
    pub avatar: Option<PathBuf>,
    pub animated: bool,
    pub scene: Option<PathBuf>,
}
impl Skin {
    pub fn full_image(&self) -> Option<PathBuf> {
        self.full.clone().or_else(|| self.thumbnail())
    }
    pub fn display_name<'a>(&'a self, character: &str) -> &'a str {
        let Some(suffix) = self.name.strip_prefix(character) else {
            return &self.name;
        };
        let suffix =
            suffix.trim_start_matches(|c: char| c.is_whitespace() || "·・:-—：/".contains(c));
        if suffix.is_empty() {
            "初始形象"
        } else {
            suffix
        }
    }
    pub fn thumbnail(&self) -> Option<PathBuf> {
        self.portrait
            .clone()
            .or_else(|| self.avatar.clone())
            .or_else(|| self.full.clone())
    }
}
#[derive(Clone, Debug)]
pub struct Character {
    pub search: SearchIndex,
    pub id: String,
    pub name: String,
    pub sort: i64,
    pub sex: u64,
    pub limited: bool,
    pub collaboration: bool,
    pub skins: Vec<Skin>,
    pub gifts: Vec<Item>,
    pub materials: Vec<Requirement>,
}
impl Character {
    pub fn thumbnail(&self) -> Option<PathBuf> {
        self.skins.iter().find_map(Skin::thumbnail)
    }
}
#[derive(Clone, Debug)]
pub struct Decoration {
    pub search: SearchIndex,
    pub id: String,
    pub name: String,
    pub category: String,
    pub image: Option<PathBuf>,
}
/// Export versions use several source names for the same business category.
pub fn decoration_category(category: &str) -> &str {
    match category {
        "background-icon" => "background",
        "riichi-bgm" | "music" => "bgm",
        other => other,
    }
}
impl Decoration {
    pub fn matches_category(&self, filter: &str) -> bool {
        filter.is_empty() || decoration_category(&self.category) == decoration_category(filter)
    }
}
#[derive(Clone, Debug)]
pub struct Title {
    pub id: String,
    pub ids: Vec<String>,
    pub name: String,
    pub image: Option<PathBuf>,
    pub search: SearchIndex,
}
fn title_entries(root: &Path, table: &Value) -> Vec<Title> {
    let mut titles = Vec::new();
    if let Some(entries) = table["entries"].as_object() {
        for entry in entries.values() {
            let image = entry["assets"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .and_then(|p| resolve(root, p));
            let mut names = BTreeMap::<String, BTreeSet<String>>::new();
            if let Some(bindings) = entry["bindings"].as_array() {
                for binding in bindings
                    .iter()
                    .filter(|b| b["table"] == "item_definition.title")
                {
                    let name = binding["name"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| entry["name"].as_str().unwrap_or_default());
                    names
                        .entry(name.into())
                        .or_default()
                        .insert(id(&binding["id"]));
                }
            }
            if names.is_empty() {
                names
                    .entry(text(entry, "name"))
                    .or_default()
                    .insert(id(&entry["id"]));
            }
            for (name, ids) in names {
                let ids = ids.into_iter().collect::<Vec<_>>();
                titles.push(Title {
                    id: ids[0].clone(),
                    search: SearchIndex::new(&name, &ids.join(" ")),
                    ids,
                    name,
                    image: image.clone(),
                });
            }
        }
    }
    titles.sort_by_key(|title| {
        (
            title.id.parse::<u64>().unwrap_or(u64::MAX),
            title.id.clone(),
        )
    });
    titles
}
#[derive(Default)]
pub struct Library {
    pub root: PathBuf,
    pub characters: Vec<Character>,
    pub decorations: Vec<Decoration>,
    pub ranks: BTreeMap<String, Item>,
    pub titles: Vec<Title>,
    pub status: String,
}
fn read(root: &Path, relative: &str) -> Result<Value> {
    let file = resolve(root, relative).with_context(|| format!("找不到包内文件：{relative}"))?;
    serde_json::from_slice(&fs::read(file)?).with_context(|| format!("JSON 读取失败：{relative}"))
}
/// Accept only existing files whose resolved path stays in the selected package.
fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    if path.is_absolute() || !path.components().all(|c| matches!(c, Component::Normal(_))) {
        return None;
    }
    let path = root.join(path).canonicalize().ok()?;
    (path.starts_with(root) && path.is_file()).then_some(path)
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap_or_default().into()
}
fn id(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}
impl Library {
    pub fn load(path: &Path) -> Result<Self> {
        let root = if path.is_file() {
            path.parent().context("素材包路径无效")?
        } else {
            path
        };
        let root = root.canonicalize().context("素材包目录无法访问")?;
        let manifest = read(&root, "manifest.json")?;
        ensure!(
            manifest["schema"] == "majsoul-gallery-assets" && manifest["schema_version"] == 2,
            "请选择 schema v2 的 majsoul-assets 素材包"
        );
        let mut icons = BTreeMap::new();
        let mut decorations = Vec::new();
        let mut ranks = BTreeMap::new();
        let mut titles = Vec::new();
        if let Some(categories) = manifest["categories"].as_object() {
            for (kind, file) in categories {
                let table = read(&root, file.as_str().context("分类路径无效")?)?;
                if kind == "titles" {
                    titles = title_entries(&root, &table);
                }
                if let Some(map) = table["item_assets"].as_object() {
                    for (key, file) in map {
                        if let Some(image) = file.as_str().and_then(|f| resolve(&root, f)) {
                            icons.insert(key.clone(), image);
                        }
                    }
                }
                if kind == "ranks"
                    && let Some(entries) = table["entries"].as_object()
                {
                    for entry in entries.values() {
                        let id = id(&entry["id"]);
                        let icon = entry["assets"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(Value::as_str)
                            .and_then(|f| resolve(&root, f));
                        ranks.insert(
                            id.clone(),
                            Item {
                                id,
                                name: text(entry, "name"),
                                icon,
                            },
                        );
                    }
                }
                if kind == "decorations"
                    && let Some(entries) = table["entries"].as_object()
                {
                    for (key, entry) in entries {
                        let category = text(entry, "category");
                        let image = entry["assets"]
                            .as_array()
                            .and_then(|a| a.first())
                            .and_then(Value::as_str)
                            .and_then(|f| resolve(&root, f));
                        if decoration_category(&category) == "bgm" {
                            let mut expanded = false;
                            if let Some(music) = table["music"].as_object() {
                                for song in
                                    music.values().filter(|song| song["entry"] == key.as_str())
                                {
                                    decorations.push(Decoration {
                                        search: SearchIndex::new(
                                            &text(song, "name"),
                                            &id(&song["id"]),
                                        ),
                                        id: id(&song["id"]),
                                        name: text(song, "name"),
                                        category: category.clone(),
                                        image: image.clone(),
                                    });
                                    expanded = true;
                                }
                            }
                            if !expanded && let Some(bindings) = entry["bindings"].as_array() {
                                for binding in bindings
                                    .iter()
                                    .filter(|b| b["table"] == "item_definition.item")
                                {
                                    decorations.push(Decoration {
                                        search: SearchIndex::new(
                                            &text(binding, "name"),
                                            &id(&binding["id"]),
                                        ),
                                        id: id(&binding["id"]),
                                        name: text(binding, "name"),
                                        category: category.clone(),
                                        image: image.clone(),
                                    });
                                    expanded = true;
                                }
                            }
                            if expanded {
                                continue;
                            }
                        }
                        decorations.push(Decoration {
                            search: SearchIndex::new(
                                &text(entry, "name"),
                                entry["id"].as_str().unwrap_or(key),
                            ),
                            id: entry["id"].as_str().unwrap_or(key).to_owned(),
                            name: text(entry, "name"),
                            category,
                            image,
                        });
                    }
                }
            }
        }
        let item = |value: &Value| {
            let key = id(&value["id"]);
            Item {
                icon: icons.get(&key).cloned(),
                id: key,
                name: text(value, "name"),
            }
        };
        let mut characters = Vec::new();
        if let Some(map) = manifest["characters"].as_object() {
            for (key, file) in map {
                let profile = read(&root, file.as_str().context("角色路径无效")?)?;
                let mut skins = Vec::new();
                if let Some(map) = profile["skins"].as_object() {
                    for (skin_id, file) in map {
                        let skin = read(&root, file.as_str().context("皮肤路径无效")?)?;
                        let image = |role: &str| {
                            skin["images"][role]
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(Value::as_str)
                                .and_then(|f| resolve(&root, f))
                        };
                        skins.push(Skin {
                            search: SearchIndex::new(&text(&skin, "name"), skin_id),
                            id: skin_id.clone(),
                            name: text(&skin, "name"),
                            full: image("full"),
                            portrait: image("portrait"),
                            avatar: image("avatar"),
                            animated: skin["spine"].as_str().is_some(),
                            scene: skin["spine"].as_str().and_then(|f| resolve(&root, f)),
                        });
                    }
                }
                let initial = id(&profile["initial_skin"]);
                skins.sort_by_key(|s| (s.id != initial, s.id.clone()));
                let gifts = profile["gifts"]
                    .as_array()
                    .map(|a| a.iter().map(&item).collect())
                    .unwrap_or_default();
                let materials = profile["contract_materials"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|r| Requirement {
                                quantity: r["quantity"].as_u64().unwrap_or(0),
                                items: r["items"]
                                    .as_array()
                                    .map(|a| a.iter().map(&item).collect())
                                    .unwrap_or_default(),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                characters.push(Character {
                    search: SearchIndex::new(
                        profile["name"]
                            .as_str()
                            .filter(|s| !s.is_empty())
                            .unwrap_or(key),
                        key,
                    ),
                    id: key.clone(),
                    name: profile["name"]
                        .as_str()
                        .filter(|s| !s.is_empty())
                        .unwrap_or(key)
                        .into(),
                    sort: profile["sort"].as_i64().unwrap_or(i64::MAX),
                    sex: profile["sex"].as_u64().unwrap_or(0),
                    limited: profile["limited"] == true
                        || profile["collaboration"].as_i64().unwrap_or(0) > 0,
                    collaboration: profile["collaboration"].as_i64().unwrap_or(0) > 0,
                    skins,
                    gifts,
                    materials,
                });
            }
        }
        characters.sort_by_key(|c| (c.sort, c.id.clone()));
        Ok(Self {
            root,
            characters,
            decorations,
            ranks,
            titles,
            status: text(&manifest, "status"),
        })
    }
    pub fn skin_count(&self) -> usize {
        self.characters.iter().map(|c| c.skins.len()).sum()
    }
}

pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else {
        text.strip_prefix(r"\\?\").unwrap_or(&text).into()
    }
}
pub fn state_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("MAJSOUL_GALLERY_DATA_DIR") {
        return PathBuf::from(path);
    }

    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("majsoul-gallery-data")))
        .unwrap_or_else(|| PathBuf::from("majsoul-gallery-data"))
}
pub fn initial_source(settings: &crate::settings::Settings) -> Option<PathBuf> {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if args.iter().any(|arg| arg == "--setup") {
        return None;
    }
    if let Some(path) = args
        .first()
        .filter(|arg| *arg == "--assets")
        .and_then(|_| args.get(1))
    {
        return Some(path.into());
    }
    if let Some(path) = &settings.assets
        && path.join("manifest.json").is_file()
    {
        return Some(path.clone());
    }
    let managed = state_dir().join("materials");
    if managed.join("manifest.json").is_file() {
        return Some(managed);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decoration_aliases_share_filtering_and_display_names() {
        for category in [
            "background",
            "background-icon",
            "bgm",
            "riichi-bgm",
            "music",
            "future-category",
        ] {
            let item = Decoration {
                search: SearchIndex::new("", ""),
                id: "1".into(),
                name: String::new(),
                category: category.into(),
                image: None,
            };
            let canonical = decoration_category(category);
            assert!(item.matches_category(canonical));
            assert!(item.matches_category(""));
            assert!(!item.matches_category("tile-back"));
            assert_eq!(
                crate::text::decoration_name(category),
                crate::text::decoration_name(canonical)
            );
        }
    }
    use crate::account::AccountSnapshot;
    #[test]
    fn title_aliases_preserve_ownership_and_business_id_search() {
        let table = serde_json::json!({"entries": {"shared-image": {
            "id": "305563", "name": "冠军", "bindings": [
                {"table": "item_definition.title", "id": 600052, "name": "冠军"},
                {"table": "item_definition.title", "id": 600079, "name": "冠军"},
                {"table": "item_definition.item", "id": 305563, "name": "图标"}
            ]
        }}});
        let titles = title_entries(Path::new("."), &table);
        assert_eq!(titles.len(), 1);
        assert_eq!(titles[0].ids, ["600052", "600079"]);
        assert!(titles[0].search.matches("600079"));
        let mut account = AccountSnapshot::demo(&Library::default());
        account.owned_titles.insert("600079".into());
        assert!(account.owns_title(&titles[0]));
    }
    #[test]
    fn loads_the_local_export_package() -> Result<()> {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(".local/target/debug/majsoul-gallery-data/materials");
        if !path.is_dir() {
            return Ok(());
        }
        let lib = Library::load(&path)?;
        ensure!(!lib.characters.is_empty(), "missing characters");
        for rank in ["10401", "20301"] {
            ensure!(
                lib.ranks
                    .get(rank)
                    .and_then(|r| r.icon.as_ref())
                    .is_some_and(|p| p.is_file()),
                "missing account rank icon {rank}"
            );
        }
        let ichihime = lib
            .characters
            .iter()
            .find(|c| c.id == "200001")
            .context("missing Ichihime")?;
        let contract = ichihime
            .skins
            .iter()
            .find(|s| s.name.ends_with("契约"))
            .context("missing contract skin")?;
        assert_eq!(contract.display_name(&ichihime.name), "契约");
        ensure!(
            lib.characters.iter().any(|c| c.thumbnail().is_some()),
            "missing thumbnails"
        );
        ensure!(!lib.decorations.is_empty(), "missing decorations");
        ensure!(
            lib.decorations
                .iter()
                .any(|d| d.category == "background-icon"),
            "missing lobby backgrounds"
        );
        ensure!(
            lib.decorations.iter().any(|d| d.category == "hand"),
            "missing hands"
        );
        let songs = lib
            .decorations
            .iter()
            .filter(|d| matches!(d.category.as_str(), "bgm" | "riichi-bgm"))
            .collect::<Vec<_>>();
        ensure!(
            songs.iter().any(|d| d.name == "一触即发"),
            "missing individual riichi music"
        );
        ensure!(
            songs.iter().all(|d| d.image.is_some()),
            "music icon mapping missing"
        );
        ensure!(
            songs
                .iter()
                .map(|d| &d.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == songs.len(),
            "duplicate music business IDs"
        );
        ensure!(
            lib.characters.iter().any(|c| c.sex == 1) && lib.characters.iter().any(|c| c.sex == 2),
            "missing character pool classification"
        );
        assert!(resolve(&lib.root, "../manifest.json").is_none());
        Ok(())
    }
}
