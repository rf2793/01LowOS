use eframe::egui::{self, Color32, RichText, Stroke};

use crate::iso::{inspect_iso_state, run_full_build, run_phases, run_verify};
use crate::ui::{badge, AppMsg, LowOsApp};

pub fn render(app: &mut LowOsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.heading("Orquestrador de Build de ISO (Projeto_ISO_Definitiva)");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Reverificar ambiente").clicked() {
                app.iso_state = inspect_iso_state();
            }
        });
    });
    ui.label(
        RichText::new("Executa o pipeline oficial (fases 01-08 via Rodar_Como_SYSTEM.ps1 / rebuild_iso_master.ps1) apenas quando solicitado explicitamente.")
            .small()
            .color(Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(8.0);

    let st = app.iso_state.clone();
    egui::Frame::NONE
        .fill(Color32::from_rgb(23, 30, 41))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
        .corner_radius(8.0)
        .inner_margin(10.0)
        .show(ui, |ui| {
            ui.label(RichText::new("Checklist do Ambiente").strong());
            ui.label(format!(
                "Projeto encontrado: {}",
                st.project_dir
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "Não encontrado".into())
            ));
            ui.label(format!(
                "Cópia local NTFS (C:\\01LowOS\\Projeto_ISO_Definitiva): {}",
                if st.local_build_dir_exists { "SIM" } else { "NÃO (SYSTEM não enxerga Google Drive)" }
            ));
            ui.label(format!(
                "Mídia original LTSC 2024: {} ({})",
                if st.original_iso_exists { "OK" } else { "Ausente" },
                st.original_iso_path.display()
            ));
            ui.label(format!(
                "Atualizações .msu: {}",
                if st.msu_files.is_empty() {
                    "Nenhum .msu em C:\\ISO_Updates".into()
                } else {
                    st.msu_files.join(", ")
                }
            ));
            ui.horizontal_wrapped(|ui| {
                status_chip(ui, "C:\\ISO extraído", st.iso_root_extracted);
                status_chip(ui, "C:\\ISO_Mount montado", st.mount_dir_active);
                status_chip(ui, "C:\\ISO.processado", st.processed_marker);
                status_chip(ui, "Saída nova.iso", st.output_iso_exists);
                for (ph, ok) in &st.phase_ok {
                    status_chip(ui, &format!("Fase {ph}.ok"), *ok);
                }
            });
            if st.commit_pending_marker {
                badge(
                    ui,
                    "ALERTA: C:\\ISO.commit_pendente presente (commit interrompido)",
                    Color32::from_rgb(251, 113, 133),
                    Color32::from_rgb(136, 19, 55),
                );
            }
        });

    ui.add_space(8.0);
    if let Some(proj) = st.project_dir.clone() {
        ui.horizontal(|ui| {
            ui.label("Fases (ex.: 05,06):");
            ui.add(egui::TextEdit::singleline(&mut app.iso_phases_input).desired_width(110.0));
            let can_run = !app.iso_busy && app.elevated;
            if ui
                .add_enabled(can_run, egui::Button::new("Executar Fases como SYSTEM"))
                .clicked()
            {
                app.iso_busy = true;
                app.iso_logs.clear();
                let phases = app.iso_phases_input.trim().to_string();
                let p = proj.clone();
                let tx = app.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let (log_tx, log_rx) = std::sync::mpsc::channel();
                    let tx_fwd = tx.clone();
                    let ctx_fwd = ctx.clone();
                    let fwd = std::thread::spawn(move || {
                        while let Ok(l) = log_rx.recv() {
                            let _ = tx_fwd.send(AppMsg::IsoLog(l));
                            ctx_fwd.request_repaint();
                        }
                    });
                    let res = run_phases(&p, &phases, &log_tx);
                    drop(log_tx);
                    let _ = fwd.join();
                    let _ = tx.send(AppMsg::IsoDone(res));
                    ctx.request_repaint();
                });
            }

            if ui
                .add_enabled(can_run, egui::Button::new("Verificar Imagem"))
                .clicked()
            {
                app.iso_busy = true;
                app.iso_logs.clear();
                let p = proj.clone();
                let tx = app.tx.clone();
                let ctx = ctx.clone();
                std::thread::spawn(move || {
                    let (log_tx, log_rx) = std::sync::mpsc::channel();
                    let tx_fwd = tx.clone();
                    let ctx_fwd = ctx.clone();
                    let fwd = std::thread::spawn(move || {
                        while let Ok(l) = log_rx.recv() {
                            let _ = tx_fwd.send(AppMsg::IsoLog(l));
                            ctx_fwd.request_repaint();
                        }
                    });
                    let res = run_verify(&p, &log_tx);
                    drop(log_tx);
                    let _ = fwd.join();
                    let _ = tx.send(AppMsg::IsoDone(res));
                    ctx.request_repaint();
                });
            }

            ui.separator();
            if !app.iso_confirm_full {
                if ui
                    .add_enabled(
                        can_run,
                        egui::Button::new("Gerar ISO Completa (rebuild_iso_master)")
                            .fill(Color32::from_rgb(120, 53, 15)),
                    )
                    .clicked()
                {
                    app.iso_confirm_full = true;
                }
            } else {
                ui.label(RichText::new("Confirmar build completo?").color(Color32::from_rgb(251, 191, 36)));
                if ui.button("Sim, iniciar agora").clicked() {
                    app.iso_confirm_full = false;
                    app.iso_busy = true;
                    app.iso_logs.clear();
                    let p = proj.clone();
                    let tx = app.tx.clone();
                    let ctx = ctx.clone();
                    std::thread::spawn(move || {
                        let (log_tx, log_rx) = std::sync::mpsc::channel();
                        let tx_fwd = tx.clone();
                        let ctx_fwd = ctx.clone();
                        let fwd = std::thread::spawn(move || {
                            while let Ok(l) = log_rx.recv() {
                                let _ = tx_fwd.send(AppMsg::IsoLog(l));
                                ctx_fwd.request_repaint();
                            }
                        });
                        let res = run_full_build(&p, &log_tx);
                        drop(log_tx);
                        let _ = fwd.join();
                        let _ = tx.send(AppMsg::IsoDone(res));
                        ctx.request_repaint();
                    });
                }
                if ui.button("Cancelar").clicked() {
                    app.iso_confirm_full = false;
                }
            }
        });
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Saída do Pipeline ISO").strong());
    egui::ScrollArea::vertical()
        .id_salt("iso_logs")
        .stick_to_bottom(true)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for line in &app.iso_logs {
                ui.label(RichText::new(line).monospace().small());
            }
        });
}

fn status_chip(ui: &mut egui::Ui, label: &str, ok: bool) {
    if ok {
        badge(
            ui,
            &format!("{label}: SIM"),
            Color32::from_rgb(52, 211, 153),
            Color32::from_rgb(6, 78, 59),
        );
    } else {
        badge(
            ui,
            &format!("{label}: NÃO"),
            Color32::from_rgb(148, 163, 184),
            Color32::from_rgb(30, 41, 59),
        );
    }
}
