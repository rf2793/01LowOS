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
        Self::write_record(&path, &rec)?;
        Ok(true)
    }

    fn write_record(path: &Path, rec: &BackupRecord) -> Result<(), String> {
        let json = serde_json::to_string_pretty(rec)
            .map_err(|e| format!("Falha ao serializar backup de {}: {e}", rec.tweak_id))?;
        // Grava em arquivo temporario e renomeia: uma queda de energia no meio da escrita
        // nunca deixa um .json truncado (que tornaria a reversao impossivel).
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json)
            .map_err(|e| format!("Falha ao gravar backup {:?}: {e}", tmp))?;
        fs::rename(&tmp, path).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            format!("Falha ao finalizar backup {:?}: {e}", path)
        })
    }

    /// Para backups no formato {secao: [itens]}: cria o backup se nao existir; se existir,
    /// mantem o estado original dos itens ja salvos e acrescenta so os itens novos (identidade
    /// dada por `key_of(secao, item)`). Sem isso, itens instalados depois do primeiro apply
    /// (ex.: tarefa do Google Updater com nome de versao nova) ficam sem reversao.
    /// Retorna (backup_criado_agora, itens_acrescentados).
    pub fn save_or_merge_lists<F>(
        &self,
        tweak_id: &str,
        data: serde_json::Value,
        key_of: F,
    ) -> Result<(bool, usize), String>
    where
        F: Fn(&str, &serde_json::Value) -> String,
    {
        let Some(mut rec) = self.load(tweak_id)? else {
            return self.save_if_absent(tweak_id, data).map(|c| (c, 0));
        };
        let (Some(old), Some(new)) = (rec.data.as_object_mut(), data.as_object()) else {
            return Err(format!("Backup de {tweak_id} nao tem o formato de listas esperado."));
        };
        let mut added = 0usize;
        for (section, items) in new {
            let Some(items) = items.as_array() else { continue };
            let slot = old
                .entry(section.clone())
                .or_insert_with(|| serde_json::Value::Array(Vec::new()));
            let Some(saved) = slot.as_array_mut() else {
                return Err(format!("Secao '{section}' do backup de {tweak_id} nao e uma lista."));
            };
            for item in items {
                let key = key_of(section, item);
                if !saved.iter().any(|s| key_of(section, s) == key) {
                    saved.push(item.clone());
                    added += 1;
                }
            }
        }
        if added > 0 {
            Self::write_record(&self.record_path(tweak_id), &rec)?;
        }
        Ok((false, added))
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

    #[test]
    fn merge_keeps_original_state_and_adds_only_new_items() {
        let tmp = std::env::temp_dir().join(format!(
            "01lowos_backup_merge_test_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        let store = BackupStore::with_dir(tmp.clone());
        let key = |_: &str, v: &serde_json::Value| v["Name"].as_str().unwrap_or("").to_string();

        let first = json!({"servicos": [{"Name": "A", "StartMode": "Auto"}]});
        assert_eq!(store.save_or_merge_lists("t", first, key).unwrap(), (true, 0));

        // "A" reaparece com outro estado (nao pode sobrescrever o original); "B" e novo.
        let second = json!({
            "servicos": [{"Name": "A", "StartMode": "Manual"}, {"Name": "B", "StartMode": "Auto"}],
            "tarefas": [{"Name": "T1"}]
        });
        assert_eq!(store.save_or_merge_lists("t", second.clone(), key).unwrap(), (false, 2));
        assert_eq!(store.save_or_merge_lists("t", second, key).unwrap(), (false, 0));

        let data = store.load("t").unwrap().unwrap().data;
        assert_eq!(data["servicos"].as_array().unwrap().len(), 2);
        assert_eq!(data["servicos"][0]["StartMode"], "Auto");
        assert_eq!(data["servicos"][1]["Name"], "B");
        assert_eq!(data["tarefas"][0]["Name"], "T1");
        let _ = fs::remove_dir_all(&tmp);
    }
}
