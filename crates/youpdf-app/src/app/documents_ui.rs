//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel documents_ui).

use super::*;

impl YouPdfApp {

        /// Ajoute un message au bandeau et au journal d'activité.
        pub(super) fn record(&mut self, text: String, now: f64) {
            self.toast = (text.clone(), now);
            let duplicate = matches!(self.activity.first(), Some((last, _)) if *last == text);
            if !duplicate {
                self.activity.insert(0, (text, now));
                self.activity.truncate(40);
            }
        }

        /// Ajoute un document déjà ouvert comme nouvel onglet actif.
        ///
        /// Les annotations YouPDF sortent du document pour la durée de l'édition :
        /// l'interface les dessine, l'onglet les garde, l'enregistrement les remet
        /// en place avec leur apparence. Sans cette purge, le moteur de rendu les
        /// peindrait en même temps que l'interface et chaque marque apparaîtrait
        /// deux fois.
        pub(super) fn push_document(&mut self, mut doc: Document, path: Option<PathBuf>) {
            let title = doc.display_name();
            let (initial_zoom, fit_page) = match self.ai.settings.default_zoom.as_str() {
                "fit-page" => (None, true),
                "actual" => (Some(1.0), false),
                "fit-width" => (None, false),
                other => match other.trim_end_matches('%').parse::<f32>() {
                    Ok(percent) if percent > 0.0 => (Some(percent / 100.0), false),
                    _ => (None, false),
                },
            };
            let protected = path
                .as_deref()
                .map(Document::file_is_encrypted)
                .unwrap_or(false);
            // Un scan se reconnaît avant que le document n'entre dans l'onglet.
            let scanned = doc.looks_scanned(1);
            let annots = annots_to_display(&doc, doc.read_annotations());
            if !annots.is_empty() && doc.clear_youpdf_annotations().is_err() {
                tracing::warn!("purge des annotations impossible");
            }
            let baseline_annots = annots.clone();
            let history_base = doc.to_bytes().ok();
            let saved_hash = history_base.as_deref().map(fingerprint).unwrap_or(0);
            self.tabs.push(Tab {
                doc,
                path: path.clone(),
                title: title.clone(),
                page: 1,
                zoom: initial_zoom,
                zoom_fit_page: fit_page,
                cache: HashMap::new(),
                page_sizes: Vec::new(),
                last_bitmap: None,
                dirty: false,
                structural_dirty: false,
                annots,
                baseline_annots,
                annots_loaded: true,
                undo: Vec::new(),
                redo: Vec::new(),
                history_base,
                saved_hash,
                outline: Vec::new(),
                forms: Vec::new(),
                attachments: Vec::new(),
                fragments: HashMap::new(),
                structure_loaded: false,
                pinned: false,
                protected,
            });
            self.active = self.tabs.len() - 1;
            self.scan_banner = scanned && !self.scan_banner_off;
            self.panel = Some(Panel::Thumbnails);
            self.home_open = false;
            // Le nouvel onglet devient actif tout de suite : sans cela, la trame
            // courante réutilise la session du document précédent.
            self.last_active = self.active;
            self.reset_document_session();
            if let Some(path) = path {
                self.recents.retain(|old| old != &path);
                self.recents.insert(0, path);
                self.recents.truncate(8);
                save_recents(&self.recents);
            }
            self.search_index = None;
            self.results.clear();
            self.jump = Some(1);

            if self.renderer.renders_real_content() {
                self.toast_owned(format!("Opened {title}"));
            } else {
                self.toast_owned(
                    "PDFium missing — pages render blank. Run scripts/install-pdfium.sh",
                );
            }
        }

        /// Tout ce qui appartient à la session ouverte sur un document : marques
        /// de caviardage, champ de formulaire, sélection, pages cochées,
        /// résultats de recherche. Changer d'onglet doit les remettre à zéro,
        /// sinon la suite de l'action s'applique au mauvais fichier.
        pub(super) fn reset_document_session(&mut self) {
            self.redact_marks.clear();
            self.redact_phase = RedactPhase::Marking;
            self.callout_pending = None;
            self.form_edit = None;
            self.selected_annot = 0;
            self.has_annot_selection = false;
            self.text_sel = None;
            self.inspector_target = None;
            self.inspector_size = None;
            self.inspector_text_ui = TextStyleUi::default();
            self.selected_image = None;
            self.image_crop = None;
            self.content_selection = None;
            self.content_drag = None;
            self.image_alpha = 1.0;
            self.organize_selected.clear();
            // Les clés de chemin des signets sont les mêmes d'un document à
            // l'autre : sans ce nettoyage, le nouveau plan s'ouvrirait en édition
            // avec le titre de l'ancien.
            self.bookmark_rename = None;
            self.bookmark_rename_text.clear();
            self.bookmark_rename_focus = false;
            self.bookmark_drag = None;
            self.bookmark_collapsed.clear();
            // La liste de commentaires repart de zéro : filtres, tri, citation et
            // réponse en cours visent tous le document précédent.
            self.comment_search.clear();
            self.comment_kind = None;
            self.comment_author = None;
            self.comment_color = None;
            self.comment_state = CommentStateFilter::All;
            self.comment_sort = CommentSort::Page;
            self.comment_reply = None;
            self.comment_reply_text.clear();
            self.comment_quotes.clear();
            self.comment_quote_owner = usize::MAX;
            self.thumb_owner = usize::MAX;
            self.replace_open = false;
            self.edit_text = None;
            self.text_draft = None;
            self.ink_current.clear();
            self.drag_from = None;
            self.drag_to = None;
            self.results.clear();
            self.search_index = None;
            self.search_rects.clear();
            self.search_rects_page = None;
            self.search_active = None;
            // Le contexte IA appartient au document : on le ré-extraira.
            self.ai_doc_tab = None;
            self.ai_doc_text.clear();
            // Les résultats d'extraction et l'outil de mesure appartiennent au
            // document : les garder ferait agir la fenêtre sur le mauvais onglet.
            self.extract_tables.clear();
            self.extract_pairs.clear();
            self.measure_active = false;
            self.batch_report.clear();
        }

        pub(super) fn undo(&mut self, ctx: &egui::Context) {
            self.step_history(ctx, true);
        }

        pub(super) fn redo(&mut self, ctx: &egui::Context) {
            self.step_history(ctx, false);
        }

        /// Dépile un état et le restaure, en poussant l'état courant sur la pile
        /// opposée. Une entrée structurelle remplace le document entier ; une
        /// entrée d'annotations ne touche que la liste en mémoire.
        pub(super) fn step_history(&mut self, ctx: &egui::Context, backward: bool) {
            let mut invalidate_search = false;
            let message = {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return;
                };
                let entry = if backward {
                    tab.undo.pop()
                } else {
                    tab.redo.pop()
                };
                let Some(HistoryEntry { doc, annots }) = entry else {
                    return;
                };
                let structural = doc.is_some();
                let current = HistoryEntry {
                    // Les octets courants ne valent que si l'on quitte un état
                    // structurel : sinon le document n'a pas bougé.
                    doc: if structural {
                        tab.history_base
                            .take()
                            .or_else(|| tab.doc.to_bytes().ok())
                    } else {
                        None
                    },
                    annots: tab.annots.clone(),
                };

                let restored = match doc {
                    Some(bytes) => match Document::from_bytes(&bytes) {
                        Ok(document) => {
                            tab.doc = document;
                            tab.annots = annots;
                            tab.invalidate_render();
                            tab.fragments.clear();
                            tab.structure_loaded = false;
                            tab.history_base = Some(bytes);
                            true
                        }
                        Err(_) => {
                            // Remettre l'entrée en place : sans cela un document
                            // illisible ferait disparaître un pas d'historique.
                            let back = HistoryEntry {
                                doc: Some(bytes),
                                annots,
                            };
                            if backward {
                                tab.undo.push(back);
                            } else {
                                tab.redo.push(back);
                            }
                            if tab.history_base.is_none() {
                                tab.history_base = current.doc.clone();
                            }
                            false
                        }
                    },
                    None => {
                        tab.annots = annots;
                        true
                    }
                };

                if restored {
                    if backward {
                        tab.redo.push(current);
                        trim_history(&mut tab.redo);
                    } else {
                        tab.undo.push(current);
                        trim_history(&mut tab.undo);
                    }
                    if structural {
                        let hash = tab
                            .history_base
                            .as_deref()
                            .map(fingerprint)
                            .unwrap_or_default();
                        tab.structural_dirty = hash != tab.saved_hash;
                        // Le document a été remplacé : l'index, les résultats et
                        // les surlignages décrivent l'état précédent.
                        invalidate_search = true;
                    }
                    tab.dirty = tab.structural_dirty || tab.annots != tab.baseline_annots;
                    tab.page = tab.page.clamp(1, tab.total().max(1));
                    Some(if backward { "Undone" } else { "Redone" })
                } else {
                    Some(if backward {
                        "Undo failed: document illisible"
                    } else {
                        "Redo failed: document illisible"
                    })
                }
            };
            if invalidate_search {
                self.search_index = None;
                self.clear_search_state();
            }
            if let Some(message) = message {
                self.toast(ctx, message);
            }
        }

        /// Lit le plan, les formulaires et les pièces jointes à la demande.
        pub(super) fn ensure_structure(&mut self) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if !tab.structure_loaded {
                    tab.outline = tab.doc.outline();
                    tab.forms = tab.doc.read_form_fields();
                    tab.attachments = tab.doc.attachments();
                    tab.structure_loaded = true;
                }
            }
        }

        /// À appeler après toute opération qui réécrit le document (pages,
        /// caviardage…) : caches de rendu, plan, formulaires, résultats de
        /// recherche et instantanés d'annulation deviennent caducs.
        pub(super) fn mark_document_changed(&mut self) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                // L'état d'avant l'édition est celui du dernier point de
                // restauration connu. Une édition sans image (document trop gros
                // ou lecture impossible) n'ajoute pas d'entrée : mieux vaut pas
                // d'annulation qu'une annulation fausse.
                let previous = tab.history_base.take();
                if let Some(bytes) = previous {
                    tab.undo.push(HistoryEntry {
                        doc: Some(bytes),
                        annots: tab.annots.clone(),
                    });
                    trim_history(&mut tab.undo);
                    tab.redo.clear();
                }
                tab.dirty = true;
                tab.structural_dirty = true;
                tab.invalidate_render();
                // Les annotations en attente ne vivent qu'en mémoire : les
                // marquer « à recharger » les remplacerait par celles du
                // document, c'est-à-dire par rien.
                tab.fragments.clear();
                tab.structure_loaded = false;
                tab.history_base = tab.doc.to_bytes().ok();
                tab.page = tab.page.clamp(1, tab.total().max(1));
            }
            self.search_index = None;
            self.clear_search_state();
        }

        /// Exécute une écriture qui part du document lui-même (copie compressée,
        /// copie chiffrée) en y appliquant d'abord les annotations en attente.
        ///
        /// Sans cela la copie écrite ne contiendrait pas les annotations de la
        /// session. Elles sont retirées du document juste après : l'affichage les
        /// dessine lui-même, et les garder dans le document les dessinerait deux
        /// fois.
        pub(super) fn with_annotations_applied<T>(
            &mut self,
            write: impl FnOnce(&mut Tab) -> T,
        ) -> Option<T> {
            let tab = self.tabs.get_mut(self.active)?;
            let page_annots = annots_to_page(&tab.doc, &tab.annots);
            if tab.doc.apply_annotations(&page_annots).is_err() {
                return None;
            }
            let outcome = write(tab);
            let _ = tab.doc.clear_youpdf_annotations();
            Some(outcome)
        }

        /// Applique le thème choisi : palette du chrome, puis réglages egui quand
        /// le thème a changé.
        pub(super) fn sync_theme(&mut self, ctx: &egui::Context) {
            set_palette(self.theme.palette());
            if self.theme != self.applied_theme {
                self.applied_theme = self.theme;
                apply_theme(ctx, self.theme);
            }
        }

}
