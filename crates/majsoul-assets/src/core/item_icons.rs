//! Resolve item icons through the client's atlas page map and named Sprites.
use super::{
    Library,
    characters::NamedItem,
    index::{cache_dir, digest, fingerprint},
    names::{Names, safe_bundle},
};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use std::{collections::BTreeMap, fs};
use unity_rs_core::{sprite::SpriteReadLimits, texture::TextureReadLimits};

#[derive(Serialize)]
pub struct ItemIcon {
    pub cached: bool,
    pub item_id: i64,
    pub path: Option<String>,
    pub error: Option<String>,
}
/// Resolve dependencies without decoding the texture page.
pub fn icon_bundles(library: &Library, names: &Names, icon: &str) -> Result<(String, String)> {
    let logical = icon.trim_start_matches('/').to_lowercase();
    let logical = logical.strip_prefix("myassets/").unwrap_or(&logical);
    let directory = logical.rsplit_once('/').context("图标目录无效")?.0;
    let stem = std::path::Path::new(icon)
        .file_stem()
        .and_then(|s| s.to_str())
        .context("物品图标名称无效")?;
    // The client uses common icons unless a localized page explicitly owns the name.
    let mut resolved = None;
    for locale in ["chs", "common"] {
        let prefix = format!("myassets/{directory}/{locale}/");
        let Some((map_path, bundle)) = names
            .icon_assets
            .iter()
            .find(|(p, _)| p.starts_with(&prefix) && p.ends_with("_config.json"))
        else {
            continue;
        };
        let map = super::bundles::open(library, bundle)?.json(map_path)?;
        if let Some(page) = map.get(stem).and_then(Value::as_u64) {
            let page_path = format!("{}_{page}.png", map_path.trim_end_matches("_config.json"));
            let page_bundle = names
                .icon_assets
                .get(&page_path)
                .context("物品图集页面未在清单中")?;
            resolved = Some((bundle.clone(), page_bundle.clone()));
            break;
        }
    }
    resolved.context("本地物品图集未找到此图标")
}
pub fn decode_items(library: &Library, names: &Names, items: &[NamedItem]) -> Vec<ItemIcon> {
    let mut seen = BTreeMap::new();
    for item in items {
        seen.entry(item.id).or_insert(item);
    }
    seen.into_values()
        .map(|item| {
            let result = (|| -> Result<(String, bool)> {
                let logical = item.icon.trim_start_matches('/').to_lowercase();
                let logical = logical.strip_prefix("myassets/").unwrap_or(&logical);
                ensure!(
                    logical.starts_with("extendres/items/")
                        || logical.starts_with("ui/common/main/pic/"),
                    "此图标尚不支持该路径：{logical}"
                );
                let stem = std::path::Path::new(&item.icon)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .context("物品图标名称无效")?;
                let (map_bundle, page_bundle) = icon_bundles(library, names, &item.icon)?;
                let cache = cache_dir().join(format!(
                    "{}.png",
                    digest(format!(
                        "item-sprite-v1|{}|{}|{}|{:?}|{}|{:?}|{}",
                        library.index.source.display(),
                        library.index.unity,
                        logical,
                        fingerprint(&safe_bundle(&library.index.source, &map_bundle)?)?,
                        page_bundle,
                        fingerprint(&safe_bundle(&library.index.source, &page_bundle)?)?,
                        stem
                    ))
                ));
                if image::image_dimensions(&cache).is_ok() {
                    return Ok((cache.to_string_lossy().into(), true));
                }
                let studio = super::bundles::open(library, &page_bundle)?;
                let objects = studio
                    .objects()
                    .filter(|o| o.class_id() == 213 && o.name() == Some(stem))
                    .collect::<Vec<_>>();
                ensure!(objects.len() == 1, "物品图集 Sprite 不存在或不唯一：{stem}");
                let rgba = objects[0]
                    .decode_sprite(SpriteReadLimits::default(), TextureReadLimits::default())?;
                let image = image::RgbaImage::from_raw(rgba.width, rgba.height, rgba.pixels)
                    .context("物品图标像素无效")?;
                fs::create_dir_all(cache_dir())?;
                let mut temp = tempfile::NamedTempFile::new_in(cache_dir())?;
                image.write_to(temp.as_file_mut(), image::ImageFormat::Png)?;
                temp.persist(&cache)?;
                Ok((cache.to_string_lossy().into(), false))
            })();
            match result {
                Ok((path, cached)) => ItemIcon {
                    cached,
                    item_id: item.id,
                    path: Some(path),
                    error: None,
                },
                Err(e) => ItemIcon {
                    cached: false,
                    item_id: item.id,
                    path: None,
                    error: Some(format!("{e:#}")),
                },
            }
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn all_character_gift_and_contract_icons_decode() -> Result<()> {
        let library = Library::open(None)?;
        let names = Names::load(&library)?;
        let characters = super::super::characters::extract(&names)?;
        let items = characters
            .into_values()
            .flat_map(|c| {
                c.gifts
                    .into_iter()
                    .chain(c.contract_materials.into_iter().flat_map(|r| r.items))
            })
            .collect::<Vec<_>>();
        let icons = decode_items(&library, &names, &items);
        ensure!(!icons.is_empty(), "No icons");
        for icon in &icons {
            ensure!(icon.error.is_none(), "{}: {:?}", icon.item_id, icon.error);
            let dimensions = image::image_dimensions(icon.path.as_ref().unwrap())?;
            ensure!(
                dimensions.0 > 0 && dimensions.0 <= 512 && dimensions.1 > 0 && dimensions.1 <= 512,
                "Wrong icon canvas: {dimensions:?}"
            );
            let image = image::open(icon.path.as_ref().unwrap())?.to_rgba8();
            ensure!(
                image.pixels().any(|p| p[3] > 0),
                "Empty item icon: {}",
                icon.item_id
            );
        }
        println!("Decoded {} distinct item IDs", icons.len());
        Ok(())
    }
}
