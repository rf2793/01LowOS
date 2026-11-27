use serde::{Deserialize, Serialize};

use crate::sys::backup::BackupStore;
use crate::sys::cmd::run_powershell_checked;
use crate::tweaks::{Tweak, TweakMeta, TweakStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TaskSnapshotEntry {
    path: String,
    name: String,
    enabled: bool,
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

    fn query_matching_tasks(&self) -> Result<Vec<(TaskSnapshotEntry, bool, bool)>, String> {
        let arr = self.ps_patterns_array();
        let script = format!(
            r#"
$patterns = {arr}
$tasks = @(Get-ScheduledTask -ErrorAction SilentlyContinue)
$out = foreach ($t in $tasks) {{
    $full = $t.TaskPath + $t.TaskName
    $match = $patterns | Where-Object {{ $full -like $_.Pattern }} | Select-Object -First 1
    if ($match) {{
        [pscustomobject]@{{
            path = $t.TaskPath
            name = $t.TaskName
            enabled = [bool]$t.Settings.Enabled
            stop = [bool]$match.Stop
            optional = [bool]$match.Optional
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
        }

        let rows: Vec<RawRow> = serde_json::from_str(json_str)
            .map_err(|e| format!("Falha ao interpretar tarefas ({e}): {json_str}"))?;
        Ok(rows
            .into_iter()
            .map(|r| {
                (
                    TaskSnapshotEntry {
                        path: r.path,
                        name: r.name,
                        enabled: r.enabled,
                    },
                    r.stop,
                    r.optional,
                )
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
        let disabled = tasks.iter().filter(|(t, _, _)| !t.enabled).count();
        let total = tasks.len();
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

        let items: Vec<String> = tasks
            .iter()
            .filter(|(e, _, _)| e.enabled)
            .map(|(e, stop, opt)| {
                format!(
                    "@{{P='{}';N='{}';Stop={};Opt={}}}",
                    e.path.replace('\'', "''"),
                    e.name.replace('\'', "''"),
                    if *stop { "$true" } else { "$false" },
                    if *opt { "$true" } else { "$false" }
                )
            })
            .collect();

        if items.is_empty() {
            return Ok(format!(
                "Todas as {} tarefas já estavam desativadas.",
                tasks.len()
            ));
        }

        let snapshot: Vec<TaskSnapshotEntry> = tasks.iter().map(|(e, _, _)| e.clone()).collect();
        let json = serde_json::to_value(&snapshot)
            .map_err(|e| format!("Falha ao serializar tarefas: {e}"))?;
        let created_backup = backups.save_if_absent(self.meta.id, json)?;

        let script = format!(
            r#"
$list = @({items})
$fail = @()
$warn = @()
$ok = 0
foreach ($x in $list) {{
    try {{
        if ($x.Stop) {{ Stop-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction SilentlyContinue }}
        Disable-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction Stop | Out-Null
        $ok++
    }} catch {{
        if ($x.Opt) {{ $warn += "$($x.N): $($_.Exception.Message)" }}
        else {{ $fail += "$($x.N): $($_.Exception.Message)" }}
    }}
}}
if ($fail.Count) {{ throw ($fail -join '; ') }}
Write-Output ("OK=$ok WARN=" + ($warn -join ' | '))
"#,
            items = items.join(",")
        );

        match run_powershell_checked(&script) {
            Ok(out) => Ok(format!(
                "Tarefas processadas ({}). {}",
                items.len(),
                out.stdout.trim()
            )),
            Err(e) => {
                if created_backup {
                    let _ = backups.remove(self.meta.id);
                }
                Err(e)
            }
        }
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
$fail = @()
foreach ($x in $list) {{
    try {{
        if ($x.En) {{ Enable-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction Stop | Out-Null }}
        else {{ Disable-ScheduledTask -TaskPath $x.P -TaskName $x.N -ErrorAction Stop | Out-Null }}
    }} catch {{
        $fail += "$($x.N): $($_.Exception.Message)"
    }}
}}
if ($fail.Count) {{ Write-Warning ($fail -join '; ') }}
"#,
                items = items.join(",")
            );
            run_powershell_checked(&script)?;
        }
        backups.remove(self.meta.id)?;
        Ok(format!(
            "{} tarefa(s) restaurada(s) ao estado original.",
            snapshot.len()
        ))
    }
}
