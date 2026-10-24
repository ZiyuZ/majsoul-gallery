use super::names::Names;
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Serialize, Deserialize)]
pub struct NamedItem {
    pub id: i64,
    pub name: String,
    pub icon: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Requirement {
    pub quantity: u64,
    /// The client sums inventory counts across these IDs for this requirement.
    pub items: Vec<NamedItem>,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Character {
    pub id: i64,
    pub name: String,
    pub sort: Option<i64>,
    pub sex: Option<i64>,
    pub limited: Option<bool>,
    pub collaboration: Option<i64>,
    pub treasure_sp: Option<i64>,
    pub favorite_type: i64,
    pub gifts: Vec<NamedItem>,
    pub can_contract: bool,
    pub contract_materials: Vec<Requirement>,
    pub raw_contract_materials: String,
    pub contract_cost: i64,
    pub initial_skin: i64,
    pub contract_skin: i64,
}
fn named(id: i64, items: &BTreeMap<i64, &Value>) -> NamedItem {
    let item = items.get(&id);
    NamedItem {
        id,
        name: item
            .and_then(|i| i["name_chs"].as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("未知物品 #{id}")),
        icon: item.and_then(|i| i["icon"].as_str()).unwrap_or("").into(),
    }
}
fn materials(raw: &str, items: &BTreeMap<i64, &Value>) -> Result<Vec<Requirement>> {
    raw.split(',')
        .filter(|s| !s.trim().is_empty())
        .map(|part| {
            let (ids, quantity) = part
                .rsplit_once('-')
                .context("契约材料格式变化：缺少数量分隔符")?;
            let quantity = quantity.parse()?;
            let items = ids
                .split('|')
                .map(|s| Ok(named(s.parse()?, items)))
                .collect::<Result<Vec<_>>>()?;
            ensure!(!items.is_empty(), "契约材料物品组为空");
            Ok(Requirement { quantity, items })
        })
        .collect()
}
pub fn extract(names: &Names) -> Result<BTreeMap<i64, Character>> {
    let items = names
        .rows("item_definition.item")
        .iter()
        .filter_map(|r| Some((r["id"].as_i64()?, r)))
        .collect::<BTreeMap<_, _>>();
    names
        .rows("item_definition.character")
        .iter()
        .map(|row| {
            let id = row["id"].as_i64().context("角色缺少 ID")?;
            let favorite_type = row["favorite"].as_i64().context("角色缺少喜好类型")?;
            let raw_contract_materials = row["star_5_material"].as_str().unwrap_or("").to_owned();
            let contract_materials =
                materials(&raw_contract_materials, &items).with_context(|| format!("角色 {id}"))?;
            Ok((
                id,
                Character {
                    id,
                    name: row["name_chs"].as_str().unwrap_or("").into(),
                    sort: row["sort"].as_i64(),
                    sex: row["sex"].as_i64(),
                    limited: row["limited"].as_i64().map(|v| v > 0),
                    collaboration: row["collaboration"].as_i64(),
                    treasure_sp: row["treasure_sp"].as_i64(),
                    favorite_type,
                    gifts: items
                        .values()
                        .filter(|i| i["category"] == 2 && i["type"].as_i64() == Some(favorite_type))
                        .filter_map(|i| i["id"].as_i64())
                        .map(|id| named(id, &items))
                        .collect(),
                    can_contract: row["can_marry"].as_i64().unwrap_or(0) > 0,
                    contract_materials,
                    raw_contract_materials,
                    contract_cost: row["star_5_cost"].as_i64().unwrap_or(0),
                    initial_skin: row["init_skin"].as_i64().unwrap_or(0),
                    contract_skin: row["full_fetter_skin"].as_i64().unwrap_or(0),
                },
            ))
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn contract_materials_preserve_shared_inventory_groups() {
        let value = materials("302010-10,303113|303013-5", &BTreeMap::new()).unwrap();
        assert_eq!(value[0].quantity, 10);
        assert_eq!(value[1].quantity, 5);
        assert_eq!(
            value[1].items.iter().map(|i| i.id).collect::<Vec<_>>(),
            [303113, 303013]
        );
        assert!(materials("changed-format", &BTreeMap::new()).is_err());
    }
}
