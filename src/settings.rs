use anyhow::Result;
use serde_json::{Value, json};
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub assets: Option<PathBuf>,
    pub installation: Option<PathBuf>,
    pub dark: bool,
    pub card_size: usize,
    pub training_columns: usize,
    pub bond_hearts: bool,
    pub skins_grouped: bool,
    pub skin_full_art: bool,
    pub descending: bool,
    pub sidebar_compact: bool,
    pub login_username: String,
    pub remember_password: bool,
    pub auto_login: bool,
    pub device_id: String,
    pub sync_minutes: u64,
}

impl Settings {
    fn from_json(value: Value) -> Self {
        Self {
            assets: value["assets"].as_str().map(PathBuf::from),
            installation: value["installation"].as_str().map(PathBuf::from),
            dark: value["dark"].as_bool().unwrap_or(false),
            card_size: value["card_size"]
                .as_u64()
                .filter(|size| *size <= 2)
                .unwrap_or(1) as usize,
            training_columns: value["training_columns"]
                .as_u64()
                .filter(|n| [1, 2, 3].contains(n))
                .unwrap_or(2) as usize,
            bond_hearts: value["bond_hearts"].as_bool().unwrap_or(true),
            skins_grouped: value["skins_grouped"].as_bool().unwrap_or(true),
            skin_full_art: value["skin_full_art"].as_bool().unwrap_or(false),
            descending: value["descending"].as_bool().unwrap_or(false),
            sidebar_compact: value["sidebar_compact"].as_bool().unwrap_or(false),
            login_username: value["login_username"].as_str().unwrap_or_default().into(),
            remember_password: value["remember_password"].as_bool().unwrap_or(false),
            auto_login: value["remember_password"].as_bool().unwrap_or(false)
                && value["auto_login"].as_bool().unwrap_or(false),
            device_id: value["device_id"]
                .as_str()
                .filter(|id| uuid::Uuid::parse_str(id).is_ok())
                .map(str::to_owned)
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
            sync_minutes: value["sync_minutes"]
                .as_u64()
                .filter(|v| [0, 15, 60, 180].contains(v))
                .unwrap_or(0),
        }
    }

    pub fn load() -> Self {
        let value = fs::read(crate::model::state_dir().join("settings.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null);
        Self::from_json(value)
    }

    pub fn save(&self) -> Result<()> {
        let dir = crate::model::state_dir();
        crate::files::write_json(
            &dir.join("settings.json"),
            &json!({
                "assets": self.assets,
                "installation": self.installation,
                "dark": self.dark,
                "card_size": self.card_size,
                "training_columns": self.training_columns,
                "bond_hearts": self.bond_hearts,
                "skins_grouped": self.skins_grouped,
                "skin_full_art": self.skin_full_art,
                "descending": self.descending,
                "sidebar_compact": self.sidebar_compact,
                "login_username": self.login_username,
                "remember_password": self.remember_password,
                "auto_login": self.auto_login,
                "device_id": self.device_id,
                "sync_minutes": self.sync_minutes,
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initial_preferences_use_defaults_and_validate_auto_login() {
        let settings = Settings::from_json(json!({"assets": "D:/majsoul-assets"}));
        assert_eq!(settings.assets, Some(PathBuf::from("D:/majsoul-assets")));
        assert!(!settings.dark);
        assert_eq!(settings.card_size, 1);
        assert_eq!(settings.training_columns, 2);
        assert!(settings.bond_hearts);
        assert!(settings.skins_grouped);
        assert!(!settings.descending);
        assert!(!settings.sidebar_compact);
        assert_eq!(settings.sync_minutes, 0);
        assert!(!settings.remember_password);
        assert!(!settings.auto_login);
        assert!(
            Settings::from_json(json!({"remember_password":true,"auto_login":true})).auto_login
        );
        assert!(
            !Settings::from_json(json!({"remember_password":false,"auto_login":true})).auto_login
        );
    }

    #[test]
    fn restores_preferences_and_rejects_out_of_range_card_sizes() {
        let settings = Settings::from_json(json!({
            "dark": true, "card_size": 2, "skins_grouped": false, "descending": true,
            "sidebar_compact":true,"login_username":"雀士","sync_minutes":60,"training_columns":1,"bond_hearts":false
        }));
        assert!(settings.dark);
        assert_eq!(settings.card_size, 2);
        assert_eq!(settings.training_columns, 1);
        assert!(!settings.bond_hearts);
        assert!(!settings.skins_grouped);
        assert!(settings.descending);
        assert!(settings.sidebar_compact);
        assert_eq!(settings.login_username, "雀士");
        assert_eq!(settings.sync_minutes, 60);
        assert_eq!(
            Settings::from_json(json!({"sync_minutes":17})).sync_minutes,
            0
        );
        assert_eq!(
            Settings::from_json(json!({"training_columns":3})).training_columns,
            3
        );
        for size in [json!(-1), json!(4), json!("large")] {
            assert_eq!(Settings::from_json(json!({"card_size": size})).card_size, 1);
            assert_eq!(
                Settings::from_json(json!({"training_columns": size})).training_columns,
                2
            );
        }
    }
}
