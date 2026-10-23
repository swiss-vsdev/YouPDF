//! Types et constantes des annotations.

use lopdf::{dictionary, Dictionary, Object};
use serde::{Deserialize, Serialize};

use super::render::{
    dynamic_stamp_text, pdf_date, point_segment_distance, smooth_polyline, stamp_name,
    stamp_template,
};

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

/// Noms de tampons standard reconnus par les lecteurs PDF.
pub const STAMP_NAMES: &[&str] = &[
    "Approuvé",
    "Brouillon",
    "Confidentiel",
    "Final",
    "Expiré",
    "Pour commentaire",
    "Approuvé",       // alias English
    "Non approuvé",   // Not Approved
    "Signer ici",     // Sign Here
    "Initial ici",    // Initial Here
    "Témoin",         // Witness
    "Personnalisé",   // Custom stamp
];

/// Modèle de tampon proposé par la palette.
///
/// Le libellé est celui du bouton, `pdf_name` celui du `/Name` écrit dans
/// l'annotation : un lecteur tiers affiche le sien, pas le nôtre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StampTemplate {
    /// Identifiant stable, utilisé par la palette et la configuration.
    pub id: &'static str,
    /// Libellé affiché.
    pub label: &'static str,
    /// Nom standard PDF (`/Name`).
    pub pdf_name: &'static str,
    /// Couleur du cadre et du texte.
    pub color: [u8; 3],
    /// Ajouter la date et l'auteur sous le libellé.
    pub dynamic: bool,
}

/// Les douze tampons de la palette, dans l'ordre d'affichage.
pub const STAMP_TEMPLATES: &[StampTemplate] = &[
    StampTemplate {
        id: "approved",
        label: "Approved",
        pdf_name: "Approved",
        color: [32, 132, 92],
        dynamic: true,
    },
    StampTemplate {
        id: "not_approved",
        label: "Not Approved",
        pdf_name: "NotApproved",
        color: [186, 58, 58],
        dynamic: true,
    },
    StampTemplate {
        id: "draft",
        label: "Draft",
        pdf_name: "Draft",
        color: [120, 120, 128],
        dynamic: true,
    },
    StampTemplate {
        id: "final",
        label: "Final",
        pdf_name: "Final",
        color: [32, 96, 176],
        dynamic: true,
    },
    StampTemplate {
        id: "confidential",
        label: "Confidential",
        pdf_name: "Confidential",
        color: [176, 40, 48],
        dynamic: true,
    },
    StampTemplate {
        id: "expired",
        label: "Expired",
        pdf_name: "Expired",
        color: [128, 96, 40],
        dynamic: true,
    },
    StampTemplate {
        id: "for_comment",
        label: "For Comment",
        pdf_name: "Draft",
        color: [96, 96, 168],
        dynamic: true,
    },
    StampTemplate {
        id: "sign_here",
        label: "Sign Here",
        pdf_name: "SignHere",
        color: [32, 96, 176],
        dynamic: false,
    },
    StampTemplate {
        id: "initial_here",
        label: "Initial Here",
        pdf_name: "InitialHere",
        color: [32, 96, 176],
        dynamic: false,
    },
    StampTemplate {
        id: "witness",
        label: "Witness",
        pdf_name: "Witness",
        color: [72, 72, 80],
        dynamic: true,
    },
    StampTemplate {
        id: "reviewed",
        label: "Reviewed",
        pdf_name: "Approved",
        color: [32, 132, 92],
        dynamic: true,
    },
    StampTemplate {
        id: "void",
        label: "Void",
        pdf_name: "NotApproved",
        color: [186, 58, 58],
        dynamic: true,
    },
];

/// Taille de police d'un tampon, en points.
pub const STAMP_FONT_SIZE: f32 = 14.0;

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
    /// Tampon : modèle choisi dans la palette (vide pour un tampon importé
    /// ou créé à la main, qui n'a pas de libellé dynamique).
    #[serde(default)]
    pub stamp: Option<String>,
}

/// Nom de la police des apparences (Helvetica standard).
pub const APPEARANCE_FONT_NAME: &str = "YouPdfF1";

/// Nom de l'état graphique des apparences (transparence du surlignage).
pub const APPEARANCE_GS_NAME: &str = "YouPdfGS0";

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
            stamp: None,
        }
    }

    /// Crée un tampon dynamique : libellé, auteur et date du modèle.
    ///
    /// `date` est le texte affiché (déjà formaté par l'appelant), de sorte
    /// que la même annotation se reproduise à l'identique dans un test.
    pub fn stamp(page: usize, rect: [f32; 4], template_id: &str, author: &str, date: &str) -> Self {
        let template = stamp_template(template_id);
        let mut annot = Self::new(page, AnnotationKind::Stamp, rect);
        annot.content = dynamic_stamp_text(template, author, date);
        annot.style.color = template.color;
        annot.style.font_size = STAMP_FONT_SIZE;
        if !author.trim().is_empty() {
            annot.author = author.trim().to_string();
        }
        annot.stamp = Some(template.id.to_string());
        annot
    }

    /// Vrai si c'est un tampon dynamique : le texte se régénère à partir du
    /// modèle, de l'auteur et de la date.
    pub fn is_dynamic_stamp(&self) -> bool {
        self.kind == AnnotationKind::Stamp
            && self
                .stamp
                .as_deref()
                .map(|id| stamp_template(id).dynamic)
                .unwrap_or(false)
    }

    /// Met à jour le texte d'un tampon dynamique sans toucher au reste.
    pub fn refresh_stamp(&mut self, author: &str, date: &str) {
        let Some(id) = self.stamp.clone() else {
            return;
        };
        let template = stamp_template(&id);
        if template.dynamic {
            self.content = dynamic_stamp_text(template, author, date);
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
                // Le `/Name` vient du modèle quand l'annotation en a un :
                // le contenu porte désormais l'auteur et la date, que
                // `stamp_name` ne saurait pas interpréter.
                let pdf_name = match self.stamp.as_deref() {
                    Some(id) => stamp_template(id).pdf_name.as_bytes(),
                    None => stamp_name(&self.content),
                };
                d.set("Name", Object::Name(pdf_name.to_vec()));
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
