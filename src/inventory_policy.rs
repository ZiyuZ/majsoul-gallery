//! Presentation rules embedded in the executable; source metadata remains complete.
use anyhow::{Context, Result};
use majsoul_assets::core::items::{ItemDefinition, ItemGroup};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InventoryPolicy {
    excluded_groups: Vec<ItemGroup>,
    excluded_items: BTreeMap<String, String>,
}

impl InventoryPolicy {
    pub fn embedded() -> Result<Self> {
        serde_json::from_str(include_str!("assets/inventory-filter.json"))
            .context("内嵌道具过滤规则无效")
    }

    pub fn allows(&self, item: &ItemDefinition) -> bool {
        !self.excluded_groups.contains(&item.group) && !self.excluded_items.contains_key(&item.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_rules_exclude_groups_and_exact_ids_but_keep_normal_and_locked_gifts() {
        let rules = InventoryPolicy::embedded().unwrap();
        let mut item = ItemDefinition {
            id: "303013".into(),
            name: "香喷喷曲奇".into(),
            sort: 1,
            group: ItemGroup::Gifts,
            gift_type: 1,
            gift_bond: 5000,
            rarity: None,
            locked: false,
            description: String::new(),
            expires: String::new(),
        };
        assert!(rules.allows(&item));
        item.id = "303113".into();
        item.locked = true;
        assert!(rules.allows(&item));
        for id in ["300005", "303995"] {
            item.id = id.into();
            assert!(!rules.allows(&item));
        }
        item.id = "future-item".into();
        for group in [ItemGroup::Activity, ItemGroup::Other] {
            item.group = group;
            assert!(!rules.allows(&item));
        }
        for group in [
            ItemGroup::Materials,
            ItemGroup::Vouchers,
            ItemGroup::Containers,
        ] {
            item.group = group;
            assert!(rules.allows(&item));
        }
    }
}
