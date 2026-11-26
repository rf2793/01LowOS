use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::sys::registry::{list_subkeys, read_value, RegHive, RegValue};

fn normalize_existing_dir(p: PathBuf, out: &mut BTreeSet<PathBuf>) {
    let s = p.to_string_lossy().trim().trim_end_matches('\\').to_string();
    if s.is_empty() {
        return;
    }
    let clean = PathBuf::from(s);
    if clean.is_dir() {
        out.insert(clean);
    }
}

pub fn discover_game_folders() -> Vec<PathBuf> {
    let mut dirs = BTreeSet::new();

    // 1. Steam libraryfolders.vdf
    let vdf_candidates = [
        PathBuf::from(r"C:\Program Files (x86)\Steam\steamapps\libraryfolders.vdf"),
        PathBuf::from(r"C:\Program Files\Steam\steamapps\libraryfolders.vdf"),
    ];
    for vdf in &vdf_candidates {
        if let Ok(content) = fs::read_to_string(vdf) {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("\"path\"") {
                    let val = rest.trim().trim_matches('"').replace("\\\\", "\\");
                    if !val.is_empty() {
                        normalize_existing_dir(PathBuf::from(val).join("steamapps").join("common"), &mut dirs);
                    }
                }
            }
        }
    }

    // 2. Standard default library folders
    for static_dir in [
        r"C:\Program Files (x86)\Steam\steamapps\common",
        r"C:\Program Files\EA Games",
        r"C:\Program Files (x86)\EA Games",
        r"C:\Program Files\Epic Games",
        r"C:\Program Files (x86)\GOG Galaxy\Games",
        r"C:\Riot Games",
        r"C:\Program Files (x86)\Ubisoft\Ubisoft Game Launcher\games",
        r"C:\Program Files\Rockstar Games",
        r"C:\Amazon Games\Library",
    ] {
        normalize_existing_dir(PathBuf::from(static_dir), &mut dirs);
    }

    // 3. EA registry Install Dir -> parent
    for base in [r"SOFTWARE\WOW6432Node\EA Games", r"SOFTWARE\EA Games"] {
        for sub in list_subkeys(RegHive::Hklm, base) {
            let k = format!(r"{base}\{sub}");
            if let Ok(Some(RegValue::String(v))) = read_value(RegHive::Hklm, &k, "Install Dir") {
                if let Some(parent) = Path::new(v.trim_end_matches('\\')).parent() {
                    normalize_existing_dir(parent.to_path_buf(), &mut dirs);
                }
            }
        }
    }

    // 4. Epic manifests *.item
    let epic_manifests = Path::new(r"C:\ProgramData\Epic\EpicGamesLauncher\Data\Manifests");
    if let Ok(entries) = fs::read_dir(epic_manifests) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.extension().and_then(|s| s.to_str()) == Some("item") {
                if let Ok(raw) = fs::read_to_string(&p) {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&raw) {
                        if let Some(loc) = val["InstallLocation"].as_str() {
                            if let Some(parent) = Path::new(loc.trim_end_matches('\\')).parent() {
                                normalize_existing_dir(parent.to_path_buf(), &mut dirs);
                            }
                        }
                    }
                }
            }
        }
    }

    // 5. GOG & Ubisoft registry
    for sub in list_subkeys(RegHive::Hklm, r"SOFTWARE\WOW6432Node\GOG.com\Games") {
        let k = format!(r"SOFTWARE\WOW6432Node\GOG.com\Games\{sub}");
        if let Ok(Some(RegValue::String(v))) = read_value(RegHive::Hklm, &k, "path") {
            normalize_existing_dir(PathBuf::from(v), &mut dirs);
        }
    }
    for sub in list_subkeys(RegHive::Hklm, r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs") {
        let k = format!(r"SOFTWARE\WOW6432Node\Ubisoft\Launcher\Installs\{sub}");
        if let Ok(Some(RegValue::String(v))) = read_value(RegHive::Hklm, &k, "InstallDir") {
            normalize_existing_dir(PathBuf::from(v), &mut dirs);
        }
    }

    // 6. Rockstar registry
    for sub in list_subkeys(RegHive::Hklm, r"SOFTWARE\WOW6432Node\Rockstar Games") {
        let k = format!(r"SOFTWARE\WOW6432Node\Rockstar Games\{sub}");
        if let Ok(Some(RegValue::String(v))) = read_value(RegHive::Hklm, &k, "InstallFolder") {
            normalize_existing_dir(PathBuf::from(v), &mut dirs);
        }
    }

    // 7. Blizzard / Riot Uninstall entries
    for base in [
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
    ] {
        for sub in list_subkeys(RegHive::Hklm, base) {
            let k = format!(r"{base}\{sub}");
            let pub_str = match read_value(RegHive::Hklm, &k, "Publisher") {
                Ok(Some(RegValue::String(s))) => s,
                _ => continue,
            };
            if !(pub_str.contains("Blizzard") || pub_str.contains("Riot Games")) {
                continue;
            }
            let disp = match read_value(RegHive::Hklm, &k, "DisplayName") {
                Ok(Some(RegValue::String(s))) => s,
                _ => String::new(),
            };
            if disp.eq_ignore_ascii_case("Battle.net") || disp.eq_ignore_ascii_case("Riot Client") {
                continue;
            }
            if let Ok(Some(RegValue::String(loc))) = read_value(RegHive::Hklm, &k, "InstallLocation")
            {
                normalize_existing_dir(PathBuf::from(loc), &mut dirs);
            }
        }
    }

    // 8. Drive roots: XboxGames and Games
    for letter in b'C'..=b'Z' {
        let root = format!("{}:\\", letter as char);
        if Path::new(&root).exists() {
            normalize_existing_dir(PathBuf::from(&root).join("XboxGames"), &mut dirs);
            normalize_existing_dir(PathBuf::from(&root).join("Games"), &mut dirs);
        }
    }

    // 9. User profile Games & itch.io
    if let Some(appdata) = std::env::var_os("APPDATA") {
        normalize_existing_dir(PathBuf::from(appdata).join("itch").join("apps"), &mut dirs);
    }
    if let Some(profile) = std::env::var_os("USERPROFILE") {
        normalize_existing_dir(PathBuf::from(profile).join("Games"), &mut dirs);
    }

    // 10. Extra list files (exe dir or ProgramData)
    let mut extra_paths = vec![PathBuf::from(r"C:\ProgramData\01LowOS\pastas_jogos_extra.txt")];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            extra_paths.push(dir.join("pastas_jogos_extra.txt"));
        }
    }
    for extra in extra_paths {
        if let Ok(text) = fs::read_to_string(extra) {
            for line in text.lines() {
                let t = line.trim();
                if !t.is_empty() && !t.starts_with('#') {
                    normalize_existing_dir(PathBuf::from(t), &mut dirs);
                }
            }
        }
    }

    dirs.into_iter().collect()
}
