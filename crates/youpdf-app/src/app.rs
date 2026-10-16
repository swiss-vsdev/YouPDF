//! Fenêtre principale de YouPDF — v0.1 « lecteur ».
//!
//! Structure calquée sur le modèle d'interaction de la spec UX (rail de 8
//! outils, onglets, panneau latéral, canevas continu, barre d'état).
//! Identité visuelle propre : emoji système, palette neutre mesurée,
//! aucune ressource propriétaire.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{
    pos2, vec2, Align, Align2, Button, CentralPanel, Color32, FontId, Frame, Rect, RichText,
    ScrollArea, Sense, SidePanel, Stroke, TextureHandle, TextureOptions, TopBottomPanel,
};
use youpdf_core::render::NullRenderer;
use youpdf_core::{Document, PdfiumRenderer, RenderOptions, Renderer, SearchIndex, SearchResult};

// --------------------------------------------------------------------------
// Palette
// --------------------------------------------------------------------------

const CANVAS_BG: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
const CHROME_BG: Color32 = Color32::from_rgb(0x28, 0x28, 0x28);
const WIDGET_BG: Color32 = Color32::from_rgb(0x38, 0x38, 0x38);
const HOVER_BG: Color32 = Color32::from_rgb(0x48, 0x48, 0x48);
const ACCENT: Color32 = Color32::from_rgb(0x7A, 0x7E, 0xF6);
const MUTED: Color32 = Color32::from_rgb(0xA0, 0xA0, 0xA0);

const PAGE_SPACING: f32 = 12.0;
const THUMB_HEIGHT: f32 = 120.0;

// --------------------------------------------------------------------------
// Outils du rail
// --------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
enum Panel {
    Thumbnails,
    Search,
    Bookmarks,
    Comments,
    Forms,
    Attachments,
    Appearance,
}

impl Panel {
    fn title(self) -> &'static str {
        match self {
            Panel::Thumbnails => "Pages",
            Panel::Search => "Search",
            Panel::Bookmarks => "Bookmarks",
            Panel::Comments => "Comments",
            Panel::Forms => "Form fields",
            Panel::Attachments => "Attachments",
            Panel::Appearance => "Appearance",
        }
    }
}

struct RailTool {
    glyph: &'static str,
    label: &'static str,
    /// `None` = action directe (pas de panneau).
    panel: Option<Panel>,
}

const RAIL: &[RailTool] = &[
    RailTool { glyph: "🗂", label: "Organize pages", panel: None },
    RailTool { glyph: "🔍", label: "Search", panel: Some(Panel::Search) },
    RailTool { glyph: "📄", label: "Pages", panel: Some(Panel::Thumbnails) },
    RailTool { glyph: "🔖", label: "Bookmarks", panel: Some(Panel::Bookmarks) },
    RailTool { glyph: "💬", label: "Comments", panel: Some(Panel::Comments) },
    RailTool { glyph: "🧾", label: "Form fields", panel: Some(Panel::Forms) },
    RailTool { glyph: "📎", label: "Attachments", panel: Some(Panel::Attachments) },
    RailTool { glyph: "🎨", label: "Appearance", panel: Some(Panel::Appearance) },
];

// --------------------------------------------------------------------------
// Onglets
// --------------------------------------------------------------------------

struct Tab {
    doc: Document,
    path: Option<PathBuf>,
    title: String,
    /// Page courante (1-indexée).
    page: usize,
    /// `None` = ajuster à la largeur.
    zoom: Option<f32>,
    /// Bitmaps déjà rendus ; clé `(page, bucket)`, bucket 1 = vignette.
    cache: HashMap<(u32, u32), TextureHandle>,
}

impl Tab {
    fn total(&self) -> usize {
        self.doc.page_count()
    }
}

// --------------------------------------------------------------------------
// Application
// --------------------------------------------------------------------------

pub struct YouPdfApp {
    renderer: Box<dyn Renderer>,
    renderer_label: &'static str,

    tabs: Vec<Tab>,
    active: usize,

    panel: Option<Panel>,
    status: String,

    search_query: String,
    search_index: Option<(usize, SearchIndex)>,
    results: Vec<SearchResult>,

    /// Demande de défilement vers une page (vignette, recherche).
    jump: Option<usize>,
    /// Dernière échelle « ajuster à la largeur » : base des pas de zoom.
    last_fit: f32,
}

impl YouPdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        apply_theme(&cc.egui_ctx);

        let (renderer, renderer_label): (Box<dyn Renderer>, &'static str) =
            match PdfiumRenderer::load() {
                Ok(r) => (Box::new(r), "PDFium"),
                Err(err) => {
                    tracing::warn!("pdfium indisponible : {err}");
                    (Box::new(NullRenderer), "no PDFium")
                }
            };

        let mut app = Self {
            renderer,
            renderer_label,
            tabs: Vec::new(),
            active: 0,
            panel: Some(Panel::Thumbnails),
            status: "Ready".to_owned(),
            search_query: String::new(),
            search_index: None,
            results: Vec::new(),
            jump: None,
            last_fit: 1.0,
        };

        if let Some(path) = initial {
            app.open_path(path);
        }
        app
    }

    // -- Fichiers ----------------------------------------------------------

    fn open_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
        {
            self.open_path(path);
        }
    }

    fn open_path(&mut self, path: PathBuf) {
        if let Some(idx) = self
            .tabs
            .iter()
            .position(|t| t.path.as_deref() == Some(path.as_path()))
        {
            self.active = idx;
            return;
        }

        match Document::open(&path) {
            Ok(doc) => {
                let title = doc.display_name();
                if self.renderer.renders_real_content() {
                    self.status = format!("Opened {title}");
                } else {
                    self.status =
                        "PDFium missing — pages render blank. Run scripts/install-pdfium.sh".into();
                }
                self.tabs.push(Tab {
                    doc,
                    path: Some(path),
                    title,
                    page: 1,
                    zoom: None,
                    cache: HashMap::new(),
                });
                self.active = self.tabs.len() - 1;
                self.panel = Some(Panel::Thumbnails);
                self.search_index = None;
                self.results.clear();
                self.jump = Some(1);
            }
            Err(err) => {
                self.status = format!("Open failed: {err}");
            }
        }
    }

    fn close_tab(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
            self.search_index = None;
            self.results.clear();
        }
    }

    fn active_tab(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    // -- Zoom --------------------------------------------------------------

    fn set_zoom(&mut self, zoom: Option<f32>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.zoom = zoom.map(|z| z.clamp(0.1, 8.0));
        }
    }

    fn zoom_by(&mut self, factor: f32) {
        let base = self
            .active_tab()
            .and_then(|t| t.zoom)
            .unwrap_or(self.last_fit.max(0.05));
        self.set_zoom(Some(base * factor));
    }

    // -- Recherche ---------------------------------------------------------

    fn run_search(&mut self) {
        if self.search_query.trim().is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };

        if self.search_index.as_ref().map(|(idx, _)| *idx) != Some(self.active) {
            match SearchIndex::build(&tab.doc) {
                Ok(index) => self.search_index = Some((self.active, index)),
                Err(err) => {
                    self.status = format!("Search index failed: {err}");
                    return;
                }
            }
        }

        if let Some((_, index)) = &self.search_index {
            self.results = index.search(&self.search_query, 200);
            self.status = format!("{} result(s)", self.results.len());
        }
    }

    // -- Rendu des pages ---------------------------------------------------

    /// Texture d'une page, via le cache de l'onglet.
    fn page_texture(
        ctx: &egui::Context,
        renderer: &dyn Renderer,
        tab: &mut Tab,
        page: u32,
        bucket: u32,
        scale: f32,
        status: &mut String,
    ) -> Option<TextureHandle> {
        let key = (page, bucket);
        if let Some(tex) = tab.cache.get(&key) {
            return Some(tex.clone());
        }

        let options = RenderOptions { scale, ..Default::default() };
        match renderer.render_page(&tab.doc, page, &options) {
            Ok(bitmap) => {
                let handle = ctx.load_texture(
                    format!("p{page}b{bucket}"),
                    bitmap.to_egui_image(),
                    TextureOptions::LINEAR,
                );
                tab.cache.insert(key, handle.clone());
                Some(handle)
            }
            Err(err) => {
                *status = format!("Render failed (page {page}): {err}");
                None
            }
        }
    }

    // -- Barres et panneaux ------------------------------------------------

    fn top_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("tabs")
            .exact_height(40.0)
            .frame(Frame::none().fill(CHROME_BG))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(6.0);
                    // ponytail: accueil « fichiers récents » prévu plus tard.
                    let home = ui.add(
                        Button::new(RichText::new("🏠").size(15.0))
                            .frame(false)
                            .min_size(vec2(28.0, 26.0)),
                    );
                    if home.clicked() {
                        self.status = "Home (recent files) — planned".into();
                    }
                    ui.add_space(4.0);

                    let mut select = None;
                    let mut close = None;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let selected = idx == self.active;
                        let resp = ui.add(
                            Button::new(RichText::new(shorten(&tab.title, 22)).size(12.0))
                                .frame(false)
                                .fill(if selected { WIDGET_BG } else { Color32::TRANSPARENT })
                                .min_size(vec2(0.0, 26.0)),
                        );
                        if resp.clicked() {
                            select = Some(idx);
                        }
                        let x = ui.add(
                            Button::new(RichText::new("✕").size(10.0).color(MUTED))
                                .frame(false)
                                .min_size(vec2(20.0, 24.0)),
                        );
                        if x.clicked() {
                            close = Some(idx);
                        }
                        ui.add_space(2.0);
                    }

                    let plus = ui.add(
                        Button::new(RichText::new("+").size(13.0))
                            .frame(false)
                            .min_size(vec2(24.0, 24.0)),
                    );
                    if plus.clicked() {
                        self.open_dialog();
                    }

                    if let Some(idx) = select {
                        self.active = idx;
                    }
                    if let Some(idx) = close {
                        self.close_tab(idx);
                    }
                });
            });
    }

    fn rail(&mut self, ctx: &egui::Context) {
        SidePanel::left("rail")
            .exact_width(52.0)
            .resizable(false)
            .frame(Frame::none().fill(CHROME_BG))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.vertical_centered(|ui| {
                    for tool in RAIL {
                        let selected = tool.panel.is_some() && self.panel == tool.panel;
                        let resp = ui
                            .add(
                                Button::new(RichText::new(tool.glyph).size(17.0))
                                    .frame(false)
                                    .fill(if selected { ACCENT } else { Color32::TRANSPARENT })
                                    .min_size(vec2(40.0, 34.0)),
                            )
                            .on_hover_text(tool.label);
                        if resp.clicked() {
                            match tool.panel {
                                Some(p) => {
                                    self.panel = if self.panel == Some(p) { None } else { Some(p) }
                                }
                                None => self.status = format!("{} — planned", tool.label),
                            }
                        }
                        ui.add_space(2.0);
                    }
                });
            });
    }

    fn side_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.panel else {
            return;
        };
        SidePanel::left("content")
            .exact_width(220.0)
            .resizable(false)
            .frame(Frame::none().fill(CHROME_BG))
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(panel.title()).size(13.0).strong());
                });
                ui.add_space(6.0);
                match panel {
                    Panel::Thumbnails => self.thumbnails(ui),
                    Panel::Search => self.search_ui(ui),
                    other => {
                        ui.add_space(12.0);
                        ui.vertical_centered(|ui| {
                            ui.label(
                                RichText::new(format!("{} — planned", other.title()))
                                    .size(12.0)
                                    .color(MUTED),
                            );
                        });
                    }
                }
            });
    }

    fn thumbnails(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            return;
        }
        let active = self.active;
        let ctx = ui.ctx().clone();
        let renderer = &*self.renderer;
        let status = &mut self.status;
        let tab = &mut self.tabs[active];
        let total = tab.total();
        let mut jump = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for page in 1..=total as u32 {
                    let (wp, hp) = tab.doc.page_size(page);
                    let scale = THUMB_HEIGHT / hp.max(1.0);
                    let size = vec2(wp * scale, hp * scale);
                    let (rect, resp) =
                        ui.allocate_exact_size(size + vec2(0.0, 20.0), Sense::click());
                    let img_rect = Rect::from_min_size(rect.min, size);

                    if let Some(tex) =
                        Self::page_texture(&ctx, renderer, tab, page, 1, scale, status)
                    {
                        ui.painter().image(
                            tex.id(),
                            img_rect,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    if page as usize == tab.page {
                        ui.painter().rect_stroke(
                            img_rect.expand(2.0),
                            3.0,
                            Stroke::new(1.5_f32, ACCENT),
                        );
                    }
                    ui.painter().text(
                        pos2(rect.center().x, img_rect.max.y + 10.0),
                        Align2::CENTER_CENTER,
                        page.to_string(),
                        FontId::proportional(11.0),
                        if page as usize == tab.page { ACCENT } else { MUTED },
                    );
                    if resp.clicked() {
                        jump = Some(page as usize);
                    }
                    ui.add_space(4.0);
                }
            });

        if let Some(page) = jump {
            if let Some(tab) = self.tabs.get_mut(active) {
                tab.page = page;
            }
            self.jump = Some(page);
        }
    }

    fn search_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let resp = ui.add_sized(
                vec2(170.0, 22.0),
                egui::TextEdit::singleline(&mut self.search_query).hint_text("Search…"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_search();
            }
        });
        ui.add_space(6.0);

        let mut jump = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for result in &self.results {
                    let label = format!("#{}  {}", result.page, shorten(result.context.trim(), 46));
                    if ui
                        .add(
                            egui::SelectableLabel::new(false, RichText::new(label).size(11.0)),
                        )
                        .clicked()
                    {
                        jump = Some(result.page);
                    }
                }
            });

        if let Some(page) = jump {
            self.jump = Some(page);
        }
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(140.0);
            ui.label(RichText::new("YouPDF").size(34.0).strong());
            ui.add_space(8.0);
            ui.label(
                RichText::new("Free, local-first PDF editing")
                    .size(13.0)
                    .color(MUTED),
            );
            ui.add_space(24.0);
            let open = ui.add_sized(
                vec2(160.0, 32.0),
                Button::new(RichText::new("Open PDF").size(14.0)).fill(ACCENT),
            );
            if open.clicked() {
                self.open_dialog();
            }
            ui.add_space(10.0);
            ui.label(RichText::new("or drop a file anywhere").size(11.0).color(MUTED));
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            self.empty_state(ui);
            return;
        }

        // Pincement / ⌘-molette = zoom (egui agrège les deux).
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if (zoom_delta - 1.0).abs() > 0.0001 {
            self.zoom_by(zoom_delta);
        }

        let active = self.active;
        let mut jump = self.jump.take();
        let ctx = ui.ctx().clone();
        let renderer = &*self.renderer;
        let status = &mut self.status;
        let last_fit = &mut self.last_fit;
        let tab = &mut self.tabs[active];
        let total = tab.total();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let avail = ui.available_width().max(50.0);
                let mut first_visible: Option<usize> = None;
                let mut clicked_page: Option<usize> = None;

                for page in 1..=total as u32 {
                    let (wp, hp) = tab.doc.page_size(page);
                    let scale = tab.zoom.unwrap_or(avail / wp.max(1.0)).clamp(0.05, 8.0);
                    if tab.zoom.is_none() {
                        *last_fit = scale;
                    }
                    let size = vec2(wp * scale, hp * scale);
                    let (rect, resp) = ui
                        .allocate_exact_size(size + vec2(0.0, PAGE_SPACING), Sense::click());
                    let page_rect = Rect::from_min_size(rect.min, size);

                    if jump == Some(page as usize) {
                        ui.scroll_to_rect(rect, Some(Align::Center));
                        jump = None;
                    }

                    if viewport.intersects(rect) {
                        if first_visible.is_none() {
                            first_visible = Some(page as usize);
                        }
                        let bucket = (scale * 100.0).round() as u32;
                        if let Some(tex) =
                            Self::page_texture(&ctx, renderer, tab, page, bucket, scale, status)
                        {
                            ui.painter().image(
                                tex.id(),
                                page_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
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

                // ponytail: cache borné par nettoyage grossier, LRU plus tard.
                if tab.cache.len() > 32 {
                    let current = tab.page as i64;
                    tab.cache
                        .retain(|key, _| (key.0 as i64 - current).abs() < 8);
                }
            });
    }

    fn status_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::bottom("status")
            .exact_height(24.0)
            .frame(Frame::none().fill(CHROME_BG))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(10.0);
                    let (page, total, zoom) = match self.active_tab() {
                        Some(tab) => (
                            tab.page,
                            tab.total(),
                            tab.zoom
                                .map(|z| format!("{:.0}%", z * 100.0))
                                .unwrap_or_else(|| "Fit".to_owned()),
                        ),
                        None => (0, 0, "—".to_owned()),
                    };
                    ui.label(
                        RichText::new(format!("Page {page}/{total}"))
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.add_space(12.0);
                    ui.label(RichText::new(format!("Zoom {zoom}")).size(11.0).color(MUTED));
                    ui.add_space(12.0);
                    ui.label(RichText::new(&self.status).size(11.0).color(MUTED));

                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        ui.label(RichText::new(self.renderer_label).size(11.0).color(MUTED));
                    });
                });
            });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let (cmd, open, close, plus, minus, zero) = ctx.input(|i| {
            (
                i.modifiers.command,
                i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::W),
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::Num0),
            )
        });
        if !cmd {
            return;
        }
        if open {
            self.open_dialog();
        }
        if close {
            let idx = self.active;
            self.close_tab(idx);
        }
        if plus {
            self.zoom_by(1.1);
        }
        if minus {
            self.zoom_by(1.0 / 1.1);
        }
        if zero {
            self.set_zoom(None);
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .filter_map(|f| f.path.clone())
                .collect()
        });
        for path in dropped {
            self.open_path(path);
        }
    }
}

impl eframe::App for YouPdfApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_drops(ctx);
        self.shortcuts(ctx);
        self.top_bar(ctx);
        self.status_bar(ctx);
        self.rail(ctx);
        self.side_panel(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(CANVAS_BG))
            .show(ctx, |ui| {
                self.canvas(ui);
            });
    }
}

/// Applique la palette sombre.
fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CANVAS_BG;
    visuals.window_fill = CHROME_BG;
    visuals.extreme_bg_color = CHROME_BG;
    visuals.faint_bg_color = WIDGET_BG;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.noninteractive.bg_fill = CANVAS_BG;
    visuals.widgets.inactive.bg_fill = WIDGET_BG;
    visuals.widgets.inactive.weak_bg_fill = WIDGET_BG;
    visuals.widgets.hovered.bg_fill = HOVER_BG;
    visuals.widgets.hovered.weak_bg_fill = HOVER_BG;
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    ctx.set_visuals(visuals);
}

/// Tronque avec ellipse si nécessaire.
fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}
