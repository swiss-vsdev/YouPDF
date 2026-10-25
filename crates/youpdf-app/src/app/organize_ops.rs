//! Fenêtres d'action de la vue « Organiser les pages » : extraction,
//! insertion (fichier, page vierge, presse-papiers), remplacement, découpe.

use super::*;
use super::organize_ui::pages_to_confirm_delete;
use youpdf_shared::{LABEL, t};

impl YouPdfApp {
    pub(super) fn rotate_pages(
        &mut self,
        ctx: &egui::Context,
        selected: &BTreeSet<usize>,
        total: usize,
        delta: i64,
    ) {
        let pages = self.organize_targets(selected, total);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            match tab.doc.rotate_pages(&pages, delta) {
                Ok(count) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) rotated"));
                }
                Err(err) => self.toast(ctx, format!("Rotation failed: {err}")),
            }
        }
    }

    /// Demande confirmation avant de supprimer : la suppression est
    /// destructive, et la seule autre sortie est Annuler.
    pub(super) fn ask_delete_pages(&mut self, selected: &BTreeSet<usize>, total: usize) {
        if let Some(pages) = pages_to_confirm_delete(&self.organize_targets(selected, total), total) {
            self.organize_pending_delete = Some(pages);
        }
    }

    /// Fenêtre de confirmation de la suppression.
    pub(super) fn delete_confirm_window(&mut self, ctx: &egui::Context) {
        let Some(pages) = self.organize_pending_delete.clone() else {
            return;
        };
        let total = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;
        self.show_modal(ctx, "modal-delete-pages", "Delete pages", 340.0, &mut open, |_app, ui| {
            ui.label(
                RichText::new(format!("Remove {} page(s) from this document?", pages.len()))
                    .size(BODY)
                    .color(pal().icon_bright),
            );
            ui.label(
                RichText::new(t("The pages leave the file. Undo brings them back."))
                    .size(LABEL)
                    .color(pal().muted),
            );
            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                if ui.button(t("Delete")).clicked() {
                    confirmed = true;
                }
                if ui.button(t("Cancel")).clicked() {
                    cancelled = true;
                }
            });
        });
        if confirmed {
            self.organize_pending_delete = None;
            let selection: BTreeSet<usize> = pages.into_iter().collect();
            self.delete_pages(ctx, &selection, total);
        } else if cancelled || !open {
            self.organize_pending_delete = None;
        }
    }

    pub(super) fn delete_pages(&mut self, ctx: &egui::Context, selected: &BTreeSet<usize>, total: usize) {
        let pages = self.organize_targets(selected, total);
        if pages.len() >= total {
            return;
        }
        let outcome = match self.tabs.get_mut(self.active) {
            Some(tab) => match tab.doc.delete_pages(&pages) {
                Ok(count) => {
                    let total = tab.total();
                    if tab.page > total {
                        tab.page = total.max(1);
                    }
                    Ok(count)
                }
                Err(err) => Err(err),
            },
            None => return,
        };
        match outcome {
            Ok(count) => {
                self.mark_document_changed();
                self.organize_selected.clear();
                self.toast(ctx, format!("{count} page(s) removed"));
            }
            Err(err) => self.toast(ctx, format!("Delete failed: {err}")),
        }
    }

    pub(super) fn duplicate_pages(&mut self, ctx: &egui::Context, selected: &BTreeSet<usize>, total: usize) {
        let pages = self.organize_targets(selected, total);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            match tab.doc.duplicate_pages(&pages) {
                Ok(count) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) duplicated"));
                }
                Err(err) => self.toast(ctx, format!("Duplicate failed: {err}")),
            }
        }
    }

}
