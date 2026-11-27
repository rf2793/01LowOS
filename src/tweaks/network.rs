use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::run_powershell_checked;
use crate::sys::registry::{delete_value, read_value, write_value, RegHive, RegValue};
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

const AFD_PARAMS_KEY: &str = r"SYSTEM\CurrentControlSet\Services\AFD\Parameters";

#[derive(Debug, Serialize, Deserialize)]
struct RscUdpBackup {
    rsc_before: String,
    pcf_before: String,
    rsc_adapters: Vec<String>,
    fast_send_existed: bool,
    fast_send_val: Option<RegValue>,
}

pub struct NetRscUdpLatencyTweak {
    meta: TweakMeta,
}

impl NetRscUdpLatencyTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.rsc_udp_latency",
                name: "Rede: Desativar RSC, Packet Coalescing e ativar UDP FastSend (AFD)",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Desativa Receive Segment Coalescing (RSC) e Packet Coalescing Filter para eliminar atraso por agrupamento de pacotes, e define AFD\\Parameters\\FastSendDatagramThreshold=1500 para caminho rápido de datagramas UDP em jogos online.",
                details: vec![
                    "netsh int tcp set global rsc=disabled".into(),
                    "Set-NetOffloadGlobalSetting -PacketCoalescingFilter Disabled".into(),
                    "Disable-NetAdapterRsc nos adaptadores físicos".into(),
                    r"HKLM\SYSTEM\CurrentControlSet\Services\AFD\Parameters\FastSendDatagramThreshold = 1500".into(),
                ],
            },
        }
    }
}

impl Tweak for NetRscUdpLatencyTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let script = r#"
$g = Get-NetOffloadGlobalSetting -ErrorAction SilentlyContinue
if (-not $g) { Write-Output 'ERR'; exit 0 }
$rscOff = if ("$($g.ReceiveSegmentCoalescing)" -eq 'Disabled') { 1 } else { 0 }
$pcfOff = if ("$($g.PacketCoalescingFilter)" -eq 'Disabled') { 1 } else { 0 }
Write-Output "$rscOff,$pcfOff"
"#;
        let net_ok = match run_powershell_checked(script) {
            Ok(out) => {
                let s = out.stdout.trim();
                if s == "ERR" {
                    return TweakStatus::Error("Get-NetOffloadGlobalSetting indisponível".into());
                }
                let mut parts = s.split(',');
                let a: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                let b: usize = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0);
                a + b
            }
            Err(e) => return TweakStatus::Error(e),
        };
        let afd_ok = usize::from(matches!(
            read_value(RegHive::Hklm, AFD_PARAMS_KEY, "FastSendDatagramThreshold"),
            Ok(Some(RegValue::DWord(1500)))
        ));
        let total = net_ok + afd_ok;
        match total {
            3 => TweakStatus::Applied,
            0 => TweakStatus::NotApplied,
            n => TweakStatus::Partial(format!("{n}/3 ajustes")),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let query = r#"
$g = Get-NetOffloadGlobalSetting -ErrorAction Stop
$rscAdapters = @(Get-NetAdapterRsc -ErrorAction SilentlyContinue | Where-Object { $_.IPv4Enabled -or $_.IPv6Enabled } | ForEach-Object { $_.Name })
[pscustomobject]@{
    rsc_before = [string]$g.ReceiveSegmentCoalescing
    pcf_before = [string]$g.PacketCoalescingFilter
    rsc_adapters = @($rscAdapters)
} | ConvertTo-Json -Depth 3 -Compress
"#;
        let out = run_powershell_checked(query)?;
        let val: serde_json::Value =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        let cur_afd = read_value(RegHive::Hklm, AFD_PARAMS_KEY, "FastSendDatagramThreshold")?;
        let snap = RscUdpBackup {
            rsc_before: val["rsc_before"].as_str().unwrap_or("Enabled").to_string(),
            pcf_before: val["pcf_before"].as_str().unwrap_or("Enabled").to_string(),
            rsc_adapters: val["rsc_adapters"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            fast_send_existed: cur_afd.is_some(),
            fast_send_val: cur_afd,
        };
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        let apply_script = r#"
netsh int tcp set global rsc=disabled | Out-Null
Set-NetOffloadGlobalSetting -PacketCoalescingFilter Disabled -ErrorAction Stop
Get-NetAdapter -Physical -ErrorAction SilentlyContinue | ForEach-Object {
    Disable-NetAdapterRsc -Name $_.Name -ErrorAction SilentlyContinue
}
"#;
        if let Err(e) = run_powershell_checked(apply_script) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }
        write_value(
            RegHive::Hklm,
            AFD_PARAMS_KEY,
            "FastSendDatagramThreshold",
            &RegValue::DWord(1500),
        )?;
        Ok("RSC e Packet Coalescing desativados; UDP FastSend (1500) ativado.".into())
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de RSC/UDP encontrado.".into());
        };
        let snap: RscUdpBackup =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        let rsc_arg = if snap.rsc_before.eq_ignore_ascii_case("Disabled") {
            "disabled"
        } else {
            "enabled"
        };
        let pcf_arg = if snap.pcf_before.eq_ignore_ascii_case("Disabled") {
            "Disabled"
        } else {
            "Enabled"
        };
        let adapters_json = serde_json::to_string(&snap.rsc_adapters)
            .map_err(|e| e.to_string())?
            .replace('\'', "''");
        let script = format!(
            r#"
netsh int tcp set global rsc={rsc_arg} | Out-Null
Set-NetOffloadGlobalSetting -PacketCoalescingFilter {pcf_arg} -ErrorAction Stop
$nics = @((ConvertFrom-Json '{adapters_json}') | ForEach-Object {{ $_ }} | Where-Object {{ $_ }})
foreach ($n in $nics) {{
    Enable-NetAdapterRsc -Name $n -ErrorAction SilentlyContinue
}}
"#
        );
        run_powershell_checked(&script)?;
        if snap.fast_send_existed {
            if let Some(v) = &snap.fast_send_val {
                write_value(RegHive::Hklm, AFD_PARAMS_KEY, "FastSendDatagramThreshold", v)?;
            }
        } else {
            delete_value(RegHive::Hklm, AFD_PARAMS_KEY, "FastSendDatagramThreshold")?;
        }
        backups.remove(self.meta.id)?;
        Ok("RSC, Packet Coalescing e UDP FastSend restaurados.".into())
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        let script = r#"
netsh int tcp set global rsc=enabled | Out-Null
Set-NetOffloadGlobalSetting -PacketCoalescingFilter Enabled -ErrorAction Stop
Get-NetAdapter -Physical -ErrorAction SilentlyContinue | ForEach-Object {
    Enable-NetAdapterRsc -Name $_.Name -ErrorAction SilentlyContinue
}
"#;
        run_powershell_checked(script)?;
        delete_value(RegHive::Hklm, AFD_PARAMS_KEY, "FastSendDatagramThreshold")?;
        Ok("RSC, Packet Coalescing e AFD restaurados para o padrão do Windows.".into())
    }
}

const PSCHED_KEY: &str = r"SOFTWARE\Policies\Microsoft\Windows\Psched";
const QOS_NIC_PROPS: &[&str] = &["*FlowControl", "*PMARPOffload", "*PMNSOffload"];

#[derive(Debug, Serialize, Deserialize)]
struct QosNicFlowBackup {
    qos_existed: bool,
    qos_val: Option<RegValue>,
    nic_props: Vec<WolEntry>,
}

pub struct NetQosNicFlowTweak {
    meta: TweakMeta,
}

impl NetQosNicFlowTweak {
    pub fn new() -> Self {
        Self {
            meta: TweakMeta {
                id: "net.qos_nic_flow_off",
                name: "Rede: Remover limite QoS (20%) e desativar Flow Control / Offloads ARP-NS",
                category: Category::Network,
                risk: RiskLevel::Low,
                requires_reboot: true,
                requires_admin: true,
                description: "Define Psched\\NonBestEffortLimit=0 (remove a reserva padrão de 20% de banda do Packet Scheduler) e desativa *FlowControl, *PMARPOffload e *PMNSOffload nos adaptadores físicos para evitar quadros PAUSE e offloads de suspensão.",
                details: vec![
                    format!(r"HKLM\{PSCHED_KEY}\NonBestEffortLimit = 0"),
                    "Adaptadores físicos: *FlowControl = 0, *PMARPOffload = 0, *PMNSOffload = 0".into(),
                ],
            },
        }
    }
}

impl Tweak for NetQosNicFlowTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let qos_ok = usize::from(matches!(
            read_value(RegHive::Hklm, PSCHED_KEY, "NonBestEffortLimit"),
            Ok(Some(RegValue::DWord(0)))
        ));
        let keywords = QOS_NIC_PROPS
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
                    let nic_ok: usize = a.parse().unwrap_or(0);
                    let nic_total: usize = b.parse().unwrap_or(0);
                    let ok = qos_ok + nic_ok;
                    let total = 1 + nic_total;
                    if ok == total {
                        TweakStatus::Applied
                    } else if ok == 0 {
                        TweakStatus::NotApplied
                    } else {
                        TweakStatus::Partial(format!("{ok}/{total} ajustes"))
                    }
                } else {
                    TweakStatus::Error(out.stdout)
                }
            }
            Err(e) => TweakStatus::Error(e),
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let cur_qos = read_value(RegHive::Hklm, PSCHED_KEY, "NonBestEffortLimit")?;
        let keywords = QOS_NIC_PROPS
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
            $cur = [string]@($p.RegistryValue)[0]
            if ($cur -ne '0') {{ [pscustomobject]@{{ nic=$n.Name; key=$k; val=$cur }} }}
        }}
    }}
}}
if ($rows) {{ ConvertTo-Json -InputObject @($rows) -Compress }} else {{ '[]' }}
"#
        );
        let out = run_powershell_checked(&query)?;
        let nic_rows: Vec<WolEntry> =
            serde_json::from_str(out.stdout.trim()).map_err(|e| e.to_string())?;
        let snap = QosNicFlowBackup {
            qos_existed: cur_qos.is_some(),
            qos_val: cur_qos,
            nic_props: nic_rows.clone(),
        };
        let json = serde_json::to_value(&snap).map_err(|e| e.to_string())?;
        let created = backups.save_if_absent(self.meta.id, json)?;

        if let Err(e) = write_value(
            RegHive::Hklm,
            PSCHED_KEY,
            "NonBestEffortLimit",
            &RegValue::DWord(0),
        ) {
            if created {
                let _ = backups.remove(self.meta.id);
            }
            return Err(e);
        }

        if !nic_rows.is_empty() {
            let escaped = serde_json::to_string(&nic_rows)
                .map_err(|e| e.to_string())?
                .replace('\'', "''");
            let apply_script = format!(
                r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    Set-NetAdapterAdvancedProperty -Name $r.nic -RegistryKeyword $r.key -RegistryValue 0 -NoRestart -ErrorAction Stop
}}
"#
            );
            run_powershell_checked(&apply_script)?;
        }
        Ok(format!(
            "QoS NonBestEffortLimit=0 aplicado; {} propriedade(s) de Flow/Offload desativada(s).",
            nic_rows.len()
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup de QoS/Flow Control encontrado.".into());
        };
        let snap: QosNicFlowBackup =
            serde_json::from_value(rec.data).map_err(|e| format!("Backup inválido: {e}"))?;
        if snap.qos_existed {
            if let Some(v) = &snap.qos_val {
                write_value(RegHive::Hklm, PSCHED_KEY, "NonBestEffortLimit", v)?;
            }
        } else {
            delete_value(RegHive::Hklm, PSCHED_KEY, "NonBestEffortLimit")?;
        }
        if !snap.nic_props.is_empty() {
            let escaped = serde_json::to_string(&snap.nic_props)
                .map_err(|e| e.to_string())?
                .replace('\'', "''");
            let script = format!(
                r#"
$rows = @((ConvertFrom-Json '{escaped}') | ForEach-Object {{ $_ }})
foreach ($r in $rows) {{
    if (-not $r.key) {{ continue }}
    Set-NetAdapterAdvancedProperty -Name $r.nic -RegistryKeyword $r.key -RegistryValue $r.val -NoRestart -ErrorAction Stop
}}
"#
            );
            run_powershell_checked(&script)?;
        }
        backups.remove(self.meta.id)?;
        Ok(format!(
            "QoS e {} propriedade(s) da placa de rede restaurada(s).",
            snap.nic_props.len()
        ))
    }

    fn can_reset_default(&self) -> bool {
        true
    }

    fn reset_to_default(&self) -> Result<String, String> {
        delete_value(RegHive::Hklm, PSCHED_KEY, "NonBestEffortLimit")?;
        Ok("Política NonBestEffortLimit removida (padrão do Windows).".into())
    }
}
