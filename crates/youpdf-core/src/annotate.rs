//! Annotations PDF natives.
//!
//! ## Approche retenue
//!
//! YouPDF crée de **véritables annotations PDF** (entrées dans le tableau
//! `/Annots` de chaque page), plutôt que de dessiner sur le contenu de la
//! page. C'est l'approche standard des éditeurs PDF professionnels :
//!
//! - les annotations restent éditables dans n'importe quel lecteur ;
//! - elles peuvent être listées, filtrées, exportées séparément ;
//! - le contenu original de la page n'est pas modifié.
//!
//! ## Types supportés
//!
//! | Type          | Subtype PDF  |
//! |---------------|--------------|
//! | Surlignage    | `/Highlight` |
//! | Soulignage    | `/Underline` |
//! | Barré         | `/StrikeOut` |
//! | Note          | `/Text`      |
//! | Texte libre   | `/FreeText`  |
//! | Rectangle     | `/Square`    |
//! | Cercle        | `/Circle`    |
//! | Crayon        | `/Ink`       |
//!
//! ## Marquage
//!
//! Toutes les annotations créées par YouPDF portent un `/NM` (nom) qui
//! commence par `youpdf-`. Cela permet de les identifier et de les
//! remplacer sans toucher aux annotations créées par d'autres outils.

use lopdf::{dictionary, Dictionary, Object, Stream};
use serde::{Deserialize, Serialize};

use youpdf_shared::{Error, Result};

/// Type d'annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationKind {
    Highlight,
    Underline,
    StrikeOut,
    /// Ligne ondulée (comme le soulignage mais en zigzag).
    Squiggly,
    Note,
    FreeText,
    Square,
    Circle,
    /// Ligne droite.
    Line,
    /// Flèche (ligne avec une pointe).
    Arrow,
    /// Polygone fermé à plusieurs sommets.
    Polygon,
    /// Polyligne ouverte à plusieurs sommets.
    PolyLine,
    Ink,
    /// Tampon texte prédéfini (Approuvé, Brouillon…).
    Stamp,
    /// Lien hypertexte.
    Link,
    /// Callout : flèche connectée à une zone de texte (3 points :
    /// extrémité de la flèche, genou, coin du texte).
    Callout,
    /// Pièce jointe (fichier embarqué).
    FileAttachment,
}

impl AnnotationKind {
    /// Nom du sous-type PDF correspondant.
    pub fn pdf_subtype(&self) -> &'static [u8] {
        match self {
            AnnotationKind::Highlight => b"Highlight",
            AnnotationKind::Underline => b"Underline",
            AnnotationKind::StrikeOut => b"StrikeOut",
            AnnotationKind::Squiggly => b"Squiggly",
            AnnotationKind::Note => b"Text",
            AnnotationKind::FreeText => b"FreeText",
            AnnotationKind::Square => b"Square",
            AnnotationKind::Circle => b"Circle",
            AnnotationKind::Line | AnnotationKind::Arrow => b"Line",
            AnnotationKind::Polygon => b"Polygon",
            AnnotationKind::PolyLine => b"PolyLine",
            AnnotationKind::Ink => b"Ink",
            AnnotationKind::Stamp => b"Stamp",
            AnnotationKind::Link => b"Link",
            AnnotationKind::Callout => b"Callout",
            AnnotationKind::FileAttachment => b"FileAttachment",
        }
    }

    /// Vrai pour les quatre mises en évidence, qui se posent sur du texte
    /// sélectionné plutôt que sur un rectangle dessiné.
    pub fn is_markup(&self) -> bool {
        matches!(
            self,
            AnnotationKind::Highlight
                | AnnotationKind::Underline
                | AnnotationKind::StrikeOut
                | AnnotationKind::Squiggly
        )
    }

    /// Nom lisible pour l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            AnnotationKind::Highlight => "Surlignage",
            AnnotationKind::Underline => "Soulignage",
            AnnotationKind::StrikeOut => "Barré",
            AnnotationKind::Squiggly => "Ondulé",
            AnnotationKind::Note => "Note",
            AnnotationKind::FreeText => "Texte libre",
            AnnotationKind::Square => "Rectangle",
            AnnotationKind::Circle => "Cercle",
            AnnotationKind::Line => "Ligne",
            AnnotationKind::Arrow => "Flèche",
            AnnotationKind::Polygon => "Polygone",
            AnnotationKind::PolyLine => "Polyligne",
            AnnotationKind::Callout => "Callout",
            AnnotationKind::Ink => "Crayon",
            AnnotationKind::Stamp => "Tampon",
            AnnotationKind::Link => "Lien",
            AnnotationKind::FileAttachment => "Pièce jointe",
        }
    }

    /// Vrai pour les formes géométriques.
    pub fn is_shape(&self) -> bool {
        matches!(
            self,
            AnnotationKind::Square
                | AnnotationKind::Circle
                | AnnotationKind::Line
                | AnnotationKind::Arrow
                | AnnotationKind::Polygon
                | AnnotationKind::PolyLine
        )
    }

    /// Ces types se dessinent en glissant un rectangle.
    pub fn is_drag_rect(&self) -> bool {
        matches!(
            self,
            AnnotationKind::Highlight
                | AnnotationKind::Underline
                | AnnotationKind::StrikeOut
                | AnnotationKind::Squiggly
                | AnnotationKind::Square
                | AnnotationKind::Circle
        )
    }
}

/// Quadrilatère `/QuadPoints` d'un rectangle : coins dans l'ordre
/// haut-gauche, haut-droit, bas-gauche, bas-droit.
pub fn quad_from_rect(x1: f32, y1: f32, x2: f32, y2: f32) -> [f32; 8] {
    [x1, y2, x2, y2, x1, y1, x2, y1]
}

/// Boîte englobante d'un quadrilatère `/QuadPoints` : `(x1, y1, x2, y2)`.
fn quad_bounds(quad: [f32; 8]) -> (f32, f32, f32, f32) {
    let mut x1 = f32::INFINITY;
    let mut y1 = f32::INFINITY;
    let mut x2 = f32::NEG_INFINITY;
    let mut y2 = f32::NEG_INFINITY;
    for point in quad.chunks_exact(2) {
        x1 = x1.min(point[0]);
        y1 = y1.min(point[1]);
        x2 = x2.max(point[0]);
        y2 = y2.max(point[1]);
    }
    (x1, y1, x2, y2)
}

/// Distance d'un point au segment `[a, b]` ; zéro s'il est dessus.
fn point_segment_distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let len2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if len2 <= f32::EPSILON {
        0.0
    } else {
        ((ap[0] * ab[0] + ap[1] * ab[1]) / len2).clamp(0.0, 1.0)
    };
    let dx = point[0] - (a[0] + ab[0] * t);
    let dy = point[1] - (a[1] + ab[1] * t);
    (dx * dx + dy * dy).sqrt()
}

/// Rééchantillonne une polyligne par des splines Catmull-Rom.
///
/// Chaque segment est remplacé par `samples` points. La courbe passe par
/// tous les points d'origine : elle arrondit les angles sans déplacer le
/// tracé, ce qui supprime le tremblement de la souris.
pub fn smooth_polyline(points: &[[f32; 2]], samples: usize) -> Vec<[f32; 2]> {
    if points.len() < 3 || samples < 2 {
        return points.to_vec();
    }
    let last = points.len() - 1;
    let mut out = Vec::with_capacity(points.len() * samples);
    for i in 0..last {
        let p0 = points[i.saturating_sub(1)];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[(i + 2).min(last)];
        for step in 0..samples {
            let t = step as f32 / samples as f32;
            let (t2, t3) = (t * t, t * t * t);
            let mut point = [0.0f32; 2];
            for (axis, slot) in point.iter_mut().enumerate() {
                *slot = 0.5
                    * (2.0 * p1[axis]
                        + (-p0[axis] + p2[axis]) * t
                        + (2.0 * p0[axis] - 5.0 * p1[axis] + 4.0 * p2[axis] - p3[axis]) * t2
                        + (-p0[axis] + 3.0 * p1[axis] - 3.0 * p2[axis] + p3[axis]) * t3);
            }
            out.push(point);
        }
    }
    out.push(points[last]);
    out
}

/// Noms de tampons standard reconnus par les lecteurs PDF.
pub const STAMP_NAMES: &[&str] = &[
    "Approuvé",
    "Brouillon",
    "Confidentiel",
    "Final",
    "Expiré",
    "Pour commentaire",
];

/// Associe un libellé de tampon au nom standard PDF (`/Name`).
pub fn stamp_name(label: &str) -> &'static [u8] {
    match label.trim().to_ascii_lowercase().as_str() {
        "approuvé" | "approved" => b"Approved",
        "confidentiel" | "confidential" => b"Confidential",
        "final" => b"Final",
        "expiré" | "expired" => b"Expired",
        "pour commentaire" | "draft" | "brouillon" => b"Draft",
        _ => b"Draft",
    }
}

/// Style du trait d'une annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BorderDash {
    /// Trait continu.
    #[default]
    Solid,
    /// Tirets.
    Dashed,
    /// Pointillés.
    Dotted,
}

impl BorderDash {
    /// Motif `/D` du PDF, vide pour un trait continu.
    pub fn pdf_pattern(self) -> &'static [f32] {
        match self {
            Self::Solid => &[],
            Self::Dashed => &[4.0, 3.0],
            Self::Dotted => &[1.0, 2.0],
        }
    }
}

/// Style visuel d'une annotation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnnotationStyle {
    /// Couleur au format RGB (0–255).
    pub color: [u8; 3],
    /// Opacité (0.0 à 1.0). Utilisée pour le `/CA` PDF.
    pub opacity: f32,
    /// Épaisseur du trait en points (pour Ink, Square, Circle).
    pub line_width: f32,
    /// Taille de police pour FreeText et Note.
    pub font_size: f32,
    /// Couleur de remplissage des formes ; `None` = pas de remplissage.
    #[serde(default)]
    pub fill: Option<[u8; 3]>,
    /// Style du trait : continu, tirets ou pointillés.
    #[serde(default)]
    pub dash: BorderDash,
    /// Bord festonné « nuage » pour les rectangles et polygones.
    #[serde(default)]
    pub cloudy: bool,
}

impl Default for AnnotationStyle {
    fn default() -> Self {
        Self {
            color: [255, 235, 59], // jaune surligneur
            opacity: 0.4,
            line_width: 1.5,
            font_size: 11.0,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
        }
    }
}

impl AnnotationStyle {
    /// Style pour un surlignage classique.
    pub fn highlighter() -> Self {
        Self::default()
    }

    /// Style pour une note (icône jaune).
    pub fn note() -> Self {
        Self {
            color: [255, 200, 0],
            opacity: 1.0,
            line_width: 1.0,
            font_size: 10.0,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
        }
    }

    /// Style pour un trait de crayon rouge.
    pub fn pen() -> Self {
        Self {
            color: [200, 40, 40],
            opacity: 1.0,
            line_width: 2.0,
            font_size: 11.0,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
        }
    }

    /// Style pour un rectangle rouge (caviardage temporaire, mesure).
    pub fn outline_red() -> Self {
        Self {
            color: [200, 40, 40],
            opacity: 1.0,
            line_width: 2.0,
            font_size: 11.0,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
        }
    }

    /// Convertit la couleur en composantes 0.0–1.0 pour le PDF.
    fn pdf_color(&self) -> Vec<Object> {
        vec![
            Object::Real(self.color[0] as f32 / 255.0),
            Object::Real(self.color[1] as f32 / 255.0),
            Object::Real(self.color[2] as f32 / 255.0),
        ]
    }
}

/// État de revue d'une annotation (`/State` + `/StateModel Review`).
///
/// C'est le marqueur « coché » de la liste de commentaires : Acrobat et UPDF
/// relisent un commentaire accepté, rejeté, annulé ou terminé.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnnotationState {
    /// Aucun état : commentaire non relu.
    #[default]
    None,
    /// Accepté.
    Accepted,
    /// Rejeté.
    Rejected,
    /// Annulé.
    Cancelled,
    /// Terminé.
    Completed,
}

impl AnnotationState {
    /// Nom `/State` du PDF.
    pub fn pdf_name(self) -> &'static [u8] {
        match self {
            Self::None => b"None",
            Self::Accepted => b"Accepted",
            Self::Rejected => b"Rejected",
            Self::Cancelled => b"Cancelled",
            Self::Completed => b"Completed",
        }
    }

    /// Depuis un nom `/State`. Un nom inconnu retombe sur [`Self::None`].
    pub fn from_pdf_name(name: &[u8]) -> Self {
        match name {
            b"Accepted" => Self::Accepted,
            b"Rejected" => Self::Rejected,
            b"Cancelled" => Self::Cancelled,
            b"Completed" => Self::Completed,
            _ => Self::None,
        }
    }

    /// Libellé anglais pour l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Accepted => "Accepted",
            Self::Rejected => "Rejected",
            Self::Cancelled => "Cancelled",
            Self::Completed => "Completed",
        }
    }

    /// Vrai quand la revue a abouti (accepté ou terminé).
    pub fn is_resolved(self) -> bool {
        matches!(self, Self::Accepted | Self::Completed)
    }

    /// Les cinq états, dans l'ordre du menu.
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Accepted,
        Self::Rejected,
        Self::Cancelled,
        Self::Completed,
    ];
}

/// Une annotation posée par l'utilisateur.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// Identifiant unique (sert de `/NM` dans le PDF).
    pub id: String,
    /// Page (1-indexée).
    pub page: usize,
    /// Type.
    pub kind: AnnotationKind,
    /// Rectangle en coordonnées PDF (origine bas-gauche).
    /// Pour Ink, il s'agit de la boîte englobante.
    pub rect: [f32; 4], // [x1, y1, x2, y2]
    /// Contenu textuel (note, texte libre, commentaire).
    pub content: String,
    /// Auteur (optionnel).
    pub author: String,
    /// Style.
    pub style: AnnotationStyle,
    /// Points pour le crayon (coordonnées PDF absolues).
    #[serde(default)]
    pub ink_points: Vec<Vec<[f32; 2]>>,
    /// Quadrilatères `/QuadPoints` (8 nombres chacun), pour les mises en
    /// évidence posées sur du texte.
    ///
    /// Un surlignage de texte suit les lignes sélectionnées ; vide, le
    /// `rect` sert de quadrilatère unique.
    #[serde(default)]
    pub quads: Vec<[f32; 8]>,
    /// Horodatage ISO 8601 (rempli à la création).
    pub created_at: String,
    /// État de revue (`/State`). Voir [`AnnotationState`].
    #[serde(default)]
    pub state: AnnotationState,
    /// `/NM` de l'annotation à laquelle celle-ci répond (`/IRT`).
    #[serde(default)]
    pub reply_to: Option<String>,
}

/// Date PDF (`D:AAAAMMJJHHmmSSZ`) pour l'entrée `/M`.
fn pdf_date(now: chrono::DateTime<chrono::Utc>) -> String {
    now.format("D:%Y%m%d%H%M%SZ").to_string()
}

/// Nom de la police des apparences (Helvetica standard).
pub const APPEARANCE_FONT_NAME: &str = "YouPdfF1";

/// Nom de l'état graphique des apparences (transparence du surlignage).
pub const APPEARANCE_GS_NAME: &str = "YouPdfGS0";

/// Ajoute un littéral PDF `(…)` : échappé pour la syntaxe, encodé WinAnsi
/// pour la police standard.
pub(crate) fn push_text_literal(out: &mut Vec<u8>, text: &str) {
    out.push(b'(');
    out.extend_from_slice(&crate::edit::encode_pdf_literal(text));
    out.push(b')');
}

/// Boîte d'un flux de formulaire (`/BBox`). Sans boîte, la taille de
/// l'apparence est indéterminée.
fn form_bbox(stream: &Stream) -> Option<[f32; 4]> {
    let bbox = stream.dict.get(b"BBox").ok()?.as_array().ok()?;
    if bbox.len() != 4 {
        return None;
    }
    let mut out = [0.0f32; 4];
    for (i, obj) in bbox.iter().enumerate() {
        out[i] = obj.as_float().ok()?;
    }
    Some(out)
}

/// Apparence d'une annotation prête à peindre dans une page.
struct Paintable {
    /// Nom de ressource unique dans `/Resources /XObject`.
    name: String,
    /// Flux d'apparence (référence, ou copie si le flux était en ligne).
    stream: Object,
    /// Matrice `cm` qui amène la boîte du flux sur le `/Rect`.
    matrix: [f32; 6],
}

impl Annotation {
    /// Coin bas-gauche et dimensions du rectangle. Un `/Rect` PDF peut
    /// arriver avec ses coins dans n'importe quel ordre.
    pub fn frame(&self) -> (f32, f32, f32, f32) {
        (
            self.rect[0].min(self.rect[2]),
            self.rect[1].min(self.rect[3]),
            (self.rect[2] - self.rect[0]).abs(),
            (self.rect[3] - self.rect[1]).abs(),
        )
    }

    /// Quadrilatères à écrire et à peindre : ceux de la sélection, ou le
    /// rectangle comme quadrilatère unique.
    pub fn quads_or_rect(&self) -> Vec<[f32; 8]> {
        if self.quads.is_empty() {
            let [x1, y1, x2, y2] = self.rect;
            vec![quad_from_rect(x1, y1, x2, y2)]
        } else {
            self.quads.clone()
        }
    }

    /// Pose les quadrilatères d'une mise en évidence et ajuste `rect` à leur
    /// union, avec la même marge que le crayon.
    pub fn set_quads(&mut self, quads: Vec<[f32; 8]>) {
        let mut x1 = f32::INFINITY;
        let mut y1 = f32::INFINITY;
        let mut x2 = f32::NEG_INFINITY;
        let mut y2 = f32::NEG_INFINITY;
        for quad in &quads {
            let (bx1, by1, bx2, by2) = quad_bounds(*quad);
            x1 = x1.min(bx1);
            y1 = y1.min(by1);
            x2 = x2.max(bx2);
            y2 = y2.max(by2);
        }
        if x1.is_finite() {
            self.rect = [x1 - 2.0, y1 - 2.0, x2 + 2.0, y2 + 2.0];
        }
        self.quads = quads;
    }

    /// Contenu de l'apparence (`/AP /N`) dans le repère local de
    /// l'annotation : origine au coin bas-gauche du `Rect`, axe Y vers le
    /// haut, unités PDF. Le lecteur pose cette apparence sur le `Rect`.
    ///
    /// Renvoie `None` quand il n'y a rien à dessiner : un lien est
    /// invisible, et lecteurs dessinent eux-mêmes l'icône des notes et des
    /// pièces jointes. Sans apparence, les autres lecteurs n'afficheraient
    /// rien du tout pour les autres types.
    pub fn appearance_content(&self) -> Option<Vec<u8>> {
        let (x1, y1, w, h) = self.frame();
        if w <= 0.5 || h <= 0.5 {
            return None;
        }
        let [r, g, b] = self.style.color;
        let (rf, gf, bf) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
        let stroke = format!("{rf:.3} {gf:.3} {bf:.3} RG");
        let fill = format!("{rf:.3} {gf:.3} {bf:.3} rg");
        let lw = self.style.line_width.clamp(0.5, 12.0);
        let gs = format!("q /{APPEARANCE_GS_NAME} gs ");
        let mut c: Vec<u8> = Vec::new();

        match self.kind {
            AnnotationKind::Link | AnnotationKind::Note | AnnotationKind::FileAttachment => {
                return None
            }

            AnnotationKind::Polygon | AnnotationKind::PolyLine => {
                // Fond transparent pour le polygone/polyligne (on se contente
                /// du trait via BS). Un polygone rempli n'aurait pas de sens
                /// en PDF, le même que rect fait sans bloc de remplissage.
                let lw = self.style.line_width.clamp(0.5, 12.0);
                let gs = format!("q /{APPEARANCE_GS_NAME} gs ");
                let (x1, y1, w, h) = self.frame();
                c.extend_from_slice(
                    format!("{gs}{stroke} {lw:.2} w 1 J 1 j {} {} {} {} re S Q\n", x1, y1, w - lw, h - lw).as_bytes(),
                );
                return Some(c);
            }

            AnnotationKind::Callout => {
                // Callout : flèche + texte, dessinée avec le trait (BS) et
                /// un rectangle simple pour le fond du texte.
                let (x1, y1, w, h) = self.frame();
                let lw = self.style.line_width.clamp(0.5, 12.0);
                let gs = format!("q /{APPEARANCE_GS_NAME} gs ");
                let stroke = format!("{:.3} {:.3} {:.3} RG", self.style.color[0] as f32 / 255.0, self.style.color[1] as f32 / 255.0, self.style.color[2] as f32 / 255.0);
                let fill = format!("{:.3} {:.3} {:.3} rg", self.style.color[0] as f32 / 255.0, self.style.color[1] as f32 / 255.0, self.style.color[2] as f32 / 255.0);
                // Flèche : simple segment du point 1 au point 2 (on utilise
                /// les trois points d'encre stockés).
                let amp = 2.0_f32.min(h / 3.0);
                c.extend_from_slice(
                    format!("{gs}{stroke} {lw:.2} w 1 J 1 j {} {} m {} {} l S Q\n", x1, y1, x1, y1 + amp).as_bytes(),
                );
                // Fond du texte : rectangle en bas.
                let text_y = h - lw - 12.0;
                c.extend_from_slice(
                    format!("q {fill} {} {} {} {} re S Q\n", x1, y1 + text_y, w - lw, text_y).as_bytes(),
                );
                // Étiquette de texte (simplifiée).
                let txt_w = 50.0_f32.max(w * 0.3);
                let txt_x = (w - txt_w) / 2.0;
                c.extend_from_slice(
                    format!("BT /Helv {:.2} Tf {} {} Td ({:.1} Tj) ET Q\n", self.style.font_size, txt_x, text_y + 3.0, "Callout").as_bytes(),
                );
            }

            // Une mise en évidence suit les lignes sélectionnées : un bloc par
            // quadrilatère, sinon un rectangle unique avalerait l'interligne.
            AnnotationKind::Highlight
            | AnnotationKind::Underline
            | AnnotationKind::StrikeOut
            | AnnotationKind::Squiggly => {
                for quad in self.quads_or_rect() {
                    let (qx1, qy1, qx2, qy2) = quad_bounds(quad);
                    let bx = qx1 - x1;
                    let by = qy1 - y1;
                    let bw = (qx2 - qx1).max(0.1);
                    let bh = (qy2 - qy1).max(0.1);
                    match self.kind {
                        AnnotationKind::Highlight => c.extend_from_slice(
                            format!("{gs}{fill} {bx:.2} {by:.2} {bw:.2} {bh:.2} re f\n")
                                .as_bytes(),
                        ),
                        AnnotationKind::Underline => {
                            let y = by + (lw * 0.6).min(bh / 2.0);
                            c.extend_from_slice(
                                format!(
                                    "q {stroke} {lw:.2} w {bx:.2} {y:.2} m {:.2} {y:.2} l S Q\n",
                                    bx + bw
                                )
                                .as_bytes(),
                            );
                        }
                        AnnotationKind::StrikeOut => {
                            let y = by + bh / 2.0;
                            c.extend_from_slice(
                                format!(
                                    "q {stroke} {lw:.2} w {bx:.2} {y:.2} m {:.2} {y:.2} l S Q\n",
                                    bx + bw
                                )
                                .as_bytes(),
                            );
                        }
                        // Squiggly : le zigzag se dessine sur le bas du bloc.
                        _ => {
                            let amp = 2.0_f32.min(bh / 3.0);
                            c.extend_from_slice(
                                format!("q {stroke} {lw:.2} w 1 J 1 j {bx:.2} {:.2} m", by + amp)
                                    .as_bytes(),
                            );
                            let mut x = 0.0_f32;
                            let mut up = true;
                            while x < bw {
                                x = (x + 2.0).min(bw);
                                let y = by + if up { amp * 2.0 } else { 0.0 };
                                c.extend_from_slice(format!(" {:.2} {y:.2} l", bx + x).as_bytes());
                                up = !up;
                            }
                            c.extend_from_slice(b" S Q\n");
                        }
                    }
                }
            }

            AnnotationKind::FreeText => {
                let size = self.style.font_size.clamp(4.0, 72.0);
                let leading = size * 1.2;
                c.extend_from_slice(
                    format!(
                        "{gs}BT /{APPEARANCE_FONT_NAME} {size:.2} Tf {fill} {leading:.2} TL \
                         2 {:.2} Td\n",
                        (h - size - 2.0).max(1.0)
                    )
                    .as_bytes(),
                );
                for line in self.content.lines() {
                    push_text_literal(&mut c, line);
                    c.extend_from_slice(b" Tj T*\n");
                }
                c.extend_from_slice(b"ET Q\n");
            }

            AnnotationKind::Square => {
                let inset = lw / 2.0;
                c.extend_from_slice(
                    format!(
                        "q {stroke} {lw:.2} w {inset:.2} {inset:.2} {:.2} {:.2} re S Q\n",
                        (w - lw).max(0.1),
                        (h - lw).max(0.1)
                    )
                    .as_bytes(),
                );
            }
            AnnotationKind::Circle => {
                // Ellipse : quatre courbes de Bézier, constante 0.5523.
                let (rx, ry) = ((w - lw) / 2.0, (h - lw) / 2.0);
                let (cx, cy) = (w / 2.0, h / 2.0);
                let (kx, ky) = (rx * 0.5523, ry * 0.5523);
                let quad = |x1: f32, y1: f32, x2: f32, y2: f32, x3: f32, y3: f32| {
                    format!("{x1:.2} {y1:.2} {x2:.2} {y2:.2} {x3:.2} {y3:.2} c ")
                };
                let mut path = format!("q {stroke} {lw:.2} w {:.2} {:.2} m ", cx + rx, cy);
                path.push_str(&quad(cx + rx, cy + ky, cx + kx, cy + ry, cx, cy + ry));
                path.push_str(&quad(cx - kx, cy + ry, cx - rx, cy + ky, cx - rx, cy));
                path.push_str(&quad(cx - rx, cy - ky, cx - kx, cy - ry, cx, cy - ry));
                path.push_str(&quad(cx + kx, cy - ry, cx + rx, cy - ky, cx + rx, cy));
                path.push_str(" h S Q\n");
                c.extend_from_slice(path.as_bytes());
            }

            AnnotationKind::Line | AnnotationKind::Arrow => {
                let (x1, y1, _w, _h) = self.frame();
                let fallback = vec![[self.rect[0], self.rect[1]], [self.rect[2], self.rect[3]]];
                let pts = if self.ink_points.first().map_or(0, |p| p.len()) >= 2 {
                    self.ink_points.first().cloned().unwrap_or(fallback)
                } else {
                    fallback
                };
                let a = pts[0];
                let b = pts[1];
                let (ax, ay) = (a[0] - x1, a[1] - y1);
                let (bx, by) = (b[0] - x1, b[1] - y1);
                c.extend_from_slice(
                    format!(
                        "q {stroke} {lw:.2} w 0 J {ax:.2} {ay:.2} m {bx:.2} {by:.2} l S"
                    )
                    .as_bytes(),
                );
                if self.kind == AnnotationKind::Arrow {
                    let (dx, dy) = (bx - ax, by - ay);
                    let len = (dx * dx + dy * dy).sqrt().max(0.01);
                    let head = (len * 0.2).clamp(5.0, 14.0);
                    let (ux, uy) = (dx / len, dy / len);
                    let (px, py) = (-uy, ux);
                    let (hx, hy) = (bx - ux * head, by - uy * head);
                    let half = head * 0.45;
                    c.extend_from_slice(
                        format!(
                            " {fill} {bx:.2} {by:.2} m {:.2} {:.2} l {:.2} {:.2} l f",
                            hx + px * half,
                            hy + py * half,
                            hx - px * half,
                            hy - py * half
                        )
                        .as_bytes(),
                    );
                }
                c.extend_from_slice(b" Q\n");
            }

            AnnotationKind::Ink => {
                let (x1, y1, _w, _h) = self.frame();
                c.extend_from_slice(
                    format!("{gs}{stroke} {lw:.2} w 1 J 1 j ").as_bytes(),
                );
                for stroke in &self.ink_points {
                    if stroke.len() < 2 {
                        continue;
                    }
                    let mut first = true;
                    for p in stroke {
                        let (x, y) = (p[0] - x1, p[1] - y1);
                        if first {
                            c.extend_from_slice(format!("{x:.2} {y:.2} m ").as_bytes());
                            first = false;
                        } else {
                            c.extend_from_slice(format!("{x:.2} {y:.2} l ").as_bytes());
                        }
                    }
                }
                c.extend_from_slice(b"S Q\n");
            }

            AnnotationKind::Stamp => {
                let size = self.style.font_size.clamp(6.0, 48.0);
                let chars = self.content.chars().count() as f32;
                // Largeur approchée de l'Helvetica (0.5 cadratin par
                // caractère) : le centrage ne peut pas être mesuré sans
                // analyser la police.
                let text_w = chars * size * 0.5;
                let (x, y) = (((w - text_w) / 2.0).max(2.0), (h / 2.0) - size * 0.35);
                c.extend_from_slice(
                    format!(
                        "q {stroke} {lw:.2} w 0 0 {w:.2} {h:.2} re S\n\
                         BT /{APPEARANCE_FONT_NAME} {size:.2} Tf {fill} 1 0 0 1 {x:.2} {y:.2} Tm\n"
                    )
                    .as_bytes(),
                );
                push_text_literal(&mut c, &self.content);
                c.extend_from_slice(b" Tj ET Q\n");
            }
        }

        Some(c)
    }

    /// Ressources référencées par [`Self::appearance_content`] : Helvetica
    /// pour le texte, état graphique pour la transparence.
    pub fn appearance_resources(&self, font: Object) -> Dictionary {
        let mut gs = Dictionary::new();
        gs.set("Type", Object::Name(b"ExtGState".to_vec()));
        gs.set("ca", Object::Real(self.style.opacity.clamp(0.05, 1.0)));
        gs.set("CA", Object::Real(1.0));

        let mut gstates = Dictionary::new();
        gstates.set(APPEARANCE_GS_NAME, Object::Dictionary(gs));

        let mut fonts = Dictionary::new();
        fonts.set(APPEARANCE_FONT_NAME, font);

        let mut res = Dictionary::new();
        res.set("Font", Object::Dictionary(fonts));
        res.set("ExtGState", Object::Dictionary(gstates));
        res
    }
}

impl Annotation {
    /// Crée une annotation avec un identifiant unique généré.
    pub fn new(page: usize, kind: AnnotationKind, rect: [f32; 4]) -> Self {
        Self {
            id: format!("youpdf-{}", uuid::Uuid::new_v4()),
            page,
            kind,
            rect,
            content: String::new(),
            author: "YouPDF".to_string(),
            style: match kind {
                AnnotationKind::Highlight | AnnotationKind::Squiggly => {
                    AnnotationStyle::highlighter()
                }
                AnnotationKind::Note => AnnotationStyle::note(),
                AnnotationKind::Ink => AnnotationStyle::pen(),
                AnnotationKind::Square
                | AnnotationKind::Circle
                | AnnotationKind::Polygon
                | AnnotationKind::PolyLine => AnnotationStyle::outline_red(),
                AnnotationKind::Line | AnnotationKind::Arrow => AnnotationStyle {
                    color: [40, 96, 200],
                    opacity: 1.0,
                    line_width: 1.5,
                    font_size: 11.0,
                    fill: None,
                    dash: BorderDash::Solid,
                    cloudy: false,
                },
                AnnotationKind::Stamp | AnnotationKind::Link | AnnotationKind::FileAttachment => {
                    AnnotationStyle {
                        color: [40, 96, 200],
                        opacity: 1.0,
                        line_width: 1.0,
                        font_size: 11.0,
                        fill: None,
                        dash: BorderDash::Solid,
                        cloudy: false,
                    }
                }
                _ => AnnotationStyle::default(),
            },
            ink_points: Vec::new(),
            quads: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            state: AnnotationState::None,
            reply_to: None,
        }
    }

    /// Ajoute une ligne de points pour le crayon.
    pub fn add_ink_stroke(&mut self, points: Vec<[f32; 2]>) {
        if points.len() >= 2 {
            self.ink_points.push(points);
        }
    }

    /// Vrai si un des tracés passe à `radius` du point : sert à la gomme
    /// mode trait, qui doit viser le tracé et non sa boîte englobante.
    pub fn ink_hit(&self, point: [f32; 2], radius: f32) -> bool {
        self.ink_points.iter().any(|stroke| {
            stroke
                .windows(2)
                .any(|pair| point_segment_distance(point, pair[0], pair[1]) <= radius)
        })
    }

    /// Coupe les tracés qui passent à `radius` du segment `[from, to]` :
    /// gomme mode segment. Un tracé touché au milieu devient deux morceaux.
    ///
    /// Renvoie le nombre de points retirés. Zéro veut dire que rien n'a
    /// bougé. Les morceaux d'un seul point ne sont pas conservés.
    pub fn erase_ink_segment(&mut self, from: [f32; 2], to: [f32; 2], radius: f32) -> usize {
        let mut removed = 0usize;
        let mut strokes: Vec<Vec<[f32; 2]>> = Vec::new();
        for stroke in &self.ink_points {
            let mut piece: Vec<[f32; 2]> = Vec::new();
            for point in stroke {
                if point_segment_distance(*point, from, to) <= radius {
                    removed += 1;
                    if piece.len() >= 2 {
                        strokes.push(std::mem::take(&mut piece));
                    } else {
                        piece.clear();
                    }
                } else {
                    piece.push(*point);
                }
            }
            if piece.len() >= 2 {
                strokes.push(piece);
            }
        }
        if removed > 0 {
            self.ink_points = strokes;
        }
        removed
    }

    /// Lisse chaque tracé par des splines Catmull-Rom et recalcule la boîte
    /// englobante. Utilisé à la fin d'un tracé au crayon.
    pub fn smooth_ink(&mut self, samples_per_segment: usize) {
        if self.ink_points.is_empty() {
            return;
        }
        self.ink_points = self
            .ink_points
            .iter()
            .map(|stroke| smooth_polyline(stroke, samples_per_segment))
            .collect();
        self.recompute_ink_bbox();
    }

    /// Met à jour la boîte englobante pour le crayon.
    pub fn recompute_ink_bbox(&mut self) {
        let mut x1 = f32::INFINITY;
        let mut y1 = f32::INFINITY;
        let mut x2 = f32::NEG_INFINITY;
        let mut y2 = f32::NEG_INFINITY;
        for stroke in &self.ink_points {
            for p in stroke {
                x1 = x1.min(p[0]);
                y1 = y1.min(p[1]);
                x2 = x2.max(p[0]);
                y2 = y2.max(p[1]);
            }
        }
        if x1.is_finite() {
            // Marge de 2 points pour que le trait ne soit pas coupé.
            self.rect = [x1 - 2.0, y1 - 2.0, x2 + 2.0, y2 + 2.0];
        }
    }

    /// Convertit l'annotation en dictionnaire PDF.
    ///
    /// Le dictionnaire contient les clés communes (`/Type`, `/Subtype`,
    /// `/Rect`, `/NM`, `/T`, `/M`, `/C`, `/CA`, `/F`) plus les clés
    /// spécifiques au sous-type.
    pub fn to_pdf_dict(&self) -> Dictionary {
        let mut d = Dictionary::new();
        d.set("Type", Object::Name(b"Annot".to_vec()));
        d.set("Subtype", Object::Name(self.kind.pdf_subtype().to_vec()));
        d.set(
            "Rect",
            Object::Array(vec![
                Object::Real(self.rect[0]),
                Object::Real(self.rect[1]),
                Object::Real(self.rect[2]),
                Object::Real(self.rect[3]),
            ]),
        );
        d.set("NM", Object::string_literal(self.id.as_str()));
        d.set("T", Object::string_literal(self.author.as_str()));
        // `/M` suit le format de date du PDF, pas celui de l'horloge interne :
        // une chaîne RFC 3339 n'est pas lisible par les autres lecteurs.
        let modified = chrono::DateTime::parse_from_rfc3339(&self.created_at)
            .map(|date| date.with_timezone(&chrono::Utc))
            .unwrap_or_else(|_| chrono::Utc::now());
        d.set("M", Object::string_literal(pdf_date(modified)));
        d.set("C", Object::Array(self.style.pdf_color()));
        d.set("CA", Object::Real(self.style.opacity));
        // F = 4 (Print) : l'annotation doit apparaître à l'impression.
        d.set("F", Object::Integer(4));

        match self.kind {
            AnnotationKind::Highlight
            | AnnotationKind::Underline
            | AnnotationKind::StrikeOut
            | AnnotationKind::Squiggly => {
                // QuadPoints : 8 nombres par quadrilatère. Une mise en
                // évidence posée sur du texte en porte un par ligne ; sinon
                // le rectangle fournit l'unique quadrilatère.
                let mut points: Vec<Object> = Vec::new();
                for quad in self.quads_or_rect() {
                    points.extend(quad.iter().map(|v| Object::Real(*v)));
                }
                d.set("QuadPoints", Object::Array(points));
                if !self.content.is_empty() {
                    d.set("Contents", Object::string_literal(self.content.as_str()));
                }
            }

            AnnotationKind::Callout => {
                // Callout : flèche + texte, stockés dans /InkList (3 points).
                // On écrit juste les points ; l'apparence est gérée par le render.
                if !self.ink_points.is_empty() {
                    let stroke = self.ink_points[0].clone();
                    let mut flat = Vec::with_capacity(stroke.len() * 2);
                    for p in &stroke {
                        flat.push(Object::Real(p[0]));
                        flat.push(Object::Real(p[1]));
                    }
                    d.set("InkList", Object::Array(vec![Object::Array(flat)]));
                }
                d.set(
                    "BS",
                    Object::Dictionary(dictionary! {
                        "W" => Object::Real(self.style.line_width),
                        "S" => Object::Name(b"S".to_vec()),
                    }),
                );
            }

            AnnotationKind::Note => {
                // Icône "Note" par défaut.
                d.set("Name", Object::Name(b"Note".to_vec()));
                d.set("Contents", Object::string_literal(self.content.as_str()));
                // La note est repliée par défaut.
                d.set("Open", Object::Boolean(false));
            }

            AnnotationKind::FreeText => {
                d.set("Contents", Object::string_literal(self.content.as_str()));
                // DA : Default Appearance, obligatoire pour FreeText.
                let color = format!(
                    "{:.3} {:.3} {:.3} rg",
                    self.style.color[0] as f32 / 255.0,
                    self.style.color[1] as f32 / 255.0,
                    self.style.color[2] as f32 / 255.0,
                );
                let da = format!("/Helv {:.1} Tf {}", self.style.font_size, color);
                d.set("DA", Object::string_literal(da.as_str()));
                // Q = 0 : aligné à gauche.
                d.set("Q", Object::Integer(0));
            }

            AnnotationKind::Square | AnnotationKind::Circle => {
                if !self.content.is_empty() {
                    d.set("Contents", Object::string_literal(self.content.as_str()));
                }
                // BS : border style.
                d.set(
                    "BS",
                    Object::Dictionary(dictionary! {
                        "W" => Object::Real(self.style.line_width),
                        "S" => Object::Name(b"S".to_vec()),
                    }),
                );
            }

            AnnotationKind::Polygon | AnnotationKind::PolyLine => {
                // Vertices are in /Vertices (already parsed into ink_points).
                // For now use the existing ink_points if any, else a fallback.
                let points = if !self.ink_points.is_empty() {
                    self.ink_points[0].clone()
                } else {
                    vec![[self.rect[0], self.rect[1]], [self.rect[2], self.rect[3]]]
                };
                let mut flat = Vec::with_capacity(points.len() * 2);
                for p in &points {
                    flat.push(Object::Real(p[0]));
                    flat.push(Object::Real(p[1]));
                }
                d.set("QuadPoints", Object::Array(flat));
                d.set(
                    "BS",
                    Object::Dictionary(dictionary! {
                        "W" => Object::Real(self.style.line_width),
                        "S" => Object::Name(b"S".to_vec()),
                    }),
                );
            }

            AnnotationKind::Ink => {
                if !self.content.is_empty() {
                    d.set("Contents", Object::string_literal(self.content.as_str()));
                }
                // InkList : tableau de tableaux de points.
                let ink_list = self
                    .ink_points
                    .iter()
                    .map(|stroke| {
                        let mut flat = Vec::with_capacity(stroke.len() * 2);
                        for p in stroke {
                            flat.push(Object::Real(p[0]));
                            flat.push(Object::Real(p[1]));
                        }
                        Object::Array(flat)
                    })
                    .collect();
                d.set("InkList", Object::Array(ink_list));
                d.set(
                    "BS",
                    Object::Dictionary(dictionary! {
                        "W" => Object::Real(self.style.line_width),
                        "S" => Object::Name(b"S".to_vec()),
                    }),
                );
            }

            AnnotationKind::Line | AnnotationKind::Arrow => {
                // /L = [x1 y1 x2 y2] ; extrémités dans le premier trait.
                let pts = self.ink_points.first().cloned().unwrap_or_default();
                let first = pts.first().copied().unwrap_or([self.rect[0], self.rect[1]]);
                let second = pts.get(1).copied().unwrap_or([self.rect[2], self.rect[3]]);
                let (x1, y1) = (first[0], first[1]);
                let (x2, y2) = (second[0], second[1]);
                d.set(
                    "L",
                    Object::Array(vec![
                        Object::Real(x1),
                        Object::Real(y1),
                        Object::Real(x2),
                        Object::Real(y2),
                    ]),
                );
                if self.kind == AnnotationKind::Arrow {
                    // Pointe de flèche à l'extrémité 2.
                    d.set(
                        "LE",
                        Object::Array(vec![
                            Object::Name(b"None".to_vec()),
                            Object::Name(b"OpenArrow".to_vec()),
                        ]),
                    );
                }
                d.set("IC", Object::Array(self.style.pdf_color()));
                d.set("Cap", Object::Boolean(false));
                d.set(
                    "BS",
                    Object::Dictionary(dictionary! {
                        "W" => Object::Real(self.style.line_width),
                        "S" => Object::Name(b"S".to_vec()),
                    }),
                );
            }

            AnnotationKind::Stamp => {
                d.set("Name", Object::Name(stamp_name(&self.content).to_vec()));
                if !self.content.is_empty() {
                    d.set("Contents", Object::string_literal(self.content.as_str()));
                }
            }

            AnnotationKind::Link => {
                // Aucune bordure visible : le lien se confond avec le texte.
                d.set(
                    "Border",
                    Object::Array(vec![Object::Integer(0), Object::Integer(0), Object::Integer(0)]),
                );
                d.set(
                    "A",
                    Object::Dictionary(dictionary! {
                        "S" => Object::Name(b"URI".to_vec()),
                        "URI" => Object::string_literal(self.content.as_str()),
                    }),
                );
            }

            AnnotationKind::FileAttachment => {
                d.set("Name", Object::Name(b"Paperclip".to_vec()));
                if !self.content.is_empty() {
                    d.set("Contents", Object::string_literal(self.content.as_str()));
                }
                // Le dictionnaire /FS (fichier embarqué) est ajouté par
                // `apply_annotations`, qui seule a accès au document.
            }
        }

        // État de revue : `/StateModel Review` est obligatoire pour que les
        // lecteurs interprètent `/State` (sinon la clé est ignorée).
        if self.state != AnnotationState::None {
            d.set("StateModel", Object::Name(b"Review".to_vec()));
            d.set("State", Object::Name(self.state.pdf_name().to_vec()));
        }
        // `/IRT` est une référence indirecte : elle est posée par
        // `apply_annotations`, qui connaît l'objet du parent.

        d
    }
}

// ────────────────────────────────────────────────────────────────────────
// Intégration dans un Document
// ────────────────────────────────────────────────────────────────────────

use crate::document::Document;

/// Préfixe d'identification des annotations créées par YouPDF.
const NM_PREFIX: &str = "youpdf-";

impl Document {
    /// Lit les annotations YouPDF présentes dans le document.
    ///
    /// Utile pour recharger un document précédemment annoté.
    pub fn read_annotations(&self) -> Vec<Annotation> {
        let mut out = Vec::new();
        let pages = self.inner().get_pages();

        for (page_num, page_id) in pages {
            let Ok(page) = self.inner().get_dictionary(page_id) else {
                continue;
            };
            let Ok(annots_ref) = page.get(b"Annots") else {
                continue;
            };
            let Ok(annots) = annots_ref.as_array() else {
                continue;
            };

            for annot_ref in annots {
                let Ok(annot_id) = annot_ref.as_reference() else {
                    continue;
                };
                let Ok(annot_obj) = self.inner().get_object(annot_id) else {
                    continue;
                };
                let Ok(annot_dict) = annot_obj.as_dict() else {
                    continue;
                };

                // Ignore les annotations qui ne sont pas de nous.
                // lopdf 0.34 : `as_str()` rend des octets.
                let nm = annot_dict
                    .get(b"NM")
                    .ok()
                    .and_then(|o| o.as_str().ok())
                    .map(|b| String::from_utf8_lossy(b).to_string())
                    .unwrap_or_default();
                if !nm.starts_with(NM_PREFIX) {
                    continue;
                }

                // Reconstitue l'annotation.
                let Some(mut ann) = parse_annotation(annot_dict, &nm, page_num as usize) else {
                    continue;
                };
                // `/IRT` vise l'objet du parent ; on remonte à son `/NM`.
                ann.reply_to = annot_dict
                    .get(b"IRT")
                    .ok()
                    .and_then(|o| o.as_reference().ok())
                    .and_then(|parent_id| self.inner().get_dictionary(parent_id).ok())
                    .and_then(|parent| parent.get(b"NM").ok())
                    .and_then(|o| o.as_str().ok())
                    .map(|bytes| String::from_utf8_lossy(bytes).to_string());
                out.push(ann);
            }
        }

        out
    }

    /// Enregistre le document en y ajoutant `annots`.
    ///
    /// Les annotations en cours d'édition vivent hors du document : c'est au
    /// moment d'écrire le fichier qu'elles y prennent place, apparence
    /// comprise. L'état en mémoire est rendu tel qu'il était — sans quoi le
    /// moteur de rendu les peindrait lui-même, en plus du dessin de
    /// l'interface, et chaque marque apparaîtrait deux fois.
    pub fn save_with_annotations(
        &mut self,
        path: impl AsRef<std::path::Path>,
        annots: &[Annotation],
    ) -> Result<()> {
        if !annots.is_empty() {
            self.apply_annotations(annots)?;
        }
        let saved = self.save(path);
        // Même si l'écriture a échoué, le document ne doit pas rester dans
        // l'état « annotations posées » : l'interface dessine à partir de la
        // liste de l'onglet, pas du document.
        let cleared = self.clear_youpdf_annotations();
        saved.and(cleared)
    }

    /// Applique un ensemble d'annotations au document.
    ///
    /// **Idempotent** : les annotations préfixées `youpdf-` déjà présentes
    /// sont supprimées, puis remplacées par celles fournies. Les autres
    /// annotations ne sont pas touchées.
    pub fn apply_annotations(&mut self, annotations: &[Annotation]) -> Result<()> {
        // 1. Retirer les anciennes annotations YouPDF.
        self.clear_youpdf_annotations()?;

        // 2. Grouper par page.
        let mut by_page: std::collections::HashMap<usize, Vec<&Annotation>> =
            std::collections::HashMap::new();
        for ann in annotations {
            by_page.entry(ann.page).or_default().push(ann);
        }

        // 3. Ajouter les nouvelles.
        // Chaque annotation créée garde de quoi résoudre `/IRT` : (objet,
        // identifiant, identifiant du parent visé).
        let mut created: Vec<(ObjectId, String, Option<String>)> = Vec::new();
        for (page_num, anns) in by_page {
            let page_id = self
                .page_id(page_num as u32)
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;

            // Créer les objets annotation.
            let mut annot_refs: Vec<Object> = Vec::new();
            // Police des apparences : créée une fois pour tout l'appel.
            let mut appearance_font: Option<Object> = None;
            for ann in anns {
                let mut dict = ann.to_pdf_dict();

                // Pièce jointe : embarquer le fichier dans le PDF.
                if ann.kind == AnnotationKind::FileAttachment {
                    if let Some(fs) = self.build_embedded_file(&ann.content)? {
                        dict.set("FS", fs);
                    }
                }

                // Apparence : sans `/AP`, les autres lecteurs n'affichent
                // rien. YouPDF dessine les siennes à l'écran, mais un PDF
                // partagé doit se voir partout.
                if let Some(content) = ann.appearance_content() {
                    let (_, _, w, h) = ann.frame();
                    let font = match &appearance_font {
                        Some(font) => font.clone(),
                        None => {
                            let font = Object::Reference(self.ensure_appearance_font()?);
                            appearance_font = Some(font.clone());
                            font
                        }
                    };
                    let mut xobject = Dictionary::new();
                    xobject.set("Type", Object::Name(b"XObject".to_vec()));
                    xobject.set("Subtype", Object::Name(b"Form".to_vec()));
                    xobject.set("FormType", Object::Integer(1));
                    xobject.set(
                        "BBox",
                        Object::Array(vec![
                            Object::Real(0.0),
                            Object::Real(0.0),
                            Object::Real(w),
                            Object::Real(h),
                        ]),
                    );
                    xobject.set("Resources", Object::Dictionary(ann.appearance_resources(font)));
                    let stream_id = self.inner_mut().add_object(Stream::new(xobject, content));
                    let mut ap = Dictionary::new();
                    ap.set("N", Object::Reference(stream_id));
                    dict.set("AP", Object::Dictionary(ap));
                }

                let id = self.inner_mut().add_object(Object::Dictionary(dict));
                annot_refs.push(Object::Reference(id));
                created.push((id, ann.id.clone(), ann.reply_to.clone()));
            }

            // Récupérer / créer le tableau /Annots de la page.
            let page = self
                .inner_mut()
                .get_object_mut(page_id)
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;
            let dict = page
                .as_dict_mut()
                .map_err(|e| Error::Pdf(format!("page {page_num} : {e}")))?;

            match dict.get_mut(b"Annots") {
                Ok(Object::Array(existing)) => {
                    existing.extend(annot_refs);
                }
                Ok(Object::Reference(existing_ref)) => {
                    // Cas rare : /Annots est une référence indirecte.
                    let existing_id = *existing_ref;
                    let existing_obj = self
                        .inner_mut()
                        .get_object_mut(existing_id)
                        .map_err(|e| Error::Pdf(format!("Annots indirects : {e}")))?;
                    if let Object::Array(arr) = existing_obj {
                        arr.extend(annot_refs);
                    }
                }
                _ => {
                    dict.set("Annots", Object::Array(annot_refs));
                }
            }
        }

        // 4. Réponses : `/IRT` est une référence indirecte vers l'objet du
        //    parent. Elle se pose au second passage, une fois tous les objets
        //    créés — un parent peut se trouver sur une autre page.
        if created.iter().any(|(_, _, reply)| reply.is_some()) {
            let by_name: std::collections::HashMap<&str, ObjectId> = created
                .iter()
                .map(|(id, name, _)| (name.as_str(), *id))
                .collect();
            for (id, _, reply) in &created {
                let Some(parent_name) = reply else {
                    continue;
                };
                let Some(&parent_id) = by_name.get(parent_name.as_str()) else {
                    // Parent absent : la réponse reste sans lien plutôt que
                    // de pointer un objet inexistant.
                    continue;
                };
                if let Ok(dict) = self.inner_mut().get_dictionary_mut(*id) {
                    dict.set("IRT", Object::Reference(parent_id));
                    // `/RT R` : réponse proprement dite (et non un simple
                    // commentaire « en réponse à »).
                    dict.set("RT", Object::Name(b"R".to_vec()));
                }
            }
        }

        Ok(())
    }

    /// Construit une /Filespec embarquant le fichier `path`, et renvoie
    /// sa référence (`/FS`). `None` si le fichier est illisible.
    fn build_embedded_file(&mut self, path: &str) -> Result<Option<Object>> {
        use std::path::Path;
        let path = Path::new(path);
        let bytes = std::fs::read(path).map_err(Error::Io)?;
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("piece_jointe");

        let stream = Stream::new(
            dictionary! {
                "Type" => Object::Name(b"EmbeddedFile".to_vec()),
                "Subtype" => Object::Name(b"application/octet-stream".to_vec()),
            },
            bytes,
        );
        let stream_id = self.inner_mut().add_object(stream);

        let mut ef = Dictionary::new();
        ef.set("F", Object::Reference(stream_id));
        let mut filespec = dictionary! {
            "Type" => Object::Name(b"Filespec".to_vec()),
            "F" => Object::string_literal(file_name),
            "UF" => Object::string_literal(file_name),
        };
        filespec.set("EF", Object::Dictionary(ef));

        let fs_id = self.inner_mut().add_object(Object::Dictionary(filespec));
        Ok(Some(Object::Reference(fs_id)))
    }

    /// Supprime du document toutes les annotations préfixées `youpdf-`.
    ///
    /// Sert à sortir les annotations du document pour les éditer : elles
    /// reviennent dans le fichier écrit par [`Self::save_with_annotations`].
    /// Les annotations des autres producteurs ne sont pas touchées.
    pub fn clear_youpdf_annotations(&mut self) -> Result<()> {
        let pages = self.inner().get_pages();
        // (page_id, annotations à conserver) — liste déjà filtrée au
        // premier passage, réécrite telle quelle au second.
        let mut updates: Vec<(ObjectId, Vec<Object>)> = Vec::new();

        for (_page_num, page_id) in pages {
            let Ok(page) = self.inner().get_dictionary(page_id) else {
                continue;
            };
            let Ok(annots_ref) = page.get(b"Annots") else {
                continue;
            };
            let Ok(annots) = annots_ref.as_array() else {
                continue;
            };

            // Filtrer : garder tout ce qui n'est pas youpdf-.
            let mut kept = Vec::with_capacity(annots.len());
            for annot_ref in annots {
                let is_ours = match annot_ref.as_reference() {
                    Ok(id) => self
                        .inner()
                        .get_object(id)
                        .ok()
                        .and_then(|o| o.as_dict().ok())
                        .and_then(|d| d.get(b"NM").ok())
                        .and_then(|nm| nm.as_str().ok())
                        .map(|b| String::from_utf8_lossy(b).starts_with(NM_PREFIX))
                        .unwrap_or(false),
                    Err(_) => false,
                };
                if !is_ours {
                    kept.push(annot_ref.clone());
                }
            }

            if kept.len() != annots.len() {
                updates.push((page_id, kept));
            }
        }

        for (page_id, kept) in updates {
            let page = self
                .inner_mut()
                .get_object_mut(page_id)
                .map_err(|e| Error::Pdf(format!("mise à jour page : {e}")))?;
            if let Object::Dictionary(dict) = page {
                dict.set("Annots", Object::Array(kept));
            }
        }

        Ok(())
    }

    /// Aplatit les annotations dans le contenu des pages.
    ///
    /// Chaque annotation dont l'apparence (`/AP /N`) est un flux de
    /// formulaire est peinte dans la page par-dessus le contenu existant,
    /// puis retirée de `/Annots`. Les annotations sans apparence exploitable
    /// — liens, et ce que d'autres outils laissent sans `/AP` — restent en
    /// place : les aplatir effacerait un dessin que seul le lecteur sait
    /// produire.
    ///
    /// Renvoie le nombre d'annotations réellement aplaties.
    pub fn flatten_annotations(&mut self) -> Result<usize> {
        let pages = self.inner().get_pages();
        let mut flattened = 0usize;

        for (_page_num, page_id) in pages {
            let annots = self.page_annots(page_id);
            if annots.is_empty() {
                continue;
            }

            let mut kept: Vec<Object> = Vec::new();
            let mut painted: Vec<Paintable> = Vec::new();
            for annot_ref in annots {
                match self.paintable_for(&annot_ref, painted.len()) {
                    Some(appearance) => painted.push(appearance),
                    None => kept.push(annot_ref),
                }
            }
            if painted.is_empty() {
                continue;
            }

            // Les apparences sont ajoutées à la fin du contenu : dans un
            // lecteur, une annotation se dessine par-dessus la page.
            let mut ops: Vec<u8> = Vec::new();
            let mut xobjects = Dictionary::new();
            for appearance in &painted {
                let [a, b, c, d, e, f] = appearance.matrix;
                ops.extend_from_slice(
                    format!(
                        "q {a:.4} {b:.4} {c:.4} {d:.4} {e:.2} {f:.2} cm /{} Do Q\n",
                        appearance.name
                    )
                    .as_bytes(),
                );
                xobjects.set(appearance.name.as_str(), appearance.stream.clone());
            }

            let stream_id = self
                .inner_mut()
                .add_object(Stream::new(Dictionary::new(), ops));
            self.add_page_xobjects(page_id, &xobjects)?;
            self.append_page_content(page_id, stream_id)?;
            self.set_page_annots(page_id, &kept)?;
            flattened += painted.len();
        }

        if flattened > 0 {
            tracing::info!(flattened, "annotations aplaties dans les pages");
        }
        Ok(flattened)
    }

    /// Annotations d'une page, tableau déjà déplié.
    pub(crate) fn page_annots(&self, page_id: ObjectId) -> Vec<Object> {
        let Ok(page) = self.inner().get_dictionary(page_id) else {
            return Vec::new();
        };
        match page.get(b"Annots") {
            Ok(Object::Array(arr)) => arr.clone(),
            Ok(Object::Reference(id)) => self
                .inner()
                .get_object(*id)
                .ok()
                .and_then(|o| o.as_array().ok())
                .cloned()
                .unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// Réécrit `/Annots` : la clé disparaît quand il ne reste rien.
    pub(crate) fn set_page_annots(&mut self, page_id: ObjectId, annots: &[Object]) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        if annots.is_empty() {
            dict.remove(b"Annots");
        } else {
            dict.set("Annots", Object::Array(annots.to_vec()));
        }
        Ok(())
    }

    /// Déplie l'apparence d'une annotation, si elle est peignable.
    fn paintable_for(&mut self, annot_ref: &Object, index: usize) -> Option<Paintable> {
        let dict = match annot_ref {
            Object::Reference(id) => self.inner().get_dictionary(*id).ok()?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => return None,
        };

        // Sans `/Rect` exploitable, il n'y a pas de place où peindre.
        let rect = dict.get(b"Rect").ok()?.as_array().ok()?;
        if rect.len() != 4 {
            return None;
        }
        let mut r = [0.0f32; 4];
        for (i, obj) in rect.iter().enumerate() {
            r[i] = obj.as_float().ok()?;
        }
        if (r[2] - r[0]).abs() < 0.5 || (r[3] - r[1]).abs() < 0.5 {
            return None;
        }

        // `/AP /N` doit être un flux de formulaire. Un dictionnaire d'états
        // (cases à cocher, boutons radio) demande de choisir l'état courant :
        // ces annotations restent en place.
        let ap = match dict.get(b"AP").ok()? {
            Object::Reference(id) => self.inner().get_dictionary(*id).ok()?.clone(),
            Object::Dictionary(d) => d.clone(),
            _ => return None,
        };
        let (stream, bbox) = match ap.get(b"N").ok()?.clone() {
            Object::Reference(id) => {
                let stream = self.inner().get_object(id).ok()?.as_stream().ok()?.clone();
                (Object::Reference(id), form_bbox(&stream)?)
            }
            Object::Stream(stream) => {
                let bbox = form_bbox(&stream)?;
                let id = self.inner_mut().add_object(stream);
                (Object::Reference(id), bbox)
            }
            _ => return None,
        };

        let bw = (bbox[2] - bbox[0]).max(0.5);
        let bh = (bbox[3] - bbox[1]).max(0.5);
        let sx = (r[2] - r[0]).abs() / bw;
        let sy = (r[3] - r[1]).abs() / bh;
        let tx = r[0].min(r[2]) - bbox[0].min(bbox[2]) * sx;
        let ty = r[1].min(r[3]) - bbox[1].min(bbox[3]) * sy;
        Some(Paintable {
            name: format!("YouPdfFlat{index}"),
            stream,
            matrix: [sx, 0.0, 0.0, sy, tx, ty],
        })
    }

    /// Ajoute des XObjects aux ressources d'une page.
    fn add_page_xobjects(&mut self, page_id: ObjectId, xobjects: &Dictionary) -> Result<()> {
        let mut resources = self.read_page_resources(page_id)?;
        let mut existing = resources
            .get(b"XObject")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        for (name, obj) in xobjects.iter() {
            existing.set(name.to_vec(), obj.clone());
        }
        resources.set("XObject", Object::Dictionary(existing));
        self.write_page_resources(page_id, resources)
    }

    /// Ressources d'une page, résolues en dictionnaire direct.
    fn read_page_resources(&self, page_id: ObjectId) -> Result<Dictionary> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        match page.get(b"Resources") {
            Ok(Object::Dictionary(d)) => Ok(d.clone()),
            Ok(Object::Reference(id)) => Ok(self
                .inner()
                .get_dictionary(*id)
                .map_err(|e| Error::Pdf(format!("ressources : {e}")))?
                .clone()),
            _ => Ok(Dictionary::new()),
        }
    }

    /// Écrit les ressources d'une page.
    fn write_page_resources(&mut self, page_id: ObjectId, resources: Dictionary) -> Result<()> {
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
    fn append_page_content(&mut self, page_id: ObjectId, stream_id: ObjectId) -> Result<()> {
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

    /// Crée la police Helvetica des apparences.
    pub(crate) fn ensure_appearance_font(&mut self) -> Result<ObjectId> {
        let font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        };
        Ok(self.inner_mut().add_object(Object::Dictionary(font)))
    }
}

use lopdf::ObjectId;

/// Reconstruit une `Annotation` à partir d'un dictionnaire PDF.
fn parse_annotation(dict: &Dictionary, id: &str, page: usize) -> Option<Annotation> {
    let subtype = dict.get(b"Subtype").ok()?.as_name().ok()?;
    let kind = match subtype {
        b"Highlight" => AnnotationKind::Highlight,
        b"Underline" => AnnotationKind::Underline,
        b"StrikeOut" => AnnotationKind::StrikeOut,
        b"Squiggly" => AnnotationKind::Squiggly,
        b"Text" => AnnotationKind::Note,
        b"FreeText" => AnnotationKind::FreeText,
        b"Square" => AnnotationKind::Square,
        b"Circle" => AnnotationKind::Circle,
        b"Line" => {
            // Une flèche a un /LE contenant OpenArrow.
            if dict.get(b"LE").ok().and_then(|o| o.as_array().ok()).is_some() {
                AnnotationKind::Arrow
            } else {
                AnnotationKind::Line
            }
        }
        b"Ink" => AnnotationKind::Ink,
        b"Stamp" => AnnotationKind::Stamp,
        b"Link" => AnnotationKind::Link,
        b"FileAttachment" => AnnotationKind::FileAttachment,
        _ => return None,
    };

    let rect_arr = dict.get(b"Rect").ok()?.as_array().ok()?;
    if rect_arr.len() < 4 {
        return None;
    }
    let num = |i: usize| -> f32 {
        rect_arr[i]
            .as_float()
            .or_else(|_| rect_arr[i].as_i64().map(|v| v as f32))
            .unwrap_or(0.0)
    };
    let rect = [num(0), num(1), num(2), num(3)];

    // Ligne/flèche : la /L donne les deux points.
    let mut ink_points = Vec::new();
    if matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow) {
        if let Some(l) = dict.get(b"L").ok().and_then(|o| o.as_array().ok()) {
            let mut pts = Vec::new();
            let mut iter = l.iter();
            while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
                pts.push([
                    x.as_float().unwrap_or(0.0),
                    y.as_float().unwrap_or(0.0),
                ]);
            }
            if pts.len() >= 2 {
                ink_points.push(pts);
            }
        }
    }
    // Polygone/polyligne : les sommets sont dans /Vertices.
    if matches!(kind, AnnotationKind::Polygon | AnnotationKind::PolyLine) {
        if let Some(v) = dict.get(b"Vertices").ok().and_then(|o| o.as_array().ok()) {
            let mut pts = Vec::new();
            let mut iter = v.iter();
            while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
                pts.push([
                    x.as_float().unwrap_or(0.0),
                    y.as_float().unwrap_or(0.0),
                ]);
            }
            if pts.len() >= 2 {
                ink_points.push(pts);
            }
        }
    }

    let content = dict
        .get(b"Contents")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_default();

    // Pour un lien, le content est l'URL (dans /A/URI), pas dans /Contents.
    let content = if matches!(kind, AnnotationKind::Link) {
        dict.get(b"A")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .and_then(|d| d.get(b"URI").ok())
            .and_then(|o| o.as_str().ok())
            .map(|s| String::from_utf8_lossy(s).to_string())
            .unwrap_or(content)
    } else {
        content
    };

    let author = dict
        .get(b"T")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).to_string())
        .unwrap_or_else(|| "YouPDF".to_string());

    let created_at = dict
        .get(b"M")
        .ok()
        .and_then(|o| o.as_str().ok())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        // Une date PDF redevient une date lisible ; une chaîne illisible est
        // gardée telle quelle plutôt que remplacée par l'heure courante.
        .map(|raw| match crate::metadata::parse_pdf_date(&raw) {
            Some(date) => date.to_rfc3339(),
            None => raw,
        })
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());

    let color = dict
        .get(b"C")
        .ok()
        .and_then(|o| o.as_array().ok())
        .map(|arr| {
            let mut c = [0u8; 3];
            for (i, v) in arr.iter().take(3).enumerate() {
                let f = v.as_float().unwrap_or(0.0);
                c[i] = (f * 255.0).clamp(0.0, 255.0) as u8;
            }
            c
        })
        .unwrap_or([255, 235, 59]);

    let opacity = dict
        .get(b"CA")
        .ok()
        .and_then(|o| o.as_float().ok())
        .unwrap_or(0.4);

    let line_width = dict
        .get(b"BS")
        .ok()
        .and_then(|o| o.as_dict().ok())
        .and_then(|d| d.get(b"W").ok())
        .and_then(|o| o.as_float().ok())
        .unwrap_or(1.5);

    let font_size = dict
        .get(b"DA")
        .ok()
        .and_then(|o| o.as_str().ok())
        .and_then(|s| {
            let text = String::from_utf8_lossy(s);
            // Format attendu : "/Helv 11.0 Tf ..." → on extrait le nombre.
            text.split_whitespace()
                .skip_while(|w| !w.ends_with("Tf"))
                .nth(1)
                .and_then(|n| n.parse::<f32>().ok())
                .or_else(|| text.split_whitespace().find_map(|w| w.parse::<f32>().ok()))
        })
        .unwrap_or(11.0);

    let mut ink_points = Vec::new();
    if kind == AnnotationKind::Ink {
        if let Ok(list) = dict.get(b"InkList").and_then(|o| o.as_array()) {
            for stroke_obj in list {
                if let Ok(stroke) = stroke_obj.as_array() {
                    let mut points = Vec::new();
                    let mut iter = stroke.iter();
                    while let (Some(x), Some(y)) = (iter.next(), iter.next()) {
                        points.push([x.as_float().unwrap_or(0.0), y.as_float().unwrap_or(0.0)]);
                    }
                    if points.len() >= 2 {
                        ink_points.push(points);
                    }
                }
            }
        }
    }

    let mut quads = Vec::new();
    if kind.is_markup() {
        if let Ok(points) = dict.get(b"QuadPoints").and_then(|o| o.as_array()) {
            let values: Vec<f32> = points
                .iter()
                .map(|v| v.as_float().unwrap_or(0.0))
                .collect();
            for chunk in values.chunks_exact(8) {
                let mut quad = [0.0f32; 8];
                quad.copy_from_slice(chunk);
                quads.push(quad);
            }
        }
    }

    let state = dict
        .get(b"State")
        .ok()
        .and_then(|o| o.as_name().ok())
        .map(AnnotationState::from_pdf_name)
        .unwrap_or_default();

    Some(Annotation {
        id: id.to_string(),
        page,
        kind,
        rect,
        content,
        author,
        style: AnnotationStyle {
            color,
            opacity,
            line_width,
            font_size,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
        },
        ink_points,
        quads,
        created_at,
        state,
        // `/IRT` est une référence indirecte : elle est résolue par
        // `read_annotations`, qui seule a accès au document.
        reply_to: None,
    })
}

// ────────────────────────────────────────────────────────────────────────
// Export d'une liste de commentaires
// ────────────────────────────────────────────────────────────────────────

/// Format d'export de la liste de commentaires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentFormat {
    /// Markdown lisible (rapport).
    Markdown,
    /// CSV `page,type,author,date,state,reply_to,content`.
    Csv,
    /// XFDF, le format d'échange de commentaires d'Acrobat.
    Xfdf,
    /// FDF, conteneur PDF de commentaires.
    Fdf,
}

impl CommentFormat {
    /// Nom lisible pour l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Csv => "CSV",
            Self::Xfdf => "XFDF",
            Self::Fdf => "FDF",
        }
    }

    /// Extension de fichier.
    pub fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Csv => "csv",
            Self::Xfdf => "xfdf",
            Self::Fdf => "fdf",
        }
    }

    /// Les quatre formats, dans l'ordre du menu.
    pub const ALL: [Self; 4] = [Self::Csv, Self::Markdown, Self::Xfdf, Self::Fdf];
}

/// Exporte une liste de commentaires vers `format`.
///
/// L'ordre reçu est conservé : l'appelant l'a déjà trié. Les réponses
/// (`reply_to`) sont indentées en Markdown et portent leur lien `/IRT`
/// ailleurs.
pub fn export_comments(annotations: &[Annotation], format: CommentFormat) -> Result<Vec<u8>> {
    match format {
        CommentFormat::Markdown => Ok(export_comments_markdown(annotations).into_bytes()),
        CommentFormat::Csv => Ok(export_comments_csv(annotations).into_bytes()),
        CommentFormat::Xfdf => Ok(export_comments_xfdf(annotations).into_bytes()),
        CommentFormat::Fdf => export_comments_fdf(annotations),
    }
}

/// Nom du sous-type PDF, stable et lisible par les autres outils.
fn comment_type_name(annotation: &Annotation) -> String {
    String::from_utf8_lossy(annotation.kind.pdf_subtype()).to_string()
}

/// Jour seul de l'horodatage (`AAAA-MM-JJ`).
fn comment_day(annotation: &Annotation) -> &str {
    annotation.created_at.get(..10).unwrap_or("")
}

/// Date PDF (`D:AAAAMMJJHHmmSSZ`) depuis l'horodatage interne.
///
/// Les formats d'échange de commentaires attendent une date PDF, pas une
/// chaîne RFC 3339.
fn comment_pdf_date(annotation: &Annotation) -> String {
    chrono::DateTime::parse_from_rfc3339(&annotation.created_at)
        .map(|date| pdf_date(date.with_timezone(&chrono::Utc)))
        .unwrap_or_else(|_| annotation.created_at.clone())
}

fn export_comments_markdown(annotations: &[Annotation]) -> String {
    let mut out = String::from("# Comments\n");
    for annotation in annotations {
        out.push('\n');
        if annotation.reply_to.is_some() {
            // Une réponse se place sous son commentaire parent.
            out.push_str(&format!(
                "#### ↳ Reply · {} · {}\n",
                annotation.author,
                comment_day(annotation)
            ));
        } else {
            out.push_str(&format!(
                "## p.{} · {} · {} · {}\n",
                annotation.page,
                comment_type_name(annotation),
                annotation.author,
                comment_day(annotation)
            ));
            if annotation.state != AnnotationState::None {
                out.push_str(&format!("\n**Status:** {}\n", annotation.state.display_name()));
            }
        }
        if !annotation.content.is_empty() {
            out.push('\n');
            out.push_str(&annotation.content);
            out.push('\n');
        }
    }
    out
}

fn export_comments_csv(annotations: &[Annotation]) -> String {
    use crate::util::csv_escape;

    let mut out = String::from("page,type,author,date,state,reply_to,content\n");
    for annotation in annotations {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            annotation.page,
            csv_escape(&comment_type_name(annotation)),
            csv_escape(&annotation.author),
            csv_escape(comment_day(annotation)),
            csv_escape(annotation.state.display_name()),
            csv_escape(annotation.reply_to.as_deref().unwrap_or("")),
            csv_escape(&annotation.content),
        ));
    }
    out
}

/// Nom d'élément XFDF associé à un type d'annotation.
fn xfdf_element(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Highlight => "highlight",
        AnnotationKind::Underline => "underline",
        AnnotationKind::StrikeOut => "strikeout",
        AnnotationKind::Squiggly => "squiggly",
        AnnotationKind::Note => "text",
        AnnotationKind::FreeText => "freetext",
        AnnotationKind::Square => "square",
        AnnotationKind::Circle => "circle",
        AnnotationKind::Line | AnnotationKind::Arrow => "line",
        AnnotationKind::Polygon => "polygon",
        AnnotationKind::PolyLine => "polyline",
        AnnotationKind::Callout => "callout",
        AnnotationKind::Ink => "ink",
        AnnotationKind::Stamp => "stamp",
        AnnotationKind::Link => "link",
        AnnotationKind::FileAttachment => "fileattachment",
    }
}

fn export_comments_xfdf(annotations: &[Annotation]) -> String {
    use crate::util::escape_xml;

    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n<annots>\n",
    );
    for annotation in annotations {
        let element = xfdf_element(annotation.kind);
        let color = format!(
            "#{:02X}{:02X}{:02X}",
            annotation.style.color[0], annotation.style.color[1], annotation.style.color[2]
        );
        out.push_str(&format!(
            "<{element} page=\"{}\" rect=\"{:.2},{:.2},{:.2},{:.2}\" color=\"{color}\" \
             date=\"{}\" title=\"{}\" name=\"{}\" subject=\"{}\"",
            annotation.page.saturating_sub(1),
            annotation.rect[0],
            annotation.rect[1],
            annotation.rect[2],
            annotation.rect[3],
            escape_xml(&comment_pdf_date(annotation)),
            escape_xml(&annotation.author),
            escape_xml(&annotation.id),
            escape_xml(&comment_type_name(annotation)),
        ));
        if annotation.state != AnnotationState::None {
            out.push_str(&format!(" state=\"{}\"", annotation.state.display_name()));
        }
        if let Some(parent) = &annotation.reply_to {
            out.push_str(&format!(" inreplyto=\"{}\"", escape_xml(parent)));
        }
        out.push('>');
        if !annotation.content.is_empty() {
            out.push_str(&format!(
                "<contents>{}</contents>",
                escape_xml(&annotation.content)
            ));
        }
        out.push_str(&format!("</{element}>\n"));
    }
    out.push_str("</annots>\n</xfdf>\n");
    out
}

/// Dictionnaire FDF d'une annotation (sans `/IRT`, posé au second passage).
fn fdf_annot_dict(annotation: &Annotation) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(annotation.kind.pdf_subtype().to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(annotation.rect[0]),
            Object::Real(annotation.rect[1]),
            Object::Real(annotation.rect[2]),
            Object::Real(annotation.rect[3]),
        ]),
    );
    // `/Page` est l'indice de page, base zéro, comme dans tout FDF.
    d.set(
        "Page",
        Object::Integer(annotation.page.saturating_sub(1) as i64),
    );
    d.set("NM", Object::string_literal(annotation.id.as_str()));
    d.set("T", Object::string_literal(annotation.author.as_str()));
    // `/M` est une date PDF, comme dans `to_pdf_dict`.
    d.set("M", Object::string_literal(comment_pdf_date(annotation)));
    if !annotation.content.is_empty() {
        d.set("Contents", Object::string_literal(annotation.content.as_str()));
    }
    if annotation.state != AnnotationState::None {
        d.set("StateModel", Object::Name(b"Review".to_vec()));
        d.set("State", Object::Name(annotation.state.pdf_name().to_vec()));
    }
    d
}

fn export_comments_fdf(annotations: &[Annotation]) -> Result<Vec<u8>> {
    let mut doc = lopdf::Document::with_version("1.2");

    // Les annotations sont des objets indirects : `/IRT` peut alors viser
    // l'objet parent plutôt que de le recopier.
    let mut ids: Vec<(String, Option<String>, lopdf::ObjectId)> = Vec::new();
    for annotation in annotations {
        let id = doc.add_object(Object::Dictionary(fdf_annot_dict(annotation)));
        ids.push((annotation.id.clone(), annotation.reply_to.clone(), id));
    }
    for (name, reply, id) in &ids {
        let Some(parent) = reply else { continue };
        let Some((_, _, parent_id)) = ids.iter().find(|(other, _, _)| other == parent) else {
            continue;
        };
        if name == parent {
            continue;
        }
        if let Ok(dict) = doc.get_dictionary_mut(*id) {
            dict.set("IRT", Object::Reference(*parent_id));
        }
    }

    let mut fdf = Dictionary::new();
    fdf.set(
        "Annots",
        Object::Array(ids.iter().map(|(_, _, id)| Object::Reference(*id)).collect()),
    );
    let root = doc.add_object(Object::Dictionary(
        dictionary! { "FDF" => Object::Dictionary(fdf) },
    ));
    doc.trailer.set("Root", Object::Reference(root));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| Error::Pdf(format!("export FDF : {e}")))?;
    // Un FDF est un PDF dont seul l'en-tête change ; les lecteurs refusent
    // un conteneur étiqueté « PDF ».
    if bytes.starts_with(b"%PDF-1.2") {
        bytes[..8].copy_from_slice(b"%FDF-1.2");
    }
    Ok(bytes)
}
