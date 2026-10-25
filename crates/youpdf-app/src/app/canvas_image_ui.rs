//! Image sélectionnée sur la page : cadre, poignées de rognage,
//! glissé, rotation et échelle.

use super::*;
use super::canvas_ui::CanvasFrame;


/// Dessine l'empilement de l'image sélectionnée et relève ses
/// interactions dans `frame` (mouvements, rognage).
pub(super) fn page_image_overlay(
    ui: &mut egui::Ui,
    tab: &mut Tab,
    frame: &mut CanvasFrame,
    page: u32,
    _rect: Rect,
    page_rect: Rect,
    scale: f32,
    _resp: &egui::Response,
    _mode: Option<usize>,
    _total: usize,
    crop_draft: Option<ImageCrop>,
    selected_image: &Option<SelectedImage>,
    content_drag: &mut Option<ContentDrag>,
    _inspector_hovered: bool,
) {
if let Some(image) = &selected_image {
    if image.page == page as usize {
        let [x1, y1, x2, y2] = image.rect;
        let mut box_screen = Rect::from_two_pos(
            pos2(
                page_rect.min.x + x1 * scale,
                page_rect.max.y - y2 * scale,
            ),
            pos2(
                page_rect.min.x + x2 * scale,
                page_rect.max.y - y1 * scale,
            ),
        );
        // Glisser-déposer pour déplacer l'image : la
        // matrice qui la dessine est modifiée au
        // relâchement, sans réencodage des pixels. Le
        // cadre suit le curseur pendant le glissé.
        if crop_draft.is_none() {
            let drag_resp = ui.interact(
                box_screen,
                Id::new("YouPDF image drag").with(page),
                Sense::drag(),
            );
            if drag_resp.drag_started() {
                if let Some(pos) = drag_resp.interact_pointer_pos() {
                    *content_drag = Some(ContentDrag::Move {
                        start: [pos.x, pos.y],
                        offset: [0.0, 0.0],
                    });
                }
            }
            let mut offset = [0.0_f32; 2];
            if let Some(ContentDrag::Move { start, offset: o }) =
                content_drag.as_mut()
            {
                if let Some(pos) = drag_resp.interact_pointer_pos() {
                    *o = [pos.x - start[0], pos.y - start[1]];
                }
                offset = *o;
                if drag_resp.drag_stopped() {
                    frame.pending_image_move =
                        Some((image.clone(), offset, scale));
                    *content_drag = None;
                }
            }
            if offset != [0.0, 0.0] {
                box_screen =
                    box_screen.translate(vec2(offset[0], offset[1]));
            }
        }
        ui.painter().rect_stroke(
            box_screen,
            Rounding::same(RADIUS_XS),
            Stroke::new(1.5_f32, pal().accent),
        );
        frame.inspector_target = Some(InspectorTarget::ImageElement {
            page: image.page,
            id: ImageId(image.index),
            bounds: box_screen,
        });
        // Poignées de redimensionnement sur les quatre coins :
        // un glissé étire la matrice (a, d) et déplace (e, f)
        // pour que le coin tenu reste collé au curseur. Les
        // pixels ne sont pas réencodés, la surface est
        // étirée comme dans le test d'engine.
        if crop_draft.is_none() {
            let mx1 = box_screen.min.x;
            let my1 = box_screen.max.y;
            let mx2 = box_screen.max.x;
            let my2 = box_screen.min.y;
            let mut scale_pending: Option<[f32; 6]> = None;
            for (handle, corner) in [
                ("YouPDF resize tl", pos2(mx1, my1)),
                ("YouPDF resize tr", pos2(mx2, my1)),
                ("YouPDF resize bl", pos2(mx1, my2)),
                ("YouPDF resize br", pos2(mx2, my2)),
            ] {
                let centre = ui.painter().round_pos_to_pixels(corner);
                let resp = ui.interact(
                    Rect::from_center_size(centre, vec2(12.0, 12.0)),
                    Id::new(handle),
                    Sense::drag(),
                );
                ui.painter().rect_filled(
                    Rect::from_center_size(centre, vec2(8.0, 8.0)),
                    Rounding::same(RADIUS_XS),
                    if resp.hovered() || resp.dragged() {
                        pal().accent
                    } else {
                        pal().icon_bright
                    },
                );
                if !resp.dragged() {
                    continue;
                }
                let Some(pointer) = resp.interact_pointer_pos() else {
                    continue;
                };
                let info = tab
                    .doc
                    .extract_images(image.page as u32)
                    .ok()
                    .and_then(|list| {
                        list.into_iter()
                            .find(|i| i.resource_name == image.name)
                    });
                let [a, b, c, d, e, f] = info.and_then(|i| {
                    tab.doc.image_matrix(&i).ok().flatten()
                }).unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                // La poignée fixe le coin qui reste collé au curseur :
                // la largeur et la hauteur se mesurent à partir de
                // ce coin, pas toujours du haut-gauche.
                let (px1, py1, px2, py2) = match handle {
                    "YouPDF resize tl" => (
                        box_screen.min.x,
                        box_screen.max.y,
                        box_screen.max.x,
                        box_screen.min.y,
                    ),
                    "YouPDF resize tr" => (
                        box_screen.max.x,
                        box_screen.max.y,
                        box_screen.min.x,
                        box_screen.min.y,
                    ),
                    "YouPDF resize bl" => (
                        box_screen.min.x,
                        box_screen.min.y,
                        box_screen.max.x,
                        box_screen.max.y,
                    ),
                    _ => (
                        box_screen.max.x,
                        box_screen.min.y,
                        box_screen.min.x,
                        box_screen.max.y,
                    ),
                };
                let nw = (pointer.x - px1).abs().max(0.001);
                let nh = (pointer.y - py1).abs().max(0.001);
                let sx = nw / (px2 - px1).abs().max(0.001);
                let sy = nh / (py2 - py1).abs().max(0.001);
                let na = a * sx;
                let nd = d * sy;
                let (ne, nf) = match handle {
                    "YouPDF resize tl" => (e, f + d - nd),
                    "YouPDF resize tr" => (e + a - na, f + d - nd),
                    "YouPDF resize bl" => (e, f),
                    _ => (e + a - na, f),
                };
                scale_pending = Some([na, b, c, nd, ne, nf]);
            }
            if let Some(matrix) = scale_pending {
                frame.pending_image_scale =
                    Some((image.clone(), matrix));
            }
            // Poignée de rotation au centre : un glissé
            // tourne l'image d'un angle libre autour de sa
            // boîte, sans réencodage des pixels. L'angle se
            // mesure entre le centre et le curseur ; chaque
            // trame ajoute son écart à l'angle précédent et
            // seul le total part au relâchement.
            if crop_draft.is_none() {
                let centre_px = ui
                    .painter()
                    .round_pos_to_pixels(box_screen.center());
                let ring_r = 10.0_f32;
                let ring = Rect::from_center_size(
                    centre_px,
                    vec2(ring_r * 2.0, ring_r * 2.0),
                );
                let rotate_resp = ui.interact(
                    ring,
                    Id::new("YouPDF image rotate").with(page),
                    Sense::drag(),
                );
                ui.painter().circle_stroke(
                    centre_px,
                    ring_r,
                    Stroke::new(1.5_f32, pal().accent),
                );
                let angle_at = |pos: egui::Pos2| {
                    // L'écran a le y vers le bas : atan2
                    // donne le sens horaire, comme la
                    // rotation de l'engine.
                    (pos.y - centre_px.y)
                        .atan2(pos.x - centre_px.x)
                        .to_degrees()
                };
                if rotate_resp.drag_started() {
                    if let Some(pos) =
                        rotate_resp.interact_pointer_pos()
                    {
                        *content_drag =
                            Some(ContentDrag::Rotate {
                                last_angle: angle_at(pos),
                                accumulated: 0.0,
                            });
                    }
                }
                if let Some(ContentDrag::Rotate {
                    last_angle,
                    accumulated,
                }) = content_drag.as_mut()
                {
                    if let Some(pos) =
                        rotate_resp.interact_pointer_pos()
                    {
                        let mut step =
                            angle_at(pos) - *last_angle;
                        // Le saut de ±180° au passage du
                        // demi-axe n'est pas une rotation.
                        if step > 180.0 {
                            step -= 360.0;
                        } else if step < -180.0 {
                            step += 360.0;
                        }
                        *accumulated += step;
                        *last_angle += step;
                    }
                    if rotate_resp.drag_stopped() {
                        if accumulated.abs() > 0.5 {
                            frame.pending_image_rotate = Some((
                                image.clone(),
                                *accumulated,
                            ));
                        }
                        *content_drag = None;
                    }
                }
            }
        }
        if let Some(crop) = crop_draft {
            // Zone conservée : les poignées vivent sur ses
            // coins, le reste de l'image est assombri.
            let kept = Rect::from_two_pos(
                pos2(
                    box_screen.min.x + crop.left * box_screen.width(),
                    box_screen.min.y + crop.top * box_screen.height(),
                ),
                pos2(
                    box_screen.max.x - crop.right * box_screen.width(),
                    box_screen.max.y - crop.bottom * box_screen.height(),
                ),
            );
            let shade = Color32::from_black_alpha(110);
            for outside in [
                Rect::from_min_max(box_screen.min, pos2(box_screen.max.x, kept.min.y)),
                Rect::from_min_max(pos2(box_screen.min.x, kept.max.y), box_screen.max),
                Rect::from_min_max(pos2(box_screen.min.x, kept.min.y), pos2(kept.min.x, kept.max.y)),
                Rect::from_min_max(pos2(kept.max.x, kept.min.y), pos2(box_screen.max.x, kept.max.y)),
            ] {
                ui.painter().rect_filled(outside, Rounding::ZERO, shade);
            }
            ui.painter().rect_stroke(
                kept,
                Rounding::ZERO,
                Stroke::new(1.0_f32, pal().accent),
            );
            let mut dragged = crop;
            for (handle, corner) in [
                ("YouPDF crop tl", kept.left_top()),
                ("YouPDF crop tr", kept.right_top()),
                ("YouPDF crop bl", kept.left_bottom()),
                ("YouPDF crop br", kept.right_bottom()),
            ] {
                let centre = ui.painter().round_pos_to_pixels(corner);
                let response = ui.interact(
                    Rect::from_center_size(centre, vec2(12.0, 12.0)),
                    Id::new(handle),
                    Sense::drag(),
                );
                ui.painter().rect_filled(
                    Rect::from_center_size(centre, vec2(8.0, 8.0)),
                    Rounding::same(RADIUS_XS),
                    if response.hovered() || response.dragged() {
                        pal().accent
                    } else {
                        pal().icon_bright
                    },
                );
                // Seul un glissé déplace la poignée : un
                // simple survol ne doit pas écrire dans
                // le document.
                if !response.dragged() {
                    continue;
                }
                let Some(pointer) = response.interact_pointer_pos() else {
                    continue;
                };
                let fx = ((pointer.x - box_screen.min.x)
                    / box_screen.width())
                .clamp(0.0, 1.0);
                let fy = ((pointer.y - box_screen.min.y)
                    / box_screen.height())
                .clamp(0.0, 1.0);
                match handle {
                    "YouPDF crop tl" => {
                        dragged.left = fx;
                        dragged.top = fy;
                    }
                    "YouPDF crop tr" => {
                        dragged.right = 1.0 - fx;
                        dragged.top = fy;
                    }
                    "YouPDF crop bl" => {
                        dragged.left = fx;
                        dragged.bottom = 1.0 - fy;
                    }
                    _ => {
                        dragged.right = 1.0 - fx;
                        dragged.bottom = 1.0 - fy;
                    }
                }
            }
            // Les bords opposés ne doivent pas se croiser.
            if dragged.left + dragged.right > 0.9 {
                let excess = dragged.left + dragged.right - 0.9;
                if dragged.left > dragged.right {
                    dragged.left -= excess;
                } else {
                    dragged.right -= excess;
                }
            }
            if dragged.top + dragged.bottom > 0.9 {
                let excess = dragged.top + dragged.bottom - 0.9;
                if dragged.top > dragged.bottom {
                    dragged.top -= excess;
                } else {
                    dragged.bottom -= excess;
                }
            }
            if response_changed(crop, dragged) {
                frame.crop_next = Some(dragged);
            }
        }
    }
}
}
