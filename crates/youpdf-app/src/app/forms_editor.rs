//! Outils du champ de formulaire : import/export, panneau de propriétés
//! à onglets, sélection, alignement et création.

use super::*;
use youpdf_shared::{caption, t, BODY, CAPTION, RADIUS_L, SPACE_M, SPACE_S};

impl YouPdfApp {
    /// Import / export de données de formulaire (FDF, XFDF, CSV, JSON).
    /// Renvoie `true` si la méthode a consommé la frame (return dans le panel).
    pub(super) fn form_import_export(
        &mut self,
        ctx: &egui::Context,
        import: bool,
        export: bool,
    ) -> bool {
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
            return true;
        } else if export {
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
            return true;
        }
        false
    }

    /// Affiche l'éditeur de valeur et l'onglet de propriétés d'un champ
    /// sélectionné. Renvoie `true` si le panel doit s'arrêter là.
    pub(super) fn form_field_editor(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
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
                    RichText::new(format!(
                        "{}  ({})",
                        field.name,
                        form_kind_label(&field.kind)
                    ))
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
                ui.label(
                    RichText::new(t("Properties"))
                        .size(BODY)
                        .color(pal().icon_bright),
                );
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
                        ui.add(egui::Slider::new(&mut props.border_width, 0.0..=8.0).text("Width"));
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
                                    if ui.selectable_label(props.font == font, font).clicked() {
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
                                    ["Left", "Centre", "Right"][props.alignment.min(2) as usize],
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
                            ui.add(egui::Slider::new(&mut props.comb, 0..=20).text("Comb cells"));
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
                                    (
                                        "Number 2 decimals",
                                        "AFNumber_Format(2, 0, 0, 0, \"\", true);",
                                    ),
                                    ("Percentage", "AFPercent_Format(0, 2, false);"),
                                    ("Date YYYY-MM-DD", "AFDate_FormatEx(\"yyyy-mm-dd\");"),
                                    ("Date DD/MM/YYYY", "AFDate_FormatEx(\"dd/mm/yyyy\");"),
                                ] {
                                    if ui
                                        .selectable_label(props.format_js == script, label)
                                        .clicked()
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
                        ui.add_sized(vec2(width, 70.0), TextEdit::multiline(&mut props.format_js));
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
                                    TextEdit::singleline(&mut sources).hint_text("A, B, C"),
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
                        ui.add_sized(vec2(width, 70.0), TextEdit::multiline(&mut props.calc_js));
                    }
                }
                ui.add_space(SPACE_S);
                if ui
                    .add_enabled(
                        !props.name.trim().is_empty(),
                        egui::Button::new("Apply properties"),
                    )
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
}
