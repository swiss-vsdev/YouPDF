//! Surface plein écran « Organiser les pages ».
//!
//! Remplace le canevas du document : grille de vignettes, sélection multiple
//! (clic, Maj, Cmd), actions de page, glisser-déposer pour réordonner, et
//! fenêtres d'extraction, d'insertion, de remplacement et de découpe.

use super::*;
use youpdf_shared::t;

/// Fenêtre ouverte par-dessus la grille.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OrganizeDialog {
    Extract,
    Insert,
    Replace,
    Split,
}

impl YouPdfApp {
    /// Dessine la surface plein écran (appelée à la place du canevas).
    pub(super) fn organize_view(&mut self, ui: &mut egui::Ui) {
        let Some(tab) = self.tabs.get(self.active) else {
            self.organize_open = false;
            return;
        };
        let total = tab.total();
        let current = tab.page;
        let mut selected = std::mem::take(&mut self.organize_selected);
        selected.retain(|page| *page >= 1 && *page <= total);
        let mut anchor = self.organize_anchor;
        let mut drag = self.organize_drag;
        let mut dialog = self.organize_dialog;
        let mut close = false;

        // ---- Barre supérieure -------------------------------------------
        egui::TopBottomPanel::top("organize-toolbar")
            .frame(Frame::none().fill(pal().widget_bg).inner_margin(Margin::symmetric(12.0, 8.0)))
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(t("Organize pages"))
                            .size(13.0)
                            .color(pal().icon_bright),
                    );
                    ui.add_space(10.0);
                    let count = if selected.is_empty() { total } else { selected.len() };
                    ui.label(
                        RichText::new(format!("{count} of {total} pages selected"))
                            .size(11.0)
                            .color(pal().muted),
                    );
                    if ui.small_button("Select all").clicked() {
                        selected = (1..=total).collect();
                        anchor = None;
                    }
                    if ui.small_button("Deselect all").clicked() {
                        selected.clear();
                        anchor = None;
                    }
                    if ui.small_button("Odd").clicked() {
                        selected = (1..=total).filter(|page| page % 2 == 1).collect();
                    }
                    if ui.small_button("Even").clicked() {
                        selected = (1..=total).filter(|page| page % 2 == 0).collect();
                    }
                    ui.separator();
                    if organize_button(ui, icon_rotate_left, "Rotate left") {
                        self.rotate_pages(ui.ctx(), &selected, total, -90);
                    }
                    if organize_button(ui, icon_rotate_right, "Rotate right") {
                        self.rotate_pages(ui.ctx(), &selected, total, 90);
                    }
                    if organize_button(ui, icon_trash, "Delete") {
                        if selected.len() >= total {
                            self.toast(ui.ctx(), "Keep at least one page".to_string());
                        } else {
                            self.ask_delete_pages(&selected, total);
                        }
                    }
                    if organize_button(ui, icon_export, "Extract…") {
                        dialog = Some(OrganizeDialog::Extract);
                    }
                    if organize_button(ui, icon_file_plus, "Insert…") {
                        dialog = Some(OrganizeDialog::Insert);
                    }
                    if ui.small_button("Replace…").clicked() {
                        dialog = Some(OrganizeDialog::Replace);
                    }
                    if organize_button(ui, icon_scissors, "Split…") {
                        dialog = Some(OrganizeDialog::Split);
                    }
                    if ui.small_button("Duplicate").clicked() {
                        self.duplicate_pages(ui.ctx(), &selected, total);
                    }
                    ui.separator();
                    if ui.button(format!("{} Close", ph::X)).clicked() {
                        close = true;
                    }
                });
            });

        // ---- Grille ------------------------------------------------------
        let active = self.active;
        let ppp = ui.ctx().pixels_per_point();
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
        let Some(tab) = self.tabs.get_mut(active) else {
            return;
        };
        let mut pending_move: Option<(usize, usize)> = None;
        let mut insert_bar: Option<(Rect, bool)> = None;
        // Actions rapides différées : la grille emprunte déjà `self.tabs`.
        let mut quick_rotate: Option<usize> = None;
        let mut quick_delete: Option<usize> = None;
        let pointer = ui.ctx().pointer_interact_pos();

        egui::CentralPanel::default()
            .frame(Frame::none().fill(pal().canvas_bg))
            .show_inside(ui, |ui| {
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(12.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.add_space(12.0);
                        for page in 1..=total as u32 {
                            let (wp, hp) = tab.page_size_cached(page);
                            let scale = 104.0 / wp.max(1.0);
                            let thumb = vec2(wp * scale, hp * scale);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(132.0, 196.0), Sense::click_and_drag());
                            let img_rect = Rect::from_center_size(
                                pos2(rect.center().x, rect.min.y + 10.0 + thumb.y * 0.5),
                                thumb,
                            );
                            let picked = selected.contains(&(page as usize));
                            ui.painter().rect_filled(
                                img_rect.expand(3.0),
                                Rounding::same(3.0),
                                if picked {
                                    pal().thumb_tint
                                } else {
                                    Color32::from_rgb(0x28, 0x28, 0x28)
                                },
                            );

                            if let Some(tex) = Self::page_texture(
                                ui.ctx(),
                                renderer,
                                tab,
                                page,
                                THUMBNAIL_STEP,
                                scale,
                                ppp,
                                true,
                                false,
                                status,
                            ) {
                                ui.painter().image(
                                    tex.id(),
                                    img_rect,
                                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                            if picked {
                                ui.painter().rect_stroke(
                                    img_rect.expand(3.0),
                                    3.0,
                                    Stroke::new(1.5_f32, pal().accent),
                                );
                                // Pastille de sélection.
                                let badge = pos2(img_rect.min.x + 9.0, img_rect.min.y + 9.0);
                                ui.painter().circle_filled(badge, 8.0, pal().accent);
                                ui.painter().text(
                                    badge,
                                    Align2::CENTER_CENTER,
                                    ph::CHECK,
                                    FontId::proportional(11.0),
                                    Color32::WHITE,
                                );
                            }
                            ui.painter().text(
                                pos2(rect.center().x, img_rect.max.y + 14.0),
                                Align2::CENTER_CENTER,
                                page.to_string(),
                                FontId::proportional(11.0),
                                if picked { pal().accent } else { pal().muted },
                            );
                            if page as usize == current {
                                ui.painter().circle_filled(
                                    pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                                    3.0,
                                    pal().accent,
                                );
                            }

                            // Actions rapides au survol.
                            if resp.hovered() {
                                let quick =
                                    Rect::from_min_size(img_rect.max - vec2(44.0, 20.0), vec2(40.0, 18.0));
                                ui.painter().rect_filled(
                                    quick,
                                    Rounding::same(3.0),
                                    Color32::from_black_alpha(170),
                                );
                                let left = Rect::from_min_size(quick.min, vec2(20.0, 18.0));
                                let right = Rect::from_min_size(
                                    pos2(quick.min.x + 20.0, quick.min.y),
                                    vec2(20.0, 18.0),
                                );
                                if ui.put(left, Button::new("⟳").small()).clicked() {
                                    quick_rotate = Some(page as usize);
                                }
                                if ui.put(right, Button::new(ph::X).small()).clicked() {
                                    quick_delete = Some(page as usize);
                                }
                            }

                            // Glisser-déposer : barre d'insertion avant/après.
                            if resp.drag_started() {
                                drag = Some(page as usize);
                            }
                            if drag.is_some() && resp.hovered() {
                                let after = pointer
                                    .map(|point| point.x > rect.center().x)
                                    .unwrap_or(false);
                                let x = if after { rect.max.x } else { rect.min.x };
                                insert_bar = Some((
                                    Rect::from_min_max(
                                        pos2(x - 1.5, rect.min.y),
                                        pos2(x + 1.5, rect.max.y),
                                    ),
                                    after,
                                ));
                                if let Some(from) = drag {
                                    if ui.input(|i| i.pointer.any_released()) {
                                        let mut to = page as usize;
                                        if after {
                                            to += 1;
                                        }
                                        pending_move = Some((from, to));
                                    }
                                }
                            }
                            if resp.drag_stopped() {
                                drag = None;
                            }
                            if resp.clicked() {
                                let modifiers = ui.input(|i| i.modifiers);
                                let page_usize = page as usize;
                                if modifiers.shift {
                                    if let Some(origin) = anchor {
                                        let (low, high) =
                                            (origin.min(page_usize), origin.max(page_usize));
                                        selected.extend(low..=high);
                                    } else {
                                        selected.insert(page_usize);
                                        anchor = Some(page_usize);
                                    }
                                } else if modifiers.command {
                                    if !selected.remove(&page_usize) {
                                        selected.insert(page_usize);
                                    }
                                    anchor = Some(page_usize);
                                } else {
                                    selected.clear();
                                    selected.insert(page_usize);
                                    anchor = Some(page_usize);
                                }
                            }
                            ui.add_space(2.0);
                        }
                    });
                    ui.add_space(16.0);
                });
            });

        if let Some((bar, _)) = insert_bar {
            ui.painter().rect_filled(
                bar,
                Rounding::ZERO,
                Color32::from_rgb(230, 120, 40),
            );
        }

        self.organize_selected = selected;
        self.organize_anchor = anchor;
        self.organize_drag = drag;
        self.organize_dialog = dialog;
        if close {
            self.organize_open = false;
            self.organize_selected.clear();
            self.organize_dialog = None;
        }
        // Les actions rapides passent après la restitution de la sélection :
        // une suppression remet la sienne à zéro, et l'écrire avant
        // ressusciterait des pages disparues.
        if let Some(page) = quick_rotate {
            let single: BTreeSet<usize> = [page].into_iter().collect();
            self.rotate_pages(ui.ctx(), &single, total, 90);
        }
        if let Some(page) = quick_delete {
            let single: BTreeSet<usize> = [page].into_iter().collect();
            self.ask_delete_pages(&single, total);
        }
        if let Some((from, to)) = pending_move {
            if let Some(tab) = self.tabs.get_mut(active) {
                let target = to.min(tab.total());
                if from != target && tab.doc.move_page(from, target).is_ok() {
                    self.mark_document_changed();
                    self.toast(ui.ctx(), format!("Page {from} moved to {target}"));
                }
            }
        }

        // Raccourcis de la surface.
        if ui.input(|i| i.key_pressed(egui::Key::A) && i.modifiers.command) {
            self.organize_selected = (1..=total).collect();
        }
        if ui.input(|i| {
            (i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace))
                && !i.modifiers.command
        }) && self.organize_selected.len() < total
        {
            let sel = self.organize_selected.clone();
            self.ask_delete_pages(&sel, total);
        }

        self.organize_dialogs(ui.ctx());
        self.delete_confirm_window(ui.ctx());
    }

    /// Actions groupées sur la sélection (vide = tout le document).
    fn organize_targets(&self, selected: &BTreeSet<usize>, total: usize) -> Vec<usize> {
        if selected.is_empty() {
            (1..=total).collect()
        } else {
            selected.iter().copied().collect()
        }
    }

    fn rotate_pages(
        &mut self,
        ctx: &egui::Context,
        selected: &BTreeSet<usize>,
        total: usize,
        delta: i64,
    ) {
        let pages = self.organize_targets(selected, total);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            match tab.doc.rotate_pages(&pages, delta) {
                Ok(count) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) rotated"));
                }
                Err(err) => self.toast(ctx, format!("Rotation failed: {err}")),
            }
        }
    }

    /// Demande confirmation avant de supprimer : la suppression est
    /// destructive, et la seule autre sortie est Annuler.
    fn ask_delete_pages(&mut self, selected: &BTreeSet<usize>, total: usize) {
        if let Some(pages) = pages_to_confirm_delete(&self.organize_targets(selected, total), total) {
            self.organize_pending_delete = Some(pages);
        }
    }

    /// Fenêtre de confirmation de la suppression.
    fn delete_confirm_window(&mut self, ctx: &egui::Context) {
        let Some(pages) = self.organize_pending_delete.clone() else {
            return;
        };
        let total = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        let mut open = true;
        let mut confirmed = false;
        let mut cancelled = false;
        self.show_modal(ctx, "modal-delete-pages", "Delete pages", 340.0, &mut open, |_app, ui| {
            ui.label(
                RichText::new(format!("Remove {} page(s) from this document?", pages.len()))
                    .size(12.5)
                    .color(pal().icon_bright),
            );
            ui.label(
                RichText::new(t("The pages leave the file. Undo brings them back."))
                    .size(11.0)
                    .color(pal().muted),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(t("Delete")).clicked() {
                    confirmed = true;
                }
                if ui.button(t("Cancel")).clicked() {
                    cancelled = true;
                }
            });
        });
        if confirmed {
            self.organize_pending_delete = None;
            let selection: BTreeSet<usize> = pages.into_iter().collect();
            self.delete_pages(ctx, &selection, total);
        } else if cancelled || !open {
            self.organize_pending_delete = None;
        }
    }

    fn delete_pages(&mut self, ctx: &egui::Context, selected: &BTreeSet<usize>, total: usize) {
        let pages = self.organize_targets(selected, total);
        if pages.len() >= total {
            return;
        }
        let outcome = match self.tabs.get_mut(self.active) {
            Some(tab) => match tab.doc.delete_pages(&pages) {
                Ok(count) => {
                    let total = tab.total();
                    if tab.page > total {
                        tab.page = total.max(1);
                    }
                    Ok(count)
                }
                Err(err) => Err(err),
            },
            None => return,
        };
        match outcome {
            Ok(count) => {
                self.mark_document_changed();
                self.organize_selected.clear();
                self.toast(ctx, format!("{count} page(s) removed"));
            }
            Err(err) => self.toast(ctx, format!("Delete failed: {err}")),
        }
    }

    fn duplicate_pages(&mut self, ctx: &egui::Context, selected: &BTreeSet<usize>, total: usize) {
        let pages = self.organize_targets(selected, total);
        if let Some(tab) = self.tabs.get_mut(self.active) {
            match tab.doc.duplicate_pages(&pages) {
                Ok(count) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) duplicated"));
                }
                Err(err) => self.toast(ctx, format!("Duplicate failed: {err}")),
            }
        }
    }

    /// Fenêtres d'action (extraction, insertion, remplacement, découpe).
    fn organize_dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = self.organize_dialog else {
            return;
        };
        let total = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        let mut open = true;
        match dialog {
            OrganizeDialog::Extract => {
                egui::Window::new(t("Extract pages"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .open(&mut open)
                    .show(ctx, |ui| {
                        ui.checkbox(
                            &mut self.organize_extract_single,
                            "Extract as a single PDF",
                        );
                        ui.checkbox(
                            &mut self.organize_extract_delete,
                            "Delete original pages after extraction",
                        );
                        ui.label(
                            RichText::new(format!(
                                "{} page(s) selected",
                                self.organize_selected.len().max(1)
                            ))
                            .size(10.5)
                            .color(pal().muted),
                        );
                        if ui.button(t("Extract…")).clicked() {
                            self.run_extract(ctx);
                        }
                    });
            }
            OrganizeDialog::Insert => {
                egui::Window::new(t("Insert pages"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .open(&mut open)
                    .show(ctx, |ui| {
                        ui.label(RichText::new(t("Position")).size(10.5).color(pal().muted));
                        egui::ComboBox::from_id_salt("insert-where")
                            .width(170.0)
                            .selected_text(
                                ["Before page", "After page", "First page", "Last page"]
                                    [self.organize_insert_where.min(3) as usize],
                            )
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (0u8, "Before page"),
                                    (1, "After page"),
                                    (2, "First page"),
                                    (3, "Last page"),
                                ] {
                                    if ui
                                        .selectable_label(
                                            self.organize_insert_where == value,
                                            label,
                                        )
                                        .clicked()
                                    {
                                        self.organize_insert_where = value;
                                    }
                                }
                            });
                        if self.organize_insert_where <= 1 {
                            ui.add(
                                egui::DragValue::new(&mut self.organize_insert_page)
                                    .range(1..=total.max(1)),
                            );
                        }
                        ui.separator();
                        ui.label(RichText::new(t("Source")).size(10.5).color(pal().muted));
                        if ui.button(t("From PDF file…")).clicked() {
                            self.run_insert_file(ctx);
                        }
                        if ui.button(t("Blank A4 page")).clicked() {
                            self.run_insert_blank(ctx);
                        }
                        let pasted = ui.button(t("From Clipboard")).clicked();
                        if pasted {
                            self.run_insert_clipboard(ctx);
                        }
                    });
            }
            OrganizeDialog::Replace => {
                egui::Window::new(t("Replace pages"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .open(&mut open)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(t("Pages"));
                            ui.add(
                                egui::DragValue::new(&mut self.organize_replace_from)
                                    .range(1..=total.max(1)),
                            );
                            ui.label(t("to"));
                            ui.add(
                                egui::DragValue::new(&mut self.organize_replace_to)
                                    .range(1..=total.max(1)),
                            );
                        });
                        ui.label(RichText::new(t("with pages from a PDF")).size(10.5).color(pal().muted));
                        if ui.button(t("Choose file and replace…")).clicked() {
                            self.run_replace(ctx);
                        }
                    });
            }
            OrganizeDialog::Split => {
                egui::Window::new(t("Split document"))
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                    .open(&mut open)
                    .show(ctx, |ui| {
                        egui::ComboBox::from_id_salt("split-mode")
                            .width(200.0)
                            .selected_text(
                                ["Every N pages", "Maximum file size", "Top-level bookmarks"]
                                    [self.organize_split_mode.min(2) as usize],
                            )
                            .show_ui(ui, |ui| {
                                for (value, label) in [
                                    (0u8, "Every N pages"),
                                    (1, "Maximum file size"),
                                    (2, "Top-level bookmarks"),
                                ] {
                                    if ui
                                        .selectable_label(
                                            self.organize_split_mode == value,
                                            label,
                                        )
                                        .clicked()
                                    {
                                        self.organize_split_mode = value;
                                    }
                                }
                            });
                        match self.organize_split_mode {
                            0 => {
                                ui.add(
                                    egui::DragValue::new(&mut self.organize_split)
                                        .range(1..=200)
                                        .speed(0.2),
                                );
                            }
                            1 => {
                                ui.add(
                                    egui::DragValue::new(&mut self.organize_split_mb)
                                        .range(0.1..=500.0)
                                        .speed(0.1)
                                        .suffix(" MB"),
                                );
                            }
                            _ => {
                                ui.label(
                                    RichText::new(t("One file per top-level bookmark"))
                                        .size(10.5)
                                        .color(pal().muted),
                                );
                            }
                        }
                        if ui.button(t("Choose folder and split…")).clicked() {
                            self.run_split(ctx);
                        }
                    });
            }
        }
        if !open {
            self.organize_dialog = None;
        }
    }

    fn run_extract(&mut self, ctx: &egui::Context) {
        let pages = self.organize_targets(
            &self.organize_selected.clone(),
            self.tabs.get(self.active).map_or(0, |tab| tab.total()),
        );
        let delete_after = self.organize_extract_delete;
        let single_file = self.organize_extract_single;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if single_file {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("extract.pdf")
                .save_file()
            else {
                return;
            };
            let result = tab.doc.extract_pages(&pages, &path);
            if result.is_ok() && delete_after {
                let total = tab.total();
                if pages.len() < total {
                    let _ = tab.doc.delete_pages(&pages);
                    self.mark_document_changed();
                }
            }
            match result {
                Ok(()) => self.toast(ctx, "Pages extracted".to_string()),
                Err(err) => self.toast(ctx, format!("Extract failed: {err}")),
            }
        } else {
            let Some(directory) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let mut written = 0usize;
            for page in pages.iter().copied() {
                let target = directory.join(format!("page-{page}.pdf"));
                if tab.doc.extract_pages(&[page], &target).is_ok() {
                    written += 1;
                }
            }
            self.toast(ctx, format!("{written} file(s) written"));
        }
    }

    fn run_insert_file(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
        else {
            return;
        };
        let total = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        let after = self.insert_position(total);
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match tab.doc.insert_pages_after(after, &path) {
            Ok(count) => {
                self.mark_document_changed();
                self.toast(ctx, format!("{count} page(s) inserted"));
            }
            Err(err) => self.toast(ctx, format!("Insert failed: {err}")),
        }
    }

    fn run_insert_blank(&mut self, ctx: &egui::Context) {
        let total = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        let after = self.insert_position(total);
        match youpdf_core::blank_document(1, 595.0, 842.0) {
            Ok(doc) => self.insert_document(ctx, doc, after, "blank"),
            Err(err) => self.toast(ctx, format!("Blank page failed: {err}")),
        }
    }

    /// Insère l'image du presse-papiers comme une page.
    ///
    /// egui ne connaît que le texte du presse-papiers ; l'image vient donc
    /// de son backend de fenêtre, `arboard`, que `egui-winit` tire déjà.
    fn run_insert_clipboard(&mut self, ctx: &egui::Context) {
        let total = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        let after = self.insert_position(total);
        let image = match clipboard_image() {
            Ok(bytes) => bytes,
            Err(message) => {
                self.toast(ctx, message);
                return;
            }
        };
        // 96 dpi : la résolution qu'annonce un presse-papiers, et celle qui
        // rend une capture d'écran à sa taille perçue.
        let doc = match youpdf_core::document_from_image_bytes(&image, 96.0) {
            Ok(doc) => doc,
            Err(err) => {
                self.toast(ctx, format!("Clipboard image failed: {err}"));
                return;
            }
        };
        self.insert_document(ctx, doc, after, "image");
    }

    /// Où poser les pages insérées, d'après le choix du dialogue.
    fn insert_position(&self, total: usize) -> usize {
        match self.organize_insert_where {
            0 => self.organize_insert_page.saturating_sub(1),
            2 => 0,
            3 => total,
            _ => self.organize_insert_page.min(total),
        }
    }

    /// Écrit `doc` dans un fichier temporaire puis l'insère après `after`.
    ///
    /// `insert_pages_after` prend un chemin : le temporaire est le seul
    /// moyen de réunir un document construit en mémoire et cette API.
    fn insert_document(
        &mut self,
        ctx: &egui::Context,
        mut doc: youpdf_core::Document,
        after: usize,
        what: &str,
    ) {
        let path = std::env::temp_dir().join(format!("youpdf-insert-{what}.pdf"));
        if let Err(err) = doc.save(&path) {
            self.toast(ctx, format!("Insert failed: {err}"));
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            let _ = std::fs::remove_file(&path);
            return;
        };
        match tab.doc.insert_pages_after(after, &path) {
            Ok(count) => {
                let _ = std::fs::remove_file(&path);
                self.mark_document_changed();
                self.toast(ctx, format!("{count} {what} page(s) inserted"));
            }
            Err(err) => {
                let _ = std::fs::remove_file(&path);
                self.toast(ctx, format!("Insert failed: {err}"));
            }
        }
    }

    fn run_replace(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
        else {
            return;
        };
        let (from, to) = (
            self.organize_replace_from.min(self.organize_replace_to),
            self.organize_replace_from.max(self.organize_replace_to),
        );
        let pages: Vec<usize> = (from..=to).collect();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match tab.doc.replace_pages(&pages, &path) {
            Ok(count) => {
                self.mark_document_changed();
                self.toast(ctx, format!("{count} page(s) replaced"));
            }
            Err(err) => self.toast(ctx, format!("Replace failed: {err}")),
        }
    }

    fn run_split(&mut self, ctx: &egui::Context) {
        let Some(directory) = rfd::FileDialog::new().pick_folder() else {
            return;
        };
        let mode = self.organize_split_mode;
        let per_file = self.organize_split.max(1);
        let max_bytes = (self.organize_split_mb.max(0.1) * 1024.0 * 1024.0) as u64;
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let result = match mode {
            1 => tab.doc.split_by_size(&directory, max_bytes),
            2 => tab.doc.split_by_bookmarks(&directory),
            _ => tab.doc.split(&directory, per_file),
        };
        match result {
            Ok(files) => self.toast(ctx, format!("{} file(s) written", files.len())),
            Err(err) => self.toast(ctx, format!("Split failed: {err}")),
        }
    }
}

/// Octets PNG de l'image du presse-papiers.
///
/// Le presse-papiers peut être vide, ne contenir que du texte, ou refuser
/// l'accès (session sans gestionnaire) : chaque cas rend un message
/// affichable plutôt qu'une panique.
fn clipboard_image() -> Result<Vec<(String, Vec<u8>)>, String> {
    use arboard::Clipboard;

    let mut clipboard =
        Clipboard::new().map_err(|_| "Clipboard is not available".to_string())?;
    let image = clipboard
        .get_image()
        .map_err(|_| "No image in the clipboard".to_string())?;
    let rgba = image::RgbaImage::from_raw(
        image.width as u32,
        image.height as u32,
        image.bytes.into_owned(),
    )
    .ok_or_else(|| "Clipboard image is malformed".to_string())?;

    let mut png = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut png, image::ImageFormat::Png)
        .map_err(|err| format!("Clipboard image failed: {err}"))?;
    Ok(vec![("clipboard".to_string(), png.into_inner())])
}

/// Pages qu'une suppression doit viser, ou `None` quand il n'y a rien à
/// confirmer : plus rien à supprimer, ou tout le document (que l'app refuse
/// de vider).
fn pages_to_confirm_delete(pages: &[usize], total: usize) -> Option<Vec<usize>> {
    if pages.is_empty() || pages.len() >= total {
        None
    } else {
        Some(pages.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::pages_to_confirm_delete;

    #[test]
    fn une_selection_partielle_demande_confirmation() {
        assert_eq!(pages_to_confirm_delete(&[2, 3], 10), Some(vec![2, 3]));
    }

    #[test]
    fn une_selection_vide_ne_demande_rien() {
        assert_eq!(pages_to_confirm_delete(&[], 10), None);
    }

    #[test]
    fn tout_le_document_ne_demande_rien() {
        let tout: Vec<usize> = (1..=4).collect();
        assert_eq!(pages_to_confirm_delete(&tout, 4), None);
    }

    #[test]
    fn tout_sauf_une_page_demande_confirmation() {
        assert_eq!(pages_to_confirm_delete(&[1, 2, 3], 4), Some(vec![1, 2, 3]));
    }
}
