//! Fenêtres de protection et d'habillage des pages : mot de passe,
//! recadrage, découpage en tuiles, arrière-plan, filigrane Bates.

use super::*;
use youpdf_shared::{BODY, CAPTION, LABEL, muted, section, t, RADIUS_S, SPACE_L, SPACE_M, SPACE_S};

impl YouPdfApp {
    /// Fenêtre « Protéger ».
    pub(super) fn protect_window(&mut self, ctx: &egui::Context) {
        if !self.protect_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let mut decrypt = false;
        self.show_modal(ctx, "modal-protect", "Protect", 420.0, &mut open, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(section(t("Encryption")));
                for strength in [EncryptionStrength::Aes256, EncryptionStrength::Rc4_128] {
                    if ui
                        .selectable_label(
                            app.protect.strength == strength,
                            strength.display_name(),
                        )
                        .clicked()
                    {
                        app.protect.strength = strength;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(section(t("Open password")));
                ui.add(
                    TextEdit::singleline(&mut app.protect.user_password)
                        .password(true)
                        .desired_width(190.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label(section(t("Permissions password")));
                ui.add(
                    TextEdit::singleline(&mut app.protect.owner_password)
                        .password(true)
                        .desired_width(150.0),
                );
            });
            ui.add_space(SPACE_M);
            ui.label(section(t("Allowed actions")));
            egui::Grid::new("protect-perms")
                .num_columns(3)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    ui.checkbox(&mut app.protect.permissions.print, t("Print"));
                    ui.checkbox(
                        &mut app.protect.permissions.print_high_quality,
                        "High-quality print",
                    );
                    ui.checkbox(&mut app.protect.permissions.copy, t("Copy text"));
                    ui.end_row();
                    ui.checkbox(&mut app.protect.permissions.modify, t("Modify"));
                    ui.checkbox(&mut app.protect.permissions.annotate, t("Comment"));
                    ui.checkbox(&mut app.protect.permissions.fill_forms, t("Fill forms"));
                    ui.end_row();
                    ui.checkbox(&mut app.protect.permissions.assemble, t("Assemble"));
                    ui.checkbox(&mut app.protect.permissions.accessibility, t("Accessibility"));
                    ui.end_row();
                });
            ui.add_space(SPACE_L);
            ui.label(
                RichText::new(
                    "The copy is encrypted on disk; the document in the window stays open                          and unencrypted.",
                )
                .size(LABEL)
                .color(pal().muted),
            );
            ui.add_space(SPACE_M);
            ui.horizontal(|ui| {
                if ui.button(t("Save encrypted copy…")).clicked() {
                    run = true;
                }
                if ui.button(t("Remove protection from a file…")).clicked() {
                    decrypt = true;
                }
            });
        });
        if run {
            self.protect_active(ctx);
        }
        if decrypt {
            self.remove_protection(ctx);
        }
        if !open {
            self.protect_open = false;
        }
    }

    /// Écrit une copie chiffrée du document.
    fn protect_active(&mut self, ctx: &egui::Context) {
        if self.protect.user_password.is_empty() {
            self.toast(ctx, "Set an open password first");
            return;
        }
        let name = self
            .active_stem()
            .map(|stem| format!("{stem}-protected.pdf"))
            .unwrap_or_else(|| "protected.pdf".to_owned());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(&name)
            .save_file()
        else {
            return;
        };
        let options = self.protect.clone();
        let result = self.with_annotations_applied(|tab| tab.doc.save_protected(&path, &options));
        match result {
            Some(Ok(())) => {
                let saved = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "the copy".to_owned());
                self.toast(ctx, format!("Encrypted copy saved: {saved}"));
            }
            Some(Err(err)) => self.toast(ctx, format!("Protect failed: {err}")),
            None => {}
        }
    }

    // -- Canevas -----------------------------------------------------------

    // -- Fonctionnalités de document ---------------------------------------

    /// Pages visées par la fenêtre de recadrage.
    pub(super) fn crop_pages(&self) -> Vec<usize> {
        let total = self.active_total() as usize;
        match self.crop_range {
            1 => vec![self.tabs.get(self.active).map_or(1, |tab| tab.page)],
            2 => (1..=total).filter(|page| page % 2 == 1).collect(),
            3 => (1..=total).filter(|page| page % 2 == 0).collect(),
            _ => (1..=total).collect(),
        }
    }

    /// Recadre chaque page sur la boîte englobante de son texte, avec 12 pt
    /// de marge. Les pages sans texte sont laissées telles quelles.
    pub(super) fn auto_crop_pages(&mut self, ctx: &egui::Context, pages: &[usize]) {
        let mut cropped = 0usize;
        for page in pages.iter().copied() {
            let page_u32 = page as u32;
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            if !tab.fragments.contains_key(&page_u32) {
                reader::extract_page_fragments(&*self.renderer, tab, page_u32);
            }
            let Some(frags) = tab.fragments.get(&page_u32) else {
                continue;
            };
            if frags.is_empty() {
                continue;
            }
            let mut bounds = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
            for frag in frags {
                bounds[0] = bounds[0].min(frag.rect[0]);
                bounds[1] = bounds[1].min(frag.rect[1]);
                bounds[2] = bounds[2].max(frag.rect[2]);
                bounds[3] = bounds[3].max(frag.rect[3]);
            }
            let view = tab.doc.page_view(page_u32);
            const PAD: f32 = 12.0;
            let margins = CropMargins {
                left: (bounds[0] - view.x - PAD).max(0.0),
                bottom: (bounds[1] - view.y - PAD).max(0.0),
                right: (view.x + view.width - bounds[2] - PAD).max(0.0),
                top: (view.y + view.height - bounds[3] - PAD).max(0.0),
            };
            if tab.doc.crop_pages(&[page], margins).is_ok() {
                cropped += 1;
            }
        }
        if cropped > 0 {
            self.mark_document_changed();
            self.toast(ctx, format!("{cropped} page(s) auto-cropped"));
        }
    }

    /// Fenêtre « Découper en tuiles » : lignes × colonnes sur les pages visées.
    pub(super) fn tile_split_window(&mut self, ctx: &egui::Context) {
        if !self.tile_split_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut rows = self.tile_rows;
        let mut cols = self.tile_cols;
        let mut gap = self.tile_gap;
        self.show_modal(ctx, "modal-tile-split", "Split pages", 360.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Each page becomes a grid of pages, one per tile. \
                     Useful for posters printed across several sheets.",
                )
                .size(LABEL)
                .color(pal().muted),
            );
            ui.add_space(SPACE_M);
            ui.add(egui::Slider::new(&mut rows, 1..=8).text("Rows"));
            ui.add(egui::Slider::new(&mut cols, 1..=8).text("Columns"));
            ui.horizontal(|ui| {
                ui.label(section(t("Gap")));
                ui.add(egui::DragValue::new(&mut gap).range(0.0..=200.0).suffix(" pt"));
            });
            ui.add_space(SPACE_S);
            ui.label(
                RichText::new(format!(
                    "{} page(s) selected → {} page(s) per page",
                    app.crop_pages().len(),
                    rows * cols
                ))
                .size(CAPTION)
                .color(pal().muted),
            );
            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                if ui.button(t("Split")).clicked() {
                    apply = true;
                }
                if ui.button(t("Cancel")).clicked() {
                    app.tile_split_open = false;
                }
            });
        });
        self.tile_rows = rows;
        self.tile_cols = cols;
        self.tile_gap = gap;
        if !open {
            self.tile_split_open = false;
        }
        if apply {
            let pages = self.crop_pages();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.split_tiles_with_gap(&pages, rows, cols, gap));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.tile_split_open = false;
                    self.toast(ctx, format!("{count} tile page(s) created"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Split failed: {err}")),
                None => {}
            }
        }
    }

    /// Bandeau proposant l'OCR quand la page 1 ressemble à un scan.
    pub(super) fn scan_banner_ui(&mut self, ui: &mut egui::Ui) {
        if !self.scan_banner {
            return;
        }
        let palette = pal();
        Frame::none()
            .fill(palette.widget_bg)
            .inner_margin(Margin::symmetric(12.0, 8.0))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(
                            "Scanned document detected. Recognize text to search, copy and edit it.",
                        )
                        .size(BODY)
                        .color(palette.icon_bright),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Dismiss").clicked() {
                            self.scan_banner = false;
                        }
                        if ui.small_button("Don't show again").clicked() {
                            self.scan_banner = false;
                            self.scan_banner_off = true;
                        }
                        if ui.button(t("Perform OCR")).clicked() {
                            self.ocr_open = true;
                            self.scan_banner = false;
                        }
                    });
                });
            });
    }

    /// Fenêtre « Recadrage » : marges retirées des quatre bords.
    pub(super) fn crop_window(&mut self, ctx: &egui::Context) {
        if !self.crop_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut reset = false;
        let mut auto = false;
        self.show_modal(ctx, "modal-crop", "Crop pages", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Margins removed from every edge. Negative values grow the page.",
                )
                .size(LABEL)
                .color(pal().muted),
            );
            ui.add_space(SPACE_M);
            ui.horizontal(|ui| {
                ui.label(muted(t("Unit"), LABEL));
                egui::ComboBox::from_id_salt("crop-unit")
                    .width(64.0)
                    .selected_text(app.crop_unit.suffix())
                    .show_ui(ui, |ui| {
                        for unit in Unit::ALL {
                            if ui
                                .selectable_label(app.crop_unit == unit, unit.suffix())
                                .clicked()
                            {
                                app.crop_unit = unit;
                            }
                        }
                    });
                ui.label(muted(t("Pages"), LABEL));
                egui::ComboBox::from_id_salt("crop-range")
                    .width(96.0)
                    .selected_text(
                        ["All pages", "Current page", "Odd pages", "Even pages"]
                            [app.crop_range.min(3) as usize],
                    )
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (0u8, "All pages"),
                            (1, "Current page"),
                            (2, "Odd pages"),
                            (3, "Even pages"),
                        ] {
                            if ui
                                .selectable_label(app.crop_range == value, label)
                                .clicked()
                            {
                                app.crop_range = value;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label(muted(t("Box"), LABEL));
                egui::ComboBox::from_id_salt("crop-box")
                    .width(120.0)
                    .selected_text(
                        ["CropBox", "ArtBox", "BleedBox", "TrimBox"]
                            [app.crop_target.min(3) as usize],
                    )
                    .show_ui(ui, |ui| {
                        for (value, label) in [
                            (0u8, "CropBox"),
                            (1, "ArtBox"),
                            (2, "BleedBox"),
                            (3, "TrimBox"),
                        ] {
                            if ui
                                .selectable_label(app.crop_target == value, label)
                                .clicked()
                            {
                                app.crop_target = value;
                            }
                        }
                    });
            });
            ui.add_space(SPACE_M);
            egui::Grid::new("crop-grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        ("Left", &mut app.crop.left),
                        ("Right", &mut app.crop.right),
                        ("Top", &mut app.crop.top),
                        ("Bottom", &mut app.crop.bottom),
                    ] {
                        ui.label(section(label));
                        let mut shown = app.crop_unit.from_points(*value);
                        if ui
                            .add(
                                egui::DragValue::new(&mut shown)
                                    .range(-2000.0..=2000.0)
                                    .speed(0.1)
                                    .suffix(format!(" {}", app.crop_unit.suffix())),
                            )
                            .changed()
                        {
                            *value = app.crop_unit.to_points(shown);
                        }
                        ui.end_row();
                    }
                });
            ui.add_space(SPACE_L);
            ui.horizontal_wrapped(|ui| {
                if ui.button(t("Apply")).clicked() {
                    apply = true;
                }
                if ui.button(t("Auto-crop margins")).clicked() {
                    auto = true;
                }
                if ui.button(t("Reset crop")).clicked() {
                    reset = true;
                }
            });
        });

        let pages = self.crop_pages();
        if auto {
            self.auto_crop_pages(ctx, &pages);
        }
        if apply {
            let margins = self.crop;
            let target = self.crop_target;
            let result = self.tabs.get_mut(self.active).map(|tab| {
                let target = match target {
                    1 => youpdf_core::crop::PageBox::Art,
                    2 => youpdf_core::crop::PageBox::Bleed,
                    3 => youpdf_core::crop::PageBox::Trim,
                    _ => youpdf_core::crop::PageBox::Crop,
                };
                tab.doc.crop_pages_to(&pages, margins, target)
            });
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) cropped"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Crop failed: {err}")),
                None => {}
            }
        }
        if reset {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.reset_crop(&pages));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Crop removed on {count} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Reset failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.crop_open = false;
        }
    }

    /// Fenêtre « Arrière-plan » : aplat de couleur ou image derrière la page.
    pub(super) fn background_window(&mut self, ctx: &egui::Context) {
        if !self.background_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let mut choose = false;
        let total = self.active_total();
        let image_name = self
            .background
            .image
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        self.show_modal(ctx, "modal-background", "Background", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "The background is painted behind the page content and prints with \
                     the document.",
                )
                .size(LABEL)
                .color(pal().muted),
            );
            ui.add_space(SPACE_M);
            ui.horizontal(|ui| {
                ui.label(section(t("Colour")));
                let [mut r, mut g, mut b] = app.background.color;
                let mut changed = false;
                changed |= ui
                    .add(egui::Slider::new(&mut r, 0..=255).show_value(false))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut g, 0..=255).show_value(false))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut b, 0..=255).show_value(false))
                    .changed();
                if changed {
                    app.background.color = [r, g, b];
                }
                let (rect, _) = ui.allocate_exact_size(vec2(24.0, 18.0), Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    Rounding::same(RADIUS_S),
                    Color32::from_rgb(r, g, b),
                );
            });
            ui.horizontal(|ui| {
                ui.label(section(t("Opacity")));
                ui.add(
                    egui::Slider::new(&mut app.background.opacity, 0.05..=1.0)
                        .show_value(false),
                );
            });
            ui.horizontal(|ui| {
                if ui.button(t("Choose an image…")).clicked() {
                    choose = true;
                }
                ui.label(
                    RichText::new(
                        image_name
                            .clone()
                            .unwrap_or_else(|| "No image (colour fill)".to_owned()),
                    )
                    .size(LABEL)
                    .color(pal().muted),
                );
                if app.background.image.is_some() && ui.small_button("Remove").clicked() {
                    app.background.image = None;
                }
            });
            if app.background.image.is_some() {
                ui.horizontal(|ui| {
                    ui.label(section(t("Image scale")));
                    ui.add(egui::Slider::new(&mut app.background.scale, 0.2..=2.0));
                });
            }
            ui.horizontal(|ui| {
                ui.label(section(t("Pages")));
                let mut all = app.background_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.background_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.background_from.as_mut() {
                    ui.label(section(t("from")));
                    ui.add(egui::DragValue::new(from).range(1..=total));
                }
            });
            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                if ui.button(t("Apply")).clicked() {
                    apply = true;
                }
                if ui.button(t("Remove from the document")).clicked() {
                    clear = true;
                }
            });
        });

        if choose {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_file()
            {
                self.background.image = Some(path);
            }
        }
        if apply {
            self.background.pages = Self::page_range_from(self.background_from);
            let bg = self.background.clone();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_background(&bg));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Background applied to {count} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Background failed: {err}")),
                None => {}
            }
        }
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_backgrounds());
            match result {
                Some(Ok(removed)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {removed} background(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.background_open = false;
        }
    }

    /// Fenêtre « Numérotation Bates ».
    pub(super) fn bates_window(&mut self, ctx: &egui::Context) {
        if !self.bates_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        self.show_modal(ctx, "modal-bates", "Bates numbering", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "A sequential stamp on every chosen page, for legal discovery and \
                     document control.",
                )
                .size(LABEL)
                .color(pal().muted),
            );
            ui.add_space(SPACE_M);
            egui::Grid::new("bates-grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(section(t("Prefix")));
                    ui.add(
                        TextEdit::singleline(&mut app.bates.prefix).desired_width(160.0),
                    );
                    ui.end_row();
                    ui.label(section(t("Suffix")));
                    ui.add(
                        TextEdit::singleline(&mut app.bates.suffix).desired_width(160.0),
                    );
                    ui.end_row();
                    ui.label(section(t("Start at")));
                    ui.add(egui::DragValue::new(&mut app.bates.start).range(0..=999_999));
                    ui.end_row();
                    ui.label(section(t("Step")));
                    ui.add(egui::DragValue::new(&mut app.bates.step).range(1..=999_999));
                    ui.end_row();
                    ui.label(section(t("Digits")));
                    ui.add(egui::DragValue::new(&mut app.bates.digits).range(1..=12));
                    ui.end_row();
                    ui.label(section(t("Font size")));
                    ui.add(
                        egui::DragValue::new(&mut app.bates.font_size).range(4.0..=72.0),
                    );
                    ui.end_row();
                    ui.label(section(t("Margin")));
                    ui.add(egui::DragValue::new(&mut app.bates.margin).range(0.0..=200.0));
                    ui.end_row();
                });
            ui.add_space(SPACE_S);
            ui.label(section(t("Position")));
            ui.horizontal_wrapped(|ui| {
                for (position, label) in BATES_POSITIONS {
                    if ui
                        .selectable_label(app.bates.position == position, label)
                        .clicked()
                    {
                        app.bates.position = position;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(section(t("Pages")));
                let mut all = app.bates_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.bates_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.bates_from.as_mut() {
                    ui.label(section(t("from")));
                    ui.add(egui::DragValue::new(from).range(1..=total));
                }
            });
            ui.add_space(SPACE_L);
            ui.horizontal(|ui| {
                if ui.button(t("Apply")).clicked() {
                    apply = true;
                }
                if ui.button(t("Remove from the document")).clicked() {
                    clear = true;
                }
            });
        });

        if apply {
            self.bates.pages = Self::page_range_from(self.bates_from);
            let bates = self.bates.clone();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_bates(&bates));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Bates stamp applied to {count} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Bates failed: {err}")),
                None => {}
            }
        }
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_bates());
            match result {
                Some(Ok(removed)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {removed} Bates stamp(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.bates_open = false;
        }
    }
}
