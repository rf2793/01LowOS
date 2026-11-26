use std::fs;
use std::io::{Read, Write};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::sys::cmd::run_powershell_checked;
use crate::sys::registry::{key_exists, read_value, write_value, RegHive, RegValue};

const EMBEDDED_CATALOG: &str = include_str!("../../assets/Aplicativos_01LowOS.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AppEntry {
    pub nome: String,
    pub escopo: String,
    pub url: String,
    pub arquivo: String,
    pub tipo: String,
    pub argumentos: String,
    pub assinante: Option<String>,
    pub sha256: Option<String>,
    pub detectar: String,
    pub permitir_microfone: Option<String>,
    pub fonte: Option<String>,
}

pub fn load_catalog() -> Result<Vec<AppEntry>, String> {
    let mut candidates = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("Aplicativos_01LowOS.json"));
        }
    }
    candidates.push(PathBuf::from(r"assets\Aplicativos_01LowOS.json"));
    candidates.push(PathBuf::from(r"Projeto_ISO_Definitiva\Aplicativos_01LowOS.json"));
    candidates.push(PathBuf::from(
        r"C:\01LowOS\Projeto_ISO_Definitiva\Aplicativos_01LowOS.json",
    ));

    for path in candidates {
        if path.is_file() {
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(parsed) = serde_json::from_str::<Vec<AppEntry>>(&raw) {
                    return Ok(parsed);
                }
            }
        }
    }

    serde_json::from_str::<Vec<AppEntry>>(EMBEDDED_CATALOG)
        .map_err(|e| format!("Falha ao interpretar catálogo embutido: {e}"))
}

pub fn expand_env_vars(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            if let Some(end_rel) = chars[i + 1..].iter().position(|&c| c == '%') {
                let var_name: String = chars[i + 1..i + 1 + end_rel].iter().collect();
                if let Ok(val) = std::env::var(&var_name) {
                    out.push_str(&val);
                } else {
                    out.push('%');
                    out.push_str(&var_name);
                    out.push('%');
                }
                i += end_rel + 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn check_path_with_wildcard(pattern: &str) -> bool {
    let expanded = expand_env_vars(pattern);
    if !expanded.contains('*') {
        return Path::new(&expanded).exists();
    }
    let parts: Vec<&str> = expanded.split('\\').collect();
    let mut current_paths = vec![PathBuf::new()];
    for (idx, part) in parts.iter().enumerate() {
        if idx == 0 && part.ends_with(':') {
            current_paths = vec![PathBuf::from(format!("{part}\\"))];
            continue;
        }
        let mut next = Vec::new();
        for base in &current_paths {
            if part.contains('*') {
                if let Ok(rd) = fs::read_dir(base) {
                    let prefix = part.split('*').next().unwrap_or("");
                    let suffix = part.split('*').nth(1).unwrap_or("");
                    for entry in rd.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())
                            && name.to_ascii_lowercase().ends_with(&suffix.to_ascii_lowercase())
                        {
                            next.push(entry.path());
                        }
                    }
                }
            } else {
                let candidate = base.join(part);
                if candidate.exists() {
                    next.push(candidate);
                }
            }
        }
        current_paths = next;
        if current_paths.is_empty() {
            return false;
        }
    }
    !current_paths.is_empty()
}

pub fn detect_installed(entry: &AppEntry) -> bool {
    let rule = entry.detectar.trim();
    if rule.is_empty() {
        return false;
    }
    if let Some(reg_path) = rule.strip_prefix("registro:") {
        if let Some(sub) = reg_path.strip_prefix(r"HKLM:\") {
            return key_exists(RegHive::Hklm, sub);
        }
        if let Some(sub) = reg_path.strip_prefix(r"HKCU:\") {
            return key_exists(RegHive::Hkcu, sub);
        }
        return false;
    }
    if let Some(guid) = rule.strip_prefix("produto:") {
        for base in [
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
            r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            if key_exists(RegHive::Hklm, &format!(r"{base}\{guid}")) {
                return true;
            }
        }
        return false;
    }
    if let Some(pkg) = rule.strip_prefix("appx:") {
        let esc = pkg.replace('\'', "''");
        return run_powershell_checked(&format!(
            "[bool](Get-AppxPackage -Name '{esc}' -ErrorAction SilentlyContinue)"
        ))
        .map(|o| o.stdout.trim().eq_ignore_ascii_case("True"))
        .unwrap_or(false);
    }
    check_path_with_wildcard(rule)
}

fn verify_file_trust(entry: &AppEntry, file_path: &Path) -> Result<String, String> {
    if let Some(expected_sha) = entry.sha256.as_ref().filter(|s| !s.trim().is_empty()) {
        let mut f = fs::File::open(file_path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        let mut buf = [0u8; 65536];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let actual = hex::encode_upper(hasher.finalize());
        if actual != expected_sha.trim().to_ascii_uppercase() {
            return Err(format!(
                "SHA-256 inválido para {}: esperado {}, obtido {}",
                entry.arquivo, expected_sha, actual
            ));
        }
        return Ok("SHA-256 verificado".into());
    }

    let signer = entry
        .assinante
        .as_ref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| format!("{}: catálogo sem Assinante nem SHA-256.", entry.nome))?;

    let f_esc = file_path.to_string_lossy().replace('\'', "''");
    let s_esc = signer.replace('\'', "''");
    let script = format!(
        r#"
$sig = Get-AuthenticodeSignature -LiteralPath '{f_esc}'
if ($sig.Status -ne 'Valid') {{ throw "Assinatura inválida: $($sig.Status)" }}
if ($sig.SignerCertificate.Subject -notlike '*{s_esc}*') {{ throw "Assinante inesperado: $($sig.SignerCertificate.Subject)" }}
Write-Output 'OK'
"#
    );
    run_powershell_checked(&script)?;
    Ok(format!("Assinatura válida ({signer})"))
}

fn split_args(raw: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    for ch in raw.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            ' ' | '\t' if !in_quotes => {
                if !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                }
            }
            _ => cur.push(ch),
        }
    }
    if !cur.is_empty() {
        args.push(cur);
    }
    args
}

pub fn install_app<F: FnMut(&str)>(
    entry: &AppEntry,
    cache_dir: &Path,
    mut log: F,
) -> Result<String, String> {
    if detect_installed(entry) {
        return Ok(format!("{}: já instalado.", entry.nome));
    }

    let cached_file = cache_dir.join(&entry.arquivo);
    let temp_dir = std::env::temp_dir().join(format!("01LowOS_app_{}", std::process::id()));
    let mut cleanup_temp = false;

    let installer_path = if cached_file.is_file() {
        log(&format!(
            "{}: usando cache local {}",
            entry.nome,
            cached_file.display()
        ));
        cached_file
    } else {
        fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
        cleanup_temp = true;
        let dest = temp_dir.join(&entry.arquivo);
        log(&format!("{}: baixando de {}", entry.nome, entry.url));

        let resp = ureq::get(&entry.url)
            .timeout(Duration::from_secs(300))
            .call()
            .map_err(|e| format!("Falha no download de {}: {e}", entry.nome))?;

        let mut reader = resp.into_reader();
        let mut out_file = fs::File::create(&dest).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 65536];
        loop {
            let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            out_file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        }
        dest
    };

    let trust_res = verify_file_trust(entry, &installer_path);
    if let Err(e) = trust_res {
        if cleanup_temp {
            let _ = fs::remove_dir_all(&temp_dir);
        }
        return Err(e);
    }
    log(&format!("{}: {}", entry.nome, trust_res.unwrap()));

    let mut cmd = if entry.tipo.eq_ignore_ascii_case("msi") {
        let mut c = Command::new("msiexec.exe");
        c.arg("/i").arg(&installer_path);
        for a in split_args(&entry.argumentos) {
            c.arg(a);
        }
        c
    } else {
        let mut c = Command::new(&installer_path);
        for a in split_args(&entry.argumentos) {
            c.arg(a);
        }
        c
    };
    cmd.creation_flags(0x0800_0000);

    let status = cmd
        .status()
        .map_err(|e| format!("Falha ao iniciar instalador de {}: {e}", entry.nome))?;
    let code = status.code().unwrap_or(-1);

    let mut detected = false;
    for _ in 0..18 {
        if detect_installed(entry) {
            detected = true;
            break;
        }
        std::thread::sleep(Duration::from_secs(2));
    }

    if cleanup_temp {
        let _ = fs::remove_dir_all(&temp_dir);
    }

    if !detected {
        return Err(format!(
            "{} terminou com código {code}, mas não foi detectado após instalação.",
            entry.nome
        ));
    }

    if let Some(ref mic_pkg) = entry.permitir_microfone {
        let sub = format!(
            r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone\{mic_pkg}"
        );
        let cur = read_value(RegHive::Hkcu, &sub, "Value").ok().flatten();
        let should_set = match cur {
            None => true,
            Some(RegValue::String(ref s)) if s.eq_ignore_ascii_case("Prompt") || s.is_empty() => {
                true
            }
            _ => false,
        };
        if should_set {
            let _ = write_value(
                RegHive::Hkcu,
                &sub,
                "Value",
                &RegValue::String("Allow".into()),
            );
            log(&format!(
                "{}: permissão de microfone liberada ({mic_pkg}).",
                entry.nome
            ));
        }
    }

    Ok(format!("{} instalado com sucesso (código {code}).", entry.nome))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_apps_catalog_parses_cleanly() {
        let apps = serde_json::from_str::<Vec<AppEntry>>(EMBEDDED_CATALOG).unwrap();
        assert!(apps.len() >= 10);
        assert!(apps.iter().any(|a| a.nome == "Steam"));
        assert!(apps.iter().any(|a| a.nome == "Discord"));
        assert!(apps.iter().all(|a| !a.url.is_empty() && !a.detectar.is_empty()));
    }

    #[test]
    fn env_expansion_works() {
        let expanded = expand_env_vars("%SystemRoot%\\System32");
        assert!(!expanded.contains("%SystemRoot%"));
    }
}
