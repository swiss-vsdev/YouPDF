//! Lecture et écriture des annotations natives dans un `Document`.

use lopdf::Dictionary;

use crate::document::Document;

use super::NM_PREFIX;

use super::{Annotation, AnnotationKind, AnnotationState, AnnotationStyle, BorderDash, stamp_id_for_pdf_name};

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
        // Un tampon relu garde son `/Name` : c'est lui qui dit si le texte
        // vient d'un modèle dynamique.
        stamp: dict
            .get(b"Name")
            .ok()
            .and_then(|o| o.as_name().ok())
            .and_then(|name| std::str::from_utf8(name).ok())
            .and_then(stamp_id_for_pdf_name),
    })
}

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
}
