//! Tampons personnalisés et créateur de signatures.
//!
//! Deux fenêtres qui complètent la palette d'annotations :
//!
//! - le **tampon personnalisé** pose soit une image (PNG/JPEG), soit un
//!   texte encadré, avec ou sans date ;
//! - le **créateur de signature** fabrique une image à partir d'un tracé à
//!   la souris, d'un nom saisi dans une police scripte, ou d'un fichier
//!   importé, et la conserve dans une liste de dix signatures.
//!
//! L'image produite par le créateur est ensuite posée par
//! [`YouPdfApp::insert_image_at`], comme n'importe quelle image : elle
//! n'ajoute donc pas d'annotation, ce qui est le comportement attendu d'une
//! signature « à l'encre ».

use super::*;
use youpdf_shared::t;
use ab_glyph::ScaleFont;

/// Nombre de signatures conservées d'un côté et de l'autre.
pub const STORED_SIGNATURE_LIMIT: usize = 10;

/// Résolution de l'image de signature, en points par pouce.
///
/// Une signature est agrandie à la taille d'un paraphe sur la page : la
/// rasteriser à 300 dpi garde le trait net une fois réduit.
const SIGNATURE_DPI: f32 = 300.0;

/// Taille du canevas de dessin, en pixels CSS.
const DRAW_CANVAS: f32 = 300.0;

/// Largeur du rendu d'une signature tapée, en points.
const TYPED_WIDTH: f32 = 420.0;

/// Signature conservée : son PNG et le nom qui l'identifie dans la liste.
#[derive(Clone)]
pub(crate) struct StoredSignature {
    pub(crate) name: String,
    pub(crate) png: Vec<u8>,
}

/// État des deux fenêtres.
pub(crate) struct MarksUi {
    /// Fenêtre « Tampon personnalisé » ouvert.
    pub(crate) stamp_open: bool,
    /// Sous-vue du tampon : 0 = texte, 1 = image.
    pub(crate) stamp_tab: u8,
    /// Texte du tampon.
    pub(crate) stamp_text: String,
    /// Encadrer le texte.
    pub(crate) stamp_border: bool,
    /// Ajouter la date sous le texte.
    pub(crate) stamp_date: bool,
    /// Image du tampon, déjà décodée en RGB.
    pub(crate) stamp_image: Option<(Vec<u8>, u32, u32)>,
    /// Nom du fichier image choisi, pour l'afficher.
    pub(crate) stamp_image_name: String,

    /// Fenêtre « Créer une signature » ouvert.
    pub(crate) signature_open: bool,
    /// Sous-vue : 0 = dessiner, 1 = taper, 2 = image.
    pub(crate) signature_tab: u8,
    /// Points du tracé, en pixels du canevas.
    pub(crate) draw_points: Vec<Vec<[f32; 2]>>,
    /// Tracé en cours.
    pub(crate) draw_current: Vec<[f32; 2]>,
    /// Nom tapé.
    pub(crate) typed_name: String,
    /// Signature importée.
    pub(crate) imported: Option<(Vec<u8>, u32, u32)>,
    /// Point de pose demandé par un clic sur la page (page, point PDF) :
    /// la signature suivante atterrit là plutôt qu'au centre.
    pub(crate) signature_at: Option<(usize, [f32; 2])>,
    /// Signatures enregistrées, la plus récente en tête.
    pub(crate) stored: Vec<StoredSignature>,
    /// Message d'erreur de la dernière tentative.
    pub(crate) error: Option<String>,
}

impl Default for MarksUi {
    fn default() -> Self {
        Self {
            stamp_open: false,
            stamp_tab: 0,
            stamp_text: "APPROVED".to_string(),
            stamp_border: true,
            stamp_date: true,
            stamp_image: None,
            stamp_image_name: String::new(),
            signature_open: false,
            signature_tab: 0,
            draw_points: Vec::new(),
            draw_current: Vec::new(),
            typed_name: String::new(),
            imported: None,
            signature_at: None,
            stored: Vec::new(),
            error: None,
        }
    }
}

impl YouPdfApp {
    /// Fenêtre « Tampon personnalisé ».
    pub(super) fn stamp_window(&mut self, ctx: &egui::Context) {
        if !self.marks.stamp_open {
            return;
        }
        let mut open = true;
        let mut place_text = false;
        let mut place_image = false;
        self.show_modal(
            ctx,
            "modal-custom-stamp",
            "Custom stamp",
            420.0,
            &mut open,
            |app, ui| {
                ui.horizontal(|ui| {
                    for (value, label) in [(0u8, "Text"), (1, "Image")] {
                        if ui
                            .selectable_label(app.marks.stamp_tab == value, label)
                            .clicked()
                        {
                            app.marks.stamp_tab = value;
                        }
                    }
                });
                ui.separator();
                if app.marks.stamp_tab == 0 {
                    ui.horizontal(|ui| {
                        ui.label(t("Text"));
                        ui.add(
                            TextEdit::singleline(&mut app.marks.stamp_text).desired_width(260.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut app.marks.stamp_border, t("Border"));
                        ui.checkbox(&mut app.marks.stamp_date, t("Add the date"));
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(t("The stamp is placed at the centre of the current page."))
                            .size(10.5)
                            .color(pal().muted),
                    );
                    ui.add_space(6.0);
                    place_text = ui
                        .add_enabled(
                            !app.marks.stamp_text.trim().is_empty(),
                            egui::Button::new("Place on page"),
                        )
                        .clicked();
                } else {
                    ui.horizontal(|ui| {
                        if ui.button(t("Choose image…")).clicked() {
                            app.choose_stamp_image();
                        }
                        if !app.marks.stamp_image_name.is_empty() {
                            ui.label(
                                RichText::new(&app.marks.stamp_image_name)
                                    .size(10.5)
                                    .color(pal().muted),
                            );
                        }
                    });
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(t("PNG or JPEG, drawn at a quarter of the page width."))
                            .size(10.5)
                            .color(pal().muted),
                    );
                    ui.add_space(6.0);
                    place_image = ui
                        .add_enabled(
                            app.marks.stamp_image.is_some(),
                            egui::Button::new("Place on page"),
                        )
                        .clicked();
                }
                if let Some(error) = app.marks.error.clone() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(11.0).color(MARKS_RED));
                }
            },
        );
        if !open {
            self.marks.stamp_open = false;
        }
        if place_text {
            self.place_custom_text_stamp(ctx);
        } else if place_image {
            self.place_custom_image_stamp(ctx);
        }
    }

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
                    ui.add_space(4.0);
                    ui.label(RichText::new(error).size(11.0).color(MARKS_RED));
                }
                ui.separator();
                app.stored_signatures_list(ui);
            },
        );
        if !open {
            self.marks.signature_open = false;
        }
        if let Some(png) = created {
            self.store_signature(png);
        }
    }

    /// Onglet « Dessiner » : un canevas et un bouton de validation.
    fn signature_draw_tab(&mut self, ui: &mut egui::Ui) -> Option<Vec<u8>> {
        let (rect, response) = ui.allocate_exact_size(
            vec2(DRAW_CANVAS, DRAW_CANVAS * 0.45),
            Sense::drag(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, Rounding::same(4.0), pal().widget_bg);
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
        ui.add_space(4.0);
        let preview = self.marks.typed_name.trim().to_string();
        let (rect, _) = ui.allocate_exact_size(vec2(TYPED_WIDTH, 90.0), Sense::hover());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, Rounding::same(4.0), pal().widget_bg);
        if preview.is_empty() {
            painter.text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Your name appears here",
                FontId::proportional(13.0),
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
                        .size(10.5)
                        .color(pal().muted),
                );
            }
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "A photo or a scan works: the light background is made transparent.",
            )
            .size(10.5)
            .color(pal().muted),
        );
        ui.add_space(6.0);
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
            .size(11.0)
            .color(pal().muted),
        );
        if self.marks.stored.is_empty() {
            ui.label(
                RichText::new(t("Nothing saved yet."))
                    .size(10.5)
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
            self.place_signature_png(&png);
        }
    }

    /// Décode l'image d'un tampon personnalisé.
    fn choose_stamp_image(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        match youpdf_core::edit::load_image_rgb(&path) {
            Ok((rgb, width, height)) => {
                self.marks.stamp_image = Some((rgb, width, height));
                self.marks.stamp_image_name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("image")
                    .to_string();
                self.marks.error = None;
            }
            Err(err) => self.marks.error = Some(format!("Image failed: {err}")),
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

    /// Pose le tampon texte au centre de la page courante.
    fn place_custom_text_stamp(&mut self, ctx: &egui::Context) {
        let page = self.tabs.get(self.active).map_or(1, |tab| tab.page);
        let author = self.comment_author.clone().unwrap_or_default();
        let date = if self.marks.stamp_date {
            chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
        } else {
            String::new()
        };
        let mut lines = vec![self.marks.stamp_text.trim().to_string()];
        if !date.is_empty() {
            lines.push(date);
        }
        let (page_w, page_h) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.doc.page_size(page as u32))
            .unwrap_or((595.0, 842.0));
        let widest = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as f32;
        let width = (widest * YOUPDF_STAMP_FONT * 0.5 + 24.0).clamp(90.0, page_w * 0.8);
        let height = (lines.len() as f32 * YOUPDF_STAMP_FONT * 1.3 + 16.0).max(40.0);
        let rect = centered_rect(page_w, page_h, width, height);
        let mut annot = Annotation::new(page, AnnotationKind::Stamp, rect);
        annot.content = lines.join("\n");
        annot.author = if author.trim().is_empty() {
            "YouPDF".to_string()
        } else {
            author.trim().to_string()
        };
        annot.style.font_size = YOUPDF_STAMP_FONT;
        annot.style.color = self.annot_color;
        annot.style.line_width = if self.marks.stamp_border { 1.5 } else { 0.5 };
        self.push_annot(annot);
        self.marks.stamp_open = false;
        self.toast(ctx, "Stamp placed".to_string());
    }

    /// Pose l'image du tampon personnalisé au centre de la page.
    fn place_custom_image_stamp(&mut self, ctx: &egui::Context) {
        let Some((rgb, width, height)) = self.marks.stamp_image.clone() else {
            return;
        };
        let page = self.tabs.get(self.active).map_or(1, |tab| tab.page);
        let (page_w, page_h) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.doc.page_size(page as u32))
            .unwrap_or((595.0, 842.0));
        let width_pt = page_w * 0.25;
        let height_pt = width_pt * height as f32 / width.max(1) as f32;
        let rect = centered_rect(page_w, page_h, width_pt, height_pt);
        let inserted = match self.tabs.get_mut(self.active) {
            Some(tab) => tab
                .doc
                .insert_image_bitmap(page, rect, &rgb, width, height),
            None => return,
        };
        match inserted {
            Ok(()) => {
                self.mark_document_changed();
                self.marks.stamp_open = false;
                self.toast(ctx, "Stamp placed".to_string());
            }
            Err(err) => self.marks.error = Some(format!("Image failed: {err}")),
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

/// Couleur d'erreur des deux fenêtres.
const MARKS_RED: Color32 = Color32::from_rgb(196, 62, 62);

/// Taille de police du tampon texte personnalisé.
const YOUPDF_STAMP_FONT: f32 = 18.0;

/// Rectangle centré de la taille demandée, en coordonnées PDF.
fn centered_rect(page_w: f32, page_h: f32, width: f32, height: f32) -> [f32; 4] {
    let x = (page_w - width) / 2.0;
    let y = (page_h - height) / 2.0;
    [x, y, x + width, y + height]
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

/// Canevas RGB minimal : de quoi rasteriser un tracé puis un contour de
/// glyphe, sans tirer une bibliothèque de dessin supplémentaire.
struct Canvas {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Canvas {
    /// Canevas blanc opaque.
    fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![255; (width as usize) * (height as usize) * 3],
        }
    }

    fn rgb(&self) -> &[u8] {
        &self.pixels
    }

    fn set(&mut self, x: u32, y: u32, color: [u8; 3], coverage: f32) {
        if x >= self.width || y >= self.height {
            return;
        }
        let coverage = coverage.clamp(0.0, 1.0);
        let index = ((y as usize) * (self.width as usize) + x as usize) * 3;
        for (channel, over) in color.iter().enumerate() {
            let under = self.pixels[index + channel] as f32;
            let over = *over as f32;
            self.pixels[index + channel] = (under + (over - under) * coverage).round() as u8;
        }
    }

    /// Peint un disque, avec un bord adouci d'un pixel.
    fn disc(&mut self, cx: f32, cy: f32, radius: f32, color: [u8; 3]) {
        let left = (cx - radius - 1.0).floor().max(0.0) as u32;
        let right = (cx + radius + 1.0).ceil().min(self.width as f32) as u32;
        let top = (cy - radius - 1.0).floor().max(0.0) as u32;
        let bottom = (cy + radius + 1.0).ceil().min(self.height as f32) as u32;
        for y in top..bottom {
            for x in left..right {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let distance = (dx * dx + dy * dy).sqrt();
                let coverage = (radius + 0.5 - distance).clamp(0.0, 1.0);
                self.set(x, y, color, coverage);
            }
        }
    }

    /// Peint une ligne d'épaisseur donnée en chaînant des disques.
    fn stroke(&mut self, points: &[[f32; 2]], width: f32, color: [u8; 3]) {
        let radius = (width / 2.0).max(0.5);
        if points.len() == 1 {
            self.disc(points[0][0], points[0][1], radius, color);
            return;
        }
        // Un pas d'un demi-rayon donne un trait continu sans trou.
        let step = (radius * 0.5).max(0.7);
        for pair in points.windows(2) {
            let (ax, ay) = (pair[0][0], pair[0][1]);
            let (bx, by) = (pair[1][0], pair[1][1]);
            let length = ((bx - ax).powi(2) + (by - ay).powi(2)).sqrt();
            let count = ((length / step).ceil() as u32).max(1);
            for index in 0..=count {
                let t = index as f32 / count as f32;
                self.disc(ax + (bx - ax) * t, ay + (by - ay) * t, radius, color);
            }
        }
    }

    /// Remplit un contour de glyphe posé à `(x, y)`.
    fn fill_outline(&mut self, outline: &ab_glyph::OutlinedGlyph, color: [u8; 3]) {
        let bounds = outline.px_bounds();
        let left = bounds.min.x.floor() as i32;
        let top = bounds.min.y.floor() as i32;
        let width = bounds.width().ceil() as u32 + 1;
        let height = bounds.height().ceil() as u32 + 1;
        let mut coverage = vec![0.0f32; (width * height) as usize];
        outline.draw(|gx, gy, value| {
            let index = gy as usize * width as usize + gx as usize;
            if index < coverage.len() {
                coverage[index] = value;
            }
        });
        for gy in 0..height {
            for gx in 0..width {
                let alpha = coverage[(gy * width + gx) as usize];
                if alpha <= 0.0 {
                    continue;
                }
                let px = left + gx as i32;
                let py = top + gy as i32;
                if px < 0 || py < 0 {
                    continue;
                }
                self.set(px as u32, py as u32, color, alpha);
            }
        }
    }

    /// Recadre sur les pixels non blancs, avec une marge d'un pixel.
    fn trim(&self) -> Canvas {
        let mut min_x = self.width;
        let mut min_y = self.height;
        let mut max_x = 0u32;
        let mut max_y = 0u32;
        let mut found = false;
        for y in 0..self.height {
            for x in 0..self.width {
                let index = ((y as usize) * (self.width as usize) + x as usize) * 3;
                let ink = self.pixels[index..index + 3].iter().any(|c| *c < 240);
                if ink {
                    found = true;
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }
        if !found {
            return Canvas::new(self.width, self.height);
        }
        let (left, top) = (min_x.saturating_sub(1), min_y.saturating_sub(1));
        let right = (max_x + 2).min(self.width);
        let bottom = (max_y + 2).min(self.height);
        let (w, h) = (right - left, bottom - top);
        let mut out = Canvas::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let from = (((y + top) as usize) * (self.width as usize) + (x + left) as usize) * 3;
                let to = ((y as usize) * (w as usize) + x as usize) * 3;
                out.pixels[to..to + 3].copy_from_slice(&self.pixels[from..from + 3]);
            }
        }
        out
    }
}

/// Encode un canevas RGB en PNG, éventuellement avec l'alpha dérivé du
/// blanc (une photo de signature posée sur du papier devient transparente).
fn encode_png(
    rgb: &[u8],
    width: u32,
    height: u32,
    key_out_white: bool,
) -> std::result::Result<Vec<u8>, String> {
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for pixel in rgb.chunks_exact(3) {
        let [r, g, b] = [pixel[0], pixel[1], pixel[2]];
        let alpha = if key_out_white {
            // Le papier d'un scan est rarement blanc pur : transparent dès
            // 200 sur les trois canaux, opaque à 150 et moins.
            let darkest = r.min(g).min(b) as u32;
            (200u32.saturating_sub(darkest) * 255 / 50).min(255) as u8
        } else {
            255
        };
        rgba.extend_from_slice(&[r, g, b, alpha]);
    }
    let image = image::RgbaImage::from_raw(width, height, rgba)
        .ok_or_else(|| "Signature image is malformed".to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|err| format!("PNG encoding failed: {err}"))?;
    Ok(out.into_inner())
}
