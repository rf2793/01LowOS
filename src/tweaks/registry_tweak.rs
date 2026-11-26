use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::registry::{
    delete_value, delete_value_backup_restore, read_value, write_value, write_value_backup_restore,
    RegHive, RegValue,
};
use crate::tweaks::{Tweak, TweakMeta, TweakStatus};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegTarget {
    Set(RegValue),
    Delete,
}

#[derive(Debug, Clone)]
pub struct RegOp {
    pub hive: RegHive,
    pub subkey: String,
    pub name: String,
    pub target: RegTarget,
    pub optional: bool,
    pub use_backup_restore: bool,
}

impl RegOp {
    pub fn set(hive: RegHive, subkey: &str, name: &str, value: RegValue) -> Self {
        Self {
            hive,
            subkey: subkey.to_string(),
            name: name.to_string(),
            target: RegTarget::Set(value),
            optional: false,
            use_backup_restore: false,
        }
    }

    pub fn set_opt(hive: RegHive, subkey: &str, name: &str, value: RegValue) -> Self {
        Self {
            hive,
            subkey: subkey.to_string(),
            name: name.to_string(),
            target: RegTarget::Set(value),
            optional: true,
            use_backup_restore: false,
        }
    }

    pub fn set_br(hive: RegHive, subkey: &str, name: &str, value: RegValue) -> Self {
        Self {
            hive,
            subkey: subkey.to_string(),
            name: name.to_string(),
            target: RegTarget::Set(value),
            optional: false,
            use_backup_restore: true,
        }
    }

    pub fn delete(hive: RegHive, subkey: &str, name: &str) -> Self {
        Self {
            hive,
            subkey: subkey.to_string(),
            name: name.to_string(),
            target: RegTarget::Delete,
            optional: false,
            use_backup_restore: false,
        }
    }

    pub fn delete_opt(hive: RegHive, subkey: &str, name: &str) -> Self {
        Self {
            hive,
            subkey: subkey.to_string(),
            name: name.to_string(),
            target: RegTarget::Delete,
            optional: true,
            use_backup_restore: false,
        }
    }

    pub fn display_detail(&self) -> String {
        let val_name = if self.name.is_empty() {
            "(Padrão)"
        } else {
            &self.name
        };
        match &self.target {
            RegTarget::Set(v) => format!(
                "{}\\{} [{}] = {}{}",
                self.hive.display_prefix(),
                self.subkey,
                val_name,
                v.summary(),
                if self.use_backup_restore {
                    " (BACKUP_RESTORE)"
                } else {
                    ""
                }
            ),
            RegTarget::Delete => format!(
                "REMOVER {}\\{} [{}]",
                self.hive.display_prefix(),
                self.subkey,
                val_name
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegSnapshotEntry {
    pub hive: RegHive,
    pub subkey: String,
    pub name: String,
    pub existed: bool,
    pub value: Option<RegValue>,
    pub use_backup_restore: bool,
}

pub struct RegistryTweak {
    meta: TweakMeta,
    ops: Vec<RegOp>,
    default_ops: Vec<RegOp>,
}

impl RegistryTweak {
    pub fn new(mut meta: TweakMeta, ops: Vec<RegOp>) -> Self {
        if meta.details.is_empty() {
            meta.details = ops.iter().map(RegOp::display_detail).collect();
        }
        Self {
            meta,
            ops,
            default_ops: Vec::new(),
        }
    }

    pub fn with_defaults(mut meta: TweakMeta, ops: Vec<RegOp>, default_ops: Vec<RegOp>) -> Self {
        if meta.details.is_empty() {
            meta.details = ops.iter().map(RegOp::display_detail).collect();
        }
        Self {
            meta,
            ops,
            default_ops,
        }
    }

    fn capture_snapshot(ops: &[RegOp]) -> Result<Vec<RegSnapshotEntry>, String> {
        let mut snap = Vec::with_capacity(ops.len());
        for op in ops {
            let current = read_value(op.hive, &op.subkey, &op.name)?;
            snap.push(RegSnapshotEntry {
                hive: op.hive,
                subkey: op.subkey.clone(),
                name: op.name.clone(),
                existed: current.is_some(),
                value: current,
                use_backup_restore: op.use_backup_restore,
            });
        }
        Ok(snap)
    }

    fn restore_snapshot(snapshot: &[RegSnapshotEntry]) -> Result<(), String> {
        let mut errors = Vec::new();
        for entry in snapshot {
            if let Ok(cur) = read_value(entry.hive, &entry.subkey, &entry.name) {
                let unchanged = if entry.existed {
                    cur.as_ref() == entry.value.as_ref()
                } else {
                    cur.is_none()
                };
                if unchanged {
                    continue;
                }
            }
            let res = if entry.existed {
                if let Some(val) = &entry.value {
                    if entry.use_backup_restore {
                        write_value(entry.hive, &entry.subkey, &entry.name, val).or_else(|_| {
                            write_value_backup_restore(entry.hive, &entry.subkey, &entry.name, val)
                        })
                    } else {
                        write_value(entry.hive, &entry.subkey, &entry.name, val)
                    }
                } else {
                    Ok(())
                }
            } else if entry.use_backup_restore {
                delete_value(entry.hive, &entry.subkey, &entry.name).or_else(|_| {
                    delete_value_backup_restore(entry.hive, &entry.subkey, &entry.name)
                })
            } else {
                delete_value(entry.hive, &entry.subkey, &entry.name)
            };
            if let Err(e) = res {
                errors.push(e);
            }
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("; "))
        }
    }

    fn apply_one_op(op: &RegOp) -> Result<(), String> {
        match &op.target {
            RegTarget::Set(val) => {
                if op.use_backup_restore {
                    write_value_backup_restore(op.hive, &op.subkey, &op.name, val)
                        .or_else(|_| write_value(op.hive, &op.subkey, &op.name, val))?;
                } else {
                    write_value(op.hive, &op.subkey, &op.name, val)?;
                }
                let actual = read_value(op.hive, &op.subkey, &op.name)?;
                if actual.as_ref() != Some(val) {
                    return Err(format!(
                        "Verificação pós-gravação falhou em {}\\{}\\{}",
                        op.hive.display_prefix(),
                        op.subkey,
                        op.name
                    ));
                }
            }
            RegTarget::Delete => {
                if op.use_backup_restore {
                    delete_value_backup_restore(op.hive, &op.subkey, &op.name)
                        .or_else(|_| delete_value(op.hive, &op.subkey, &op.name))?;
                } else {
                    delete_value(op.hive, &op.subkey, &op.name)?;
                }
                let actual = read_value(op.hive, &op.subkey, &op.name)?;
                if actual.is_some() {
                    return Err(format!(
                        "Valor residual após remoção em {}\\{}\\{}",
                        op.hive.display_prefix(),
                        op.subkey,
                        op.name
                    ));
                }
            }
        }
        Ok(())
    }
}

impl Tweak for RegistryTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let has_mandatory = self.ops.iter().any(|op| !op.optional);
        let relevant: Vec<&RegOp> = if has_mandatory {
            self.ops.iter().filter(|op| !op.optional).collect()
        } else {
            self.ops.iter().collect()
        };

        let mut matched = 0usize;
        let total = relevant.len();
        for op in &relevant {
            match read_value(op.hive, &op.subkey, &op.name) {
                Ok(current) => {
                    let ok = match &op.target {
                        RegTarget::Set(expected) => current.as_ref() == Some(expected),
                        RegTarget::Delete => current.is_none(),
                    };
                    if ok {
                        matched += 1;
                    }
                }
                Err(e) => return TweakStatus::Error(e),
            }
        }

        if total == 0 {
            TweakStatus::NotApplicable("Sem operações".into())
        } else if matched == total {
            TweakStatus::Applied
        } else if matched == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{matched}/{total} chaves"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let snapshot = Self::capture_snapshot(&self.ops)?;
        let snapshot_json = serde_json::to_value(&snapshot)
            .map_err(|e| format!("Falha ao serializar snapshot: {e}"))?;
        let created_backup = backups.save_if_absent(self.meta.id, snapshot_json)?;

        let rollback_data = if created_backup {
            snapshot
        } else {
            backups
                .load(self.meta.id)?
                .and_then(|r| serde_json::from_value::<Vec<RegSnapshotEntry>>(r.data).ok())
                .unwrap_or(snapshot)
        };

        let mut warnings = Vec::new();
        let mut applied_count = 0usize;
        for op in &self.ops {
            match Self::apply_one_op(op) {
                Ok(()) => applied_count += 1,
                Err(e) if op.optional => {
                    warnings.push(format!("Opcional ignorado ({}): {e}", op.name));
                }
                Err(e) => {
                    let rb = Self::restore_snapshot(&rollback_data);
                    if created_backup && rb.is_ok() {
                        let _ = backups.remove(self.meta.id);
                    }
                    return match rb {
                        Ok(()) => Err(format!("{e} (reversão automática concluída)")),
                        Err(re) => Err(format!("{e}; falha também no rollback: {re}")),
                    };
                }
            }
        }

        if warnings.is_empty() {
            Ok(format!(
                "Aplicadas {applied_count}/{} entradas de registro.",
                self.ops.len()
            ))
        } else {
            Ok(format!(
                "Aplicadas {applied_count}/{} entradas. Avisos: {}",
                self.ops.len(),
                warnings.join(" | ")
            ))
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para esta melhoria.".into());
        };
        let snapshot: Vec<RegSnapshotEntry> = serde_json::from_value(rec.data)
            .map_err(|e| format!("Backup inválido para {}: {e}", self.meta.id))?;
        Self::restore_snapshot(&snapshot)?;
        backups.remove(self.meta.id)?;
        Ok(format!(
            "Restauradas {} entradas ao estado original.",
            snapshot.len()
        ))
    }

    fn can_reset_default(&self) -> bool {
        !self.default_ops.is_empty()
    }

    fn reset_to_default(&self) -> Result<String, String> {
        if self.default_ops.is_empty() {
            return Err("Sem padrão do Windows definido para esta melhoria.".into());
        }
        let mut count = 0usize;
        for op in &self.default_ops {
            match Self::apply_one_op(op) {
                Ok(()) => count += 1,
                Err(e) if op.optional => {
                    let _ = e;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(format!(
            "Padrão do Windows restaurado ({count} operação(ões))."
        ))
    }
}
