//! Conservative reconstruction of static, axis-aligned background quads.
//! Material references select textures; filenames do not determine composition.
use super::{Library, names::Names, preview, spine};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
#[derive(Serialize)]
pub struct Background {
    pub path: String,
    pub logical_path: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub uv: [f64; 4],
    pub color: [f64; 4],
    pub order: i64,
}
fn open(lib: &Library, bundle: &str) -> Result<Arc<super::bundles::Bundle>> {
    super::bundles::open(lib, bundle)
}
fn id(v: &Value) -> i64 {
    v["m_PathID"].as_i64().unwrap_or(0)
}
fn num(v: &Value, default: f64) -> f64 {
    v.as_f64().unwrap_or(default)
}
fn property<'a>(v: &'a Value, kind: &str, key: &str) -> Option<&'a Value> {
    v["m_SavedProperties"][kind]
        .as_array()?
        .iter()
        .find(|p| p["key"] == key)
        .map(|p| &p["value"])
}
pub fn prepare(lib: &Library, names: &Names, prefix: &str) -> Result<Vec<Background>> {
    let Some((_, bundle)) = names
        .spine_assets
        .iter()
        .find(|(p, _)| p.starts_with(prefix) && p.to_lowercase().ends_with("/exportroot.prefab"))
    else {
        return Ok(vec![]);
    };
    let scene = open(lib, bundle)?;
    let mut objects = BTreeMap::new();
    for o in scene
        .objects()
        .filter(|o| [1, 4, 23, 33, 224].contains(&o.class_id()))
    {
        let json: Value = serde_json::from_slice(&o.read_type_tree_json(false, 8 * 1024 * 1024)?)?;
        objects.insert(o.path_id(), (o.class_id(), json));
    }
    let transforms = objects
        .iter()
        .filter(|(_, (c, _))| [4, 224].contains(c))
        .map(|(i, (_, v))| (id(&v["m_GameObject"]), *i))
        .collect::<BTreeMap<_, _>>();
    let meshes = objects
        .values()
        .filter(|(c, _)| *c == 33)
        .map(|(_, v)| (id(&v["m_GameObject"]), id(&v["m_Mesh"])))
        .collect::<BTreeMap<_, _>>();
    let mut candidates = vec![];
    for (_, renderer) in objects.values().filter(|(c, _)| *c == 23) {
        if renderer["m_Enabled"] == false {
            continue;
        }
        let go = id(&renderer["m_GameObject"]);
        // 10210 is Unity's built-in Quad. Arbitrary meshes/particles are skipped.
        if meshes.get(&go) != Some(&10210) {
            continue;
        }
        let Some(mut transform) = transforms.get(&go).copied() else {
            continue;
        };
        let (mut x, mut y, mut sx, mut sy) = (0., 0., 1., 1.);
        let mut background = false;
        let mut valid = true;
        let mut seen = BTreeSet::new();
        while transform != 0 {
            if !seen.insert(transform) {
                valid = false;
                break;
            }
            let Some((_, t)) = objects.get(&transform) else {
                valid = false;
                break;
            };
            let rot = &t["m_LocalRotation"];
            if ["x", "y", "z"]
                .iter()
                .any(|k| num(&rot[k], 0.).abs() > 0.0001)
            {
                valid = false;
                break;
            }
            let g = id(&t["m_GameObject"]);
            if let Some((_, v)) = objects.get(&g) {
                if v["m_IsActive"] == false {
                    valid = false;
                    break;
                }
                let name = v["m_Name"].as_str().unwrap_or("").to_lowercase();
                background |= name == "bg" || name == "background";
            }
            let ax = num(&t["m_LocalScale"]["x"], 1.);
            let ay = num(&t["m_LocalScale"]["y"], 1.);
            x = x * ax + num(&t["m_LocalPosition"]["x"], 0.);
            y = y * ay + num(&t["m_LocalPosition"]["y"], 0.);
            sx *= ax;
            sy *= ay;
            transform = id(&t["m_Father"]);
        }
        if valid
            && background
            && sx > 0.
            && sy > 0.
            && let Some(material) = renderer["m_Materials"].as_array().and_then(|a| a.first())
        {
            candidates.push((id(material), x - sx / 2., y - sy / 2., sx, sy));
        }
    }
    if candidates.is_empty() {
        return Ok(vec![]);
    }
    let wanted = candidates.iter().map(|c| c.0).collect::<BTreeSet<_>>();
    let mut materials = BTreeMap::new();
    for bundle in names
        .spine_assets
        .iter()
        .filter(|(p, _)| p.starts_with(prefix) && p.ends_with(".mat"))
        .map(|(_, b)| b)
        .collect::<BTreeSet<_>>()
    {
        let studio = open(lib, bundle)?;
        for obj in studio
            .objects()
            .filter(|o| o.class_id() == 21 && wanted.contains(&o.path_id()))
        {
            ensure!(
                !materials.contains_key(&obj.path_id()),
                "背景材质引用不唯一"
            );
            materials.insert(
                obj.path_id(),
                serde_json::from_slice::<Value>(&obj.read_type_tree_json(false, 8 * 1024 * 1024)?)?,
            );
        }
    }
    let mut textures = BTreeMap::new();
    let wanted_textures = materials
        .values()
        .filter_map(|m| property(m, "m_TexEnvs", "_MainTex"))
        .map(|v| id(&v["m_Texture"]))
        .collect::<BTreeSet<_>>();
    for (path, bundle) in names
        .spine_assets
        .iter()
        .filter(|(p, _)| p.starts_with(prefix) && p.contains("/texture/") && p.ends_with(".png"))
    {
        // Open each bundle once even when many textures share it.
        textures
            .entry(bundle.clone())
            .or_insert_with(Vec::new)
            .push(path.clone());
    }
    let mut logicals = BTreeMap::new();
    for (bundle, paths) in textures {
        let studio = open(lib, &bundle)?;
        for path in paths {
            if let Some(obj) = preview::direct_object(&studio, &path)?
                && wanted_textures.contains(&obj.path_id())
            {
                ensure!(!logicals.contains_key(&obj.path_id()), "背景纹理引用不唯一");
                logicals.insert(obj.path_id(), path);
            }
        }
    }
    let mut result = vec![];
    for (material, x, y, width, height) in candidates {
        let Some(mat) = materials.get(&material) else {
            continue;
        };
        let Some(tex) = property(mat, "m_TexEnvs", "_MainTex") else {
            continue;
        };
        let Some(logical) = logicals.get(&id(&tex["m_Texture"])) else {
            continue;
        };
        // Unknown shader masks/dissolves must not become opaque rectangles.
        if property(mat, "m_Floats", "_Dissolve").is_some_and(|v| num(v, 0.) != 0.) {
            continue;
        }
        let decoded = spine::preview_texture(lib, names, logical).context("背景纹理解码失败")?;
        let tint = property(mat, "m_Colors", "_MainColor")
            .cloned()
            .unwrap_or(Value::Null);
        result.push(Background {
            path: decoded.path,
            logical_path: logical.clone(),
            x,
            y,
            width,
            height,
            uv: [
                num(&tex["m_Scale"]["x"], 1.),
                num(&tex["m_Scale"]["y"], 1.),
                num(&tex["m_Offset"]["x"], 0.),
                num(&tex["m_Offset"]["y"], 0.),
            ],
            color: [
                num(&tint["r"], 1.),
                num(&tint["g"], 1.),
                num(&tint["b"], 1.),
                num(&tint["a"], 1.)
                    * property(mat, "m_Floats", "_Alpha")
                        .map(|v| num(v, 1.))
                        .unwrap_or(1.),
            ],
            order: mat["m_CustomRenderQueue"].as_i64().unwrap_or(3000),
        });
    }
    result.sort_by_key(|b| b.order);
    Ok(result)
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn local_background_follows_material_reference() -> anyhow::Result<()> {
        let library = super::super::Library::open(None)?;
        let names = super::super::names::Names::load(&library)?;
        let backgrounds = super::prepare(&library, &names, "myassets/spine/40010803/")?;
        for b in &backgrounds {
            println!(
                "{} {},{} {}x{}",
                b.logical_path, b.x, b.y, b.width, b.height
            );
        }
        let bg = backgrounds
            .iter()
            .find(|b| b.logical_path.ends_with("/jiantongying_001.png"))
            .expect("referenced background missing");
        assert!(bg.width > 5000. && bg.height > 3000.);
        assert!((bg.uv[0] - 1.22).abs() < 0.001);
        Ok(())
    }
}
