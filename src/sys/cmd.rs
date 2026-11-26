use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone)]
pub struct CmdOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.code == 0
    }

    pub fn combined(&self) -> String {
        let out = self.stdout.trim();
        let err = self.stderr.trim();
        match (out.is_empty(), err.is_empty()) {
            (true, true) => String::new(),
            (false, true) => out.to_string(),
            (true, false) => err.to_string(),
            (false, false) => format!("{out}\n{err}"),
        }
    }
}

fn decode_windows_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    String::from_utf8_lossy(bytes).into_owned()
}

pub fn run_hidden(program: &str, args: &[&str]) -> Result<CmdOutput, String> {
    let out = Command::new(program)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("Falha ao executar {program}: {e}"))?;

    Ok(CmdOutput {
        code: out.status.code().unwrap_or(-1),
        stdout: decode_windows_output(&out.stdout),
        stderr: decode_windows_output(&out.stderr),
    })
}

pub fn run_hidden_checked(
    program: &str,
    args: &[&str],
    allowed_codes: &[i32],
) -> Result<CmdOutput, String> {
    let out = run_hidden(program, args)?;
    if !allowed_codes.contains(&out.code) {
        return Err(format!(
            "{program} {} terminou com código {}: {}",
            args.join(" "),
            out.code,
            out.combined()
        ));
    }
    Ok(out)
}

pub fn run_powershell(script: &str) -> Result<CmdOutput, String> {
    let wrapped = format!(
        "[Console]::OutputEncoding = [System.Text.Encoding]::UTF8; $OutputEncoding = [System.Text.Encoding]::UTF8; $ErrorActionPreference = 'Stop';\n{script}"
    );
    run_hidden(
        "powershell.exe",
        &["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", &wrapped],
    )
}

pub fn run_powershell_checked(script: &str) -> Result<CmdOutput, String> {
    let out = run_powershell(script)?;
    if !out.success() {
        return Err(format!(
            "PowerShell falhou (código {}): {}",
            out.code,
            out.combined()
        ));
    }
    Ok(out)
}
