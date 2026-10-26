//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel slideshow_ui).

use std::time::Instant;

use super::*;

/// Vue « présentateur » : un second cadrage sur les pages, ouvert en plus de
/// la présentation plein écran. Elle affiche la page courante en grand, le
/// temps écoulé, et la page suivante en petite vignette.
///
/// ponytail: la page suivante est servie par le même cache de vignettes que
/// le panneau Pages (palier `THUMBNAIL_STEP`) : pas de second rendu en
/// résolution complète. Les layouts complexes (multi-colonnes, tableaux) peuvent
/// faire chevaucher la vignette sur la page courante ; un moteur de réflow
/// viendrait si des workflows de traduction professionnelle en avaient
/// besoin.
#[derive(Clone, Copy, Debug)]
pub struct PresenterView {
    /// Instant de départ du chronomètre de présentation.
    pub start_time: Instant,
    /// Afficher la page suivante à droite du bouton « Suivant ».
    pub show_next_page: bool,
    /// Afficher le chronomètre dans la barre d'en-tête.
    pub show_timer: bool,
}

impl Default for PresenterView {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            show_next_page: true,
            show_timer: true,
        }
    }
}

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
            // La fenêtre du présentateur s'ouvre en même temps que la
            // présentation : c'est un second cadrage sur les mêmes pages, et
            // non une fenêtre indépendante.
            self.presenter_open = true;
            self.presenter.start_time = Instant::now();
            self.panel = None;
            if !self.fullscreen {
                self.toggle_fullscreen(ctx);
            }
        }

        /// Sort de la présentation.
        pub(super) fn exit_slideshow(&mut self, ctx: &egui::Context) {
            self.slideshow = false;
            self.presenter_open = false;
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

    /// Fenêtre « Presenter View » : la page courante en grand, un chronomètre,
    /// et la page suivante en petite vignette (le même cache de vignettes que
    /// le panneau Pages, donc aucun second rendu à pleine résolution).
    pub(super) fn presenter_window(&mut self, ctx: &egui::Context) {
        // On lit l'état du présentateur avant d'emprunter le tableau : la
        // fenêtre emprunte `self` de façon mutable (pour les cases à cocher et
        // le statut de rendu), et le tableau à son tour est emprunté mutably
        // pour la vignette. Les deux emprunts ne se chevauchent pas.
        let presenter = self.presenter;
        let open = self.presenter_open;
        let active = self.active;
        let Some(tab) = self.tabs.get_mut(active) else {
            self.presenter_open = false;
            return;
        };
        let total = tab.total().max(1);
        let page = tab.page.clamp(1, total);
        let ppp = ctx.pixels_per_point();
        let ctx2 = ctx.clone();
        let mut next_page: Option<usize> = None;

        // La page suivante, en vignette : le même cache que le panneau Pages
        // dessert ce palier, pas un second rendu à pleine résolution. Elle est
        // calculée avant la fenêtre pour éviter un double emprunt de `tab`.
        let next_thumb = if presenter.show_next_page {
            let next = (page + 1).min(total);
            let (wp, _hp) = tab.page_size_cached(next as u32);
            let scale = (200.0 / wp.max(1.0)).min(1.0);
            Self::page_texture(
                &ctx2,
                &*self.renderer,
                tab,
                next as u32,
                THUMBNAIL_STEP,
                scale,
                ppp,
                true,
                false,
                &mut self.render_status,
            )
        } else {
            None
        };

        egui::Window::new("Presenter View")
            .collapsible(false)
            .resizable(true)
            .default_width(420.0)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-16.0, -16.0))
            .show(ctx, |ui| {
                let elapsed = Instant::now() - presenter.start_time;
                let minutes = elapsed.as_secs() / 60;
                let seconds = elapsed.as_secs() % 60;
                ui.horizontal(|ui| {
                    if presenter.show_timer {
                        ui.label(
                            RichText::new(format!("{minutes}:{seconds:02}"))
                                .size(LABEL)
                                .color(pal().muted),
                        );
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!("{page} / {total}"))
                                .size(LABEL)
                                .color(pal().icon_bright),
                        );
                    });
                });
                ui.add_space(SPACE_XS);

                ui.horizontal(|ui| {
                    // Page courante : les boutons la font tourner.
                    if ui.button(t("◀ Prev")).clicked() {
                        next_page = Some(page.saturating_sub(1).max(1));
                    }
                    if ui.button(t("Next ▶")).clicked() {
                        next_page = Some((page + 1).min(total));
                    }
                    if let Some(tex) = next_thumb.as_ref() {
                        ui.add(egui::Image::from_texture(tex));
                    }
                });
                ui.add_space(SPACE_XS);
                ui.horizontal(|ui| {
                    let mut show_timer = presenter.show_timer;
                    let mut show_next = presenter.show_next_page;
                    ui.checkbox(&mut show_timer, t("Timer"));
                    ui.checkbox(&mut show_next, t("Next page"));
                    if ui.button(t("Reset timer")).clicked() {
                        self.presenter.start_time = Instant::now();
                    }
                    self.presenter.show_timer = show_timer;
                    self.presenter.show_next_page = show_next;
                });
            });

        self.presenter_open = open;
        if let Some(target) = next_page {
            if let Some(tab) = self.tabs.get_mut(active) {
                tab.page = target;
            }
        }
    }

}
