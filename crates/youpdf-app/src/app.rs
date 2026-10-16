//! Fenêtre principale de YouPDF — v0.1 « lecteur ».
//!
//! Chrome calqué sur la référence UX mesurée : rail de 8 outils, barre de
//! titre maison (feux natifs conservés), onglets en pastilles, barre d'outils
//! flottante (Outils · 4 modes · annuler/rétablir), panneau latéral, canevas
//! continu avec ombres de page. Icônes vectorielles maison, palette mesurée.
//!
//! Rendu net sur écrans HiDPI : les bitmaps PDFium sont rastérisés à
//! `échelle_logique × pixels_per_point`, puis dessinés à la taille logique.

use std::collections::HashMap;
use std::path::PathBuf;

use egui::{
    pos2, vec2, Align, Align2, Area, Button, CentralPanel, Color32, FontId, Frame, Id, Layout,
    Margin, Order, Painter, Rect, RichText, Rounding, ScrollArea, Sense, Shape, SidePanel, Stroke,
    TextEdit, TextureHandle, TextureOptions, TopBottomPanel, ViewportCommand,
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
const ICON: Color32 = Color32::from_rgb(0xC8, 0xC8, 0xC8);
const ICON_BRIGHT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xE8);
const THUMB_TINT: Color32 = Color32::from_rgb(0xE8, 0xE8, 0xF8);
const SEPARATOR: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3A);

const TITLE_H: f32 = 40.0;
const RAIL_W: f32 = 52.0;
const PANEL_W: f32 = 220.0;
const PAGE_SPACING: f32 = 16.0;
const THUMB_HEIGHT: f32 = 118.0;

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
    Aspect,
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
            Panel::Aspect => "Aspect",
        }
    }
}

struct RailTool {
    label: &'static str,
    panel: Option<Panel>,
    draw: fn(&Painter, Rect, Color32),
}

const RAIL: &[RailTool] = &[
    RailTool { label: "Organize pages", panel: None, draw: icon_organize },
    RailTool { label: "Search", panel: Some(Panel::Search), draw: icon_search },
    RailTool { label: "Pages", panel: Some(Panel::Thumbnails), draw: icon_page },
    RailTool { label: "Bookmarks", panel: Some(Panel::Bookmarks), draw: icon_bookmark },
    RailTool { label: "Comments", panel: Some(Panel::Comments), draw: icon_bubble },
    RailTool { label: "Form fields", panel: Some(Panel::Forms), draw: icon_form },
    RailTool { label: "Attachments", panel: Some(Panel::Attachments), draw: icon_attach },
];

const MODES: [&str; 4] = ["Comment", "Edit", "Form", "Redact"];

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
    mode: Option<usize>,

    search_query: String,
    search_index: Option<(usize, SearchIndex)>,
    results: Vec<SearchResult>,
    history: Vec<String>,

    /// Message éphémère : (texte, date d'apparition).
    toast: (String, f64),
    /// Dernière erreur de rendu (remontée en toast).
    render_status: String,
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
            mode: None,
            search_query: String::new(),
            search_index: None,
            results: Vec::new(),
            history: Vec::new(),
            toast: (String::new(), f64::NEG_INFINITY),
            render_status: String::new(),
            jump: None,
            last_fit: 1.0,
        };

        if let Some(path) = initial {
            app.open_path(path);
        }
        app
    }

    fn toast(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.toast = (text.into(), ctx.input(|i| i.time));
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
                self.tabs.push(Tab {
                    doc,
                    path: Some(path),
                    title: title.clone(),
                    page: 1,
                    zoom: None,
                    cache: HashMap::new(),
                });
                self.active = self.tabs.len() - 1;
                self.panel = Some(Panel::Thumbnails);
                self.search_index = None;
                self.results.clear();
                self.jump = Some(1);

                if self.renderer.renders_real_content() {
                    self.toast_owned(format!("Opened {title}"));
                } else {
                    self.toast_owned(
                        "PDFium missing — pages render blank. Run scripts/install-pdfium.sh",
                    );
                }
            }
            Err(err) => self.toast_owned(format!("Open failed: {err}")),
        }
    }

    /// Pose un message sans disposer du contexte.
    fn toast_owned(&mut self, text: impl Into<String>) {
        self.toast = (text.into(), f64::NAN); // horodaté au prochain rendu
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

    fn run_search(&mut self, ctx: &egui::Context) {
        let query = self.search_query.trim().to_owned();
        if query.is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };

        if self.search_index.as_ref().map(|(idx, _)| *idx) != Some(self.active) {
            match SearchIndex::build(&tab.doc) {
                Ok(index) => self.search_index = Some((self.active, index)),
                Err(err) => {
                    self.toast(ctx, format!("Search index failed: {err}"));
                    return;
                }
            }
        }

        if let Some((_, index)) = &self.search_index {
            self.results = index.search(&query, 200);
            let count = self.results.len();
            self.history.retain(|q| q != &query);
            self.history.insert(0, query);
            self.history.truncate(8);
            self.toast(ctx, format!("{count} result(s)"));
        }
    }

    // -- Rendu des pages ---------------------------------------------------

    /// Texture d'une page, nette sur écrans HiDPI.
    #[allow(clippy::too_many_arguments)]
    fn page_texture(
        ctx: &egui::Context,
        renderer: &dyn Renderer,
        tab: &mut Tab,
        page: u32,
        bucket: u32,
        logical_scale: f32,
        ppp: f32,
        status_holder: &mut String,
    ) -> Option<TextureHandle> {
        let key = (page, bucket);
        if let Some(tex) = tab.cache.get(&key) {
            return Some(tex.clone());
        }

        let options = RenderOptions {
            scale: logical_scale * ppp,
            ..Default::default()
        };
        match renderer.render_page(&tab.doc, page, &options) {
            Ok(bitmap) => {
                tracing::info!(
                    "render page {page} bucket {bucket}: bitmap {}x{} (scale {:.3})",
                    bitmap.width,
                    bitmap.height,
                    logical_scale * ppp
                );
                let handle = ctx.load_texture(
                    format!("p{page}b{bucket}"),
                    bitmap.to_egui_image(),
                    TextureOptions::LINEAR,
                );
                tab.cache.insert(key, handle.clone());
                Some(handle)
            }
            Err(err) => {
                *status_holder = format!("Render failed (page {page}): {err}");
                None
            }
        }
    }

    // -- Barre de titre ----------------------------------------------------

    fn title_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("titlebar")
            .exact_height(TITLE_H)
            .frame(Frame::none().fill(CHROME_BG))
            .show(ctx, |ui| {
                // Drag fenêtre : interaction posée en premier = priorité basse,
                // les boutons ajoutés ensuite restent cliquables.
                let drag = ui.interact(ui.max_rect(), Id::new("titlebar-drag"), Sense::drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(ViewportCommand::StartDrag);
                }

                ui.horizontal_centered(|ui| {
                    ui.add_space(76.0); // place des feux de circulation natifs

                    let home = icon_button(ui, 26.0, 26.0, false, |p, r, c| icon_home(p, r, c));
                    if home.clicked() {
                        self.toast(ctx, "Home (recent files) — planned");
                    }
                    ui.add_space(10.0);

                    let mut select = None;
                    let mut close = None;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let selected = idx == self.active;
                        let color = if selected { ICON_BRIGHT } else { MUTED };
                        let galley = ui.painter().layout_no_wrap(
                            shorten(&tab.title, 20),
                            FontId::proportional(12.5),
                            color,
                        );
                        let width = galley.size().x + 20.0 + if selected { 22.0 } else { 6.0 };
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(width, 28.0), Sense::click());

                        if selected {
                            ui.painter().rect_filled(rect, Rounding::same(8.0), WIDGET_BG);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                Rounding::same(8.0),
                                Color32::from_rgb(0x33, 0x33, 0x33),
                            );
                        }
                        ui.painter().galley(
                            pos2(rect.min.x + 10.0, rect.center().y - galley.size().y / 2.0),
                            galley,
                            color,
                        );

                        if selected {
                            let x_rect = Rect::from_center_size(
                                pos2(rect.max.x - 15.0, rect.center().y),
                                vec2(16.0, 16.0),
                            );
                            let x_resp =
                                ui.interact(x_rect, Id::new(("tab-close", idx)), Sense::click());
                            if x_resp.hovered() {
                                ui.painter().rect_filled(
                                    x_rect,
                                    Rounding::same(4.0),
                                    Color32::from_rgb(0x4A, 0x4A, 0x4A),
                                );
                            }
                            icon_close(
                                ui.painter(),
                                x_rect,
                                if x_resp.hovered() { ICON_BRIGHT } else { MUTED },
                            );
                            if x_resp.clicked() {
                                close = Some(idx);
                            }
                        }

                        if resp.clicked() {
                            select = Some(idx);
                        }
                        ui.add_space(4.0);
                    }

                    let plus = icon_button(ui, 24.0, 24.0, false, |p, r, c| icon_plus(p, r, c));
                    if plus.clicked() {
                        self.open_dialog();
                    }

                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(12.0);
                        // Avatar maison (identité YouPDF).
                        let (rect, resp) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                        ui.painter()
                            .circle_filled(rect.center(), 10.5, ACCENT);
                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Y",
                            FontId::proportional(11.0),
                            Color32::WHITE,
                        );
                        let _ = resp;
                        ui.add_space(6.0);
                        let bell = icon_button(ui, 26.0, 26.0, false, |p, r, c| {
                            icon_bell(p, r, c)
                        });
                        if bell.clicked() {
                            self.toast(ctx, "Notifications — planned");
                        }
                    });

                    if let Some(idx) = select {
                        self.active = idx;
                    }
                    if let Some(idx) = close {
                        self.close_tab(idx);
                    }
                });
            });
    }

    // -- Rail --------------------------------------------------------------

    fn rail(&mut self, ctx: &egui::Context) {
        SidePanel::left("rail")
            .exact_width(RAIL_W)
            .resizable(false)
            .frame(Frame::none().fill(CHROME_BG))
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.vertical_centered(|ui| {
                    for tool in RAIL {
                        let selected = tool.panel.is_some() && self.panel == tool.panel;
                        if rail_button(ui, selected, tool.draw).clicked() {
                            match tool.panel {
                                Some(p) => {
                                    self.panel = if self.panel == Some(p) { None } else { Some(p) }
                                }
                                None => self.toast(ctx, format!("{} — planned", tool.label)),
                            }
                        }
                        ui.add_space(6.0);
                    }

                    ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                        ui.add_space(10.0);
                        let selected = self.panel == Some(Panel::Aspect);
                        if rail_button(ui, selected, icon_aspect).clicked() {
                            self.panel =
                                if selected { None } else { Some(Panel::Aspect) };
                        }
                    });
                });
            });
    }

    // -- Panneau latéral ---------------------------------------------------

    fn side_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.panel else {
            return;
        };
        SidePanel::left("content")
            .exact_width(PANEL_W)
            .resizable(false)
            .frame(Frame::none().fill(CANVAS_BG).stroke(Stroke::new(1.0_f32, SEPARATOR)))
            .show(ctx, |ui| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.label(RichText::new(panel.title()).size(13.0).strong());
                });
                ui.add_space(8.0);
                match panel {
                    Panel::Thumbnails => self.thumbnails(ui),
                    Panel::Search => self.search_ui(ctx, ui),
                    other => {
                        ui.add_space(14.0);
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
        let ppp = ctx.pixels_per_point();
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
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
                        ui.allocate_exact_size(size + vec2(0.0, 22.0), Sense::click());
                    let img_rect = Rect::from_min_size(rect.min, size);
                    let selected = page as usize == tab.page;

                    if selected {
                        ui.painter()
                            .rect_filled(img_rect.expand(4.0), Rounding::same(4.0), THUMB_TINT);
                    }

                    let bucket = 1;
                    if let Some(tex) = Self::page_texture(
                        &ctx, renderer, tab, page, bucket, scale, ppp, status,
                    ) {
                        ui.painter().image(
                            tex.id(),
                            img_rect,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    ui.painter().text(
                        pos2(rect.center().x, img_rect.max.y + 12.0),
                        Align2::CENTER_CENTER,
                        page.to_string(),
                        FontId::proportional(11.0),
                        if selected { ACCENT } else { MUTED },
                    );
                    if resp.clicked() {
                        jump = Some(page as usize);
                    }
                    ui.add_space(6.0);
                }
            });

        if let Some(page) = jump {
            if let Some(tab) = self.tabs.get_mut(active) {
                tab.page = page;
            }
            self.jump = Some(page);
        }
    }

    fn search_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // Rangée d'actions : recherche · Aa (remplacer) · corbeille.
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
            ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
            icon_search(ui.painter(), rect.shrink(8.0), ICON_BRIGHT);
            let _ = resp;

            let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
            }
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Aa",
                FontId::proportional(12.0),
                ICON,
            );
            if resp.clicked() {
                self.toast(ctx, "Find & replace — planned");
            }

            let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 26.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
            }
            icon_trash(ui.painter(), rect.shrink(8.0), ICON);
            if resp.clicked() {
                self.history.clear();
                self.results.clear();
            }
        });
        ui.add_space(6.0);

        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let resp = ui.add_sized(
                vec2(PANEL_W - 26.0, 26.0),
                TextEdit::singleline(&mut self.search_query).hint_text("Search…"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_search(ctx);
            }
        });
        ui.add_space(10.0);

        let mut jump = None;
        let mut forget = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if self.results.is_empty() {
                    if self.history.is_empty() {
                        return;
                    }
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(RichText::new("History").size(12.0).strong());
                    });
                    ui.add_space(6.0);
                    for (idx, query) in self.history.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(PANEL_W - 24.0, 28.0), Sense::click());
                        ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
                        ui.painter().text(
                            pos2(rect.min.x + 10.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            shorten(query, 22),
                            FontId::proportional(11.5),
                            ICON_BRIGHT,
                        );
                        let x_rect = Rect::from_center_size(
                            pos2(rect.max.x - 14.0, rect.center().y),
                            vec2(16.0, 16.0),
                        );
                        let x_resp =
                            ui.interact(x_rect, Id::new(("hist-x", idx)), Sense::click());
                        icon_close(
                            ui.painter(),
                            x_rect,
                            if x_resp.hovered() { ICON_BRIGHT } else { MUTED },
                        );
                        if x_resp.clicked() {
                            forget = Some(idx);
                            continue;
                        }
                        if resp.clicked() {
                            jump = Some(idx);
                        }
                    }
                } else {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("Results · {}", self.results.len()))
                                .size(12.0)
                                .strong(),
                        );
                    });
                    ui.add_space(6.0);
                    for (idx, result) in self.results.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(PANEL_W - 24.0, 28.0), Sense::click());
                        if resp.hovered() {
                            ui.painter()
                                .rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
                        }
                        ui.painter().text(
                            pos2(rect.min.x + 8.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("#{}", result.page),
                            FontId::proportional(11.5),
                            ACCENT,
                        );
                        ui.painter().text(
                            pos2(rect.min.x + 40.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            shorten(result.context.trim(), 24),
                            FontId::proportional(11.0),
                            MUTED,
                        );
                        if resp.clicked() {
                            jump = Some(idx);
                        }
                    }
                }
            });

        if let Some(idx) = forget {
            self.history.remove(idx);
        }
        if let Some(idx) = jump {
            if self.results.is_empty() {
                if let Some(query) = self.history.get(idx).cloned() {
                    self.search_query = query;
                    self.run_search(ctx);
                }
            } else if let Some(page) = self.results.get(idx).map(|r| r.page) {
                self.jump = Some(page);
            }
        }
    }

    // -- Barre d'outils flottante ------------------------------------------

    fn toolbar(&mut self, ctx: &egui::Context) {
        let top = TITLE_H + 8.0;

        // `Outils ▾` à gauche.
        Area::new(Id::new("tools"))
            .fixed_pos(pos2(RAIL_W + 10.0, top))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let inner = Frame::none()
                    .fill(WIDGET_BG)
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(10.0, 5.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                            icon_tools(ui.painter(), rect, ICON_BRIGHT);
                            ui.add_space(6.0);
                            ui.label(RichText::new("Tools").size(12.5).color(ICON_BRIGHT));
                            ui.add_space(4.0);
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(9.0, 9.0), Sense::hover());
                            icon_chevron(ui.painter(), rect, MUTED);
                        });
                    });
                let rect = inner.response.rect;
                let resp = ui.interact(rect, Id::new("tools-chip"), Sense::click());
                if resp.clicked() {
                    self.toast(ctx, "Tool list — planned");
                }
            });

        // Pastilles de mode au centre.
        Area::new(Id::new("modes"))
            .anchor(Align2::CENTER_TOP, vec2(0.0, top))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(WIDGET_BG)
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(4.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (idx, label) in MODES.iter().enumerate() {
                                let selected = self.mode == Some(idx);
                                let btn = ui.add(
                                    Button::new(RichText::new(*label).size(12.5).color(
                                        if selected { Color32::WHITE } else { ICON_BRIGHT },
                                    ))
                                    .frame(false)
                                    .fill(if selected {
                                        ACCENT
                                    } else {
                                        Color32::TRANSPARENT
                                    })
                                    .rounding(Rounding::same(6.0))
                                    .min_size(vec2(0.0, 24.0)),
                                );
                                if btn.clicked() {
                                    self.mode = if selected { None } else { Some(idx) };
                                    self.toast(ctx, format!("{label} mode — planned"));
                                }
                            }
                        });
                    });
            });

        // Annuler / rétablir / débordement à droite.
        Area::new(Id::new("actions"))
            .anchor(Align2::RIGHT_TOP, vec2(-14.0, top))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(WIDGET_BG)
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(6.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let undo = {
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                let color = if resp.hovered() { ICON_BRIGHT } else { ICON };
                                icon_undo(ui.painter(), rect.shrink(6.0), color);
                                resp
                            };
                            if undo.clicked() {
                                self.toast(ctx, "Undo — planned");
                            }
                            let redo = {
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                let color = if resp.hovered() { ICON_BRIGHT } else { ICON };
                                icon_redo(ui.painter(), rect.shrink(6.0), color);
                                resp
                            };
                            if redo.clicked() {
                                self.toast(ctx, "Redo — planned");
                            }
                            let more = {
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                let color = if resp.hovered() { ICON_BRIGHT } else { ICON };
                                icon_ellipsis(ui.painter(), rect.shrink(6.0), color);
                                resp
                            };
                            if more.clicked() {
                                self.toast(ctx, "Save / more — planned");
                            }
                        });
                    });
            });
    }

    // -- Canevas -----------------------------------------------------------

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
        let ppp = ctx.pixels_per_point();
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
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
                    let (rect, resp) =
                        ui.allocate_exact_size(size + vec2(0.0, PAGE_SPACING), Sense::click());
                    let page_rect = Rect::from_min_size(rect.min, size);

                    if jump == Some(page as usize) {
                        ui.scroll_to_rect(rect, Some(Align::Min));
                        jump = None;
                    }

                    if viewport.intersects(rect) {
                        if first_visible.is_none() {
                            first_visible = Some(page as usize);
                        }

                        // Ombre douce sous la page.
                        ui.painter().rect_filled(
                            page_rect.translate(vec2(0.0, 2.0)).expand(1.0),
                            Rounding::same(3.0),
                            Color32::from_black_alpha(70),
                        );

                        let bucket = (scale * ppp * 100.0).round() as u32;
                        if let Some(tex) = Self::page_texture(
                            &ctx, renderer, tab, page, bucket, scale, ppp, status,
                        ) {
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
                if tab.cache.len() > 40 {
                    let current = tab.page as i64;
                    tab.cache
                        .retain(|key, _| (key.0 as i64 - current).abs() < 10);
                }
            });
    }

    fn empty_state(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(160.0);
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

    // -- Superpositions ----------------------------------------------------

    /// Pastille bas-droite : page · zoom, façon « organize » de la référence.
    fn zoom_overlay(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.active_tab() else {
            return;
        };
        let (page, total, zoom_label) = (
            tab.page,
            tab.total(),
            tab.zoom
                .map(|z| format!("{:.0}%", z * 100.0))
                .unwrap_or_else(|| "Fit".to_owned()),
        );
        let engine = self.renderer_label;
        let mut delta = 0.0_f32;

        Area::new(Id::new("zoombar"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-14.0, -12.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(0x28, 0x28, 0x28, 0xF0))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(8.0, 4.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let minus = ui.add(
                                Button::new(RichText::new("−").size(13.0).color(ICON))
                                    .frame(false)
                                    .min_size(vec2(18.0, 18.0)),
                            );
                            if minus.clicked() {
                                delta = -1.0;
                            }
                            ui.label(
                                RichText::new(format!("{page} / {total}"))
                                    .size(11.0)
                                    .color(MUTED),
                            );
                            ui.add_space(6.0);
                            ui.label(RichText::new(&zoom_label).size(11.0).color(MUTED));
                            ui.add_space(6.0);
                            ui.label(RichText::new(engine).size(10.0).color(MUTED));
                            let plus = ui.add(
                                Button::new(RichText::new("+").size(13.0).color(ICON))
                                    .frame(false)
                                    .min_size(vec2(18.0, 18.0)),
                            );
                            if plus.clicked() {
                                delta = 1.0;
                            }
                        });
                    });
            });

        if delta < 0.0 {
            self.zoom_by(1.0 / 1.1);
        } else if delta > 0.0 {
            self.zoom_by(1.1);
        }
    }

    /// Message éphémère en bas au centre (5 s).
    fn toast_overlay(&mut self, ctx: &egui::Context) {
        let (text, mut stamp) = self.toast.clone();
        if text.is_empty() {
            return;
        }
        let now = ctx.input(|i| i.time);
        if stamp.is_nan() {
            stamp = now;
            self.toast.1 = now;
        }
        if now - stamp > 5.0 {
            return;
        }

        Area::new(Id::new("toast"))
            .anchor(Align2::CENTER_BOTTOM, vec2(0.0, -40.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(Color32::from_rgba_unmultiplied(0x28, 0x28, 0x28, 0xF2))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(12.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new(&text).size(11.5).color(ICON_BRIGHT));
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
        self.title_bar(ctx);
        self.rail(ctx);
        self.side_panel(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(CANVAS_BG))
            .show(ctx, |ui| {
                self.canvas(ui);
            });
        self.toolbar(ctx);
        self.zoom_overlay(ctx);
        if !self.render_status.is_empty() {
            let message = std::mem::take(&mut self.render_status);
            self.toast(ctx, message);
        }
        self.toast_overlay(ctx);
    }
}

// --------------------------------------------------------------------------
// Thème
// --------------------------------------------------------------------------

fn apply_theme(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CANVAS_BG;
    visuals.window_fill = CHROME_BG;
    visuals.extreme_bg_color = WIDGET_BG;
    visuals.faint_bg_color = WIDGET_BG;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.0_f32, ACCENT);
    visuals.widgets.noninteractive.bg_fill = CANVAS_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, SEPARATOR);
    visuals.widgets.inactive.bg_fill = WIDGET_BG;
    visuals.widgets.inactive.weak_bg_fill = WIDGET_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, SEPARATOR);
    visuals.widgets.hovered.bg_fill = HOVER_BG;
    visuals.widgets.hovered.weak_bg_fill = HOVER_BG;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(0x5A));
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    ctx.set_visuals(visuals);

    ctx.style_mut(|style| {
        style.spacing.button_padding = vec2(8.0, 4.0);
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.visuals.window_rounding = Rounding::same(10.0);
        style.visuals.menu_rounding = Rounding::same(8.0);
    });
}

// --------------------------------------------------------------------------
// Widgets maison
// --------------------------------------------------------------------------

/// Bouton-icône 36×36 avec fond sélection/hover.
fn rail_button(
    ui: &mut egui::Ui,
    selected: bool,
    draw: fn(&Painter, Rect, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(8.0), ACCENT);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(8.0), WIDGET_BG);
    }
    let color = if selected {
        Color32::WHITE
    } else if resp.hovered() {
        ICON_BRIGHT
    } else {
        ICON
    };
    draw(ui.painter(), rect.shrink(9.0), color);
    resp
}

/// Bouton-icône neutre de la barre de titre.
fn icon_button(
    ui: &mut egui::Ui,
    w: f32,
    h: f32,
    selected: bool,
    draw: fn(&Painter, Rect, Color32),
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
    }
    let color = if resp.hovered() { ICON_BRIGHT } else { ICON };
    draw(ui.painter(), rect.shrink(6.0), color);
    resp
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

// --------------------------------------------------------------------------
// Icônes vectorielles (identité maison)
// --------------------------------------------------------------------------

fn icon_home(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.5_f32, c);
    let base = r.shrink2(vec2(1.0, 2.0));
    let apex = pos2(base.center().x, base.min.y);
    let left = pos2(base.min.x, base.min.y + base.height() * 0.45);
    let right = pos2(base.max.x, base.min.y + base.height() * 0.45);
    p.line_segment([left, apex], stroke);
    p.line_segment([apex, right], stroke);
    p.line_segment(
        [
            pos2(left.x, left.y + 1.0),
            pos2(left.x, base.max.y),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(right.x, right.y + 1.0),
            pos2(right.x, base.max.y),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(left.x, base.max.y),
            pos2(right.x, base.max.y),
        ],
        stroke,
    );
}

fn icon_organize(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let s = r.width() * 0.32;
    let gap = r.width() * 0.14;
    let origin = r.center() - vec2((s + gap) / 2.0, (s + gap) / 2.0);
    for (dx, dy) in [(0.0, 0.0), (s + gap, 0.0), (0.0, s + gap), (s + gap, s + gap)] {
        p.rect_stroke(
            Rect::from_min_size(origin + vec2(dx, dy), vec2(s, s)),
            Rounding::same(1.5),
            stroke,
        );
    }
}

fn icon_search(p: &Painter, r: Rect, c: Color32) {    let stroke = Stroke::new(1.5_f32, c);
    let center = r.center() - vec2(r.width() * 0.08, r.height() * 0.08);
    let radius = r.width() * 0.28;
    p.circle_stroke(center, radius, stroke);
    let dir = vec2(0.707, 0.707);
    p.line_segment(
        [
            center + dir * (radius + 1.0),
            center + dir * (radius + r.width() * 0.22),
        ],
        stroke,
    );
}

fn icon_page(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let page = Rect::from_center_size(r.center(), vec2(r.width() * 0.66, r.height() * 0.9));
    p.rect_stroke(page, Rounding::same(2.0), stroke);
    for i in 0..3 {
        let y = page.min.y + page.height() * (0.3 + 0.22 * i as f32);
        p.line_segment(
            [pos2(page.min.x + 3.0, y), pos2(page.max.x - 3.0, y)],
            Stroke::new(1.1_f32, c),
        );
    }
}

fn icon_bookmark(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.5_f32, c);
    let w = r.width() * 0.6;
    let x0 = r.center().x - w / 2.0;
    let x1 = r.center().x + w / 2.0;
    let top = r.min.y + r.height() * 0.06;
    let bottom = r.max.y - r.height() * 0.06;
    let notch = bottom - r.height() * 0.28;
    p.add(Shape::closed_line(
        vec![
            pos2(x0, top),
            pos2(x1, top),
            pos2(x1, bottom),
            pos2(r.center().x, notch),
            pos2(x0, bottom),
        ],
        stroke,
    ));
}

fn icon_bubble(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let body = Rect::from_center_size(
        pos2(r.center().x, r.center().y - r.height() * 0.06),
        vec2(r.width() * 0.9, r.height() * 0.68),
    );
    p.rect_stroke(body, Rounding::same(4.0), stroke);
    p.line_segment(
        [
            pos2(body.min.x + body.width() * 0.22, body.max.y),
            pos2(body.min.x + body.width() * 0.3, body.max.y + r.height() * 0.2),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(body.min.x + body.width() * 0.3, body.max.y + r.height() * 0.2),
            pos2(body.min.x + body.width() * 0.46, body.max.y),
        ],
        stroke,
    );
}

fn icon_form(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let body = Rect::from_center_size(r.center(), vec2(r.width() * 0.9, r.height() * 0.8));
    p.rect_stroke(body, Rounding::same(2.0), stroke);
    for i in 0..2 {
        let y = body.min.y + body.height() * (0.32 + 0.36 * i as f32);
        p.line_segment(
            [pos2(body.min.x, y), pos2(body.max.x, y)],
            Stroke::new(1.0_f32, c),
        );
    }
    p.line_segment(
        [
            pos2(body.min.x + body.width() * 0.55, body.min.y),
            pos2(body.min.x + body.width() * 0.55, body.max.y),
        ],
        Stroke::new(1.0_f32, c),
    );
}

fn icon_attach(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.5_f32, c);
    // Trombone simplifié : diagonale + boucle.
    let top = pos2(r.max.x - 3.0, r.min.y + 2.0);
    let bottom = pos2(r.min.x + 3.0, r.max.y - 2.0);
    p.line_segment([top, bottom], stroke);
    p.circle_stroke(top, r.width() * 0.16, stroke);
}

fn icon_aspect(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let body = Rect::from_center_size(r.center(), vec2(r.width() * 0.8, r.height() * 0.8));
    p.rect_stroke(body, Rounding::same(2.0), stroke);
    p.line_segment([body.min, pos2(body.min.x + 4.0, body.min.y)], stroke);
}

fn icon_plus(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.5_f32, c);
    p.line_segment(
        [
            pos2(r.center().x, r.min.y + r.height() * 0.22),
            pos2(r.center().x, r.max.y - r.height() * 0.22),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(r.min.x + r.width() * 0.22, r.center().y),
            pos2(r.max.x - r.width() * 0.22, r.center().y),
        ],
        stroke,
    );
}

fn icon_close(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.3_f32, c);
    let d = r.width() * 0.22;
    p.line_segment(
        [
            pos2(r.center().x - d, r.center().y - d),
            pos2(r.center().x + d, r.center().y + d),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(r.center().x + d, r.center().y - d),
            pos2(r.center().x - d, r.center().y + d),
        ],
        stroke,
    );
}

fn icon_chevron(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.3_f32, c);
    p.line_segment(
        [pos2(r.min.x + 1.0, r.min.y + 1.0), pos2(r.center().x, r.max.y - 1.0)],
        stroke,
    );
    p.line_segment(
        [pos2(r.center().x, r.max.y - 1.0), pos2(r.max.x - 1.0, r.min.y + 1.0)],
        stroke,
    );
}

fn icon_tools(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let body = Rect::from_center_size(
        pos2(r.center().x, r.center().y + r.height() * 0.12),
        vec2(r.width() * 0.9, r.height() * 0.55),
    );
    p.rect_stroke(body, Rounding::same(2.0), stroke);
    p.line_segment(
        [
            pos2(body.min.x + 2.0, body.min.y),
            pos2(body.center().x - 2.0, body.min.y - r.height() * 0.28),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(body.center().x - 2.0, body.min.y - r.height() * 0.28),
            pos2(body.max.x - 2.0, body.min.y),
        ],
        stroke,
    );
}

fn icon_trash(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let body = Rect::from_min_max(
        pos2(r.min.x + r.width() * 0.22, r.min.y + r.height() * 0.3),
        pos2(r.max.x - r.width() * 0.22, r.max.y - r.height() * 0.08),
    );
    p.rect_stroke(body, Rounding::same(1.5), stroke);
    p.line_segment(
        [
            pos2(r.min.x + r.width() * 0.12, body.min.y),
            pos2(r.max.x - r.width() * 0.12, body.min.y),
        ],
        stroke,
    );
    p.line_segment(
        [
            pos2(r.center().x - r.width() * 0.14, r.min.y + r.height() * 0.16),
            pos2(r.center().x + r.width() * 0.14, r.min.y + r.height() * 0.16),
        ],
        stroke,
    );
}

fn icon_undo(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.6_f32, c);
    let centre = r.center() + vec2(0.0, r.height() * 0.12);
    let rad = r.width() * 0.34;
    let pts: Vec<egui::Pos2> = (0..=14)
        .map(|i| {
            let a = std::f32::consts::PI * (1.15 - 1.3 * (i as f32 / 14.0));
            centre + vec2(a.cos() * rad, -a.sin() * rad)
        })
        .collect();
    let tip = pts[0];
    p.add(Shape::line(pts, stroke));
    p.line_segment([tip, tip + vec2(4.0, -5.0)], stroke);
    p.line_segment([tip, tip + vec2(6.0, 3.0)], stroke);
}

fn icon_redo(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.6_f32, c);
    let centre = r.center() + vec2(0.0, r.height() * 0.12);
    let rad = r.width() * 0.34;
    let pts: Vec<egui::Pos2> = (0..=14)
        .map(|i| {
            let a = std::f32::consts::PI * (1.15 - 1.3 * (i as f32 / 14.0));
            centre + vec2(-a.cos() * rad, -a.sin() * rad)
        })
        .collect();
    let tip = pts[0];
    p.add(Shape::line(pts, stroke));
    p.line_segment([tip, tip + vec2(-4.0, -5.0)], stroke);
    p.line_segment([tip, tip + vec2(-6.0, 3.0)], stroke);
}

fn icon_ellipsis(p: &Painter, r: Rect, c: Color32) {
    for dx in [-1.0_f32, 0.0, 1.0] {
        p.circle_filled(
            r.center() + vec2(dx * r.width() * 0.3, 0.0),
            r.width() * 0.07,
            c,
        );
    }
}

fn icon_bell(p: &Painter, r: Rect, c: Color32) {
    let stroke = Stroke::new(1.4_f32, c);
    let dome = Rect::from_center_size(
        pos2(r.center().x, r.center().y - r.height() * 0.04),
        vec2(r.width() * 0.62, r.height() * 0.66),
    );
    p.add(Shape::closed_line(
        vec![
            pos2(dome.min.x, dome.max.y),
            pos2(dome.min.x + 1.5, dome.min.y + 3.0),
            pos2(dome.center().x, dome.min.y),
            pos2(dome.max.x - 1.5, dome.min.y + 3.0),
            pos2(dome.max.x, dome.max.y),
        ],
        stroke,
    ));
    p.line_segment(
        [
            pos2(dome.min.x - 1.5, dome.max.y),
            pos2(dome.max.x + 1.5, dome.max.y),
        ],
        stroke,
    );
    p.circle_filled(pos2(r.center().x, dome.max.y + 3.0), 1.4, c);
}
