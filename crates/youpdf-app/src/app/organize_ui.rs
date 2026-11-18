//! Surface plein écran « Organiser les pages ».
//!
//! Remplace le canevas du document : grille de vignettes, sélection multiple
//! (clic, Maj, Cmd), actions de page, glisser-déposer pour réordonner, et
//! fenêtres d'extraction, d'insertion, de remplacement et de découpe.

use super::*;
use youpdf_shared::{LABEL, t, RADIUS_TINY};

/// Fenêtre ouverte par-dessus la grille.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OrganizeDialog {
    Extract,
    Insert,
    Replace,
    Split,
}

impl YouPdfApp {
    /// Dessine la surface plein écran (appelée à la place du canevas).
    pub(super) fn organize_view(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get(self.active) else {
            self.organize_open = false;
            return;
        };
        let total = tab.total();
        let current = tab.page;
        let mut selected = std::mem::take(&mut self.organize_selected);
        selected.retain(|page| *page >= 1 && *page <= total);
        let mut anchor = self.organize_anchor;
        let mut drag = self.organize_drag;
        let mut dialog = self.organize_dialog;
        let mut close = false;

        // ---- Barre flottante « Organiser », calquée sur UPDF 2.5.6 -------
        // Capture : docs/screenshots/Capture … 18.59.15.png.
        fn bar_button(ui: &mut egui::Ui, icon: IconFn, label: &str) -> bool {
            let (rect, resp) = ui.allocate_exact_size(vec2(104.0, 34.0), Sense::click());
            let fill = if resp.hovered() {
                pal().hover_bg
            } else {
                Color32::TRANSPARENT
            };
            ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), fill);
            let color = if resp.hovered() {
                pal().icon_bright
            } else {
                pal().icon
            };
            icon(
                ui.painter(),
                Rect::from_center_size(pos2(rect.min.x + 18.0, rect.center().y), vec2(18.0, 18.0)),
                color,
            );
            ui.painter().text(
                pos2(rect.min.x + 34.0, rect.center().y),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(12.5),
                color,
            );
            resp.clicked()
        }

        fn icon_button(ui: &mut egui::Ui, icon: IconFn, enabled: bool) -> bool {
            let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
            let color = if !enabled {
                pal().muted
            } else if resp.hovered() {
                pal().icon_bright
            } else {
                pal().icon
            };
            icon(ui.painter(), rect.shrink(6.0), color);
            resp.clicked()
        }

        // Mesures natives sur la capture UPDF 18.59.15 (2934×1912, 2×) :
        // groupe gauche x=15–420 px (8–210 pt), groupe outils x=568–2359 px
        // (284–1180 pt, donc centré), groupe actions x=2660–2912 px à droite.
        let group = || {
            Frame::none()
                .fill(pal().widget_bg)
                .rounding(Rounding::same(RADIUS_L))
                .stroke(Stroke::new(1.0_f32, pal().separator))
                .inner_margin(Margin::symmetric(8.0, 5.0))
        };

        // Aide + interrupteur « Sélection multiple », collé au bord gauche.
        egui::Area::new(Id::new("org-select"))
            .anchor(Align2::LEFT_TOP, vec2(8.0, TITLE_H + 8.0))
            .order(Order::Foreground)
            .show(ui.ctx(), |ui| {
                group().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (help, _) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                        ui.painter().text(
                            help.center(),
                            Align2::CENTER_CENTER,
                            "?",
                            FontId::proportional(14.0),
                            pal().muted,
                        );
                        ui.add_space(4.0);
                        ui.label(RichText::new(t("Multiple selection")).size(LABEL));
                        let (sw, sw_resp) =
                            ui.allocate_exact_size(vec2(28.0, 16.0), Sense::click());
                        ui.painter().rect_filled(
                            sw,
                            Rounding::same(8.0),
                            if self.organize_multi {
                                pal().accent
                            } else {
                                pal().hover_bg
                            },
                        );
                        let knob = if self.organize_multi {
                            sw.max.x - 8.0
                        } else {
                            sw.min.x + 8.0
                        };
                        ui.painter()
                            .circle_filled(pos2(knob, sw.center().y), 6.0, Color32::WHITE);
                        if sw_resp.clicked() {
                            self.organize_multi = !self.organize_multi;
                        }
                    });
                });
            });

        // Outils de page, centrés.
        egui::Area::new(Id::new("org-tools"))
            .anchor(Align2::CENTER_TOP, vec2(0.0, TITLE_H + 8.0))
            .order(Order::Foreground)
            .show(ui.ctx(), |ui| {
                group().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.menu_button(
                            RichText::new(t("Select pages")).size(LABEL),
                            |ui| {
                                if ui.button(t("Select all")).clicked() {
                                    selected = (1..=total).collect();
                                    anchor = None;
                                    ui.close_menu();
                                }
                                if ui.button(t("Deselect all")).clicked() {
                                    selected.clear();
                                    anchor = None;
                                    ui.close_menu();
                                }
                                if ui.button(t("Odd")).clicked() {
                                    selected = (1..=total).filter(|p| p % 2 == 1).collect();
                                    ui.close_menu();
                                }
                                if ui.button(t("Even")).clicked() {
                                    selected = (1..=total).filter(|p| p % 2 == 0).collect();
                                    ui.close_menu();
                                }
                            },
                        );
                        ui.separator();
                        if bar_button(ui, icon_file_plus, t("Insert")) {
                            dialog = Some(OrganizeDialog::Insert);
                        }
                        if bar_button(ui, icon_export, t("Extract")) {
                            dialog = Some(OrganizeDialog::Extract);
                        }
                        if bar_button(ui, icon_page, t("Replace")) {
                            dialog = Some(OrganizeDialog::Replace);
                        }
                        if bar_button(ui, icon_scissors, &format!("{} ▾", t("Split"))) {
                            dialog = Some(OrganizeDialog::Split);
                        }
                        ui.menu_button("•••", |ui| {
                            if ui.button(t("Rotate left")).clicked() {
                                self.rotate_pages(ui.ctx(), &selected, total, -90);
                                ui.close_menu();
                            }
                            if ui.button(t("Rotate right")).clicked() {
                                self.rotate_pages(ui.ctx(), &selected, total, 90);
                                ui.close_menu();
                            }
                            if ui.button(t("Duplicate")).clicked() {
                                self.duplicate_pages(ui.ctx(), &selected, total);
                                ui.close_menu();
                            }
                            if ui.button(t("Merge")).clicked() {
                                self.merge_pages(ui.ctx(), &selected, 2);
                                ui.close_menu();
                            }
                            if ui.button(t("Page Tools…")).clicked() {
                                self.page_tools_open = true;
                                ui.close_menu();
                            }
                            if ui.button(t("Page Labels…")).clicked() {
                                self.page_labels_open = true;
                                ui.close_menu();
                            }
                        });
                    });
                });
            });

        // Annuler / rétablir / supprimer / Fermer, collé au bord droit.
        egui::Area::new(Id::new("org-actions"))
            .anchor(Align2::RIGHT_TOP, vec2(-10.0, TITLE_H + 8.0))
            .order(Order::Foreground)
            .show(ui.ctx(), |ui| {
                group().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let can_undo = self.tabs.get(self.active).map(|t| !t.undo.is_empty()).unwrap_or(false);
                        let can_redo = self.tabs.get(self.active).map(|t| !t.redo.is_empty()).unwrap_or(false);
                        if icon_button(ui, icon_undo, can_undo) && can_undo {
                            self.undo(ui.ctx());
                        }
                        if icon_button(ui, icon_redo, can_redo) && can_redo {
                            self.redo(ui.ctx());
                        }
                        if icon_button(ui, icon_trash, true) && !selected.is_empty() {
                            if selected.len() >= total {
                                self.toast(ui.ctx(), "Keep at least one page".to_string());
                            } else {
                                self.ask_delete_pages(&selected, total);
                            }
                        }
                        ui.add_space(6.0);
                        if ui.button(RichText::new(t("Close")).size(LABEL)).clicked() {
                            close = true;
                        }
                    });
                });
            });

        // ---- Grille ------------------------------------------------------
        let active = self.active;
        let ppp = ui.ctx().pixels_per_point();
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
        let Some(tab) = self.tabs.get_mut(active) else {
            return;
        };
        let mut pending_move: Option<(usize, usize)> = None;
        let mut insert_bar: Option<(Rect, bool)> = None;
        // Actions rapides différées : la grille emprunte déjà `self.tabs`.
        let mut quick_rotate: Option<usize> = None;
        let mut quick_delete: Option<usize> = None;
        let pointer = ui.ctx().pointer_interact_pos();

        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal().canvas_bg))
            .show_inside(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    // La barre flottante occupe le haut : on laisse la place
                    // pour que la première rangée ne passe pas dessous.
                    ui.add_space(68.0);
                    ui.horizontal_wrapped(|ui| {
                        // Cellules dimensionnées sur la page : plus de grand
                        // vide vertical pour les documents paysage.
                        ui.spacing_mut().item_spacing = vec2(20.0, 28.0);
                        ui.add_space(20.0);
                        for page in 1..=total as u32 {
                            let (wp, hp) = tab.page_size_cached(page);
                            // Mesuré sur la capture UPDF 18.59.15 (2×) :
                            // page ≈ 432 px → 216 pt de large.
                            let scale = 216.0 / wp.max(1.0);
                            let thumb = vec2(wp * scale, hp * scale);
                            let (rect, resp) = ui.allocate_exact_size(
                                vec2(thumb.x + 20.0, thumb.y + 48.0),
                                Sense::click_and_drag(),
                            );
                            let img_rect = Rect::from_min_max(
                                pos2(rect.min.x + 10.0, rect.min.y + 8.0),
                                pos2(rect.min.x + 10.0 + thumb.x, rect.min.y + 8.0 + thumb.y),
                            );
                            let picked = selected.contains(&(page as usize));
                            ui.painter().rect_filled(
                                img_rect.expand(3.0),
                                Rounding::same(RADIUS_TINY),
                                if picked {
                                    pal().thumb_tint
                                } else {
                                    pal().widget_bg
                                },
                            );

                            if let Some(tex) = Self::page_texture(
                                ui.ctx(),
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
                                    img_rect.expand(3.0),
                                    Rounding::same(6.0),
                                    Stroke::new(4.0_f32, pal().accent),
                                );
                                // Pastille de sélection.
                                let badge = pos2(img_rect.min.x + 9.0, img_rect.min.y + 9.0);
                                ui.painter().circle_filled(badge, 8.0, pal().accent);
                                ui.painter().text(
                                    badge,
                                    Align2::CENTER_CENTER,
                                    ph::CHECK,
                                    FontId::proportional(LABEL),
                                    Color32::WHITE,
                                );
                            }
                            ui.painter().text(
                                pos2(rect.center().x, img_rect.max.y + 14.0),
                                Align2::CENTER_CENTER,
                                page.to_string(),
                                FontId::proportional(LABEL),
                                if picked { pal().accent } else { pal().muted },
                            );
                            if page as usize == current {
                                ui.painter().circle_filled(
                                    pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                                    3.0,
                                    pal().accent,
                                );
                            }

                            // Actions rapides au survol. `ui.put` peint sur un Ui
                            // fils autonome : son min_rect remonte au parent, qui
                            // recule alors son curseur de ligne jusqu'au bouton et
                            // déplace la page suivante. On repousse le curseur sur
                            // la carte après les deux `put`.
                            if resp.hovered() {
                                let quick =
                                    Rect::from_min_size(img_rect.max - vec2(44.0, 20.0), vec2(40.0, 18.0));
                                ui.painter().rect_filled(
                                    quick,
                                    Rounding::same(RADIUS_TINY),
                                    Color32::from_black_alpha(170),
                                );
                                let left = Rect::from_min_size(quick.min, vec2(20.0, 18.0));
                                let right = Rect::from_min_size(
                                    pos2(quick.min.x + 20.0, quick.min.y),
                                    vec2(20.0, 18.0),
                                );
                                if ui.put(left, Button::new("⟳").small()).clicked() {
                                    quick_rotate = Some(page as usize);
                                }
                                if ui.put(right, Button::new(ph::X).small()).clicked() {
                                    quick_delete = Some(page as usize);
                                }
                                ui.advance_cursor_after_rect(rect);
                            }

                            // Glisser-déposer : barre d'insertion avant/après.
                            if resp.drag_started() {
                                drag = Some(page as usize);
                            }
                            if drag.is_some() && resp.hovered() {
                                let after = pointer
                                    .map(|point| point.x > rect.center().x)
                                    .unwrap_or(false);
                                let x = if after { rect.max.x } else { rect.min.x };
                                insert_bar = Some((
                                    Rect::from_min_max(
                                        pos2(x - 1.5, rect.min.y),
                                        pos2(x + 1.5, rect.max.y),
                                    ),
                                    after,
                                ));
                                if let Some(from) = drag {
                                    if ui.input(|i| i.pointer.any_released()) {
                                        let mut to = page as usize;
                                        if after {
                                            to += 1;
                                        }
                                        pending_move = Some((from, to));
                                    }
                                }
                            }
                            if resp.drag_stopped() {
                                drag = None;
                            }
                            if resp.clicked() {
                                let modifiers = ui.input(|i| i.modifiers);
                                let page_usize = page as usize;
                                if modifiers.shift {
                                    if let Some(origin) = anchor {
                                        let (low, high) =
                                            (origin.min(page_usize), origin.max(page_usize));
                                        selected.extend(low..=high);
                                    } else {
                                        selected.insert(page_usize);
                                        anchor = Some(page_usize);
                                    }
                                } else if modifiers.command || self.organize_multi {
                                    if !selected.remove(&page_usize) {
                                        selected.insert(page_usize);
                                    }
                                    anchor = Some(page_usize);
                                } else {
                                    selected.clear();
                                    selected.insert(page_usize);
                                    anchor = Some(page_usize);
                                }
                            }
                        }
                    });
                    ui.add_space(16.0);
                });
            });

        if let Some((bar, _)) = insert_bar {
            ui.painter().rect_filled(
                bar,
                Rounding::ZERO,
                pal().warn,
            );
        }

        self.organize_selected = selected;
        self.organize_anchor = anchor;
        self.organize_drag = drag;
        self.organize_dialog = dialog;
        if close {
            self.organize_open = false;
            self.organize_selected.clear();
            self.organize_dialog = None;
        }
        // Les actions rapides passent après la restitution de la sélection :
        // une suppression remet la sienne à zéro, et l'écrire avant
        // ressusciterait des pages disparues.
        if let Some(page) = quick_rotate {
            let single: BTreeSet<usize> = [page].into_iter().collect();
            self.rotate_pages(ui.ctx(), &single, total, 90);
        }
        if let Some(page) = quick_delete {
            let single: BTreeSet<usize> = [page].into_iter().collect();
            self.ask_delete_pages(&single, total);
        }
        if let Some((from, to)) = pending_move {
            if let Some(tab) = self.tabs.get_mut(active) {
                let target = to.min(tab.total());
                if from != target && tab.doc.move_page(from, target).is_ok() {
                    self.mark_document_changed();
                    self.toast(ui.ctx(), format!("Page {from} moved to {target}"));
                }
            }
        }

        // Raccourcis de la surface.
        if ui.input(|i| i.key_pressed(egui::Key::A) && i.modifiers.command) {
            self.organize_selected = (1..=total).collect();
        }
        if ui.input(|i| {
            (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
                && !i.modifiers.command
        }) && self.organize_selected.len() < total
        {
            let sel = self.organize_selected.clone();
            self.ask_delete_pages(&sel, total);
        }

        self.organize_dialogs(ui.ctx());
        self.delete_confirm_window(ui.ctx());
    }

    /// Actions groupées sur la sélection (vide = tout le document).
    pub(super) fn organize_targets(&self, selected: &BTreeSet<usize>, total: usize) -> Vec<usize> {
        if selected.is_empty() {
            (1..=total).collect()
        } else {
            selected.iter().copied().collect()
        }
    }

}

/// Pages qu'une suppression doit viser, ou `None` quand il n'y a rien à
/// confirmer : plus rien à supprimer, ou tout le document (que l'app refuse
/// de vider).
pub(super) fn pages_to_confirm_delete(pages: &[usize], total: usize) -> Option<Vec<usize>> {
    if pages.is_empty() || pages.len() >= total {
        None
    } else {
        Some(pages.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::pages_to_confirm_delete;

    #[test]
    fn une_selection_partielle_demande_confirmation() {
        assert_eq!(pages_to_confirm_delete(&[2, 3], 10), Some(vec![2, 3]));
    }

    #[test]
    fn une_selection_vide_ne_demande_rien() {
        assert_eq!(pages_to_confirm_delete(&[], 10), None);
    }

    #[test]
    fn tout_le_document_ne_demande_rien() {
        let tout: Vec<usize> = (1..=4).collect();
        assert_eq!(pages_to_confirm_delete(&tout, 4), None);
    }

    #[test]
    fn tout_sauf_une_page_demande_confirmation() {
        assert_eq!(pages_to_confirm_delete(&[1, 2, 3], 4), Some(vec![1, 2, 3]));
    }
}
