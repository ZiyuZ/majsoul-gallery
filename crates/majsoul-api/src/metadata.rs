//! Read only Unity's configuration/version metadata, entirely in memory.
use anyhow::{Context, Result, bail, ensure};
use serde_json::Value;
use std::collections::BTreeMap;
use unity_rs_core::{loader::AssetLoadOptions, source::Region, studio::Studio};

pub(crate) fn read_json(bytes: Vec<u8>, path: Option<&str>) -> Result<Value> {
    let studio = Studio::open_region_with_options(
        "client-metadata.majset",
        Region::from_bytes(bytes),
        AssetLoadOptions {
            unity_version_override: Some("2022.3.62f2c1".parse()?),
            ..Default::default()
        },
    )?;
    for object in studio.objects() {
        if let Some(path) = path {
            let name = path.rsplit('/').next().unwrap();
            if object.class_id() == 49
                && (object
                    .container()
                    .is_some_and(|p| p.eq_ignore_ascii_case(path))
                    || object
                        .name()
                        .is_some_and(|n| n == name || n == name.trim_end_matches(".json")))
            {
                return serde_json::from_slice(&object.read_text_bytes(2 * 1024 * 1024)?)
                    .context("客户端配置 JSON 无效");
            }
        } else if object.class_id() == 114 {
            let value: Value =
                serde_json::from_slice(&object.read_type_tree_json(false, 32 * 1024 * 1024)?)?;
            if value["assetInfos"].is_array() && value["bundleInfos"].is_array() {
                return Ok(value);
            }
        }
    }
    bail!("客户端缺少版本或配置元数据")
}

pub(crate) fn version_bundles(info: &Value) -> Result<BTreeMap<String, String>> {
    let bundles = info["bundleInfos"]
        .as_array()
        .context("客户端资源清单缺少资源包")?;
    let mut selected = BTreeMap::new();
    for asset in info["assetInfos"]
        .as_array()
        .context("客户端资源清单缺少资产表")?
    {
        let Some(path) = asset["assetPath"].as_str() else {
            continue;
        };
        if ![
            "MyAssets/docs_version/version.json",
            "MyAssets/docs/config.json",
        ]
        .contains(&path)
        {
            continue;
        }
        let index = asset["ownerBundleIndex"]
            .as_u64()
            .context("版本资源包索引无效")? as usize;
        let bundle = bundles
            .get(index)
            .and_then(|b| b["name"].as_str())
            .context("版本资源包不存在")?;
        ensure!(
            !bundle.is_empty() && !bundle.contains(['/', '\\', ':']) && bundle != "..",
            "版本资源包名无效"
        );
        selected.insert(path.into(), bundle.into());
    }
    ensure!(selected.len() == 2, "资源清单缺少版本或网关配置");
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn resolves_only_config_and_version_owners_without_image_downloads() -> Result<()> {
        let info = json!({"bundleInfos":[{"name":"config.majset"},{"name":"version.majset"},{"name":"texture.majset"}],
            "assetInfos":[{"assetPath":"MyAssets/docs/config.json","ownerBundleIndex":0},
                {"assetPath":"MyAssets/docs_version/version.json","ownerBundleIndex":1},
                {"assetPath":"MyAssets/deco/texture.png","ownerBundleIndex":2}]});
        let selected = version_bundles(&info)?;
        assert_eq!(selected.len(), 2);
        assert!(!selected.values().any(|v| v == "texture.majset"));
        Ok(())
    }
}
