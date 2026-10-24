//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel shortcuts_ui).


use super::*;

impl YouPdfApp {

        pub(super) fn shortcuts(&mut self, ctx: &egui::Context) {
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
                self.prefs_open = true;
                return;
            }
            // Une saisie en cours (recherche, nom de champ, remplacement) a la
            // priorité : sinon Retour arrière efface l'annotation sélectionnée au
            // lieu d'un caractère.
            let typing = ctx.wants_keyboard_input();
            // ⌘Z / ⇧⌘Z : l'historique de l'onglet actif. Un champ de saisie au
            // premier plan garde son propre annuler/refaire.
            if !typing && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
                if ctx.input(|i| i.modifiers.shift) {
                    self.redo(ctx);
                } else {
                    self.undo(ctx);
                }
                return;
            }
            if ctx.input(|i| i.key_pressed(egui::Key::F5)) && !typing {
                self.enter_slideshow(ctx);
                return;
            }
            let delete_pressed = ctx.input(|i| {
                i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)
            }) && !typing;
            if delete_pressed && self.has_annot_selection {
                let idx = self.selected_annot;
                self.remove_annot(idx);
                self.toast(ctx, "Annotation removed");
                return;
            }
            let (cmd, shift, open, close, plus, minus, zero, save, find, full, print, props) = ctx
                .input(|i| {
                    (
                        i.modifiers.command,
                        i.modifiers.shift,
                        i.key_pressed(egui::Key::O),
                        i.key_pressed(egui::Key::W),
                        i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                        i.key_pressed(egui::Key::Minus),
                        i.key_pressed(egui::Key::Num0),
                        i.key_pressed(egui::Key::S),
                        i.key_pressed(egui::Key::G),
                        i.key_pressed(egui::Key::F),
                        i.key_pressed(egui::Key::P),
                        i.key_pressed(egui::Key::D),
                    )
                });
            // Copie et « tout sélectionner » portent sur le texte de la page : un
            // champ de saisie au premier plan garde la priorité (garde `typing`).
            let (copy_text, select_all) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::C) && i.modifiers.command && !i.modifiers.shift,
                    i.key_pressed(egui::Key::A) && i.modifiers.command && !i.modifiers.shift,
                )
            });
            if !typing && copy_text {
                self.copy_selection(ctx);
                return;
            }
            if !typing && select_all {
                self.select_page_text();
                return;
            }
            if cmd && find {
                self.search_step(if shift { -1 } else { 1 });
                return;
            }
            // Raccourcis Ctrl+1..0 du menu « Outils ». Ils sont lus avant le
            // garde `cmd` ci-dessous : sinon toutes les combinaisons Ctrl seules
            // seraient ignorées.
            let ctrl_tool = ctx.input(|i| {
                if !i.modifiers.ctrl || i.modifiers.command {
                    return None;
                }
                const DIGITS: [egui::Key; 10] = [
                    egui::Key::Num1,
                    egui::Key::Num2,
                    egui::Key::Num3,
                    egui::Key::Num4,
                    egui::Key::Num5,
                    egui::Key::Num6,
                    egui::Key::Num7,
                    egui::Key::Num8,
                    egui::Key::Num9,
                    egui::Key::Num0,
                ];
                DIGITS.iter().position(|key| i.key_pressed(*key))
            });
            if let Some(index) = ctrl_tool {
                match index {
                    0..=3 => {
                        self.mode = Some(index);
                        self.annot_tool = None;
                        self.preset_open = false;
                        self.text_sel = None;
                    }
                    4 => self.organize_open = true,
                    5 => self.crop_open = true,
                    6 => self.tile_split_open = true,
                    7 => self.watermark_open = true,
                    8 => self.background_open = true,
                    _ => self.header_footer_open = true,
                }
                return;
            }
            if !cmd {
                return;
            }
            if full {
                if shift {
                    self.toggle_fullscreen(ctx);
                } else {
                    self.panel = Some(Panel::Search);
                    self.find_focus = true;
                }
            }
            if save {
                if shift {
                    self.save_as(ctx);
                } else {
                    self.save_active(ctx);
                }
            }
            if open {
                self.open_dialog();
            }
            if close {
                let idx = self.active;
                self.close_tab(idx);
            }
            if plus {
                self.zoom_by(1.1);
            }
            if minus {
                self.zoom_by(1.0 / 1.1);
            }
            if zero {
                self.set_zoom(None);
            }
            if print {
                self.print_active(ctx);
            }
            if props {
                self.props_open = true;
            }
        }

}
