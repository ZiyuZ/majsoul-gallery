//! One managed material library, with small persistent import metadata.
use anyhow::{Result, ensure};
use std::{fs, path::Path};

/// Clear only scratch files inside this application's data directory.
pub fn clear_import_cache(data: &Path) -> Result<()> {
    if !data.try_exists()? {
        return Ok(());
    }
    let root = data.canonicalize()?;
    let mut path = root.clone();
    for component in ["import", "cache"] {
        path.push(component);
        if path.try_exists()? {
            let metadata = fs::symlink_metadata(&path)?;
            ensure!(!metadata.file_type().is_symlink(), "素材缓存目录包含链接");
            #[cfg(windows)]
            {
                use std::os::windows::fs::MetadataExt;
                ensure!(
                    metadata.file_attributes() & 0x400 == 0,
                    "素材缓存目录包含重解析点"
                );
            }
            ensure!(
                path.canonicalize()?.starts_with(&root),
                "素材缓存目录超出应用目录"
            );
        }
    }
    if path.try_exists()? {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}
