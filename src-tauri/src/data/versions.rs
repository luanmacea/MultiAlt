use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(target_os = "windows")]
pub(crate) fn atomic_replace(src: &Path, dst: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};

    let src_wide: Vec<u16> = src.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let dst_wide: Vec<u16> = dst.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    let ok = unsafe {
        MoveFileExW(
            src_wide.as_ptr(),
            dst_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        let err = std::io::Error::last_os_error();
        return Err(format!("MoveFileExW failed: {}", err));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn atomic_replace(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::rename(src, dst).map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub channel: String,
    pub version_hash: String,
    pub binary_type: String,
    #[serde(default)]
    pub display_version: Option<String>,
    pub install_path: String,
    #[serde(default)]
    pub install_size_bytes: u64,
    #[serde(default)]
    pub installed_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_launched_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub user_label: Option<String>,
}

impl VersionEntry {
    pub fn version_id(&self) -> String {
        format!("{}:{}", self.channel, self.version_hash)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VersionsCatalogFile {
    #[serde(default)]
    pub installed: Vec<VersionEntry>,
}

pub struct VersionsCatalogStore {
    catalog: Mutex<VersionsCatalogFile>,
    file_path: PathBuf,
}

impl VersionsCatalogStore {
    pub fn new(file_path: PathBuf) -> Self {
        let loaded = if file_path.exists() {
            std::fs::read_to_string(&file_path)
                .ok()
                .and_then(|s| serde_json::from_str::<VersionsCatalogFile>(&s).ok())
                .unwrap_or_default()
        } else {
            VersionsCatalogFile::default()
        };

        Self {
            catalog: Mutex::new(loaded),
            file_path,
        }
    }

    fn save_locked(&self, catalog: &VersionsCatalogFile) -> Result<(), String> {
        let json = serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?;
        if let Some(parent) = self.file_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp_path = self.file_path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json).map_err(|e| e.to_string())?;
        atomic_replace(&tmp_path, &self.file_path)
    }

    pub fn list(&self) -> Vec<VersionEntry> {
        self.catalog
            .lock()
            .map(|c| c.installed.clone())
            .unwrap_or_default()
    }

    pub fn find(&self, version_id: &str) -> Option<VersionEntry> {
        let mut parts = version_id.splitn(2, ':');
        let channel = parts.next()?;
        let hash = parts.next()?;
        self.catalog
            .lock()
            .ok()
            .and_then(|c| {
                c.installed
                    .iter()
                    .find(|e| e.channel == channel && e.version_hash == hash)
                    .cloned()
            })
    }

    pub fn upsert(&self, entry: VersionEntry) -> Result<(), String> {
        let mut catalog = self.catalog.lock().map_err(|e| e.to_string())?;
        if let Some(existing) = catalog
            .installed
            .iter_mut()
            .find(|e| e.channel == entry.channel && e.version_hash == entry.version_hash)
        {
            *existing = entry;
        } else {
            catalog.installed.push(entry);
        }
        self.save_locked(&catalog)
    }

    pub fn remove(&self, channel: &str, version_hash: &str) -> Result<bool, String> {
        let mut catalog = self.catalog.lock().map_err(|e| e.to_string())?;
        let before = catalog.installed.len();
        catalog
            .installed
            .retain(|e| !(e.channel == channel && e.version_hash == version_hash));
        let removed = catalog.installed.len() != before;
        if removed {
            self.save_locked(&catalog)?;
        }
        Ok(removed)
    }

    pub fn set_label(
        &self,
        channel: &str,
        version_hash: &str,
        label: Option<String>,
    ) -> Result<(), String> {
        let mut catalog = self.catalog.lock().map_err(|e| e.to_string())?;
        if let Some(entry) = catalog
            .installed
            .iter_mut()
            .find(|e| e.channel == channel && e.version_hash == version_hash)
        {
            entry.user_label = label.filter(|l| !l.trim().is_empty());
            self.save_locked(&catalog)
        } else {
            Err("Version not found in catalog".into())
        }
    }

    pub fn touch_launched(&self, channel: &str, version_hash: &str) {
        if let Ok(mut catalog) = self.catalog.lock() {
            if let Some(entry) = catalog
                .installed
                .iter_mut()
                .find(|e| e.channel == channel && e.version_hash == version_hash)
            {
                entry.last_launched_at = Some(Utc::now());
                let _ = self.save_locked(&catalog);
            }
        }
    }
}

fn legacy_versions_catalog_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("RAMVersions.json")
}

pub fn get_versions_catalog_path() -> PathBuf {
    let modern = std::env::var_os("LOCALAPPDATA").map(|local| {
        PathBuf::from(local)
            .join("Roblox Account Manager")
            .join("RAMVersions.json")
    });
    let Some(modern) = modern else {
        return legacy_versions_catalog_path();
    };

    if !modern.exists() {
        let legacy = legacy_versions_catalog_path();
        if legacy.exists() {
            if let Some(parent) = modern.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&legacy, &modern);
        }
    }

    modern
}

pub fn ram_managed_versions_root() -> Option<PathBuf> {
    let local = std::env::var_os("LOCALAPPDATA")?;
    Some(
        PathBuf::from(local)
            .join("Roblox Account Manager")
            .join("RobloxVersions"),
    )
}

#[cfg(test)]
mod versions_atomic_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ram-versions-{name}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn atomic_replace_overwrites_an_existing_destination() {
        let dir = unique_dir("replace");
        let src = dir.join("RAMVersions.json.tmp");
        let dst = dir.join("RAMVersions.json");

        std::fs::write(&dst, "old contents").expect("write dst");
        std::fs::write(&src, "new contents").expect("write src");

        atomic_replace(&src, &dst).expect("atomic_replace");

        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "new contents");
        assert!(!src.exists(), "the temp file must not survive the replace");

        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(leftovers, vec!["RAMVersions.json".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_replace_creates_the_destination_when_it_does_not_exist() {
        let dir = unique_dir("create");
        let src = dir.join("RAMVersions.json.tmp");
        let dst = dir.join("RAMVersions.json");

        std::fs::write(&src, "fresh").expect("write src");
        assert!(!dst.exists());

        atomic_replace(&src, &dst).expect("atomic_replace");

        assert_eq!(std::fs::read_to_string(&dst).unwrap(), "fresh");
        assert!(!src.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn atomic_replace_fails_when_the_source_is_missing() {
        let dir = unique_dir("missing");
        let src = dir.join("does-not-exist.tmp");
        let dst = dir.join("RAMVersions.json");

        assert!(atomic_replace(&src, &dst).is_err());
        assert!(!dst.exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_id_joins_channel_and_hash() {
        let entry = VersionEntry {
            channel: "LIVE".into(),
            version_hash: "version-abc123".into(),
            binary_type: "WindowsPlayer".into(),
            display_version: None,
            install_path: String::new(),
            install_size_bytes: 0,
            installed_at: None,
            last_launched_at: None,
            user_label: None,
        };
        assert_eq!(entry.version_id(), "LIVE:version-abc123");
    }
}
