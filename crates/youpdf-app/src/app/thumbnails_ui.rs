//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel thumbnails_ui).


use super::*;

impl YouPdfApp {

        pub(super) fn thumbnails(&mut self, ui: &mut egui::Ui) {
            if self.tabs.is_empty() {
                // Plus aucun document : la sélection ne doit pas survivre à
                // l'ouverture suivante.
                self.thumb_selected.clear();
                self.thumb_owner = usize::MAX;
                return;
            }
            let active = self.active;
            let ctx = ui.ctx().clone();
            let ppp = ctx.pixels_per_point();
            let renderer = &*self.renderer;
            let status = &mut self.render_status;
            let total = self.tabs[active].total();
            let mut zoom = self.thumb_pt;
            let mut selected = std::mem::take(&mut self.thumb_selected);
            // Changer d'onglet change de document : les mêmes numéros de page
            // viseraient d'autres pages.
            if self.thumb_owner != active {
                self.thumb_owner = active;
                selected.clear();
            }
            // Un document plus court (suppression, nouvelle ouverture) laisse des
            // pages fantômes dans la sélection.
            selected.retain(|page| *page >= 1 && *page <= total);
            let mut drag_from = self.thumb_drag;
            let mut action: Option<PageAction> = None;
            let mut jump = None;

            // Curseur en bas du panneau : largeur des vignettes, de 80 à 240 pt.
            egui::TopBottomPanel::bottom("thumb_zoom")
                .frame(Frame::none().fill(pal().canvas_bg))
                .show_inside(ui, |ui| {
                    ui.add_space(SPACE_M);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.add_sized(
                            vec2((self.panel_w - 40.0).max(80.0), 18.0),
                            egui::Slider::new(&mut zoom, 80.0..=240.0).show_value(false),
                        );
                    });
                    ui.add_space(SPACE_M);
                });

            let tab = &mut self.tabs[active];
            let pointer = ctx.pointer_interact_pos();
            let mut drop_target = None;
            let mut drag_stopped = false;

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let tile_width = ui.available_width().max(80.0);
                    for page in 1..=total as u32 {
                        let (wp, hp) = tab.page_size_cached(page);
                        let scale = zoom / wp.max(1.0);
                        let size = vec2(wp * scale, hp * scale);
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2(tile_width, size.y + 28.0),
                            Sense::click_and_drag(),
                        );
                        let picked = selected.contains(&(page as usize));
                        let img_rect = Rect::from_center_size(
                            pos2(rect.center().x, rect.min.y + 8.0 + size.y / 2.0),
                            size,
                        );

                        if picked {
                            ui.painter().rect_filled(
                                img_rect.expand(5.0),
                                Rounding::same(RADIUS_S),
                                pal().thumb_tint,
                            );
                        } else if resp.hovered() {
                            ui.painter().rect_filled(
                                img_rect.expand(5.0),
                                Rounding::same(RADIUS_S),
                                Color32::from_rgb(0x3C, 0x3C, 0x3C),
                            );
                        }

                        if let Some(tex) = Self::page_texture(
                            &ctx,
                            renderer,
                            tab,
                            page,
                            THUMBNAIL_STEP,
                            scale,
                            ppp,
                            true,
                            false,
                            status,
                        ) {
                            ui.painter().image(
                                tex.id(),
                                img_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }
                        if picked {
                            ui.painter().rect_stroke(
                                img_rect.expand(5.0),
                                4.0,
                                Stroke::new(1.0_f32, pal().accent),
                            );
                        }
                        // Repère de la page affichée.
                        if page as usize == tab.page {
                            ui.painter().circle_filled(
                                pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                                3.0,
                                pal().accent,
                            );
                        }
                        ui.painter().text(
                            pos2(rect.center().x, img_rect.max.y + 14.0),
                            Align2::CENTER_CENTER,
                            page.to_string(),
                            FontId::proportional(LABEL),
                            if picked { pal().accent } else { pal().muted },
                        );

                        // Cible du glissé : la vignette sous le curseur.
                        if drag_from.is_some() && pointer.map(|p| rect.contains(p)).unwrap_or(false) {
                            drop_target = Some(page as usize);
                            ui.painter().rect_stroke(
                                img_rect.expand(5.0),
                                4.0,
                                Stroke::new(2.0_f32, pal().accent),
                            );
                        }
                        if resp.drag_started() {
                            drag_from = Some(page as usize);
                        }
                        if resp.drag_stopped() {
                            drag_stopped = true;
                        }
                        if resp.clicked() {
                            let mods = ui.input(|i| i.modifiers);
                            if mods.shift {
                                let anchor = selected
                                    .iter()
                                    .next_back()
                                    .copied()
                                    .unwrap_or(tab.page.max(1));
                                let (lo, hi) = (anchor.min(page as usize), anchor.max(page as usize));
                                for number in lo..=hi {
                                    selected.insert(number);
                                }
                            } else if mods.command || mods.ctrl {
                                if !selected.remove(&(page as usize)) {
                                    selected.insert(page as usize);
                                }
                            } else {
                                selected.clear();
                                selected.insert(page as usize);
                                jump = Some(page as usize);
                            }
                        }
                        resp.context_menu(|ui| {
                            // Le menu agit sur toute la sélection ; un clic droit
                            // sur une page hors sélection la sélectionne d'abord.
                            if !selected.contains(&(page as usize)) {
                                selected.clear();
                                selected.insert(page as usize);
                            }
                            if ui.button(t("Rotate Clockwise")).clicked() {
                                action = Some(PageAction::Rotate(90));
                                ui.close_menu();
                            }
                            if ui.button(t("Rotate Counter-Clockwise")).clicked() {
                                action = Some(PageAction::Rotate(-90));
                                ui.close_menu();
                            }
                            ui.separator();
                            ui.menu_button("Insert Blank Page After", |ui| {
                                for (label, width, height) in BLANK_SIZES {
                                    if ui.button(label).clicked() {
                                        action = Some(PageAction::Blank(width, height));
                                        ui.close_menu();
                                    }
                                }
                            });
                            if ui.button(t("Insert from File…")).clicked() {
                                action = Some(PageAction::InsertFile);
                                ui.close_menu();
                            }
                            if ui.button(t("Extract Selected Pages…")).clicked() {
                                action = Some(PageAction::Extract);
                                ui.close_menu();
                            }
                            if ui.button(t("Duplicate Pages")).clicked() {
                                action = Some(PageAction::Duplicate);
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button(t("Page Properties…")).clicked() {
                                action = Some(PageAction::Properties(page as usize));
                                ui.close_menu();
                            }
                            if ui.button(t("Delete Pages")).clicked() {
                                action = Some(PageAction::Delete);
                                ui.close_menu();
                            }
                            ui.label(
                                RichText::new(format!("{} page(s) selected", selected.len()))
                                    .size(LABEL)
                                    .color(pal().muted),
                            );
                        });

                        ui.add_space(SPACE_M);
                    }
                });

            // Fin du glissé : la vignette survolée devient la nouvelle position.
            // `drag_stopped` vient de la vignette qui a porté le glissé : un
            // relâchement ailleurs (curseur sorti de la fenêtre) ne déplace rien.
            if drag_stopped {
                if let Some(from) = drag_from.take() {
                    if let Some(to) = drop_target {
                        if to != from {
                            action = Some(PageAction::Move(from, to));
                        }
                    }
                }
            }

            self.thumb_selected = selected;
            self.thumb_pt = zoom;
            self.thumb_drag = drag_from;
            if let Some(page) = jump {
                if let Some(tab) = self.tabs.get_mut(active) {
                    tab.page = page;
                }
                self.jump = Some(page);
            }
            if let Some(action) = action {
                self.run_page_action(&ctx, active, action);
            }
        }

}
