use eframe::egui::{self, Color32, RichText, Stroke};

use crate::tweaks::{Category, RiskLevel, TweakStatus};
use crate::ui::{badge, LowOsApp, StatusFilter};

pub fn render(app: &mut LowOsApp, ui: &mut egui::Ui, ctx: &egui::Context) {
    ui.horizontal(|ui| {
        ui.heading("Melhorias Individuais");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let check_btn = ui.add_enabled(
                !app.checking_all,
                egui::Button::new(if app.checking_all {
                    "Verificando..."
                } else {
                    "Reverificar tudo"
                }),
            );
            if check_btn.clicked() {
                app.trigger_check_all(ctx);
            }
        });
    });
    ui.label(
        RichText::new(format!(
            "Cada melhoria possui snapshot e backup independente em {} para reversão segura.",
            app.backups.dir().display()
        ))
        .small()
        .color(Color32::from_rgb(148, 163, 184)),
    );
    ui.add_space(8.0);

    // Live progress summary bar
    {
        let mut applied = 0usize;
        let mut partial = 0usize;
        let mut not_applicable = 0usize;
        let mut backups_count = 0usize;
        let mut cat_applied = 0usize;
        let mut cat_total = 0usize;

        for t in &app.catalog {
            let m = t.meta();
            let st = app.statuses.get(m.id);
            if *app.backup_flags.get(m.id).unwrap_or(&false) {
                backups_count += 1;
            }
            let in_cat = app.category_filter == Some(m.category);
            match st {
                Some(TweakStatus::Applied) => {
                    applied += 1;
                    if in_cat {
                        cat_applied += 1;
                        cat_total += 1;
                    }
                }
                Some(TweakStatus::Partial(_)) => {
                    partial += 1;
                    if in_cat {
                        cat_total += 1;
                    }
                }
                Some(TweakStatus::NotApplicable(_)) => {
                    not_applicable += 1;
                }
                _ => {
                    if in_cat {
                        cat_total += 1;
                    }
                }
            }
        }

        let applicable = app.catalog.len().saturating_sub(not_applicable);
        let remaining = applicable.saturating_sub(applied);
        let fraction = if applicable > 0 {
            applied as f32 / applicable as f32
        } else {
            0.0
        };
        let pct = (fraction * 100.0).round() as usize;

        egui::Frame::NONE
            .fill(Color32::from_rgb(20, 27, 38))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
            .corner_radius(8.0)
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new("Progresso geral:").strong());
                    badge(
                        ui,
                        &format!("Aplicadas: {applied}"),
                        Color32::from_rgb(52, 211, 153),
                        Color32::from_rgb(6, 78, 59),
                    );
                    if partial > 0 {
                        badge(
                            ui,
                            &format!("Parciais: {partial}"),
                            Color32::from_rgb(251, 191, 36),
                            Color32::from_rgb(120, 53, 15),
                        );
                    }
                    badge(
                        ui,
                        &format!("Faltam: {remaining}"),
                        Color32::from_rgb(203, 213, 225),
                        Color32::from_rgb(30, 41, 59),
                    );
                    if not_applicable > 0 {
                        badge(
                            ui,
                            &format!("N/A: {not_applicable}"),
                            Color32::from_rgb(148, 163, 184),
                            Color32::from_rgb(24, 30, 40),
                        );
                    }
                    badge(
                        ui,
                        &format!("Backups: {backups_count}"),
                        Color32::from_rgb(103, 232, 249),
                        Color32::from_rgb(22, 78, 99),
                    );
                    if let Some(cat) = app.category_filter {
                        ui.label(
                            RichText::new(format!(
                                "| {}: {cat_applied}/{cat_total}",
                                cat.label()
                            ))
                            .small()
                            .color(Color32::from_rgb(148, 163, 184)),
                        );
                    }
                });
                ui.add_space(5.0);
                let bar_fill = if applied == applicable && applicable > 0 {
                    Color32::from_rgb(16, 185, 129)
                } else {
                    Color32::from_rgb(14, 116, 144)
                };
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_height(18.0)
                        .fill(bar_fill)
                        .text(format!(
                            "{applied} de {applicable} aplicadas ({pct}%) — faltam {remaining}"
                        ))
                        .animate(app.checking_all || !app.busy_tweaks.is_empty()),
                );
            });
    }
    ui.add_space(8.0);

    // Search and status filter row
    ui.horizontal(|ui| {
        ui.label("Buscar:");
        ui.add(
            egui::TextEdit::singleline(&mut app.search_query)
                .desired_width(260.0)
                .hint_text("nome, id, registro ou serviço..."),
        );
        ui.separator();
        ui.selectable_value(&mut app.status_filter, StatusFilter::All, "Todas");
        ui.selectable_value(&mut app.status_filter, StatusFilter::Applied, "Aplicadas");
        ui.selectable_value(&mut app.status_filter, StatusFilter::Pending, "Pendentes");
        ui.selectable_value(&mut app.status_filter, StatusFilter::HasBackup, "Com Backup");
    });
    ui.add_space(6.0);

    // Category chips
    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(app.category_filter.is_none(), "Todas categorias")
            .clicked()
        {
            app.category_filter = None;
        }
        for &cat in Category::all() {
            if ui
                .selectable_label(app.category_filter == Some(cat), cat.label())
                .clicked()
            {
                app.category_filter = Some(cat);
            }
        }
    });
    ui.add_space(6.0);

    // Batch selection & preset bar
    let catalog = app.catalog.clone();
    let query_pre = app.search_query.trim().to_ascii_lowercase();
    let visible_ids: Vec<(String, RiskLevel, bool)> = catalog
        .iter()
        .filter_map(|t| {
            let meta = t.meta();
            if let Some(cat) = app.category_filter {
                if meta.category != cat {
                    return None;
                }
            }
            let status = app
                .statuses
                .get(meta.id)
                .cloned()
                .unwrap_or(TweakStatus::NotApplied);
            let has_backup = *app.backup_flags.get(meta.id).unwrap_or(&false);
            match app.status_filter {
                StatusFilter::All => {}
                StatusFilter::Applied => {
                    if !matches!(status, TweakStatus::Applied) {
                        return None;
                    }
                }
                StatusFilter::Pending => {
                    if matches!(status, TweakStatus::Applied | TweakStatus::NotApplicable(_)) {
                        return None;
                    }
                }
                StatusFilter::HasBackup => {
                    if !has_backup {
                        return None;
                    }
                }
            }
            if !query_pre.is_empty() {
                let matches_text = meta.id.to_ascii_lowercase().contains(&query_pre)
                    || meta.name.to_ascii_lowercase().contains(&query_pre)
                    || meta.description.to_ascii_lowercase().contains(&query_pre)
                    || meta
                        .details
                        .iter()
                        .any(|d| d.to_ascii_lowercase().contains(&query_pre));
                if !matches_text {
                    return None;
                }
            }
            Some((
                meta.id.to_string(),
                meta.risk,
                matches!(status, TweakStatus::NotApplicable(_)),
            ))
        })
        .collect();

    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(!app.checking_all, egui::Button::new("Selecionar todas"))
            .on_hover_text("Marca todas as otimizações visíveis aplicáveis (respeita os filtros ativos).")
            .on_disabled_hover_text("Aguarde a verificação inicial terminar...")
            .clicked()
        {
            for (id, _, is_na) in &visible_ids {
                if !*is_na {
                    app.selected_tweaks.insert(id.clone());
                }
            }
        }
        if ui
            .add_enabled(!app.checking_all, egui::Button::new("Selecionar Risco Baixo"))
            .on_disabled_hover_text("Aguarde a verificação inicial terminar...")
            .clicked()
        {
            for (id, risk, is_na) in &visible_ids {
                if *risk == RiskLevel::Low && !*is_na {
                    app.selected_tweaks.insert(id.clone());
                }
            }
        }
        if ui
            .add_enabled(
                !app.checking_all,
                egui::Button::new("Preset Jogos Seguro").fill(Color32::from_rgb(17, 66, 50)),
            )
            .on_hover_text("Seleciona os ajustes de maior impacto real para jogos sem desligar Wi-Fi, impressora, SMB, UAC ou SmartScreen.")
            .on_disabled_hover_text("Aguarde a verificação inicial terminar...")
            .clicked()
        {
            app.selected_tweaks.clear();
            for &id in crate::tweaks::catalog::SAFE_GAMING_PRESET {
                let is_na = matches!(
                    app.statuses.get(id),
                    Some(TweakStatus::NotApplicable(_))
                );
                if !is_na && catalog.iter().any(|t| t.meta().id == id) {
                    app.selected_tweaks.insert(id.to_string());
                }
            }
            app.banner = Some((
                true,
                format!(
                    "Preset Jogos Seguro selecionado ({} itens aplicáveis).",
                    app.selected_tweaks.len()
                ),
            ));
        }
        if ui
            .add_enabled(!app.selected_tweaks.is_empty(), egui::Button::new("Limpar seleção"))
            .clicked()
        {
            app.selected_tweaks.clear();
        }

        let sel_count = app.selected_tweaks.len();
        if ui
            .add_enabled(
                sel_count > 0 && !app.checking_all,
                egui::Button::new(format!("Aplicar selecionadas ({sel_count})"))
                    .fill(Color32::from_rgb(14, 116, 144)),
            )
            .on_disabled_hover_text(if app.checking_all {
                "Aguarde a verificação inicial terminar..."
            } else {
                "Selecione ao menos uma melhoria aplicável."
            })
            .clicked()
        {
            let batch: Vec<_> = catalog
                .iter()
                .filter(|t| {
                    let m = t.meta();
                    app.selected_tweaks.contains(m.id)
                        && (!m.requires_admin || app.elevated)
                        && !matches!(app.statuses.get(m.id), Some(TweakStatus::NotApplicable(_)))
                })
                .cloned()
                .collect();
            app.trigger_batch_tweaks(batch, true, ctx);
        }

        let sel_with_bk = app
            .selected_tweaks
            .iter()
            .filter(|id| *app.backup_flags.get(*id).unwrap_or(&false))
            .count();
        if ui
            .add_enabled(
                sel_with_bk > 0,
                egui::Button::new(format!("Reverter selecionadas ({sel_with_bk})"))
                    .fill(Color32::from_rgb(68, 28, 41)),
            )
            .clicked()
        {
            let batch: Vec<_> = catalog
                .iter()
                .filter(|t| {
                    let m = t.meta();
                    app.selected_tweaks.contains(m.id)
                        && *app.backup_flags.get(m.id).unwrap_or(&false)
                        && (!m.requires_admin || app.elevated)
                })
                .cloned()
                .collect();
            app.trigger_batch_tweaks(batch, false, ctx);
        }

        ui.separator();
        if ui
            .add_enabled(sel_count > 0, egui::Button::new("Salvar Preset"))
            .on_hover_text(&app.preset_path)
            .clicked()
        {
            let mut ids: Vec<String> = app.selected_tweaks.iter().cloned().collect();
            ids.sort();
            let p = std::path::PathBuf::from(&app.preset_path);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match serde_json::to_string_pretty(&ids)
                .map_err(|e| e.to_string())
                .and_then(|j| std::fs::write(&p, j).map_err(|e| e.to_string()))
            {
                Ok(()) => {
                    app.banner = Some((true, format!("Preset salvo com {} itens em {}", ids.len(), p.display())));
                }
                Err(e) => {
                    app.banner = Some((false, format!("Falha ao salvar preset: {e}")));
                }
            }
        }
        if ui
            .button("Carregar Preset")
            .on_hover_text(&app.preset_path)
            .clicked()
        {
            let p = std::path::PathBuf::from(&app.preset_path);
            match std::fs::read_to_string(&p)
                .map_err(|e| e.to_string())
                .and_then(|s| serde_json::from_str::<Vec<String>>(&s).map_err(|e| e.to_string()))
            {
                Ok(ids) => {
                    app.selected_tweaks = ids.into_iter().collect();
                    app.banner = Some((true, format!("Preset carregado ({} itens).", app.selected_tweaks.len())));
                }
                Err(e) => {
                    app.banner = Some((false, format!("Falha ao carregar preset: {e}")));
                }
            }
        }
    });

    ui.add_space(6.0);
    ui.separator();

    let query = app.search_query.trim().to_ascii_lowercase();

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let mut visible_count = 0usize;

            for tweak in &catalog {
                let meta = tweak.meta();
                if let Some(cat) = app.category_filter {
                    if meta.category != cat {
                        continue;
                    }
                }
                let status = app
                    .statuses
                    .get(meta.id)
                    .cloned()
                    .unwrap_or(TweakStatus::NotApplied);
                let has_backup = *app.backup_flags.get(meta.id).unwrap_or(&false);

                match app.status_filter {
                    StatusFilter::All => {}
                    StatusFilter::Applied => {
                        if !matches!(status, TweakStatus::Applied) {
                            continue;
                        }
                    }
                    StatusFilter::Pending => {
                        if matches!(
                            status,
                            TweakStatus::Applied | TweakStatus::NotApplicable(_)
                        ) {
                            continue;
                        }
                    }
                    StatusFilter::HasBackup => {
                        if !has_backup {
                            continue;
                        }
                    }
                }

                if !query.is_empty() {
                    let matches_text = meta.id.to_ascii_lowercase().contains(&query)
                        || meta.name.to_ascii_lowercase().contains(&query)
                        || meta.description.to_ascii_lowercase().contains(&query)
                        || meta
                            .details
                            .iter()
                            .any(|d| d.to_ascii_lowercase().contains(&query));
                    if !matches_text {
                        continue;
                    }
                }

                visible_count += 1;
                let is_busy = app.busy_tweaks.contains(meta.id);

                egui::Frame::NONE
                    .fill(Color32::from_rgb(23, 30, 41))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 49, 66)))
                    .corner_radius(8.0)
                    .inner_margin(10.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let mut checked = app.selected_tweaks.contains(meta.id);
                            if ui.checkbox(&mut checked, "").changed() {
                                if checked {
                                    app.selected_tweaks.insert(meta.id.to_string());
                                } else {
                                    app.selected_tweaks.remove(meta.id);
                                }
                            }
                            ui.vertical(|ui| {
                                ui.set_max_width(ui.available_width() - 220.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(meta.name).strong().size(14.5));
                                    ui.label(
                                        RichText::new(format!("[{}]", meta.id))
                                            .small()
                                            .color(Color32::from_rgb(100, 116, 139)),
                                    );
                                });
                                ui.add_space(3.0);
                                ui.horizontal_wrapped(|ui| {
                                    let (st_fg, st_bg) = match &status {
                                        TweakStatus::Applied => (
                                            Color32::from_rgb(52, 211, 153),
                                            Color32::from_rgb(6, 78, 59),
                                        ),
                                        TweakStatus::NotApplied => (
                                            Color32::from_rgb(148, 163, 184),
                                            Color32::from_rgb(30, 41, 59),
                                        ),
                                        TweakStatus::Partial(_) => (
                                            Color32::from_rgb(251, 191, 36),
                                            Color32::from_rgb(120, 53, 15),
                                        ),
                                        TweakStatus::NotApplicable(_) => (
                                            Color32::from_rgb(100, 116, 139),
                                            Color32::from_rgb(24, 30, 40),
                                        ),
                                        TweakStatus::Error(_) => (
                                            Color32::from_rgb(251, 113, 133),
                                            Color32::from_rgb(136, 19, 55),
                                        ),
                                    };
                                    badge(ui, &status.label(), st_fg, st_bg);
                                    badge(
                                        ui,
                                        meta.category.label(),
                                        Color32::from_rgb(34, 211, 238),
                                        Color32::from_rgb(22, 78, 99),
                                    );
                                    let (rk_fg, rk_bg) = match meta.risk {
                                        RiskLevel::Low => (
                                            Color32::from_rgb(147, 197, 253),
                                            Color32::from_rgb(30, 58, 138),
                                        ),
                                        RiskLevel::Medium => (
                                            Color32::from_rgb(253, 186, 116),
                                            Color32::from_rgb(124, 45, 18),
                                        ),
                                        RiskLevel::High => (
                                            Color32::from_rgb(253, 164, 175),
                                            Color32::from_rgb(136, 19, 55),
                                        ),
                                    };
                                    badge(
                                        ui,
                                        &format!("Risco: {}", meta.risk.label()),
                                        rk_fg,
                                        rk_bg,
                                    );
                                    if meta.requires_reboot {
                                        badge(
                                            ui,
                                            "Reinício",
                                            Color32::from_rgb(216, 180, 254),
                                            Color32::from_rgb(88, 28, 135),
                                        );
                                    }
                                    if has_backup {
                                        badge(
                                            ui,
                                            "Backup OK",
                                            Color32::from_rgb(52, 211, 153),
                                            Color32::from_rgb(17, 66, 50),
                                        );
                                    }
                                });
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(meta.description)
                                        .color(Color32::from_rgb(203, 213, 225)),
                                );
                            });

                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    let status_known = app.statuses.contains_key(meta.id);
                                    let can_apply = !is_busy
                                        && status_known
                                        && (!meta.requires_admin || app.elevated)
                                        && !matches!(status, TweakStatus::NotApplicable(_));
                                    let can_revert = !is_busy
                                        && (!meta.requires_admin || app.elevated)
                                        && has_backup;
                                    let show_reset_default = !has_backup
                                        && matches!(status, TweakStatus::Applied | TweakStatus::Partial(_))
                                        && tweak.can_reset_default();

                                    if show_reset_default {
                                        let can_reset =
                                            !is_busy && (!meta.requires_admin || app.elevated);
                                        if ui
                                            .add_enabled(
                                                can_reset,
                                                egui::Button::new("Padrão Win")
                                                    .fill(Color32::from_rgb(91, 46, 20)),
                                            )
                                            .on_hover_text("Sem backup local: restaura para os valores padrão do Windows")
                                            .on_disabled_hover_text(if is_busy {
                                                "Operação em andamento..."
                                            } else {
                                                "Execute o 01LowOS como Administrador para restaurar o padrão."
                                            })
                                            .clicked()
                                        {
                                            app.trigger_reset_default_tweak(tweak.clone(), ctx);
                                        }
                                    } else {
                                        let revert_btn = ui
                                            .add_enabled(
                                                can_revert,
                                                egui::Button::new("Reverter")
                                                    .fill(Color32::from_rgb(68, 28, 41)),
                                            )
                                            .on_disabled_hover_text(if is_busy {
                                                "Operação em andamento..."
                                            } else if !has_backup {
                                                "Nenhum backup salvo para este item."
                                            } else {
                                                "Execute o 01LowOS como Administrador para reverter."
                                            });
                                        if revert_btn.clicked() {
                                            app.trigger_revert_tweak(tweak.clone(), ctx);
                                        }
                                    }

                                    let apply_label = if is_busy {
                                        "..."
                                    } else if matches!(status, TweakStatus::Applied) {
                                        "Reaplicar"
                                    } else {
                                        "Aplicar"
                                    };
                                    let apply_btn = ui
                                        .add_enabled(
                                            can_apply,
                                            egui::Button::new(apply_label)
                                                .fill(Color32::from_rgb(14, 116, 144)),
                                        )
                                        .on_disabled_hover_text(if is_busy {
                                            "Operação em andamento..."
                                        } else if !status_known {
                                            "Verificando status inicial..."
                                        } else if matches!(status, TweakStatus::NotApplicable(_)) {
                                            "Não aplicável neste computador/hardware."
                                        } else {
                                            "Execute o 01LowOS como Administrador para aplicar este item."
                                        });
                                    if apply_btn.clicked() {
                                        app.trigger_apply_tweak(tweak.clone(), ctx);
                                    }

                                    let is_open = app.expanded_details.contains(meta.id);
                                    if ui
                                        .button(if is_open { "Ocultar" } else { "Detalhes" })
                                        .clicked()
                                    {
                                        if is_open {
                                            app.expanded_details.remove(meta.id);
                                        } else {
                                            app.expanded_details.insert(meta.id.to_string());
                                        }
                                    }
                                },
                            );
                        });

                        if app.expanded_details.contains(meta.id) && !meta.details.is_empty() {
                            ui.add_space(6.0);
                            ui.separator();
                            for d in &meta.details {
                                ui.label(
                                    RichText::new(format!("• {d}"))
                                        .monospace()
                                        .small()
                                        .color(Color32::from_rgb(148, 163, 184)),
                                );
                            }
                        }
                    });
                ui.add_space(6.0);
            }

            if visible_count == 0 {
                ui.add_space(24.0);
                ui.label("Nenhuma melhoria corresponde aos filtros selecionados.");
            }
        });
}
