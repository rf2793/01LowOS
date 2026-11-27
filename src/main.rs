mod apps;
mod iso;
mod links;
mod sys;
mod tools;
mod tweaks;
mod ui;
mod watchdog;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use eframe::egui;

use crate::sys::backup::BackupStore;
use crate::sys::elevation::is_elevated;
use crate::tweaks::catalog::{build_catalog, validate_catalog};
use crate::ui::LowOsApp;
use crate::watchdog::WatchdogEngine;

fn print_help() {
    println!("01LowOS — Software Modular em Rust");
    println!("Uso:");
    println!("  01LowOS.exe                Abre a interface gráfica");
    println!("  01LowOS.exe --list         Lista todas as melhorias disponíveis");
    println!("  01LowOS.exe --status       Verifica o estado atual no sistema (somente leitura)");
    println!("  01LowOS.exe --apply <id>         Aplica uma melhoria específica com backup");
    println!("  01LowOS.exe --revert <id>        Reverte uma melhoria específica pelo backup");
    println!("  01LowOS.exe --reset-default <id> Restaura para o padrão do Windows sem backup");
    println!("  01LowOS.exe --self-test          Testa aplicar+reverter melhorias pendentes");
    println!("  01LowOS.exe --self-test-all      Inclui AppX e Optional Features no auto-teste");
    println!("  01LowOS.exe --watchdog           Executa apenas o Vigia de Afinidade em segundo plano");
}

fn run_list() -> i32 {
    let catalog = build_catalog();
    if let Err(e) = validate_catalog(&catalog) {
        eprintln!("ERRO no catálogo: {e}");
        return 1;
    }
    println!("{:<32} | {:<20} | {:<6} | Nome", "ID", "Categoria", "Risco");
    println!("{}", "-".repeat(100));
    for t in &catalog {
        let m = t.meta();
        println!(
            "{:<32} | {:<20} | {:<6} | {}",
            m.id,
            m.category.label(),
            m.risk.label(),
            m.name
        );
    }
    println!("\nTotal: {} melhorias.", catalog.len());
    0
}

fn run_status() -> i32 {
    let catalog = build_catalog();
    let backups = BackupStore::default_store();
    println!(
        "01LowOS Status (Admin: {})\n",
        if is_elevated() { "SIM" } else { "NÃO" }
    );
    println!(
        "{:<32} | {:<26} | {:<8} | Nome",
        "ID", "Status", "Backup"
    );
    println!("{}", "-".repeat(105));
    for t in &catalog {
        let m = t.meta();
        let st = t.check();
        let bk = if backups.exists(m.id) { "SIM" } else { "-" };
        println!("{:<32} | {:<26} | {:<8} | {}", m.id, st.label(), bk, m.name);
    }
    0
}

fn run_apply_or_revert(id: &str, mode: u8) -> i32 {
    let catalog = build_catalog();
    let Some(tweak) = catalog.iter().find(|t| t.meta().id == id) else {
        eprintln!("Melhoria não encontrada: {id}");
        return 1;
    };
    if tweak.meta().requires_admin && !is_elevated() {
        eprintln!("Esta melhoria requer execução como Administrador.");
        return 1;
    }
    let backups = BackupStore::default_store();
    let res = match mode {
        0 => tweak.apply(&backups),
        1 => tweak.revert(&backups),
        _ => tweak.reset_to_default(),
    };
    match res {
        Ok(msg) => {
            crate::sys::logger::append_log(&format!("[CLI] OK [{id}]: {msg}"));
            println!("OK [{id}]: {msg}");
            0
        }
        Err(err) => {
            crate::sys::logger::append_log(&format!("[CLI] ERRO [{id}]: {err}"));
            eprintln!("ERRO [{id}]: {err}");
            1
        }
    }
}

fn run_watchdog_headless() -> i32 {
    unsafe {
        windows_sys::Win32::System::Console::FreeConsole();
    }
    let mut engine = WatchdogEngine::new();
    engine.start();
    let alive = Arc::new(AtomicBool::new(true));
    let alive_cb = Arc::clone(&alive);
    let _tray = crate::sys::tray::TrayIcon::new(
        "01LowOS — Vigia + ISLC Ativo (Clique direito para abrir)",
        Arc::new(move |action| match action {
            crate::sys::tray::TrayAction::Restore => {
                if let Ok(exe) = std::env::current_exe() {
                    let _ = std::process::Command::new(exe)
                        .arg("--start-watchdog")
                        .spawn();
                }
                alive_cb.store(false, Ordering::SeqCst);
            }
            crate::sys::tray::TrayAction::Exit => {
                alive_cb.store(false, Ordering::SeqCst);
            }
        }),
    )
    .ok();

    while alive.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(400));
    }
    engine.stop();
    0
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(cmd) = args.first().map(String::as_str) {
        if cmd == "--start-watchdog" {
            // Fall through into GUI with Watchdog auto-started
        } else {
            let code = match cmd {
            "--help" | "-h" => {
                print_help();
                0
            }
            "--list" => run_list(),
            "--status" => run_status(),
            "--apply" => match args.get(1) {
                Some(id) => run_apply_or_revert(id, 0),
                None => {
                    eprintln!("Uso: 01LowOS.exe --apply <id>");
                    1
                }
            },
            "--revert" => match args.get(1) {
                Some(id) => run_apply_or_revert(id, 1),
                None => {
                    eprintln!("Uso: 01LowOS.exe --revert <id>");
                    1
                }
            },
            "--reset-default" => match args.get(1) {
                Some(id) => run_apply_or_revert(id, 2),
                None => {
                    eprintln!("Uso: 01LowOS.exe --reset-default <id>");
                    1
                }
            },
            "--self-test" => {
                let catalog: Vec<std::sync::Arc<dyn crate::tweaks::Tweak>> =
                    build_catalog().into_iter().map(std::sync::Arc::from).collect();
                let backups = BackupStore::default_store();
                let summary = crate::tweaks::run_self_test(&catalog, &backups, false, |line| {
                    println!("{line}");
                });
                println!("Relatório salvo em: {}", summary.report_path.display());
                if summary.failed == 0 { 0 } else { 1 }
            }
            "--self-test-all" => {
                let catalog: Vec<std::sync::Arc<dyn crate::tweaks::Tweak>> =
                    build_catalog().into_iter().map(std::sync::Arc::from).collect();
                let backups = BackupStore::default_store();
                let summary = crate::tweaks::run_self_test(&catalog, &backups, true, |line| {
                    println!("{line}");
                });
                println!("Relatório salvo em: {}", summary.report_path.display());
                if summary.failed == 0 { 0 } else { 1 }
            }
            "--watchdog" => run_watchdog_headless(),
            other => {
                eprintln!("Argumento desconhecido: {other}");
                print_help();
                1
            }
        };
        std::process::exit(code);
        }
    }

    unsafe {
        windows_sys::Win32::System::Console::FreeConsole();
    }

    let icon_rgba = include_bytes!(concat!(env!("OUT_DIR"), "/icon.rgba")).to_vec();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1220.0, 780.0])
            .with_min_inner_size([920.0, 600.0])
            .with_icon(Arc::new(egui::IconData {
                rgba: icon_rgba,
                width: 32,
                height: 32,
            })),
        ..Default::default()
    };

    if let Err(err) = eframe::run_native(
        "01LowOS — Otimizador Modular em Rust",
        options,
        Box::new(|cc| Ok(Box::new(LowOsApp::new(cc)))),
    ) {
        eprintln!("Erro ao iniciar interface gráfica: {err}");
        std::process::exit(1);
    }
}
