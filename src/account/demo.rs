//! Deterministic preview data, independent of real account snapshots.
use super::*;

impl AccountSnapshot {
    pub fn demo(library: &Library) -> Self {
        let mut characters = BTreeMap::new();
        let mut inventory = BTreeMap::new();
        let mut owned_skins = BTreeSet::new();
        for (i, character) in library.characters.iter().enumerate() {
            let owned = i % 5 != 4;
            let contracted = owned && i % 7 == 0;
            if owned {
                owned_skins.extend(character.skins.iter().take(2).map(|s| s.id.clone()));
            }
            characters.insert(
                character.id.clone(),
                CharacterState {
                    owned,
                    contracted,
                    favorite: owned && i % 4 == 0,
                    bond: if !owned {
                        0
                    } else if contracted || i % 11 == 1 {
                        crate::bond::MAX_BOND
                    } else {
                        (i as u64 * 7_321 + 26_800) % crate::bond::MAX_BOND
                    },
                },
            );
            for item in character
                .gifts
                .iter()
                .chain(character.materials.iter().flat_map(|m| &m.items))
            {
                let count = item.id.parse::<u64>().unwrap_or(0) % 17;
                inventory.entry(item.id.clone()).or_insert(count);
            }
        }
        // Give the preview a few material-ready, uncontracted characters.
        for character in library
            .characters
            .iter()
            .enumerate()
            .filter_map(|(i, c)| (i % 11 == 1).then_some(c))
        {
            for group in &character.materials {
                if let Some(item) = group.display_item() {
                    let count = inventory.entry(item.id.clone()).or_default();
                    *count = (*count).max(group.quantity);
                }
            }
        }
        for (i, item) in library.items.iter().enumerate() {
            inventory
                .entry(item.definition.id.clone())
                .or_insert(if i % 4 == 3 {
                    0
                } else {
                    (i as u64 * 7 + 3) % 23
                });
        }
        let character_order = library
            .characters
            .iter()
            .filter(|c| characters.get(&c.id).is_some_and(|s| s.favorite))
            .map(|c| c.id.clone())
            .chain(
                library
                    .characters
                    .iter()
                    .filter(|c| !characters.get(&c.id).is_some_and(|s| s.favorite))
                    .map(|c| c.id.clone()),
            )
            .collect();
        Self {
            profile: AccountProfile {
                uid: "100000001".into(),
                nickname: "示例雀士".into(),
                rank_four: "10401".into(),
                rank_three: "20301".into(),
                rank_four_details: "2 星 · 1600 分".into(),
                rank_three_details: "1 星 · 820 分".into(),
            },
            is_demo: true,
            synced_at: None,
            characters,
            character_order,
            inventory,
            owned_skins,
            owned_decorations: library
                .decorations
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 4 != 3)
                .map(|(_, d)| d.id.clone())
                .collect(),
            owned_titles: library
                .titles
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 4 != 3)
                .flat_map(|(_, title)| title.ids.iter().cloned())
                .collect(),
            equipped_illustrations: library
                .illustrations
                .iter()
                .enumerate()
                .filter(|(i, _)| i % 4 != 3)
                .map(|(_, item)| item.image_id.clone())
                .collect(),
        }
    }
}
