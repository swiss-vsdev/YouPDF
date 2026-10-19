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
        let mut reset_form = false;
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
        // Outils d'alignement du mode Préparer un formulaire.
        if self.mode == Some(2) {
            let selected = self.form_selection.len();
            ui.horizontal(|ui| {
                ui.add_space(2.0);
                ui.label(
                    RichText::new(format!("{selected} selected"))
                        .size(10.5)
                        .color(pal().muted),
                );
                if ui
                    .add_enabled(selected >= 2, egui::Button::new("Reset form"))
                    .clicked()
                {
                    reset_form = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                for (label, action) in [
                    ("L", FormAlign::Left),
                    ("R", FormAlign::Right),
                    ("T", FormAlign::Top),
                    ("B", FormAlign::Bottom),
                    ("H", FormAlign::DistributeH),
                    ("V", FormAlign::DistributeV),
                    ("W", FormAlign::MatchWidth),
                    ("H=", FormAlign::MatchHeight),
                ] {
                    if ui
                        .add_enabled(selected >= 2, egui::Button::new(label))
                        .clicked()
                    {
                        self.align_form_fields(action);
                    }
                }
            });
        }
        ui.add_space(2.0);

        if reset_form {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if tab.doc.reset_form_fields().is_ok() {
                    tab.forms = tab.doc.read_form_fields();
                    tab.dirty = true;
                    tab.structural_dirty = true;
                    tab.invalidate_render();
                }
            }
            self.toast(ctx, "Form reset".to_string());
        }

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

impl YouPdfApp {
    /// Sélectionne les champs de `page` qui croisent `rect` (repère affiché).
    pub(super) fn select_form_fields_in(&mut self, page: usize, rect: [f32; 4]) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if (rect[2] - rect[0]) < 2.0 && (rect[3] - rect[1]) < 2.0 {
            self.form_selection.clear();
            return;
        }
        let hits: Vec<usize> = tab
            .forms
            .iter()
            .enumerate()
            .filter(|(_, field)| {
                field.page == Some(page + 1)
                    && field.rect[0] < rect[2]
                    && field.rect[2] > rect[0]
                    && field.rect[1] < rect[3]
                    && field.rect[3] > rect[1]
            })
            .map(|(idx, _)| idx)
            .collect();
        self.form_selection = hits.into_iter().collect();
        self.form_editing = None;
        self.form_edit = None;
    }

    /// Bascule un champ dans la sélection (clic avec Maj).
    pub(super) fn toggle_form_selection(&mut self, idx: usize) {
        if !self.form_selection.insert(idx) {
            self.form_selection.remove(&idx);
        }
        self.form_editing = None;
        self.form_edit = None;
    }

    /// Aligne, répartit ou égalise les champs sélectionnés.
    pub(super) fn align_form_fields(&mut self, action: FormAlign) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let mut fields: Vec<(usize, FormField)> = self
            .form_selection
            .iter()
            .filter_map(|idx| tab.forms.get(*idx).cloned().map(|field| (*idx, field)))
            .collect();
        if fields.len() < 2 {
            return;
        }
        let bounds = fields.iter().fold(
            [f32::MAX, f32::MAX, f32::MIN, f32::MIN],
            |mut acc, (_, field)| {
                acc[0] = acc[0].min(field.rect[0].min(field.rect[2]));
                acc[1] = acc[1].min(field.rect[1].min(field.rect[3]));
                acc[2] = acc[2].max(field.rect[0].max(field.rect[2]));
                acc[3] = acc[3].max(field.rect[1].max(field.rect[3]));
                acc
            },
        );
        match action {
            FormAlign::Left => {
                for (_, field) in fields.iter_mut() {
                    let w = field.rect[2] - field.rect[0];
                    field.rect[0] = bounds[0];
                    field.rect[2] = bounds[0] + w;
                }
            }
            FormAlign::Right => {
                for (_, field) in fields.iter_mut() {
                    let w = field.rect[2] - field.rect[0];
                    field.rect[2] = bounds[2];
                    field.rect[0] = bounds[2] - w;
                }
            }
            FormAlign::Top => {
                for (_, field) in fields.iter_mut() {
                    let h = field.rect[3] - field.rect[1];
                    field.rect[3] = bounds[3];
                    field.rect[1] = bounds[3] - h;
                }
            }
            FormAlign::Bottom => {
                for (_, field) in fields.iter_mut() {
                    let h = field.rect[3] - field.rect[1];
                    field.rect[1] = bounds[1];
                    field.rect[3] = bounds[1] + h;
                }
            }
            FormAlign::DistributeH | FormAlign::DistributeV => {
                let (low, high, vertical) = if action == FormAlign::DistributeH {
                    (0usize, 2usize, true)
                } else {
                    (1, 3, false)
                };
                fields.sort_by(|a, b| {
                    a.1.rect[low]
                        .partial_cmp(&b.1.rect[low])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let first = fields.first().unwrap().1.rect[low];
                let last = fields.last().unwrap().1.rect[high];
                let count = fields.len() as f32;
                let span = (last - first).max(0.0);
                for (index, (_, field)) in fields.iter_mut().enumerate() {
                    let size = field.rect[high] - field.rect[low];
                    let slot = first + span * index as f32 / (count - 1.0);
                    if vertical {
                        field.rect[0] = slot;
                        field.rect[2] = slot + size;
                    } else {
                        field.rect[1] = slot;
                        field.rect[3] = slot + size;
                    }
                }
            }
            FormAlign::MatchWidth | FormAlign::MatchHeight => {
                let mut target = fields[0].1.rect;
                for (_, field) in fields.iter() {
                    if action == FormAlign::MatchWidth {
                        target[2] = target[2].max(field.rect[2]);
                        target[0] = target[0].min(field.rect[0]);
                    } else {
                        target[3] = target[3].max(field.rect[3]);
                        target[1] = target[1].min(field.rect[1]);
                    }
                }
                for (_, field) in fields.iter_mut() {
                    if action == FormAlign::MatchWidth {
                        let w = target[2] - target[0];
                        field.rect[2] = field.rect[0] + w;
                    } else {
                        let h = target[3] - target[1];
                        field.rect[3] = field.rect[1] + h;
                    }
                }
            }
        }
        for (idx, field) in fields {
            if tab.doc.set_form_field_rect(&field, field.rect).is_ok() {
                if let Some(entry) = tab.forms.get_mut(idx) {
                    entry.rect = field.rect;
                }
            }
        }
        tab.dirty = true;
        tab.structural_dirty = true;
        tab.invalidate_render();
    }
}

/// Opérations d'alignement de champs du mode Préparer un formulaire.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FormAlign {
    Left,
    Right,
    Top,
    Bottom,
    DistributeH,
    DistributeV,
    MatchWidth,
    MatchHeight,
}
