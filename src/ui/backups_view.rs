use chrono::Local;
use eframe::egui::{self, Color32, RichText, Stroke};

use crate::tweaks::TweakStatus;
use crate::ui::LowOsApp;

pub fn render(app: &mut LowOsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.heading("Backups Individuais & Logs da Sessão");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Recarregar lista").clicked() {
                app.trigger_refresh_backups(ctx);
            }
            if ui
                .button("Abrir Log (.log)")
                .on_hover_text("Abre o arquivo de log persistente do 01LowOS no Bloco de Notas")
                .clicked()
            {
                match crate::sys::logger::open_log_file() {
                    Ok(m) => {
                        app.banner = Some((true, m));
                    }
                    Err(e) => {
                        app.banner = Some((false, e));
                    }
                }
            }
            if ui
                .button("Exportar Relatório (.txt)")
                .on_hover_text(format!(
                    "Salva um relatório completo em texto com hardware e estado das {} otimizações",
                    app.catalog.len()
                ))
                .clicked()
            {
                let base = if app.elevated {
                    std::env::var_os("ProgramData")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\ProgramData"))
                } else {
                    std::env::var_os("LOCALAPPDATA")
                        .map(std::path::PathBuf::from)
                        .unwrap_or_else(std::env::temp_dir)
                };
                let report_path = base.join("01LowOS").join("relatorio_sistema.txt");
                if let Some(p) = report_path.parent() {
                    let _ = std::fs::create_dir_all(p);
                }

                let snap = app
                    .watchdog
                    .snapshot
                    .lock()
                    .map(|g| g.clone())
                    .unwrap_or_default();

                let mut applied = 0usize;
                let mut partial = 0usize;
                let mut pending = 0usize;
                let mut na = 0usize;
                let mut bk_count = 0usize;
                let mut rows = Vec::new();
                for t in &app.catalog {
                    let m = t.meta();
                    let st = app
                        .statuses
                        .get(m.id)
                        .cloned()
                        .unwrap_or(TweakStatus::NotApplied);
                    let has_bk = *app.backup_flags.get(m.id).unwrap_or(&false);
                    if has_bk {
                        bk_count += 1;
                    }
                    match &st {
                        TweakStatus::Applied => applied += 1,
                        TweakStatus::Partial(_) => partial += 1,
                        TweakStatus::NotApplicable(_) => na += 1,
                        _ => pending += 1,
                    }
                    rows.push(format!(
                        "{:<40} | {:<28} | {:<6} | {:<20} | {}",
                        m.id,
                        st.label(),
                        if has_bk { "SIM" } else { "-" },
                        m.category.label(),
                        m.name
                    ));
                }

                let mut out = Vec::new();
                out.push("=========================================================================".to_string());
                out.push(format!(
                    "01LowOS — Relatório do Sistema ({})",
                    Local::now().format("%d/%m/%Y %H:%M:%S")
                ));
                out.push(format!(
                    "Admin: {} | Watchdog: {} | Threads: {} | Núcleos Físicos: {} | SMT: {}",
                    if app.elevated { "SIM" } else { "NÃO" },
                    if app.watchdog.is_running() { "ATIVO" } else { "INATIVO" },
                    snap.topology.logical_count,
                    snap.topology.physical_cores,
                    if snap.topology.smt_enabled { "ON" } else { "OFF" }
                ));
                out.push(format!(
                    "Máscaras Watchdog -> Jogo: 0x{:X} | Fundo: 0x{:X}",
                    snap.topology.game_mask, snap.topology.background_mask
                ));
                out.push(format!(
                    "Resumo ({} otimizações): Aplicadas={applied} | Parciais={partial} | Pendentes={pending} | N/A={na} | Backups={bk_count}",
                    app.catalog.len()
                ));
                out.push("=========================================================================".to_string());
                out.push(format!(
                    "{:<40} | {:<28} | {:<6} | {:<20} | {}",
                    "ID", "Status", "Backup", "Categoria", "Nome"
                ));
                out.push("-".repeat(130));
                out.extend(rows);

                match std::fs::write(&report_path, out.join("\r\n")) {
                    Ok(()) => {
                        let msg = format!("Relatório exportado em {}", report_path.display());
                        app.banner = Some((true, msg.clone()));
                        app.push_session_log(msg);
                    }
                    Err(e) => {
                        let msg = format!("Falha ao exportar relatório: {e}");
                        app.banner = Some((false, msg.clone()));
                        app.push_session_log(msg);
                    }
                }
            }
            let test_btn = ui.add_enabled(
                !app.self_test_busy,
                egui::Button::new(if app.self_test_busy {
                    "Testando..."
                } else {
                    "Executar Auto-Teste (Aplicar + Reverter)"
                })
                .fill(Color32::from_rgb(14, 116, 144)),
            );
            if test_btn.clicked() {
                app.self_test_busy = true;
                let catalog = app.catalog.clone();
                let backups = app.backups.clone();
                let tx = app.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let sum = crate::tweaks::run_self_test(&catalog, &backups, false, |line| {
                        let _ = tx.send(crate::ui::AppMsg::SelfTestLog(line.to_string()));
                        ctx.request_repaint();
                    });
                    let _ = tx.send(crate::ui::AppMsg::SelfTestDone {
                        passed: sum.passed,
                        failed: sum.failed,
                        skipped: sum.skipped,
                        report: sum.report_path.display().to_string(),
                    });
                    ctx.request_repaint();
                });
            }
            if !app.backup_records.is_empty() {
                let revertible_count = app
                    .backup_records
                    .iter()
                    .filter(|r| {
                        app.catalog
                            .iter()
                            .find(|t| t.meta().id == r.tweak_id)
                            .is_some_and(|t| !t.meta().requires_admin || app.elevated)
                    })
                    .count();
                if !app.confirm_revert_all {
                    if ui
                        .add_enabled(
                            revertible_count > 0,
                            egui::Button::new("Reverter TODOS os backups")
                                .fill(Color32::from_rgb(136, 19, 55)),
                        )
                        .on_disabled_hover_text(
                            "Os backups salvos exigem execução como Administrador para reverter.",
                        )
                        .clicked()
                    {
                        app.confirm_revert_all = true;
                    }
                } else {
                    if ui.button("Cancelar").clicked() {
                        app.confirm_revert_all = false;
                    }
                    if ui
                        .add(
                            egui::Button::new("Confirmar Reversão Total")
                                .fill(Color32::from_rgb(185, 28, 28)),
                        )
                        .clicked()
                    {
                        app.confirm_revert_all = false;
                        let ids: Vec<String> =
                            app.backup_records.iter().map(|r| r.tweak_id.clone()).collect();
                        let catalog = app.catalog.clone();
                        let batch: Vec<_> = ids
                            .into_iter()
                            .filter_map(|id| {
                                catalog
                                    .iter()
                                    .find(|t| {
                                        t.meta().id == id
                                            && (!t.meta().requires_admin || app.elevated)
                                    })
                                    .cloned()
                            })
                            .collect();
                        app.trigger_batch_tweaks(batch, false, ctx);
                    }
                }
            }
        });
    });

    ui.label(
        RichText::new(format!(
            "Diretório de backups: {}   |   Log: {}",
            app.backups.dir().display(),
            crate::sys::logger::log_file_path().display()
        ))
            .small()
            .monospace()
            .color(Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(8.0);

    ui.columns(2, |cols| {
        cols[0].label(
            RichText::new(format!(
                "Backups Registrados ({})",
                app.backup_records.len()
            ))
            .strong(),
        );
        let recs = app.backup_records.clone();
        let catalog = app.catalog.clone();
        egui::ScrollArea::vertical()
            .id_salt("bk_list")
            .max_height(500.0)
            .show(&mut cols[0], |ui| {
                if recs.is_empty() {
                    ui.label("Nenhum backup gravado no momento.");
                }
                for rec in &recs {
                    egui::Frame::NONE
                        .fill(Color32::from_rgb(23, 30, 41))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                        .corner_radius(6.0)
                        .inner_margin(8.0)
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(&rec.tweak_id).strong().monospace());
                                    ui.label(
                                        RichText::new(format!("Criado em: {}", rec.created_at))
                                            .small()
                                            .color(Color32::from_rgb(148, 163, 184)),
                                    );
                                });
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        if let Some(tw) =
                                            catalog.iter().find(|t| t.meta().id == rec.tweak_id)
                                        {
                                            let busy = app.busy_tweaks.contains(&rec.tweak_id);
                                            let can_rev =
                                                !busy && (!tw.meta().requires_admin || app.elevated);
                                            if ui
                                                .add_enabled(can_rev, egui::Button::new("Reverter"))
                                                .clicked()
                                            {
                                                app.trigger_revert_tweak(tw.clone(), ctx);
                                            }
                                        }
                                    },
                                );
                            });
                        });
                    ui.add_space(4.0);
                }
            });

        cols[1].label(RichText::new("Log da Sessão").strong());
        egui::ScrollArea::vertical()
            .id_salt("session_logs")
            .stick_to_bottom(true)
            .max_height(500.0)
            .show(&mut cols[1], |ui| {
                for l in &app.session_logs {
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
