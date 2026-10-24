//! Méthodes de `YouPdfApp` extraites de `app.rs` (panel toolbar_ui).


use super::*;

impl YouPdfApp {

        pub(super) fn toolbar(&mut self, ctx: &egui::Context) {
            // Sur l'accueil (aucun document, ou accueil affiché), pas de barres
            // flottantes : elles n'auraient aucun document sur quoi agir.
            if self.home_open || self.tabs.is_empty() {
                return;
            }
            // Les boîtes de dialogue de page (organiser, extraire, etc.) masquent
            // les barres flottantes : elles gèrent leur propre interface.
            if self.organize_open
                || self.crop_open
                || self.watermark_open
                || self.background_open
                || self.header_footer_open
                || self.bates_open
                || self.compress_open
                || self.protect_open
                || self.sign_open
            {
                return;
            }
            let top = TITLE_H + 8.0;
            // Les barres flottantes sont ancrées au canevas, pas à la fenêtre :
            // le panneau latéral les décale (comme dans la référence).
            let canvas_left = RAIL_W
                + if self.panel.is_some() { self.panel_w } else { 0.0 };

            // `Outils ▾` à gauche.
            Area::new(Id::new("tools"))
                .fixed_pos(pos2(canvas_left + 10.0, top))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    let inner = Frame::none()
                        .fill(pal().widget_bg)
                        .rounding(Rounding::same(RADIUS_L))
                        .inner_margin(Margin::symmetric(10.0, 5.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (rect, _) =
                                    ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                                icon_tools(ui.painter(), rect, pal().icon_bright);
                                ui.add_space(SPACE_M);
                                ui.label(RichText::new(t("Tools")).size(BODY).color(pal().icon_bright));
                                ui.add_space(SPACE_S);
                                let (rect, _) =
                                    ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
                                icon_chevron(ui.painter(), rect, pal().muted);
                            });
                        });
                    let rect = inner.response.rect;
                    let resp = ui.interact(rect, Id::new("tools-chip"), Sense::click());
                    if resp.clicked() {
                        self.tools_open = !self.tools_open;
                    }
                });

            // Pastilles de mode au centre.
            // Pastilles de mode, ou barre d'outils du mode actif.
            let modes_anchor = vec2(canvas_left / 2.0, top);
            if let Some(mode) = self.mode {
                let mut exit = false;
                let mut chosen_tool: Option<(usize, bool)> = None;
                let annot_tool = self.annot_tool;
                Area::new(Id::new("modetools"))
                    .anchor(Align2::CENTER_TOP, modes_anchor)
                    .order(Order::Foreground)
                    .show(ctx, |ui| {
                        Frame::none()
                            .fill(pal().widget_bg)
                            .rounding(Rounding::same(RADIUS_L))
                            .inner_margin(Margin::symmetric(4.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    // Les modes à libellés longs gardent le texte ;
                                    // les autres restent compacts, info-bulle au survol.
                                    let labeled = mode == 1 || mode == 3;
                                    for (idx, ModeTool(label, draw, chevron, primary)) in
                                        mode_tools(mode).iter().enumerate()
                                    {
                                        let selected = annot_tool == Some(idx);
                                        let resp = if labeled {
                                            pill_button(ui, label, *draw, *chevron, *primary, selected)
                                        } else {
                                            icon_pill(ui, *draw, *chevron, *primary, selected)
                                        };
                                        let hovered = resp.on_hover_text(*label);
                                        if hovered.clicked() {
                                            let in_chevron = hovered
                                                .interact_pointer_pos()
                                                .map(|p| p.x > hovered.rect.max.x - 20.0)
                                                .unwrap_or(false);
                                            chosen_tool = Some((idx, *chevron && in_chevron));
                                        }
                                    }
                                    if pill_button(ui, "Close", icon_close, false, false, false)
                                        .clicked()
                                    {
                                        exit = true;
                                    }
                                });
                            });
                    });
                if let Some((idx, want_preset)) = chosen_tool {
                    self.annot_tool = Some(idx);
                    // La sélection de texte n'appartient qu'à l'outil Sélection.
                    self.text_sel = None;
                    if want_preset {
                        self.preset_open = true;
                    } else if !mode_tools(mode).get(idx).map(|t| t.2).unwrap_or(false) {
                        self.preset_open = false;
                    }
                    match mode {
                        0 => {}
                        1 => match idx {
                            // Texte, Image et Lien agissent sur le canevas :
                            // rien à faire au moment du clic.
                            1..=3 => {}
                            4 => self.toast(
                                ctx,
                                "Layout: reflowing a block\u{2019}s text is not available; \
                                 use Text to change its wording",
                            ),
                            _ => {}
                        },
                        3 => match idx {
                            1 => {}
                            2 => {
                                self.panel = Some(Panel::Search);
                                self.redact_modal = false;
                            }
                            3 => {
                                let cleaned = self
                                    .tabs
                                    .get_mut(self.active)
                                    .map(|tab| {
                                        let removed = tab.doc.sanitize().unwrap_or(0);
                                        if removed > 0 {
                                            tab.dirty = true;
                                            tab.structural_dirty = true;
                                        }
                                        removed
                                    })
                                    .unwrap_or(0);
                                self.toast(ctx, format!("{cleaned} metadata entries removed"));
                            }
                            4 => {
                                if self.redact_marks.is_empty() {
                                    self.toast(ctx, "Mark areas first");
                                } else {
                                    self.redact_phase = RedactPhase::Marking;
                                    self.redact_modal = true;
                                }
                            }
                            _ => {}
                        },
                        2 => match idx {
                            // Select et les champs créables travaillent au clic.
                            0..=5 | 9 => {}
                            _ => self.toast(
                                ctx,
                                format!(
                                    "{} is not available yet",
                                    mode_tools(mode).get(idx).map(|t| t.0).unwrap_or("Tool")
                                ),
                            ),
                        },
                        _ => {}
                    }
                }
                if exit {
                    self.mode = None;
                    self.annot_tool = None;
                    self.preset_open = false;
                    self.text_sel = None;
                }
            } else {
                Area::new(Id::new("modes"))
                    .anchor(Align2::CENTER_TOP, modes_anchor)
                    .order(Order::Foreground)
                    .show(ctx, |ui| {
                        Frame::none()
                            .fill(pal().widget_bg)
                            .rounding(Rounding::same(RADIUS_L))
                            .inner_margin(Margin::symmetric(4.0, 4.0))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    for (idx, (label, draw)) in MODES.iter().enumerate() {
                                        let selected = self.mode == Some(idx);
                                        let resp = pill_button(
                                            ui, label, *draw, false, false, selected,
                                        );
                                        if resp.clicked() {
                                            self.mode = Some(idx);
                                            self.tools_open = false;
                                            self.annot_tool = None;
                                            self.preset_open = false;
                                            self.text_sel = None;
                                        }
                                    }
                                });
                            });
                    });
            }

            // Liste déroulante `Outils`.
            if self.tools_open {
                let mut chosen: Option<ToolsEntry> = None;
                Area::new(Id::new("tools-menu"))
                    .fixed_pos(pos2(canvas_left + 10.0, top + 38.0))
                    .order(Order::Foreground)
                    .show(ctx, |ui| {
                        Frame::none()
                            .fill(pal().chrome_bg)
                            .rounding(Rounding::same(RADIUS_L))
                            .stroke(Stroke::new(1.0_f32, pal().separator))
                            .inner_margin(Margin::symmetric(4.0, 4.0))
                            .show(ui, |ui| {
                                for (idx, (label, shortcut, entry)) in TOOLS_MENU.iter().enumerate() {
                                    // Deux séparateurs : après les quatre modes, et
                                    // avant la conversion et l'entretien.
                                    if idx == 4 || idx == 11 {
                                        ui.add_space(SPACE_S);
                                        let sep = ui.cursor().top();
                                        ui.painter().hline(
                                            ui.max_rect().x_range(),
                                            sep,
                                            Stroke::new(1.0_f32, pal().separator),
                                        );
                                        ui.add_space(SPACE_S);
                                    }
                                    if menu_row(ui, t(label), shortcut, None) {
                                        chosen = Some(*entry);
                                    }
                                }
                            });
                    });
                if let Some(entry) = chosen {
                    self.tools_open = false;
                    match entry {
                        ToolsEntry::Mode(idx) => {
                            self.mode = Some(idx);
                            self.annot_tool = None;
                            self.preset_open = false;
                            self.text_sel = None;
                        }
                        ToolsEntry::Organize => self.organize_open = true,
                        ToolsEntry::TileSplit => self.tile_split_open = true,
                        ToolsEntry::Crop => self.crop_open = true,
                        ToolsEntry::Watermark => self.watermark_open = true,
                        ToolsEntry::Background => self.background_open = true,
                        ToolsEntry::HeaderFooter => self.header_footer_open = true,
                        ToolsEntry::Bates => self.bates_open = true,
                        ToolsEntry::Measure => self.measure_open = true,
                        ToolsEntry::Ocr => self.ocr_open = true,
                        ToolsEntry::Convert => self.export_open = true,
                        ToolsEntry::Create => self.create_open = true,
                        ToolsEntry::Compare => self.compare_open = true,
                        ToolsEntry::Protect => self.protect_open = true,
                        ToolsEntry::Compress => self.compress_open = true,
                        ToolsEntry::Flatten => self.flatten_annotations(ctx),
                        ToolsEntry::Sign => self.sign_open = true,
                        ToolsEntry::CreateSignature => self.marks.signature_open = true,
                        ToolsEntry::CustomStamp => self.marks.stamp_open = true,
                    }
                }
            }

            self.preset_popup(ctx, canvas_left, top);

            // Annuler / rétablir / débordement à droite.
            Area::new(Id::new("actions"))
                .anchor(Align2::RIGHT_TOP, vec2(-14.0, top))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(pal().widget_bg)
                        .rounding(Rounding::same(RADIUS_L))
                        .inner_margin(Margin::symmetric(6.0, 4.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let can_undo = self
                                    .tabs
                                    .get(self.active)
                                    .map(|t| !t.undo.is_empty())
                                    .unwrap_or(false);
                                let can_redo = self
                                    .tabs
                                    .get(self.active)
                                    .map(|t| !t.redo.is_empty())
                                    .unwrap_or(false);
                                let undo = {
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                    let color = if !can_undo {
                                        pal().muted
                                    } else if resp.hovered() {
                                        pal().icon_bright
                                    } else {
                                        pal().icon
                                    };
                                    icon_undo(ui.painter(), rect.shrink(6.0), color);
                                    resp
                                };
                                if undo.clicked() && can_undo {
                                    self.undo(ctx);
                                }
                                let redo = {
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                    let color = if !can_redo {
                                        pal().muted
                                    } else if resp.hovered() {
                                        pal().icon_bright
                                    } else {
                                        pal().icon
                                    };
                                    icon_redo(ui.painter(), rect.shrink(6.0), color);
                                    resp
                                };
                                if redo.clicked() && can_redo {
                                    self.redo(ctx);
                                }
                                let more = {
                                    let (rect, resp) =
                                        ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                    let color = if resp.hovered() { pal().icon_bright } else { pal().muted };
                                    icon_ellipsis(ui.painter(), rect.shrink(6.0), color);
                                    resp
                                };
                                if more.clicked() {
                                    self.actions_open = !self.actions_open;
                                    self.actions_opened = true;
                                }
                            });
                        });
                });

            // Menu `…` : les mêmes familles que le menu Fichier d'UPDF, en
            // sous-menus. Une seule colonne scrollable : la hauteur suit la
            // fenêtre au lieu de déborder de l'écran.
            if self.actions_open {
                let mut action = None;
                let area = Area::new(Id::new("actions-menu"))
                    .anchor(Align2::RIGHT_TOP, vec2(-14.0, top + 38.0))
                    .order(Order::Foreground)
                    .show(ctx, |ui| {
                        Frame::none()
                            .fill(pal().chrome_bg)
                            .rounding(Rounding::same(RADIUS_L))
                            .stroke(Stroke::new(1.0_f32, pal().separator))
                            .inner_margin(Margin::symmetric(4.0, 4.0))
                            .show(ui, |ui| {
                                let max_height =
                                    (ctx.screen_rect().height() - top - 70.0).max(220.0);
                                ScrollArea::vertical()
                                    .id_salt("actions-scroll")
                                    .max_height(max_height)
                                    .max_width(230.0)
                                    .auto_shrink([true, true])
                                    .show(ui, |ui| {
                                        ui.set_min_width(210.0);
                                        if menu_row(ui, "Undo", "⌘Z", Some(icon_undo)) {
                                            action = Some(MenuAction::Undo);
                                        }
                                        if menu_row(ui, "Redo", "⇧⌘Z", Some(icon_redo)) {
                                            action = Some(MenuAction::Redo);
                                        }
                                        menu_separator(ui);
                                        if menu_row(ui, "Save", "⌘S", None) {
                                            action = Some(MenuAction::Save);
                                        }
                                        if menu_row(ui, "Save as…", "⇧⌘S", None) {
                                            action = Some(MenuAction::SaveAs);
                                        }
                                        menu_submenu(ui, "Save as", |ui| {
                                            for level in [
                                                Conformance::PdfA1b,
                                                Conformance::PdfA2b,
                                                Conformance::PdfX1a,
                                                Conformance::PdfX4,
                                                Conformance::PdfE1,
                                            ] {
                                                if submenu_row(ui, level.display_name(), None) {
                                                    action = Some(MenuAction::Conformance(level));
                                                    ui.close_menu();
                                                }
                                            }
                                            ui.separator();
                                            if submenu_row(ui, "Image PDF…", None) {
                                                action = Some(MenuAction::ImagePdf);
                                                ui.close_menu();
                                            }
                                        });
                                        menu_submenu(ui, "Export to", |ui| {
                                            for (format, label) in [
                                                (OfficeFormat::Word, "Word (.docx)"),
                                                (OfficeFormat::PowerPoint, "PowerPoint (.pptx)"),
                                                (OfficeFormat::Excel, "Excel (.xlsx)"),
                                            ] {
                                                if submenu_row(ui, label, None) {
                                                    action =
                                                        Some(MenuAction::ExportOffice(format));
                                                    ui.close_menu();
                                                }
                                            }
                                            ui.separator();
                                            for (format, label) in [
                                                (ImageFormat::Png, "Image — PNG"),
                                                (ImageFormat::Jpeg, "Image — JPEG"),
                                                (ImageFormat::Tiff, "Image — TIFF"),
                                            ] {
                                                if submenu_row(ui, label, None) {
                                                    action = Some(MenuAction::ExportImage(format));
                                                    ui.close_menu();
                                                }
                                            }
                                            if submenu_row(ui, "Long image (PNG)", None) {
                                                action = Some(MenuAction::ExportLongImage);
                                                ui.close_menu();
                                            }
                                            if submenu_row(ui, "Embedded images (PNG)", None) {
                                                action = Some(MenuAction::ExportEmbedded);
                                                ui.close_menu();
                                            }
                                            ui.separator();
                                            for (format, label) in [
                                                (TextFormat::Plain, "Text (.txt)"),
                                                (TextFormat::Rtf, "RTF (.rtf)"),
                                                (TextFormat::Csv, "CSV (.csv)"),
                                                (TextFormat::Xml, "XML (.xml)"),
                                                (TextFormat::Html, "HTML (.htm)"),
                                                (TextFormat::Markdown, "Markdown (.md)"),
                                            ] {
                                                if submenu_row(ui, label, None) {
                                                    action = Some(MenuAction::ExportText(format));
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                        if menu_row(ui, "Export…", "", None) {
                                            action = Some(MenuAction::Export);
                                        }
                                        if menu_row(ui, "Flatten annotations", "", None) {
                                            action = Some(MenuAction::Flatten);
                                        }
                                        menu_separator(ui);
                                        menu_submenu(ui, "New", |ui| {
                                            if submenu_row(ui, "Blank page", None) {
                                                action = Some(MenuAction::NewBlank);
                                                ui.close_menu();
                                            }
                                            if submenu_row(ui, "From images…", None) {
                                                action = Some(MenuAction::FromImages);
                                                ui.close_menu();
                                            }
                                            if submenu_row(ui, "Combine with a PDF…", None) {
                                                action = Some(MenuAction::Combine);
                                                ui.close_menu();
                                            }
                                        });
                                        menu_submenu(ui, "Tools", |ui| {
                                            for (label, item) in [
                                                ("Create…", MenuAction::Create),
                                                ("Batch processing…", MenuAction::Batch),
                                                ("OCR…", MenuAction::Ocr),
                                                ("Extract data…", MenuAction::Extract),
                                                ("Compare PDFs…", MenuAction::Compare),
                                            ] {
                                                if submenu_row(ui, label, None) {
                                                    action = Some(item);
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                        menu_submenu(ui, "Document", |ui| {
                                            for (label, item) in [
                                                ("Watermark…", MenuAction::Watermark),
                                                ("Header & footer…", MenuAction::HeaderFooter),
                                                ("Page numbers…", MenuAction::PageNumbers),
                                                ("Compress…", MenuAction::Compress),
                                                ("Crop pages…", MenuAction::Crop),
                                                ("Background…", MenuAction::Background),
                                                ("Bates numbering…", MenuAction::Bates),
                                            ] {
                                                if submenu_row(ui, label, None) {
                                                    action = Some(item);
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                        menu_submenu(ui, "Protect", |ui| {
                                            if submenu_row(ui, "Protect with a password…", None) {
                                                action = Some(MenuAction::Protect);
                                                ui.close_menu();
                                            }
                                            if submenu_row(ui, "Remove protection…", None) {
                                                action = Some(MenuAction::RemoveProtection);
                                                ui.close_menu();
                                            }
                                        });
                                        menu_submenu(ui, "Bookmarks", |ui| {
                                            if submenu_row(ui, "Generate bookmarks", None) {
                                                action = Some(MenuAction::GenerateOutline);
                                                ui.close_menu();
                                            }
                                            if submenu_row(ui, "Clear bookmarks", None) {
                                                action = Some(MenuAction::ClearOutline);
                                                ui.close_menu();
                                            }
                                        });
                                        menu_separator(ui);
                                        menu_submenu(ui, "View", |ui| {
                                            for (label, layout) in [
                                                ("Single page", PageLayout::Single),
                                                ("Continuous scroll", PageLayout::Continuous),
                                                ("Two pages", PageLayout::TwoPages),
                                                ("Cover (book)", PageLayout::Cover),
                                            ] {
                                                if ui
                                                    .selectable_label(
                                                        self.page_layout == layout,
                                                        RichText::new(label),
                                                    )
                                                    .clicked()
                                                {
                                                    action = Some(MenuAction::Layout(layout));
                                                    ui.close_menu();
                                                }
                                            }
                                            ui.separator();
                                            if ui
                                                .selectable_label(
                                                    self.fullscreen,
                                                    RichText::new(t("Fullscreen")),
                                                )
                                                .clicked()
                                            {
                                                action = Some(MenuAction::Fullscreen);
                                                ui.close_menu();
                                            }
                                            if submenu_row(ui, "Slideshow  (F5)", None) {
                                                action = Some(MenuAction::Slideshow);
                                                ui.close_menu();
                                            }
                                        });
                                        menu_submenu(ui, "Theme", |ui| {
                                            for theme in Theme::ALL {
                                                if ui
                                                    .selectable_label(
                                                        self.theme == theme,
                                                        RichText::new(theme.label()),
                                                    )
                                                    .clicked()
                                                {
                                                    action = Some(MenuAction::Theme(theme));
                                                    ui.close_menu();
                                                }
                                            }
                                        });
                                        if menu_row(ui, "Preferences…", "⌘,", None) {
                                            action = Some(MenuAction::Preferences);
                                        }
                                        if menu_row_checked(ui, "AI features…", self.ai.enabled()) {
                                            action = Some(MenuAction::AiEnabled);
                                        }
                                        menu_separator(ui);
                                        if menu_row(ui, "Print…", "⌘P", None) {
                                            action = Some(MenuAction::Print);
                                        }
                                        if menu_row(ui, "Reveal in Finder", "", None) {
                                            action = Some(MenuAction::Reveal);
                                        }
                                        if menu_row(ui, "Properties…", "⌘D", None) {
                                            action = Some(MenuAction::Properties);
                                        }
                                        if menu_row(ui, "Close tab", "⌘W", None) {
                                            action = Some(MenuAction::Close);
                                        }
                                    });
                            });
                    });
                // Fermeture au clic hors du menu, comme les menus natifs. Le clic
                // qui a ouvert le menu dans cette même trame ne compte pas.
                if !self.actions_opened {
                    let (clicked, pointer) =
                        ctx.input(|i| (i.pointer.any_click(), i.pointer.interact_pos()));
                    let inside = pointer.is_some_and(|p| area.response.rect.contains(p));
                    if clicked && !inside {
                        self.actions_open = false;
                    }
                }
                self.actions_opened = false;
                if let Some(action) = action {
                    self.actions_open = false;
                    match action {
                        MenuAction::Undo => self.undo(ctx),
                        MenuAction::Redo => self.redo(ctx),
                        MenuAction::Save => self.save_active(ctx),
                        MenuAction::SaveAs => self.save_as(ctx),
                        MenuAction::Export => self.export_open = true,
                        MenuAction::ExportText(format) => {
                            self.export_mode = ExportMode::Text;
                            self.export_format = format;
                            self.export_open = true;
                        }
                        MenuAction::ExportImage(format) => {
                            self.export_mode = ExportMode::Image;
                            self.export_image_format = format;
                            self.export_open = true;
                        }
                        MenuAction::ExportLongImage => {
                            self.export_mode = ExportMode::LongImage;
                            self.export_open = true;
                        }
                        MenuAction::ExportEmbedded => {
                            self.export_mode = ExportMode::Embedded;
                            self.export_open = true;
                        }
                        MenuAction::ExportOffice(format) => {
                            self.export_mode = ExportMode::Office;
                            self.export_office_format = format;
                            self.export_open = true;
                        }
                        MenuAction::Flatten => self.flatten_annotations(ctx),
                        MenuAction::Properties => self.props_open = true,
                        MenuAction::Close => {
                            let idx = self.active;
                            self.close_tab(idx);
                        }
                        MenuAction::Layout(layout) => self.set_page_layout(ctx, layout),
                        MenuAction::Fullscreen => self.toggle_fullscreen(ctx),
                        MenuAction::Watermark => self.watermark_open = true,
                        MenuAction::HeaderFooter => self.header_footer_open = true,
                        MenuAction::PageNumbers => {
                            // Même fenêtre : on préremplit la numérotation.
                            if self.header_footer.page_numbers.is_none() {
                                self.header_footer.page_numbers = Some(NumberStyle::Numeric);
                            }
                            self.header_footer_open = true;
                        }
                        MenuAction::Compress => self.compress_open = true,
                        MenuAction::Protect => self.protect_open = true,
                        MenuAction::Crop => self.crop_open = true,
                        MenuAction::Background => self.background_open = true,
                        MenuAction::Bates => self.bates_open = true,
                        MenuAction::Compare => {
                            self.compare_report = None;
                            self.compare_open = true;
                        }
                        MenuAction::Ocr => self.ocr_open = true,
                        MenuAction::Create => self.create_open = true,
                        MenuAction::Print => self.print_active(ctx),
                        MenuAction::Reveal => self.reveal_active(ctx),
                        MenuAction::RemoveProtection => self.remove_protection(ctx),
                        MenuAction::NewBlank => self.new_blank_page(ctx),
                        MenuAction::FromImages => self.import_images(ctx),
                        MenuAction::Combine => self.combine_active(ctx),
                        MenuAction::Slideshow => self.enter_slideshow(ctx),
                        MenuAction::Batch => self.batch_open = true,
                        MenuAction::Extract => self.extract_open = true,
                        MenuAction::Preferences => self.prefs_open = true,
                        MenuAction::AiEnabled => self.ai.open = true,
                        MenuAction::Conformance(level) => self.conformance_save(ctx, level),
                        MenuAction::ImagePdf => self.image_pdf_save(ctx),
                        MenuAction::GenerateOutline => self.generate_bookmarks(ctx),
                        MenuAction::ClearOutline => self.clear_bookmarks(ctx),
                        MenuAction::Theme(theme) => {
                            self.theme = theme;
                            // Le thème est une préférence persistante : on
                            // l'enregistre au même endroit que le reste.
                            self.ai.settings.theme_index = Theme::ALL
                                .iter()
                                .position(|candidate| *candidate == theme)
                                .unwrap_or(0);
                            let _ = self.ai.settings.save();
                        }
                    }
                }
            }
        }

}
