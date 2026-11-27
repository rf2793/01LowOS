use std::fs;
use std::path::{Path, PathBuf};

use crate::sys::cmd::run_powershell_checked;

const RIOT_VANGUARD_BLOCKED: &[&str] = &[
    "valorant",
    "league",
    "riotclient",
    "vgc",
    "vgtray",
    "vanguard",
    "packman",
];

pub fn normalize_exe_name(input: &str) -> Result<String, String> {
    let trimmed = input.trim().trim_matches('"').trim();
    if trimmed.is_empty() {
        return Err("Informe o nome do executável (ex.: Cyberpunk2077.exe).".into());
    }
    let base = trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).trim();
    if base.is_empty() {
        return Err("Nome de executável inválido.".into());
    }
    if !base
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' '))
    {
        return Err("Use apenas letras, números, ponto, hífen, sublinhado ou espaço.".into());
    }
    if base.to_ascii_lowercase().ends_with(".exe") {
        Ok(base.to_string())
    } else {
        Ok(format!("{base}.exe"))
    }
}

pub fn is_riot_vanguard_blocked(exe_name: &str) -> bool {
    let stem = exe_name
        .trim()
        .strip_suffix(".exe")
        .or_else(|| exe_name.trim().strip_suffix(".EXE"))
        .unwrap_or(exe_name.trim())
        .to_ascii_lowercase();
    RIOT_VANGUARD_BLOCKED
        .iter()
        .any(|pat| stem.contains(pat))
}

pub fn list_cfg_disabled_exes() -> Result<Vec<String>, String> {
    let script = r#"
$all = @(Get-ProcessMitigation -FullPolicy -ErrorAction Stop)
$out = @($all | Where-Object { $_.Cfg -and "$($_.Cfg.Enable)" -eq 'OFF' } | ForEach-Object { $_.ProcessName } | Where-Object { $_ } | Sort-Object -Unique)
if ($out.Count) { ConvertTo-Json -InputObject @($out) -Compress } else { '[]' }
"#;
    let res = run_powershell_checked(script)?;
    let list: Vec<String> = serde_json::from_str(res.stdout.trim())
        .map_err(|e| format!("Falha ao ler políticas CFG: {e}"))?;
    Ok(list)
}

pub fn disable_cfg_for_exe(input: &str) -> Result<String, String> {
    let exe = normalize_exe_name(input)?;
    if is_riot_vanguard_blocked(&exe) {
        return Err(format!(
            "Bloqueado por segurança: '{exe}' usa Riot Vanguard, que recusa abrir (erro VAN9002) se o CFG for desativado."
        ));
    }
    let escaped = exe.replace('\'', "''");
    let script = format!(
        "Set-ProcessMitigation -Name '{escaped}' -Disable CFG -ErrorAction Stop"
    );
    run_powershell_checked(&script)?;
    Ok(format!(
        "CFG desativado apenas para '{exe}' (reinicie o jogo para aplicar)."
    ))
}

pub fn restore_cfg_for_exe(exe_name: &str) -> Result<String, String> {
    let exe = normalize_exe_name(exe_name)?;
    let escaped = exe.replace('\'', "''");
    let script = format!(
        "Set-ProcessMitigation -Name '{escaped}' -Remove -Disable CFG -ErrorAction Stop"
    );
    run_powershell_checked(&script)?;
    Ok(format!("CFG restaurado ao padrão do sistema para '{exe}'."))
}

#[derive(Debug, Clone)]
pub struct ShaderCacheDir {
    pub label: String,
    pub path: PathBuf,
    pub files: usize,
    pub bytes: u64,
}

fn candidate_shader_dirs() -> Vec<(&'static str, PathBuf)> {
    let mut dirs = Vec::new();
    if let Some(la) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
        dirs.push(("NVIDIA DXCache", la.join("NVIDIA").join("DXCache")));
        dirs.push(("NVIDIA GLCache", la.join("NVIDIA").join("GLCache")));
        dirs.push(("AMD DxCache", la.join("AMD").join("DxCache")));
        dirs.push(("DirectX D3DSCache", la.join("D3DSCache")));
    }
    let pd = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    dirs.push((
        "NVIDIA NV_Cache",
        pd.join("NVIDIA Corporation").join("NV_Cache"),
    ));
    dirs
}

fn dir_stats(dir: &Path) -> (usize, u64) {
    let mut files = 0usize;
    let mut bytes = 0u64;
    let Ok(rd) = fs::read_dir(dir) else {
        return (0, 0);
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let (f, b) = dir_stats(&p);
            files += f;
            bytes += b;
        } else if let Ok(meta) = entry.metadata() {
            files += 1;
            bytes += meta.len();
        }
    }
    (files, bytes)
}

pub fn scan_shader_caches() -> Vec<ShaderCacheDir> {
    candidate_shader_dirs()
        .into_iter()
        .filter(|(_, p)| p.is_dir())
        .map(|(label, path)| {
            let (files, bytes) = dir_stats(&path);
            ShaderCacheDir {
                label: label.to_string(),
                path,
                files,
                bytes,
            }
        })
        .collect()
}

fn clear_dir_files(dir: &Path) -> (usize, u64, usize) {
    let mut deleted = 0usize;
    let mut freed = 0u64;
    let mut skipped = 0usize;
    let Ok(rd) = fs::read_dir(dir) else {
        return (0, 0, 0);
    };
    for entry in rd.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let (d, f, s) = clear_dir_files(&p);
            deleted += d;
            freed += f;
            skipped += s;
            let _ = fs::remove_dir(&p);
        } else {
            let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
            match fs::remove_file(&p) {
                Ok(()) => {
                    deleted += 1;
                    freed += len;
                }
                Err(_) => skipped += 1,
            }
        }
    }
    (deleted, freed, skipped)
}

pub fn clean_shader_caches() -> (usize, u64, usize) {
    let mut deleted = 0usize;
    let mut freed = 0u64;
    let mut skipped = 0usize;
    for (_, path) in candidate_shader_dirs() {
        if path.is_dir() {
            let (d, f, s) = clear_dir_files(&path);
            deleted += d;
            freed += f;
            skipped += s;
        }
    }
    (deleted, freed, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riot_executables_are_blocked_from_cfg_override() {
        assert!(is_riot_vanguard_blocked("VALORANT-Win64-Shipping.exe"));
        assert!(is_riot_vanguard_blocked("League of Legends.exe"));
        assert!(is_riot_vanguard_blocked("RiotClientServices.exe"));
        assert!(is_riot_vanguard_blocked("vgc.exe"));
        assert!(!is_riot_vanguard_blocked("Cyberpunk2077.exe"));
        assert!(!is_riot_vanguard_blocked("bf1.exe"));
    }

    #[test]
    fn normalize_exe_name_works() {
        assert_eq!(normalize_exe_name("bf1").unwrap(), "bf1.exe");
        assert_eq!(
            normalize_exe_name(r#""D:\Jogos\Cyberpunk2077.exe""#).unwrap(),
            "Cyberpunk2077.exe"
        );
        assert!(normalize_exe_name("   ").is_err());
        assert!(normalize_exe_name("bad;cmd").is_err());
    }
}
