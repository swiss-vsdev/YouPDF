//! Configuration partagée des fenêtres popup (modales et fenêtres libres).
//!
//! Une seule définition de l'ancrage, du cadre, de la taille et du redimension-
//! nement : les dialogues n'ont plus qu'à choisir un préréglage et fournir
//! leur contenu. Les `Area` (menus volants, toast, inspecteur) ne sont pas des
//! fenêtres egui et restent hors de ce module.

use egui::{Align2, Context, Id, Ui, Vec2};
use youpdf_shared::{RADIUS_XL, current_theme, liquid_glass, modal_frame};

/// Préréglage et configuration d'une popup.
pub struct Popup {
    id: Option<Id>,
    title: String,
    anchor: Option<(Align2, Vec2)>,
    max_width: f32,
    default_size: Option<Vec2>,
    default_width: Option<f32>,
    min_width: Option<f32>,
    resizable: bool,
    modal_frame: bool,
}

impl Popup {
    /// Dialogue centré, non redimensionnable (politique de `show_modal`).
    pub fn modal(id: impl std::hash::Hash, title: impl Into<String>, max_width: f32) -> Self {
        Self {
            id: Some(Id::new(id)),
            title: title.into(),
            anchor: Some((Align2::CENTER_CENTER, Vec2::ZERO)),
            max_width,
            default_size: None,
            default_width: None,
            min_width: None,
            resizable: false,
            modal_frame: true,
        }
    }

    /// Fenêtre libre : mêmes cadre et thème que les modales.
    pub fn window(title: impl Into<String>) -> Self {
        Self {
            id: None,
            title: title.into(),
            anchor: None,
            max_width: 0.0,
            default_size: None,
            default_width: None,
            min_width: None,
            resizable: true,
            modal_frame: true,
        }
    }

    pub fn id(mut self, id: Id) -> Self {
        self.id = Some(id);
        self
    }

    pub fn anchor(mut self, anchor: Align2, offset: Vec2) -> Self {
        self.anchor = Some((anchor, offset));
        self
    }

    pub fn fixed(mut self) -> Self {
        self.resizable = false;
        self
    }

    pub fn default_size(mut self, size: impl Into<Vec2>) -> Self {
        self.default_size = Some(size.into());
        self
    }

    pub fn default_width(mut self, width: f32) -> Self {
        self.default_width = Some(width);
        self
    }

    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = Some(width);
        self
    }

    /// Dessine la fenêtre avec un booléen de fermeture externe.
    pub fn show<R>(
        &self,
        ctx: &Context,
        open: &mut bool,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        self.draw(ctx, Some(open), content)
    }

    /// Fenêtre dont l'état d'ouverture est géré par egui (pas de `.open`).
    pub fn show_detached<R>(&self, ctx: &Context, content: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        self.draw(ctx, None, content)
    }

    /// Surface commune à toutes les popup : fond de carte arrondi et marge,
    /// pour que les dialogues existants paraissent modernes sans être réécrits.
    fn surface<R>(ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
        let p = youpdf_shared::pal();
        let fill = if liquid_glass() {
            p.chrome_bg.gamma_multiply(0.82)
        } else {
            p.widget_bg
        };
        egui::Frame::none()
            .fill(fill)
            .stroke(egui::Stroke::new(1.0_f32, p.separator))
            .rounding(egui::Rounding::same(RADIUS_XL))
            .inner_margin(egui::Margin::symmetric(12.0, 10.0))
            .show(ui, content)
            .inner
    }

    fn draw<R>(
        &self,
        ctx: &Context,
        mut open: Option<&mut bool>,
        content: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        // Une fenêtre native a son propre Context : sans ça elle resterait en
        // Dark alors que le chrome principal suit le thème choisi.
        current_theme().apply(ctx);
        let id = self.id.unwrap_or_else(|| egui::Id::new(&self.title));
        let viewport_id = egui::ViewportId::from_hash_of(id);
        let mut builder = egui::ViewportBuilder::default()
            .with_title(self.title.clone())
            .with_resizable(self.resizable);
        if let Some(size) = self.default_size {
            builder = builder.with_inner_size(size);
        } else if let Some(width) = self.default_width {
            builder = builder.with_inner_size(egui::vec2(width, 280.0));
        }
        if let Some(width) = self.min_width {
            builder = builder.with_min_inner_size(egui::vec2(width, 120.0));
        }
        // Les fenêtres natives ignorent `Window::anchor` (appliqué plus bas au
        // seul repli intégré). Poser la fenêtre ici respecte l'ancrage voulu.
        if let Some((align, offset)) = self.anchor {
            let screen = ctx.screen_rect();
            let size = self.default_size.unwrap_or_else(|| {
                Vec2::new(self.default_width.unwrap_or(300.0), 280.0)
            });
            let x = align.0[0].align_size_within_range(size.x, screen.x_range()).min + offset.x;
            let y = align.0[1].align_size_within_range(size.y, screen.y_range()).min + offset.y;
            builder = builder.with_position([x, y]);
        }

        let anchor = self.anchor;
        let frame_modal = self.modal_frame;
        let max_width = self.max_width;
        let mut content = Some(content);

        // Vrai NSWindow quand le backend le permet (feux de circulation natifs,
        // fenêtre indépendante) ; repli sur une fenêtre egui intégrée sinon.
        ctx.show_viewport_immediate(viewport_id, builder, |ctx, class| {
            let draw = content.take()?;
            if class == egui::ViewportClass::Embedded {
                let mut window = egui::Window::new(self.title.clone())
                    .collapsible(false)
                    .resizable(self.resizable);
                if let Some(open) = open.as_deref_mut() {
                    window = window.open(open);
                }
                if let Some((a, offset)) = anchor {
                    window = window.anchor(a, offset);
                }
                if frame_modal {
                    window = window.frame(modal_frame(ctx));
                }
                let mut out = None;
                window.show(ctx, |ui| {
                    if max_width > 0.0 {
                        ui.set_max_width(max_width);
                    }
                    out = Some(Self::surface(ui, draw));
                });
                out
            } else {
                if ctx.input(|i| i.viewport().close_requested()) {
                    if let Some(open) = open.as_deref_mut() {
                        *open = false;
                    }
                }
                let mut out = None;
                egui::CentralPanel::default()
                    .frame(
                        egui::Frame::none()
                            .fill(youpdf_shared::pal().chrome_bg)
                            .inner_margin(egui::Margin::symmetric(14.0, 12.0)),
                    )
                    .show(ctx, |ui| {
                        if max_width > 0.0 {
                            ui.set_max_width(max_width);
                        }
                        out = Some(Self::surface(ui, draw));
                    });
                out
            }
        })
    }
}
