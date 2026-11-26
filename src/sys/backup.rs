use std::fs;
use std::path::{Path, PathBuf};

use chrono::Local;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRecord {
    pub tweak_id: String,
    pub created_at: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct BackupStore {
    dir: PathBuf,
}

impl BackupStore {
    pub fn default_store() -> Self {
        let prog_data = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
            .join("01LowOS")
            .join("Backups")
            .join("tweaks");

        if crate::sys::elevation::is_elevated() || Self::is_dir_writable(&prog_data) {
            return Self { dir: prog_data };
        }

        let local_app = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("01LowOS")
            .join("Backups")
            .join("tweaks");
        Self { dir: local_app }
    }

    fn is_dir_writable(dir: &Path) -> bool {
        if fs::create_dir_all(dir).is_err() {
            return false;
        }
        let probe = dir.join(format!(".write_test_{}", std::process::id()));
        if fs::write(&probe, b"ok").is_ok() {
            let _ = fs::remove_file(&probe);
            true
        } else {
            false
        }
    }

    #[cfg(test)]
    pub fn with_dir(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn sanitize_id(tweak_id: &str) -> String {
        tweak_id
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                _ => c,
            })
            .collect()
    }

    pub fn record_path(&self, tweak_id: &str) -> PathBuf {
        self.dir
            .join(format!("{}.json", Self::sanitize_id(tweak_id)))
    }

    pub fn extra_file_path(&self, tweak_id: &str, ext: &str) -> PathBuf {
        self.dir
            .join(format!("{}.{}", Self::sanitize_id(tweak_id), ext))
    }

    pub fn exists(&self, tweak_id: &str) -> bool {
        self.record_path(tweak_id).exists()
    }

    /// Saves a backup snapshot ONLY if one does not already exist for `tweak_id`.
    /// This preserves the original pre-tweak state across repeated apply calls.
    pub fn save_if_absent(&self, tweak_id: &str, data: serde_json::Value) -> Result<bool, String> {
        let path = self.record_path(tweak_id);
        if path.exists() {
            return Ok(false);
        }
        fs::create_dir_all(&self.dir)
            .map_err(|e| format!("Falha ao criar pasta de backup {:?}: {e}", self.dir))?;
        let rec = BackupRecord {
            tweak_id: tweak_id.to_string(),
            created_at: Local::now().to_rfc3339(),
            data,
        };
        let json = serde_json::to_string_pretty(&rec)
            .map_err(|e| format!("Falha ao serializar backup de {tweak_id}: {e}"))?;
        fs::write(&path, json)
            .map_err(|e| format!("Falha ao gravar backup {:?}: {e}", path))?;
        Ok(true)
    }

    pub fn load(&self, tweak_id: &str) -> Result<Option<BackupRecord>, String> {
        let path = self.record_path(tweak_id);
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(&path)
            .map_err(|e| format!("Falha ao ler backup {:?}: {e}", path))?;
        let rec: BackupRecord = serde_json::from_str(&raw)
            .map_err(|e| format!("Backup corrompido em {:?}: {e}", path))?;
        Ok(Some(rec))
    }

    pub fn remove(&self, tweak_id: &str) -> Result<(), String> {
        let path = self.record_path(tweak_id);
        if path.exists() {
            fs::remove_file(&path)
                .map_err(|e| format!("Falha ao remover backup {:?}: {e}", path))?;
        }
        Ok(())
    }

    pub fn list_all(&self) -> Result<Vec<BackupRecord>, String> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }
        let mut records = Vec::new();
        let entries = fs::read_dir(&self.dir)
            .map_err(|e| format!("Falha ao listar backups em {:?}: {e}", self.dir))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(rec) = serde_json::from_str::<BackupRecord>(&raw) {
                    records.push(rec);
                }
            }
        }
        records.sort_by(|a, b| a.tweak_id.cmp(&b.tweak_id));
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn backup_save_if_absent_is_idempotent_and_removable() {
        let tmp = std::env::temp_dir().join(format!(
            "01lowos_backup_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let store = BackupStore::with_dir(tmp.clone());

        assert!(!store.exists("gpu.hags"));
        assert!(store.save_if_absent("gpu.hags", json!({"orig": 1})).unwrap());
        assert!(store.exists("gpu.hags"));

        // Second call must NOT overwrite original value 1 with 999
        assert!(!store
            .save_if_absent("gpu.hags", json!({"orig": 999}))
            .unwrap());

        let loaded = store.load("gpu.hags").unwrap().unwrap();
        assert_eq!(loaded.data["orig"], 1);

        let all = store.list_all().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].tweak_id, "gpu.hags");

        store.remove("gpu.hags").unwrap();
        assert!(!store.exists("gpu.hags"));
        let _ = fs::remove_dir_all(&tmp);
    }
}
