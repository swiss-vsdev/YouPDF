//! Fenêtre principale de YouPDF — v0.1 « lecteur ».
//!
//! Chrome calqué sur la référence UX mesurée : rail de 8 outils, barre de
//! titre maison (feux natifs conservés), onglets en pastilles, barre d'outils
//! flottante (Outils · 4 modes · annuler/rétablir), panneau latéral, canevas
//! continu avec ombres de page. Icônes vectorielles maison, palette mesurée.
//!
//! Rendu net sur écrans HiDPI : les bitmaps PDFium sont rastérisés à
//! `échelle_logique × pixels_per_point`, puis dessinés à la taille logique.

use std::collections::{BTreeSet, HashMap};
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
    EncryptionOptions, EncryptionStrength, FormField, FormFieldKind, HeaderFooter, NumberStyle,
    OutlineItem, PageRange, PdfiumRenderer, RedactionRegion, RenderOptions, Renderer, SearchIndex,
    SearchOptions, SearchResult, TextFormat, TextFragment, Watermark, STAMP_NAMES,
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
// Disposition des pages
// --------------------------------------------------------------------------

/// Disposition des pages dans le canevas.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum PageLayout {
    /// Une seule page : la page courante, sans défilement continu.
    Single,
    /// Défilement continu, une page par ligne.
    #[default]
    Continuous,
    /// Deux pages côte à côte.
    TwoPages,
    /// Première page seule, puis deux par deux, comme un livre ouvert.
    Cover,
}

impl PageLayout {
    fn label(self) -> &'static str {
        match self {
            PageLayout::Single => "Single page",
            PageLayout::Continuous => "Continuous scroll",
            PageLayout::TwoPages => "Two pages",
            PageLayout::Cover => "Cover (book)",
        }
    }
}

/// Entrée choisie dans le menu `…`.
enum MenuAction {
    Save,
    SaveAs,
    /// Ouvre la fenêtre d'export.
    Export,
    /// Peint les annotations dans la page et les supprime.
    Flatten,
    Properties,
    Close,
    /// Change la disposition des pages du canevas.
    Layout(PageLayout),
    /// Bascule le plein écran.
    Fullscreen,
    /// Ouvre la fenêtre de filigrane.
    Watermark,
    /// Ouvre la fenêtre d'en-tête et pied de page.
    HeaderFooter,
    /// Même fenêtre, préremplie pour des numéros de page.
    PageNumbers,
    /// Ouvre la fenêtre de compression.
    Compress,
    /// Ouvre la fenêtre de protection par mot de passe.
    Protect,
}

/// Répartit les pages en rangées selon la disposition choisie. Chaque rangée
/// occupe une ligne du canevas et contient une ou deux pages.
fn page_layout_rows(layout: PageLayout, total: u32, current: u32) -> Vec<Vec<u32>> {
    if total == 0 {
        return Vec::new();
    }
    // Deux pages par rangée à partir de `first`.
    let pairs = |first: u32| -> Vec<Vec<u32>> {
        let mut rows = Vec::new();
        let mut page = first;
        while page <= total {
            rows.push(if page < total {
                vec![page, page + 1]
            } else {
                vec![page]
            });
            page += 2;
        }
        rows
    };
    match layout {
        PageLayout::Single => vec![vec![current.clamp(1, total)]],
        PageLayout::Continuous => (1..=total).map(|page| vec![page]).collect(),
        PageLayout::TwoPages => pairs(1),
        PageLayout::Cover => {
            let mut rows = vec![vec![1]];
            rows.extend(pairs(2));
            rows
        }
    }
}

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

/// Édition en cours du texte d'un bloc de la page (mode Édition).
struct EditTextDraft {
    /// Bloc touché, tel qu'extrait du document.
    fragment: TextFragment,
    buffer: String,
    /// Premier affichage : demande le focus clavier.
    fresh: bool,
}

/// Champ de formulaire que trace un outil du mode « Prepare form ».
///
/// Volontairement recopiable : `ToolAction` l'est aussi, et `FormFieldKind`
/// du cœur ne l'est pas.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FormKind {
    Text,
    Checkbox,
    Radio,
    Choice,
    MultiChoice,
    Signature,
}

impl FormKind {
    /// Libellé affiché après création.
    fn label(&self) -> &'static str {
        match self {
            FormKind::Text => "Text field",
            FormKind::Checkbox => "Checkbox",
            FormKind::Radio => "Radio button",
            FormKind::Choice => "Dropdown",
            FormKind::MultiChoice => "List",
            FormKind::Signature => "Signature field",
        }
    }
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
    /// Clic sur un bloc de texte à réécrire (mode Édition).
    EditText,
    /// Clic pour poser une image (modes Comment et Édition).
    InsertImage,
    /// Glissé d'une boîte de champ de formulaire (mode Prepare form).
    FormField(FormKind),
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
    /// Fragments de texte de chaque page, extraits à la demande pour
    /// l'outil Texte du mode Édition.
    fragments: HashMap<u32, Vec<TextFragment>>,
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
    /// Fenêtre « Filigrane » et son brouillon.
    watermark_open: bool,
    watermark: Watermark,
    /// Première page décorée ; `None` = toutes.
    watermark_from: Option<u32>,
    /// Fenêtre « En-tête et pied de page » et son brouillon.
    header_footer_open: bool,
    header_footer: HeaderFooter,
    header_footer_from: Option<u32>,
    /// Dernier onglet affiché, pour détecter un changement d'onglet.
    last_active: usize,
    /// Confirmation de fermeture en attente d'un onglet modifié.
    pending_close: Option<usize>,
    /// La recherche doit prendre le focus à la prochaine trame (⌘F).
    find_focus: bool,
    /// Fenêtre « Exporter » et le format choisi.
    export_open: bool,
    export_format: TextFormat,
    /// Fenêtre « Compresser » : dernier bilan affiché.
    compress_open: bool,
    compress_report: Option<String>,
    /// Fenêtre « Protéger » et ses réglages.
    protect_open: bool,
    protect: EncryptionOptions,
    /// Outil d'annotation actif (index dans la barre du mode Comment).
    annot_tool: Option<usize>,
    /// Type de marque du surligneur (surlignage, soulignage, barré, ondulé).
    marker: AnnotationKind,
    /// Type de forme de l'outil Formes (rectangle, ellipse, ligne, flèche).
    shape: AnnotationKind,
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
    /// Filtre « respecter la casse » de la recherche.
    search_case: bool,
    /// Filtre « mots entiers » de la recherche.
    search_word: bool,
    /// Saisie de la page à atteindre (pastille bas-droite).
    page_entry: String,
    /// Surface d'accueil (fichiers récents) affichée à la place du canevas.
    home_open: bool,
    /// Disposition des pages du canevas (une, continue, deux, couverture).
    page_layout: PageLayout,
    /// Plein écran natif demandé par le menu `…`.
    fullscreen: bool,
    /// Le menu `…` vient d'être ouvert dans cette trame (le clic qui l'ouvre
    /// ne doit pas le refermer aussitôt).
    actions_opened: bool,
    /// Journal d'activité : messages récents et leur horodatage.
    activity: Vec<(String, f64)>,
    /// Fenêtre « Activity » ouverte.
    activity_open: bool,
    /// Derniers fichiers ouverts, du plus récent au plus ancien.
    recents: Vec<PathBuf>,
    /// Fenêtre « Organiser les pages » ouverte.
    organize_open: bool,
    /// Pages cochées dans cette fenêtre (vide = document entier).
    organize_selected: BTreeSet<usize>,
    /// Pages par fichier pour le découpage.
    organize_split: usize,
    /// Ligne « remplacer » ouverte.
    replace_open: bool,
    /// Texte de remplacement.
    replace_text: String,
    /// Régions marquées pour le caviardage.
    redact_regions: Vec<RedactionRegion>,
    /// Fenêtre de confirmation du caviardage.
    redact_modal: bool,
    /// Bloc de texte en cours de réécriture (mode Édition).
    edit_text: Option<EditTextDraft>,
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
            panel: None,
            mode: None,
            tools_open: false,
            actions_open: false,
            props_open: false,
            watermark_open: false,
            watermark: Watermark::default(),
            watermark_from: None,
            header_footer_open: false,
            header_footer: HeaderFooter::default(),
            header_footer_from: None,
            last_active: 0,
            pending_close: None,
            find_focus: false,
            export_open: false,
            export_format: TextFormat::Plain,
            compress_open: false,
            compress_report: None,
            protect_open: false,
            protect: EncryptionOptions::default(),
            annot_tool: None,
            marker: AnnotationKind::Highlight,
            shape: AnnotationKind::Square,
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
            search_case: false,
            search_word: false,
            page_entry: String::new(),
            home_open: initial.is_none(),
            page_layout: PageLayout::default(),
            fullscreen: false,
            actions_opened: false,
            activity: Vec::new(),
            activity_open: false,
            recents: load_recents(),
            organize_open: false,
            organize_selected: BTreeSet::new(),
            organize_split: 1,
            replace_open: false,
            replace_text: String::new(),
            redact_regions: Vec::new(),
            redact_modal: false,
            edit_text: None,
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
        let now = ctx.input(|i| i.time);
        self.record(text.into(), now);
    }

    /// Ajoute un message au bandeau et au journal d'activité.
    fn record(&mut self, text: String, now: f64) {
        self.toast = (text.clone(), now);
        let duplicate = matches!(self.activity.first(), Some((last, _)) if *last == text);
        if !duplicate {
            self.activity.insert(0, (text, now));
            self.activity.truncate(40);
        }
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
            Ok(mut doc) => {
                let title = doc.display_name();
                // Les annotations YouPDF sortent du document pour la durée de
                // l'édition : l'interface les dessine, l'onglet les garde,
                // l'enregistrement les remet en place avec leur apparence.
                // Sans cette purge, le moteur de rendu les peindrait en même
                // temps que l'interface et chaque marque apparaîtrait deux
                // fois.
                let annots = doc.read_annotations();
                if !annots.is_empty() && doc.clear_youpdf_annotations().is_err() {
                    tracing::warn!(path = %path.display(), "purge des annotations impossible");
                }
                self.tabs.push(Tab {
                    doc,
                    path: Some(path),
                    title: title.clone(),
                    page: 1,
                    zoom: None,
                    zoom_fit_page: false,
                    cache: HashMap::new(),
                    dirty: false,
                    annots,
                    annots_loaded: true,
                    undo: Vec::new(),
                    redo: Vec::new(),
                    outline: Vec::new(),
                    forms: Vec::new(),
                    attachments: Vec::new(),
                    fragments: HashMap::new(),
                    structure_loaded: false,
                });
                self.active = self.tabs.len() - 1;
                self.panel = Some(Panel::Thumbnails);
                self.home_open = false;
                let path = self.tabs[self.active].path.clone().unwrap_or_default();
                self.recents.retain(|old| old != &path);
                self.recents.insert(0, path);
                self.recents.truncate(8);
                save_recents(&self.recents);
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
    /// Variante sans `Context` (utilisable avant la première trame) : le
    /// message est horodaté au prochain rendu, comme dans le journal.
    fn toast_owned(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.toast = (text.clone(), f64::NAN);
        let duplicate = matches!(self.activity.first(), Some((last, _)) if *last == text);
        if !duplicate {
            self.activity.insert(0, (text, f64::NAN));
            self.activity.truncate(40);
        }
    }

    /// Demande la fermeture d'un onglet.
    ///
    /// Un onglet modifié passe par une confirmation : la fermeture est la
    /// seule action de l'interface qui puisse perdre du travail sans retour.
    fn close_tab(&mut self, idx: usize) {
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
    fn close_tab_now(&mut self, idx: usize) {
        if idx < self.tabs.len() {
            self.tabs.remove(idx);
            if self.active >= self.tabs.len() {
                self.active = self.tabs.len().saturating_sub(1);
            }
            self.last_active = self.active;
            self.search_index = None;
            self.results.clear();
        }
    }

    /// Tout ce qui appartient à la session ouverte sur un document : marques
    /// de caviardage, champ de formulaire, sélection, pages cochées,
    /// résultats de recherche. Changer d'onglet doit les remettre à zéro,
    /// sinon la suite de l'action s'applique au mauvais fichier.
    fn reset_document_session(&mut self) {
        self.redact_regions.clear();
        self.form_edit = None;
        self.selected_annot = 0;
        self.has_annot_selection = false;
        self.organize_selected.clear();
        self.replace_open = false;
        self.edit_text = None;
        self.text_draft = None;
        self.ink_current.clear();
        self.drag_from = None;
        self.drag_to = None;
        self.results.clear();
        self.search_index = None;
        self.search_rects.clear();
        self.search_rects_page = None;
        self.search_active = None;
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
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        match tab.doc.save_with_annotations(&path, &tab.annots) {
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
        match tab.doc.save_with_annotations(&path, &tab.annots) {
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
    /// Confirmation de fermeture d'un onglet modifié.
    fn close_confirm_window(&mut self, ctx: &egui::Context) {
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
        egui::Window::new("Unsaved changes")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(340.0);
                ui.label(
                    RichText::new(format!("\u{201c}{title}\u{201d} has unsaved changes."))
                        .size(12.5)
                        .color(ICON_BRIGHT),
                );
                ui.label(
                    RichText::new("Closing now would lose them.")
                        .size(11.0)
                        .color(MUTED),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Save and close").clicked() {
                        choice = Some(0);
                    }
                    if ui.button("Discard and close").clicked() {
                        choice = Some(1);
                    }
                    if ui.button("Cancel").clicked() {
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

    /// Fenêtre « Exporter » : choix du format texte.
    fn export_window(&mut self, ctx: &egui::Context) {
        if !self.export_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        egui::Window::new("Export")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.label(RichText::new("Format").size(12.0).color(MUTED));
                for format in [TextFormat::Plain, TextFormat::Markdown, TextFormat::Html] {
                    if ui
                        .selectable_label(self.export_format == format, format.display_name())
                        .clicked()
                    {
                        self.export_format = format;
                    }
                }
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "Only the text layer is exported: images, tables and the page layout \
                         are not rebuilt, and pages without extractable text come out empty.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(8.0);
                if ui.button("Export…").clicked() {
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

    /// Écrit la couche texte du document dans le format choisi.
    fn export_active(&mut self, ctx: &egui::Context) {
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
                    // La page est déjà rendue avec l'ancienne valeur.
                    tab.cache.clear();
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
        // Tant qu'un bloc est ouvert à la réécriture, le canevas ne réagit
        // plus : les clics reviennent au champ posé sur la page.
        if self.edit_text.is_some() {
            return ToolAction::None;
        }
        if self.mode == Some(3) {
            return if self.annot_tool == Some(1) {
                ToolAction::RedactRect
            } else {
                ToolAction::None
            };
        }
        if self.mode == Some(1) {
            // Mode Édition : on touche au contenu de la page, pas aux
            // annotations.
            return match self.annot_tool {
                Some(1) => ToolAction::EditText,
                Some(2) => ToolAction::InsertImage,
                Some(3) => ToolAction::Click(AnnotationKind::Link),
                _ => ToolAction::None,
            };
        }
        if self.mode == Some(2) {
            // Mode formulaire : chaque outil trace la boîte du champ. Les
            // types sans écriture côté cœur (bouton, code-barres, image,
            // séparateur) restent inertes et l'interface le dit.
            return match self.annot_tool {
                Some(0) => ToolAction::Select,
                Some(1) => ToolAction::FormField(FormKind::Text),
                Some(2) => ToolAction::FormField(FormKind::Checkbox),
                Some(3) => ToolAction::FormField(FormKind::Radio),
                Some(4) => ToolAction::FormField(FormKind::Choice),
                Some(5) => ToolAction::FormField(FormKind::MultiChoice),
                Some(9) => ToolAction::FormField(FormKind::Signature),
                _ => ToolAction::None,
            };
        }
        if self.mode != Some(0) {
            return ToolAction::None;
        }
        match self.annot_tool {
            Some(0) => ToolAction::Select,
            Some(1) => ToolAction::DragRect(self.marker),
            Some(2) => ToolAction::Click(AnnotationKind::FreeText),
            Some(3) => ToolAction::Click(AnnotationKind::Note),
            Some(4) => ToolAction::Ink,
            Some(5) => ToolAction::DragRect(self.shape),
            Some(6) => ToolAction::InsertImage,
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
        if (rect[2] - rect[0]).abs() < 2.0 && (rect[3] - rect[1]).abs() < 2.0 {
            return;
        }
        // Une ligne ou une flèche, c'est deux extrémités : le rectangle n'est
        // que leur boîte englobante, et il ne faut donc pas l'épaissir comme
        // une bande de surlignage.
        if matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow) {
            let mut annot = Annotation::new(page, kind, rect);
            annot.style = self.annot_style(kind);
            annot.add_ink_stroke(vec![[rect[0], rect[1]], [rect[2], rect[3]]]);
            annot.recompute_ink_bbox();
            self.push_annot(annot);
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
    /// Extrait le texte des pages visibles, une fois par page.
    fn ensure_fragments(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let total = tab.total() as u32;
        let current = tab.page as u32;
        let mut pages: Vec<u32> = page_layout_rows(self.page_layout, total, current)
            .into_iter()
            .find(|row| row.contains(&current))
            .unwrap_or_default();
        if pages.is_empty() {
            pages.push(current);
        }
        let missing: Vec<u32> = pages
            .iter()
            .copied()
            .filter(|page| !tab.fragments.contains_key(page))
            .collect();
        if missing.is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        for page in missing {
            match tab.doc.extract_text_fragments(page) {
                Ok(frags) => {
                    tab.fragments.insert(page, frags);
                }
                Err(err) => {
                    tracing::warn!(%err, page, "extraction du texte de la page");
                    tab.fragments.insert(page, Vec::new());
                }
            }
        }
    }

    /// Ouvre la réécriture du bloc de texte touché.
    fn edit_text_at(&mut self, page: usize, point: [f32; 2], ctx: &egui::Context) {
        self.ensure_fragments();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let found = tab.fragments.get(&(page as u32)).and_then(|frags| {
            frags
                .iter()
                .find(|frag| {
                    let [x1, y1, x2, y2] = frag.rect;
                    point[0] >= x1 - 2.0
                        && point[0] <= x2 + 2.0
                        && point[1] >= y1 - 2.0
                        && point[1] <= y2 + 2.0
                })
                .cloned()
        });
        match found {
            Some(fragment) => {
                self.edit_text = Some(EditTextDraft {
                    // Réécriture sur place : on part du texte existant, le
                    // curseur à la fin, comme dans un traitement de texte.
                    buffer: fragment.text.clone(),
                    fresh: true,
                    fragment,
                });
            }
            None => self.toast(ctx, "Click inside a text block"),
        }
    }

    /// Réécrit le texte du bloc en cours d'édition.
    fn replace_fragment(&mut self, ctx: &egui::Context) {
        let Some(draft) = self.edit_text.take() else {
            return;
        };
        if draft.buffer == draft.fragment.text {
            return;
        }
        let replaced = match self.tabs.get_mut(self.active) {
            Some(tab) => tab.doc.replace_text_fragment(&draft.fragment, &draft.buffer),
            None => return,
        };
        match replaced {
            Ok(()) => {
                self.mark_document_changed();
                self.toast(ctx, "Text replaced");
            }
            Err(err) => self.toast(ctx, format!("Replace failed: {err}")),
        }
    }

    /// Demande une image puis la pose au point touché.
    fn insert_image_at(&mut self, page: usize, point: [f32; 2], ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Image", &["png", "jpg", "jpeg"])
            .pick_file()
        else {
            return;
        };
        let (pixels, width, height) = match youpdf_core::edit::load_image_rgb(&path) {
            Ok(decoded) => decoded,
            Err(err) => {
                self.toast(ctx, format!("Image failed: {err}"));
                return;
            }
        };
        // Taille : au plus deux cinquièmes de la page, proportions gardées.
        let (page_w, page_h) = self
            .tabs
            .get(self.active)
            .map(|tab| tab.doc.page_size(page as u32))
            .unwrap_or((595.0, 842.0));
        let scale = (page_w * 0.4 / width as f32).min(page_h * 0.4 / height as f32);
        let half_w = width as f32 * scale / 2.0;
        let half_h = height as f32 * scale / 2.0;
        let rect = [
            point[0] - half_w,
            point[1] - half_h,
            point[0] + half_w,
            point[1] + half_h,
        ];
        let inserted = match self.tabs.get_mut(self.active) {
            Some(tab) => {
                tab.doc
                    .insert_image_bitmap(page, rect, &pixels, width, height)
            }
            None => return,
        };
        match inserted {
            Ok(()) => {
                self.mark_document_changed();
                self.toast(ctx, "Image inserted");
            }
            Err(err) => self.toast(ctx, format!("Image failed: {err}")),
        }
    }

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
                        // Type de marque (surligneur) ou de forme : le même
                        // outil couvre les quatre variantes que PDF connaît.
                        let kinds: &[(AnnotationKind, &str)] = match tool {
                            1 => &[
                                (AnnotationKind::Highlight, "Highlight"),
                                (AnnotationKind::Underline, "Underline"),
                                (AnnotationKind::StrikeOut, "Strikeout"),
                                (AnnotationKind::Squiggly, "Squiggly"),
                            ],
                            5 => &[
                                (AnnotationKind::Square, "Rectangle"),
                                (AnnotationKind::Circle, "Ellipse"),
                                (AnnotationKind::Line, "Line"),
                                (AnnotationKind::Arrow, "Arrow"),
                            ],
                            _ => &[],
                        };
                        if !kinds.is_empty() {
                            ui.horizontal(|ui| {
                                for (kind, label) in kinds {
                                    let current = self.annot_kind_for(tool) == *kind;
                                    if ui.selectable_label(current, *label).clicked() {
                                        self.set_annot_kind(tool, *kind);
                                    }
                                }
                            });
                            ui.add_space(2.0);
                        }
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

    /// Type de marque ou de forme associé à un outil du menu.
    fn annot_kind_for(&self, tool: usize) -> AnnotationKind {
        match tool {
            1 => self.marker,
            5 => self.shape,
            _ => AnnotationKind::Highlight,
        }
    }

    /// Choisit le type de marque ou de forme d'un outil.
    fn set_annot_kind(&mut self, tool: usize, kind: AnnotationKind) {
        match tool {
            1 => self.marker = kind,
            5 => {
                self.shape = kind;
                // Un trait fin se règle en points, pas en bande de
                // surlignage : la largeur courante serait énorme.
                if matches!(kind, AnnotationKind::Line | AnnotationKind::Arrow) && self.annot_width > 4.0
                {
                    self.annot_width = 2.0;
                }
            }
            _ => {}
        }
    }

    /// À appeler après toute opération qui réécrit le document (pages,
    /// caviardage…) : caches de rendu, plan, formulaires, résultats de
    /// recherche et instantanés d'annulation deviennent caducs.
    fn mark_document_changed(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.dirty = true;
            tab.cache.clear();
            // Les annotations en attente ne vivent qu'en mémoire : les
            // marquer « à recharger » les remplacerait par celles du
            // document, c'est-à-dire par rien.
            tab.fragments.clear();
            tab.structure_loaded = false;
            tab.undo.clear();
            tab.redo.clear();
            tab.page = tab.page.clamp(1, tab.total().max(1));
        }
        self.search_index = None;
        self.search_rects.clear();
        self.search_rects_page = None;
        self.search_active = None;
        self.results.clear();
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

        let options = SearchOptions {
            case_sensitive: self.search_case,
            whole_word: self.search_word,
        };
        if let Some((_, index)) = &self.search_index {
            self.results = index.search_with(&query, 200, options);
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
                    Ok((regions.len(), cleaned))
                }
                Err(err) => Err(err.to_string()),
            }
        };
        // Le caviardage réécrit les pages : caches et instantanés caducs.
        self.mark_document_changed();
        match outcome {
            Ok((areas, cleaned)) => {
                self.annot_tool = Some(0);
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

                    let home = icon_button(ui, 30.0, 28.0, self.home_open, icon_home);
                    if home.clicked() {
                        self.home_open = !self.home_open;
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
                            self.activity_open = !self.activity_open;
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
                        let hit = rail_button(ui, selected, tool.draw)
                            .on_hover_text(tool.label);
                        if hit.clicked() {
                            match tool.panel {
                                Some(p) => {
                                    self.panel = if self.panel == Some(p) { None } else { Some(p) }
                                }
                                None => self.organize_open = true,
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
                    if panel == Panel::Thumbnails {
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
                            icon_grid(
                                ui.painter(),
                                rect.shrink(7.0),
                                if resp.hovered() { ICON_BRIGHT } else { ICON },
                            );
                            if resp.clicked() {
                                self.organize_open = true;
                            }
                        });
                    }
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
                            let active = self.search_case || self.search_word;
                            icon_funnel(
                                ui.painter(),
                                rect.shrink(7.0),
                                if resp.hovered() || active { ICON_BRIGHT } else { ICON },
                            );
                            if resp.clicked() {
                                ui.memory_mut(|m| m.toggle_popup(Id::new("search_filters")));
                            }
                            let mut refilter = false;
                            egui::popup_above_or_below_widget(
                                ui,
                                Id::new("search_filters"),
                                &resp,
                                egui::AboveOrBelow::Below,
                                egui::PopupCloseBehavior::CloseOnClickOutside,
                                |ui| {
                                    ui.set_min_width(150.0);
                                    ui.label(
                                        RichText::new("Search options")
                                            .size(11.0)
                                            .color(MUTED),
                                    );
                                    ui.add_space(4.0);
                                    refilter |= ui
                                        .checkbox(&mut self.search_case, "Case sensitive")
                                        .changed();
                                    refilter |= ui
                                        .checkbox(&mut self.search_word, "Whole words")
                                        .changed();
                                },
                            );
                            if refilter {
                                self.run_search(ctx);
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
                self.run_search(ctx);
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
            if std::mem::take(&mut self.find_focus) {
                resp.request_focus();
            }
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

    /// Fenêtre « Organiser les pages » : grille des pages avec rotation,
    /// suppression, extraction, découpage et insertion. Les opérations
    /// s'appliquent immédiatement au document en mémoire ; rien n'est écrit
    /// sur le disque tant que l'utilisateur n'enregistre pas.
    fn organize_window(&mut self, ctx: &egui::Context) {
        if !self.organize_open {
            return;
        }
        if self.tabs.is_empty() {
            self.organize_open = false;
            return;
        }

        let active = self.active;
        let ppp = ctx.pixels_per_point();
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
        let tab = &mut self.tabs[active];
        let total = tab.total();
        let current = tab.page;
        let mut selected = std::mem::take(&mut self.organize_selected);
        // Un document plus court (suppression, nouvelle ouverture) laisse des
        // pages fantômes dans la sélection.
        selected.retain(|page| *page >= 1 && *page <= total);
        let mut split_per = self.organize_split;
        let mut action: Option<OrganizeAction> = None;
        let mut open = true;

        egui::Window::new("Organize pages")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(720.0, 520.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if organize_button(ui, icon_rotate_left, "Rotate left") {
                        action = Some(OrganizeAction::Rotate(-90));
                    }
                    if organize_button(ui, icon_rotate_right, "Rotate right") {
                        action = Some(OrganizeAction::Rotate(90));
                    }
                    if organize_button(ui, icon_trash, "Delete") {
                        action = Some(OrganizeAction::Delete);
                    }
                    if organize_button(ui, icon_export, "Extract…") {
                        action = Some(OrganizeAction::Extract);
                    }
                    if organize_button(ui, icon_scissors, "Split…") {
                        action = Some(OrganizeAction::Split);
                    }
                    if organize_button(ui, icon_file_plus, "Insert…") {
                        action = Some(OrganizeAction::Insert);
                    }
                });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    let count = if selected.is_empty() { total } else { selected.len() };
                    ui.label(
                        RichText::new(if selected.is_empty() {
                            format!("No selection: actions apply to all {total} page(s)")
                        } else {
                            format!("{count} of {total} page(s) selected")
                        })
                        .size(11.0)
                        .color(MUTED),
                    );
                    if ui.small_button("Select all").clicked() {
                        selected = (1..=total).collect();
                    }
                    if ui.small_button("Clear").clicked() {
                        selected.clear();
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Split every").size(11.0).color(MUTED));
                    ui.add(egui::DragValue::new(&mut split_per).range(1..=200).speed(0.2));
                    ui.label(RichText::new("page(s)").size(11.0).color(MUTED));
                });
                ui.add_space(6.0);
                ui.separator();

                ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for page in 1..=total as u32 {
                                let (wp, hp) = tab.doc.page_size(page);
                                let scale = 104.0 / wp.max(1.0);
                                let thumb = vec2(wp * scale, hp * scale);
                                let (rect, resp) =
                                    ui.allocate_exact_size(vec2(116.0, 172.0), Sense::click());
                                let img_rect = Rect::from_center_size(
                                    pos2(rect.center().x, rect.min.y + 8.0 + thumb.y * 0.5),
                                    thumb,
                                );
                                let picked = selected.contains(&(page as usize));
                                ui.painter().rect_filled(
                                    img_rect.expand(3.0),
                                    Rounding::same(3.0),
                                    if picked {
                                        THUMB_TINT
                                    } else {
                                        Color32::from_rgb(0x28, 0x28, 0x28)
                                    },
                                );
                                if let Some(tex) = Self::page_texture(
                                    ctx, renderer, tab, page, 1, scale, ppp, status,
                                ) {
                                    ui.painter().image(
                                        tex.id(),
                                        img_rect,
                                        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                        Color32::WHITE,
                                    );
                                }
                                if picked {
                                    ui.painter().rect_stroke(
                                        img_rect.expand(3.0),
                                        3.0,
                                        Stroke::new(1.5_f32, ACCENT),
                                    );
                                }
                                ui.painter().text(
                                    pos2(rect.center().x, img_rect.max.y + 14.0),
                                    Align2::CENTER_CENTER,
                                    page.to_string(),
                                    FontId::proportional(11.0),
                                    if picked { ACCENT } else { MUTED },
                                );
                                if page as usize == current {
                                    ui.painter().circle_filled(
                                        pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                                        3.0,
                                        ACCENT,
                                    );
                                }
                                if resp.clicked() {
                                    if picked {
                                        selected.remove(&(page as usize));
                                    } else {
                                        selected.insert(page as usize);
                                    }
                                }
                                ui.add_space(2.0);
                            }
                        });
                    });
            });

        self.organize_selected = selected;
        self.organize_split = split_per.max(1);
        if !open {
            self.organize_open = false;
        }
        let Some(action) = action else {
            return;
        };

        // Pages visées : la sélection, ou le document entier.
        let pages: Vec<usize> = if self.organize_selected.is_empty() {
            (1..=total).collect()
        } else {
            self.organize_selected.iter().copied().collect()
        };
        let Some(tab) = self.tabs.get_mut(active) else {
            return;
        };
        let per_file = self.organize_split;

        let outcome: Result<String, String> = match action {
            OrganizeAction::Rotate(delta) => tab
                .doc
                .rotate_pages(&pages, delta)
                .map(|count| format!("{count} page(s) rotated"))
                .map_err(|err| format!("Rotation failed: {err}")),
            OrganizeAction::Delete => {
                if pages.len() >= total {
                    Err("Keep at least one page".to_owned())
                } else {
                    tab.doc
                        .delete_pages(&pages)
                        .map(|count| format!("{count} page(s) removed"))
                        .map_err(|err| format!("Delete failed: {err}"))
                }
            }
            OrganizeAction::Extract => match rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("extract.pdf")
                .save_file()
            {
                Some(path) => tab
                    .doc
                    .extract_pages(&pages, &path)
                    .map(|_| {
                        format!(
                            "{} page(s) extracted to {}",
                            pages.len(),
                            path.file_name().unwrap_or_default().to_string_lossy()
                        )
                    })
                    .map_err(|err| format!("Extract failed: {err}")),
                None => Err(String::new()),
            },
            OrganizeAction::Split => match rfd::FileDialog::new().pick_folder() {
                Some(directory) => tab
                    .doc
                    .split(&directory, per_file)
                    .map(|files| format!("{} file(s) written", files.len()))
                    .map_err(|err| format!("Split failed: {err}")),
                None => Err(String::new()),
            },
            OrganizeAction::Insert => match rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
            {
                Some(path) => {
                    let after = pages.iter().copied().max().unwrap_or(1);
                    tab.doc
                        .insert_pages_after(after, &path)
                        .map(|count| format!("{count} page(s) inserted after page {after}"))
                        .map_err(|err| format!("Insert failed: {err}"))
                }
                None => Err(String::new()),
            },
        };

        match action {
            OrganizeAction::Rotate(_) | OrganizeAction::Delete | OrganizeAction::Insert => {
                if outcome.is_ok() {
                    self.mark_document_changed();
                }
            }
            OrganizeAction::Extract | OrganizeAction::Split => {}
        }
        match outcome {
            Ok(message) => self.toast(ctx, message),
            Err(message) if !message.is_empty() => self.toast(ctx, message),
            Err(_) => {}
        }
    }

    /// Description du champ à créer pour un outil donné.
    fn new_form_field(
        kind: FormKind,
        page: usize,
        rect: [f32; 4],
        name: String,
    ) -> youpdf_core::forms::NewFormField {
        use youpdf_core::forms::NewFormField;
        match kind {
            FormKind::Text => NewFormField::text(page as u32, rect, name),
            FormKind::Checkbox => NewFormField::checkbox(page as u32, rect, name),
            FormKind::Radio => {
                let mut field = NewFormField::checkbox(page as u32, rect, name);
                field.kind = FormFieldKind::Radio;
                field
            }
            FormKind::Choice => {
                NewFormField::choice(page as u32, rect, name, &["Option 1", "Option 2"])
            }
            FormKind::MultiChoice => {
                let mut field =
                    NewFormField::choice(page as u32, rect, name, &["Option 1", "Option 2"]);
                field.kind = FormFieldKind::MultiChoice;
                field
            }
            FormKind::Signature => {
                let mut field = NewFormField::text(page as u32, rect, name);
                field.kind = FormFieldKind::Signature;
                field
            }
        }
    }

    /// Crée un champ de formulaire sur la zone tracée, et l'inscrit au
    /// panneau des formulaires.
    fn create_form_field(
        &mut self,
        page: usize,
        rect: [f32; 4],
        kind: FormKind,
        ctx: &egui::Context,
    ) {
        if (rect[2] - rect[0]) < 6.0 || (rect[3] - rect[1]) < 6.0 {
            return;
        }
        let created = self.tabs.get_mut(self.active).map(|tab| {
            // Les noms de champ sont des identifiants dans tout le document :
            // deux champs ne peuvent pas partager le même.
            let existing: Vec<String> = tab.forms.iter().map(|f| f.name.clone()).collect();
            let mut num = existing.len() + 1;
            while existing.contains(&format!("Field{num}")) {
                num += 1;
            }
            let field = Self::new_form_field(kind, page, rect, format!("Field{num}"));
            tab.doc.create_form_field(&field)
        });
        match created {
            Some(Ok(field)) => {
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.forms.push(field);
                }
                self.mark_document_changed();
                self.toast(ctx, format!("{} added", kind.label()));
            }
            Some(Err(err)) => self.toast(ctx, format!("Field failed: {err}")),
            None => {}
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
        // Sur l'accueil (aucun document, ou accueil affiché), pas de barres
        // flottantes : elles n'auraient aucun document sur quoi agir.
        if self.home_open || self.tabs.is_empty() {
            return;
        }
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
                                let c = menu_row(ui, label, shortcut);
                                if c {
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
                                self.organize_open = true;
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
                                self.actions_opened = true;
                            }
                        });
                    });
            });

        // Menu `…` : enregistrer, exporter, propriétés.
        if self.actions_open {
            let mut action = None;
            let area = Area::new(Id::new("actions-menu"))
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
                                action = Some(MenuAction::Save);
                            }
                            if menu_row(ui, "Save as…", "Shift+⌘S") {
                                action = Some(MenuAction::SaveAs);
                            }
                            if menu_row(ui, "Export…", "") {
                                action = Some(MenuAction::Export);
                            }
                            if menu_row(ui, "Flatten annotations", "") {
                                action = Some(MenuAction::Flatten);
                            }
                            ui.add_space(4.0);
                            let sep = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, SEPARATOR),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Document").size(10.0).color(MUTED));
                            });
                            if menu_row(ui, "Watermark…", "") {
                                action = Some(MenuAction::Watermark);
                            }
                            if menu_row(ui, "Header & footer…", "") {
                                action = Some(MenuAction::HeaderFooter);
                            }
                            if menu_row(ui, "Page numbers…", "") {
                                action = Some(MenuAction::PageNumbers);
                            }
                            if menu_row(ui, "Compress…", "") {
                                action = Some(MenuAction::Compress);
                            }
                            if menu_row(ui, "Protect…", "") {
                                action = Some(MenuAction::Protect);
                            }
                            ui.add_space(4.0);
                            let sep = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, SEPARATOR),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("View").size(10.0).color(MUTED));
                            });
                            for (mark, layout) in [
                                ("Single page", PageLayout::Single),
                                ("Continuous scroll", PageLayout::Continuous),
                                ("Two pages", PageLayout::TwoPages),
                                ("Cover (book)", PageLayout::Cover),
                            ] {
                                if menu_row_checked(ui, mark, self.page_layout == layout) {
                                    action = Some(MenuAction::Layout(layout));
                                }
                            }
                            if menu_row_checked(ui, "Fullscreen", self.fullscreen) {
                                action = Some(MenuAction::Fullscreen);
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
                                action = Some(MenuAction::Properties);
                            }
                            if menu_row(ui, "Close tab", "⌘W") {
                                action = Some(MenuAction::Close);
                            }
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
                    MenuAction::Save => self.save_active(ctx),
                    MenuAction::SaveAs => self.save_as(ctx),
                    MenuAction::Export => self.export_open = true,
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
                }
            }
        }
    }

    /// Peint les annotations dans les pages du document ouvert.
    ///
    /// L'opération est définitive pour la session : le dessin devient du
    /// contenu de page, les annotations disparaissent — y compris de
    /// l'historique, qui ne saurait plus les remettre en état sans les
    /// réintroduire dans le PDF au moment de l'enregistrement.
    fn flatten_annotations(&mut self, ctx: &egui::Context) {
        // Les annotations vivent dans l'onglet jusqu'à l'enregistrement : il
        // faut d'abord les écrire dans le document pour pouvoir les aplatir.
        if let Err(err) = self.flush_annots() {
            self.toast(ctx, format!("Flatten failed: {err}"));
            return;
        }
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let result = tab.doc.flatten_annotations();
        let count = match result {
            Ok(count) => count,
            Err(err) => {
                self.toast(ctx, format!("Flatten failed: {err}"));
                return;
            }
        };
        if count == 0 {
            self.toast(ctx, "Nothing to flatten");
            return;
        }
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.annots.clear();
            tab.annots_loaded = true;
        }
        self.selected_annot = 0;
        self.has_annot_selection = false;
        self.mark_document_changed();
        self.toast(ctx, format!("{count} annotation(s) flattened"));
    }

    /// Change la disposition des pages et ramène la page courante sous les
    /// yeux : dans une autre grille, la position de défilement n'a plus de
    /// sens.
    fn set_page_layout(&mut self, ctx: &egui::Context, layout: PageLayout) {
        if self.page_layout == layout {
            return;
        }
        self.page_layout = layout;
        self.jump = Some(self.tabs.get(self.active).map_or(1, |tab| tab.page));
        self.toast(ctx, format!("Layout: {}", layout.label()));
    }

    /// Bascule le plein écran natif (le chrome maison reste visible).
    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
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

    // -- Filigrane, en-tête, compression, protection -----------------------

    /// Nom de fichier sans extension de l'onglet actif, pour proposer un
    /// nom de sortie (« rapport-protected.pdf »).
    fn active_stem(&self) -> Option<String> {
        self.tabs
            .get(self.active)
            .and_then(|tab| tab.path.as_ref())
            .and_then(|path| path.file_stem())
            .map(|stem| stem.to_string_lossy().into_owned())
    }

    /// « Toutes les pages » ou « à partir de N » donnent une plage.
    fn page_range_from(from: Option<u32>) -> PageRange {
        match from {
            Some(page) => PageRange::From(page.max(1)),
            None => PageRange::All,
        }
    }

    /// Nombre de pages de l'onglet actif, au moins une.
    fn active_total(&self) -> u32 {
        self.tabs
            .get(self.active)
            .map(|tab| tab.total())
            .unwrap_or(0)
            .max(1) as u32
    }

    /// Fenêtre « Filigrane ».
    fn watermark_window(&mut self, ctx: &egui::Context) {
        if !self.watermark_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        egui::Window::new("Watermark")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(360.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Text").size(12.0).color(MUTED));
                    ui.add(TextEdit::singleline(&mut self.watermark.text).desired_width(230.0));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Size").size(12.0).color(MUTED));
                    ui.add(
                        egui::DragValue::new(&mut self.watermark.font_size).range(6.0..=200.0),
                    );
                    ui.label(RichText::new("Angle").size(12.0).color(MUTED));
                    ui.add(
                        egui::DragValue::new(&mut self.watermark.rotation_deg).range(-90.0..=90.0),
                    );
                    ui.checkbox(&mut self.watermark.tiled, "Tiled");
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Opacity").size(12.0).color(MUTED));
                    ui.add(
                        egui::Slider::new(&mut self.watermark.opacity, 0.05..=1.0).show_value(false),
                    );
                    ui.label(
                        RichText::new(format!("{:.0} %", self.watermark.opacity * 100.0))
                            .size(12.0)
                            .color(ICON_BRIGHT),
                    );
                    ui.label(RichText::new("Grey").size(12.0).color(MUTED));
                    let mut grey = self.watermark.color[0];
                    if ui
                        .add(egui::Slider::new(&mut grey, 0..=255).show_value(false))
                        .changed()
                    {
                        self.watermark.color = [grey, grey, grey];
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Pages").size(12.0).color(MUTED));
                    let mut all = self.watermark_from.is_none();
                    if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                        self.watermark_from = if all { None } else { Some(1) };
                    }
                    if let Some(from) = self.watermark_from.as_mut() {
                        ui.label(RichText::new("then from").size(12.0).color(MUTED));
                        ui.add(egui::DragValue::new(from).range(1..=total));
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Remove from the document").clicked() {
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
    fn header_footer_window(&mut self, ctx: &egui::Context) {
        if !self.header_footer_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        egui::Window::new("Header & footer")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                ui.label(
                    RichText::new(
                        "The band is drawn at the top and at the bottom of the page;                          the page number joins the footer centre.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(4.0);
                egui::Grid::new("hf-grid")
                    .num_columns(3)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Left").size(11.0).color(MUTED));
                        ui.label(RichText::new("Centre").size(11.0).color(MUTED));
                        ui.label(RichText::new("Right").size(11.0).color(MUTED));
                        ui.end_row();
                        ui.add(
                            TextEdit::singleline(&mut self.header_footer.left).desired_width(140.0),
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.header_footer.center)
                                .desired_width(140.0),
                        );
                        ui.add(
                            TextEdit::singleline(&mut self.header_footer.right)
                                .desired_width(140.0),
                        );
                        ui.end_row();
                    });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Page numbers").size(12.0).color(MUTED));
                    for (text, style) in [
                        ("None", None),
                        ("1", Some(NumberStyle::Numeric)),
                        ("1 / n", Some(NumberStyle::NumericTotal)),
                        ("i", Some(NumberStyle::Roman)),
                    ] {
                        if ui
                            .selectable_label(self.header_footer.page_numbers == style, text)
                            .clicked()
                        {
                            self.header_footer.page_numbers = style;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Start at").size(12.0).color(MUTED));
                    ui.add(egui::DragValue::new(&mut self.header_footer.start_page).range(1..=total));
                    ui.checkbox(&mut self.header_footer.skip_first_page, "Skip first page");
                    ui.label(RichText::new("Size").size(12.0).color(MUTED));
                    ui.add(egui::DragValue::new(&mut self.header_footer.font_size).range(4.0..=36.0));
                    ui.label(RichText::new("Margin").size(12.0).color(MUTED));
                    ui.add(egui::DragValue::new(&mut self.header_footer.margin).range(0.0..=120.0));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Pages").size(12.0).color(MUTED));
                    let mut all = self.header_footer_from.is_none();
                    if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                        self.header_footer_from = if all { None } else { Some(1) };
                    }
                    if let Some(from) = self.header_footer_from.as_mut() {
                        ui.label(RichText::new("then from").size(12.0).color(MUTED));
                        ui.add(egui::DragValue::new(from).range(1..=total));
                    }
                });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Apply").clicked() {
                        apply = true;
                    }
                    if ui.button("Remove from the document").clicked() {
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
    fn compress_window(&mut self, ctx: &egui::Context) {
        if !self.compress_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        egui::Window::new("Compress")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(
                    RichText::new(
                        "Streams are deflated and unreachable objects are dropped.                          Images are not resampled and fonts are not subset, so a file                          made of scans barely shrinks.",
                    )
                    .size(12.0)
                    .color(MUTED),
                );
                if let Some(report) = self.compress_report.clone() {
                    ui.add_space(6.0);
                    ui.label(RichText::new(report).size(12.0).color(ICON_BRIGHT));
                }
                ui.add_space(8.0);
                if ui.button("Save compressed copy…").clicked() {
                    run = true;
                }
            });
        if run {
            self.compress_active(ctx);
        }
        if !open {
            self.compress_open = false;
        }
    }

    /// Exécute une écriture qui part du document lui-même (copie compressée,
    /// copie chiffrée) en y appliquant d'abord les annotations en attente.
    ///
    /// Sans cela la copie écrite ne contiendrait pas les annotations de la
    /// session. Elles sont retirées du document juste après : l'affichage les
    /// dessine lui-même, et les garder dans le document les dessinerait deux
    /// fois.
    fn with_annotations_applied<T>(
        &mut self,
        write: impl FnOnce(&mut Tab) -> T,
    ) -> Option<T> {
        let tab = self.tabs.get_mut(self.active)?;
        if tab.doc.apply_annotations(&tab.annots).is_err() {
            return None;
        }
        let outcome = write(tab);
        let _ = tab.doc.clear_youpdf_annotations();
        Some(outcome)
    }

    /// Écrit une copie compressée du document, et retient le bilan.
    fn compress_active(&mut self, ctx: &egui::Context) {
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
        let result = self.with_annotations_applied(|tab| tab.doc.compress(&path));
        match result {
            Some(Ok(report)) => {
                let before = report.before_bytes as f64 / 1024.0;
                let after = report.after_bytes as f64 / 1024.0;
                self.compress_report = Some(format!(
                    "{before:.0} KB → {after:.0} KB ({} byte(s) saved)",
                    report.saved_bytes()
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

    /// Fenêtre « Protéger ».
    fn protect_window(&mut self, ctx: &egui::Context) {
        if !self.protect_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        egui::Window::new("Protect")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(420.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Encryption").size(12.0).color(MUTED));
                    for strength in [EncryptionStrength::Aes256, EncryptionStrength::Rc4_128] {
                        if ui
                            .selectable_label(
                                self.protect.strength == strength,
                                strength.display_name(),
                            )
                            .clicked()
                        {
                            self.protect.strength = strength;
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Open password").size(12.0).color(MUTED));
                    ui.add(
                        TextEdit::singleline(&mut self.protect.user_password)
                            .password(true)
                            .desired_width(190.0),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Permissions password").size(12.0).color(MUTED));
                    ui.add(
                        TextEdit::singleline(&mut self.protect.owner_password)
                            .password(true)
                            .desired_width(150.0),
                    );
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Allowed actions").size(12.0).color(MUTED));
                egui::Grid::new("protect-perms")
                    .num_columns(3)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        ui.checkbox(&mut self.protect.permissions.print, "Print");
                        ui.checkbox(
                            &mut self.protect.permissions.print_high_quality,
                            "High-quality print",
                        );
                        ui.checkbox(&mut self.protect.permissions.copy, "Copy text");
                        ui.end_row();
                        ui.checkbox(&mut self.protect.permissions.modify, "Modify");
                        ui.checkbox(&mut self.protect.permissions.annotate, "Comment");
                        ui.checkbox(&mut self.protect.permissions.fill_forms, "Fill forms");
                        ui.end_row();
                        ui.checkbox(&mut self.protect.permissions.assemble, "Assemble");
                        ui.checkbox(&mut self.protect.permissions.accessibility, "Accessibility");
                        ui.end_row();
                    });
                ui.add_space(8.0);
                ui.label(
                    RichText::new(
                        "The copy is encrypted on disk; the document in the window stays open                          and unencrypted.",
                    )
                    .size(11.0)
                    .color(MUTED),
                );
                ui.add_space(6.0);
                if ui.button("Save encrypted copy…").clicked() {
                    run = true;
                }
            });
        if run {
            self.protect_active(ctx);
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
        // Tous les modes passent par le même aiguillage : l'outil courant
        // décide, et un mode sans outil de canevas répond « rien ».
        let action = self.canvas_action();
        // L'outil Texte a besoin des blocs de la page visible : survol,
        // choix du bloc, réécriture.
        if action == ToolAction::EditText {
            self.ensure_fragments();
        }
        // Le bloc en cours de réécriture quitte `self` le temps de la trame :
        // le canevas emprunte déjà `self.tabs`, et le champ posé sur la page
        // doit pouvoir modifier son tampon pendant la boucle.
        let mut draft = self.edit_text.take();
        let mut edit_commit = false;
        let mut edit_cancel = false;
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
        let last_fit = &mut self.last_fit;
        let page_layout = self.page_layout;
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
        let mut pending_edit: Option<(usize, [f32; 2])> = None;
        let mut pending_image: Option<(usize, [f32; 2])> = None;
        let mut pending_redact: Option<(usize, [f32; 4])> = None;
        let mut pending_form: Option<(usize, [f32; 4], FormKind)> = None;
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
                // Géométrie : on répartit les pages en rangées, on alloue une
                // ligne par rangée, puis on place les pages dedans. La
                // disposition choisie décide combien de pages partagent une
                // ligne ; « Une page » n'en garde que la page courante.
                let mut placed: Vec<(u32, Rect, Rect, Id, f32)> = Vec::new();
                for row_pages in &page_layout_rows(page_layout, total as u32, tab.page as u32) {
                    // Toutes les pages d'une rangée partagent la même échelle :
                    // sinon deux pages côte à côte n'auraient pas la même
                    // hauteur, et la lecture perdrait ses repères.
                    let sizes: Vec<(f32, f32)> = row_pages
                        .iter()
                        .map(|page| tab.doc.page_size(*page))
                        .collect();
                    let total_w: f32 = sizes.iter().map(|(w, _)| *w).sum();
                    let max_h = sizes.iter().map(|(_, h)| *h).fold(1.0_f32, f32::max);
                    let slots = row_pages.len() as f32;
                    let scale = tab
                        .zoom
                        .unwrap_or_else(|| {
                            let fit_width =
                                (avail - PAGE_SPACING * (slots - 1.0)) / total_w.max(1.0);
                            if tab.zoom_fit_page {
                                fit_width.min(view_h / max_h)
                            } else {
                                fit_width
                            }
                        })
                        .clamp(0.05, 8.0);
                    if tab.zoom.is_none() {
                        *last_fit = scale;
                    }
                    let row_h = max_h * scale;
                    // La ligne occupe toute la largeur disponible : les pages
                    // sont centrées dedans, comme dans la référence. Sans cela,
                    // une page plus étroite que le canevas (zoom < ajustement)
                    // resterait collée à gauche.
                    let (row_rect, row) =
                        ui.allocate_exact_size(vec2(avail, row_h + PAGE_SPACING), Sense::hover());
                    let row_w = total_w * scale + PAGE_SPACING * (slots - 1.0);
                    let mut cursor_x = row_rect.min.x + ((avail - row_w) * 0.5).max(0.0);

                    for (slot, page) in row_pages.iter().enumerate() {
                        let (wp, hp) = sizes[slot];
                        let size = vec2(wp * scale, hp * scale);
                        let page_rect = Rect::from_min_size(
                            pos2(cursor_x, row_rect.min.y + (row_h - size.y) * 0.5),
                            size,
                        );
                        cursor_x += size.x + PAGE_SPACING;
                        // La zone de clic descend jusqu'au bas de la rangée :
                        // entre deux pages empilées, la bande d'espacement
                        // répond donc encore au clic.
                        let hit =
                            Rect::from_min_size(page_rect.min, vec2(size.x, size.y + PAGE_SPACING));
                        placed.push((*page, hit, page_rect, row.id, scale));
                    }
                }

                for (page, rect, page_rect, row_id, scale) in placed {
                    let size = page_rect.size();
                    let resp = ui.interact(rect, row_id.with(page), sense);

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

                        // Réécriture sur place : le bloc touché devient un
                        // champ posé sur la page. Entrée valide, Échap annule,
                        // un clic à côté valide aussi.
                        if let Some(d) = draft.as_mut() {
                            if d.fragment.page as u32 == page {
                                let [x1, y1, x2, y2] = d.fragment.rect;
                                let block = Rect::from_min_max(
                                    pos2(
                                        page_rect.min.x + x1 * scale,
                                        page_rect.max.y - y2 * scale,
                                    ),
                                    pos2(
                                        page_rect.min.x + x2 * scale,
                                        page_rect.max.y - y1 * scale,
                                    ),
                                );
                                let height = (d.fragment.font_size * scale * 1.3).max(20.0);
                                let field = Rect::from_min_size(
                                    pos2(block.min.x - 3.0, block.center().y - height / 2.0),
                                    vec2(block.width().max(140.0) + 6.0, height),
                                );
                                ui.painter()
                                    .rect_filled(field, Rounding::same(2.0), Color32::WHITE);
                                ui.painter().rect_stroke(
                                    field,
                                    2.0,
                                    Stroke::new(1.0_f32, ACCENT),
                                );
                                let ink = Color32::from_rgb(
                                    d.fragment.color[0],
                                    d.fragment.color[1],
                                    d.fragment.color[2],
                                );
                                let resp = ui.put(
                                    field.shrink2(vec2(3.0, 0.0)),
                                    TextEdit::singleline(&mut d.buffer)
                                        .frame(false)
                                        .font(FontId::proportional(
                                            (d.fragment.font_size * scale).clamp(8.0, 96.0),
                                        ))
                                        .text_color(ink),
                                );
                                if d.fresh {
                                    resp.request_focus();
                                    d.fresh = false;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    edit_cancel = true;
                                } else if ui.input(|i| i.key_pressed(egui::Key::Enter))
                                    || resp.lost_focus()
                                {
                                    edit_commit = true;
                                }
                            }
                        }
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
                        // Outil Texte : chaque bloc est encadré, celui sous le
                        // curseur plus franchement — c'est ce bloc que le clic
                        // réécrira.
                        if action == ToolAction::EditText {
                            if let Some(frags) = tab.fragments.get(&page) {
                                let hover = resp.hover_pos();
                                for frag in frags {
                                    let [x1, y1, x2, y2] = frag.rect;
                                    let r = Rect::from_min_max(
                                        to_screen([x1, y1]),
                                        to_screen([x2, y2]),
                                    );
                                    let lit = hover.map(|p| r.contains(p)).unwrap_or(false);
                                    let stroke = if lit {
                                        Stroke::new(1.5_f32, ACCENT)
                                    } else {
                                        Stroke::new(
                                            1.0_f32,
                                            Color32::from_rgba_unmultiplied(0x7A, 0x7E, 0xF6, 70),
                                        )
                                    };
                                    ui.painter().rect_stroke(r.expand(1.0), 2.0, stroke);
                                }
                            }
                        }
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
                            ToolAction::EditText | ToolAction::InsertImage => {
                                if resp.clicked() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        let point = to_pdf(p);
                                        if action == ToolAction::EditText {
                                            pending_edit = Some((page as usize, point));
                                        } else {
                                            pending_image = Some((page as usize, point));
                                        }
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
                            ToolAction::FormField(kind) => {
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
                                        pending_form = Some((
                                            page as usize,
                                            [
                                                a[0].min(b[0]),
                                                a[1].min(b[1]),
                                                a[0].max(b[0]),
                                                a[1].max(b[1]),
                                            ],
                                            kind,
                                        ));
                                    }
                                }
                                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                    let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                    ui.painter().rect_stroke(
                                        rect,
                                        Rounding::same(2.0),
                                        Stroke::new(1.5_f32, ACCENT),
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

        self.edit_text = draft;
        if edit_cancel {
            self.edit_text = None;
        } else if edit_commit {
            self.replace_fragment(&ctx);
        }

        if let Some((page, kind, rect)) = pending_rect {
            self.add_rect_annot(page, kind, rect);
        }
        if let Some((page, points)) = pending_ink {
            tracing::info!("ink annot page {page} points {}", points.len());
            self.add_ink_annot(page, points);
        }
        if let Some((page, point)) = pending_edit {
            self.edit_text_at(page, point, &ctx);
        }
        if let Some((page, point)) = pending_image {
            self.insert_image_at(page, point, &ctx);
        }
        if let Some((page, point)) = pending_click {
            self.open_draft(page, point);
        }
        if let Some((page, rect, kind)) = pending_form {
            self.create_form_field(page, rect, kind, &ctx);
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

    /// Surface d'accueil : accès direct aux fichiers récents.
    fn empty_state(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        let mut forget: Option<PathBuf> = None;
        let mut clear = false;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(90.0);
                    ui.label(RichText::new("YouPDF").size(34.0).strong());
                    ui.add_space(8.0);
                    ui.label(
                        RichText::new("Free, local-first PDF editing")
                            .size(13.0)
                            .color(MUTED),
                    );
                    ui.add_space(22.0);
                    let button = ui.add_sized(
                        vec2(160.0, 32.0),
                        Button::new(RichText::new("Open PDF").size(14.0)).fill(ACCENT),
                    );
                    if button.clicked() {
                        self.open_dialog();
                    }
                    ui.add_space(10.0);
                    ui.label(
                        RichText::new("or drop a file anywhere")
                            .size(11.0)
                            .color(MUTED),
                    );

                    if self.recents.is_empty() {
                        return;
                    }
                    ui.add_space(34.0);
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() * 0.5 - 220.0).max(0.0));
                        ui.allocate_ui_with_layout(
                            vec2(440.0, 20.0),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.label(RichText::new("Recent files").size(13.0).strong());
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if ui.small_button("Clear").clicked() {
                                        clear = true;
                                    }
                                });
                            },
                        );
                    });
                    ui.add_space(8.0);

                    for path in self.recents.clone() {
                        let exists = path.is_file();
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| path.to_string_lossy().into_owned());
                        let parent = path
                            .parent()
                            .map(|p| p.to_string_lossy().into_owned())
                            .unwrap_or_default();

                        ui.horizontal(|ui| {
                            ui.add_space((ui.available_width() * 0.5 - 220.0).max(0.0));
                            let (rect, resp) =
                                ui.allocate_exact_size(vec2(440.0, 46.0), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(8.0),
                                    Color32::from_rgb(0x38, 0x38, 0x38),
                                );
                            }
                            ui.painter().text(
                                pos2(rect.min.x + 14.0, rect.min.y + 15.0),
                                Align2::LEFT_CENTER,
                                shorten(&name, 46),
                                FontId::proportional(12.5),
                                if exists { ICON_BRIGHT } else { MUTED },
                            );
                            ui.painter().text(
                                pos2(rect.min.x + 14.0, rect.min.y + 32.0),
                                Align2::LEFT_CENTER,
                                if exists {
                                    shorten(&parent, 60)
                                } else {
                                    "File not found".to_owned()
                                },
                                FontId::proportional(10.5),
                                MUTED,
                            );

                            let forget_rect = Rect::from_center_size(
                                pos2(rect.max.x - 20.0, rect.center().y),
                                vec2(20.0, 20.0),
                            );
                            let forget_resp =
                                ui.interact(forget_rect, resp.id.with("forget"), Sense::click());
                            ui.painter().text(
                                forget_rect.center(),
                                Align2::CENTER_CENTER,
                                if forget_resp.hovered() { "✕" } else { "" },
                                FontId::proportional(12.0),
                                ICON,
                            );
                            if forget_resp.clicked() {
                                forget = Some(path.clone());
                            } else if resp.clicked() {
                                open = Some(path.clone());
                            }
                        });
                        ui.add_space(4.0);
                    }
                    ui.add_space(30.0);
                });
            });

        if clear {
            self.recents.clear();
            save_recents(&self.recents);
        }
        if let Some(path) = forget {
            self.recents.retain(|old| old != &path);
            save_recents(&self.recents);
        }
        if let Some(path) = open {
            if path.is_file() {
                self.open_path(path);
            } else {
                self.toast(ui.ctx(), "File not found");
            }
        }
    }

    /// Journal d'activité (cloche de la barre de titre) : les derniers
    /// messages affichés, du plus récent au plus ancien.
    fn activity_window(&mut self, ctx: &egui::Context) {
        if !self.activity_open {
            return;
        }
        let now = ctx.input(|i| i.time);
        let mut open = true;
        let mut clear = false;

        egui::Window::new("Activity")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .anchor(Align2::RIGHT_TOP, vec2(-14.0, TITLE_H + 10.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("What YouPDF just did")
                            .size(11.0)
                            .color(MUTED),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            clear = true;
                        }
                    });
                });
                ui.add_space(4.0);
                if self.activity.is_empty() {
                    ui.label(
                        RichText::new("Nothing yet.")
                            .size(11.5)
                            .color(MUTED),
                    );
                    return;
                }
                ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                    for (text, stamp) in &self.activity {
                        let age = (now - stamp).max(0.0);
                        let when = if stamp.is_nan() || age < 60.0 {
                            "just now".to_owned()
                        } else if age < 3600.0 {
                            format!("{:.0} min ago", age / 60.0)
                        } else {
                            format!("{:.0} h ago", age / 3600.0)
                        };
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new(text).size(11.5));
                            ui.label(RichText::new(when).size(10.0).color(MUTED));
                        });
                        ui.add_space(2.0);
                    }
                });
            });

        if clear {
            self.activity.clear();
        }
        if !open {
            self.activity_open = false;
        }
    }

    // -- Superpositions ----------------------------------------------------

    /// Pastille bas-droite : page · zoom, façon « organize » de la référence.
    fn zoom_overlay(&mut self, ctx: &egui::Context) {
        if self.home_open {
            return;
        }
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
        // Une saisie en cours (recherche, nom de champ, remplacement) a la
        // priorité : sinon Retour arrière efface l'annotation sélectionnée au
        // lieu d'un caractère.
        let typing = ctx.wants_keyboard_input();
        let delete_pressed = ctx.input(|i| {
            i.key_pressed(egui::Key::Backspace) || i.key_pressed(egui::Key::Delete)
        }) && !typing;
        if delete_pressed && self.has_annot_selection {
            let idx = self.selected_annot;
            self.remove_annot(idx);
            self.toast(ctx, "Annotation removed");
            return;
        }
        let (cmd, shift, open, close, plus, minus, zero, save, find, full) = ctx.input(|i| {
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
                i.key_pressed(egui::Key::F),
            )
        });
        if cmd && find {
            self.search_step(if shift { -1 } else { 1 });
            return;
        }
        if !cmd {
            return;
        }
        if full {
            if shift {
                self.toggle_fullscreen(ctx);
            } else {
                self.panel = Some(Panel::Search);
                self.find_focus = true;
            }
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
        if self.active != self.last_active {
            self.last_active = self.active;
            self.reset_document_session();
        }
        self.ensure_annots();
        self.shortcuts(ctx);
        self.title_bar(ctx);
        self.rail(ctx);
        self.side_panel(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(CANVAS_STYLES[self.canvas_style.min(CANVAS_STYLES.len() - 1)]))
            .show(ctx, |ui| {
                if self.home_open || self.tabs.is_empty() {
                    self.empty_state(ui);
                } else {
                    self.canvas(ui);
                }
            });
        self.toolbar(ctx);
        self.zoom_overlay(ctx);
        self.properties_window(ctx);
        self.close_confirm_window(ctx);
        self.export_window(ctx);
        self.watermark_window(ctx);
        self.header_footer_window(ctx);
        self.compress_window(ctx);
        self.protect_window(ctx);
        self.draft_window(ctx);
        self.redact_window(ctx);
        self.organize_window(ctx);
        self.activity_window(ctx);
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

/// Ligne de menu cochable : la place de la coche est réservée même quand
/// l'entrée ne l'est pas, sinon les libellés d'un même groupe danseraient
/// d'une ligne à l'autre.
fn menu_row_checked(ui: &mut egui::Ui, label: &str, checked: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), HOVER_BG);
    }
    if checked {
        glyph(
            ui.painter(),
            Rect::from_center_size(pos2(rect.min.x + 18.0, rect.center().y), vec2(15.0, 15.0)),
            ACCENT,
            ph::CHECK,
        );
    }
    ui.painter().text(
        pos2(rect.min.x + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        ICON_BRIGHT,
    );
    resp.clicked()
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
/// Fichier des fichiers récents (`~/.youpdf/recents.txt`, un chemin par ligne).
fn recents_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(PathBuf::from(home).join(".youpdf").join("recents.txt"))
}

/// Lit la liste des fichiers récents ; liste vide si le fichier manque.
fn load_recents() -> Vec<PathBuf> {
    let Some(path) = recents_file() else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| PathBuf::from(line.trim()))
        .take(8)
        .collect()
}

/// Écrit la liste des fichiers récents. Une erreur d'écriture est ignorée :
/// la liste est un confort, pas une donnée du document.
fn save_recents(recents: &[PathBuf]) {
    let Some(path) = recents_file() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut text = String::new();
    for entry in recents {
        text.push_str(&entry.to_string_lossy());
        text.push('\n');
    }
    let _ = std::fs::write(path, text);
}

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
fn icon_grid(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::SQUARES_FOUR);
}
fn icon_rotate_left(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::ARROW_COUNTER_CLOCKWISE);
}
fn icon_rotate_right(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::ARROW_CLOCKWISE);
}
fn icon_export(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::EXPORT);
}
fn icon_scissors(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::SCISSORS);
}
fn icon_file_plus(p: &Painter, r: Rect, c: Color32) {
    glyph(p, r, c, ph::FILE_PLUS);
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

/// Opérations de la fenêtre « Organiser les pages ».
#[derive(Clone, Copy)]
enum OrganizeAction {
    /// Rotation de `delta` degrés (multiple de 90).
    Rotate(i64),
    /// Suppression des pages sélectionnées.
    Delete,
    /// Extraction vers un nouveau fichier.
    Extract,
    /// Découpage en fichiers de N pages.
    Split,
    /// Insertion des pages d'un autre PDF.
    Insert,
}

/// Bouton d'action de la fenêtre d'organisation : icône puis libellé.
fn organize_button(ui: &mut egui::Ui, icon: IconFn, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(116.0, 28.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        Rounding::same(6.0),
        if resp.hovered() { HOVER_BG } else { WIDGET_BG },
    );
    let color = if resp.hovered() { ICON_BRIGHT } else { ICON };
    icon(
        ui.painter(),
        Rect::from_center_size(pos2(rect.min.x + 15.0, rect.center().y), vec2(14.0, 14.0)),
        color,
    );
    ui.painter().text(
        pos2(rect.min.x + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        color,
    );
    resp.clicked()
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
            AnnotationKind::Highlight => {
                painter.rect_filled(r, Rounding::same(2.0), fill);
            }
            AnnotationKind::Squiggly => {
                // Même ondulation que l'apparence enregistrée : deux crans
                // par motif, posés sur le bas de la boîte.
                let amp = (3.0 * scale).max(1.0);
                let steps = ((r.width() / (4.0 * scale)).ceil() as usize * 2).max(4);
                let pts: Vec<egui::Pos2> = (0..=steps)
                    .map(|i| {
                        let t = i as f32 / steps as f32;
                        pos2(
                            r.min.x + t * r.width(),
                            r.max.y - if i % 2 == 0 { amp * 0.6 } else { amp * 1.8 },
                        )
                    })
                    .collect();
                painter.add(Shape::line(pts, Stroke::new(width, color)));
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
            AnnotationKind::Circle => {
                painter.add(Shape::circle_stroke(
                    r.center(),
                    r.width().min(r.height()) / 2.0,
                    Stroke::new(width, color),
                ));
            }
            AnnotationKind::Line | AnnotationKind::Arrow => {
                let pts = annot.ink_points.first().cloned().unwrap_or_default();
                let (a, b) = match (pts.first(), pts.get(1)) {
                    (Some(a), Some(b)) => (to_screen(a[0], a[1]), to_screen(b[0], b[1])),
                    _ => (r.min, r.max),
                };
                painter.line_segment([a, b], Stroke::new(width, color));
                if annot.kind == AnnotationKind::Arrow {
                    let d = b - a;
                    let len = d.length().max(0.01);
                    let u = d / len;
                    let side = vec2(-u.y, u.x);
                    let head = (len * 0.2).clamp(6.0, 20.0);
                    painter.add(Shape::convex_polygon(
                        vec![b, b - u * head + side * head * 0.45, b - u * head - side * head * 0.45],
                        color,
                        Stroke::NONE,
                    ));
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn cachet(rows: &[Vec<u32>]) -> HashSet<u32> {
        rows.iter().flatten().copied().collect()
    }

    /// Chaque disposition doit couvrir exactement les pages, une fois chacune.
    #[test]
    fn layouts_cover_every_page_once() {
        for total in 1..=7u32 {
            for layout in [
                PageLayout::Single,
                PageLayout::Continuous,
                PageLayout::TwoPages,
                PageLayout::Cover,
            ] {
                let rows = page_layout_rows(layout, total, 3);
                let pages = cachet(&rows);
                assert_eq!(
                    pages.len(),
                    if layout == PageLayout::Single { 1 } else { total as usize },
                    "{layout:?} sur {total} pages : {rows:?}"
                );
                assert!(pages.iter().all(|p| *p >= 1 && *p <= total), "{rows:?}");
                for row in &rows {
                    let expected = match layout {
                        PageLayout::Single | PageLayout::Continuous => 1,
                        PageLayout::TwoPages | PageLayout::Cover => 2,
                    };
                    assert!(
                        row.len() <= expected,
                        "{layout:?} : rangée trop longue {row:?}"
                    );
                }
            }
        }
    }

    /// La couverture laisse la première page seule, puis apparie 2-3, 4-5…
    #[test]
    fn cover_pairs_pages_after_the_first() {
        assert_eq!(
            page_layout_rows(PageLayout::Cover, 5, 1),
            vec![vec![1], vec![2, 3], vec![4, 5]]
        );
        assert_eq!(
            page_layout_rows(PageLayout::TwoPages, 5, 1),
            vec![vec![1, 2], vec![3, 4], vec![5]]
        );
        assert_eq!(
            page_layout_rows(PageLayout::Continuous, 3, 2),
            vec![vec![1], vec![2], vec![3]]
        );
        assert_eq!(
            page_layout_rows(PageLayout::Single, 3, 9),
            vec![vec![3]],
            "la page courante est ramenée dans les bornes"
        );
        assert!(page_layout_rows(PageLayout::Continuous, 0, 1).is_empty());
    }
}
