//! Panneau Formulaires et remplissage interactif des champs.

use super::*;

impl YouPdfApp {
    pub(super) fn forms_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_structure();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.forms.is_empty() {
            panel_hint(ui, "No form fields", "This PDF has no AcroForm fields");
            return;
        }
        let rows: Vec<(String, String, Option<usize>, bool, String)> = tab
            .forms
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    form_kind_label(&field.kind),
                    field.page,
                    field.read_only,
                    field.value.clone().unwrap_or_default(),
                )
            })
            .collect();

        let mut jump = None;
        let mut select = None;

        // Échange de données de formulaire (FDF, XFDF, CSV, JSON).
        let mut import = false;
        let mut export = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("form-data-format")
                .width(80.0)
                .selected_text(self.form_data_format.display_name())
                .show_ui(ui, |ui| {
                    for format in [
                        FormDataFormat::Fdf,
                        FormDataFormat::Xfdf,
                        FormDataFormat::Csv,
                        FormDataFormat::Json,
                    ] {
                        if ui
                            .selectable_label(
                                self.form_data_format == format,
                                format.display_name(),
                            )
                            .clicked()
                        {
                            self.form_data_format = format;
                        }
                    }
                });
            if ui.button("Import").clicked() {
                import = true;
            }
            if ui.button("Export").clicked() {
                export = true;
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            ui.checkbox(&mut self.form_highlight, "Highlight fields");
        });
        ui.add_space(2.0);

        if import {
            let format = self.form_data_format;
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(format.display_name(), &[format.extension()])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(data) => {
                        let result = self
                            .tabs
                            .get_mut(self.active)
                            .map(|tab| tab.doc.import_form_data(format, &data));
                        match result {
                            Some(Ok(count)) => {
                                self.mark_document_changed();
                                self.ensure_structure();
                                self.toast(ctx, format!("{count} field value(s) imported"));
                            }
                            Some(Err(err)) => self.toast(ctx, format!("Import failed: {err}")),
                            None => {}
                        }
                    }
                    Err(err) => self.toast(ctx, format!("Import failed: {err}")),
                }
            }
            return;
        }
        if export {
            let format = self.form_data_format;
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_form_data(format));
            match result {
                Some(Ok(data)) => {
                    let stem = self.active_stem().unwrap_or_else(|| "form".to_owned());
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(format.display_name(), &[format.extension()])
                        .set_file_name(format!("{stem}.{}", format.extension()))
                        .save_file()
                    {
                        match std::fs::write(&path, data) {
                            Ok(()) => self.toast(ctx, "Form data exported"),
                            Err(err) => self.toast(ctx, format!("Export failed: {err}")),
                        }
                    }
                }
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (idx, (name, kind, page, read_only, value)) in rows.iter().enumerate() {
                    let (rect, resp) = ui
                        .allocate_exact_size(vec2(self.panel_w - 24.0, 40.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 12.0),
                        Align2::LEFT_CENTER,
                        shorten(name, 26),
                        FontId::proportional(12.0),
                        pal().icon_bright,
                    );
                    let detail = if value.is_empty() {
                        format!("{kind} · empty")
                    } else {
                        format!("{kind} · {}", shorten(value, 20))
                    };
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 28.0),
                        Align2::LEFT_CENTER,
                        if *read_only { format!("{detail} · read-only") } else { detail },
                        FontId::proportional(10.5),
                        pal().muted,
                    );
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 10.0, rect.min.y + 12.0),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(10.0),
                            pal().muted,
                        );
                    }
                    if resp.clicked() {
                        if *read_only {
                            jump = *page;
                        } else {
                            select = Some(idx);
                        }
                    }
                    ui.add_space(4.0);
                }
            });

        if let Some(idx) = select {
            let current = rows
                .get(idx)
                .map(|row| row.4.clone())
                .unwrap_or_default();
            self.form_edit = Some((idx, current));
            self.form_props = self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.forms.get(idx))
                .map(|field| {
                    (
                        idx,
                        FieldProperties {
                            name: field.name.clone(),
                            tooltip: field.tooltip.clone(),
                            required: field.required,
                            read_only: field.read_only,
                            multiline: field.multiline,
                            options: field.options.clone(),
                        },
                    )
                });
            if let Some(page) = rows.get(idx).and_then(|row| row.2) {
                self.jump = Some(page);
            }
        } else if let Some(page) = jump {
            self.jump = Some(page);
        }

        // Éditeur du champ sélectionné.
        let Some((idx, mut buffer)) = self.form_edit.take() else {
            return;
        };
        let field = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.forms.get(idx))
            .cloned();
        let Some(field) = field else {
            return;
        };
        let mut apply = false;
        let mut close = false;
        Frame::none()
            .fill(pal().widget_bg)
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("{}  ({})", field.name, form_kind_label(&field.kind)))
                        .size(11.5)
                        .color(pal().icon_bright),
                );
                ui.add_space(4.0);
                match field.kind {
                    FormFieldKind::Checkbox | FormFieldKind::Radio => {
                        ui.horizontal(|ui| {
                            if ui.button("On").clicked() {
                                buffer = "On".to_owned();
                                apply = true;
                            }
                            if ui.button("Off").clicked() {
                                buffer = "Off".to_owned();
                                apply = true;
                            }
                        });
                    }
                    _ => {
                        ui.add_sized(
                            vec2(self.panel_w - 46.0, 24.0),
                            TextEdit::singleline(&mut buffer).hint_text("Value"),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply = true;
                            }
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        ui.add_space(6.0);

        // Propriétés du champ : nom, aide, drapeaux, options.
        let mut props = self
            .form_props
            .take()
            .filter(|(i, _)| *i == idx)
            .map(|(_, props)| props)
            .unwrap_or_else(|| FieldProperties {
                name: field.name.clone(),
                tooltip: field.tooltip.clone(),
                required: field.required,
                read_only: field.read_only,
                multiline: field.multiline,
                options: field.options.clone(),
            });
        let mut apply_props = false;
        let choice = matches!(
            field.kind,
            FormFieldKind::Choice | FormFieldKind::MultiChoice
        );
        Frame::none()
            .fill(pal().widget_bg)
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Properties").size(11.5).color(pal().icon_bright));
                ui.add_space(4.0);
                ui.add_sized(
                    vec2(self.panel_w - 46.0, 24.0),
                    TextEdit::singleline(&mut props.name).hint_text("Name"),
                );
                ui.add_sized(
                    vec2(self.panel_w - 46.0, 24.0),
                    TextEdit::singleline(&mut props.tooltip).hint_text("Tooltip"),
                );
                ui.checkbox(&mut props.required, "Required");
                ui.checkbox(&mut props.read_only, "Read only");
                if matches!(field.kind, FormFieldKind::Text) {
                    ui.checkbox(&mut props.multiline, "Multiline");
                }
                if choice {
                    let mut options = props.options.join(", ");
                    if ui
                        .add_sized(
                            vec2(self.panel_w - 46.0, 24.0),
                            TextEdit::singleline(&mut options)
                                .hint_text("Options, comma separated"),
                        )
                        .changed()
                    {
                        props.options = options
                            .split(',')
                            .map(|item| item.trim().to_owned())
                            .filter(|item| !item.is_empty())
                            .collect();
                    }
                }
                ui.add_space(4.0);
                if ui.button("Apply properties").clicked() {
                    apply_props = true;
                }
            });
        ui.add_space(6.0);

        if apply_props {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if let Some(field) = tab.forms.get(idx).cloned() {
                    match tab.doc.update_form_field(&field, &props) {
                        Ok(()) => {
                            if let Some(entry) = tab.forms.get_mut(idx) {
                                entry.name = props.name.clone();
                                entry.tooltip = props.tooltip.clone();
                                entry.required = props.required;
                                entry.read_only = props.read_only;
                                entry.multiline = props.multiline;
                                entry.options = props.options.clone();
                            }
                            tab.dirty = true;
                            tab.structural_dirty = true;
                            tab.invalidate_render();
                            self.toast(ctx, format!("Field {} updated", props.name));
                        }
                        Err(err) => self.toast(ctx, format!("Field update failed: {err}")),
                    }
                }
            }
            self.form_props = None;
        } else {
            self.form_props = Some((idx, props));
        }

        if apply {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(field) = tab.forms.get(idx).cloned() else {
                return;
            };
            let result = tab.doc.fill_form_field(&field, &buffer);
            match result {
                Ok(()) => {
                    if let Some(entry) = tab.forms.get_mut(idx) {
                        entry.value = Some(buffer.clone());
                    }
                    tab.dirty = true;
                    tab.structural_dirty = true;
                    // La page est déjà rendue avec l'ancienne valeur.
                    tab.invalidate_render();
                    self.toast(ctx, format!("Field {} updated", field.name));
                }
                Err(err) => self.toast(ctx, format!("Field update failed: {err}")),
            }
        } else if !close {
            self.form_edit = Some((idx, buffer));
        }
    }

    /// Hit-test sur un champ de formulaire sur la page `page` aux coordonnées `point`.
    /// Renvoie l'index du champ dans `tab.forms`, ou `None`.
    pub(super) fn form_hit_test(&self, page: usize, point: [f32; 2]) -> Option<usize> {
        let tab = self.tabs.get(self.active)?;
        tab.forms
            .iter()
            .enumerate()
            .rev()
            .find(|(_, f)| f.page == Some(page + 1) && {
                point[0] >= f.rect[0] - 3.0 && point[0] <= f.rect[2] + 3.0 && point[1] >= f.rect[1] - 3.0 && point[1] <= f.rect[3] + 3.0
            })
            .map(|(idx, _)| idx)
    }

    /// Valide la saisie d'un champ de formulaire faite sur la page.
    pub(super) fn commit_form_edit(&mut self) {
        let Some((idx, buffer)) = self.form_edit.take() else {
            return;
        };
        self.form_editing = None;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(field) = tab.forms.get(idx).cloned() else {
            return;
        };
        let value = if field.kind == FormFieldKind::Radio {
            if buffer.is_empty() {
                "Off"
            } else {
                "Yes"
            }
        } else {
            buffer.as_str()
        };
        if tab.doc.fill_form_field(&field, value).is_ok() {
            // Un bouton radio sélectionné désélectionne ses frères.
            if field.kind == FormFieldKind::Radio && value != "Off" {
                let siblings: Vec<FormField> = tab
                    .forms
                    .iter()
                    .filter(|other| {
                        other.kind == FormFieldKind::Radio
                            && other.name == field.name
                            && other.object_id != field.object_id
                    })
                    .cloned()
                    .collect();
                for other in siblings {
                    let _ = tab.doc.fill_form_field(&other, "Off");
                }
            }
            tab.forms = tab.doc.read_form_fields();
        }
    }
}
