use std::ffi::c_void;
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_SUCCESS, HANDLE, LUID};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use super::elevation::to_wide_null;

#[derive(Debug, Clone, Copy, Default)]
pub struct RamSnapshot {
    pub total_mb: u64,
    pub avail_mb: u64,
    pub used_mb: u64,
    pub load_pct: u32,
}

pub fn query_ram() -> Option<RamSnapshot> {
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) == 0 {
            return None;
        }
        let total_mb = status.ullTotalPhys / (1024 * 1024);
        let avail_mb = status.ullAvailPhys / (1024 * 1024);
        let used_mb = total_mb.saturating_sub(avail_mb);
        Some(RamSnapshot {
            total_mb,
            avail_mb,
            used_mb,
            load_pct: status.dwMemoryLoad,
        })
    }
}

fn enable_profile_privilege() -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err("Falha ao abrir token do processo para SeProfileSingleProcessPrivilege.".into());
        }
        let priv_w = to_wide_null("SeProfileSingleProcessPrivilege");
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        if LookupPrivilegeValueW(std::ptr::null(), priv_w.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return Err("Falha ao consultar SeProfileSingleProcessPrivilege.".into());
        }
        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };
        let ok = AdjustTokenPrivileges(token, 0, &tp, 0, null_mut(), null_mut());
        let err = GetLastError();
        CloseHandle(token);
        if ok == 0 || err != ERROR_SUCCESS {
            return Err("Privilégio SeProfileSingleProcessPrivilege recusado (execute como Administrador).".into());
        }
        Ok(())
    }
}

pub fn purge_standby_list() -> Result<String, String> {
    enable_profile_privilege()?;

    type NtSetSystemInformationFn = unsafe extern "system" fn(u32, *mut c_void, u32) -> i32;
    let ntdll = to_wide_null("ntdll.dll");
    let proc = unsafe {
        let hmod = GetModuleHandleW(ntdll.as_ptr());
        if hmod.is_null() {
            return Err("Falha ao localizar ntdll.dll.".into());
        }
        GetProcAddress(hmod, c"NtSetSystemInformation".as_ptr().cast())
    };
    let Some(func_ptr) = proc else {
        return Err("NtSetSystemInformation não encontrado em ntdll.dll.".into());
    };
    let nt_set_system_information: NtSetSystemInformationFn =
        unsafe { std::mem::transmute(func_ptr) };

    let before = query_ram().map(|r| r.avail_mb).unwrap_or(0);
    // SystemMemoryListInformation (80), MemoryPurgeStandbyList (4)
    let mut cmd: u32 = 4;
    let status = unsafe { nt_set_system_information(80, (&mut cmd as *mut u32).cast(), 4) };
    if status != 0 {
        let ustatus = status as u32;
        if ustatus == 0xC000_0061 {
            return Err(
                "Privilégio insuficiente (NTSTATUS 0xC0000061): execute o 01LowOS como Administrador."
                    .into(),
            );
        }
        return Err(format!(
            "NtSetSystemInformation(MemoryPurgeStandbyList) retornou NTSTATUS 0x{ustatus:08X}."
        ));
    }

    let after = query_ram().map(|r| r.avail_mb).unwrap_or(0);
    Ok(format!(
        "Standby List limpa (RAM disponível: {before} MB -> {after} MB)."
    ))
}
