//! Fenêtre d'export et écriture des formats de sortie.

use super::*;
use youpdf_shared::t;

impl YouPdfApp {
    /// Fenêtre « Exporter » : choix du format de sortie.
    pub(super) fn export_window(&mut self, ctx: &egui::Context) {
        if !self.export_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        self.show_modal(ctx, "modal-export", "Export", 380.0, &mut open, |app, ui| {
            ui.label(RichText::new(t("Text")).size(12.0).color(pal().muted));
            for format in [
                TextFormat::Plain,
                TextFormat::Markdown,
                TextFormat::Html,
                TextFormat::Csv,
                TextFormat::Xml,
                TextFormat::Rtf,
            ] {
                if ui
                    .selectable_label(
                        app.export_mode == ExportMode::Text && app.export_format == format,
                        format.display_name(),
                    )
                    .clicked()
                {
                    app.export_mode = ExportMode::Text;
                    app.export_format = format;
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new(t("Images")).size(12.0).color(pal().muted));
            for (format, label) in [
                (ImageFormat::Png, "PNG (one file per page)"),
                (ImageFormat::Jpeg, "JPEG (one file per page)"),
                (ImageFormat::Tiff, "TIFF (one file per page)"),
            ] {
                if ui
                    .selectable_label(
                        app.export_mode == ExportMode::Image
                            && app.export_image_format == format,
                        label,
                    )
                    .clicked()
                {
                    app.export_mode = ExportMode::Image;
                    app.export_image_format = format;
                }
            }
            if ui
                .selectable_label(
                    app.export_mode == ExportMode::LongImage,
                    "Long image (all pages stacked, PNG)",
                )
                .clicked()
            {
                app.export_mode = ExportMode::LongImage;
            }
            if matches!(app.export_mode, ExportMode::Image | ExportMode::LongImage) {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t("Resolution")).size(12.0).color(pal().muted));
                    ui.add(
                        egui::DragValue::new(&mut app.export_dpi)
                            .range(36.0..=600.0)
                            .suffix(" dpi"),
                    );
                });
            }
            ui.add_space(6.0);
            ui.label(RichText::new(t("Embedded images")).size(12.0).color(pal().muted));
            if ui
                .selectable_label(
                    app.export_mode == ExportMode::Embedded,
                    "PNG (one file per embedded image)",
                )
                .clicked()
            {
                app.export_mode = ExportMode::Embedded;
            }
            ui.add_space(6.0);
            ui.label(RichText::new(t("Office")).size(12.0).color(pal().muted));
            for format in OfficeFormat::ALL {
                if ui
                    .selectable_label(
                        app.export_mode == ExportMode::Office
                            && app.export_office_format == format,
                        format.display_name(),
                    )
                    .clicked()
                {
                    app.export_mode = ExportMode::Office;
                    app.export_office_format = format;
                }
            }
            if app.export_mode == ExportMode::Office
                && app.export_office_format == OfficeFormat::Word
            {
                ui.checkbox(
                    &mut app.export_office_exact,
                    "Exact layout (position each text run)",
                );
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(match app.export_mode {
                    ExportMode::Image => {
                        "Each page is rendered at the chosen resolution and written into a \
                         folder as page-0001, page-0002…"
                    }
                    ExportMode::LongImage => {
                        "Every page is rendered at the chosen resolution and stacked into \
                         one tall PNG, handy for scrolling on a phone."
                    }
                    ExportMode::Office => {
                        "The text layer is rebuilt as an Office document: one paragraph or \
                         row per line, one slide per page. Images, tables and the layout are \
                         not reconstructed."
                    }
                    ExportMode::Embedded => {
                        "Each image embedded in the document is written as a PNG. Rendered \
                         page pictures are not included — use Images for those."
                    }
                    ExportMode::Text => {
                        "Only the text layer is exported: images, tables and the page layout \
                         are not rebuilt, and pages without extractable text come out empty."
                    }
                })
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(8.0);
            if ui.button(t("Export…")).clicked() {
                run = true;
            }
        });
        if run {
            self.export_active(ctx);
        }
        if !open {
            self.export_open = false;
        }
    }

    /// Écrit la couche texte du document dans le format choisi, les pages en
    /// images, ou un document bureautique.
    pub(super) fn export_active(&mut self, ctx: &egui::Context) {
        if self.export_mode == ExportMode::LongImage {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PNG", &["png"])
                .set_file_name("long-page.png")
                .save_file()
            else {
                return;
            };
            let dpi = self.export_dpi;
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_long_image(&path, dpi));
            match result {
                Some(Ok(())) => self.toast(ctx, format!("Saved {}", path.display())),
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
        if self.export_mode == ExportMode::Embedded {
            let Some(directory) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let result = self.tabs.get(self.active).map(|tab| {
                let mut written = 0usize;
                let mut skipped = 0usize;
                for page in 1..=tab.total() as u32 {
                    let images = tab.doc.extract_images(page).unwrap_or_default();
                    for (index, info) in images.iter().enumerate() {
                        match tab.doc.extract_image_png(info) {
                            Ok(bytes) => {
                                let name =
                                    format!("page{page:04}-image{:02}.png", index + 1);
                                if std::fs::write(directory.join(name), bytes).is_ok() {
                                    written += 1;
                                } else {
                                    skipped += 1;
                                }
                            }
                            Err(_) => skipped += 1,
                        }
                    }
                }
                (written, skipped)
            });
            if let Some((written, skipped)) = result {
                let name = directory
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "the folder".to_owned());
                self.toast(
                    ctx,
                    format!("{written} embedded image(s) written to {name} ({skipped} skipped)"),
                );
            }
            return;
        }
        if self.export_mode == ExportMode::Office {
            let format = self.export_office_format;
            let stem = self
                .active_stem()
                .or_else(|| self.tabs.get(self.active).map(|tab| tab.title.clone()))
                .unwrap_or_else(|| "document".to_owned());
            let Some(path) = rfd::FileDialog::new()
                .add_filter(format.display_name(), &[format.extension()])
                .set_file_name(format!("{stem}.{}", format.extension()))
                .save_file()
            else {
                return;
            };
            let layout = if self.export_office_exact {
                youpdf_core::office::OfficeLayout::Exact
            } else {
                youpdf_core::office::OfficeLayout::Flow
            };
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_office_layout(&path, format, layout));
            match result {
                Some(Ok(())) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "the file".to_owned());
                    self.toast(ctx, format!("Exported {name}"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
        if self.export_mode == ExportMode::Image {
            let format = self.export_image_format;
            let dpi = self.export_dpi;
            let Some(directory) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_images(&directory, format, dpi));
            match result {
                Some(Ok(paths)) => {
                    let name = directory
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "the folder".to_owned());
                    self.toast(ctx, format!("{} image(s) written to {name}", paths.len()));
                }
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
        let format = self.export_format;
        let stem = self
            .active_stem()
            .or_else(|| self.tabs.get(self.active).map(|tab| tab.title.clone()))
            .unwrap_or_else(|| "document".to_owned());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Export", &[format.extension()])
            .set_file_name(format!("{stem}.{}", format.extension()))
            .save_file()
        else {
            return;
        };
        let result = self
            .tabs
            .get(self.active)
            .map(|tab| tab.doc.export_text(format));
        match result {
            Some(Ok(text)) => {
                let pages = self.tabs.get(self.active).map(|tab| tab.total()).unwrap_or(0);
                let chars = text.chars().count();
                match std::fs::write(&path, text) {
                    Ok(()) => {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "the file".to_owned());
                        self.toast(
                            ctx,
                            format!("Exported {pages} page(s), {chars} characters to {name}"),
                        );
                    }
                    Err(err) => self.toast(ctx, format!("Export failed: {err}")),
                }
            }
            Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
            None => {}
        }
    }
}
