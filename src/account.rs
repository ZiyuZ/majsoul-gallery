//! Account snapshots shared by the UI, report CLI and API synchronisation.
mod api;
pub mod credentials;
pub use api::{default_snapshot_path, load_snapshot, save_sync_interval};
mod demo;
pub mod network;
use crate::model::{Character, Decoration, Library, Requirement, Skin, Title};
use std::collections::{BTreeMap, BTreeSet};

/// Account data shared by views and portable reports; `demo` builds the debug fixture.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AccountSnapshot {
    pub is_demo: bool,
    pub profile: AccountProfile,
    pub synced_at: Option<std::time::SystemTime>,
    pub characters: BTreeMap<String, CharacterState>,
    pub inventory: BTreeMap<String, u64>,
    pub owned_skins: BTreeSet<String>,
    pub owned_titles: BTreeSet<String>,
    #[serde(skip)]
    // Derived from the raw account's currently selected loading images when a snapshot is loaded.
    pub equipped_illustrations: BTreeSet<String>,
    pub owned_decorations: BTreeSet<String>,
    pub character_order: Vec<String>,
}
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AccountProfile {
    pub uid: String,
    pub nickname: String,
    pub rank_four: String,
    pub rank_three: String,
    pub rank_four_details: String,
    pub rank_three_details: String,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum CharacterOrder {
    #[default]
    Game,
    Release,
    Bond,
}
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CharacterState {
    pub owned: bool,
    pub contracted: bool,
    pub bond: u64,
    pub favorite: bool,
}
impl AccountSnapshot {
    pub fn sort_characters(
        &self,
        characters: &mut [(usize, &Character)],
        order: CharacterOrder,
        descending: bool,
    ) {
        let mut positions = BTreeMap::new();
        if order == CharacterOrder::Game {
            for (position, id) in self.character_order.iter().enumerate() {
                positions.entry(id.as_str()).or_insert(position as u64);
            }
        }
        characters.sort_by_key(|(_, c)| {
            let primary = match order {
                CharacterOrder::Game => positions.get(c.id.as_str()).copied().unwrap_or(u64::MAX),
                CharacterOrder::Release => 0,
                CharacterOrder::Bond => u64::MAX - self.character(&c.id).bond,
            };
            (primary, c.sort)
        });
        if descending {
            characters.reverse();
        }
    }
    /// New assets may appear before the next account sync.
    pub fn character(&self, id: &str) -> CharacterState {
        self.characters.get(id).copied().unwrap_or_default()
    }
    /// Asset refreshes only regenerate the explicit debug fixture.
    pub fn refresh_library(&mut self, library: &Library) {
        if self.is_demo {
            let synced_at = self.synced_at;
            *self = Self::demo(library);
            self.synced_at = synced_at;
        }
    }

    pub fn owns_title(&self, title: &Title) -> bool {
        title.ids.iter().any(|id| self.owned_titles.contains(id))
    }
    pub fn owns_illustration(&self, item: &crate::model::Illustration) -> bool {
        self.equipped_illustrations.contains(&item.image_id)
            || item
                .unlock_items
                .iter()
                .any(|id| self.inventory.get(id).is_some_and(|count| *count > 0))
    }
    pub fn contract_materials_ready(&self, character: &Character) -> bool {
        self.characters
            .get(&character.id)
            .is_some_and(|state| state.owned && !state.contracted)
            && !character.materials.is_empty()
            && character
                .materials
                .iter()
                .all(|group| self.available(group) >= group.quantity)
    }
    pub fn can_contract(&self, character: &Character) -> bool {
        self.character(&character.id).bond >= crate::bond::MAX_BOND
            && self.contract_materials_ready(character)
    }
    pub fn owns_skin(&self, skin: &Skin) -> bool {
        self.owned_skins.contains(&skin.id)
    }
    pub fn owns_decoration(&self, decoration: &Decoration) -> bool {
        self.owned_decorations.contains(&decoration.id)
    }
    pub fn available(&self, group: &Requirement) -> u64 {
        let ids = group
            .items
            .iter()
            .map(|i| &i.id)
            .collect::<std::collections::BTreeSet<_>>();
        ids.into_iter()
            .map(|id| self.inventory.get(id).copied().unwrap_or(0))
            .sum()
    }
}

/// The currently selected account. Asset libraries are shared independently.
#[derive(Default)]
pub struct AccountSession {
    pub snapshot: AccountSnapshot,
    connected: bool,
}
impl AccountSession {
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn activate(&mut self, snapshot: AccountSnapshot) {
        self.snapshot = snapshot;
        self.connected = true;
    }

    pub fn disconnect(&mut self) {
        *self = Self::default();
    }

    pub fn refresh_library(&mut self, library: &Library) {
        self.snapshot.refresh_library(library);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{model::Item, search::SearchIndex};
    #[test]
    fn sorting_preserves_user_order_and_supports_bond_and_reverse() {
        let characters = (1..=3)
            .map(|i| Character {
                id: i.to_string(),
                name: i.to_string(),
                sort: i,
                search: SearchIndex::new("", ""),
                sex: 1,
                limited: false,
                collaboration: false,
                skins: vec![],
                gifts: vec![],
                materials: vec![],
            })
            .collect::<Vec<_>>();
        let mut snapshot = AccountSnapshot {
            character_order: vec!["3".into(), "1".into()],
            ..Default::default()
        };
        snapshot.characters.insert(
            "2".into(),
            CharacterState {
                bond: 4000,
                ..Default::default()
            },
        );
        let mut rows = characters.iter().enumerate().collect::<Vec<_>>();
        snapshot.sort_characters(&mut rows, CharacterOrder::Game, false);
        assert_eq!(
            rows.iter().map(|(_, c)| c.id.as_str()).collect::<Vec<_>>(),
            ["3", "1", "2"]
        );
        snapshot.sort_characters(&mut rows, CharacterOrder::Bond, false);
        assert_eq!(rows[0].1.id, "2");
        snapshot.sort_characters(&mut rows, CharacterOrder::Release, true);
        assert_eq!(
            rows.iter().map(|(_, c)| c.id.as_str()).collect::<Vec<_>>(),
            ["3", "2", "1"]
        );
    }
    #[test]
    fn contract_material_filter_requires_inventory_ownership_and_no_contract() {
        let mut character = Character {
            search: SearchIndex::new("", ""),
            id: "1".into(),
            name: "角色".into(),
            sort: 0,
            sex: 1,
            limited: false,
            collaboration: false,
            skins: vec![],
            gifts: vec![],
            materials: vec![Requirement {
                quantity: 10,
                items: vec![Item {
                    id: "stone".into(),
                    name: "材料".into(),
                    icon: None,
                }],
            }],
        };
        let mut account = AccountSnapshot::demo(&Library::default());
        account.characters.insert(
            "1".into(),
            CharacterState {
                owned: true,
                contracted: false,
                bond: 0,
                favorite: false,
            },
        );
        account.inventory.insert("stone".into(), 9);
        assert!(!account.contract_materials_ready(&character));
        account.inventory.insert("stone".into(), 10);
        assert!(account.contract_materials_ready(&character));
        assert!(!account.can_contract(&character));
        account.characters.get_mut("1").unwrap().bond = crate::bond::MAX_BOND;
        assert!(account.can_contract(&character));
        account.characters.get_mut("1").unwrap().contracted = true;
        assert!(!account.contract_materials_ready(&character));
        assert!(!account.can_contract(&character));
        account.characters.get_mut("1").unwrap().contracted = false;
        account.characters.get_mut("1").unwrap().owned = false;
        assert!(!account.contract_materials_ready(&character));
        account.characters.get_mut("1").unwrap().owned = true;
        character.materials.clear();
        assert!(!account.contract_materials_ready(&character));
    }
    #[test]
    fn material_inventory_sums_distinct_business_ids() {
        let a = AccountSnapshot {
            is_demo: false,
            synced_at: None,
            profile: AccountProfile::default(),
            characters: BTreeMap::new(),
            owned_skins: BTreeSet::new(),
            owned_titles: BTreeSet::new(),
            equipped_illustrations: BTreeSet::new(),
            owned_decorations: BTreeSet::new(),
            inventory: BTreeMap::from([("1".into(), 3), ("2".into(), 4)]),
            character_order: vec![],
        };
        let item = |id: &str| Item {
            id: id.into(),
            name: id.into(),
            icon: None,
        };
        let group = Requirement {
            quantity: 10,
            items: vec![item("1"), item("2"), item("1")],
        };
        assert_eq!(a.available(&group), 7);
    }

    #[test]
    fn unknown_character_uses_empty_state() {
        assert_eq!(
            AccountSnapshot::default().character("new-character"),
            CharacterState::default()
        );
    }

    #[test]
    fn refreshing_assets_preserves_account_snapshot() {
        let mut account = AccountSnapshot::default();
        account.profile.uid = "player-1".into();
        account.inventory.insert("stone".into(), 42);
        account.refresh_library(&Library::default());
        assert_eq!(account.profile.uid, "player-1");
        assert_eq!(account.inventory["stone"], 42);
        assert!(!account.is_demo);
    }

    #[test]
    fn switching_account_replaces_all_state_and_sync_time() {
        let mut session = AccountSession::default();
        let mut first = AccountSnapshot::default();
        first.profile.uid = "first".into();
        first.inventory.insert("stone".into(), 42);
        first.owned_skins.insert("skin".into());
        session.activate(first);
        session.snapshot.synced_at = Some(std::time::SystemTime::now());
        let mut second = AccountSnapshot::default();
        second.profile.uid = "second".into();
        session.activate(second);
        assert!(session.is_connected());
        assert_eq!(session.snapshot.profile.uid, "second");
        assert!(session.snapshot.inventory.is_empty());
        assert!(session.snapshot.owned_skins.is_empty());
        assert!(session.snapshot.synced_at.is_none());
        session.disconnect();
        assert!(!session.is_connected());
        assert!(session.snapshot.profile.uid.is_empty());
    }
}

#[cfg(test)]
mod persistence_tests {
    use super::*;
    #[test]
    fn activating_cached_account_restores_its_sync_time() {
        let mut snapshot = AccountSnapshot::default();
        snapshot.profile.uid = "cached-account".into();
        snapshot.synced_at = Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1234));
        let json = serde_json::to_vec(&snapshot).unwrap();
        let restored = serde_json::from_slice(&json).unwrap();
        let mut session = AccountSession::default();
        session.activate(restored);
        assert_eq!(session.snapshot.synced_at, snapshot.synced_at);
    }
}
