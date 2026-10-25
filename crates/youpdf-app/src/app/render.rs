//! Rendu et formatage extraits de `app.rs` : thème, texte, libellés,
//! annotations dessinées sur le canevas.

use super::*;

/// Réglages egui d'un thème. La palette et les visuels vivent dans
/// `youpdf_shared::theme` : un seul endroit définit les couleurs.
pub(crate) fn apply_theme(ctx: &egui::Context, theme: Theme) {
    theme.apply(ctx);
}

/// Extrait le texte du document pour le contexte IA.
///
/// Plafonné à 40 pages et 60 000 caractères : inutile d'envoyer un roman
/// entier au modèle à chaque question. Le plafond est volontairement large
/// pour couvrir un contrat ou un rapport typique.
pub(crate) fn extract_document_text(doc: &Document) -> String {
    const MAX_PAGES: usize = 40;
    const MAX_CHARS: usize = 60_000;

    let total = doc.page_count().min(MAX_PAGES);
    let mut out = String::new();
    for page in 1..=total as u32 {
        if let Ok(text) = doc.page_text(page) {
            out.push_str(&text.replace('\u{c}', "\n"));
            out.push('\n');
        }
    }
    if out.chars().count() > MAX_CHARS {
        out = out.chars().take(MAX_CHARS).collect();
    }
    out
}

/// Libellé anglais d'un type de champ de formulaire.
pub(crate) fn form_kind_label(kind: &FormFieldKind) -> String {
    kind.display_name().to_owned()
}

/// Libellé anglais d'un type d'annotation (alias de
/// [`youpdf_core::annotate::AnnotationKind::display_name`]).
pub(crate) fn annot_label(kind: AnnotationKind) -> &'static str {
    kind.display_name()
}

/// Distance au présent, en clair (« 5 min », « 3 h », « 2 d »).
pub(crate) fn relative_time(when: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(when) else {
        return when.get(..10).unwrap_or("").to_owned();
    };
    let seconds = (now - then.with_timezone(&chrono::Utc)).num_seconds();
    if seconds < 60 {
        return "now".to_owned();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours} h");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days} d");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months} mo");
    }
    format!("{} y", months / 12)
}

/// Icône du marqueur d'état.
pub(crate) fn comment_state_icon(state: AnnotationState) -> &'static str {
    match state {
        AnnotationState::None => ph::CHAT_CIRCLE,
        AnnotationState::Accepted => ph::CHECK_CIRCLE,
        AnnotationState::Rejected => ph::X_CIRCLE,
        AnnotationState::Cancelled => ph::PROHIBIT,
        AnnotationState::Completed => ph::CHECK,
    }
}

/// Couleur du marqueur d'état.
pub(crate) fn comment_state_color(state: AnnotationState) -> Color32 {
    match state {
        AnnotationState::None => pal().muted,
        AnnotationState::Accepted => Color32::from_rgb(72, 168, 104),
        AnnotationState::Rejected => Color32::from_rgb(206, 88, 84),
        AnnotationState::Cancelled => Color32::from_rgb(206, 158, 72),
        AnnotationState::Completed => pal().icon_bright,
    }
}

/// Dessine les annotations d'une page ; coordonnées PDF → écran.
pub(super) fn draw_annots(
    painter: &Painter,
    tab: &Tab,
    page: u32,
    page_rect: Rect,
    scale: f32,
    selected: Option<usize>,
) {
    for (idx, annot) in tab.annots.iter().enumerate() {
        // Une réponse vit dans le fil de discussion, pas sur la page.
        if annot.page != page as usize || annot.reply_to.is_some() {
            continue;
        }
        let to_screen =
            |x: f32, y: f32| pos2(page_rect.min.x + x * scale, page_rect.max.y - y * scale);
        let r = Rect::from_two_pos(
            to_screen(annot.rect[0], annot.rect[1]),
            to_screen(annot.rect[2], annot.rect[3]),
        );
        let color = Color32::from_rgb(
            annot.style.color[0],
            annot.style.color[1],
            annot.style.color[2],
        );
        let alpha = (annot.style.opacity.clamp(0.05, 1.0) * 255.0) as u8;
        let fill = Color32::from_rgba_unmultiplied(
            annot.style.color[0],
            annot.style.color[1],
            annot.style.color[2],
            alpha,
        );
        let width = (annot.style.line_width * scale.max(0.3)).max(1.0);
        let font = (annot.style.font_size * scale).clamp(6.0, 48.0);
        // Une mise en évidence posée sur du texte porte un quadrilatère par
        // ligne ; sans quadrilatère, le rectangle fait la boîte.
        let quad_rect = |q: [f32; 8]| -> Rect {
            let points = [
                to_screen(q[0], q[1]),
                to_screen(q[2], q[3]),
                to_screen(q[4], q[5]),
                to_screen(q[6], q[7]),
            ];
            let mut rect = Rect::from_min_max(points[0], points[0]);
            for point in points.iter().skip(1) {
                rect = rect.union(Rect::from_min_max(*point, *point));
            }
            rect
        };
        let blocks: Vec<Rect> = if annot.quads.is_empty() {
            vec![r]
        } else {
            annot.quads.iter().map(|q| quad_rect(*q)).collect()
        };

        match annot.kind {
            AnnotationKind::Highlight => {
                for block in &blocks {
                    painter.rect_filled(*block, Rounding::same(RADIUS_XS), fill);
                }
            }
            AnnotationKind::Squiggly => {
                // Même ondulation que l'apparence enregistrée : deux crans
                // par motif, posés sur le bas de la boîte.
                let amp = (3.0 * scale).max(1.0);
                for block in &blocks {
                    let steps = ((block.width() / (4.0 * scale)).ceil() as usize * 2).max(4);
                    let pts: Vec<egui::Pos2> = (0..=steps)
                        .map(|i| {
                            let t = i as f32 / steps as f32;
                            pos2(
                                block.min.x + t * block.width(),
                                block.max.y - if i % 2 == 0 { amp * 0.6 } else { amp * 1.8 },
                            )
                        })
                        .collect();
                    painter.add(Shape::line(pts, Stroke::new(width, color)));
                }
            }
            AnnotationKind::Underline => {
                for block in &blocks {
                    painter.line_segment(
                        [pos2(block.min.x, block.max.y), pos2(block.max.x, block.max.y)],
                        Stroke::new(width, color),
                    );
                }
            }
            AnnotationKind::StrikeOut => {
                for block in &blocks {
                    painter.line_segment(
                        [
                            pos2(block.min.x, block.center().y),
                            pos2(block.max.x, block.center().y),
                        ],
                        Stroke::new(width, color),
                    );
                }
            }
            AnnotationKind::Ink => {
                for stroke in &annot.ink_points {
                    if stroke.len() < 2 {
                        continue;
                    }
                    let pts: Vec<egui::Pos2> =
                        stroke.iter().map(|p| to_screen(p[0], p[1])).collect();
                    painter.add(Shape::line(pts, Stroke::new(width, color)));
                }
            }
            AnnotationKind::Note => {
                painter.rect_filled(r, Rounding::same(RADIUS_TINY), fill);
                painter.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.0_f32, Color32::from_black_alpha(120)),
                );
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    "!",
                    FontId::proportional((font * 0.9).max(8.0)),
                    Color32::from_black_alpha(200),
                );
            }
            AnnotationKind::Stamp => {
                painter.rect_stroke(r, 3.0, Stroke::new((2.0 * scale).max(1.0), color));
                let lines: Vec<&str> = annot
                    .content
                    .lines()
                    .filter(|line| !line.is_empty())
                    .collect();
                let first = font.max(9.0);
                let leading = first * 1.15;
                let block = leading * lines.len() as f32;
                let top = r.center().y - block / 2.0 + leading / 2.0;
                for (index, line) in lines.iter().enumerate() {
                    let size = if index == 0 { first } else { first * 0.72 };
                    painter.text(
                        pos2(r.center().x, top + index as f32 * leading),
                        Align2::CENTER_CENTER,
                        *line,
                        FontId::proportional(size),
                        color,
                    );
                }
            }
            AnnotationKind::Circle => {
                painter.add(Shape::circle_stroke(
                    r.center(),
                    r.width().min(r.height()) / 2.0,
                    Stroke::new(width, color),
                ));
            }
            AnnotationKind::Polygon => {
                // Dessine le polygone à partir des points d'encre.
                let pts: Vec<egui::Pos2> = annot
                    .ink_points
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| to_screen(p[0], p[1]))
                    .collect();
                if pts.len() >= 3 {
                    // Ferme le polygone en reliant le dernier au premier.
                    let mut pts_closed = pts.clone();
                    pts_closed.push(pts[0]);
                    painter.add(Shape::convex_polygon(pts_closed, color, Stroke::NONE));
                }
            }
            // AnnotationKind::PolyLine => {
            //     // Dessine la polyligne ouverte.
            //     let pts: Vec<egui::Pos2> = annot
            //         .ink_points
            //         .first()
            //         .cloned()
            //         .unwrap_or_default()
            //         .iter()
            //         .map(|p| to_screen(p[0], p[1]))
            //         .collect();
            //     if pts.len() >= 2 {
            //         painter.add(Shape::line_strip(pts, Stroke::new(width, color)));
            //     }
            // }
            AnnotationKind::Line | AnnotationKind::Arrow => {
                let pts = annot.ink_points.first().cloned().unwrap_or_default();
                let (a, b) = match (pts.first(), pts.get(1)) {
                    (Some(a), Some(b)) => (to_screen(a[0], a[1]), to_screen(b[0], b[1])),
                    _ => (r.min, r.max),
                };
                painter.line_segment([a, b], Stroke::new(width, color));
                if annot.kind == AnnotationKind::Arrow {
                    let d = b - a;
                    let len = d.length().max(0.01);
                    let u = d / len;
                    let side = vec2(-u.y, u.x);
                    let head = (len * 0.2).clamp(6.0, 20.0);
                    painter.add(Shape::convex_polygon(
                        vec![b, b - u * head + side * head * 0.45, b - u * head - side * head * 0.45],
                        color,
                        Stroke::NONE,
                    ));
                }
            }
            AnnotationKind::Link => {
                painter.line_segment(
                    [pos2(r.min.x, r.max.y), pos2(r.max.x, r.max.y)],
                    Stroke::new(1.0_f32, color),
                );
            }
            _ => {
                painter.rect_stroke(r, 1.0, Stroke::new(width, color));
                if !annot.content.is_empty() {
                    painter.text(
                        r.left_top() + vec2(3.0, 2.0),
                        Align2::LEFT_TOP,
                        &annot.content,
                        FontId::proportional(font.max(8.0)),
                        color,
                    );
                }
            }
        }

        if selected == Some(idx) {
            painter.rect_stroke(r.expand(3.0), 2.0, Stroke::new(1.5_f32, pal().accent));
        }
    }
}
