//! Extract one exported Spine skeleton and its atlas pages, without reconstructing Unity effects.
use super::{
    Catalog, Library, Resource,
    index::{cache_dir, digest, fingerprint},
    names::{Names, safe_bundle},
    preview,
};
use anyhow::{Context, Result, bail, ensure};
use serde::Serialize;
use std::{collections::BTreeMap, fs};
#[derive(Serialize)]
pub struct SpinePreview {
    pub skeleton: String,
    pub atlas: String,
    pub pages: BTreeMap<String, String>,
    pub logical_path: String,
    pub version: String,
    pub format: String,
    pub premultiplied_alpha: bool,
    pub cached: bool,
}
fn text(
    library: &Library,
    names: &Names,
    logical: &str,
    extension: &str,
) -> Result<(Vec<u8>, String, bool)> {
    let bundle_name = names
        .spine_assets
        .get(logical)
        .with_context(|| format!("清单中缺少 {logical}"))?;
    let source = safe_bundle(&library.index.source, bundle_name)?;
    let stamp = fingerprint(&source)?;
    let indexed = library
        .index
        .bundles
        .get(bundle_name)
        .context("骨架资源包缺少索引，请刷新索引")?;
    ensure!(
        stamp == (indexed.size, indexed.modified),
        "骨架源资源已变化，请刷新索引"
    );
    let cache = cache_dir().join(format!(
        "{}.{}",
        digest(format!(
            "spine-text-v1|{}|{}|{logical}|{stamp:?}",
            library.index.source.display(),
            library.index.unity
        )),
        extension
    ));
    if cache.exists() {
        return Ok((fs::read(&cache)?, cache.to_string_lossy().into(), true));
    }
    let studio = super::bundles::open(library, bundle_name)?;
    // Studio metadata may give a dependency the container of its referring
    // SkeletonDataAsset. Resolve the AssetBundle's direct asset pointer instead.
    let mut target = None;
    for bundle in studio.objects().filter(|o| o.class_id() == 142) {
        let metadata = bundle
            .read_asset_bundle(unity_rs_core::serialized::ContainerMetadataReadLimits::default())?;
        if let Some(entry) = metadata
            .container
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(logical))
        {
            ensure!(
                entry.asset.file_id == 0,
                "骨架直接引用位于外部序列化文件，尚未支持"
            );
            target = studio.object(bundle.file_index(), entry.asset.path_id);
            break;
        }
    }
    let object = target.with_context(|| format!("找不到 {logical} 的直接资源引用"))?;
    ensure!(object.class_id() == 49, "骨架／图集引用不是 TextAsset");
    let bytes = object.read_text_bytes(64 * 1024 * 1024)?;
    fs::create_dir_all(cache_dir())?;
    let mut temp = tempfile::NamedTempFile::new_in(cache_dir())?;
    std::io::Write::write_all(temp.as_file_mut(), &bytes)?;
    temp.persist(&cache)?;
    Ok((bytes, cache.to_string_lossy().into(), false))
}
pub fn preview_texture(
    library: &Library,
    names: &Names,
    logical: &str,
) -> Result<preview::Preview> {
    let bundle_name = names
        .spine_assets
        .get(logical)
        .context("纹理不在本地清单中")?;
    let source = safe_bundle(&library.index.source, bundle_name)?;
    let stamp = fingerprint(&source)?;
    let indexed = library
        .index
        .bundles
        .get(bundle_name)
        .context("纹理包缺少索引")?;
    ensure!(
        stamp == (indexed.size, indexed.modified),
        "纹理源资源已变化，请刷新索引"
    );
    let cache = cache_dir().join(format!(
        "{}.png",
        digest(format!(
            "spine-direct-texture-v1|{}|{}|{logical}|{stamp:?}",
            library.index.source.display(),
            library.index.unity
        ))
    ));
    if cache.exists()
        && let Ok((width, height)) = image::image_dimensions(&cache)
    {
        return Ok(preview::Preview {
            media: "image".into(),
            path: cache.to_string_lossy().into(),
            width,
            height,
            kind: "Spine 原始图集纹理".into(),
            cached: true,
        });
    }
    let studio = super::bundles::open(library, bundle_name)?;
    let mut target = None;
    for bundle in studio.objects().filter(|o| o.class_id() == 142) {
        let metadata = bundle
            .read_asset_bundle(unity_rs_core::serialized::ContainerMetadataReadLimits::default())?;
        if let Some(entry) = metadata
            .container
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(logical))
        {
            ensure!(entry.asset.file_id == 0, "图集引用位于外部文件，尚未支持");
            target = studio.object(bundle.file_index(), entry.asset.path_id);
            break;
        }
    }
    let object = target.context("图集缺少直接 Texture2D 引用")?;
    ensure!(object.class_id() == 28, "图集引用不是 Texture2D");
    let rgba =
        object.decode_texture_mip(0, unity_rs_core::texture::TextureReadLimits::default())?;
    let (width, height) = (rgba.width, rgba.height);
    let mut image =
        image::RgbaImage::from_raw(width, height, rgba.pixels).context("无效纹理像素")?;
    image::imageops::flip_vertical_in_place(&mut image);
    fs::create_dir_all(cache_dir())?;
    let mut temp = tempfile::NamedTempFile::new_in(cache_dir())?;
    image.write_to(temp.as_file_mut(), image::ImageFormat::Png)?;
    temp.persist(&cache)?;
    Ok(preview::Preview {
        media: "image".into(),
        path: cache.to_string_lossy().into(),
        width,
        height,
        kind: "Spine 原始图集纹理".into(),
        cached: false,
    })
}
pub fn supplement_catalog(names: &Names, catalog: &mut Catalog) {
    use super::{catalog::Group, index::digest};
    let existing = catalog
        .resources
        .values()
        .map(|r| r.path.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for (path, bundle) in names
        .spine_assets
        .iter()
        .filter(|(p, _)| p.ends_with(".png") && !existing.contains(*p))
    {
        let folder = path
            .strip_prefix("myassets/spine/")
            .unwrap()
            .split('/')
            .next()
            .unwrap();
        let group_path = format!("myassets/spine/{folder}");
        let id = digest(format!("manifest-spine|{path}"));
        let resource = Resource {
            id: id.clone(),
            path: path.clone(),
            label: path.rsplit('/').next().unwrap().into(),
            role: "纹理".into(),
            class: "SpineTexture".into(),
            bindings: vec![],
            bundle: bundle.clone(),
            asset: None,
            item: None,
        };
        catalog.resources.insert(id, resource.clone());
        if let Some(group) = catalog.groups.iter_mut().find(|g| g.path == group_path) {
            group.resources.push(resource);
        } else {
            catalog.groups.push(Group {
                id: digest(format!("spine|{group_path}")),
                path: group_path,
                label: folder.into(),
                category: "spine".into(),
                resources: vec![resource],
                bindings: vec![],
                character: None,
                character_sort: None,
                parent: None,
                story: false,
            });
        }
    }
}
fn page_names(atlas: &str) -> Result<Vec<String>> {
    let mut new_page = true;
    let mut pages = Vec::new();
    for line in atlas.lines() {
        let line = line.trim();
        if line.is_empty() {
            new_page = true;
            continue;
        }
        if new_page {
            ensure!(
                !line.contains(['/', '\\', ':']) && line.to_lowercase().ends_with(".png"),
                "不支持的图集页名：{line}"
            );
            pages.push(line.to_owned());
            new_page = false;
        }
    }
    ensure!(!pages.is_empty(), "图集没有纹理页");
    Ok(pages)
}
fn prepare_atlas(
    library: &Library,
    names: &Names,
    _catalog: &Catalog,
    logical: &str,
) -> Result<SpinePreview> {
    let (atlas_bytes, atlas, atlas_cached) = text(library, names, logical, "atlas")?;
    let atlas_text = std::str::from_utf8(&atlas_bytes).context("图集描述不是 UTF-8")?;
    let directory = logical.rsplit_once('/').unwrap().0;
    let stem = logical.trim_end_matches(".atlas.txt");
    let binary = format!("{stem}.skel.txt");
    let json = format!("{stem}.json");
    let (skeleton_logical, format) = if names.spine_assets.contains_key(&binary) {
        (binary, "skel")
    } else if names.spine_assets.contains_key(&json) {
        (json, "json")
    } else {
        bail!("图集缺少对应的二进制或 JSON 骨架：{logical}");
    };
    let (bytes, skeleton, skeleton_cached) = text(library, names, &skeleton_logical, format)?;
    let version = if format == "json" {
        let value: serde_json::Value = serde_json::from_slice(&bytes)?;
        value["skeleton"]["spine"]
            .as_str()
            .context("JSON 缺少骨架版本")?
            .to_owned()
    } else {
        let header = String::from_utf8_lossy(&bytes[..bytes.len().min(128)]);
        regex::Regex::new(r"\d+\.\d+\.\d+")?
            .find(&header)
            .context("无法读取骨架版本")?
            .as_str()
            .to_owned()
    };
    ensure!(version.starts_with("4.2."), "暂不支持 Spine {version}");
    let mut pages = BTreeMap::new();
    let mut cached = atlas_cached && skeleton_cached;
    for name in page_names(atlas_text)? {
        let path = format!("{directory}/{}", name.to_lowercase());
        let decoded = preview_texture(library, names, &path)?;
        cached &= decoded.cached;
        pages.insert(name, decoded.path);
    }
    Ok(SpinePreview {
        skeleton,
        atlas,
        pages,
        logical_path: skeleton_logical,
        version,
        format: format.into(),
        cached,
        premultiplied_alpha: atlas_text
            .lines()
            .filter_map(|line| line.split_once(':'))
            .any(|(k, v)| k.trim() == "pma" && v.trim() == "true"),
    })
}
fn prefix(resource: &Resource) -> Result<String> {
    let relative = resource
        .path
        .strip_prefix("myassets/spine/")
        .context("请选择动态皮肤原料中的图片")?;
    let folder = relative.split('/').next().context("动态皮肤目录无效")?;
    Ok(format!("myassets/spine/{folder}/"))
}
pub fn prepare(
    library: &Library,
    names: &Names,
    catalog: &Catalog,
    resource: &Resource,
) -> Result<SpinePreview> {
    let prefix = prefix(resource)?;
    for logical in names
        .spine_assets
        .keys()
        .filter(|p| p.starts_with(&prefix) && p.ends_with(".atlas.txt"))
    {
        let (bytes, _, _) = text(library, names, logical, "atlas")?;
        let directory = logical.rsplit_once('/').unwrap().0;
        if page_names(std::str::from_utf8(&bytes)?)?
            .iter()
            .any(|name| format!("{directory}/{}", name.to_lowercase()) == resource.path)
        {
            return prepare_atlas(library, names, catalog, logical);
        }
    }
    bail!("此图片没有对应的 Spine 图集；可以尝试同皮肤的合成播放")
}
#[derive(Serialize)]
pub struct SpineScene {
    pub layers: Vec<SpinePreview>,
    pub backgrounds: Vec<super::spine_background::Background>,
    pub warnings: Vec<String>,
    pub order_source: String,
}
pub fn prepare_scene(
    library: &Library,
    names: &Names,
    catalog: &Catalog,
    resource: &Resource,
    composite: bool,
) -> Result<SpineScene> {
    if !composite {
        return Ok(SpineScene {
            layers: vec![prepare(library, names, catalog, resource)?],
            backgrounds: vec![],
            warnings: vec![],
            order_source: "所选图集".into(),
        });
    }
    let prefix = prefix(resource)?;
    let folder = prefix.trim_end_matches('/').rsplit('/').next().unwrap();
    let info_path = format!("{prefix}{folder}_info.json");
    let mut directories = Vec::new();
    let mut warnings = Vec::new();
    let order_source = if names.spine_assets.contains_key(&info_path) {
        let (bytes, _, _) = text(library, names, &info_path, "json")?;
        let info: serde_json::Value = serde_json::from_slice(&bytes)?;
        for layer in info["layers"]
            .as_array()
            .context("皮肤配置缺少 layers 数组")?
        {
            let layer = layer.as_str().context("皮肤层名称不是字符串")?;
            ensure!(
                !layer.is_empty() && !layer.contains(['/', '\\', ':']) && layer != "..",
                "非法皮肤层名称"
            );
            directories.push(format!("{prefix}{layer}/"));
        }
        ensure!(!directories.is_empty(), "皮肤 layers 配置为空");
        if info["isEffect"] == true {
            warnings.push("当前合成骨架图层，尚未包含额外场景粒子特效".into());
        }
        if info["isMask"] == true {
            warnings.push("此皮肤包含客户端场景遮罩，当前尚未重建该遮罩".into());
        }
        info_path
    } else {
        // Numbered character prefabs are loaded in ascending order;
        // the single-layer effect layout may contain only directory 2.
        let mut numbered = BTreeMap::new();
        for path in names
            .spine_assets
            .keys()
            .filter(|p| p.starts_with(&prefix) && p.ends_with("/character.prefab"))
        {
            let relative = path.strip_prefix(&prefix).unwrap();
            let parts = relative.split('/').collect::<Vec<_>>();
            if parts.len() == 2
                && let Ok(number) = parts[0].parse::<u32>()
            {
                numbered.insert(number, format!("{prefix}{}/", parts[0]));
            }
        }
        directories.extend(numbered.into_values());
        if directories.is_empty() {
            directories.push(prefix.clone());
        }
        "客户端编号 character.prefab 顺序".into()
    };
    let mut layers = Vec::new();
    for directory in directories {
        let atlases = names
            .spine_assets
            .keys()
            .filter(|p| {
                p.rsplit_once('/')
                    .is_some_and(|(dir, _)| format!("{dir}/") == directory)
                    && p.ends_with(".atlas.txt")
            })
            .collect::<Vec<_>>();
        if atlases.is_empty() {
            warnings.push(format!("{directory} 未找到骨架图集"));
        }
        for atlas in atlases {
            match prepare_atlas(library, names, catalog, atlas) {
                Ok(layer) => layers.push(layer),
                Err(error) => warnings.push(format!("图层未加载 {atlas}：{error:#}")),
            }
        }
    }
    ensure!(
        !layers.is_empty(),
        "没有可播放图层：{}",
        warnings.join("；")
    );
    let backgrounds = match super::spine_background::prepare(library, names, &prefix) {
        Ok(backgrounds) => backgrounds,
        Err(e) => {
            warnings.push(format!("场景背景未加载：{e:#}"));
            vec![]
        }
    };
    if !backgrounds.is_empty() {
        warnings.push("已恢复静态场景背景；未重建 Unity 材质的扭曲动画、粒子与遮罩".into());
    }
    Ok(SpineScene {
        layers,
        backgrounds,
        warnings,
        order_source,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atlas_pages_preserve_region_rotation_and_reject_paths() -> Result<()> {
        assert_eq!(
            page_names("a.png\nsize: 20,20\nregion\n rotate:90\n\nb.png\nsize:10,10\n")?,
            vec!["a.png", "b.png"]
        );
        assert!(page_names("../a.png\nsize:1,1").is_err());
        Ok(())
    }
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn local_layers_follow_configuration_and_json_skeletons_work() -> Result<()> {
        let library = Library::open(None)?;
        let names = Names::load(&library)?;
        let catalog = Catalog::build(&library);
        let resource = catalog
            .resources
            .values()
            .find(|r| r.path == "myassets/spine/40010704/2/yuanxiao_xr_2.png")
            .context("Multilayer sample missing")?;
        let scene = prepare_scene(&library, &names, &catalog, resource, true)?;
        ensure!(
            scene.layers.len() == 3 && scene.order_source.ends_with("40010704_info.json"),
            "Layers {}, source {}, warnings {:?}",
            scene.layers.len(),
            scene.order_source,
            scene.warnings
        );
        for (i, layer) in scene.layers.iter().enumerate() {
            ensure!(layer.logical_path.contains(&format!("/{}/", i + 1)));
        }
        let mut incomplete = Names::load(&library)?;
        incomplete
            .spine_assets
            .remove("myassets/spine/40010704/1/yuanxiao_xr_1.png");
        let partial = prepare_scene(&library, &incomplete, &catalog, resource, true)?;
        ensure!(
            partial.layers.len() == 2
                && partial.warnings.iter().any(|w| w.contains("yuanxiao_xr_1"))
        );
        let json_resource = catalog
            .resources
            .values()
            .find(|r| r.path == "myassets/spine/409903/2/huayubai_lf.png")
            .context("JSON sample missing")?;
        let layer = prepare(&library, &names, &catalog, json_resource)?;
        ensure!(layer.format == "json" && layer.version.starts_with("4.2."));
        let scene = prepare_scene(&library, &names, &catalog, json_resource, true)?;
        ensure!(scene.layers.len() == 1 && scene.layers[0].format == "json");
        Ok(())
    }
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn local_single_skeleton_extracts_across_bundles() -> Result<()> {
        let library = Library::open(None)?;
        let names = Names::load(&library)?;
        let catalog = Catalog::build(&library);
        let resource = catalog
            .resources
            .values()
            .find(|r| r.path == "myassets/spine/400107/yiji_kxj.png")
            .context("Sample missing")?;
        let result = prepare(&library, &names, &catalog, resource)?;
        ensure!(result.version == "4.2.08" && result.pages.len() == 1);
        let bytes = fs::read(&result.skeleton)?;
        ensure!(bytes.len() > 900_000);
        for path in result.pages.values() {
            let size = image::image_dimensions(path)?;
            ensure!(size.0 > 0 && size.1 > 0);
        }
        ensure!(prepare(&library, &names, &catalog, resource)?.cached);
        Ok(())
    }
}
