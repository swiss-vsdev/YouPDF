//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel tabs_ui).

use super::*;

impl YouPdfApp {

        /// Déplace l'onglet `from` à la position `to` (glissé dans la barre).
        ///
        /// `self.active` suit le même document : il est décalé pour le retrait,
        /// puis pour l'insertion, ce qui évite qu'un glissé change l'onglet affiché.
        pub(super) fn move_tab(&mut self, from: usize, to: usize) {
            if from == to || from >= self.tabs.len() {
                return;
            }
            let after = self.tabs.len() - 1;
            let tab = self.tabs.remove(from);
            let pos = to.min(after);
            self.tabs.insert(pos, tab);
            self.active = moved_active(self.active, from, pos);
            self.last_active = self.active;
        }

        /// Applique une action du menu contextuel d'un onglet.
        pub(super) fn apply_tab_action(&mut self, ctx: &egui::Context, action: TabAction) {
            let count = self.tabs.len();
            match action {
                TabAction::Close(idx) => self.close_tab(idx),
                TabAction::CloseOthers(keep) => {
                    let targets: Vec<usize> = (0..count).filter(|idx| *idx != keep).collect();
                    self.close_many(ctx, &targets);
                }
                TabAction::CloseRight(from) => {
                    let targets: Vec<usize> = (from + 1..count).collect();
                    self.close_many(ctx, &targets);
                }
                TabAction::CloseAll => {
                    let targets: Vec<usize> = (0..count).collect();
                    self.close_many(ctx, &targets);
                }
                TabAction::TogglePin(idx) => {
                    if let Some(tab) = self.tabs.get_mut(idx) {
                        tab.pinned = !tab.pinned;
                    }
                }
                TabAction::Duplicate(idx) => {
                    let path = self.tabs.get(idx).and_then(|tab| tab.path.clone());
                    match path {
                        Some(path) => self.open_path_new_tab(path),
                        None => self.toast(ctx, "This document has no file on disk yet"),
                    }
                }
                TabAction::Reveal(idx) => {
                    let path = self.tabs.get(idx).and_then(|tab| tab.path.clone());
                    match path {
                        Some(path) => self.reveal_path(&path),
                        None => self.toast(ctx, "This document has no file on disk yet"),
                    }
                }
                TabAction::CopyPath(idx) => {
                    let path = self.tabs.get(idx).and_then(|tab| tab.path.clone());
                    match path {
                        Some(path) => ctx.copy_text(path.display().to_string()),
                        None => self.toast(ctx, "This document has no file on disk yet"),
                    }
                }
            }
        }

}
