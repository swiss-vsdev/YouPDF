//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel files_ui).

use super::*;

impl YouPdfApp {

        pub(super) fn open_dialog(&mut self) {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
            {
                self.open_path(path);
            }
        }

        pub(super) fn open_path(&mut self, path: PathBuf) {
            if let Some(idx) = self
                .tabs
                .iter()
                .position(|t| t.path.as_deref() == Some(path.as_path()))
            {
                self.active = idx;
                return;
            }

            // Un fichier chiffré exige son mot de passe avant toute lecture :
            // `Document::open` échouerait sur un flux illisible et l'erreur ne
            // dirait pas à l'utilisateur quoi faire.
            if Document::file_is_encrypted(&path) {
                self.password_prompt = Some(PasswordPrompt {
                    path,
                    purpose: PasswordPurpose::Open,
                    value: String::new(),
                    error: None,
                });
                return;
            }

            match Document::open(&path) {
                Ok(doc) => self.push_document(doc, Some(path)),
                Err(err) => self.toast_owned(format!("Open failed: {err}")),
            }
        }

        /// Ouvre un fichier dans un nouvel onglet, même s'il est déjà ouvert.
        pub(super) fn open_path_new_tab(&mut self, path: PathBuf) {
            if Document::file_is_encrypted(&path) {
                self.password_prompt = Some(PasswordPrompt {
                    path,
                    purpose: PasswordPurpose::Open,
                    value: String::new(),
                    error: None,
                });
                return;
            }
            match Document::open(&path) {
                Ok(doc) => self.push_document(doc, Some(path)),
                Err(err) => self.toast_owned(format!("Open failed: {err}")),
            }
        }

        /// Demande la fermeture d'un onglet.
        ///
        /// Un onglet modifié passe par une confirmation : la fermeture est la
        /// seule action de l'interface qui puisse perdre du travail sans retour.
        pub(super) fn close_tab(&mut self, idx: usize) {
            if idx >= self.tabs.len() {
                return;
            }
            if self.tabs[idx].dirty {
                self.pending_close = Some(idx);
                return;
            }
            self.close_tab_now(idx);
        }

        /// Ferme l'onglet, sans confirmation.
        pub(super) fn close_tab_now(&mut self, idx: usize) {
            if idx >= self.tabs.len() {
                return;
            }
            // Le document affiché change seulement quand on ferme l'onglet actif :
            // fermer un autre onglet décale les index mais garde la même page.
            let was_active = idx == self.active;
            self.tabs.remove(idx);
            if idx < self.active {
                self.active -= 1;
            } else if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
            self.last_active = self.active;
            if was_active {
                self.reset_document_session();
            }
        }

        /// Ferme une liste d'onglets.
        ///
        /// Un onglet modifié exige une confirmation, et `pending_close` n'en porte
        /// qu'une : dès qu'un onglet ciblé est modifié, on refuse le lot et on
        /// laisse l'utilisateur les fermer un par un. Cela évite de fermer des
        /// index décalés par les retraits.
        pub(super) fn close_many(&mut self, ctx: &egui::Context, targets: &[usize]) {
            let dirty = targets
                .iter()
                .any(|&idx| self.tabs.get(idx).is_some_and(|tab| tab.dirty));
            if dirty {
                self.toast(ctx, "Close modified tabs one by one");
                return;
            }
            let mut idxs: Vec<usize> = targets.to_vec();
            idxs.sort_unstable();
            for idx in idxs.into_iter().rev() {
                self.close_tab_now(idx);
            }
        }

        pub(super) fn active_tab(&self) -> Option<&Tab> {
            self.tabs.get(self.active)
        }

        /// ⌘S : écrit le document sur son chemin, ou demande un chemin.
        pub(super) fn save_active(&mut self, ctx: &egui::Context) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(path) = tab.path.clone() else {
                self.save_as(ctx);
                return;
            };
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let annots = annots_to_page(&tab.doc, &tab.annots);
            match tab.doc.save_with_annotations(&path, &annots) {
                Ok(()) => {
                    tab.dirty = false;
                    tab.structural_dirty = false;
                    tab.baseline_annots = tab.annots.clone();
                    tab.history_base = tab.doc.to_bytes().ok();
                    tab.saved_hash = tab
                        .history_base
                        .as_deref()
                        .map(fingerprint)
                        .unwrap_or_default();
                    // L'écriture rétablit l'état réel du fichier : le cadenas
                    // suit, sans quoi il mentirait après une déprotection.
                    tab.protected = Document::file_is_encrypted(&path);
                    self.toast(ctx, format!("Saved {}", path.display()));
                }
                Err(err) => self.toast(ctx, format!("Save failed: {err}")),
            }
        }

        /// ⇧⌘S : choisit une destination puis écrit une copie.
        pub(super) fn save_as(&mut self, ctx: &egui::Context) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let default_name = tab
                .path
                .as_ref()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .unwrap_or_else(|| format!("{}.pdf", tab.title));
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(&default_name)
                .save_file()
            else {
                return;
            };
            let annots = annots_to_page(&tab.doc, &tab.annots);
            match tab.doc.save_with_annotations(&path, &annots) {
                Ok(()) => {
                    tab.dirty = false;
                    tab.structural_dirty = false;
                    tab.baseline_annots = tab.annots.clone();
                    tab.history_base = tab.doc.to_bytes().ok();
                    tab.saved_hash = tab
                        .history_base
                        .as_deref()
                        .map(fingerprint)
                        .unwrap_or_default();
                    tab.path = Some(path.clone());
                    tab.protected = Document::file_is_encrypted(&path);
                    tab.title = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| tab.title.clone());
                    let title = tab.title.clone();
                    self.toast(ctx, format!("Saved {title}"));
                }
                Err(err) => self.toast(ctx, format!("Save failed: {err}")),
            }
        }

        /// Exporte le texte extractible dans un fichier `.txt`.
        /// Confirmation de fermeture d'un onglet modifié.
        pub(super) fn close_confirm_window(&mut self, ctx: &egui::Context) {
            let Some(idx) = self.pending_close else {
                return;
            };
            if idx >= self.tabs.len() {
                self.pending_close = None;
                return;
            }
            let title = self.tabs[idx].title.clone();
            let mut open = true;
            let mut choice: Option<u8> = None;
            self.show_modal(ctx, "modal-close-confirm", "Unsaved changes", 340.0, &mut open, |_app, ui| {
                ui.label(
                    RichText::new(format!("\u{201c}{title}\u{201d} has unsaved changes."))
                        .size(BODY)
                        .color(pal().icon_bright),
                );
                ui.label(
                    RichText::new(t("Closing now would lose them."))
                        .size(LABEL)
                        .color(pal().muted),
                );
                ui.add_space(SPACE_L);
                ui.horizontal(|ui| {
                    if ui.button(t("Save and close")).clicked() {
                        choice = Some(0);
                    }
                    if ui.button(t("Discard and close")).clicked() {
                        choice = Some(1);
                    }
                    if ui.button(t("Cancel")).clicked() {
                        choice = Some(2);
                    }
                });
            });
            match choice {
                Some(0) => {
                    // Enregistrer d'abord : `save_active` peut ouvrir une boîte
                    // de dialogue, l'onglet concerné est donc celui qui est actif.
                    self.active = idx;
                    self.last_active = idx;
                    self.save_active(ctx);
                    if !self.tabs[idx].dirty {
                        self.close_tab_now(idx);
                    }
                    self.pending_close = None;
                }
                Some(1) => {
                    self.close_tab_now(idx);
                    self.pending_close = None;
                }
                Some(_) => self.pending_close = None,
                None => {}
            }
            if !open {
                self.pending_close = None;
            }
        }

        /// Ouvre un fichier avec l'application par défaut du système.
        pub(super) fn open_with_system(&self, path: &std::path::Path) -> std::io::Result<()> {
            #[cfg(target_os = "macos")]
            let child = std::process::Command::new("open").arg(path).spawn();
            #[cfg(target_os = "windows")]
            let child = std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(path)
                .spawn();
            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            let child = std::process::Command::new("xdg-open").arg(path).spawn();
            child.map(|_| ())
        }

        /// Ouvre une adresse avec l'application par défaut du système.
        ///
        /// C'est le pendant de `open_with_system` pour les liens : un clic sur un
        /// lien `/URI` confie l'adresse au navigateur du système.
        pub(super) fn open_url(&self, url: &str) -> std::io::Result<()> {
            #[cfg(target_os = "macos")]
            let child = std::process::Command::new("open").arg(url).spawn();
            #[cfg(target_os = "windows")]
            let child = std::process::Command::new("cmd")
                .args(["/C", "start", ""])
                .arg(url)
                .spawn();
            #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
            let child = std::process::Command::new("xdg-open").arg(url).spawn();
            child.map(|_| ())
        }

        /// Nom de fichier sans extension de l'onglet actif, pour proposer un
        /// nom de sortie (« rapport-protected.pdf »).
        pub(super) fn active_stem(&self) -> Option<String> {
            self.tabs
                .get(self.active)
                .and_then(|tab| tab.path.as_ref())
                .and_then(|path| path.file_stem())
                .map(|stem| stem.to_string_lossy().into_owned())
        }

        /// « Toutes les pages » ou « à partir de N » donnent une plage.
        pub(super) fn page_range_from(from: Option<u32>) -> PageRange {
            match from {
                Some(page) => PageRange::From(page.max(1)),
                None => PageRange::All,
            }
        }

        /// Nombre de pages de l'onglet actif, au moins une.
        pub(super) fn active_total(&self) -> u32 {
            self.tabs
                .get(self.active)
                .map(|tab| tab.total())
                .unwrap_or(0)
                .max(1) as u32
        }

        /// Nouveau document dont chaque page porte une image choisie.
        pub(super) fn import_images(&mut self, ctx: &egui::Context) {
            let Some(paths) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_files()
            else {
                return;
            };
            if paths.is_empty() {
                return;
            }
            match document_from_images(&paths, 300.0) {
                Ok(doc) => {
                    let pages = paths.len();
                    self.push_document(doc, None);
                    self.create_open = false;
                    self.toast(ctx, format!("{pages} page(s) created from images"));
                }
                Err(err) => self.toast(ctx, format!("Create failed: {err}")),
            }
        }

        /// Nouveau document à partir d'un fichier Office (.docx/.xlsx/.pptx).
        ///
        /// ponytail: convertit par LibreOffice (`soffice --convert-to pdf`)
        /// si le logiciel est installé ; sinon affiche un message d'erreur
        /// explicite. Le PDF intermédiaire est ouvert comme un document
        /// normal : il n'est pas supprimé (l'utilisateur peut le retrouver
        /// à côté du fichier source).
        pub(super) fn import_office(&mut self, ctx: &egui::Context) {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("Office files", &["docx", "xlsx", "pptx"])
                .pick_file()
            else {
                return;
            };
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "office".to_owned());
            let dest = std::env::temp_dir().join(format!("youpdf-{stem}.pdf"));
            match youpdf_core::import_office(&path, &dest) {
                Ok(()) => match Document::open(&dest) {
                    Ok(doc) => {
                        self.push_document(doc, Some(dest.clone()));
                        self.create_open = false;
                        self.toast(ctx, format!("Imported from {}", path.display()));
                    }
                    Err(err) => self.toast(ctx, format!("Imported PDF unreadable: {err}")),
                },
                Err(err) => self.toast(ctx, format!("Office import failed: {err}")),
            }
        }

        /// Nouveau document à partir d'une capture d'écran ou du presse-papiers.
        ///
        /// ponytail: macOS seulement (`screencapture` + `osascript`). Ajouter les
        /// branches `SnippingTool.exe /clip` / `flameshot gui -r` si le support
        /// multiplateforme devient nécessaire.
        pub(super) fn import_capture(&mut self, ctx: &egui::Context, from_clipboard: bool) {
            let bytes = if from_clipboard {
                youpdf_core::capture::clipboard_as_png()
            } else {
                // Le sélecteur interactif bloque la boucle UI : acceptable pour
                // une capture, et il évite de toucher à l'état avant le résultat.
                youpdf_core::capture::capture_region_interactive()
                    .map(|path| std::fs::read(&path).ok())
                    .ok()
                    .flatten()
                    .map_or_else(
                        || Err(youpdf_shared::Error::InvalidInput("capture annulée".into())),
                        Ok,
                    )
            };
            match bytes.and_then(|bytes| {
                document_from_image_bytes(&[("capture".to_owned(), bytes)], 300.0)
            }) {
                Ok(doc) => {
                    self.push_document(doc, None);
                    self.create_open = false;
                    self.toast(ctx, "One page created from capture".to_owned());
                }
                Err(err) => self.toast(ctx, format!("Capture failed: {err}")),
            }
        }

        /// Insère un autre PDF à la fin du document courant.
        pub(super) fn combine_active(&mut self, ctx: &egui::Context) {
            if self.tabs.is_empty() {
                self.toast(ctx, "Open a document first");
                return;
            }
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
            else {
                return;
            };
            let other = match Document::open(&path) {
                Ok(doc) => doc,
                Err(err) => {
                    self.toast(ctx, format!("Open failed: {err}"));
                    return;
                }
            };
            let after = self.tabs.get(self.active).map(|tab| tab.total()).unwrap_or(0);
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.insert_document(after, &other));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) appended"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Combine failed: {err}")),
                None => {}
            }
        }

        /// Envoie le document à l'imprimante par défaut du système.
        ///
        /// Utilise `lp` : pas d'aperçu, pas de boîte de dialogue de réglages.
        /// Le fichier est écrit dans un temporaire, annotations en attente
        /// comprises, puis remis au spouleur.
        pub(super) fn print_active(&mut self, ctx: &egui::Context) {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            let bytes = match tab.doc.to_bytes() {
                Ok(bytes) => bytes,
                Err(err) => {
                    self.toast(ctx, format!("Print failed: {err}"));
                    return;
                }
            };
            let tmp = std::env::temp_dir().join(format!("youpdf-print-{}.pdf", std::process::id()));
            if let Err(err) = std::fs::write(&tmp, &bytes) {
                self.toast(ctx, format!("Print failed: {err}"));
                return;
            }
            match std::process::Command::new("lp").arg(&tmp).output() {
                Ok(output) if output.status.success() => {
                    self.toast(ctx, "Sent to the default printer");
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    self.toast(ctx, format!("Print failed: {}", stderr.trim()));
                }
                Err(err) => self.toast(ctx, format!("Print unavailable: {err}")),
            }
        }

        /// Révèle un fichier dans le gestionnaire de fichiers du système.
        pub(super) fn reveal_path(&self, path: &std::path::Path) {
            #[cfg(target_os = "macos")]
            let command = std::process::Command::new("open").arg("-R").arg(path).spawn();
            #[cfg(not(target_os = "macos"))]
            let command = std::process::Command::new("xdg-open")
                .arg(path.parent().unwrap_or(path))
                .spawn();
            let _ = command;
        }

        /// Révèle le fichier courant dans le gestionnaire de fichiers.
        pub(super) fn reveal_active(&mut self, ctx: &egui::Context) {
            let Some(path) = self.tabs.get(self.active).and_then(|tab| tab.path.clone()) else {
                self.toast(ctx, "This document has no file on disk yet");
                return;
            };
            #[cfg(target_os = "macos")]
            let command = std::process::Command::new("open").arg("-R").arg(&path).spawn();
            #[cfg(not(target_os = "macos"))]
            let command = std::process::Command::new("xdg-open")
                .arg(path.parent().unwrap_or(&path))
                .spawn();
            match command {
                Ok(_) => {}
                Err(err) => self.toast(ctx, format!("Reveal failed: {err}")),
            }
        }

        /// Enregistre une copie déclarée conforme (PDF/A, PDF/X, PDF/E).
        pub(super) fn conformance_save(&mut self, ctx: &egui::Context, level: Conformance) {
            let name = self
                .active_stem()
                .map(|stem| format!("{stem}-{}.pdf", level.display_name().replace('/', "-")))
                .unwrap_or_else(|| "conformant.pdf".to_owned());
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(&name)
                .save_file()
            else {
                return;
            };
            let result =
                self.with_annotations_applied(|tab| tab.doc.save_with_conformance(&path, level));
            match result {
                Some(Ok(())) => {
                    self.toast(
                        ctx,
                        format!(
                            "{} copy written (markers only — validate it externally)",
                            level.display_name()
                        ),
                    );
                }
                Some(Err(err)) => self.toast(ctx, format!("Save failed: {err}")),
                None => {}
            }
        }

        /// Enregistre un PDF dont chaque page est l'image rendue de la page.
        pub(super) fn image_pdf_save(&mut self, ctx: &egui::Context) {
            let name = self
                .active_stem()
                .map(|stem| format!("{stem}-images.pdf"))
                .unwrap_or_else(|| "images.pdf".to_owned());
            let Some(path) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name(&name)
                .save_file()
            else {
                return;
            };
            let renderer = &*self.renderer;
            let result = self.tabs.get_mut(self.active).map(|tab| {
                // Les annotations en attente font partie de l'image produite.
                let page_annots = annots_to_page(&tab.doc, &tab.annots);
                let _ = tab.doc.apply_annotations(&page_annots);
                let outcome = tab.doc.save_image_pdf(&path, renderer, 150.0);
                let _ = tab.doc.clear_youpdf_annotations();
                outcome
            });
            match result {
                Some(Ok(())) => self.toast(ctx, "Image PDF written"),
                Some(Err(err)) => self.toast(ctx, format!("Save failed: {err}")),
                None => {}
            }
        }

        /// Écrit un fichier d'extraction choisi par l'utilisateur.
        pub(super) fn save_extract(&mut self, ctx: &egui::Context, name: &str, body: &str) {
            let Some(path) = rfd::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name(name)
                .save_file()
            else {
                return;
            };
            match std::fs::write(&path, body) {
                Ok(()) => self.toast(ctx, "Extraction exported"),
                Err(err) => self.toast(ctx, format!("Export failed: {err}")),
            }
        }

        pub(super) fn handle_drops(&mut self, ctx: &egui::Context) {
            let dropped: Vec<PathBuf> = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .filter_map(|f| f.path.clone())
                    .collect()
            });
            // Une fenêtre de lot ouverte prend les fichiers déposés : les
            // ajouter à la file est ce qu'on attend d'un glissé sur elle, alors
            // qu'ouvrir chaque PDF l'un après l'autre perdrait le lot en cours.
            if self.batch_open {
                for path in dropped {
                    if path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
                    {
                        self.push_batch_file(path);
                    } else {
                        self.open_path(path);
                    }
                }
                return;
            }
            for path in dropped {
                self.open_path(path);
            }
        }

}
