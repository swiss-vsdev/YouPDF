//! Panneau Formulaires et remplissage interactif des champs.

use super::*;
use youpdf_shared::{BODY, CAPTION, caption, t, RADIUS_L, RADIUS_M, SPACE_M, SPACE_S, SPACE_XS};

/// Un champ est-il posé sur la page `page` du canevas ?
///
/// Le numéro de page d'un champ vient du PDF et celui du canevas compte les
/// pages de la même façon : les deux numérotations se superposent, sans
/// décalage.
pub(super) fn field_on_page(field_page: Option<usize>, page: usize) -> bool {
    field_page == Some(page)
}

/// Index du champ de la page `page` touché par `point` (repère page).
///
/// Le champ trouvé est le dernier de la pile : c'est celui qui passe au-dessus.
pub(super) fn field_at_point(
    fields: &[FormField],
    page: usize,
    point: [f32; 2],
) -> Option<usize> {
    fields
        .iter()
        .enumerate()
        .rev()
        .find(|(_, f)| {
            field_on_page(f.page, page)
                && point[0] >= f.rect[0] - 3.0
                && point[0] <= f.rect[2] + 3.0
                && point[1] >= f.rect[1] - 3.0
                && point[1] <= f.rect[3] + 3.0
        })
        .map(|(idx, _)| idx)
}

/// Index des champs de la page `page` que croise `rect` (repère affiché).
pub(super) fn fields_in_rect(
    fields: &[FormField],
    page: usize,
    rect: [f32; 4],
) -> Vec<usize> {
    fields
        .iter()
        .enumerate()
        .filter(|(_, field)| {
            field_on_page(field.page, page)
                && field.rect[0] < rect[2]
                && field.rect[2] > rect[0]
                && field.rect[1] < rect[3]
                && field.rect[3] > rect[1]
        })
        .map(|(idx, _)| idx)
        .collect()
}

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
            if ui.button(t("Import")).clicked() {
                import = true;
            }
            if ui.button(t("Export")).clicked() {
                export = true;
            }
        });
        ui.add_space(SPACE_S);
        ui.horizontal(|ui| {
            ui.add_space(SPACE_XS);
            ui.checkbox(&mut self.form_highlight, t("Highlight fields"));
        });
        ui.add_space(SPACE_XS);
        // Outils d'alignement du mode Préparer un formulaire.
        if self.mode == Some(2) {
            let selected = self.form_selection.len();
            ui.horizontal(|ui| {
                ui.add_space(SPACE_XS);
                ui.label(
                    RichText::new(format!("{selected} selected"))
                        .size(CAPTION)
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
        ui.add_space(SPACE_XS);

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
                ui.add_space(SPACE_M);
                for (idx, (name, kind, page, read_only, value)) in rows.iter().enumerate() {
                    let (rect, resp) = ui
                        .allocate_exact_size(vec2(self.panel_w - 24.0, 40.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(RADIUS_M), pal().widget_bg);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 12.0),
                        Align2::LEFT_CENTER,
                        shorten(name, 26),
                        FontId::proportional(BODY),
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
                        FontId::proportional(CAPTION),
                        pal().muted,
                    );
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 10.0, rect.min.y + 12.0),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(CAPTION),
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
                    ui.add_space(SPACE_S);
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
                .and_then(|tab| tab.forms.get(idx).map(|field| (idx, tab.doc.read_field_properties(field))));
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
            .rounding(Rounding::same(RADIUS_L))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("{}  ({})", field.name, form_kind_label(&field.kind)))
                        .size(BODY)
                        .color(pal().icon_bright),
                );
                ui.add_space(SPACE_S);
                match field.kind {
                    FormFieldKind::Checkbox | FormFieldKind::Radio => {
                        ui.horizontal(|ui| {
                            if ui.button(t("On")).clicked() {
                                buffer = "On".to_owned();
                                apply = true;
                            }
                            if ui.button(t("Off")).clicked() {
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
                        ui.add_space(SPACE_S);
                        ui.horizontal(|ui| {
                            if ui.button(t("Apply")).clicked() {
                                apply = true;
                            }
                            if ui.button(t("Close")).clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        ui.add_space(SPACE_M);

        // Propriétés du champ, en onglets : Général, Apparence, Options,
        // Actions, Format, Calcul.
        let mut props = self
            .form_props
            .take()
            .filter(|(i, _)| *i == idx)
            .map(|(_, props)| props)
            .unwrap_or_else(|| FieldProperties::from_field(&field));
        let mut apply_props = false;
        let mut tab = self.form_props_tab.min(5);
        let choice = matches!(
            field.kind,
            FormFieldKind::Choice | FormFieldKind::MultiChoice
        );
        let text_like = matches!(
            field.kind,
            FormFieldKind::Text | FormFieldKind::Date | FormFieldKind::Barcode
        );
        Frame::none()
            .fill(pal().widget_bg)
            .rounding(Rounding::same(RADIUS_L))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(RichText::new(t("Properties")).size(BODY).color(pal().icon_bright));
                ui.add_space(SPACE_S);
                ui.horizontal_wrapped(|ui| {
                    for (index, label) in [
                        "General",
                        "Appearance",
                        "Options",
                        "Actions",
                        "Format",
                        "Calculate",
                    ]
                    .iter()
                    .enumerate()
                    {
                        if ui.selectable_label(tab == index, *label).clicked() {
                            tab = index;
                        }
                    }
                });
                ui.separator();
                let width = self.panel_w - 46.0;
                match tab {
                    0 => {
                        ui.add_sized(
                            vec2(width, 24.0),
                            TextEdit::singleline(&mut props.name).hint_text("Name"),
                        );
                        if props.name.trim().is_empty() {
                            ui.label(
                                RichText::new(t("A field needs a name"))
                                    .size(CAPTION)
                                    .color(pal().muted),
                            );
                        }
                        ui.add_sized(
                            vec2(width, 24.0),
                            TextEdit::singleline(&mut props.tooltip).hint_text("Tooltip"),
                        );
                        egui::ComboBox::from_id_salt("form-visibility")
                            .width(width)
                            .selected_text(
                                [
                                    "Visible",
                                    "Hidden",
                                    "Visible, no print",
                                    "Hidden, printable",
                                ][props.visibility.min(3) as usize],
                            )
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (0u8, "Visible"),
                                    (1, "Hidden"),
                                    (2, "Visible, no print"),
                                    (3, "Hidden, printable"),
                                ] {
                                    if ui
                                        .selectable_label(props.visibility == value, label)
                                        .clicked()
                                    {
                                        props.visibility = value;
                                    }
                                }
                            });
                        ui.checkbox(&mut props.required, t("Required"));
                        ui.checkbox(&mut props.read_only, t("Read only"));
                    }
                    1 => {
                        ui.label(caption(t("Border")));
                        ui.add(
                            egui::Slider::new(&mut props.border_width, 0.0..=8.0)
                                .text("Width"),
                        );
                        egui::ComboBox::from_id_salt("form-border-style")
                            .width(width)
                            .selected_text(
                                ["Solid", "Dashed", "Beveled", "Inset", "Underline"]
                                    [props.border_style.min(4) as usize],
                            )
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (0u8, "Solid"),
                                    (1, "Dashed"),
                                    (2, "Beveled"),
                                    (3, "Inset"),
                                    (4, "Underline"),
                                ] {
                                    if ui
                                        .selectable_label(props.border_style == value, label)
                                        .clicked()
                                    {
                                        props.border_style = value;
                                    }
                                }
                            });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(t("Border colour")).size(CAPTION));
                            ui.color_edit_button_srgb(&mut props.border_color);
                        });
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut props.fill, t("Fill"));
                            ui.color_edit_button_srgb(&mut props.fill_color);
                        });
                        ui.separator();
                        ui.label(caption(t("Text")));
                        egui::ComboBox::from_id_salt("form-font")
                            .width(width)
                            .selected_text(props.font.clone())
                            .show_ui(ui, |ui| {
                                for font in ["Helvetica", "Courier", "Times"] {
                                    if ui
                                        .selectable_label(props.font == font, font)
                                        .clicked()
                                    {
                                        props.font = font.to_string();
                                    }
                                }
                            });
                        ui.add(
                            egui::Slider::new(&mut props.font_size, 0.0..=72.0)
                                .text("Size (0 = auto)"),
                        );
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(t("Text colour")).size(CAPTION));
                            ui.color_edit_button_srgb(&mut props.text_color);
                        });
                    }
                    2 => {
                        if text_like {
                            egui::ComboBox::from_id_salt("form-align")
                                .width(width)
                                .selected_text(
                                    ["Left", "Centre", "Right"]
                                        [props.alignment.min(2) as usize],
                                )
                                .show_ui(ui, |ui| {
                                    for (value, label) in
                                        [(0u8, "Left"), (1, "Centre"), (2, "Right")]
                                    {
                                        if ui
                                            .selectable_label(props.alignment == value, label)
                                            .clicked()
                                        {
                                            props.alignment = value;
                                        }
                                    }
                                });
                            ui.add_sized(
                                vec2(width, 24.0),
                                TextEdit::singleline(&mut props.default_value)
                                    .hint_text("Default value"),
                            );
                            ui.checkbox(&mut props.multiline, t("Multiline"));
                            ui.checkbox(&mut props.password, t("Password"));
                            ui.add(
                                egui::Slider::new(&mut props.comb, 0..=20)
                                    .text("Comb cells"),
                            );
                        }
                        if choice {
                            let mut options = props.options.join("\n");
                            if ui
                                .add_sized(
                                    vec2(width, 72.0),
                                    TextEdit::multiline(&mut options)
                                        .hint_text("One option per line"),
                                )
                                .changed()
                            {
                                props.options = options
                                    .lines()
                                    .map(|item| item.trim().to_owned())
                                    .filter(|item| !item.is_empty())
                                    .collect();
                            }
                            if ui.button(t("Sort options")).clicked() {
                                props.options.sort();
                            }
                            ui.checkbox(&mut props.editable, t("Allow custom text"));
                        }
                    }
                    3 => {
                        ui.label(
                            RichText::new(t("Run JavaScript on click"))
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                        ui.add_sized(
                            vec2(width, 90.0),
                            TextEdit::multiline(&mut props.action_js)
                                .hint_text("app.alert(\"Hello\");"),
                        );
                    }
                    4 => {
                        egui::ComboBox::from_id_salt("form-format")
                            .width(width)
                            .selected_text(if props.format_js.is_empty() {
                                "None".to_string()
                            } else {
                                "Custom / preset".to_string()
                            })
                            .show_ui(ui, |ui| {
                                for (label, script) in [
                                    ("None", ""),
                                    ("Number 2 decimals", "AFNumber_Format(2, 0, 0, 0, \"\", true);"),
                                    ("Percentage", "AFPercent_Format(0, 2, false);"),
                                    ("Date YYYY-MM-DD", "AFDate_FormatEx(\"yyyy-mm-dd\");"),
                                    ("Date DD/MM/YYYY", "AFDate_FormatEx(\"dd/mm/yyyy\");"),
                                ] {
                                    if ui.selectable_label(props.format_js == script, label).clicked()
                                    {
                                        props.format_js = script.to_string();
                                    }
                                }
                            });
                        ui.label(
                            RichText::new(t("Format script"))
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                        ui.add_sized(
                            vec2(width, 70.0),
                            TextEdit::multiline(&mut props.format_js),
                        );
                    }
                    _ => {
                        let mut sources = String::new();
                        egui::ComboBox::from_id_salt("form-calc")
                            .width(width)
                            .selected_text("Sum / Product / Average")
                            .show_ui(ui, |ui| {
                                ui.label(
                                    RichText::new(t("Source fields, comma separated"))
                                        .size(CAPTION),
                                );
                                ui.add_sized(
                                    vec2(width - 20.0, 24.0),
                                    TextEdit::singleline(&mut sources)
                                        .hint_text("A, B, C"),
                                );
                                for (label, op) in [
                                    ("Sum", "SUM"),
                                    ("Product", "PRD"),
                                    ("Average", "AVG"),
                                    ("Minimum", "MIN"),
                                    ("Maximum", "MAX"),
                                ] {
                                    if ui.button(label).clicked() {
                                        let names: Vec<String> = sources
                                            .split(',')
                                            .map(|name| name.trim().to_owned())
                                            .filter(|name| !name.is_empty())
                                            .map(|name| format!("\"{name}\""))
                                            .collect();
                                        props.calc_js = format!(
                                            "AFSimple_Calculate(\"{op}\", new Array({}));",
                                            names.join(", ")
                                        );
                                    }
                                }
                            });
                        ui.label(
                            RichText::new(t("Calculate script"))
                                .size(CAPTION)
                                .color(pal().muted),
                        );
                        ui.add_sized(
                            vec2(width, 70.0),
                            TextEdit::multiline(&mut props.calc_js),
                        );
                    }
                }
                ui.add_space(SPACE_S);
                if ui
                    .add_enabled(!props.name.trim().is_empty(), egui::Button::new("Apply properties"))
                    .clicked()
                {
                    apply_props = true;
                }
            });
        self.form_props_tab = tab;
        ui.add_space(SPACE_M);

        if apply {
            let outcome = {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return;
                };
                let Some(field) = tab.forms.get(idx).cloned() else {
                    return;
                };
                match tab.doc.fill_form_field(&field, &buffer) {
                    Ok(()) => {
                        tab.forms = tab.doc.read_form_fields();
                        Ok(field.name)
                    }
                    Err(err) => Err(format!("{err}")),
                }
            };
            match outcome {
                Ok(name) => {
                    // Une édition de champ passe par l'historique comme les
                    // autres modifications structurelles.
                    self.mark_document_changed();
                    self.toast(ctx, format!("Field {name} updated"));
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
        field_at_point(&tab.forms, page, point)
    }

    /// Valide la saisie d'un champ de formulaire faite sur la page.
    pub(super) fn commit_form_edit(&mut self) {
        let Some((idx, buffer)) = self.form_edit.take() else {
            return;
        };
        self.form_editing = None;
        let mut fields_changed = false;
        {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(field) = tab.forms.get(idx).cloned() else {
                return;
            };
            let value = if field.kind == FormFieldKind::Radio {
                if buffer.is_empty() {
                    "Off".to_string()
                } else {
                    field
                        .states
                        .first()
                        .cloned()
                        .unwrap_or_else(|| "Yes".to_string())
                }
            } else {
                buffer
            };
            if tab.doc.fill_form_field(&field, &value).is_ok() {
                // Le groupe radio partage `/V` : relire remet les `/AS` d'aplomb.
                tab.forms = tab.doc.read_form_fields();
                fields_changed = true;
            }
        }
        if fields_changed {
            self.mark_document_changed();
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
        let hits = fields_in_rect(&tab.forms, page, rect);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Un champ de test, posé sur la page `page`.
    fn field(page: usize, rect: [f32; 4]) -> FormField {
        FormField {
            object_id: (1, 0),
            widget_id: (1, 0),
            name: "a".to_string(),
            kind: FormFieldKind::Text,
            value: None,
            options: Vec::new(),
            read_only: false,
            tooltip: String::new(),
            required: false,
            multiline: false,
            page: Some(page),
            rect,
            states: Vec::new(),
            state: None,
        }
    }

    /// Le numéro de page d'un champ et celui du canevas se superposent : pas
    /// de décalage d'une page entre les deux.
    #[test]
    fn field_page_matches_the_canvas_page() {
        assert!(field_on_page(Some(1), 1), "le champ de la page 1 est sur la page 1");
        assert!(field_on_page(Some(2), 2), "le champ de la page 2 est sur la page 2");
        assert!(!field_on_page(Some(1), 2), "la page 2 ne porte pas le champ de la page 1");
        assert!(!field_on_page(Some(2), 1), "la page 1 ne porte pas le champ de la page 2");
        assert!(!field_on_page(None, 1), "un champ sans page n'est sur aucune page");
    }

    /// Un clic trouve le champ de la page cliquée, et rien des autres pages.
    #[test]
    fn hit_test_finds_the_field_of_the_clicked_page() {
        let fields = vec![field(1, [10.0, 10.0, 100.0, 30.0])];
        // Le clic tombe dans le champ de la page 1.
        assert_eq!(field_at_point(&fields, 1, [50.0, 20.0]), Some(0));
        // La page 2 n'a pas de champ : le clic ne trouve rien.
        assert_eq!(field_at_point(&fields, 2, [50.0, 20.0]), None);
    }

    /// Un clic sur la page 2 ne vise pas le champ posé sur la page 1.
    #[test]
    fn hit_test_does_not_reach_a_neighbouring_page() {
        let fields = vec![field(2, [10.0, 10.0, 100.0, 30.0])];
        assert_eq!(field_at_point(&fields, 1, [50.0, 20.0]), None);
        assert_eq!(field_at_point(&fields, 2, [50.0, 20.0]), Some(0));
    }

    /// L'encadrement ne sélectionne que les champs de la page visée.
    #[test]
    fn selection_only_covers_the_selected_page() {
        let fields = vec![
            field(1, [10.0, 10.0, 100.0, 30.0]),
            field(2, [10.0, 10.0, 100.0, 30.0]),
        ];
        // L'encadrement englobe les deux champs, mais sur la page 1 seulement.
        let hits = fields_in_rect(&fields, 1, [0.0, 0.0, 200.0, 200.0]);
        assert_eq!(hits, vec![0], "le champ de la page 2 ne doit pas être sélectionné");
        let hits = fields_in_rect(&fields, 2, [0.0, 0.0, 200.0, 200.0]);
        assert_eq!(hits, vec![1]);
    }
}

// Suite du panel forms_ui (extraite de `app.rs`).


impl YouPdfApp {

        /// Fenêtre « Organiser les pages » : grille des pages avec rotation,
        /// suppression, extraction, découpage et insertion. Les opérations
        /// s'appliquent immédiatement au document en mémoire ; rien n'est écrit
        /// sur le disque tant que l'utilisateur n'enregistre pas.
        /// Description du champ à créer pour un outil donné.
        pub(super) fn new_form_field(
            kind: FormKind,
            page: usize,
            rect: [f32; 4],
            name: String,
        ) -> youpdf_core::forms::NewFormField {
            use youpdf_core::forms::NewFormField;
            match kind {
                FormKind::Text => NewFormField::text(page as u32, rect, name),
                FormKind::Checkbox => NewFormField::checkbox(page as u32, rect, name),
                FormKind::Radio => {
                    let mut field = NewFormField::checkbox(page as u32, rect, name);
                    field.kind = FormFieldKind::Radio;
                    field
                }
                FormKind::Choice => {
                    NewFormField::choice(page as u32, rect, name, &["Option 1", "Option 2"])
                }
                FormKind::MultiChoice => {
                    let mut field =
                        NewFormField::choice(page as u32, rect, name, &["Option 1", "Option 2"]);
                    field.kind = FormFieldKind::MultiChoice;
                    field
                }
                FormKind::Signature => {
                    let mut field = NewFormField::text(page as u32, rect, name);
                    field.kind = FormFieldKind::Signature;
                    field
                }
                FormKind::Button => {
                    let mut field = NewFormField::text(page as u32, rect, name);
                    field.kind = FormFieldKind::Button;
                    field.value = Some("Button".to_string());
                    field
                }
                FormKind::Date => {
                    let mut field = NewFormField::text(page as u32, rect, name);
                    field.kind = FormFieldKind::Date;
                    field
                }
                FormKind::Barcode => {
                    let mut field = NewFormField::text(page as u32, rect, name);
                    field.kind = FormFieldKind::Barcode;
                    field
                }
            }
        }

        /// Crée un champ de formulaire sur la zone tracée, et l'inscrit au
        /// panneau des formulaires.
        pub(super) fn create_form_field(
            &mut self,
            page: usize,
            rect: [f32; 4],
            kind: FormKind,
            ctx: &egui::Context,
        ) {
            if (rect[2] - rect[0]) < 6.0 || (rect[3] - rect[1]) < 6.0 {
                return;
            }
            let created = self.tabs.get_mut(self.active).map(|tab| {
                // La boîte tracée est en repère affiché, le champ s'écrit en repère
                // page.
                let rect = youpdf_core::geom::rect_to_page(&tab.doc, page, rect);
                // Les noms de champ sont des identifiants dans tout le document :
                // deux champs ne peuvent pas partager le même.
                // Les noms sont des identifiants uniques : un `contains` sur un vecteur
                // est O(k) par candidat, O(k²) au total. Un HashSet ramène la
                // recherche à O(1).
                let mut existing: std::collections::HashSet<&str> =
                    std::collections::HashSet::with_capacity(tab.forms.len());
                for form in &tab.forms {
                    existing.insert(form.name.as_str());
                }
                let mut num = tab.forms.len() + 1;
                while existing.contains(format!("Field{num}").as_str()) {
                    num += 1;
                }
                let field = Self::new_form_field(kind, page, rect, format!("Field{num}"));
                tab.doc.create_form_field(&field)
            });
            match created {
                Some(Ok(field)) => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.forms.push(field);
                    }
                    self.mark_document_changed();
                    self.toast(ctx, format!("{} added", kind.label()));
                }
                Some(Err(err)) => self.toast(ctx, format!("Field failed: {err}")),
                None => {}
            }
        }

        /// Nouveau document d'une page vierge.
        pub(super) fn new_blank_page(&mut self, ctx: &egui::Context) {
            match blank_document(1, 595.0, 842.0) {
                Ok(doc) => self.push_document(doc, None),
                Err(err) => self.toast(ctx, format!("Create failed: {err}")),
            }
        }

}
