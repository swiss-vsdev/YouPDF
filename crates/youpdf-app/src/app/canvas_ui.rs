//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel canvas_ui).


use super::*;


/// Ce que la trame du canevas relève pendant le dessiné des pages,
/// et que `YouPdfApp::commit_canvas_frame` applique à la fin de la
/// trame. Les rectangles sont en repère page sauf mention contraire.
pub(super) struct CanvasFrame {
    pub(super) ctx: egui::Context,
    pub(super) action: ToolAction,
    pub(super) sel: Option<TextSel>,
    pub(super) sel_all: bool,
    pub(super) sel_copy: bool,
    pub(super) sel_markup: Option<AnnotationKind>,
    pub(super) form_draft: Option<(usize, String)>,
    pub(super) form_commit: bool,
    pub(super) form_cancel: bool,
    pub(super) edit_commit: bool,
    pub(super) edit_cancel: bool,
    pub(super) inspector_target: Option<InspectorTarget>,
    pub(super) crop_next: Option<ImageCrop>,
    pub(super) pending_text_move: Option<(TextFragment, [f32; 2], f32)>,
    pub(super) pending_image_move: Option<(SelectedImage, [f32; 2], f32)>,
    pub(super) pending_image_scale: Option<(SelectedImage, [f32; 6])>,
    pub(super) pending_image_rotate: Option<(SelectedImage, f32)>,
    pub(super) pending_rect: Option<(usize, AnnotationKind, [f32; 4])>,
    pub(super) pending_markup: Option<(usize, AnnotationKind, Vec<[f32; 8]>)>,
    pub(super) pending_ink: Option<(usize, Vec<[f32; 2]>)>,
    pub(super) pending_click: Option<(usize, [f32; 2])>,
    pub(super) pending_hit: Option<(usize, [f32; 2])>,
    pub(super) pending_erase_segment: Option<(usize, [f32; 2], [f32; 2])>,
    pub(super) pending_edit: Option<(usize, [f32; 2])>,
    pub(super) pending_image: Option<(usize, [f32; 2])>,
    pub(super) pending_redact: Option<(usize, [f32; 4])>,
    pub(super) pending_form: Option<(usize, [f32; 4], FormKind)>,
    pub(super) pending_form_select: Option<(usize, [f32; 4])>,
    pub(super) pending_measure: Option<(usize, [f32; 2], [f32; 2])>,
    pub(super) pending_link: Option<LinkAction>,
}

impl Default for CanvasFrame {
    fn default() -> Self {
        Self {
            ctx: egui::Context::default(),
            action: ToolAction::None,
            sel: None,
            sel_all: false,
            sel_copy: false,
            sel_markup: None,
            form_draft: None,
            form_commit: false,
            form_cancel: false,
            edit_commit: false,
            edit_cancel: false,
            inspector_target: None,
            crop_next: None,
            pending_text_move: None,
            pending_image_move: None,
            pending_image_scale: None,
            pending_image_rotate: None,
            pending_rect: None,
            pending_markup: None,
            pending_ink: None,
            pending_click: None,
            pending_hit: None,
            pending_erase_segment: None,
            pending_edit: None,
            pending_image: None,
            pending_redact: None,
            pending_form: None,
            pending_form_select: None,
            pending_measure: None,
            pending_link: None,
        }
    }
}

impl YouPdfApp {

        /// Fenêtre « Traitement par lots » : plusieurs fichiers, mêmes opérations.
        pub(super) fn canvas(&mut self, ui: &mut egui::Ui) {
            if self.tabs.is_empty() {
                self.empty_state(ui);
                return;
            }

            // Pincement / ⌘-molette = zoom (egui agrège les deux).
            let zoom_delta = ui.input(|i| i.zoom_delta());
            if (zoom_delta - 1.0).abs() > 0.0001 {
                self.zoom_dirty = true;
                self.zoom_by(zoom_delta);
            }
            // Horodatage du geste : il gèle la rastérisation, on étire le palier
            // voisin, puis le rendu net arrive une fois la main levée.
            if self.zoom_dirty {
                self.zoom_dirty = false;
                self.zoom_moved_at = ui.input(|i| i.time);
            }
            let now = ui.input(|i| i.time);
            if now - self.zoom_moved_at < ZOOM_SETTLE_SECS + 0.15 {
                // La trame nette est encore due : on la programme.
                ui.ctx().request_repaint();
            }
            let zoom_settled = now - self.zoom_moved_at >= ZOOM_SETTLE_SECS;
            // Défilement : même mécanique que le zoom. La molette et le
            // trackpad signalent le geste tout de suite ; le glissement de la
            // barre de défilement est repéré plus bas, d'après le mouvement des
            // pages. Tant que le geste dure, on rasterise à basse résolution.
            if ui.input(|i| i.raw_scroll_delta.length() > 0.0) {
                self.scroll_moved_at = now;
            }
            let scroll_settled = now - self.scroll_moved_at >= SCROLL_SETTLE_SECS;
            let fast = !zoom_settled || !scroll_settled;
            if fast {
                // La trame nette est encore due : on la programme.
                ui.ctx().request_repaint();
            }

            let active = self.active;
            let mut jump = self.jump.take();
            let ctx = ui.ctx().clone();
            let ppp = ctx.pixels_per_point();
            // Tous les modes passent par le même aiguillage : l'outil courant
            // décide, et un mode sans outil de canevas répond « rien ».
            let action = self.canvas_action();
            // L'outil Texte a besoin des blocs de la page visible : survol,
            // choix du bloc, réécriture.
            if action == ToolAction::EditText {
                self.ensure_fragments();
            }
            // Le bloc en cours de réécriture quitte `self` le temps de la trame :
            // le canevas emprunte déjà `self.tabs`, et le champ posé sur la page
            // doit pouvoir modifier son tampon pendant la boucle.
            let mut draft = self.edit_text.take();
            // Le glissé d'un élément de contenu est pris hors de `self` : la boucle
            // du canevas emprunte déjà les onglets et le moteur de rendu.
            let mut content_drag = self.content_drag.take();
            // Couleur du fond sous le bloc réécrit : lue avant que `self.tabs` ne
            // soit emprunté, `self` n'est plus accessible dans la boucle.
            let mask_bg = match draft.as_ref() {
                Some(d) => self.mask_color(d.fragment.page, d.fragment.rect),
                None => [255, 255, 255],
            };
            let mut frame = CanvasFrame {
                ctx: ctx.clone(),
                action,
                sel: self.text_sel,
                form_draft: self.form_edit.take(),
                ..Default::default()
            };
            // Sélection de texte : c'est l'état de lecture. Elle est active quand
            // aucun mode n'est ouvert, ou dans le mode Commentaire sans outil ou
            // avec l'outil Sélection. Aucun de ces états ne consomme le glissé.
            // Les autres modes et les outils qui dessinent gardent le leur.
            let select_text_tool = matches!(self.mode, None | Some(0))
                && matches!(self.annot_tool, None | Some(0));
            // Une mise en évidence se pose sur le texte : son glissé sélectionne,
            // il ne dessine pas de rectangle.
            let markup_kind = match action {
                ToolAction::DragRect(kind) if kind.is_markup() => Some(kind),
                _ => None,
            };
            let text_drag = select_text_tool || markup_kind.is_some();
            // Un clic suivi sur un lien est exécuté après la trame : l'action
            // touche le document et le focus, pas le canevas.
            let renderer = &*self.renderer;
            let status = &mut self.render_status;
            let last_fit = &mut self.last_fit;
            let page_layout = self.page_layout;
            let tab = &mut self.tabs[active];
            let total = tab.total();
            let sense = if action == ToolAction::None && !select_text_tool {
                Sense::click()
            } else {
                Sense::click_and_drag()
            };
            let annot_width = self.annot_width;
            let eraser_mode = self.eraser_mode;
            let ink_current = &mut self.ink_current;
            let drag_from = &mut self.drag_from;
            let drag_to = &mut self.drag_to;
            let redact_marks = &self.redact_marks;
            let callout_pending = self.callout_pending;
            let search_rects = &self.search_rects;
            let search_rects_page = self.search_rects_page;
            let selected_annot = self.selected_annot;
            let has_selection = self.has_annot_selection;
            let show_shadow = self.page_shadow;
            let tint = PAGE_TINTS[self.page_tint.min(PAGE_TINTS.len() - 1)].1;
            let tint_color = Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], tint[3]);
            let tint_on = tint[3] > 0;
            let show_border = self.page_border;
            // Le clic qui vise l'inspecteur ne doit pas valider la saisie en cours :
            // sans ce garde, le premier clic sur un réglage rendrait le focus du
            // champ, clavier compris, avant même d'avoir pu le régler.
            let inspector_hovered = self.inspector_target.as_ref().is_some_and(|target| {
                let size = self
                    .inspector_size
                    .unwrap_or(vec2(INSPECTOR_WIDTH, TEXT_INSPECTOR_HEIGHT));
                let rect = place_inspector(target.bounds(), ui.ctx().screen_rect(), size).rect;
                ui.ctx()
                    .pointer_latest_pos()
                    .is_some_and(|pos| rect.expand(6.0).contains(pos))
            });
            // L'inspecteur suit la sélection : la trame du canevas le calcule, la
            // barre d'outils (dessinée après) le pose.
            let mode = self.mode;
            let form_highlight = self.form_highlight;
            let form_editing = self.form_editing;
            let form_hover = self.form_hover;
            let form_selection = self.form_selection.clone();
            // L'image sélectionnée et le rognage en cours sont lus une fois : la
            // trame du canevas emprunte les onglets, elle ne peut pas toucher à
            // `self` en même temps.
            let selected_image = self.selected_image.clone();
            let crop_draft = self.image_crop;

            // Réserve pour les barres flottantes : les pages commencent sous la
            // barre d'outils, sinon la première page passe dessous.
            ui.add_space(CANVAS_TOP_GAP);
            // Position écran de la première page : sa variation d'une trame à
            // l'autre trahit un défilement que la molette n'a pas signalé
            // (barre de défilement tenue, inertie).
            let mut first_page_y: Option<f32> = None;

            ScrollArea::vertical()
                .auto_shrink([false, false])
                .show_viewport(ui, |ui, _viewport| {
                    let avail = ui.available_width().max(50.0);
                    let mut first_visible: Option<usize> = None;
                    let mut clicked_page: Option<usize> = None;

                    // Hauteur utile : la fenêtre moins deux marges de page.
                    let clip = ui.clip_rect();
                    let view_h = (clip.height() - PAGE_SPACING * 2.0).max(50.0);
                    // Géométrie : on répartit les pages en rangées, on alloue une
                    // ligne par rangée, puis on place les pages dedans. La
                    // disposition choisie décide combien de pages partagent une
                    // ligne ; « Une page » n'en garde que la page courante.
                    let mut placed: Vec<(u32, Rect, Rect, Id, f32)> = Vec::new();
                    for row_pages in &page_layout_rows(page_layout, total as u32, tab.page as u32) {
                        // Toutes les pages d'une rangée partagent la même échelle :
                        // sinon deux pages côte à côte n'auraient pas la même
                        // hauteur, et la lecture perdrait ses repères.
                        let sizes: Vec<(f32, f32)> = row_pages
                            .iter()
                            .map(|page| tab.page_size_cached(*page))
                            .collect();
                        let total_w: f32 = sizes.iter().map(|(w, _)| *w).sum();
                        let max_h = sizes.iter().map(|(_, h)| *h).fold(1.0_f32, f32::max);
                        let slots = row_pages.len() as f32;
                        let scale = tab
                            .zoom
                            .unwrap_or_else(|| {
                                let fit_width =
                                    (avail - PAGE_SPACING * (slots - 1.0)) / total_w.max(1.0);
                                if tab.zoom_fit_page {
                                    fit_width.min(view_h / max_h)
                                } else {
                                    fit_width
                                }
                            })
                            .clamp(0.05, 8.0);
                        if tab.zoom.is_none() {
                            *last_fit = scale;
                        }
                        let row_h = max_h * scale;
                        // La ligne occupe toute la largeur disponible : les pages
                        // sont centrées dedans, comme dans la référence. Sans cela,
                        // une page plus étroite que le canevas (zoom < ajustement)
                        // resterait collée à gauche.
                        let (row_rect, row) =
                            ui.allocate_exact_size(vec2(avail, row_h + PAGE_SPACING), Sense::hover());
                        let row_w = total_w * scale + PAGE_SPACING * (slots - 1.0);
                        let mut cursor_x = row_rect.min.x + ((avail - row_w) * 0.5).max(0.0);

                        for (slot, page) in row_pages.iter().enumerate() {
                            let (wp, hp) = sizes[slot];
                            let size = vec2(wp * scale, hp * scale);
                            let page_rect = Rect::from_min_size(
                                pos2(cursor_x, row_rect.min.y + (row_h - size.y) * 0.5),
                                size,
                            );
                            cursor_x += size.x + PAGE_SPACING;
                            // La zone de clic descend jusqu'au bas de la rangée :
                            // entre deux pages empilées, la bande d'espacement
                            // répond donc encore au clic.
                            let hit =
                                Rect::from_min_size(page_rect.min, vec2(size.x, size.y + PAGE_SPACING));
                            placed.push((*page, hit, page_rect, row.id, scale));
                        }
                    }
                    if let Some((_, _, page_rect, _, _)) = placed.first() {
                        first_page_y = Some(page_rect.min.y);
                    }

                    // Une seule page rastérisée par trame, et seulement une fois
                    // le zoom et le défilement au repos : la page courante
                    // d'abord, puis les autres pages visibles au fil des trames
                    // suivantes. La comparaison porte sur le palier *manquant ou
                    // trop grossier*, d'où `needs_render` sans négation.
                    let pending_render: Option<(u32, i32, f32)> = if zoom_settled && scroll_settled {
                        let mut visible: Vec<(u32, i32, f32)> = placed
                            .iter()
                            .filter(|(_, _, page_rect, _, _)| clip.intersects(*page_rect))
                            .map(|(page, _, _, _, scale)| (*page, render_step(*scale, ppp), *scale * ppp))
                            .collect();
                        visible.sort_by_key(|(page, _, _)| if *page == tab.page as u32 { 0 } else { 1 });
                        visible
                            .into_iter()
                            .find(|(page, step, target)| tab.needs_render(*page, *step, *target))
                    } else {
                        None
                    };

                    for (page, rect, page_rect, row_id, scale) in placed {
                        let size = page_rect.size();
                        let resp = ui.interact(rect, row_id.with(page), sense);

                        // Curseur de surlignage : l'outil de surbrillance est actif
                        // et le pointeur est sur la page. egui n'a pas de curseur
                        // « marqueur » natif, la croix de visée sert de surligneur.
                        // On interroge la position du pointeur et non
                        // `resp.hovered()` : pendant le glissé de surlignage, le
                        // widget n'est plus « survolé ».
                        if markup_kind.is_some() {
                            if let Some(pos) = ctx.pointer_hover_pos() {
                                if rect.contains(pos) {
                                    ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                                }
                            }
                        }

                        if jump == Some(page as usize) {
                            ui.scroll_to_rect(Rect::from_min_size(page_rect.min, size), Some(Align::Min));
                            jump = None;
                        }

                        // `viewport` est exprimé dans le repère du *contenu*
                        // (min = défilement courant) alors que `rect` est en
                        // coordonnées écran : les comparer directement saute
                        // des pages dès que l'on fait défiler. Le rectangle de
                        // découpe, lui, est bien en coordonnées écran.
                        if clip.intersects(rect) {
                            if first_visible.is_none() {
                                first_visible = Some(page as usize);
                            }

                            // Ombre douce sous la page.
                            if show_shadow {
                                ui.painter().rect_filled(
                                    page_rect.translate(vec2(0.0, 2.0)).expand(1.0),
                                    Rounding::same(RADIUS_TINY),
                                    Color32::from_black_alpha(70),
                                );
                            }

                            let step = render_step(scale, ppp);
                            let wanted = pending_render == Some((page, step, scale * ppp));
                            if let Some(tex) = Self::page_texture(
                                &ctx, renderer, tab, page, step, scale, ppp, wanted, fast, status,
                            ) {
                                ui.painter().image(
                                    tex.id(),
                                    page_rect,
                                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }

                            // Teinte et filet se posent sur la page rendue, sous
                            // tout ce que l'interface ajoute ensuite.
                            if tint_on {
                                ui.painter().rect_filled(page_rect, Rounding::ZERO, tint_color);
                            }
                            if show_border {
                                // Gris moyen plutôt que la couleur de séparation
                                // du thème : le filet doit se lire aussi bien sur
                                // une page blanche que contre un canevas sombre.
                                ui.painter().rect_stroke(
                                    page_rect,
                                    Rounding::ZERO,
                                    Stroke::new(
                                        1.0_f32,
                                        Color32::from_rgba_unmultiplied(0x80, 0x80, 0x88, 170),
                                    ),
                                );
                            }

                            // Sélection de texte en cours.
                            if let Some(s) = frame.sel.as_ref() {
                                if s.page == page {
                                    if let Some(frags) = tab.fragments.get(&page) {
                                        for [x1, y1, x2, y2] in hit_rects(frags, s.span) {
                                            let r = Rect::from_min_max(
                                                pos2(
                                                    page_rect.min.x + x1 * scale,
                                                    page_rect.max.y - y2 * scale,
                                                ),
                                                pos2(
                                                    page_rect.min.x + x2 * scale,
                                                    page_rect.max.y - y1 * scale,
                                                ),
                                            );
                                            ui.painter().rect_filled(
                                                r.expand(1.0),
                                                Rounding::same(RADIUS_XS),
                                                Color32::from_rgba_unmultiplied(
                                                    pal().accent.r(),
                                                    pal().accent.g(),
                                                    pal().accent.b(),
                                                    90,
                                                ),
                                            );
                                        }
                                    }
                                }
                            }

                            // Régions marquées pour caviardage et résultats
                            // de recherche actifs.
                            for region in redact_marks.iter().filter(|r| r.page == page as usize) {
                                let rect = Rect::from_two_pos(
                                    pos2(
                                        page_rect.min.x + region.rect[0] * scale,
                                        page_rect.max.y - region.rect[3] * scale,
                                    ),
                                    pos2(
                                        page_rect.min.x + region.rect[2] * scale,
                                        page_rect.max.y - region.rect[1] * scale,
                                    ),
                                );
                                // Phase 1 : le marquage est visible mais la page
                                // n'est pas encore caviardée. Le rouge signale
                                // « à supprimer », pas « déjà supprimé ».
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::ZERO,
                                    Color32::from_rgba_unmultiplied(
                                        pal().redact_red.r(),
                                        pal().redact_red.g(),
                                        pal().redact_red.b(),
                                        110,
                                    ),
                                );
                                ui.painter().rect_stroke(
                                    rect,
                                    Rounding::ZERO,
                                    Stroke::new(2.0_f32, pal().redact_red),
                                );
                            }
                            if search_rects_page == Some(page) {
                                for rect in search_rects {
                                    let rect = Rect::from_two_pos(
                                        pos2(
                                            page_rect.min.x + rect[0] * scale,
                                            page_rect.max.y - rect[3] * scale,
                                        ),
                                        pos2(
                                            page_rect.min.x + rect[2] * scale,
                                            page_rect.max.y - rect[1] * scale,
                                        ),
                                    );
                                    ui.painter().rect_filled(
                                        rect.expand(1.0),
                                        Rounding::same(RADIUS_XS),
                                        pal().search_match,
                                    );
                                }
                            }

                            // Annotations posées sur la page.
                            let selected = if has_selection {
                                Some(selected_annot)
                            } else {
                                None
                            };
                            draw_annots(ui.painter(), tab, page, page_rect, scale, selected);

                            // Champs de formulaire : teinte bleue quand le
                            // surlignage est actif, plus le champ en cours
                            // d'édition.
                            if form_highlight || form_editing.is_some() {
                                for (index, field) in tab.forms.iter().enumerate() {
                                    if field.page != Some(page as usize) || field.rect == [0.0; 4] {
                                        continue;
                                    }
                                    let [x1, y1, x2, y2] = field.rect;
                                    let rect = Rect::from_min_max(
                                        pos2(
                                            page_rect.min.x + x1.min(x2) * scale,
                                            page_rect.max.y - y1.max(y2) * scale,
                                        ),
                                        pos2(
                                            page_rect.min.x + x1.max(x2) * scale,
                                            page_rect.max.y - y1.min(y2) * scale,
                                        ),
                                    );
                                    let selected = form_selection.contains(&index);
                                    let active = form_editing == Some(index)
                                        || form_hover == Some(index)
                                        || selected;
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::same(RADIUS_XS),
                                        Color32::from_rgba_unmultiplied(
                                            70,
                                            130,
                                            220,
                                            if active { 70 } else { 34 },
                                        ),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        Rounding::same(RADIUS_XS),
                                        Stroke::new(
                                            if selected { 2.0 } else if active { 1.5 } else { 1.0_f32 },
                                            if selected {
                                                Color32::from_rgb(230, 120, 40)
                                            } else {
                                                Color32::from_rgb(70, 130, 220)
                                            },
                                        ),
                                    );
                                }
                            }

                            // Éditeur en place du champ de formulaire touché.
                            if let Some((idx, buffer)) = frame.form_draft.as_mut() {
                                if let Some(field) = tab.forms.get(*idx) {
                                    if field.page == Some(page as usize) && field.rect != [0.0; 4] {
                                        let [x1, y1, x2, y2] = field.rect;
                                        let rect = Rect::from_min_max(
                                            pos2(
                                                page_rect.min.x + x1.min(x2) * scale,
                                                page_rect.max.y - y1.max(y2) * scale,
                                            ),
                                            pos2(
                                                page_rect.min.x + x1.max(x2) * scale,
                                                page_rect.max.y - y1.min(y2) * scale,
                                            ),
                                        );
                                        ui.painter().rect_filled(
                                            rect.expand(1.0),
                                            Rounding::same(RADIUS_XS),
                                            Color32::WHITE,
                                        );
                                        ui.painter().rect_stroke(
                                            rect,
                                            Rounding::same(RADIUS_XS),
                                            Stroke::new(1.5_f32, pal().accent),
                                        );
                                        let single_line = !field.multiline;
                                        if matches!(
                                            field.kind,
                                            FormFieldKind::Choice | FormFieldKind::MultiChoice
                                        ) {
                                            egui::ComboBox::from_id_salt(("form-choice", *idx))
                                                .width(rect.width())
                                                .selected_text(if buffer.is_empty() {
                                                    "Select…".to_string()
                                                } else {
                                                    buffer.clone()
                                                })
                                                .show_ui(ui, |ui| {
                                                    for option in &field.options {
                                                        if ui
                                                            .selectable_label(buffer == option, option)
                                                            .clicked()
                                                        {
                                                            *buffer = option.clone();
                                                            frame.form_commit = true;
                                                        }
                                                    }
                                                });
                                            ui.painter().rect_stroke(
                                                rect,
                                                Rounding::same(RADIUS_XS),
                                                Stroke::new(1.5_f32, pal().accent),
                                            );
                                        } else {
                                            let edit = if single_line {
                                                TextEdit::singleline(buffer)
                                            } else {
                                                TextEdit::multiline(buffer)
                                            };
                                            let resp = ui.put(
                                                rect.shrink(3.0),
                                                edit.frame(false).desired_width(f32::INFINITY),
                                            );
                                            if ui.memory(|m| m.focused().is_none()) {
                                                resp.request_focus();
                                            }
                                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                                frame.form_cancel = true;
                                            } else if (single_line
                                                && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                                || (resp.lost_focus() && !inspector_hovered)
                                            {
                                                frame.form_commit = true;
                                            }
                                        }
                                    }
                                }
                            }
                            if mode == Some(0) {
                                if let Some(idx) = selected {
                                    if let Some(annot) = tab.annots.get(idx) {
                                        // Une réponse vit dans le fil, pas sur la
                                        // page : elle n'est pas dessinée, donc pas
                                        // d'inspecteur non plus.
                                        if annot.page == page as usize && annot.reply_to.is_none() {
                                            let screen = Rect::from_two_pos(
                                                pos2(
                                                    page_rect.min.x + annot.rect[0] * scale,
                                                    page_rect.max.y - annot.rect[3] * scale,
                                                ),
                                                pos2(
                                                    page_rect.min.x + annot.rect[2] * scale,
                                                    page_rect.max.y - annot.rect[1] * scale,
                                                ),
                                            );
                                            frame.inspector_target = Some(InspectorTarget::AnnotationElement {
                                                page: annot.page,
                                                id: AnnotationId(idx),
                                                bounds: screen,
                                            });
                                        }
                                    }
                                }
                            }

                            // Réécriture sur place : le bloc touché devient un
                            // champ posé sur la page. Entrée valide, Échap annule,
                            // un clic à côté valide aussi.
                            if let Some(d) = draft.as_mut() {
                                if d.fragment.page as u32 == page {
                                    let [x1, y1, x2, y2] = d.fragment.rect;
                                    let block = Rect::from_min_max(
                                        pos2(
                                            page_rect.min.x + x1 * scale,
                                            page_rect.max.y - y2 * scale,
                                        ),
                                        pos2(
                                            page_rect.min.x + x2 * scale,
                                            page_rect.max.y - y1 * scale,
                                        ),
                                    );
                                    // Barre de préhension au-dessus du champ : le
                                    // champ lui-même doit garder ses clics pour le
                                    // curseur, donc seul le bandeau déplace.
                                    let grip = Rect::from_min_size(
                                        pos2(block.min.x, block.min.y - 14.0),
                                        vec2(block.width(), 14.0),
                                    );
                                    let grip_resp = ui.interact(
                                        grip,
                                        Id::new("YouPDF text block drag").with(page),
                                        Sense::drag(),
                                    );
                                    if grip_resp.drag_started() {
                                        if let Some(pos) = grip_resp.interact_pointer_pos() {
                                            content_drag = Some(ContentDrag::Move {
                                                start: [pos.x, pos.y],
                                                offset: [0.0, 0.0],
                                            });
                                        }
                                    }
                                    let mut offset = [0.0_f32; 2];
                                    let mut move_pending: Option<(TextFragment, [f32; 2])> = None;
                                    if let Some(ContentDrag::Move { start, offset: o }) =
                                        content_drag.as_mut()
                                    {
                                        if let Some(pos) = grip_resp.interact_pointer_pos() {
                                            *o = [pos.x - start[0], pos.y - start[1]];
                                        }
                                        offset = *o;
                                        if grip_resp.drag_stopped() {
                                            move_pending =
                                                Some((d.fragment.clone(), offset));
                                            content_drag = None;
                                        }
                                    }
                                    let block = block.translate(vec2(offset[0], offset[1]));
                                    if let Some((fragment, off)) = move_pending {
                                        frame.pending_text_move =
                                            Some((fragment, off, scale));
                                    }
                                    // Un seul masque, au moins aussi grand que le
                                    // bloc d'origine *et* que le champ : sinon les
                                    // jambages dépassent au-dessus et en dessous,
                                    // et une réécriture plus longue que l'original
                                    // déborde du blanc. La couleur suit la page.
                                    let height =
                                        (block.height().max(d.style.size * scale) * 1.3)
                                            .max(20.0);
                                    let field = Rect::from_min_size(
                                        pos2(block.min.x - 3.0, block.center().y - height / 2.0),
                                        vec2(block.width().max(140.0) + 6.0, height),
                                    );
                                    let mask = block.expand(2.0).union(field);
                                    ui.painter().rect_filled(
                                        mask,
                                        Rounding::same(RADIUS_XS),
                                        Color32::from_rgb(mask_bg[0], mask_bg[1], mask_bg[2]),
                                    );
                                    ui.painter().rect_stroke(
                                        field,
                                        2.0,
                                        Stroke::new(1.0_f32, pal().accent),
                                    );
                                    ui.painter().rect_filled(
                                        grip.translate(vec2(offset[0], offset[1])),
                                        Rounding::same(RADIUS_XS),
                                        pal().accent,
                                    );
                                    let ink = Color32::from_rgb(
                                        d.style.color[0],
                                        d.style.color[1],
                                        d.style.color[2],
                                    );
                                    // Courier est la seule des trois familles que
                                    // l'interface sait rendre : egui n'embarque
                                    // pas de police à empattements.
                                    let font_id = match d.style.font {
                                        PatchFont::Courier => {
                                            FontId::monospace((d.style.size * scale).clamp(8.0, 96.0))
                                        }
                                        _ => {
                                            FontId::proportional((d.style.size * scale).clamp(8.0, 96.0))
                                        }
                                    };
                                    let edit_rect = field.shrink2(vec2(3.0, 0.0));
                                    // Un paragraphe (texte déjà coupé) se réécrit
                                    // en plusieurs lignes, Entrée y insère un saut ;
                                    // une ligne seule se valide avec Entrée.
                                    let single_line = d.single_line;
                                    let field_edit = if single_line {
                                        TextEdit::singleline(&mut d.buffer)
                                    } else {
                                        TextEdit::multiline(&mut d.buffer)
                                    };
                                    let resp = ui.put(
                                        edit_rect,
                                        field_edit
                                            .frame(false)
                                            .font(font_id.clone())
                                            .text_color(ink),
                                    );
                                    // Le curseur se pose entre les glyphes sous
                                    // le clic d'origine : le champ n'existait pas
                                    // encore à ce moment-là, egui ne peut pas le
                                    // placer seul.
                                    if let Some(x) = d.caret_at.take() {
                                        // Le curseur d'un paragraphe se place à
                                        // deux dimensions : ici on ne sait viser
                                        // qu'une abscisse, donc seulement une
                                        // ligne. `TextEdit` dessine son texte à sa
                                        // marge interne, 4 points à gauche, 2 en
                                        // haut.
                                        if single_line {
                                            let text_origin = edit_rect.min + vec2(4.0, 2.0);
                                            let galley = ui.fonts(|fonts| {
                                                fonts.layout_no_wrap(d.buffer.clone(), font_id, ink)
                                            });
                                            let local = vec2(
                                                (page_rect.min.x + x * scale - text_origin.x)
                                                    .clamp(0.0, galley.size().x),
                                                galley.size().y / 2.0,
                                            );
                                            let index = galley.cursor_from_pos(local).ccursor.index;
                                            let mut state = TextEdit::load_state(
                                                ui.ctx(),
                                                resp.id,
                                            )
                                            .unwrap_or_default();
                                            state.cursor.set_char_range(Some(
                                                egui::text_selection::CCursorRange::one(
                                                    egui::text::CCursor::new(index),
                                                ),
                                            ));
                                            state.store(ui.ctx(), resp.id);
                                        }
                                    }
                                    // Aperçu des traits : le texte écrit dans le
                                    // PDF les portera, autant les voir ici.
                                    if d.style.underline || d.style.strikeout {
                                        let rule = (d.style.size * scale * 0.07).max(0.5);
                                        if d.style.underline {
                                            ui.painter().line_segment(
                                                [
                                                    pos2(field.min.x, field.max.y - 3.0),
                                                    pos2(field.max.x, field.max.y - 3.0),
                                                ],
                                                Stroke::new(rule, ink),
                                            );
                                        }
                                        if d.style.strikeout {
                                            let y = field.center().y;
                                            ui.painter().line_segment(
                                                [pos2(field.min.x, y), pos2(field.max.x, y)],
                                                Stroke::new(rule, ink),
                                            );
                                        }
                                    }
                                    // Inspecteur du bloc en cours de réécriture.
                                    if mode == Some(1) {
                                        let block_id = tab
                                            .fragments
                                            .get(&page)
                                            .and_then(|frags| {
                                                frags.iter().position(|frag| {
                                                    frag.rect == d.fragment.rect
                                                        && frag.text == d.fragment.text
                                                })
                                            })
                                            .unwrap_or(0);
                                        frame.inspector_target =
                                            Some(InspectorTarget::TextElement {
                                                page: d.fragment.page,
                                                id: TextBlockId(block_id),
                                                bounds: field,
                                            });
                                    }
                                    if d.fresh {
                                        resp.request_focus();
                                        d.fresh = false;
                                    }
                                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        frame.edit_cancel = true;
                                    } else if (single_line
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                        || (resp.lost_focus() && !inspector_hovered)
                                    {
                                        frame.edit_commit = true;
                                    }
                                }
                            }
                        }

                        // Image sélectionnée : cadre, poignées de rognage, et
                        // inspecteur posé au-dessus.
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
                                            content_drag = Some(ContentDrag::Move {
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
                                            content_drag = None;
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
                                                content_drag =
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
                                                content_drag = None;
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

                        // Repères de la page : le même calcul sert aux outils
                        // d'annotation et à la sélection de texte.
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

                        // Sélection de texte : c'est l'état de lecture, donc elle
                        // ne dépend pas de l'outil. Elle est active quand aucun
                        // outil n'est armé, et avec l'outil Sélection du mode
                        // Commentaire. Les outils qui dessinent gardent leur
                        // glissé.
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

                        // Interaction des outils d'annotation.
                        if action != ToolAction::None {
                            // Outil Texte : chaque bloc est encadré, celui sous le
                            // curseur plus franchement — c'est ce bloc que le clic
                            // réécrira.
                            if action == ToolAction::EditText {
                                if let Some(frags) = tab.fragments.get(&page) {
                                    let hover = resp.hover_pos();
                                    for frag in frags {
                                        let [x1, y1, x2, y2] = frag.rect;
                                        let r = Rect::from_min_max(
                                            to_screen([x1, y1]),
                                            to_screen([x2, y2]),
                                        );
                                        let lit = hover.map(|p| r.contains(p)).unwrap_or(false);
                                        let stroke = if lit {
                                            Stroke::new(1.5_f32, pal().accent)
                                        } else {
                                            Stroke::new(
                                                1.0_f32,
                                                Color32::from_rgba_unmultiplied(0x7A, 0x7E, 0xF6, 70),
                                            )
                                        };
                                        ui.painter().rect_stroke(r.expand(1.0), 2.0, stroke);
                                    }
                                }
                            }
                            match action {
                                // Une mise en évidence passe par la sélection de
                                // texte ; seul un rectangle dessiné arrive ici.
                                ToolAction::DragRect(_) if markup_kind.is_some() => {}
                                ToolAction::DragRect(kind) => {
                                    if resp.dragged() {
                                        let point = resp.interact_pointer_pos().map(to_pdf);
                                        if drag_from.is_none() {
                                            *drag_from = point;
                                        }
                                        if point.is_some() {
                                            *drag_to = point;
                                        }
                                    }
                                    if resp.drag_stopped() {
                                        let from = drag_from.take();
                                        let to = drag_to.take();
                                        if let (Some(a), Some(b)) = (from, to) {
                                            frame.pending_rect = Some((
                                                page as usize,
                                                kind,
                                                [
                                                    a[0].min(b[0]),
                                                    a[1].min(b[1]),
                                                    a[0].max(b[0]),
                                                    a[1].max(b[1]),
                                                ],
                                            ));
                                        }
                                    }
                                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                        let r = Rect::from_two_pos(to_screen(a), to_screen(b));
                                        ui.painter().rect_stroke(
                                            r,
                                            2.0,
                                            Stroke::new(1.5_f32, pal().accent),
                                        );
                                    }
                                }
                                ToolAction::Callout => {
                                    // Mode callout : la pose se fait dans le
                                    // gestionnaire d'événements plus bas. Ici, on
                                    // dessine ce qui est déjà posé, pour que les
                                    // trois clics se lisent comme une seule forme.
                                    if let Some(p) = callout_pending {
                                        if p.page == page as usize {
                                            let knee = p.knee.unwrap_or(p.start);
                                            let mut points = vec![to_screen(p.start), to_screen(knee)];
                                            if let Some(corner) = p.text_corner {
                                                points.push(to_screen(corner));
                                            }
                                            if points.len() > 1 {
                                                ui.painter().add(Shape::line(
                                                    points.clone(),
                                                    Stroke::new(1.5_f32, pal().accent),
                                                ));
                                            }
                                            for p in points {
                                                ui.painter().circle_filled(
                                                    p,
                                                    4.0,
                                                    Color32::from_rgba_unmultiplied(
                                                        pal().accent.r(),
                                                        pal().accent.g(),
                                                        pal().accent.b(),
                                                        160,
                                                    ),
                                                );
                                            }
                                        }
                                    }
                                }
                                ToolAction::Ink => {
                                    if resp.drag_started() {
                                        ink_current.clear();
                                    }
                                    if resp.dragged() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            let point = to_pdf(p);
                                            // Deux points plus proches qu'un point
                                            // PDF ne sont que du tremblement.
                                            let far_enough = ink_current.last().map_or(true, |last| {
                                                (point[0] - last[0]).hypot(point[1] - last[1]) >= 1.0
                                            });
                                            if far_enough {
                                                ink_current.push(point);
                                            }
                                        }
                                    }
                                    if resp.drag_stopped() {
                                        let smooth = smooth_polyline(ink_current, 4);
                                        ink_current.clear();
                                        frame.pending_ink = Some((page as usize, smooth));
                                    }
                                    if ink_current.len() > 1 {
                                        let pts: Vec<egui::Pos2> =
                                            ink_current.iter().map(|p| to_screen(*p)).collect();
                                        ui.painter().add(Shape::line(
                                            pts,
                                            Stroke::new(annot_width * scale.max(0.4), pal().accent),
                                        ));
                                    }
                                }
                                ToolAction::Click(_) => {
                                    if resp.clicked() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            frame.pending_click = Some((page as usize, to_pdf(p)));
                                        }
                                    }
                                }
                                ToolAction::EditText | ToolAction::InsertImage => {
                                    if resp.clicked() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            let point = to_pdf(p);
                                            if action == ToolAction::EditText {
                                                frame.pending_edit = Some((page as usize, point));
                                            } else {
                                                frame.pending_image = Some((page as usize, point));
                                            }
                                        }
                                    }
                                }
                                ToolAction::RedactRect => {
                                    if resp.dragged() {
                                        let point = resp.interact_pointer_pos().map(to_pdf);
                                        if drag_from.is_none() {
                                            *drag_from = point;
                                        }
                                        if point.is_some() {
                                            *drag_to = point;
                                        }
                                    }
                                    if resp.drag_stopped() {
                                        let from = drag_from.take();
                                        let to = drag_to.take();
                                        if let (Some(a), Some(b)) = (from, to) {
                                            frame.pending_redact = Some((
                                                page as usize,
                                                [
                                                    a[0].min(b[0]),
                                                    a[1].min(b[1]),
                                                    a[0].max(b[0]),
                                                    a[1].max(b[1]),
                                                ],
                                            ));
                                        }
                                    }
                                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                        let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                        ui.painter().rect_filled(
                                            rect,
                                            Rounding::ZERO,
                                            Color32::from_black_alpha(140),
                                        );
                                        ui.painter().rect_stroke(
                                            rect,
                                            Rounding::ZERO,
                                            Stroke::new(1.5_f32, pal().redact_red),
                                        );
                                    }
                                }
                                ToolAction::Measure => {
                                    if resp.dragged() {
                                        let point = resp.interact_pointer_pos().map(to_pdf);
                                        if drag_from.is_none() {
                                            *drag_from = point;
                                        }
                                        if point.is_some() {
                                            *drag_to = point;
                                        }
                                    }
                                    if resp.drag_stopped() {
                                        let from = drag_from.take();
                                        let to = drag_to.take();
                                        if let (Some(a), Some(b)) = (from, to) {
                                            // Maj contraint le tracé à l'axe
                                            // dominant, comme une règle.
                                            let (a, b) = if ui.input(|i| i.modifiers.shift) {
                                                if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() {
                                                    (a, [b[0], a[1]])
                                                } else {
                                                    (a, [a[0], b[1]])
                                                }
                                            } else {
                                                (a, b)
                                            };
                                            frame.pending_measure = Some((page as usize, a, b));
                                        }
                                    }
                                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                        let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                        ui.painter().rect_stroke(
                                            rect,
                                            Rounding::ZERO,
                                            Stroke::new(1.5_f32, pal().accent),
                                        );
                                    }
                                }
                                ToolAction::FormField(kind) => {
                                    if resp.dragged() {
                                        let point = resp.interact_pointer_pos().map(to_pdf);
                                        if drag_from.is_none() {
                                            *drag_from = point;
                                        }
                                        if point.is_some() {
                                            *drag_to = point;
                                        }
                                    }
                                    if resp.drag_stopped() {
                                        let from = drag_from.take();
                                        let to = drag_to.take();
                                        if let (Some(a), Some(b)) = (from, to) {
                                            let rect = [
                                                a[0].min(b[0]),
                                                a[1].min(b[1]),
                                                a[0].max(b[0]),
                                                a[1].max(b[1]),
                                            ];
                                            let (rect, _) = snap_form_rect(
                                                rect,
                                                &tab.forms,
                                                page as usize,
                                                6.0 / scale,
                                            );
                                            frame.pending_form = Some((page as usize, rect, kind));
                                        }
                                    }
                                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                        let raw = [
                                            a[0].min(b[0]),
                                            a[1].min(b[1]),
                                            a[0].max(b[0]),
                                            a[1].max(b[1]),
                                        ];
                                        let (snapped, guides) =
                                            snap_form_rect(raw, &tab.forms, page as usize, 6.0 / scale);
                                        let rect = Rect::from_two_pos(
                                            pos2(
                                                page_rect.min.x + snapped[0] * scale,
                                                page_rect.max.y - snapped[3] * scale,
                                            ),
                                            pos2(
                                                page_rect.min.x + snapped[2] * scale,
                                                page_rect.max.y - snapped[1] * scale,
                                            ),
                                        );
                                        ui.painter().rect_stroke(
                                            rect,
                                            Rounding::same(RADIUS_XS),
                                            Stroke::new(1.5_f32, pal().accent),
                                        );
                                        // Guides d'alignement rouges.
                                        for (vertical, at) in guides {
                                            let stroke =
                                                Stroke::new(1.0_f32, Color32::from_rgb(220, 60, 60));
                                            if vertical {
                                                let x = page_rect.min.x + at * scale;
                                                ui.painter().line_segment(
                                                    [
                                                        pos2(x, page_rect.min.y),
                                                        pos2(x, page_rect.max.y),
                                                    ],
                                                    stroke,
                                                );
                                            } else {
                                                let y = page_rect.max.y - at * scale;
                                                ui.painter().line_segment(
                                                    [
                                                        pos2(page_rect.min.x, y),
                                                        pos2(page_rect.max.x, y),
                                                    ],
                                                    stroke,
                                                );
                                            }
                                        }
                                    }
                                }
                                ToolAction::Erase => {
                                    if eraser_mode == EraseMode::Segment {
                                        if resp.drag_started() {
                                            if let Some(p) = resp.interact_pointer_pos() {
                                                *drag_from = Some(to_pdf(p));
                                                *drag_to = Some(to_pdf(p));
                                            }
                                        }
                                        if resp.dragged() {
                                            if let Some(p) = resp.interact_pointer_pos() {
                                                *drag_to = Some(to_pdf(p));
                                            }
                                        }
                                        if resp.drag_stopped() {
                                            if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                                frame.pending_erase_segment = Some((page as usize, a, b));
                                            }
                                            *drag_from = None;
                                            *drag_to = None;
                                        }
                                        // Aperçu du balayage de la gomme.
                                        if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                            ui.painter().line_segment(
                                                [to_screen(a), to_screen(b)],
                                                Stroke::new(6.0_f32, pal().accent.gamma_multiply(0.5)),
                                            );
                                        }
                                    } else if resp.clicked() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            frame.pending_hit = Some((page as usize, to_pdf(p)));
                                        }
                                    }
                                }
                                ToolAction::Select => {
                                    // Mode Préparer un formulaire : un glissé
                                    // encadre plusieurs champs, un clic en vise un.
                                    if mode == Some(2) {
                                        if resp.dragged() {
                                            let point = resp.interact_pointer_pos().map(to_pdf);
                                            if drag_from.is_none() {
                                                *drag_from = point;
                                            }
                                            if point.is_some() {
                                                *drag_to = point;
                                            }
                                        }
                                        if resp.drag_stopped() {
                                            let from = drag_from.take();
                                            let to = drag_to.take();
                                            if let (Some(a), Some(b)) = (from, to) {
                                                let rect = [
                                                    a[0].min(b[0]),
                                                    a[1].min(b[1]),
                                                    a[0].max(b[0]),
                                                    a[1].max(b[1]),
                                                ];
                                                frame.pending_form_select = Some((page as usize, rect));
                                            }
                                        }
                                        if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                            let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                            ui.painter().rect_filled(
                                                rect,
                                                Rounding::same(RADIUS_XS),
                                                Color32::from_rgba_unmultiplied(70, 130, 220, 30),
                                            );
                                            ui.painter().rect_stroke(
                                                rect,
                                                Rounding::same(RADIUS_XS),
                                                Stroke::new(1.0_f32, Color32::from_rgb(70, 130, 220)),
                                            );
                                        }
                                    }
                                    if resp.clicked() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            frame.pending_hit = Some((page as usize, to_pdf(p)));
                                        }
                                    }
                                }
                                ToolAction::None => {
                                    // Le mode lecture n'a pas d'outil : le clic est
                                    // traité avec la sélection de texte, plus bas.
                                }
                            }
                        }

                        if resp.clicked() {
                            clicked_page = Some(page as usize);
                        }
                    }

                    if let Some(page) = clicked_page {
                        tab.page = page;
                    } else if let Some(page) = first_visible {
                        tab.page = page;
                    }

                    // La page demandée est à jour : la trame suivante s'occupe de
                    // la suivante. Si son rendu a échoué, `needs_render` reste vrai
                    // et on ne redemande rien — sinon on tournerait à vide.
                    if let Some((page, step, target)) = pending_render {
                        if !tab.needs_render(page, step, target) {
                            ctx.request_repaint();
                        }
                    }

                    // ponytail: cache borné par nettoyage grossier, LRU plus tard.
                    if tab.cache.len() > 40 {
                        let current = tab.page as i64;
                        tab.cache
                            .retain(|key, _| (key.0 as i64 - current).abs() < 10);
                    }
                });

            // Le contenu a glissé : la première page n'est plus à la même place.
            // Cela couvre le défilement à la barre, que la molette ne signale pas.
            if let (Some(y), Some(last_y)) = (first_page_y, self.last_page_y) {
                if (y - last_y).abs() > 0.5 {
                    self.scroll_moved_at = now;
                }
            }
            self.last_page_y = first_page_y;

            self.edit_text = draft;
            self.content_drag = content_drag;
            self.commit_canvas_frame(frame);
        }

}

// Suite du panel canvas_ui (extraite de `app.rs`).


impl YouPdfApp {

        /// Exécute l'action d'un lien et prévient l'utilisateur du résultat.
        ///
        /// Une adresse part dans le navigateur du système, un saut interne change
        /// de page, une action inconnue est dite telle : mieux vaut l'annoncer que
        /// laisser croire que le clic n'a rien fait.
        pub(super) fn follow_link(&mut self, action: &LinkAction, ctx: &egui::Context) {
            match action {
                LinkAction::Uri(url) => match self.open_url(url) {
                    Ok(()) => self.toast(ctx, format!("Opened {url}")),
                    Err(err) => self.toast(ctx, format!("Could not open link: {err}")),
                },
                LinkAction::GoTo { page } => {
                    let total = self.tabs.get(self.active).map(|tab| tab.total()).unwrap_or(0);
                    if total == 0 {
                        return;
                    }
                    let target = (*page).clamp(1, total);
                    self.jump = Some(target);
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.page = target;
                    }
                    self.toast(ctx, format!("Page {target}"));
                }
                LinkAction::Unsupported(name) => {
                    self.toast(ctx, format!("Unsupported link action: {name}"));
                }
            }
        }

        /// Suis le lien sous un point, s'il y en a un.
        ///
        /// Renvoie `true` quand un lien a été suivi : l'appelant laisse alors le
        /// clic tranquille, sans sélection ni ouverture d'inspecteur.
        pub(super) fn follow_link_at(&mut self, page: usize, point: [f32; 2], ctx: &egui::Context) -> bool {
            let Some(tab) = self.tabs.get(self.active) else {
                return false;
            };
            let links = tab.doc.page_links(page);
            let Some(link) = link_at(&links, point) else {
                return false;
            };
            let action = link.action.clone();
            self.follow_link(&action, ctx);
            true
        }

        pub(super) fn set_zoom(&mut self, zoom: Option<f32>) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                tab.zoom = zoom.map(|z| z.clamp(0.1, 8.0));
                if zoom.is_none() {
                    tab.zoom_fit_page = false;
                }
            }
            self.zoom_dirty = true;
        }

        /// Applique un réglage de zoom nommé.
        pub(super) fn set_zoom_mode(&mut self, mode: ZoomMode) {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                match mode {
                    ZoomMode::FitWidth => {
                        tab.zoom = None;
                        tab.zoom_fit_page = false;
                    }
                    ZoomMode::FitPage => {
                        tab.zoom = None;
                        tab.zoom_fit_page = true;
                    }
                    ZoomMode::Fixed(scale) => {
                        tab.zoom = Some(scale.clamp(0.1, 8.0));
                        tab.zoom_fit_page = false;
                    }
                }
            }
            self.zoom_dirty = true;
        }

        pub(super) fn zoom_by(&mut self, factor: f32) {
            let base = self
                .active_tab()
                .and_then(|t| t.zoom)
                .unwrap_or(self.last_fit.max(0.05));
            self.set_zoom(Some(base * factor));
        }

        /// Change la disposition des pages et ramène la page courante sous les
        /// yeux : dans une autre grille, la position de défilement n'a plus de
        /// sens.
        pub(super) fn set_page_layout(&mut self, ctx: &egui::Context, layout: PageLayout) {
            if self.page_layout == layout {
                return;
            }
            self.page_layout = layout;
            self.jump = Some(self.tabs.get(self.active).map_or(1, |tab| tab.page));
            self.toast(ctx, format!("Layout: {}", layout.label()));
        }

}
