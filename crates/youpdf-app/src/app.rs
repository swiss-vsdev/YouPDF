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
use egui_phosphor::regular as ph;
use youpdf_core::render::NullRenderer;
use youpdf_core::{
    fold_case, Annotation, AnnotationKind, AnnotationStyle, AttachmentInfo, Document,
    FormField, FormFieldKind, OutlineItem, PdfiumRenderer, RedactionRegion, RenderOptions,
    Renderer, SearchIndex, SearchResult, STAMP_NAMES,
};

// --------------------------------------------------------------------------
// Palette
// --------------------------------------------------------------------------

const CANVAS_BG: Color32 = Color32::from_rgb(0x30, 0x30, 0x30);
/// Rouge des marques de caviardage.
const REDACT_RED: Color32 = Color32::from_rgb(0xE0, 0x4F, 0x5F);
/// Jaune des correspondances de recherche.
const SEARCH_MATCH: Color32 = Color32::from_rgba_premultiplied(0x50, 0x40, 0x05, 0x6E);

/// Fonds proposés dans le panneau Aspect.
const CANVAS_STYLES: [Color32; 4] = [
    Color32::from_rgb(0x30, 0x30, 0x30),
    Color32::from_rgb(0x1E, 0x1E, 0x1E),
    Color32::from_rgb(0x4A, 0x4A, 0x4A),
    Color32::from_rgb(0xD8, 0xD8, 0xD8),
];
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

/// Nature du brouillon de texte en cours de saisie.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DraftKind {
    Note,
    FreeText,
    Signature,
    Link,
    Stamp,
}

/// Saisie en cours pour une annotation textuelle.
struct TextDraft {
    kind: DraftKind,
    page: usize,
    rect: [f32; 4],
    buffer: String,
    /// Premier affichage : demande le focus clavier.
    fresh: bool,
}

/// Ce que fait un outil sur le canevas.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolAction {
    None,
    /// Glissé rectangulaire (surlignage, formes).
    DragRect(AnnotationKind),
    /// Crayon libre.
    Ink,
    /// Clic simple (note, texte, tampon, signature, lien).
    Click(AnnotationKind),
    Erase,
    Select,
    /// Glissé d'une zone à caviarder.
    RedactRect,
}

/// Couleurs de préréglage proposées dans la popup.
const PRESET_COLORS: [[u8; 3]; 7] = [
    [255, 235, 59],
    [120, 220, 120],
    [255, 140, 200],
    [255, 170, 60],
    [120, 170, 255],
    [230, 80, 80],
    [40, 40, 40],
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
    /// `None` = ajustement automatique (voir `zoom_fit_page`).
    zoom: Option<f32>,
    /// Vrai quand l'ajustement automatique garde toute la page visible.
    zoom_fit_page: bool,
    /// Bitmaps déjà rendus ; clé `(page, bucket)`, bucket 1 = vignette.
    cache: HashMap<(u32, u32), TextureHandle>,
    /// Modifications non enregistrées.
    dirty: bool,
    /// Annotations YouPDF en mémoire (chargées à la demande).
    annots: Vec<Annotation>,
    annots_loaded: bool,
    /// Piles d'annulation/rétablissement (instantanés des annotations).
    undo: Vec<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
    /// Plan du document (signets).
    outline: Vec<OutlineItem>,
    /// Champs de formulaire du document.
    forms: Vec<FormField>,
    /// Pièces jointes du document.
    attachments: Vec<AttachmentInfo>,
    /// Vrai quand plan, formulaires et pièces jointes ont été lus.
    structure_loaded: bool,
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
    /// Menu `…` (enregistrer, exporter, propriétés).
    actions_open: bool,
    /// Fenêtre « Propriétés du document ».
    props_open: bool,
    /// Outil d'annotation actif (index dans la barre du mode Comment).
    annot_tool: Option<usize>,
    /// Préréglages du pinceau/surligneur.
    annot_color: [u8; 3],
    annot_opacity: f32,
    annot_width: f32,
    /// Popup de préréglages d'outil.
    preset_open: bool,
    /// Brouillon de texte (note, texte libre, signature, lien).
    text_draft: Option<TextDraft>,
    /// Annotation sélectionnée (outil Sélection).
    selected_annot: usize,
    /// Sélection active.
    has_annot_selection: bool,
    /// Tracé de crayon en cours (coordonnées PDF).
    ink_current: Vec<[f32; 2]>,
    /// Coin de départ d'un glissé en cours (coordonnées PDF).
    drag_from: Option<[f32; 2]>,
    /// Position courante du glissé (coordonnées PDF), pour l'aperçu.
    drag_to: Option<[f32; 2]>,
    /// Tampon choisi pour l'outil Tampon.
    stamp_choice: usize,
    /// Type d'annotation créé par le brouillon courant.
    draft_annot_kind: AnnotationKind,
    /// Résultat de recherche actif (navigation ⌘G).
    search_active: Option<usize>,
    /// Rectangles de la correspondance active, en coordonnées PDF.
    search_rects: Vec<[f32; 4]>,
    /// Page des rectangles de recherche affichés.
    search_rects_page: Option<u32>,
    /// Saisie de la page à atteindre (pastille bas-droite).
    page_entry: String,
    /// Ligne « remplacer » ouverte.
    replace_open: bool,
    /// Texte de remplacement.
    replace_text: String,
    /// Régions marquées pour le caviardage.
    redact_regions: Vec<RedactionRegion>,
    /// Fenêtre de confirmation du caviardage.
    redact_modal: bool,
    /// Nettoyer les métadonnées en même temps que le caviardage.
    redact_sanitize: bool,
    /// Champ de formulaire sélectionné et saisie en cours.
    form_edit: Option<(usize, String)>,
    /// Fond du canevas choisi dans Aspect.
    canvas_style: usize,
    /// Ombre portée sous les pages.
    page_shadow: bool,
    /// Échelle des vignettes du panneau Pages.
    thumb_scale: f32,

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
            actions_open: false,
            props_open: false,
            annot_tool: None,
            annot_color: [255, 235, 59],
            annot_opacity: 0.45,
            annot_width: 2.0,
            preset_open: false,
            text_draft: None,
            selected_annot: 0,
            has_annot_selection: false,
            ink_current: Vec::new(),
            drag_from: None,
            drag_to: None,
            stamp_choice: 0,
            draft_annot_kind: AnnotationKind::FreeText,
            search_active: None,
            search_rects: Vec::new(),
            search_rects_page: None,
            page_entry: String::new(),
            replace_open: false,
            replace_text: String::new(),
            redact_regions: Vec::new(),
            redact_modal: false,
            redact_sanitize: true,
            form_edit: None,
            canvas_style: 0,
            page_shadow: true,
            thumb_scale: 1.0,
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
                    zoom_fit_page: false,
                    cache: HashMap::new(),
                    dirty: false,
                    annots: Vec::new(),
                    annots_loaded: false,
                    undo: Vec::new(),
                    redo: Vec::new(),
                    outline: Vec::new(),
                    forms: Vec::new(),
                    attachments: Vec::new(),
                    structure_loaded: false,
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

    // -- Enregistrement ----------------------------------------------------

    /// ⌘S : écrit le document sur son chemin, ou demande un chemin.
    fn save_active(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(path) = tab.path.clone() else {
            self.save_as(ctx);
            return;
        };
        if let Err(err) = self.flush_annots() {
            self.toast(ctx, format!("Save failed: {err}"));
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        tab.annots_loaded = false;
        match tab.doc.save(&path) {
            Ok(()) => {
                tab.dirty = false;
                self.toast(ctx, format!("Saved {}", path.display()));
            }
            Err(err) => self.toast(ctx, format!("Save failed: {err}")),
        }
    }

    /// ⇧⌘S : choisit une destination puis écrit une copie.
    fn save_as(&mut self, ctx: &egui::Context) {
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
        match tab.doc.save(&path) {
            Ok(()) => {
                tab.dirty = false;
                tab.path = Some(path.clone());
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
    fn export_text(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let stem = tab
            .path
            .as_ref()
            .and_then(|p| p.file_stem().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_else(|| tab.title.clone());
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Texte", &["txt"])
            .set_file_name(format!("{stem}.txt"))
            .save_file()
        else {
            return;
        };
        let text = match tab.doc.all_text() {
            Ok(text) => text,
            Err(err) => {
                self.toast(ctx, format!("Text export failed: {err}"));
                return;
            }
        };
        match std::fs::write(&path, text) {
            Ok(()) => self.toast(ctx, format!("Exported {}", path.display())),
            Err(err) => self.toast(ctx, format!("Text export failed: {err}")),
        }
    }

    // -- Annotations -------------------------------------------------------

    /// Charge les annotations du document à la première utilisation.
    fn ensure_annots(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if !tab.annots_loaded {
                tab.annots = tab.doc.read_annotations();
                tab.annots_loaded = true;
            }
        }
    }

    fn undo(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if let Some(previous) = tab.undo.pop() {
            tab.redo.push(std::mem::replace(&mut tab.annots, previous));
            tab.dirty = true;
            self.toast(ctx, "Undone");
        }
    }

    fn redo(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if let Some(next) = tab.redo.pop() {
            tab.undo.push(std::mem::replace(&mut tab.annots, next));
            tab.dirty = true;
            self.toast(ctx, "Redone");
        }
    }

    /// Style courant pour un type d'annotation donné.
    fn annot_style(&self, kind: AnnotationKind) -> AnnotationStyle {
        let mut style = AnnotationStyle {
            color: self.annot_color,
            opacity: 1.0,
            line_width: self.annot_width,
            font_size: 12.0,
        };
        let _ = &mut style;
        style.opacity = match kind {
            AnnotationKind::Highlight | AnnotationKind::Squiggly | AnnotationKind::Underline
            | AnnotationKind::StrikeOut => self.annot_opacity,
            AnnotationKind::Ink | AnnotationKind::Square | AnnotationKind::Circle
            | AnnotationKind::Line | AnnotationKind::Arrow => 1.0,
            _ => 1.0,
        };
        if kind == AnnotationKind::Note {
            style.font_size = 10.0;
        }
        style
    }

    /// Ajoute une annotation et l'inscrit dans l'historique.
    fn push_annot(&mut self, annot: Annotation) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.undo.push(tab.annots.clone());
            if tab.undo.len() > 60 {
                tab.undo.remove(0);
            }
            tab.redo.clear();
            tab.annots.push(annot);
            tab.dirty = true;
        }
    }

    /// Retire l'annotation d'indice donné.
    fn remove_annot(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if index < tab.annots.len() {
                tab.undo.push(tab.annots.clone());
                tab.redo.clear();
                tab.annots.remove(index);
                tab.dirty = true;
            }
        }
        self.has_annot_selection = false;
    }

    /// Écrit les annotations dans le document avant enregistrement.
    fn flush_annots(&mut self) -> std::result::Result<(), String> {
        self.ensure_annots();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return Ok(());
        };
        if tab.annots.is_empty() {
            return Ok(());
        }
        tab.doc
            .apply_annotations(&tab.annots)
            .map_err(|err| err.to_string())
    }

    // -- Structure du document ---------------------------------------------

    /// Lit le plan, les formulaires et les pièces jointes à la demande.
    fn ensure_structure(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if !tab.structure_loaded {
                tab.outline = tab.doc.outline();
                tab.forms = tab.doc.read_form_fields();
                tab.attachments = tab.doc.attachments();
                tab.structure_loaded = true;
            }
        }
    }

    /// Panneau Signets : plan du document.
    fn bookmarks_panel(&mut self, ui: &mut egui::Ui) {
        self.ensure_structure();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.outline.is_empty() {
            panel_hint(ui, "No bookmarks", "The document has no outline");
            return;
        }
        let entries: Vec<(String, Option<usize>, usize)> = tab
            .outline
            .iter()
            .map(|item| (item.title.clone(), item.page, item.level))
            .collect();

        let mut jump = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (title, page, level) in &entries {
                    let indent = 12.0 + (*level as f32 * 12.0).min(48.0);
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(PANEL_W - 24.0, 26.0),
                        Sense::click(),
                    );
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + indent, rect.center().y),
                        Align2::LEFT_CENTER,
                        shorten(title, 30),
                        FontId::proportional(12.0),
                        ICON_BRIGHT,
                    );
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 10.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(10.5),
                            MUTED,
                        );
                    }
                    if resp.clicked() {
                        jump = *page;
                    }
                    ui.add_space(2.0);
                }
            });
        if let Some(page) = jump {
            self.jump = Some(page);
        }
    }

    /// Panneau Formulaires : lecture et remplissage des champs.
    fn forms_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_structure();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.forms.is_empty() {
            panel_hint(ui, "No form fields", "This PDF has no AcroForm fields");
            return;
        }
        let rows: Vec<(String, String, Option<usize>, bool, String)> = tab
            .forms
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    form_kind_label(&field.kind),
                    field.page,
                    field.read_only,
                    field.value.clone().unwrap_or_default(),
                )
            })
            .collect();

        let mut jump = None;
        let mut select = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (idx, (name, kind, page, read_only, value)) in rows.iter().enumerate() {
                    let (rect, resp) = ui
                        .allocate_exact_size(vec2(PANEL_W - 24.0, 40.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 12.0),
                        Align2::LEFT_CENTER,
                        shorten(name, 26),
                        FontId::proportional(12.0),
                        ICON_BRIGHT,
                    );
                    let detail = if value.is_empty() {
                        format!("{kind} · empty")
                    } else {
                        format!("{kind} · {}", shorten(value, 20))
                    };
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 28.0),
                        Align2::LEFT_CENTER,
                        if *read_only { format!("{detail} · read-only") } else { detail },
                        FontId::proportional(10.5),
                        MUTED,
                    );
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 10.0, rect.min.y + 12.0),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(10.0),
                            MUTED,
                        );
                    }
                    if resp.clicked() {
                        if *read_only {
                            jump = *page;
                        } else {
                            select = Some(idx);
                        }
                    }
                    ui.add_space(4.0);
                }
            });

        if let Some(idx) = select {
            let current = rows
                .get(idx)
                .map(|row| row.4.clone())
                .unwrap_or_default();
            self.form_edit = Some((idx, current));
            if let Some(page) = rows.get(idx).and_then(|row| row.2) {
                self.jump = Some(page);
            }
        } else if let Some(page) = jump {
            self.jump = Some(page);
        }

        // Éditeur du champ sélectionné.
        let Some((idx, mut buffer)) = self.form_edit.take() else {
            return;
        };
        let field = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.forms.get(idx))
            .cloned();
        let Some(field) = field else {
            return;
        };
        let mut apply = false;
        let mut close = false;
        Frame::none()
            .fill(WIDGET_BG)
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("{}  ({})", field.name, form_kind_label(&field.kind)))
                        .size(11.5)
                        .color(ICON_BRIGHT),
                );
                ui.add_space(4.0);
                match field.kind {
                    FormFieldKind::Checkbox | FormFieldKind::Radio => {
                        ui.horizontal(|ui| {
                            if ui.button("On").clicked() {
                                buffer = "On".to_owned();
                                apply = true;
                            }
                            if ui.button("Off").clicked() {
                                buffer = "Off".to_owned();
                                apply = true;
                            }
                        });
                    }
                    _ => {
                        ui.add_sized(
                            vec2(PANEL_W - 46.0, 24.0),
                            TextEdit::singleline(&mut buffer).hint_text("Value"),
                        );
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            if ui.button("Apply").clicked() {
                                apply = true;
                            }
                            if ui.button("Close").clicked() {
                                close = true;
                            }
                        });
                    }
                }
            });
        ui.add_space(6.0);

        if apply {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let Some(field) = tab.forms.get(idx).cloned() else {
                return;
            };
            let result = tab.doc.fill_form_field(&field, &buffer);
            match result {
                Ok(()) => {
                    if let Some(entry) = tab.forms.get_mut(idx) {
                        entry.value = Some(buffer.clone());
                    }
                    tab.dirty = true;
                    self.toast(ctx, format!("Field {} updated", field.name));
                }
                Err(err) => self.toast(ctx, format!("Field update failed: {err}")),
            }
        } else if !close {
            self.form_edit = Some((idx, buffer));
        }
    }

    /// Panneau Pièces jointes : liste et extraction.
    fn attachments_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_structure();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.attachments.is_empty() {
            panel_hint(ui, "No attachments", "Use Insert to attach a file");
            return;
        }
        let rows: Vec<(String, Option<usize>, Option<String>)> = tab
            .attachments
            .iter()
            .map(|item| (item.name.clone(), item.size, item.description.clone()))
            .collect();

        let mut save = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (idx, (name, size, description)) in rows.iter().enumerate() {
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(PANEL_W - 24.0, 44.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
                    }
                    let icon_rect = Rect::from_center_size(
                        pos2(rect.min.x + 16.0, rect.center().y),
                        vec2(15.0, 15.0),
                    );
                    icon_attach(ui.painter(), icon_rect, ICON_BRIGHT);
                    ui.painter().text(
                        pos2(rect.min.x + 30.0, rect.min.y + 13.0),
                        Align2::LEFT_CENTER,
                        shorten(name, 24),
                        FontId::proportional(12.0),
                        ICON_BRIGHT,
                    );
                    let mut detail = size
                        .map(format_size)
                        .unwrap_or_else(|| "size unknown".to_owned());
                    if let Some(description) = description {
                        detail = format!("{detail} · {}", shorten(description, 16));
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 30.0, rect.min.y + 30.0),
                        Align2::LEFT_CENTER,
                        detail,
                        FontId::proportional(10.5),
                        MUTED,
                    );
                    let save_rect = Rect::from_center_size(
                        pos2(rect.max.x - 18.0, rect.center().y),
                        vec2(22.0, 22.0),
                    );
                    let save_resp =
                        ui.interact(save_rect, Id::new(("attach-save", idx)), Sense::click());
                    icon_save(
                        ui.painter(),
                        save_rect.shrink(3.0),
                        if save_resp.hovered() { ICON_BRIGHT } else { ICON },
                    );
                    if save_resp.clicked() {
                        save = Some(idx);
                    }
                    ui.add_space(4.0);
                }
            });

        let Some(idx) = save else {
            return;
        };
        let Some((tab, name)) = self
            .tabs
            .get(self.active)
            .zip(rows.get(idx).map(|row| row.0.clone()))
        else {
            return;
        };
        let bytes = match tab.doc.attachment_bytes(idx) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.toast(ctx, format!("Attachment failed: {err}"));
                return;
            }
        };
        let Some(target) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
            return;
        };
        match std::fs::write(&target, bytes) {
            Ok(()) => self.toast(ctx, format!("Saved {}", target.display())),
            Err(err) => self.toast(ctx, format!("Write failed: {err}")),
        }
    }

    /// Panneau Aspect : fond du canevas, ombre, taille des vignettes.
    fn aspect_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Background").size(11.5).color(MUTED));
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            for (idx, color) in CANVAS_STYLES.iter().enumerate() {
                let selected = self.canvas_style == idx;
                let (rect, resp) =
                    ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                ui.painter().rect_filled(rect, Rounding::same(6.0), *color);
                if selected {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        7.0,
                        Stroke::new(1.5_f32, ACCENT),
                    );
                }
                if resp.clicked() {
                    self.canvas_style = idx;
                }
            }
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut shadow = self.page_shadow;
            if ui.checkbox(&mut shadow, "Page shadow").changed() {
                self.page_shadow = shadow;
            }
        });

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Thumbnails").size(11.5).color(MUTED));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.add_sized(
                vec2(PANEL_W - 40.0, 18.0),
                egui::Slider::new(&mut self.thumb_scale, 0.6..=1.6)
                    .show_value(false),
            );
        });

        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new("Notation")
                    .size(11.0)
                    .color(MUTED),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new("Colours come from the current tool presets.")
                    .size(10.5)
                    .color(MUTED),
            );
        });
    }

    // -- Outils d'annotation ------------------------------------------------

    /// Action du canevas selon le mode et l'outil choisi.
    fn canvas_action(&self) -> ToolAction {
        if self.mode == Some(3) {
            return if self.annot_tool == Some(1) {
                ToolAction::RedactRect
            } else {
                ToolAction::None
            };
        }
        if self.mode != Some(0) {
            return ToolAction::None;
        }
        match self.annot_tool {
            Some(0) => ToolAction::Select,
            Some(1) => ToolAction::DragRect(AnnotationKind::Highlight),
            Some(2) => ToolAction::Click(AnnotationKind::FreeText),
            Some(3) => ToolAction::Click(AnnotationKind::Note),
            Some(4) => ToolAction::Ink,
            Some(5) => ToolAction::DragRect(AnnotationKind::Square),
            Some(7) => ToolAction::Click(AnnotationKind::Link),
            Some(8) => ToolAction::Click(AnnotationKind::Stamp),
            Some(9) => ToolAction::Click(AnnotationKind::FreeText),
            Some(10) => ToolAction::Erase,
            _ => ToolAction::None,
        }
    }

    /// Type de brouillon pour l'outil courant (texte, note, tampon, lien).
    fn draft_kind(&self) -> DraftKind {
        match self.annot_tool {
            Some(2) => DraftKind::FreeText,
            Some(3) => DraftKind::Note,
            Some(7) => DraftKind::Link,
            Some(8) => DraftKind::Stamp,
            Some(9) => DraftKind::Signature,
            _ => DraftKind::FreeText,
        }
    }

    /// Ajoute une annotation de type rectangle (surlignage, formes).
    fn add_rect_annot(&mut self, page: usize, kind: AnnotationKind, rect: [f32; 4]) {
        if (rect[2] - rect[0]).abs() < 2.0 {
            return;
        }
        // Un glissé horizontal (surlignage) donne une bande de hauteur nulle :
        // on l'épaissit autour de la ligne visée.
        let mut rect = rect;
        if (rect[3] - rect[1]).abs() < 3.0 {
            let middle = (rect[1] + rect[3]) / 2.0;
            rect[1] = middle - 6.0;
            rect[3] = middle + 6.0;
        }
        let mut annot = Annotation::new(page, kind, rect);
        annot.style = self.annot_style(kind);
        if kind == AnnotationKind::Stamp {
            let label = STAMP_NAMES[self.stamp_choice.min(STAMP_NAMES.len() - 1)];
            annot.content = label.to_owned();
        }
        self.push_annot(annot);
    }

    /// Ajoute un tracé de crayon.
    fn add_ink_annot(&mut self, page: usize, points: Vec<[f32; 2]>) {
        if points.len() < 2 {
            return;
        }
        let mut annot = Annotation::new(page, AnnotationKind::Ink, [0.0, 0.0, 0.0, 0.0]);
        annot.style = self.annot_style(AnnotationKind::Ink);
        annot.add_ink_stroke(points);
        annot.recompute_ink_bbox();
        self.push_annot(annot);
    }

    /// Ouvre le brouillon de saisie pour un clic sur la page.
    fn open_draft(&mut self, page: usize, point: [f32; 2]) {
        let kind = self.draft_kind();
        let draft_kind = self.draft_kind();
        let (annot_kind, rect, buffer) = match draft_kind {
            DraftKind::Note => (
                AnnotationKind::Note,
                [point[0], point[1] - 20.0, point[0] + 20.0, point[1]],
                String::new(),
            ),
            DraftKind::Stamp => {
                let label = STAMP_NAMES[self.stamp_choice.min(STAMP_NAMES.len() - 1)];
                (
                    AnnotationKind::Stamp,
                    [point[0], point[1] - 20.0, point[0] + 130.0, point[1] + 20.0],
                    label.to_owned(),
                )
            }
            DraftKind::Link => (
                AnnotationKind::Link,
                [point[0], point[1] - 12.0, point[0] + 150.0, point[1] + 12.0],
                String::new(),
            ),
            DraftKind::Signature => (
                AnnotationKind::FreeText,
                [point[0], point[1] - 18.0, point[0] + 170.0, point[1] + 18.0],
                String::new(),
            ),
            DraftKind::FreeText => (
                AnnotationKind::FreeText,
                [point[0], point[1] - 14.0, point[0] + 180.0, point[1] + 22.0],
                String::new(),
            ),
        };
        let _ = kind;
        self.text_draft = Some(TextDraft {
            kind: draft_kind,
            page,
            rect,
            buffer,
            fresh: true,
        });
        self.draft_annot_kind = annot_kind;
        if draft_kind == DraftKind::Stamp {
            self.confirm_draft();
        }
    }

    /// Valide le brouillon courant.
    fn confirm_draft(&mut self) {
        let Some(draft) = self.text_draft.take() else {
            return;
        };
        let kind = self.draft_annot_kind;
        if draft.buffer.trim().is_empty() && draft.kind != DraftKind::Stamp {
            return;
        }
        let mut annot = Annotation::new(draft.page, kind, draft.rect);
        annot.content = draft.buffer.trim().to_owned();
        annot.style = self.annot_style(kind);
        if kind == AnnotationKind::FreeText {
            match draft.kind {
                DraftKind::Signature => {
                    annot.style.font_size = 22.0;
                    annot.style.color = [20, 20, 20];
                }
                _ => {
                    annot.style.font_size = 13.0;
                    annot.style.color = self.annot_color;
                }
            }
        }
        if kind == AnnotationKind::Link {
            annot.style.color = [40, 96, 200];
        }
        self.push_annot(annot);
    }

    /// Zone réagissant aux clics pour les annulations/sélections.
    fn hit_test(&self, page: usize, point: [f32; 2]) -> Option<usize> {
        let tab = self.tabs.get(self.active)?;
        tab.annots
            .iter()
            .enumerate()
            .rev()
            .find(|(_, a)| {
                a.page == page
                    && point[0] >= a.rect[0] - 4.0
                    && point[0] <= a.rect[2] + 4.0
                    && point[1] >= a.rect[1] - 4.0
                    && point[1] <= a.rect[3] + 4.0
            })
            .map(|(idx, _)| idx)
    }

    /// Fenêtre de saisie du brouillon (texte, note, lien, signature).
    fn draft_window(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.text_draft.as_mut() else {
            return;
        };
        let title = match draft.kind {
            DraftKind::Note => "Add note",
            DraftKind::FreeText => "Add text",
            DraftKind::Signature => "Add signature",
            DraftKind::Link => "Add link",
            DraftKind::Stamp => "Stamp text",
        };
        let hint = match draft.kind {
            DraftKind::Link => "https://…",
            DraftKind::Signature => "Type your name",
            _ => "Text…",
        };
        let fresh = draft.fresh;
        draft.fresh = false;
        let buffer = &mut draft.buffer;

        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let resp = ui.add_sized(
                    vec2(280.0, 26.0),
                    TextEdit::singleline(buffer).hint_text(hint),
                );
                if fresh {
                    resp.request_focus();
                }
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    confirm = true;
                }
                ui.horizontal(|ui| {
                    if ui.button("Add").clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            self.confirm_draft();
        } else if cancel {
            self.text_draft = None;
        }
    }

    /// Popup des préréglages (couleur, épaisseur, opacité).
    fn preset_popup(&mut self, ctx: &egui::Context, canvas_left: f32, top: f32) {
        if !self.preset_open || self.mode != Some(0) {
            return;
        }
        let tool = self.annot_tool.unwrap_or(0);
        let wants_text = matches!(self.draft_kind(), DraftKind::FreeText | DraftKind::Signature)
            && matches!(tool, 2 | 9);
        let wants_opacity = tool == 1;
        Area::new(Id::new("presets"))
            .anchor(Align2::CENTER_TOP, vec2(canvas_left / 2.0, top + 40.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(CHROME_BG)
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, SEPARATOR))
                    .inner_margin(Margin::symmetric(10.0, 8.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for color in PRESET_COLORS {
                                let selected = color == self.annot_color;
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(4.0),
                                    Color32::from_rgb(color[0], color[1], color[2]),
                                );
                                if selected {
                                    ui.painter().rect_stroke(
                                        rect.expand(2.0),
                                        5.0,
                                        Stroke::new(1.5_f32, Color32::WHITE),
                                    );
                                }
                                if resp.clicked() {
                                    self.annot_color = color;
                                }
                            }
                        });
                        if tool != 3 && tool != 8 {
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Width").size(11.0).color(MUTED));
                                ui.add(
                                    egui::Slider::new(&mut self.annot_width, 0.5..=8.0)
                                        .fixed_decimals(1),
                                );
                            });
                        }
                        if wants_opacity {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Opacity").size(11.0).color(MUTED));
                                ui.add(
                                    egui::Slider::new(&mut self.annot_opacity, 0.1..=1.0)
                                        .fixed_decimals(2),
                                );
                            });
                        }
                        if tool == 8 {
                            ui.horizontal(|ui| {
                                for (idx, name) in STAMP_NAMES.iter().take(3).enumerate() {
                                    if ui.selectable_label(self.stamp_choice == idx, *name).clicked()
                                    {
                                        self.stamp_choice = idx;
                                    }
                                }
                            });
                        }
                        if wants_text {
                            ui.label(
                                RichText::new("Color applies to new text")
                                    .size(10.0)
                                    .color(MUTED),
                            );
                        }
                        if ui.button("Done").clicked() {
                            self.preset_open = false;
                        }
                    });
            });
    }

    // -- Zoom --------------------------------------------------------------

    fn set_zoom(&mut self, zoom: Option<f32>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.zoom = zoom.map(|z| z.clamp(0.1, 8.0));
            if zoom.is_none() {
                tab.zoom_fit_page = false;
            }
        }
    }

    /// Applique un réglage de zoom nommé.
    fn set_zoom_mode(&mut self, mode: ZoomMode) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            match mode {
                ZoomMode::FitWidth => {
                    tab.zoom = None;
                    tab.zoom_fit_page = false;
                }
                ZoomMode::FitPage => {
                    tab.zoom = None;
                    tab.zoom_fit_page = true;
                }
                ZoomMode::Fixed(scale) => {
                    tab.zoom = Some(scale.clamp(0.1, 8.0));
                    tab.zoom_fit_page = false;
                }
            }
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
            self.search_active = if count > 0 { Some(0) } else { None };
            self.focus_search_result();
            self.toast(ctx, format!("{count} result(s)"));
        }
    }

    // -- Recherche active, remplacement, caviardage ------------------------

    /// Passe au résultat suivant (`delta` = ±1) et y saute.
    fn search_step(&mut self, delta: isize) {
        if self.results.is_empty() {
            return;
        }
        let count = self.results.len() as isize;
        let current = self.search_active.map(|idx| idx as isize).unwrap_or(-1);
        let next = (((current + delta) % count) + count) % count;
        self.search_active = Some(next as usize);
        self.focus_search_result();
    }

    /// Saute à la page du résultat actif et prépare son surlignage.
    fn focus_search_result(&mut self) {
        let Some(result) = self
            .search_active
            .and_then(|idx| self.results.get(idx))
            .cloned()
        else {
            return;
        };
        let page = result.page;
        self.jump = Some(page);
        self.search_rects.clear();
        self.search_rects_page = Some(page as u32);
        let needle = fold_case(self.search_query.trim());
        if needle.is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Ok(fragments) = tab.doc.extract_text_fragments(page as u32) else {
            return;
        };
        for fragment in fragments {
            self.search_rects
                .extend(match_rects(&fragment.text, fragment.rect, &needle));
        }
    }

    /// Remplace (ou supprime, `replacement` vide) toutes les correspondances.
    fn replace_matches(&mut self, ctx: &egui::Context, replacement: &str) -> usize {
        let query = self.search_query.trim().to_owned();
        if query.is_empty() {
            return 0;
        }
        let needle = fold_case(&query);
        let pages: std::collections::BTreeSet<usize> =
            self.results.iter().map(|result| result.page).collect();
        let mut changed = 0usize;
        {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return 0;
            };
            for page in pages {
                let Ok(fragments) = tab.doc.extract_text_fragments(page as u32) else {
                    continue;
                };
                for fragment in fragments {
                    if !fold_case(&fragment.text).contains(&needle) {
                        continue;
                    }
                    let new_text = replace_occurrences(&fragment.text, &needle, replacement);
                    if new_text == fragment.text {
                        continue;
                    }
                    if tab.doc.replace_text_fragment(&fragment, &new_text).is_ok() {
                        changed += 1;
                    }
                }
            }
            if changed > 0 {
                tab.dirty = true;
                tab.cache.clear();
                tab.annots_loaded = false;
            }
        }
        if changed > 0 {
            self.search_index = None;
            self.run_search(ctx);
        }
        changed
    }

    /// Marque toutes les correspondances pour caviardage et ouvre le mode.
    ///
    /// `confirm` ouvre directement la fenêtre d'application : c'est le
    /// comportement de « supprimer les correspondances », qui doit être
    /// destructif pour être honnête (masquer le texte ne le supprime pas).
    fn mark_matches_for_redaction(&mut self, ctx: &egui::Context, confirm: bool) {
        let needle = fold_case(self.search_query.trim());
        if needle.is_empty() {
            return;
        }
        let pages: std::collections::BTreeSet<usize> =
            self.results.iter().map(|result| result.page).collect();
        let mut added = 0usize;
        {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            for page in pages {
                let Ok(fragments) = tab.doc.extract_text_fragments(page as u32) else {
                    continue;
                };
                for fragment in fragments {
                    for rect in match_rects(&fragment.text, fragment.rect, &needle) {
                        // Ne pas empiler deux fois la même marque : le bouton
                        // corbeille et le bouton surlignage peuvent se suivre.
                        let region = RedactionRegion { page, rect };
                        if !self.redact_regions.contains(&region) {
                            self.redact_regions.push(region);
                            added += 1;
                        }
                    }
                }
            }
        }
        if added == 0 {
            self.toast(ctx, "Nothing to mark");
            return;
        }
        self.mode = Some(3);
        self.annot_tool = Some(1);
        self.panel = Some(Panel::Search);
        self.redact_modal = confirm;
        if confirm {
            self.toast(ctx, format!("{added} match(es) marked for deletion"));
        } else {
            self.toast(ctx, format!("{added} area(s) marked — press Apply"));
        }
    }

    /// Applique le caviardage marqué (destructif).
    fn apply_redactions(&mut self, ctx: &egui::Context) {
        let regions = std::mem::take(&mut self.redact_regions);
        self.redact_modal = false;
        if regions.is_empty() {
            self.toast(ctx, "No areas marked");
            return;
        }
        let sanitize = self.redact_sanitize;
        let outcome: Result<(usize, usize), String> = {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let renderer = &*self.renderer;
            match tab.doc.redact(&regions, renderer) {
                Ok(()) => {
                    let cleaned = if sanitize {
                        tab.doc.sanitize().unwrap_or(0)
                    } else {
                        0
                    };
                    tab.dirty = true;
                    tab.cache.clear();
                    tab.annots_loaded = false;
                    tab.structure_loaded = false;
                    // Le caviardage détruit des annotations : les instantanés
                    // d'annulation ne sont plus valables.
                    tab.undo.clear();
                    tab.redo.clear();
                    Ok((regions.len(), cleaned))
                }
                Err(err) => Err(err.to_string()),
            }
        };
        match outcome {
            Ok((areas, cleaned)) => {
                self.search_index = None;
                self.search_rects.clear();
                self.search_active = None;
                self.annot_tool = Some(0);
                // Les correspondances affichées n'existent plus.
                self.results.clear();
                let suffix = if sanitize {
                    format!(" · {cleaned} metadata entries removed")
                } else {
                    String::new()
                };
                self.toast(ctx, format!("Redaction applied ({areas} areas){suffix}"));
            }
            Err(err) => {
                self.redact_regions = regions;
                self.toast(ctx, format!("Redaction failed: {err}"));
            }
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
                        let label = if tab.dirty {
                            format!("{} •", shorten(&tab.title, 22))
                        } else {
                            shorten(&tab.title, 24)
                        };
                        let galley = ui.painter().layout_no_wrap(
                            label,
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

    // -- Panneau Commentaires ----------------------------------------------

    fn comments_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_annots();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.annots.is_empty() {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No annotations yet")
                        .size(12.0)
                        .color(MUTED),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Use Comment mode to add one")
                        .size(11.0)
                        .color(MUTED),
                );
            });
            return;
        }

        let entries: Vec<(usize, String, String, usize, String, [u8; 3])> = tab
            .annots
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    i,
                    annot_label(a.kind).to_owned(),
                    if a.content.is_empty() {
                        "(no text)".to_owned()
                    } else {
                        a.content.clone()
                    },
                    a.page,
                    a.created_at.clone(),
                    a.style.color,
                )
            })
            .collect();

        let mut jump = None;
        let mut remove = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (idx, kind, text, page, when, color) in &entries {
                    let (rect, resp) = ui
                        .allocate_exact_size(vec2(PANEL_W - 24.0, 44.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), WIDGET_BG);
                    }
                    let swatch = Rect::from_center_size(
                        pos2(rect.min.x + 16.0, rect.center().y),
                        vec2(10.0, 10.0),
                    );
                    ui.painter().rect_filled(
                        swatch,
                        Rounding::same(2.0),
                        Color32::from_rgb(color[0], color[1], color[2]),
                    );
                    ui.painter().text(
                        pos2(rect.min.x + 28.0, rect.min.y + 12.0),
                        Align2::LEFT_CENTER,
                        format!("{kind} · p.{page}"),
                        FontId::proportional(11.5),
                        ICON_BRIGHT,
                    );
                    ui.painter().text(
                        pos2(rect.min.x + 28.0, rect.min.y + 28.0),
                        Align2::LEFT_CENTER,
                        shorten(text, 26),
                        FontId::proportional(11.0),
                        MUTED,
                    );
                    ui.painter().text(
                        pos2(rect.max.x - 10.0, rect.min.y + 12.0),
                        Align2::RIGHT_CENTER,
                        when.get(..10).unwrap_or(""),
                        FontId::proportional(9.5),
                        MUTED,
                    );
                    let x_rect = Rect::from_center_size(
                        pos2(rect.max.x - 16.0, rect.max.y - 12.0),
                        vec2(16.0, 16.0),
                    );
                    let x_resp = ui.interact(x_rect, Id::new(("annot-x", *idx)), Sense::click());
                    icon_close(
                        ui.painter(),
                        x_rect,
                        if x_resp.hovered() { ICON_BRIGHT } else { MUTED },
                    );
                    if x_resp.clicked() {
                        remove = Some(*idx);
                    } else if resp.clicked() {
                        jump = Some(*page);
                    }
                    ui.add_space(4.0);
                }
            });

        if let Some(idx) = remove {
            self.remove_annot(idx);
            let _ = ctx;
        }
        if let Some(page) = jump {
            self.jump = Some(page);
        }
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
                    Panel::Comments => self.comments_panel(ctx, ui),
                    Panel::Bookmarks => self.bookmarks_panel(ui),
                    Panel::Forms => self.forms_panel(ctx, ui),
                    Panel::Attachments => self.attachments_panel(ctx, ui),
                    Panel::Aspect => self.aspect_panel(ui),
                    #[allow(unreachable_patterns)]
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
        let thumb_scale = self.thumb_scale;
        let mut jump = None;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let tile_width = ui.available_width().max(80.0);
                for page in 1..=total as u32 {
                    let (wp, hp) = tab.doc.page_size(page);
                    let scale = (THUMB_HEIGHT * thumb_scale) / hp.max(1.0);
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
                self.replace_open = !self.replace_open;
            }

            let delete_clicked = panel_action(ui, false, icon_trash).clicked();
            let redact_clicked = panel_action(ui, false, icon_highlighter).clicked();
            if delete_clicked {
                self.mark_matches_for_redaction(ctx, true);
            }
            if redact_clicked {
                self.mark_matches_for_redaction(ctx, false);
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
                if self.results.is_empty() {
                    self.run_search(ctx);
                } else {
                    self.search_step(1);
                }
            }
        });

        if self.replace_open {
            ui.add_space(6.0);
            let mut replace_now = false;
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let resp = ui.add_sized(
                    vec2(PANEL_W - 26.0, 28.0),
                    TextEdit::singleline(&mut self.replace_text).hint_text("Replace with"),
                );
                if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    replace_now = true;
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                if ui.button("Replace all").clicked() {
                    replace_now = true;
                }
            });
            if replace_now {
                let replacement = self.replace_text.clone();
                let changed = self.replace_matches(ctx, &replacement);
                if changed > 0 {
                    self.toast(ctx, format!("{changed} replacement(s)"));
                } else {
                    self.toast(ctx, "Nothing replaced");
                }
            }
        }
        ui.add_space(12.0);

        let mut jump = None;
        let mut forget = None;
        let mut clear_history = false;
        let mut pending_step: Option<isize> = None;
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
                    let active = self.search_active;
                    let total = self.results.len();
                    let mut step: Option<isize> = None;
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        let shown = active.map(|idx| idx + 1).unwrap_or(0);
                        ui.label(
                            RichText::new(format!("Results · {shown}/{total}"))
                                .size(12.0)
                                .strong(),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.add_space(10.0);
                            if panel_action(ui, false, icon_chevron).clicked() {
                                step = Some(1);
                            }
                            {
                                // Flèche vers le haut : résultat précédent.
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(30.0, 28.0), Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::same(6.0),
                                        Color32::from_rgb(0x33, 0x33, 0x33),
                                    );
                                }
                                icon_caret_up(
                                    ui.painter(),
                                    rect.shrink(8.0),
                                    if resp.hovered() { ICON_BRIGHT } else { ICON },
                                );
                                if resp.clicked() {
                                    step = Some(-1);
                                }
                            }
                        });
                    });
                    if let Some(delta) = step {
                        pending_step = Some(delta);
                    }
                    ui.add_space(6.0);
                    for (idx, result) in self.results.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(PANEL_W - 24.0, 28.0), Sense::click());
                        if resp.hovered() || active == Some(idx) {
                            ui.painter().rect_filled(
                                rect,
                                Rounding::same(6.0),
                                if active == Some(idx) {
                                    HOVER_BG
                                } else {
                                    WIDGET_BG
                                },
                            );
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
        if let Some(delta) = pending_step {
            self.search_step(delta);
        }
        if let Some(idx) = jump {
            if self.results.is_empty() {
                if let Some(query) = self.history.get(idx).cloned() {
                    self.search_query = query;
                    self.run_search(ctx);
                }
            } else if self.results.get(idx).is_some() {
                self.search_active = Some(idx);
                self.focus_search_result();
            }
        }
    }

    /// Fenêtre de confirmation du caviardage.
    fn redact_window(&mut self, ctx: &egui::Context) {
        if !self.redact_modal {
            return;
        }
        let areas = self.redact_regions.len();
        let mut confirm = false;
        let mut cancel = false;
        egui::Window::new("Apply redaction")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.label(
                    RichText::new(format!("{areas} area(s) will be blacked out permanently."))
                        .size(12.5)
                        .color(ICON_BRIGHT),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Redaction replaces each marked page with an image: the text                          underneath is destroyed, not hidden, and cannot be recovered.                          Annotations overlapping a marked area are deleted.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(8.0);
                ui.checkbox(
                    &mut self.redact_sanitize,
                    "Also remove metadata, XMP, JavaScript and attachments",
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Apply").color(Color32::WHITE))
                                .fill(REDACT_RED),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                });
            });
        if cancel {
            self.redact_modal = false;
        }
        if confirm {
            self.apply_redactions(ctx);
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
            let mut chosen_tool: Option<(usize, bool)> = None;
            let annot_tool = self.annot_tool;
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
                if want_preset {
                    self.preset_open = true;
                } else if !mode_tools(mode).get(idx).map(|t| t.2).unwrap_or(false) {
                    self.preset_open = false;
                }
                match mode {
                    0 => {}
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
                                    }
                                    removed
                                })
                                .unwrap_or(0);
                            self.toast(ctx, format!("{cleaned} metadata entries removed"));
                        }
                        4 => {
                            if self.redact_regions.is_empty() {
                                self.toast(ctx, "Mark areas first");
                            } else {
                                self.redact_modal = true;
                            }
                        }
                        _ => {}
                    },
                    _ => self.toast(
                        ctx,
                        format!(
                            "{} — planned",
                            mode_tools(mode).get(idx).map(|t| t.0).unwrap_or("Tool")
                        ),
                    ),
                }
            }
            if exit {
                self.mode = None;
                self.annot_tool = None;
                self.preset_open = false;
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
                                        self.annot_tool = None;
                                        self.preset_open = false;
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
                self.annot_tool = None;
                self.preset_open = false;
            } else if close {
                self.tools_open = false;
            }
        }

        self.preset_popup(ctx, canvas_left, top);

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
                                    MUTED
                                } else if resp.hovered() {
                                    ICON_BRIGHT
                                } else {
                                    ICON
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
                                    MUTED
                                } else if resp.hovered() {
                                    ICON_BRIGHT
                                } else {
                                    ICON
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
                                let color = if resp.hovered() { ICON_BRIGHT } else { MUTED };
                                icon_ellipsis(ui.painter(), rect.shrink(6.0), color);
                                resp
                            };
                            if more.clicked() {
                                self.actions_open = !self.actions_open;
                            }
                        });
                    });
            });

        // Menu `…` : enregistrer, exporter, propriétés.
        if self.actions_open {
            let mut action = None;
            Area::new(Id::new("actions-menu"))
                .anchor(Align2::RIGHT_TOP, vec2(-14.0, top + 38.0))
                .order(Order::Foreground)
                .show(ctx, |ui| {
                    Frame::none()
                        .fill(CHROME_BG)
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, SEPARATOR))
                        .inner_margin(Margin::symmetric(4.0, 4.0))
                        .show(ui, |ui| {
                            if menu_row(ui, "Save", "⌘S") {
                                action = Some(0);
                            }
                            if menu_row(ui, "Save as…", "⇧⌘S") {
                                action = Some(1);
                            }
                            if menu_row(ui, "Export text…", "") {
                                action = Some(2);
                            }
                            ui.add_space(4.0);
                            let sep = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, SEPARATOR),
                            );
                            ui.add_space(4.0);
                            if menu_row(ui, "Properties…", "⌘D") {
                                action = Some(3);
                            }
                            if menu_row(ui, "Close tab", "⌘W") {
                                action = Some(4);
                            }
                        });
                });
            if let Some(action) = action {
                self.actions_open = false;
                match action {
                    0 => self.save_active(ctx),
                    1 => self.save_as(ctx),
                    2 => self.export_text(ctx),
                    3 => self.props_open = true,
                    4 => {
                        let idx = self.active;
                        self.close_tab(idx);
                    }
                    _ => {}
                }
            }
        }
    }

    // -- Propriétés --------------------------------------------------------

    /// Fenêtre « Propriétés du document » (lecture seule).
    fn properties_window(&mut self, ctx: &egui::Context) {
        if !self.props_open {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            self.props_open = false;
            return;
        };
        let meta = tab.doc.metadata().clone();
        let total = tab.total();
        let path = tab
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "(not saved yet)".to_owned());
        let size = tab
            .path
            .as_ref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| format!("{:.1} KB", m.len() as f64 / 1024.0))
            .unwrap_or_else(|| "—".to_owned());
        let mut open = true;

        egui::Window::new("Document properties")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let mut row = |label: &str, value: &str| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(label).size(12.0).color(MUTED));
                        ui.label(RichText::new(value).size(12.0).color(ICON_BRIGHT));
                    });
                };
                row("File", &tab.title);
                row("Path", &path);
                row("Size", &size);
                row("Pages", &total.to_string());
                row("Title", meta.title.as_deref().unwrap_or("—"));
                row("Author", meta.author.as_deref().unwrap_or("—"));
                row("Subject", meta.subject.as_deref().unwrap_or("—"));
                row("Keywords", meta.keywords.as_deref().unwrap_or("—"));
                row("Producer", meta.producer.as_deref().unwrap_or("—"));
            });

        if !open {
            self.props_open = false;
        }
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
        let action = if self.mode == Some(0) {
            self.canvas_action()
        } else {
            ToolAction::None
        };
        let status = &mut self.render_status;
        let last_fit = &mut self.last_fit;
        let tab = &mut self.tabs[active];
        let total = tab.total();
        let sense = if action == ToolAction::None {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let annot_width = self.annot_width;
        let ink_current = &mut self.ink_current;
        let drag_from = &mut self.drag_from;
        let drag_to = &mut self.drag_to;
        let mut pending_rect: Option<(usize, AnnotationKind, [f32; 4])> = None;
        let mut pending_ink: Option<(usize, Vec<[f32; 2]>)> = None;
        let mut pending_click: Option<(usize, [f32; 2])> = None;
        let mut pending_hit: Option<(usize, [f32; 2])> = None;
        let mut pending_redact: Option<(usize, [f32; 4])> = None;
        let redact_marks = &self.redact_regions;
        let search_rects = &self.search_rects;
        let search_rects_page = self.search_rects_page;
        let selected_annot = self.selected_annot;
        let has_selection = self.has_annot_selection;
        let show_shadow = self.page_shadow;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, _viewport| {
                let avail = ui.available_width().max(50.0);
                let mut first_visible: Option<usize> = None;
                let mut clicked_page: Option<usize> = None;

                // Hauteur utile : la fenêtre moins deux marges de page.
                let clip = ui.clip_rect();
                let view_h = (clip.height() - PAGE_SPACING * 2.0).max(50.0);
                for page in 1..=total as u32 {
                    let (wp, hp) = tab.doc.page_size(page);
                    let scale = tab
                        .zoom
                        .unwrap_or_else(|| {
                            let fit_width = avail / wp.max(1.0);
                            if tab.zoom_fit_page {
                                fit_width.min(view_h / hp.max(1.0))
                            } else {
                                fit_width
                            }
                        })
                        .clamp(0.05, 8.0);
                    if tab.zoom.is_none() {
                        *last_fit = scale;
                    }
                    let size = vec2(wp * scale, hp * scale);
                    // La ligne occupe toute la largeur disponible : la page est
                    // centrée dedans, comme dans la référence. Sans cela, une
                    // page plus étroite que le canevas (zoom < ajustement)
                    // resterait collée à gauche.
                    let (rect, row) = ui.allocate_exact_size(
                        vec2(avail, size.y + PAGE_SPACING),
                        Sense::hover(),
                    );
                    let page_rect = Rect::from_min_size(
                        pos2(rect.min.x + ((avail - size.x) * 0.5).max(0.0), rect.min.y),
                        size,
                    );
                    let resp = ui.interact(page_rect, row.id.with(page), sense);

                    if jump == Some(page as usize) {
                        ui.scroll_to_rect(Rect::from_min_size(page_rect.min, size), Some(Align::Min));
                        jump = None;
                    }

                    // `viewport` est exprimé dans le repère du *contenu*
                    // (min = défilement courant) alors que `rect` est en
                    // coordonnées écran : les comparer directement saute
                    // des pages dès que l'on fait défiler. Le rectangle de
                    // découpe, lui, est bien en coordonnées écran.
                    if clip.intersects(rect) {
                        if first_visible.is_none() {
                            first_visible = Some(page as usize);
                        }

                        // Ombre douce sous la page.
                        if show_shadow {
                            ui.painter().rect_filled(
                                page_rect.translate(vec2(0.0, 2.0)).expand(1.0),
                                Rounding::same(3.0),
                                Color32::from_black_alpha(70),
                            );
                        }

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

                        // Régions marquées pour caviardage et résultats
                        // de recherche actifs.
                        for region in redact_marks.iter().filter(|r| r.page == page as usize) {
                            let rect = Rect::from_two_pos(
                                pos2(
                                    page_rect.min.x + region.rect[0] * scale,
                                    page_rect.max.y - region.rect[3] * scale,
                                ),
                                pos2(
                                    page_rect.min.x + region.rect[2] * scale,
                                    page_rect.max.y - region.rect[1] * scale,
                                ),
                            );
                            ui.painter().rect_filled(
                                rect,
                                Rounding::ZERO,
                                Color32::from_black_alpha(190),
                            );
                            ui.painter().rect_stroke(
                                rect,
                                Rounding::ZERO,
                                Stroke::new(1.5_f32, REDACT_RED),
                            );
                        }
                        if search_rects_page == Some(page) {
                            for rect in search_rects {
                                let rect = Rect::from_two_pos(
                                    pos2(
                                        page_rect.min.x + rect[0] * scale,
                                        page_rect.max.y - rect[3] * scale,
                                    ),
                                    pos2(
                                        page_rect.min.x + rect[2] * scale,
                                        page_rect.max.y - rect[1] * scale,
                                    ),
                                );
                                ui.painter().rect_filled(
                                    rect.expand(1.0),
                                    Rounding::same(2.0),
                                    SEARCH_MATCH,
                                );
                            }
                        }

                        // Annotations posées sur la page.
                        let selected = if has_selection {
                            Some(selected_annot)
                        } else {
                            None
                        };
                        draw_annots(ui.painter(), tab, page, page_rect, scale, selected);
                    }

                    // Interaction des outils d'annotation.
                    if action != ToolAction::None {
                        let to_pdf = |pos: egui::Pos2| -> [f32; 2] {
                            [
                                (pos.x - page_rect.min.x) / scale,
                                (page_rect.max.y - pos.y) / scale,
                            ]
                        };
                        let to_screen = |p: [f32; 2]| -> egui::Pos2 {
                            pos2(
                                page_rect.min.x + p[0] * scale,
                                page_rect.max.y - p[1] * scale,
                            )
                        };
                        match action {
                            ToolAction::DragRect(kind) => {
                                if resp.dragged() {
                                    let point = resp.interact_pointer_pos().map(to_pdf);
                                    if drag_from.is_none() {
                                        *drag_from = point;
                                    }
                                    if point.is_some() {
                                        *drag_to = point;
                                    }
                                }
                                if resp.drag_stopped() {
                                    let from = drag_from.take();
                                    let to = drag_to.take();
                                    if let (Some(a), Some(b)) = (from, to) {
                                        pending_rect = Some((
                                            page as usize,
                                            kind,
                                            [
                                                a[0].min(b[0]),
                                                a[1].min(b[1]),
                                                a[0].max(b[0]),
                                                a[1].max(b[1]),
                                            ],
                                        ));
                                    }
                                }
                                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                    let r = Rect::from_two_pos(to_screen(a), to_screen(b));
                                    ui.painter().rect_stroke(
                                        r,
                                        2.0,
                                        Stroke::new(1.5_f32, ACCENT),
                                    );
                                }
                            }
                            ToolAction::Ink => {
                                if resp.dragged() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        ink_current.push(to_pdf(p));
                                    }
                                }
                                if resp.drag_stopped() {
                                    pending_ink =
                                        Some((page as usize, std::mem::take(ink_current)));
                                }
                                if ink_current.len() > 1 {
                                    let pts: Vec<egui::Pos2> =
                                        ink_current.iter().map(|p| to_screen(*p)).collect();
                                    ui.painter().add(Shape::line(
                                        pts,
                                        Stroke::new(annot_width * scale.max(0.4), ACCENT),
                                    ));
                                }
                            }
                            ToolAction::Click(_) => {
                                if resp.clicked() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        pending_click = Some((page as usize, to_pdf(p)));
                                    }
                                }
                            }
                            ToolAction::RedactRect => {
                                if resp.dragged() {
                                    let point = resp.interact_pointer_pos().map(to_pdf);
                                    if drag_from.is_none() {
                                        *drag_from = point;
                                    }
                                    if point.is_some() {
                                        *drag_to = point;
                                    }
                                }
                                if resp.drag_stopped() {
                                    let from = drag_from.take();
                                    let to = drag_to.take();
                                    if let (Some(a), Some(b)) = (from, to) {
                                        pending_redact = Some((
                                            page as usize,
                                            [
                                                a[0].min(b[0]),
                                                a[1].min(b[1]),
                                                a[0].max(b[0]),
                                                a[1].max(b[1]),
                                            ],
                                        ));
                                    }
                                }
                                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                    let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                    ui.painter().rect_filled(
                                        rect,
                                        Rounding::ZERO,
                                        Color32::from_black_alpha(140),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        Rounding::ZERO,
                                        Stroke::new(1.5_f32, REDACT_RED),
                                    );
                                }
                            }
                            ToolAction::Erase | ToolAction::Select => {
                                if resp.clicked() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        pending_hit = Some((page as usize, to_pdf(p)));
                                    }
                                }
                            }
                            ToolAction::None => {}
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

        if let Some((page, kind, rect)) = pending_rect {
            self.add_rect_annot(page, kind, rect);
        }
        if let Some((page, points)) = pending_ink {
            tracing::info!("ink annot page {page} points {}", points.len());
            self.add_ink_annot(page, points);
        }
        if let Some((page, point)) = pending_click {
            self.open_draft(page, point);
        }
        if let Some((page, rect)) = pending_redact {
            if (rect[2] - rect[0]).abs() > 2.0 && (rect[3] - rect[1]).abs() > 2.0 {
                self.redact_regions.push(RedactionRegion { page, rect });
            }
        }
        if let Some((page, point)) = pending_hit {
            match action {
                ToolAction::Erase => {
                    if let Some(idx) = self.hit_test(page, point) {
                        self.remove_annot(idx);
                    }
                }
                ToolAction::Select => {
                    if let Some(idx) = self.hit_test(page, point) {
                        self.selected_annot = idx;
                        self.has_annot_selection = true;
                    } else {
                        self.has_annot_selection = false;
                    }
                }
                _ => {}
            }
        }
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
        let current_zoom = tab.zoom;
        let tab_zoom_fit_page = tab.zoom_fit_page;
        let mut delta = 0.0_f32;
        let mut goto: Option<usize> = None;
        let mut zoom_choice: Option<ZoomMode> = None;
        let mut page_text = std::mem::take(&mut self.page_entry);

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

                            // Numéro de page : clic = saut direct.
                            let (rect, page_resp) =
                                ui.allocate_exact_size(vec2(46.0, 20.0), Sense::click());
                            if page_resp.hovered() {
                                ui.painter().rect_filled(rect, Rounding::same(5.0), HOVER_BG);
                            }
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                format!("{page} / {total}"),
                                FontId::proportional(11.0),
                                if page_resp.hovered() { ICON_BRIGHT } else { MUTED },
                            );
                            let just_opened = page_resp.clicked();
                            if just_opened {
                                page_text.clear();
                                ui.memory_mut(|m| m.toggle_popup(Id::new("page_jump")));
                            }
                            egui::popup_above_or_below_widget(
                                ui,
                                Id::new("page_jump"),
                                &page_resp,
                                egui::AboveOrBelow::Above,
                                egui::PopupCloseBehavior::CloseOnClickOutside,
                                |ui| {
                                    ui.set_min_width(130.0);
                                    ui.label(
                                        RichText::new("Go to page").size(11.0).color(MUTED),
                                    );
                                    ui.add_space(4.0);
                                    let field = ui.add(
                                        TextEdit::singleline(&mut page_text)
                                            .desired_width(90.0)
                                            .hint_text("1"),
                                    );
                                    // Le champ prend le focus à l'ouverture
                                    // seulement : le redemander à chaque
                                    // trame empêcherait `lost_focus` de
                                    // signaler la validation par Entrée.
                                    if just_opened {
                                        field.request_focus();
                                    }
                                    if field.lost_focus()
                                        && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    {
                                        goto = page_text.trim().parse::<usize>().ok();
                                    }
                                },
                            );

                            ui.add_space(6.0);
                            // Étiquette de zoom : clic = réglages prédéfinis.
                            let text = if tab_zoom_fit_page {
                                format!("{zoom_label} · page")
                            } else {
                                zoom_label.clone()
                            };
                            let (rect, zoom_resp) = ui.allocate_exact_size(
                                vec2(
                                    ui.painter()
                                        .layout_no_wrap(
                                            text.clone(),
                                            FontId::proportional(11.0),
                                            MUTED,
                                        )
                                        .size()
                                        .x
                                        + 12.0,
                                    20.0,
                                ),
                                Sense::click(),
                            );
                            if zoom_resp.hovered() {
                                ui.painter().rect_filled(rect, Rounding::same(5.0), HOVER_BG);
                            }
                            ui.painter().text(
                                rect.center(),
                                Align2::CENTER_CENTER,
                                &text,
                                FontId::proportional(11.0),
                                if zoom_resp.hovered() { ICON_BRIGHT } else { MUTED },
                            );
                            if zoom_resp.clicked() {
                                ui.memory_mut(|m| m.toggle_popup(Id::new("zoom_menu")));
                            }
                            egui::popup_above_or_below_widget(
                                ui,
                                Id::new("zoom_menu"),
                                &zoom_resp,
                                egui::AboveOrBelow::Above,
                                egui::PopupCloseBehavior::CloseOnClick,
                                |ui| {
                                    ui.set_min_width(150.0);
                                    for (label, mode) in ZoomMode::CHOICES {
                                        let selected = zoom_choice_matches(*mode, current_zoom, tab_zoom_fit_page);
                                        if menu_row(ui, label, if selected { "✓" } else { "" }) {
                                            zoom_choice = Some(*mode);
                                        }
                                    }
                                },
                            );

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

        self.page_entry = page_text;
        if let Some(choice) = zoom_choice {
            self.set_zoom_mode(choice);
        }
        if let Some(target) = goto {
            // La fenêtre de saisie a rempli son rôle : on la referme.
            ctx.memory_mut(|m| m.toggle_popup(Id::new("page_jump")));
            if (1..=total).contains(&target) {
                self.jump = Some(target);
            } else {
                self.toast(ctx, format!("Page out of range (1–{total})"));
            }
        }
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
                        ui.add(
                            egui::Label::new(RichText::new(&text).size(11.5).color(ICON_BRIGHT))
                                .extend(),
                        );
                    });
            });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let delete_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)
        });
        if delete_pressed && self.has_annot_selection {
            let idx = self.selected_annot;
            self.remove_annot(idx);
            self.toast(ctx, "Annotation removed");
            return;
        }
        let (cmd, shift, open, close, plus, minus, zero, save, find) = ctx.input(|i| {
            (
                i.modifiers.command,
                i.modifiers.shift,
                i.key_pressed(egui::Key::O),
                i.key_pressed(egui::Key::W),
                i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals),
                i.key_pressed(egui::Key::Minus),
                i.key_pressed(egui::Key::Num0),
                i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::G),
            )
        });
        if cmd && find {
            self.search_step(if shift { -1 } else { 1 });
            return;
        }
        if !cmd {
            return;
        }
        if save {
            if shift {
                self.save_as(ctx);
            } else {
                self.save_active(ctx);
            }
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
        self.ensure_annots();
        self.shortcuts(ctx);
        self.title_bar(ctx);
        self.rail(ctx);
        self.side_panel(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(CANVAS_STYLES[self.canvas_style.min(CANVAS_STYLES.len() - 1)]))
            .show(ctx, |ui| {
                self.canvas(ui);
            });
        self.toolbar(ctx);
        self.zoom_overlay(ctx);
        self.properties_window(ctx);
        self.draft_window(ctx);
        self.redact_window(ctx);
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
    selected: bool,
) -> egui::Response {
    let width = if chevron { 38.0 } else { 30.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    if primary || selected {
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
fn icon_caret_up(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::CARET_UP);
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

/// Sous-rectangles d'une ligne couvrant chaque occurrence de `needle`.
///
/// Le texte d'un fragment est une ligne : la position de la correspondance
/// dans la chaîne donne sa place proportionnelle dans la largeur du
/// rectangle. C'est une approximation (police proportionnelle), suffisante
/// pour surligner ou marquer une correspondance.
///
/// `needle` doit déjà être plié par [`fold_case`], comme le texte comparé.
fn match_rects(text: &str, rect: [f32; 4], needle: &str) -> Vec<[f32; 4]> {
    if needle.is_empty() {
        return Vec::new();
    }
    let folded = fold_case(text);
    let total = folded.len().max(1) as f32;
    let width = rect[2] - rect[0];
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = folded[cursor..].find(needle) {
        let start = cursor + offset;
        let end = start + needle.len();
        let before = start as f32 / total;
        let hit = (needle.len() as f32 / total).min(1.0 - before);
        let left = rect[0] + width * before;
        let right = (rect[0] + width * (before + hit)).max(left + 2.0);
        out.push([left, rect[1], right, rect[3]]);
        cursor = end;
    }
    out
}

/// Remplace toutes les occurrences de `needle` (déjà plié) dans `text`.
///
/// `fold_case` préserve la longueur en octets : les positions trouvées dans la
/// version pliée sont donc directement valables dans l'original.
fn replace_occurrences(text: &str, needle: &str, replacement: &str) -> String {
    if needle.is_empty() {
        return text.to_owned();
    }
    let folded = fold_case(text);
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    while let Some(offset) = folded[cursor..].find(needle) {
        let start = cursor + offset;
        let end = start + needle.len();
        out.push_str(&text[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Réglages de zoom proposés par la pastille bas-droite.
#[derive(Clone, Copy, PartialEq)]
enum ZoomMode {
    /// Ajuster à la largeur du canevas.
    FitWidth,
    /// Garder la page entière visible.
    FitPage,
    /// Échelle fixe (1.0 = 100 %).
    Fixed(f32),
}

impl ZoomMode {
    /// Menu de la pastille, dans l'ordre d'affichage.
    const CHOICES: &'static [(&'static str, ZoomMode)] = &[
        ("Fit width", ZoomMode::FitWidth),
        ("Fit page", ZoomMode::FitPage),
        ("Actual size", ZoomMode::Fixed(1.0)),
        ("50 %", ZoomMode::Fixed(0.5)),
        ("75 %", ZoomMode::Fixed(0.75)),
        ("125 %", ZoomMode::Fixed(1.25)),
        ("150 %", ZoomMode::Fixed(1.5)),
        ("200 %", ZoomMode::Fixed(2.0)),
        ("400 %", ZoomMode::Fixed(4.0)),
    ];
}

/// Vrai si le réglage `mode` correspond au zoom courant de l'onglet.
fn zoom_choice_matches(mode: ZoomMode, zoom: Option<f32>, fit_page: bool) -> bool {
    match mode {
        ZoomMode::FitWidth => zoom.is_none() && !fit_page,
        ZoomMode::FitPage => zoom.is_none() && fit_page,
        ZoomMode::Fixed(scale) => zoom.is_some_and(|z| (z - scale).abs() < 0.001),
    }
}

/// Taille lisible : octets sous 1 Ko, puis Ko et Mo.
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Message centré d'un panneau vide.
fn panel_hint(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        ui.label(RichText::new(title).size(12.0).color(MUTED));
        ui.add_space(4.0);
        ui.label(RichText::new(detail).size(11.0).color(MUTED));
    });
}

/// Libellé anglais d'un type de champ de formulaire.
fn form_kind_label(kind: &FormFieldKind) -> String {
    match kind {
        FormFieldKind::Text => "Text field".to_owned(),
        FormFieldKind::Checkbox => "Checkbox".to_owned(),
        FormFieldKind::Radio => "Radio".to_owned(),
        FormFieldKind::Choice => "Dropdown".to_owned(),
        FormFieldKind::MultiChoice => "List".to_owned(),
        FormFieldKind::Signature => "Signature".to_owned(),
        FormFieldKind::Other(name) => format!("Field ({name})"),
    }
}

fn icon_save(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::FLOPPY_DISK);
}

/// Libellé anglais d'un type d'annotation (le cœur est en français).
fn annot_label(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Highlight => "Highlight",
        AnnotationKind::Underline => "Underline",
        AnnotationKind::StrikeOut => "Strikeout",
        AnnotationKind::Squiggly => "Squiggly",
        AnnotationKind::Note => "Note",
        AnnotationKind::FreeText => "Text",
        AnnotationKind::Square => "Rectangle",
        AnnotationKind::Circle => "Circle",
        AnnotationKind::Line => "Line",
        AnnotationKind::Arrow => "Arrow",
        AnnotationKind::Ink => "Ink",
        AnnotationKind::Stamp => "Stamp",
        AnnotationKind::Link => "Link",
        AnnotationKind::FileAttachment => "Attachment",
    }
}

/// Dessine les annotations d'une page ; coordonnées PDF → écran.
fn draw_annots(
    painter: &Painter,
    tab: &Tab,
    page: u32,
    page_rect: Rect,
    scale: f32,
    selected: Option<usize>,
) {
    for (idx, annot) in tab.annots.iter().enumerate() {
        if annot.page != page as usize {
            continue;
        }
        let to_screen =
            |x: f32, y: f32| pos2(page_rect.min.x + x * scale, page_rect.max.y - y * scale);
        let r = Rect::from_two_pos(
            to_screen(annot.rect[0], annot.rect[1]),
            to_screen(annot.rect[2], annot.rect[3]),
        );
        let color = Color32::from_rgb(
            annot.style.color[0],
            annot.style.color[1],
            annot.style.color[2],
        );
        let alpha = (annot.style.opacity.clamp(0.05, 1.0) * 255.0) as u8;
        let fill = Color32::from_rgba_unmultiplied(
            annot.style.color[0],
            annot.style.color[1],
            annot.style.color[2],
            alpha,
        );
        let width = (annot.style.line_width * scale.max(0.3)).max(1.0);
        let font = (annot.style.font_size * scale).clamp(6.0, 48.0);

        match annot.kind {
            AnnotationKind::Highlight | AnnotationKind::Squiggly => {
                painter.rect_filled(r, Rounding::same(2.0), fill);
            }
            AnnotationKind::Underline => {
                painter.line_segment(
                    [pos2(r.min.x, r.max.y), pos2(r.max.x, r.max.y)],
                    Stroke::new(width, color),
                );
            }
            AnnotationKind::StrikeOut => {
                painter.line_segment(
                    [pos2(r.min.x, r.center().y), pos2(r.max.x, r.center().y)],
                    Stroke::new(width, color),
                );
            }
            AnnotationKind::Ink => {
                for stroke in &annot.ink_points {
                    if stroke.len() < 2 {
                        continue;
                    }
                    let pts: Vec<egui::Pos2> =
                        stroke.iter().map(|p| to_screen(p[0], p[1])).collect();
                    painter.add(Shape::line(pts, Stroke::new(width, color)));
                }
            }
            AnnotationKind::Note => {
                painter.rect_filled(r, Rounding::same(3.0), fill);
                painter.rect_stroke(
                    r,
                    3.0,
                    Stroke::new(1.0_f32, Color32::from_black_alpha(120)),
                );
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    "!",
                    FontId::proportional((font * 0.9).max(8.0)),
                    Color32::from_black_alpha(200),
                );
            }
            AnnotationKind::Stamp => {
                painter.rect_stroke(r, 3.0, Stroke::new((2.0 * scale).max(1.0), color));
                painter.text(
                    r.center(),
                    Align2::CENTER_CENTER,
                    &annot.content,
                    FontId::proportional(font.max(9.0)),
                    color,
                );
            }
            AnnotationKind::Link => {
                painter.line_segment(
                    [pos2(r.min.x, r.max.y), pos2(r.max.x, r.max.y)],
                    Stroke::new(1.0_f32, color),
                );
            }
            _ => {
                painter.rect_stroke(r, 1.0, Stroke::new(width, color));
                if !annot.content.is_empty() {
                    painter.text(
                        r.left_top() + vec2(3.0, 2.0),
                        Align2::LEFT_TOP,
                        &annot.content,
                        FontId::proportional(font.max(8.0)),
                        color,
                    );
                }
            }
        }

        if selected == Some(idx) {
            painter.rect_stroke(r.expand(3.0), 2.0, Stroke::new(1.5_f32, ACCENT));
        }
    }
}
