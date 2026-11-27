use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::run_powershell_checked;
use crate::tweaks::{Tweak, TweakMeta, TweakStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct TaskSnapshotEntry {
    path: String,
    name: String,
    enabled: bool,
}

#[derive(Debug, Clone)]
struct MatchedTask {
    entry: TaskSnapshotEntry,
    stop: bool,
    optional: bool,
    writable: bool,
}

#[derive(Debug, Default, Deserialize)]
struct ApplyTaskOutput {
    #[serde(default)]
    disabled: Vec<TaskSnapshotEntry>,
    #[serde(default)]
    denied: usize,
    #[serde(default)]
    warn: Vec<String>,
    #[serde(default)]
    fail: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RevertTaskOutput {
    #[serde(default)]
    restored: usize,
    #[serde(default)]
    skipped: usize,
    #[serde(default)]
    fail: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct TaskPattern {
    pub full_like: String,
    pub stop_running: bool,
    pub optional: bool,
}

pub struct ScheduledTaskGroupTweak {
    meta: TweakMeta,
    patterns: Vec<TaskPattern>,
}

fn format_apply_summary(disabled: usize, total_protected: usize, warns: &[String]) -> String {
    let mut msg = if disabled > 0 {
        format!("{disabled} tarefa(s) desativada(s).")
    } else {
        "Nenhuma tarefa modificável precisou ser desativada.".to_string()
    };
    if total_protected > 0 {
        msg.push_str(&format!(
            " ({total_protected} protegida(s) por SYSTEM/TrustedInstaller no Windows 11 ignoradas)"
        ));
    }
    if !warns.is_empty() {
        msg.push_str(&format!(" Avisos: {}", warns.join(" | ")));
    }
    msg
}

impl ScheduledTaskGroupTweak {
    pub fn new(mut meta: TweakMeta, patterns: Vec<TaskPattern>) -> Self {
        if meta.details.is_empty() {
            meta.details = patterns
                .iter()
                .map(|p| {
                    format!(
                        "Desativar tarefa: {}{}",
                        p.full_like,
                        if p.optional { " (opcional)" } else { "" }
                    )
                })
                .collect();
        }
        Self { meta, patterns }
    }

    fn ps_patterns_array(&self) -> String {
        let items: Vec<String> = self
            .patterns
            .iter()
            .map(|p| {
                let escaped = p.full_like.replace('\'', "''");
                format!(
                    "@{{Pattern='{escaped}';Stop={};Optional={}}}",
                    if p.stop_running { "$true" } else { "$false" },
                    if p.optional { "$true" } else { "$false" }
                )
            })
            .collect();
        format!("@({})", items.join(","))
    }

    fn query_matching_tasks(&self) -> Result<Vec<MatchedTask>, String> {
        let arr = self.ps_patterns_array();
        let script = format!(
            r#"
$patterns = {arr}
$allowedSids = [System.Collections.Generic.HashSet[string]]::new([System.StringComparer]::OrdinalIgnoreCase)
$id = [System.Security.Principal.WindowsIdentity]::GetCurrent()
if ($id.User) {{ [void]$allowedSids.Add($id.User.Value) }}
foreach ($g in $id.Groups) {{ [void]$allowedSids.Add($g.Value) }}
[void]$allowedSids.Add('S-1-5-32-544')
$writeMask = 0x50000002

$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue)
$out = foreach ($t in $tasks) {{
    $full = $t.TaskPath + $t.TaskName
    $match = $patterns | Where-Object {{ $full -like $_.Pattern }} | Select-Object -First 1
    if ($match) {{
        $writable = $true
        if (-not [string]::IsNullOrWhiteSpace($t.SecurityDescriptor)) {{
            try {{
                $rsd = New-Object System.Security.AccessControl.RawSecurityDescriptor $t.SecurityDescriptor
                if ($null -ne $rsd.DiscretionaryAcl) {{
                    $canWrite = $false
                    $denied = $false
                    foreach ($ace in $rsd.DiscretionaryAcl) {{
                        if ($ace -is [System.Security.AccessControl.CommonAce]) {{
                            if ($allowedSids.Contains($ace.SecurityIdentifier.Value) -and (($ace.AccessMask -band $writeMask) -ne 0)) {{
                                if ($ace.AceQualifier -eq 'AccessDenied') {{ $denied = $true }}
                                elseif ($ace.AceQualifier -eq 'AccessAllowed') {{ $canWrite = $true }}
                            }}
                        }}
                    }}
                    $writable = ($canWrite -and -not $denied)
                }}
            }} catch {{
                $writable = $true
            }}
        }}
        [pscustomobject]@{{
            path = $t.TaskPath
            name = $t.TaskName
            enabled = [bool]$t.Settings.Enabled
            stop = [bool]$match.Stop
            optional = [bool]$match.Optional
            writable = [bool]$writable
        }}
    }}
}}
if ($out) {{ ConvertTo-Json -InputObject @($out) -Compress }} else {{ '[]' }}
"#
        );
        let res = run_powershell_checked(&script)?;
        let json_str = res.stdout.trim();
        if json_str.is_empty() || json_str == "[]" {
            return Ok(Vec::new());
        }

        #[derive(Deserialize)]
        struct RawRow {
            path: String,
            name: String,
            enabled: bool,
            stop: bool,
            optional: bool,
            #[serde(default = "default_true")]
            writable: bool,
        }
        fn default_true() -> bool {
            true
        }

        let rows: Vec<RawRow> = serde_json::from_str(json_str)
            .map_err(|e| format!("Falha ao interpretar tarefas ({e}): {json_str}"))?;
        Ok(rows
            .into_iter()
            .map(|r| MatchedTask {
                entry: TaskSnapshotEntry {
                    path: r.path,
                    name: r.name,
                    enabled: r.enabled,
                },
                stop: r.stop,
                optional: r.optional,
                writable: r.writable,
            })
            .collect())
    }
}

impl Tweak for ScheduledTaskGroupTweak {
    fn meta(&self) -> &TweakMeta {
        &self.meta
    }

    fn check(&self) -> TweakStatus {
        let tasks = match self.query_matching_tasks() {
            Ok(t) => t,
            Err(e) => return TweakStatus::Error(e),
        };
        if tasks.is_empty() {
            return TweakStatus::NotApplicable("Nenhuma tarefa correspondente encontrada".into());
        }
        let modifiable: Vec<&MatchedTask> = tasks.iter().filter(|t| t.writable).collect();
        let protected_enabled = tasks
            .iter()
            .filter(|t| !t.writable && t.entry.enabled)
            .count();

        if modifiable.is_empty() {
            if protected_enabled > 0 {
                return TweakStatus::NotApplicable(format!(
                    "{protected_enabled} tarefa(s) protegida(s) por SYSTEM/TrustedInstaller no Windows 11"
                ));
            }
            return TweakStatus::Applied;
        }

        let disabled = modifiable.iter().filter(|t| !t.entry.enabled).count();
        let total = modifiable.len();
        if disabled == total {
            TweakStatus::Applied
        } else if disabled == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial(format!("{disabled}/{total} desativadas"))
        }
    }

    fn apply(&self, backups: &BackupStore) -> Result<String, String> {
        let tasks = self.query_matching_tasks()?;
        if tasks.is_empty() {
            return Ok("Nenhuma tarefa correspondente instalada.".into());
        }

        let protected_enabled = tasks
            .iter()
            .filter(|t| t.entry.enabled && !t.writable)
            .count();

        let items: Vec<String> = tasks
            .iter()
            .filter(|t| t.entry.enabled && t.writable)
            .map(|t| {
                format!(
                    "@{{P='{}';N='{}';Stop={};Opt={}}}",
                    t.entry.path.replace('\'', "''"),
                    t.entry.name.replace('\'', "''"),
                    if t.stop { "$true" } else { "$false" },
                    if t.optional { "$true" } else { "$false" }
                )
            })
            .collect();

        if items.is_empty() {
            if protected_enabled > 0 {
                return Ok(format!(
                    "Todas as tarefas modificáveis já estavam desativadas ({protected_enabled} protegida(s) por SYSTEM/TrustedInstaller ignoradas)."
                ));
            }
            return Ok(format!(
                "Todas as {} tarefas já estavam desativadas.",
                tasks.len()
            ));
        }

        let script = format!(
            r#"
$list = @({items})
$disabled = [System.Collections.Generic.List[object]]::new()
$warn = [System.Collections.Generic.List[string]]::new()
$fail = [System.Collections.Generic.List[string]]::new()
$denied = 0
foreach ($x in $list) {{
    try {{
        if ($x.Stop) {{ Stop-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction SilentlyContinue }}
        Disable-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction Stop | Out-Null
        $disabled.Add([pscustomobject]@{{ path = $x.P; name = $x.N; enabled = $true }})
    }} catch {{
        $msg = $_.Exception.Message
        $isAccessDenied = ($msg -match 'Acesso negado|Access is denied|0x80070005') -or ($_.CategoryInfo.Category -eq 'PermissionDenied')
        if ($x.Opt) {{
            if ($isAccessDenied) {{ $denied++ }}
            else {{ $warn.Add("$($x.N): $msg") }}
        }} else {{
            $fail.Add("$($x.N): $msg")
        }}
    }}
}}
[pscustomobject]@{{
    disabled = $disabled.ToArray()
    denied = $denied
    warn = $warn.ToArray()
    fail = $fail.ToArray()
}} | ConvertTo-Json -Depth 4 -Compress
"#,
            items = items.join(",")
        );

        let res = run_powershell_checked(&script)?;
        let out: ApplyTaskOutput = serde_json::from_str(res.stdout.trim()).map_err(|e| {
            format!(
                "Falha ao interpretar resultado das tarefas ({e}): {}",
                res.stdout.trim()
            )
        })?;

        if !out.disabled.is_empty() {
            let json = serde_json::to_value(&out.disabled)
                .map_err(|e| format!("Falha ao serializar tarefas: {e}"))?;
            let _ = backups.save_if_absent(self.meta.id, json)?;
        }

        if !out.fail.is_empty() {
            return Err(out.fail.join("; "));
        }

        let total_protected = protected_enabled + out.denied;
        Ok(format_apply_summary(
            out.disabled.len(),
            total_protected,
            &out.warn,
        ))
    }

    fn revert(&self, backups: &BackupStore) -> Result<String, String> {
        let Some(rec) = backups.load(self.meta.id)? else {
            return Err("Nenhum backup encontrado para esta melhoria.".into());
        };
        let snapshot: Vec<TaskSnapshotEntry> = serde_json::from_value(rec.data)
            .map_err(|e| format!("Backup de tarefas inválido: {e}"))?;

        let items: Vec<String> = snapshot
            .iter()
            .map(|e| {
                format!(
                    "@{{P='{}';N='{}';En={}}}",
                    e.path.replace('\'', "''"),
                    e.name.replace('\'', "''"),
                    if e.enabled { "$true" } else { "$false" }
                )
            })
            .collect();

        if !items.is_empty() {
            let script = format!(
                r#"
$list = @({items})
$restored = 0
$skipped = 0
$fail = [System.Collections.Generic.List[string]]::new()
foreach ($x in $list) {{
    try {{
        $cur = Get-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction SilentlyContinue
        if ($null -eq $cur -or ([bool]$cur.Settings.Enabled -eq [bool]$x.En)) {{
            $skipped++
            continue
        }}
        if ($x.En) {{ Enable-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction Stop | Out-Null }}
        else {{ Disable-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction Stop | Out-Null }}
        $restored++
    }} catch {{
        $fail.Add("$($x.N): $($_.Exception.Message)")
    }}
}}
[pscustomobject]@{{
    restored = $restored
    skipped = $skipped
    fail = $fail.ToArray()
}} | ConvertTo-Json -Depth 4 -Compress
"#,
                items = items.join(",")
            );
            let res = run_powershell_checked(&script)?;
            let out: RevertTaskOutput = serde_json::from_str(res.stdout.trim()).map_err(|e| {
                format!(
                    "Falha ao interpretar restauração das tarefas ({e}): {}",
                    res.stdout.trim()
                )
            })?;
            if !out.fail.is_empty() && out.restored == 0 {
                return Err(out.fail.join("; "));
            }
            backups.remove(self.meta.id)?;
            if out.fail.is_empty() {
                if out.skipped > 0 {
                    return Ok(format!(
                        "{} tarefa(s) restaurada(s) ao estado original ({} já no estado original ou inexistente(s)).",
                        out.restored, out.skipped
                    ));
                }
                return Ok(format!(
                    "{} tarefa(s) restaurada(s) ao estado original.",
                    out.restored
                ));
            }
            return Ok(format!(
                "{} tarefa(s) restaurada(s); {} aviso(s): {}",
                out.restored,
                out.fail.len(),
                out.fail.join(" | ")
            ));
        }
        backups.remove(self.meta.id)?;
        Ok(format!(
            "{} tarefa(s) restaurada(s) ao estado original.",
            snapshot.len()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_apply_summary_handles_protected_and_disabled() {
        assert_eq!(
            format_apply_summary(3, 33, &[]),
            "3 tarefa(s) desativada(s). (33 protegida(s) por SYSTEM/TrustedInstaller no Windows 11 ignoradas)"
        );
        assert_eq!(
            format_apply_summary(0, 5, &[]),
            "Nenhuma tarefa modificável precisou ser desativada. (5 protegida(s) por SYSTEM/TrustedInstaller no Windows 11 ignoradas)"
        );
        assert_eq!(
            format_apply_summary(2, 0, &["TaskX: erro".to_string()]),
            "2 tarefa(s) desativada(s). Avisos: TaskX: erro"
        );
    }

    #[test]
    fn apply_task_output_deserializes_empty_single_and_multiple() {
        let empty: ApplyTaskOutput =
            serde_json::from_str(r#"{"disabled":[],"denied":33,"warn":[],"fail":[]}"#).unwrap();
        assert!(empty.disabled.is_empty());
        assert_eq!(empty.denied, 33);

        let single: ApplyTaskOutput = serde_json::from_str(
            r#"{"disabled":[{"path":"\\Microsoft\\Windows\\","name":"T1","enabled":true}],"denied":0,"warn":[],"fail":[]}"#,
        )
        .unwrap();
        assert_eq!(single.disabled.len(), 1);
        assert_eq!(single.disabled[0].name, "T1");

        let multi: ApplyTaskOutput = serde_json::from_str(
            r#"{"disabled":[{"path":"\\A\\","name":"T1","enabled":true},{"path":"\\B\\","name":"T2","enabled":true}],"denied":2,"warn":["w1"],"fail":[]}"#,
        )
        .unwrap();
        assert_eq!(multi.disabled.len(), 2);
        assert_eq!(multi.denied, 2);
        assert_eq!(multi.warn, vec!["w1"]);
    }
}
