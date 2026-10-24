//! Explicit resource visibility and title membership embedded in the executable.
use anyhow::{Context, Result};
use majsoul_assets::core::items::{ItemDefinition, ItemGroup};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResourceCatalog {
    items: ItemVisibility,
    pub titles: TitleCatalog,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemVisibility {
    hidden_groups: Vec<ItemGroup>,
    hidden: BTreeMap<String, String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TitleCategory {
    Supporter,
    Achievement,
    Event,
    Tournament,
    Exclusive,
    Legacy,
    Other,
}

impl TitleCategory {
    pub const ALL: [Self; 7] = [
        Self::Supporter,
        Self::Achievement,
        Self::Event,
        Self::Tournament,
        Self::Exclusive,
        Self::Legacy,
        Self::Other,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Supporter => "充值回馈",
            Self::Achievement => "成就奖励",
            Self::Event => "活动纪念",
            Self::Tournament => "赛事荣誉",
            Self::Exclusive => "专属称号",
            Self::Legacy => "早期纪念",
            Self::Other => "其他",
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TitleCatalog {
    hidden: BTreeMap<String, String>,
    variants: BTreeMap<String, String>,
    categories: BTreeMap<TitleCategory, BTreeMap<String, String>>,
}

impl TitleCatalog {
    pub fn allows(&self, id: &str) -> bool {
        !self.hidden.contains_key(id)
    }
    pub fn variant(&self, ids: &[String]) -> Option<String> {
        ids.iter().find_map(|id| self.variants.get(id)).cloned()
    }
    pub fn category(&self, ids: &[String]) -> TitleCategory {
        self.categories
            .iter()
            .find(|(_, titles)| ids.iter().any(|id| titles.contains_key(id)))
            .map(|(category, _)| *category)
            .unwrap_or(TitleCategory::Other)
    }
}

impl ResourceCatalog {
    pub fn embedded() -> Result<Self> {
        let catalog: Self = serde_json::from_str(include_str!("assets/resource-catalog.json"))
            .context("内嵌资源目录无效")?;
        let mut seen = std::collections::BTreeSet::new();
        for titles in catalog.titles.categories.values() {
            for id in titles.keys() {
                anyhow::ensure!(seen.insert(id), "称号 {id} 被分配到多个分类");
                anyhow::ensure!(
                    !catalog.titles.hidden.contains_key(id),
                    "隐藏称号 {id} 同时出现在分类中"
                );
            }
        }
        Ok(catalog)
    }

    pub fn allows(&self, item: &ItemDefinition) -> bool {
        !self.items.hidden_groups.contains(&item.group) && !self.items.hidden.contains_key(&item.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_catalog_keeps_explicit_membership_and_unknown_titles_separate() {
        let rules = ResourceCatalog::embedded().unwrap().titles;
        for id in ["600001", "common", "hunzhiqiyuezhe"] {
            assert!(!rules.allows(id));
        }
        for (id, category) in [
            ("600005", TitleCategory::Supporter),
            ("600113", TitleCategory::Supporter),
            ("600116", TitleCategory::Achievement),
            ("600028", TitleCategory::Event),
            ("600056", TitleCategory::Tournament),
            ("600154", TitleCategory::Tournament),
            ("600096", TitleCategory::Exclusive),
            ("600004", TitleCategory::Legacy),
            ("future-title", TitleCategory::Other),
        ] {
            assert_eq!(rules.category(&[id.into()]), category, "{id}");
        }
        assert_eq!(
            rules.category(&["600052".into(), "600079".into()]),
            TitleCategory::Tournament
        );
        assert_eq!(
            rules.variant(&["600004".into()]).as_deref(),
            Some("四人麻将")
        );
        assert_eq!(
            rules.variant(&["600022".into()]).as_deref(),
            Some("三人麻将")
        );
        for (id, variant) in [
            ("600090", "青龙"),
            ("600091", "朱雀"),
            ("600099", "白虎"),
            ("600106", "玄武"),
            ("600172", "白虎"),
        ] {
            assert_eq!(rules.variant(&[id.into()]).as_deref(), Some(variant));
        }
    }

    #[test]
    fn embedded_rules_exclude_groups_and_exact_ids_but_keep_normal_and_locked_gifts() {
        let rules = ResourceCatalog::embedded().unwrap();
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
