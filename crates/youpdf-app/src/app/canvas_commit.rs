//! Fin de trame du canevas : les actions différées (`pending_*`) relevées
//! pendant le dessiné des pages sont appliquées ici, une fois la boucle
//! terminée.

use super::*;
use super::canvas_ui::CanvasFrame;

impl YouPdfApp {
    /// Applique à `self` tout ce que la trame du canevas a différé.
    pub(super) fn commit_canvas_frame(&mut self, frame: CanvasFrame) {
        let CanvasFrame { ctx, action, sel, sel_all, sel_copy, sel_markup, form_draft, form_commit, form_cancel, edit_commit, edit_cancel, inspector_target, crop_next, pending_text_move, pending_image_move, pending_image_scale, pending_image_rotate, pending_rect, pending_markup, pending_ink, pending_click, pending_hit, pending_erase_segment, pending_edit, pending_image, pending_redact, pending_form, pending_form_select, pending_measure, pending_link, .. } = frame;
        if let Some((fragment, off, scale)) = pending_text_move {
            self.commit_text_block_move(&fragment, off, scale, &ctx);
            self.edit_text = None;
        }
        if let Some((image, off, scale)) = pending_image_move {
            self.commit_image_move(&image, off, scale, &ctx);
        }
        if let Some((image, matrix)) = pending_image_scale {
            self.commit_image_scale(&image, matrix, &ctx);
        }
        if let Some((image, degrees)) = pending_image_rotate {
            self.commit_image_rotate(&image, degrees, &ctx);
        }
        self.form_edit = form_draft;
        if form_cancel {
            self.form_edit = None;
            self.form_editing = None;
        } else if form_commit {
            self.commit_form_edit();
        }
        self.text_sel = sel;
        self.inspector_target = inspector_target;
        if let Some(crop) = crop_next {
            self.image_crop = Some(crop);
        }
        if edit_cancel {
            self.edit_text = None;
        } else if edit_commit {
            self.replace_fragment(&ctx);
        }

        if sel_all {
            self.select_page_text();
        } else if sel_copy {
            self.copy_selection(&ctx);
        }
        if let Some(kind) = sel_markup {
            self.markup_selection(kind, &ctx);
        }

        if let Some((page, kind, rect)) = pending_rect {
            self.add_rect_annot(page, kind, rect);
        }
        if let Some((page, kind, quads)) = pending_markup {
            self.add_markup_annot(page, kind, quads);
        }
        if let Some((page, points)) = pending_ink {
            self.add_ink_annot(page, points);
        }
        if let Some((page, point)) = pending_edit {
            self.edit_text_at(page, point, &ctx);
        }
        if let Some((page, point)) = pending_image {
            self.insert_image_at(page, point, &ctx);
        }
        if let Some((page, point)) = pending_click {
            self.open_draft(page, point);
        }
        if let Some((page, rect, kind)) = pending_form {
            self.create_form_field(page, rect, kind, &ctx);
        }
        if let Some((page, rect)) = pending_form_select {
            self.select_form_fields_in(page, rect);
        }
        if let Some((page, rect)) = pending_redact {
            if (rect[2] - rect[0]).abs() > 2.0 && (rect[3] - rect[1]).abs() > 2.0 {
                // L'apparence est figée au moment du marquage : changer le
                // remplissage ensuite ne touche que les zones nouvelles.
                let fill = self.redact_fill;
                let overlay = self.overlay_for(fill);
                self.redact_marks.push(RedactionMark {
                    page,
                    rect,
                    fill,
                    overlay,
                });
            }
        }
        if let Some((page, from, to)) = pending_measure {
            // Le glissé est en repère affiché ; la mesure s'écrit en repère page.
            let (from, to) = match self.tabs.get(self.active) {
                Some(tab) => (
                    youpdf_core::geom::point_to_page(&tab.doc, page, from),
                    youpdf_core::geom::point_to_page(&tab.doc, page, to),
                ),
                None => (from, to),
            };
            let low = [from[0].min(to[0]), from[1].min(to[1])];
            let high = [from[0].max(to[0]), from[1].max(to[1])];
            let points = match self.measure_kind {
                // Un glissé donne deux points : la distance va de l'un à
                // l'autre, le périmètre et l'aire prennent le rectangle.
                MeasureKind::Distance => vec![from, to],
                MeasureKind::Perimeter => vec![
                    low,
                    [high[0], low[1]],
                    high,
                    [low[0], high[1]],
                    low,
                ],
                MeasureKind::Area => vec![low, [high[0], low[1]], high, [low[0], high[1]]],
            };
            let drawing_len = ((from[0] - to[0]).powi(2) + (from[1] - to[1]).powi(2)).sqrt();
            self.measure_last_len = Some(drawing_len);
            let measurement = Measurement {
                kind: self.measure_kind,
                points,
                scale: self.measure_scale,
                unit: self.measure_unit.clone(),
            };
            let page = page as u32;
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_measurement(page, &measurement));
            match result {
                Some(Ok(())) => {
                    self.mark_document_changed();
                    self.toast(&ctx, "Measurement added");
                }
                Some(Err(err)) => self.toast(&ctx, format!("Measure failed: {err}")),
                None => {}
            }
        }
        if let Some((page, from, to)) = pending_erase_segment {
            self.erase_ink_segment(page, from, to, &ctx);
        }
        if let Some(action) = pending_link {
            self.follow_link(&action, &ctx);
        }
        if let Some((page, point)) = pending_hit {
            // Un lien prime sur tout le reste : c'est le geste attendu du
            // lecteur, et un clic sur un lien ne doit pas sélectionner autre
            // chose. Le point est ici en repère page, comme les liens.
            if self.follow_link_at(page, point, &ctx) {
                return;
            }
            // Vérifier d'abord si un champ de formulaire a été touché (mode Lecture).
            if matches!(self.mode, None | Some(0) | Some(2)) {
                // La liste des champs n'est chargée qu'à la demande : sans
                // cela, un clic sur un formulaire ne trouve rien tant que le
                // panneau n'a pas été ouvert.
                self.ensure_structure();
                if let Some(form_idx) = self.form_hit_test(page, point) {
                    // En mode Préparer, le clic sélectionne le champ (Maj :
                    // ajoute ou retire de la sélection).
                    if self.mode == Some(2) {
                        self.toggle_form_selection(form_idx);
                    } else {
                    let field = self.tabs[self.active].forms[form_idx].clone();
                    match &field.kind {
                        FormFieldKind::Text => {
                            self.form_editing = Some(form_idx);
                            self.form_edit =
                                Some((form_idx, field.value.clone().unwrap_or_default()));
                        }
                        FormFieldKind::Checkbox => {
                            let checked = field
                                .value
                                .as_deref()
                                .is_some_and(|value| value != "Off")
                                || field
                                    .state
                                    .as_deref()
                                    .is_some_and(|state| state != "Off");
                            let on_state = field
                                .states
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "Yes".to_string());
                            let new_value = if checked { "Off" } else { on_state.as_str() };
                            let filled = if let Some(tab) = self.tabs.get_mut(self.active) {
                                let filled = tab.doc.fill_form_field(&field, new_value).is_ok();
                                tab.forms = tab.doc.read_form_fields();
                                filled
                            } else {
                                false
                            };
                            if filled {
                                self.mark_document_changed();
                            }
                            self.form_editing = None;
                        }
                        FormFieldKind::Radio => {
                            let on_state = field
                                .states
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "Yes".to_string());
                            let filled = if let Some(tab) = self.tabs.get_mut(self.active) {
                                let filled =
                                    tab.doc.fill_form_field(&field, &on_state).is_ok();
                                // Le groupe partage `/V` : relire tous les
                                // boutons remet leurs `/AS` à jour.
                                tab.forms = tab.doc.read_form_fields();
                                filled
                            } else {
                                false
                            };
                            if filled {
                                self.mark_document_changed();
                            }
                            self.form_editing = None;
                        }
                        _ => {
                            self.form_editing = Some(form_idx);
                            self.form_edit =
                                Some((form_idx, field.value.clone().unwrap_or_default()));
                        }
                    }
                    }
                }
            }
            match action {
                ToolAction::Select => {
                    if let Some(idx) = self.hit_test(page, point) {
                        self.selected_annot = idx;
                        self.has_annot_selection = true;
                        self.selected_image = None;
                    } else {
                        self.has_annot_selection = false;
                        // En mode Édition, le clic à vide cherche une image :
                        // c'est ainsi qu'on ouvre l'inspecteur d'image.
                        if self.mode == Some(1) {
                            self.select_image_at(page, point);
                        } else {
                            self.selected_image = None;
                        }
                    }
                }
                ToolAction::Callout => {
                    // Mode callout : 3 clics pour poser la flèche + texte.
                    // L'état vit dans l'application, pas dans la trame : sans
                    // cela, chaque clic repartirait du premier.
                    let pending = match self.callout_pending {
                        // Premier clic : le départ de la flèche.
                        None => Some(CalloutPending {
                            page,
                            start: point,
                            knee: None,
                            text_corner: None,
                        }),
                        // Deuxième clic : le genou de la flèche.
                        Some(p) if p.knee.is_none() => Some(CalloutPending {
                            knee: Some(point),
                            ..p
                        }),
                        // Troisième clic : le coin du texte, l'annotation est complète.
                        Some(p) if p.text_corner.is_none() => {
                            let p = CalloutPending {
                                text_corner: Some(point),
                                ..p
                            };
                            let knee = p.knee.unwrap_or(p.start);
                            let corner = p.text_corner.unwrap_or(p.start);
                            let mut annot = Annotation::new(p.page, AnnotationKind::Callout, [
                                p.start[0].min(knee[0]).min(corner[0]),
                                p.start[1].min(knee[1]).min(corner[1]),
                                (p.start[0].max(knee[0]).max(corner[0])
                                    - p.start[0].min(knee[0]).min(corner[0]))
                                .max(20.0),
                                (p.start[1].max(knee[1]).max(corner[1])
                                    - p.start[1].min(knee[1]).min(corner[1]))
                                .max(20.0),
                            ]);
                            annot.style = self.annot_style(AnnotationKind::Callout);
                            annot.content = format!(
                                "Callout: arrow from ({:.1}, {:.1}) to knee ({:.1}, {:.1}), text at ({:.1}, {:.1})",
                                p.start[0], p.start[1], knee[0], knee[1], corner[0], corner[1]
                            );
                            annot.ink_points = vec![vec![p.start, knee, corner]];
                            annot.recompute_ink_bbox();
                            self.push_annot(annot);
                            None
                        }
                        // Le callout est déjà complet : on repart d'un nouveau.
                        Some(_) => Some(CalloutPending {
                            page,
                            start: point,
                            knee: None,
                            text_corner: None,
                        }),
                    };
                    self.callout_pending = pending;
                }
                ToolAction::Erase => {
                    // Mode trait : un clic efface le trait entier sous le
                    // pointeur. Tous ses tracés partent avec lui — c'est la
                    // différence avec le mode segment, qui n'en coupe qu'un
                    // morceau.
                    if let Some(idx) = self.ink_hit_test(page, point) {
                        self.remove_annot(idx);
                    }
                }
                _ => {}
            }
        }
    }
}
