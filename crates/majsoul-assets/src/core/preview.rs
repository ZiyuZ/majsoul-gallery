use super::{
    Library, Resource,
    index::{cache_dir, digest, fingerprint},
};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use std::fs;
use unity_rs_core::{sprite::SpriteReadLimits, studio::Studio, texture::TextureReadLimits};

#[derive(Serialize)]
pub struct Preview {
    pub media: String,
    pub path: String,
    pub width: u32,
    pub height: u32,
    pub kind: String,
    pub cached: bool,
}
pub fn decode(library: &Library, resource: &Resource) -> Result<Preview> {
    if resource.class == "Font" {
        return decode_font(library, resource);
    }
    let asset = resource.asset.as_ref().context("素材缺少对象身份")?;
    let bundle = library
        .index
        .bundles
        .get(&resource.bundle)
        .context("素材包不在索引中")?;
    let path = library.index.source.join(&bundle.name);
    let (size, modified) = fingerprint(&path)?;
    if (size, modified) != (bundle.size, bundle.modified) {
        bail!("源资源已更新，请在切换资源目录中勾选重新建立索引");
    }
    let cache = cache_dir().join(format!(
        "{}.png",
        digest(format!(
            "viewer-texture-first-v2|{}|{}|{}|{}|{}|{}|{}|{}",
            library.index.source.display(),
            bundle.name,
            size,
            modified,
            resource.path,
            asset.file,
            asset.id,
            library.index.unity
        ))
    ));
    let kind_file = cache.with_extension("kind");
    if cache.exists()
        && let Ok((width, height)) = image::image_dimensions(&cache)
    {
        return Ok(Preview {
            media: "image".into(),
            path: cache.to_string_lossy().into(),
            width,
            height,
            kind: fs::read_to_string(kind_file).unwrap_or_else(|_| "图片".into()),
            cached: true,
        });
    }
    let studio = super::bundles::open(library, &bundle.name)?;
    // Prefer the container's standalone texture to preserve its original canvas;
    // sprites remain supported when only an atlas/sprite entry is available.
    let object = direct_object(&studio, &resource.path)?
        .or_else(|| {
            studio
                .objects()
                .find(|o| o.class_id() == 28 && o.container() == Some(resource.path.as_str()))
                .or_else(|| studio.object(asset.file, asset.id))
        })
        .context("找不到图片对象")?;
    let sprite = object.class_id() == 213;
    let rgba = if sprite {
        object.decode_sprite(SpriteReadLimits::default(), TextureReadLimits::default())?
    } else {
        object.decode_texture_mip(0, TextureReadLimits::default())?
    };
    let (width, height) = (rgba.width, rgba.height);
    let mut image =
        image::RgbaImage::from_raw(width, height, rgba.pixels).context("无效图片像素")?;
    if !sprite {
        image::imageops::flip_vertical_in_place(&mut image);
    }
    fs::create_dir_all(cache_dir())?;
    let mut temp = tempfile::NamedTempFile::new_in(cache_dir())?;
    image.write_to(temp.as_file_mut(), image::ImageFormat::Png)?;
    temp.persist(&cache)?;
    let kind = if sprite {
        "Sprite 裁切"
    } else {
        "Texture2D 原始画布"
    }
    .to_owned();
    fs::write(kind_file, &kind)?;
    Ok(Preview {
        media: "image".into(),
        path: cache.to_string_lossy().into(),
        width,
        height,
        kind,
        cached: false,
    })
}

fn decode_font(library: &Library, resource: &Resource) -> Result<Preview> {
    let bundle = library
        .index
        .bundles
        .get(&resource.bundle)
        .context("字体包不在索引中")?;
    let source = library.index.source.join(&bundle.name);
    let stamp = fingerprint(&source)?;
    anyhow::ensure!(
        stamp == (bundle.size, bundle.modified),
        "字体源资源已更新，请刷新索引"
    );
    let cache = cache_dir().join(format!(
        "{}.ttf",
        digest(format!(
            "font-v1|{}|{}|{}|{:?}",
            library.index.source.display(),
            library.index.unity,
            resource.id,
            stamp
        ))
    ));
    let cached = cache.exists();
    if !cached {
        let studio = super::bundles::open(library, &bundle.name)?;
        let object = direct_object(&studio, &resource.path)?
            .or_else(|| {
                studio
                    .objects()
                    .find(|o| o.class_id() == 128 && o.container() == Some(resource.path.as_str()))
            })
            .context("资源包中未找到对应的 Font 对象")?;
        let font =
            object.read_font(unity_rs_core::simple_assets::SimpleAssetReadLimits::default())?;
        let bytes = font.payload.read_to_vec(64 * 1024 * 1024)?;
        anyhow::ensure!(
            bytes.starts_with(&[0, 1, 0, 0])
                || bytes.starts_with(b"OTTO")
                || bytes.starts_with(b"ttcf"),
            "Font 对象没有可用的嵌入字体数据"
        );
        fs::create_dir_all(cache_dir())?;
        let mut temp = tempfile::NamedTempFile::new_in(cache_dir())?;
        std::io::Write::write_all(temp.as_file_mut(), &bytes)?;
        temp.persist(&cache)?;
    }
    Ok(Preview {
        media: "font".into(),
        path: cache.to_string_lossy().into(),
        width: 0,
        height: 0,
        kind: "Font 嵌入字体".into(),
        cached,
    })
}

pub fn direct_object<'a>(
    studio: &'a Studio,
    path: &str,
) -> Result<Option<unity_rs_core::studio::StudioObject<'a>>> {
    for bundle in studio.objects().filter(|o| o.class_id() == 142) {
        let metadata = bundle
            .read_asset_bundle(unity_rs_core::serialized::ContainerMetadataReadLimits::default())?;
        if let Some(entry) = metadata
            .container
            .iter()
            .find(|e| e.key.eq_ignore_ascii_case(path))
        {
            anyhow::ensure!(entry.asset.file_id == 0, "资源引用位于外部序列化文件");
            return Ok(studio.object(bundle.file_index(), entry.asset.path_id));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Catalog;

    #[test]
    #[ignore = "requires the local installation and saved index"]
    fn local_fonts_have_embedded_data() -> Result<()> {
        let library = Library::open(None)?;
        let catalog = Catalog::build(&library);
        let fonts = catalog
            .resources
            .values()
            .filter(|r| r.class == "Font")
            .collect::<Vec<_>>();
        assert_eq!(fonts.len(), 19);
        for font in fonts {
            assert_eq!(decode(&library, font)?.media, "font");
        }
        Ok(())
    }
    #[test]
    #[ignore = "requires the local installation and saved index"]
    fn local_library_preserves_texture_canvas() -> Result<()> {
        let library = Library::open(None)?;
        let catalog = Catalog::build(&library);
        assert_eq!(
            catalog
                .categories
                .iter()
                .find(|c| c.id == "character")
                .unwrap()
                .groups,
            522
        );
        assert!(catalog.page("character", "jiutiao", 0, false).total > 0);
        let visible = catalog.page("all", "", 0, false);
        assert!(
            visible
                .groups
                .iter()
                .all(|g| !matches!(g.category.as_str(), "other" | "spine"))
        );
        for (path, dimensions) in [
            ("myassets/deco/character/jiutiao/full/full.png", (749, 1296)),
            (
                "myassets/deco/character/xiaoniaoyouxingye_qp/full/full.png",
                (1185, 1079),
            ),
            (
                "myassets/deco/tablecloth/tablecloth_newyear/pic/common/tablecloth_newyear.jpg",
                (170, 170),
            ),
        ] {
            let resource = catalog
                .resources
                .values()
                .find(|r| r.path == path)
                .context("missing sample")?;
            let preview = decode(&library, resource)?;
            assert_eq!((preview.width, preview.height), dimensions);
            assert_eq!(preview.kind, "Texture2D 原始画布");
            assert!(decode(&library, resource)?.cached);
            println!("{path}: {}×{}", preview.width, preview.height);
        }
        Ok(())
    }
}
