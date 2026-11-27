use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct IsoBuildState {
    pub project_dir: Option<PathBuf>,
    pub local_build_dir_exists: bool,
    pub original_iso_path: PathBuf,
    pub original_iso_exists: bool,
    pub msu_files: Vec<String>,
    pub iso_root_extracted: bool,
    pub mount_dir_active: bool,
    pub processed_marker: bool,
    pub commit_pending_marker: bool,
    pub phase_ok: Vec<(&'static str, bool)>,
    pub output_iso_exists: bool,
}

pub fn find_project_dir() -> Option<PathBuf> {
    // A copia NTFS local vem primeiro: o build roda como SYSTEM, que nao enxerga o Google
    // Drive (Rodar_Como_SYSTEM.ps1 recusa esse volume). As demais sao so fallback.
    let mut candidates = vec![PathBuf::from(r"C:\01LowOS\Projeto_ISO_Definitiva")];
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("Projeto_ISO_Definitiva"));
        if let Some(p) = cwd.parent() {
            candidates.push(p.join("Projeto_ISO_Definitiva"));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for ancestor in exe.ancestors().take(4) {
            candidates.push(ancestor.join("Projeto_ISO_Definitiva"));
        }
    }

    candidates
        .into_iter()
        .find(|p| p.join("rebuild_iso_master.ps1").is_file())
}

pub fn inspect_iso_state() -> IsoBuildState {
    let project_dir = find_project_dir();
    let local_build_dir_exists = Path::new(r"C:\01LowOS\Projeto_ISO_Definitiva").is_dir();

    let orig_c = PathBuf::from(
        r"C:\ISO_Midia\en-us_windows_11_iot_enterprise_ltsc_2024_x64_dvd_f6b14814.iso",
    );
    let (original_iso_path, original_iso_exists) = (orig_c.clone(), orig_c.is_file());

    let mut msu_files = Vec::new();
    for dir in [Path::new(r"C:\ISO_Updates")] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|s| s.to_str()) == Some("msu") {
                    msu_files.push(p.to_string_lossy().to_string());
                }
            }
            if !msu_files.is_empty() {
                break;
            }
        }
    }

    let iso_root_extracted = Path::new(r"C:\ISO\sources\install.wim").is_file()
        || Path::new(r"C:\ISO\sources\install.esd").is_file();
    let mount_dir_active = Path::new(r"C:\ISO_Mount\Windows\System32").is_dir();
    let processed_marker = Path::new(r"C:\ISO.processado").exists();
    let commit_pending_marker = Path::new(r"C:\ISO.commit_pendente").exists();

    let phase_ok = ["02", "03", "04", "05", "06"]
        .into_iter()
        .map(|ph| {
            let ok = PathBuf::from(format!(r"C:\ISO_Mount.estado\{ph}.ok")).is_file();
            (ph, ok)
        })
        .collect();

    let output_iso_exists = project_dir
        .as_ref()
        .map(|d| d.join("WindowsDefinitivoGamer_nova.iso").is_file())
        .unwrap_or(false);

    IsoBuildState {
        project_dir,
        local_build_dir_exists,
        original_iso_path,
        original_iso_exists,
        msu_files,
        iso_root_extracted,
        mount_dir_active,
        processed_marker,
        commit_pending_marker,
        phase_ok,
        output_iso_exists,
    }
}

fn stream_powershell(project_dir: &Path, ps_command: &str, log_tx: &Sender<String>) -> Result<(), String> {
    let _ = log_tx.send(format!("Executando em {}: {ps_command}", project_dir.display()));
    let mut child = Command::new("powershell.exe")
        .current_dir(project_dir)
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", ps_command])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("Falha ao iniciar PowerShell: {e}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let tx_err = log_tx.clone();
    let err_handle = stderr.map(|se| {
        std::thread::spawn(move || {
            let reader = BufReader::new(se);
            for line in reader.lines().map_while(Result::ok) {
                let _ = tx_err.send(format!("[ERR] {line}"));
            }
        })
    });

    if let Some(so) = stdout {
        let reader = BufReader::new(so);
        for line in reader.lines().map_while(Result::ok) {
            let _ = log_tx.send(line);
        }
    }

    if let Some(h) = err_handle {
        let _ = h.join();
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if status.success() {
        let _ = log_tx.send("Comando ISO concluído com sucesso.".to_string());
        Ok(())
    } else {
        Err(format!(
            "Pipeline ISO falhou com código {}",
            status.code().unwrap_or(-1)
        ))
    }
}

pub fn run_phases(project_dir: &Path, phases: &str, log_tx: &Sender<String>) -> Result<(), String> {
    let trimmed = phases.trim();
    if trimmed.is_empty()
        || !trimmed
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == ' ')
    {
        return Err("Formato de fases inválido. Use números separados por vírgula (ex.: 05,06).".into());
    }
    let cmd = format!(r".\Rodar_Como_SYSTEM.ps1 -Comando '.\Executar_Fases.ps1 -Fases {trimmed}'");
    stream_powershell(project_dir, &cmd, log_tx)
}

pub fn run_verify(project_dir: &Path, log_tx: &Sender<String>) -> Result<(), String> {
    let cmd = r".\Rodar_Como_SYSTEM.ps1 -Comando '.\Verificar_Imagem.ps1'";
    stream_powershell(project_dir, cmd, log_tx)
}

pub fn run_full_build(project_dir: &Path, log_tx: &Sender<String>) -> Result<(), String> {
    let cmd = r".\rebuild_iso_master.ps1";
    stream_powershell(project_dir, cmd, log_tx)
}
