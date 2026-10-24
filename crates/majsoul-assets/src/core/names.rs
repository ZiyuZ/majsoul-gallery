//! Local configuration decoding and traceable display-name relationships.
use super::{
    Catalog, Library,
    index::{data_dir, digest, fingerprint},
    lua,
};
use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
use anyhow::{Context, Result, ensure};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};
use unity_rs_core::{loader::AssetLoadOptions, studio::Studio};

const TABLES: &[&str] = &[
    "character",
    "skin",
    "item",
    "view",
    "title",
    "loading_image",
];
const EXTRA_TABLES: &[(&str, &str)] = &[
    ("audio", "bgm"),
    ("audio", "audio"),
    ("spot", "skin_spot"),
    ("spot", "character_spot"),
    ("level_definition", "level_definition"),
];
#[derive(Clone, Serialize, Deserialize)]
pub struct Binding {
    pub table: String,
    pub id: i64,
    pub name: String,
    pub via: String,
}
#[derive(Serialize, Deserialize)]
pub struct Names {
    signature: String,
    pub tables: BTreeMap<String, Vec<Value>>,
    pub audio_assets: BTreeMap<String, String>,
    pub icon_assets: BTreeMap<String, String>,
    pub spine_assets: BTreeMap<String, String>,
}
pub fn decode(payload: &[u8]) -> Result<Vec<u8>> {
    let mut encrypted = STANDARD.decode(
        payload
            .iter()
            .copied()
            .filter(|b| !b.is_ascii_whitespace())
            .collect::<Vec<_>>(),
    )?;
    let plain =
        cbc::Decryptor::<aes::Aes128>::new(b"wrelupqezdfrqdsd".into(), b"sfaqweertfvfdxad".into())
            .decrypt_padded_mut::<Pkcs7>(&mut encrypted)
            .map_err(|_| anyhow::anyhow!("本地配置 AES 校验失败；可能是不同版本的编码"))?;
    Ok(plain.to_vec())
}
pub fn cache_path(library: &Library) -> PathBuf {
    data_dir().join("names").join(format!(
        "{}.json",
        digest(library.index.source.to_string_lossy().as_bytes())
    ))
}
pub fn safe_bundle(source: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        Path::new(name).components().count() == 1
            && matches!(
                Path::new(name).components().next(),
                Some(Component::Normal(_))
            ),
        "非法配置包路径"
    );
    Ok(source.join(name))
}
fn manifest(
    value: &Value,
    prefix: &str,
    bundles: &[String],
    out: &mut BTreeMap<String, String>,
) -> Result<()> {
    for (key, value) in value.as_object().context("资源目录结构无效")? {
        if key == "/f/" {
            for file in value.as_array().context("资源文件列表无效")? {
                let (name, index) = file
                    .as_str()
                    .context("资源文件名无效")?
                    .rsplit_once("_!_")
                    .context("资源包引用无效")?;
                let bundle = bundles
                    .get(index.parse::<usize>()?)
                    .context("资源包索引越界")?;
                out.insert(
                    format!("{prefix}/{name}")
                        .trim_start_matches('/')
                        .to_lowercase(),
                    bundle.clone(),
                );
            }
        } else {
            manifest(value, &format!("{prefix}/{key}"), bundles, out)?;
        }
    }
    Ok(())
}
pub fn asset_manifest(source: &Path) -> Result<BTreeMap<String, String>> {
    let path = source.join("AssetBundleConfig.json");
    ensure!(
        fs::metadata(&path)?.len() < 32 * 1024 * 1024,
        "资产清单过大"
    );
    let config: Value = serde_json::from_slice(&decode(&fs::read(path)?)?)?;
    let bundles = config["BundlesInfo"]
        .as_object()
        .context("缺少 BundlesInfo")?
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    let mut assets = BTreeMap::new();
    manifest(&config["AssetsInfo"], "", &bundles, &mut assets)?;
    Ok(assets)
}
fn needed(path: &str) -> bool {
    let Some(name) = path.rsplit('/').next() else {
        return false;
    };
    TABLES
        .iter()
        .map(|t| ("item_definition", *t))
        .chain(EXTRA_TABLES.iter().copied())
        .any(|(domain, table)| {
            if path.starts_with("luabyte/lua/excels/langs/") {
                name == format!("{domain}_{table}_name_chs.lua.bytes")
                    || name == format!("{domain}_{table}_full_name_chs.lua.bytes")
            } else {
                path.starts_with(&format!("luabyte/lua/excels/data/{domain}/"))
                    && (name == format!("{table}.lua.bytes")
                        || name.starts_with(&format!("{table}_b")))
            }
        })
}
impl Names {
    pub fn load(library: &Library) -> Result<Self> {
        let config_path = library.index.source.join("AssetBundleConfig.json");
        ensure!(
            fs::metadata(&config_path)?.len() < 32 * 1024 * 1024,
            "资源清单过大"
        );
        // preserve_order is essential: _!_ references the insertion order of BundlesInfo.
        let config: Value = serde_json::from_slice(&decode(&fs::read(&config_path)?)?)?;
        let bundles = config["BundlesInfo"]
            .as_object()
            .context("缺少 BundlesInfo")?
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut assets = BTreeMap::new();
        manifest(&config["AssetsInfo"], "", &bundles, &mut assets)?;
        let required = assets.iter().filter(|(p, _)| needed(p)).collect::<Vec<_>>();
        ensure!(!required.is_empty(), "本地清单未找到名称配置");
        let bundle_names = required
            .iter()
            .map(|(_, b)| b.as_str())
            .collect::<BTreeSet<_>>();
        let mut signature = format!(
            "local-names-v4|{}|{}|{:?}",
            library.index.source.display(),
            library.index.unity,
            fingerprint(&config_path)?
        );
        for name in &bundle_names {
            signature.push_str(&format!(
                "|{name}|{:?}",
                fingerprint(&safe_bundle(&library.index.source, name)?)?
            ));
        }
        let signature = digest(signature);
        let cache = cache_path(library);
        if let Ok(bytes) = fs::read(&cache)
            && let Ok(value) = serde_json::from_slice::<Self>(&bytes)
            && value.signature == signature
        {
            return Ok(value);
        }
        let mut sources = BTreeMap::new();
        for name in bundle_names {
            let options = AssetLoadOptions {
                unity_version_override: Some(library.index.unity.parse()?),
                ..Default::default()
            };
            let studio =
                Studio::open_with_options(safe_bundle(&library.index.source, name)?, options)?;
            for object in studio.objects().filter(|o| o.class_id() == 49) {
                let Some(path) = object.container() else {
                    continue;
                };
                let path = path.to_lowercase();
                if !needed(&path) {
                    continue;
                }
                let source =
                    String::from_utf8(decode(&object.read_text_bytes(16 * 1024 * 1024)?)?)?;
                ensure!(
                    source.starts_with("local "),
                    "配置未还原为预期 Lua 数据：{path}"
                );
                sources.insert(path, source);
            }
        }
        for (path, _) in required {
            ensure!(
                sources.contains_key(path),
                "配置清单对应的 TextAsset 缺失：{path}"
            );
        }
        let mut languages = BTreeMap::new();
        for (path, source) in &sources {
            if path.contains("/langs/") {
                languages.insert(
                    path.rsplit('/')
                        .next()
                        .unwrap()
                        .trim_end_matches(".lua.bytes")
                        .to_owned(),
                    lua::language(source)?,
                );
            }
        }
        let mut tables = BTreeMap::new();
        for (domain, table) in TABLES
            .iter()
            .map(|t| ("item_definition", *t))
            .chain(EXTRA_TABLES.iter().copied())
        {
            let stem = format!("luabyte/lua/excels/data/{domain}/{table}");
            let schema = sources
                .get(&format!("{stem}.lua.bytes"))
                .with_context(|| format!("缺少 {domain}.{table} 表入口"))?;
            let blocks = sources
                .iter()
                .filter(|(p, _)| {
                    *p == &format!("{stem}.lua.bytes") || p.starts_with(&format!("{stem}_b"))
                })
                .map(|(_, s)| s.as_str())
                .collect::<Vec<_>>();
            let rows = lua::sheet(schema, &blocks, &languages, &format!("{domain}_{table}"))
                .with_context(|| format!("解析 {domain}.{table}"))?;
            ensure!(!rows.is_empty(), "配置表为空：{domain}.{table}");
            tables.insert(format!("{domain}.{table}"), rows);
        }
        let value = Self {
            signature,
            tables,
            icon_assets: assets
                .iter()
                .filter(|(p, _)| {
                    p.starts_with("myassets/extendres/items/")
                        || p.starts_with("myassets/ui/common/main/pic/")
                })
                .map(|(p, b)| (p.clone(), b.clone()))
                .collect(),
            spine_assets: assets
                .iter()
                .filter(|(p, _)| p.starts_with("myassets/spine/"))
                .map(|(p, b)| (p.clone(), b.clone()))
                .collect(),
            audio_assets: assets
                .into_iter()
                .filter(|(p, _)| {
                    p.starts_with("myassets/")
                        && [".mp3", ".ogg", ".wav"].iter().any(|ext| p.ends_with(ext))
                })
                .collect(),
        };
        fs::create_dir_all(cache.parent().context("名称缓存目录无效")?)?;
        let mut temp = tempfile::NamedTempFile::new_in(cache.parent().unwrap())?;
        serde_json::to_writer(temp.as_file_mut(), &value)?;
        temp.persist(cache)?;
        Ok(value)
    }
    pub fn rows(&self, table: &str) -> &[Value] {
        self.tables
            .get(table)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }
    pub fn apply(&self, catalog: &mut Catalog) -> usize {
        let characters = self
            .rows("item_definition.character")
            .iter()
            .chain(self.rows("spot.character_spot"))
            .filter_map(|r| Some((r["id"].as_i64()?, r)))
            .collect::<BTreeMap<_, _>>();
        let views = self
            .rows("item_definition.view")
            .iter()
            .filter_map(|r| Some((r["id"].as_i64()?, r)))
            .collect::<BTreeMap<_, _>>();
        let mut icons = BTreeMap::<String, Vec<&Value>>::new();
        let mut families = BTreeMap::<String, Vec<&Value>>::new();
        for item in self.rows("item_definition.item") {
            icons
                .entry(canonical_icon(&normalize(text(item, "icon"))))
                .or_default()
                .push(item);
            if let Some(view) = item["id"].as_i64().and_then(|id| views.get(&id)) {
                let res = text(view, "res_name").to_lowercase();
                if !res.is_empty() {
                    families.entry(res).or_default().push(item);
                }
            }
        }
        let skins = self.skins();
        for group in &mut catalog.groups {
            let folder = group.path.rsplit('/').next().unwrap_or("");
            if matches!(group.category.as_str(), "character" | "spine") {
                for (table, skin) in skins.iter().copied().filter(|(_, s)| {
                    if group.category == "character" {
                        skin_directory(s) == group.path
                    } else {
                        s["id"].as_i64().is_some_and(|id| id.to_string() == folder)
                            && s["spine_type"].as_i64().unwrap_or(0) > 0
                    }
                }) {
                    let mut name = text(skin, "name_chs").to_owned();
                    if let Some(character) = skin["character_id"]
                        .as_i64()
                        .and_then(|id| characters.get(&id))
                    {
                        let who = text(character, "name_chs");
                        name = if table == "spot.skin_spot" {
                            name
                        } else if character["init_skin"] == skin["id"] {
                            format!("{who} · 初始形象")
                        } else if who == name {
                            name
                        } else {
                            format!("{who} · {name}")
                        };
                    }
                    add(
                        &mut group.bindings,
                        table,
                        skin,
                        &name,
                        if group.category == "character" {
                            "SkinTool.GetSkinIconDir：skin_version=1 用 path，否则用完整 skin.id；角色按配置外键关联"
                        } else {
                            "skin.id → Spine 数字目录，且 spine_type > 0；仅关联原料身份"
                        },
                    );
                    group.story = table == "spot.skin_spot";
                    if let Some(character) = skin["character_id"]
                        .as_i64()
                        .and_then(|id| characters.get(&id))
                    {
                        group.parent = Some(super::catalog::GroupParent {
                            id: format!("character:{}", skin["character_id"]),
                            label: text(character, "name_chs").into(),
                        });
                    }
                }
            }
            let mut candidates = BTreeMap::new();
            for resource in &mut group.resources {
                if let Some(items) = icons.get(&canonical_icon(&resource.path)) {
                    for item in items {
                        candidates.insert(item["id"].as_i64().unwrap(), (*item, true));
                        add(
                            &mut resource.bindings,
                            "item_definition.item",
                            item,
                            text(item, "name_chs"),
                            "item.icon → 图片路径",
                        );
                    }
                }
            }
            if matches!(
                group.category.as_str(),
                "hand"
                    | "frame"
                    | "stick"
                    | "tile-back"
                    | "tile-face"
                    | "table"
                    | "background"
                    | "background-icon"
                    | "win-effect"
                    | "riichi-effect"
                    | "call-effect"
            ) && let Some(items) = families.get(folder)
            {
                for item in items {
                    candidates
                        .entry(item["id"].as_i64().unwrap())
                        .or_insert((*item, false));
                }
            }
            for (item, direct) in candidates.into_values() {
                add(
                    &mut group.bindings,
                    "item_definition.item",
                    item,
                    text(item, "name_chs"),
                    if direct {
                        "item.icon → 图片路径"
                    } else {
                        "item.id → view.id → view.res_name → 资源目录"
                    },
                );
            }
            if group.category == "hand" {
                for character in characters.values() {
                    if character["hand"]
                        .as_i64()
                        .and_then(|id| views.get(&id))
                        .is_some_and(|v| text(v, "res_name").eq_ignore_ascii_case(folder))
                    {
                        add(
                            &mut group.bindings,
                            "item_definition.character",
                            character,
                            &format!("{} · 默认手模", text(character, "name_chs")),
                            "character.hand → view.id → view.res_name",
                        );
                    }
                }
            }
            if group.category == "title" {
                for title in self.rows("item_definition.title") {
                    if ["icon", "icon_item"].iter().any(|field| {
                        normalize(text(title, field)).starts_with(&format!("{}/", group.path))
                    }) {
                        add(
                            &mut group.bindings,
                            "item_definition.title",
                            title,
                            text(title, "name_chs"),
                            "title.icon / title.icon_item → 资源目录",
                        );
                    }
                }
            }
            if group.category == "loading" {
                for cg in self.rows("item_definition.loading_image") {
                    if ["img_path", "thumb_path"].iter().any(|field| {
                        normalize(text(cg, field)).starts_with(&format!("{}/", group.path))
                    }) {
                        for id in cg["unlock_items"].as_array().into_iter().flatten() {
                            if let Some(item) = self
                                .rows("item_definition.item")
                                .iter()
                                .find(|item| item["id"] == *id)
                            {
                                add(
                                    &mut group.bindings,
                                    "item_definition.item",
                                    item,
                                    text(item, "name_chs"),
                                    "loading_image.img_path / thumb_path → 目录；unlock_items → item.id",
                                );
                            }
                        }
                    }
                }
            }
            if !group.bindings.is_empty() {
                let names = group
                    .bindings
                    .iter()
                    .map(|b| b.name.as_str())
                    .collect::<BTreeSet<_>>();
                group.label = if names.len() > 3 {
                    format!("{} 等 {} 项", names.first().unwrap(), names.len())
                } else {
                    names.into_iter().collect::<Vec<_>>().join(" / ")
                };
                if group.story {
                    group.label.push_str(" · 剧情形象");
                }
            }
            for resource in &mut group.resources {
                if resource.bindings.is_empty() {
                    resource.bindings = group.bindings.clone();
                }
                if let Some(original) = catalog.resources.get_mut(&resource.id) {
                    original.bindings = resource.bindings.clone();
                }
            }
        }
        catalog
            .groups
            .iter()
            .filter(|g| !g.bindings.is_empty())
            .count()
    }

    pub fn apply_characters(&self, library: &Library, catalog: &mut Catalog) -> Result<()> {
        let characters = super::characters::extract(self)?;
        for group in &mut catalog.groups {
            if let Some(binding) = group
                .bindings
                .iter()
                .find(|b| b.table == "item_definition.skin" || b.table == "spot.skin_spot")
            {
                let character_id = self
                    .rows(&binding.table)
                    .iter()
                    .find(|s| s["id"].as_i64() == Some(binding.id))
                    .and_then(|s| s["character_id"].as_i64());
                group.character = character_id.and_then(|id| characters.get(&id)).cloned();
                // Story skins of a regular character share its official ordering.
                // Story-only NPCs use their own character_spot entry.
                group.character_sort =
                    group.character.as_ref().and_then(|c| c.sort).or_else(|| {
                        character_id.and_then(|id| {
                            self.rows("spot.character_spot")
                                .iter()
                                .find(|r| r["id"].as_i64() == Some(id))
                                .and_then(|r| r["sort"].as_i64())
                        })
                    });
            }
        }
        let path = cache_path(library).with_extension("characters.json");
        let bytes = serde_json::to_vec_pretty(&characters)?;
        if fs::read(&path).ok().as_deref() != Some(bytes.as_slice()) {
            let mut temp =
                tempfile::NamedTempFile::new_in(path.parent().context("角色资料缓存目录无效")?)?;
            use std::io::Write;
            temp.write_all(&bytes)?;
            temp.persist(path)?;
        }
        Ok(())
    }
    fn skins(&self) -> Vec<(&str, &Value)> {
        let mut values = BTreeMap::new();
        for table in ["item_definition.skin", "spot.skin_spot"] {
            for row in self.rows(table) {
                if let Some(id) = row["id"].as_i64() {
                    values.insert(id, (table, row));
                }
            }
        }
        values.into_values().collect()
    }
}
fn skin_directory(skin: &Value) -> String {
    if skin["skin_version"].as_i64() == Some(1) {
        normalize(text(skin, "path"))
    } else {
        format!("myassets/deco/character/{}", skin["id"])
    }
}
fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or("")
}
fn normalize(path: &str) -> String {
    let path = path.trim_start_matches('/').to_lowercase();
    if path.starts_with("myassets/") {
        path
    } else {
        format!("myassets/{path}")
    }
}
fn canonical_icon(path: &str) -> String {
    ["common", "chs", "chs_t", "jp", "en", "kr"]
        .iter()
        .fold(path.to_owned(), |path, locale| {
            path.replace(&format!("/pic/{locale}/"), "/pic/")
        })
}
fn add(bindings: &mut Vec<Binding>, table: &str, row: &Value, name: &str, via: &str) {
    if let Some(id) = row["id"].as_i64()
        && !name.is_empty()
        && !bindings.iter().any(|b| b.table == table && b.id == id)
    {
        bindings.push(Binding {
            table: table.into(),
            id,
            name: name.into(),
            via: via.into(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manifest_uses_original_bundle_order() -> Result<()> {
        let config: Value = serde_json::from_str(
            r#"{"BundlesInfo":{"z-new.majset":{},"a-old.majset":{}},"AssetsInfo":{"LuaByte":{"Lua":{"/f/":["table.lua.bytes_!_0","lang.lua.bytes_!_1"]}}}}"#,
        )?;
        let bundles = config["BundlesInfo"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let mut assets = BTreeMap::new();
        manifest(&config["AssetsInfo"], "", &bundles, &mut assets)?;
        assert_eq!(assets["luabyte/lua/table.lua.bytes"], "z-new.majset");
        assert_eq!(assets["luabyte/lua/lang.lua.bytes"], "a-old.majset");
        Ok(())
    }
    #[test]
    #[ignore = "requires the local installation and saved index"]
    fn local_configuration_names_multiple_asset_categories() -> Result<()> {
        let library = Library::open(None)?;
        let names = Names::load(&library)?;
        for (table, rows) in &names.tables {
            println!("{table}: {} rows", rows.len());
        }
        let mut catalog = Catalog::build(&library);
        println!("named groups: {}", names.apply(&mut catalog));
        names.apply_characters(&library, &mut catalog)?;
        let profiles = super::super::characters::extract(&names)?;
        assert_eq!(profiles.len(), 124);
        let ichihime = profiles.get(&200001).context("missing character")?;
        assert!(ichihime.gifts.iter().any(|g| g.name == "香喷喷曲奇"));
        assert_eq!(ichihime.contract_materials.len(), 5);
        assert_eq!(ichihime.contract_materials[2].items.len(), 2);
        assert_eq!(ichihime.contract_materials[2].quantity, 5);
        for category in &catalog.categories {
            let matched = catalog
                .groups
                .iter()
                .filter(|g| g.category == category.id && !g.bindings.is_empty())
                .count();
            println!("{}: {matched}/{}", category.id, category.groups);
        }
        for (query, expected) in [
            ("jiutiao", "九条璃雨"),
            ("xiaoniaoyouxingye_qp", "华芳雅韵"),
            ("tablecloth_newyear", "新春桌布"),
            ("liqi_fish", "咸鱼立直棒"),
            ("ron_fire", "烈焰"),
        ] {
            let group = catalog
                .groups
                .iter()
                .find(|g| g.path.ends_with(&format!("/{query}")))
                .context("Missing sample group")?;
            ensure!(group.label.contains(expected), "{query}: {}", group.label);
            println!("{query}: {}", group.label);
        }
        assert!(catalog.page("character", "九条璃雨", 0, false).total > 0);
        assert!(catalog.page("win-effect", "烈焰", 0, false).total > 0);
        assert!(!names.rows("audio.bgm").is_empty());
        Ok(())
    }
}
