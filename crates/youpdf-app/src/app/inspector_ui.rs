//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel inspector_ui).


use super::*;

impl YouPdfApp {

        /// Popup des préréglages (couleur, épaisseur, opacité).
        pub(super) fn preset_popup(&mut self, ctx: &egui::Context, canvas_left: f32, top: f32) {
            if !self.preset_open || self.mode != Some(0) {
                return;
            }
            let tool = self.annot_tool.unwrap_or(0);
            let wants_text = matches!(self.draft_kind(), DraftKind::FreeText | DraftKind::Signature)
                && matches!(tool, 2 | 9);
            // Le surligneur et le crayon se dessinent en transparence.
            let wants_opacity = tool == 1 || tool == 4;
            Area::new(Id::new("presets"))
                .anchor(Align2::CENTER_TOP, vec2(canvas_left / 2.0, top + 40.0))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(pal().chrome_bg)
                        .rounding(Rounding::same(RADIUS_L))
                        .stroke(Stroke::new(1.0_f32, pal().separator))
                        .inner_margin(Margin::symmetric(10.0, 8.0))
                        .show(ui, |ui| {
                            // Type de marque (surligneur) ou de forme : le même
                            // outil couvre les quatre variantes que PDF connaît.
                            let kinds: &[(AnnotationKind, &str)] = match tool {
                                1 => &[
                                    (AnnotationKind::Highlight, "Highlight"),
                                    (AnnotationKind::Underline, "Underline"),
                                    (AnnotationKind::StrikeOut, "Strikeout"),
                                    (AnnotationKind::Squiggly, "Squiggly"),
                                ],
                                5 => &[
                                    (AnnotationKind::Square, "Rectangle"),
                                    (AnnotationKind::Circle, "Ellipse"),
                                    (AnnotationKind::Line, "Line"),
                                    (AnnotationKind::Arrow, "Arrow"),
                                ],
                                _ => &[],
                            };
                            if !kinds.is_empty() {
                                ui.horizontal(|ui| {
                                    for (kind, label) in kinds {
                                        let current = self.annot_kind_for(tool) == *kind;
                                        if ui.selectable_label(current, *label).clicked() {
                                            self.set_annot_kind(tool, *kind);
                                        }
                                    }
                                });
                                ui.add_space(SPACE_XS);
                            }
                            ui.horizontal(|ui| {
                                for color in PRESET_COLORS {
                                    let selected = color == self.annot_color;
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::same(RADIUS_S),
                                        Color32::from_rgb(color[0], color[1], color[2]),
                                    );
                                    if selected {
                                        ui.painter().rect_stroke(
                                            rect.expand(2.0),
                                            5.0,
                                            Stroke::new(1.5_f32, Color32::WHITE),
                                        );
                                    }
                                    if resp.clicked() {
                                        self.annot_color = color;
                                    }
                                }
                            });
                            if tool != 3 && tool != 8 {
                                ui.add_space(SPACE_M);
                                ui.horizontal(|ui| {
                                    ui.label(muted(t("Width"), LABEL));
                                    ui.add(
                                        egui::Slider::new(&mut self.annot_width, 0.5..=12.0)
                                            .fixed_decimals(1),
                                    );
                                });
                            }
                            if tool == 10 {
                                ui.add_space(SPACE_S);
                                ui.horizontal(|ui| {
                                    ui.label(muted(t("Eraser"), LABEL));
                                    let stroke = self.eraser_mode == EraseMode::Stroke;
                                    if ui.selectable_label(stroke, t("Stroke")).clicked() {
                                        self.eraser_mode = EraseMode::Stroke;
                                    }
                                    if ui.selectable_label(!stroke, t("Segment")).clicked() {
                                        self.eraser_mode = EraseMode::Segment;
                                    }
                                });
                            }
                            if wants_opacity {
                                ui.horizontal(|ui| {
                                    ui.label(muted(t("Opacity"), LABEL));
                                    ui.add(
                                        egui::Slider::new(&mut self.annot_opacity, 0.1..=1.0)
                                            .fixed_decimals(2),
                                    );
                                });
                            }
                            if tool == 8 {
                                ui.label(
                                    muted(t("Stamp"), LABEL),
                                );
                                let mut picked = None;
                                egui::Grid::new("stamp-palette")
                                    .num_columns(3)
                                    .spacing([4.0, 4.0])
                                    .show(ui, |ui| {
                                        for (idx, template) in STAMP_TEMPLATES.iter().enumerate() {
                                            let selected = self.stamp_choice == idx;
                                            let colour = Color32::from_rgb(
                                                template.color[0],
                                                template.color[1],
                                                template.color[2],
                                            );
                                            let text = RichText::new(template.label)
                                                .size(CAPTION)
                                                .color(if selected { colour } else { pal().icon });
                                            if ui.selectable_label(selected, text).clicked() {
                                                picked = Some(idx);
                                            }
                                            if idx % 3 == 2 {
                                                ui.end_row();
                                            }
                                        }
                                        let custom = RichText::new(t("Custom…"))
                                            .size(CAPTION)
                                            .color(pal().icon);
                                        if ui.selectable_label(false, custom).clicked() {
                                            self.marks.stamp_open = true;
                                        }
                                        ui.end_row();
                                    });
                                if let Some(idx) = picked {
                                    self.stamp_choice = idx;
                                }
                            }
                            if wants_text {
                                ui.label(
                                    RichText::new(t("Color applies to new text"))
                                        .size(CAPTION)
                                        .color(pal().muted),
                                );
                            }
                            if ui.button(t("Done")).clicked() {
                                self.preset_open = false;
                            }
                        });
                });
        }

        /// Inspecteur d'attributs flottant, posé au-dessus de la sélection.
        ///
        /// Le placement vient de [`place_inspector`] : jamais hors de la fenêtre,
        /// flèche pointant vers l'élément. Le cadre suit la taille réelle de son
        /// contenu : elle est mesurée à la trame et sert à la suivante, sinon un
        /// texte long déborderait du fond.
        pub(super) fn inspector(&mut self, ctx: &egui::Context) {
            let Some(target) = self.inspector_target.clone() else {
                return;
            };
            if self.home_open || self.tabs.is_empty() {
                return;
            }
            let annot = match &target {
                InspectorTarget::AnnotationElement { id, .. } => self
                    .tabs
                    .get(self.active)
                    .and_then(|tab| tab.annots.get(id.0))
                    .cloned(),
                _ => None,
            };
            // Taille de la trame précédente ; à défaut, une estimation par type de
            // cible, large plutôt que courte : un panneau qui se croit plus haut
            // qu'il n'est laisse un vide, plus bas il recouvre son élément.
            let fallback = if matches!(target, InspectorTarget::TextElement { .. }) {
                vec2(INSPECTOR_WIDTH, TEXT_INSPECTOR_HEIGHT)
            } else if matches!(target, InspectorTarget::TextSelection { .. }) {
                // La sélection porte deux lignes : l'en-tête et les quatre
                // boutons de mise en évidence.
                vec2(INSPECTOR_WIDTH, SELECTION_INSPECTOR_HEIGHT)
            } else {
                vec2(INSPECTOR_WIDTH, INSPECTOR_HEIGHT)
            };
            let mut size = self.inspector_size.unwrap_or(fallback);
            if let InspectorTarget::TextElement { .. } = &target {
                if self.inspector_text_ui.palette_open {
                    size.y += PALETTE_HEIGHT;
                }
            }
            let placement = place_inspector(target.bounds(), ctx.screen_rect(), size);
            let mut measured: Option<egui::Vec2> = None;
            let mut style_changed = false;
            let mut close = false;
            let mut delete = false;
            let mut delete_id = None;
            let mut image_action = None;
            let mut markup = None;
            Area::new(Id::new("inspector"))
                .fixed_pos(placement.rect.min)
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    let body = Frame::none()
                        .fill(pal().widget_bg)
                        .stroke(Stroke::new(1.0_f32, pal().separator))
                        .rounding(Rounding::same(RADIUS_L))
                        .inner_margin(Margin::symmetric(8.0, 6.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(target.headline())
                                        .size(BODY)
                                        .color(pal().icon_bright),
                                );
                                ui.label(
                                    RichText::new(format!("p. {}", target.page()))
                                        .size(CAPTION)
                                        .color(pal().muted),
                                );
                                if let Some(annot) = &annot {
                                    ui.label(
                                        RichText::new(shorten(&annot.author, 10))
                                            .size(CAPTION)
                                            .color(pal().muted),
                                    );
                                    if ui.small_button("Delete").clicked() {
                                        delete = true;
                                        delete_id = Some(annot.id.clone());
                                    }
                                }
                                if ui.small_button("Close").clicked() {
                                    close = true;
                                }
                            });
                            // Contrôles de texte : ils agissent sur le bloc en
                            // cours de réécriture, qui porte la mise en forme.
                            if matches!(target, InspectorTarget::TextElement { .. }) {
                                if let Some(draft) = self.edit_text.as_mut() {
                                    style_changed = text_style_row(
                                        ui,
                                        &mut draft.style,
                                        &mut self.inspector_text_ui,
                                    );
                                }
                            }
                            // Contrôles de l'image sélectionnée.
                            if matches!(target, InspectorTarget::ImageElement { .. }) {
                                image_action = image_controls(
                                    ui,
                                    &mut self.image_alpha,
                                    self.image_crop.is_some(),
                                );
                            }
                            // Mise en évidence de la sélection de texte : les
                            // quatre marques, posées sur les lignes choisies.
                            if matches!(target, InspectorTarget::TextSelection { .. }) {
                                markup = markup_row(ui);
                            }
                            // Apparence des liens : visibilité, épaisseur, couleur,
                            // pointillé. Le panneau s'ouvre seulement pour un lien.
                            if let Some(annot) = &annot {
                                if annot.kind == AnnotationKind::Link {
                                    let rect = annot.rect;
                                    let page = annot.page;
                                    let mut appearance = self
                                        .tabs
                                        .get(self.active)
                                        .and_then(|tab| youpdf_core::links::get_link_appearance(&tab.doc, page, rect))
                                        .unwrap_or_default();
                                    ui.add_space(SPACE_S);
                                    ui.label(
                                        RichText::new("Link appearance")
                                            .size(CAPTION)
                                            .color(pal().muted),
                                    );
                                    ui.checkbox(&mut appearance.visible, "Visible");
                                    ui.horizontal(|ui| {
                                        ui.label(muted("Width", LABEL));
                                        ui.add(
                                            egui::DragValue::new(&mut appearance.width)
                                                .range(0.0..=10.0)
                                                .speed(0.1),
                                        );
                                    });
                                    let mut color = Color32::from_rgb(appearance.color[0], appearance.color[1], appearance.color[2]);
                                    ui.color_edit_button_srgba(&mut color);
                                    appearance.color = [color.r(), color.g(), color.b()];
                                    if ui.button("Apply").clicked() {
                                        if let Some(tab) = self.tabs.get_mut(self.active) {
                                            if youpdf_core::links::set_link_appearance(
                                                &mut tab.doc,
                                                page,
                                                rect,
                                                &appearance,
                                            )
                                            .is_ok()
                                        {
                                            self.mark_document_changed();
                                        }
                                    }
                                }
                            }
                        }
                        });
                    let rect = body.response.rect;
                    measured = Some(rect.size());
                    paint_arrow(
                        ui.painter(),
                        rect,
                        arrow_x(target.bounds(), rect),
                        placement.arrow_up,
                        pal().widget_bg,
                        pal().separator,
                    );
                });
            self.inspector_size = measured;
            if let Some(action) = image_action {
                self.image_action(action, ctx);
            }
            if let Some(kind) = markup {
                self.markup_selection(kind, ctx);
                close = true;
            }
            if style_changed {
                // L'aperçu sur la page suit le style : un repaint immédiat évite
                // de croire que le réglage n'a rien fait.
                ctx.request_repaint();
            }
            if delete {
                // C'est bien l'annotation visée qui part, pas celle que la trame a
                // sélectionnée entre-temps.
                if let Some(id) = delete_id {
                    let idx = self
                        .tabs
                        .get(self.active)
                        .and_then(|tab| tab.annots.iter().position(|a| a.id == id));
                    if let Some(idx) = idx {
                        self.selected_annot = idx;
                        self.remove_annot(idx);
                    }
                }
                close = true;
            }
            if close {
                self.has_annot_selection = false;
                self.inspector_target = None;
            }
        }

}
