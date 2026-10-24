//! Génération de l'apparence PDF d'une annotation (flux, XObject, encre).

use lopdf::{dictionary, Dictionary, Object};

use super::render::{
    dynamic_stamp_text, point_segment_distance,
    smooth_polyline, stamp_name, stamp_template,
};
use crate::annotate::render::pdf_date;
use super::{
    Annotation, AnnotationKind, AnnotationState, AnnotationStyle, BorderDash, STAMP_FONT_SIZE,
};

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
