//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel image_ui).


use super::*;

impl YouPdfApp {

        /// Applique une action de l'inspecteur d'image.
        pub(super) fn image_action(&mut self, action: ImageAction, ctx: &egui::Context) {
            let Some(selected) = self.selected_image.clone() else {
                return;
            };
            let info = self.tabs.get(self.active).and_then(|tab| {
                tab.doc
                    .extract_images(selected.page as u32)
                    .ok()
                    .and_then(|list| {
                        list.into_iter()
                            .find(|info| info.resource_name == selected.name)
                    })
            });
            let Some(info) = info else {
                self.selected_image = None;
                self.inspector_target = None;
                return;
            };
            match action {
                ImageAction::Replace => {
                    let Some(path) = rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg"])
                        .pick_file()
                    else {
                        return;
                    };
                    let (pixels, width, height) = match youpdf_core::edit::load_image_rgb(&path) {
                        Ok(decoded) => decoded,
                        Err(err) => {
                            self.toast(ctx, format!("Image failed: {err}"));
                            return;
                        }
                    };
                    let replaced = self
                        .tabs
                        .get_mut(self.active)
                        .map(|tab| tab.doc.replace_image_pixels(&info, &pixels, width, height));
                    match replaced {
                        Some(Ok(())) => {
                            self.mark_document_changed();
                            self.toast(ctx, "Image replaced");
                        }
                        Some(Err(err)) => self.toast(ctx, format!("Replace failed: {err}")),
                        None => {}
                    }
                }
                ImageAction::Extract => {
                    let Some(path) = rfd::FileDialog::new()
                        .set_file_name("image.png")
                        .save_file()
                    else {
                        return;
                    };
                    let png = self
                        .tabs
                        .get(self.active)
                        .map(|tab| tab.doc.extract_image_png(&info));
                    match png {
                        Some(Ok(bytes)) => match std::fs::write(&path, &bytes) {
                            Ok(()) => self.toast(ctx, "Image exported"),
                            Err(err) => self.toast(ctx, format!("Export failed: {err}")),
                        },
                        Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                        None => {}
                    }
                }
                ImageAction::Transform(transform) => {
                    let done = self
                        .tabs
                        .get_mut(self.active)
                        .map(|tab| tab.doc.transform_image(&info, transform));
                    match done {
                        Some(Ok(())) => self.mark_document_changed(),
                        Some(Err(err)) => self.toast(ctx, format!("Image failed: {err}")),
                        None => {}
                    }
                }
                ImageAction::StartCrop => {
                    self.image_crop = Some(ImageCrop::default());
                }
                ImageAction::ApplyCrop => {
                    let crop = self.image_crop.take().unwrap_or_default();
                    if crop.is_empty() {
                        return;
                    }
                    // Les poignées vivent sur la boîte **affichée** ; le cœur
                    // rogne selon les axes de l'image. Les deux repères diffèrent
                    // sur une page tournée, et plus encore après une rotation de
                    // l'image : on passe par la matrice de dessin.
                    let crop = self
                        .tabs
                        .get(self.active)
                        .and_then(|tab| {
                            let matrix = tab.doc.image_matrix(&info).ok().flatten()?;
                            let page_box = tab.doc.image_rect(&info).ok().flatten()?;
                            let view = tab.doc.page_view(selected.page as u32);
                            let [dx1, dy1, dx2, dy2] = view.display_rect(page_box);
                            let (dw, dh) = ((dx2 - dx1).max(1.0), (dy2 - dy1).max(1.0));
                            let kept = [
                                dx1 + crop.left * dw,
                                dy1 + crop.bottom * dh,
                                dx2 - crop.right * dw,
                                dy2 - crop.top * dh,
                            ];
                            let [kx1, ky1, kx2, ky2] = view.page_rect(kept);
                            Some(crop_fractions_in_image(matrix, [kx1, ky1, kx2, ky2]))
                        })
                        .unwrap_or(crop);
                    let done = self
                        .tabs
                        .get_mut(self.active)
                        .map(|tab| tab.doc.crop_image(&info, crop));
                    match done {
                        Some(Ok(())) => self.mark_document_changed(),
                        Some(Err(err)) => self.toast(ctx, format!("Crop failed: {err}")),
                        None => {}
                    }
                }
                ImageAction::CancelCrop => {
                    self.image_crop = None;
                }
                ImageAction::Opacity(alpha) => {
                    let done = self
                        .tabs
                        .get_mut(self.active)
                        .map(|tab| tab.doc.set_image_opacity(&info, alpha));
                    match done {
                        Some(Ok(())) => self.mark_document_changed(),
                        Some(Err(err)) => self.toast(ctx, format!("Opacity failed: {err}")),
                        None => {}
                    }
                }
                ImageAction::ZOrder(front) => {
                    let done = self
                        .tabs
                        .get_mut(self.active)
                        .map(|tab| tab.doc.set_image_z_order(&info, front));
                    match done {
                        Some(Ok(())) => self.mark_document_changed(),
                        Some(Err(err)) => self.toast(ctx, format!("Reorder failed: {err}")),
                        None => {}
                    }
                }
                ImageAction::Delete => {
                    let done = self
                        .tabs
                        .get_mut(self.active)
                        .map(|tab| tab.doc.delete_image(&info));
                    match done {
                        Some(Ok(())) => {
                            self.mark_document_changed();
                            self.toast(ctx, "Image deleted");
                        }
                        Some(Err(err)) => self.toast(ctx, format!("Delete failed: {err}")),
                        None => {}
                    }
                }
            }
            self.refresh_selected_image();
        }

}

// Suite du panel image_ui (extraite de `app.rs`).

use super::*;

impl YouPdfApp {

        /// Action du canevas selon le mode et l'outil choisi.
        pub(super) fn canvas_action(&self) -> ToolAction {
            // Tant qu'un bloc est ouvert à la réécriture, le canevas ne réagit
            // plus : les clics reviennent au champ posé sur la page.
            if self.edit_text.is_some() {
                return ToolAction::None;
            }
            if self.measure_active {
                return ToolAction::Measure;
            }
            if self.mode == Some(3) {
                return if self.annot_tool == Some(1) {
                    ToolAction::RedactRect
                } else {
                    ToolAction::None
                };
            }
            if self.mode == Some(1) {
                // Mode Édition : on touche au contenu de la page, pas aux
                // annotations.
                return match self.annot_tool {
                    // L'outil Sélection choisit le contenu à éditer : un bloc de
                    // texte, une image.
                    Some(0) => ToolAction::Select,
                    Some(1) => ToolAction::EditText,
                    Some(2) => ToolAction::InsertImage,
                    Some(3) => ToolAction::Click(AnnotationKind::Link),
                    _ => ToolAction::None,
                };
            }
            if self.mode == Some(2) {
                // Mode formulaire : chaque outil trace la boîte du champ. Les
                // types sans écriture côté cœur (bouton, code-barres, image,
                // séparateur) restent inertes et l'interface le dit.
                return match self.annot_tool {
                    Some(0) => ToolAction::Select,
                    Some(1) => ToolAction::FormField(FormKind::Text),
                    Some(2) => ToolAction::FormField(FormKind::Checkbox),
                    Some(3) => ToolAction::FormField(FormKind::Radio),
                    Some(4) => ToolAction::FormField(FormKind::Choice),
                    Some(5) => ToolAction::FormField(FormKind::MultiChoice),
                    Some(6) => ToolAction::FormField(FormKind::Button),
                    Some(7) => ToolAction::FormField(FormKind::Date),
                    Some(8) => ToolAction::FormField(FormKind::Barcode),
                    Some(9) => ToolAction::FormField(FormKind::Signature),
                    _ => ToolAction::None,
                };
            }
            if self.mode != Some(0) {
                return ToolAction::None;
            }
            match self.annot_tool {
                Some(0) => ToolAction::Select,
                Some(1) => ToolAction::DragRect(self.marker),
                Some(2) => ToolAction::Click(AnnotationKind::FreeText),
                Some(3) => ToolAction::Click(AnnotationKind::Note),
                Some(4) => ToolAction::Ink,
                Some(5) => ToolAction::DragRect(self.shape),
                Some(6) => ToolAction::InsertImage,
                Some(7) => ToolAction::Click(AnnotationKind::Link),
                Some(8) => ToolAction::Click(AnnotationKind::Stamp),
                Some(9) => ToolAction::Click(AnnotationKind::FreeText),
                Some(10) => ToolAction::Erase,
                Some(11) => ToolAction::Callout,
                _ => ToolAction::None,
            }
        }

        /// Demande une image puis la pose au point touché.
        pub(super) fn insert_image_at(&mut self, page: usize, point: [f32; 2], ctx: &egui::Context) {
            // Le clic arrive en repère affiché ; l'insertion se fait en repère page.
            let point = match self.tabs.get(self.active) {
                Some(tab) => youpdf_core::geom::point_to_page(&tab.doc, page, point),
                None => return,
            };
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Image", &["png", "jpg", "jpeg"])
                .pick_file()
            else {
                return;
            };
            let (pixels, width, height) = match youpdf_core::edit::load_image_rgb(&path) {
                Ok(decoded) => decoded,
                Err(err) => {
                    self.toast(ctx, format!("Image failed: {err}"));
                    return;
                }
            };
            // Taille : au plus deux cinquièmes de la page, proportions gardées.
            let (page_w, page_h) = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.page_size(page as u32))
                .unwrap_or((595.0, 842.0));
            let scale = (page_w * 0.4 / width as f32).min(page_h * 0.4 / height as f32);
            let half_w = width as f32 * scale / 2.0;
            let half_h = height as f32 * scale / 2.0;
            let rect = [
                point[0] - half_w,
                point[1] - half_h,
                point[0] + half_w,
                point[1] + half_h,
            ];
            let inserted = match self.tabs.get_mut(self.active) {
                Some(tab) => {
                    tab.doc
                        .insert_image_bitmap(page, rect, &pixels, width, height)
                }
                None => return,
            };
            match inserted {
                Ok(()) => {
                    self.mark_document_changed();
                    self.toast(ctx, "Image inserted");
                }
                Err(err) => self.toast(ctx, format!("Image failed: {err}")),
            }
        }

        /// Sélectionne l'image posée sous le point, s'il y en a une.
        ///
        /// L'image est retenue par son nom de ressource et sa boîte en repère
        /// page : le canevas la replace à l'écran à chaque trame, sans relire le
        /// flux de contenu.
        pub(super) fn select_image_at(&mut self, page: usize, point: [f32; 2]) {
            let found = self.tabs.get(self.active).and_then(|tab| {
                let images = tab.doc.extract_images(page as u32).ok()?;
                // `image_rect` est en repère page ; le clic arrive en repère
                // affiché. Sur une page tournée les deux ne coïncident pas.
                let view = tab.doc.page_view(page as u32);
                images.iter().enumerate().find_map(|(index, info)| {
                    let rect = view.display_rect(tab.doc.image_rect(info).ok().flatten()?);
                    let [x1, y1, x2, y2] = rect;
                    let inside = point[0] >= x1 && point[0] <= x2 && point[1] >= y1 && point[1] <= y2;
                    inside.then(|| SelectedImage {
                        page,
                        index,
                        name: info.resource_name.clone(),
                        rect,
                    })
                })
            });
            if found.is_some() {
                self.image_alpha = 1.0;
            }
            self.image_crop = None;
            self.selected_image = found;
        }

        /// Relit la boîte de l'image sélectionnée après une modification.
        pub(super) fn refresh_selected_image(&mut self) {
            let Some(selected) = self.selected_image.clone() else {
                return;
            };
            let info = self.tabs.get(self.active).and_then(|tab| {
                let view = tab.doc.page_view(selected.page as u32);
                tab.doc
                    .extract_images(selected.page as u32)
                    .ok()
                    .and_then(|list| {
                        list.into_iter()
                            .find(|info| info.resource_name == selected.name)
                    })
                    .map(|info| tab.doc.image_rect(&info).ok().flatten().map(|rect| view.display_rect(rect)))
            });
            match info {
                Some(Some(rect)) => {
                    if let Some(current) = self.selected_image.as_mut() {
                        current.rect = rect;
                    }
                }
                Some(None) => {}
                None => {
                    // L'image n'est plus sur la page : la cible n'a plus d'objet.
                    self.selected_image = None;
                    self.image_crop = None;
                    self.inspector_target = None;
                }
            }
        }

        /// Applique un glissé-déplacer sur l'image sélectionnée.
        ///
        /// L'offset est en coordonnées écran ; on le convertit en points de
        /// page (repère PDF, y vers le haut) avant d'écrire la matrice. Le geste
        /// ne réencode aucun pixel : la translation est appliquée à `(e, f)`.
        pub(super) fn commit_image_move(
            &mut self,
            selected: &SelectedImage,
            offset: [f32; 2],
            scale: f32,
            ctx: &egui::Context,
        ) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(info) = tab
                .doc
                .extract_images(selected.page as u32)
                .ok()
                .and_then(|list| list.into_iter().find(|i| i.resource_name == selected.name))
            else {
                return;
            };
            let dx = offset[0] / scale.max(0.001);
            let dy = -offset[1] / scale.max(0.001);
            let matrix = match tab.doc.image_matrix(&info) {
                Ok(Some(m)) => m,
                _ => [1.0, 0.0, 0.0, 1.0, dx, dy],
            };
            let mut moved = matrix;
            moved[4] += dx;
            moved[5] += dy;
            match tab.doc.set_image_matrix(&info, moved) {
                Ok(()) => {
                    self.mark_document_changed();
                    self.refresh_selected_image();
                    self.toast(ctx, "Image moved");
                }
                Err(err) => self.toast(ctx, format!("Move failed: {err}")),
            }
        }

        /// Applique un redimensionnement à l'image sélectionnée : la matrice
        /// `(a, d)` est étirée et `(e, f)` est déplacée pour que le coin tenu
        /// reste collé au curseur. Les pixels ne sont pas réencodés.
        pub(super) fn commit_image_scale(
            &mut self,
            selected: &SelectedImage,
            matrix: [f32; 6],
            ctx: &egui::Context,
        ) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(info) = tab
                .doc
                .extract_images(selected.page as u32)
                .ok()
                .and_then(|list| list.into_iter().find(|i| i.resource_name == selected.name))
            else {
                return;
            };
            match tab.doc.set_image_matrix(&info, matrix) {
                Ok(()) => {
                    self.mark_document_changed();
                    self.refresh_selected_image();
                    self.toast(ctx, "Image resized");
                }
                Err(err) => self.toast(ctx, format!("Resize failed: {err}")),
            }
        }

        /// Fait tourner l'image sélectionnée d'un angle libre, en degrés.
        ///
        /// L'angle est le sens horaire à l'écran, comme le geste de la poignée de
        /// rotation au centre de la boîte. Seule la matrice change : les pixels
        /// ne sont jamais réencodés, donc l'opération est réversible.
        pub(super) fn commit_image_rotate(
            &mut self,
            selected: &SelectedImage,
            degrees: f32,
            ctx: &egui::Context,
        ) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(info) = tab
                .doc
                .extract_images(selected.page as u32)
                .ok()
                .and_then(|list| list.into_iter().find(|i| i.resource_name == selected.name))
            else {
                return;
            };
            match tab.doc.rotate_image(&info, degrees) {
                Ok(()) => {
                    self.mark_document_changed();
                    self.refresh_selected_image();
                    self.toast(ctx, "Image rotated");
                }
                Err(err) => self.toast(ctx, format!("Rotate failed: {err}")),
            }
        }

        /// Déplace un bloc de texte en réécrivant son emplacement cible.
        ///
        /// L'offset écran est converti en points de page (y inversé) et ajouté
        /// au rectangle du fragment. Le texte, la police et la tailre sont
        /// conservés — seul l'emplacement change.
        pub(super) fn commit_text_block_move(
            &mut self,
            fragment: &TextFragment,
            offset: [f32; 2],
            scale: f32,
            ctx: &egui::Context,
        ) {
            let Some(tab) = self.tabs.get_mut(self.active) else { return };
            let dx = offset[0] / scale.max(0.001);
            let dy = -offset[1] / scale.max(0.001);
            let [x1, y1, x2, y2] = fragment.rect;
            let target = [x1 + dx, y1 + dy, x2 + dx, y2 + dy];
            let style = TextStyle::from_fragment(fragment);
            match tab.doc.replace_text_fragment_at(fragment, &fragment.text, target, None, &style) {
                Ok(()) => {
                    self.mark_document_changed();
                    self.toast(ctx, "Text block moved");
                }
                Err(err) => self.toast(ctx, format!("Move failed: {err}")),
            }
        }

}
