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
    Ink,
    /// Tampon texte prédéfini (Approuvé, Brouillon…).
    Stamp,
    /// Lien hypertexte.
    Link,
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
            AnnotationKind::Ink => b"Ink",
            AnnotationKind::Stamp => b"Stamp",
            AnnotationKind::Link => b"Link",
            AnnotationKind::FileAttachment => b"FileAttachment",
        }
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
            AnnotationKind::Ink => "Crayon",
            AnnotationKind::Stamp => "Tampon",
            AnnotationKind::Link => "Lien",
            AnnotationKind::FileAttachment => "Pièce jointe",
        }
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

/// Style visuel d'une annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnotationStyle {
    /// Couleur au format RGB (0–255).
    pub color: [u8; 3],
    /// Opacité (0.0 à 1.0). Utilisée pour le `/CA` PDF.
    pub opacity: f32,
    /// Épaisseur du trait en points (pour Ink, Square, Circle).
    pub line_width: f32,
    /// Taille de police pour FreeText et Note.
    pub font_size: f32,
}

impl Default for AnnotationStyle {
    fn default() -> Self {
        Self {
            color: [255, 235, 59], // jaune surligneur
            opacity: 0.4,
            line_width: 1.5,
            font_size: 11.0,
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
        }
    }

    /// Style pour un trait de crayon rouge.
    pub fn pen() -> Self {
        Self {
            color: [200, 40, 40],
            opacity: 1.0,
            line_width: 2.0,
            font_size: 11.0,
        }
    }

    /// Style pour un rectangle rouge (caviardage temporaire, mesure).
    pub fn outline_red() -> Self {
        Self {
            color: [200, 40, 40],
            opacity: 1.0,
            line_width: 2.0,
            font_size: 11.0,
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

/// Une annotation posée par l'utilisateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Horodatage ISO 8601 (rempli à la création).
    pub created_at: String,
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

    /// Contenu de l'apparence (`/AP /N`) dans le repère local de
    /// l'annotation : origine au coin bas-gauche du `Rect`, axe Y vers le
    /// haut, unités PDF. Le lecteur pose cette apparence sur le `Rect`.
    ///
    /// Renvoie `None` quand il n'y a rien à dessiner : un lien est
    /// invisible, et lecteurs dessinent eux-mêmes l'icône des notes et des
    /// pièces jointes. Sans apparence, les autres lecteurs n'afficheraient
    /// rien du tout pour les autres types.
    pub fn appearance_content(&self) -> Option<Vec<u8>> {
        let (_x1, _y1, w, h) = self.frame();
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

            // Le surlignage est une bande translucide ; le zigzag se dessine
            // plus bas, comme un souligné ondulé.
            AnnotationKind::Highlight => {
                c.extend_from_slice(
                    format!("{gs}{fill} 0 0 {w:.2} {h:.2} re f Q\n").as_bytes(),
                );
            }
            AnnotationKind::Underline => {
                let y = (lw * 0.6).min(h / 2.0);
                c.extend_from_slice(
                    format!("q {stroke} {lw:.2} w 0 {y:.2} m {w:.2} {y:.2} l S Q\n").as_bytes(),
                );
            }
            AnnotationKind::StrikeOut => {
                let y = h / 2.0;
                c.extend_from_slice(
                    format!("q {stroke} {lw:.2} w 0 {y:.2} m {w:.2} {y:.2} l S Q\n").as_bytes(),
                );
            }
            AnnotationKind::Squiggly => {
                let amp = 2.0_f32.min(h / 3.0);
                c.extend_from_slice(
                    format!("q {stroke} {lw:.2} w 1 J 1 j 0 {amp:.2} m").as_bytes(),
                );
                let mut x = 0.0_f32;
                let mut up = true;
                while x < w {
                    x = (x + 2.0).min(w);
                    let y = if up { amp * 2.0 } else { 0.0 };
                    c.extend_from_slice(format!(" {x:.2} {y:.2} l").as_bytes());
                    up = !up;
                }
                c.extend_from_slice(b" S Q\n");
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
                AnnotationKind::Square | AnnotationKind::Circle => {
                    AnnotationStyle::outline_red()
                }
                AnnotationKind::Line | AnnotationKind::Arrow => AnnotationStyle {
                    color: [40, 96, 200],
                    opacity: 1.0,
                    line_width: 1.5,
                    font_size: 11.0,
                },
                AnnotationKind::Stamp | AnnotationKind::Link | AnnotationKind::FileAttachment => {
                    AnnotationStyle {
                        color: [40, 96, 200],
                        opacity: 1.0,
                        line_width: 1.0,
                        font_size: 11.0,
                    }
                }
                _ => AnnotationStyle::default(),
            },
            ink_points: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Ajoute une ligne de points pour le crayon.
    pub fn add_ink_stroke(&mut self, points: Vec<[f32; 2]>) {
        if points.len() >= 2 {
            self.ink_points.push(points);
        }
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
                // QuadPoints : 8 nombres par quadrilatère.
                // Pour un rectangle simple, un seul quadrilatère aux coins
                // dans l'ordre (haut-gauche, haut-droit, bas-gauche, bas-droit).
                let [x1, y1, x2, y2] = self.rect;
                d.set(
                    "QuadPoints",
                    Object::Array(vec![
                        Object::Real(x1),
                        Object::Real(y2),
                        Object::Real(x2),
                        Object::Real(y2),
                        Object::Real(x1),
                        Object::Real(y1),
                        Object::Real(x2),
                        Object::Real(y1),
                    ]),
                );
                if !self.content.is_empty() {
                    d.set("Contents", Object::string_literal(self.content.as_str()));
                }
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
                let Some(ann) = parse_annotation(annot_dict, &nm, page_num as usize) else {
                    continue;
                };
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
        },
        ink_points,
        created_at,
    })
}
