pub mod advanced;
pub mod catalog;
pub mod network;
pub mod power;
pub mod registry_tweak;
pub mod service_tweak;
pub mod ssd_only;
pub mod task_tweak;

use crate::sys::backup::BackupStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    GpuLatency,
    SystemKernel,
    Power,
    Network,
    UserHkcu,
    PrivacyBloat,
    WindowsServices,
    ThirdParty,
    ScheduledTasks,
    EtwAutologgers,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::GpuLatency => "GPU & Latência",
            Category::SystemKernel => "Sistema & Kernel",
            Category::Power => "Energia",
            Category::Network => "Rede",
            Category::UserHkcu => "Usuário (HKCU)",
            Category::PrivacyBloat => "Privacidade & Bloat",
            Category::WindowsServices => "Serviços Windows",
            Category::ThirdParty => "Terceiros",
            Category::ScheduledTasks => "Tarefas Agendadas",
            Category::EtwAutologgers => "ETW Autologgers",
        }
    }

    pub fn all() -> &'static [Category] {
        &[
            Category::GpuLatency,
            Category::SystemKernel,
            Category::Power,
            Category::Network,
            Category::UserHkcu,
            Category::PrivacyBloat,
            Category::WindowsServices,
            Category::ThirdParty,
            Category::ScheduledTasks,
            Category::EtwAutologgers,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

impl RiskLevel {
    pub fn label(self) -> &'static str {
        match self {
            RiskLevel::Low => "Baixo",
            RiskLevel::Medium => "Médio",
            RiskLevel::High => "Alto",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TweakStatus {
    Applied,
    NotApplied,
    Partial(String),
    NotApplicable(String),
    Error(String),
}

impl TweakStatus {
    pub fn label(&self) -> String {
        match self {
            TweakStatus::Applied => "Aplicado".to_string(),
            TweakStatus::NotApplied => "Não aplicado".to_string(),
            TweakStatus::Partial(detail) => format!("Parcial ({detail})"),
            TweakStatus::NotApplicable(reason) => format!("N/A ({reason})"),
            TweakStatus::Error(err) => format!("Erro ({err})"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct TweakMeta {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub risk: RiskLevel,
    pub requires_reboot: bool,
    pub requires_admin: bool,
    pub description: &'static str,
    pub details: Vec<String>,
}

pub trait Tweak: Send + Sync {
    fn meta(&self) -> &TweakMeta;
    fn check(&self) -> TweakStatus;
    fn apply(&self, backups: &BackupStore) -> Result<String, String>;
    fn revert(&self, backups: &BackupStore) -> Result<String, String>;

    fn can_reset_default(&self) -> bool {
        false
    }

    fn reset_to_default(&self) -> Result<String, String> {
        Err("Sem padrão conhecido; use o backup.".into())
    }
}

#[derive(Debug, Clone)]
pub struct SelfTestSummary {
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub report_path: std::path::PathBuf,
}

pub fn run_self_test<F: FnMut(&str)>(
    catalog: &[std::sync::Arc<dyn Tweak>],
    backups: &BackupStore,
    include_heavy: bool,
    mut on_line: F,
) -> SelfTestSummary {
    let elevated = crate::sys::elevation::is_elevated();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut skipped = 0usize;
    let mut lines = Vec::new();

    let header = format!(
        "=== 01LowOS Self-Test (Admin: {}, Heavy: {}, {} tweaks) ===",
        if elevated { "SIM" } else { "NÃO" },
        if include_heavy { "SIM" } else { "NÃO" },
        catalog.len()
    );
    on_line(&header);
    lines.push(header);

    for t in catalog {
        let m = t.meta();
        let before = t.check();
        let skip_reason: Option<String> = match &before {
            TweakStatus::NotApplicable(r) => Some(format!("N/A ({r})")),
            TweakStatus::Applied => Some("já aplicado".into()),
            _ if m.requires_admin && !elevated => Some("requer Admin".into()),
            _ if !include_heavy
                && matches!(
                    m.id,
                    "bloat.optional_features_off"
                        | "bloat.system_appx"
                        | "bloat.consumer_appx"
                        | "power.hiberfil_off"
                        | "sys.reserved_storage_off"
                        | "sys.hypervisor_off"
                ) =>
            {
                Some("pesado (use --self-test-all)".into())
            }
            _ if backups.exists(m.id) => Some("backup prévio existente".into()),
            _ => None,
        };

        if let Some(reason) = skip_reason {
            skipped += 1;
            let msg = format!("SKIP [{}] — {}", m.id, reason);
            on_line(&msg);
            lines.push(msg);
            continue;
        }

        let apply_res = t.apply(backups);
        let after_apply = t.check();
        let revert_res = t.revert(backups);
        let after_revert = t.check();

        let apply_ok_status = matches!(after_apply, TweakStatus::Applied)
            || (matches!(
                m.id,
                "tasks.windows_bloat"
                    | "net.wol_eee_off"
                    | "net.qos_nic_flow_off"
                    | "net.nic_lso_uso_off"
            )
                && matches!(after_apply, TweakStatus::Partial(_))
                && after_apply != before);
        let ok = apply_res.is_ok()
            && revert_res.is_ok()
            && apply_ok_status
            && after_revert == before;

        if ok {
            passed += 1;
            let msg = format!(
                "PASS [{}] — before='{}' -> apply='{}' -> revert='{}'",
                m.id,
                before.label(),
                after_apply.label(),
                after_revert.label()
            );
            on_line(&msg);
            lines.push(msg);
        } else {
            failed += 1;
            let msg = format!(
                "FAIL [{}] — before='{}' apply={:?} ('{}') revert={:?} ('{}')",
                m.id,
                before.label(),
                apply_res,
                after_apply.label(),
                revert_res,
                after_revert.label()
            );
            on_line(&msg);
            lines.push(msg);
        }
    }

    let summary_line = format!(
        "=== Resumo: PASS={passed} | FAIL={failed} | SKIP={skipped} ==="
    );
    on_line(&summary_line);
    lines.push(summary_line);

    let report_base = if elevated {
        std::env::var_os("ProgramData")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"))
    } else {
        std::env::var_os("LOCALAPPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
    };
    let report_path = report_base.join("01LowOS").join("self_test_report.txt");
    if let Some(p) = report_path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let _ = std::fs::write(&report_path, lines.join("\r\n"));

    SelfTestSummary {
        passed,
        failed,
        skipped,
        report_path,
    }
}
