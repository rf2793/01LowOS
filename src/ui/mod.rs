pub mod apps_view;
pub mod backups_view;
pub mod iso_view;
pub mod tools_view;
pub mod tweaks_view;
pub mod watchdog_view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use chrono::Local;
use eframe::egui::{self, Color32, RichText, Stroke};

use crate::apps::{detect_installed, install_app, load_catalog, AppEntry};
use crate::iso::{inspect_iso_state, IsoBuildState};
use crate::sys::backup::{BackupRecord, BackupStore};
use crate::sys::elevation::{is_elevated, relaunch_elevated};
use crate::tweaks::catalog::build_catalog;
use crate::tweaks::{Category, Tweak, TweakStatus};
use crate::watchdog::{is_autostart_installed, WatchdogEngine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Tweaks,
    Watchdog,
    Tools,
    Apps,
    IsoBuild,
    Backups,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    All,
    Applied,
    Pending,
    HasBackup,
}

pub enum AppMsg {
    TweakChecked {
        id: String,
        status: TweakStatus,
        has_backup: bool,
    },
    TweakFinished {
        id: String,
        is_apply: bool,
        result: Result<String, String>,
        new_status: TweakStatus,
        has_backup: bool,
    },
    CheckAllFinished,
    WatchdogTaskStatus(bool),
    AppsChecked(Vec<(String, bool)>),
    AppLog(String),
    AppInstallDone {
        name: String,
        result: Result<String, String>,
        installed: bool,
    },
    AppsBatchDone,
    IsoLog(String),
    IsoDone(Result<(), String>),
    BackupsLoaded(Vec<BackupRecord>),
    SelfTestLog(String),
    SelfTestDone {
        passed: usize,
        failed: usize,
        skipped: usize,
        report: String,
    },
    Banner {
        ok: bool,
        text: String,
    },
    CfgListLoaded(Vec<String>),
    ShaderScanLoaded(Vec<crate::tools::ShaderCacheDir>),
    ToolBusyDone {
        ok: bool,
        text: String,
        refresh_cfg: bool,
        refresh_shaders: bool,
    },
    RestoredFromTray,
    QuitFromTray,
}

pub struct LowOsApp {
    pub tab: Tab,
    pub elevated: bool,
    pub catalog: Vec<Arc<dyn Tweak>>,
    pub backups: BackupStore,

    pub statuses: HashMap<String, TweakStatus>,
    pub backup_flags: HashMap<String, bool>,
    pub expanded_details: HashSet<String>,
    pub selected_tweaks: HashSet<String>,
    pub preset_path: String,
    pub busy_tweaks: HashSet<String>,
    pub checking_all: bool,

    pub search_query: String,
    pub category_filter: Option<Category>,
    pub status_filter: StatusFilter,

    pub watchdog: WatchdogEngine,
    pub watchdog_task_installed: bool,
    pub wd_new_folder: String,
    pub wd_new_ignored: String,

    pub apps: Vec<AppEntry>,
    pub apps_installed: HashMap<String, bool>,
    pub apps_busy: bool,
    pub apps_cache_dir: String,
    pub apps_logs: Vec<String>,

    pub iso_state: IsoBuildState,
    pub iso_logs: Vec<String>,
    pub iso_busy: bool,
    pub iso_phases_input: String,
    pub iso_confirm_full: bool,

    pub backup_records: Vec<BackupRecord>,
    pub confirm_revert_all: bool,
    pub self_test_busy: bool,
    pub session_logs: Vec<String>,
    pub banner: Option<(bool, String)>,
    pub tray: Option<crate::sys::tray::TrayIcon>,
    pub minimized_to_tray: bool,
    pub app_hwnd: Option<isize>,
    pub show_donate_modal: bool,

    pub cfg_exe_input: String,
    pub cfg_disabled_list: Vec<String>,
    pub shader_dirs: Vec<crate::tools::ShaderCacheDir>,
    pub tools_busy: bool,
    pub confirm_clean_shaders: bool,

    pub tx: Sender<AppMsg>,
    pub rx: Receiver<AppMsg>,
}

impl LowOsApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_theme(&cc.egui_ctx);
        let (tx, rx) = mpsc::channel();
        let catalog: Vec<Arc<dyn Tweak>> = build_catalog().into_iter().map(Arc::from).collect();
        let apps = load_catalog().unwrap_or_default();
        let backups = BackupStore::default_store();
        let elevated = is_elevated();

        let preset_path = if elevated {
            std::env::var_os("ProgramData")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\ProgramData"))
        } else {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
        }
        .join("01LowOS")
        .join("preset_selecao.json")
        .to_string_lossy()
        .to_string();

        let init_msg = format!(
            "01LowOS inicializado (Admin: {}).",
            if elevated { "SIM" } else { "NÃO" }
        );
        crate::sys::logger::append_log(&init_msg);
        let mut app = Self {
            tab: Tab::Tweaks,
            elevated,
            catalog,
            backups,
            statuses: HashMap::new(),
            backup_flags: HashMap::new(),
            expanded_details: HashSet::new(),
            selected_tweaks: HashSet::new(),
            preset_path,
            busy_tweaks: HashSet::new(),
            checking_all: false,
            search_query: String::new(),
            category_filter: None,
            status_filter: StatusFilter::All,
            watchdog: WatchdogEngine::new(),
            watchdog_task_installed: false,
            wd_new_folder: String::new(),
            wd_new_ignored: String::new(),
            apps,
            apps_installed: HashMap::new(),
            apps_busy: false,
            apps_cache_dir: r"C:\ISO_Apps".to_string(),
            apps_logs: Vec::new(),
            iso_state: inspect_iso_state(),
            iso_logs: Vec::new(),
            iso_busy: false,
            iso_phases_input: "05,06".to_string(),
            iso_confirm_full: false,
            backup_records: Vec::new(),
            confirm_revert_all: false,
            self_test_busy: false,
            session_logs: vec![format!(
                "{} 01LowOS inicializado (Admin: {}).",
                Local::now().format("%H:%M:%S"),
                if elevated { "SIM" } else { "NÃO" }
            )],
            banner: None,
            tray: None,
            minimized_to_tray: false,
            app_hwnd: None,
            show_donate_modal: false,
            cfg_exe_input: String::new(),
            cfg_disabled_list: Vec::new(),
            shader_dirs: crate::tools::scan_shader_caches(),
            tools_busy: false,
            confirm_clean_shaders: false,
            tx,
            rx,
        };

        app.trigger_check_all(&cc.egui_ctx);
        app.trigger_refresh_backups(&cc.egui_ctx);
        app.trigger_check_apps(&cc.egui_ctx);
        app.trigger_check_watchdog_task(&cc.egui_ctx);
        app.trigger_refresh_cfg(&cc.egui_ctx);
        let auto_start = std::env::args().any(|a| a == "--start-watchdog")
            || app
                .watchdog
                .snapshot
                .lock()
                .map(|g| g.config.auto_start_in_gui)
                .unwrap_or(false);
        if auto_start {
            app.watchdog.start();
        }
        {
            let snap = std::sync::Arc::clone(&app.watchdog.snapshot);
            let ctx_wd = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                let found = crate::watchdog::folders::discover_game_folders();
                if let Ok(mut g) = snap.lock() {
                    g.folders = found;
                }
                ctx_wd.request_repaint();
            });
        }
        app
    }

    pub fn push_session_log(&mut self, msg: String) {
        crate::sys::logger::append_log(&msg);
        self.session_logs
            .push(format!("{} {msg}", Local::now().format("%H:%M:%S")));
        if self.session_logs.len() > 400 {
            let excess = self.session_logs.len() - 400;
            self.session_logs.drain(0..excess);
        }
    }

    pub fn trigger_check_all(&mut self, ctx: &egui::Context) {
        if self.checking_all {
            return;
        }
        self.checking_all = true;
        let catalog = self.catalog.clone();
        let backups = self.backups.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            for t in catalog {
                let id = t.meta().id.to_string();
                let status = t.check();
                let has_backup = backups.exists(&id);
                let _ = tx.send(AppMsg::TweakChecked {
                    id,
                    status,
                    has_backup,
                });
                ctx.request_repaint();
            }
            let _ = tx.send(AppMsg::CheckAllFinished);
            ctx.request_repaint();
        });
    }

    pub fn trigger_apply_tweak(&mut self, tweak: Arc<dyn Tweak>, ctx: &egui::Context) {
        let id = tweak.meta().id.to_string();
        if self.busy_tweaks.contains(&id) {
            return;
        }
        self.busy_tweaks.insert(id.clone());
        let backups = self.backups.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let result = tweak.apply(&backups);
            let new_status = tweak.check();
            let has_backup = backups.exists(&id);
            let _ = tx.send(AppMsg::TweakFinished {
                id,
                is_apply: true,
                result,
                new_status,
                has_backup,
            });
            ctx.request_repaint();
        });
    }

    pub fn trigger_revert_tweak(&mut self, tweak: Arc<dyn Tweak>, ctx: &egui::Context) {
        let id = tweak.meta().id.to_string();
        if self.busy_tweaks.contains(&id) {
            return;
        }
        self.busy_tweaks.insert(id.clone());
        let backups = self.backups.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let result = tweak.revert(&backups);
            let new_status = tweak.check();
            let has_backup = backups.exists(&id);
            let _ = tx.send(AppMsg::TweakFinished {
                id,
                is_apply: false,
                result,
                new_status,
                has_backup,
            });
            ctx.request_repaint();
        });
    }

    pub fn trigger_reset_default_tweak(&mut self, tweak: Arc<dyn Tweak>, ctx: &egui::Context) {
        let id = tweak.meta().id.to_string();
        if self.busy_tweaks.contains(&id) {
            return;
        }
        self.busy_tweaks.insert(id.clone());
        let backups = self.backups.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            let result = tweak.reset_to_default();
            let new_status = tweak.check();
            let has_backup = backups.exists(&id);
            let _ = tx.send(AppMsg::TweakFinished {
                id,
                is_apply: false,
                result,
                new_status,
                has_backup,
            });
            ctx.request_repaint();
        });
    }

    pub fn trigger_batch_tweaks(
        &mut self,
        tweaks: Vec<Arc<dyn Tweak>>,
        is_apply: bool,
        ctx: &egui::Context,
    ) {
        let mut queue = Vec::new();
        for t in tweaks {
            let id = t.meta().id.to_string();
            if self.busy_tweaks.insert(id) {
                queue.push(t);
            }
        }
        if queue.is_empty() {
            return;
        }
        let backups = self.backups.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            for tweak in queue {
                let id = tweak.meta().id.to_string();
                let result = if is_apply {
                    tweak.apply(&backups)
                } else {
                    tweak.revert(&backups)
                };
                let new_status = tweak.check();
                let has_backup = backups.exists(&id);
                let _ = tx.send(AppMsg::TweakFinished {
                    id,
                    is_apply,
                    result,
                    new_status,
                    has_backup,
                });
                ctx.request_repaint();
            }
        });
    }

    pub fn trigger_refresh_backups(&self, ctx: &egui::Context) {
        let backups = self.backups.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Ok(list) = backups.list_all() {
                let _ = tx.send(AppMsg::BackupsLoaded(list));
                ctx.request_repaint();
            }
        });
    }

    pub fn trigger_check_watchdog_task(&self, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let ok = is_autostart_installed();
            let _ = tx.send(AppMsg::WatchdogTaskStatus(ok));
            ctx.request_repaint();
        });
    }

    pub fn trigger_refresh_cfg(&self, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            if let Ok(list) = crate::tools::list_cfg_disabled_exes() {
                let _ = tx.send(AppMsg::CfgListLoaded(list));
                ctx.request_repaint();
            }
        });
    }

    pub fn trigger_scan_shaders(&self, ctx: &egui::Context) {
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let list = crate::tools::scan_shader_caches();
            let _ = tx.send(AppMsg::ShaderScanLoaded(list));
            ctx.request_repaint();
        });
    }

    pub fn trigger_check_apps(&self, ctx: &egui::Context) {
        let apps = self.apps.clone();
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let results: Vec<(String, bool)> = apps
                .iter()
                .map(|a| (a.nome.clone(), detect_installed(a)))
                .collect();
            let _ = tx.send(AppMsg::AppsChecked(results));
            ctx.request_repaint();
        });
    }

    pub fn trigger_install_apps(&mut self, targets: Vec<AppEntry>, ctx: &egui::Context) {
        if self.apps_busy || targets.is_empty() {
            return;
        }
        self.apps_busy = true;
        let cache = PathBuf::from(&self.apps_cache_dir);
        let tx = self.tx.clone();
        let ctx = ctx.clone();

        std::thread::spawn(move || {
            for entry in targets {
                let name = entry.nome.clone();
                let res = install_app(&entry, &cache, |line| {
                    let _ = tx.send(AppMsg::AppLog(line.to_string()));
                    ctx.request_repaint();
                });
                let installed = detect_installed(&entry);
                let _ = tx.send(AppMsg::AppInstallDone {
                    name,
                    result: res,
                    installed,
                });
                ctx.request_repaint();
            }
            let _ = tx.send(AppMsg::AppsBatchDone);
            ctx.request_repaint();
        });
    }

    fn drain_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMsg::TweakChecked {
                    id,
                    status,
                    has_backup,
                } => {
                    self.statuses.insert(id.clone(), status);
                    self.backup_flags.insert(id, has_backup);
                }
                AppMsg::TweakFinished {
                    id,
                    is_apply,
                    result,
                    new_status,
                    has_backup,
                } => {
                    self.busy_tweaks.remove(&id);
                    self.statuses.insert(id.clone(), new_status);
                    self.backup_flags.insert(id.clone(), has_backup);
                    let verb = if is_apply { "Aplicação" } else { "Reversão" };
                    match result {
                        Ok(msg) => {
                            let text = format!("[{id}] {verb}: {msg}");
                            self.banner = Some((true, text.clone()));
                            self.push_session_log(text);
                        }
                        Err(err) => {
                            let text = format!("[{id}] ERRO na {verb}: {err}");
                            self.banner = Some((false, text.clone()));
                            self.push_session_log(text);
                        }
                    }
                    self.trigger_refresh_backups(ctx);
                }
                AppMsg::CheckAllFinished => {
                    self.checking_all = false;
                }
                AppMsg::WatchdogTaskStatus(ok) => {
                    self.watchdog_task_installed = ok;
                }
                AppMsg::AppsChecked(rows) => {
                    for (name, inst) in rows {
                        self.apps_installed.insert(name, inst);
                    }
                }
                AppMsg::AppLog(line) => {
                    self.apps_logs.push(line.clone());
                    self.push_session_log(format!("[Apps] {line}"));
                }
                AppMsg::AppInstallDone {
                    name,
                    result,
                    installed,
                } => {
                    self.apps_installed.insert(name.clone(), installed);
                    match result {
                        Ok(m) => {
                            self.apps_logs.push(m.clone());
                            self.banner = Some((true, m.clone()));
                            self.push_session_log(m);
                        }
                        Err(e) => {
                            let text = format!("{name}: FALHA {e}");
                            self.apps_logs.push(text.clone());
                            self.banner = Some((false, text.clone()));
                            self.push_session_log(text);
                        }
                    }
                }
                AppMsg::AppsBatchDone => {
                    self.apps_busy = false;
                }
                AppMsg::IsoLog(line) => {
                    self.iso_logs.push(line);
                    if self.iso_logs.len() > 600 {
                        let excess = self.iso_logs.len() - 600;
                        self.iso_logs.drain(0..excess);
                    }
                }
                AppMsg::IsoDone(res) => {
                    self.iso_busy = false;
                    self.iso_state = inspect_iso_state();
                    match res {
                        Ok(()) => {
                            self.banner = Some((true, "Operação de build ISO concluída.".into()));
                            self.push_session_log("[ISO] Concluído com sucesso.".into());
                        }
                        Err(e) => {
                            self.banner = Some((false, format!("Erro no build ISO: {e}")));
                            self.push_session_log(format!("[ISO] ERRO: {e}"));
                        }
                    }
                }
                AppMsg::BackupsLoaded(list) => {
                    self.backup_records = list;
                }
                AppMsg::SelfTestLog(line) => {
                    self.push_session_log(line);
                }
                AppMsg::SelfTestDone {
                    passed,
                    failed,
                    skipped,
                    report,
                } => {
                    self.self_test_busy = false;
                    self.banner = Some((
                        failed == 0,
                        format!("Auto-teste: PASS={passed}, FAIL={failed}, SKIP={skipped} ({report})"),
                    ));
                    self.trigger_check_all(ctx);
                    self.trigger_refresh_backups(ctx);
                }
                AppMsg::Banner { ok, text } => {
                    self.banner = Some((ok, text.clone()));
                    self.push_session_log(text);
                }
                AppMsg::CfgListLoaded(list) => {
                    self.cfg_disabled_list = list;
                }
                AppMsg::ShaderScanLoaded(list) => {
                    self.shader_dirs = list;
                }
                AppMsg::ToolBusyDone {
                    ok,
                    text,
                    refresh_cfg,
                    refresh_shaders,
                } => {
                    self.tools_busy = false;
                    self.banner = Some((ok, text.clone()));
                    self.push_session_log(text);
                    if refresh_cfg {
                        self.trigger_refresh_cfg(ctx);
                    }
                    if refresh_shaders {
                        self.trigger_scan_shaders(ctx);
                    }
                }
                AppMsg::RestoredFromTray => {
                    self.minimized_to_tray = false;
                    self.tray = None;
                }
                AppMsg::QuitFromTray => {
                    self.watchdog.stop();
                    self.minimized_to_tray = false;
                    self.tray = None;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    pub fn minimize_to_tray(&mut self, ctx: &egui::Context) {
        let Some(hwnd) = self
            .app_hwnd
            .or_else(crate::sys::tray::find_own_visible_window)
        else {
            self.banner = Some((
                false,
                "Não foi possível localizar a janela para minimizar.".into(),
            ));
            return;
        };
        self.app_hwnd = Some(hwnd);

        if self.tray.is_none() {
            let tx = self.tx.clone();
            let ctx_cb = ctx.clone();
            match crate::sys::tray::TrayIcon::new(
                "01LowOS — Vigia de Afinidade",
                Arc::new(move |action| match action {
                    crate::sys::tray::TrayAction::Restore => {
                        crate::sys::tray::restore_window(hwnd);
                        let _ = tx.send(AppMsg::RestoredFromTray);
                        ctx_cb.request_repaint();
                    }
                    crate::sys::tray::TrayAction::Exit => {
                        let _ = tx.send(AppMsg::QuitFromTray);
                        crate::sys::tray::close_window(hwnd);
                        ctx_cb.request_repaint();
                    }
                }),
            ) {
                Ok(t) => self.tray = Some(t),
                Err(e) => {
                    self.banner = Some((false, format!("Falha ao criar ícone na bandeja: {e}")));
                    return;
                }
            }
        }
        self.minimized_to_tray = true;
        crate::sys::tray::hide_window(hwnd);
    }
}

impl eframe::App for LowOsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_messages(ctx);

        if self.watchdog.is_running() {
            ctx.request_repaint_after(std::time::Duration::from_millis(800));
        }

        // Left navigation sidebar
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(215.0)
            .show(ctx, |ui| {
                ui.add_space(12.0);
                ui.heading(RichText::new("01LowOS").color(Color32::from_rgb(34, 211, 238)).strong());
                ui.label(
                    RichText::new("Suite Modular em Rust")
                        .small()
                        .color(Color32::from_rgb(148, 163, 184)),
                );
                ui.add_space(12.0);
                ui.separator();
                ui.add_space(8.0);

                let nav_items = [
                    (Tab::Tweaks, "Melhorias Individuais"),
                    (Tab::Watchdog, "Vigia de Afinidade"),
                    (Tab::Tools, "Ferramentas"),
                    (Tab::Apps, "Aplicativos"),
                    (Tab::IsoBuild, "Build de ISO"),
                    (Tab::Backups, "Backups & Logs"),
                ];

                for (tab, label) in nav_items {
                    let selected = self.tab == tab;
                    let btn = egui::Button::new(
                        RichText::new(label)
                            .size(14.0)
                            .color(if selected {
                                Color32::from_rgb(15, 23, 42)
                            } else {
                                Color32::from_rgb(226, 232, 240)
                            }),
                    )
                    .fill(if selected {
                        Color32::from_rgb(34, 211, 238)
                    } else {
                        Color32::from_rgb(23, 30, 41)
                    })
                    .min_size(egui::vec2(190.0, 34.0));

                    if ui.add(btn).clicked() {
                        self.tab = tab;
                    }
                    ui.add_space(4.0);
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.add_space(8.0);
                    let applied = self
                        .statuses
                        .values()
                        .filter(|s| matches!(s, TweakStatus::Applied))
                        .count();
                    let backups_n = self.backup_flags.values().filter(|&&v| v).count();
                    ui.label(
                        RichText::new(format!(
                            "Aplicadas: {applied}/{}\nBackups ativos: {backups_n}",
                            self.catalog.len()
                        ))
                        .small()
                        .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.separator();
                    if ui
                        .add(
                            egui::Button::new("Reportar Problema")
                                .fill(Color32::from_rgb(30, 41, 59))
                                .min_size(egui::vec2(190.0, 28.0)),
                        )
                        .on_hover_text("Abre o GitHub Issues já preenchido com diagnóstico técnico anônimo")
                        .clicked()
                    {
                        let topo = self
                            .watchdog
                            .snapshot
                            .lock()
                            .map(|g| g.topology.clone())
                            .unwrap_or_else(|_| crate::watchdog::win32::detect_cpu_topology());
                        let recent_errs: Vec<String> = self
                            .session_logs
                            .iter()
                            .rev()
                            .filter(|l| l.contains("ERRO") || l.contains("FALHA"))
                            .take(5)
                            .cloned()
                            .collect();
                        let url = crate::links::build_bug_report_url(
                            env!("CARGO_PKG_VERSION"),
                            self.elevated,
                            topo.logical_count,
                            topo.physical_cores,
                            topo.smt_enabled,
                            &recent_errs,
                        );
                        if let Err(e) = crate::links::open_url(&url) {
                            self.banner = Some((false, e));
                        }
                    }
                    ui.add_space(4.0);
                    let autostart_btn = if self.watchdog_task_installed {
                        egui::Button::new("Autostart Windows: ATIVO")
                            .fill(Color32::from_rgb(6, 78, 59))
                            .min_size(egui::vec2(190.0, 28.0))
                    } else {
                        egui::Button::new("Iniciar com o Windows")
                            .fill(Color32::from_rgb(14, 116, 144))
                            .min_size(egui::vec2(190.0, 28.0))
                    };
                    if ui
                        .add_enabled(self.elevated, autostart_btn)
                        .on_hover_text(
                            "Inicia o Vigia de Afinidade + ISLC Standby Auto-Purge na bandeja 5s após o logon (sem UAC).",
                        )
                        .on_disabled_hover_text(
                            "Execute como Administrador para configurar a inicialização com o Windows.",
                        )
                        .clicked()
                    {
                        if self.watchdog_task_installed {
                            match crate::watchdog::remove_autostart_task() {
                                Ok(m) => {
                                    self.watchdog_task_installed = false;
                                    self.banner = Some((true, m.clone()));
                                    self.push_session_log(m);
                                }
                                Err(e) => {
                                    self.banner = Some((false, e.clone()));
                                    self.push_session_log(e);
                                }
                            }
                        } else {
                            match crate::watchdog::enable_autostart_with_runtime(&mut self.watchdog) {
                                Ok(m) => {
                                    self.watchdog_task_installed = true;
                                    self.banner = Some((true, m.clone()));
                                    self.push_session_log(m);
                                }
                                Err(e) => {
                                    self.banner = Some((false, e.clone()));
                                    self.push_session_log(e);
                                }
                            }
                        }
                    }
                    ui.add_space(4.0);
                    if ui
                        .add(
                            egui::Button::new("Apoiar / Doar")
                                .fill(Color32::from_rgb(17, 66, 50))
                                .min_size(egui::vec2(190.0, 28.0)),
                        )
                        .on_hover_text("O 01LowOS é 100% gratuito e open-source. Apoie o projeto!")
                        .clicked()
                    {
                        self.show_donate_modal = true;
                    }
                    ui.add_space(4.0);
                });
            });

        // Top header bar
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if self.elevated {
                    badge(
                        ui,
                        "ADMIN ATIVO",
                        Color32::from_rgb(16, 185, 129),
                        Color32::from_rgb(6, 78, 59),
                    );
                } else {
                    badge(
                        ui,
                        "SEM ELEVAÇÃO (UAC)",
                        Color32::from_rgb(245, 158, 11),
                        Color32::from_rgb(120, 53, 15),
                    );
                    if ui.button("Reiniciar como Administrador").clicked() {
                        match relaunch_elevated() {
                            Ok(()) => std::process::exit(0),
                            Err(e) => {
                                self.banner = Some((false, e));
                            }
                        }
                    }
                }

                ui.separator();
                if self.watchdog.is_running() {
                    badge(
                        ui,
                        "VIGIA RUST ON",
                        Color32::from_rgb(34, 211, 238),
                        Color32::from_rgb(22, 78, 99),
                    );
                } else {
                    badge(
                        ui,
                        "VIGIA OFF",
                        Color32::from_rgb(148, 163, 184),
                        Color32::from_rgb(30, 41, 59),
                    );
                }

                if let Some((ok, ref text)) = self.banner {
                    ui.separator();
                    let col = if ok {
                        Color32::from_rgb(52, 211, 153)
                    } else {
                        Color32::from_rgb(251, 113, 133)
                    };
                    ui.label(RichText::new(text).color(col).small());
                }
            });
            ui.add_space(4.0);
        });

        // Main central workspace
        egui::CentralPanel::default().show(ctx, |ui| match self.tab {
            Tab::Tweaks => tweaks_view::render(self, ui, ctx),
            Tab::Watchdog => watchdog_view::render(self, ui, ctx),
            Tab::Tools => tools_view::render(self, ui, ctx),
            Tab::Apps => apps_view::render(self, ui, ctx),
            Tab::IsoBuild => iso_view::render(self, ui, ctx),
            Tab::Backups => backups_view::render(self, ui, ctx),
        });

        if self.show_donate_modal {
            let mut open = true;
            egui::Window::new("Apoiar o 01LowOS")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.set_max_width(420.0);
                    ui.label("O 01LowOS é 100% gratuito e de código aberto (licença GPL-3.0).");
                    ui.label(
                        RichText::new("Se o software melhorou seus FPS/latência, você pode apoiar o desenvolvimento:")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add(
                                egui::Button::new("Apoiar no GitHub Sponsors")
                                    .fill(Color32::from_rgb(17, 66, 50)),
                            )
                            .clicked()
                        {
                            if let Err(e) = crate::links::open_url(crate::links::GITHUB_SPONSORS_URL) {
                                self.banner = Some((false, e));
                            }
                        }
                        if ui.button("Abrir no GitHub (Dar Estrela)").clicked() {
                            if let Err(e) = crate::links::open_url(crate::links::GITHUB_REPO_URL) {
                                self.banner = Some((false, e));
                            }
                        }
                    });
                    if !crate::links::DONATE_PIX_KEY.is_empty() {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("PIX: {}", crate::links::DONATE_PIX_KEY)).monospace());
                            if ui.button("Copiar chave PIX").clicked() {
                                ctx.copy_text(crate::links::DONATE_PIX_KEY.to_string());
                                self.banner = Some((true, "Chave PIX copiada para a área de transferência.".into()));
                            }
                        });
                    }
                    ui.add_space(10.0);
                    if ui.button("Fechar").clicked() {
                        self.show_donate_modal = false;
                    }
                });
            if !open {
                self.show_donate_modal = false;
            }
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.watchdog.stop();
        self.tray = None;
    }
}

pub fn badge(ui: &mut egui::Ui, text: &str, fg: Color32, bg: Color32) {
    egui::Frame::NONE
        .fill(bg)
        .stroke(Stroke::new(1.0_f32, fg.linear_multiply(0.5)))
        .corner_radius(4.0)
        .inner_margin(egui::Margin::symmetric(7, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(fg).small().strong());
        });
}

fn configure_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.window_fill = Color32::from_rgb(15, 20, 28);
    visuals.panel_fill = Color32::from_rgb(15, 20, 28);
    visuals.extreme_bg_color = Color32::from_rgb(10, 14, 20);
    visuals.widgets.noninteractive.bg_fill = Color32::from_rgb(23, 30, 41);
    visuals.widgets.inactive.bg_fill = Color32::from_rgb(30, 41, 59);
    visuals.widgets.hovered.bg_fill = Color32::from_rgb(51, 65, 85);
    visuals.selection.bg_fill = Color32::from_rgb(14, 116, 144);
    ctx.set_visuals(visuals);
}
