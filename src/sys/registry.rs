use std::io::ErrorKind;
use std::ptr::{null, null_mut};

use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS,
    HANDLE, LUID,
};
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED,
    TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    HKEY_LOCAL_MACHINE, KEY_WRITE, REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ,
    REG_OPTION_BACKUP_RESTORE, REG_QWORD, REG_SZ,
};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use winreg::enums::{
    HKEY_CURRENT_USER as WR_HKCU, HKEY_LOCAL_MACHINE as WR_HKLM, KEY_READ, KEY_WRITE as WR_KEY_WRITE,
    REG_BINARY as WR_REG_BINARY, REG_DWORD as WR_REG_DWORD, REG_EXPAND_SZ as WR_REG_EXPAND_SZ,
    REG_MULTI_SZ as WR_REG_MULTI_SZ, REG_QWORD as WR_REG_QWORD, REG_SZ as WR_REG_SZ,
};
use winreg::{RegKey, RegValue as WinRegValue};

use super::elevation::to_wide_null;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegHive {
    Hklm,
    Hkcu,
}

impl RegHive {
    pub fn display_prefix(self) -> &'static str {
        match self {
            RegHive::Hklm => "HKLM",
            RegHive::Hkcu => "HKCU",
        }
    }

    fn as_winreg_hkey(self) -> winreg::HKEY {
        match self {
            RegHive::Hklm => WR_HKLM,
            RegHive::Hkcu => WR_HKCU,
        }
    }

    fn as_winsys_hkey(self) -> HKEY {
        match self {
            RegHive::Hklm => HKEY_LOCAL_MACHINE,
            RegHive::Hkcu => HKEY_CURRENT_USER,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RegValue {
    DWord(u32),
    QWord(u64),
    String(String),
    ExpandString(String),
    MultiString(Vec<String>),
    Binary(Vec<u8>),
}

impl RegValue {
    pub fn summary(&self) -> String {
        match self {
            RegValue::DWord(v) => format!("DWORD({v} / 0x{v:X})"),
            RegValue::QWord(v) => format!("QWORD({v})"),
            RegValue::String(s) => format!("\"{s}\""),
            RegValue::ExpandString(s) => format!("Expand(\"{s}\")"),
            RegValue::MultiString(v) => format!("Multi({})", v.join(", ")),
            RegValue::Binary(b) => {
                let preview: Vec<String> = b.iter().take(12).map(|x| format!("{x:02X}")).collect();
                if b.len() > 12 {
                    format!("Bin[{}... ({}B)]", preview.join(" "), b.len())
                } else {
                    format!("Bin[{}]", preview.join(" "))
                }
            }
        }
    }
}

fn decode_utf16_le_str(bytes: &[u8]) -> String {
    let mut u16s: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    while u16s.last() == Some(&0) {
        u16s.pop();
    }
    String::from_utf16_lossy(&u16s)
}

fn decode_utf16_le_multi(bytes: &[u8]) -> Vec<String> {
    let u16s: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    let mut res = Vec::new();
    let mut start = 0;
    for (i, &ch) in u16s.iter().enumerate() {
        if ch == 0 {
            if i == start {
                break;
            }
            res.push(String::from_utf16_lossy(&u16s[start..i]));
            start = i + 1;
        }
    }
    if start < u16s.len() {
        res.push(String::from_utf16_lossy(&u16s[start..]));
    }
    res
}

fn encode_utf16_le_null(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity((s.len() + 1) * 2);
    for ch in s.encode_utf16().chain(Some(0)) {
        out.extend_from_slice(&ch.to_le_bytes());
    }
    out
}

fn encode_utf16_le_multi(items: &[String]) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        for ch in item.encode_utf16().chain(Some(0)) {
            out.extend_from_slice(&ch.to_le_bytes());
        }
    }
    out.extend_from_slice(&[0, 0]);
    if items.is_empty() {
        out.extend_from_slice(&[0, 0]);
    }
    out
}

fn from_winreg(v: WinRegValue) -> Result<RegValue, String> {
    match v.vtype {
        WR_REG_DWORD => {
            if v.bytes.len() < 4 {
                return Err("REG_DWORD truncado".into());
            }
            Ok(RegValue::DWord(u32::from_le_bytes([
                v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3],
            ])))
        }
        WR_REG_QWORD => {
            if v.bytes.len() < 8 {
                return Err("REG_QWORD truncado".into());
            }
            Ok(RegValue::QWord(u64::from_le_bytes([
                v.bytes[0], v.bytes[1], v.bytes[2], v.bytes[3], v.bytes[4], v.bytes[5], v.bytes[6],
                v.bytes[7],
            ])))
        }
        WR_REG_SZ => Ok(RegValue::String(decode_utf16_le_str(&v.bytes))),
        WR_REG_EXPAND_SZ => Ok(RegValue::ExpandString(decode_utf16_le_str(&v.bytes))),
        WR_REG_MULTI_SZ => Ok(RegValue::MultiString(decode_utf16_le_multi(&v.bytes))),
        WR_REG_BINARY => Ok(RegValue::Binary(v.bytes)),
        other => Ok(RegValue::Binary({
            let _ = other;
            v.bytes
        })),
    }
}

fn to_raw_type_and_bytes(val: &RegValue) -> (u32, Vec<u8>) {
    match val {
        RegValue::DWord(d) => (REG_DWORD, d.to_le_bytes().to_vec()),
        RegValue::QWord(q) => (REG_QWORD, q.to_le_bytes().to_vec()),
        RegValue::String(s) => (REG_SZ, encode_utf16_le_null(s)),
        RegValue::ExpandString(s) => (REG_EXPAND_SZ, encode_utf16_le_null(s)),
        RegValue::MultiString(v) => (REG_MULTI_SZ, encode_utf16_le_multi(v)),
        RegValue::Binary(b) => (REG_BINARY, b.clone()),
    }
}

fn to_winreg_value(val: &RegValue) -> WinRegValue {
    match val {
        RegValue::DWord(d) => WinRegValue {
            bytes: d.to_le_bytes().to_vec(),
            vtype: WR_REG_DWORD,
        },
        RegValue::QWord(q) => WinRegValue {
            bytes: q.to_le_bytes().to_vec(),
            vtype: WR_REG_QWORD,
        },
        RegValue::String(s) => WinRegValue {
            bytes: encode_utf16_le_null(s),
            vtype: WR_REG_SZ,
        },
        RegValue::ExpandString(s) => WinRegValue {
            bytes: encode_utf16_le_null(s),
            vtype: WR_REG_EXPAND_SZ,
        },
        RegValue::MultiString(v) => WinRegValue {
            bytes: encode_utf16_le_multi(v),
            vtype: WR_REG_MULTI_SZ,
        },
        RegValue::Binary(b) => WinRegValue {
            bytes: b.clone(),
            vtype: WR_REG_BINARY,
        },
    }
}

pub fn key_exists(hive: RegHive, subkey: &str) -> bool {
    let root = RegKey::predef(hive.as_winreg_hkey());
    root.open_subkey_with_flags(subkey, KEY_READ).is_ok()
}

pub fn list_subkeys(hive: RegHive, subkey: &str) -> Vec<String> {
    let root = RegKey::predef(hive.as_winreg_hkey());
    let Ok(key) = root.open_subkey_with_flags(subkey, KEY_READ) else {
        return Vec::new();
    };
    key.enum_keys().filter_map(|r| r.ok()).collect()
}

pub fn read_value(hive: RegHive, subkey: &str, name: &str) -> Result<Option<RegValue>, String> {
    let root = RegKey::predef(hive.as_winreg_hkey());
    let key = match root.open_subkey_with_flags(subkey, KEY_READ) {
        Ok(k) => k,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "Erro ao abrir {}\\{}: {e}",
                hive.display_prefix(),
                subkey
            ))
        }
    };

    match key.get_raw_value(name) {
        Ok(raw) => from_winreg(raw).map(Some),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!(
            "Erro ao ler {}\\{}\\{}: {e}",
            hive.display_prefix(),
            subkey,
            name
        )),
    }
}

pub fn write_value(hive: RegHive, subkey: &str, name: &str, val: &RegValue) -> Result<(), String> {
    let root = RegKey::predef(hive.as_winreg_hkey());
    let (key, _) = root
        .create_subkey_with_flags(subkey, WR_KEY_WRITE)
        .map_err(|e| {
            format!(
                "Erro ao abrir/criar {}\\{}: {e}",
                hive.display_prefix(),
                subkey
            )
        })?;
    let raw = to_winreg_value(val);
    key.set_raw_value(name, &raw).map_err(|e| {
        format!(
            "Erro ao gravar {}\\{}\\{}: {e}",
            hive.display_prefix(),
            subkey,
            name
        )
    })
}

pub fn delete_value(hive: RegHive, subkey: &str, name: &str) -> Result<(), String> {
    let root = RegKey::predef(hive.as_winreg_hkey());
    let key = match root.open_subkey_with_flags(subkey, WR_KEY_WRITE) {
        Ok(k) => k,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(()),
        Err(e) => {
            return Err(format!(
                "Erro ao abrir {}\\{} para remoção: {e}",
                hive.display_prefix(),
                subkey
            ))
        }
    };
    match key.delete_value(name) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!(
            "Erro ao remover {}\\{}\\{}: {e}",
            hive.display_prefix(),
            subkey,
            name
        )),
    }
}

fn enable_privilege(priv_name: &str) -> Result<(), String> {
    unsafe {
        let mut token: HANDLE = null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return Err(format!(
                "OpenProcessToken falhou para {priv_name} (Win32={})",
                GetLastError()
            ));
        }

        let wide_name = to_wide_null(priv_name);
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        if LookupPrivilegeValueW(null(), wide_name.as_ptr(), &mut luid) == 0 {
            let err = GetLastError();
            CloseHandle(token);
            return Err(format!(
                "LookupPrivilegeValueW({priv_name}) falhou (Win32={err})"
            ));
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
            return Err(format!(
                "AdjustTokenPrivileges({priv_name}) falhou (Win32={err})"
            ));
        }
        Ok(())
    }
}

fn open_key_backup_restore(hive: RegHive, subkey: &str) -> Result<HKEY, String> {
    enable_privilege("SeBackupPrivilege")?;
    enable_privilege("SeRestorePrivilege")?;

    let wide_sub = to_wide_null(subkey);
    let mut handle: HKEY = null_mut();
    let mut disp: u32 = 0;
    let rc = unsafe {
        RegCreateKeyExW(
            hive.as_winsys_hkey(),
            wide_sub.as_ptr(),
            0,
            null(),
            REG_OPTION_BACKUP_RESTORE,
            KEY_WRITE,
            null(),
            &mut handle,
            &mut disp,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(format!(
            "RegCreateKeyExW(BACKUP_RESTORE) em {}\\{} falhou (Win32={rc})",
            hive.display_prefix(),
            subkey
        ));
    }
    Ok(handle)
}

pub fn write_value_backup_restore(
    hive: RegHive,
    subkey: &str,
    name: &str,
    val: &RegValue,
) -> Result<(), String> {
    let key = open_key_backup_restore(hive, subkey)?;
    let wide_name = to_wide_null(name);
    let (reg_type, data) = to_raw_type_and_bytes(val);
    let rc = unsafe {
        RegSetValueExW(
            key,
            wide_name.as_ptr(),
            0,
            reg_type,
            if data.is_empty() {
                null()
            } else {
                data.as_ptr()
            },
            data.len() as u32,
        )
    };
    unsafe { RegCloseKey(key) };
    if rc != ERROR_SUCCESS {
        return Err(format!(
            "RegSetValueExW(BACKUP_RESTORE) em {}\\{}\\{} falhou (Win32={rc})",
            hive.display_prefix(),
            subkey,
            name
        ));
    }
    Ok(())
}

pub fn delete_value_backup_restore(hive: RegHive, subkey: &str, name: &str) -> Result<(), String> {
    let key = open_key_backup_restore(hive, subkey)?;
    let wide_name = to_wide_null(name);
    let rc = unsafe { RegDeleteValueW(key, wide_name.as_ptr()) };
    unsafe { RegCloseKey(key) };
    if rc != ERROR_SUCCESS && rc != ERROR_FILE_NOT_FOUND && rc != ERROR_PATH_NOT_FOUND {
        return Err(format!(
            "RegDeleteValueW(BACKUP_RESTORE) em {}\\{}\\{} falhou (Win32={rc})",
            hive.display_prefix(),
            subkey,
            name
        ));
    }
    Ok(())
}
