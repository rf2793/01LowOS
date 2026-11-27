use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::Local;

use super::elevation::is_elevated;

const MAX_LOG_BYTES: u64 = 2 * 1024 * 1024;

fn programdata_log_path() -> PathBuf {
    let base = std::env::var_os("ProgramData")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
    base.join("01LowOS").join("01LowOS.log")
}

fn localappdata_log_path() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("01LowOS").join("01LowOS.log")
}

pub fn log_file_path() -> PathBuf {
    if is_elevated() {
        programdata_log_path()
    } else {
        localappdata_log_path()
    }
}

fn rotate_if_large(path: &Path) {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() >= MAX_LOG_BYTES {
            let mut old = path.to_path_buf();
            old.set_extension("log.old");
            let _ = fs::remove_file(&old);
            let _ = fs::rename(path, &old);
        }
    }
}

fn write_line_to(path: &Path, formatted: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    rotate_if_large(path);
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(formatted.as_bytes())
}

pub fn append_log(line: &str) {
    let formatted = format!("{} {line}\r\n", Local::now().format("%Y-%m-%d %H:%M:%S"));
    let primary = log_file_path();
    if write_line_to(&primary, &formatted).is_err() {
        let fallback = localappdata_log_path();
        if fallback != primary {
            let _ = write_line_to(&fallback, &formatted);
        }
    }
}

pub fn open_log_file() -> Result<String, String> {
    let path = log_file_path();
    if !path.exists() {
        let _ = write_line_to(
            &path,
            &format!(
                "{} 01LowOS log inicializado.\r\n",
                Local::now().format("%Y-%m-%d %H:%M:%S")
            ),
        );
    }
    Command::new("notepad.exe")
        .arg(&path)
        .spawn()
        .map_err(|e| format!("Falha ao abrir log ({}): {e}", path.display()))?;
    Ok(format!("Log aberto: {}", path.display()))
}
