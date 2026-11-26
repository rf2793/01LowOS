use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::run_hidden;
use crate::sys::registry::{
    delete_value, key_exists, read_value, write_value, write_value_backup_restore, RegHive,
    RegValue,
};
use crate::tweaks::{Tweak, TweakMeta, TweakStatus};

#[derive(Debug, Clone)]
pub struct ServiceTarget {
    pub name: &'static str,
    pub target_start: u32,
    pub default_start: u32,
    pub clear_delayed: bool,
    pub use_backup_restore: bool,
    pub skip_if_hyperv: bool,
    pub stop_live: bool,
}

impl ServiceTarget {
    pub const fn disable(name: &'static str) -> Self {
        Self {
            name,
            target_start: 4,
            default_start: 3,
            clear_delayed: false,
            use_backup_restore: false,
            skip_if_hyperv: false,
            stop_live: false,
        }
    }

    pub const fn disable_default_auto(name: &'static str) -> Self {
        Self {
            name,
            target_start: 4,
            default_start: 2,
            clear_delayed: false,
            use_backup_restore: false,
            skip_if_hyperv: false,
            stop_live: false,
        }
    }

    pub const fn manual(name: &'static str) -> Self {
        Self {
            name,
            target_start: 3,
            default_start: 3,
            clear_delayed: false,
            use_backup_restore: false,
            skip_if_hyperv: false,
            stop_live: false,
        }
    }

    pub const fn auto(name: &'static str) -> Self {
        Self {
            name,
            target_start: 2,
            default_start: 3,
            clear_delayed: false,
            use_backup_restore: false,
            skip_if_hyperv: false,
            stop_live: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ServiceSnapshotEntry {
    name: String,
    original_start: Option<u32>,
    original_delayed: Option<u32>,
}

pub struct ServiceGroupTweak {
    meta: TweakMeta,
    targets: Vec<ServiceTarget>,
}

impl ServiceGroupTweak {
    pub fn new(mut meta: TweakMeta, targets: Vec<ServiceTarget>) -> Self {
        if meta.details.is_empty() {
            meta.details = targets
                .iter()
                .map(|t| {
                    let mode = match t.target_start {
                        2 => "Auto (2)",
                        3 => "Manual (3)",
                        4 => "Disabled (4)",
                        other => return format!("Serviço {}: Start={other}", t.name),
                    };
                    format!(
                        "Serviço {}: {mode}{}",
                        t.name,
                        if t.skip_if_hyperv {
                            " [preservado se Hyper-V ativo]"
                        } else {
                            ""
                        }
                    )
                })
                .collect();
        }
        Self { meta, targets }
    }

    fn hyperv_present() -> bool {
        key_exists(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\vmms")
            || key_exists(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\vmcompute")
    }

    fn service_subkey(name: &str) -> String {
        format!(r"SYSTEM\CurrentControlSet\Services\{name}")
    }

    fn read_dword(subkey: &str, name: &str) -> Option<u32> {
        match read_value(RegHive::Hklm, subkey, name) {
            Ok(Some(RegValue::DWord(v))) => Some(v),
            _ => None,
        }
    }

    fn set_service_start(
        name: &str,
        start: u32,
        clear_delayed: bool,
        use_backup_restore: bool,
        stop_live: bool,
    ) -> Result<(), String> {
        let subkey = Self::service_subkey(name);
        let sc_mode = match start {
            2 => Some("auto"),
            3 => Some("demand"),
            4 => Some("disabled"),
            _ => None,
        };

        if let Some(mode) = sc_mode {
            let _ = run_hidden("sc.exe", &["config", name, "start=", mode]);
        }

        if Self::read_dword(&subkey, "Start") != Some(start) {
            let val = RegValue::DWord(start);
            if use_backup_restore {
                write_value_backup_restore(RegHive::Hklm, &subkey, "Start", &val)
                    .or_else(|_| write_value(RegHive::Hklm, &subkey, "Start", &val))?;
            } else {
                write_value(RegHive::Hklm, &subkey, "Start", &val)
                    .or_else(|_| write_value_backup_restore(RegHive::Hklm, &subkey, "Start", &val))?;
            }
        }

        if clear_delayed {
            let val = RegValue::DWord(0);
            if use_backup_restore {
                let _ = write_value_backup_restore(RegHive::Hklm, &subkey, "DelayedAutostart", &val);
            } else {
                let _ = write_value(RegHive::Hklm, &subkey, "DelayedAutostart", &val);
            }
        }

        if stop_live && start == 4 {
            let _ = run_hidden("sc.exe", &["stop", name]);
        }

        if Self::read_dword(&subkey, "Start") != Some(start) {
            return Err(format!("Start de '{name}' não confere com {start}."));
        }
        Ok(())
    }

    fn active_targets(&self) -> Vec<&ServiceTarget> {
        let hv = Self::hyperv_present();
        self.targets
            .iter()
            .filter(|t| !(t.skip_if_hyperv && hv))
            .filter(|t| key_exists(RegHive::Hklm, &Self::service_subkey(t.name)))
            .collect()
    }
}

impl Tweak for ServiceGroupTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let active = self.active_targets();
        if active.is_empty() {
            return TweakStatus::NotApplicable("Nenhum serviço alvo presente".into());
        }
        let mut ok_count = 0usize;
        for t in &active {
            let subkey = Self::service_subkey(t.name);
            let start = Self::read_dword(&subkey, "Start");
            let delayed_ok = !t.clear_delayed
                || Self::read_dword(&subkey, "DelayedAutostart").unwrap_or(0) == 0;
            if start == Some(t.target_start) && delayed_ok {
                ok_count += 1;
            }
        }
        if ok_count == active.len() {
            TweakStatus::Applied
        } else if ok_count == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok_count}/{} serviços", active.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let active = self.active_targets();
        if active.is_empty() {
            return Ok("Nenhum serviço alvo presente neste sistema.".into());
        }

        let mut snap = Vec::with_capacity(active.len());
        for t in &active {
            let subkey = Self::service_subkey(t.name);
            snap.push(ServiceSnapshotEntry {
                name: t.name.to_string(),
                original_start: Self::read_dword(&subkey, "Start"),
                original_delayed: Self::read_dword(&subkey, "DelayedAutostart"),
            });
        }
        let json = serde_json::to_value(&snap)
            .map_err(|e| format!("Falha ao serializar snapshot de serviços: {e}"))?;
        let created_backup = backups.save_if_absent(self.meta.id, json)?;

        let mut failures = Vec::new();
        let mut applied = 0usize;
        for t in &active {
            match Self::set_service_start(
                t.name,
                t.target_start,
                t.clear_delayed,
                t.use_backup_restore,
                t.stop_live,
            ) {
                Ok(()) => applied += 1,
                Err(e) => failures.push(e),
            }
        }

        if failures.is_empty() {
            Ok(format!("{applied} serviço(s) configurado(s)."))
        } else {
            if created_backup && applied == 0 {
                let _ = backups.remove(self.meta.id);
            }
            Err(format!("Falhas em serviços: {}", failures.join("; ")))
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para esta melhoria.".into());
        };
        let snap: Vec<ServiceSnapshotEntry> = serde_json::from_value(rec.data)
            .map_err(|e| format!("Backup de serviços inválido: {e}"))?;

        let mut failures = Vec::new();
        let mut restored = 0usize;
        for entry in &snap {
            let subkey = Self::service_subkey(&entry.name);
            if !key_exists(RegHive::Hklm, &subkey) {
                continue;
            }
            if let Some(orig) = entry.original_start {
                if let Err(e) = Self::set_service_start(&entry.name, orig, false, true, false) {
                    failures.push(e);
                    continue;
                }
            }
            if let Some(delayed) = entry.original_delayed {
                let _ = write_value_backup_restore(
                    RegHive::Hklm,
                    &subkey,
                    "DelayedAutostart",
                    &RegValue::DWord(delayed),
                )
                .or_else(|_| {
                    write_value(
                        RegHive::Hklm,
                        &subkey,
                        "DelayedAutostart",
                        &RegValue::DWord(delayed),
                    )
                });
            } else {
                let _ = delete_value(RegHive::Hklm, &subkey, "DelayedAutostart");
            }
            restored += 1;
        }

        if failures.is_empty() {
            backups.remove(self.meta.id)?;
            Ok(format!("{restored} serviço(s) restaurado(s)."))
        } else {
            Err(format!("Reversão parcial: {}", failures.join("; ")))
        }
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let active = self.active_targets();
        if active.is_empty() {
            return Ok("Nenhum serviço alvo presente neste sistema.".into());
        }
        let mut failures = Vec::new();
        let mut restored = 0usize;
        for t in &active {
            match Self::set_service_start(t.name, t.default_start, false, t.use_backup_restore, false) {
                Ok(()) => restored += 1,
                Err(e) => failures.push(e),
            }
        }
        if failures.is_empty() {
            Ok(format!("{restored} serviço(s) restaurado(s) ao padrão do Windows."))
        } else {
            Err(format!("Restauração parcial: {}", failures.join("; ")))
        }
    }
}
