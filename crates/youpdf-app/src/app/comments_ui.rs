//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel comments_ui).


use super::*;

impl YouPdfApp {

        /// Liste des commentaires : filtres, tri, réponses et export.
        pub(super) fn comments_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
            self.ensure_annots();
            self.ensure_comment_quotes();
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            if tab.annots.is_empty() {
                ui.add_space(14.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        section(t("No annotations yet")),
                    );
                    ui.add_space(SPACE_S);
                    ui.label(
                        RichText::new(t("Use Comment mode to add one"))
                            .size(LABEL)
                            .color(pal().muted),
                    );
                });
                return;
            }

            let panel_w = self.panel_w;
            let quotes = &self.comment_quotes;
            let now = chrono::Utc::now();

            // L'état de l'interface est relu en fin de fonction : les fermetures
            // de peinture ne peuvent pas emprunter `self`.
            let mut filter = self.comment_filter();
            let mut sort = self.comment_sort;
            let reply_open = self.comment_reply.clone();
            let mut reply_text = self.comment_reply_text.clone();
            let mut action: Option<CommentAction> = None;

            // Types, auteurs et couleurs réellement présents : proposer un filtre
            // qui ne peut rien retenir n'apporte rien.
            let mut kinds: Vec<AnnotationKind> = tab.annots.iter().map(|a| a.kind).collect();
            kinds.sort_by_key(|kind| kind.pdf_subtype());
            kinds.dedup();
            let mut authors: Vec<String> = tab.annots.iter().map(|a| a.author.clone()).collect();
            authors.sort();
            authors.dedup();
            let mut colors: Vec<[u8; 3]> = tab.annots.iter().map(|a| a.style.color).collect();
            colors.sort_unstable();
            colors.dedup();

            ui.horizontal(|ui| {
                ui.add_space(SPACE_S);
                ui.menu_button(RichText::new(t("Export Comments")).size(LABEL), |ui| {
                    for format in CommentFormat::ALL {
                        if ui.button(format.display_name()).clicked() {
                            action = Some(CommentAction::Export(format));
                            ui.close_menu();
                        }
                    }
                });
            });
            ui.add_space(SPACE_S);
            ui.horizontal(|ui| {
                ui.add_space(SPACE_S);
                ui.add_sized(
                    [panel_w - 32.0, 20.0],
                    TextEdit::singleline(&mut filter.query).hint_text("Search comments"),
                );
            });
            ui.add_space(SPACE_S);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(SPACE_S);
                egui::ComboBox::from_id_salt("comment-kind")
                    .width(70.0)
                    .selected_text(filter.kind.map_or("All types", annot_label))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(filter.kind.is_none(), t("All types"))
                            .clicked()
                        {
                            filter.kind = None;
                        }
                        for kind in &kinds {
                            if ui
                                .selectable_label(filter.kind == Some(*kind), annot_label(*kind))
                                .clicked()
                            {
                                filter.kind = Some(*kind);
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("comment-author")
                    .width(76.0)
                    .selected_text(filter.author.as_deref().unwrap_or("All authors"))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(filter.author.is_none(), t("All authors"))
                            .clicked()
                        {
                            filter.author = None;
                        }
                        for author in &authors {
                            if ui
                                .selectable_label(filter.author.as_deref() == Some(author.as_str()), author)
                                .clicked()
                            {
                                filter.author = Some(author.clone());
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("comment-color")
                    .width(74.0)
                    .selected_text(filter.color.map_or_else(|| "All colors".to_owned(), hex_color))
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(filter.color.is_none(), t("All colors"))
                            .clicked()
                        {
                            filter.color = None;
                        }
                        for color in &colors {
                            let label = RichText::new(format!("#{}", hex_color(*color))).color(Color32::from_rgb(
                                color[0], color[1], color[2],
                            ));
                            if ui
                                .selectable_label(filter.color == Some(*color), label)
                                .clicked()
                            {
                                filter.color = Some(*color);
                            }
                        }
                    });
                egui::ComboBox::from_id_salt("comment-state")
                    .width(78.0)
                    .selected_text(filter.state.display_name())
                    .show_ui(ui, |ui| {
                        for state in CommentStateFilter::ALL {
                            if ui
                                .selectable_label(filter.state == state, state.display_name())
                                .clicked()
                            {
                                filter.state = state;
                            }
                        }
                    });
            });
            ui.add_space(SPACE_S);
            ui.horizontal_wrapped(|ui| {
                ui.add_space(SPACE_S);
                ui.label(caption(t("Sort")));
                for (choice, name) in [
                    (CommentSort::Page, "Page"),
                    (CommentSort::Date, "Date"),
                    (CommentSort::Author, "Author"),
                ] {
                    if ui
                        .selectable_label(sort == choice, RichText::new(name).size(CAPTION))
                        .clicked()
                    {
                        sort = choice;
                    }
                }
            });
            ui.add_space(SPACE_XS);
            ui.separator();

            let rows = comment_rows(&tab.annots, &filter, sort);
            if rows.is_empty() {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(t("No comment matches"))
                            .size(LABEL)
                            .color(pal().muted),
                    );
                });
            }

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add_space(SPACE_S);
                    for row in &rows {
                        let card_w = (panel_w - 16.0 - row.depth as f32 * 14.0).max(80.0);
                        ui.horizontal(|ui| {
                            ui.add_space(4.0 + row.depth as f32 * 14.0);
                            ui.vertical(|ui| {
                                let response = ui
                                    .scope(|ui| {
                                        ui.set_width(card_w);
                                        ui.horizontal(|ui| {
                                            // Avatar : l'initiale de l'auteur.
                                            let (rect, _) = ui
                                                .allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                                            ui.painter().circle_filled(
                                                rect.center(),
                                                9.0,
                                                pal().widget_bg,
                                            );
                                            ui.painter().text(
                                                rect.center(),
                                                Align2::CENTER_CENTER,
                                                author_initial(&row.author),
                                                FontId::proportional(CAPTION),
                                                pal().icon_bright,
                                            );
                                            ui.vertical(|ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        RichText::new(&row.author).size(BODY).strong(),
                                                    );
                                                    ui.label(
                                                        RichText::new(relative_time(
                                                            &row.created_at,
                                                            now,
                                                        ))
                                                        .size(CAPTION)
                                                        .color(pal().muted),
                                                    );
                                                });
                                                ui.horizontal(|ui| {
                                                    let (swatch, _) = ui.allocate_exact_size(
                                                        vec2(8.0, 8.0),
                                                        Sense::hover(),
                                                    );
                                                    ui.painter().rect_filled(
                                                        swatch,
                                                        Rounding::same(RADIUS_XS),
                                                        Color32::from_rgb(
                                                            row.color[0],
                                                            row.color[1],
                                                            row.color[2],
                                                        ),
                                                    );
                                                    ui.label(
                                                        RichText::new(format!(
                                                            "{} · p.{}",
                                                            annot_label(row.kind),
                                                            row.page
                                                        ))
                                                        .size(CAPTION)
                                                        .color(pal().icon_bright),
                                                    );
                                                });
                                            });
                                        });
                                        // Texte visé par le surlignage.
                                        if let Some(quote) = quotes.get(&row.id) {
                                            if !quote.is_empty() && row.kind.is_markup() {
                                                ui.label(
                                                    RichText::new(format!("“{quote}”"))
                                                        .size(CAPTION)
                                                        .italics()
                                                        .color(pal().muted),
                                                );
                                            }
                                        }
                                        if !row.content.is_empty() {
                                            ui.label(RichText::new(&row.content).size(LABEL));
                                        }
                                        ui.horizontal(|ui| {
                                            ui.menu_button(
                                                RichText::new(format!(
                                                    "{} {}",
                                                    comment_state_icon(row.state),
                                                    row.state.display_name()
                                                ))
                                                .size(CAPTION)
                                                .color(comment_state_color(row.state)),
                                                |ui| {
                                                    for state in AnnotationState::ALL {
                                                        if ui
                                                            .selectable_label(
                                                                row.state == state,
                                                                state.display_name(),
                                                            )
                                                            .clicked()
                                                        {
                                                            action = Some(CommentAction::SetState(
                                                                row.id.clone(),
                                                                state,
                                                            ));
                                                            ui.close_menu();
                                                        }
                                                    }
                                                },
                                            );
                                            if ui
                                                .button(RichText::new(t("Reply")).size(CAPTION))
                                                .clicked()
                                            {
                                                action =
                                                    Some(CommentAction::Reply(row.id.clone()));
                                            }
                                        });
                                        if reply_open.as_deref() == Some(row.id.as_str()) {
                                            ui.horizontal(|ui| {
                                                let edit = ui.add_sized(
                                                    [(card_w - 56.0).max(60.0), 20.0],
                                                    TextEdit::singleline(&mut reply_text)
                                                        .hint_text("Reply…"),
                                                );
                                                // Entrée valide la saisie ; la
                                                // perte de focus seule (clic
                                                // ailleurs) n'envoie rien.
                                                let entered = edit.lost_focus()
                                                    && !reply_text.trim().is_empty()
                                                    && ui.input(|i| {
                                                        i.key_pressed(egui::Key::Enter)
                                                    });
                                                let send = ui
                                                    .button(RichText::new(t("Send")).size(CAPTION))
                                                    .clicked();
                                                if send || entered {
                                                    action = Some(CommentAction::Send(
                                                        row.id.clone(),
                                                        reply_text.clone(),
                                                    ));
                                                }
                                            });
                                        }
                                    })
                                    .response;
                                // Carte cliquable : saut vers la page visée.
                                let card = response.interact(Sense::click());
                                if card.clicked() && action.is_none() {
                                    action = Some(CommentAction::Jump(row.page));
                                }
                                card.context_menu(|ui| {
                                    if ui.button(t("Reply")).clicked() {
                                        action = Some(CommentAction::Reply(row.id.clone()));
                                        ui.close_menu();
                                    }
                                    if ui.button(t("Delete")).clicked() {
                                        action = Some(CommentAction::Delete(row.id.clone()));
                                        ui.close_menu();
                                    }
                                });
                            });
                        });
                        ui.add_space(SPACE_S);
                    }
                });

            // Les filtres et la réponse en cours repartent dans l'application…
            self.comment_search = filter.query.clone();
            self.comment_kind = filter.kind;
            self.comment_author = filter.author.clone();
            self.comment_color = filter.color;
            self.comment_state = filter.state;
            self.comment_sort = sort;
            self.comment_reply = reply_open;
            self.comment_reply_text = reply_text;
            // …puis l'action demandée a le dernier mot.
            match action {
                Some(CommentAction::Jump(page)) => self.jump = Some(page),
                Some(CommentAction::Delete(id)) => self.remove_comment_thread(&id, ctx),
                Some(CommentAction::SetState(id, state)) => self.set_comment_state(&id, state),
                Some(CommentAction::Reply(id)) => {
                    // Recliquer sur « Reply » referme la zone de saisie.
                    if self.comment_reply.as_deref() == Some(id.as_str()) {
                        self.comment_reply = None;
                    } else {
                        self.comment_reply = Some(id);
                    }
                    self.comment_reply_text.clear();
                }
                Some(CommentAction::Send(parent, text)) => self.send_comment_reply(&parent, &text, ctx),
                Some(CommentAction::Export(format)) => self.export_comments_dialog(format, ctx),
                None => {}
            }
        }

}

// Suite du panel comments_ui (extraite de `app.rs`).


impl YouPdfApp {

        /// Résultats synthétiques pour les signets et les commentaires.
        ///
        /// Actifs seulement si les filtres correspondants sont cochés : les
        /// titres de signets et le texte des commentaires ne font pas partie du
        /// texte des pages, donc le moteur ne les voit pas.
        pub(super) fn bookmark_comment_matches(
            &self,
            query: &str,
            options: SearchOptions,
        ) -> Vec<SearchResult> {
            let mut out = Vec::new();
            if !self.search_bookmarks && !self.search_comments {
                return out;
            }
            let Some(tab) = self.tabs.get(self.active) else {
                return out;
            };
            let needle = query.trim();
            if needle.is_empty() {
                return out;
            }

            if self.search_bookmarks {
                for item in tab.doc.outline() {
                    if let Some((start, end)) = find_query(&item.title, needle, options) {
                        out.push(SearchResult {
                            page: item.page.unwrap_or(1),
                            start,
                            end,
                            context: format!("Bookmark · {}", item.title),
                        });
                    }
                }
            }
            if self.search_comments {
                for annot in &tab.annots {
                    if annot.content.is_empty() {
                        continue;
                    }
                    if let Some((start, end)) = find_query(&annot.content, needle, options) {
                        out.push(SearchResult {
                            page: annot.page,
                            start,
                            end,
                            context: format!("Comment · {}", annot.content),
                        });
                    }
                }
            }
            out
        }

        /// Construit les filtres courants de la liste des commentaires.
        pub(super) fn comment_filter(&self) -> CommentFilter {
            CommentFilter {
                kind: self.comment_kind,
                author: self.comment_author.clone(),
                color: self.comment_color,
                state: self.comment_state,
                query: self.comment_search.clone(),
            }
        }

        /// Calcule une fois par document le texte cité sous chaque surlignage.
        pub(super) fn ensure_comment_quotes(&mut self) {
            let Self {
                tabs,
                comment_quotes,
                comment_quote_owner,
                active,
                ..
            } = self;
            if *comment_quote_owner != *active {
                comment_quotes.clear();
                *comment_quote_owner = *active;
            }
            let Some(tab) = tabs.get(*active) else {
                return;
            };
            // Un seul passage par page : l'extraction de texte coûte bien plus
            // cher que la comparaison de rectangles.
            let mut by_page: std::collections::BTreeMap<u32, Vec<(String, [f32; 4])>> =
                std::collections::BTreeMap::new();
            for annot in &tab.annots {
                if annot.reply_to.is_some()
                    || !annot.kind.is_markup()
                    || comment_quotes.contains_key(&annot.id)
                {
                    continue;
                }
                // Le rectangle est dans le repère affiché ; l'extraction de texte
                // travaille dans celui de la page.
                let rect = tab.doc.page_view(annot.page as u32).page_rect(annot.rect);
                by_page
                    .entry(annot.page as u32)
                    .or_default()
                    .push((annot.id.clone(), rect));
            }
            for (page, requests) in by_page {
                let rects: Vec<[f32; 4]> = requests.iter().map(|(_, rect)| *rect).collect();
                let texts = tab.doc.text_in_rects(page, &rects);
                for ((id, _), text) in requests.into_iter().zip(texts) {
                    comment_quotes.insert(id, text);
                }
            }
        }

        /// Supprime un commentaire et toute sa descendance de réponses.
        pub(super) fn remove_comment_thread(&mut self, id: &str, ctx: &egui::Context) {
            let doomed = match self.tabs.get_mut(self.active) {
                Some(tab) if tab.annots.iter().any(|annot| annot.id == id) => {
                    let before_state = tab.annots.clone();
                    tab.record_annots(before_state);
                    let before = tab.annots.len();
                    // Fermeture transitive par `reply_to` (HashSet : O(a + k)).
                    let doomed = youpdf_core::annotation::comments::remove_comment_thread(
                        &mut tab.annots,
                        id,
                    );
                    if tab.annots.len() != before {
                        tab.dirty = true;
                    }
                    doomed
                }
                _ => return,
            };
            if self.has_annot_selection {
                self.has_annot_selection = false;
            }
            for id in &doomed {
                self.comment_quotes.remove(id);
            }
            if self.comment_reply.as_deref() == Some(id) {
                self.comment_reply = None;
                self.comment_reply_text.clear();
            }
            let _ = ctx;
        }

        /// Applique un état de revue à un commentaire.
        pub(super) fn set_comment_state(&mut self, id: &str, state: AnnotationState) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(index) = tab.annots.iter().position(|annot| annot.id == id) else {
                return;
            };
            if tab.annots[index].state == state {
                return;
            }
            let before = tab.annots.clone();
            tab.record_annots(before);
            tab.annots[index].state = state;
            tab.dirty = true;
        }

        /// Ajoute une réponse à un commentaire.
        ///
        /// Une réponse est une note invisible sur la page : elle vit dans le fil,
        /// pas comme marque du document.
        pub(super) fn send_comment_reply(&mut self, parent: &str, text: &str, ctx: &egui::Context) {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            let Some((page, rect, color)) = self.tabs.get(self.active).and_then(|tab| {
                tab.annots
                    .iter()
                    .find(|annot| annot.id == parent)
                    .map(|annot| (annot.page, annot.rect, annot.style.color))
            }) else {
                return;
            };
            let mut reply = Annotation::new(page, AnnotationKind::Note, rect);
            reply.content = text.to_owned();
            reply.reply_to = Some(parent.to_owned());
            reply.style.color = color;
            self.push_annot(reply);
            self.comment_reply = None;
            self.comment_reply_text.clear();
            self.toast(ctx, "Reply added");
        }

        /// Exporte les commentaires affichés (filtres et tri compris).
        pub(super) fn export_comments_dialog(&mut self, format: CommentFormat, ctx: &egui::Context) {
            let filter = self.comment_filter();
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let rows = comment_rows(&tab.annots, &filter, self.comment_sort);
            let annots: Vec<Annotation> = rows
                .iter()
                .filter_map(|row| tab.annots.get(row.index).cloned())
                .collect();
            // Les rectangles en mémoire sont dans le repère affiché ; l'export
            // les veut dans celui de la page.
            let annots = annots_to_page(&tab.doc, &annots);
            let data = match export_comments(&annots, format) {
                Ok(data) => data,
                Err(err) => {
                    self.toast(ctx, format!("Export failed: {err}"));
                    return;
                }
            };
            let stem = self
                .active_stem()
                .unwrap_or_else(|| "comments".to_owned());
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(format.display_name(), &[format.extension()])
                .set_file_name(format!("{stem}-comments.{}", format.extension()))
                .save_file()
            {
                match std::fs::write(&path, data) {
                    Ok(()) => self.toast(ctx, "Comments exported"),
                    Err(err) => self.toast(ctx, format!("Export failed: {err}")),
                }
            }
        }

}
