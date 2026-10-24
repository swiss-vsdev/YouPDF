//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel page_action_ui).


use super::*;

impl YouPdfApp {

        /// Applique l'action choisie dans le menu contextuel des vignettes.
        pub(super) fn run_page_action(&mut self, ctx: &egui::Context, active: usize, action: PageAction) {
            if self.tabs.is_empty() {
                return;
            }
            let total = self.tabs[active].total();
            // Pages visées : la sélection, ou la page affichée si rien n'est coché.
            let mut pages: Vec<usize> = self.thumb_selected.iter().copied().collect();
            if pages.is_empty() {
                pages = vec![self.tabs[active].page.clamp(1, total)];
            }
            let last = pages.iter().copied().max().unwrap_or(1);

            if let PageAction::Properties(page) = action {
                self.page_props = Some(page);
                return;
            }

            let doc = &mut self.tabs[active].doc;
            let outcome: Result<Option<String>, String> = match action {
                PageAction::Rotate(delta) => doc
                    .rotate_pages(&pages, delta)
                    .map(|count| Some(format!("{count} page(s) rotated")))
                    .map_err(|err| format!("Rotation failed: {err}")),
                PageAction::Blank(mut width, mut height) => {
                    if width <= 0.0 || height <= 0.0 {
                        let (dw, dh) = doc.page_size(last as u32);
                        width = dw;
                        height = dh;
                    }
                    doc.insert_blank_page_after(last, width, height)
                        .map(|_| Some(format!("Blank page inserted after page {last}")))
                        .map_err(|err| format!("Insert failed: {err}"))
                }
                PageAction::InsertFile => match rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .pick_file()
                {
                    Some(path) => doc
                        .insert_pages_after(last, &path)
                        .map(|count| Some(format!("{count} page(s) inserted after page {last}")))
                        .map_err(|err| format!("Insert failed: {err}")),
                    None => Ok(None),
                },
                PageAction::Extract => match rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_file_name("extract.pdf")
                    .save_file()
                {
                    Some(path) => doc
                        .extract_pages(&pages, &path)
                        .map(|_| {
                            Some(format!(
                                "{} page(s) extracted to {}",
                                pages.len(),
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ))
                        })
                        .map_err(|err| format!("Extract failed: {err}")),
                    None => Ok(None),
                },
                PageAction::Duplicate => doc
                    .duplicate_pages(&pages)
                    .map(|count| Some(format!("{count} page(s) duplicated")))
                    .map_err(|err| format!("Duplicate failed: {err}")),
                PageAction::Delete => {
                    if pages.len() >= total {
                        Err("Keep at least one page".to_owned())
                    } else {
                        doc.delete_pages(&pages)
                            .map(|count| Some(format!("{count} page(s) removed")))
                            .map_err(|err| format!("Delete failed: {err}"))
                    }
                }
                PageAction::Move(from, to) => doc
                    .move_page(from, to)
                    .map(|_| Some(format!("Page {from} moved to position {to}")))
                    .map_err(|err| format!("Move failed: {err}")),
                PageAction::Properties(_) => Ok(None),
            };

            let structural = matches!(
                action,
                PageAction::Rotate(_)
                    | PageAction::Blank(_, _)
                    | PageAction::InsertFile
                    | PageAction::Duplicate
                    | PageAction::Delete
                    | PageAction::Move(_, _)
            );
            let message = match outcome {
                Ok(message) => {
                    if structural && message.is_some() {
                        // Les numéros de page changent : la sélection ne suit plus.
                        self.thumb_selected.clear();
                        self.mark_document_changed();
                        if let Some(tab) = self.tabs.get_mut(active) {
                            let count = tab.doc.page_count().max(1);
                            if tab.page > count {
                                tab.page = count;
                            }
                        }
                    }
                    message
                }
                Err(message) => Some(message),
            };
            if let Some(message) = message {
                self.toast(ctx, message);
            }
        }

}
