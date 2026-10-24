use super::{AccountProfile, AccountSnapshot, CharacterState};
use crate::model::Library;
use anyhow::{Context, Result, ensure};
use majsoul_api::Snapshot;
use serde_json::Value;
use std::{collections::BTreeSet, fs, path::Path};

fn number(value: &Value) -> u64 {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .unwrap_or(0)
}
fn ids(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(number)
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
        .collect()
}
fn rank(value: &Value) -> String {
    let id = number(&value["id"]);
    if id == 0 {
        String::new()
    } else {
        (id / 100 * 100 + 1).to_string()
    }
}
fn cumulative_bond(level: u64, experience: u64) -> u64 {
    const START: [u64; 6] = [0, 2000, 8000, 18000, 38000, 88000];
    START[level.min(5) as usize]
        .saturating_add(experience)
        .min(crate::bond::MAX_BOND)
}
fn rank_details(value: &Value) -> String {
    let id = number(&value["id"]);
    if id == 0 {
        return String::new();
    }
    let step = id % 100;
    let score = number(&value["score"]);
    if id / 100 % 100 >= 6 {
        format!("Lv.{step} · {score} 分")
    } else {
        format!("{step} 星 · {score} 分")
    }
}

impl AccountSnapshot {
    pub fn from_api(source: &Snapshot, library: &Library) -> Result<Self> {
        ensure!(
            source.region == "cn"
                && source.uid > 0
                && number(&source.account["account_id"]) == u64::from(source.uid),
            "账号快照身份不匹配"
        );
        let favorites = ids(&source.characters["character_sort"]);
        let mut snapshot = Self {
            profile: AccountProfile {
                uid: source.uid.to_string(),
                nickname: source.account["nickname"]
                    .as_str()
                    .unwrap_or_default()
                    .into(),
                rank_four: rank(&source.account["level"]),
                rank_three: rank(&source.account["level3"]),
                rank_four_details: rank_details(&source.account["level"]),
                rank_three_details: rank_details(&source.account["level3"]),
            },
            synced_at: Some(source.synced_at),
            owned_skins: ids(&source.characters["skins"]),
            owned_titles: ids(&source.titles["title_list"]),
            ..Default::default()
        };
        for id in source.characters["character_sort"]
            .as_array()
            .into_iter()
            .flatten()
            .chain(
                source.characters["other_character_sort"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
        {
            let id = number(id);
            if id > 0 && !snapshot.character_order.contains(&id.to_string()) {
                snapshot.character_order.push(id.to_string());
            }
        }
        for character in source.characters["characters"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let id = number(&character["charid"]);
            if id == 0 {
                continue;
            }
            let id = id.to_string();
            let contracted = character["is_upgraded"].as_bool().unwrap_or(false);
            snapshot.characters.insert(
                id.clone(),
                CharacterState {
                    owned: true,
                    contracted,
                    bond: cumulative_bond(number(&character["level"]), number(&character["exp"])),
                    favorite: favorites.contains(&id),
                },
            );
            let current_skin = number(&character["skin"]);
            if current_skin > 0 {
                snapshot.owned_skins.insert(current_skin.to_string());
            }
            // The game grants initial/contract skins without listing them as purchased skins.
            let path = library.root.join(format!("characters/{id}/character.json"));
            if path.is_file() {
                let metadata: Value = serde_json::from_slice(&fs::read(path)?)?;
                for key in [Some("initial_skin"), contracted.then_some("contract_skin")]
                    .into_iter()
                    .flatten()
                {
                    let skin = number(&metadata[key]);
                    if skin > 0 {
                        snapshot.owned_skins.insert(skin.to_string());
                    }
                }
            }
        }
        for item in source.bag["bag"]["items"].as_array().into_iter().flatten() {
            let id = number(&item["item_id"]);
            let quantity = number(&item["stack"]);
            if id > 0 && quantity > 0 {
                *snapshot.inventory.entry(id.to_string()).or_default() += quantity;
                snapshot.owned_decorations.insert(id.to_string());
            }
        }
        let frame = number(&source.account["avatar_frame"]);
        if frame > 0 {
            snapshot.owned_decorations.insert(frame.to_string());
        }
        Ok(snapshot)
    }

    pub fn save_account(&self, source: &Snapshot, data: &Path, sync_minutes: u64) -> Result<()> {
        ensure!(
            !self.is_demo && self.profile.uid == source.uid.to_string() && source.region == "cn",
            "账号保存身份不匹配"
        );
        let directory = data.join("accounts/cn").join(source.uid.to_string());
        let value = serde_json::json!({"region":"cn","uid":source.uid,"snapshot":self,"source":source,"sync_minutes":sync_minutes});
        crate::files::write_json(&directory.join("account.json"), &value)
            .context("无法保存账号快照")?;
        crate::files::write_json(
            &data.join("accounts/selected.json"),
            &serde_json::json!({"region":"cn","uid":source.uid}),
        )?;
        Ok(())
    }
}

pub fn default_snapshot_path(data: &Path) -> Result<std::path::PathBuf> {
    let selection: Value = serde_json::from_slice(&fs::read(data.join("accounts/selected.json"))?)?;
    ensure!(selection["region"] == "cn", "账号区服无效");
    let uid = number(&selection["uid"]);
    ensure!(uid > 0 && uid <= u32::MAX as u64, "账号 ID 无效");
    Ok(data
        .join("accounts/cn")
        .join(uid.to_string())
        .join("account.json"))
}

pub fn load_snapshot(path: &Path) -> Result<AccountSnapshot> {
    let value: Value = serde_json::from_slice(&fs::read(path)?)?;
    let snapshot: AccountSnapshot =
        serde_json::from_value(value["snapshot"].clone()).context("账号快照格式无效")?;
    ensure!(
        value["region"] == "cn" && number(&value["uid"]).to_string() == snapshot.profile.uid,
        "账号快照身份不匹配"
    );
    Ok(snapshot)
}

pub fn save_sync_interval(data: &Path, uid: &str, minutes: u64) -> Result<()> {
    let uid: u32 = uid.parse().context("账号 ID 无效")?;
    let directory = data.join("accounts/cn").join(uid.to_string());
    let mut value: Value = serde_json::from_slice(&fs::read(directory.join("account.json"))?)?;
    ensure!(
        number(&value["uid"]) == uid as u64 && value["region"] == "cn",
        "账号设置身份不匹配"
    );
    value["sync_minutes"] = minutes.into();
    crate::files::write_json(&directory.join("account.json"), &value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn account_record_requires_current_envelope_and_matching_identity() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let path = temp.path().join("account.json");
        let mut snapshot = AccountSnapshot::default();
        snapshot.profile.uid = "123".into();
        crate::files::write_json(&path, &snapshot)?;
        assert!(load_snapshot(&path).is_err());
        crate::files::write_json(
            &path,
            &serde_json::json!({"region":"cn","uid":123,"snapshot":snapshot}),
        )?;
        assert_eq!(load_snapshot(&path)?.profile.uid, "123");
        crate::files::write_json(
            &path,
            &serde_json::json!({"region":"cn","uid":456,"snapshot":snapshot}),
        )?;
        assert!(load_snapshot(&path).is_err());
        Ok(())
    }

    #[test]
    fn maps_stage_experience_favorites_inventory_and_granted_skins() -> Result<()> {
        let temp = tempfile::tempdir()?;
        fs::create_dir_all(temp.path().join("characters/200001"))?;
        fs::write(
            temp.path().join("characters/200001/character.json"),
            br#"{"initial_skin":400101,"contract_skin":400102}"#,
        )?;
        let library = Library {
            root: temp.path().to_path_buf(),
            ..Default::default()
        };
        let source = Snapshot {
            region: "cn".into(),
            uid: 123,
            synced_at: std::time::SystemTime::now(),
            account: json!({"account_id":123,"nickname":"test","level":{"id":10403},"level3":{"id":20302}}),
            characters: json!({"character_sort":[200001],"other_character_sort":[200003,200002],"skins":[400103],"characters":[{"charid":200001,"level":2,"exp":4000,"skin":400103,"is_upgraded":true}]}),
            bag: json!({"bag":{"items":[{"item_id":302002,"stack":10},{"item_id":302002,"stack":2},{"item_id":303013,"stack":3},{"item_id":303113,"stack":4}]}}),
            titles: json!({"title_list":[600001]}),
        };
        let snapshot = AccountSnapshot::from_api(&source, &library)?;
        assert_eq!(snapshot.character("200001").bond, 12000);
        assert!(snapshot.character("200001").favorite);
        assert!(snapshot.character("200001").contracted);
        assert_eq!(snapshot.inventory["302002"], 12);
        assert_eq!(snapshot.inventory["303013"], 3);
        assert_eq!(snapshot.inventory["303113"], 4);
        assert_eq!(
            snapshot.available(&crate::model::Requirement {
                quantity: 5,
                items: ["303013", "303113"]
                    .map(|id| crate::model::Item {
                        id: id.into(),
                        name: String::new(),
                        icon: None,
                    })
                    .into(),
            }),
            7
        );
        assert_eq!(snapshot.profile.rank_four, "10401");
        assert_eq!(snapshot.profile.rank_three, "20301");
        assert_eq!(snapshot.character_order, ["200001", "200003", "200002"]);
        assert_eq!(snapshot.profile.rank_four_details, "3 星 · 0 分");
        assert_eq!(snapshot.owned_skins.len(), 3);
        snapshot.save_account(&source, temp.path(), 60)?;
        assert!(temp.path().join("accounts/cn/123/account.json").is_file());
        assert_eq!(
            load_snapshot(&default_snapshot_path(temp.path())?)?
                .profile
                .uid,
            "123"
        );
        save_sync_interval(temp.path(), "123", 15)?;
        let saved: Value =
            serde_json::from_slice(&fs::read(temp.path().join("accounts/cn/123/account.json"))?)?;
        assert_eq!(saved["sync_minutes"], 15);
        let mut second = source.clone();
        second.uid = 456;
        second.account["account_id"] = 456.into();
        let second_snapshot = AccountSnapshot::from_api(&second, &library)?;
        second_snapshot.save_account(&second, temp.path(), 0)?;
        assert_eq!(
            load_snapshot(&default_snapshot_path(temp.path())?)?
                .profile
                .uid,
            "456"
        );
        assert_eq!(
            load_snapshot(&temp.path().join("accounts/cn/123/account.json"))?
                .profile
                .uid,
            "123"
        );
        assert!(snapshot.save_account(&second, temp.path(), 0).is_err());
        assert_eq!(cumulative_bond(5, 999), 88000);
        Ok(())
    }
}
