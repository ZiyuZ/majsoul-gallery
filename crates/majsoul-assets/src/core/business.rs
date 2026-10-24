//! Add business entries whose individual icons are packed inside atlases.
use super::{
    Catalog,
    catalog::{Group, GroupParent, Resource},
    characters::NamedItem,
    index::digest,
    names::{Binding, Names},
};
use serde_json::Value;

pub fn populate(names: &Names, catalog: &mut Catalog) {
    const GIFTS: [&str; 8] = [
        "曲奇",
        "游戏机",
        "美术品",
        "酒",
        "宝石",
        "玩偶",
        "同人志",
        "裙子",
    ];
    for row in names.rows("item_definition.item") {
        let Some(id) = row["id"].as_i64() else {
            continue;
        };
        let icon = text(row, "icon");
        if !icon.to_lowercase().starts_with("extendres/items/") {
            continue;
        }
        let kind = row["type"].as_i64().unwrap_or(0);
        let gift = row["category"] == 2;
        if !matches!(row["category"].as_i64(), Some(1 | 2 | 3 | 6)) {
            continue;
        }
        // Old activity definitions can refer to removed standalone images.
        // Their inventory definitions remain available even without a local icon.
        let directory = icon
            .to_lowercase()
            .rsplit_once('/')
            .map(|(dir, _)| dir.to_owned())
            .unwrap_or_default();
        if !names.icon_assets.keys().any(|path| {
            path.starts_with(&format!("myassets/{directory}/")) && path.ends_with("_config.json")
        }) {
            continue;
        }
        let category = if gift { "gift" } else { "item" };
        let parent = if gift && (1..=8).contains(&kind) {
            GIFTS[kind as usize - 1]
        } else if gift {
            "特殊礼物"
        } else if row["category"] == 6 {
            "活动道具"
        } else {
            match kind {
                1 => "宝盒",
                2 => "福袋",
                _ => "材料与其他道具",
            }
        };
        insert(
            catalog,
            category,
            NamedItem {
                id,
                name: text(row, "name_chs").into(),
                icon: icon.into(),
            },
            Binding {
                table: "item_definition.item".into(),
                id,
                name: text(row, "name_chs").into(),
                via: "item.icon → 本地图集页号 → 命名 Sprite".into(),
            },
            Some(GroupParent {
                id: format!("{category}:{parent}"),
                label: parent.into(),
            }),
            "物品图标",
        );
    }
    // Stars and Soul levels share a major-rank badge. Keep one per icon/mode.
    let mut badges = std::collections::BTreeSet::new();
    for rank in names.rows("level_definition.level_definition") {
        let Some(id) = rank["id"].as_i64() else {
            continue;
        };
        let icon = text(rank, "primary_icon");
        if icon.is_empty() || !badges.insert(icon.to_lowercase()) {
            continue;
        }
        let name = text(rank, "name_chs");
        let mode = if icon.contains("sanma_") {
            "三人麻将"
        } else {
            "四人麻将"
        };
        insert(
            catalog,
            "rank",
            NamedItem {
                id,
                name: name.into(),
                icon: icon.into(),
            },
            Binding {
                table: "level_definition.level_definition".into(),
                id,
                name: name.into(),
                via: "primary_icon 去重 → 大段位徽章（不区分星级和魂天等级）".into(),
            },
            Some(GroupParent {
                id: format!("rank:{mode}"),
                label: mode.into(),
            }),
            "大段位徽章",
        );
    }
    catalog.organize();
}
fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row[key].as_str().unwrap_or("")
}
fn insert(
    catalog: &mut Catalog,
    category: &str,
    item: NamedItem,
    binding: Binding,
    parent: Option<GroupParent>,
    role: &str,
) {
    let id = digest(format!("business|{category}|{}|{}", binding.table, item.id));
    let path = format!("myassets/{}", item.icon.to_lowercase());
    let resource = Resource {
        id: id.clone(),
        path: path.clone(),
        label: item.name.clone(),
        role: role.into(),
        class: "AtlasSprite".into(),
        bindings: vec![binding.clone()],
        bundle: String::new(),
        asset: None,
        item: Some(item.clone()),
    };
    catalog.resources.insert(id.clone(), resource.clone());
    catalog.groups.push(Group {
        character_sort: None,
        id: format!("entry:{id}"),
        label: item.name,
        path,
        category: category.into(),
        bindings: vec![binding],
        resources: vec![resource],
        character: None,
        parent,
        story: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::{Context, Result, ensure};
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn character_filters_combine_without_splitting_skins() -> Result<()> {
        use super::super::catalog::CharacterFilter;
        let library = super::super::Library::open(None)?;
        let names = Names::load(&library)?;
        let mut catalog = Catalog::build(&library);
        names.apply(&mut catalog);
        names.apply_characters(&library, &mut catalog)?;
        populate(&names, &mut catalog);
        let filter = CharacterFilter {
            sex: Some(2),
            limited_only: true,
            collaboration_only: true,
        };
        let page = catalog.page_filtered("character", "", 0, false, &filter);
        ensure!(page.total > 0);
        for group in &page.groups {
            let character = group
                .character
                .as_ref()
                .context("Filtered character has no profile")?;
            ensure!(
                character.sex == Some(2)
                    && character.limited == Some(true)
                    && character.collaboration.is_some_and(|v| v > 0)
            );
        }
        let owner = page.groups[0].parent.as_ref().unwrap().id.clone();
        let belongs = |g: &&Group| {
            g.category == "character" && g.parent.as_ref().is_some_and(|p| p.id == owner)
        };
        ensure!(
            page.groups.iter().filter(belongs).count()
                == catalog.groups.iter().filter(belongs).count()
        );
        let gintama = catalog.page_filtered("character", "坂田银时", 0, false, &filter);
        ensure!(gintama.total == 1 && gintama.groups.len() > 1);
        let female = CharacterFilter {
            sex: Some(1),
            ..Default::default()
        };
        ensure!(
            catalog
                .page_filtered("character", "坂田银时", 0, false, &female)
                .total
                == 0
        );
        ensure!(
            catalog
                .page_filtered("item", "日光福袋", 0, false, &filter)
                .total
                == 1
        );
        let profiles = super::super::characters::extract(&names)?;
        ensure!(
            profiles[&200052].limited == Some(true) && profiles[&200052].collaboration == Some(0)
        );
        ensure!(profiles[&200001].limited == Some(false));
        Ok(())
    }
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn local_business_catalog_includes_seven_categories_and_atlas_icons() -> Result<()> {
        let library = super::super::Library::open(None)?;
        let names = Names::load(&library)?;
        let mut catalog = Catalog::build(&library);
        names.apply(&mut catalog);
        names.apply_characters(&library, &mut catalog)?;
        populate(&names, &mut catalog);
        let first_page = catalog.page("character", "", 0, false);
        let owner = "character:200005";
        let all_skins = catalog
            .groups
            .iter()
            .filter(|g| {
                g.category == "character" && g.parent.as_ref().is_some_and(|p| p.id == owner)
            })
            .count();
        let paged_skins = first_page
            .groups
            .iter()
            .filter(|g| g.parent.as_ref().is_some_and(|p| p.id == owner))
            .count();
        ensure!(
            all_skins > 1 && paged_skins == all_skins,
            "Character pagination split the skins"
        );
        let searched = catalog.page("character", "相原舞", 0, false);
        ensure!(
            searched.total == 1 && searched.groups.len() == all_skins,
            "Character name search must preserve all skins"
        );
        let roots = catalog
            .categories
            .iter()
            .filter(|c| c.parent.is_none() && !matches!(c.id.as_str(), "spine" | "other"))
            .map(|c| c.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            roots,
            [
                "character",
                "item",
                "gift",
                "decoration",
                "illustration",
                "title",
                "rank",
                "extras"
            ]
        );
        assert_eq!(
            catalog
                .categories
                .iter()
                .filter(|c| c.parent.as_deref() == Some("decoration"))
                .count(),
            11
        );
        for (folder, name) in [
            ("400501", "相原舞"),
            ("400502", "契约"),
            ("402601", "雏桃"),
            ("ailisha_99", "滤镜"),
            ("erzhigonghua_99", "滤镜"),
            ("guixiaotailang_99", "滤镜"),
            ("huiyeji_jia", "辉夜姬-假"),
            ("jichuanxiang_99", "姬川响"),
        ] {
            let group = catalog
                .groups
                .iter()
                .find(|g| g.path == format!("myassets/deco/character/{folder}"))
                .context("Missing character folder")?;
            ensure!(group.label.contains(name), "{folder}: {}", group.label);
            ensure!(group.parent.is_some(), "Missing character parent: {folder}");
            if folder.contains("_99") || folder == "huiyeji_jia" {
                ensure!(group.story, "Missing story tag");
            }
            println!("{folder}: {}", group.label);
        }
        let items = catalog
            .groups
            .iter()
            .filter(|g| matches!(g.category.as_str(), "item" | "gift" | "rank"))
            .flat_map(|g| g.resources.iter().filter_map(|r| r.item.clone()))
            .collect::<Vec<_>>();
        // Item IDs and rank IDs live in distinct ranges; atlas helpers are negative.
        let icons = super::super::item_icons::decode_items(&library, &names, &items);
        for icon in &icons {
            ensure!(icon.error.is_none(), "{}: {:?}", icon.item_id, icon.error);
        }
        println!("Decoded {} business icon entries", icons.len());
        ensure!(
            catalog.page("gift", "曲奇", 0, false).total >= 3,
            "Gift search"
        );
        ensure!(
            catalog.page("rank", "初心", 0, false).total == 2,
            "Rank search"
        );
        ensure!(
            catalog.page("item", "日光福袋", 0, false).total == 1,
            "Item search"
        );
        Ok(())
    }
}
