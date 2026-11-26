use eframe::egui::{self, Color32, RichText, Stroke};

use crate::apps::AppEntry;
use crate::ui::{badge, LowOsApp};

pub fn render(app: &mut LowOsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.heading("Gerenciador de Aplicativos (Aplicativos_01LowOS.json)");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui
                .add_enabled(!app.apps_busy, egui::Button::new("Atualizar status"))
                .clicked()
            {
                app.trigger_check_apps(ctx);
            }
            let missing: Vec<AppEntry> = app
                .apps
                .iter()
                .filter(|a| {
                    !*app.apps_installed.get(&a.nome).unwrap_or(&false)
                        && (!a.escopo.eq_ignore_ascii_case("Maquina") || app.elevated)
                })
                .cloned()
                .collect();
            let can_batch = !app.apps_busy && !missing.is_empty();
            if ui
                .add_enabled(
                    can_batch,
                    egui::Button::new(format!("Instalar ausentes ({})", missing.len()))
                        .fill(Color32::from_rgb(14, 116, 144)),
                )
                .on_disabled_hover_text(if app.apps_busy {
                    "Instalação em andamento..."
                } else if !app.elevated {
                    "Os aplicativos ausentes de escopo Máquina exigem execução como Administrador."
                } else {
                    "Todos os aplicativos já estão instalados."
                })
                .clicked()
            {
                app.trigger_install_apps(missing, ctx);
            }
        });
    });

    ui.horizontal(|ui| {
        ui.label("Pasta de cache offline opcional:");
        ui.add(egui::TextEdit::singleline(&mut app.apps_cache_dir).desired_width(240.0));
        ui.label(
            RichText::new("Download oficial validado por assinatura Authenticode ou SHA-256.")
                .small()
                .color(Color32::from_rgb(148, 163, 184)),
        );
    });
    ui.add_space(6.0);
    ui.separator();

    let apps_clone = app.apps.clone();
    egui::ScrollArea::vertical()
        .id_salt("apps_list")
        .max_height(430.0)
        .show(ui, |ui| {
            for entry in &apps_clone {
                let installed = *app.apps_installed.get(&entry.nome).unwrap_or(&false);
                let is_machine = entry.escopo.eq_ignore_ascii_case("Maquina");

                egui::Frame::NONE
                    .fill(Color32::from_rgb(23, 30, 41))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                    .corner_radius(6.0)
                    .inner_margin(8.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&entry.nome).strong());
                                    if installed {
                                        badge(
                                            ui,
                                            "Instalado",
                                            Color32::from_rgb(52, 211, 153),
                                            Color32::from_rgb(6, 78, 59),
                                        );
                                    } else {
                                        badge(
                                            ui,
                                            "Ausente",
                                            Color32::from_rgb(148, 163, 184),
                                            Color32::from_rgb(30, 41, 59),
                                        );
                                    }
                                    badge(
                                        ui,
                                        &entry.escopo,
                                        Color32::from_rgb(34, 211, 238),
                                        Color32::from_rgb(22, 78, 99),
                                    );
                                    if let Some(ref signer) = entry.assinante {
                                        ui.label(
                                            RichText::new(format!("Assinante: {signer}"))
                                                .small()
                                                .color(Color32::from_rgb(148, 163, 184)),
                                        );
                                    }
                                });
                                ui.label(
                                    RichText::new(format!(
                                        "Detectar: {}  |  Args: {}",
                                        entry.detectar, entry.argumentos
                                    ))
                                    .small()
                                    .monospace()
                                    .color(Color32::from_rgb(148, 163, 184)),
                                );
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let needs_admin = is_machine && !app.elevated;
                                    let btn = ui.add_enabled(
                                        !installed && !app.apps_busy && !needs_admin,
                                        egui::Button::new(if installed {
                                            "Instalado"
                                        } else if needs_admin {
                                            "Requer Admin"
                                        } else {
                                            "Instalar"
                                        }),
                                    );
                                    if btn.clicked() {
                                        app.trigger_install_apps(vec![entry.clone()], ctx);
                                    }
                                },
                            );
                        });
                    });
                ui.add_space(4.0);
            }
        });

    if !app.apps_logs.is_empty() {
        ui.add_space(6.0);
        ui.label(RichText::new("Log de Instalação").strong());
        egui::ScrollArea::vertical()
            .id_salt("apps_logs")
            .stick_to_bottom(true)
            .max_height(130.0)
            .show(ui, |ui| {
                for l in &app.apps_logs {
                    ui.label(RichText::new(l).monospace().small());
                }
            });
    }
}
