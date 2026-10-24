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
