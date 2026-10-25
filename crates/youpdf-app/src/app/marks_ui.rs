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
use youpdf_shared::{CAPTION, LABEL, t, SPACE_M, SPACE_S};

/// Nombre de signatures conservées d'un côté et de l'autre.
pub const STORED_SIGNATURE_LIMIT: usize = 10;




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
                    ui.add_space(SPACE_M);
                    ui.label(
                        RichText::new(t("The stamp is placed at the centre of the current page."))
                            .size(CAPTION)
                            .color(pal().muted),
                    );
                    ui.add_space(SPACE_M);
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
                                    .size(CAPTION)
                                    .color(pal().muted),
                            );
                        }
                    });
                    ui.add_space(SPACE_M);
                    ui.label(
                        RichText::new(t("PNG or JPEG, drawn at a quarter of the page width."))
                            .size(CAPTION)
                            .color(pal().muted),
                    );
                    ui.add_space(SPACE_M);
                    place_image = ui
                        .add_enabled(
                            app.marks.stamp_image.is_some(),
                            egui::Button::new("Place on page"),
                        )
                        .clicked();
                }
                if let Some(error) = app.marks.error.clone() {
                    ui.add_space(SPACE_S);
                    ui.label(RichText::new(error).size(LABEL).color(MARKS_RED));
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

}

/// Couleur d'erreur des deux fenêtres.
pub(super) const MARKS_RED: Color32 = Color32::from_rgb(196, 62, 62);

/// Taille de police du tampon texte personnalisé.
const YOUPDF_STAMP_FONT: f32 = 18.0;

/// Rectangle centré de la taille demandée, en coordonnées PDF.
pub(super) fn centered_rect(page_w: f32, page_h: f32, width: f32, height: f32) -> [f32; 4] {
    let x = (page_w - width) / 2.0;
    let y = (page_h - height) / 2.0;
    [x, y, x + width, y + height]
}





/// Canevas RGB minimal : de quoi rasteriser un tracé puis un contour de
/// glyphe, sans tirer une bibliothèque de dessin supplémentaire.
pub(super) struct Canvas {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) pixels: Vec<u8>,
}

impl Canvas {
    /// Canevas blanc opaque.
    pub(super) fn new(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            pixels: vec![255; (width as usize) * (height as usize) * 3],
        }
    }

    pub(super) fn rgb(&self) -> &[u8] {
        &self.pixels
    }

    pub(super) fn set(&mut self, x: u32, y: u32, color: [u8; 3], coverage: f32) {
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
    pub(super) fn disc(&mut self, cx: f32, cy: f32, radius: f32, color: [u8; 3]) {
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
    pub(super) fn stroke(&mut self, points: &[[f32; 2]], width: f32, color: [u8; 3]) {
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
    pub(super) fn fill_outline(&mut self, outline: &ab_glyph::OutlinedGlyph, color: [u8; 3]) {
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
    pub(super) fn trim(&self) -> Canvas {
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
pub(super) fn encode_png(
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
