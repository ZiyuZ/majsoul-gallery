//! Replace a completed file atomically, with temporary files on the same volume.
use anyhow::{Context, Result};
use serde::Serialize;
use std::{fs, io::Write, path::Path};

pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)
        .with_context(|| format!("无法创建目录 {}", crate::model::display_path(parent)))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(bytes)?;
    file.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("无法保存文件 {}", crate::model::display_path(path)))?;
    Ok(())
}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    write_atomic(path, &serde_json::to_vec_pretty(value)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replaces_existing_files_and_cleans_failed_temporaries() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let file = directory.path().join("子目录/设置.json");
        write_json(&file, &serde_json::json!({"revision":1}))?;
        write_json(&file, &serde_json::json!({"revision":2}))?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&file)?)?["revision"],
            2
        );
        assert!(write_atomic(file.parent().unwrap(), b"cannot replace directory").is_err());
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        assert_eq!(fs::read_dir(file.parent().unwrap())?.count(), 1);
        Ok(())
    }
}
