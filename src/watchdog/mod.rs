pub mod folders;
pub mod win32;

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::sys::cmd::run_powershell_checked;
use folders::discover_game_folders;
use win32::{
    detect_cpu_topology, get_process_affinity_and_priority, has_visible_window_for_pids,
    list_processes, set_process_affinity_and_priority, CpuTopology, Gpu3dMonitor, ProcInfo,
    ProcPriority,
};

pub const TASK_NAME: &str = "01LowOS_Vigia_Rust";
pub const TASK_PATH: &str = r"\01LowOS\";

pub const HEIRS: &[&str] = &[
    "explorer",
    "steam",
    "EADesktop",
    "EpicGamesLauncher",
    "GalaxyClient",
    "upc",
    "UbisoftConnect",
    "Battle.net",
    "RiotClientServices",
    "XboxPcApp",
];

pub const IDLE_BG: &[&str] = &[
    "EACefSubProcess",
    "steamwebhelper",
    "claude",
    "GoogleDriveFS",
    "SCSKiller",
    "RadeonSoftware",
    "msedgewebview2",
    "GoogleUpdater",
    "Adguard",
    "MSIAfterburner",
    "rzappengine",
    "RazerAppEngine",
    "OneDrive",
    "Blitz",
    "Overwolf",
    "OverwolfBrowser",
    "chrome",
    "msedge",
    "brave",
    "opera",
    "firefox",
];

pub const BELOW_NORMAL_BG: &[&str] = &["EALocalHostSvc", "Discord", "Spotify"];

pub const PROTECTED_CORE0: &[&str] = &[
    "csrss",
    "winlogon",
    "wininit",
    "dwm",
    "fontdrvhost",
    "sihost",
    "ctfmon",
    "TextInputHost",
    "LogonUI",
    "userinit",
    "smss",
    "lsass",
    "services",
    "svchost",
    "dllhost",
    "RuntimeBroker",
    "conhost",
    "audiodg",
    "SteamService",
    "gameoverlayui",
    "gameoverlayui64",
    "SteamOverlayUI",
    "ShellExperienceHost",
    "StartMenuExperienceHost",
    "SearchHost",
    "ApplicationFrameHost",
    "WUDFHost",
    "atieclxx",
    "AdguardSvc",
    "WmiPrvSE",
    "EADesktop",
];

pub const NEVER_TOUCH: &[&str] = &[
    "System",
    "Idle",
    "Registry",
    "MemCompression",
    "Secure System",
    "vmmem",
    "vmwp",
    "vmcompute",
    "vmms",
    "EAAntiCheat.GameService",
    "EAAntiCheat.GameServiceLauncher",
    "EasyAntiCheat",
    "EasyAntiCheat_EOS",
    "BEService",
    "BEService_x64",
    "vgc",
    "vgtray",
    "FACEITService",
    "FACEIT",
    "01LowOS",
    "ExitLag",
    "ExitLagPmService",
    "NoPing",
    "GearUP",
    "VirtualDesktop.Streamer",
];

const IGNORED_LIBRARY_TOOLS: &[&str] = &[
    "DSX",
    "LosslessScaling",
    "steamerrorreporter",
    "steamerrorreporter64",
    "crashpad_handler",
    "vc_redist.x64",
    "vc_redist.x86",
    "DXSETUP",
    "UnityCrashHandler64",
    "UnityCrashHandler32",
];

fn default_purge_threshold_mb() -> u64 {
    1024
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchdogConfig {
    pub extra_folders: Vec<String>,
    pub ignored_processes: Vec<String>,
    #[serde(default)]
    pub auto_purge_standby: bool,
    #[serde(default = "default_purge_threshold_mb")]
    pub purge_free_threshold_mb: u64,
    #[serde(default)]
    pub auto_start_in_gui: bool,
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            extra_folders: Vec::new(),
            ignored_processes: Vec::new(),
            auto_purge_standby: false,
            purge_free_threshold_mb: default_purge_threshold_mb(),
            auto_start_in_gui: false,
        }
    }
}

impl WatchdogConfig {
    fn programdata_path() -> PathBuf {
        let base = std::env::var_os("ProgramData")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"));
        base.join("01LowOS").join("watchdog_config.json")
    }

    fn localappdata_path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("01LowOS").join("watchdog_config.json")
    }

    pub fn default_path() -> PathBuf {
        if crate::sys::elevation::is_elevated() {
            Self::programdata_path()
        } else {
            Self::localappdata_path()
        }
    }

    pub fn load_from(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str::<WatchdogConfig>(&s).ok())
            .unwrap_or_default()
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(path, json).map_err(|e| e.to_string())
    }

    pub fn load_default() -> Self {
        let pd = Self::programdata_path();
        if pd.is_file() {
            return Self::load_from(&pd);
        }
        let la = Self::localappdata_path();
        if la.is_file() {
            return Self::load_from(&la);
        }
        Self::default()
    }

    pub fn save_default(&self) -> Result<(), String> {
        let primary = Self::default_path();
        match self.save_to(&primary) {
            Ok(()) => Ok(()),
            Err(_) => self.save_to(&Self::localappdata_path()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcRole {
    NeverTouch,
    IdleBg,
    BelowNormalBg,
    ProtectedCore0,
    Heir,
    CommonOther,
}

pub fn matches_ci(list: &[&str], stem: &str) -> bool {
    list.iter().any(|x| x.eq_ignore_ascii_case(stem))
}

pub fn matches_ci_strings(list: &[String], stem: &str) -> bool {
    list.iter().any(|x| x.eq_ignore_ascii_case(stem))
}

pub fn classify_process(stem: &str) -> ProcRole {
    if matches_ci(NEVER_TOUCH, stem) {
        ProcRole::NeverTouch
    } else if matches_ci(IDLE_BG, stem) {
        ProcRole::IdleBg
    } else if matches_ci(BELOW_NORMAL_BG, stem) {
        ProcRole::BelowNormalBg
    } else if matches_ci(PROTECTED_CORE0, stem) {
        ProcRole::ProtectedCore0
    } else if matches_ci(HEIRS, stem) {
        ProcRole::Heir
    } else {
        ProcRole::CommonOther
    }
}

#[derive(Debug, Clone)]
pub struct SavedProcState {
    pub affinity: usize,
    pub priority: ProcPriority,
}

#[derive(Debug, Clone)]
pub struct WatchdogSnapshot {
    pub running: bool,
    pub topology: CpuTopology,
    pub active_game: Option<String>,
    pub active_game_pid: Option<u32>,
    pub gpu_confirmed: bool,
    pub isolated_count: usize,
    pub refused_count: usize,
    pub folders: Vec<PathBuf>,
    pub config: WatchdogConfig,
    pub logs: Vec<String>,
}

impl Default for WatchdogSnapshot {
    fn default() -> Self {
        Self {
            running: false,
            topology: detect_cpu_topology(),
            active_game: None,
            active_game_pid: None,
            gpu_confirmed: false,
            isolated_count: 0,
            refused_count: 0,
            folders: Vec::new(),
            config: WatchdogConfig::load_default(),
            logs: Vec::new(),
        }
    }
}

fn push_log(snap: &Arc<Mutex<WatchdogSnapshot>>, msg: String) {
    crate::sys::logger::append_log(&format!("[Vigia] {msg}"));
    if let Ok(mut guard) = snap.lock() {
        let line = format!("{} {msg}", Local::now().format("%H:%M:%S"));
        guard.logs.push(line);
        if guard.logs.len() > 250 {
            let excess = guard.logs.len() - 250;
            guard.logs.drain(0..excess);
        }
    }
}

fn path_in_folders(path: &Path, folders: &[PathBuf]) -> bool {
    let p_str = path.to_string_lossy().to_ascii_lowercase();
    folders.iter().any(|f| {
        let mut f_str = f.to_string_lossy().to_ascii_lowercase();
        if !f_str.ends_with('\\') {
            f_str.push('\\');
        }
        p_str.starts_with(&f_str)
    })
}

fn effective_folders(cfg: &WatchdogConfig) -> Vec<PathBuf> {
    let mut set: BTreeSet<PathBuf> = discover_game_folders().into_iter().collect();
    for f in &cfg.extra_folders {
        let p = PathBuf::from(f.trim().trim_end_matches('\\'));
        if p.is_dir() {
            set.insert(p);
        }
    }
    set.into_iter().collect()
}

fn is_excluded_stem(stem: &str, cfg: &WatchdogConfig) -> bool {
    matches_ci(NEVER_TOUCH, stem)
        || matches_ci(HEIRS, stem)
        || matches_ci(IDLE_BG, stem)
        || matches_ci(BELOW_NORMAL_BG, stem)
        || matches_ci(PROTECTED_CORE0, stem)
        || matches_ci(IGNORED_LIBRARY_TOOLS, stem)
        || matches_ci_strings(&cfg.ignored_processes, stem)
}

fn is_candidate_game_proc(
    p: &ProcInfo,
    folders: &[PathBuf],
    cfg: &WatchdogConfig,
    self_pid: u32,
) -> bool {
    if p.pid == self_pid || p.pid <= 4 {
        return false;
    }
    if is_excluded_stem(&p.name_stem, cfg) {
        return false;
    }
    if let Some(ref exe) = p.exe_path {
        return path_in_folders(exe, folders);
    }
    false
}

fn collect_sibling_game_exes(
    exe: &Path,
    folders: &[PathBuf],
    cfg: &WatchdogConfig,
) -> HashSet<String> {
    let mut root = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let exe_low = exe.to_string_lossy().to_ascii_lowercase();
    for f in folders {
        let mut f_str = f.to_string_lossy().to_ascii_lowercase();
        if !f_str.ends_with('\\') {
            f_str.push('\\');
        }
        if let Some(rel) = exe_low.strip_prefix(&f_str) {
            if let Some(first_seg) = rel.split('\\').next() {
                if !first_seg.is_empty() && rel.contains('\\') {
                    let candidate = f.join(first_seg);
                    if candidate.is_dir() {
                        root = candidate;
                    }
                }
            }
            break;
        }
    }

    let mut stems = HashSet::new();
    fn walk(dir: &Path, depth: usize, cfg: &WatchdogConfig, out: &mut HashSet<String>) {
        if depth > 4 {
            return;
        }
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for entry in rd.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, depth + 1, cfg, out);
            } else if p
                .extension()
                .and_then(|s| s.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("exe"))
            {
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    if !is_excluded_stem(stem, cfg) {
                        out.insert(stem.to_ascii_lowercase());
                    }
                }
            }
        }
    }
    walk(&root, 0, cfg, &mut stems);
    stems
}

pub struct WatchdogEngine {
    stop_flag: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pub snapshot: Arc<Mutex<WatchdogSnapshot>>,
}

impl WatchdogEngine {
    pub fn new() -> Self {
        Self {
            stop_flag: Arc::new(AtomicBool::new(false)),
            handle: None,
            snapshot: Arc::new(Mutex::new(WatchdogSnapshot::default())),
        }
    }

    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }

    pub fn refresh_folders(&self) {
        let cfg = self
            .snapshot
            .lock()
            .map(|g| g.config.clone())
            .unwrap_or_default();
        let found = effective_folders(&cfg);
        if let Ok(mut g) = self.snapshot.lock() {
            g.folders = found;
        }
    }

    pub fn update_config(&self, cfg: WatchdogConfig) -> Result<(), String> {
        cfg.save_default()?;
        let found = effective_folders(&cfg);
        if let Ok(mut g) = self.snapshot.lock() {
            g.config = cfg;
            g.folders = found;
        }
        Ok(())
    }

    pub fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }
        self.stop_flag.store(false, Ordering::SeqCst);
        let stop = Arc::clone(&self.stop_flag);
        let snap = Arc::clone(&self.snapshot);

        self.handle = Some(thread::spawn(move || {
            run_watchdog_loop(stop, snap);
        }));
    }

    pub fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for WatchdogEngine {
    fn drop(&mut self) {
        self.stop();
    }
}

fn run_watchdog_loop(stop: Arc<AtomicBool>, snap: Arc<Mutex<WatchdogSnapshot>>) {
    let self_pid = std::process::id();
    let topo = detect_cpu_topology();
    let mut cfg = snap
        .lock()
        .map(|g| g.config.clone())
        .unwrap_or_default();
    let mut folders = effective_folders(&cfg);
    let mut last_folder_refresh = Instant::now();
    let gpu_mon = Gpu3dMonitor::new();

    if let Ok(mut g) = snap.lock() {
        g.running = true;
        g.topology = topo.clone();
        g.folders = folders.clone();
    }
    push_log(
        &snap,
        format!(
            "Vigia Rust iniciado: {} threads ({} físicos, SMT {}), game=0x{:X}, bg=0x{:X}, GPU3D={}, {} pastas",
            topo.logical_count,
            topo.physical_cores,
            if topo.smt_enabled { "ON" } else { "OFF" },
            topo.game_mask,
            topo.background_mask,
            if gpu_mon.is_some() { "PDH" } else { "Fallback" },
            folders.len()
        ),
    );

    let mut saved: HashMap<u32, SavedProcState> = HashMap::new();
    let mut refused: HashSet<u32> = HashSet::new();
    let mut active_game_pid: Option<u32> = None;
    let mut active_game_name: Option<String> = None;
    let mut active_game_session: u32 = 0;
    let mut sibling_stems: HashSet<String> = HashSet::new();
    let mut gpu_confirmed = false;
    let mut pdh_fail_streak = 0u32;
    let mut last_standby_purge: Option<Instant> = None;

    while !stop.load(Ordering::SeqCst) {
        if let Ok(g) = snap.lock() {
            if g.config != cfg {
                cfg = g.config.clone();
                folders = effective_folders(&cfg);
            }
        }
        if last_folder_refresh.elapsed() >= Duration::from_secs(60) {
            folders = effective_folders(&cfg);
            last_folder_refresh = Instant::now();
            if let Ok(mut g) = snap.lock() {
                g.folders = folders.clone();
            }
        }

        let procs = list_processes();
        let proc_map: HashMap<u32, &ProcInfo> = procs.iter().map(|p| (p.pid, p)).collect();

        // Check if active game PID or one of its sibling executables (anti-cheat safe) is still alive
        let mut game_alive = active_game_pid
            .and_then(|pid| proc_map.get(&pid).copied())
            .is_some();

        if !game_alive && !sibling_stems.is_empty() {
            if let Some(sib) = procs
                .iter()
                .find(|p| sibling_stems.contains(&p.name_stem.to_ascii_lowercase()))
            {
                active_game_pid = Some(sib.pid);
                active_game_name = Some(sib.name_stem.clone());
                active_game_session = sib.session_id;
                game_alive = true;
            }
        }

        if !game_alive {
            if let Some(found) = procs
                .iter()
                .find(|p| is_candidate_game_proc(p, &folders, &cfg, self_pid))
            {
                active_game_pid = Some(found.pid);
                active_game_name = Some(found.name_stem.clone());
                active_game_session = found.session_id;
                gpu_confirmed = false;
                sibling_stems = found
                    .exe_path
                    .as_deref()
                    .map(|p| collect_sibling_game_exes(p, &folders, &cfg))
                    .unwrap_or_default();
                game_alive = true;
                push_log(
                    &snap,
                    format!(
                        "Candidato a jogo: {} (pid={}, {} exe(s) na pasta)",
                        found.name_stem,
                        found.pid,
                        sibling_stems.len()
                    ),
                );
            }
        }

        if game_alive {
            let g_pid = active_game_pid.unwrap();
            let mut game_ids: HashSet<u32> = HashSet::new();
            game_ids.insert(g_pid);

            for p in &procs {
                if p.parent_pid == g_pid
                    || is_candidate_game_proc(p, &folders, &cfg, self_pid)
                    || sibling_stems.contains(&p.name_stem.to_ascii_lowercase())
                {
                    game_ids.insert(p.pid);
                }
            }

            // Force game processes onto game_mask and >= Normal priority
            for &gid in &game_ids {
                if let Some((aff, pri)) = get_process_affinity_and_priority(gid) {
                    let target_aff = if aff != topo.game_mask {
                        Some(topo.game_mask)
                    } else {
                        None
                    };
                    let target_pri = if pri.is_low() {
                        Some(ProcPriority::Normal)
                    } else {
                        None
                    };
                    if target_aff.is_some() || target_pri.is_some() {
                        let _ = set_process_affinity_and_priority(gid, target_aff, target_pri);
                    }
                }
            }

            if gpu_confirmed
                && cfg.auto_purge_standby
                && crate::sys::elevation::is_elevated()
                && last_standby_purge.is_none_or(|t| t.elapsed() >= Duration::from_secs(60))
            {
                if let Some(ram) = crate::sys::memory::query_ram() {
                    if ram.avail_mb < cfg.purge_free_threshold_mb {
                        match crate::sys::memory::purge_standby_list() {
                            Ok(m) => push_log(&snap, format!("[ISLC] {m}")),
                            Err(e) => push_log(&snap, format!("[ISLC] {e}")),
                        }
                        last_standby_purge = Some(Instant::now());
                    }
                }
            }

            // Confirm real 3D GPU usage (>= 5%) before isolating background onto Core 0
            if !gpu_confirmed {
                let confirmed_now = if let Some(ref mon) = gpu_mon {
                    if let Some(usage) = mon.sample_by_pid() {
                        pdh_fail_streak = 0;
                        let total_3d: f64 = game_ids.iter().filter_map(|id| usage.get(id)).sum();
                        if total_3d >= 5.0 {
                            push_log(
                                &snap,
                                format!(
                                    "Jogo confirmado pela GPU 3D ({:.1}%): {} -> isolamento ativo",
                                    total_3d,
                                    active_game_name.as_deref().unwrap_or("")
                                ),
                            );
                            true
                        } else {
                            false
                        }
                    } else {
                        pdh_fail_streak += 1;
                        if pdh_fail_streak >= 3 {
                            has_visible_window_for_pids(&game_ids)
                        } else {
                            false
                        }
                    }
                } else {
                    has_visible_window_for_pids(&game_ids)
                };
                gpu_confirmed = confirmed_now;
            }

            if gpu_confirmed {
                for p in &procs {
                    if p.pid <= 4 || p.pid == self_pid || game_ids.contains(&p.pid) {
                        continue;
                    }
                    if refused.contains(&p.pid) {
                        continue;
                    }
                    let role = classify_process(&p.name_stem);
                    if role == ProcRole::NeverTouch {
                        continue;
                    }

                    let Some((cur_aff, cur_pri)) = get_process_affinity_and_priority(p.pid) else {
                        refused.insert(p.pid);
                        continue;
                    };

                    if !saved.contains_key(&p.pid) {
                        let orig_aff = if cur_aff == topo.background_mask {
                            if let Some(parent_saved) = saved.get(&p.parent_pid) {
                                parent_saved.affinity
                            } else {
                                topo.all_mask
                            }
                        } else {
                            cur_aff
                        };
                        let orig_pri = if cur_aff == topo.background_mask && cur_pri.is_low() {
                            ProcPriority::Normal
                        } else {
                            cur_pri
                        };
                        saved.insert(
                            p.pid,
                            SavedProcState {
                                affinity: orig_aff,
                                priority: orig_pri,
                            },
                        );
                    }

                    let desired_pri = match role {
                        ProcRole::IdleBg if !cur_pri.is_high_or_realtime() => {
                            Some(ProcPriority::Idle)
                        }
                        ProcRole::BelowNormalBg if !cur_pri.is_high_or_realtime() => {
                            Some(ProcPriority::BelowNormal)
                        }
                        ProcRole::CommonOther
                            if p.session_id == active_game_session
                                && matches!(
                                    cur_pri,
                                    ProcPriority::Normal | ProcPriority::AboveNormal
                                ) =>
                        {
                            Some(ProcPriority::BelowNormal)
                        }
                        _ => None,
                    };

                    let set_aff = if cur_aff != topo.background_mask {
                        Some(topo.background_mask)
                    } else {
                        None
                    };
                    let set_pri = desired_pri.filter(|&d| d != cur_pri);

                    if (set_aff.is_some() || set_pri.is_some())
                        && set_process_affinity_and_priority(p.pid, set_aff, set_pri).is_err()
                    {
                        saved.remove(&p.pid);
                        refused.insert(p.pid);
                    }
                }
            }
        } else {
            if active_game_pid.is_some() {
                let gname = active_game_name.take().unwrap_or_default();
                active_game_pid = None;
                sibling_stems.clear();
                gpu_confirmed = false;
                pdh_fail_streak = 0;
                let restored = restore_all_background(&mut saved, &topo);
                refused.clear();
                push_log(
                    &snap,
                    format!("Jogo encerrado ({gname}): {restored} processo(s) restaurado(s)."),
                );
            }

            // Outside games: keep Explorer and launchers inheriting game_mask
            for p in &procs {
                if matches_ci(HEIRS, &p.name_stem) {
                    if let Some((aff, _)) = get_process_affinity_and_priority(p.pid) {
                        if aff != topo.game_mask {
                            let _ = set_process_affinity_and_priority(
                                p.pid,
                                Some(topo.game_mask),
                                None,
                            );
                        }
                    }
                }
            }
        }

        if let Ok(mut g) = snap.lock() {
            g.active_game = active_game_name.clone();
            g.active_game_pid = active_game_pid;
            g.gpu_confirmed = gpu_confirmed;
            g.isolated_count = saved.len();
            g.refused_count = refused.len();
        }

        let wait = if active_game_pid.is_some() {
            Duration::from_secs(3)
        } else {
            Duration::from_secs(2)
        };
        let step = Duration::from_millis(200);
        let mut elapsed = Duration::ZERO;
        while elapsed < wait && !stop.load(Ordering::SeqCst) {
            thread::sleep(step);
            elapsed += step;
        }
    }

    let restored = restore_all_background(&mut saved, &topo);
    for p in list_processes() {
        if matches_ci(HEIRS, &p.name_stem) {
            let _ = set_process_affinity_and_priority(p.pid, Some(topo.all_mask), None);
        }
    }
    if let Ok(mut g) = snap.lock() {
        g.running = false;
        g.active_game = None;
        g.active_game_pid = None;
        g.gpu_confirmed = false;
        g.isolated_count = 0;
        g.refused_count = 0;
    }
    push_log(
        &snap,
        format!("Vigia encerrado ({restored} processos restaurados)."),
    );
}

fn restore_all_background(saved: &mut HashMap<u32, SavedProcState>, topo: &CpuTopology) -> usize {
    let mut restored = 0usize;
    for (pid, state) in saved.drain() {
        if let Some((cur_aff, cur_pri)) = get_process_affinity_and_priority(pid) {
            let target_aff = if cur_aff != state.affinity {
                Some(state.affinity)
            } else {
                None
            };
            let target_pri = if cur_pri != state.priority {
                Some(state.priority)
            } else {
                None
            };
            if target_aff.is_some() || target_pri.is_some() {
                let _ = set_process_affinity_and_priority(pid, target_aff, target_pri);
            }
            restored += 1;
        }
    }
    let self_pid = std::process::id();
    for p in list_processes() {
        if p.pid <= 4 || p.pid == self_pid || matches_ci(NEVER_TOUCH, &p.name_stem) {
            continue;
        }
        if let Some((cur_aff, cur_pri)) = get_process_affinity_and_priority(p.pid) {
            if cur_aff == topo.background_mask {
                let fix_pri = if cur_pri.is_low() {
                    Some(ProcPriority::Normal)
                } else {
                    None
                };
                let _ = set_process_affinity_and_priority(p.pid, Some(topo.all_mask), fix_pri);
            }
        }
    }
    restored
}

pub fn is_autostart_installed() -> bool {
    let script = format!(
        "[bool](Get-ScheduledTask -TaskName '{TASK_NAME}' -TaskPath '{TASK_PATH}' -ErrorAction SilentlyContinue)"
    );
    run_powershell_checked(&script)
        .map(|o| o.stdout.trim().eq_ignore_ascii_case("True"))
        .unwrap_or(false)
}

pub fn install_autostart_task() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_esc = exe.to_string_lossy().replace('\'', "''");
    let script = format!(
        r#"
$act = New-ScheduledTaskAction -Execute '{exe_esc}' -Argument '--watchdog'
$trg = New-ScheduledTaskTrigger -AtLogOn
$trg.Delay = 'PT5S'
$prn = New-ScheduledTaskPrincipal -GroupId 'S-1-5-32-545' -RunLevel Highest
$set = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -MultipleInstances IgnoreNew -ExecutionTimeLimit (New-TimeSpan -Days 0)
Register-ScheduledTask -TaskName '{TASK_NAME}' -TaskPath '{TASK_PATH}' -Action $act -Trigger $trg -Principal $prn -Settings $set -Force | Out-Null
"#
    );
    run_powershell_checked(&script)?;
    Ok(format!("Tarefa {TASK_PATH}{TASK_NAME} registrada no logon (5s, Admin sem UAC)."))
}

pub fn enable_autostart_with_runtime(engine: &mut WatchdogEngine) -> Result<String, String> {
    let mut cfg = engine
        .snapshot
        .lock()
        .map(|g| g.config.clone())
        .unwrap_or_default();
    cfg.auto_start_in_gui = true;
    cfg.auto_purge_standby = true;
    engine.update_config(cfg)?;
    let msg = install_autostart_task()?;
    if !engine.is_running() {
        engine.start();
    }
    Ok(format!(
        "{msg} Vigia + ISLC Standby Auto-Purge ativados agora e em todos os boots."
    ))
}

pub fn remove_autostart_task() -> Result<String, String> {
    let script = format!(
        "Unregister-ScheduledTask -TaskName '{TASK_NAME}' -TaskPath '{TASK_PATH}' -Confirm:$false -ErrorAction SilentlyContinue"
    );
    run_powershell_checked(&script)?;
    Ok(format!("Tarefa {TASK_PATH}{TASK_NAME} removida."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_topology_masks_match_9800x3d_smt_off_and_on() {
        let off = CpuTopology::from_counts(8, 8);
        assert!(!off.smt_enabled);
        assert_eq!(off.background_mask, 0x1);
        assert_eq!(off.game_mask, 0xFE);
        assert_eq!(off.all_mask, 0xFF);

        let on = CpuTopology::from_counts(16, 8);
        assert!(on.smt_enabled);
        assert_eq!(on.background_mask, 0x3);
        assert_eq!(on.game_mask, 0xFFFC);
        assert_eq!(on.all_mask, 0xFFFF);
    }

    #[test]
    fn process_classification_protects_anticheat_vm_and_background_tiers() {
        assert_eq!(classify_process("vgc"), ProcRole::NeverTouch);
        assert_eq!(classify_process("EasyAntiCheat_EOS"), ProcRole::NeverTouch);
        assert_eq!(classify_process("vmwp"), ProcRole::NeverTouch);
        assert_eq!(classify_process("System"), ProcRole::NeverTouch);

        assert_eq!(classify_process("steam"), ProcRole::Heir);
        assert_eq!(classify_process("Explorer"), ProcRole::Heir);

        assert_eq!(classify_process("Discord"), ProcRole::BelowNormalBg);
        assert_eq!(classify_process("Spotify"), ProcRole::BelowNormalBg);
        assert_eq!(classify_process("steamwebhelper"), ProcRole::IdleBg);
        assert_eq!(classify_process("dwm"), ProcRole::ProtectedCore0);
        assert_eq!(classify_process("audiodg"), ProcRole::ProtectedCore0);
        assert_eq!(classify_process("chrome"), ProcRole::IdleBg);
        assert_eq!(classify_process("ExitLag"), ProcRole::NeverTouch);
        assert_eq!(classify_process("notepad"), ProcRole::CommonOther);
    }

    #[test]
    fn watchdog_config_roundtrip_works() {
        let tmp = std::env::temp_dir().join(format!("01lowos_wd_cfg_{}.json", std::process::id()));
        let cfg = WatchdogConfig {
            extra_folders: vec![r"D:\Jogos".into()],
            ignored_processes: vec!["Wallpaper32".into()],
            ..Default::default()
        };
        cfg.save_to(&tmp).unwrap();
        let loaded = WatchdogConfig::load_from(&tmp);
        assert_eq!(loaded, cfg);
        let _ = fs::remove_file(&tmp);
    }
}
