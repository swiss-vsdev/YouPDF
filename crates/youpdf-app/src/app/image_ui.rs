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
