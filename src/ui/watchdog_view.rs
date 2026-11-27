use eframe::egui::{self, Color32, RichText, Stroke};

use crate::ui::{badge, AppMsg, LowOsApp};
use crate::watchdog::{install_autostart_task, remove_autostart_task};

pub fn render(app: &mut LowOsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.heading("Vigia de Afinidade de Jogos (Nativo em Rust)");
    ui.label(
        RichText::new("Substitui o WMI/PowerShell por polling Win32 leve (<0,1% CPU): mantém jogos fora do núcleo 0 (onde ficam IRQs da GPU) e isola o fundo no núcleo 0 com restauração automática ao fechar o jogo.")
            .small()
            .color(Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(8.0);

    let snap = app.watchdog.snapshot.lock().map(|g| g.clone()).unwrap_or_default();

    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 30, 41))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
        .corner_radius(8.0)
        .inner_margin(12.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Topologia da CPU Detectada").strong());
                    ui.label(format!(
                        "Threads lógicas: {}  |  Núcleos físicos: {}  |  SMT: {}",
                        snap.topology.logical_count,
                        snap.topology.physical_cores,
                        if snap.topology.smt_enabled { "Ligado" } else { "Desligado" }
                    ));
                    ui.label(
                        RichText::new(format!(
                            "Máscara de Jogo: 0x{:X}   |   Máscara de Fundo (Núcleo 0): 0x{:X}",
                            snap.topology.game_mask, snap.topology.background_mask
                        ))
                        .monospace()
                        .color(Color32::from_rgb(34, 211, 238)),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Minimizar para Bandeja")
                        .on_hover_text("Oculta a janela mantendo o Vigia ativo na bandeja do sistema")
                        .clicked()
                    {
                        if !app.watchdog.is_running() {
                            app.watchdog.start();
                        }
                        app.minimize_to_tray(ctx);
                    }
                    if app.watchdog.is_running() {
                        if ui
                            .add(
                                egui::Button::new("Parar Vigia e Restaurar")
                                    .fill(Color32::from_rgb(136, 19, 55)),
                            )
                            .clicked()
                        {
                            app.watchdog.stop();
                        }
                    } else if ui
                        .add(
                            egui::Button::new("Iniciar Vigia Agora")
                                .fill(Color32::from_rgb(14, 116, 144)),
                        )
                        .clicked()
                    {
                        app.watchdog.start();
                    }

                    if app.elevated {
                        if app.watchdog_task_installed {
                            if ui.button("Remover Tarefa de Logon").clicked() {
                                match remove_autostart_task() {
                                    Ok(m) => {
                                        app.watchdog_task_installed = false;
                                        let _ = app.tx.send(AppMsg::Banner { ok: true, text: m });
                                    }
                                    Err(e) => {
                                        let _ = app.tx.send(AppMsg::Banner { ok: false, text: e });
                                    }
                                }
                            }
                        } else if ui.button("Instalar Tarefa no Logon (--watchdog)").clicked() {
                            match install_autostart_task() {
                                Ok(m) => {
                                    app.watchdog_task_installed = true;
                                    let _ = app.tx.send(AppMsg::Banner { ok: true, text: m });
                                }
                                Err(e) => {
                                    let _ = app.tx.send(AppMsg::Banner { ok: false, text: e });
                                }
                            }
                        }
                    }
                });
            });

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if let Some(ref game) = snap.active_game {
                    badge(
                        ui,
                        &format!(
                            "JOGO: {} (PID {}) | {}",
                            game,
                            snap.active_game_pid.unwrap_or(0),
                            if snap.gpu_confirmed { "GPU 3D OK" } else { "Aguardando GPU 3D" }
                        ),
                        Color32::from_rgb(52, 211, 153),
                        Color32::from_rgb(6, 78, 59),
                    );
                } else {
                    badge(
                        ui,
                        "AGUARDANDO JOGO",
                        Color32::from_rgb(148, 163, 184),
                        Color32::from_rgb(30, 41, 59),
                    );
                }
                badge(
                    ui,
                    &format!("Processos isolados: {}", snap.isolated_count),
                    Color32::from_rgb(34, 211, 238),
                    Color32::from_rgb(22, 78, 99),
                );
                badge(
                    ui,
                    &format!("Protegidos/recusados: {}", snap.refused_count),
                    Color32::from_rgb(251, 191, 36),
                    Color32::from_rgb(120, 53, 15),
                );
                if app.watchdog_task_installed {
                    badge(
                        ui,
                        "Autostart no Logon: ATIVO",
                        Color32::from_rgb(52, 211, 153),
                        Color32::from_rgb(6, 78, 59),
                    );
                }
            });
        });

    ui.add_space(10.0);

    // Editable extra folders & ignored processes
    let mut cfg = snap.config.clone();
    let mut cfg_changed = false;
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 30, 41))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.columns(2, |cols| {
                cols[0].label(RichText::new("Pastas Extras de Jogos").strong());
                cols[0].horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.wd_new_folder)
                            .desired_width(240.0)
                            .hint_text(r"Ex.: D:\MeusJogos"),
                    );
                    if ui.button("Adicionar").clicked() {
                        let v = app.wd_new_folder.trim().to_string();
                        if !v.is_empty() && !cfg.extra_folders.iter().any(|x| x.eq_ignore_ascii_case(&v)) {
                            cfg.extra_folders.push(v);
                            app.wd_new_folder.clear();
                            cfg_changed = true;
                        }
                    }
                });
                let mut rm_folder = None;
                for (idx, f) in cfg.extra_folders.iter().enumerate() {
                    cols[0].horizontal(|ui| {
                        ui.label(RichText::new(f).monospace().small());
                        if ui.small_button("x").clicked() {
                            rm_folder = Some(idx);
                        }
                    });
                }
                if let Some(idx) = rm_folder {
                    cfg.extra_folders.remove(idx);
                    cfg_changed = true;
                }

                cols[1].label(RichText::new("Processos Ignorados (sem .exe)").strong());
                cols[1].horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut app.wd_new_ignored)
                            .desired_width(220.0)
                            .hint_text("Ex.: wallpaper32"),
                    );
                    if ui.button("Adicionar").clicked() {
                        let v = app
                            .wd_new_ignored
                            .trim()
                            .trim_end_matches(".exe")
                            .trim_end_matches(".EXE")
                            .to_string();
                        if !v.is_empty()
                            && !cfg.ignored_processes.iter().any(|x| x.eq_ignore_ascii_case(&v))
                        {
                            cfg.ignored_processes.push(v);
                            app.wd_new_ignored.clear();
                            cfg_changed = true;
                        }
                    }
                });
                let mut rm_ign = None;
                cols[1].horizontal_wrapped(|ui| {
                    for (idx, p) in cfg.ignored_processes.iter().enumerate() {
                        if ui.small_button(format!("{p} [x]")).clicked() {
                            rm_ign = Some(idx);
                        }
                    }
                });
                if let Some(idx) = rm_ign {
                    cfg.ignored_processes.remove(idx);
                    cfg_changed = true;
                }
            });
            ui.add_space(6.0);
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .checkbox(
                        &mut cfg.auto_purge_standby,
                        format!(
                            "Limpar Standby List em jogo se RAM livre < {} MB (ISLC nativo)",
                            cfg.purge_free_threshold_mb
                        ),
                    )
                    .changed()
                {
                    cfg_changed = true;
                }
                if let Some(ram) = crate::sys::memory::query_ram() {
                    ui.label(
                        RichText::new(format!("RAM livre: {} MB / {} MB", ram.avail_mb, ram.total_mb))
                            .small()
                            .monospace()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                }
                if ui
                    .add_enabled(app.elevated, egui::Button::new("Limpar Standby Agora"))
                    .on_disabled_hover_text("Execute como Administrador para limpar a Standby List.")
                    .clicked()
                {
                    match crate::sys::memory::purge_standby_list() {
                        Ok(m) => {
                            let _ = app.tx.send(AppMsg::Banner { ok: true, text: m });
                        }
                        Err(e) => {
                            let _ = app.tx.send(AppMsg::Banner { ok: false, text: e });
                        }
                    }
                }
            });
        });
    if cfg_changed {
        match app.watchdog.update_config(cfg) {
            Ok(()) => {
                let _ = app.tx.send(AppMsg::Banner {
                    ok: true,
                    text: "Configuração do Vigia salva.".into(),
                });
            }
            Err(e) => {
                let _ = app.tx.send(AppMsg::Banner { ok: false, text: e });
            }
        }
    }

    ui.add_space(10.0);

    ui.columns(2, |cols| {
        cols[0].horizontal(|ui| {
            ui.label(RichText::new(format!("Pastas de Jogos Monitoradas ({})", snap.folders.len())).strong());
            if ui.small_button("Atualizar").clicked() {
                app.watchdog.refresh_folders();
                ctx.request_repaint();
            }
        });
        egui::ScrollArea::vertical()
            .id_salt("wd_folders")
            .max_height(260.0)
            .show(&mut cols[0], |ui| {
                for f in &snap.folders {
                    ui.label(
                        RichText::new(f.to_string_lossy().as_ref())
                            .monospace()
                            .small(),
                    );
                }
            });

        cols[1].label(RichText::new("Log em Tempo Real do Vigia").strong());
        egui::ScrollArea::vertical()
            .id_salt("wd_logs")
            .stick_to_bottom(true)
            .max_height(260.0)
            .show(&mut cols[1], |ui| {
                for l in &snap.logs {
                    ui.label(
                        RichText::new(l)
                            .monospace()
                            .small()
                            .color(Color32::from_rgb(203, 213, 225)),
                    );
                }
            });
    });
}
