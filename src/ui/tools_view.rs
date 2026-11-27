use eframe::egui::{self, Color32, RichText, Stroke};

use crate::sys::memory::query_ram;
use crate::tools::read_gaming_health;
use crate::ui::{badge, AppMsg, LowOsApp};

pub fn render(app: &mut LowOsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.heading("Ferramentas Extras de Latência & Manutenção");
    ui.label(
        RichText::new("Diagnóstico ao vivo e utilitários nativos sob demanda (ISLC nativo, CFG por jogo DX12, limpeza de Shaders e Temporários).")
            .small()
            .color(Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(8.0);

    egui::ScrollArea::vertical()
        .id_salt("tools_scroll")
        .show(ui, |ui| {
            // 0. Live Gaming Health Diagnostics
            egui::Frame::NONE
                .fill(Color32::from_rgb(23, 30, 41))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                .corner_radius(8.0)
                .inner_margin(10.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("Diagnóstico Rápido de Saúde Gamer (Tempo Real)").strong());
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        for item in read_gaming_health() {
                            let (fg, bg) = if item.ok {
                                (Color32::from_rgb(52, 211, 153), Color32::from_rgb(6, 78, 59))
                            } else {
                                (Color32::from_rgb(251, 191, 36), Color32::from_rgb(120, 53, 15))
                            };
                            badge(ui, &format!("{}: {}", item.label, item.detail), fg, bg);
                        }
                    });
                });

            ui.add_space(8.0);

            // 1. Standby List Cleaner
            egui::Frame::NONE
                .fill(Color32::from_rgb(23, 30, 41))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                .corner_radius(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("1. Limpador de Standby List (ISLC Nativo em Rust)").strong());
                            if let Some(ram) = query_ram() {
                                ui.label(format!(
                                    "RAM Física: {} MB em uso ({}%)  |  {} MB disponível  |  Total: {} MB",
                                    ram.used_mb, ram.load_pct, ram.avail_mb, ram.total_mb
                                ));
                            } else {
                                ui.label("Não foi possível ler o uso de memória RAM.");
                            }
                            ui.label(
                                RichText::new("Útil principalmente em PCs com 16 GB de RAM ou jogos DX12 pesados. Você também pode ativar a limpeza automática na aba 'Vigia de Afinidade'.")
                                    .small()
                                    .color(Color32::from_rgb(148, 163, 184)),
                            );
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add_enabled(
                                    app.elevated && !app.tools_busy,
                                    egui::Button::new("Limpar Standby List Agora")
                                        .fill(Color32::from_rgb(14, 116, 144)),
                                )
                                .on_disabled_hover_text("Execute como Administrador para limpar a Standby List.")
                                .clicked()
                            {
                                app.tools_busy = true;
                                let tx = app.tx.clone();
                                let ctx = ctx.clone();
                                std::thread::spawn(move || {
                                    let res = crate::sys::memory::purge_standby_list();
                                    let (ok, text) = match res {
                                        Ok(m) => (true, m),
                                        Err(e) => (false, e),
                                    };
                                    let _ = tx.send(AppMsg::ToolBusyDone {
                                        ok,
                                        text,
                                        refresh_cfg: false,
                                        refresh_shaders: false,
                                        refresh_temp: false,
                                    });
                                    ctx.request_repaint();
                                });
                            }
                        });
                    });
                });

            ui.add_space(8.0);

            // 2. Per-game DX12 Control Flow Guard (CFG)
            egui::Frame::NONE
                .fill(Color32::from_rgb(23, 30, 41))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                .corner_radius(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("2. Control Flow Guard (CFG) por Jogo DX12").strong());
                        badge(
                            ui,
                            "PROTEÇÃO ANTI-VANGUARD (VAN9002)",
                            Color32::from_rgb(251, 191, 36),
                            Color32::from_rgb(120, 53, 15),
                        );
                    });
                    ui.label(
                        RichText::new("Desativar CFG apenas no .exe específico de um jogo DX12 reduz micro-stutters causados por chamadas rápidas de API. Nunca desative globalmente nem em jogos da Riot (bloqueado automaticamente aqui para evitar VAN9002).")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.cfg_exe_input)
                                .desired_width(260.0)
                                .hint_text("Ex.: Cyberpunk2077.exe ou bf1.exe"),
                        );
                        if ui
                            .add_enabled(
                                app.elevated && !app.tools_busy,
                                egui::Button::new("Desativar CFG no .exe")
                                    .fill(Color32::from_rgb(14, 116, 144)),
                            )
                            .on_disabled_hover_text("Requer execução como Administrador.")
                            .clicked()
                        {
                            let target = app.cfg_exe_input.clone();
                            app.tools_busy = true;
                            let tx = app.tx.clone();
                            let ctx = ctx.clone();
                            std::thread::spawn(move || {
                                let res = crate::tools::disable_cfg_for_exe(&target);
                                let (ok, text) = match res {
                                    Ok(m) => (true, m),
                                    Err(e) => (false, e),
                                };
                                let _ = tx.send(AppMsg::ToolBusyDone {
                                    ok,
                                    text,
                                    refresh_cfg: ok,
                                    refresh_shaders: false,
                                    refresh_temp: false,
                                });
                                ctx.request_repaint();
                            });
                        }
                        if ui
                            .add_enabled(!app.tools_busy, egui::Button::new("Atualizar lista"))
                            .clicked()
                        {
                            app.trigger_refresh_cfg(ctx);
                        }
                    });

                    ui.add_space(6.0);
                    if app.cfg_disabled_list.is_empty() {
                        ui.label(
                            RichText::new("Nenhum executável com CFG desativado por política individual.")
                                .small()
                                .color(Color32::from_rgb(148, 163, 184)),
                        );
                    } else {
                        let list = app.cfg_disabled_list.clone();
                        for exe in &list {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(exe).monospace());
                                if ui
                                    .add_enabled(
                                        app.elevated && !app.tools_busy,
                                        egui::Button::new("Restaurar CFG"),
                                    )
                                    .clicked()
                                {
                                    let target = exe.clone();
                                    app.tools_busy = true;
                                    let tx = app.tx.clone();
                                    let ctx = ctx.clone();
                                    std::thread::spawn(move || {
                                        let res = crate::tools::restore_cfg_for_exe(&target);
                                        let (ok, text) = match res {
                                            Ok(m) => (true, m),
                                            Err(e) => (false, e),
                                        };
                                        let _ = tx.send(AppMsg::ToolBusyDone {
                                            ok,
                                            text,
                                            refresh_cfg: ok,
                                            refresh_shaders: false,
                                            refresh_temp: false,
                                        });
                                        ctx.request_repaint();
                                    });
                                }
                            });
                        }
                    }
                });

            ui.add_space(8.0);

            // 3. On-Demand Shader Cache Cleaner
            egui::Frame::NONE
                .fill(Color32::from_rgb(23, 30, 41))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                .corner_radius(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("3. Limpador de Cache de Shaders (Sob Demanda)").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !app.confirm_clean_shaders {
                                if ui
                                    .add_enabled(
                                        !app.tools_busy,
                                        egui::Button::new("Limpar Caches de Shader")
                                            .fill(Color32::from_rgb(136, 19, 55)),
                                    )
                                    .clicked()
                                {
                                    app.confirm_clean_shaders = true;
                                }
                            } else {
                                if ui.button("Cancelar").clicked() {
                                    app.confirm_clean_shaders = false;
                                }
                                if ui
                                    .add(
                                        egui::Button::new("Confirmar Limpeza (Recompila na 1ª partida)")
                                            .fill(Color32::from_rgb(185, 28, 28)),
                                    )
                                    .clicked()
                                {
                                    app.confirm_clean_shaders = false;
                                    app.tools_busy = true;
                                    let tx = app.tx.clone();
                                    let ctx = ctx.clone();
                                    std::thread::spawn(move || {
                                        let (deleted, freed, skipped) = crate::tools::clean_shader_caches();
                                        let mb = freed as f64 / (1024.0 * 1024.0);
                                        let text = format!(
                                            "Cache de shaders limpo: {deleted} arquivo(s) removido(s) ({mb:.1} MB liberados, {skipped} em uso ignorado(s))."
                                        );
                                        let _ = tx.send(AppMsg::ToolBusyDone {
                                            ok: true,
                                            text,
                                            refresh_cfg: false,
                                            refresh_shaders: true,
                                            refresh_temp: false,
                                        });
                                        ctx.request_repaint();
                                    });
                                }
                            }
                            if ui
                                .add_enabled(!app.tools_busy, egui::Button::new("Escanear"))
                                .clicked()
                            {
                                app.trigger_scan_shaders(ctx);
                            }
                        });
                    });
                    ui.label(
                        RichText::new("Use apenas ao trocar de driver de GPU ou se suspeitar de cache corrompido. Na primeira partida após limpar, o jogo recompilará shaders normalmente.")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(6.0);
                    if app.shader_dirs.is_empty() {
                        ui.label("Nenhuma pasta de cache de shaders encontrada.");
                    } else {
                        for d in &app.shader_dirs {
                            let mb = d.bytes as f64 / (1024.0 * 1024.0);
                            ui.label(
                                RichText::new(format!(
                                    "{:<18} | {:>5} arquivo(s) | {:>7.1} MB | {}",
                                    d.label,
                                    d.files,
                                    mb,
                                    d.path.display()
                                ))
                                .monospace()
                                .small(),
                            );
                        }
                    }
                });

            ui.add_space(8.0);

            // 4. Temp & Windows Update Cache Cleaner
            egui::Frame::NONE
                .fill(Color32::from_rgb(23, 30, 41))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                .corner_radius(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("4. Limpador de Temporários & Cache Windows Update").strong());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !app.confirm_clean_temp {
                                if ui
                                    .add_enabled(
                                        !app.tools_busy,
                                        egui::Button::new("Limpar Temporários")
                                            .fill(Color32::from_rgb(136, 19, 55)),
                                    )
                                    .clicked()
                                {
                                    app.confirm_clean_temp = true;
                                }
                            } else {
                                if ui.button("Cancelar").clicked() {
                                    app.confirm_clean_temp = false;
                                }
                                if ui
                                    .add(
                                        egui::Button::new("Confirmar Limpeza de Temporários")
                                            .fill(Color32::from_rgb(185, 28, 28)),
                                    )
                                    .clicked()
                                {
                                    app.confirm_clean_temp = false;
                                    app.tools_busy = true;
                                    let tx = app.tx.clone();
                                    let ctx = ctx.clone();
                                    std::thread::spawn(move || {
                                        let (deleted, freed, skipped) = crate::tools::clean_temp_caches();
                                        let mb = freed as f64 / (1024.0 * 1024.0);
                                        let text = format!(
                                            "Temporários limpos: {deleted} arquivo(s) removido(s) ({mb:.1} MB liberados, {skipped} em uso ignorado(s))."
                                        );
                                        let _ = tx.send(AppMsg::ToolBusyDone {
                                            ok: true,
                                            text,
                                            refresh_cfg: false,
                                            refresh_shaders: false,
                                            refresh_temp: true,
                                        });
                                        ctx.request_repaint();
                                    });
                                }
                            }
                            if ui
                                .add_enabled(!app.tools_busy, egui::Button::new("Escanear"))
                                .clicked()
                            {
                                app.trigger_scan_temp(ctx);
                            }
                        });
                    });
                    ui.label(
                        RichText::new("Remove arquivos temporários (%TEMP%, Windows\\Temp) e downloads antigos do Windows Update (SoftwareDistribution\\Download), ignorando arquivos em uso.")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(6.0);
                    if app.temp_dirs.is_empty() {
                        ui.label("Nenhuma pasta temporária encontrada.");
                    } else {
                        for d in &app.temp_dirs {
                            let mb = d.bytes as f64 / (1024.0 * 1024.0);
                            ui.label(
                                RichText::new(format!(
                                    "{:<20} | {:>5} arquivo(s) | {:>7.1} MB | {}",
                                    d.label,
                                    d.files,
                                    mb,
                                    d.path.display()
                                ))
                                .monospace()
                                .small(),
                            );
                        }
                    }
                });

            ui.add_space(8.0);

            // 5. Per-game Fullscreen Optimizations (FSO) Manager
            egui::Frame::NONE
                .fill(Color32::from_rgb(23, 30, 41))
                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                .corner_radius(8.0)
                .inner_margin(12.0)
                .show(ui, |ui| {
                    ui.label(RichText::new("5. Fullscreen Optimizations (FSO) por Jogo — Exclusive Fullscreen").strong());
                    ui.label(
                        RichText::new("Aplica DISABLEDXMAXIMIZEDWINDOWEDMODE via HKCU\\AppCompatFlags\\Layers sem alterar arquivos do jogo (100% seguro com anti-cheats, não requer Admin).")
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut app.fso_exe_input)
                                .desired_width(360.0)
                                .hint_text(r"Ex.: C:\Program Files (x86)\Steam\steamapps\common\...\cs2.exe"),
                        );
                        if ui
                            .button("Desativar FSO no .exe")
                            .on_hover_text("Adiciona DISABLEDXMAXIMIZEDWINDOWEDMODE para o executável informado")
                            .clicked()
                        {
                            match crate::tools::disable_fso_for_exe(&app.fso_exe_input) {
                                Ok(msg) => {
                                    app.fso_exe_input.clear();
                                    app.fso_disabled_list = crate::tools::list_fso_disabled_exes();
                                    app.banner = Some((true, msg.clone()));
                                    app.push_session_log(msg);
                                }
                                Err(err) => {
                                    app.banner = Some((false, err));
                                }
                            }
                        }
                        if ui.button("Atualizar lista").clicked() {
                            app.fso_disabled_list = crate::tools::list_fso_disabled_exes();
                        }
                    });
                    ui.add_space(6.0);
                    if app.fso_disabled_list.is_empty() {
                        ui.label(
                            RichText::new("Nenhum jogo com FSO desativado em AppCompatFlags\\Layers.")
                                .small()
                                .color(Color32::from_rgb(148, 163, 184)),
                        );
                    } else {
                        let list = app.fso_disabled_list.clone();
                        for exe in &list {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(exe).monospace().small());
                                if ui.small_button("Restaurar").clicked() {
                                    match crate::tools::restore_fso_for_exe(exe) {
                                        Ok(msg) => {
                                            app.fso_disabled_list = crate::tools::list_fso_disabled_exes();
                                            app.banner = Some((true, msg.clone()));
                                            app.push_session_log(msg);
                                        }
                                        Err(err) => {
                                            app.banner = Some((false, err));
                                        }
                                    }
                                }
                            });
                        }
                    }
                });
        });
}
