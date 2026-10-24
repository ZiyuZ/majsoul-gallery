//! Shared, bounded LRU for parsed Unity bundles. Never hashes bundle contents.
use super::{Library, index::fingerprint, names::safe_bundle};
use anyhow::{Context, Result, ensure};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex, OnceLock},
};
use unity_rs_core::{loader::AssetLoadOptions, studio::Studio};
const BUDGET: u64 = 512 * 1024 * 1024;
#[derive(Clone, PartialEq, Eq)]
struct Key {
    path: PathBuf,
    unity: String,
    stamp: (u64, u64),
}
pub struct Bundle {
    studio: Studio,
    json: Mutex<BTreeMap<String, Arc<Value>>>,
}
impl Deref for Bundle {
    type Target = Studio;
    fn deref(&self) -> &Studio {
        &self.studio
    }
}
impl Bundle {
    pub fn json(&self, path: &str) -> Result<Arc<Value>> {
        let mut maps = self.json.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(value) = maps.get(path) {
            return Ok(value.clone());
        }
        let object = super::preview::direct_object(&self.studio, path)?
            .filter(|o| o.class_id() == 49)
            .or_else(|| {
                self.studio
                    .objects()
                    .find(|o| o.class_id() == 49 && o.container() == Some(path))
            })
            .context("图集配置对象缺失")?;
        let value = Arc::new(serde_json::from_slice(
            &object.read_text_bytes(4 * 1024 * 1024)?,
        )?);
        if maps.len() < 16 {
            maps.insert(path.into(), Arc::clone(&value));
        }
        Ok(value)
    }
}
struct Entry {
    key: Key,
    bundle: Arc<Bundle>,
    weight: u64,
}
#[derive(Default)]
struct Cache {
    entries: VecDeque<Entry>,
    bytes: u64,
    hits: u64,
    loads: u64,
}
static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
fn cache() -> &'static Mutex<Cache> {
    CACHE.get_or_init(|| Mutex::new(Cache::default()))
}
#[derive(Clone, Copy, Serialize)]
pub struct Stats {
    pub hits: u64,
    pub loads: u64,
    pub retained_bytes_estimate: u64,
}
pub fn stats() -> Stats {
    let c = cache().lock().unwrap_or_else(|e| e.into_inner());
    Stats {
        hits: c.hits,
        loads: c.loads,
        retained_bytes_estimate: c.bytes,
    }
}
pub fn open(library: &Library, name: &str) -> Result<Arc<Bundle>> {
    let path = safe_bundle(&library.index.source, name)?;
    let stamp = fingerprint(&path)?;
    if let Some(indexed) = library.index.bundles.get(name) {
        ensure!(
            stamp == (indexed.size, indexed.modified),
            "源资源已更新，请重建索引：{name}"
        );
    }
    let key = Key {
        path,
        unity: library.index.unity.clone(),
        stamp,
    };
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if let Some(i) = c.entries.iter().position(|e| e.key == key) {
        let entry = c.entries.remove(i).unwrap();
        let bundle = entry.bundle.clone();
        c.entries.push_back(entry);
        c.hits += 1;
        return Ok(bundle);
    }
    // Discard stale versions of the same bundle before loading its replacement.
    let mut i = 0;
    while i < c.entries.len() {
        if c.entries[i].key.path == key.path {
            let old = c.entries.remove(i).unwrap();
            c.bytes -= old.weight;
        } else {
            i += 1;
        }
    }
    // Loading is serialized to prevent two workers expanding the same cache miss.
    // Texture decoding/PNG encoding happens after this lock is released.
    let studio = Studio::open_with_options(
        &key.path,
        AssetLoadOptions {
            unity_version_override: Some(key.unity.parse()?),
            ..Default::default()
        },
    )?;
    let expanded = studio
        .collection()
        .serialized_files()
        .iter()
        .map(|f| f.file.region().len())
        .sum::<u64>()
        + studio.resources().map(|r| r.byte_size()).sum::<u64>();
    // Reserve for metadata/JSON and the Unity reader's own decoded Sprite-page cache.
    let sprite_reserve = if studio.objects().any(|o| o.class_id() == 213) {
        256 * 1024 * 1024
    } else {
        0
    };
    let weight = expanded
        .saturating_add(studio.object_count() as u64 * 512)
        .saturating_add(16 * 1024 * 1024)
        .saturating_add(sprite_reserve);
    let bundle = Arc::new(Bundle {
        studio,
        json: Mutex::new(BTreeMap::new()),
    });
    c.loads += 1;
    if weight <= BUDGET {
        while c.bytes + weight > BUDGET {
            // Keep entries currently used by decoders, so another worker can
            // still share them instead of opening the same bundle again.
            if let Some(i) = c
                .entries
                .iter()
                .position(|e| Arc::strong_count(&e.bundle) == 1)
            {
                let old = c.entries.remove(i).unwrap();
                c.bytes -= old.weight;
            } else {
                return Ok(bundle);
            }
        }
        c.bytes += weight;
        c.entries.push_back(Entry {
            key,
            bundle: bundle.clone(),
            weight,
        });
    }
    Ok(bundle)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires local installation and saved index"]
    fn local_bundle_is_reused_and_a_changed_file_invalidates_it() -> Result<()> {
        let original = Library::open(None)?;
        let (name, _) = original
            .index
            .bundles
            .iter()
            .find(|(_, b)| b.size < 512 * 1024 && !b.assets.is_empty())
            .context("small fixture bundle missing")?;
        let temp = tempfile::tempdir()?;
        let path = temp.path().join(name);
        std::fs::copy(original.index.source.join(name), &path)?;
        let mut library = Library {
            index: original.index.clone(),
            index_path: temp.path().join("index.json"),
        };
        library.index.source = temp.path().to_owned();
        let stamp = fingerprint(&path)?;
        let indexed = library.index.bundles.get_mut(name).unwrap();
        indexed.size = stamp.0;
        indexed.modified = stamp.1;
        let first = open(&library, name)?;
        let second = open(&library, name)?;
        assert!(Arc::ptr_eq(&first, &second));
        assert!(stats().retained_bytes_estimate <= BUDGET);
        let changed_time = std::time::SystemTime::now() + std::time::Duration::from_secs(2);
        std::fs::File::options()
            .write(true)
            .open(&path)?
            .set_times(std::fs::FileTimes::new().set_modified(changed_time))?;
        assert!(
            open(&library, name).is_err(),
            "stale index must not reuse a cached bundle"
        );
        library.index.bundles.get_mut(name).unwrap().modified = fingerprint(&path)?.1;
        let third = open(&library, name)?;
        assert!(!Arc::ptr_eq(&first, &third));
        assert_eq!(first.object_count(), third.object_count());
        Ok(())
    }
}
