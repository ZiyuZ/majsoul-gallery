//! Resolve decoration directories by exact, locale-specific thumbnail pixels.
use super::{
    Catalog, Library, Resource,
    index::{data_dir, digest, fingerprint},
    names::Binding,
    preview,
};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
#[derive(Default, Serialize, Deserialize)]
struct Cache {
    hashes: BTreeMap<String, String>,
}
fn locale(resource: &Resource) -> Option<&str> {
    if !resource.path.contains("/pic/") {
        return None;
    }
    ["chs_t", "chs", "common", "jp", "en", "kr"]
        .into_iter()
        .find(|language| resource.path.contains(&format!("/pic/{language}/")))
}
fn pixels(library: &Library, resource: &Resource, cache: &mut Cache) -> Result<String> {
    let bundle = &library.index.bundles[&resource.bundle];
    let stamp = fingerprint(&library.index.source.join(&bundle.name))?;
    let key = digest(format!(
        "alias-pixels-v1|{}|{}|{}|{stamp:?}",
        library.index.source.display(),
        library.index.unity,
        resource.id
    ));
    if let Some(hash) = cache.hashes.get(&key) {
        return Ok(hash.clone());
    }
    let decoded = preview::decode(library, resource)?;
    let rgba = image::open(decoded.path)?.into_rgba8();
    let hash = digest(
        [
            rgba.width().to_le_bytes().as_slice(),
            rgba.height().to_le_bytes().as_slice(),
            rgba.as_raw(),
        ]
        .concat(),
    );
    cache.hashes.insert(key, hash.clone());
    Ok(hash)
}
pub fn apply(library: &Library, catalog: &mut Catalog) -> Result<usize> {
    let path = data_dir().join("names").join("thumbnail-pixels-v1.json");
    let original = fs::read(&path).ok();
    let mut cache: Cache = original
        .clone()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let eligible = [
        "win-effect",
        "riichi-effect",
        "stick",
        "table",
        "tile-back",
        "tile-face",
        "frame",
        "call-effect",
        "hand",
        "background-icon",
    ];
    let categories = catalog
        .groups
        .iter()
        .filter(|g| {
            g.bindings.is_empty()
                && eligible.contains(&g.category.as_str())
                && g.resources.iter().any(|r| locale(r).is_some())
        })
        .map(|g| g.category.clone())
        .collect::<BTreeSet<_>>();
    let mut known = BTreeMap::<(String, String, String), Vec<Binding>>::new();
    let mut ambiguous = BTreeSet::new();
    for group in &catalog.groups {
        if !categories.contains(&group.category) || group.bindings.is_empty() {
            continue;
        }
        for resource in &group.resources {
            let Some(language) = locale(resource) else {
                continue;
            };
            let Ok(hash) = pixels(library, resource, &mut cache) else {
                continue;
            };
            let key = (group.category.clone(), language.into(), hash);
            if let Some(existing) = known.get(&key) {
                let identity = |bindings: &[Binding]| {
                    bindings
                        .iter()
                        .map(|b| (b.table.clone(), b.id))
                        .collect::<BTreeSet<_>>()
                };
                if identity(existing) != identity(&group.bindings) {
                    ambiguous.insert(key.clone());
                }
            } else {
                known.insert(key, group.bindings.clone());
            }
        }
    }
    let mut count = 0;
    for group in &mut catalog.groups {
        if !group.bindings.is_empty() || !categories.contains(&group.category) {
            continue;
        }
        let mut matches = Vec::new();
        for resource in &mut group.resources {
            let Some(language) = locale(resource) else {
                continue;
            };
            let Ok(hash) = pixels(library, resource, &mut cache) else {
                continue;
            };
            let key = (group.category.clone(), language.into(), hash.clone());
            if ambiguous.contains(&key) {
                continue;
            }
            if let Some(bindings) = known.get(&key) {
                resource.bindings = bindings
                    .iter()
                    .cloned()
                    .map(|mut b| {
                        b.via = format!(
                            "同分类／{language} 图标像素一致（SHA256 {hash}）→ {}",
                            b.via
                        );
                        b
                    })
                    .collect();
                matches.extend(resource.bindings.clone());
                catalog
                    .resources
                    .insert(resource.id.clone(), resource.clone());
            }
        }
        let identities = matches
            .iter()
            .filter(|b| b.table == "item_definition.item")
            .map(|b| b.id)
            .collect::<BTreeSet<_>>();
        if identities.len() == 1 {
            group.label = matches
                .iter()
                .find(|b| b.table == "item_definition.item")
                .unwrap()
                .name
                .clone();
            group.bindings = matches;
            count += 1;
        }
    }
    let bytes = serde_json::to_vec(&cache)?;
    if original.as_deref() == Some(bytes.as_slice()) {
        return Ok(count);
    }
    fs::create_dir_all(path.parent().unwrap())?;
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap())?;
    std::io::Write::write_all(temp.as_file_mut(), &bytes)?;
    temp.persist(path)?;
    Ok(count)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn local_gintama_aliases_are_proven_by_pixels() -> Result<()> {
        let library = Library::open(None)?;
        let names = super::super::names::Names::load(&library)?;
        let mut catalog = Catalog::build(&library);
        names.apply(&mut catalog);
        apply(&library, &mut catalog)?;
        for (folder, name) in [
            ("25yh_ron", "谢幕"),
            ("25yh_liqi_effect", "伊丽莎白"),
            ("25yh_liqi", "志村新八"),
            ("25yh_pai", "Just a way"),
            ("25yh_headframe", "伊丽莎白"),
        ] {
            let g = catalog
                .groups
                .iter()
                .find(|g| g.path.ends_with(&format!("/{folder}")))
                .unwrap();
            assert!(g.label.contains(name), "{}: {}", folder, g.label);
        }
        let g = catalog
            .groups
            .iter()
            .find(|g| g.path.ends_with("/25yh_headframe"))
            .unwrap();
        assert!(
            g.resources
                .iter()
                .filter(|r| r.path.contains("/chs_t/"))
                .all(|r| r.bindings.is_empty())
        );
        Ok(())
    }
}
