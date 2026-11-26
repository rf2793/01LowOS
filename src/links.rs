use std::ptr::{null, null_mut};

use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use crate::sys::elevation::to_wide_null;

pub const GITHUB_REPO_URL: &str = "https://github.com/rf2793/01LowOS";
pub const GITHUB_ISSUES_NEW_URL: &str = "https://github.com/rf2793/01LowOS/issues/new";
pub const GITHUB_SPONSORS_URL: &str = "https://github.com/sponsors/rf2793";
pub const DONATE_PIX_KEY: &str = "";

pub fn open_url(url: &str) -> Result<(), String> {
    let verb = to_wide_null("open");
    let target = to_wide_null(url);
    let res = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    };
    if (res as usize) <= 32 {
        return Err(format!(
            "Não foi possível abrir o navegador (ShellExecuteW={}).",
            res as usize
        ));
    }
    Ok(())
}

fn percent_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.as_bytes() {
        match *b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push_str("%20"),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

pub fn build_bug_report_url(
    version: &str,
    elevated: bool,
    logical: usize,
    physical: usize,
    smt: bool,
    recent_errors: &[String],
) -> String {
    let mut body = format!(
        "### Diagnóstico Automático (01LowOS)\n\
         - **Versão:** {version}\n\
         - **Admin:** {}\n\
         - **CPU:** {logical} threads ({physical} núcleos físicos, SMT: {})\n\n\
         ### Descrição do problema\n\
         Descreva o que aconteceu e qual ajuste/aba estava usando:\n\n",
        if elevated { "SIM" } else { "NÃO" },
        if smt { "ON" } else { "OFF" },
    );

    if !recent_errors.is_empty() {
        body.push_str("### Erros recentes da sessão\n```\n");
        for line in recent_errors.iter().take(5) {
            body.push_str(line);
            body.push('\n');
        }
        body.push_str("```\n");
    }

    let title = percent_encode("[Bug] ");
    let encoded_body = percent_encode(&body);
    format!(
        "{GITHUB_ISSUES_NEW_URL}?template=bug_report.yml&title={title}&body={encoded_body}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bug_report_url_encodes_properly() {
        let errs = vec!["12:00:00 [gpu.hags] ERRO: Acesso negado".to_string()];
        let url = build_bug_report_url("0.1.0", true, 8, 8, false, &errs);
        assert!(url.starts_with(GITHUB_ISSUES_NEW_URL));
        assert!(url.contains("template=bug_report.yml"));
        assert!(url.contains("0.1.0"));
        assert!(!url.contains(' '));
        assert!(!url.contains('\n'));
    }
}
