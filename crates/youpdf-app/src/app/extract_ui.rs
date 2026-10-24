//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel extract_ui).


use super::*;

impl YouPdfApp {

        /// Fenêtre « Extraire les données » : tableaux et paires clé/valeur.
        pub(super) fn extract_window(&mut self, ctx: &egui::Context) {
            if !self.extract_open {
                return;
            }
            let mut open = true;
            let mut scan = false;
            let mut export_tables = false;
            let mut export_pairs = false;
            egui::Window::new(t("Extract data"))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size(vec2(560.0, 460.0))
                .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
                .frame(modal_frame(ctx))
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button(t("Scan the document")).clicked() {
                            scan = true;
                        }
                        if ui
                            .add_enabled(
                                !self.extract_tables.is_empty(),
                                egui::Button::new("Tables to CSV…"),
                            )
                            .clicked()
                        {
                            export_tables = true;
                        }
                        if ui
                            .add_enabled(
                                !self.extract_pairs.is_empty(),
                                egui::Button::new("Pairs to CSV…"),
                            )
                            .clicked()
                        {
                            export_pairs = true;
                        }
                    });
                    ui.add_space(SPACE_M);
                    ui.label(
                        RichText::new(format!(
                            "{} table(s), {} key/value pair(s)",
                            self.extract_tables.len(),
                            self.extract_pairs.len()
                        ))
                        .size(LABEL)
                        .color(pal().muted),
                    );
                    ui.add_space(SPACE_M);
                    ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                        for (idx, table) in self.extract_tables.iter().enumerate() {
                            ui.label(
                                RichText::new(format!("Table {} — page {}", idx + 1, table.page))
                                    .size(BODY)
                                    .color(pal().icon_bright),
                            );
                            for row in &table.rows {
                                ui.label(
                                    RichText::new(row.join("  |  ")).size(LABEL).color(pal().icon_bright),
                                );
                            }
                            ui.add_space(SPACE_M);
                        }
                        for pair in &self.extract_pairs {
                            ui.label(
                                RichText::new(format!("p{}  {} = {}", pair.page, pair.key, pair.value))
                                    .size(LABEL)
                                    .color(pal().icon_bright),
                            );
                        }
                    });
                });

            if scan {
                let result = self
                    .tabs
                    .get(self.active)
                    .map(|tab| (tab.doc.extract_tables(), tab.doc.extract_key_values()));
                match result {
                    Some((Ok(tables), Ok(pairs))) => {
                        let count = tables.len() + pairs.len();
                        self.extract_tables = tables;
                        self.extract_pairs = pairs;
                        self.toast(ctx, format!("Found {count} item(s)"));
                    }
                    Some((Err(err), _)) | Some((_, Err(err))) => {
                        self.toast(ctx, format!("Scan failed: {err}"));
                    }
                    None => self.toast(ctx, "Open a document first"),
                }
            }
            if export_tables {
                let mut body = String::new();
                for (idx, table) in self.extract_tables.iter().enumerate() {
                    if idx > 0 {
                        body.push('\n');
                    }
                    for row in &table.rows {
                        let line: Vec<String> = row.iter().map(|cell| youpdf_core::util::csv_escape(cell)).collect();
                        body.push_str(&line.join(","));
                        body.push('\n');
                    }
                }
                self.save_extract(ctx, "tables.csv", &body);
            }
            if export_pairs {
                let mut body = String::from("key,value\n");
                for pair in &self.extract_pairs {
                    body.push_str(&format!("{},{}", youpdf_core::util::csv_escape(&pair.key), youpdf_core::util::csv_escape(&pair.value)));
                    body.push('\n');
                }
                self.save_extract(ctx, "pairs.csv", &body);
            }
            if !open {
                self.extract_open = false;
            }
        }

}
