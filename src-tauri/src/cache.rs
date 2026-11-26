use std::collections::BTreeMap;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsutil::write_atomic;

pub const CACHE_FILE_NAME: &str = ".sltpreview-cache.json";
pub const CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheEntry {
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub modified_ms: u64,
    #[serde(default)]
    pub preview_rel: Option<String>,
    #[serde(default)]
    pub preview_rendered_ms: Option<u64>,
    #[serde(default)]
    pub triangles: Option<u64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub custom_preview_rel: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheFile {
    pub version: u32,
    #[serde(default)]
    pub scanned_at_ms: Option<u64>,
    #[serde(default)]
    pub entries: BTreeMap<String, CacheEntry>,
}

impl Default for CacheFile {
    fn default() -> Self {
        CacheFile {
            version: CACHE_VERSION,
            scanned_at_ms: None,
            entries: BTreeMap::new(),
        }
    }
}

pub fn cache_path(root: &Path) -> PathBuf {
    root.join(CACHE_FILE_NAME)
}

pub fn load_cache(root: &Path) -> Result<CacheFile, String> {
    let path: PathBuf = cache_path(root);
    match fs::read(&path) {
        Ok(bytes) => Ok(serde_json::from_slice::<CacheFile>(&bytes)
            .ok()
            .filter(|cache| cache.version == CACHE_VERSION)
            .unwrap_or_default()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(CacheFile::default()),
        Err(error) => Err(format!("Cannot read {}: {error}", path.display())),
    }
}

pub fn save_cache(root: &Path, cache: &CacheFile) -> Result<(), String> {
    let bytes: Vec<u8> =
        serde_json::to_vec_pretty(cache).map_err(|e| format!("Cannot serialize cache: {e}"))?;
    write_atomic(&cache_path(root), &bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::unique_temp_dir;

    #[test]
    fn cache_round_trip_uses_camel_case() {
        let root: PathBuf = unique_temp_dir("cache");
        assert_eq!(load_cache(&root), Ok(CacheFile::default()));
        let mut cache: CacheFile = CacheFile {
            scanned_at_ms: Some(1_730_000_000_000),
            ..CacheFile::default()
        };
        cache.entries.insert(
            "toys/cube.stl".to_string(),
            CacheEntry {
                size_bytes: 684,
                modified_ms: 1_729_999_999_000,
                preview_rel: Some("toys/img/cube.png".to_string()),
                preview_rendered_ms: Some(1_730_000_000_000),
                triangles: Some(12),
                error: None,
                custom_preview_rel: Some("toys/img/cube.custom.png".to_string()),
            },
        );
        assert_eq!(save_cache(&root, &cache), Ok(()));
        assert_eq!(load_cache(&root), Ok(cache));
        let raw: String = fs::read_to_string(cache_path(&root)).unwrap_or_default();
        for key in [
            "scannedAtMs",
            "sizeBytes",
            "modifiedMs",
            "previewRel",
            "previewRenderedMs",
            "customPreviewRel",
        ] {
            assert!(raw.contains(key), "missing {key}");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn phase_one_entries_default_custom_preview_to_none() {
        let raw: &str = r#"{"version":1,"scannedAtMs":5,"entries":{"a.stl":{"sizeBytes":1,"modifiedMs":2,"previewRel":"img/a.png","previewRenderedMs":3,"triangles":4,"error":null}}}"#;
        let parsed: CacheFile = serde_json::from_str(raw).unwrap_or_default();
        let entry: Option<&CacheEntry> = parsed.entries.get("a.stl");
        assert_eq!(entry.map(|e| e.custom_preview_rel.clone()), Some(None));
        assert_eq!(entry.map(|e| e.size_bytes), Some(1));
    }
}
