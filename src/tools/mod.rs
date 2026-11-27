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
        dirs.push(("AMD GLCache", la.join("AMD").join("GLCache")));
        dirs.push(("AMD VkCache", la.join("AMD").join("VkCache")));
        dirs.push(("Intel ShaderCache", la.join("Intel").join("ShaderCache")));
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
        // file_type() nao segue links: juncoes/symlinks ficam fora da contagem (evita loop e
        // contar arquivos de outra pasta).
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
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
        // Nunca entrar em juncao/symlink: a limpeza roda elevada e um link dentro de %TEMP%
        // apontando para outra pasta faria apagar arquivos fora do cache.
        let Ok(ft) = entry.file_type() else {
            skipped += 1;
            continue;
        };
        if ft.is_symlink() {
            skipped += 1;
            continue;
        }
        if ft.is_dir() {
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

fn candidate_temp_dirs() -> Vec<(&'static str, PathBuf)> {
    let mut dirs = Vec::new();
    if let Some(t) = std::env::var_os("TEMP").map(PathBuf::from) {
        dirs.push(("Temp do Usuário", t));
    }
    let win_root = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    dirs.push(("Windows\\Temp", win_root.join("Temp")));
    dirs.push((
        "Cache Windows Update",
        win_root.join("SoftwareDistribution").join("Download"),
    ));
    dirs
}

pub fn scan_temp_caches() -> Vec<ShaderCacheDir> {
    candidate_temp_dirs()
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

pub fn clean_temp_caches() -> (usize, u64, usize) {
    let mut deleted = 0usize;
    let mut freed = 0u64;
    let mut skipped = 0usize;
    for (_, path) in candidate_temp_dirs() {
        if path.is_dir() {
            let (d, f, s) = clear_dir_files(&path);
            deleted += d;
            freed += f;
            skipped += s;
        }
    }
    (deleted, freed, skipped)
}

#[derive(Debug, Clone)]
pub struct HealthItem {
    pub label: &'static str,
    pub ok: bool,
    pub detail: String,
}

fn svc_not_disabled(name: &str) -> bool {
    let subkey = format!(r"SYSTEM\CurrentControlSet\Services\{name}");
    !matches!(
        crate::sys::registry::read_value(crate::sys::registry::RegHive::Hklm, &subkey, "Start"),
        Ok(Some(crate::sys::registry::RegValue::DWord(4)))
    )
}

pub fn read_gaming_health() -> Vec<HealthItem> {
    use crate::sys::registry::{read_value, RegHive, RegValue};

    let hags = matches!(
        read_value(
            RegHive::Hklm,
            r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers",
            "HwSchMode"
        ),
        Ok(Some(RegValue::DWord(2)))
    );
    let hvci_off = !matches!(
        read_value(
            RegHive::Hklm,
            r"SYSTEM\CurrentControlSet\Control\DeviceGuard\Scenarios\HypervisorEnforcedCodeIntegrity",
            "Enabled"
        ),
        Ok(Some(RegValue::DWord(1)))
    );
    let game_mode = matches!(
        read_value(
            RegHive::Hkcu,
            r"Software\Microsoft\GameBar",
            "AutoGameModeEnabled"
        ),
        Ok(Some(RegValue::DWord(1)))
    );
    let onedrive_ok = svc_not_disabled("CldFlt");
    let exitlag_ok =
        svc_not_disabled("WinHttpAutoProxySvc") && svc_not_disabled("iphlpsvc") && svc_not_disabled("Wcmsvc");
    let ms_acc_ok = svc_not_disabled("TokenBroker") && svc_not_disabled("wlidsvc");
    let trim_ok = !matches!(
        read_value(
            RegHive::Hklm,
            r"SYSTEM\CurrentControlSet\Control\FileSystem",
            "DisableDeleteNotify"
        ),
        Ok(Some(RegValue::DWord(1)))
    );
    let power_throttling_off = matches!(
        read_value(
            RegHive::Hklm,
            r"SYSTEM\CurrentControlSet\Control\Power\PowerThrottling",
            "PowerThrottlingOff"
        ),
        Ok(Some(RegValue::DWord(1)))
    );

    vec![
        HealthItem {
            label: "HAGS",
            ok: hags,
            detail: if hags { "Ativo (2)".into() } else { "Inativo".into() },
        },
        HealthItem {
            label: "VBS / HVCI",
            ok: hvci_off,
            detail: if hvci_off { "Desligado".into() } else { "Ativo".into() },
        },
        HealthItem {
            label: "Modo de Jogo",
            ok: game_mode,
            detail: if game_mode { "Ativo".into() } else { "Inativo".into() },
        },
        HealthItem {
            label: "TRIM SSD",
            ok: trim_ok,
            detail: if trim_ok { "Ativo".into() } else { "Desativado".into() },
        },
        HealthItem {
            label: "Power Throttling",
            ok: power_throttling_off,
            detail: if power_throttling_off { "Desligado".into() } else { "Padrão Win".into() },
        },
        HealthItem {
            label: "OneDrive (CldFlt)",
            ok: onedrive_ok,
            detail: if onedrive_ok { "Preservado".into() } else { "Desativado".into() },
        },
        HealthItem {
            label: "ExitLag / VPNs",
            ok: exitlag_ok,
            detail: if exitlag_ok { "Pronto".into() } else { "Serviço bloqueado".into() },
        },
        HealthItem {
            label: "Conta Microsoft",
            ok: ms_acc_ok,
            detail: if ms_acc_ok { "Preservada".into() } else { "Serviço bloqueado".into() },
        },
    ]
}

const APPCOMPAT_LAYERS_KEY: &str =
    r"Software\Microsoft\Windows NT\CurrentVersion\AppCompatFlags\Layers";
const FSO_DISABLE_FLAG: &str = "DISABLEDXMAXIMIZEDWINDOWEDMODE";

pub fn validate_full_exe_path(input: &str) -> Result<String, String> {
    let clean = input.trim().trim_matches('"').trim().to_string();
    let bytes = clean.as_bytes();
    if bytes.len() < 7
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || (bytes[2] != b'\\' && bytes[2] != b'/')
    {
        return Err("Informe o caminho completo do .exe (ex.: C:\\Jogos\\Game\\game.exe).".into());
    }
    if !clean.to_ascii_lowercase().ends_with(".exe") {
        return Err("O arquivo precisa terminar com .exe.".into());
    }
    if !Path::new(&clean).is_file() {
        return Err(format!("Executável não encontrado em '{clean}'."));
    }
    Ok(clean)
}

fn add_fso_flag(existing: Option<&str>) -> String {
    let mut tokens: Vec<&str> = existing
        .unwrap_or("")
        .split_whitespace()
        .filter(|t| *t != "~" && !t.eq_ignore_ascii_case(FSO_DISABLE_FLAG))
        .collect();
    tokens.push(FSO_DISABLE_FLAG);
    format!("~ {}", tokens.join(" "))
}

fn remove_fso_flag(existing: &str) -> Option<String> {
    let tokens: Vec<&str> = existing
        .split_whitespace()
        .filter(|t| *t != "~" && !t.eq_ignore_ascii_case(FSO_DISABLE_FLAG))
        .collect();
    if tokens.is_empty() {
        None
    } else {
        Some(format!("~ {}", tokens.join(" ")))
    }
}

pub fn list_fso_disabled_exes() -> Vec<String> {
    use crate::sys::registry::{list_value_names, read_value, RegHive, RegValue};
    let mut list = Vec::new();
    for name in list_value_names(RegHive::Hkcu, APPCOMPAT_LAYERS_KEY) {
        if let Ok(Some(RegValue::String(val))) =
            read_value(RegHive::Hkcu, APPCOMPAT_LAYERS_KEY, &name)
        {
            if val
                .split_whitespace()
                .any(|t| t.eq_ignore_ascii_case(FSO_DISABLE_FLAG))
            {
                list.push(name);
            }
        }
    }
    list.sort();
    list
}

pub fn disable_fso_for_exe(input: &str) -> Result<String, String> {
    use crate::sys::registry::{read_value, write_value, RegHive, RegValue};
    let exe_path = validate_full_exe_path(input)?;
    let existing = match read_value(RegHive::Hkcu, APPCOMPAT_LAYERS_KEY, &exe_path)? {
        Some(RegValue::String(s)) => Some(s),
        _ => None,
    };
    let new_val = add_fso_flag(existing.as_deref());
    write_value(
        RegHive::Hkcu,
        APPCOMPAT_LAYERS_KEY,
        &exe_path,
        &RegValue::String(new_val),
    )?;
    Ok(format!(
        "Fullscreen Optimizations desativado para '{exe_path}'."
    ))
}

pub fn restore_fso_for_exe(exe_path: &str) -> Result<String, String> {
    use crate::sys::registry::{delete_value, read_value, write_value, RegHive, RegValue};
    let clean = exe_path.trim().trim_matches('"').trim();
    if let Ok(Some(RegValue::String(cur))) =
        read_value(RegHive::Hkcu, APPCOMPAT_LAYERS_KEY, clean)
    {
        if let Some(updated) = remove_fso_flag(&cur) {
            write_value(
                RegHive::Hkcu,
                APPCOMPAT_LAYERS_KEY,
                clean,
                &RegValue::String(updated),
            )?;
        } else {
            delete_value(RegHive::Hkcu, APPCOMPAT_LAYERS_KEY, clean)?;
        }
    } else {
        delete_value(RegHive::Hkcu, APPCOMPAT_LAYERS_KEY, clean)?;
    }
    Ok(format!("FSO restaurado ao padrão para '{clean}'."))
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

    #[test]
    fn cache_cleaner_never_follows_junctions() {
        let base = std::env::temp_dir().join(format!("01lowos_junction_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let cache = base.join("cache");
        let outside = base.join("fora_do_cache");
        fs::create_dir_all(cache.join("sub")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(cache.join("sub").join("lixo.bin"), b"x").unwrap();
        fs::write(outside.join("importante.txt"), b"manter").unwrap();

        let link = cache.join("link");
        let made = crate::sys::cmd::run_hidden(
            "cmd.exe",
            &["/c", "mklink", "/J", &link.to_string_lossy(), &outside.to_string_lossy()],
        )
        .map(|o| o.success())
        .unwrap_or(false);
        assert!(made, "mklink /J falhou");

        let (deleted, _, skipped) = clear_dir_files(&cache);
        assert_eq!(deleted, 1);
        assert!(skipped >= 1);
        assert!(outside.join("importante.txt").is_file(), "limpeza atravessou a juncao");
        assert_eq!(dir_stats(&cache).0, 0);

        let _ = fs::remove_dir(&link);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn fso_layer_helpers_preserve_and_clean_flags() {
        assert!(validate_full_exe_path("cs2.exe").is_err());
        assert!(validate_full_exe_path(r"C:\__nonexistent_01lowos__\game.exe").is_err());
        assert_eq!(add_fso_flag(None), "~ DISABLEDXMAXIMIZEDWINDOWEDMODE");
        assert_eq!(
            add_fso_flag(Some("~ HIGHDPIAWARE")),
            "~ HIGHDPIAWARE DISABLEDXMAXIMIZEDWINDOWEDMODE"
        );
        assert_eq!(remove_fso_flag("~ DISABLEDXMAXIMIZEDWINDOWEDMODE"), None);
        assert_eq!(
            remove_fso_flag("~ HIGHDPIAWARE DISABLEDXMAXIMIZEDWINDOWEDMODE").as_deref(),
            Some("~ HIGHDPIAWARE")
        );
    }
}
