//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel text_edit_ui).

use super::*;

impl YouPdfApp {

        /// Fenêtre de saisie du brouillon (texte, note, lien, signature).
        /// Extrait le texte des pages visibles, une fois par page.
        pub(super) fn ensure_fragments(&mut self) {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let total = tab.total() as u32;
            let current = tab.page as u32;
            let mut pages: Vec<u32> = page_layout_rows(self.page_layout, total, current)
                .into_iter()
                .find(|row| row.contains(&current))
                .unwrap_or_default();
            if pages.is_empty() {
                pages.push(current);
            }
            let missing: Vec<u32> = pages
                .iter()
                .copied()
                .filter(|page| !tab.fragments.contains_key(page))
                .collect();
            if missing.is_empty() {
                return;
            }
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            for page in missing {
                reader::extract_page_fragments(&*self.renderer, tab, page);
            }
        }

        /// Sélectionne tout le texte de la page courante.
        pub(super) fn select_page_text(&mut self) {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let page = tab.page as u32;
            // Le texte de la page peut ne pas avoir été extrait : c'est le cas
            // tant qu'aucun glissé n'a eu lieu dessus.
            if !tab.fragments.contains_key(&page) {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return;
                };
                if !reader::extract_page_fragments(&*self.renderer, tab, page) {
                    return;
                }
            }
            self.text_sel = Some(TextSel { page, span: None });
        }

        /// Copie le texte de la sélection dans le presse-papiers.
        pub(super) fn copy_selection(&mut self, ctx: &egui::Context) {
            let Some(sel) = self.text_sel else {
                return;
            };
            let text = self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.fragments.get(&sel.page))
                .map(|frags| hit_selection_text(frags, sel.span))
                .unwrap_or_default();
            if text.is_empty() {
                self.toast(ctx, "No text selected");
                return;
            }
            ctx.copy_text(text);
            self.toast(ctx, "Copied");
        }

        /// Ouvre la réécriture du bloc de texte touché.
        pub(super) fn edit_text_at(&mut self, page: usize, point: [f32; 2], ctx: &egui::Context) {
            self.ensure_fragments();
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let found = tab.fragments.get(&(page as u32)).and_then(|frags| {
                frags
                    .iter()
                    .find(|frag| {
                        let [x1, y1, x2, y2] = frag.rect;
                        point[0] >= x1 - 2.0
                            && point[0] <= x2 + 2.0
                            && point[1] >= y1 - 2.0
                            && point[1] <= y2 + 2.0
                    })
                    .cloned()
            });
            match found {
                Some(fragment) => {
                    // Un bloc vide ou retourné ne peut pas être réécrit.
                    let [x1, y1, x2, y2] = fragment.rect;
                    if x1 >= x2 || y1 >= y2 || fragment.text.is_empty() {
                        self.toast(ctx, "Invalid text block");
                        self.edit_text = None;
                        return;
                    }
                    self.edit_text = Some(EditTextDraft {
                        // Réécriture sur place : on part du texte existant, le
                        // curseur à la fin, comme dans un traitement de texte.
                        buffer: fragment.text.clone(),
                        style: TextStyle::from_fragment(&fragment),
                        fresh: true,
                        caret_at: Some(point[0]),
                        single_line: !fragment.text.contains('\n'),
                        fragment,
                    });
                }
                None => self.toast(ctx, "Click inside a text block"),
            }
        }

        /// Réécrit le texte du bloc en cours d'édition.
        pub(super) fn replace_fragment(&mut self, ctx: &egui::Context) {
            let Some(draft) = self.edit_text.take() else {
                return;
            };
            // Une retouche de style compte autant qu'une retouche de texte : sans
            // cela, changer la couleur ou le corps n'écrirait rien.
            if draft.buffer == draft.fragment.text && draft.style == TextStyle::from_fragment(&draft.fragment) {
                return;
            }
            // Couleur du fond sous le bloc : le patch écrit par le cœur doit se
            // fondre dans la page, pas poser un rectangle blanc.
            let bg = self.mask_color(draft.fragment.page, draft.fragment.rect);
            let replaced = match self.tabs.get_mut(self.active) {
                Some(tab) => {
                    // Le fragment vit en repère affiché, la réécriture attend le
                    // repère page.
                    let mut fragment = draft.fragment.clone();
                    fragment.rect = youpdf_core::geom::rect_to_page(&tab.doc, fragment.page, fragment.rect);
                    let background = Some([bg[0] as f32 / 255.0, bg[1] as f32 / 255.0, bg[2] as f32 / 255.0]);
                    tab.doc
                        .replace_text_fragment_styled(&fragment, &draft.buffer, background, &draft.style)
                }
                None => return,
            };
            match replaced {
                Ok(()) => {
                    self.mark_document_changed();
                    self.toast(ctx, "Text replaced");
                }
                Err(err) => self.toast(ctx, format!("Replace failed: {err}")),
            }
        }

        /// Texte utile de la requête, ou `None` si elle est vide.
        pub(super) fn search_needle(&self) -> Option<String> {
            let query = self.search_query.trim();
            (!query.is_empty()).then(|| query.to_owned())
        }

        /// Oublie les résultats et les surlignages, sans toucher à l'index.
        ///
        /// L'index reste valable tant que le document ne change pas : une requête
        /// vidée puis retapée ne doit pas relire toutes les pages.
        pub(super) fn clear_search_state(&mut self) {
            self.results.clear();
            self.search_active = None;
            self.search_rects.clear();
            self.search_rects_page = None;
        }

        /// Recherche explicite (Entrée, champ modifié, bouton) : la requête
        /// entre dans l'historique et le nombre de résultats est annoncé.
        pub(super) fn run_search(&mut self, ctx: &egui::Context) {
            // La recherche explicite couvre la frappe en attente.
            self.search_typed_at = None;
            let Some(query) = self.search_needle() else {
                self.clear_search_state();
                return;
            };
            let options = SearchOptions {
                case_sensitive: self.search_case,
                whole_word: self.search_word,
            };
            let Some(count) = self.refresh_search_results(ctx, &query, options, true) else {
                return;
            };
            self.history.retain(|q| q != &query);
            self.history.insert(0, query);
            self.history.truncate(8);
            self.toast(ctx, format!("{count} result(s)"));
        }

        /// Recalcule les résultats et les surlignages sans bruit.
        ///
        /// Utilisé pendant la saisie : ni historique, ni toast (`announce` les
        /// tait), et l'index déjà construit est réutilisé — seule la recherche
        /// elle-même est refaite. Renvoie le nombre de résultats, ou `None` si
        /// l'index a échoué.
        pub(super) fn refresh_search_results(
            &mut self,
            ctx: &egui::Context,
            query: &str,
            options: SearchOptions,
            announce: bool,
        ) -> Option<usize> {
            let tab = self.tabs.get(self.active)?;

            if self.search_index.as_ref().map(|(idx, _)| *idx) != Some(self.active) {
                match SearchIndex::build(&tab.doc) {
                    Ok(index) => self.search_index = Some((self.active, index)),
                    Err(err) => {
                        if announce {
                            self.toast(ctx, format!("Search index failed: {err}"));
                        }
                        return None;
                    }
                }
            }

            let Some((_, index)) = &self.search_index else {
                return None;
            };
            let mut results = index.search_with(query, 200, options);
            // Les signets et les commentaires ne vivent pas dans le texte des
            // pages : on les ajoute à la main quand les filtres les demandent.
            let synonyms = self.bookmark_comment_matches(query, options);
            results.extend(synonyms);
            let count = results.len();
            self.results = results;
            self.search_active = if count > 0 { Some(0) } else { None };
            self.focus_search_result();
            Some(count)
        }

        /// Passe au résultat suivant (`delta` = ±1) et y saute.
        pub(super) fn search_step(&mut self, delta: isize) {
            if self.results.is_empty() {
                return;
            }
            let count = self.results.len() as isize;
            let current = self.search_active.map(|idx| idx as isize).unwrap_or(-1);
            let next = (((current + delta) % count) + count) % count;
            self.search_active = Some(next as usize);
            self.focus_search_result();
        }

        /// Saute à la page du résultat actif et prépare son surlignage.
        pub(super) fn focus_search_result(&mut self) {
            let Some(result) = self
                .search_active
                .and_then(|idx| self.results.get(idx))
                .cloned()
            else {
                return;
            };
            let page = result.page;
            self.jump = Some(page);
            self.search_rects.clear();
            self.search_rects_page = Some(page as u32);
            let Some(needle) = self.search_needle() else {
                return;
            };
            let options = SearchOptions {
                case_sensitive: self.search_case,
                whole_word: self.search_word,
            };
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
                return;
            };
            for fragment in fragments {
                // Les fragments sortent du moteur en repère page ; les surlignages
                // se dessinent en repère affiché.
                let rect = youpdf_core::geom::rect_to_display(&tab.doc, page, fragment.rect);
                // Quand le moteur fournit une boîte par caractère, le
                // surlignage colle aux glyphes réels au lieu d'être
                // réparti proportionnellement sur la ligne entière.
                let to_display =
                    |r: [f32; 4]| youpdf_core::geom::rect_to_display(&tab.doc, page, r);
                let rects: Vec<[f32; 4]> = if fragment.chars.is_empty() {
                    match_rects(&fragment.text, rect, &needle, options)
                } else {
                    match_rects_with_chars(
                        &fragment.text,
                        &fragment.chars,
                        &needle,
                        options,
                        to_display,
                    )
                };
                self.search_rects.extend(rects);
            }
        }

        /// Remplace (ou supprime, `replacement` vide) toutes les correspondances.
        pub(super) fn replace_matches(&mut self, ctx: &egui::Context, replacement: &str) -> usize {
            let Some(query) = self.search_needle() else {
                return 0;
            };
            let options = SearchOptions {
                case_sensitive: self.search_case,
                whole_word: self.search_word,
            };
            let pages: std::collections::BTreeSet<usize> =
                self.results.iter().map(|result| result.page).collect();
            let mut changed = 0usize;
            {
                let Some(tab) = self.tabs.get_mut(self.active) else {
                    return 0;
                };
                for page in pages {
                    let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
                        continue;
                    };
                    for fragment in fragments {
                        let new_text = replace_occurrences(&fragment.text, &query, replacement, options);
                        if new_text == fragment.text {
                            continue;
                        }
                        // Même masque que la réécriture en place : la couleur du
                        // fond de la page quand elle est connue, blanc sinon.
                        let bg = mask_color_in(
                            tab,
                            page,
                            youpdf_core::geom::rect_to_display(&tab.doc, page, fragment.rect),
                        );
                        let background =
                            Some([bg[0] as f32 / 255.0, bg[1] as f32 / 255.0, bg[2] as f32 / 255.0]);
                        if tab
                            .doc
                            .replace_text_fragment(&fragment, &new_text, background)
                            .is_ok()
                        {
                            changed += 1;
                        }
                    }
                }
                if changed > 0 {
                    tab.dirty = true;
                    tab.structural_dirty = true;
                    tab.invalidate_render();
                }
            }
            if changed > 0 {
                self.search_index = None;
                self.run_search(ctx);
            }
            changed
        }

}
