use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::{run_hidden_checked, run_powershell_checked};
use crate::sys::registry::{read_value, RegHive, RegValue};
use crate::tweaks::{Category, RiskLevel, Tweak, TweakMeta, TweakStatus};

const BALANCED_GUID: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";
const HIGH_PERF_GUID: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
const PLAN_DISPLAY_NAME: &str = "01LowOS - perfil corrigido";

const SUB_CPU: &str = "54533251-82be-4824-96c1-47b60b740d00";
const SUB_DISK: &str = "0012ee47-9041-4b5d-9b77-535fba8b1442";
const SUB_USB: &str = "2a737441-1930-4402-8d77-b2bebba308a3";
const SUB_SLEEP: &str = "238c9fa8-0aad-41ed-83f4-97be242c8f20";
const SUB_PCIE: &str = "501a4d13-42af-4429-9fd1-a8218c268e20";
const SUB_BUTTON: &str = "4f971e89-eebd-4455-a8de-9e59040e7347";
const SUB_VIDEO: &str = "7516b95f-f776-4464-8c53-06167f40cc99";

struct PowerSettingDef {
    name: &'static str,
    subgroup: &'static str,
    setting: &'static str,
    value: u32,
}

const POWER_SETTINGS: &[PowerSettingDef] = &[
    PowerSettingDef { name: "CPMINCORES", subgroup: SUB_CPU, setting: "0cc5b647-c1df-4637-891a-dec35c318583", value: 100 },
    PowerSettingDef { name: "CPMAXCORES", subgroup: SUB_CPU, setting: "ea062031-0e34-4ff1-9b6d-eb1059334028", value: 100 },
    PowerSettingDef { name: "PERFEPP", subgroup: SUB_CPU, setting: "36687f9e-e3a5-4dbf-b1dc-15eb381c6863", value: 0 },
    PowerSettingDef { name: "IDLEDISABLE", subgroup: SUB_CPU, setting: "5d76a2ca-e8c0-402f-a133-2158492d58ad", value: 0 },
    PowerSettingDef { name: "PROCTHROTTLEMIN", subgroup: SUB_CPU, setting: "893dee8e-2bef-41e0-89c6-b55d0929964c", value: 100 },
    PowerSettingDef { name: "PROCTHROTTLEMAX", subgroup: SUB_CPU, setting: "bc5038f7-23e0-4960-96da-33abaf5935ec", value: 100 },
    PowerSettingDef { name: "PERFBOOSTMODE", subgroup: SUB_CPU, setting: "be337238-0d82-4146-a960-4f3749d470c7", value: 2 },
    PowerSettingDef { name: "ASPM", subgroup: SUB_PCIE, setting: "ee12f906-d277-404b-b6da-e5fa1a576df5", value: 0 },
    PowerSettingDef { name: "USBSELECTIVE", subgroup: SUB_USB, setting: "48e6b7a6-50f5-4782-a5d4-53bb8f07e226", value: 0 },
    PowerSettingDef { name: "USB3LPM", subgroup: SUB_USB, setting: "d4e98f31-5ffe-4ce1-be31-1b38b384c009", value: 0 },
    PowerSettingDef { name: "NVMEIDLETIMEOUT", subgroup: SUB_DISK, setting: "d639518a-e56d-4345-8af2-b9f32fb26109", value: 0 },
    PowerSettingDef { name: "NVMELATENCYTOLERANCE", subgroup: SUB_DISK, setting: "fc95af4d-40e7-4b6d-835a-56d131dbc80e", value: 0 },
    PowerSettingDef { name: "DISKIDLE", subgroup: SUB_DISK, setting: "6738e2c4-e8a5-4a42-b16a-e040e769756e", value: 0 },
    PowerSettingDef { name: "STANDBYIDLE", subgroup: SUB_SLEEP, setting: "29f6c1db-86da-48c5-9fdb-f2b67b1f44da", value: 0 },
    PowerSettingDef { name: "HIBERNATEIDLE", subgroup: SUB_SLEEP, setting: "9d7815a6-7ee4-497e-8888-515a05f02364", value: 0 },
    PowerSettingDef { name: "HYBRIDSLEEP", subgroup: SUB_SLEEP, setting: "94ac6d29-73ce-41a6-809f-6363ba21b47e", value: 0 },
    PowerSettingDef { name: "RTCWAKE", subgroup: SUB_SLEEP, setting: "bd3b718a-0680-4d9d-8ab2-e1d2b4ac806d", value: 0 },
    PowerSettingDef { name: "UNATTENDSLEEP", subgroup: SUB_SLEEP, setting: "7bc4a2f9-d8fc-4469-b07b-33eb785aaca0", value: 0 },
    PowerSettingDef { name: "SBUTTONACTION", subgroup: SUB_BUTTON, setting: "96996bc0-ad50-47ec-923b-6f41874dd9eb", value: 0 },
    PowerSettingDef { name: "VIDEOIDLE", subgroup: SUB_VIDEO, setting: "3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e", value: 0 },
];

pub fn extract_guid(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < 36 {
        return None;
    }
    for i in 0..=bytes.len() - 36 {
        let slice = &bytes[i..i + 36];
        let valid = slice.iter().enumerate().all(|(idx, &b)| match idx {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
        if valid {
            return Some(String::from_utf8_lossy(slice).to_ascii_lowercase());
        }
    }
    None
}

fn generate_guid() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0x1234_5678_9abc_def0);
    let pid = std::process::id() as u128;
    let mut v = nanos ^ (pid << 64) ^ 0x4F1B_BC23_9A47_D109_u128;
    v = v.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let b = v.to_be_bytes();
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-4{:01x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        b[0], b[1], b[2], b[3],
        b[4], b[5],
        b[6] & 0x0f, b[7],
        (b[8] & 0x3f) | 0x80, b[9],
        b[10], b[11], b[12], b[13], b[14], b[15]
    )
}

#[derive(Debug, Serialize, Deserialize)]
struct PowerPlanBackup {
    original_plan: String,
    new_plan: String,
    created_plan: bool,
    pow_file: String,
}

pub struct PowerPlanTweak {
    meta: TweakMeta,
}

impl PowerPlanTweak {
    pub fn new() -> Self {
        let mut details: Vec<String> = POWER_SETTINGS
            .iter()
            .map(|s| format!("{} = {}", s.name, s.value))
            .collect();
        details.insert(0, format!("Plano clonado: '{PLAN_DISPLAY_NAME}'"));
        Self {
            meta: TweakMeta {
                id: "power.plan_01lowos",
                name: "Plano de Energia 01LowOS - perfil corrigido",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Clona o plano Alto Desempenho e aplica os 20 parâmetros validados (CPPC 100%, C-states mantidos, ASPM/USB LPM/NVMe APST off, sem suspensão).",
                details,
            },
        }
    }

    fn get_active_scheme_line() -> Result<(String, String), String> {
        let out = run_hidden_checked("powercfg.exe", &["/getactivescheme"], &[0])?;
        let text = out.stdout.trim().to_string();
        let guid = extract_guid(&text)
            .ok_or_else(|| format!("Não foi possível identificar o GUID do plano ativo: {text}"))?;
        Ok((guid, text))
    }

    fn read_ac_index(scheme_guid: &str, subgroup: &str, setting: &str) -> Option<u32> {
        let subkey = format!(
            r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes\{scheme_guid}\{subgroup}\{setting}"
        );
        match read_value(RegHive::Hklm, &subkey, "ACSettingIndex") {
            Ok(Some(RegValue::DWord(v))) => Some(v),
            _ => None,
        }
    }
}

impl Tweak for PowerPlanTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let (guid, line) = match Self::get_active_scheme_line() {
            Ok(v) => v,
            Err(e) => return TweakStatus::Error(e),
        };
        if !line
            .to_ascii_lowercase()
            .contains(&PLAN_DISPLAY_NAME.to_ascii_lowercase())
        {
            return TweakStatus::NotApplied;
        }
        let ok_count = POWER_SETTINGS
            .iter()
            .filter(|s| Self::read_ac_index(&guid, s.subgroup, s.setting) == Some(s.value))
            .count();
        if ok_count == POWER_SETTINGS.len() {
            TweakStatus::Applied
        } else {
            TweakStatus::Partial(format!("{ok_count}/{} parâmetros", POWER_SETTINGS.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let (old_active, line) = Self::get_active_scheme_line()?;
        let reuse_plan = line
            .to_ascii_lowercase()
            .contains(&PLAN_DISPLAY_NAME.to_ascii_lowercase());

        let new_plan = if reuse_plan {
            old_active.clone()
        } else {
            generate_guid()
        };
        let revert_target = if reuse_plan {
            BALANCED_GUID.to_string()
        } else {
            old_active.clone()
        };

        let pow_path = backups.extra_file_path(self.meta.id, "pow");
        if !backups.exists(self.meta.id) {
            if let Some(parent) = pow_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("Falha ao criar pasta de backup: {e}"))?;
            }
            let pow_str = pow_path.to_string_lossy().to_string();
            run_hidden_checked("powercfg.exe", &["/export", &pow_str, &revert_target], &[0])?;
        }

        let mut created_plan = false;
        if !reuse_plan {
            let list_out = run_hidden_checked("powercfg.exe", &["/list"], &[0])?;
            let base_plan = if list_out
                .stdout
                .to_ascii_lowercase()
                .contains(HIGH_PERF_GUID)
            {
                HIGH_PERF_GUID
            } else {
                &old_active
            };
            run_hidden_checked(
                "powercfg.exe",
                &["/duplicatescheme", base_plan, &new_plan],
                &[0],
            )?;
            created_plan = true;
            run_hidden_checked(
                "powercfg.exe",
                &["/changename", &new_plan, PLAN_DISPLAY_NAME],
                &[0],
            )?;
        }

        let backup_payload = serde_json::to_value(&PowerPlanBackup {
            original_plan: revert_target.clone(),
            new_plan: new_plan.clone(),
            created_plan,
            pow_file: pow_path.to_string_lossy().to_string(),
        })
        .map_err(|e| format!("Falha ao serializar backup do plano: {e}"))?;
        let created_backup = backups.save_if_absent(self.meta.id, backup_payload)?;

        for s in POWER_SETTINGS {
            let val_str = s.value.to_string();
            if let Err(e) = run_hidden_checked(
                "powercfg.exe",
                &["/setacvalueindex", &new_plan, s.subgroup, s.setting, &val_str],
                &[0],
            ) {
                if created_plan {
                    let _ = run_hidden_checked("powercfg.exe", &["/delete", &new_plan], &[0]);
                }
                if created_backup {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
            if Self::read_ac_index(&new_plan, s.subgroup, s.setting) != Some(s.value) {
                if created_plan {
                    let _ = run_hidden_checked("powercfg.exe", &["/delete", &new_plan], &[0]);
                }
                if created_backup {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(format!("Falha na verificação de {}", s.name));
            }
        }

        run_hidden_checked("powercfg.exe", &["/setactive", &new_plan], &[0])?;
        let (verify_guid, _) = Self::get_active_scheme_line()?;
        if verify_guid != new_plan {
            return Err("O novo plano de energia não ficou ativo.".into());
        }

        Ok(format!(
            "Plano '{PLAN_DISPLAY_NAME}' ({new_plan}) ativo com {} ajustes.",
            POWER_SETTINGS.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para o plano de energia.".into());
        };
        let bk: PowerPlanBackup = serde_json::from_value(rec.data)
            .map_err(|e| format!("Backup do plano inválido: {e}"))?;

        let target = if run_hidden_checked("powercfg.exe", &["/list"], &[0])
            .map(|o| o.stdout.to_ascii_lowercase().contains(&bk.original_plan))
            .unwrap_or(false)
        {
            bk.original_plan.clone()
        } else {
            BALANCED_GUID.to_string()
        };

        run_hidden_checked("powercfg.exe", &["/setactive", &target], &[0])?;
        if bk.created_plan && bk.new_plan != target {
            let _ = run_hidden_checked("powercfg.exe", &["/delete", &bk.new_plan], &[0]);
        }
        let pow = std::path::Path::new(&bk.pow_file);
        if pow.exists() {
            let _ = std::fs::remove_file(pow);
        }
        backups.remove(self.meta.id)?;
        Ok(format!("Plano original restaurado ({target})."))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        run_hidden_checked("powercfg.exe", &["/setactive", BALANCED_GUID], &[0])?;
        Ok(format!("Plano Equilibrado padrão ativado ({BALANCED_GUID})."))
    }
}

pub struct DeviceWakeTweak {
    meta: TweakMeta,
}

impl DeviceWakeTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "power.device_wake_off",
                name: "Desativar Wake-up de dispositivos (mantém Mouse/Teclado)",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Remove permissão de acordar o PC de dispositivos de rede/áudio/outros via powercfg, mantendo mouse e teclado intactos.",
                details: vec![
                    "Consulta powercfg /devicequery wake_armed".into(),
                    "Preserva dispositivos das classes Mouse e Keyboard".into(),
                    "Executa powercfg /devicedisablewake nos demais".into(),
                ],
            },
        }
    }

    fn query_non_hid_wake_devices() -> Result<Vec<String>, String> {
        let script = r#"
$keep = @(Get-PnpDevice -PresentOnly -Class Mouse,Keyboard -ErrorAction SilentlyContinue | ForEach-Object { $_.FriendlyName })
$armed = @(& powercfg.exe /devicequery wake_armed | ForEach-Object { $_.Trim() } | Where-Object {
    $_ -and $_ -ne 'NONE' -and $_ -ne 'NENHUM' -and ($keep -notcontains $_)
})
if ($armed.Count) { ConvertTo-Json -InputObject @($armed) -Compress } else { '[]' }
"#;
        let out = run_powershell_checked(script)?;
        let s = out.stdout.trim();
        if s.is_empty() || s == "[]" {
            return Ok(Vec::new());
        }
        serde_json::from_str::<Vec<String>>(s)
            .map_err(|e| format!("Falha ao interpretar wake_armed: {e}"))
    }
}

impl Tweak for DeviceWakeTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        match Self::query_non_hid_wake_devices() {
            Ok(list) if list.is_empty() => TweakStatus::Applied,
            Ok(_) => TweakStatus::NotApplied,
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let devices = Self::query_non_hid_wake_devices()?;
        if devices.is_empty() {
            return Ok("Nenhum dispositivo extra com wake armado.".into());
        }
        let json = serde_json::to_value(&devices)
            .map_err(|e| format!("Falha ao serializar wake: {e}"))?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let mut errs = Vec::new();
        for d in &devices {
            if let Err(e) = run_hidden_checked("powercfg.exe", &["/devicedisablewake", d], &[0]) {
                errs.push(e);
            }
        }
        if errs.is_empty() {
            Ok(format!(
                "Wake desativado em {} dispositivo(s): {}",
                devices.len(),
                devices.join(", ")
            ))
        } else {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            Err(errs.join("; "))
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de dispositivos wake encontrado.".into());
        };
        let devices: Vec<String> = serde_json::from_value(rec.data)
            .map_err(|e| format!("Backup de wake inválido: {e}"))?;
        let mut errs = Vec::new();
        for d in &devices {
            if let Err(e) = run_hidden_checked("powercfg.exe", &["/deviceenablewake", d], &[0]) {
                errs.push(e);
            }
        }
        if errs.is_empty() {
            backups.remove(self.meta.id)?;
            Ok(format!(
                "Wake restaurado em {} dispositivo(s).",
                devices.len()
            ))
        } else {
            Err(errs.join("; "))
        }
    }
}

pub struct HiberfilOffTweak {
    meta: TweakMeta,
}

impl HiberfilOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "power.hiberfil_off",
                name: "Hibernação: Remover hiberfil.sys via powercfg /h off (libera 12–24 GB no SSD)",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Executa powercfg /h off para remover definitivamente o arquivo C:\\hiberfil.sys (economizando 12–24 GB no SSD/NVMe) e garantir boot frio limpo. Atenção: o modo Hibernar fica indisponível até reverter (powercfg /h on).",
                details: vec![
                    r"powercfg.exe /h off (remove %SystemDrive%\hiberfil.sys)".into(),
                    "Reversão: powercfg.exe /h on".into(),
                ],
            },
        }
    }

    fn hiberfil_path() -> PathBuf {
        let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        PathBuf::from(format!(r"{drive}\hiberfil.sys"))
    }
}

impl Tweak for HiberfilOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let exists = Self::hiberfil_path().exists();
        let enabled = matches!(
            read_value(
                RegHive::Hklm,
                r"SYSTEM\CurrentControlSet\Control\Power",
                "HibernateEnabled"
            ),
            Ok(Some(RegValue::DWord(1)))
        );
        if !exists && !enabled {
            TweakStatus::Applied
        } else {
            TweakStatus::NotApplied
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let existed = Self::hiberfil_path().exists();
        let json = serde_json::json!({ "existed": existed });
        let created = backups.save_if_absent(self.meta.id, json)?;
        if let Err(e) = run_hidden_checked("powercfg.exe", &["/h", "off"], &[0]) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("Hibernação desativada e hiberfil.sys removido (powercfg /h off).".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de hibernação encontrado.".into());
        };
        let existed = rec.data["existed"].as_bool().unwrap_or(true);
        if existed {
            run_hidden_checked("powercfg.exe", &["/h", "on"], &[0])?;
        }
        backups.remove(self.meta.id)?;
        Ok("Hibernação restaurada ao estado original.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        run_hidden_checked("powercfg.exe", &["/h", "on"], &[0])?;
        Ok("Hibernação reativada (powercfg /h on).".into())
    }
}
