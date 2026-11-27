use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::{run_hidden, run_powershell_checked};
use crate::sys::registry::{
    delete_value, delete_value_backup_restore, key_exists, list_subkeys, list_value_names,
    read_value, write_value, write_value_backup_restore, RegHive, RegValue,
};
use crate::tweaks::registry_tweak::RegSnapshotEntry;
use crate::tweaks::{Category, RiskLevel, Tweak, TweakMeta, TweakStatus};

const PCI_CLASSES: &[&str] = &[
    "{4d36e968-e325-11ce-bfc1-08002be10318}",
    "{4d36e972-e325-11ce-bfc1-08002be10318}",
    "{36fc9e60-c465-11cf-8056-444553540000}",
    "{4d36e97b-e325-11ce-bfc1-08002be10318}",
    "{4d36e96a-e325-11ce-bfc1-08002be10318}",
];

pub struct LegacyHardwareIrqTweak {
    meta: TweakMeta,
}

impl LegacyHardwareIrqTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.legacy_cleanup_irq",
                name: "Limpeza: Remover afinidade fixa legada de IRQ/MSI em dispositivos PCI",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Remove AssignmentSetOverride, DevicePolicy, DevicePriority e MessageNumberLimit gravados por versões antigas quando presentes em Enum\\PCI (preserva MSISupported do driver).",
                details: vec![
                    r"Escaneia HKLM\SYSTEM\CurrentControlSet\Enum\PCI\*\*".into(),
                    "Remove valores apenas onde AssignmentSetOverride foi gravado".into(),
                    "Preserva MSISupported definido pelo INF do driver".into(),
                ],
            },
        }
    }

    fn scan_legacy_targets() -> Vec<(String, String)> {
        let mut ops = Vec::new();
        for vendor in list_subkeys(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Enum\PCI") {
            let v_path = format!(r"SYSTEM\CurrentControlSet\Enum\PCI\{vendor}");
            for device in list_subkeys(RegHive::Hklm, &v_path) {
                let dev_path = format!(r"{v_path}\{device}");
                let class_guid = match read_value(RegHive::Hklm, &dev_path, "ClassGUID") {
                    Ok(Some(RegValue::String(s))) => s.to_ascii_lowercase(),
                    _ => continue,
                };
                if !PCI_CLASSES.contains(&class_guid.as_str()) {
                    continue;
                }
                let aff_path = format!(r"{dev_path}\Device Parameters\Interrupt Management\Affinity Policy");
                if !matches!(
                    read_value(RegHive::Hklm, &aff_path, "AssignmentSetOverride"),
                    Ok(Some(_))
                ) {
                    continue;
                }
                for name in ["DevicePolicy", "AssignmentSetOverride", "DevicePriority"] {
                    if matches!(read_value(RegHive::Hklm, &aff_path, name), Ok(Some(_))) {
                        ops.push((aff_path.clone(), name.to_string()));
                    }
                }
                let msi_path = format!(
                    r"{dev_path}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
                );
                if matches!(read_value(RegHive::Hklm, &msi_path, "MessageNumberLimit"), Ok(Some(_))) {
                    ops.push((msi_path, "MessageNumberLimit".to_string()));
                }
            }
        }
        ops
    }
}

impl Tweak for LegacyHardwareIrqTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_legacy_targets();
        if targets.is_empty() {
            TweakStatus::Applied
        } else {
            TweakStatus::NotApplied
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_legacy_targets();
        if targets.is_empty() {
            return Ok("Nenhuma afinidade fixa de IRQ legada encontrada.".into());
        }
        let mut snap = Vec::new();
        for (subkey, name) in &targets {
            let val = read_value(RegHive::Hklm, subkey, name)?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: subkey.clone(),
                name: name.clone(),
                existed: val.is_some(),
                value: val,
                use_backup_restore: false,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (subkey, name) in &targets {
            if let Err(e) = delete_value(RegHive::Hklm, subkey, name) {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "Removido(s) {} valor(es) legado(s) de afinidade PCI.",
            targets.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para esta limpeza.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for entry in &snap {
            if entry.existed {
                if let Some(v) = &entry.value {
                    write_value(entry.hive, &entry.subkey, &entry.name, v)?;
                }
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("Restaurados {} valor(es) PCI.", snap.len()))
    }
}

pub struct MemoryCompressionTweak {
    meta: TweakMeta,
}

impl MemoryCompressionTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.memory_compression_off",
                name: "Desativar Compressão de Memória (MMAgent)",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Desativa compressão de memória em RAM via Disable-MMAgent -MemoryCompression (útil com 32 GB DDR5 para evitar trabalho de CPU comprimindo páginas).",
                details: vec![
                    "Get-MMAgent / Disable-MMAgent -MemoryCompression".into(),
                    "Reversão: garante SysMain rodando e chama Enable-MMAgent -MemoryCompression".into(),
                ],
            },
        }
    }

    fn is_enabled() -> Result<bool, String> {
        let out = run_powershell_checked("Write-Output ([bool](Get-MMAgent).MemoryCompression)")?;
        Ok(out.stdout.trim().eq_ignore_ascii_case("True"))
    }
}

impl Tweak for MemoryCompressionTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        match Self::is_enabled() {
            Ok(false) => TweakStatus::Applied,
            Ok(true) => TweakStatus::NotApplied,
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let was_enabled = Self::is_enabled()?;
        if !was_enabled {
            return Ok("Compressão de memória já estava desativada.".into());
        }
        let json = serde_json::json!({ "memory_compression": was_enabled });
        let created = backups.save_if_absent(self.meta.id, json)?;
        if let Err(e) = run_powershell_checked(
            "Start-Service SysMain -ErrorAction SilentlyContinue; Disable-MMAgent -MemoryCompression -ErrorAction Stop",
        ) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("Compressão de memória desativada (efeito completo após reiniciar).".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para Compressão de Memória.".into());
        };
        let was_enabled = rec.data["memory_compression"].as_bool().unwrap_or(true);
        if was_enabled {
            run_powershell_checked(
                "Start-Service SysMain -ErrorAction SilentlyContinue; Enable-MMAgent -MemoryCompression -ErrorAction Stop",
            )?;
        }
        backups.remove(self.meta.id)?;
        Ok(format!(
            "Compressão de memória restaurada (era {was_enabled})."
        ))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        run_powershell_checked(
            "Start-Service SysMain -ErrorAction SilentlyContinue; Enable-MMAgent -MemoryCompression -ErrorAction Stop",
        )?;
        Ok("Compressão de memória reativada (padrão do Windows).".into())
    }
}

const ETW_AUTOLOGGERS: &[&str] = &[
    "AutoLogger-Diagtrack-Listener",
    "Diagtrack-Listener",
    "DiagLog",
    "WdiContextLog",
    "NetCore",
    "LwtNetLog",
    "WiFiSession",
    "RadioMgr",
    "SpoolerLogger",
    "Microsoft-Windows-Rdp-Graphics-RdpIdd-Trace",
    "ReFSLog",
    "EventLog-Application",
    "EventLog-System",
    "Eventlog-Security",
    "EventLog-Microsoft-Windows-CodeIntegrity-Verbose",
    "DefenderApiLogger",
    "DefenderAuditLogger",
    "NtfsLog",
    "FilterMgr-Logger",
    "CimFSUnionFS-Filter",
    "ReadyBoot",
    "ScreenOnPowerStudyTraceSession",
    "CloudExperienceHostOobe",
    "SQMLogger",
];

#[derive(Debug, Serialize, Deserialize)]
struct EtwSnapshotEntry {
    name: String,
    original_start: u32,
}

pub struct EtwAutologgerTweak {
    meta: TweakMeta,
}

impl EtwAutologgerTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "etw.autologgers_off",
                name: "Desativar 24 sessões de rastreamento ETW (Autologger)",
                category: Category::EtwAutologgers,
                risk: RiskLevel::Medium,
                requires_reboot: true,
                requires_admin: true,
                description: "Define Start=0 via REG_OPTION_BACKUP_RESTORE nos Autologgers não essenciais (incluindo AutoLogger-Diagtrack-Listener, ReadyBoot, ScreenOnPowerStudyTraceSession, CloudExperienceHostOobe, SQMLogger, rede, Wi-Fi, EventLog e Defender) e envia logman stop. Preserva UBPM e Circular Kernel Context Logger.",
                details: ETW_AUTOLOGGERS
                    .iter()
                    .map(|n| format!("Autologger\\{n}\\Start = 0"))
                    .collect(),
            },
        }
    }

    fn subkey(name: &str) -> String {
        format!(r"SYSTEM\CurrentControlSet\Control\WMI\Autologger\{name}")
    }

    fn defender_active() -> bool {
        matches!(
            read_value(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Services\WinDefend", "Start"),
            Ok(Some(RegValue::DWord(s))) if s != 4
        )
    }

    fn is_protected_defender_logger(name: &str) -> bool {
        Self::defender_active() && (name == "DefenderApiLogger" || name == "DefenderAuditLogger")
    }
}

impl Tweak for EtwAutologgerTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let mut present = 0usize;
        let mut off = 0usize;
        for name in ETW_AUTOLOGGERS {
            if Self::is_protected_defender_logger(name) {
                continue;
            }
            let sk = Self::subkey(name);
            if !key_exists(RegHive::Hklm, &sk) {
                continue;
            }
            present += 1;
            if matches!(read_value(RegHive::Hklm, &sk, "Start"), Ok(Some(RegValue::DWord(0)))) {
                off += 1;
            }
        }
        if present == 0 {
            TweakStatus::NotApplicable("Nenhum Autologger alvo encontrado".into())
        } else if off == present {
            TweakStatus::Applied
        } else if off == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{off}/{present} sessões"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let mut snap = Vec::new();
        let mut targets = Vec::new();
        for name in ETW_AUTOLOGGERS {
            if Self::is_protected_defender_logger(name) {
                continue;
            }
            let sk = Self::subkey(name);
            if !key_exists(RegHive::Hklm, &sk) {
                continue;
            }
            if let Ok(Some(RegValue::DWord(v))) = read_value(RegHive::Hklm, &sk, "Start") {
                snap.push(EtwSnapshotEntry {
                    name: (*name).to_string(),
                    original_start: v,
                });
                targets.push((*name, sk));
            }
        }
        if targets.is_empty() {
            return Ok("Nenhum Autologger encontrado.".into());
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let mut errs = Vec::new();
        for (name, sk) in &targets {
            if let Err(e) = write_value_backup_restore(RegHive::Hklm, sk, "Start", &RegValue::DWord(0))
                .or_else(|_| write_value(RegHive::Hklm, sk, "Start", &RegValue::DWord(0)))
            {
                errs.push(format!("{name}: {e}"));
                continue;
            }
            let _ = run_hidden("logman.exe", &["stop", name, "-ets"]);
            if *name == "AutoLogger-Diagtrack-Listener" {
                let _ = run_hidden("logman.exe", &["stop", "Diagtrack-Listener", "-ets"]);
            }
        }
        if errs.is_empty() {
            Ok(format!("{} Autologger(s) desativado(s) (vale completo no boot).", targets.len()))
        } else {
            if created && errs.len() == targets.len() {
                let _ = backups.remove(self.meta.id);
            }
            Err(errs.join("; "))
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de Autologger encontrado.".into());
        };
        let snap: Vec<EtwSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup ETW inválido: {e}"))?;
        let mut errs = Vec::new();
        for entry in &snap {
            if Self::is_protected_defender_logger(&entry.name) {
                continue;
            }
            let sk = Self::subkey(&entry.name);
            if !key_exists(RegHive::Hklm, &sk) {
                continue;
            }
            let val = RegValue::DWord(entry.original_start);
            if let Err(e) = write_value_backup_restore(RegHive::Hklm, &sk, "Start", &val)
                .or_else(|_| write_value(RegHive::Hklm, &sk, "Start", &val))
            {
                errs.push(format!("{}: {e}", entry.name));
            }
        }
        if errs.is_empty() {
            backups.remove(self.meta.id)?;
            Ok(format!("{} Autologger(s) restaurado(s).", snap.len()))
        } else {
            Err(errs.join("; "))
        }
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let mut count = 0usize;
        for name in ETW_AUTOLOGGERS {
            if Self::is_protected_defender_logger(name) {
                continue;
            }
            let sk = Self::subkey(name);
            if !key_exists(RegHive::Hklm, &sk) {
                continue;
            }
            let val = RegValue::DWord(1);
            write_value_backup_restore(RegHive::Hklm, &sk, "Start", &val)
                .or_else(|_| write_value(RegHive::Hklm, &sk, "Start", &val))?;
            count += 1;
        }
        Ok(format!("{count} Autologger(s) definidos para Start=1 (padrão)."))
    }
}

pub struct SmartScreenAclTweak {
    meta: TweakMeta,
}

impl SmartScreenAclTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.smartscreen_acl",
                name: "Bloquear execução COM de smartscreen.exe (ACL)",
                category: Category::SystemKernel,
                risk: RiskLevel::Medium,
                requires_reboot: false,
                requires_admin: true,
                description: "Nega execução para Todos (*S-1-1-0:(X)) em System32\\smartscreen.exe impedindo abertura em segundo plano via COM (-Embedding), salvando o SDDL original. Atenção: desativa a verificação do SmartScreen ao abrir apps.",
                details: vec![
                    r"Arquivo: %SystemRoot%\System32\smartscreen.exe".into(),
                    "Salva SDDL original no backup antes de aplicar icacls /deny *S-1-1-0:(X)".into(),
                    "Devolve propriedade do arquivo para NT SERVICE\\TrustedInstaller".into(),
                ],
            },
        }
    }

    fn exe_path() -> PathBuf {
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
        root.join("System32").join("smartscreen.exe")
    }
}

impl Tweak for SmartScreenAclTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let exe = Self::exe_path();
        if !exe.exists() {
            return TweakStatus::NotApplicable("smartscreen.exe ausente".into());
        }
        let p = exe.to_string_lossy().replace('\'', "''");
        let script = format!(
            r#"
$acl = Get-Acl -LiteralPath '{p}'
$hasDeny = @($acl.Access | Where-Object {{ $_.AccessControlType -eq 'Deny' -and $_.IdentityReference -match 'Everyone|Todos|S-1-1-0' }}).Count -gt 0
Write-Output $hasDeny
"#
        );
        match run_powershell_checked(&script) {
            Ok(out) => {
                if out.stdout.trim().eq_ignore_ascii_case("True") {
                    TweakStatus::Applied
                } else {
                    TweakStatus::NotApplied
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let exe = Self::exe_path();
        if !exe.exists() {
            return Ok("smartscreen.exe não está presente.".into());
        }
        let p = exe.to_string_lossy().replace('\'', "''");
        let sddl_out = run_powershell_checked(&format!("(Get-Acl -LiteralPath '{p}').Sddl"))?;
        let sddl = sddl_out.stdout.trim().to_string();
        let json = serde_json::json!({ "arquivo": exe.to_string_lossy(), "sddl": sddl });
        let created = backups.save_if_absent(self.meta.id, json)?;

        let apply_script = format!(
            r#"
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class Priv01Low {{
 [StructLayout(LayoutKind.Sequential, Pack=1)] struct TP {{ public int Count; public long Luid; public int Attr; }}
 [DllImport("advapi32.dll", SetLastError=true)] static extern bool OpenProcessToken(IntPtr h, int acc, out IntPtr t);
 [DllImport("advapi32.dll", SetLastError=true)] static extern bool LookupPrivilegeValue(string s, string n, out long l);
 [DllImport("advapi32.dll", SetLastError=true)] static extern bool AdjustTokenPrivileges(IntPtr t, bool d, ref TP n, int l, IntPtr p, IntPtr r);
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 public static bool Ativar(string nome){{ IntPtr t; if(!OpenProcessToken(GetCurrentProcess(),0x28,out t)) return false; var tp=new TP{{Count=1,Attr=2}}; if(!LookupPrivilegeValue(null,nome,out tp.Luid)) return false; return AdjustTokenPrivileges(t,false,ref tp,0,IntPtr.Zero,IntPtr.Zero) && Marshal.GetLastWin32Error()==0; }} }}
'@ -ErrorAction SilentlyContinue
[void][Priv01Low]::Ativar('SeRestorePrivilege')
[void][Priv01Low]::Ativar('SeTakeOwnershipPrivilege')
$exe = '{p}'
& takeown.exe /f $exe /a | Out-Null
& icacls.exe $exe /deny '*S-1-1-0:(X)' | Out-Null
$a = Get-Acl -LiteralPath $exe
$a.SetOwner((New-Object System.Security.Principal.NTAccount('NT SERVICE','TrustedInstaller')))
Set-Acl -LiteralPath $exe -AclObject $a -ErrorAction Stop
if (-not @((Get-Acl -LiteralPath $exe).Access | Where-Object {{ $_.AccessControlType -eq 'Deny' }}).Count) {{
    throw "Bloqueio ACL não aplicado em $exe"
}}
"#
        );
        match run_powershell_checked(&apply_script) {
            Ok(_) => Ok("Bloqueio de execução aplicado em smartscreen.exe.".into()),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup SDDL de smartscreen.exe encontrado.".into());
        };
        let arquivo = rec.data["arquivo"]
            .as_str()
            .ok_or("Backup SDDL sem caminho de arquivo.")?
            .replace('\'', "''");
        let sddl = rec.data["sddl"]
            .as_str()
            .ok_or("Backup SDDL inválido.")?
            .replace('\'', "''");

        let script = format!(
            r#"
Add-Type -TypeDefinition @'
using System; using System.Runtime.InteropServices;
public static class PrivRev01Low {{
 [StructLayout(LayoutKind.Sequential, Pack=1)] struct TP {{ public int Count; public long Luid; public int Attr; }}
 [DllImport("advapi32.dll", SetLastError=true)] static extern bool OpenProcessToken(IntPtr h, int acc, out IntPtr t);
 [DllImport("advapi32.dll", SetLastError=true)] static extern bool LookupPrivilegeValue(string s, string n, out long l);
 [DllImport("advapi32.dll", SetLastError=true)] static extern bool AdjustTokenPrivileges(IntPtr t, bool d, ref TP n, int l, IntPtr p, IntPtr r);
 [DllImport("kernel32.dll")] static extern IntPtr GetCurrentProcess();
 public static bool Ativar(string nome){{ IntPtr t; if(!OpenProcessToken(GetCurrentProcess(),0x28,out t)) return false; var tp=new TP{{Count=1,Attr=2}}; if(!LookupPrivilegeValue(null,nome,out tp.Luid)) return false; return AdjustTokenPrivileges(t,false,ref tp,0,IntPtr.Zero,IntPtr.Zero) && Marshal.GetLastWin32Error()==0; }} }}
'@ -ErrorAction SilentlyContinue
[void][PrivRev01Low]::Ativar('SeRestorePrivilege')
[void][PrivRev01Low]::Ativar('SeTakeOwnershipPrivilege')
$a = Get-Acl -LiteralPath '{arquivo}'
$a.SetSecurityDescriptorSddlForm('{sddl}')
Set-Acl -LiteralPath '{arquivo}' -AclObject $a -ErrorAction Stop
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok("ACL e proprietário originais de smartscreen.exe restaurados.".into())
    }
}

const SYSTEM_APPX_LIST: &[&str] = &[
    "Windows.CBSPreview",
    "Microsoft.Windows.XGpuEjectDialog",
    "Microsoft.MicrosoftEdgeDevToolsClient",
    "Microsoft.ECApp",
    "Microsoft.Windows.SecureAssessmentBrowser",
    "Microsoft.Windows.ParentalControls",
    "Microsoft.Windows.PeopleExperienceHost",
    "Microsoft.Windows.NarratorQuickStart",
    "Microsoft.Windows.AssignedAccessLockApp",
    "Microsoft.Windows.Apprep.ChxApp",
    "Microsoft.SecHealthUI",
    "Microsoft.Windows.ContentDeliveryManager",
    "Microsoft.Windows.PrintQueueActionCenter",
    "Microsoft.AsyncTextService",
    "MicrosoftWindows.61869720.Voiess",
    "MicrosoftWindows.61869721.Livtop",
    "MicrosoftWindows.61869722.Speion",
    "MicrosoftWindows.Client.CoreAI",
    "Microsoft.AIFabric.CBS.1.6",
    "Microsoft.Windows.AugLoop.CBS",
    "Microsoft.Windows.PinningConfirmationDialog",
];

#[derive(Debug, Serialize, Deserialize)]
struct AppxBackupEntry {
    name: String,
    full: String,
    family: String,
    local: String,
    sids: Vec<String>,
}

pub struct SystemAppxTweak {
    meta: TweakMeta,
}

impl SystemAppxTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "bloat.system_appx",
                name: "Desprovisionar 21 pacotes AppX de sistema sem uso para jogos",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Medium,
                requires_reboot: false,
                requires_admin: true,
                description: "Marca EndOfLife/Deprovisioned e remove para todos os usuários os 21 pacotes SystemApps não essenciais (preserva shell, OOBE, BioEnrollment/Windows Hello, captura de tela, XboxGameCallableUI e TextInputHost).",
                details: SYSTEM_APPX_LIST
                    .iter()
                    .map(|n| format!("AppX: {n}"))
                    .collect(),
            },
        }
    }

    fn ps_names() -> String {
        SYSTEM_APPX_LIST
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Tweak for SystemAppxTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let names = Self::ps_names();
        let all_users = crate::sys::elevation::is_elevated();
        let script = if all_users {
            format!(
                r#"
$nomes = @({names})
$inst = 0
foreach ($n in $nomes) {{
    $p = Get-AppxPackage -AllUsers -Name $n -ErrorAction SilentlyContinue
    if ($p) {{
        $users = @($p | ForEach-Object {{ $_.PackageUserInformation }} | Where-Object {{ $_.UserSecurityId.Sid -ne 'S-1-5-18' -and "$($_.InstallState)" -eq 'Installed' }})
        if ($users.Count) {{ $inst++ }}
    }}
}}
Write-Output $inst
"#
            )
        } else {
            format!(
                r#"
$nomes = @({names})
$inst = 0
foreach ($n in $nomes) {{
    if (Get-AppxPackage -Name $n -ErrorAction SilentlyContinue) {{ $inst++ }}
}}
Write-Output $inst
"#
            )
        };
        match run_powershell_checked(&script) {
            Ok(out) => {
                let remaining: usize = out.stdout.trim().parse().unwrap_or(0);
                if remaining == 0 {
                    TweakStatus::Applied
                } else {
                    TweakStatus::Partial(format!("{remaining} pacote(s) ainda ativo(s)"))
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let names = Self::ps_names();
        let query = format!(
            r#"
function Get-SidsAppx($p){{
    $s=@('S-1-5-18')
    foreach($u in $p.PackageUserInformation){{
        if($u.UserSecurityId.Sid){{ $s+=$u.UserSecurityId.Sid }}
        elseif($u.UserSecurityId.Username){{ try{{ $s+=(New-Object System.Security.Principal.NTAccount($u.UserSecurityId.Username)).Translate([System.Security.Principal.SecurityIdentifier]).Value }}catch{{}} }}
    }}
    $s+=@(Get-CimInstance Win32_UserProfile -ErrorAction SilentlyContinue | Where-Object {{ -not $_.Special }} | ForEach-Object {{ $_.SID }})
    @($s | Where-Object {{ $_ }} | Select-Object -Unique)
}}
$alvo = @(foreach($n in @({names})) {{ Get-AppxPackage -AllUsers -Name $n -ErrorAction SilentlyContinue }})
$rows = foreach ($p in $alvo) {{
    [pscustomobject]@{{ name=$p.Name; full=$p.PackageFullName; family=$p.PackageFamilyName; local=$p.InstallLocation; sids=@(Get-SidsAppx $p) }}
}}
if ($rows) {{ ConvertTo-Json -InputObject @($rows) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let rows: Vec<AppxBackupEntry> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(format!("Nenhum dos {} pacotes AppX alvo está instalado.", SYSTEM_APPX_LIST.len()));
        }
        let json = serde_json::to_value(&rows).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply_script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
$base = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Appx\AppxAllUserStore'
$falhas = @()
$ok = 0
foreach ($p in $rows) {{
    foreach ($s in @($p.sids)) {{ New-Item -Path "$base\EndOfLife\$s\$($p.full)" -Force | Out-Null }}
    New-Item -Path "$base\Deprovisioned\$($p.family)" -Force | Out-Null
    $msg = $null
    try {{ Remove-AppxPackage -Package $p.full -AllUsers -ErrorAction Stop }} catch {{ $msg = $_.Exception.Message }}
    $resta = @(Get-AppxPackage -AllUsers -Name $p.name -ErrorAction SilentlyContinue | ForEach-Object {{ $_.PackageUserInformation }} |
        Where-Object {{ $_.UserSecurityId.Sid -ne 'S-1-5-18' -and "$($_.InstallState)" -eq 'Installed' }})
    if ($resta.Count) {{ $falhas += "$($p.name) ($msg)" }} else {{ $ok++ }}
}}
if ($falhas.Count) {{ throw ($falhas -join '; ') }}
Write-Output $ok
"#
        );
        match run_powershell_checked(&apply_script) {
            Ok(r) => Ok(format!("{} pacote(s) AppX de sistema removido(s).", r.stdout.trim())),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de AppX encontrado.".into());
        };
        let rows: Vec<AppxBackupEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
$base = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Appx\AppxAllUserStore'
$falhas = @()
foreach ($a in $rows) {{
    foreach ($s in @($a.sids)) {{ Remove-Item -LiteralPath "$base\EndOfLife\$s\$($a.full)" -Force -ErrorAction SilentlyContinue }}
    Remove-Item -LiteralPath "$base\Deprovisioned\$($a.family)" -Force -ErrorAction SilentlyContinue
    try {{ Add-AppxPackage -Register (Join-Path $a.local 'AppxManifest.xml') -DisableDevelopmentMode -ErrorAction Stop }}
    catch {{ $falhas += "$($a.name): $($_.Exception.Message)" }}
}}
if ($falhas.Count) {{ throw ($falhas -join '; ') }}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("{} pacote(s) AppX re-registrado(s).", rows.len()))
    }
}

const FW_GROUPS: &[&str] = &[
    "Network Discovery",
    "Cast to Device functionality",
    "Remote Assistance",
    "Wi-Fi Direct Network Discovery",
    "Connected Devices Platform",
    "mDNS",
    "WLAN Service - WFD Services Kernel Mode Driver Rules",
    "AllJoyn Router",
    "Wireless Display",
    "Windows Device Management",
    "Windows Default Lock Screen",
    "DIAL protocol server",
];

pub struct FirewallOrphansTweak {
    meta: TweakMeta,
}

impl FirewallOrphansTweak {
    pub fn new() -> Self {
        let mut details: Vec<String> = FW_GROUPS.iter().map(|g| format!("Grupo FW: {g}")).collect();
        details.push("Grupo FW: *SecHealthUI*".into());
        Self {
            meta: TweakMeta {
                id: "bloat.firewall_orphans",
                name: "Desativar regras de firewall órfãs (Descoberta, mDNS, Cast, Miracast, etc.)",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa regras de firewall ativas associadas a recursos removidos/desativados, salvando os nomes exatos para restauração.",
                details,
            },
        }
    }

    fn ps_groups() -> String {
        FW_GROUPS
            .iter()
            .map(|g| format!("'{g}'"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Tweak for FirewallOrphansTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let groups = Self::ps_groups();
        let script = format!(
            r#"
$g = @({groups})
$active = @(Get-NetFirewallRule -Enabled True -ErrorAction SilentlyContinue | Where-Object {{ $g -contains $_.DisplayGroup -or $_.DisplayGroup -match 'SecHealthUI' }}).Count
Write-Output $active
"#
        );
        match run_powershell_checked(&script) {
            Ok(out) => {
                let active: usize = out.stdout.trim().parse().unwrap_or(0);
                if active == 0 {
                    TweakStatus::Applied
                } else {
                    TweakStatus::NotApplied
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let groups = Self::ps_groups();
        let query = format!(
            r#"
$g = @({groups})
$rules = @(Get-NetFirewallRule -Enabled True -ErrorAction SilentlyContinue | Where-Object {{ $g -contains $_.DisplayGroup -or $_.DisplayGroup -match 'SecHealthUI' }} | ForEach-Object {{ $_.Name }})
if ($rules.Count) {{ ConvertTo-Json -InputObject @($rules) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let names: Vec<String> = serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if names.is_empty() {
            return Ok("Nenhuma regra de firewall órfã ativa.".into());
        }
        let json = serde_json::to_value(&names).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&names)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply = format!(
            r#"
$names = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
if ($names.Count) {{ Disable-NetFirewallRule -Name $names -ErrorAction Stop }}
"#
        );
        match run_powershell_checked(&apply) {
            Ok(_) => Ok(format!("{} regra(s) de firewall desativada(s).", names.len())),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de regras de firewall encontrado.".into());
        };
        let names: Vec<String> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        let escaped = serde_json::to_string(&names)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$names = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
if ($names.Count) {{ Enable-NetFirewallRule -Name $names -ErrorAction Stop }}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("{} regra(s) de firewall reativada(s).", names.len()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct PnpDeviceEntry {
    name: String,
    instance_id: String,
}

pub struct PnpUnusedDevicesTweak {
    meta: TweakMeta,
}

impl PnpUnusedDevicesTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "bloat.pnp_unused",
                name: "Desativar dispositivos PnP sem uso (SATA vazio e Áudio HDMI AMD)",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Medium,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa controladoras SATA sem nenhum disco conectado e áudio HDMI/DP AMD, reduzindo dispositivos ociosos no barramento. O NdisVirtualBus é preservado para manter ExitLag, NoPing e VPNs funcionando. Atenção: se usar som via cabo HDMI/DP de GPU AMD no monitor, o áudio por HDMI/DP para.",
                details: vec![
                    "Controladoras SATA sem filhos conectados".into(),
                    "AMD High Definition Audio Device (HDMI/DP)".into(),
                ],
            },
        }
    }

    fn query_targets() -> Result<Vec<PnpDeviceEntry>, String> {
        let script = r#"
$disp = @()
$disp += @(Get-PnpDevice -PresentOnly -Class SCSIAdapter,HDC -ErrorAction SilentlyContinue | Where-Object { $_.FriendlyName -match 'SATA' -and $_.Status -eq 'OK' } |
    Where-Object { -not @((Get-PnpDeviceProperty -InstanceId $_.InstanceId -KeyName DEVPKEY_Device_Children -ErrorAction SilentlyContinue).Data | Where-Object { $_ }).Count })
$disp += @(Get-PnpDevice -PresentOnly -Class MEDIA -ErrorAction SilentlyContinue | Where-Object { $_.FriendlyName -eq 'AMD High Definition Audio Device' -and $_.Status -eq 'OK' })
$rows = foreach ($d in $disp) { [pscustomobject]@{ name = $d.FriendlyName; instance_id = $d.InstanceId } }
if ($rows) { ConvertTo-Json -InputObject @($rows) -Compress } else { '[]' }
"#;
        let out = run_powershell_checked(script)?;
        serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())
    }
}

impl Tweak for PnpUnusedDevicesTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        match Self::query_targets() {
            Ok(list) if list.is_empty() => TweakStatus::Applied,
            Ok(_) => TweakStatus::NotApplied,
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::query_targets()?;
        if targets.is_empty() {
            return Ok("Nenhum dispositivo PnP ocioso ativo encontrado.".into());
        }
        let json = serde_json::to_value(&targets).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&targets)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($d in $rows) {{
    Disable-PnpDevice -InstanceId $d.instance_id -Confirm:$false -ErrorAction Stop
}}
"#
        );
        match run_powershell_checked(&script) {
            Ok(_) => Ok(format!("{} dispositivo(s) PnP desativado(s).", targets.len())),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de dispositivos PnP encontrado.".into());
        };
        let rows: Vec<PnpDeviceEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($d in $rows) {{
    Enable-PnpDevice -InstanceId $d.instance_id -Confirm:$false -ErrorAction Stop
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("{} dispositivo(s) PnP reativado(s).", rows.len()))
    }
}

const GPU_CLASS_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

pub struct AmdShaderCacheTweak {
    meta: TweakMeta,
}

impl AmdShaderCacheTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.amd_shader_cache",
                name: "GPU AMD: Cache de Shader em 'Always On'",
                category: Category::GpuLatency,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Define UMD\\ShaderCache = 32 00 para GPUs AMD (VEN_1002). Marca como N/A em sistemas com GPU NVIDIA (ex.: RTX 5080).",
                details: vec![
                    format!(r"HKLM\{GPU_CLASS_KEY}\00xx\UMD\ShaderCache = [0x32, 0x00]"),
                ],
            },
        }
    }

    fn amd_umd_keys() -> Vec<String> {
        let mut res = Vec::new();
        for sub in list_subkeys(RegHive::Hklm, GPU_CLASS_KEY) {
            if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let dev_key = format!(r"{GPU_CLASS_KEY}\{sub}");
            if let Ok(Some(RegValue::String(id))) =
                read_value(RegHive::Hklm, &dev_key, "MatchingDeviceId")
            {
                if id.to_ascii_uppercase().contains("VEN_1002") {
                    let umd = format!(r"{dev_key}\UMD");
                    if key_exists(RegHive::Hklm, &umd) {
                        res.push(umd);
                    }
                }
            }
        }
        res
    }
}

impl Tweak for AmdShaderCacheTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let umds = Self::amd_umd_keys();
        if umds.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma GPU AMD (VEN_1002) detectada".into());
        }
        let target = vec![0x32u8, 0x00u8];
        let ok = umds
            .iter()
            .filter(|u| matches!(read_value(RegHive::Hklm, u, "ShaderCache"), Ok(Some(RegValue::Binary(b))) if b == target))
            .count();
        if ok == umds.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} GPUs AMD", umds.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let umds = Self::amd_umd_keys();
        if umds.is_empty() {
            return Err("Nenhuma GPU AMD (VEN_1002) instalada neste PC.".into());
        }
        let mut snap = Vec::new();
        for u in &umds {
            let cur = read_value(RegHive::Hklm, u, "ShaderCache")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: u.clone(),
                name: "ShaderCache".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: false,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for u in &umds {
            if let Err(e) = write_value(
                RegHive::Hklm,
                u,
                "ShaderCache",
                &RegValue::Binary(vec![0x32, 0x00]),
            ) {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!("ShaderCache AMD definido em Always On ({} GPU(s)).", umds.len()))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup do ShaderCache AMD encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("ShaderCache AMD restaurado ao padrão do driver.".into())
    }
}

pub struct PresentMonFpsTweak {
    meta: TweakMeta,
}

impl PresentMonFpsTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.presentmon_fps",
                name: "NVIDIA App / PresentMon: Permissões para FPS e Frametime",
                category: Category::GpuLatency,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Adiciona Users e o usuário atual ao grupo 'Performance Log Users' (SID S-1-5-32-559) e registra manifesto ETW FrameViewSDK (nvinstall.cmd) para métricas de FPS/1% low.",
                details: vec![
                    "Grupo local S-1-5-32-559 (Performance Log Users) += Users / usuário atual".into(),
                    r"Executa C:\Program Files\NVIDIA Corporation\FrameViewSDK\etw_nv\nvinstall.cmd se presente".into(),
                ],
            },
        }
    }
}

impl Tweak for PresentMonFpsTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let script = r#"
$g = Get-LocalGroup | Where-Object { $_.SID -eq 'S-1-5-32-559' } | Select-Object -First 1
if (-not $g) { Write-Output 'NA'; exit 0 }
$usersGroup = $null
try {
    $usersGroup = (New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-545')).Translate([System.Security.Principal.NTAccount]).Value
} catch {}
$members = @(Get-LocalGroupMember -Group $g.Name -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
$ok = ($members -match '\\Users$|\\Usuários$|\\Gamer$' -or ($usersGroup -and ($members -contains $usersGroup)) -or $members -contains "$env:USERDOMAIN\$env:USERNAME" -or $members -contains "$env:COMPUTERNAME\$env:USERNAME").Count -gt 0
Write-Output $ok
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NA" {
                    TweakStatus::NotApplicable("Grupo S-1-5-32-559 ausente".into())
                } else if s.eq_ignore_ascii_case("True") {
                    TweakStatus::Applied
                } else {
                    TweakStatus::NotApplied
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let script = r#"
$g = Get-LocalGroup | Where-Object { $_.SID -eq 'S-1-5-32-559' } | Select-Object -First 1
if (-not $g) { throw 'Grupo Performance Log Users (S-1-5-32-559) não encontrado.' }
$usersGroup = 'Users'
try {
    $usersGroup = (New-Object System.Security.Principal.SecurityIdentifier('S-1-5-32-545')).Translate([System.Security.Principal.NTAccount]).Value
} catch {}
$added = @()
$candidates = @($usersGroup, 'Gamer', "$env:USERDOMAIN\$env:USERNAME") | Where-Object { $_ } | Select-Object -Unique
foreach ($m in $candidates) {
    try {
        Add-LocalGroupMember -Group $g.Name -Member $m -ErrorAction Stop
        $added += $m
    } catch {}
}
$nvInst = 'C:\Program Files\NVIDIA Corporation\FrameViewSDK\etw_nv\nvinstall.cmd'
$nvRan = $false
if (Test-Path -LiteralPath $nvInst) {
    Start-Process cmd.exe -ArgumentList "/c cd /d `"$(Split-Path $nvInst)`" && nvinstall.cmd" -Wait -WindowStyle Hidden
    $nvRan = $true
}
[pscustomobject]@{ group = $g.Name; added = @($added); nv = $nvRan } | ConvertTo-Json -Compress
"#;
        let out = run_powershell_checked(script)?;
        let val: serde_json::Value =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        backups.save_if_absent(self.meta.id, val.clone())?;
        Ok(format!(
            "Permissões PresentMon configuradas (adicionados: {}, FrameView ETW: {}).",
            val["added"]
                .as_array()
                .map(|a| a.len())
                .unwrap_or(0),
            val["nv"].as_bool().unwrap_or(false)
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup PresentMon encontrado.".into());
        };
        let group = rec.data["group"].as_str().unwrap_or("Performance Log Users");
        let added = rec.data["added"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>();
        if !added.is_empty() {
            let escaped = serde_json::to_string(&added)
                .map_err(|e| e.to_string())?
                .replace('\'', "''");
            let g_esc = group.replace('\'', "''");
            let script = format!(
                r#"
$members = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
foreach ($m in $members) {{
    Remove-LocalGroupMember -Group '{g_esc}' -Member $m -ErrorAction SilentlyContinue
}}
"#
            );
            run_powershell_checked(&script)?;
        }
        backups.remove(self.meta.id)?;
        Ok("Membros adicionados a Performance Log Users removidos.".into())
    }
}

pub struct ThirdPartyBloatTweak {
    meta: TweakMeta,
}

impl ThirdPartyBloatTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "svc.third_party_bloat",
                name: "Terceiros: Desativar serviços, tarefas e updates (AMD, Razer, MSI, ASUS, Google, Adobe, Intel DSA)",
                category: Category::ThirdParty,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa itens de atualização/telemetria de terceiros (Crash Defender, Razer Game Manager/Update, MSI Center, amd3dvcacheSvc, AsusUpdateCheck, GoogleUpdater, AdobeARMservice/AGSService, Intel DSAService/DSAUpdateService, NvDriverUpdateCheckDaily, NVIDIA App SelfUpdate, PlatformExperiencesHelper, StartCN/StartDVR, StartupApproved Razer/AMDNoiseSuppression/GoogleUpdaterTaskUser). Mantém Synapse/External Events utilizáveis manualmente.",
                details: vec![
                    "Serviços: AMD Crash Defender, Razer Game Manager 3/Update, MSI_Case/Center, amd3dvcacheSvc, AsusUpdateCheck, GoogleUpdater*, AdobeARMservice, AGSService, DSAService, DSAUpdateService".into(),
                    "Tarefas: AMDRyzenMasterSDKTask, AMD Check For Updates, AMDInstallLauncher, StartDVR, StartCN, NvDriverUpdateCheckDaily*, NVIDIA App SelfUpdate*, PlatformExperiencesHelper_*, GoogleUpdaterTaskSystem/User*, Adobe Acrobat Update Task*, AdobeGCInvoker*".into(),
                    "Drivers Ryzen Master V29/V32 Start=4, AMD DisableAutoUpdates=1, StartupApproved RazerAppEngine/AMDNoiseSuppression/GoogleUpdaterTaskUser*".into(),
                ],
            },
        }
    }
}

impl Tweak for ThirdPartyBloatTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let script = r#"
$names = @('AMD Crash Defender Service','Razer Game Manager Service 3','Razer Update Service','MSI_Case_Service','MSI_Center_Service','amd3dvcacheSvc','AsusUpdateCheck','AdobeARMservice','AGSService','DSAService','DSAUpdateService')
$svcAll = @(Get-CimInstance Win32_Service -ErrorAction SilentlyContinue | Where-Object { $names -contains $_.Name -or $_.Name -like 'GoogleUpdaterService*' -or $_.Name -like 'GoogleUpdaterInternalService*' })
$svcPending = @($svcAll | Where-Object { $_.StartMode -ne 'Disabled' }).Count
$tasksAll = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object {
    $_.TaskName -in @('AMDRyzenMasterSDKTask','AMD Install Manager - Check For Updates','AMDInstallLauncher','StartDVR','StartCN') -or
    $_.TaskName -like 'GoogleUpdaterTaskSystem*' -or
    $_.TaskName -like 'GoogleUpdaterTaskUser*' -or
    $_.TaskName -like 'NvDriverUpdateCheckDaily*' -or
    $_.TaskName -like 'NVIDIA App SelfUpdate*' -or
    $_.TaskName -like 'PlatformExperiencesHelper_*' -or
    $_.TaskName -like 'Adobe Acrobat Update Task*' -or
    $_.TaskName -like 'AdobeGCInvoker*'
})
$tasksPending = @($tasksAll | Where-Object { $_.State -ne 'Disabled' }).Count
$drvPending = 0
$drvTotal = 0
foreach ($n in 'AMDRyzenMasterDriverV29','AMDRyzenMasterDriverV32') {
    $st = (Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$n" -Name Start -ErrorAction SilentlyContinue).Start
    if ($null -ne $st) {
        $drvTotal++
        if ($st -lt 4) { $drvPending++ }
    }
}
$amdKey = 'HKLM:\SOFTWARE\AMD\AMDInstallManager\AutoUpdate'
$amdVal = (Get-ItemProperty -LiteralPath $amdKey -Name DisableAutoUpdates -ErrorAction SilentlyContinue).DisableAutoUpdates
$amdTotal = if ($null -ne $amdVal) { 1 } else { 0 }
$amdPending = if ($null -ne $amdVal -and $amdVal -ne 1) { 1 } else { 0 }
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$apKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
$runReg = Get-Item -LiteralPath $runKey -ErrorAction SilentlyContinue
$apAtual = Get-ItemProperty -LiteralPath $apKey -ErrorAction SilentlyContinue
$runItems = @(if ($runReg) { $runReg.GetValueNames() | Where-Object { $_ -in @('RazerAppEngine','AMDNoiseSuppression') -or $_ -like 'GoogleUpdaterTaskUser*' } } else { @() })
$runPending = @($runItems | Where-Object { -not ($apAtual -and $null -ne $apAtual.$_ -and ([int]@($apAtual.$_)[0] -band 1)) }).Count
$total = $svcAll.Count + $tasksAll.Count + $runItems.Count + $drvTotal + $amdTotal
$pend = $svcPending + $tasksPending + $drvPending + $amdPending + $runPending
Write-Output "$pend/$total"
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                if let Some((p, t)) = out.stdout.trim().split_once('/') {
                    let pend: usize = p.parse().unwrap_or(0);
                    let total: usize = t.parse().unwrap_or(0);
                    if total == 0 && pend == 0 {
                        TweakStatus::NotApplicable("Nenhum item de terceiros instalado".into())
                    } else if pend == 0 {
                        TweakStatus::Applied
                    } else {
                        TweakStatus::NotApplied
                    }
                } else {
                    TweakStatus::Error(out.stdout)
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query_script = r#"
$names = @('AMD Crash Defender Service','Razer Game Manager Service 3','Razer Update Service','MSI_Case_Service','MSI_Center_Service','amd3dvcacheSvc','AsusUpdateCheck','AdobeARMservice','AGSService','DSAService','DSAUpdateService')
$found = @(Get-CimInstance Win32_Service -ErrorAction SilentlyContinue | Where-Object { $names -contains $_.Name -or $_.Name -like 'GoogleUpdaterService*' -or $_.Name -like 'GoogleUpdaterInternalService*' } | Where-Object { $_.StartMode -ne 'Disabled' })
$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object {
    ($_.TaskName -in @('AMDRyzenMasterSDKTask','AMD Install Manager - Check For Updates','AMDInstallLauncher','StartDVR','StartCN') -or
    $_.TaskName -like 'GoogleUpdaterTaskSystem*' -or
    $_.TaskName -like 'GoogleUpdaterTaskUser*' -or
    $_.TaskName -like 'NvDriverUpdateCheckDaily*' -or
    $_.TaskName -like 'NVIDIA App SelfUpdate*' -or
    $_.TaskName -like 'PlatformExperiencesHelper_*' -or
    $_.TaskName -like 'Adobe Acrobat Update Task*' -or
    $_.TaskName -like 'AdobeGCInvoker*') -and $_.State -ne 'Disabled'
})
$drivers = @(foreach ($n in 'AMDRyzenMasterDriverV29','AMDRyzenMasterDriverV32') {
    $st = (Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$n" -Name Start -ErrorAction SilentlyContinue).Start
    if ($null -ne $st -and $st -lt 4) { [pscustomobject]@{ Name=$n; Start=$st } }
})
$amdKey = 'HKLM:\SOFTWARE\AMD\AMDInstallManager\AutoUpdate'
$amdAuto = @(Get-ItemProperty -LiteralPath $amdKey -Name DisableAutoUpdates -ErrorAction SilentlyContinue | Where-Object { $_.DisableAutoUpdates -ne 1 })
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
$apKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
$runReg = Get-Item -LiteralPath $runKey -ErrorAction SilentlyContinue
$apAtual = Get-ItemProperty -LiteralPath $apKey -ErrorAction SilentlyContinue
$runItems = @(if ($runReg) { $runReg.GetValueNames() | Where-Object { $_ -in @('RazerAppEngine','AMDNoiseSuppression') -or $_ -like 'GoogleUpdaterTaskUser*' } |
    Where-Object { -not ($apAtual -and $null -ne $apAtual.$_ -and ([int]@($apAtual.$_)[0] -band 1)) } })

$snap = [ordered]@{
    servicos = @($found | ForEach-Object { [pscustomobject]@{ Name=$_.Name; StartMode=$_.StartMode; DelayedAutoStart=[bool](Get-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$($_.Name)" -Name DelayedAutostart -ErrorAction SilentlyContinue).DelayedAutostart } })
    tarefas = @($tasks | ForEach-Object { [pscustomobject]@{ TaskPath=$_.TaskPath; TaskName=$_.TaskName } })
    inicializacao = @($runItems | ForEach-Object { $b=$null; if($apAtual){$b=$apAtual.$_}; [pscustomobject]@{ Name=$_; Approved=$(if($null -ne $b){[Convert]::ToBase64String([byte[]]$b)}else{$null}) } })
    amd = @($amdAuto | ForEach-Object { [pscustomobject]@{ DisableAutoUpdates=$_.DisableAutoUpdates } })
    drivers = @($drivers)
}
$snap | ConvertTo-Json -Depth 4 -Compress
"#;
        let out = run_powershell_checked(query_script)?;
        let val: serde_json::Value =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        let count = val["servicos"].as_array().map(|a| a.len()).unwrap_or(0)
            + val["tarefas"].as_array().map(|a| a.len()).unwrap_or(0)
            + val["inicializacao"].as_array().map(|a| a.len()).unwrap_or(0)
            + val["drivers"].as_array().map(|a| a.len()).unwrap_or(0)
            + val["amd"].as_array().map(|a| a.len()).unwrap_or(0);
        if count == 0 {
            return Ok("0 item(ns) de terceiros pendentes.".into());
        }
        let created = backups.save_if_absent(self.meta.id, val.clone())?;
        let escaped = serde_json::to_string(&val)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply_script = format!(
            r#"
$snap = ConvertFrom-Json '{escaped}'
$amdKey = 'HKLM:\SOFTWARE\AMD\AMDInstallManager\AutoUpdate'
$apKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
foreach ($d in @($snap.drivers)) {{
    if (-not $d.Name) {{ continue }}
    Set-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$($d.Name)" -Name Start -Value 4 -Type DWord -ErrorAction Stop
}}
if (@($snap.amd).Count) {{
    Set-ItemProperty -LiteralPath $amdKey -Name DisableAutoUpdates -Value 1 -Type DWord -ErrorAction Stop
}}
foreach ($s in @($snap.servicos)) {{
    if (-not $s.Name) {{ continue }}
    Set-Service -Name $s.Name -StartupType Disabled -ErrorAction Stop
    Stop-Service -Name $s.Name -Force -ErrorAction SilentlyContinue
}}
foreach ($t in @($snap.tarefas)) {{
    if (-not $t.TaskName) {{ continue }}
    Stop-ScheduledTask -TaskPath $t.TaskPath -TaskName $t.TaskName -ErrorAction SilentlyContinue
    Disable-ScheduledTask -TaskPath $t.TaskPath -TaskName $t.TaskName -ErrorAction Stop | Out-Null
    Get-Process cpumetricsserver -ErrorAction SilentlyContinue | Stop-Process -Force
    if ($t.TaskName -eq 'StartCN') {{ Get-Process RadeonSoftware -ErrorAction SilentlyContinue | Stop-Process -Force }}
}}
foreach ($i in @($snap.inicializacao)) {{
    if (-not $i.Name) {{ continue }}
    if (-not (Test-Path -LiteralPath $apKey)) {{ New-Item -Path $apKey -Force | Out-Null }}
    New-ItemProperty -LiteralPath $apKey -Name $i.Name -Value ([byte[]](3,0,0,0,0,0,0,0,0,0,0,0)) -PropertyType Binary -Force | Out-Null
}}
Get-Process AMDInstallManager -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
"#
        );
        if let Err(e) = run_powershell_checked(&apply_script) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok(format!("{count} item(ns) de terceiros desativado(s)."))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de terceiros encontrado.".into());
        };
        let escaped = serde_json::to_string(&rec.data)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$snap = ConvertFrom-Json '{escaped}'
$map = @{{ Auto='Automatic'; Manual='Manual'; Disabled='Disabled' }}
foreach ($r in @($snap.servicos)) {{
    if (-not $r.Name) {{ continue }}
    Set-Service -Name $r.Name -StartupType $map[[string]$r.StartMode] -ErrorAction Stop
    if ($r.DelayedAutoStart) {{ & sc.exe config "$($r.Name)" start= delayed-auto | Out-Null }}
    if ($r.StartMode -eq 'Auto') {{ Start-Service -Name $r.Name -ErrorAction SilentlyContinue }}
}}
foreach ($t in @($snap.tarefas)) {{
    if (-not $t.TaskName) {{ continue }}
    Enable-ScheduledTask -TaskPath $t.TaskPath -TaskName $t.TaskName -ErrorAction Stop | Out-Null
}}
$apKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run'
foreach ($i in @($snap.inicializacao)) {{
    if (-not $i.Name) {{ continue }}
    if ($i.Approved) {{ New-ItemProperty -LiteralPath $apKey -Name $i.Name -Value ([Convert]::FromBase64String($i.Approved)) -PropertyType Binary -Force | Out-Null }}
    else {{ Remove-ItemProperty -LiteralPath $apKey -Name $i.Name -ErrorAction SilentlyContinue }}
}}
foreach ($d in @($snap.drivers)) {{
    if (-not $d.Name) {{ continue }}
    Set-ItemProperty -LiteralPath "HKLM:\SYSTEM\CurrentControlSet\Services\$($d.Name)" -Name Start -Value ([int]$d.Start) -Type DWord
}}
foreach ($a in @($snap.amd)) {{
    $k = 'HKLM:\SOFTWARE\AMD\AMDInstallManager\AutoUpdate'
    if ($null -ne $a.DisableAutoUpdates) {{ Set-ItemProperty -LiteralPath $k -Name DisableAutoUpdates -Value ([int]$a.DisableAutoUpdates) -Type DWord }}
    else {{ Remove-ItemProperty -LiteralPath $k -Name DisableAutoUpdates -ErrorAction SilentlyContinue }}
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok("Itens de terceiros restaurados.".into())
    }
}

const SSO_ACTIONCENTER_SUBKEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\ShellServiceObjects\{F56F6FDD-AA9D-4618-A949-C1B91AF43B1A}";
const SSO_ACTIONCENTER_REG: &str =
    r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\ShellServiceObjects\{F56F6FDD-AA9D-4618-A949-C1B91AF43B1A}";

pub struct ActionCenterSsoTweak {
    meta: TweakMeta,
}

impl ActionCenterSsoTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "bloat.actioncenter_sso",
                name: "Explorer: Remover SSO de Segurança e Manutenção (ActionCenter)",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Exporta e remove o ShellServiceObject do ActionCenter (~1,8 MB no Explorer que só gera avisos de manutenção).",
                details: vec![format!("Chave: {SSO_ACTIONCENTER_REG}")],
            },
        }
    }
}

impl Tweak for ActionCenterSsoTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if key_exists(RegHive::Hklm, SSO_ACTIONCENTER_SUBKEY) {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Applied
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        if !key_exists(RegHive::Hklm, SSO_ACTIONCENTER_SUBKEY) {
            return Ok("Chave SSO ActionCenter já não está presente.".into());
        }
        let reg_file = backups.extra_file_path(self.meta.id, "reg");
        if let Some(parent) = reg_file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let reg_str = reg_file.to_string_lossy().to_string();
        if !reg_file.exists() {
            crate::sys::cmd::run_hidden_checked(
                "reg.exe",
                &["export", SSO_ACTIONCENTER_REG, &reg_str, "/y"],
                &[0],
            )?;
        }
        let json = serde_json::json!({ "reg_file": reg_str });
        let created = backups.save_if_absent(self.meta.id, json)?;

        if let Err(e) = crate::sys::cmd::run_hidden_checked(
            "reg.exe",
            &["delete", SSO_ACTIONCENTER_REG, "/f"],
            &[0],
        ) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("SSO ActionCenter removido do Explorer.".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup do SSO ActionCenter encontrado.".into());
        };
        let reg_str = rec.data["reg_file"]
            .as_str()
            .ok_or("Backup do SSO ActionCenter sem caminho .reg.")?;
        crate::sys::cmd::run_hidden_checked("reg.exe", &["import", reg_str], &[0])?;
        let p = std::path::Path::new(reg_str);
        if p.exists() {
            let _ = std::fs::remove_file(p);
        }
        backups.remove(self.meta.id)?;
        Ok("SSO ActionCenter restaurado via .reg.".into())
    }
}

const OPTIONAL_FEATURES: &[&str] = &[
    "Recall",
    "WorkFolders-Client",
    "SmbDirect",
    "Printing-PrintToPDFServices-Features",
    "Printing-XPSServices-Features",
    "Printing-Foundation-InternetPrinting-Client",
    "Printing-Foundation-Features",
    "SMB1Protocol",
    "MicrosoftWindowsPowerShellV2Root",
    "MSRDC-Infrastructure",
    "WCF-TCP-PortSharing45",
    "WCF-Services45",
    "Microsoft-RemoteDesktopConnection",
    "Microsoft-Windows-Camera-CCX-Deployment",
    "SearchEngine-Client-Package",
];

pub struct OptionalFeaturesTweak {
    meta: TweakMeta,
}

impl OptionalFeaturesTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "bloat.optional_features_off",
                name: "Recursos Opcionais do Windows (Recall, XPS/PDF Print, WorkFolders, SMB1, WCF, SearchEngine)",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Medium,
                requires_reboot: true,
                requires_admin: true,
                description: "Desativa recursos opcionais do Windows sem utilidade para jogos (XPS, Fax, WorkFolders, PowerShell v2, etc.), salvando os que estavam ativos para reabilitação.",
                details: OPTIONAL_FEATURES
                    .iter()
                    .map(|f| format!("OptionalFeature: {f}"))
                    .collect(),
            },
        }
    }

    fn ps_list() -> String {
        OPTIONAL_FEATURES
            .iter()
            .map(|f| format!("'{f}'"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Tweak for OptionalFeaturesTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        let list = Self::ps_list();
        let script = format!(
            r#"
$alvo = @({list})
$all = @(Get-WindowsOptionalFeature -Online -ErrorAction Stop | Where-Object {{ $alvo -contains $_.FeatureName }})
if (-not $all.Count) {{ Write-Output 'NONE'; exit 0 }}
$off = @($all | Where-Object {{ "$($_.State)" -notmatch 'Enabled' }}).Count
Write-Output "$off/$($all.Count)"
"#
        );
        match run_powershell_checked(&script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NONE" {
                    TweakStatus::NotApplicable("Nenhum recurso alvo presente".into())
                } else if let Some((a, b)) = s.split_once('/') {
                    let off: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(0);
                    if off == total {
                        TweakStatus::Applied
                    } else if off == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{off}/{total} desativados"))
                    }
                } else {
                    TweakStatus::Error(s.to_string())
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let list = Self::ps_list();
        let query = format!(
            r#"
$alvo = @({list})
$on = @(Get-WindowsOptionalFeature -Online -ErrorAction Stop | Where-Object {{ $alvo -contains $_.FeatureName -and "$($_.State)" -match 'Enabled' }} | ForEach-Object {{ $_.FeatureName }})
if ($on.Count) {{ ConvertTo-Json -InputObject @($on) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let enabled: Vec<String> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if enabled.is_empty() {
            return Ok("Todos os recursos opcionais alvo já estão desativados.".into());
        }
        let json = serde_json::to_value(&enabled).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&enabled)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply = format!(
            r#"
$feats = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
foreach ($f in $feats) {{
    Disable-WindowsOptionalFeature -Online -FeatureName $f -NoRestart -ErrorAction Stop | Out-Null
}}
"#
        );
        match run_powershell_checked(&apply) {
            Ok(_) => Ok(format!("{} recurso(s) opcional(is) desativado(s).", enabled.len())),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de recursos opcionais encontrado.".into());
        };
        let names: Vec<String> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        let escaped = serde_json::to_string(&names)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$feats = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
foreach ($f in $feats) {{
    Enable-WindowsOptionalFeature -Online -FeatureName $f -NoRestart -ErrorAction Stop | Out-Null
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("{} recurso(s) opcional(is) reativado(s).", names.len()))
    }
}

pub struct UsbDevicePowerMgmtTweak {
    meta: TweakMeta,
}

impl UsbDevicePowerMgmtTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "power.usb_device_pm_off",
                name: "USB: Desativar EnhancedPowerManagementEnabled nos dispositivos",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Desativa EnhancedPowerManagementEnabled=0 em Enum\\USB\\*\\*\\Device Parameters onde presente, evitando micro-atrasos ao acordar mouses 1000 Hz+ e headsets USB.",
                details: vec![
                    r"Escaneia HKLM\SYSTEM\CurrentControlSet\Enum\USB\*\*\Device Parameters".into(),
                    "Define EnhancedPowerManagementEnabled = 0 onde a chave já existe".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for dev in list_subkeys(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Enum\USB") {
            let dev_path = format!(r"SYSTEM\CurrentControlSet\Enum\USB\{dev}");
            for inst in list_subkeys(RegHive::Hklm, &dev_path) {
                let dp = format!(r"{dev_path}\{inst}\Device Parameters");
                if matches!(
                    read_value(RegHive::Hklm, &dp, "EnhancedPowerManagementEnabled"),
                    Ok(Some(RegValue::DWord(_)))
                ) {
                    out.push(dp);
                }
            }
        }
        out
    }
}

impl Tweak for UsbDevicePowerMgmtTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum dispositivo USB com EnhancedPowerManagementEnabled".into());
        }
        let off = targets
            .iter()
            .filter(|sk| matches!(read_value(RegHive::Hklm, sk, "EnhancedPowerManagementEnabled"), Ok(Some(RegValue::DWord(0)))))
            .count();
        if off == targets.len() {
            TweakStatus::Applied
        } else if off == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{off}/{} dispositivos", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum dispositivo USB alvo encontrado.".into());
        }
        let mut snap = Vec::new();
        for sk in &targets {
            let cur = read_value(RegHive::Hklm, sk, "EnhancedPowerManagementEnabled")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: "EnhancedPowerManagementEnabled".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, sk) in targets.iter().enumerate() {
            if let Err(e) = write_value_backup_restore(
                RegHive::Hklm,
                sk,
                "EnhancedPowerManagementEnabled",
                &RegValue::DWord(0),
            ) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!("{} dispositivo(s) USB ajustado(s).", targets.len()))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup USB encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("{} dispositivo(s) USB restaurado(s).", snap.len()))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let targets = Self::scan_targets();
        for sk in &targets {
            write_value_backup_restore(
                RegHive::Hklm,
                sk,
                "EnhancedPowerManagementEnabled",
                &RegValue::DWord(1),
            )?;
        }
        Ok(format!("{} dispositivo(s) USB definidos para padrão (1).", targets.len()))
    }
}

const MSI_CLASSES: &[&str] = &[
    "{4d36e968-e325-11ce-bfc1-08002be10318}",
    "{4d36e972-e325-11ce-bfc1-08002be10318}",
];

pub struct GpuNicMsiModeTweak {
    meta: TweakMeta,
}

impl GpuNicMsiModeTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.msi_mode",
                name: "GPU e Rede: Ativar MSI Mode (Message Signaled Interrupts)",
                category: Category::GpuLatency,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Garante MSISupported=1 em dispositivos PCI de GPU e placa de rede (reduz latência DPC sem tocar em afinidade/prioridade fixa de IRQ).",
                details: vec![
                    r"Escaneia Enum\PCI\*\* para classes Display e Net".into(),
                    "Define MessageSignaledInterruptProperties\\MSISupported = 1".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for vendor in list_subkeys(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Enum\PCI") {
            let v_path = format!(r"SYSTEM\CurrentControlSet\Enum\PCI\{vendor}");
            for device in list_subkeys(RegHive::Hklm, &v_path) {
                let dev_path = format!(r"{v_path}\{device}");
                let class_guid = match read_value(RegHive::Hklm, &dev_path, "ClassGUID") {
                    Ok(Some(RegValue::String(s))) => s.to_ascii_lowercase(),
                    _ => continue,
                };
                if !MSI_CLASSES.contains(&class_guid.as_str()) {
                    continue;
                }
                let msi_path = format!(
                    r"{dev_path}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
                );
                if key_exists(RegHive::Hklm, &msi_path) {
                    out.push(msi_path);
                }
            }
        }
        out
    }
}

impl Tweak for GpuNicMsiModeTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum dispositivo GPU/Rede com chave MSI".into());
        }
        let ok = targets
            .iter()
            .filter(|sk| matches!(read_value(RegHive::Hklm, sk, "MSISupported"), Ok(Some(RegValue::DWord(1)))))
            .count();
        if ok == targets.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} dispositivos", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum dispositivo GPU/Rede compatível encontrado.".into());
        }
        let mut snap = Vec::new();
        for sk in &targets {
            let cur = read_value(RegHive::Hklm, sk, "MSISupported")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: "MSISupported".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, sk) in targets.iter().enumerate() {
            if let Err(e) =
                write_value_backup_restore(RegHive::Hklm, sk, "MSISupported", &RegValue::DWord(1))
            {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!("MSI Mode garantido em {} dispositivo(s) GPU/Rede.", targets.len()))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de MSI Mode encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value_backup_restore(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("MSI Mode restaurado em {} dispositivo(s).", snap.len()))
    }
}

pub struct NvidiaTelemetryTweak {
    meta: TweakMeta,
}

impl NvidiaTelemetryTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.nvidia_telemetry_off",
                name: "GPU NVIDIA: Desativar telemetria do driver (preserva NVIDIA App e FPS)",
                category: Category::GpuLatency,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa NvTelemetryContainer (se presente), tarefas NvTmRep_*/NvTmMon_*/NvProfileUpdater* e OptInOrOutPreference=0. Mantém NVDisplay.ContainerLocalSystem e NvBackend intactos.",
                details: vec![
                    "Serviço NvTelemetryContainer -> Disabled (se instalado)".into(),
                    "Tarefas agendadas \\NvTmRep_*, \\NvTmMon_*, \\NvProfileUpdater* -> Disabled".into(),
                    r"HKLM\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client\OptInOrOutPreference = 0".into(),
                ],
            },
        }
    }
}

impl Tweak for NvidiaTelemetryTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let script = r#"
$hasNv = Test-Path 'HKLM:\SOFTWARE\NVIDIA Corporation'
$svc = Get-ItemProperty 'HKLM:\SYSTEM\CurrentControlSet\Services\NvTelemetryContainer' -Name Start -ErrorAction SilentlyContinue
$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { $_.TaskName -like 'NvTmRep_*' -or $_.TaskName -like 'NvTmMon_*' -or $_.TaskName -like 'NvProfileUpdater*' })
if (-not $hasNv -and -not $svc -and -not $tasks.Count) { Write-Output 'NA'; exit 0 }
$pend = 0; $total = 0
if ($svc) { $total++; if ($svc.Start -ne 4) { $pend++ } }
foreach ($t in $tasks) { $total++; if ($t.State -ne 'Disabled') { $pend++ } }
if ($hasNv) {
    $total++
    $opt = (Get-ItemProperty 'HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client' -Name OptInOrOutPreference -ErrorAction SilentlyContinue).OptInOrOutPreference
    if ($opt -ne 0) { $pend++ }
}
Write-Output "$pend/$total"
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NA" {
                    TweakStatus::NotApplicable("Driver/telemetria NVIDIA não encontrados".into())
                } else if let Some((p, t)) = s.split_once('/') {
                    let pend: usize = p.parse().unwrap_or(0);
                    let total: usize = t.parse().unwrap_or(0);
                    if total == 0 {
                        TweakStatus::NotApplicable("Sem itens de telemetria NVIDIA".into())
                    } else if pend == 0 {
                        TweakStatus::Applied
                    } else if pend == total {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{}/{} ajustados", total - pend, total))
                    }
                } else {
                    TweakStatus::Error(s.to_string())
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query_script = r#"
$svcKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\NvTelemetryContainer'
$svcStart = if (Test-Path $svcKey) { (Get-ItemProperty $svcKey -Name Start -ErrorAction SilentlyContinue).Start } else { $null }
$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue | Where-Object { ($_.TaskName -like 'NvTmRep_*' -or $_.TaskName -like 'NvTmMon_*' -or $_.TaskName -like 'NvProfileUpdater*') -and $_.State -ne 'Disabled' })
$regKey = 'HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client'
$hasNv = Test-Path 'HKLM:\SOFTWARE\NVIDIA Corporation'
$optBefore = if (Test-Path $regKey) { (Get-ItemProperty $regKey -Name OptInOrOutPreference -ErrorAction SilentlyContinue).OptInOrOutPreference } else { $null }

$snap = [ordered]@{
    svc_start = $svcStart
    tasks = @($tasks | ForEach-Object { [pscustomobject]@{ TaskPath=$_.TaskPath; TaskName=$_.TaskName } })
    has_nv = $hasNv
    opt_before = $optBefore
}
$snap | ConvertTo-Json -Depth 3 -Compress
"#;
        let out = run_powershell_checked(query_script)?;
        let val: serde_json::Value =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, val.clone())?;

        let escaped = serde_json::to_string(&val)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply_script = format!(
            r#"
$snap = ConvertFrom-Json '{escaped}'
$svcKey = 'HKLM:\SYSTEM\CurrentControlSet\Services\NvTelemetryContainer'
if ($null -ne $snap.svc_start) {{
    Set-Service -Name NvTelemetryContainer -StartupType Disabled -ErrorAction SilentlyContinue
    Stop-Service -Name NvTelemetryContainer -Force -ErrorAction SilentlyContinue
    Set-ItemProperty -LiteralPath $svcKey -Name Start -Value 4 -Type DWord -ErrorAction Stop
}}
foreach ($t in @($snap.tasks)) {{
    if (-not $t.TaskName) {{ continue }}
    try {{ Disable-ScheduledTask -TaskPath $t.TaskPath -TaskName $t.TaskName -ErrorAction Stop | Out-Null }} catch {{}}
}}
$regKey = 'HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client'
if ($snap.has_nv) {{
    if (-not (Test-Path -LiteralPath $regKey)) {{ New-Item -Path $regKey -Force | Out-Null }}
    New-ItemProperty -LiteralPath $regKey -Name OptInOrOutPreference -Value 0 -PropertyType DWord -Force | Out-Null
}}
"#
        );
        if let Err(e) = run_powershell_checked(&apply_script) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("Telemetria NVIDIA desativada.".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de telemetria NVIDIA encontrado.".into());
        };
        let escaped = serde_json::to_string(&rec.data)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$snap = ConvertFrom-Json '{escaped}'
if ($null -ne $snap.svc_start) {{
    Set-ItemProperty -LiteralPath 'HKLM:\SYSTEM\CurrentControlSet\Services\NvTelemetryContainer' -Name Start -Value ([int]$snap.svc_start) -Type DWord -ErrorAction SilentlyContinue
}}
foreach ($t in @($snap.tasks)) {{
    if (-not $t.TaskName) {{ continue }}
    Enable-ScheduledTask -TaskPath $t.TaskPath -TaskName $t.TaskName -ErrorAction SilentlyContinue | Out-Null
}}
$regKey = 'HKLM:\SOFTWARE\NVIDIA Corporation\NvControlPanel2\Client'
if ($snap.has_nv -and (Test-Path -LiteralPath $regKey)) {{
    if ($null -ne $snap.opt_before) {{
        New-ItemProperty -LiteralPath $regKey -Name OptInOrOutPreference -Value ([int]$snap.opt_before) -PropertyType DWord -Force | Out-Null
    }} else {{
        Remove-ItemProperty -LiteralPath $regKey -Name OptInOrOutPreference -ErrorAction SilentlyContinue
    }}
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok("Telemetria NVIDIA restaurada.".into())
    }
}

pub struct NvidiaDynamicPstateTweak {
    meta: TweakMeta,
}

impl NvidiaDynamicPstateTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.nvidia_p0_state",
                name: "GPU NVIDIA: Travar P0 State (DisableDynamicPstate=1)",
                category: Category::GpuLatency,
                risk: RiskLevel::Medium,
                requires_reboot: true,
                requires_admin: true,
                description: "Impede que a GPU NVIDIA reduza clocks dinamicamente durante jogos leves/CPU-bound (reduz stutters de rampa de clock, mas aumenta consumo e temperatura em idle).",
                details: vec![
                    format!(r"HKLM\{GPU_CLASS_KEY}\00xx\DisableDynamicPstate = 1 (VEN_10DE)"),
                ],
            },
        }
    }

    fn nvidia_keys() -> Vec<String> {
        let mut res = Vec::new();
        for sub in list_subkeys(RegHive::Hklm, GPU_CLASS_KEY) {
            if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let dev_key = format!(r"{GPU_CLASS_KEY}\{sub}");
            if let Ok(Some(RegValue::String(id))) =
                read_value(RegHive::Hklm, &dev_key, "MatchingDeviceId")
            {
                if id.to_ascii_uppercase().contains("VEN_10DE") {
                    res.push(dev_key);
                }
            }
        }
        res
    }
}

impl Tweak for NvidiaDynamicPstateTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let keys = Self::nvidia_keys();
        if keys.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma GPU NVIDIA (VEN_10DE) detectada".into());
        }
        let ok = keys
            .iter()
            .filter(|k| matches!(read_value(RegHive::Hklm, k, "DisableDynamicPstate"), Ok(Some(RegValue::DWord(1)))))
            .count();
        if ok == keys.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} GPUs NVIDIA", keys.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let keys = Self::nvidia_keys();
        if keys.is_empty() {
            return Err("Nenhuma GPU NVIDIA instalada neste PC.".into());
        }
        let mut snap = Vec::new();
        for k in &keys {
            let cur = read_value(RegHive::Hklm, k, "DisableDynamicPstate")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: k.clone(),
                name: "DisableDynamicPstate".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: false,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, k) in keys.iter().enumerate() {
            if let Err(e) = write_value(
                RegHive::Hklm,
                k,
                "DisableDynamicPstate",
                &RegValue::DWord(1),
            ) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "DisableDynamicPstate=1 aplicado em {} GPU(s) NVIDIA (reinicie para valer).",
            keys.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de DisableDynamicPstate encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("DisableDynamicPstate restaurado ao padrão.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        for k in Self::nvidia_keys() {
            delete_value(RegHive::Hklm, &k, "DisableDynamicPstate")?;
        }
        Ok("DisableDynamicPstate removido (padrão do driver NVIDIA).".into())
    }
}

const TCPIP_IFACES_KEY: &str = r"SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces";

pub struct TcpNagleTweak {
    meta: TweakMeta,
}

impl TcpNagleTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.tcp_nagle_off",
                name: "Rede: Desativar algoritmo de Nagle (TcpAckFrequency=1, TCPNoDelay=1, TcpDelAckTicks=0)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Envia pacotes TCP pequenos imediatamente sem aguardar agrupamento/ACK atrasado nas interfaces ativas (útil para MMOs/jogos TCP; jogos em UDP não são afetados).",
                details: vec![
                    format!(r"HKLM\{TCPIP_IFACES_KEY}\{{GUID}}\TcpAckFrequency = 1"),
                    format!(r"HKLM\{TCPIP_IFACES_KEY}\{{GUID}}\TCPNoDelay = 1"),
                    format!(r"HKLM\{TCPIP_IFACES_KEY}\{{GUID}}\TcpDelAckTicks = 0"),
                ],
            },
        }
    }

    fn has_valid_ip(subkey: &str) -> bool {
        for name in ["DhcpIPAddress", "IPAddress"] {
            match read_value(RegHive::Hklm, subkey, name) {
                Ok(Some(RegValue::String(s))) if !s.is_empty() && s != "0.0.0.0" => return true,
                Ok(Some(RegValue::MultiString(v)))
                    if v.iter().any(|s| !s.is_empty() && s != "0.0.0.0") =>
                {
                    return true
                }
                _ => {}
            }
        }
        false
    }

    fn active_interfaces() -> Vec<String> {
        list_subkeys(RegHive::Hklm, TCPIP_IFACES_KEY)
            .into_iter()
            .map(|guid| format!(r"{TCPIP_IFACES_KEY}\{guid}"))
            .filter(|sk| Self::has_valid_ip(sk))
            .collect()
    }
}

impl Tweak for TcpNagleTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let ifaces = Self::active_interfaces();
        if ifaces.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma interface TCP/IP ativa encontrada".into());
        }
        let total = ifaces.len() * 3;
        let mut ok = 0usize;
        for sk in &ifaces {
            for val in ["TcpAckFrequency", "TCPNoDelay"] {
                if matches!(read_value(RegHive::Hklm, sk, val), Ok(Some(RegValue::DWord(1)))) {
                    ok += 1;
                }
            }
            if matches!(read_value(RegHive::Hklm, sk, "TcpDelAckTicks"), Ok(Some(RegValue::DWord(0)))) {
                ok += 1;
            }
        }
        if ok == total {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{total} valores"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let ifaces = Self::active_interfaces();
        if ifaces.is_empty() {
            return Ok("Nenhuma interface TCP/IP ativa encontrada.".into());
        }
        let mut snap = Vec::new();
        for sk in &ifaces {
            for val_name in ["TcpAckFrequency", "TCPNoDelay", "TcpDelAckTicks"] {
                let cur = read_value(RegHive::Hklm, sk, val_name)?;
                snap.push(RegSnapshotEntry {
                    hive: RegHive::Hklm,
                    subkey: sk.clone(),
                    name: val_name.into(),
                    existed: cur.is_some(),
                    value: cur,
                    use_backup_restore: false,
                });
            }
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let mut modified = 0usize;
        for sk in &ifaces {
            for (val_name, target_dw) in [
                ("TcpAckFrequency", 1),
                ("TCPNoDelay", 1),
                ("TcpDelAckTicks", 0),
            ] {
                if let Err(e) =
                    write_value(RegHive::Hklm, sk, val_name, &RegValue::DWord(target_dw))
                {
                    if created && modified == 0 {
                        let _ = backups.remove(self.meta.id);
                    }
                    return Err(e);
                }
                modified += 1;
            }
        }
        Ok(format!(
            "Nagle desativado em {} interface(s) TCP/IP ativa(s).",
            ifaces.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de Nagle/TCP encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("Configurações TCP/Nagle restauradas.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        for sk in Self::active_interfaces() {
            delete_value(RegHive::Hklm, &sk, "TcpAckFrequency")?;
            delete_value(RegHive::Hklm, &sk, "TCPNoDelay")?;
            delete_value(RegHive::Hklm, &sk, "TcpDelAckTicks")?;
        }
        Ok("TcpAckFrequency, TCPNoDelay e TcpDelAckTicks removidos (padrão do Windows).".into())
    }
}

const CONSUMER_APPX_LIST: &[&str] = &[
    "Microsoft.Copilot",
    "MicrosoftWindows.Client.WebExperience",
    "Clipchamp.Clipchamp",
    "Microsoft.BingNews",
    "Microsoft.BingWeather",
    "Microsoft.BingSearch",
    "Microsoft.OutlookForWindows",
    "MSTeams",
    "Microsoft.YourPhone",
    "Microsoft.WindowsFeedbackHub",
    "Microsoft.GetHelp",
    "Microsoft.Getstarted",
    "Microsoft.MicrosoftSolitaireCollection",
    "Microsoft.PowerAutomateDesktop",
    "MicrosoftCorporationII.QuickAssist",
    "Microsoft.Windows.DevHome",
    "Microsoft.MicrosoftOfficeHub",
    "Microsoft.Todos",
    "Microsoft.People",
    "Microsoft.549981C3F5F10",
    "Microsoft.WindowsMaps",
    "MicrosoftWindows.CrossDevice",
    "Microsoft.WidgetsPlatformRuntime",
    "Microsoft.XboxSpeechToTextOverlay",
    "MicrosoftCorporationII.MicrosoftFamily",
    "Microsoft.BingFinance",
    "Microsoft.BingSports",
    "Microsoft.Windows.Ai.Copilot.Provider",
];

#[derive(Debug, Serialize, Deserialize)]
struct ConsumerAppxEntry {
    name: String,
    full: String,
    local: String,
}

pub struct ConsumerAppxTweak {
    meta: TweakMeta,
}

impl ConsumerAppxTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "bloat.consumer_appx",
                name: "Remover apps bloatware de consumo (Win11 24H2/25H2/26H1 Home/Pro)",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Remove Copilot/Copilot Provider, Widgets (WebExperience), Clipchamp, Bing News/Weather/Search/Finance/Sports, Outlook Novo, Teams, Phone Link, Family Safety, GetHelp, Solitaire, QuickAssist, DevHome e afins. Preserva Microsoft Store, Xbox/GamingServices, Captura de Tela, Calculadora, Paint, Fotos, Bloco de Notas e Terminal. Reversível (ou reinstalável pela Store).",
                details: CONSUMER_APPX_LIST
                    .iter()
                    .map(|n| format!("AppX consumo: {n}"))
                    .collect(),
            },
        }
    }

    fn ps_names() -> String {
        CONSUMER_APPX_LIST
            .iter()
            .map(|n| format!("'{n}'"))
            .collect::<Vec<_>>()
            .join(",")
    }
}

impl Tweak for ConsumerAppxTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let names = Self::ps_names();
        let all_users = crate::sys::elevation::is_elevated();
        let script = if all_users {
            format!(
                r#"
$nomes = @({names})
$inst = 0
foreach ($n in $nomes) {{
    if (Get-AppxPackage -AllUsers -Name $n -ErrorAction SilentlyContinue) {{ $inst++ }}
}}
Write-Output $inst
"#
            )
        } else {
            format!(
                r#"
$nomes = @({names})
$inst = 0
foreach ($n in $nomes) {{
    if (Get-AppxPackage -Name $n -ErrorAction SilentlyContinue) {{ $inst++ }}
}}
Write-Output $inst
"#
            )
        };
        match run_powershell_checked(&script) {
            Ok(out) => {
                let remaining: usize = out.stdout.trim().parse().unwrap_or(0);
                if remaining == 0 {
                    TweakStatus::Applied
                } else {
                    TweakStatus::Partial(format!("{remaining} app(s) instalado(s)"))
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let names = Self::ps_names();
        let query = format!(
            r#"
$alvo = @(foreach ($n in @({names})) {{ Get-AppxPackage -AllUsers -Name $n -ErrorAction SilentlyContinue }})
$rows = foreach ($p in $alvo) {{
    [pscustomobject]@{{ name=$p.Name; full=$p.PackageFullName; local=$p.InstallLocation }}
}}
if ($rows) {{ ConvertTo-Json -InputObject @($rows) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let rows: Vec<ConsumerAppxEntry> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok(format!("Nenhum dos {} apps bloatware de consumo está instalado.", CONSUMER_APPX_LIST.len()));
        }
        let json = serde_json::to_value(&rows).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply_script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
$ok = 0
foreach ($p in $rows) {{
    try {{
        Remove-AppxPackage -Package $p.full -AllUsers -ErrorAction SilentlyContinue
        Get-AppxProvisionedPackage -Online -ErrorAction SilentlyContinue |
            Where-Object {{ $_.DisplayName -eq $p.name }} |
            ForEach-Object {{ Remove-AppxProvisionedPackage -Online -PackageName $_.PackageName -ErrorAction SilentlyContinue | Out-Null }}
        $ok++
    }} catch {{}}
}}
Write-Output $ok
"#
        );
        match run_powershell_checked(&apply_script) {
            Ok(r) => Ok(format!("{} app(s) de consumo removido(s).", r.stdout.trim())),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de apps de consumo encontrado.".into());
        };
        let rows: Vec<ConsumerAppxEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
$ok = 0
foreach ($a in $rows) {{
    if (-not $a.local) {{ continue }}
    $manifest = Join-Path $a.local 'AppxManifest.xml'
    if (Test-Path -LiteralPath $manifest) {{
        try {{
            Add-AppxPackage -Register $manifest -DisableDevelopmentMode -ErrorAction SilentlyContinue
            $ok++
        }} catch {{}}
    }}
}}
Write-Output $ok
"#
        );
        let out = run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!(
            "{} app(s) re-registrado(s) (demais podem ser baixados na Microsoft Store se desejar).",
            out.stdout.trim()
        ))
    }
}

pub struct UsbHubPowerMgmtTweak {
    meta: TweakMeta,
}

impl UsbHubPowerMgmtTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "power.usb_hub_pm_off",
                name: "USB: Desativar economia de energia nos Hubs Raiz (WDF IdleInWorkingState)",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Define IdleInWorkingState=0 em Enum\\USB\\ROOT_HUB*\\*\\Device Parameters\\WDF (equivalente a desmarcar 'O computador pode desligar o dispositivo para economizar energia' nos Hubs Raiz USB), evitando micro-stutters em mouses 1000–8000 Hz e DACs USB.",
                details: vec![
                    r"Escaneia HKLM\SYSTEM\CurrentControlSet\Enum\USB\ROOT_HUB*\*\Device Parameters\WDF".into(),
                    "Define IdleInWorkingState = 0 onde a chave já existe".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for hub_class in ["ROOT_HUB30", "ROOT_HUB20", "ROOT_HUB"] {
            let base = format!(r"SYSTEM\CurrentControlSet\Enum\USB\{hub_class}");
            for inst in list_subkeys(RegHive::Hklm, &base) {
                let wdf = format!(r"{base}\{inst}\Device Parameters\WDF");
                if matches!(
                    read_value(RegHive::Hklm, &wdf, "IdleInWorkingState"),
                    Ok(Some(RegValue::DWord(_)))
                ) {
                    out.push(wdf);
                }
            }
        }
        out
    }
}

impl Tweak for UsbHubPowerMgmtTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum Hub Raiz USB com IdleInWorkingState".into());
        }
        let off = targets
            .iter()
            .filter(|sk| {
                matches!(
                    read_value(RegHive::Hklm, sk, "IdleInWorkingState"),
                    Ok(Some(RegValue::DWord(0)))
                )
            })
            .count();
        if off == targets.len() {
            TweakStatus::Applied
        } else if off == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{off}/{} hubs", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum Hub Raiz USB encontrado.".into());
        }
        let mut snap = Vec::new();
        for sk in &targets {
            let cur = read_value(RegHive::Hklm, sk, "IdleInWorkingState")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: "IdleInWorkingState".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, sk) in targets.iter().enumerate() {
            if let Err(e) = write_value_backup_restore(
                RegHive::Hklm,
                sk,
                "IdleInWorkingState",
                &RegValue::DWord(0),
            ) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "Economia de energia desativada em {} Hub(s) Raiz USB.",
            targets.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de Hubs USB encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("{} Hub(s) Raiz USB restaurado(s).", snap.len()))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let targets = Self::scan_targets();
        for sk in &targets {
            write_value_backup_restore(
                RegHive::Hklm,
                sk,
                "IdleInWorkingState",
                &RegValue::DWord(1),
            )?;
        }
        Ok(format!(
            "{} Hub(s) Raiz USB definidos para padrão (1).",
            targets.len()
        ))
    }
}

const USB_AUDIO_MSI_CLASSES: &[&str] = &[
    "{4d36e96c-e325-11ce-bfc1-08002be10318}",
    "{36fc9e60-c465-11cf-8056-444553540000}",
];

pub struct UsbAudioMsiModeTweak {
    meta: TweakMeta,
}

impl UsbAudioMsiModeTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.msi_mode_usb_audio",
                name: "USB e Áudio: Ativar MSI Mode em controladores xHCI e HD Audio",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Ativa MSISupported=1 em controladores USB xHCI e dispositivos de áudio PCI que já expõem suporte MSI, reduzindo latência DPC e conflitos de IRQ compartilhada (sem fixar afinidade de IRQ).",
                details: vec![
                    r"Escaneia Enum\PCI\*\* para classes MEDIA (Áudio) e USB (xHCI)".into(),
                    "Define MessageSignaledInterruptProperties\\MSISupported = 1".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for vendor in list_subkeys(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Enum\PCI") {
            let v_path = format!(r"SYSTEM\CurrentControlSet\Enum\PCI\{vendor}");
            for device in list_subkeys(RegHive::Hklm, &v_path) {
                let dev_path = format!(r"{v_path}\{device}");
                let class_guid = match read_value(RegHive::Hklm, &dev_path, "ClassGUID") {
                    Ok(Some(RegValue::String(s))) => s.to_ascii_lowercase(),
                    _ => String::new(),
                };
                let is_hdaudbus = matches!(
                    read_value(RegHive::Hklm, &dev_path, "Service"),
                    Ok(Some(RegValue::String(s))) if s.eq_ignore_ascii_case("hdaudbus")
                );
                if !USB_AUDIO_MSI_CLASSES.contains(&class_guid.as_str()) && !is_hdaudbus {
                    continue;
                }
                let msi_path = format!(
                    r"{dev_path}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
                );
                if key_exists(RegHive::Hklm, &msi_path) {
                    out.push(msi_path);
                }
            }
        }
        out
    }
}

impl Tweak for UsbAudioMsiModeTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum controlador USB/Áudio com chave MSI".into());
        }
        let ok = targets
            .iter()
            .filter(|sk| matches!(read_value(RegHive::Hklm, sk, "MSISupported"), Ok(Some(RegValue::DWord(1)))))
            .count();
        if ok == targets.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} dispositivos", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum controlador USB/Áudio compatível encontrado.".into());
        }
        let mut snap = Vec::new();
        for sk in &targets {
            let cur = read_value(RegHive::Hklm, sk, "MSISupported")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: "MSISupported".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, sk) in targets.iter().enumerate() {
            if let Err(e) =
                write_value_backup_restore(RegHive::Hklm, sk, "MSISupported", &RegValue::DWord(1))
            {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!("MSI Mode garantido em {} controlador(es) USB/Áudio.", targets.len()))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de MSI USB/Áudio encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value_backup_restore(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("MSI Mode USB/Áudio restaurado em {} item(ns).", snap.len()))
    }
}

const MMDEVICES_RENDER_KEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\MMDevices\Audio\Render";
const AUDIO_DISABLE_FX_PROP: &str = "{1da5d803-d492-4edd-8c23-e0c0ffee7f0e},5";

pub struct AudioEnhancementsTweak {
    meta: TweakMeta,
}

impl AudioEnhancementsTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.audio_enhancements_off",
                name: "Áudio: Desativar aprimoramentos de áudio (reduz latência do audiodg.exe)",
                category: Category::GpuLatency,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa APOs/aprimoramentos nativos do Windows nos dispositivos de reprodução ativos, reduzindo latência de processamento e estalos DPC no audiodg.exe. Atenção: recursos como som espacial/equalizador nativo do Windows ficam desligados.",
                details: vec![
                    format!(r"HKLM\{MMDEVICES_RENDER_KEY}\{{GUID}}\FxProperties\{AUDIO_DISABLE_FX_PROP} = 1"),
                ],
            },
        }
    }

    fn active_fx_keys() -> Vec<String> {
        let mut out = Vec::new();
        for guid in list_subkeys(RegHive::Hklm, MMDEVICES_RENDER_KEY) {
            let dev_key = format!(r"{MMDEVICES_RENDER_KEY}\{guid}");
            if matches!(
                read_value(RegHive::Hklm, &dev_key, "DeviceState"),
                Ok(Some(RegValue::DWord(1)))
            ) {
                let fx_key = format!(r"{dev_key}\FxProperties");
                if key_exists(RegHive::Hklm, &fx_key) {
                    out.push(fx_key);
                }
            }
        }
        out
    }
}

impl Tweak for AudioEnhancementsTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let keys = Self::active_fx_keys();
        if keys.is_empty() {
            return TweakStatus::NotApplicable("Nenhum dispositivo de áudio ativo com FxProperties".into());
        }
        let ok = keys
            .iter()
            .filter(|k| matches!(read_value(RegHive::Hklm, k, AUDIO_DISABLE_FX_PROP), Ok(Some(RegValue::DWord(1)))))
            .count();
        if ok == keys.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} dispositivos", keys.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let keys = Self::active_fx_keys();
        if keys.is_empty() {
            return Ok("Nenhum dispositivo de áudio ativo encontrado.".into());
        }
        let mut snap = Vec::new();
        for k in &keys {
            let cur = read_value(RegHive::Hklm, k, AUDIO_DISABLE_FX_PROP)?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: k.clone(),
                name: AUDIO_DISABLE_FX_PROP.into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, k) in keys.iter().enumerate() {
            if let Err(e) = write_value_backup_restore(
                RegHive::Hklm,
                k,
                AUDIO_DISABLE_FX_PROP,
                &RegValue::DWord(1),
            ) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "Aprimoramentos de áudio desativados em {} dispositivo(s).",
            keys.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de áudio encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value_backup_restore(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("Aprimoramentos de áudio restaurados em {} dispositivo(s).", snap.len()))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let keys = Self::active_fx_keys();
        for k in &keys {
            write_value_backup_restore(
                RegHive::Hklm,
                k,
                AUDIO_DISABLE_FX_PROP,
                &RegValue::DWord(0),
            )?;
        }
        Ok(format!("Aprimoramentos de áudio reativados (0) em {} dispositivo(s).", keys.len()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct DnsBackupEntry {
    alias: String,
    static_ns: String,
    addresses: Vec<String>,
}

pub struct CloudflareDnsTweak {
    meta: TweakMeta,
}

impl CloudflareDnsTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.dns_cloudflare_doh",
                name: "Rede: DNS Cloudflare (1.1.1.1 / 1.0.0.1) com DNS-over-HTTPS (DoH)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Configura DNS Cloudflare (1.1.1.1 / 1.0.0.1) com DoH automático nos adaptadores físicos ativos, acelerando resolução de nomes e conexões iniciais de launchers/jogos (salva o DNS anterior/DHCP para reversão).",
                details: vec![
                    "Set-DnsClientServerAddress -> 1.1.1.1, 1.0.0.1 nos adaptadores físicos Up".into(),
                    "Add/Set-DnsClientDohServerAddress com AutoUpgrade=$true".into(),
                ],
            },
        }
    }
}

impl Tweak for CloudflareDnsTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let script = r#"
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' })
if (-not $nics.Count) { Write-Output 'NA'; exit 0 }
$ok = 0
foreach ($n in $nics) {
    $dns = (Get-DnsClientServerAddress -InterfaceIndex $n.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue).ServerAddresses
    if ($dns -and $dns[0] -eq '1.1.1.1') { $ok++ }
}
Write-Output "$ok/$($nics.Count)"
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NA" {
                    TweakStatus::NotApplicable("Nenhum adaptador físico conectado (Up)".into())
                } else if let Some((a, b)) = s.split_once('/') {
                    let ok: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(0);
                    if total == 0 {
                        TweakStatus::NotApplicable("Nenhum adaptador físico ativo".into())
                    } else if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} adaptadores"))
                    }
                } else {
                    TweakStatus::Error(s.to_string())
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query = r#"
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | Where-Object { $_.Status -eq 'Up' })
$rows = foreach ($n in $nics) {
    $guid = $n.InterfaceGuid
    $reg = "HKLM:\SYSTEM\CurrentControlSet\Services\Tcpip\Parameters\Interfaces\$guid"
    $staticNs = [string](Get-ItemProperty $reg -Name NameServer -ErrorAction SilentlyContinue).NameServer
    $addrs = @((Get-DnsClientServerAddress -InterfaceIndex $n.ifIndex -AddressFamily IPv4 -ErrorAction SilentlyContinue).ServerAddresses)
    [pscustomobject]@{ alias = $n.Name; static_ns = $staticNs; addresses = $addrs }
}
if ($rows) { ConvertTo-Json -InputObject @($rows) -Depth 3 -Compress } else { '[]' }
"#;
        let out = run_powershell_checked(query)?;
        let rows: Vec<DnsBackupEntry> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok("Nenhum adaptador físico conectado para configurar DNS.".into());
        }
        let json = serde_json::to_value(&rows).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply_script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    Set-DnsClientServerAddress -InterfaceAlias $r.alias -ServerAddresses ('1.1.1.1','1.0.0.1') -ErrorAction Stop
}}
foreach ($ip in @('1.1.1.1','1.0.0.1')) {{
    try {{
        if (Get-DnsClientDohServerAddress -ServerAddress $ip -ErrorAction SilentlyContinue) {{
            Set-DnsClientDohServerAddress -ServerAddress $ip -AllowFallbackToUdp $true -AutoUpgrade $true -ErrorAction SilentlyContinue | Out-Null
        }} else {{
            Add-DnsClientDohServerAddress -ServerAddress $ip -DohTemplate 'https://cloudflare-dns.com/dns-query' -AllowFallbackToUdp $true -AutoUpgrade $true -ErrorAction SilentlyContinue | Out-Null
        }}
    }} catch {{}}
}}
"#
        );
        if let Err(e) = run_powershell_checked(&apply_script) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok(format!(
            "DNS Cloudflare (1.1.1.1 / 1.0.0.1 + DoH) aplicado em {} adaptador(es).",
            rows.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de DNS encontrado.".into());
        };
        let rows: Vec<DnsBackupEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    if ([string]::IsNullOrWhiteSpace($r.static_ns)) {{
        Set-DnsClientServerAddress -InterfaceAlias $r.alias -ResetServerAddresses -ErrorAction SilentlyContinue
    }} else {{
        $ips = @($r.addresses | Where-Object {{ $_ }})
        if ($ips.Count) {{
            Set-DnsClientServerAddress -InterfaceAlias $r.alias -ServerAddresses $ips -ErrorAction SilentlyContinue
        }} else {{
            Set-DnsClientServerAddress -InterfaceAlias $r.alias -ResetServerAddresses -ErrorAction SilentlyContinue
        }}
    }}
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok("DNS original/DHCP restaurado.".into())
    }
}

const NIC_CLASS_KEY: &str = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e972-e325-11ce-bfc1-08002be10318}";

pub struct NicPowerMgmtTweak {
    meta: TweakMeta,
}

impl NicPowerMgmtTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.nic_power_mgmt_off",
                name: "Rede: Desativar economia de energia nos adaptadores físicos (PnPCapabilities)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Marca o bit 0x10 em PnPCapabilities (equivalente a desmarcar 'O computador pode desligar o dispositivo para economizar energia') nas placas de rede PCI físicas (Ethernet/Wi-Fi), evitando picos de latência e micro-quedas ao sair do modo ocioso.",
                details: vec![
                    format!(r"Escaneia HKLM\{NIC_CLASS_KEY}\00xx (ComponentId pci\*)"),
                    "Define PnPCapabilities |= 0x10 (bit 'Allow the computer to turn off this device' OFF)".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for sub in list_subkeys(RegHive::Hklm, NIC_CLASS_KEY) {
            if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let dev_key = format!(r"{NIC_CLASS_KEY}\{sub}");
            if let Ok(Some(RegValue::String(cid))) =
                read_value(RegHive::Hklm, &dev_key, "ComponentId")
            {
                if cid.to_ascii_lowercase().starts_with("pci\\") {
                    out.push(dev_key);
                }
            }
        }
        out
    }
}

impl Tweak for NicPowerMgmtTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum adaptador de rede PCI físico encontrado".into());
        }
        let off = targets
            .iter()
            .filter(|sk| {
                matches!(
                    read_value(RegHive::Hklm, sk, "PnPCapabilities"),
                    Ok(Some(RegValue::DWord(v))) if (v & 0x10) != 0
                )
            })
            .count();
        if off == targets.len() {
            TweakStatus::Applied
        } else if off == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{off}/{} adaptadores", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum adaptador de rede PCI físico encontrado.".into());
        }
        let mut snap = Vec::new();
        for sk in &targets {
            let cur = read_value(RegHive::Hklm, sk, "PnPCapabilities")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: "PnPCapabilities".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, entry) in snap.iter().enumerate() {
            let cur_val = match &entry.value {
                Some(RegValue::DWord(v)) => *v,
                _ => 0,
            };
            if let Err(e) = write_value_backup_restore(
                RegHive::Hklm,
                &entry.subkey,
                "PnPCapabilities",
                &RegValue::DWord(cur_val | 0x10),
            ) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "Economia de energia desativada em {} adaptador(es) de rede PCI.",
            targets.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de economia de energia de rede encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value_backup_restore(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("Gerenciamento de energia de rede restaurado em {} item(ns).", snap.len()))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let targets = Self::scan_targets();
        for sk in &targets {
            if let Ok(Some(RegValue::DWord(v))) = read_value(RegHive::Hklm, sk, "PnPCapabilities") {
                write_value_backup_restore(
                    RegHive::Hklm,
                    sk,
                    "PnPCapabilities",
                    &RegValue::DWord(v & !0x10),
                )?;
            }
        }
        Ok(format!(
            "PnPCapabilities restaurado ao padrão em {} adaptador(es).",
            targets.len()
        ))
    }
}

const EDGE_POLICY_KEY: &str = r"SOFTWARE\Policies\Microsoft\Edge";
const HKCU_RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const EDGE_POLICY_NAMES: &[&str] = &[
    "StartupBoostEnabled",
    "BackgroundModeEnabled",
    "HubsSidebarEnabled",
    "EdgeShoppingAssistantEnabled",
];

pub struct EdgeBackgroundTweak {
    meta: TweakMeta,
}

impl EdgeBackgroundTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "bloat.edge_background_off",
                name: "Microsoft Edge: Desativar Startup Boost, modo em 2º plano e barra lateral",
                category: Category::PrivacyBloat,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Impede que o Microsoft Edge pré-carregue no boot (Startup Boost), remove MicrosoftEdgeAutoLaunch_* do Run e desativa execução em 2º plano após fechar, economizando ~150–250 MB de RAM. O navegador continua funcionando normalmente ao abrir.",
                details: vec![
                    format!(r"HKLM\{EDGE_POLICY_KEY}: StartupBoostEnabled=0, BackgroundModeEnabled=0, HubsSidebarEnabled=0, EdgeShoppingAssistantEnabled=0"),
                    format!(r"HKCU\{HKCU_RUN_KEY}\MicrosoftEdgeAutoLaunch_* -> REMOVIDO (com backup)"),
                ],
            },
        }
    }

    fn edge_autolaunch_names() -> Vec<String> {
        list_value_names(RegHive::Hkcu, HKCU_RUN_KEY)
            .into_iter()
            .filter(|n| n.starts_with("MicrosoftEdgeAutoLaunch_"))
            .collect()
    }
}

impl Tweak for EdgeBackgroundTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let mut ok = 0usize;
        for name in EDGE_POLICY_NAMES {
            if matches!(read_value(RegHive::Hklm, EDGE_POLICY_KEY, name), Ok(Some(RegValue::DWord(0)))) {
                ok += 1;
            }
        }
        let autolaunch = Self::edge_autolaunch_names();
        let total = EDGE_POLICY_NAMES.len() + usize::from(!autolaunch.is_empty());
        if autolaunch.is_empty() && ok == EDGE_POLICY_NAMES.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{total} itens"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let mut snap = Vec::new();
        for &name in EDGE_POLICY_NAMES {
            let cur = read_value(RegHive::Hklm, EDGE_POLICY_KEY, name)?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: EDGE_POLICY_KEY.into(),
                name: name.into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: false,
            });
        }
        let autolaunch = Self::edge_autolaunch_names();
        for name in &autolaunch {
            let cur = read_value(RegHive::Hkcu, HKCU_RUN_KEY, name)?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hkcu,
                subkey: HKCU_RUN_KEY.into(),
                name: name.clone(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: false,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, &name) in EDGE_POLICY_NAMES.iter().enumerate() {
            if let Err(e) = write_value(RegHive::Hklm, EDGE_POLICY_KEY, name, &RegValue::DWord(0)) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        for name in &autolaunch {
            delete_value(RegHive::Hkcu, HKCU_RUN_KEY, name)?;
        }
        Ok(format!(
            "Políticas do Edge aplicadas ({} autolaunch removido(s)).",
            autolaunch.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup do Microsoft Edge encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("Configurações do Microsoft Edge restauradas.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        for &name in EDGE_POLICY_NAMES {
            delete_value(RegHive::Hklm, EDGE_POLICY_KEY, name)?;
        }
        Ok("Políticas do Microsoft Edge removidas (padrão do Windows).".into())
    }
}

pub struct NvidiaHdcpOffTweak {
    meta: TweakMeta,
}

impl NvidiaHdcpOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.nvidia_hdcp_off",
                name: "GPU NVIDIA: Desativar HDCP (RMHdcpKeyglobZero=1)",
                category: Category::GpuLatency,
                risk: RiskLevel::Medium,
                requires_reboot: true,
                requires_admin: true,
                description: "Define RMHdcpKeyglobZero=1 no driver NVIDIA para eliminar handshakes HDCP em Alt-Tab e trocas de resolução em tela cheia. Atenção: serviços de streaming 4K com DRM (Netflix, Prime Video, Disney+) exigem HDCP ativado.",
                details: vec![
                    format!(r"HKLM\{GPU_CLASS_KEY}\00xx\RMHdcpKeyglobZero = 1 (VEN_10DE)"),
                ],
            },
        }
    }
}

impl Tweak for NvidiaHdcpOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let keys = NvidiaDynamicPstateTweak::nvidia_keys();
        if keys.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma GPU NVIDIA (VEN_10DE) detectada".into());
        }
        let ok = keys
            .iter()
            .filter(|k| {
                matches!(
                    read_value(RegHive::Hklm, k, "RMHdcpKeyglobZero"),
                    Ok(Some(RegValue::DWord(1)))
                )
            })
            .count();
        if ok == keys.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} GPUs NVIDIA", keys.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let keys = NvidiaDynamicPstateTweak::nvidia_keys();
        if keys.is_empty() {
            return Err("Nenhuma GPU NVIDIA instalada neste PC.".into());
        }
        let mut snap = Vec::new();
        for k in &keys {
            let cur = read_value(RegHive::Hklm, k, "RMHdcpKeyglobZero")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: k.clone(),
                name: "RMHdcpKeyglobZero".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: false,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, k) in keys.iter().enumerate() {
            if let Err(e) = write_value(
                RegHive::Hklm,
                k,
                "RMHdcpKeyglobZero",
                &RegValue::DWord(1),
            ) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "RMHdcpKeyglobZero=1 aplicado em {} GPU(s) NVIDIA (reinicie para valer).",
            keys.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de RMHdcpKeyglobZero encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("RMHdcpKeyglobZero restaurado ao padrão.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        for k in NvidiaDynamicPstateTweak::nvidia_keys() {
            delete_value(RegHive::Hklm, &k, "RMHdcpKeyglobZero")?;
        }
        Ok("RMHdcpKeyglobZero removido (HDCP padrão do driver NVIDIA).".into())
    }
}

pub struct DefenderGameExclusionsTweak {
    meta: TweakMeta,
}

impl DefenderGameExclusionsTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.defender_game_exclusions",
                name: "Windows Defender: Exclusões automáticas para pastas de jogos e cache de shaders",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Adiciona exclusões oficiais (Add-MpPreference) somente para pastas de bibliotecas de jogos existentes (Steam, Epic, Riot, EA, Ubisoft, Battle.net, XboxGames) e caches de shaders NVIDIA/AMD/DirectX, reduzindo micro-stutters de I/O sem desligar o antivírus.",
                details: vec![
                    "Escaneia discos locais fixos (Steam, SteamLibrary, Epic, Riot, EA, Ubisoft, Battle.net, XboxGames)".into(),
                    "Inclui caches de shaders (NVIDIA\\DXCache, NVIDIA\\GLCache, NVIDIA\\NV_Cache, AMD\\DxCache, D3DSCache)".into(),
                ],
            },
        }
    }

    const CANDIDATE_SCRIPT: &'static str = r#"
$cands = @()
$fixed = @(Get-CimInstance Win32_LogicalDisk -Filter "DriveType=3" -ErrorAction SilentlyContinue | ForEach-Object { $_.DeviceID })
$rels = @(
    'Program Files (x86)\Steam\steamapps',
    'SteamLibrary\steamapps',
    'Program Files\Epic Games',
    'Epic Games',
    'Riot Games',
    'Program Files\EA Games',
    'EA Games',
    'Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games',
    'Program Files (x86)\Battle.net',
    'Games',
    'XboxGames'
)
foreach ($d in $fixed) {
    foreach ($r in $rels) {
        $p = Join-Path "$d\" $r
        if (Test-Path -LiteralPath $p) { $cands += $p }
    }
}
foreach ($p in @(
    (Join-Path $env:LOCALAPPDATA 'NVIDIA\DXCache'),
    (Join-Path $env:LOCALAPPDATA 'NVIDIA\GLCache'),
    (Join-Path $env:LOCALAPPDATA 'AMD\DxCache'),
    (Join-Path $env:LOCALAPPDATA 'D3DSCache'),
    'C:\ProgramData\NVIDIA Corporation\NV_Cache'
)) {
    if ($p -and (Test-Path -LiteralPath $p)) { $cands += $p }
}
$cands = @($cands | ForEach-Object { $_.TrimEnd('\') } | Select-Object -Unique)
"#;
}

impl Tweak for DefenderGameExclusionsTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        let script = format!(
            r#"
$wd = Get-Service WinDefend -ErrorAction SilentlyContinue
if (-not $wd -or $wd.Status -ne 'Running') {{ Write-Output 'NO_DEFENDER'; exit 0 }}
{}
if (-not $cands.Count) {{ Write-Output 'NONE'; exit 0 }}
$excl = @((Get-MpPreference -ErrorAction Stop).ExclusionPath | ForEach-Object {{ "$_".TrimEnd('\').ToLowerInvariant() }})
$ok = @($cands | Where-Object {{ $excl -contains $_.ToLowerInvariant() }}).Count
Write-Output "$ok/$($cands.Count)"
"#,
            Self::CANDIDATE_SCRIPT
        );
        match run_powershell_checked(&script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NO_DEFENDER" {
                    TweakStatus::NotApplicable("Windows Defender não está ativo".into())
                } else if s == "NONE" {
                    TweakStatus::NotApplicable("Nenhuma pasta de jogos/shaders detectada".into())
                } else if let Some((a, b)) = s.split_once('/') {
                    let ok: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(0);
                    if total == 0 {
                        TweakStatus::NotApplicable("Nenhuma pasta alvo".into())
                    } else if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} pastas"))
                    }
                } else {
                    TweakStatus::Error(s.to_string())
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query = format!(
            r#"
$wd = Get-Service WinDefend -ErrorAction SilentlyContinue
if (-not $wd -or $wd.Status -ne 'Running') {{ Write-Output '[]'; exit 0 }}
{}
$excl = @((Get-MpPreference -ErrorAction Stop).ExclusionPath | ForEach-Object {{ "$_".TrimEnd('\').ToLowerInvariant() }})
$toAdd = @($cands | Where-Object {{ $excl -notcontains $_.ToLowerInvariant() }})
if ($toAdd.Count) {{ ConvertTo-Json -InputObject @($toAdd) -Compress }} else {{ '[]' }}
"#,
            Self::CANDIDATE_SCRIPT
        );
        let out = run_powershell_checked(&query)?;
        let to_add: Vec<String> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if to_add.is_empty() {
            return Ok("Todas as pastas de jogos/shaders já estão excluídas (ou Defender inativo).".into());
        }
        let json = serde_json::to_value(&to_add).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&to_add)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply_script = format!(
            r#"
$paths = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
if ($paths.Count) {{ Add-MpPreference -ExclusionPath $paths -ErrorAction Stop }}
"#
        );
        match run_powershell_checked(&apply_script) {
            Ok(_) => Ok(format!(
                "{} pasta(s) de jogos/shaders adicionada(s) às exclusões do Defender.",
                to_add.len()
            )),
            Err(e) => {
                if created {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de exclusões do Defender encontrado.".into());
        };
        let paths: Vec<String> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        if !paths.is_empty() {
            let escaped = serde_json::to_string(&paths)
                .map_err(|e| e.to_string())?
                .replace('\'', "''");
            let script = format!(
                r#"
$paths = [string[]]@((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
if ($paths.Count) {{ Remove-MpPreference -ExclusionPath $paths -ErrorAction SilentlyContinue }}
"#
            );
            run_powershell_checked(&script)?;
        }
        backups.remove(self.meta.id)?;
        Ok(format!(
            "{} exclusão(ões) do Defender removida(s).",
            paths.len()
        ))
    }
}

const USB_D3_FLAGS: &[&str] = &[
    "AllowIdleIrpInD3",
    "DeviceSelectiveSuspended",
    "SelectiveSuspendEnabled",
    "SelectiveSuspendOn",
];

pub struct UsbD3SuspendTweak {
    meta: TweakMeta,
}

impl UsbD3SuspendTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "power.usb_d3_suspend_off",
                name: "USB: Desativar suspensão profunda D3 (AllowIdleIrpInD3 / DeviceSelectiveSuspended)",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Zera AllowIdleIrpInD3, DeviceSelectiveSuspended, SelectiveSuspendEnabled e SelectiveSuspendOn em Enum\\USB\\*\\*\\Device Parameters onde já existem (preservando o tipo DWORD/Binary original), impedindo que mouses 1000–8000 Hz, teclados e DACs USB entrem em estado ocioso D3.",
                details: vec![
                    r"Escaneia Enum\USB\*\*\Device Parameters para AllowIdleIrpInD3, DeviceSelectiveSuspended, SelectiveSuspendEnabled e SelectiveSuspendOn".into(),
                    "Define 0 preservando o tipo original (DWORD ou Binary 1 byte)".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<(String, String, RegValue)> {
        let mut out = Vec::new();
        for dev in list_subkeys(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Enum\USB") {
            let dev_path = format!(r"SYSTEM\CurrentControlSet\Enum\USB\{dev}");
            for inst in list_subkeys(RegHive::Hklm, &dev_path) {
                let dp = format!(r"{dev_path}\{inst}\Device Parameters");
                for &flag in USB_D3_FLAGS {
                    match read_value(RegHive::Hklm, &dp, flag) {
                        Ok(Some(v @ RegValue::DWord(_))) => out.push((dp.clone(), flag.into(), v)),
                        Ok(Some(RegValue::Binary(b))) if b.len() == 1 => {
                            out.push((dp.clone(), flag.into(), RegValue::Binary(b)))
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }

    fn is_zero(val: &RegValue) -> bool {
        match val {
            RegValue::DWord(0) => true,
            RegValue::Binary(b) if b.len() == 1 && b[0] == 0 => true,
            _ => false,
        }
    }

    fn zero_for(val: &RegValue) -> RegValue {
        match val {
            RegValue::Binary(_) => RegValue::Binary(vec![0]),
            _ => RegValue::DWord(0),
        }
    }

    fn one_for(val: &RegValue) -> RegValue {
        match val {
            RegValue::Binary(_) => RegValue::Binary(vec![1]),
            _ => RegValue::DWord(1),
        }
    }
}

impl Tweak for UsbD3SuspendTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma chave de suspensão D3 USB encontrada".into());
        }
        let off = targets.iter().filter(|(_, _, v)| Self::is_zero(v)).count();
        if off == targets.len() {
            TweakStatus::Applied
        } else if off == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{off}/{} chaves", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhuma chave de suspensão D3 USB encontrada.".into());
        }
        let mut snap = Vec::with_capacity(targets.len());
        for (sk, name, val) in &targets {
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: name.clone(),
                existed: true,
                value: Some(val.clone()),
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, (sk, name, val)) in targets.iter().enumerate() {
            if let Err(e) =
                write_value_backup_restore(RegHive::Hklm, sk, name, &Self::zero_for(val))
            {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "Suspensão D3 desativada em {} entrada(s) USB.",
            targets.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de suspensão D3 USB encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("{} entrada(s) D3 USB restaurada(s).", snap.len()))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let targets = Self::scan_targets();
        for (sk, name, val) in &targets {
            write_value_backup_restore(RegHive::Hklm, sk, name, &Self::one_for(val))?;
        }
        Ok(format!(
            "{} entrada(s) D3 USB definidas para padrão (1).",
            targets.len()
        ))
    }
}

pub struct AmdUlpsOffTweak {
    meta: TweakMeta,
}

impl AmdUlpsOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.amd_ulps_off",
                name: "GPU AMD: Desativar ULPS (Ultra Low Power State)",
                category: Category::GpuLatency,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Define EnableUlps=0 para GPUs AMD Radeon (VEN_1002), evitando quedas profundas de clock/tensão e micro-stutters de rampa. Marca automaticamente como N/A em PCs NVIDIA.",
                details: vec![
                    format!(r"HKLM\{GPU_CLASS_KEY}\00xx\EnableUlps = 0 (VEN_1002)"),
                ],
            },
        }
    }

    fn amd_keys() -> Vec<String> {
        let mut res = Vec::new();
        for sub in list_subkeys(RegHive::Hklm, GPU_CLASS_KEY) {
            if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let dev_key = format!(r"{GPU_CLASS_KEY}\{sub}");
            if let Ok(Some(RegValue::String(id))) =
                read_value(RegHive::Hklm, &dev_key, "MatchingDeviceId")
            {
                let has_driver = key_exists(RegHive::Hklm, &format!(r"{dev_key}\UMD"))
                    || matches!(read_value(RegHive::Hklm, &dev_key, "EnableUlps"), Ok(Some(_)));
                if id.to_ascii_uppercase().contains("VEN_1002") && has_driver {
                    res.push(dev_key);
                }
            }
        }
        res
    }
}

impl Tweak for AmdUlpsOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let keys = Self::amd_keys();
        if keys.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma GPU AMD (VEN_1002) detectada".into());
        }
        let ok = keys
            .iter()
            .filter(|k| matches!(read_value(RegHive::Hklm, k, "EnableUlps"), Ok(Some(RegValue::DWord(0)))))
            .count();
        if ok == keys.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} GPUs AMD", keys.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let keys = Self::amd_keys();
        if keys.is_empty() {
            return Err("Nenhuma GPU AMD (VEN_1002) instalada neste PC.".into());
        }
        let mut snap = Vec::new();
        for k in &keys {
            let cur = read_value(RegHive::Hklm, k, "EnableUlps")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: k.clone(),
                name: "EnableUlps".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: false,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, k) in keys.iter().enumerate() {
            if let Err(e) = write_value(RegHive::Hklm, k, "EnableUlps", &RegValue::DWord(0)) {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "ULPS desativado (EnableUlps=0) em {} GPU(s) AMD.",
            keys.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de EnableUlps encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("EnableUlps restaurado ao padrão.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        for k in Self::amd_keys() {
            write_value(RegHive::Hklm, &k, "EnableUlps", &RegValue::DWord(1))?;
        }
        Ok("EnableUlps definido para padrão (1).".into())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct BcdFlagEntry {
    flag: String,
    value: String,
}

pub struct BcdCleanTimersTweak {
    meta: TweakMeta,
}

impl BcdCleanTimersTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.bcd_clean_timers",
                name: "Limpeza BCD: Remover flags legadas de timer/HPET (se presentes)",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Verifica o BCD atual ({current}) e remove flags legadas de timer/HPET de scripts antigos que degradam latência DPC e 1% low no Windows 11 24H2/25H2 (com backup para reversão). Se o BCD já estiver limpo, nenhuma alteração é feita.",
                details: vec![
                    "REMOVER flags legadas de timer/HPET no BCD (bcdedit /deletevalue {current})".into(),
                ],
            },
        }
    }

    fn scan_legacy_flags() -> Result<Vec<BcdFlagEntry>, String> {
        let out = crate::sys::cmd::run_hidden_checked("bcdedit.exe", &["/enum", "{current}"], &[0])?;
        let mut found = Vec::new();
        for line in out.stdout.lines() {
            let mut parts = line.split_whitespace();
            let Some(key) = parts.next() else {
                continue;
            };
            let low = key.to_ascii_lowercase();
            if matches!(
                low.as_str(),
                "useplatformclock" | "disabledynamictick" | "tscsyncpolicy"
            ) {
                let value = parts.collect::<Vec<_>>().join(" ");
                found.push(BcdFlagEntry {
                    flag: key.to_string(),
                    value,
                });
            }
        }
        Ok(found)
    }
}

impl Tweak for BcdCleanTimersTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        match Self::scan_legacy_flags() {
            Ok(list) if list.is_empty() => TweakStatus::Applied,
            Ok(_) => TweakStatus::NotApplied,
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let flags = Self::scan_legacy_flags()?;
        if flags.is_empty() {
            return Ok("BCD já está limpo (nenhuma flag legada de timer presente).".into());
        }
        let json = serde_json::to_value(&flags).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (idx, f) in flags.iter().enumerate() {
            if let Err(e) = crate::sys::cmd::run_hidden_checked(
                "bcdedit.exe",
                &["/deletevalue", "{current}", &f.flag],
                &[0],
            ) {
                if created && idx == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!(
            "{} flag(s) legada(s) de timer removida(s) do BCD.",
            flags.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de flags BCD encontrado.".into());
        };
        let flags: Vec<BcdFlagEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for f in &flags {
            crate::sys::cmd::run_hidden_checked(
                "bcdedit.exe",
                &["/set", "{current}", &f.flag, &f.value],
                &[0],
            )?;
        }
        backups.remove(self.meta.id)?;
        Ok(format!("{} flag(s) BCD restaurada(s).", flags.len()))
    }
}

const STORAGE_MSI_CLASSES: &[&str] = &[
    "{4d36e97b-e325-11ce-bfc1-08002be10318}",
    "{4d36e96a-e325-11ce-bfc1-08002be10318}",
];

pub struct NvmeStorageMsiTweak {
    meta: TweakMeta,
}

impl NvmeStorageMsiTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.nvme_storage_msi",
                name: "Armazenamento: Ativar MSI Mode em controladores NVMe e SATA AHCI",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Garante MSISupported=1 em controladores PCI de armazenamento NVMe (SCSIAdapter) e SATA AHCI (HDC) que expõem suporte MSI, evitando compartilhamento de IRQ legada durante carregamento de texturas/assets.",
                details: vec![
                    r"Escaneia Enum\PCI\*\* para classes SCSIAdapter (NVMe) e HDC (AHCI)".into(),
                    "Define MessageSignaledInterruptProperties\\MSISupported = 1".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for vendor in list_subkeys(RegHive::Hklm, r"SYSTEM\CurrentControlSet\Enum\PCI") {
            let v_path = format!(r"SYSTEM\CurrentControlSet\Enum\PCI\{vendor}");
            for device in list_subkeys(RegHive::Hklm, &v_path) {
                let dev_path = format!(r"{v_path}\{device}");
                let class_guid = match read_value(RegHive::Hklm, &dev_path, "ClassGUID") {
                    Ok(Some(RegValue::String(s))) => s.to_ascii_lowercase(),
                    _ => continue,
                };
                if !STORAGE_MSI_CLASSES.contains(&class_guid.as_str()) {
                    continue;
                }
                let msi_path = format!(
                    r"{dev_path}\Device Parameters\Interrupt Management\MessageSignaledInterruptProperties"
                );
                if key_exists(RegHive::Hklm, &msi_path) {
                    out.push(msi_path);
                }
            }
        }
        out
    }
}

impl Tweak for NvmeStorageMsiTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum controlador NVMe/AHCI com chave MSI".into());
        }
        let ok = targets
            .iter()
            .filter(|sk| matches!(read_value(RegHive::Hklm, sk, "MSISupported"), Ok(Some(RegValue::DWord(1)))))
            .count();
        if ok == targets.len() {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{} controladores", targets.len()))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum controlador NVMe/AHCI compatível encontrado.".into());
        }
        let mut snap = Vec::new();
        for sk in &targets {
            let cur = read_value(RegHive::Hklm, sk, "MSISupported")?;
            snap.push(RegSnapshotEntry {
                hive: RegHive::Hklm,
                subkey: sk.clone(),
                name: "MSISupported".into(),
                existed: cur.is_some(),
                value: cur,
                use_backup_restore: true,
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        for (modified, sk) in targets.iter().enumerate() {
            if let Err(e) =
                write_value_backup_restore(RegHive::Hklm, sk, "MSISupported", &RegValue::DWord(1))
            {
                if created && modified == 0 {
                    let _ = backups.remove(self.meta.id);
                }
                return Err(e);
            }
        }
        Ok(format!("MSI Mode garantido em {} controlador(es) NVMe/AHCI.", targets.len()))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de MSI NVMe/AHCI encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value_backup_restore(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value_backup_restore(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!("MSI Mode NVMe/AHCI restaurado em {} item(ns).", snap.len()))
    }
}

pub struct MmAgentPrelaunchTweak {
    meta: TweakMeta,
}

impl MmAgentPrelaunchTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.mmagent_prelaunch_off",
                name: "Sistema: Desativar ApplicationPreLaunch e PageCombining (MMAgent)",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa ApplicationPreLaunch e PageCombining no MMAgent (Disable-MMAgent), impedindo o Windows de pré-abrir apps em segundo plano (que acordam a GPU dedicada e geram picos de frametime) e mantendo MemoryCompression intacta para quem tem 8–16 GB de RAM.",
                details: vec![
                    "Get-MMAgent / Disable-MMAgent -ApplicationPreLaunch -PageCombining".into(),
                    "Reversão: restaura cada flag ao estado original salvo no backup".into(),
                ],
            },
        }
    }

    fn query_flags() -> Result<(bool, bool), String> {
        let out = run_powershell_checked(
            "$m = Get-MMAgent -ErrorAction Stop; Write-Output \"$([bool]$m.ApplicationPreLaunch),$([bool]$m.PageCombining)\"",
        )?;
        let s = out.stdout.trim();
        let (a, b) = s
            .split_once(',')
            .ok_or_else(|| format!("Saída inesperada de Get-MMAgent: {s}"))?;
        Ok((
            a.eq_ignore_ascii_case("True"),
            b.eq_ignore_ascii_case("True"),
        ))
    }
}

impl Tweak for MmAgentPrelaunchTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        match Self::query_flags() {
            Ok((false, false)) => TweakStatus::Applied,
            Ok((true, true)) => TweakStatus::NotApplied,
            Ok(_) => TweakStatus::Partial("1/2 flags desativadas".into()),
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let (prelaunch, page_combining) = Self::query_flags()?;
        if !prelaunch && !page_combining {
            return Ok("ApplicationPreLaunch e PageCombining já estavam desativados.".into());
        }
        let json = serde_json::json!({
            "application_prelaunch": prelaunch,
            "page_combining": page_combining,
        });
        let created = backups.save_if_absent(self.meta.id, json)?;
        if let Err(e) = run_powershell_checked(
            "Start-Service SysMain -ErrorAction SilentlyContinue; Disable-MMAgent -ApplicationPreLaunch -PageCombining -ErrorAction Stop",
        ) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("ApplicationPreLaunch e PageCombining desativados via MMAgent.".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para MMAgent PreLaunch.".into());
        };
        let prelaunch = rec.data["application_prelaunch"].as_bool().unwrap_or(true);
        let page_combining = rec.data["page_combining"].as_bool().unwrap_or(false);
        let pre_cmd = if prelaunch {
            "Enable-MMAgent -ApplicationPreLaunch -ErrorAction Stop"
        } else {
            "Disable-MMAgent -ApplicationPreLaunch -ErrorAction Stop"
        };
        let pc_cmd = if page_combining {
            "Enable-MMAgent -PageCombining -ErrorAction Stop"
        } else {
            "Disable-MMAgent -PageCombining -ErrorAction Stop"
        };
        run_powershell_checked(&format!(
            "Start-Service SysMain -ErrorAction SilentlyContinue; {pre_cmd}; {pc_cmd}"
        ))?;
        backups.remove(self.meta.id)?;
        Ok("Flags MMAgent restauradas ao estado original.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        run_powershell_checked(
            "Start-Service SysMain -ErrorAction SilentlyContinue; Enable-MMAgent -ApplicationPreLaunch -ErrorAction Stop",
        )?;
        Ok("ApplicationPreLaunch reativado (padrão do Windows).".into())
    }
}

const AUDIO_CLASS_KEY: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e96c-e325-11ce-bfc1-08002be10318}";

pub struct HdAudioIdleOffTweak {
    meta: TweakMeta,
}

impl HdAudioIdleOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "power.hdaudio_idle_off",
                name: "Áudio HD: Desativar suspensão D3 do codec (ConservationIdleTime/IdlePowerState)",
                category: Category::Power,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Impede o driver PortCls.sys de colocar o codec de áudio (Realtek, NVIDIA/AMD HD Audio) em sono D3 após poucos segundos de silêncio, eliminando estalos/pops e picos de latência DPC ao retomar o som.",
                details: vec![
                    format!(r"Escaneia HKLM\{AUDIO_CLASS_KEY}\00xx\PowerSettings"),
                    "ConservationIdleTime = [FF,FF,FF,FF], PerformanceIdleTime = [FF,FF,FF,FF], IdlePowerState = [00,00,00,00]".into(),
                ],
            },
        }
    }

    fn scan_targets() -> Vec<String> {
        let mut out = Vec::new();
        for sub in list_subkeys(RegHive::Hklm, AUDIO_CLASS_KEY) {
            if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            let ps_key = format!(r"{AUDIO_CLASS_KEY}\{sub}\PowerSettings");
            if key_exists(RegHive::Hklm, &ps_key) {
                out.push(ps_key);
            }
        }
        out
    }

    fn desired_values() -> [(&'static str, Vec<u8>); 3] {
        [
            ("ConservationIdleTime", vec![0xFF, 0xFF, 0xFF, 0xFF]),
            ("PerformanceIdleTime", vec![0xFF, 0xFF, 0xFF, 0xFF]),
            ("IdlePowerState", vec![0x00, 0x00, 0x00, 0x00]),
        ]
    }
}

impl Tweak for HdAudioIdleOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return TweakStatus::NotApplicable("Nenhum dispositivo de áudio com PowerSettings".into());
        }
        let desired = Self::desired_values();
        let total = targets.len() * desired.len();
        let mut ok = 0usize;
        for sk in &targets {
            for (name, bytes) in &desired {
                if matches!(read_value(RegHive::Hklm, sk, name), Ok(Some(RegValue::Binary(b))) if b == *bytes) {
                    ok += 1;
                }
            }
        }
        if ok == total {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{total} chaves"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let targets = Self::scan_targets();
        if targets.is_empty() {
            return Ok("Nenhum dispositivo de áudio com PowerSettings encontrado.".into());
        }
        let desired = Self::desired_values();
        let mut snap = Vec::new();
        for sk in &targets {
            for (name, _) in &desired {
                let cur = read_value(RegHive::Hklm, sk, name)?;
                snap.push(RegSnapshotEntry {
                    hive: RegHive::Hklm,
                    subkey: sk.clone(),
                    name: (*name).into(),
                    existed: cur.is_some(),
                    value: cur,
                    use_backup_restore: false,
                });
            }
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let mut modified = 0usize;
        for sk in &targets {
            for (name, bytes) in &desired {
                if let Err(e) = write_value(RegHive::Hklm, sk, name, &RegValue::Binary(bytes.clone())) {
                    if created && modified == 0 {
                        let _ = backups.remove(self.meta.id);
                    }
                    return Err(e);
                }
                modified += 1;
            }
        }
        Ok(format!(
            "Suspensão D3 de áudio desativada em {} dispositivo(s).",
            targets.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de PowerSettings de áudio encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok(format!(
            "PowerSettings de áudio restaurado em {} entrada(s).",
            snap.len()
        ))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let targets = Self::scan_targets();
        for sk in &targets {
            write_value(RegHive::Hklm, sk, "ConservationIdleTime", &RegValue::Binary(vec![0x0A, 0, 0, 0]))?;
            write_value(RegHive::Hklm, sk, "PerformanceIdleTime", &RegValue::Binary(vec![0x0A, 0, 0, 0]))?;
            write_value(RegHive::Hklm, sk, "IdlePowerState", &RegValue::Binary(vec![0x03, 0, 0, 0]))?;
        }
        Ok(format!(
            "PowerSettings de áudio restaurado ao padrão (10s / D3) em {} dispositivo(s).",
            targets.len()
        ))
    }
}

pub struct DefenderScanThrottleTweak {
    meta: TweakMeta,
}

impl DefenderScanThrottleTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.defender_scan_throttle",
                name: "Windows Defender: Limitar CPU de varredura a 20% e desativar catch-up/amostras",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Configura via Set-MpPreference ScanAvgCPULoadFactor=20 (padrão 50%), DisableCatchupFullScan=$true, DisableCatchupQuickScan=$true e SubmitSamplesConsent=2 (NeverSend), evitando picos de CPU/disco do Defender ao ligar o PC mantendo a proteção em tempo real 100% ativa.",
                details: vec![
                    "Set-MpPreference -ScanAvgCPULoadFactor 20".into(),
                    "Set-MpPreference -DisableCatchupFullScan $true -DisableCatchupQuickScan $true -SubmitSamplesConsent NeverSend".into(),
                ],
            },
        }
    }
}

impl Tweak for DefenderScanThrottleTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        let script = r#"
$wd = Get-Service WinDefend -ErrorAction SilentlyContinue
if (-not $wd -or $wd.Status -ne 'Running') { Write-Output 'NO_DEFENDER'; exit 0 }
$p = Get-MpPreference -ErrorAction Stop
$ok = 0
if ($p.ScanAvgCPULoadFactor -le 20 -and $p.ScanAvgCPULoadFactor -gt 0) { $ok++ }
if ($p.DisableCatchupFullScan) { $ok++ }
if ($p.DisableCatchupQuickScan) { $ok++ }
if ([int]$p.SubmitSamplesConsent -eq 2) { $ok++ }
Write-Output "$ok/4"
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NO_DEFENDER" {
                    TweakStatus::NotApplicable("Windows Defender não está ativo".into())
                } else if let Some((a, b)) = s.split_once('/') {
                    let ok: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(4);
                    if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} preferências"))
                    }
                } else {
                    TweakStatus::Error(s.to_string())
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query = r#"
$wd = Get-Service WinDefend -ErrorAction SilentlyContinue
if (-not $wd -or $wd.Status -ne 'Running') { throw 'Windows Defender não está ativo neste sistema.' }
$p = Get-MpPreference -ErrorAction Stop
[pscustomobject]@{
    cpu = [int]$p.ScanAvgCPULoadFactor
    catchup_full = [bool]$p.DisableCatchupFullScan
    catchup_quick = [bool]$p.DisableCatchupQuickScan
    samples = [int]$p.SubmitSamplesConsent
} | ConvertTo-Json -Compress
"#;
        let out = run_powershell_checked(query)?;
        let val: serde_json::Value =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, val)?;

        let apply_script = "Set-MpPreference -ScanAvgCPULoadFactor 20 -DisableCatchupFullScan $true -DisableCatchupQuickScan $true -SubmitSamplesConsent NeverSend -ErrorAction Stop";
        if let Err(e) = run_powershell_checked(apply_script) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("Preferências de varredura do Defender otimizadas (CPU <= 20%, catch-up/amostras off).".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de preferências do Defender encontrado.".into());
        };
        let cpu = rec.data["cpu"].as_i64().unwrap_or(50);
        let cf = if rec.data["catchup_full"].as_bool().unwrap_or(false) { "$true" } else { "$false" };
        let cq = if rec.data["catchup_quick"].as_bool().unwrap_or(false) { "$true" } else { "$false" };
        let samples = rec.data["samples"].as_i64().unwrap_or(1);
        let script = format!(
            "Set-MpPreference -ScanAvgCPULoadFactor {cpu} -DisableCatchupFullScan {cf} -DisableCatchupQuickScan {cq} -SubmitSamplesConsent {samples} -ErrorAction Stop"
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok("Preferências de varredura do Defender restauradas.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        run_powershell_checked(
            "Set-MpPreference -ScanAvgCPULoadFactor 50 -DisableCatchupFullScan $false -DisableCatchupQuickScan $false -SubmitSamplesConsent 1 -ErrorAction Stop",
        )?;
        Ok("Preferências do Defender restauradas ao padrão Microsoft.".into())
    }
}

pub struct ReservedStorageOffTweak {
    meta: TweakMeta,
}

impl ReservedStorageOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.reserved_storage_off",
                name: "Armazenamento Reservado do Windows: Desativar (libera ~7–12 GB no SSD)",
                category: Category::SystemKernel,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Desativa o Armazenamento Reservado do Windows Update via Set-WindowsReservedStorageState -State Disabled, liberando ~7–12 GB no SSD/NVMe (100% reversível com -State Enabled). Opcional: mantenha pelo menos 15 GB livres na unidade C: para futuras atualizações.",
                details: vec![
                    "Set-WindowsReservedStorageState -State Disabled (libera ~7-12 GB; reversível via -State Enabled)".into(),
                ],
            },
        }
    }
}

impl Tweak for ReservedStorageOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        let script = r#"
if (-not (Get-Command Get-WindowsReservedStorageState -ErrorAction SilentlyContinue)) {
    Write-Output 'UNSUPPORTED'
    exit 0
}
$st = Get-WindowsReservedStorageState -ErrorAction Stop
Write-Output "$($st.State)"
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s.eq_ignore_ascii_case("UNSUPPORTED") {
                    TweakStatus::NotApplicable("Cmdlet Reserved Storage indisponível".into())
                } else if s.eq_ignore_ascii_case("Disabled") {
                    TweakStatus::Applied
                } else if s.eq_ignore_ascii_case("Enabled") {
                    TweakStatus::NotApplied
                } else {
                    TweakStatus::Error(s.to_string())
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query = r#"
if (-not (Get-Command Get-WindowsReservedStorageState -ErrorAction SilentlyContinue)) {
    throw 'Cmdlet Get-WindowsReservedStorageState indisponível nesta edição do Windows.'
}
$st = Get-WindowsReservedStorageState -ErrorAction Stop
Write-Output "$($st.State)"
"#;
        let out = run_powershell_checked(query)?;
        let state = out.stdout.trim();
        if state.eq_ignore_ascii_case("Disabled") {
            return Ok("Armazenamento Reservado do Windows já está desativado.".into());
        }
        let created = backups.save_if_absent(
            self.meta.id,
            serde_json::json!({ "was_enabled": true }),
        )?;
        if let Err(e) =
            run_powershell_checked("Set-WindowsReservedStorageState -State Disabled -ErrorAction Stop | Out-Null")
        {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("Armazenamento Reservado do Windows desativado (~7–12 GB liberados).".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de Armazenamento Reservado encontrado.".into());
        };
        if rec.data["was_enabled"].as_bool().unwrap_or(true) {
            run_powershell_checked(
                "Set-WindowsReservedStorageState -State Enabled -ErrorAction Stop | Out-Null",
            )?;
        }
        backups.remove(self.meta.id)?;
        Ok("Armazenamento Reservado do Windows restaurado.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        run_powershell_checked(
            "Set-WindowsReservedStorageState -State Enabled -ErrorAction Stop | Out-Null",
        )?;
        Ok("Armazenamento Reservado do Windows restaurado ao padrão (Enabled).".into())
    }
}

pub struct HypervisorOffTweak {
    meta: TweakMeta,
}

impl HypervisorOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "sys.hypervisor_off",
                name: "BCD / Virtualização: Desativar inicialização do Hipervisor (hypervisorlaunchtype off)",
                category: Category::SystemKernel,
                risk: RiskLevel::High,
                requires_reboot: true,
                requires_admin: true,
                description: "Define bcdedit /set {current} hypervisorlaunchtype off para eliminar a camada de virtualização/VBS residual no Windows 11 24H2/25H2 (ganho real em 1% low e jogos CPU-bound). Atenção: desativa WSL2, Hyper-V, Docker Desktop e Windows Sandbox até reverter para 'auto'.",
                details: vec![
                    "bcdedit /set {current} hypervisorlaunchtype off (reversível para valor original ou auto)".into(),
                ],
            },
        }
    }

    fn query_launch_type() -> Result<Option<String>, String> {
        let out = crate::sys::cmd::run_hidden_checked("bcdedit.exe", &["/enum", "{current}"], &[0])?;
        for line in out.stdout.lines() {
            let mut parts = line.split_whitespace();
            let Some(key) = parts.next() else {
                continue;
            };
            if key.eq_ignore_ascii_case("hypervisorlaunchtype") {
                let val = parts.collect::<Vec<_>>().join(" ");
                return Ok(if val.is_empty() { None } else { Some(val) });
            }
        }
        Ok(None)
    }
}

impl Tweak for HypervisorOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        if !crate::sys::elevation::is_elevated() {
            return TweakStatus::NotApplicable("Requer Admin para verificar".into());
        }
        match Self::query_launch_type() {
            Ok(Some(v)) if v.eq_ignore_ascii_case("off") => TweakStatus::Applied,
            Ok(_) => TweakStatus::NotApplied,
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let prev = Self::query_launch_type()?;
        if prev.as_deref().is_some_and(|v| v.eq_ignore_ascii_case("off")) {
            return Ok("Hipervisor já está desativado (hypervisorlaunchtype off).".into());
        }
        let created = backups.save_if_absent(
            self.meta.id,
            serde_json::json!({ "previous": prev }),
        )?;
        if let Err(e) = crate::sys::cmd::run_hidden_checked(
            "bcdedit.exe",
            &["/set", "{current}", "hypervisorlaunchtype", "off"],
            &[0],
        ) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        Ok("hypervisorlaunchtype definido para off (reinicie para aplicar).".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de hypervisorlaunchtype encontrado.".into());
        };
        if let Some(prev) = rec.data["previous"].as_str().filter(|s| !s.is_empty()) {
            crate::sys::cmd::run_hidden_checked(
                "bcdedit.exe",
                &["/set", "{current}", "hypervisorlaunchtype", prev],
                &[0],
            )?;
        } else {
            crate::sys::cmd::run_hidden_checked(
                "bcdedit.exe",
                &["/deletevalue", "{current}", "hypervisorlaunchtype"],
                &[0],
            )?;
        }
        backups.remove(self.meta.id)?;
        Ok("hypervisorlaunchtype restaurado ao valor original.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        crate::sys::cmd::run_hidden_checked(
            "bcdedit.exe",
            &["/set", "{current}", "hypervisorlaunchtype", "auto"],
            &[0],
        )?;
        Ok("hypervisorlaunchtype restaurado para auto (padrão do Windows).".into())
    }
}

pub struct AmdDeepSleepAspmOffTweak {
    meta: TweakMeta,
}

impl AmdDeepSleepAspmOffTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "gpu.amd_deepsleep_aspm_off",
                name: "GPU AMD: Desativar DeepSleep de Shader Clock e ASPM do driver (PP_SclkDeepSleepDisable=1, EnableAspm=0)",
                category: Category::GpuLatency,
                risk: RiskLevel::Medium,
                requires_reboot: true,
                requires_admin: true,
                description: "Define PP_SclkDeepSleepDisable=1 e EnableAspm=0 na chave do driver AMD Radeon (VEN_1002), corrigindo quedas abruptas de clock em cenas leves e micro-engasgos (aumenta levemente consumo em idle). Marca automaticamente como N/A em PCs NVIDIA.",
                details: vec![
                    format!(r"HKLM\{GPU_CLASS_KEY}\00xx\PP_SclkDeepSleepDisable = 1 (VEN_1002)"),
                    format!(r"HKLM\{GPU_CLASS_KEY}\00xx\EnableAspm = 0 (VEN_1002)"),
                ],
            },
        }
    }
}

impl Tweak for AmdDeepSleepAspmOffTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let keys = AmdUlpsOffTweak::amd_keys();
        if keys.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma GPU AMD (VEN_1002) detectada".into());
        }
        let total = keys.len() * 2;
        let mut ok = 0usize;
        for k in &keys {
            if matches!(
                read_value(RegHive::Hklm, k, "PP_SclkDeepSleepDisable"),
                Ok(Some(RegValue::DWord(1)))
            ) {
                ok += 1;
            }
            if matches!(
                read_value(RegHive::Hklm, k, "EnableAspm"),
                Ok(Some(RegValue::DWord(0)))
            ) {
                ok += 1;
            }
        }
        if ok == total {
            TweakStatus::Applied
        } else if ok == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{ok}/{total} ajustes"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let keys = AmdUlpsOffTweak::amd_keys();
        if keys.is_empty() {
            return Err("Nenhuma GPU AMD (VEN_1002) instalada neste PC.".into());
        }
        let mut snap = Vec::new();
        for k in &keys {
            for name in ["PP_SclkDeepSleepDisable", "EnableAspm"] {
                let cur = read_value(RegHive::Hklm, k, name)?;
                snap.push(RegSnapshotEntry {
                    hive: RegHive::Hklm,
                    subkey: k.clone(),
                    name: name.into(),
                    existed: cur.is_some(),
                    value: cur,
                    use_backup_restore: false,
                });
            }
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let mut modified = 0usize;
        for k in &keys {
            for (name, val) in [("PP_SclkDeepSleepDisable", 1u32), ("EnableAspm", 0u32)] {
                if let Err(e) = write_value(RegHive::Hklm, k, name, &RegValue::DWord(val)) {
                    if created && modified == 0 {
                        let _ = backups.remove(self.meta.id);
                    }
                    return Err(e);
                }
                modified += 1;
            }
        }
        Ok(format!(
            "PP_SclkDeepSleepDisable=1 e EnableAspm=0 aplicados em {} GPU(s) AMD.",
            keys.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de DeepSleep/ASPM AMD encontrado.".into());
        };
        let snap: Vec<RegSnapshotEntry> =
            serde_json::from_value(rec.data).map_err(|e| e.to_string())?;
        for e in &snap {
            if e.existed {
                if let Some(v) = &e.value {
                    write_value(e.hive, &e.subkey, &e.name, v)?;
                }
            } else {
                delete_value(e.hive, &e.subkey, &e.name)?;
            }
        }
        backups.remove(self.meta.id)?;
        Ok("DeepSleep e ASPM AMD restaurados ao padrão.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        for k in AmdUlpsOffTweak::amd_keys() {
            delete_value(RegHive::Hklm, &k, "PP_SclkDeepSleepDisable")?;
            delete_value(RegHive::Hklm, &k, "EnableAspm")?;
        }
        Ok("PP_SclkDeepSleepDisable e EnableAspm removidos (padrão AMD).".into())
    }
}
