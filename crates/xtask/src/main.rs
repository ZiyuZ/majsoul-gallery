use std::{fs, path::Path, time::Duration};

use anyhow::{Context, Result, bail, ensure};
use flate2::read::GzDecoder;
use sha2::{Digest, Sha256};

const ARCHIVE_HASH: &str = "5e4cbd4b95cd48d91f36f7384b1361a12c668275cf004cc20c3014042b73cf70";
const BUILD: &str = "cargo build --release --features spine --config 'patch.crates-io.rusty_spine.path=\"vendor/rusty-spine\"'";
const WEIGHTED_FIX: &str = include_str!("weighted-vertices.txt");

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [command] if command == "prepare-spine" => {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
            let runtime = root.join("vendor/rusty-spine");
            if !runtime.exists() {
                let archive = root.join(".local/downloads/rusty_spine-0.8.0.crate");
                if !archive.exists() {
                    println!("Downloading rusty_spine 0.8.0...");
                    let bytes = reqwest::blocking::Client::builder()
                        .timeout(Duration::from_secs(120))
                        .build()?
                        .get("https://static.crates.io/crates/rusty_spine/rusty_spine-0.8.0.crate")
                        .send()?
                        .error_for_status()?
                        .bytes()?;
                    verify_archive(&bytes)?;
                    fs::create_dir_all(archive.parent().unwrap())?;
                    fs::write(&archive, bytes)?;
                }
                extract(&fs::read(&archive)?, &runtime)?;
            }
            let count = patch_runtime(&runtime)?;
            println!("rusty_spine 0.8.0 ready; applied {count} fixes.\n{BUILD}");
            Ok(())
        }
        [] => {
            println!("Usage: cargo xtask prepare-spine");
            Ok(())
        }
        _ => bail!("Usage: cargo xtask prepare-spine"),
    }
}

fn checksum(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn verify_archive(bytes: &[u8]) -> Result<()> {
    ensure!(
        checksum(bytes) == ARCHIVE_HASH,
        "Archive does not match rusty_spine 0.8.0. Remove .local/downloads/rusty_spine-0.8.0.crate and retry."
    );
    Ok(())
}

fn extract(bytes: &[u8], runtime: &Path) -> Result<()> {
    verify_archive(bytes)?;
    let parent = runtime
        .parent()
        .context("Missing runtime parent directory")?;
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    tar::Archive::new(GzDecoder::new(bytes)).unpack(staging.path())?;
    fs::rename(staging.path().join("rusty_spine-0.8.0"), runtime)?;
    Ok(())
}

fn patch_runtime(runtime: &Path) -> Result<usize> {
    let manifest = fs::read_to_string(runtime.join("Cargo.toml"))?;
    let package = manifest
        .split("[package]")
        .nth(1)
        .context("Missing package metadata")?
        .split("\n[")
        .next()
        .unwrap();
    ensure!(
        package
            .lines()
            .any(|line| line.trim() == "name = \"rusty_spine\"")
            && package
                .lines()
                .any(|line| line.trim() == "version = \"0.8.0\""),
        "Expected rusty_spine 0.8.0"
    );
    ensure!(
        runtime.join("LICENSE").is_file(),
        "Runtime LICENSE is missing"
    );

    // Validate all source files before writing either fix.
    let mut changes = Vec::new();
    for (file, original, patched) in [
        (
            "src/c/wasm.rs",
            "62af058bbe209ecdc0d93515a121182feab4f5774508c156e5f6b20d953c338d",
            "72557614d42cbdc0749512b8b27a15db812affeada23b0d05fe87de8c23be893",
        ),
        (
            "src/c/spine_c.rs",
            "a17e0b4ddbd67df449077a49ea8341f5c5001c3e4fddd7da3eec72f9336edccb",
            "10c46f7db69333f4b692baeef43804987df3e16c6982ddad6e51c36ee735d8cc",
        ),
    ] {
        let path = runtime.join(file);
        let text = fs::read_to_string(&path)?.replace("\r\n", "\n");
        let hash = checksum(text.as_bytes());
        if hash == patched {
            continue;
        }
        ensure!(
            hash == original,
            "Unexpected source in {file}; files were left unchanged"
        );
        let updated = if file.ends_with("wasm.rs") {
            text.replace(
                "layout.size() as size_t",
                "layout.size().min(size) as size_t",
            )
        } else {
            patch_vertices(&text)?
        };
        ensure!(
            checksum(updated.as_bytes()) == patched,
            "Patch verification failed for {file}"
        );
        changes.push((path, updated));
    }
    for (path, text) in &changes {
        fs::write(path, text)?;
    }
    Ok(changes.len())
}

fn patch_vertices(text: &str) -> Result<String> {
    let function = text
        .find("unsafe extern \"C\" fn _readVerticesBinary(")
        .context("Weighted vertex function was not found")?;
    let start = function
        + text[function..]
            .find("\n    }\n")
            .context("Unweighted vertex branch was not found")?
        + "\n    }\n".len();
    let end = start
        + text[start..]
            .find("\n}")
            .context("Weighted vertex function end was not found")?
        + 1;
    Ok(format!(
        "{}{}\n\n{}",
        &text[..start],
        WEIGHTED_FIX.replace("\r\n", "\n").trim_end(),
        &text[end..]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_archive_does_not_create_runtime() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let runtime = temp.path().join("vendor/rusty-spine");
        assert!(extract(b"invalid archive", &runtime).is_err());
        assert!(!runtime.exists());
        Ok(())
    }

    #[test]
    #[ignore = "requires .local/downloads/rusty_spine-0.8.0.crate"]
    fn published_archive_patches_idempotently_and_rejects_modified_sources() -> Result<()> {
        let archive = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/downloads/rusty_spine-0.8.0.crate");
        let bytes = fs::read(archive)?;
        let temp = tempfile::tempdir()?;
        let runtime = temp.path().join("vendor/rusty-spine");
        extract(&bytes, &runtime)?;
        assert_eq!(patch_runtime(&runtime)?, 2);
        let wasm = runtime.join("src/c/wasm.rs");
        let before = fs::metadata(&wasm)?.modified()?;
        assert_eq!(patch_runtime(&runtime)?, 0);
        assert_eq!(fs::metadata(&wasm)?.modified()?, before);

        // A bad second file must prevent writing the otherwise valid first patch.
        let original = fs::read_to_string(&wasm)?.replace(
            "layout.size().min(size) as size_t",
            "layout.size() as size_t",
        );
        fs::write(&wasm, &original)?;
        let vertices = runtime.join("src/c/spine_c.rs");
        fs::write(&vertices, "modified source")?;
        assert!(patch_runtime(&runtime).is_err());
        assert_eq!(fs::read_to_string(&wasm)?, original);
        assert_eq!(fs::read_to_string(&vertices)?, "modified source");
        assert!(runtime.join("LICENSE").is_file());
        Ok(())
    }
}
