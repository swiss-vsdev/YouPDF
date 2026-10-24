//! Sélection, alignement et création des champs de formulaire.

use super::*;
use super::forms_ui::{fields_in_rect, FormAlign};

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
