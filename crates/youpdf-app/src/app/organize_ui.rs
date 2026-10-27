//! Surface plein écran « Organiser les pages ».
//!
//! Remplace le canevas du document : grille de vignettes, sélection multiple
//! (clic, Maj, Cmd), actions de page, glisser-déposer pour réordonner, et
//! fenêtres d'extraction, d'insertion, de remplacement et de découpe.

use super::*;
use youpdf_shared::{LABEL, TITLE, t, SPACE_XS, RADIUS_TINY};

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

        // ---- Barre supérieure -------------------------------------------
        egui::TopBottomPanel::top("organize-toolbar")
            .frame(Frame::none().fill(pal().widget_bg).inner_margin(Margin::symmetric(12.0, 8.0)))
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(t("Organize pages"))
                            .size(TITLE)
                            .color(pal().icon_bright),
                    );
                    ui.add_space(10.0);
                    let count = if selected.is_empty() { total } else { selected.len() };
                    ui.label(
                        RichText::new(format!("{count} of {total} pages selected"))
                            .size(LABEL)
                            .color(pal().muted),
                    );
                    if ui.small_button("Select all").clicked() {
                        selected = (1..=total).collect();
                        anchor = None;
                    }
                    if ui.small_button("Deselect all").clicked() {
                        selected.clear();
                        anchor = None;
                    }
                    if ui.small_button("Odd").clicked() {
                        selected = (1..=total).filter(|page| page % 2 == 1).collect();
                    }
                    if ui.small_button("Even").clicked() {
                        selected = (1..=total).filter(|page| page % 2 == 0).collect();
                    }
                    ui.separator();
                    if organize_button(ui, icon_rotate_left, "Rotate left") {
                        self.rotate_pages(ui.ctx(), &selected, total, -90);
                    }
                    if organize_button(ui, icon_rotate_right, "Rotate right") {
                        self.rotate_pages(ui.ctx(), &selected, total, 90);
                    }
                    if organize_button(ui, icon_trash, "Delete") {
                        if selected.len() >= total {
                            self.toast(ui.ctx(), "Keep at least one page".to_string());
                        } else {
                            self.ask_delete_pages(&selected, total);
                        }
                    }
                    if organize_button(ui, icon_export, "Extract…") {
                        dialog = Some(OrganizeDialog::Extract);
                    }
                    if organize_button(ui, icon_file_plus, "Insert…") {
                        dialog = Some(OrganizeDialog::Insert);
                    }
                    if ui.small_button("Replace…").clicked() {
                        dialog = Some(OrganizeDialog::Replace);
                    }
                    if organize_button(ui, icon_scissors, "Split…") {
                        dialog = Some(OrganizeDialog::Split);
                    }
                    if ui.small_button("Duplicate").clicked() {
                        self.duplicate_pages(ui.ctx(), &selected, total);
                    }
                    if ui.small_button("Merge").clicked() {
                        self.merge_pages(ui.ctx(), &selected, 2);
                    }
                    ui.separator();
                    if organize_button(ui, icon_gear, "Page Tools…") {
                        self.page_tools_open = true;
                    }
                    ui.separator();
                    if ui.button(format!("{} Close", ph::X)).clicked() {
                        close = true;
                    }
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
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(12.0);
                        for page in 1..=total as u32 {
                            let (wp, hp) = tab.page_size_cached(page);
                            let scale = 104.0 / wp.max(1.0);
                            let thumb = vec2(wp * scale, hp * scale);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(132.0, 196.0), Sense::click_and_drag());
                            let img_rect = Rect::from_center_size(
                                pos2(rect.center().x, rect.min.y + 10.0 + thumb.y * 0.5),
                                thumb,
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
                                    3.0,
                                    Stroke::new(1.5_f32, pal().accent),
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
                                } else if modifiers.command {
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
                            ui.add_space(SPACE_XS);
                        }
                    });
                    ui.add_space(16.0);
                });
            });

        if let Some((bar, _)) = insert_bar {
            ui.painter().rect_filled(
                bar,
                Rounding::ZERO,
                Color32::from_rgb(230, 120, 40),
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
