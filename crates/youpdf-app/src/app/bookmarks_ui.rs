//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel bookmarks_ui).


use super::*;

impl YouPdfApp {

        /// Panneau Signets : plan du document.
        pub(super) fn bookmarks_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
            self.ensure_structure();
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let tree = tab.doc.outline_tree();

            let mut generate = false;
            let mut expand_all = false;
            let mut collapse_all = false;
            ui.add_space(SPACE_M);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui.small_button("Auto-Generate").clicked() {
                    generate = true;
                }
                if ui.small_button("Expand all").clicked() {
                    expand_all = true;
                }
                if ui.small_button("Collapse all").clicked() {
                    collapse_all = true;
                }
            });
            ui.add_space(SPACE_S);

            if generate {
                self.generate_bookmarks(ctx);
                return;
            }
            if expand_all {
                self.bookmark_collapsed.clear();
            }
            if collapse_all {
                self.bookmark_collapsed = bookmark_all_keys(&tree).into_iter().collect();
            }

            if tree.is_empty() {
                panel_hint(
                    ui,
                    "No bookmarks",
                    "Auto-Generate reads the text, or add one from the current page",
                );
                return;
            }

            let current_page = self.tabs[self.active].page;
            let mut rename_key = self.bookmark_rename.take();
            let mut rename_text = std::mem::take(&mut self.bookmark_rename_text);
            let mut request_focus = std::mem::take(&mut self.bookmark_rename_focus);
            let mut collapsed = std::mem::take(&mut self.bookmark_collapsed);
            let mut drag_from = self.bookmark_drag.clone();
            let mut drag_stopped = false;
            let mut hover_target: Option<(String, bool)> = None;
            let mut request: Option<BookmarkRequest> = None;
            let mut jump = None;

            let rows = bookmark_rows(&tree, &collapsed);
            let pointer = ctx.pointer_interact_pos();

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(SPACE_S);
                    for (key, title, page, depth, has_children, is_collapsed) in &rows {
                        let indent = 8.0 + (*depth as f32 * 12.0).min(60.0);
                        let (rect, resp) = ui.allocate_exact_size(
                            vec2((self.panel_w - 24.0).max(140.0), 24.0),
                            Sense::click_and_drag(),
                        );
                        if resp.hovered() {
                            ui.painter()
                                .rect_filled(rect, Rounding::same(RADIUS_M), pal().widget_bg);
                        }

                        // Chevron : replier/déplier ce signet.
                        let chevron_x = rect.min.x + indent;
                        if *has_children {
                            ui.painter().text(
                                pos2(chevron_x, rect.center().y),
                                Align2::LEFT_CENTER,
                                if *is_collapsed {
                                    ph::CARET_RIGHT
                                } else {
                                    ph::CARET_DOWN
                                },
                                FontId::proportional(LABEL),
                                pal().muted,
                            );
                        }
                        let text_x = chevron_x + 14.0;

                        let renaming = rename_key.as_deref() == Some(key.as_str());
                        if renaming {
                            let edit_rect = Rect::from_min_max(
                                pos2(text_x, rect.min.y + 1.0),
                                pos2(rect.max.x - 34.0, rect.max.y - 1.0),
                            );
                            let edit = ui.put(
                                edit_rect,
                                TextEdit::singleline(&mut rename_text)
                                    .font(FontId::proportional(BODY)),
                            );
                            if request_focus {
                                edit.request_focus();
                                request_focus = false;
                            }
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                rename_key = None;
                            } else if edit.lost_focus() {
                                // Le champ se ferme quoi qu'il arrive : sans cela,
                                // valider un titre inchangé le laisserait ouvert.
                                rename_key = None;
                                request =
                                    Some(BookmarkRequest::Rename(key.clone(), rename_text.clone()));
                            }
                        } else {
                            ui.painter().text(
                                pos2(text_x, rect.center().y),
                                Align2::LEFT_CENTER,
                                shorten(title, 30),
                                FontId::proportional(BODY),
                                pal().icon_bright,
                            );
                        }
                        if let Some(page) = page {
                            ui.painter().text(
                                pos2(rect.max.x - 8.0, rect.center().y),
                                Align2::RIGHT_CENTER,
                                format!("p.{page}"),
                                FontId::proportional(CAPTION),
                                pal().muted,
                            );
                        }

                        // Cible de dépôt : le bord droit fait du signet glissé un
                        // enfant, le reste en fait le frère suivant.
                        if drag_from.is_some() && pointer.map(|p| rect.contains(p)).unwrap_or(false) {
                            let as_child = pointer.map(|p| p.x > rect.max.x - 30.0).unwrap_or(false);
                            hover_target = Some((key.clone(), as_child));
                            ui.painter()
                                .rect_stroke(rect.shrink(1.0), 6.0, Stroke::new(1.5_f32, pal().accent));
                        }

                        if !renaming && resp.clicked() && !resp.double_clicked() {
                            let chevron_click = resp
                                .interact_pointer_pos()
                                .map(|p| *has_children && p.x < text_x)
                                .unwrap_or(false);
                            if chevron_click {
                                if *is_collapsed {
                                    collapsed.remove(key);
                                } else {
                                    collapsed.insert(key.clone());
                                }
                            } else {
                                jump = *page;
                            }
                        }
                        if !renaming && resp.double_clicked() {
                            rename_key = Some(key.clone());
                            rename_text = title.clone();
                            request_focus = true;
                        }
                        if resp.drag_started() {
                            drag_from = Some(key.clone());
                        }
                        if resp.drag_stopped() {
                            drag_stopped = true;
                        }
                        resp.context_menu(|ui| {
                            if ui.button(t("Add Bookmark")).clicked() {
                                request = Some(BookmarkRequest::Add(None, Some(current_page)));
                                ui.close_menu();
                            }
                            if ui.button(t("Add Sub-Bookmark")).clicked() {
                                request =
                                    Some(BookmarkRequest::Add(Some(key.clone()), Some(current_page)));
                                ui.close_menu();
                            }
                            if ui.button(t("Rename")).clicked() {
                                rename_key = Some(key.clone());
                                rename_text = title.clone();
                                request_focus = true;
                                ui.close_menu();
                            }
                            if ui.button(t("Set Destination to This Page")).clicked() {
                                request = Some(BookmarkRequest::SetDestination(
                                    key.clone(),
                                    Some(current_page),
                                ));
                                ui.close_menu();
                            }
                            if ui.button(t("Delete Bookmark")).clicked() {
                                request = Some(BookmarkRequest::Delete(key.clone()));
                                ui.close_menu();
                            }
                        });
                        ui.add_space(SPACE_XS);
                    }
                });

            // Fin du glissé : re-parent ou ré-ordonne.
            if drag_stopped {
                if let Some(from) = drag_from.take() {
                    if let Some((target, as_child)) = hover_target {
                        let from_path = bookmark_path(&from);
                        let target_path = bookmark_path(&target);
                        if target != from && !bookmark_is_ancestor(&from_path, &target_path) {
                            let moved = if as_child {
                                BookmarkRequest::Move {
                                    from,
                                    parent: Some(target),
                                    index: None,
                                }
                            } else {
                                let parent = if target_path.len() > 1 {
                                    Some(bookmark_key(&target_path[..target_path.len() - 1]))
                                } else {
                                    None
                                };
                                let index = target_path.last().map(|index| index + 1);
                                BookmarkRequest::Move {
                                    from,
                                    parent,
                                    index,
                                }
                            };
                            request = Some(moved);
                        }
                    }
                }
            }

            self.bookmark_rename = rename_key;
            self.bookmark_rename_text = rename_text;
            self.bookmark_rename_focus = request_focus;
            self.bookmark_collapsed = collapsed;
            self.bookmark_drag = drag_from;
            if let Some(page) = jump {
                self.jump = Some(page);
            }
            if let Some(request) = request {
                self.apply_bookmark_request(ctx, request);
            }
        }

        /// Applique une modification du plan (renommage, ajout, destination,
        /// suppression, déplacement) puis réécrit `/Outlines`.
        pub(super) fn apply_bookmark_request(&mut self, ctx: &egui::Context, request: BookmarkRequest) {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let mut tree = tab.doc.outline_tree();
            let mut changed = false;
            match request {
                BookmarkRequest::Rename(key, title) => {
                    if let Some(entry) = bookmark_at_mut(&mut tree, &bookmark_path(&key)) {
                        if entry.title != title {
                            entry.title = title;
                            changed = true;
                        }
                    }
                }
                BookmarkRequest::Delete(key) => {
                    changed = bookmark_take(&mut tree, &bookmark_path(&key)).is_some();
                }
                BookmarkRequest::Add(parent, page) => {
                    let entry = OutlineEntry {
                        title: "New bookmark".to_owned(),
                        page,
                        target: OutlineTarget::None,
                        children: Vec::new(),
                    };
                    match &parent {
                        Some(key) => {
                            if let Some(parent) = bookmark_at_mut(&mut tree, &bookmark_path(key)) {
                                parent.children.push(entry);
                                changed = true;
                            }
                        }
                        None => {
                            tree.push(entry);
                            changed = true;
                        }
                    }
                }
                BookmarkRequest::SetDestination(key, page) => {
                    if let Some(entry) = bookmark_at_mut(&mut tree, &bookmark_path(&key)) {
                        entry.page = page;
                        // La cible d'origine est remplacée : sans cela, la
                        // destination écrite resterait l'ancienne.
                        entry.target = OutlineTarget::None;
                        changed = true;
                    }
                }
                BookmarkRequest::Move {
                    from,
                    parent,
                    index,
                } => {
                    let from_path = bookmark_path(&from);
                    let parent_path = parent.as_deref().map(bookmark_path);
                    // Un signet ne peut pas devenir le descendant de lui-même.
                    let illegal = parent_path
                        .as_deref()
                        .map(|path| bookmark_is_ancestor(&from_path, path))
                        .unwrap_or(false);
                    if !illegal {
                        // Longueur de la liste d'accueil avant tout retrait : c'est
                        // aussi le point d'insertion quand `index` est absent.
                        let dest_len = match &parent_path {
                            Some(path) => bookmark_at(&tree, path).map(|node| node.children.len()),
                            None => Some(tree.len()),
                        };
                        if let (Some(from_index), Some(dest_len)) =
                            (from_path.last().copied(), dest_len)
                        {
                            let from_parent = &from_path[..from_path.len() - 1];
                            let same_list = match &parent_path {
                                Some(path) => path.as_slice() == from_parent,
                                None => from_parent.is_empty(),
                            };
                            // `index` compte dans la liste d'avant retrait ; retirer
                            // un frère situé avant elle décale la cible de un.
                            let at =
                                bookmark_insert_index(index, dest_len, same_list, from_index);
                            if let Some(entry) = bookmark_take(&mut tree, &from_path) {
                                // Le parent visé a pu se décaler lui aussi si le
                                // signet retiré le précédait dans son propre niveau.
                                let shifted = parent_path
                                    .as_deref()
                                    .map(|path| bookmark_shift_path(path, &from_path));
                                match &shifted {
                                    Some(path) => {
                                        if let Some(target) = bookmark_at_mut(&mut tree, path) {
                                            let at = at.min(target.children.len());
                                            target.children.insert(at, entry);
                                            changed = true;
                                        }
                                    }
                                    None => {
                                        let at = at.min(tree.len());
                                        tree.insert(at, entry);
                                        changed = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if !changed {
                return;
            }

            let result = self.tabs.get_mut(self.active).map(|tab| {
                let written = tab.doc.write_outline(&tree);
                if written.is_ok() {
                    tab.outline = tab.doc.outline();
                }
                written
            });
            match result {
                Some(Ok(_)) => {
                    self.mark_document_changed();
                    self.bookmark_rename = None;
                    self.bookmark_drag = None;
                    // Les chemins ont bougé : les replis mémorisés ne veulent plus
                    // rien dire.
                    self.bookmark_collapsed.clear();
                }
                Some(Err(err)) => self.toast(ctx, format!("Bookmark failed: {err}")),
                None => {}
            }
        }

}
