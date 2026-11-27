use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    CloseHandle, BOOL, ERROR_SUCCESS, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW,
    PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
};
use windows_sys::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, GetSystemInfo, RelationProcessorCore, SYSTEM_INFO,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetPriorityClass, GetProcessAffinityMask, OpenProcess,
    QueryFullProcessImageNameW, SetPriorityClass, SetProcessAffinityMask,
    ABOVE_NORMAL_PRIORITY_CLASS, BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
    IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_QUERY_INFORMATION,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION, REALTIME_PRIORITY_CLASS,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTopology {
    pub logical_count: usize,
    pub physical_cores: usize,
    pub smt_enabled: bool,
    pub all_mask: usize,
    pub background_mask: usize,
    pub game_mask: usize,
}

impl CpuTopology {
    pub fn from_counts(logical_count: usize, physical_cores: usize) -> Self {
        let logical = logical_count.max(1).min(usize::BITS as usize);
        let smt_enabled = physical_cores > 0 && logical > physical_cores;
        let all_mask = if logical >= usize::BITS as usize {
            usize::MAX
        } else {
            (1usize << logical) - 1
        };
        let background_mask = if logical <= 1 {
            1
        } else if smt_enabled && logical >= 4 {
            0x3usize
        } else {
            0x1usize
        };
        let game_mask = all_mask & !background_mask;
        Self {
            logical_count: logical,
            physical_cores: physical_cores.max(1),
            smt_enabled,
            all_mask,
            background_mask,
            game_mask,
        }
    }
}

struct CoreRecord {
    efficiency_class: u8,
}

fn parse_processor_core_records(buf: &[u8]) -> Vec<CoreRecord> {
    const HEADER_SIZE: usize = 8;
    let mut records = Vec::new();
    let mut offset = 0usize;
    while offset + HEADER_SIZE <= buf.len() {
        let ptr = unsafe { buf.as_ptr().add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX };
        let relationship = unsafe { std::ptr::addr_of!((*ptr).Relationship).read_unaligned() };
        let size = unsafe { std::ptr::addr_of!((*ptr).Size).read_unaligned() as usize };
        if size < HEADER_SIZE || offset + size > buf.len() {
            break;
        }
        if relationship == RelationProcessorCore && size >= HEADER_SIZE + 2 {
            let efficiency_class = unsafe {
                std::ptr::addr_of!((*ptr).Anonymous.Processor.EfficiencyClass).read_unaligned()
            };
            records.push(CoreRecord { efficiency_class });
        }
        offset += size;
    }
    records
}

fn query_processor_core_records() -> Vec<CoreRecord> {
    unsafe {
        let mut len: u32 = 0;
        GetLogicalProcessorInformationEx(RelationProcessorCore, null_mut(), &mut len);
        if len == 0 {
            return Vec::new();
        }
        let mut buf = vec![0u8; len as usize];
        let ok = GetLogicalProcessorInformationEx(
            RelationProcessorCore,
            buf.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
            &mut len,
        );
        if ok == 0 {
            return Vec::new();
        }
        parse_processor_core_records(&buf[..buf.len().min(len as usize)])
    }
}

pub fn detect_cpu_topology() -> CpuTopology {
    let logical_count = unsafe {
        let mut sys_info: SYSTEM_INFO = std::mem::zeroed();
        GetSystemInfo(&mut sys_info);
        sys_info.dwNumberOfProcessors.max(1) as usize
    };

    let records = query_processor_core_records();
    let physical_cores = if records.is_empty() {
        logical_count
    } else {
        records.len()
    };

    CpuTopology::from_counts(logical_count, physical_cores)
}

pub fn is_heterogeneous_cpu() -> bool {
    let records = query_processor_core_records();
    let mut min_class: Option<u8> = None;
    let mut max_class: Option<u8> = None;
    for r in records {
        min_class = Some(min_class.map_or(r.efficiency_class, |m| m.min(r.efficiency_class)));
        max_class = Some(max_class.map_or(r.efficiency_class, |m| m.max(r.efficiency_class)));
    }
    matches!((min_class, max_class), (Some(lo), Some(hi)) if lo != hi)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcPriority {
    Idle,
    BelowNormal,
    Normal,
    AboveNormal,
    High,
    Realtime,
    Other(u32),
}

impl ProcPriority {
    pub fn from_win32(raw: u32) -> Self {
        match raw {
            IDLE_PRIORITY_CLASS => ProcPriority::Idle,
            BELOW_NORMAL_PRIORITY_CLASS => ProcPriority::BelowNormal,
            NORMAL_PRIORITY_CLASS => ProcPriority::Normal,
            ABOVE_NORMAL_PRIORITY_CLASS => ProcPriority::AboveNormal,
            HIGH_PRIORITY_CLASS => ProcPriority::High,
            REALTIME_PRIORITY_CLASS => ProcPriority::Realtime,
            other => ProcPriority::Other(other),
        }
    }

    pub fn to_win32(self) -> u32 {
        match self {
            ProcPriority::Idle => IDLE_PRIORITY_CLASS,
            ProcPriority::BelowNormal => BELOW_NORMAL_PRIORITY_CLASS,
            ProcPriority::Normal => NORMAL_PRIORITY_CLASS,
            ProcPriority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
            ProcPriority::High => HIGH_PRIORITY_CLASS,
            ProcPriority::Realtime => REALTIME_PRIORITY_CLASS,
            ProcPriority::Other(raw) => raw,
        }
    }

    pub fn is_high_or_realtime(self) -> bool {
        matches!(self, ProcPriority::High | ProcPriority::Realtime)
    }

    pub fn is_low(self) -> bool {
        matches!(self, ProcPriority::Idle | ProcPriority::BelowNormal)
    }
}

#[derive(Debug, Clone)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent_pid: u32,
    pub name_stem: String,
    pub exe_path: Option<PathBuf>,
    #[allow(dead_code)]
    pub session_id: u32,
}

fn utf16_cstr_to_string(slice: &[u16]) -> String {
    let end = slice.iter().position(|&c| c == 0).unwrap_or(slice.len());
    String::from_utf16_lossy(&slice[..end])
}

pub fn list_processes() -> Vec<ProcInfo> {
    list_processes_inner(true)
}

pub fn list_processes_fast() -> Vec<ProcInfo> {
    list_processes_inner(false)
}

fn list_processes_inner(with_paths: bool) -> Vec<ProcInfo> {
    let mut procs = Vec::new();
    unsafe {
        let snap: HANDLE = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return procs;
        }
        let mut pe: PROCESSENTRY32W = std::mem::zeroed();
        pe.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;

        if Process32FirstW(snap, &mut pe) != 0 {
            loop {
                let pid = pe.th32ProcessID;
                let exe_name = utf16_cstr_to_string(&pe.szExeFile);
                let name_stem = exe_name
                    .strip_suffix(".exe")
                    .or_else(|| exe_name.strip_suffix(".EXE"))
                    .unwrap_or(&exe_name)
                    .to_string();

                let mut session_id: u32 = 0;
                if pid > 0 {
                    let _ = ProcessIdToSessionId(pid, &mut session_id);
                }

                let exe_path = if with_paths && pid > 4 {
                    query_exe_path(pid)
                } else {
                    None
                };

                procs.push(ProcInfo {
                    pid,
                    parent_pid: pe.th32ParentProcessID,
                    name_stem,
                    exe_path,
                    session_id,
                });

                if Process32NextW(snap, &mut pe) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    procs
}

pub fn boost_current_process_temporarily(above_normal: bool) {
    unsafe {
        let pri = if above_normal {
            ABOVE_NORMAL_PRIORITY_CLASS
        } else {
            NORMAL_PRIORITY_CLASS
        };
        let _ = SetPriorityClass(GetCurrentProcess(), pri);
    }
}

fn query_exe_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok != 0 && len > 0 {
            Some(PathBuf::from(String::from_utf16_lossy(
                &buf[..len as usize],
            )))
        } else {
            None
        }
    }
}

pub fn get_process_affinity_and_priority(pid: u32) -> Option<(usize, ProcPriority)> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        let h = if h.is_null() {
            OpenProcess(PROCESS_QUERY_INFORMATION, 0, pid)
        } else {
            h
        };
        if h.is_null() {
            return None;
        }
        let mut proc_mask: usize = 0;
        let mut sys_mask: usize = 0;
        let ok_aff = GetProcessAffinityMask(h, &mut proc_mask, &mut sys_mask);
        let raw_pri = GetPriorityClass(h);
        CloseHandle(h);
        if ok_aff == 0 || raw_pri == 0 {
            return None;
        }
        Some((proc_mask, ProcPriority::from_win32(raw_pri)))
    }
}

pub fn set_process_affinity_and_priority(
    pid: u32,
    affinity: Option<usize>,
    priority: Option<ProcPriority>,
) -> Result<(), ()> {
    unsafe {
        let h = OpenProcess(PROCESS_SET_INFORMATION | PROCESS_QUERY_INFORMATION, 0, pid);
        if h.is_null() {
            return Err(());
        }
        let mut ok = true;
        if let Some(mask) = affinity {
            if mask != 0 && SetProcessAffinityMask(h, mask) == 0 {
                ok = false;
            }
        }
        if let Some(pri) = priority {
            if SetPriorityClass(h, pri.to_win32()) == 0 {
                ok = false;
            }
        }
        CloseHandle(h);
        if ok {
            Ok(())
        } else {
            Err(())
        }
    }
}

pub struct Gpu3dMonitor {
    query: isize,
    counter: isize,
}

unsafe impl Send for Gpu3dMonitor {}

impl Gpu3dMonitor {
    pub fn new() -> Option<Self> {
        unsafe {
            let mut query: isize = 0;
            if PdhOpenQueryW(null(), 0, &mut query) != ERROR_SUCCESS || query == 0 {
                return None;
            }
            let path: Vec<u16> = r"\GPU Engine(*engtype_3D)\Utilization Percentage"
                .encode_utf16()
                .chain(Some(0))
                .collect();
            let mut counter: isize = 0;
            if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) != ERROR_SUCCESS {
                PdhCloseQuery(query);
                return None;
            }
            if PdhCollectQueryData(query) != ERROR_SUCCESS {
                PdhCloseQuery(query);
                return None;
            }
            Some(Self { query, counter })
        }
    }

    pub fn sample_by_pid(&self) -> Option<HashMap<u32, f64>> {
        const PDH_INVALID_DATA: u32 = 0xC000_0BC6;
        const PDH_NO_DATA: u32 = 0x8000_07D5;
        unsafe {
            if PdhCollectQueryData(self.query) != ERROR_SUCCESS {
                return None;
            }
            let mut buf_size: u32 = 0;
            let mut item_count: u32 = 0;
            let rc = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                null_mut(),
            );
            if rc == PDH_INVALID_DATA || rc == PDH_NO_DATA {
                return Some(HashMap::new());
            }
            if rc != PDH_MORE_DATA && rc != ERROR_SUCCESS {
                return None;
            }
            if buf_size == 0 || item_count == 0 {
                return Some(HashMap::new());
            }

            let mut buf = vec![0u8; buf_size as usize];
            let rc2 = PdhGetFormattedCounterArrayW(
                self.counter,
                PDH_FMT_DOUBLE,
                &mut buf_size,
                &mut item_count,
                buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W,
            );
            if rc2 == PDH_INVALID_DATA || rc2 == PDH_NO_DATA {
                return Some(HashMap::new());
            }
            if rc2 != ERROR_SUCCESS {
                return None;
            }

            let items = std::slice::from_raw_parts(
                buf.as_ptr() as *const PDH_FMT_COUNTERVALUE_ITEM_W,
                item_count as usize,
            );
            let mut map: HashMap<u32, f64> = HashMap::new();
            for item in items {
                if item.szName.is_null() {
                    continue;
                }
                let mut len = 0usize;
                while *item.szName.add(len) != 0 && len < 512 {
                    len += 1;
                }
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(item.szName, len));
                if let Some(pid) = parse_pid_from_gpu_instance(&name) {
                    let val = item.FmtValue.Anonymous.doubleValue;
                    if val.is_finite() && val > 0.0 {
                        *map.entry(pid).or_insert(0.0) += val;
                    }
                }
            }
            Some(map)
        }
    }
}

impl Drop for Gpu3dMonitor {
    fn drop(&mut self) {
        unsafe {
            if self.query != 0 {
                PdhCloseQuery(self.query);
            }
        }
    }
}

fn parse_pid_from_gpu_instance(name: &str) -> Option<u32> {
    let rest = name.strip_prefix("pid_")?;
    let digits_end = rest.find('_').unwrap_or(rest.len());
    rest[..digits_end].parse::<u32>().ok()
}

struct WinSearchCtx<'a> {
    pids: &'a HashSet<u32>,
    found: bool,
}

unsafe extern "system" fn enum_win_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam as *mut WinSearchCtx<'_>);
    if IsWindowVisible(hwnd) == 0 {
        return 1;
    }
    let mut pid: u32 = 0;
    GetWindowThreadProcessId(hwnd, &mut pid);
    if ctx.pids.contains(&pid) {
        ctx.found = true;
        return 0;
    }
    1
}

pub fn has_visible_window_for_pids(pids: &HashSet<u32>) -> bool {
    if pids.is_empty() {
        return false;
    }
    let mut ctx = WinSearchCtx { pids, found: false };
    unsafe {
        EnumWindows(Some(enum_win_cb), &mut ctx as *mut _ as LPARAM);
    }
    ctx.found
}

/// Mutex nomeado da sessao: so um vigia por logon (GUI ou bandeja/tarefa).
/// Liberado no Drop (ou automaticamente pelo Windows se o processo morrer).
pub struct InstanceGuard(windows_sys::Win32::Foundation::HANDLE);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

/// Some(guard) se esta instancia virou a dona do vigia; None se outra ja roda.
pub fn try_acquire_watchdog_lock() -> Option<InstanceGuard> {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ALREADY_EXISTS};
    use windows_sys::Win32::System::Threading::CreateMutexW;
    let name = crate::sys::elevation::to_wide_null(r"Local\01LowOS_Vigia_Afinidade");
    unsafe {
        let h = CreateMutexW(null(), 0, name.as_ptr());
        if h.is_null() {
            return None;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            windows_sys::Win32::Foundation::CloseHandle(h);
            return None;
        }
        Some(InstanceGuard(h))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        detect_cpu_topology, parse_processor_core_records, CpuTopology, RelationProcessorCore,
    };

    #[test]
    fn cpu_topology_handles_smt_nosmt_hybrid_and_single_core() {
        let no_smt = CpuTopology::from_counts(8, 8);
        assert!(!no_smt.smt_enabled);
        assert_eq!(no_smt.background_mask, 0x1);
        assert_eq!(no_smt.game_mask, 0xFE);

        let smt = CpuTopology::from_counts(16, 8);
        assert!(smt.smt_enabled);
        assert_eq!(smt.background_mask, 0x3);
        assert_eq!(smt.game_mask, 0xFFFC);

        let hybrid = CpuTopology::from_counts(20, 14);
        assert!(hybrid.smt_enabled);
        assert_eq!(hybrid.background_mask, 0x3);
        assert_eq!(hybrid.game_mask, 0xFFFFC);

        let single = CpuTopology::from_counts(1, 1);
        assert!(!single.smt_enabled);
        assert_eq!(single.background_mask, 0x1);
    }

    #[test]
    fn parse_processor_core_records_reads_all_48_byte_records_including_last_core() {
        const RECORD_SIZE: usize = 48;
        let mut buf = vec![0u8; RECORD_SIZE * 8];
        for i in 0..8 {
            let base = i * RECORD_SIZE;
            buf[base..base + 4].copy_from_slice(&RelationProcessorCore.to_ne_bytes());
            buf[base + 4..base + 8].copy_from_slice(&(RECORD_SIZE as u32).to_ne_bytes());
            buf[base + 9] = if i == 7 { 1 } else { 0 };
        }
        let records = parse_processor_core_records(&buf);
        assert_eq!(records.len(), 8);
        assert_eq!(records[7].efficiency_class, 1);
    }

    #[test]
    fn detect_cpu_topology_live_sanity() {
        let topo = detect_cpu_topology();
        println!(
            "LIVE_TOPOLOGY logical={} physical={} smt={} bg=0x{:X} game=0x{:X}",
            topo.logical_count,
            topo.physical_cores,
            topo.smt_enabled,
            topo.background_mask,
            topo.game_mask
        );
        assert!(topo.physical_cores >= 1 && topo.physical_cores <= topo.logical_count);
    }
}
