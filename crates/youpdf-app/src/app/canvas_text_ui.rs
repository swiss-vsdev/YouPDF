//! Sélection de texte sur une page du canevas : glissé, double-clic,
//! menu contextuel, et inspecteur de la sélection.

use super::*;
use super::canvas_ui::CanvasFrame;


/// Gère la sélection de texte pour une page. Les conséquences
/// (copie, tout sélectionner, mise en évidence) sont différées dans
/// `frame`.
pub(super) fn page_text_select(
    ui: &mut egui::Ui,
    tab: &mut Tab,
    renderer: &dyn Renderer,
    frame: &mut CanvasFrame,
    page: u32,
    rect: Rect,
    page_rect: Rect,
    scale: f32,
    resp: &egui::Response,
    active: usize,
    mode: Option<usize>,
    action: ToolAction,
    markup_kind: Option<AnnotationKind>,
    text_drag: bool,
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

if text_drag {
    if resp.drag_started() {
        if !tab.fragments.contains_key(&page) {
            reader::extract_page_fragments(renderer, tab, page);
        }
        if let Some(point) = resp.interact_pointer_pos() {
            let point = to_pdf(point);
            let hit = tab
                .fragments
                .get(&page)
                .and_then(|frags| text_hit(frags, point));
            if let Some(hit) = hit {
                frame.sel = Some(TextSel {
                    page,
                    span: Some((hit, hit)),
                });
            }
        }
    } else if resp.dragged() {
        if let Some(point) = resp.interact_pointer_pos() {
            let point = to_pdf(point);
            let hit = tab
                .fragments
                .get(&page)
                .and_then(|frags| text_hit(frags, point));
            if let (Some(s), Some(hit)) = (frame.sel.as_mut(), hit) {
                if s.page == page {
                    if let Some((_, head)) = s.span.as_mut() {
                        *head = hit;
                    }
                }
            }
        }
    } else if resp.drag_stopped() {
        // Fin du glissé : avec une mise en évidence armée,
        // la sélection devient une annotation posée sur les
        // lignes touchées.
        if let Some(kind) = markup_kind {
            if let Some(frags) = tab.fragments.get(&page) {
                let quads =
                    hit_quads(frags, frame.sel.and_then(|s| s.span));
                if !quads.is_empty() {
                    frame.pending_markup =
                        Some((page as usize, kind, quads));
                }
            }
            frame.sel = None;
        }
    }
    if resp.clicked() {
        // Un clic sur un lien ouvre le lien : le geste
        // prime sur la sélection, sinon le lien serait
        // impossible à suivre.
        let link_point = resp.interact_pointer_pos().map(to_pdf);
        let link = link_point.and_then(|point| {
            let links = tab.doc.page_links(page as usize);
            link_at(&links, point).map(|link| link.action.clone())
        });
        if let Some(action) = link {
            frame.pending_link = Some(action);
            frame.sel = None;
        } else {
            // Un double-clic prend le mot sous le pointeur, un
            // triple-clic la ligne entière. Un clic simple
            // efface la sélection.
            let expand = if resp.triple_clicked() {
                Some(3)
            } else if resp.double_clicked() {
                Some(2)
            } else {
                None
            };
            match expand {
                Some(clicks) => {
                    if !tab.fragments.contains_key(&page) {
                        reader::extract_page_fragments(renderer, tab, page);
                    }
                    let point = resp.interact_pointer_pos().map(to_pdf);
                    let span = point
                        .and_then(|point| {
                            tab.fragments.get(&page).and_then(|frags| {
                                text_hit(frags, point).and_then(|hit| {
                                    if clicks == 2 {
                                        expand_to_word(frags, hit)
                                    } else {
                                        expand_to_line(frags, hit)
                                    }
                                })
                            })
                        });
                    frame.sel = span.map(|(start, end)| TextSel {
                        page,
                        span: Some((start, end)),
                    });
                }
                None => frame.sel = None,
            }
        }
    }
    let on_page = frame.sel.map(|s| s.page == page).unwrap_or(false);
    // Un glissé explicite ouvre l'inspecteur ; « tout
    // sélectionner » (sans ancre) couvrirait la page
    // entière et n'apprendrait rien.
    let explicit_sel = frame.sel.map(|s| s.span.is_some()).unwrap_or(false);
    if on_page && explicit_sel && markup_kind.is_none() {
        // Inspecteur de la sélection de texte : enveloppe
        // des blocs sélectionnés.
        if let Some(frags) = tab.fragments.get(&page) {
            let runs = hit_runs(frags, frame.sel.and_then(|s| s.span));
            let mut union: Option<Rect> = None;
            for (index, _, _) in runs.iter().copied() {
                let frag = &frags[index];
                let rect = Rect::from_two_pos(
                    to_screen([frag.rect[0], frag.rect[1]]),
                    to_screen([frag.rect[2], frag.rect[3]]),
                );
                union = Some(match union {
                    Some(previous) => previous.union(rect),
                    None => rect,
                });
            }
            if let Some(bounds) = union {
                let range = (
                    runs.first().map(|run| run.0).unwrap_or(0),
                    runs.last().map(|run| run.0).unwrap_or(0),
                );
                frame.inspector_target = Some(InspectorTarget::TextSelection {
                    page: page as usize,
                    range,
                    bounds,
                });
            }
        }
    }
    if on_page {
        resp.context_menu(|ui| {
            if ui.button(t("Copy")).clicked() {
                frame.sel_copy = true;
                ui.close_menu();
            }
            if ui.button(t("Select all")).clicked() {
                frame.sel_all = true;
                ui.close_menu();
            }
            // Les quatre mises en évidence se posent sur
            // la sélection courante : c'est le geste du
            // lecteur, qui sélectionne puis colore.
            if explicit_sel {
                for (kind, label) in MARKUP_KINDS {
                    if ui.button(label).clicked() {
                        frame.sel_markup = Some(kind);
                        ui.close_menu();
                    }
                }
            }
        });
    }
}
}
