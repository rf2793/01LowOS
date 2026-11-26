use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{
    GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

pub fn to_wide_null(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

pub fn is_elevated() -> bool {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret_len: u32 = 0;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret_len,
        );
        CloseHandle(token);
        ok != 0 && elev.TokenIsElevated != 0
    }
}

pub fn relaunch_elevated() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Falha ao localizar executável atual: {e}"))?;
    let exe_str = exe.to_string_lossy();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let args_joined = args
        .iter()
        .map(|a| {
            if a.contains(' ') {
                format!("\"{a}\"")
            } else {
                a.clone()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");

    let verb = to_wide_null("runas");
    let file = to_wide_null(&exe_str);
    let params = to_wide_null(&args_joined);
    let params_ptr = if args_joined.is_empty() {
        null()
    } else {
        params.as_ptr()
    };

    let res = unsafe {
        ShellExecuteW(
            null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params_ptr,
            null(),
            SW_SHOWNORMAL,
        )
    };
    if (res as usize) <= 32 {
        return Err(format!(
            "Elevação recusada ou falhou (código ShellExecuteW={})",
            res as usize
        ));
    }
    Ok(())
}
