//! Fenêtres d'habillage : filigrane, en-tête et pied de page, compression.

use super::*;
use youpdf_shared::t;

impl YouPdfApp {
    /// Fenêtre « Filigrane ».
    pub(super) fn watermark_window(&mut self, ctx: &egui::Context) {
        if !self.watermark_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        self.show_modal(ctx, "modal-watermark", "Watermark", 360.0, &mut open, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Text")).size(12.0).color(pal().muted));
                ui.add(TextEdit::singleline(&mut app.watermark.text).desired_width(230.0));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Size")).size(12.0).color(pal().muted));
                ui.add(
                    egui::DragValue::new(&mut app.watermark.font_size).range(6.0..=200.0),
                );
                ui.label(RichText::new(t("Angle")).size(12.0).color(pal().muted));
                ui.add(
                    egui::DragValue::new(&mut app.watermark.rotation_deg).range(-90.0..=90.0),
                );
                ui.checkbox(&mut app.watermark.tiled, t("Tiled"));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Position")).size(12.0).color(pal().muted));
                use youpdf_core::watermark::WatermarkPosition as Pos;
                let current = app.watermark.position;
                egui::ComboBox::from_id_salt("wm-position")
                    .width(104.0)
                    .selected_text(format!("{current:?}"))
                    .show_ui(ui, |ui| {
                        for value in [
                            Pos::TopLeft,
                            Pos::TopCenter,
                            Pos::TopRight,
                            Pos::MiddleLeft,
                            Pos::Center,
                            Pos::MiddleRight,
                            Pos::BottomLeft,
                            Pos::BottomCenter,
                            Pos::BottomRight,
                        ] {
                            if ui
                                .selectable_label(app.watermark.position == value, format!("{value:?}"))
                                .clicked()
                            {
                                app.watermark.position = value;
                            }
                        }
                    });
                ui.label(RichText::new(t("Offset")).size(12.0).color(pal().muted));
                ui.add(
                    egui::DragValue::new(&mut app.watermark.offset_x)
                        .range(-400.0..=400.0)
                        .prefix("x "),
                );
                ui.add(
                    egui::DragValue::new(&mut app.watermark.offset_y)
                        .range(-400.0..=400.0)
                        .prefix("y "),
                );
            });
            if app.watermark.tiled {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("Tile gap")).size(12.0).color(pal().muted));
                    ui.add(
                        egui::DragValue::new(&mut app.watermark.tile_gap).range(0.0..=400.0),
                    );
                });
            }
            ui.checkbox(&mut app.watermark.behind, t("Behind page content"));
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Opacity")).size(12.0).color(pal().muted));
                ui.add(
                    egui::Slider::new(&mut app.watermark.opacity, 0.05..=1.0).show_value(false),
                );
                ui.label(
                    RichText::new(format!("{:.0} %", app.watermark.opacity * 100.0))
                        .size(12.0)
                        .color(pal().icon_bright),
                );
                ui.label(RichText::new(t("Grey")).size(12.0).color(pal().muted));
                let mut grey = app.watermark.color[0];
                if ui
                    .add(egui::Slider::new(&mut grey, 0..=255).show_value(false))
                    .changed()
                {
                    app.watermark.color = [grey, grey, grey];
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Pages")).size(12.0).color(pal().muted));
                let mut all = app.watermark_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.watermark_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.watermark_from.as_mut() {
                    ui.label(RichText::new(t("then from")).size(12.0).color(pal().muted));
                    ui.add(egui::DragValue::new(from).range(1..=total));
                }
            });
            ui.add_space(8.0);
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
            self.watermark.pages = Self::page_range_from(self.watermark_from);
            let wm = self.watermark.clone();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_text_watermark(&wm));
            match result {
                Some(Ok(pages)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Watermark applied to {pages} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Watermark failed: {err}")),
                None => {}
            }
        }
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_watermarks());
            match result {
                Some(Ok(removed)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {removed} decoration(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.watermark_open = false;
        }
    }

    /// Fenêtre « En-tête et pied de page ».
    pub(super) fn header_footer_window(&mut self, ctx: &egui::Context) {
        if !self.header_footer_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        self.show_modal(ctx, "modal-header-footer", "Header & footer", 460.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Each band has its own three slots; the page number joins the \
                     footer centre.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(4.0);
            for (label, slots) in [
                ("Header", &mut app.header_footer.header),
                ("Footer", &mut app.header_footer.footer),
            ] {
                ui.label(RichText::new(label).size(11.5).color(pal().icon_bright));
                egui::Grid::new(format!("hf-grid-{label}"))
                    .num_columns(3)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new(t("Left")).size(11.0).color(pal().muted));
                        ui.label(RichText::new(t("Centre")).size(11.0).color(pal().muted));
                        ui.label(RichText::new(t("Right")).size(11.0).color(pal().muted));
                        ui.end_row();
                        ui.add(TextEdit::singleline(&mut slots.left).desired_width(140.0));
                        ui.add(TextEdit::singleline(&mut slots.center).desired_width(140.0));
                        ui.add(TextEdit::singleline(&mut slots.right).desired_width(140.0));
                        ui.end_row();
                    });
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Page numbers")).size(12.0).color(pal().muted));
                for (text, style) in [
                    ("None", None),
                    ("1", Some(NumberStyle::Numeric)),
                    ("1 / n", Some(NumberStyle::NumericTotal)),
                    ("i", Some(NumberStyle::Roman)),
                ] {
                    if ui
                        .selectable_label(app.header_footer.page_numbers == style, text)
                        .clicked()
                    {
                        app.header_footer.page_numbers = style;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Start at")).size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.header_footer.start_page).range(1..=total));
                ui.checkbox(&mut app.header_footer.skip_first_page, t("Skip first page"));
                ui.label(RichText::new(t("Size")).size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.header_footer.font_size).range(4.0..=36.0));
                ui.label(RichText::new(t("Margin")).size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.header_footer.margin).range(0.0..=120.0));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new(t("Pages")).size(12.0).color(pal().muted));
                let mut all = app.header_footer_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.header_footer_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.header_footer_from.as_mut() {
                    ui.label(RichText::new(t("then from")).size(12.0).color(pal().muted));
                    ui.add(egui::DragValue::new(from).range(1..=total));
                }
            });
            ui.add_space(8.0);
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
            self.header_footer.pages = Self::page_range_from(self.header_footer_from);
            let hf = self.header_footer.clone();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_header_footer(&hf));
            match result {
                Some(Ok(pages)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Band applied to {pages} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Header and footer failed: {err}")),
                None => {}
            }
        }
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_watermarks());
            match result {
                Some(Ok(removed)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {removed} decoration(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.header_footer_open = false;
        }
    }

    /// Fenêtre « Compresser ».
    pub(super) fn compress_window(&mut self, ctx: &egui::Context) {
        if !self.compress_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        let mut estimate = false;
        self.show_modal(ctx, "modal-compress", "Compress", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Streams are deflated and unreachable objects are dropped.                          Fonts are not subset, so a text-only file barely shrinks.",
                )
                .size(12.0)
                .color(pal().muted),
            );
            ui.add_space(8.0);
            ui.label(RichText::new(t("Image quality")).size(12.0).color(pal().muted));
            for level in CompressionLevel::ALL {
                if ui
                    .selectable_label(app.compress_level == level, level.display_name())
                    .clicked()
                {
                    app.compress_level = level;
                }
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Images are resampled to the chosen resolution and re-encoded as JPEG.                          A stream is replaced only when the result is smaller.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(6.0);
            ui.label(RichText::new(t("Also remove")).size(12.0).color(pal().muted));
            ui.checkbox(&mut app.compress_extras.strip_metadata, t("Metadata and XMP"));
            ui.checkbox(&mut app.compress_extras.strip_thumbnails, t("Page thumbnails"));
            ui.checkbox(&mut app.compress_extras.strip_bookmarks, t("Bookmarks"));
            ui.checkbox(
                &mut app.compress_extras.strip_structure,
                "Tagged structure tree",
            );
            ui.horizontal(|ui| {
                if ui.button(t("Estimate")).clicked() {
                    estimate = true;
                }
                if let Some((before, after)) = app.compress_estimate {
                    let delta = if after <= before {
                        format!("-{:.0}%", (1.0 - after as f64 / before.max(1) as f64) * 100.0)
                    } else {
                        format!("+{:.0}%", (after as f64 / before.max(1) as f64 - 1.0) * 100.0)
                    };
                    ui.label(
                        RichText::new(format!(
                            "{:.0} KB → {:.0} KB ({delta})",
                            before as f64 / 1024.0,
                            after as f64 / 1024.0
                        ))
                        .size(11.0)
                        .color(pal().icon_bright),
                    );
                }
            });
            if let Some(report) = app.compress_report.clone() {
                ui.add_space(6.0);
                ui.label(RichText::new(report).size(12.0).color(pal().icon_bright));
            }
            ui.add_space(8.0);
            if ui.button(t("Save compressed copy…")).clicked() {
                run = true;
            }
        });
        if run {
            self.compress_active(ctx);
        }
        if estimate {
            let level = self.compress_level.options();
            let extras = self.compress_extras;
            if let Some(tab) = self.tabs.get(self.active) {
                match tab.doc.estimate_compressed(&level, &extras) {
                    Ok(after) => {
                        let before = tab.doc.to_bytes().map(|bytes| bytes.len()).unwrap_or(after);
                        self.compress_estimate = Some((before, after));
                    }
                    Err(err) => self.toast(ctx, format!("Estimate failed: {err}")),
                }
            }
        }
        if !open {
            self.compress_open = false;
        }
    }


    /// Écrit une copie compressée du document, et retient le bilan.
    pub(super) fn compress_active(&mut self, ctx: &egui::Context) {
        let name = self
            .active_stem()
            .map(|stem| format!("{stem}-compressed.pdf"))
            .unwrap_or_else(|| "compressed.pdf".to_owned());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(&name)
            .save_file()
        else {
            return;
        };
        let level = self.compress_level;
        let extras = self.compress_extras;
        let result = self.with_annotations_applied(|tab| {
            tab.doc.compress_with_extras(&path, &level.options(), &extras)
        });
        match result {
            Some(Ok(report)) => {
                let before = report.before_bytes as f64 / 1024.0;
                let after = report.after_bytes as f64 / 1024.0;
                self.compress_report = Some(format!(
                    "{before:.0} KB → {after:.0} KB ({} byte(s) saved, {} image(s) resampled)",
                    report.saved_bytes(),
                    report.images_resampled
                ));
                self.mark_document_changed();
                let saved = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "the copy".to_owned());
                self.toast(ctx, format!("Compressed copy saved: {saved}"));
            }
            Some(Err(err)) => self.toast(ctx, format!("Compress failed: {err}")),
            None => {}
        }
    }
}
