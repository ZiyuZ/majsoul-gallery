//! Inventory definitions from the local item configuration, independent of icon availability.
use super::names::Names;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemGroup {
    Materials,
    Gifts,
    Vouchers,
    Containers,
    Activity,
    Other,
}

impl ItemGroup {
    pub fn label(self) -> &'static str {
        match self {
            Self::Materials => "养成材料",
            Self::Gifts => "礼物",
            Self::Vouchers => "券与卷轴",
            Self::Containers => "福袋与宝盒",
            Self::Activity => "活动道具",
            Self::Other => "其他道具",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GiftRarity {
    Green,
    Blue,
    Purple,
    Special,
}

impl GiftRarity {
    pub fn label(self) -> &'static str {
        match self {
            Self::Green => "绿色",
            Self::Blue => "蓝色",
            Self::Purple => "紫色",
            Self::Special => "特殊",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ItemDefinition {
    pub id: String,
    pub name: String,
    pub sort: i64,
    pub group: ItemGroup,
    pub gift_type: u64,
    pub gift_bond: u64,
    pub rarity: Option<GiftRarity>,
    pub locked: bool,
    pub description: String,
    pub expires: String,
}

impl ItemDefinition {
    pub fn same_gift(&self, other: &Self) -> bool {
        self.group == ItemGroup::Gifts
            && other.group == ItemGroup::Gifts
            && (1..=8).contains(&self.gift_type)
            && self.gift_type == other.gift_type
            && self.gift_bond == other.gift_bond
    }
}

pub fn locked_icon(icon: &str) -> bool {
    let icon = icon.to_lowercase();
    icon.contains("_limit") || icon.contains("lock")
}

pub fn definitions(names: &Names) -> BTreeMap<String, ItemDefinition> {
    let materials: BTreeSet<i64> = names
        .rows("item_definition.character")
        .iter()
        .filter_map(|row| row["star_5_material"].as_str())
        .flat_map(|raw| raw.split(','))
        .filter_map(|part| part.rsplit_once('-').map(|(ids, _)| ids))
        .flat_map(|ids| ids.split('|'))
        .filter_map(|id| id.parse().ok())
        .collect();
    names
        .rows("item_definition.item")
        .iter()
        .filter_map(|row| definition(row, &materials))
        .map(|item| (item.id.clone(), item))
        .collect()
}

fn definition(row: &Value, materials: &BTreeSet<i64>) -> Option<ItemDefinition> {
    let id = row["id"].as_i64()?;
    let category = row["category"].as_u64()?;
    if !matches!(category, 1 | 2 | 3 | 6) {
        return None;
    }
    let name = row["name_chs"].as_str().unwrap_or_default();
    let kind = row["type"].as_u64().unwrap_or(0);
    let group = if category == 2 {
        ItemGroup::Gifts
    } else if category == 6 {
        ItemGroup::Activity
    } else if materials.contains(&id) || name == "星之石" {
        ItemGroup::Materials
    } else if name.contains('券') || name.contains("卷轴") {
        ItemGroup::Vouchers
    } else if category == 3 || matches!(kind, 1 | 2) {
        ItemGroup::Containers
    } else {
        ItemGroup::Other
    };
    let gift_bond = if category == 2 {
        row["iargs"][0].as_u64().unwrap_or(0)
    } else {
        0
    };
    let rarity = (category == 2).then_some(match (kind, gift_bond) {
        (1..=8, 200) => GiftRarity::Green,
        (1..=8, 1000) => GiftRarity::Blue,
        (1..=8, 5000) => GiftRarity::Purple,
        _ => GiftRarity::Special,
    });
    Some(ItemDefinition {
        id: id.to_string(),
        name: name.into(),
        sort: row["sort"].as_i64().unwrap_or(i64::MAX),
        group,
        gift_type: if category == 2 { kind } else { 0 },
        gift_bond,
        rarity,
        locked: category == 2 && locked_icon(row["icon"].as_str().unwrap_or_default()),
        description: ["desc_chs", "desc_func_chs"]
            .iter()
            .filter_map(|key| row[key].as_str())
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>()
            .join("\n"),
        expires: row["item_expire"].as_str().unwrap_or_default().into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn gifts_preserve_lock_state_and_share_material_identity() {
        let regular = definition(&json!({"id":303013,"name_chs":"香喷喷曲奇","category":2,"type":1,"iargs":[5000],"icon":"biscuit2.jpg"}), &BTreeSet::new()).unwrap();
        let locked = definition(&json!({"id":303113,"name_chs":"香喷喷曲奇","category":2,"type":1,"iargs":[5000],"icon":"biscuit2_limit.jpg"}), &BTreeSet::new()).unwrap();
        assert!(!regular.locked);
        assert!(locked.locked);
        assert!(regular.same_gift(&locked));
        assert_eq!(locked.rarity, Some(GiftRarity::Purple));
        let special = definition(
            &json!({"id":303991,"name_chs":"羁绊之心","category":2,"type":99,"iargs":[1000]}),
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(special.rarity, Some(GiftRarity::Special));
        assert!(!regular.same_gift(&special));
    }
    #[test]
    fn inventory_includes_activity_items_without_icons_and_excludes_cosmetics() {
        let event = definition(
            &json!({"id":30910001,"name_chs":"活动道具","category":6}),
            &BTreeSet::new(),
        )
        .unwrap();
        assert_eq!(event.group, ItemGroup::Activity);
        assert!(definition(&json!({"id":308001,"category":5}), &BTreeSet::new()).is_none());
        let material = definition(
            &json!({"id":302005,"category":1}),
            &BTreeSet::from([302005]),
        )
        .unwrap();
        assert_eq!(material.group, ItemGroup::Materials);
    }
}
