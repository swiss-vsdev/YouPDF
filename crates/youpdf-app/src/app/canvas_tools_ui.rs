//! Outils d'annotation sur une page : rectangles, encres, callouts,
//! rogne, mesure, champs de formulaire.

use super::*;
use super::canvas_ui::CanvasFrame;


/// Applique l'outil courant à une page. Le dessin est différé dans
/// `frame` ; les états d'interaction (glissés, callout) vivent ici.
pub(super) fn page_tool_interaction(
    ui: &mut egui::Ui,
    tab: &mut Tab,
    frame: &mut CanvasFrame,
    page: u32,
    _rect: Rect,
    page_rect: Rect,
    scale: f32,
    resp: &egui::Response,
    action: ToolAction,
    markup_kind: Option<AnnotationKind>,
    mode: Option<usize>,
    annot_width: f32,
    eraser_mode: EraseMode,
    callout_pending: Option<CalloutPending>,
    ink_current: &mut Vec<[f32; 2]>,
    drag_from: &mut Option<[f32; 2]>,
    drag_to: &mut Option<[f32; 2]>,
) {
let to_pdf = |pos: egui::Pos2| -> [f32; 2] {
    [
        (pos.x - page_rect.min.x) / scale,
        (page_rect.max.y - pos.y) / scale,
    ]
};
let to_screen = |p: [f32; 2]| -> egui::Pos2 {
    pos2(
        page_rect.min.x + p[0] * scale,
        page_rect.max.y - p[1] * scale,
    )
};

// Interaction des outils d'annotation.
if action != ToolAction::None {
    // Outil Texte : chaque bloc est encadré, celui sous le
    // curseur plus franchement — c'est ce bloc que le clic
    // réécrira.
    if action == ToolAction::EditText {
        if let Some(frags) = tab.fragments.get(&page) {
            let hover = resp.hover_pos();
            for frag in frags {
                let [x1, y1, x2, y2] = frag.rect;
                let r = Rect::from_min_max(
                    to_screen([x1, y1]),
                    to_screen([x2, y2]),
                );
                let lit = hover.map(|p| r.contains(p)).unwrap_or(false);
                let stroke = if lit {
                    Stroke::new(1.5_f32, pal().accent)
                } else {
                    Stroke::new(
                        1.0_f32,
                        Color32::from_rgba_unmultiplied(0x7A, 0x7E, 0xF6, 70),
                    )
                };
                ui.painter().rect_stroke(r.expand(1.0), 2.0, stroke);
            }
        }
    }
    match action {
        // Une mise en évidence passe par la sélection de
        // texte ; seul un rectangle dessiné arrive ici.
        ToolAction::DragRect(_) if markup_kind.is_some() => {}
        ToolAction::DragRect(kind) => {
            if resp.dragged() {
                let point = resp.interact_pointer_pos().map(to_pdf);
                if drag_from.is_none() {
                    *drag_from = point;
                }
                if point.is_some() {
                    *drag_to = point;
                }
            }
            if resp.drag_stopped() {
                let from = drag_from.take();
                let to = drag_to.take();
                if let (Some(a), Some(b)) = (from, to) {
                    frame.pending_rect = Some((
                        page as usize,
                        kind,
                        [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[0].max(b[0]),
                            a[1].max(b[1]),
                        ],
                    ));
                }
            }
            if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                let r = Rect::from_two_pos(to_screen(a), to_screen(b));
                ui.painter().rect_stroke(
                    r,
                    2.0,
                    Stroke::new(1.5_f32, pal().accent),
                );
            }
        }
        ToolAction::Callout => {
            // Mode callout : la pose se fait dans le
            // gestionnaire d'événements plus bas. Ici, on
            // dessine ce qui est déjà posé, pour que les
            // trois clics se lisent comme une seule forme.
            if let Some(p) = callout_pending {
                if p.page == page as usize {
                    let knee = p.knee.unwrap_or(p.start);
                    let mut points = vec![to_screen(p.start), to_screen(knee)];
                    if let Some(corner) = p.text_corner {
                        points.push(to_screen(corner));
                    }
                    if points.len() > 1 {
                        ui.painter().add(Shape::line(
                            points.clone(),
                            Stroke::new(1.5_f32, pal().accent),
                        ));
                    }
                    for p in points {
                        ui.painter().circle_filled(
                            p,
                            4.0,
                            Color32::from_rgba_unmultiplied(
                                pal().accent.r(),
                                pal().accent.g(),
                                pal().accent.b(),
                                160,
                            ),
                        );
                    }
                }
            }
        }
        ToolAction::Ink => {
            if resp.drag_started() {
                ink_current.clear();
            }
            if resp.dragged() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let point = to_pdf(p);
                    // Deux points plus proches qu'un point
                    // PDF ne sont que du tremblement.
                    let far_enough = ink_current.last().map_or(true, |last| {
                        (point[0] - last[0]).hypot(point[1] - last[1]) >= 1.0
                    });
                    if far_enough {
                        ink_current.push(point);
                    }
                }
            }
            if resp.drag_stopped() {
                let smooth = smooth_polyline(ink_current, 4);
                ink_current.clear();
                frame.pending_ink = Some((page as usize, smooth));
            }
            if ink_current.len() > 1 {
                let pts: Vec<egui::Pos2> =
                    ink_current.iter().map(|p| to_screen(*p)).collect();
                ui.painter().add(Shape::line(
                    pts,
                    Stroke::new(annot_width * scale.max(0.4), pal().accent),
                ));
            }
        }
        ToolAction::Click(_) => {
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    frame.pending_click = Some((page as usize, to_pdf(p)));
                }
            }
        }
        ToolAction::EditText | ToolAction::InsertImage => {
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    let point = to_pdf(p);
                    if action == ToolAction::EditText {
                        frame.pending_edit = Some((page as usize, point));
                    } else {
                        frame.pending_image = Some((page as usize, point));
                    }
                }
            }
        }
        ToolAction::RedactRect => {
            if resp.dragged() {
                let point = resp.interact_pointer_pos().map(to_pdf);
                if drag_from.is_none() {
                    *drag_from = point;
                }
                if point.is_some() {
                    *drag_to = point;
                }
            }
            if resp.drag_stopped() {
                let from = drag_from.take();
                let to = drag_to.take();
                if let (Some(a), Some(b)) = (from, to) {
                    frame.pending_redact = Some((
                        page as usize,
                        [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[0].max(b[0]),
                            a[1].max(b[1]),
                        ],
                    ));
                }
            }
            if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                ui.painter().rect_filled(
                    rect,
                    Rounding::ZERO,
                    Color32::from_black_alpha(140),
                );
                ui.painter().rect_stroke(
                    rect,
                    Rounding::ZERO,
                    Stroke::new(1.5_f32, pal().redact_red),
                );
            }
        }
        ToolAction::Measure => {
            if resp.dragged() {
                let point = resp.interact_pointer_pos().map(to_pdf);
                if drag_from.is_none() {
                    *drag_from = point;
                }
                if point.is_some() {
                    *drag_to = point;
                }
            }
            if resp.drag_stopped() {
                let from = drag_from.take();
                let to = drag_to.take();
                if let (Some(a), Some(b)) = (from, to) {
                    // Maj contraint le tracé à l'axe
                    // dominant, comme une règle.
                    let (a, b) = if ui.input(|i| i.modifiers.shift) {
                        if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() {
                            (a, [b[0], a[1]])
                        } else {
                            (a, [a[0], b[1]])
                        }
                    } else {
                        (a, b)
                    };
                    frame.pending_measure = Some((page as usize, a, b));
                }
            }
            if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                ui.painter().rect_stroke(
                    rect,
                    Rounding::ZERO,
                    Stroke::new(1.5_f32, pal().accent),
                );
            }
        }
        ToolAction::FormField(kind) => {
            if resp.dragged() {
                let point = resp.interact_pointer_pos().map(to_pdf);
                if drag_from.is_none() {
                    *drag_from = point;
                }
                if point.is_some() {
                    *drag_to = point;
                }
            }
            if resp.drag_stopped() {
                let from = drag_from.take();
                let to = drag_to.take();
                if let (Some(a), Some(b)) = (from, to) {
                    let rect = [
                        a[0].min(b[0]),
                        a[1].min(b[1]),
                        a[0].max(b[0]),
                        a[1].max(b[1]),
                    ];
                    let (rect, _) = snap_form_rect(
                        rect,
                        &tab.forms,
                        page as usize,
                        6.0 / scale,
                    );
                    frame.pending_form = Some((page as usize, rect, kind));
                }
            }
            if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                let raw = [
                    a[0].min(b[0]),
                    a[1].min(b[1]),
                    a[0].max(b[0]),
                    a[1].max(b[1]),
                ];
                let (snapped, guides) =
                    snap_form_rect(raw, &tab.forms, page as usize, 6.0 / scale);
                let rect = Rect::from_two_pos(
                    pos2(
                        page_rect.min.x + snapped[0] * scale,
                        page_rect.max.y - snapped[3] * scale,
                    ),
                    pos2(
                        page_rect.min.x + snapped[2] * scale,
                        page_rect.max.y - snapped[1] * scale,
                    ),
                );
                ui.painter().rect_stroke(
                    rect,
                    Rounding::same(RADIUS_XS),
                    Stroke::new(1.5_f32, pal().accent),
                );
                // Guides d'alignement rouges.
                for (vertical, at) in guides {
                    let stroke =
                        Stroke::new(1.0_f32, Color32::from_rgb(220, 60, 60));
                    if vertical {
                        let x = page_rect.min.x + at * scale;
                        ui.painter().line_segment(
                            [
                                pos2(x, page_rect.min.y),
                                pos2(x, page_rect.max.y),
                            ],
                            stroke,
                        );
                    } else {
                        let y = page_rect.max.y - at * scale;
                        ui.painter().line_segment(
                            [
                                pos2(page_rect.min.x, y),
                                pos2(page_rect.max.x, y),
                            ],
                            stroke,
                        );
                    }
                }
            }
        }
        ToolAction::Erase => {
            if eraser_mode == EraseMode::Segment {
                if resp.drag_started() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        *drag_from = Some(to_pdf(p));
                        *drag_to = Some(to_pdf(p));
                    }
                }
                if resp.dragged() {
                    if let Some(p) = resp.interact_pointer_pos() {
                        *drag_to = Some(to_pdf(p));
                    }
                }
                if resp.drag_stopped() {
                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                        frame.pending_erase_segment = Some((page as usize, a, b));
                    }
                    *drag_from = None;
                    *drag_to = None;
                }
                // Aperçu du balayage de la gomme.
                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                    ui.painter().line_segment(
                        [to_screen(a), to_screen(b)],
                        Stroke::new(6.0_f32, pal().accent.gamma_multiply(0.5)),
                    );
                }
            } else if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    frame.pending_hit = Some((page as usize, to_pdf(p)));
                }
            }
        }
        ToolAction::Select => {
            // Mode Préparer un formulaire : un glissé
            // encadre plusieurs champs, un clic en vise un.
            if mode == Some(2) {
                if resp.dragged() {
                    let point = resp.interact_pointer_pos().map(to_pdf);
                    if drag_from.is_none() {
                        *drag_from = point;
                    }
                    if point.is_some() {
                        *drag_to = point;
                    }
                }
                if resp.drag_stopped() {
                    let from = drag_from.take();
                    let to = drag_to.take();
                    if let (Some(a), Some(b)) = (from, to) {
                        let rect = [
                            a[0].min(b[0]),
                            a[1].min(b[1]),
                            a[0].max(b[0]),
                            a[1].max(b[1]),
                        ];
                        frame.pending_form_select = Some((page as usize, rect));
                    }
                }
                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                    let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                    ui.painter().rect_filled(
                        rect,
                        Rounding::same(RADIUS_XS),
                        Color32::from_rgba_unmultiplied(70, 130, 220, 30),
                    );
                    ui.painter().rect_stroke(
                        rect,
                        Rounding::same(RADIUS_XS),
                        Stroke::new(1.0_f32, Color32::from_rgb(70, 130, 220)),
                    );
                }
            }
            if resp.clicked() {
                if let Some(p) = resp.interact_pointer_pos() {
                    frame.pending_hit = Some((page as usize, to_pdf(p)));
                }
            }
        }
        ToolAction::None => {
            // Mode lecture : aucun outil armé, le clic est un clic de
            // lecteur. Il ouvre un lien, remplit un champ de formula-
            // ire, ou sélectionne une annotation — tout cela passe par
            // `pending_hit`, comme l'outil Sélection. La sélection de
            // texte (glissé, double-clic) est traitée séparément, plus
            // bas, et n'est pas empêchée par ce clic.
            // `pending_link` déjà posé : la sélection de texte a reconnu
            // un lien sur ce clic, le reprendre ici l'ouvrirait deux fois.
            if resp.clicked() && frame.pending_link.is_none() {
                if let Some(p) = resp.interact_pointer_pos() {
                    frame.pending_hit = Some((page as usize, to_pdf(p)));
                }
            }
        }
    }
}
}
