//! Optional portable plaintext HMAC credentials, keyed by region and login name.
use anyhow::{Context, Result, ensure};
use majsoul_api::Zeroizing;
use std::{collections::BTreeMap, fs, path::Path};
pub const SAVED_PASSWORD_MASK: &str = "********";

/// Resolve either newly entered text or a saved credential belonging to this login name.
pub fn resolve(
    data: &Path,
    username: &str,
    typed: &str,
    saved_for: Option<&str>,
    remember: bool,
) -> Result<Option<Zeroizing<String>>> {
    if typed == SAVED_PASSWORD_MASK && saved_for.is_some() {
        return if remember && saved_for == Some(username) {
            load(data, username)
        } else {
            Ok(None)
        };
    }
    if !typed.is_empty() {
        return Ok(Some(majsoul_api::derive_password(typed)));
    }
    if remember {
        load(data, username)
    } else {
        Ok(None)
    }
}

fn read(data: &Path) -> Result<BTreeMap<String, String>> {
    let path = data.join("accounts/credentials.json");
    if !path.exists() {
        return Ok(BTreeMap::new());
    }
    serde_json::from_slice(&fs::read(path)?).context("无法读取已记住的登录信息")
}
pub fn load(data: &Path, username: &str) -> Result<Option<Zeroizing<String>>> {
    Ok(read(data)?
        .remove(&format!("cn/{username}"))
        .map(Zeroizing::new))
}
pub fn save(data: &Path, username: &str, hash: Option<&str>) -> Result<()> {
    let mut values = read(data)?;
    let key = format!("cn/{username}");
    if let Some(hash) = hash {
        ensure!(
            hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "密码派生值格式无效"
        );
        values.insert(key, hash.into());
    } else {
        values.remove(&key);
    }
    let directory = data.join("accounts");
    crate::files::write_json(&directory.join("credentials.json"), &values)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remembered_credentials_are_separate_and_removable() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let hash = majsoul_api::derive_password("example");
        save(dir.path(), "first", Some(&hash))?;
        assert_eq!(load(dir.path(), "first")?.as_deref(), Some(&*hash));
        assert!(load(dir.path(), "second")?.is_none());
        save(dir.path(), "first", None)?;
        assert!(load(dir.path(), "first")?.is_none());
        Ok(())
    }
    #[test]
    fn displayed_mask_is_never_sent_as_a_password_or_reused_for_another_account() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let hash = majsoul_api::derive_password("first-password");
        save(directory.path(), "first", Some(&hash))?;
        assert_eq!(
            resolve(
                directory.path(),
                "first",
                SAVED_PASSWORD_MASK,
                Some("first"),
                true
            )?
            .as_deref(),
            Some(&*hash)
        );
        assert!(
            resolve(
                directory.path(),
                "second",
                SAVED_PASSWORD_MASK,
                Some("first"),
                true
            )?
            .is_none()
        );
        assert!(
            resolve(
                directory.path(),
                "first",
                SAVED_PASSWORD_MASK,
                Some("first"),
                false
            )?
            .is_none()
        );
        let changed = majsoul_api::derive_password("replacement");
        assert_eq!(
            resolve(
                directory.path(),
                "first",
                "replacement",
                Some("first"),
                true
            )?
            .as_deref(),
            Some(&*changed)
        );
        Ok(())
    }
}
