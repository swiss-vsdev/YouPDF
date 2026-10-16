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
    Margin, Order, Painter, Rect, RichText, Rounding, ScrollArea, Sense, SidePanel, Stroke,
    TextEdit, TextureHandle, TextureOptions, TopBottomPanel, ViewportCommand,
};
use egui_phosphor::regular as ph;
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
const PANEL_W: f32 = 210.0;
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
    draw: IconFn,
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

/// Signature commune des icônes maison.
type IconFn = fn(&Painter, Rect, Color32);

const MODES: [(&str, IconFn); 4] = [
    ("Comment", icon_bubble),
    ("Edit", icon_cursor),
    ("Form", icon_form),
    ("Redact", icon_highlighter),
];

/// Outil d'une barre de mode : libellé, icône, sous-menu (`▾`), primaire.
struct ModeTool(&'static str, IconFn, bool, bool);

const COMMENT_TOOLS: &[ModeTool] = &[
    ModeTool("Select", icon_cursor, false, false),
    ModeTool("Highlight", icon_highlighter, true, false),
    ModeTool("Text", icon_text, true, false),
    ModeTool("Comment", icon_bubble, true, false),
    ModeTool("Ink", icon_pen, true, false),
    ModeTool("Shapes", icon_shapes, true, false),
    ModeTool("Insert", icon_plus, true, false),
    ModeTool("Link", icon_link, false, false),
    ModeTool("Stamp", icon_stamp, true, false),
    ModeTool("Signature", icon_signature, true, false),
    ModeTool("Eraser", icon_eraser, false, false),
];

const EDIT_TOOLS: &[ModeTool] = &[
    ModeTool("Select", icon_cursor, false, false),
    ModeTool("Text", icon_text, true, false),
    ModeTool("Image", icon_image, true, false),
    ModeTool("Link", icon_link, true, false),
    ModeTool("Layout", icon_layout, false, false),
];

const FORM_TOOLS: &[ModeTool] = &[
    ModeTool("Select", icon_cursor, false, false),
    ModeTool("Text field", icon_text_field, false, false),
    ModeTool("Checkbox", icon_checkbox, false, false),
    ModeTool("Radio", icon_radio, false, false),
    ModeTool("Dropdown", icon_dropdown, false, false),
    ModeTool("List", icon_list, false, false),
    ModeTool("Button", icon_button_shape, false, false),
    ModeTool("Date", icon_date, false, false),
    ModeTool("Barcode", icon_barcode, false, false),
    ModeTool("Signature", icon_signature, false, false),
    ModeTool("Image", icon_image, false, false),
    ModeTool("Divider", icon_divider, false, false),
    ModeTool("Other", icon_ellipsis, true, false),
];

const REDACT_TOOLS: &[ModeTool] = &[
    ModeTool("Select", icon_cursor, false, false),
    ModeTool("Redact pages", icon_highlighter, true, false),
    ModeTool("Find & redact", icon_search, true, false),
    ModeTool("Sanitize", icon_sparkle, false, false),
    ModeTool("Apply", icon_check, false, true),
];

fn mode_tools(mode: usize) -> &'static [ModeTool] {
    match mode {
        0 => COMMENT_TOOLS,
        1 => EDIT_TOOLS,
        2 => FORM_TOOLS,
        _ => REDACT_TOOLS,
    }
}

/// Entrées du menu `Outils` : libellé, raccourci.
const TOOLS_MENU: [(&str, &str); 4] = [
    ("Comment", "^1"),
    ("Edit PDF", "^2"),
    ("Prepare form", "^3"),
    ("Redact", "^4"),
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
    mode: Option<usize>,
    /// Liste déroulante `Outils` ouverte.
    tools_open: bool,

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
            tools_open: false,
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

        // Aide au développement : `YOUPDF_MODE=0..3` ouvre l'app dans ce mode.
        if let Ok(mode) = std::env::var("YOUPDF_MODE") {
            app.mode = mode.parse::<usize>().ok().filter(|m| *m < MODES.len());
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

                    let home = icon_button(ui, 30.0, 28.0, false, icon_home);
                    if home.clicked() {
                        self.toast(ctx, "Home (recent files) — planned");
                    }
                    ui.add_space(12.0);

                    let mut select = None;
                    let mut close = None;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let selected = idx == self.active;
                        let color = if selected { ICON_BRIGHT } else { MUTED };
                        let galley = ui.painter().layout_no_wrap(
                            shorten(&tab.title, 24),
                            FontId::proportional(12.5),
                            color,
                        );
                        let width = if selected {
                            (galley.size().x + 56.0).max(170.0)
                        } else {
                            galley.size().x + 28.0
                        };
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(width, 30.0), Sense::click());

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
                            pos2(rect.min.x + 14.0, rect.center().y - galley.size().y / 2.0),
                            galley,
                            color,
                        );

                        if selected {
                            let x_rect = Rect::from_center_size(
                                pos2(rect.max.x - 16.0, rect.center().y),
                                vec2(18.0, 18.0),
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
                        ui.add_space(10.0);
                    }

                    let plus = icon_button(ui, 24.0, 24.0, false, icon_plus);
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
                        let bell = icon_button(ui, 26.0, 26.0, false, icon_bell);
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
                        ui.add_space(10.0);
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
                    if panel == Panel::Search {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(10.0);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(6.0),
                                    Color32::from_rgb(0x33, 0x33, 0x33),
                                );
                            }
                            icon_funnel(
                                ui.painter(),
                                rect.shrink(7.0),
                                if resp.hovered() { ICON_BRIGHT } else { ICON },
                            );
                            if resp.clicked() {
                                self.toast(ctx, "Filters — planned");
                            }
                        });
                    }
                });
                ui.add_space(8.0);
                let sep_y = ui.cursor().top();
                ui.painter().hline(
                    ui.max_rect().x_range(),
                    sep_y,
                    Stroke::new(1.0_f32, SEPARATOR),
                );
                ui.add_space(10.0);
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
                let tile_width = ui.available_width().max(80.0);
                for page in 1..=total as u32 {
                    let (wp, hp) = tab.doc.page_size(page);
                    let scale = THUMB_HEIGHT / hp.max(1.0);
                    let size = vec2(wp * scale, hp * scale);
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(tile_width, size.y + 28.0), Sense::click());
                    let selected = page as usize == tab.page;
                    let img_rect = Rect::from_center_size(
                        pos2(rect.center().x, rect.min.y + 8.0 + size.y / 2.0),
                        size,
                    );

                    if selected {
                        ui.painter().rect_filled(
                            img_rect.expand(5.0),
                            Rounding::same(4.0),
                            THUMB_TINT,
                        );
                    } else if resp.hovered() {
                        ui.painter().rect_filled(
                            img_rect.expand(5.0),
                            Rounding::same(4.0),
                            Color32::from_rgb(0x3C, 0x3C, 0x3C),
                        );
                    }

                    if let Some(tex) =
                        Self::page_texture(&ctx, renderer, tab, page, 1, scale, ppp, status)
                    {
                        ui.painter().image(
                            tex.id(),
                            img_rect,
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                    }
                    if selected {
                        ui.painter().rect_stroke(
                            img_rect.expand(5.0),
                            4.0,
                            Stroke::new(1.0_f32, ACCENT),
                        );
                    }
                    ui.painter().text(
                        pos2(rect.center().x, img_rect.max.y + 14.0),
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
        // Rangée d'actions : recherche · Aa (remplacer) · corbeille · surlignage.
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if panel_action(ui, true, icon_search).clicked() {
                self.toast(ctx, "Search text");
            }

            let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
            if resp.hovered() {
                ui.painter()
                    .rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(0x33, 0x33, 0x33));
            }
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Aa",
                FontId::proportional(12.0),
                if resp.hovered() { ICON_BRIGHT } else { ICON },
            );
            if resp.clicked() {
                self.toast(ctx, "Find & replace — planned");
            }

            if panel_action(ui, false, icon_trash).clicked() {
                self.toast(ctx, "Find & delete — planned");
            }
            if panel_action(ui, false, icon_highlighter).clicked() {
                self.toast(ctx, "Find & redact — planned");
            }
        });
        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let resp = ui.add_sized(
                vec2(PANEL_W - 26.0, 28.0),
                TextEdit::singleline(&mut self.search_query).hint_text("Search text"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_search(ctx);
            }
        });
        ui.add_space(12.0);

        let mut jump = None;
        let mut forget = None;
        let mut clear_history = false;
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
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(10.0);
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(6.0),
                                    Color32::from_rgb(0x33, 0x33, 0x33),
                                );
                            }
                            icon_trash(
                                ui.painter(),
                                rect.shrink(7.0),
                                if resp.hovered() { ICON_BRIGHT } else { ICON },
                            );
                            if resp.clicked() {
                                clear_history = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    for (idx, query) in self.history.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(PANEL_W - 24.0, 28.0), Sense::click());
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
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
                        ui.add_space(4.0);
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
                        ui.add_space(4.0);
                    }
                }
            });

        if clear_history {
            self.history.clear();
        }
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
        // Les barres flottantes sont ancrées au canevas, pas à la fenêtre :
        // le panneau latéral les décale (comme dans la référence).
        let canvas_left = RAIL_W
            + if self.panel.is_some() { PANEL_W } else { 0.0 };

        // `Outils ▾` à gauche.
        Area::new(Id::new("tools"))
            .fixed_pos(pos2(canvas_left + 10.0, top))
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
                    self.tools_open = !self.tools_open;
                }
            });

        // Pastilles de mode au centre.
        // Pastilles de mode, ou barre d'outils du mode actif.
        let modes_anchor = vec2(canvas_left / 2.0, top);
        if let Some(mode) = self.mode {
            let mut exit = false;
            Area::new(Id::new("modetools"))
                .anchor(Align2::CENTER_TOP, modes_anchor)
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(WIDGET_BG)
                        .rounding(Rounding::same(8.0))
                        .inner_margin(Margin::symmetric(4.0, 4.0))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                // Les modes à libellés longs gardent le texte ;
                                // les autres restent compacts, info-bulle au survol.
                                let labeled = mode == 1 || mode == 3;
                                for ModeTool(label, draw, chevron, primary) in mode_tools(mode) {
                                    let resp = if labeled {
                                        pill_button(ui, label, *draw, *chevron, *primary, false)
                                    } else {
                                        icon_pill(ui, *draw, *chevron, *primary)
                                    };
                                    if resp.on_hover_text(*label).clicked() {
                                        self.toast(ctx, format!("{label} — planned"));
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
            if exit {
                self.mode = None;
            }
        } else {
            Area::new(Id::new("modes"))
                .anchor(Align2::CENTER_TOP, modes_anchor)
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(WIDGET_BG)
                        .rounding(Rounding::same(8.0))
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
                                    }
                                }
                            });
                        });
                });
        }

        // Liste déroulante `Outils`.
        if self.tools_open {
            let mut chosen: Option<usize> = None;
            let mut close = false;
            Area::new(Id::new("tools-menu"))
                .fixed_pos(pos2(canvas_left + 10.0, top + 38.0))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(CHROME_BG)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, SEPARATOR))
                        .inner_margin(Margin::symmetric(4.0, 4.0))
                        .show(ui, |ui| {
                            for (idx, (label, shortcut)) in TOOLS_MENU.iter().enumerate() {
                                if menu_row(ui, label, shortcut) {
                                    chosen = Some(idx);
                                }
                            }
                            let sep = ui.cursor().top() + 3.0;
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, SEPARATOR),
                            );
                            ui.add_space(8.0);
                            if menu_row(ui, "Organize pages", "^5") {
                                close = true;
                                self.toast(ctx, "Organize pages — planned");
                            }
                        });
                });
            if let Some(idx) = chosen {
                self.mode = Some(idx);
                self.tools_open = false;
            } else if close {
                self.tools_open = false;
            }
        }

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
                                let color = if resp.hovered() { ICON_BRIGHT } else { MUTED };
                                icon_undo(ui.painter(), rect.shrink(6.0), color);
                                resp
                            };
                            if undo.clicked() {
                                self.toast(ctx, "Undo — planned");
                            }
                            let redo = {
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                let color = if resp.hovered() { ICON_BRIGHT } else { MUTED };
                                icon_redo(ui.painter(), rect.shrink(6.0), color);
                                resp
                            };
                            if redo.clicked() {
                                self.toast(ctx, "Redo — planned");
                            }
                            let more = {
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(24.0, 24.0), Sense::click());
                                let color = if resp.hovered() { ICON_BRIGHT } else { MUTED };
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
                            let (rect, minus) = ui
                                .allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                            if minus.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(5.0),
                                    HOVER_BG,
                                );
                            }
                            icon_minus(
                                ui.painter(),
                                rect.shrink(2.0),
                                if minus.hovered() { ICON_BRIGHT } else { ICON },
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
                            let (rect, plus) = ui
                                .allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                            if plus.hovered() {
                                ui.painter().rect_filled(rect, Rounding::same(5.0), HOVER_BG);
                            }
                            icon_plus(
                                ui.painter(),
                                rect.shrink(2.0),
                                if plus.hovered() { ICON_BRIGHT } else { ICON },
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
    // Police d'icônes Phosphor (MIT) ajoutée à la famille proportionnelle.
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = CANVAS_BG;
    visuals.window_fill = CHROME_BG;
    visuals.extreme_bg_color = WIDGET_BG;
    visuals.faint_bg_color = WIDGET_BG;
    visuals.selection.bg_fill = ACCENT;
    visuals.selection.stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.noninteractive.bg_fill = CANVAS_BG;
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, SEPARATOR);
    visuals.widgets.inactive.bg_fill = WIDGET_BG;
    visuals.widgets.inactive.weak_bg_fill = WIDGET_BG;
    visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(0x5A));
    visuals.widgets.inactive.rounding = Rounding::same(6.0);
    visuals.widgets.hovered.bg_fill = HOVER_BG;
    visuals.widgets.hovered.weak_bg_fill = HOVER_BG;
    visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_gray(0x6E));
    visuals.widgets.hovered.rounding = Rounding::same(6.0);
    visuals.widgets.active.bg_fill = ACCENT;
    visuals.widgets.active.weak_bg_fill = ACCENT;
    visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, Color32::WHITE);
    visuals.widgets.active.rounding = Rounding::same(6.0);
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


/// Pastille de mode / d'outil : icône + libellé (+ `▾`), fond si actif.
fn pill_button(
    ui: &mut egui::Ui,
    label: &str,
    draw: IconFn,
    chevron: bool,
    primary: bool,
    selected: bool,
) -> egui::Response {
    let text_color = Color32::WHITE;
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(12.5),
        if primary || selected { Color32::WHITE } else { ICON_BRIGHT },
    );
    let width = 12.0
        + 15.0
        + 7.0
        + galley.size().x
        + if chevron { 15.0 } else { 0.0 }
        + 12.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    if primary || selected {
        ui.painter().rect_filled(rect, Rounding::same(6.0), ACCENT);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(6.0), HOVER_BG);
    }
    let icon_rect = Rect::from_center_size(
        pos2(rect.min.x + 12.0 + 7.5, rect.center().y),
        vec2(15.0, 15.0),
    );
    draw(ui.painter(), icon_rect, text_color);
    ui.painter().galley(
        pos2(
            rect.min.x + 12.0 + 15.0 + 7.0,
            rect.center().y - galley.size().y / 2.0,
        ),
        galley,
        text_color,
    );
    if chevron {
        let ch_rect = Rect::from_center_size(
            pos2(rect.max.x - 12.0 - 5.0, rect.center().y),
            vec2(9.0, 9.0),
        );
        icon_chevron(ui.painter(), ch_rect, MUTED);
    }
    resp
}

/// Pastille d'outil compacte (icône seule, info-bulle au survol).
fn icon_pill(
    ui: &mut egui::Ui,
    draw: IconFn,
    chevron: bool,
    primary: bool,
) -> egui::Response {
    let width = if chevron { 38.0 } else { 30.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    if primary {
        ui.painter().rect_filled(rect, Rounding::same(6.0), ACCENT);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(6.0), HOVER_BG);
    }
    let icon_rect = Rect::from_center_size(
        pos2(rect.min.x + 13.0, rect.center().y),
        vec2(15.0, 15.0),
    );
    draw(ui.painter(), icon_rect, Color32::WHITE);
    if chevron {
        let ch_rect = Rect::from_center_size(
            pos2(rect.max.x - 11.0, rect.center().y),
            vec2(8.0, 8.0),
        );
        icon_chevron(ui.painter(), ch_rect, MUTED);
    }
    resp
}

/// Ligne de menu `Outils` : libellé à gauche, raccourci à droite.
fn menu_row(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), HOVER_BG);
    }
    ui.painter().text(
        pos2(rect.min.x + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        ICON_BRIGHT,
    );
    ui.painter().text(
        pos2(rect.max.x - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        shortcut,
        FontId::proportional(11.0),
        MUTED,
    );
    resp.clicked()
}

/// Bouton-icône 38×38 avec fond sélection/hover.
fn rail_button(
    ui: &mut egui::Ui,
    selected: bool,
    draw: IconFn,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
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
    draw: IconFn,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click());
    if selected || resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
    }
    let color = if resp.hovered() { ICON_BRIGHT } else { ICON };
    draw(ui.painter(), rect.shrink(6.0), color);
    resp
}


/// Bouton d'action d'un panneau (icône 30×28, fond si actif).
fn panel_action(
    ui: &mut egui::Ui,
    selected: bool,
    draw: IconFn,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(0x33, 0x33, 0x33));
    }
    let color = if selected || resp.hovered() { ICON_BRIGHT } else { ICON };
    draw(ui.painter(), rect.shrink(8.0), color);
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
// Icônes Phosphor (police d'icônes, licence MIT)
// --------------------------------------------------------------------------

/// Trace un glyphe Phosphor centré dans `r`.
fn glyph(p: &Painter, r: Rect, c: Color32, ch: &str) {
    p.text(
        r.center(),
        Align2::CENTER_CENTER,
        ch,
        FontId::proportional((r.height() * 1.12).max(9.0)),
        c,
    );
}

fn icon_home(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::HOUSE);
}
fn icon_organize(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::SQUARES_FOUR);
}
fn icon_search(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::MAGNIFYING_GLASS);
}
fn icon_page(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::FILE_TEXT);
}
fn icon_bookmark(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::BOOKMARK_SIMPLE);
}
fn icon_bubble(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CHAT_CIRCLE_DOTS);
}
fn icon_form(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::LIST_CHECKS);
}
fn icon_attach(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::PAPERCLIP);
}
fn icon_aspect(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CUBE);
}
fn icon_plus(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::PLUS);
}
fn icon_close(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::X);
}
fn icon_chevron(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CARET_DOWN);
}
fn icon_tools(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::TOOLBOX);
}
fn icon_trash(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::TRASH);
}
fn icon_undo(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::ARROW_COUNTER_CLOCKWISE);
}
fn icon_redo(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::ARROW_CLOCKWISE);
}
fn icon_ellipsis(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::DOTS_THREE);
}
fn icon_bell(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::BELL);
}
fn icon_cursor(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CURSOR);
}
fn icon_highlighter(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::HIGHLIGHTER);
}
fn icon_funnel(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::FUNNEL);
}
fn icon_text(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::TEXT_T);
}
fn icon_pen(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::PEN_NIB);
}
fn icon_shapes(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::SHAPES);
}
fn icon_link(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::LINK_SIMPLE);
}
fn icon_stamp(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::STAMP);
}
fn icon_signature(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::SIGNATURE);
}
fn icon_eraser(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::ERASER);
}
fn icon_text_field(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::TEXTBOX);
}
fn icon_checkbox(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CHECK_SQUARE);
}
fn icon_radio(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::RADIO_BUTTON);
}
fn icon_dropdown(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CARET_UP_DOWN);
}
fn icon_list(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::LIST_BULLETS);
}
fn icon_button_shape(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::RECTANGLE);
}
fn icon_date(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CALENDAR);
}
fn icon_barcode(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::BARCODE);
}
fn icon_image(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::IMAGE);
}
fn icon_divider(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::MINUS);
}
fn icon_sparkle(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::SPARKLE);
}
fn icon_check(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CHECK);
}
fn icon_layout(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::LAYOUT);
}

// Icônes d'application (fichiers, zoom, gestion de pages).
fn icon_minus(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::MINUS);
}
