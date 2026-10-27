//! Créateur de signatures : tracé à la souris, saisie dans une police
//! scripte, ou import d'un fichier.
//!
//! Chaque voie produit un PNG. Le raster (trait, disque, découpe, encodage)
//! est partagé avec les tampons dans `marks_ui`.

use super::*;
use super::marks_ui::{centered_rect, encode_png, Canvas, MARKS_RED};
use super::marks_ui::{StoredSignature, STORED_SIGNATURE_LIMIT};
use ab_glyph::ScaleFont;
use youpdf_shared::{CAPTION, LABEL, TITLE, t, RADIUS_S, SPACE_M, SPACE_S};

/// Résolution de l'image de signature, en points par pouce.
const SIGNATURE_DPI: f32 = 300.0;

/// Taille du canevas de dessin, en pixels CSS.
const DRAW_CANVAS: f32 = 300.0;

/// Largeur du rendu d'une signature tapée, en points.
const TYPED_WIDTH: f32 = 420.0;

impl YouPdfApp {
    /// Fenêtre « Créer une signature ».
    pub(super) fn signature_window(&mut self, ctx: &egui::Context) {
        if !self.marks.signature_open {
            return;
        }
        let mut open = true;
        let mut created: Option<Vec<u8>> = None;
        self.show_modal(
            ctx,
            "modal-create-signature",
            "Create a signature",
            460.0,
            &mut open,
            |app, ui| {
                ui.horizontal(|ui| {
                    for (value, label) in [(0u8, "Draw"), (1, "Type"), (2, "Image")] {
                        if ui
                            .selectable_label(app.marks.signature_tab == value, label)
                            .clicked()
                        {
                            app.marks.signature_tab = value;
                        }
                    }
                });
                ui.separator();
                match app.marks.signature_tab {
                    0 => created = app.signature_draw_tab(ui),
                    1 => created = app.signature_type_tab(ui),
                    _ => created = app.signature_image_tab(ui),
                }
                if let Some(error) = app.marks.error.clone() {
                    ui.add_space(SPACE_S);
                    ui.label(RichText::new(error).size(LABEL).color(MARKS_RED));
                }
                ui.separator();
                app.stored_signatures_list(ui);
            },
        );
        if !open {
            self.marks.signature_open = false;
        }
        if let Some(png) = created {
            if self.form_signature_target.is_some() {
                let target = self.form_signature_target.take();
                if let Some(idx) = target {
                    self.fill_signature_form_field(idx, &png, ctx);
                }
            } else {
                self.store_signature(png);
            }
        }
    }
    /// Onglet « Dessiner » : un canevas et un bouton de validation.
    fn signature_draw_tab(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        let (rect, response) = ui.allocate_exact_size(
            vec2(DRAW_CANVAS, DRAW_CANVAS * 0.45),
            Sense::drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, Rounding::same(RADIUS_S), pal().widget_bg);
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0_f32, pal().separator),
        );
        for stroke in self.marks.draw_points.iter() {
            let pts: Vec<egui::Pos2> = stroke
                .iter()
                .map(|p| pos2(rect.left() + p[0], rect.top() + p[1]))
                .collect();
            if pts.len() >= 2 {
                painter.add(Shape::line(pts, Stroke::new(2.0_f32, pal().icon_bright)));
            }
        }
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let local = [p.x - rect.left(), p.y - rect.top()];
                let far = self
                    .marks
                    .draw_current
                    .last()
                    .map_or(true, |last| (local[0] - last[0]).hypot(local[1] - last[1]) >= 1.5);
                if far {
                    self.marks.draw_current.push(local);
                    let pts: Vec<egui::Pos2> = self
                        .marks
                        .draw_current
                        .iter()
                        .map(|p| pos2(rect.left() + p[0], rect.top() + p[1]))
                        .collect();
                    if pts.len() >= 2 {
                        painter.add(Shape::line(pts, Stroke::new(2.0_f32, pal().icon_bright)));
                    }
                }
            }
        }
        if response.drag_stopped() && self.marks.draw_current.len() >= 2 {
            let stroke = std::mem::take(&mut self.marks.draw_current);
            self.marks.draw_points.push(stroke);
        }
        ui.horizontal(|ui| {
            if ui
                .add_enabled(
                    !self.marks.draw_points.is_empty(),
                    egui::Button::new("Clear"),
                )
                .clicked()
            {
                self.marks.draw_points.clear();
                self.marks.draw_current.clear();
            }
            ui.add_enabled_ui(!self.marks.draw_points.is_empty(), |ui| {
                if ui.button(t("Use this signature")).clicked() {
                    match self.render_drawn_signature() {
                        Ok(png) => {
                            self.marks.error = None;
                            return Some(png);
                        }
                        Err(err) => self.marks.error = Some(err),
                    }
                }
                None
            })
            .inner
        })
        .inner
    }
    /// Onglet « Taper » : un nom rendu en scripte.
    fn signature_type_tab(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        ui.horizontal(|ui| {
            ui.label(t("Name"));
            ui.add(TextEdit::singleline(&mut self.marks.typed_name).desired_width(280.0));
        });
        ui.add_space(SPACE_S);
        let preview = self.marks.typed_name.trim().to_string();
        let (rect, _) = ui.allocate_exact_size(vec2(TYPED_WIDTH, 90.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, Rounding::same(RADIUS_S), pal().widget_bg);
        if preview.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Your name appears here",
                FontId::proportional(TITLE),
                pal().muted,
            );
        } else {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                &preview,
                FontId::new(34.0, signature_font()),
                pal().icon_bright,
            );
        }
        ui.add_enabled_ui(!preview.is_empty(), |ui| {
            if ui.button(t("Use this signature")).clicked() {
                match self.render_typed_signature() {
                    Ok(png) => {
                        self.marks.error = None;
                        return Some(png);
                    }
                    Err(err) => self.marks.error = Some(err),
                }
            }
            None
        })
        .inner
    }
    /// Onglet « Image » : import d'une signature scannée ou photographiée.
    fn signature_image_tab(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        ui.horizontal(|ui| {
            if ui.button(t("Choose image…")).clicked() {
                self.choose_signature_image();
            }
            if let Some((_, w, h)) = self.marks.imported {
                ui.label(
                    RichText::new(format!("{w} × {h} px"))
                        .size(CAPTION)
                        .color(pal().muted),
                );
            }
        });
        ui.add_space(SPACE_S);
        ui.label(
            RichText::new(
                "A photo or a scan works: the light background is made transparent.",
            )
            .size(CAPTION)
            .color(pal().muted),
        );
        ui.add_space(SPACE_M);
        ui.add_enabled_ui(self.marks.imported.is_some(), |ui| {
            if ui.button(t("Use this signature")).clicked() {
                if let Some((rgb, width, height)) = self.marks.imported.clone() {
                    match encode_png(&rgb, width, height, true) {
                        Ok(png) => {
                            self.marks.error = None;
                            return Some(png);
                        }
                        Err(err) => self.marks.error = Some(err),
                    }
                }
            }
            None
        })
        .inner
    }
    /// Liste des signatures conservées : aperçu, insertion, suppression.
    fn stored_signatures_list(&mut self, ui: &mut egui::Ui) {
        ui.label(
            RichText::new(format!(
                "Saved signatures ({}/{STORED_SIGNATURE_LIMIT})",
                self.marks.stored.len()
            ))
            .size(LABEL)
            .color(pal().muted),
        );
        if self.marks.stored.is_empty() {
            ui.label(
                RichText::new(t("Nothing saved yet."))
                    .size(CAPTION)
                    .color(pal().muted),
            );
            return;
        }
        let mut use_index = None;
        let mut remove_index = None;
        for (index, stored) in self.marks.stored.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui.button(&stored.name).clicked() {
                    use_index = Some(index);
                }
                if ui.small_button("Remove").clicked() {
                    remove_index = Some(index);
                }
            });
        }
        if let Some(index) = remove_index {
            self.marks.stored.remove(index);
        }
        if let Some(index) = use_index {
            let png = self.marks.stored[index].png.clone();
            if self.form_signature_target.is_some() {
                let target = self.form_signature_target.take();
                if let Some(idx) = target {
                    self.fill_signature_form_field(idx, &png, ui.ctx());
                }
            } else {
                self.place_signature_png(&png);
            }
        }
    }
    /// Décode l'image importée d'une signature.
    fn choose_signature_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        match youpdf_core::edit::load_image_rgb(&path) {
            Ok((rgb, width, height)) => {
                self.marks.imported = Some((rgb, width, height));
                self.marks.error = None;
            }
            Err(err) => self.marks.error = Some(format!("Image failed: {err}")),
        }
    }
    /// Ajoute une signature à la liste, la plus récente en tête.
    fn store_signature(&mut self, png: Vec<u8>) {
        let name = format!("Signature {}", self.marks.stored.len() + 1);
        self.marks.stored.insert(0, StoredSignature { name, png });
        self.marks.stored.truncate(STORED_SIGNATURE_LIMIT);
        self.place_signature_png(&self.marks.stored[0].png.clone());
    }
    /// Pose une signature au point demandé, ou au centre de la page.
    /// Remplit un champ de signature de formulaire avec l'image donnée.
    fn fill_signature_form_field(&mut self, idx: usize, png: &[u8], ctx: &egui::Context) {
        let field = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.forms.get(idx).cloned());
        let Some(field) = field else {
            return;
        };
        let page = field.page.unwrap_or(1);
        let rect = field.rect;
        let rgb = match image::load_from_memory(png) {
            Ok(image) => image.to_rgb8(),
            Err(err) => {
                self.marks.error = Some(format!("Signature unreadable: {err}"));
                return;
            }
        };
        let (width, height) = rgb.dimensions();
        let rw = (rect[2] - rect[0]).max(1.0);
        let rh = (rect[3] - rect[1]).max(1.0);
        let scale = (rw / width as f32).min(rh / height as f32);
        let wpt = width as f32 * scale;
        let hpt = height as f32 * scale;
        let x = rect[0] + (rw - wpt) / 2.0;
        let y = rect[1] + (rh - hpt) / 2.0;
        let target_rect = [x, y, x + wpt, y + hpt];
        let filled = if let Some(tab) = self.tabs.get_mut(self.active) {
            let inserted = tab
                .doc
                .insert_image_bitmap(page, target_rect, rgb.as_raw(), width, height);
            let _ = tab.doc.fill_form_field(&field, "Signed");
            tab.forms = tab.doc.read_form_fields();
            inserted.is_ok()
        } else {
            false
        };
        if filled {
            self.mark_document_changed();
            self.marks.signature_open = false;
            self.marks.error = None;
            self.toast(ctx, "Signature field filled".to_string());
        }
    }

    fn place_signature_png(&mut self, png: &[u8]) {
        let (page, at) = match self.marks.signature_at.take() {
            Some((page, at)) => (page, Some(at)),
            None => (
                self.tabs.get(self.active).map_or(1, |tab| tab.page),
                None,
            ),
        };
        let (page_w, page_h) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.doc.page_size(page as u32))
            .unwrap_or((595.0, 842.0));
        let rgb = match image::load_from_memory(png) {
            Ok(image) => image.to_rgb8(),
            Err(err) => {
                self.marks.error = Some(format!("Signature unreadable: {err}"));
                return;
            }
        };
        let (width, height) = rgb.dimensions();
        let width_pt = page_w * 0.3;
        let height_pt = width_pt * height as f32 / width.max(1) as f32;
        let rect = match at {
            Some(point) => [
                point[0] - width_pt / 2.0,
                point[1] - height_pt / 2.0,
                point[0] + width_pt / 2.0,
                point[1] + height_pt / 2.0,
            ],
            None => centered_rect(page_w, page_h, width_pt, height_pt),
        };
        let inserted = match self.tabs.get_mut(self.active) {
            Some(tab) => tab
                .doc
                .insert_image_bitmap(page, rect, rgb.as_raw(), width, height),
            None => return,
        };
        match inserted {
            Ok(()) => {
                self.mark_document_changed();
                self.marks.signature_open = false;
                self.marks.error = None;
            }
            Err(err) => self.marks.error = Some(format!("Signature failed: {err}")),
        }
    }
    /// Rasterise le tracé du créateur de signatures.
    fn render_drawn_signature(&self) -> std::result::Result<Vec<u8>, String> {
        let (rect_w, rect_h) = (DRAW_CANVAS, DRAW_CANVAS * 0.45);
        let (width, height) = (
            (rect_w * SIGNATURE_DPI / 72.0) as u32,
            (rect_h * SIGNATURE_DPI / 72.0) as u32,
        );
        let scale = width as f32 / rect_w;
        let mut canvas = Canvas::new(width, height);
        // Les traits sont épaissis à la même proportion que l'agrandissement.
        let stroke = (2.0 * scale).max(2.0);
        let mut points: Vec<Vec<[f32; 2]>> = self.marks.draw_points.clone();
        if self.marks.draw_current.len() >= 2 {
            points.push(self.marks.draw_current.clone());
        }
        for line in &points {
            let smoothed = smooth_polyline(line, 4);
            let scaled: Vec<[f32; 2]> = smoothed
                .iter()
                .map(|p| [p[0] * scale, p[1] * scale])
                .collect();
            canvas.stroke(&scaled, stroke, [24, 24, 28]);
        }
        encode_png(canvas.rgb(), width, height, false)
    }
    /// Rasterise le nom tapé dans la police de signature.
    fn render_typed_signature(&self) -> std::result::Result<Vec<u8>, String> {
        let name = self.marks.typed_name.trim();
        if name.is_empty() {
            return Err("Type a name first".to_string());
        }
        let (width, height) = (
            (TYPED_WIDTH * SIGNATURE_DPI / 72.0) as u32,
            (90.0 * SIGNATURE_DPI / 72.0) as u32,
        );
        let font = raster_font(signature_font_path()).ok_or_else(|| {
            "No script font found on this system".to_string()
        })?;
        let mut canvas = Canvas::new(width, height);
        let size = height as f32 * 0.62;
        let scale = ab_glyph::PxScale::from(size);
        let scaled = ab_glyph::Font::as_scaled(&font, scale);
        let mut pen_x = 24.0f32;
        let baseline = height as f32 * 0.72;
        for ch in name.chars() {
            let glyph_id = ab_glyph::Font::glyph_id(&font, ch);
            let glyph = ab_glyph::Glyph {
                id: glyph_id,
                scale,
                position: ab_glyph::point(pen_x, baseline),
            };
            if let Some(outlined) = scaled.outline_glyph(glyph) {
                canvas.fill_outline(&outlined, [22, 22, 30]);
                pen_x = outlined.px_bounds().max.x;
            } else {
                pen_x += ab_glyph::Font::h_advance_unscaled(&font, glyph_id) * size
                    / unscaled_em(&font);
            }
        }
        if pen_x <= 24.0 {
            return Err("No glyph could be drawn for this name".to_string());
        }
        // Recadre sur l'encre : une signature tapée ne garde pas les marges.
        let cropped = canvas.trim();
        encode_png(cropped.rgb(), cropped.width, cropped.height, false)
    }
}

/// Police du rendu « Taper » : un scripte installé sur la machine.
///
/// Aucune police n'est embarquée dans le binaire (licences) ; on prend la
/// première disponible et on échoue proprement si aucune ne l'est.
fn signature_font_path() -> &'static str {
    const CANDIDATES: &[&str] = &[
        "/System/Library/Fonts/Supplemental/SnellRoundhand.ttc",
        "/System/Library/Fonts/Supplemental/SavoyeLetPlain.ttf",
        "/Library/Fonts/Brush Script.ttf",
        "/System/Library/Fonts/Supplemental/Bradley Hand Bold.ttf",
        "/System/Library/Fonts/Supplemental/Zapfino.ttf",
        "/System/Library/Fonts/Helvetica.ttc",
    ];
    CANDIDATES
        .iter()
        .copied()
        .find(|path| std::path::Path::new(path).exists())
        .unwrap_or("/System/Library/Fonts/Helvetica.ttc")
}
/// Famille de repli pour l'aperçu d'une signature tapée.
fn signature_font() -> egui::FontFamily {
    egui::FontFamily::Proportional
}
/// Charge une police depuis le disque, premier visage de la collection.
fn raster_font(path: &str) -> Option<ab_glyph::FontVec> {
    let bytes = std::fs::read(path).ok()?;
    ab_glyph::FontVec::try_from_vec_and_index(bytes, 0).ok()
}
/// Cadratins par unité d'avance : 2048 pour une TrueType, 1000 pour une CFF.
fn unscaled_em(font: &ab_glyph::FontVec) -> f32 {
    ab_glyph::Font::units_per_em(font).unwrap_or(1000.0).max(1.0)
}
