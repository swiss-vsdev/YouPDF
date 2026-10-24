//! Panneau Formulaires et remplissage interactif des champs.

use super::*;
use youpdf_shared::{BODY, CAPTION, t, RADIUS_M, SPACE_M, SPACE_S, SPACE_XS};

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

        if self.form_import_export(ctx, import, export) {
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
        self.form_field_editor(ctx, ui);
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
