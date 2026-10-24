//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel slideshow_ui).

use super::*;

impl YouPdfApp {

        /// Bascule le plein écran natif (le chrome maison reste visible).
        pub(super) fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }

        /// Entre en présentation plein écran sur la page courante.
        pub(super) fn enter_slideshow(&mut self, ctx: &egui::Context) {
            if self.tabs.is_empty() {
                self.toast(ctx, "Open a document first");
                return;
            }
            self.slideshow = true;
            self.panel = None;
            if !self.fullscreen {
                self.toggle_fullscreen(ctx);
            }
        }

        /// Sort de la présentation.
        pub(super) fn exit_slideshow(&mut self, ctx: &egui::Context) {
            self.slideshow = false;
            if self.fullscreen {
                self.toggle_fullscreen(ctx);
            }
        }

        /// Présentation : fond noir, une page à la fois, flèches pour tourner.
        pub(super) fn slideshow_view(&mut self, ctx: &egui::Context) {
            if self.tabs.is_empty() {
                self.exit_slideshow(ctx);
                return;
            }
            let (left, right, first, last, escape, quit) = ctx.input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowLeft)
                        || i.key_pressed(egui::Key::ArrowUp)
                        || i.key_pressed(egui::Key::PageUp),
                    i.key_pressed(egui::Key::ArrowRight)
                        || i.key_pressed(egui::Key::ArrowDown)
                        || i.key_pressed(egui::Key::PageDown)
                        || i.key_pressed(egui::Key::Space),
                    i.key_pressed(egui::Key::Home),
                    i.key_pressed(egui::Key::End),
                    i.key_pressed(egui::Key::Escape),
                    i.key_pressed(egui::Key::F5),
                )
            });
            if escape || quit {
                self.exit_slideshow(ctx);
                return;
            }
            let total = self.active_total() as usize;
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if first {
                    tab.page = 1;
                }
                if last {
                    tab.page = total.max(1);
                }
                if left {
                    tab.page = tab.page.saturating_sub(1).max(1);
                }
                if right {
                    tab.page = (tab.page + 1).min(total.max(1));
                }
                // Une page entière par écran : l'ajustement automatique s'en charge.
                tab.zoom = None;
                tab.zoom_fit_page = true;
            }
            CentralPanel::default()
                .frame(Frame::none().fill(Color32::BLACK))
                .show(ctx, |ui| {
                    self.canvas(ui);
                });
            let page = self.tabs[self.active].page;
            Area::new(Id::new("slideshow-hud"))
                .fixed_pos(pos2(16.0, 16.0))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(Color32::from_black_alpha(160))
                        .rounding(Rounding::same(RADIUS_L))
                        .inner_margin(Margin::symmetric(10.0, 6.0))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("{page} / {total}"))

                                    .color(Color32::WHITE),
                            );
                            ui.label(
                                RichText::new(t("← → to turn, Esc to leave"))
                                    .size(CAPTION)
                                    .color(Color32::from_gray(170)),
                            );
                        });
                });
        }

}
