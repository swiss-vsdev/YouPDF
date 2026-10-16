//! Filigranes, en-têtes et pieds de page, numéros de page.
//!
//! ## Approche retenue
//!
//! Comme le remplacement de texte de [`crate::edit`], tout est écrit
//! **dans le flux de contenu** de la page : le filigrane s'imprime, se
//! convertit en image, et reste visible dans n'importe quel lecteur. Une
//! annotation ne donnerait qu'un objet décrit au `/Annots`, que certains
//! lecteurs n'affichent pas.
//!
//! Chaque page décorée reçoit un flux de contenu supplémentaire, posé à
//! la fin de `/Contents` et marqué par la clé `/YouPdfDecoration` : c'est
//! ce marquage qui permet de tout retirer ensuite avec
//! [`Document::clear_watermarks`].
//!
//! ## Ressources posées par ce module
//!
//! | Nom              | Type        | Rôle                        |
//! |------------------|-------------|-----------------------------|
//! | `/YouPdfF1`      | `/Font`     | Helvetica /WinAnsiEncoding  |
//! | `/YouPdfWMGS0`   | `/ExtGState`| opacité du texte (`/ca`)    |

use lopdf::{dictionary, Dictionary, Object, ObjectId, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::encode_pdf_literal;

/// Nom de ressource de la police posée par ce module.
const WM_FONT_NAME: &str = "YouPdfF1";

/// Nom de ressource de l'état graphique (opacité) posé par ce module.
const WM_GS_NAME: &str = "YouPdfWMGS0";

/// Clé marquant un flux de contenu posé par ce module.
const WM_MARKER: &str = "YouPdfDecoration";

/// Pages auxquelles un filigrane ou un en-tête s'applique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageRange {
    /// Toutes les pages.
    All,
    /// De cette page (1-indexée) jusqu'à la dernière.
    From(u32),
    /// Intervalle inclusif, bornes 1-indexées.
    Range(u32, u32),
}

impl PageRange {
    /// Vrai si la page (1-indexée) est dans la plage.
    pub fn contains(&self, page: u32) -> bool {
        match *self {
            PageRange::All => true,
            PageRange::From(start) => page >= start,
            PageRange::Range(start, end) => (start..=end).contains(&page),
        }
    }

    /// Pages concernées pour un document de `total` pages, bornes bornées.
    fn pages(&self, total: u32) -> Vec<u32> {
        if total == 0 {
            return Vec::new();
        }
        let (start, end) = match *self {
            PageRange::All => (1, total),
            PageRange::From(start) => (start.max(1), total),
            PageRange::Range(start, end) => (start.max(1), end.min(total)),
        };
        if start > end {
            return Vec::new();
        }
        (start..=end).collect()
    }
}

/// Style de numérotation d'une page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumberStyle {
    /// `1`
    Numeric,
    /// `1 / 12`
    NumericTotal,
    /// `i` (chiffres romains minuscules).
    Roman,
}

impl NumberStyle {
    /// Rend le numéro `page` sur un document de `total` pages.
    pub fn render(&self, page: u32, total: u32) -> String {
        match self {
            NumberStyle::Numeric => page.to_string(),
            NumberStyle::NumericTotal => format!("{page} / {total}"),
            NumberStyle::Roman => to_roman(page),
        }
    }
}

/// Description d'un filigrane texte.
#[derive(Debug, Clone)]
pub struct Watermark {
    /// Texte affiché.
    pub text: String,
    /// Taille de police en points.
    pub font_size: f32,
    /// Couleur RGB (0–255).
    pub color: [u8; 3],
    /// Opacité (0.0 à 1.0).
    pub opacity: f32,
    /// Rotation en degrés (45 = diagonale classique).
    pub rotation_deg: f32,
    /// Répète le texte sur toute la page au lieu d'un seul au centre.
    pub tiled: bool,
    /// Pages concernées.
    pub pages: PageRange,
}

impl Default for Watermark {
    fn default() -> Self {
        Self {
            text: "CONFIDENTIEL".to_string(),
            font_size: 48.0,
            color: [128, 128, 128],
            opacity: 0.25,
            rotation_deg: 45.0,
            tiled: false,
            pages: PageRange::All,
        }
    }
}

/// En-tête et pied de page, avec numérotation facultative.
#[derive(Debug, Clone)]
pub struct HeaderFooter {
    /// Texte à gauche, en-tête et pied de page.
    pub left: String,
    /// Texte centré.
    pub center: String,
    /// Texte à droite.
    pub right: String,
    /// Distance au bord de la page, en points.
    pub margin: f32,
    /// Taille de police en points.
    pub font_size: f32,
    /// Style de numérotation (aucun si `None`).
    pub page_numbers: Option<NumberStyle>,
    /// Pages concernées.
    pub pages: PageRange,
    /// Première page numérotée ; les précédentes portent le bandeau seul.
    pub start_page: u32,
    /// Numérote à partir de la deuxième page.
    pub skip_first_page: bool,
}

impl Default for HeaderFooter {
    fn default() -> Self {
        Self {
            left: String::new(),
            center: String::new(),
            right: String::new(),
            margin: 36.0,
            font_size: 10.0,
            page_numbers: None,
            pages: PageRange::All,
            start_page: 1,
            skip_first_page: false,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// Construction du flux de contenu
// ────────────────────────────────────────────────────────────────────────

/// Largeur approchée d'un texte : l'Helvetica vaut ~0.5 cadratin par
/// caractère. Suffisant pour centrer, impossible pour ajuster au point.
fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.5
}

/// Écrit un littéral PDF `(…)`, encodé en WinAnsi pour la police posée.
fn push_literal(out: &mut Vec<u8>, text: &str) {
    out.push(b'(');
    out.extend_from_slice(&encode_pdf_literal(text));
    out.push(b')');
}

/// Écrit un texte posé en (x, y).
fn push_text_line(out: &mut Vec<u8>, size: f32, x: f32, y: f32, text: &str) {
    out.extend_from_slice(
        format!(
            "BT\n/{WM_FONT_NAME} {size:.2} Tf\n0 0 0 rg\n1 0 0 1 {x:.2} {y:.2} Tm\n"
        )
        .as_bytes(),
    );
    push_literal(out, text);
    out.extend_from_slice(b" Tj\nET\n");
}

/// Écrit une ligne gauche / centré / droite sur une même base.
#[allow(clippy::too_many_arguments)]
fn push_three(
    out: &mut Vec<u8>,
    size: f32,
    margin: f32,
    y: f32,
    width: f32,
    left: &str,
    center: &str,
    right: &str,
) {
    if !left.is_empty() {
        push_text_line(out, size, margin, y, left);
    }
    if !center.is_empty() {
        push_text_line(out, size, (width - text_width(center, size)) / 2.0, y, center);
    }
    if !right.is_empty() {
        push_text_line(
            out,
            size,
            (width - margin - text_width(right, size)).max(margin),
            y,
            right,
        );
    }
}

/// Dessine une occurrence du filigrane, tournée d'un angle donné.
fn paint_watermark_text(
    out: &mut Vec<u8>,
    wm: &Watermark,
    size: f32,
    cos: f32,
    sin: f32,
    x: f32,
    y: f32,
) {
    let [r, g, b] = wm.color;
    out.extend_from_slice(
        format!(
            "BT\n/{WM_FONT_NAME} {size:.2} Tf\n{:.3} {:.3} {:.3} rg\n\
             {cos:.4} {sin:.4} {:.4} {cos:.4} {x:.2} {y:.2} Tm\n",
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            -sin
        )
        .as_bytes(),
    );
    push_literal(out, &wm.text);
    out.extend_from_slice(b" Tj\nET\n");
}

/// Flux de contenu d'un filigrane : centré, ou répété en trame.
fn watermark_ops(wm: &Watermark, width: f32, height: f32) -> Vec<u8> {
    let size = wm.font_size.clamp(4.0, 400.0);
    let radians = wm.rotation_deg.to_radians();
    let (cos, sin) = (radians.cos(), radians.sin());

    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");
    // L'état graphique porte l'opacité : sans lui le texte serait opaque.
    if wm.opacity < 1.0 {
        out.extend_from_slice(format!("/{WM_GS_NAME} gs\n").as_bytes());
    }

    if wm.tiled {
        let mut step_x = text_width(&wm.text, size) + 120.0;
        let mut step_y = size + 120.0;
        // Une page démesurée — affiche, plan de ville, ou boîte héritée d'un
        // fichier hostile — demanderait des centaines de milliers
        // d'opérations. L'espacement s'élargit plutôt que d'abandonner la
        // trame, qui doit continuer à couvrir la page.
        const MAX_TILES: f32 = 400.0;
        let estimated = (width / step_x + 1.0) * (height / step_y + 1.0);
        if estimated > MAX_TILES {
            let factor = (estimated / MAX_TILES).sqrt();
            step_x *= factor;
            step_y *= factor;
        }
        let mut y = size;
        while y < height + step_y {
            let mut x = 0.0_f32;
            while x < width + step_x {
                paint_watermark_text(&mut out, wm, size, cos, sin, x, y);
                x += step_x;
            }
            y += step_y;
        }
    } else {
        let x = ((width - text_width(&wm.text, size)) / 2.0).max(2.0);
        let y = height / 2.0 - size * 0.35;
        paint_watermark_text(&mut out, wm, size, cos, sin, x, y);
    }

    out.extend_from_slice(b"Q\n");
    out
}

/// Flux de contenu de l'en-tête et du pied d'une page.
///
/// Renvoie `None` quand il n'y a rien à écrire : la page est alors laissée
/// intacte, ce qui rend la numérotation « sauf première page » exacte.
fn header_footer_ops(
    hf: &HeaderFooter,
    page: u32,
    total: u32,
    width: f32,
    height: f32,
) -> Option<Vec<u8>> {
    // Numérotation : absente avant `start_page`, et sur la première page si
    // `skip_first_page` est actif.
    let number = match hf.page_numbers {
        Some(style) if page >= hf.start_page.max(1) && !(hf.skip_first_page && page == 1) => {
            let label = page - hf.start_page.max(1) + 1;
            Some(style.render(label, total))
        }
        _ => None,
    };

    // Le numéro rejoint le centre du pied de page.
    let footer_center = match (&number, hf.center.trim()) {
        (Some(n), "") => n.clone(),
        (Some(n), c) => format!("{c} {n}"),
        (None, _) => hf.center.clone(),
    };

    let banded = !hf.left.is_empty() || !hf.right.is_empty() || !hf.center.is_empty();
    if !banded && number.is_none() {
        return None;
    }

    let size = hf.font_size.clamp(4.0, 72.0);
    let margin = hf.margin.clamp(0.0, width.min(height) / 3.0);

    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");
    if banded {
        push_three(
            &mut out,
            size,
            margin,
            height - margin - size,
            width,
            &hf.left,
            &hf.center,
            &hf.right,
        );
    }
    push_three(
        &mut out,
        size,
        margin,
        margin,
        width,
        &hf.left,
        &footer_center,
        &hf.right,
    );
    out.extend_from_slice(b"Q\n");
    Some(out)
}

/// Dictionnaire des flux posés par ce module (clé de repérage).
fn marked_dict() -> Dictionary {
    dictionary! { WM_MARKER => Object::Boolean(true) }
}

/// Chiffres romains minuscules. `0` n'a pas d'écriture romaine : on le
// rend tel quel plutôt que de produire une chaîne vide.
fn to_roman(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, letters) in TABLE {
        while n >= value {
            out.push_str(letters);
            n -= value;
        }
    }
    out
}

// ────────────────────────────────────────────────────────────────────────
// Intégration dans un Document
// ────────────────────────────────────────────────────────────────────────

impl Document {
    /// Ajoute un filigrane texte aux pages de `wm.pages`.
    ///
    /// Renvoie le nombre de pages décorées.
    pub fn add_text_watermark(&mut self, wm: &Watermark) -> Result<usize> {
        if wm.text.trim().is_empty() {
            return Err(Error::InvalidInput("filigrane sans texte".to_string()));
        }

        let total = self.page_count() as u32;
        let pages = wm.pages.pages(total);
        for page in &pages {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let (width, height) = self.page_size(*page);

            let ops = watermark_ops(wm, width, height);
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict(), ops));

            self.wm_ensure_font(page_id)?;
            if wm.opacity < 1.0 {
                self.wm_ensure_extgstate(page_id, wm.opacity.clamp(0.05, 1.0))?;
            }
            self.wm_append_content(page_id, stream_id)?;
        }

        tracing::debug!(pages = pages.len(), "filigrane ajouté");
        Ok(pages.len())
    }

    /// Ajoute en-tête, pied de page et numérotation aux pages choisies.
    ///
    /// Renvoie le nombre de pages effectivement décorées (une page sans
    /// rien à écrire n'est pas touchée).
    pub fn add_header_footer(&mut self, hf: &HeaderFooter) -> Result<usize> {
        let total = self.page_count() as u32;
        let pages = hf.pages.pages(total);
        let mut decorated = 0usize;

        for page in &pages {
            let page_id = self
                .page_id(*page)
                .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;
            let (width, height) = self.page_size(*page);

            let Some(ops) = header_footer_ops(hf, *page, total, width, height) else {
                continue;
            };
            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(marked_dict(), ops));

            self.wm_ensure_font(page_id)?;
            self.wm_append_content(page_id, stream_id)?;
            decorated += 1;
        }

        tracing::debug!(pages = decorated, "en-tête et pied de page ajoutés");
        Ok(decorated)
    }

    /// Retire tous les filigranes, en-têtes et numéros posés par ce module.
    ///
    /// Les flux retirés sont exactement ceux marqués `/YouPdfDecoration` :
    /// le contenu d'origine de la page n'est jamais touché. Renvoie le
    /// nombre de flux retirés.
    pub fn clear_watermarks(&mut self) -> Result<usize> {
        let pages = self.inner().get_pages();
        let mut removed = 0usize;

        for (_num, page_id) in pages {
            let Ok(page) = self.inner().get_dictionary(page_id) else {
                continue;
            };
            let Ok(contents) = page.get(b"Contents") else {
                continue;
            };
            let listed: Vec<Object> = match contents {
                Object::Array(arr) => arr.clone(),
                single => vec![single.clone()],
            };

            let mut kept: Vec<Object> = Vec::with_capacity(listed.len());
            for item in &listed {
                let ours = item
                    .as_reference()
                    .ok()
                    .and_then(|id| self.inner().get_object(id).ok())
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(WM_MARKER.as_bytes()).ok())
                    .is_some();
                if ours {
                    removed += 1;
                } else {
                    kept.push(item.clone());
                }
            }
            if kept.len() == listed.len() {
                continue;
            }

            let page = self
                .inner_mut()
                .get_object_mut(page_id)
                .map_err(|e| Error::Pdf(format!("page : {e}")))?;
            let dict = page
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("page : {e}")))?;
            match kept.len() {
                0 => {
                    dict.remove(b"Contents");
                }
                1 => {
                    dict.set("Contents", kept.remove(0));
                }
                _ => dict.set("Contents", Object::Array(kept)),
            }
        }

        Ok(removed)
    }

    /// Pose la police Helvetica /WinAnsiEncoding du module sur une page.
    ///
    /// On n'écrit jamais dans une police existante : un `/F1` peut être en
    /// StandardEncoding ou en Identity-H, où des octets WinAnsi donneraient
    /// un texte illisible.
    fn wm_ensure_font(&mut self, page_id: ObjectId) -> Result<()> {
        if let Ok(fonts) = self
            .inner()
            .get_dictionary(page_id)
            .and_then(|p| p.get(b"Resources"))
            .and_then(|r| r.as_dict())
            .and_then(|r| r.get(b"Font"))
            .and_then(|f| f.as_dict())
        {
            if fonts.get(WM_FONT_NAME.as_bytes()).is_ok() {
                return Ok(());
            }
        }

        let font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        };
        let font_id = self.inner_mut().add_object(Object::Dictionary(font));

        let mut resources = self.wm_read_resources(page_id)?;
        let mut fonts = self.wm_sub_dictionary(&resources, b"Font")?;
        fonts.set(WM_FONT_NAME, Object::Reference(font_id));
        resources.set("Font", Object::Dictionary(fonts));
        self.wm_write_resources(page_id, resources)
    }

    /// Pose l'état graphique de transparence du filigrane sur une page.
    fn wm_ensure_extgstate(&mut self, page_id: ObjectId, opacity: f32) -> Result<()> {
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("ca", Object::Real(opacity));
        gs.set("CA", Object::Real(opacity));

        let mut resources = self.wm_read_resources(page_id)?;
        let mut gstates = self.wm_sub_dictionary(&resources, b"ExtGState")?;
        gstates.set(WM_GS_NAME, Object::Dictionary(gs));
        resources.set("ExtGState", Object::Dictionary(gstates));
        self.wm_write_resources(page_id, resources)
    }

    /// Sous-dictionnaire d'une clé de `/Resources`, résolu s'il est
    /// indirect. Vide s'il est absent ou illisible.
    fn wm_sub_dictionary(&self, resources: &Dictionary, key: &[u8]) -> Result<Dictionary> {
        match resources.get(key) {
            Ok(Object::Dictionary(d)) => Ok(d.clone()),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .cloned()
                .map_err(|e| Error::Pdf(format!("{key:?} : {e}"))),
            _ => Ok(Dictionary::new()),
        }
    }

    /// Ressources d'une page, résolues en dictionnaire direct.
    fn wm_read_resources(&self, page_id: ObjectId) -> Result<Dictionary> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match page.get(b"Resources") {
            Ok(Object::Dictionary(d)) => Ok(d.clone()),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_dictionary(*id)
                .cloned()
                .map_err(|e| Error::Pdf(format!("ressources : {e}"))),
            _ => Ok(Dictionary::new()),
        }
    }

    /// Écrit les ressources d'une page.
    fn wm_write_resources(&mut self, page_id: ObjectId, resources: Dictionary) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", Object::Dictionary(resources));
        Ok(())
    }

    /// Concatène un flux à la fin du contenu d'une page.
    fn wm_append_content(&mut self, page_id: ObjectId, stream_id: ObjectId) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match dict.get_mut(b"Contents") {
            Ok(Object::Reference(id)) => {
                let existing = *id;
                dict.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(existing),
                        Object::Reference(stream_id),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => arr.push(Object::Reference(stream_id)),
            _ => dict.set("Contents", Object::Reference(stream_id)),
        }
        Ok(())
    }
}