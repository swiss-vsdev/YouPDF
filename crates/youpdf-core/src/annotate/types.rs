//! Types et constantes des annotations.

use lopdf::Object;
use serde::{Deserialize, Serialize};


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
    pub(crate) fn pdf_color(&self) -> Vec<Object> {
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
