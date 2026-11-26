use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::run_powershell_checked;
use crate::sys::registry::{read_value, write_value, RegHive, RegValue};
use crate::tweaks::{Category, RiskLevel, Tweak, TweakMeta, TweakStatus};

#[derive(Debug, Serialize, Deserialize)]
struct RssBackupEntry {
    name: String,
    enabled: bool,
    base_processor_number: u32,
    max_processors: u32,
    interrupt_moderation: Option<String>,
    num_rss_queues: Option<String>,
}

pub struct NicRssTweak {
    meta: TweakMeta,
}

impl NicRssTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.rss_cpu2",
                name: "Rede: RSS fora do núcleo 0 (CPU base 2, 2 filas, moderação ON)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Configura Receive Side Scaling dos adaptadores físicos para iniciar no CPU 2 com 2 CPUs e 2 filas (mantendo Interrupt Moderation ligada para evitar tempestade de IRQs).",
                details: vec![
                    "Set-NetAdapterRss: Enabled=True, BaseProcessorNumber=2, MaxProcessors=2".into(),
                    "*NumRssQueues=2 (se suportado pelo driver)".into(),
                    "*InterruptModeration=1 (ligada)".into(),
                ],
            },
        }
    }
}

impl Tweak for NicRssTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let script = r#"
$phys = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
$rss = @(Get-NetAdapterRss -ErrorAction SilentlyContinue | Where-Object { $phys -contains $_.Name })
if (-not $rss.Count) { Write-Output 'NONE'; exit 0 }
$ok = @($rss | Where-Object { $_.Enabled -and $_.BaseProcessorNumber -eq 2 -and $_.MaxProcessors -eq 2 }).Count
Write-Output "$ok/$($rss.Count)"
"#;
        match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "NONE" {
                    TweakStatus::NotApplicable("Nenhum adaptador físico expõe RSS".into())
                } else if let Some((a, b)) = s.split_once('/') {
                    let ok: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(0);
                    if total == 0 {
                        TweakStatus::NotApplicable("Sem adaptadores RSS".into())
                    } else if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} adaptadores"))
                    }
                } else {
                    TweakStatus::Error(format!("Saída inesperada: {s}"))
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query_script = r#"
$phys = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
$rss = @(Get-NetAdapterRss -ErrorAction SilentlyContinue | Where-Object { $phys -contains $_.Name })
$rows = foreach ($r in $rss) {
    $m = Get-NetAdapterAdvancedProperty -Name $r.Name -RegistryKeyword '*InterruptModeration' -ErrorAction SilentlyContinue
    $nq = Get-NetAdapterAdvancedProperty -Name $r.Name -RegistryKeyword '*NumRssQueues' -ErrorAction SilentlyContinue
    [pscustomobject]@{
        name = $r.Name
        enabled = [bool]$r.Enabled
        base_processor_number = [uint32]$r.BaseProcessorNumber
        max_processors = [uint32]$r.MaxProcessors
        interrupt_moderation = if ($m) { [string]$m.RegistryValue[0] } else { $null }
        num_rss_queues = if ($nq) { [string]$nq.RegistryValue[0] } else { $null }
    }
}
if ($rows) { ConvertTo-Json -InputObject @($rows) -Compress } else { '[]' }
"#;
        let out = run_powershell_checked(query_script)?;
        let rows: Vec<RssBackupEntry> = serde_json::from_str(out.stdout.trim())
            .map_err(|e| format!("Falha ao ler RSS: {e}"))?;
        if rows.is_empty() {
            return Ok("Nenhum adaptador físico com RSS encontrado.".into());
        }

        let json = serde_json::to_value(&rows)
            .map_err(|e| format!("Falha ao serializar backup RSS: {e}"))?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let apply_script = r#"
$phys = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
$rss = @(Get-NetAdapterRss -ErrorAction SilentlyContinue | Where-Object { $phys -contains $_.Name })
$msgs = @()
foreach ($a in $rss) {
    $origMod = Get-NetAdapterAdvancedProperty -Name $a.Name -RegistryKeyword '*InterruptModeration' -ErrorAction SilentlyContinue
    $q = Get-NetAdapterAdvancedProperty -Name $a.Name -RegistryKeyword '*NumRssQueues' -ErrorAction SilentlyContinue
    try {
        $args2 = @{ Name=$a.Name; Enabled=$true; BaseProcessorNumber=2; MaxProcessors=2; NoRestart=$true; ErrorAction='Stop' }
        if ($q -and ($q.ValidRegistryValues -contains '2')) { $args2.NumberOfReceiveQueues = 2 }
        Set-NetAdapterRss @args2
        if ($origMod) { Set-NetAdapterAdvancedProperty -Name $a.Name -RegistryKeyword '*InterruptModeration' -RegistryValue 1 -NoRestart -ErrorAction Stop }
        $msgs += "$($a.Name): RSS base 2 / 2 CPUs"
    } catch {
        $err = $_.Exception.Message
        try {
            Set-NetAdapterRss -Name $a.Name -Enabled $a.Enabled -BaseProcessorNumber $a.BaseProcessorNumber -MaxProcessors $a.MaxProcessors -NoRestart -ErrorAction SilentlyContinue
            if ($origMod) { Set-NetAdapterAdvancedProperty -Name $a.Name -RegistryKeyword '*InterruptModeration' -RegistryValue $origMod.RegistryValue -NoRestart -ErrorAction SilentlyContinue }
            if ($q) { Set-NetAdapterAdvancedProperty -Name $a.Name -RegistryKeyword '*NumRssQueues' -RegistryValue $q.RegistryValue -NoRestart -ErrorAction SilentlyContinue }
        } catch {}
        throw "$($a.Name): $err"
    }
}
Write-Output ($msgs -join '; ')
"#;
        match run_powershell_checked(apply_script) {
            Ok(res) => Ok(res.stdout.trim().to_string()),
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
            return Err("Nenhum backup RSS encontrado.".into());
        };
        let rows: Vec<RssBackupEntry> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup RSS inválido: {e}"))?;
        let json = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
        let escaped = json.replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($a in $rows) {{
    if (-not $a.name) {{ continue }}
    Set-NetAdapterRss -Name $a.name -Enabled ([bool]$a.enabled) -BaseProcessorNumber $a.base_processor_number -MaxProcessors $a.max_processors -NoRestart -ErrorAction Stop
    if ($null -ne $a.num_rss_queues) {{
        Set-NetAdapterAdvancedProperty -Name $a.name -RegistryKeyword '*NumRssQueues' -RegistryValue $a.num_rss_queues -NoRestart -ErrorAction Stop
    }}
    if ($null -ne $a.interrupt_moderation) {{
        Set-NetAdapterAdvancedProperty -Name $a.name -RegistryKeyword '*InterruptModeration' -RegistryValue $a.interrupt_moderation -NoRestart -ErrorAction Stop
    }}
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("RSS restaurado em {} adaptador(es).", rows.len()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WolEntry {
    nic: String,
    key: String,
    val: String,
}

const WOL_PROPS: &[&str] = &[
    "*WakeOnMagicPacket",
    "*WakeOnPattern",
    "S5WakeOnLan",
    "WakeOnMagicPacketFromS5",
    "*EEE",
    "AdvancedEEE",
    "EnableGreenEthernet",
    "PowerSavingMode",
];

pub struct NicWolEeeTweak {
    meta: TweakMeta,
}

impl NicWolEeeTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.wol_eee_off",
                name: "Rede: Desativar Wake-on-LAN e EEE (Energy Efficient Ethernet)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa economia de energia do link (802.3az EEE / Green Ethernet) e Wake-on-LAN nos adaptadores físicos para evitar latência de saída do modo Low-Power-Idle.",
                details: WOL_PROPS
                    .iter()
                    .map(|p| format!("Adaptadores físicos: {p} = 0"))
                    .collect(),
            },
        }
    }
}

impl Tweak for NicWolEeeTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let keywords = WOL_PROPS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            r#"
$keys = @({keywords})
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue)
$total = 0; $zero = 0
foreach ($n in $nics) {{
    foreach ($k in $keys) {{
        $p = Get-NetAdapterAdvancedProperty -Name $n.Name -RegistryKeyword $k -ErrorAction SilentlyContinue
        if ($p -and (-not $p.ValidRegistryValues -or $p.ValidRegistryValues -contains '0')) {{
            $total++
            if ([string]@($p.RegistryValue)[0] -eq '0') {{ $zero++ }}
        }}
    }}
}}
Write-Output "$zero/$total"
"#
        );
        match run_powershell_checked(&script) {
            Ok(out) => {
                if let Some((a, b)) = out.stdout.trim().split_once('/') {
                    let ok: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(0);
                    if total == 0 {
                        TweakStatus::NotApplicable("Sem propriedades WOL/EEE compatíveis".into())
                    } else if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} propriedades"))
                    }
                } else {
                    TweakStatus::Error(out.stdout)
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let keywords = WOL_PROPS
            .iter()
            .map(|k| format!("'{k}'"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            r#"
$keys = @({keywords})
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue)
$rows = foreach ($n in $nics) {{
    foreach ($k in $keys) {{
        $p = Get-NetAdapterAdvancedProperty -Name $n.Name -RegistryKeyword $k -ErrorAction SilentlyContinue
        if ($p -and (-not $p.ValidRegistryValues -or $p.ValidRegistryValues -contains '0')) {{
            [pscustomobject]@{{ nic = $n.Name; key = $k; val = [string]@($p.RegistryValue)[0] }}
        }}
    }}
}}
if ($rows) {{ ConvertTo-Json -InputObject @($rows) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let rows: Vec<WolEntry> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok("Nenhuma propriedade WOL/EEE encontrada para ajustar.".into());
        }
        let json = serde_json::to_value(&rows).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let apply = format!(
            r#"
$keys = @({keywords})
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue)
$cnt = 0
foreach ($n in $nics) {{
    foreach ($k in $keys) {{
        $p = Get-NetAdapterAdvancedProperty -Name $n.Name -RegistryKeyword $k -ErrorAction SilentlyContinue
        if ($p -and (-not $p.ValidRegistryValues -or $p.ValidRegistryValues -contains '0')) {{
            try {{
                Set-NetAdapterAdvancedProperty -Name $n.Name -RegistryKeyword $k -RegistryValue 0 -NoRestart -ErrorAction Stop
                $cnt++
            }} catch {{
                if ($_.Exception.Message -notmatch 'ObjectNotFound|Nenhum objeto') {{ throw }}
            }}
        }}
    }}
}}
Write-Output $cnt
"#
        );
        match run_powershell_checked(&apply) {
            Ok(r) => Ok(format!("WOL/EEE desativado em {} propriedade(s).", r.stdout.trim())),
            Err(e) => {
                let _ = created;
                Err(e)
            }
        }
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup WOL/EEE encontrado.".into());
        };
        let rows: Vec<WolEntry> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        let json = serde_json::to_string(&rows).map_err(|e| e.to_string())?;
        let escaped = json.replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    if (-not $r.key) {{ continue }}
    try {{
        Set-NetAdapterAdvancedProperty -Name $r.nic -RegistryKeyword $r.key -RegistryValue $r.val -NoRestart -ErrorAction Stop
    }} catch {{}}
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("WOL/EEE restaurado ({} itens).", rows.len()))
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct BindEntry {
    nic: String,
    component: String,
}

const BIND_IDS: &[&str] = &[
    "ms_lltdio",
    "ms_rspndr",
    "ms_lldp",
    "ms_msclient",
    "ms_server",
];

pub struct NicBindingsTweak {
    meta: TweakMeta,
}

impl NicBindingsTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.bindings_off",
                name: "Rede: Desativar bindings LLTD, LLDP e SMB no adaptador",
                category: Category::Network,
                risk: RiskLevel::Medium,
                requires_reboot: false,
                requires_admin: true,
                description: "Desativa vínculos LLTD, LLDP e cliente/servidor SMB (ms_lltdio, ms_rspndr, ms_lldp, ms_msclient e ms_server) nos adaptadores físicos, mantendo filtros NDIS essenciais. Atenção: compartilhamento de pastas SMB na rede local para de funcionar até reverter.",
                details: BIND_IDS
                    .iter()
                    .map(|b| format!("Desativar binding: {b}"))
                    .collect(),
            },
        }
    }
}

impl Tweak for NicBindingsTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let ids = BIND_IDS
            .iter()
            .map(|c| format!("'{c}'"))
            .collect::<Vec<_>>()
            .join(",");
        let script = format!(
            r#"
$ids = @({ids})
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue)
$total = 0; $off = 0
foreach ($n in $nics) {{
    foreach ($c in $ids) {{
        $b = Get-NetAdapterBinding -Name $n.Name -ComponentID $c -ErrorAction SilentlyContinue
        if ($b) {{
            $total++
            if (-not $b.Enabled) {{ $off++ }}
        }}
    }}
}}
Write-Output "$off/$total"
"#
        );
        match run_powershell_checked(&script) {
            Ok(out) => {
                if let Some((a, b)) = out.stdout.trim().split_once('/') {
                    let ok: usize = a.parse().unwrap_or(0);
                    let total: usize = b.parse().unwrap_or(0);
                    if total == 0 {
                        TweakStatus::NotApplicable("Nenhum adaptador físico".into())
                    } else if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} bindings"))
                    }
                } else {
                    TweakStatus::Error(out.stdout)
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let ids = BIND_IDS
            .iter()
            .map(|c| format!("'{c}'"))
            .collect::<Vec<_>>()
            .join(",");
        let query = format!(
            r#"
$ids = @({ids})
$nics = @(Get-NetAdapter -Physical -ErrorAction SilentlyContinue)
$rows = foreach ($n in $nics) {{
    foreach ($c in $ids) {{
        $b = Get-NetAdapterBinding -Name $n.Name -ComponentID $c -ErrorAction SilentlyContinue
        if ($b -and $b.Enabled) {{
            [pscustomobject]@{{ nic = $n.Name; component = $c }}
        }}
    }}
}}
if ($rows) {{ ConvertTo-Json -InputObject @($rows) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let rows: Vec<BindEntry> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        if rows.is_empty() {
            return Ok("Todos os bindings alvo já estavam desativados.".into());
        }
        let json = serde_json::to_value(&rows).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let apply = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    Disable-NetAdapterBinding -Name $r.nic -ComponentID $r.component -ErrorAction Stop
}}
"#
        );
        match run_powershell_checked(&apply) {
            Ok(_) => Ok(format!("{} binding(s) desativado(s).", rows.len())),
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
            return Err("Nenhum backup de bindings encontrado.".into());
        };
        let rows: Vec<BindEntry> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        let escaped = serde_json::to_string(&rows)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    if (-not $r.component) {{ continue }}
    Enable-NetAdapterBinding -Name $r.nic -ComponentID $r.component -ErrorAction Stop
}}
"#
        );
        run_powershell_checked(&script)?;
        backups.remove(self.meta.id)?;
        Ok(format!("{} binding(s) reativado(s).", rows.len()))
    }
}

const WININET_CONN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings\Connections";

#[derive(Debug, Serialize, Deserialize)]
struct WinInetEntry {
    name: String,
    existed: bool,
    bytes: Option<Vec<u8>>,
}

pub struct WinInetWpadTweak {
    meta: TweakMeta,
}

impl WinInetWpadTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.wininet_wpad_off",
                name: "Rede: Desativar detecção automática de proxy WinINet (HKCU)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: false,
                requires_admin: false,
                description: "Limpa o bit 0x08 do byte 8 em DefaultConnectionSettings e SavedLegacySettings (desliga 'Detectar automaticamente as configurações' de WPAD sem sobrescrever o restante do blob).",
                details: vec![
                    format!(r"HKCU\{WININET_CONN_KEY}\DefaultConnectionSettings (byte[8] &= 0xF7)"),
                    format!(r"HKCU\{WININET_CONN_KEY}\SavedLegacySettings (byte[8] &= 0xF7)"),
                ],
            },
        }
    }
}

impl Tweak for WinInetWpadTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let mut checked = 0usize;
        let mut clean = 0usize;
        for val_name in ["DefaultConnectionSettings", "SavedLegacySettings"] {
            match read_value(RegHive::Hkcu, WININET_CONN_KEY, val_name) {
                Ok(Some(RegValue::Binary(b))) if b.len() > 8 => {
                    checked += 1;
                    if (b[8] & 0x08) == 0 {
                        clean += 1;
                    }
                }
                Ok(_) => {}
                Err(e) => return TweakStatus::Error(e),
            }
        }
        if checked == 0 {
            TweakStatus::NotApplicable("Blobs WinINet ausentes neste perfil".into())
        } else if clean == checked {
            TweakStatus::Applied
        } else if clean == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{clean}/{checked} entradas"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let mut snap = Vec::new();
        let mut updated = 0usize;
        for val_name in ["DefaultConnectionSettings", "SavedLegacySettings"] {
            let cur = read_value(RegHive::Hkcu, WININET_CONN_KEY, val_name)?;
            let orig_bytes = match cur {
                Some(RegValue::Binary(b)) => Some(b),
                _ => None,
            };
            snap.push(WinInetEntry {
                name: val_name.to_string(),
                existed: orig_bytes.is_some(),
                bytes: orig_bytes.clone(),
            });
        }
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        backups.save_if_absent(self.meta.id, json)?;

        for entry in &snap {
            if let Some(mut b) = entry.bytes.clone() {
                if b.len() > 8 && (b[8] & 0x08) != 0 {
                    b[8] &= 0xF7;
                    write_value(RegHive::Hkcu, WININET_CONN_KEY, &entry.name, &RegValue::Binary(b))?;
                    updated += 1;
                }
            }
        }
        Ok(format!("WinINet WPAD limpo ({updated} blob(s) atualizado(s))."))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup WinINet encontrado.".into());
        };
        let snap: Vec<WinInetEntry> =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        for entry in &snap {
            if entry.existed {
                if let Some(b) = &entry.bytes {
                    write_value(
                        RegHive::Hkcu,
                        WININET_CONN_KEY,
                        &entry.name,
                        &RegValue::Binary(b.clone()),
                    )?;
                }
            }
        }
        backups.remove(self.meta.id)?;
        Ok("Blobs WinINet originais restaurados.".into())
    }
}
