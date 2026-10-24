//! Fenêtre principale de YouPDF — v0.1 « lecteur ».
//!
//! Chrome calqué sur la référence UX mesurée : rail de 8 outils, barre de
//! titre maison (feux natifs conservés), onglets en pastilles, barre d'outils
//! flottante (Outils · 4 modes · annuler/rétablir), panneau latéral, canevas
//! continu avec ombres de page. Icônes vectorielles maison, palette mesurée.
//!
//! Rendu net sur écrans HiDPI : les bitmaps PDFium sont rastérisés à
//! `échelle_logique × pixels_per_point`, puis dessinés à la taille logique.
use youpdf_shared::{BODY, CAPTION, LABEL, Language, TITLE, Theme, caption, i18n, modal_frame, muted, pal, section, set_palette, t, RADIUS_L, RADIUS_S, RADIUS_M, RADIUS_XS, SPACE_L, SPACE_M, SPACE_S, SPACE_XS, RADIUS_TINY};

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use egui::{
    pos2, vec2, Align, Align2, Area, Button, CentralPanel, Color32, FontId, Frame, Id, Layout,
    Margin, Order, Painter, Rect, RichText, Rounding, ScrollArea, Sense, Shape, SidePanel, Stroke,
    TextEdit, TextureHandle, TextureOptions, TopBottomPanel, ViewportCommand, CursorIcon,
};
use egui_phosphor::regular as ph;

mod batch_ui;
mod forms_ui;
mod decor_ui;
mod marks_ui;
mod export_ui;
mod pages_ui;
mod prefs_ui;
mod organize_ui;
mod sign_ui;
mod reader;
mod canvas_ui;
mod toolbar_ui;
mod comments_ui;
mod search_ui;
mod bookmarks_ui;
mod thumbnails_ui;
mod home_ui;
mod chrome_ui;
mod compare_ui;
mod ocr_ui;
mod image_ui;
mod inspector_ui;
mod shortcuts_ui;
mod redact_ui;
mod extract_ui;
mod attachments_ui;
mod aspect_ui;
mod page_action_ui;
mod measure_ui;
mod password_ui;
mod icons;
mod history;
#[allow(unused_imports)]
pub use icons::*;

use history::trim_history;

use crate::ai::AiPanel;
use crate::home::{self, HomeCategory, HomeView, ViewMode, load_recents, save_recents, shorten};
use crate::app::batch_ui::{BatchEntry, BatchEvent};
use crate::inspector::{
    arrow_x, image_controls, markup_row, paint_arrow, place_inspector, text_style_row, AnnotationId,
    ImageAction, ImageId, InspectorTarget, TextBlockId, TextStyleUi, INSPECTOR_HEIGHT,
    INSPECTOR_WIDTH, MARKUP_KINDS, PALETTE_HEIGHT, SELECTION_INSPECTOR_HEIGHT,
    TEXT_INSPECTOR_HEIGHT,
};
use youpdf_core::compress::CompressionLevel;
use youpdf_core::render::NullRenderer;
use youpdf_core::ocr::{make_ocr, TesseractEngine};
use youpdf_core::{
    blank_document, document_from_images, export_comments, Annotation,
    AnnotationKind, AnnotationState, AnnotationStyle, AttachmentInfo, Background, BandSlots, Bates,
    BatesPosition, CommentFormat, ComparisonReport, CompressionExtras, Conformance, CropMargins,
    DifferenceKind, Document,
    EncryptionOptions, EncryptionStrength, ExtractedTable, FieldProperties, FormDataFormat,
    FormField, FormFieldKind, HeaderFooter, ImageFormat, KeyValue, LinkAction, MeasureKind,
    Measurement, NumberStyle, OcrEngine, OcrMode, OfficeFormat, OutlineEntry, OutlineItem,
    OutlineStyle, ImageCrop, OutlineTarget, PageRange, PageBitmap, PatchFont,
    PageLink,
    PdfiumRenderer, RedactionFill,
    RedactionMark, RedactionOverlay, RedactionReport, RenderOptions, Renderer, SearchIndex,
    SearchOptions, SearchResult, TextFormat,
    TextFragment, TextStyle, Unit, Watermark, smooth_polyline,
};
use youpdf_core::{LANGUAGES, combine_languages, language_name};

use youpdf_core::annotate::{BorderDash, STAMP_TEMPLATES};
use youpdf_core::annotation::transform::{annots_to_display, annots_to_page, link_at};
use youpdf_core::search::{find_query, match_rects, replace_occurrences};
use youpdf_core::util::{format_size, page_size_label};
use youpdf_core::forms::snap_form_rect;
use youpdf_core::annotation::comments::{
    CommentFilter, CommentSort, CommentStateFilter, comment_rows,
};
use youpdf_core::edit::{crop_fractions_in_image, response_changed};
use youpdf_core::annotate::author_initial;
use youpdf_core::structure::attachments::{prune_attachment_staging, safe_attachment_name, stage_attachment, unique_stamp};
use youpdf_core::office::hex_color;
use youpdf_core::structure::outline::{
    bookmark_all_keys, bookmark_at, bookmark_at_mut, bookmark_insert_index, bookmark_is_ancestor,
    bookmark_key, bookmark_path, bookmark_rows, bookmark_shift_path, bookmark_take,
};

// --------------------------------------------------------------------------
// Palette
// --------------------------------------------------------------------------

/// Fonds de canevas nommés par la référence, hors « Auto » (voir
/// `YouPdfApp::canvas_style`).
const CANVAS_THEMES: [(&str, Color32); 4] = [
    ("Dark", Color32::from_rgb(0x30, 0x30, 0x30)),
    ("Light", Color32::from_rgb(0xEC, 0xEC, 0xEE)),
    ("Mint", Color32::from_rgb(0xDF, 0xEE, 0xE7)),
    ("Starry Blue", Color32::from_rgb(0x08, 0x0A, 0x18)),
];

/// Teintes d'affichage des pages (protection des yeux), en RGBA non
/// prémultiplié.
///
/// La première entrée ne teinte rien : la page garde son blanc d'origine. Les
/// autres se posent en calque translucide sur la page rendue — le document
/// lui-même n'est jamais modifié.
const PAGE_TINTS: [(&str, [u8; 4]); 7] = [
    ("None", [255, 255, 255, 0]),
    ("Light Green", [198, 240, 198, 70]),
    ("Parchment", [245, 233, 200, 90]),
    ("Sepia", [222, 198, 158, 120]),
    ("Dark Night", [22, 26, 42, 190]),
    ("Rose", [255, 214, 219, 80]),
    ("Sky Blue", [203, 228, 250, 80]),
];

/// Couleurs du comparateur : vert ajouté, rouge supprimé, ambre modifié.
const ADDED_GREEN: Color32 = Color32::from_rgb(0x28, 0xA7, 0x45);
const REMOVED_RED: Color32 = Color32::from_rgb(0xDC, 0x35, 0x45);
const MODIFIED_AMBER: Color32 = Color32::from_rgb(0xFF, 0xC1, 0x07);

const TITLE_H: f32 = 40.0;
/// Nombre maximal d'états conservés dans une pile d'annulation.
const HISTORY_LIMIT: usize = 20;
/// Budget d'octets des états qui gardent une copie du document.
const HISTORY_BYTES: usize = 256 * 1024 * 1024;

/// Dâge maximal des dossiers d���des pièces jointes extraites.
const ATTACHMENT_STAGING_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);
const STATUS_H: f32 = 26.0;
const RAIL_W: f32 = 52.0;
const PANEL_W: f32 = 210.0;
const PAGE_SPACING: f32 = 16.0;
/// Cartes de l'écran d'accueil (onglets Récents / Favoris).
const CARD_W: f32 = 160.0;
const CARD_H: f32 = 200.0;
const GAP: f32 = 12.0;
/// Raccourcis d'outils de l'écran d'accueil.
const CELL_W: f32 = 160.0;
const CELL_H: f32 = 64.0;

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

/// Affichage d'une comparaison de deux documents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum CompareView {
    /// Rapport de différences de texte (défaut).
    #[default]
    Report,
    /// Les deux documents côte à côte, défilement synchronisé.
    SideBySide,
    /// Le second document posé sur le premier, opacité réglable.
    Overlay,
}

impl PageLayout {
    /// Toutes les dispositions, dans l'ordre du menu.
    const ALL: [PageLayout; 4] = [
        PageLayout::Single,
        PageLayout::Continuous,
        PageLayout::TwoPages,
        PageLayout::Cover,
    ];

    fn label(self) -> &'static str {
        match self {
            PageLayout::Single => "Single page",
            PageLayout::Continuous => "Continuous scroll",
            PageLayout::TwoPages => "Two pages",
            PageLayout::Cover => "Cover (book)",
        }
    }
}

/// Nature de l'export choisi dans la fenêtre « Export ».
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum ExportMode {
    /// Couche texte, dans l'un des `TextFormat`.
    #[default]
    Text,
    /// Une image par page, dans un dossier.
    Image,
    /// Toutes les pages empilées dans une seule image verticale.
    LongImage,
    /// Les images embarquées du document, extraites telles quelles.
    Embedded,
    /// Document bureautique (Word, Excel, PowerPoint).
    Office,
}

/// Entrée choisie dans le menu `…`.
enum MenuAction {
    /// Ouvre la fenêtre des préférences.
    Preferences,
    /// Annule le dernier pas d'historique.
    Undo,
    /// Rejoue le dernier pas annulé.
    Redo,
    /// Ouvre l'export de texte sur un format précis.
    ExportText(TextFormat),
    /// Ouvre l'export d'images sur un format précis.
    ExportImage(ImageFormat),
    /// Exporte les pages empilées en une seule image.
    ExportLongImage,
    /// Exporte les images incorporées, pas les pages.
    ExportEmbedded,
    /// Ouvre l'export bureautique sur un format précis.
    ExportOffice(OfficeFormat),
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
    /// Ouvre la fenêtre de recadrage.
    Crop,
    /// Ouvre la fenêtre d'arrière-plan de page.
    Background,
    /// Ouvre la fenêtre de numérotation Bates.
    Bates,
    /// Ouvre la fenêtre de comparaison de deux PDF.
    Compare,
    /// Ouvre la fenêtre d'OCR.
    Ocr,
    /// Ouvre la fenêtre de création d'un document.
    Create,
    /// Envoie le document à l'imprimante du système.
    Print,
    /// Révèle le fichier dans le Finder (macOS) ou le gestionnaire de fichiers.
    Reveal,
    /// Enlève la protection par mot de passe (copie déchiffrée).
    RemoveProtection,
    /// Nouveau document d'une page vierge.
    NewBlank,
    /// Nouveau document à partir d'images.
    FromImages,
    /// Insère un autre PDF dans le document courant.
    Combine,
    /// Présentation plein écran, une page à la fois.
    Slideshow,
    /// Ouvre la fenêtre de traitement par lots.
    Batch,
    /// Ouvre la fenêtre d'extraction de données.
    Extract,
    /// Active/désactive les fonctionnalités IA.
    AiEnabled,
    /// Écrit une copie déclarée conforme à un niveau.
    Conformance(Conformance),
    /// Écrit un PDF dont chaque page est une image.
    ImagePdf,
    /// Reconstruit les signets à partir du texte.
    GenerateOutline,
    /// Retire tous les signets.
    ClearOutline,
    /// Change le thème de l'interface.
    Theme(Theme),
}

/// Action demandée depuis le menu contextuel d'un onglet.
enum TabAction {
    Close(usize),
    CloseOthers(usize),
    CloseRight(usize),
    CloseAll,
    TogglePin(usize),
    Duplicate(usize),
    Reveal(usize),
    CopyPath(usize),
}

/// Nouvelle position de l'onglet actif après avoir déplacé l'onglet `from`
/// vers la position `pos` (index dans la liste **après** retrait de `from`).
fn moved_active(active: usize, from: usize, pos: usize) -> usize {
    if active == from {
        return pos;
    }
    let after_removal = if active > from { active - 1 } else { active };
    if after_removal >= pos {
        after_removal + 1
    } else {
        after_removal
    }
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
    ModeTool("Callout", icon_cursor, true, false),
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

/// Écart relatif toléré entre l'échelle rastérisée et l'échelle demandée
/// avant de re-rendre la page, une fois le zoom au repos.
///
/// Sans cela, un zoom qui reste dans le même palier laisserait une image
/// jusqu'à √2 trop molle.
const RENDER_TOLERANCE: f32 = 0.10;

/// Une page déjà rastérisée, avec l'échelle exacte employée.
struct CachedPage {
    texture: TextureHandle,
    /// Échelle réellement employée pour la rastérisation (`logical_scale * ppp`).
    scale: f32,
}

/// Image sélectionnée en mode Édition, avec sa boîte en repère page.
#[derive(Clone, Debug, PartialEq)]
struct SelectedImage {
    /// Page (1-indexée).
    page: usize,
    /// Rang dans la liste `extract_images` de la page.
    index: usize,
    /// Nom de ressource (`SigIm0`…).
    name: String,
    /// Boîte occupée sur la page, repère page.
    rect: [f32; 4],
}

/// Sélection de texte sur une page, au caractère près.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TextSel {
    /// Page concernée.
    page: u32,
    /// Ancre et tête du glissé ; `None` sélectionne tout le texte de la page.
    span: Option<(TextHit, TextHit)>,
}

use youpdf_core::text::geometry::{
    expand_to_line, expand_to_word, hit_quads, hit_rects, hit_runs, hit_selection_text, text_hit,
    TextHit,
};


/// Couleur du fond de page autour d'un bloc, en repère affiché.
///
/// Le masque de réécriture doit se fondre dans la page : blanc sur une page
/// blanche, beige sur une page beige. On échantillonne six points au bord du
/// bloc, à l'écart des glyphes, et on prend la médiane par canal — un point
/// isolé tombé sur un filet ou un glyphe voisin ne décide donc pas seul. Sans
/// bitmap de la page disponible, on répond blanc.
fn mask_color_in(tab: &Tab, page: usize, rect: [f32; 4]) -> [u8; 3] {
    const WHITE: [u8; 3] = [255, 255, 255];
    match tab.last_bitmap.as_ref() {
        Some((bitmap_page, scale, bitmap)) if *bitmap_page == page as u32 => {
            bitmap
                .background_color_around(*scale, rect)
                .unwrap_or(WHITE)
        }
        _ => WHITE,
    }
}

/// Palier de vignette : hors de la pyramide de zoom, une vignette ne doit
/// jamais être servie à la place d'une page rendue.
const THUMBNAIL_STEP: i32 = -1;

/// Délai de repos après le dernier changement de zoom avant de rastériser la
/// page à la finesse demandée.
const ZOOM_SETTLE_SECS: f64 = 0.12;

/// Délai de repos après le dernier défilement avant d'affiner les pages
/// rastérisées à basse résolution.
const SCROLL_SETTLE_SECS: f64 = 0.12;

/// Délai de repos après la dernière frappe dans le champ de recherche avant
/// de relancer la recherche. L'index est réutilisé : seule la correspondance
/// elle-même est refaite, mais on l'épargne à chaque caractère d'un mot.
const SEARCH_SETTLE_SECS: f64 = 0.18;

/// Échelle de rastérisation pendant un geste de défilement. Le coût de
/// PDFium est quadratique : à 0,4× l'image pèse 16 % de la résolution
/// complète, ce qui allège la frame d'un ordre de grandeur. Elle est
/// étirée puis affinée à l'arrêt du geste.
const FAST_RENDER_SCALE: f32 = 0.4;

/// Réserve en haut du canevas pour les barres flottantes (outils, modes) :
/// sans elle, la première page passe sous la barre d'outils. La barre de modes
/// mesure 34 pt et commence 8 pt sous la barre de titre.
const CANVAS_TOP_GAP: f32 = 48.0;

/// Palier de rendu : une pyramide de pas √2 (1×, 1,41×, 2×…).
///
/// Un palier reste valable tant que l'échelle demandée ne s'en écarte pas
/// d'un facteur √2. Entre deux paliers on étire le bitmap voisin, ce qui ne
/// se voit pas une fois filtré, au lieu de rastériser chaque page à chaque
/// cran de molette — c'est ce qui faisait ramer le zoom.
/// Cadre d'affichage d'une page de comparaison : centré dans la place
/// disponible, jamais agrandi au-delà de sa taille réelle.
fn compare_fit_rect(slot: Rect, size: egui::Vec2) -> Rect {
    let scale = (slot.width() / size.x.max(1.0))
        .min(slot.height() / size.y.max(1.0))
        .min(1.0);
    Rect::from_center_size(slot.center(), size * scale)
}

fn render_step(scale: f32, ppp: f32) -> i32 {
    // Décalage constant : les paliers restent positifs et `THUMBNAIL_STEP`
    // (-1) ne peut pas être confondu avec un palier de page.
    ((scale * ppp).max(0.01).log2() * 2.0).round() as i32 + 20
}

/// Entrée du menu `Outils`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolsEntry {
    /// Active un mode du canevas (index dans `MODES`).
    Mode(usize),
    /// Surface « Organiser les pages » (contient aussi le découpage).
    Organize,
    /// Découpage d'une page en tuiles.
    TileSplit,
    /// Fenêtre de recadrage.
    Crop,
    /// Fenêtre de filigrane.
    Watermark,
    /// Fenêtre d'arrière-plan de page.
    Background,
    /// Fenêtre d'en-tête et pied de page.
    HeaderFooter,
    /// Fenêtre de numérotation Bates.
    Bates,
    /// Fenêtre des outils de mesure.
    Measure,
    /// Fenêtre d'OCR.
    Ocr,
    /// Fenêtre d'export et de conversion.
    Convert,
    /// Fenêtre de création d'un document.
    Create,
    /// Fenêtre de comparaison.
    Compare,
    /// Fenêtre de protection.
    Protect,
    /// Fenêtre de compression.
    Compress,
    /// Aplatit les annotations.
    Flatten,
    /// Signature numérique.
    Sign,
    /// Fenêtre de création d'une signature visuelle.
    CreateSignature,
    /// Fenêtre de tampon personnalisé.
    CustomStamp,
}

/// Entrées du menu `Outils` : libellé, raccourci, action.
///
/// L'ordre suit celui de la référence : les modes d'abord, puis les outils de
/// document, puis la conversion et l'entretien.
const TOOLS_MENU: &[(&str, &str, ToolsEntry)] = &[
    ("Comment", "^1", ToolsEntry::Mode(0)),
    ("Edit PDF", "^2", ToolsEntry::Mode(1)),
    ("Prepare form", "^3", ToolsEntry::Mode(2)),
    ("Redact", "^4", ToolsEntry::Mode(3)),
    ("Organize pages", "^5", ToolsEntry::Organize),
    ("Crop pages", "^6", ToolsEntry::Crop),
    ("Split pages", "^7", ToolsEntry::TileSplit),
    ("Watermark", "^8", ToolsEntry::Watermark),
    ("Background", "^9", ToolsEntry::Background),
    ("Header & footer", "^0", ToolsEntry::HeaderFooter),
    ("Bates numbering", "", ToolsEntry::Bates),
    ("Measure", "", ToolsEntry::Measure),
    ("Convert…", "", ToolsEntry::Convert),
    ("OCR…", "", ToolsEntry::Ocr),
    ("Create…", "", ToolsEntry::Create),
    ("Compare PDFs…", "", ToolsEntry::Compare),
    ("Protect…", "", ToolsEntry::Protect),
    ("Reduce file size…", "", ToolsEntry::Compress),
    ("Flatten annotations", "", ToolsEntry::Flatten),
    ("Sign…", "", ToolsEntry::Sign),
    ("Create signature…", "", ToolsEntry::CreateSignature),
    ("Custom stamp…", "", ToolsEntry::CustomStamp),
];

/// Nature du brouillon de texte en cours de saisie.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DraftKind {
    Note,
    FreeText,
    Signature,
    Link,
    Stamp,
}

/// Type de brouillon demandé par l'outil courant.
///
/// L'index de l'outil dépend du mode : le mode Édition n'a pas les mêmes
/// outils que le mode Commentaire, et le même index ne veut pas dire la même
/// chose. `Some(3)` est « Comment » en mode Commentaire et « Lien » en mode
/// Édition.
fn draft_kind_for(mode: Option<usize>, tool: Option<usize>) -> DraftKind {
    if mode == Some(1) {
        return match tool {
            Some(1) => DraftKind::FreeText,
            Some(3) => DraftKind::Link,
            _ => DraftKind::FreeText,
        };
    }
    match tool {
        Some(2) => DraftKind::FreeText,
        Some(3) => DraftKind::Note,
        Some(7) => DraftKind::Link,
        Some(8) => DraftKind::Stamp,
        Some(9) => DraftKind::Signature,
        _ => DraftKind::FreeText,
    }
}

/// Mode de la gomme du crayon.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EraseMode {
    /// Un clic sur un trait le supprime en entier.
    Stroke,
    /// Un glissé découpe les sections de trait qu'il touche.
    Segment,
}

/// Saisie en cours pour une annotation textuelle.
#[allow(dead_code)]
struct TextDraft {
    kind: DraftKind,
    page: usize,
    rect: [f32; 4],
    buffer: String,
    /// Premier affichage : demande le focus clavier.
    fresh: bool,
    caret_at: Option<f32>,
    single_line: bool,
}

/// Édition en cours du texte d'un bloc de la page (mode Édition).
struct EditTextDraft {
    /// Bloc touché, tel qu'extrait du document.
    fragment: TextFragment,
    buffer: String,
    /// Mise en forme du texte réécrit (inspecteur de texte).
    style: TextStyle,
    /// Premier affichage : demande le focus clavier.
    fresh: bool,
    /// Abscisse du clic d'origine, en repère affiché — le même que les `rect`
    /// des fragments, pas le repère page : elle se remet à l'échelle pour
    /// poser le curseur entre les glyphes au premier rendu du champ.
    caret_at: Option<f32>,
    /// Le bloc tenait-il sur une ligne ? Figé à l'ouverture : coller un texte
    /// à saut de ligne dans un champ d'une ligne ne doit pas changer le type
    /// du champ en cours d'édition (Entrée valide, donc).
    single_line: bool,
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
    Button,
    Date,
    Barcode,
}

/// Étape du caviardage : on marque d'abord, on confirme ensuite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RedactPhase {
    /// Les zones sont en cours de marquage.
    Marking,
    /// La confirmation est demandée.
    Confirming,
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
            FormKind::Button => "Button",
            FormKind::Date => "Date field",
            FormKind::Barcode => "Barcode",
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
    /// Glissé d'une mesure (distance, périmètre, aire).
    Measure,
    /// Clic pour poser un callout (3 clics : départ, genou, coin texte).
    Callout,
}

/// Couleurs de préréglage proposées dans la popup.
const PRESET_COLORS: [[u8; 3]; 8] = [
    [255, 235, 59],
    [120, 220, 120],
    [255, 140, 200],
    [255, 170, 60],
    [120, 170, 255],
    [180, 130, 240],
    [230, 80, 80],
    [40, 40, 40],
];

/// Positions proposées pour la numérotation Bates.
const BATES_POSITIONS: [(BatesPosition, &str); 6] = [
    (BatesPosition::TopLeft, "Top left"),
    (BatesPosition::TopCenter, "Top centre"),
    (BatesPosition::TopRight, "Top right"),
    (BatesPosition::BottomLeft, "Bottom left"),
    (BatesPosition::BottomCenter, "Bottom centre"),
    (BatesPosition::BottomRight, "Bottom right"),
];

// --------------------------------------------------------------------------
// Onglets
// --------------------------------------------------------------------------

/// Ce que l'application fera du mot de passe saisi.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PasswordPurpose {
    /// Ouvrir un document protégé.
    Open,
    /// Écrire une copie déchiffrée du fichier.
    RemoveProtection,
}

/// Invite de mot de passe en attente.
struct PasswordPrompt {
    path: PathBuf,
    purpose: PasswordPurpose,
    value: String,
    error: Option<String>,
}

/// État d'un onglet avant une modification.
///
/// Les octets du document ne sont copiés que pour les éditions structurelles
/// (texte, pages, formulaires…). Une édition d'annotations vit uniquement en
/// mémoire et laisse `doc` à `None`.
struct HistoryEntry {
    doc: Option<Vec<u8>>,
    annots: Vec<Annotation>,
}

/// Empreinte rapide des octets d'un document, pour comparer à l'état enregistré.
fn fingerprint(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

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
    /// Bitmaps déjà rendus ; clé `(page, palier)`. Voir `render_step` : les
    /// paliers sont ceux de la pyramide de zoom, `THUMBNAIL_STEP` pour les
    /// vignettes.
    cache: HashMap<(u32, i32), CachedPage>,
    /// Taille d'affichage des pages, indexée par numéro de page. Lire le
    /// MediaBox parcourt l'arbre PDF, et le canevas demande la taille de
    /// *toutes* les pages à chaque trame : sans cache, un document de mille
    /// pages coûte mille parcours d'arbre par image.
    page_sizes: Vec<(f32, f32)>,
    /// Dernière page rastérisée (numéro, échelle, pixels). Sert uniquement à
    /// lire la couleur du fond de page sous un bloc de texte réécrit.
    last_bitmap: Option<(u32, f32, PageBitmap)>,
    /// Modifications non enregistrées.
    dirty: bool,
    /// Modifications qui ne sont pas de simples annotations (texte, champs,
    /// caviardage…). Elles ne s'annulent pas et gardent `dirty` vrai.
    structural_dirty: bool,
    /// Annotations YouPDF en mémoire (chargées à la demande).
    annots: Vec<Annotation>,
    /// Annotations telles qu'écrites sur le disque : sert à savoir si un
    /// retour arrière ramène bien à l'état enregistré.
    baseline_annots: Vec<Annotation>,
    annots_loaded: bool,
    /// Piles d'annulation/rétablissement. Une entrée sans octets ne touche
    /// qu'aux annotations ; une entrée avec octets restaure aussi le document.
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    /// Octets du document au dernier point de restauration connu.
    history_base: Option<Vec<u8>>,
    /// Empreinte des octets enregistrés sur le disque.
    saved_hash: u64,
    /// Plan du document (signets).
    outline: Vec<OutlineItem>,
    /// Champs de formulaire du document.
    forms: Vec<FormField>,    /// Pièces jointes du document.
    attachments: Vec<AttachmentInfo>,
    /// Fragments de texte de chaque page, extraits à la demande pour
    /// l'outil Texte du mode Édition.
    fragments: HashMap<u32, Vec<TextFragment>>,
    /// Vrai quand plan, formulaires et pièces jointes ont été lus.
    structure_loaded: bool,
    /// Onglet épinglé : conserve sa position et affiche une punaise.
    pinned: bool,
    /// Vrai si le fichier sur disque est chiffré (cadenas de la barre d'état).
    protected: bool,
}

impl Tab {
    /// Construit un onglet à partir d'un document déjà ouvert, sans entrer
    /// dans le tableau des onglets. Servira un jour à la vue de comparaison
    /// côte à côte, qui doit afficher un deuxième document sans ajouter
    /// d'onglet visible.
    fn from_document(doc: Document, path: Option<PathBuf>) -> Tab {
        let title = doc.display_name();
        let annots = annots_to_display(&doc, doc.read_annotations());
        let baseline_annots = annots.clone();
        let history_base = doc.to_bytes().ok();
        let saved_hash = history_base.as_deref().map(fingerprint).unwrap_or(0);
        Tab {
            doc,
            path,
            title,
            page: 1,
            zoom: None,
            zoom_fit_page: true,
            cache: HashMap::new(),
            page_sizes: Vec::new(),
            last_bitmap: None,
            dirty: false,
            structural_dirty: false,
            annots,
            baseline_annots,
            annots_loaded: true,
            undo: Vec::new(),
            redo: Vec::new(),
            history_base,
            saved_hash,
            outline: Vec::new(),
            forms: Vec::new(),
            attachments: Vec::new(),
            fragments: HashMap::new(),
            structure_loaded: false,
            pinned: false,
            protected: false,
        }
    }

    fn total(&self) -> usize {
        self.doc.page_count()
    }

    /// Le rendu en cache n'est plus d'actualité : les bitmaps et la couleur de
    /// fond lue jusque-là sont périmés.
    fn invalidate_render(&mut self) {
        self.cache.clear();
        self.last_bitmap = None;
        // La rotation d'une page change sa taille d'affichage.
        self.page_sizes.clear();
    }

    /// Taille d'affichage d'une page, mise en cache. Le cache est reconstruit
    /// dès que le nombre de pages change : les numéros, eux, ont bougé.
    fn page_size_cached(&mut self, page: u32) -> (f32, f32) {
        let total = self.total() as u32;
        if self.page_sizes.len() != total as usize {
            self.page_sizes = (1..=total).map(|p| self.doc.page_size(p)).collect();
        }
        self.page_sizes
            .get((page.max(1) - 1) as usize)
            .copied()
            .unwrap_or((595.0, 842.0))
    }

    /// Empile un état d'annotations avant modification.
    fn record_annots(&mut self, before: Vec<Annotation>) {
        self.undo.push(HistoryEntry {
            doc: None,
            annots: before,
        });
        trim_history(&mut self.undo);
        self.redo.clear();
    }

    /// Le palier demandé manque-t-il, ou est-il trop loin de l'échelle voulue ?
    ///
    /// Sert à décider si la page mérite une rastérisation une fois le zoom au
    /// repos. Pendant un geste, seule la présence du palier compte.
    fn needs_render(&self, page: u32, step: i32, target_scale: f32) -> bool {
        match self.cache.get(&(page, step)) {
            None => true,
            Some(cached) => {
                target_scale > 0.0
                    && (cached.scale - target_scale).abs() / target_scale > RENDER_TOLERANCE
            }
        }
    }
}

// --------------------------------------------------------------------------
// Application
// --------------------------------------------------------------------------

/// Kind de contenu : texte ou image. Détermine l'outil qui s'applique.
///
/// Les variantes appartiennent au modèle de l'éditeur de contenu : elles sont
/// réservées pour la sélection des éléments, pas encore construites.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
enum ContentKind {
    Text,
    Image,
}

/// Element de contenu (texte ou image) sélectionné en mode Édition : le
/// cadre, les poignées de redimensionnement et de rotation.
///
/// Fondation de l'éditeur de contenu : le type est réservé, il n'est pas
/// encore construit.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
struct ContentElement {
    kind: ContentKind,
    bounds_screen: Rect, // boîte en repère affiché (canvas)
    rect_page: [f32; 4], // boîte en repère page
    // Pour image seulement : matrice qui dessine l'image
    matrix: [f32; 6],
}

/// Opérations en cours sur un élément de contenu (déplacement, redimensionnement,
/// rotation). Le document n'est écrit qu'au relâchement du glissé.
///
/// Fondation de l'éditeur de contenu : les variantes sont réservées, elles
/// ne sont pas encore construites.
#[allow(dead_code)]
#[derive(Clone, Debug)]
enum ContentDrag {
    Move { start: [f32; 2], offset: [f32; 2] },
    Rotate {
        /// Dernier angle curseur-centre, en degrés.
        last_angle: f32,
        /// Rotation totale du geste, en degrés horaires.
        accumulated: f32,
    },
}

/// Le curseur couramment utilisé par l'éditeur de texte.
#[allow(dead_code)]
const TEXT_CURSOR: CursorIcon = CursorIcon::Text;

/// Callout en cours de construction.
///
/// L'outil Callout se pose en trois clics : le départ de la flèche, son
/// genou, puis le coin du texte. Chaque clic renseigne un point ; les
/// points déjà posés sont conservés entre deux trames.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CalloutPending {
    /// Page visée.
    page: usize,
    /// Départ de la flèche.
    start: [f32; 2],
    /// Genou de la flèche.
    knee: Option<[f32; 2]>,
    /// Coin du texte.
    text_corner: Option<[f32; 2]>,
}

pub struct YouPdfApp {
    renderer: Box<dyn Renderer>,
    renderer_label: &'static str,

    tabs: Vec<Tab>,
    active: usize,

    panel: Option<Panel>,
    mode: Option<usize>,
    /// Cible d'inspecteur flottant (texte, image, annotation, formulaire).
    inspector_target: Option<InspectorTarget>,
    /// Taille mesurée du dernier inspecteur affiché, pour le cadrer.
    inspector_size: Option<egui::Vec2>,
    /// État des contrôles de texte (palette ouverte, saisie hexa).
    inspector_text_ui: TextStyleUi,
    /// Image sélectionnée en mode Édition.
    selected_image: Option<SelectedImage>,
    /// Rognage en cours sur l'image sélectionnée.
    image_crop: Option<ImageCrop>,
    /// Élément de contenu sélectionné en mode Édition (bloc de texte ou
    /// image) : cadre, poignées de redimensionnement et de rotation.
    content_selection: Option<ContentElement>,
    /// Geste en cours sur un élément de contenu (déplacement, redimensionnement,
    /// rotation). Le document n'est écrit qu'au relâchement du glissé.
    content_drag: Option<ContentDrag>,
    /// Opacité affichée par le curseur de l'inspecteur d'image.
    image_alpha: f32,
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
    /// Nature de l'export : texte, images ou document bureautique.
    export_mode: ExportMode,
    /// Format d'image choisi.
    export_image_format: ImageFormat,
    /// Résolution d'export, en points par pouce.
    export_dpi: f32,
    /// Format bureautique choisi.
    export_office_format: OfficeFormat,
    /// Word : placer chaque fragment à ses coordonnées d'origine.
    export_office_exact: bool,
    /// Fenêtre « Compresser » : dernier bilan affiché.
    compress_open: bool,
    sign_open: bool,
    /// Fenêtre « Signer » : certificat chargé, apparence, validation.
    sign: sign_ui::SignUi,
    /// Fenêtres « Tampon personnalisé » et « Créer une signature ».
    marks: marks_ui::MarksUi,
    compress_report: Option<String>,
    /// Niveau de compression d'image choisi.
    compress_level: CompressionLevel,
    /// Optimisations structurelles de la compression.
    compress_extras: CompressionExtras,
    /// Estimation (avant, après) de la dernière simulation.
    compress_estimate: Option<(usize, usize)>,
    /// Fenêtre « Protéger » et ses réglages.
    protect_open: bool,
    protect: EncryptionOptions,
    /// Fenêtre « Recadrage » et ses marges.
    crop_open: bool,
    crop: CropMargins,
    /// Unité d'affichage des marges.
    crop_unit: Unit,
    /// Pages visées : 0 toutes, 1 courante, 2 impaires, 3 paires.
    crop_range: u8,
    /// Boîte écrite : 0 CropBox, 1 ArtBox, 2 BleedBox, 3 TrimBox.
    crop_target: u8,
    /// Bandeau « scan détecté », proposé à l'ouverture.
    scan_banner: bool,
    /// L'utilisateur ne veut plus voir le bandeau pendant cette session.
    scan_banner_off: bool,
    /// Fenêtre « Préférences » (⌘,).
    prefs_open: bool,
    /// Réglages en cours d'édition dans la fenêtre « Préférences ».
    ///
    /// La fenêtre est redessinée chaque frame : sans cette copie, les
    /// modifications seraient recréées à l'identique à chaque frame et ne
    /// survivraient jamais jusqu'au bouton Save.
    prefs_draft: Option<youpdf_ai::AppSettings>,
    /// Instant du dernier enregistrement automatique, en secondes egui.
    autosave_at: f64,
    /// Fenêtre de découpage en tuiles.
    tile_split_open: bool,
    tile_rows: usize,
    tile_cols: usize,
    /// Blanc laissé entre les tuiles, en points.
    tile_gap: f32,
    /// Fenêtre « Arrière-plan » et son brouillon.
    background_open: bool,
    background: Background,
    background_from: Option<u32>,
    /// Fenêtre « Numérotation Bates » et son brouillon.
    bates_open: bool,
    bates: Bates,
    bates_from: Option<u32>,
    /// Fenêtre « Comparer » : fichier choisi et rapport affiché.
    compare_open: bool,
    compare_path: Option<PathBuf>,
    /// Affichage de la comparaison : rapport texte, ou deux documents côte à côte.
    compare_view: CompareView,
    /// Onglet temporaire contenant le document choisi, pour la vue côte à côte.
    /// Il n'apparaît pas dans la barre d'onglets.
    compare_tab: Option<Tab>,
    /// Page courante dans la vue côte à côte (1-indexée).
    compare_page: usize,
    /// Options de comparaison : ignorer les espaces.
    compare_ignore_ws: bool,
    /// Options de comparaison : ignorer la casse.
    compare_ignore_case: bool,
    /// Opacité du second document dans la vue superposée, de 0 à 1.
    compare_blend: f32,
    /// Callout en cours : les points déjà posés par les clics successifs.
    /// L'outil prend trois clics (départ, genou, coin du texte) : l'état
    /// doit survivre d'une trame à l'autre, sinon chaque clic repart de zéro.
    callout_pending: Option<CalloutPending>,
    compare_report: Option<ComparisonReport>,
    /// Fenêtre « OCR » et ses réglages.
    ocr_open: bool,
    /// Langue OCR courante, telle que tesseract la nomme.
    ocr_language: String,
    /// Langues additionnelles cochées, pour un document multilingue.
    ocr_extra_languages: Vec<String>,
    ocr_mode: OcrMode,
    ocr_report: Option<String>,
    /// Vrai quand les langues OCR ont déjà été lues.
    ocr_probed: bool,
    /// Vrai quand tesseract est installé.
    ocr_installed: bool,
    /// Langues proposées par tesseract, lues une fois.
    ocr_languages: Vec<String>,
    /// Fenêtre « Créer » : nombre de pages vierges.
    create_open: bool,
    create_pages: usize,
    /// Invite de mot de passe en attente (ouverture ou déprotection).
    password_prompt: Option<PasswordPrompt>,
    /// Outil de mesure actif : le canevas trace des mesures au glissé.
    measure_active: bool,
    /// Fenêtre « Mesure ».
    measure_open: bool,
    /// Nature de la mesure posée.
    measure_kind: MeasureKind,
    /// Échelle : unités de dessin par unité réelle.
    measure_scale: f32,
    /// Unité affichée.
    measure_unit: String,
    /// Distance réelle saisie pour l'étalonnage.
    measure_known: f32,
    /// Longueur en points du dernier tracé, base de l'étalonnage.
    measure_last_len: Option<f32>,
    /// Fenêtre « Extraire les données » et son résultat.
    extract_open: bool,
    extract_tables: Vec<ExtractedTable>,
    extract_pairs: Vec<KeyValue>,
    /// Thème choisi, et celui déjà appliqué à egui.
    theme: Theme,
    applied_theme: Theme,
    /// Présentation plein écran, une page à la fois.
    slideshow: bool,
    /// Fenêtre « Traitement par lots ».
    batch_open: bool,
    /// File d'attente du lot : un état par fichier, plus ce que la ligne
    /// affiche.
    batch_queue: Vec<BatchEntry>,
    batch_dir: Option<PathBuf>,
    batch_text: bool,
    batch_images: bool,
    batch_compress: bool,
    batch_protect: bool,
    batch_flatten: bool,
    batch_report: String,
    /// Format Office de conversion du lot (`None` : pas de conversion).
    batch_convert: Option<OfficeFormat>,
    /// Pages à retirer de chaque fichier (« 1-2, 5 »).
    batch_remove_pages: String,
    /// Texte du filigrane appliqué au lot.
    batch_watermark_text: String,
    /// Texte centré d'en-tête ou de pied de page appliqué au lot.
    batch_header_text: String,
    /// Préfixe Bates, appliqué en continu sur tout le lot.
    batch_bates_prefix: String,
    batch_bates_start: u32,
    /// Fusionner tous les fichiers en un seul document.
    batch_combine: bool,
    /// Préfixe et suffixe ajoutés au nom de chaque sortie.
    batch_prefix: String,
    batch_suffix: String,
    /// Envoyer chaque fichier à l'imprimante système.
    batch_print: bool,
    /// PDF dont les pages sont insérées dans chaque fichier.
    batch_insert: Option<PathBuf>,
    /// Position d'insertion : 0 = en tête, `usize::MAX` = en fin.
    batch_insert_at: usize,
    /// Reconnaître le texte de chaque fichier (OCR).
    batch_ocr: bool,
    /// Appliquer l'arrière-plan courant à chaque fichier.
    batch_background: bool,
    /// Lignes du lot en cours, envoyées par le fil de travail.
    batch_rx: Option<std::sync::mpsc::Receiver<BatchEvent>>,
    /// Vrai tant qu'un lot tourne.
    batch_running: bool,
    /// Consigne d'arrêt lue par le fil de travail à chaque fichier.
    batch_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Instant de départ du lot : sert à mesurer le rythme et l'ETA.
    batch_started: Option<std::time::Instant>,
    /// Fin estimée, calculée au fil de l'avancement.
    batch_eta: Option<std::time::Duration>,
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
    /// Sélection de texte en cours (outil Sélection du mode Commentaire).
    text_sel: Option<TextSel>,
    /// Tracé de crayon en cours (coordonnées PDF).
    ink_current: Vec<[f32; 2]>,
    /// Mode de la gomme : trait entier ou segment.
    eraser_mode: EraseMode,
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
    /// Inclure les titres de signets dans les résultats.
    search_bookmarks: bool,
    /// Inclure le texte des commentaires dans les résultats.
    search_comments: bool,
    /// Saisie de la page à atteindre (pastille bas-droite).
    page_entry: String,
    /// Surface d'accueil (fichiers récents) affichée à la place du canevas.
    home_open: bool,
    /// Onglet en cours de glissé dans la barre de titre (réordonnancement).
    dragging_tab: Option<usize>,
    /// Onglet survolé pendant le glissé : cible du dépôt.
    drag_target: Option<usize>,
    /// État de l'écran d'accueil (filtre, onglet, grille/liste).
    home: HomeView,
    /// Fichiers marqués d'une étoile (persistés dans `~/.youpdf/starred.txt`).
    stars: Vec<PathBuf>,
    /// Vignettes de l'écran d'accueil, par chemin : `(mtime, texture)`.
    home_thumbs: HashMap<PathBuf, (u64, TextureHandle)>,
    /// Panneau IA (« apportez votre propre fournisseur »).
    ai: AiPanel,
    /// Onglet dont le texte est mis en cache pour l'IA (`None` = à extraire).
    ai_doc_tab: Option<usize>,
    /// Texte du document actif, extrait à l'ouverture du panneau IA.
    ai_doc_text: String,
    /// Disposition des pages du canevas (une, continue, deux, couverture).
    /// Largeur courante du panneau latéral (redimensionnable, 180–450 pt).
    panel_w: f32,
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
    /// Ancre du dernier clic (Maj = plage).
    organize_anchor: Option<usize>,
    /// Page en cours de glissement.
    organize_drag: Option<usize>,
    /// Fenêtre d'action ouverte.
    organize_dialog: Option<organize_ui::OrganizeDialog>,
    /// Suppression en attente de confirmation : les pages visées.
    ///
    /// Supprimer des pages casse le document sans retour possible autrement
    /// que par Annuler ; on demande donc avant, comme la fermeture d'un
    /// onglet modifié.
    organize_pending_delete: Option<Vec<usize>>,
    /// Extraction : un seul fichier, et suppression des pages d'origine.
    organize_extract_single: bool,
    organize_extract_delete: bool,
    /// Insertion : position (0 avant, 1 après, 2 début, 3 fin) et page.
    organize_insert_where: u8,
    organize_insert_page: usize,
    /// Remplacement : plage visée.
    organize_replace_from: usize,
    organize_replace_to: usize,
    /// Découpe : 0 pages, 1 taille, 2 signets ; taille max en Mo.
    organize_split_mode: u8,
    organize_split_mb: f32,
    /// Ligne « remplacer » ouverte.
    replace_open: bool,
    /// Texte de remplacement.
    replace_text: String,
    /// Régions marquées pour le caviardage, avec leur apparence.
    redact_marks: Vec<RedactionMark>,
    /// Étape courante du caviardage : marquage, puis confirmation.
    redact_phase: RedactPhase,
    /// Remplissage retenu pour les nouvelles marques.
    redact_fill: RedactionFill,
    /// Remplissage personnalisé (RGB).
    redact_custom_color: [u8; 3],
    /// Texte superposé aux zones caviardées, vide si l'on n'en veut pas.
    redact_overlay_text: String,
    /// Police du texte superposé.
    redact_overlay_font: PatchFont,
    /// Corps du texte superposé, en points.
    redact_overlay_size: f32,
    /// Fenêtre de confirmation du caviardage.
    redact_modal: bool,
    /// Bloc de texte en cours de réécriture (mode Édition).
    edit_text: Option<EditTextDraft>,
    /// Nettoyer les métadonnées en même temps que le caviardage.
    redact_sanitize: bool,
    /// Champ de formulaire sélectionné et saisie en cours.
    form_edit: Option<(usize, String)>,
    /// Brouillon des propriétés du champ sélectionné.
    form_props: Option<(usize, FieldProperties)>,
    /// Onglet actif de l'inspecteur de champ.
    form_props_tab: usize,
    /// Champ de formulaire en cours de édition interactive (lecteur).
    form_editing: Option<usize>,
    /// Dernier champ survolé pour surlignage.
    form_hover: Option<usize>,
    /// Teinte bleue sur tous les champs remplissables.
    form_highlight: bool,
    /// Champs sélectionnés dans le mode Préparer un formulaire.
    form_selection: BTreeSet<usize>,
    /// Format choisi pour l'échange de données de formulaire.
    form_data_format: FormDataFormat,
    /// Fond du canevas choisi dans Aspect.
    canvas_style: usize,
    /// Ombre portée sous les pages.
    page_shadow: bool,
    /// Teinte d'affichage des pages : indice dans `PAGE_TINTS` (0 = aucune).
    page_tint: usize,
    /// Filet de 1 pt autour de chaque page.
    page_border: bool,
    /// Échelle des vignettes du panneau Aspect.
    thumb_scale: f32,
    /// Largeur d'une vignette du panneau Pages, en points (80 à 240).
    thumb_pt: f32,
    /// Pages cochées dans le panneau Pages.
    thumb_selected: BTreeSet<usize>,
    /// Onglet auquel appartient `thumb_selected` : la sélection est vidée dès
    /// qu'on change de document, sinon les mêmes numéros viseraient d'autres
    /// pages.
    thumb_owner: usize,
    /// Page en cours de glissé dans la liste des vignettes.
    thumb_drag: Option<usize>,
    /// Page dont la fenêtre « Propriétés de la page » est ouverte.
    page_props: Option<usize>,
    /// Signets repliés, par clé de chemin (« 0/2 »).
    bookmark_collapsed: BTreeSet<String>,
    /// Signet en cours de glissé, par clé de chemin.
    bookmark_drag: Option<String>,
    /// Signet dont le titre s'édite en place, par clé de chemin.
    bookmark_rename: Option<String>,
    /// Contenu de l'édition en place.
    bookmark_rename_text: String,
    /// Vrai quand l'édition en place vient de s'ouvrir : demande le focus.
    bookmark_rename_focus: bool,
    /// Recherche plein texte de la liste des commentaires.
    comment_search: String,
    /// Filtre par type, par auteur, par couleur et par état de revue.
    comment_kind: Option<AnnotationKind>,
    comment_author: Option<String>,
    comment_color: Option<[u8; 3]>,
    comment_state: CommentStateFilter,
    /// Tri de la liste.
    comment_sort: CommentSort,
    /// Commentaire auquel on répond : identifiant du parent et texte saisi.
    comment_reply: Option<String>,
    comment_reply_text: String,
    /// Texte cité sous chaque surlignage, calculé une fois par document.
    comment_quotes: HashMap<String, String>,
    /// Onglet auquel appartient `comment_quotes`.
    comment_quote_owner: usize,

    search_query: String,
    search_index: Option<(usize, SearchIndex)>,
    results: Vec<SearchResult>,
    history: Vec<String>,
    /// Instant de la dernière frappe dans le champ de recherche (voir
    /// `SEARCH_SETTLE_SECS`) : la recherche part au repos, pas à chaque
    /// caractère.
    search_typed_at: Option<f64>,

    /// Message éphémère : (texte, date d'apparition).
    toast: (String, f64),
    /// Dernière erreur de rendu (remontée en toast).
    render_status: String,
    /// Demande de défilement vers une page (vignette, recherche).
    jump: Option<usize>,
    /// Dernière échelle « ajuster à la largeur » : base des pas de zoom.
    last_fit: f32,
    /// Un réglage de zoom vient d'être demandé : le canevas horodate le
    /// geste et diffère le rendu net tant que l'utilisateur bouge encore.
    zoom_dirty: bool,
    /// Instant du dernier changement de zoom (voir `ZOOM_SETTLE_SECS`).
    zoom_moved_at: f64,
    /// Instant du dernier défilement (voir `SCROLL_SETTLE_SECS`).
    scroll_moved_at: f64,
    /// Position écran de la première page à la frame précédente : sa
    /// variation dit que le contenu a glissé.
    last_page_y: Option<f32>,
}

impl YouPdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        crate::fonts::install(&cc.egui_ctx);
        // La configuration est lue une seule fois : le thème en vient, et le
        // panneau IA reçoit le même objet au lieu de relire le fichier.
        let settings = youpdf_ai::AppSettings::load().unwrap_or_default();
        i18n::set_language(Language::from_code(&settings.locale));
        let theme = settings.theme();
        apply_theme(&cc.egui_ctx, theme);

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
            inspector_target: None,
            content_selection: None,
            content_drag: None,
            form_editing: None,
            form_hover: None,
            form_props_tab: 0,
            form_highlight: true,
            form_selection: BTreeSet::new(),
            inspector_size: None,
            inspector_text_ui: TextStyleUi::default(),
            selected_image: None,
            image_crop: None,
            image_alpha: 1.0,
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
            export_mode: ExportMode::Text,
            export_image_format: ImageFormat::Png,
            export_dpi: 150.0,
            export_office_format: OfficeFormat::Word,
            export_office_exact: false,
            compress_open: false,
            sign_open: false,
            sign: sign_ui::SignUi::default(),
            marks: marks_ui::MarksUi::default(),
            compress_report: None,
            compress_level: CompressionLevel::Ebook,
            compress_extras: CompressionExtras::default(),
            compress_estimate: None,
            protect_open: false,
            protect: EncryptionOptions::default(),
            crop_open: false,
            crop: CropMargins::default(),
            crop_unit: Unit::Point,
            crop_range: 0,
            crop_target: 0,
            prefs_open: false,
            prefs_draft: None,
            autosave_at: 0.0,
            scan_banner: false,
            scan_banner_off: false,
            tile_split_open: false,
            tile_rows: 2,
            tile_cols: 2,
            tile_gap: 0.0,
            background_open: false,
            background: Background::default(),
            background_from: None,
            bates_open: false,
            bates: Bates::default(),
            bates_from: None,
            compare_open: false,
            compare_path: None,
            compare_view: CompareView::default(),
            compare_tab: None,
            compare_page: 1,
            compare_ignore_ws: false,
            compare_ignore_case: false,
            compare_blend: 0.5,
            compare_report: None,
            ocr_open: false,
            ocr_language: "eng".to_owned(),
            ocr_extra_languages: Vec::new(),
            ocr_mode: OcrMode::default(),
            ocr_report: None,
            ocr_probed: false,
            ocr_installed: false,
            ocr_languages: Vec::new(),
            callout_pending: None,
            create_open: false,
            create_pages: 1,
            password_prompt: None,
            measure_active: false,
            measure_open: false,
            measure_kind: MeasureKind::Distance,
            measure_scale: 72.0,
            measure_unit: "in".to_owned(),
            measure_known: 1.0,
            measure_last_len: None,
            extract_open: false,
            extract_tables: Vec::new(),
            extract_pairs: Vec::new(),
            theme,
            applied_theme: theme,
            slideshow: false,
            batch_open: false,
            batch_queue: Vec::new(),
            batch_dir: None,
            batch_text: false,
            batch_images: false,
            batch_compress: true,
            batch_protect: false,
            batch_flatten: false,
            batch_report: String::new(),
            batch_convert: None,
            batch_remove_pages: String::new(),
            batch_watermark_text: String::new(),
            batch_header_text: String::new(),
            batch_bates_prefix: "DOC-".to_string(),
            batch_bates_start: 1,
            batch_combine: false,
            batch_prefix: String::new(),
            batch_suffix: String::new(),
            batch_print: false,
            batch_insert: None,
            batch_insert_at: 0,
            batch_ocr: false,
            batch_background: false,
            batch_rx: None,
            batch_running: false,
            batch_cancel: std::sync::Arc::default(),
            batch_started: None,
            batch_eta: None,
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
            text_sel: None,
            ink_current: Vec::new(),
            eraser_mode: EraseMode::Stroke,
            drag_from: None,
            drag_to: None,
            stamp_choice: 0,
            draft_annot_kind: AnnotationKind::FreeText,
            search_active: None,
            search_rects: Vec::new(),
            search_rects_page: None,
            search_case: false,
            search_word: false,
            search_bookmarks: false,
            search_comments: false,
            page_entry: String::new(),
            home_open: initial.is_none(),
            dragging_tab: None,
            drag_target: None,
            home: HomeView::default(),
            stars: home::load_stars(),
            home_thumbs: HashMap::new(),
            ai: AiPanel::with_settings(settings),
            ai_doc_tab: None,
            ai_doc_text: String::new(),
            page_layout: PageLayout::default(),
            panel_w: PANEL_W,
            fullscreen: false,
            actions_opened: false,
            activity: Vec::new(),
            activity_open: false,
            recents: load_recents(),
            organize_open: false,
            organize_selected: BTreeSet::new(),
            organize_split: 1,
            organize_anchor: None,
            organize_drag: None,
            organize_dialog: None,
            organize_pending_delete: None,
            organize_extract_single: true,
            organize_extract_delete: false,
            organize_insert_where: 1,
            organize_insert_page: 1,
            organize_replace_from: 1,
            organize_replace_to: 1,
            organize_split_mode: 0,
            organize_split_mb: 5.0,
            replace_open: false,
            replace_text: String::new(),
            redact_marks: Vec::new(),
            redact_phase: RedactPhase::Marking,
            redact_fill: RedactionFill::default(),
            redact_custom_color: [0, 0, 0],
            redact_overlay_text: String::new(),
            redact_overlay_font: PatchFont::Helvetica,
            redact_overlay_size: 12.0,
            redact_modal: false,
            edit_text: None,
            redact_sanitize: true,
            form_edit: None,
            form_props: None,
            form_data_format: FormDataFormat::Fdf,
            canvas_style: 0,
            page_shadow: true,
            page_tint: 0,
            page_border: false,
            thumb_scale: 1.0,
            thumb_pt: 120.0,
            thumb_selected: BTreeSet::new(),
            thumb_owner: 0,
            thumb_drag: None,
            page_props: None,
            bookmark_collapsed: BTreeSet::new(),
            bookmark_drag: None,
            bookmark_rename: None,
            bookmark_rename_text: String::new(),
            bookmark_rename_focus: false,
            comment_search: String::new(),
            comment_kind: None,
            comment_author: None,
            comment_color: None,
            comment_state: CommentStateFilter::All,
            comment_sort: CommentSort::Page,
            comment_reply: None,
            comment_reply_text: String::new(),
            comment_quotes: HashMap::new(),
            comment_quote_owner: usize::MAX,
            search_query: String::new(),
            search_index: None,
            results: Vec::new(),
            history: Vec::new(),
            search_typed_at: None,
            toast: (String::new(), f64::NEG_INFINITY),
            render_status: String::new(),
            jump: None,
            last_fit: 1.0,
            zoom_dirty: false,
            zoom_moved_at: 0.0,
            scroll_moved_at: 0.0,
            last_page_y: None,
        };

        // Les préférences vivent dans le panneau IA, qui tient déjà la
        // configuration chargée : une seule lecture du fichier.
        let prefs = app.ai.settings.clone();
        app.page_layout = PageLayout::ALL
            .get(prefs.default_layout)
            .copied()
            .unwrap_or_default();
        if !prefs.author.trim().is_empty() {
            app.comment_author = Some(prefs.author.clone());
        }
        if !prefs.measure_unit.trim().is_empty() {
            app.measure_unit = prefs.measure_unit.clone();
        }
        if initial.is_none() && prefs.startup == "blank" {
            // Démarrage sur une page vierge : l'écran d'accueil doit rester
            // fermé, y compris si le document ne peut pas être créé.
            app.home_open = false;
            if let Ok(doc) = youpdf_core::blank_document(1, 595.0, 842.0) {
                app.push_document(doc, None);
            }
        }

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

    /// Affiche une fenêtre modale centrée, avec le chrome commun à tous les
    /// dialogues : non repliable, non redimensionnable, ancrée au centre de
    /// l'écran, fond `chrome_bg` et coins arrondis. Le contenu est dessiné par
    /// `add_contents`, qui reçoit `self` pour pouvoir lire et modifier l'état.
    ///
    /// `max_width == 0.0` laisse la fenêtre s'ajuster à son contenu.
    ///
    /// Les surfaces volontairement redimensionnables (organiser les pages,
    /// comparer, extraire, lot) gardent leur propre `egui::Window` : forcer
    /// leur taille les casserait.
    fn show_modal<R>(
        &mut self,
        ctx: &egui::Context,
        id_source: impl std::hash::Hash,
        title: &str,
        max_width: f32,
        open: &mut bool,
        add_contents: impl FnOnce(&mut Self, &mut egui::Ui) -> R,
    ) -> Option<R> {
        let mut result = None;
        egui::Window::new(title)
            .id(Id::new(id_source))
            .open(open)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .frame(modal_frame(ctx))
            .show(ctx, |ui| {
                if max_width > 0.0 {
                    ui.set_max_width(max_width);
                }
                result = Some(add_contents(self, ui));
            });
        result
    }

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
    fn open_path_new_tab(&mut self, path: PathBuf) {
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

    /// Ajoute un document déjà ouvert comme nouvel onglet actif.
    ///
    /// Les annotations YouPDF sortent du document pour la durée de l'édition :
    /// l'interface les dessine, l'onglet les garde, l'enregistrement les remet
    /// en place avec leur apparence. Sans cette purge, le moteur de rendu les
    /// peindrait en même temps que l'interface et chaque marque apparaîtrait
    /// deux fois.
    fn push_document(&mut self, mut doc: Document, path: Option<PathBuf>) {
        let title = doc.display_name();
        let (initial_zoom, fit_page) = match self.ai.settings.default_zoom.as_str() {
            "fit-page" => (None, true),
            "actual" => (Some(1.0), false),
            "fit-width" => (None, false),
            other => match other.trim_end_matches('%').parse::<f32>() {
                Ok(percent) if percent > 0.0 => (Some(percent / 100.0), false),
                _ => (None, false),
            },
        };
        let protected = path
            .as_deref()
            .map(Document::file_is_encrypted)
            .unwrap_or(false);
        // Un scan se reconnaît avant que le document n'entre dans l'onglet.
        let scanned = doc.looks_scanned(1);
        let annots = annots_to_display(&doc, doc.read_annotations());
        if !annots.is_empty() && doc.clear_youpdf_annotations().is_err() {
            tracing::warn!("purge des annotations impossible");
        }
        let baseline_annots = annots.clone();
        let history_base = doc.to_bytes().ok();
        let saved_hash = history_base.as_deref().map(fingerprint).unwrap_or(0);
        self.tabs.push(Tab {
            doc,
            path: path.clone(),
            title: title.clone(),
            page: 1,
            zoom: initial_zoom,
            zoom_fit_page: fit_page,
            cache: HashMap::new(),
            page_sizes: Vec::new(),
            last_bitmap: None,
            dirty: false,
            structural_dirty: false,
            annots,
            baseline_annots,
            annots_loaded: true,
            undo: Vec::new(),
            redo: Vec::new(),
            history_base,
            saved_hash,
            outline: Vec::new(),
            forms: Vec::new(),
            attachments: Vec::new(),
            fragments: HashMap::new(),
            structure_loaded: false,
            pinned: false,
            protected,
        });
        self.active = self.tabs.len() - 1;
        self.scan_banner = scanned && !self.scan_banner_off;
        self.panel = Some(Panel::Thumbnails);
        self.home_open = false;
        // Le nouvel onglet devient actif tout de suite : sans cela, la trame
        // courante réutilise la session du document précédent.
        self.last_active = self.active;
        self.reset_document_session();
        if let Some(path) = path {
            self.recents.retain(|old| old != &path);
            self.recents.insert(0, path);
            self.recents.truncate(8);
            save_recents(&self.recents);
        }
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

    /// Déplace l'onglet `from` à la position `to` (glissé dans la barre).
    ///
    /// `self.active` suit le même document : il est décalé pour le retrait,
    /// puis pour l'insertion, ce qui évite qu'un glissé change l'onglet affiché.
    fn move_tab(&mut self, from: usize, to: usize) {
        if from == to || from >= self.tabs.len() {
            return;
        }
        let after = self.tabs.len() - 1;
        let tab = self.tabs.remove(from);
        let pos = to.min(after);
        self.tabs.insert(pos, tab);
        self.active = moved_active(self.active, from, pos);
        self.last_active = self.active;
    }

    /// Ferme une liste d'onglets.
    ///
    /// Un onglet modifié exige une confirmation, et `pending_close` n'en porte
    /// qu'une : dès qu'un onglet ciblé est modifié, on refuse le lot et on
    /// laisse l'utilisateur les fermer un par un. Cela évite de fermer des
    /// index décalés par les retraits.
    fn close_many(&mut self, ctx: &egui::Context, targets: &[usize]) {
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

    /// Applique une action du menu contextuel d'un onglet.
    fn apply_tab_action(&mut self, ctx: &egui::Context, action: TabAction) {
        let count = self.tabs.len();
        match action {
            TabAction::Close(idx) => self.close_tab(idx),
            TabAction::CloseOthers(keep) => {
                let targets: Vec<usize> = (0..count).filter(|idx| *idx != keep).collect();
                self.close_many(ctx, &targets);
            }
            TabAction::CloseRight(from) => {
                let targets: Vec<usize> = (from + 1..count).collect();
                self.close_many(ctx, &targets);
            }
            TabAction::CloseAll => {
                let targets: Vec<usize> = (0..count).collect();
                self.close_many(ctx, &targets);
            }
            TabAction::TogglePin(idx) => {
                if let Some(tab) = self.tabs.get_mut(idx) {
                    tab.pinned = !tab.pinned;
                }
            }
            TabAction::Duplicate(idx) => {
                let path = self.tabs.get(idx).and_then(|tab| tab.path.clone());
                match path {
                    Some(path) => self.open_path_new_tab(path),
                    None => self.toast(ctx, "This document has no file on disk yet"),
                }
            }
            TabAction::Reveal(idx) => {
                let path = self.tabs.get(idx).and_then(|tab| tab.path.clone());
                match path {
                    Some(path) => self.reveal_path(&path),
                    None => self.toast(ctx, "This document has no file on disk yet"),
                }
            }
            TabAction::CopyPath(idx) => {
                let path = self.tabs.get(idx).and_then(|tab| tab.path.clone());
                match path {
                    Some(path) => ctx.copy_text(path.display().to_string()),
                    None => self.toast(ctx, "This document has no file on disk yet"),
                }
            }
        }
    }

    /// Tout ce qui appartient à la session ouverte sur un document : marques
    /// de caviardage, champ de formulaire, sélection, pages cochées,
    /// résultats de recherche. Changer d'onglet doit les remettre à zéro,
    /// sinon la suite de l'action s'applique au mauvais fichier.
    fn reset_document_session(&mut self) {
        self.redact_marks.clear();
        self.redact_phase = RedactPhase::Marking;
        self.callout_pending = None;
        self.form_edit = None;
        self.selected_annot = 0;
        self.has_annot_selection = false;
        self.text_sel = None;
        self.inspector_target = None;
        self.inspector_size = None;
        self.inspector_text_ui = TextStyleUi::default();
        self.selected_image = None;
        self.image_crop = None;
        self.content_selection = None;
        self.content_drag = None;
        self.image_alpha = 1.0;
        self.organize_selected.clear();
        // Les clés de chemin des signets sont les mêmes d'un document à
        // l'autre : sans ce nettoyage, le nouveau plan s'ouvrirait en édition
        // avec le titre de l'ancien.
        self.bookmark_rename = None;
        self.bookmark_rename_text.clear();
        self.bookmark_rename_focus = false;
        self.bookmark_drag = None;
        self.bookmark_collapsed.clear();
        // La liste de commentaires repart de zéro : filtres, tri, citation et
        // réponse en cours visent tous le document précédent.
        self.comment_search.clear();
        self.comment_kind = None;
        self.comment_author = None;
        self.comment_color = None;
        self.comment_state = CommentStateFilter::All;
        self.comment_sort = CommentSort::Page;
        self.comment_reply = None;
        self.comment_reply_text.clear();
        self.comment_quotes.clear();
        self.comment_quote_owner = usize::MAX;
        self.thumb_owner = usize::MAX;
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
        // Le contexte IA appartient au document : on le ré-extraira.
        self.ai_doc_tab = None;
        self.ai_doc_text.clear();
        // Les résultats d'extraction et l'outil de mesure appartiennent au
        // document : les garder ferait agir la fenêtre sur le mauvais onglet.
        self.extract_tables.clear();
        self.extract_pairs.clear();
        self.measure_active = false;
        self.batch_report.clear();
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

    // -- Annotations -------------------------------------------------------

    /// Charge les annotations du document à la première utilisation.
    fn ensure_annots(&mut self) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if !tab.annots_loaded {
                tab.annots = annots_to_display(&tab.doc, tab.doc.read_annotations());
                tab.annots_loaded = true;
            }
        }
    }

    fn undo(&mut self, ctx: &egui::Context) {
        self.step_history(ctx, true);
    }

    fn redo(&mut self, ctx: &egui::Context) {
        self.step_history(ctx, false);
    }

    /// Dépile un état et le restaure, en poussant l'état courant sur la pile
    /// opposée. Une entrée structurelle remplace le document entier ; une
    /// entrée d'annotations ne touche que la liste en mémoire.
    fn step_history(&mut self, ctx: &egui::Context, backward: bool) {
        let mut invalidate_search = false;
        let message = {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let entry = if backward {
                tab.undo.pop()
            } else {
                tab.redo.pop()
            };
            let Some(HistoryEntry { doc, annots }) = entry else {
                return;
            };
            let structural = doc.is_some();
            let current = HistoryEntry {
                // Les octets courants ne valent que si l'on quitte un état
                // structurel : sinon le document n'a pas bougé.
                doc: if structural {
                    tab.history_base
                        .take()
                        .or_else(|| tab.doc.to_bytes().ok())
                } else {
                    None
                },
                annots: tab.annots.clone(),
            };

            let restored = match doc {
                Some(bytes) => match Document::from_bytes(&bytes) {
                    Ok(document) => {
                        tab.doc = document;
                        tab.annots = annots;
                        tab.invalidate_render();
                        tab.fragments.clear();
                        tab.structure_loaded = false;
                        tab.history_base = Some(bytes);
                        true
                    }
                    Err(_) => {
                        // Remettre l'entrée en place : sans cela un document
                        // illisible ferait disparaître un pas d'historique.
                        let back = HistoryEntry {
                            doc: Some(bytes),
                            annots,
                        };
                        if backward {
                            tab.undo.push(back);
                        } else {
                            tab.redo.push(back);
                        }
                        if tab.history_base.is_none() {
                            tab.history_base = current.doc.clone();
                        }
                        false
                    }
                },
                None => {
                    tab.annots = annots;
                    true
                }
            };

            if restored {
                if backward {
                    tab.redo.push(current);
                    trim_history(&mut tab.redo);
                } else {
                    tab.undo.push(current);
                    trim_history(&mut tab.undo);
                }
                if structural {
                    let hash = tab
                        .history_base
                        .as_deref()
                        .map(fingerprint)
                        .unwrap_or_default();
                    tab.structural_dirty = hash != tab.saved_hash;
                    // Le document a été remplacé : l'index, les résultats et
                    // les surlignages décrivent l'état précédent.
                    invalidate_search = true;
                }
                tab.dirty = tab.structural_dirty || tab.annots != tab.baseline_annots;
                tab.page = tab.page.clamp(1, tab.total().max(1));
                Some(if backward { "Undone" } else { "Redone" })
            } else {
                Some(if backward {
                    "Undo failed: document illisible"
                } else {
                    "Redo failed: document illisible"
                })
            }
        };
        if invalidate_search {
            self.search_index = None;
            self.clear_search_state();
        }
        if let Some(message) = message {
            self.toast(ctx, message);
        }
    }

    /// Style courant pour un type d'annotation donné.
    fn annot_style(&self, kind: AnnotationKind) -> AnnotationStyle {
        let mut style = AnnotationStyle {
            color: self.annot_color,
            opacity: 1.0,
            line_width: self.annot_width,
            font_size: 12.0,
            fill: None,
            dash: BorderDash::Solid,
            cloudy: false,
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
            let before = tab.annots.clone();
            tab.record_annots(before);
            tab.annots.push(annot);
            tab.dirty = true;
        }
    }

    /// Retire l'annotation d'indice donné.
    fn remove_annot(&mut self, index: usize) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if index < tab.annots.len() {
                let before = tab.annots.clone();
                tab.record_annots(before);
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
        let page_annots = annots_to_page(&tab.doc, &tab.annots);
        tab.doc
            .apply_annotations(&page_annots)
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




    /// Embarque un fichier choisi par l'utilisateur.
    fn add_attachment(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new().set_title("Attach a file").pick_file() else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.toast(ctx, format!("Attach failed: {err}"));
                return;
            }
        };
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("attachment")
            .to_owned();
        let result = self
            .tabs
            .get_mut(self.active)
            .map(|tab| tab.doc.add_attachment(&name, None, &bytes));
        match result {
            Some(Ok(())) => {
                self.mark_document_changed();
                self.ensure_structure();
                self.toast(ctx, format!("Attached {name}"));
            }
            Some(Err(err)) => self.toast(ctx, format!("Attach failed: {err}")),
            None => {}
        }
    }

    /// Ouvre une pièce jointe dans l'application par défaut du système.
    ///
    /// Le fichier est extrait sous un nom assaini, dans un dossier temporaire
    /// privé et propre à cette extraction : ni un nom hostile ni un lien
    /// symbolique planté à l'avance ne doivent sortir de ce dossier.
    fn open_attachment(&mut self, index: usize, ctx: &egui::Context) {
        let Some((bytes, name)) = self.tabs.get(self.active).and_then(|tab| {
            let name = tab
                .attachments
                .get(index)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| format!("attachment-{index}"));
            tab.doc.attachment_bytes(index).ok().map(|bytes| (bytes, name))
        }) else {
            self.toast(ctx, "Attachment failed");
            return;
        };
        let file_name = safe_attachment_name(&name, index);
        let path = match stage_attachment(&bytes, &file_name) {
            Ok(path) => path,
            Err(err) => {
                self.toast(ctx, format!("Open failed: {err}"));
                return;
            }
        };
        // Les extractions d'hier ne servent plus à rien : elles partent au
        // prochain usage, faute de pouvoir être retirées dès l'ouverture
        // (l'application externe peut encore lire le fichier).
        let _ = prune_attachment_staging(ATTACHMENT_STAGING_MAX_AGE);
        match self.open_with_system(&path) {
            Ok(()) => self.toast(ctx, format!("Opened {}", path.display())),
            Err(err) => self.toast(ctx, format!("Open failed: {err}")),
        }
    }

    /// Écrit une pièce jointe à l'emplacement choisi par l'utilisateur.
    fn save_attachment(&mut self, index: usize, ctx: &egui::Context) {
        let Some((bytes, name)) = self.tabs.get(self.active).and_then(|tab| {
            let name = tab
                .attachments
                .get(index)
                .map(|item| item.name.clone())
                .unwrap_or_else(|| format!("attachment-{index}"));
            tab.doc.attachment_bytes(index).ok().map(|bytes| (bytes, name))
        }) else {
            self.toast(ctx, "Attachment failed");
            return;
        };
        let Some(target) = rfd::FileDialog::new().set_file_name(&name).save_file() else {
            return;
        };
        match std::fs::write(&target, bytes) {
            Ok(()) => self.toast(ctx, format!("Saved {}", target.display())),
            Err(err) => self.toast(ctx, format!("Write failed: {err}")),
        }
    }

    /// Retire une pièce jointe du document.
    fn delete_attachment(&mut self, index: usize, ctx: &egui::Context) {
        let result = self
            .tabs
            .get_mut(self.active)
            .map(|tab| tab.doc.remove_attachment(index));
        match result {
            Some(Ok(())) => {
                self.mark_document_changed();
                self.ensure_structure();
                self.toast(ctx, "Attachment removed");
            }
            Some(Err(err)) => self.toast(ctx, format!("Delete failed: {err}")),
            None => {}
        }
    }

    /// Ouvre un fichier avec l'application par défaut du système.
    fn open_with_system(&self, path: &std::path::Path) -> std::io::Result<()> {
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
    fn open_url(&self, url: &str) -> std::io::Result<()> {
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

    /// Exécute l'action d'un lien et prévient l'utilisateur du résultat.
    ///
    /// Une adresse part dans le navigateur du système, un saut interne change
    /// de page, une action inconnue est dite telle : mieux vaut l'annoncer que
    /// laisser croire que le clic n'a rien fait.
    fn follow_link(&mut self, action: &LinkAction, ctx: &egui::Context) {
        match action {
            LinkAction::Uri(url) => match self.open_url(url) {
                Ok(()) => self.toast(ctx, format!("Opened {url}")),
                Err(err) => self.toast(ctx, format!("Could not open link: {err}")),
            },
            LinkAction::GoTo { page } => {
                let total = self.tabs.get(self.active).map(|tab| tab.total()).unwrap_or(0);
                if total == 0 {
                    return;
                }
                let target = (*page).clamp(1, total);
                self.jump = Some(target);
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.page = target;
                }
                self.toast(ctx, format!("Page {target}"));
            }
            LinkAction::Unsupported(name) => {
                self.toast(ctx, format!("Unsupported link action: {name}"));
            }
        }
    }

    /// Suis le lien sous un point, s'il y en a un.
    ///
    /// Renvoie `true` quand un lien a été suivi : l'appelant laisse alors le
    /// clic tranquille, sans sélection ni ouverture d'inspecteur.
    fn follow_link_at(&mut self, page: usize, point: [f32; 2], ctx: &egui::Context) -> bool {
        let Some(tab) = self.tabs.get(self.active) else {
            return false;
        };
        let links = tab.doc.page_links(page);
        let Some(link) = link_at(&links, point) else {
            return false;
        };
        let action = link.action.clone();
        self.follow_link(&action, ctx);
        true
    }


    // -- Outils d'annotation ------------------------------------------------

    /// Action du canevas selon le mode et l'outil choisi.
    fn canvas_action(&self) -> ToolAction {
        // Tant qu'un bloc est ouvert à la réécriture, le canevas ne réagit
        // plus : les clics reviennent au champ posé sur la page.
        if self.edit_text.is_some() {
            return ToolAction::None;
        }
        if self.measure_active {
            return ToolAction::Measure;
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
                // L'outil Sélection choisit le contenu à éditer : un bloc de
                // texte, une image.
                Some(0) => ToolAction::Select,
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
                Some(6) => ToolAction::FormField(FormKind::Button),
                Some(7) => ToolAction::FormField(FormKind::Date),
                Some(8) => ToolAction::FormField(FormKind::Barcode),
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
            Some(11) => ToolAction::Callout,
            _ => ToolAction::None,
        }
    }

    /// Type de brouillon pour l'outil courant (texte, note, tampon, lien).
    fn draft_kind(&self) -> DraftKind {
        draft_kind_for(self.mode, self.annot_tool)
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
            let template = &STAMP_TEMPLATES[self.stamp_choice.min(STAMP_TEMPLATES.len() - 1)];
            let date = chrono::Local::now().format("%Y-%m-%d").to_string();
            let author = self.comment_author.clone().unwrap_or_default();
            annot.content = youpdf_core::annotate::dynamic_stamp_text(template, &author, &date);
            annot.style.color = template.color;
            annot.style.font_size = youpdf_core::annotate::STAMP_FONT_SIZE;
            annot.stamp = Some(template.id.to_string());
        }
        self.push_annot(annot);
    }

    /// Ajoute une mise en évidence alignée sur les lignes de texte.
    fn add_markup_annot(&mut self, page: usize, kind: AnnotationKind, quads: Vec<[f32; 8]>) {
        if quads.is_empty() {
            return;
        }
        let mut annot = Annotation::new(page, kind, [0.0, 0.0, 0.0, 0.0]);
        annot.style = self.annot_style(kind);
        annot.set_quads(quads);
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
        if draft_kind == DraftKind::Signature {
            // Une signature visuelle est une image, pas une annotation : le
            // créateur la fabrique et la pose au point cliqué.
            self.marks.signature_at = Some((page, point));
            self.marks.signature_open = true;
            return;
        }
        let (annot_kind, rect, buffer) = match draft_kind {
            DraftKind::Note => (
                AnnotationKind::Note,
                [point[0], point[1] - 20.0, point[0] + 20.0, point[1]],
                String::new(),
            ),
            DraftKind::Stamp => {
                let template = &STAMP_TEMPLATES[self.stamp_choice.min(STAMP_TEMPLATES.len() - 1)];
                let date = chrono::Local::now().format("%Y-%m-%d").to_string();
                let author = self.comment_author.clone().unwrap_or_default();
                let text = youpdf_core::annotate::dynamic_stamp_text(template, &author, &date);
                (
                    AnnotationKind::Stamp,
                    [point[0], point[1] - 26.0, point[0] + 150.0, point[1] + 26.0],
                    text,
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
            caret_at: None,
            single_line: true,
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
                    && a.reply_to.is_none()
                    && point[0] >= a.rect[0] - 4.0
                    && point[0] <= a.rect[2] + 4.0
                    && point[1] >= a.rect[1] - 4.0
                    && point[1] <= a.rect[3] + 4.0
            })
            .map(|(idx, _)| idx)
    }

    /// Indice du tracé de crayon réellement touché à `point`, en ignorant les
    /// boîtes englobantes vides.
    fn ink_hit_test(&self, page: usize, point: [f32; 2]) -> Option<usize> {
        let tab = self.tabs.get(self.active)?;
        tab.annots
            .iter()
            .enumerate()
            .rev()
            .find(|(_, a)| {
                a.page == page
                    && a.kind == AnnotationKind::Ink
                    && a.reply_to.is_none()
                    && a.ink_hit(point, 8.0)
            })
            .map(|(idx, _)| idx)
    }

    /// Gomme mode segment : coupe les tracés de `page` qui croisent le
    /// segment `[from, to]`. Les tracés entièrement balayés disparaissent.
    fn erase_ink_segment(&mut self, page: usize, from: [f32; 2], to: [f32; 2], ctx: &egui::Context) {
        /// Rayon du pinceau de la gomme, en points PDF.
        const RADIUS: f32 = 6.0;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let before = tab.annots.clone();
        let mut changed = false;
        let mut kept: Vec<Annotation> = Vec::with_capacity(tab.annots.len());
        for mut annot in std::mem::take(&mut tab.annots) {
            if annot.page == page && annot.kind == AnnotationKind::Ink
                && annot.erase_ink_segment(from, to, RADIUS) > 0
            {
                changed = true;
                if annot.ink_points.is_empty() {
                    continue;
                }
                annot.recompute_ink_bbox();
            }
            kept.push(annot);
        }
        tab.annots = kept;
        if changed {
            tab.record_annots(before);
            tab.dirty = true;
            self.toast(ctx, "Ink erased");
        }
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
            reader::extract_page_fragments(&*self.renderer, tab, page);
        }
    }

    /// Couleur du fond de page autour d'un bloc, en repère affiché.
    fn mask_color(&self, page: usize, rect: [f32; 4]) -> [u8; 3] {
        match self.tabs.get(self.active) {
            Some(tab) => mask_color_in(tab, page, rect),
            None => [255, 255, 255],
        }
    }

    /// Sélectionne tout le texte de la page courante.
    fn select_page_text(&mut self) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let page = tab.page as u32;
        // Le texte de la page peut ne pas avoir été extrait : c'est le cas
        // tant qu'aucun glissé n'a eu lieu dessus.
        if !tab.fragments.contains_key(&page) {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            if !reader::extract_page_fragments(&*self.renderer, tab, page) {
                return;
            }
        }
        self.text_sel = Some(TextSel { page, span: None });
    }

    /// Copie le texte de la sélection dans le presse-papiers.
    fn copy_selection(&mut self, ctx: &egui::Context) {
        let Some(sel) = self.text_sel else {
            return;
        };
        let text = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.fragments.get(&sel.page))
            .map(|frags| hit_selection_text(frags, sel.span))
            .unwrap_or_default();
        if text.is_empty() {
            self.toast(ctx, "No text selected");
            return;
        }
        ctx.copy_text(text);
        self.toast(ctx, "Copied");
    }

    /// Pose une mise en évidence sur la sélection de texte courante.
    ///
    /// C'est le geste du lecteur : on sélectionne le texte, puis on le
    /// colore. La marque suit les lignes du texte sélectionné — un
    /// quadrilatère par ligne — et non un rectangle posé à l'aveugle.
    fn markup_selection(&mut self, kind: AnnotationKind, ctx: &egui::Context) {
        let Some(sel) = self.text_sel else {
            return;
        };
        // Une sélection sans ancre couvre toute la page : c'est le geste
        // « tout sélectionner », pas une sélection de lecture.
        if sel.span.is_none() {
            self.toast(ctx, "Select some text first");
            return;
        }
        let quads = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.fragments.get(&sel.page))
            .map(|frags| hit_quads(frags, sel.span))
            .unwrap_or_default();
        if quads.is_empty() {
            self.toast(ctx, "No text selected");
            return;
        }
        self.add_markup_annot(sel.page as usize, kind, quads);
        self.text_sel = None;
        let label = MARKUP_KINDS
            .iter()
            .find(|(candidate, _)| *candidate == kind)
            .map(|(_, label)| *label)
            .unwrap_or("Mark");
        self.toast(ctx, format!("{label} added"));
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
                // Un bloc vide ou retourné ne peut pas être réécrit.
                let [x1, y1, x2, y2] = fragment.rect;
                if x1 >= x2 || y1 >= y2 || fragment.text.is_empty() {
                    self.toast(ctx, "Invalid text block");
                    self.edit_text = None;
                    return;
                }
                self.edit_text = Some(EditTextDraft {
                    // Réécriture sur place : on part du texte existant, le
                    // curseur à la fin, comme dans un traitement de texte.
                    buffer: fragment.text.clone(),
                    style: TextStyle::from_fragment(&fragment),
                    fresh: true,
                    caret_at: Some(point[0]),
                    single_line: !fragment.text.contains('\n'),
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
        // Une retouche de style compte autant qu'une retouche de texte : sans
        // cela, changer la couleur ou le corps n'écrirait rien.
        if draft.buffer == draft.fragment.text && draft.style == TextStyle::from_fragment(&draft.fragment) {
            return;
        }
        // Couleur du fond sous le bloc : le patch écrit par le cœur doit se
        // fondre dans la page, pas poser un rectangle blanc.
        let bg = self.mask_color(draft.fragment.page, draft.fragment.rect);
        let replaced = match self.tabs.get_mut(self.active) {
            Some(tab) => {
                // Le fragment vit en repère affiché, la réécriture attend le
                // repère page.
                let mut fragment = draft.fragment.clone();
                fragment.rect = youpdf_core::geom::rect_to_page(&tab.doc, fragment.page, fragment.rect);
                let background = Some([bg[0] as f32 / 255.0, bg[1] as f32 / 255.0, bg[2] as f32 / 255.0]);
                tab.doc
                    .replace_text_fragment_styled(&fragment, &draft.buffer, background, &draft.style)
            }
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
        // Le clic arrive en repère affiché ; l'insertion se fait en repère page.
        let point = match self.tabs.get(self.active) {
            Some(tab) => youpdf_core::geom::point_to_page(&tab.doc, page, point),
            None => return,
        };
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
        let buffer = draft.buffer.clone();

        let mut open = true;
        self.show_modal(ctx, "modal-draft", title, 300.0, &mut open, |app, ui| {
            let Some(draft) = app.text_draft.as_mut() else {
                return;
            };
            draft.buffer = buffer;
            let resp = ui.add_sized(
                vec2(280.0, 26.0),
                TextEdit::singleline(&mut draft.buffer).hint_text(hint),
            );
            if fresh {
                resp.request_focus();
            }
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                app.confirm_draft();
                app.text_draft = None;
            }
            ui.horizontal(|ui| {
                if ui.button(t("Add")).clicked() {
                    app.confirm_draft();
                    app.text_draft = None;
                }
                if ui.button(t("Cancel")).clicked() {
                    app.text_draft = None;
                }
            });
        });
        if !open {
            self.text_draft = None;
        }
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
            // L'état d'avant l'édition est celui du dernier point de
            // restauration connu. Une édition sans image (document trop gros
            // ou lecture impossible) n'ajoute pas d'entrée : mieux vaut pas
            // d'annulation qu'une annulation fausse.
            let previous = tab.history_base.take();
            if let Some(bytes) = previous {
                tab.undo.push(HistoryEntry {
                    doc: Some(bytes),
                    annots: tab.annots.clone(),
                });
                trim_history(&mut tab.undo);
                tab.redo.clear();
            }
            tab.dirty = true;
            tab.structural_dirty = true;
            tab.invalidate_render();
            // Les annotations en attente ne vivent qu'en mémoire : les
            // marquer « à recharger » les remplacerait par celles du
            // document, c'est-à-dire par rien.
            tab.fragments.clear();
            tab.structure_loaded = false;
            tab.history_base = tab.doc.to_bytes().ok();
            tab.page = tab.page.clamp(1, tab.total().max(1));
        }
        self.search_index = None;
        self.clear_search_state();
    }

    // -- Zoom --------------------------------------------------------------

    fn set_zoom(&mut self, zoom: Option<f32>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.zoom = zoom.map(|z| z.clamp(0.1, 8.0));
            if zoom.is_none() {
                tab.zoom_fit_page = false;
            }
        }
        self.zoom_dirty = true;
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
        self.zoom_dirty = true;
    }

    fn zoom_by(&mut self, factor: f32) {
        let base = self
            .active_tab()
            .and_then(|t| t.zoom)
            .unwrap_or(self.last_fit.max(0.05));
        self.set_zoom(Some(base * factor));
    }

    // -- Recherche ---------------------------------------------------------

    /// Texte utile de la requête, ou `None` si elle est vide.
    fn search_needle(&self) -> Option<String> {
        let query = self.search_query.trim();
        (!query.is_empty()).then(|| query.to_owned())
    }

    /// Oublie les résultats et les surlignages, sans toucher à l'index.
    ///
    /// L'index reste valable tant que le document ne change pas : une requête
    /// vidée puis retapée ne doit pas relire toutes les pages.
    fn clear_search_state(&mut self) {
        self.results.clear();
        self.search_active = None;
        self.search_rects.clear();
        self.search_rects_page = None;
    }

    /// Recherche explicite (Entrée, champ modifié, bouton) : la requête
    /// entre dans l'historique et le nombre de résultats est annoncé.
    fn run_search(&mut self, ctx: &egui::Context) {
        // La recherche explicite couvre la frappe en attente.
        self.search_typed_at = None;
        let Some(query) = self.search_needle() else {
            self.clear_search_state();
            return;
        };
        let options = SearchOptions {
            case_sensitive: self.search_case,
            whole_word: self.search_word,
        };
        let Some(count) = self.refresh_search_results(ctx, &query, options, true) else {
            return;
        };
        self.history.retain(|q| q != &query);
        self.history.insert(0, query);
        self.history.truncate(8);
        self.toast(ctx, format!("{count} result(s)"));
    }

    /// Recalcule les résultats et les surlignages sans bruit.
    ///
    /// Utilisé pendant la saisie : ni historique, ni toast (`announce` les
    /// tait), et l'index déjà construit est réutilisé — seule la recherche
    /// elle-même est refaite. Renvoie le nombre de résultats, ou `None` si
    /// l'index a échoué.
    fn refresh_search_results(
        &mut self,
        ctx: &egui::Context,
        query: &str,
        options: SearchOptions,
        announce: bool,
    ) -> Option<usize> {
        let tab = self.tabs.get(self.active)?;

        if self.search_index.as_ref().map(|(idx, _)| *idx) != Some(self.active) {
            match SearchIndex::build(&tab.doc) {
                Ok(index) => self.search_index = Some((self.active, index)),
                Err(err) => {
                    if announce {
                        self.toast(ctx, format!("Search index failed: {err}"));
                    }
                    return None;
                }
            }
        }

        let Some((_, index)) = &self.search_index else {
            return None;
        };
        let mut results = index.search_with(query, 200, options);
        // Les signets et les commentaires ne vivent pas dans le texte des
        // pages : on les ajoute à la main quand les filtres les demandent.
        let synonyms = self.bookmark_comment_matches(query, options);
        results.extend(synonyms);
        let count = results.len();
        self.results = results;
        self.search_active = if count > 0 { Some(0) } else { None };
        self.focus_search_result();
        Some(count)
    }

    /// Résultats synthétiques pour les signets et les commentaires.
    ///
    /// Actifs seulement si les filtres correspondants sont cochés : les
    /// titres de signets et le texte des commentaires ne font pas partie du
    /// texte des pages, donc le moteur ne les voit pas.
    fn bookmark_comment_matches(
        &self,
        query: &str,
        options: SearchOptions,
    ) -> Vec<SearchResult> {
        let mut out = Vec::new();
        if !self.search_bookmarks && !self.search_comments {
            return out;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return out;
        };
        let needle = query.trim();
        if needle.is_empty() {
            return out;
        }

        if self.search_bookmarks {
            for item in tab.doc.outline() {
                if let Some((start, end)) = find_query(&item.title, needle, options) {
                    out.push(SearchResult {
                        page: item.page.unwrap_or(1),
                        start,
                        end,
                        context: format!("Bookmark · {}", item.title),
                    });
                }
            }
        }
        if self.search_comments {
            for annot in &tab.annots {
                if annot.content.is_empty() {
                    continue;
                }
                if let Some((start, end)) = find_query(&annot.content, needle, options) {
                    out.push(SearchResult {
                        page: annot.page,
                        start,
                        end,
                        context: format!("Comment · {}", annot.content),
                    });
                }
            }
        }
        out
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
        let Some(needle) = self.search_needle() else {
            return;
        };
        let options = SearchOptions {
            case_sensitive: self.search_case,
            whole_word: self.search_word,
        };
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
            return;
        };
        for fragment in fragments {
            // Les fragments sortent du moteur en repère page ; les surlignages
            // se dessinent en repère affiché.
            let rect = youpdf_core::geom::rect_to_display(&tab.doc, page, fragment.rect);
            self.search_rects
                .extend(match_rects(&fragment.text, rect, &needle, options));
        }
    }

    /// Remplace (ou supprime, `replacement` vide) toutes les correspondances.
    fn replace_matches(&mut self, ctx: &egui::Context, replacement: &str) -> usize {
        let Some(query) = self.search_needle() else {
            return 0;
        };
        let options = SearchOptions {
            case_sensitive: self.search_case,
            whole_word: self.search_word,
        };
        let pages: std::collections::BTreeSet<usize> =
            self.results.iter().map(|result| result.page).collect();
        let mut changed = 0usize;
        {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return 0;
            };
            for page in pages {
                let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
                    continue;
                };
                for fragment in fragments {
                    let new_text = replace_occurrences(&fragment.text, &query, replacement, options);
                    if new_text == fragment.text {
                        continue;
                    }
                    // Même masque que la réécriture en place : la couleur du
                    // fond de la page quand elle est connue, blanc sinon.
                    let bg = mask_color_in(
                        tab,
                        page,
                        youpdf_core::geom::rect_to_display(&tab.doc, page, fragment.rect),
                    );
                    let background =
                        Some([bg[0] as f32 / 255.0, bg[1] as f32 / 255.0, bg[2] as f32 / 255.0]);
                    if tab
                        .doc
                        .replace_text_fragment(&fragment, &new_text, background)
                        .is_ok()
                    {
                        changed += 1;
                    }
                }
            }
            if changed > 0 {
                tab.dirty = true;
                tab.structural_dirty = true;
                tab.invalidate_render();
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
        let Some(needle) = self.search_needle() else {
            return;
        };
        let options = SearchOptions {
            case_sensitive: self.search_case,
            whole_word: self.search_word,
        };
        let pages: std::collections::BTreeSet<usize> =
            self.results.iter().map(|result| result.page).collect();
        let mut added = 0usize;
        {
            let Some(tab) = self.tabs.get(self.active) else {
                return;
            };
            for page in pages {
                let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
                    continue;
                };
                for fragment in fragments {
                    let rect = youpdf_core::geom::rect_to_display(&tab.doc, page, fragment.rect);
                    for rect in match_rects(&fragment.text, rect, &needle, options) {
                        // Ne pas empiler deux fois la même marque : le bouton
                        // corbeille et le bouton surlignage peuvent se suivre.
                        // La déduplication porte sur la page et le rectangle :
                        // le remplissage et le texte superposé ne font pas
                        // partie de l'identité de la zone.
                        if !self
                            .redact_marks
                            .iter()
                            .any(|m| m.page == page && m.rect == rect)
                        {
                            let fill = self.redact_fill;
                            let overlay = self.overlay_for(fill);
                            self.redact_marks.push(RedactionMark {
                                page,
                                rect,
                                fill,
                                overlay,
                            });
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

    /// Le texte à superposer sur une zone, s'il y en a un.
    ///
    /// La couleur suit le fond : blanc sur un remplissage sombre, noir sur
    /// un remplissage clair, sinon le texte superposé serait illisible.
    fn overlay_for(&self, fill: RedactionFill) -> Option<RedactionOverlay> {
        let text = self.redact_overlay_text.trim();
        if text.is_empty() {
            return None;
        }
        let background = match fill {
            RedactionFill::Black => [0, 0, 0],
            RedactionFill::White => [255, 255, 255],
            RedactionFill::Custom(rgb) => rgb,
        };
        // Luminance relative : au-delà de la moitié, le fond est clair.
        let luminance = 0.299 * f32::from(background[0])
            + 0.587 * f32::from(background[1])
            + 0.114 * f32::from(background[2]);
        let color = if luminance > 140.0 {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        };
        Some(RedactionOverlay {
            text: text.to_string(),
            font: self.redact_overlay_font,
            size: self.redact_overlay_size,
            color,
        })
    }

    /// Applique le caviardage marqué (destructif).
    fn apply_redactions(&mut self, ctx: &egui::Context) {
        let marks = std::mem::take(&mut self.redact_marks);
        self.redact_modal = false;
        self.redact_phase = RedactPhase::Marking;
        if marks.is_empty() {
            self.toast(ctx, "No areas marked");
            return;
        }
        let sanitize = self.redact_sanitize;
        let outcome: Result<(RedactionReport, usize), String> = {
            let Some(tab) = self.tabs.get_mut(self.active) else {
                return;
            };
            let renderer = &*self.renderer;
            match tab.doc.redact_marks(&marks, renderer) {
                Ok(report) => {
                    let cleaned = if sanitize {
                        tab.doc.sanitize().unwrap_or(0)
                    } else {
                        0
                    };
                    Ok((report, cleaned))
                }
                Err(err) => Err(err.to_string()),
            }
        };
        // Le caviardage réécrit les pages : caches et instantanés caducs.
        self.mark_document_changed();
        match outcome {
            Ok((report, cleaned)) => {
                // Le caviardage a détruit le contenu sous les zones : les
                // annotations en mémoire sont caduques. Sans ce rechargement,
                // `save_with_annotations` réécrirait dans le fichier une
                // annotation qui chevauchait une zone caviardée.
                if let Some(tab) = self.tabs.get_mut(self.active) {
                    tab.annots = annots_to_display(&tab.doc, tab.doc.read_annotations());
                    tab.annots_loaded = true;
                }
                self.selected_annot = 0;
                self.has_annot_selection = false;
                self.annot_tool = Some(0);
                // Le bilan est honnête : on n'annonce une rasterisation que
                // s'il y en a eu une, mais on dit ce qui a été fait.
                let mut message = format!(
                    "Redaction applied ({} areas) · {} pages edited",
                    marks.len(),
                    report.pages_sliced
                );
                if report.pages_rasterized > 0 {
                    message.push_str(&format!(", {} page(s) rasterized", report.pages_rasterized));
                }
                if sanitize {
                    message.push_str(&format!(" · {cleaned} metadata entries removed"));
                }
                self.toast(ctx, message);
            }
            Err(err) => {
                self.redact_marks = marks;
                self.toast(ctx, format!("Redaction failed: {err}"));
            }
        }
    }

    // -- Rendu des pages ---------------------------------------------------

    /// Texture d'une page, nette sur écrans HiDPI.
    ///
    /// `allow_render` est faux pendant un geste de zoom : le palier exact
    /// n'existe pas encore, on étire alors le palier voisin déjà en cache au
    /// lieu de rastériser. Le rendu net est demandé après le geste.
    #[allow(clippy::too_many_arguments)]
    fn page_texture(
        ctx: &egui::Context,
        renderer: &dyn Renderer,
        tab: &mut Tab,
        page: u32,
        step: i32,
        logical_scale: f32,
        ppp: f32,
        allow_render: bool,
        fast: bool,
        status_holder: &mut String,
    ) -> Option<TextureHandle> {
        let key = (page, step);
        let target_scale = logical_scale * ppp;
        if let Some(cached) = tab.cache.get(&key) {
            // Au repos, on affine : un palier rendu à une échelle trop
            // éloignée de celle demandée est redessiné. Pendant un geste,
            // l'existant suffit.
            let stale = (cached.scale - target_scale).abs() / target_scale.max(0.001)
                > RENDER_TOLERANCE;
            if !stale || !allow_render {
                return Some(cached.texture.clone());
            }
        }

        if !allow_render {
            // Palier voisin : la pyramide a un pas de √2, donc au plus un
            // facteur √2 d'étirement, invisible une fois filtré.
            let nearest = tab
                .cache
                .keys()
                .filter(|(p, s)| *p == page && *s >= 0)
                .min_by_key(|(_, s)| ((*s - step).abs(), *s))
                .copied()
                .and_then(|k| tab.cache.get(&k).map(|c| c.texture.clone()));
            if let Some(tex) = nearest {
                return Some(tex);
            }
            // Rien en cache pour cette page : elle entre à l'écran. Étirer un
            // néant afficherait une page blanche, on la rastérise donc
            // malgré le geste en cours — mais à basse résolution, sinon la
            // frame saute et le défilement saccade.
        }

        // Pendant un geste, on rasterise à échelle réduite : le bitmap est
        // étiré jusqu'à l'arrêt, puis affiné (l'entrée du cache porte alors
        // une échelle éloignée de la cible, donc elle est reprise).
        let render_scale = if fast {
            target_scale * FAST_RENDER_SCALE
        } else {
            target_scale
        };
        let options = RenderOptions {
            scale: render_scale,
            ..Default::default()
        };
        match renderer.render_page(&tab.doc, page, &options) {
            Ok(bitmap) => {
                tracing::info!(
                    "render page {page} step {step}: bitmap {}x{} (scale {:.3})",
                    bitmap.width,
                    bitmap.height,
                    render_scale
                );
                let image = bitmap.to_egui_image();
                // La page en cours d'affichage garde ses pixels : la
                // réécriture y lit la couleur du fond. Une vignette rendue à
                // une échelle minuscule ne doit pas écraser cette référence.
                if page as usize == tab.page && step != THUMBNAIL_STEP && !fast {
                    tab.last_bitmap = Some((page, target_scale, bitmap));
                }
                let handle = match tab.cache.get(&key) {
                    // Ce palier a déjà une texture : on remplace ses pixels
                    // en gardant le même `TextureId`. `Context::load_texture`,
                    // lui, alloue toujours une texture neuve (son nom n'est
                    // qu'une étiquette de débogage) et laisse l'ancienne au
                    // GPU : chaque zoom en fuyait une.
                    Some(cached) => {
                        let mut texture = cached.texture.clone();
                        texture.set(image, TextureOptions::LINEAR);
                        texture
                    }
                    None => ctx.load_texture(
                        format!("p{page}s{step}"),
                        image,
                        TextureOptions::LINEAR,
                    ),
                };
                tab.cache.insert(
                    key,
                    CachedPage {
                        texture: handle.clone(),
                        scale: render_scale,
                    },
                );

                // On ne garde que les trois paliers les plus proches de la
                // page : au-delà ils ne servent plus à étirer, et une longue
                // séance de zoom en accumulerait des dizaines.
                let mut steps: Vec<i32> = tab
                    .cache
                    .keys()
                    .filter(|(p, s)| *p == page && *s >= 0)
                    .map(|(_, s)| *s)
                    .collect();
                if steps.len() > 3 {
                    steps.sort_by_key(|s| (*s - step).abs());
                    for stale in steps.into_iter().skip(3) {
                        tab.cache.remove(&(page, stale));
                    }
                }

                Some(handle)
            }
            Err(err) => {
                *status_holder = format!("Render failed (page {page}): {err}");
                None
            }
        }
    }

    // -- Barre de titre ----------------------------------------------------


    // -- Rail --------------------------------------------------------------

    fn rail(&mut self, ctx: &egui::Context) {
        SidePanel::left("rail")
            .exact_width(RAIL_W)
            .resizable(false)
            .frame(Frame::none().fill(pal().chrome_bg))
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


    /// Construit les filtres courants de la liste des commentaires.
    fn comment_filter(&self) -> CommentFilter {
        CommentFilter {
            kind: self.comment_kind,
            author: self.comment_author.clone(),
            color: self.comment_color,
            state: self.comment_state,
            query: self.comment_search.clone(),
        }
    }

    /// Calcule une fois par document le texte cité sous chaque surlignage.
    fn ensure_comment_quotes(&mut self) {
        let Self {
            tabs,
            comment_quotes,
            comment_quote_owner,
            active,
            ..
        } = self;
        if *comment_quote_owner != *active {
            comment_quotes.clear();
            *comment_quote_owner = *active;
        }
        let Some(tab) = tabs.get(*active) else {
            return;
        };
        // Un seul passage par page : l'extraction de texte coûte bien plus
        // cher que la comparaison de rectangles.
        let mut by_page: std::collections::BTreeMap<u32, Vec<(String, [f32; 4])>> =
            std::collections::BTreeMap::new();
        for annot in &tab.annots {
            if annot.reply_to.is_some()
                || !annot.kind.is_markup()
                || comment_quotes.contains_key(&annot.id)
            {
                continue;
            }
            // Le rectangle est dans le repère affiché ; l'extraction de texte
            // travaille dans celui de la page.
            let rect = tab.doc.page_view(annot.page as u32).page_rect(annot.rect);
            by_page
                .entry(annot.page as u32)
                .or_default()
                .push((annot.id.clone(), rect));
        }
        for (page, requests) in by_page {
            let rects: Vec<[f32; 4]> = requests.iter().map(|(_, rect)| *rect).collect();
            let texts = tab.doc.text_in_rects(page, &rects);
            for ((id, _), text) in requests.into_iter().zip(texts) {
                comment_quotes.insert(id, text);
            }
        }
    }

    /// Supprime un commentaire et toute sa descendance de réponses.
    fn remove_comment_thread(&mut self, id: &str, ctx: &egui::Context) {
        let doomed = match self.tabs.get_mut(self.active) {
            Some(tab) if tab.annots.iter().any(|annot| annot.id == id) => {
                let before_state = tab.annots.clone();
                tab.record_annots(before_state);
                let before = tab.annots.len();
                // Fermeture transitive par `reply_to` (HashSet : O(a + k)).
                let doomed = youpdf_core::annotation::comments::remove_comment_thread(
                    &mut tab.annots,
                    id,
                );
                if tab.annots.len() != before {
                    tab.dirty = true;
                }
                doomed
            }
            _ => return,
        };
        if self.has_annot_selection {
            self.has_annot_selection = false;
        }
        for id in &doomed {
            self.comment_quotes.remove(id);
        }
        if self.comment_reply.as_deref() == Some(id) {
            self.comment_reply = None;
            self.comment_reply_text.clear();
        }
        let _ = ctx;
    }

    /// Applique un état de revue à un commentaire.
    fn set_comment_state(&mut self, id: &str, state: AnnotationState) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(index) = tab.annots.iter().position(|annot| annot.id == id) else {
            return;
        };
        if tab.annots[index].state == state {
            return;
        }
        let before = tab.annots.clone();
        tab.record_annots(before);
        tab.annots[index].state = state;
        tab.dirty = true;
    }

    /// Ajoute une réponse à un commentaire.
    ///
    /// Une réponse est une note invisible sur la page : elle vit dans le fil,
    /// pas comme marque du document.
    fn send_comment_reply(&mut self, parent: &str, text: &str, ctx: &egui::Context) {
        let text = text.trim();
        if text.is_empty() {
            return;
        }
        let Some((page, rect, color)) = self.tabs.get(self.active).and_then(|tab| {
            tab.annots
                .iter()
                .find(|annot| annot.id == parent)
                .map(|annot| (annot.page, annot.rect, annot.style.color))
        }) else {
            return;
        };
        let mut reply = Annotation::new(page, AnnotationKind::Note, rect);
        reply.content = text.to_owned();
        reply.reply_to = Some(parent.to_owned());
        reply.style.color = color;
        self.push_annot(reply);
        self.comment_reply = None;
        self.comment_reply_text.clear();
        self.toast(ctx, "Reply added");
    }

    /// Exporte les commentaires affichés (filtres et tri compris).
    fn export_comments_dialog(&mut self, format: CommentFormat, ctx: &egui::Context) {
        let filter = self.comment_filter();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let rows = comment_rows(&tab.annots, &filter, self.comment_sort);
        let annots: Vec<Annotation> = rows
            .iter()
            .filter_map(|row| tab.annots.get(row.index).cloned())
            .collect();
        // Les rectangles en mémoire sont dans le repère affiché ; l'export
        // les veut dans celui de la page.
        let annots = annots_to_page(&tab.doc, &annots);
        let data = match export_comments(&annots, format) {
            Ok(data) => data,
            Err(err) => {
                self.toast(ctx, format!("Export failed: {err}"));
                return;
            }
        };
        let stem = self
            .active_stem()
            .unwrap_or_else(|| "comments".to_owned());
        if let Some(path) = rfd::FileDialog::new()
            .add_filter(format.display_name(), &[format.extension()])
            .set_file_name(format!("{stem}-comments.{}", format.extension()))
            .save_file()
        {
            match std::fs::write(&path, data) {
                Ok(()) => self.toast(ctx, "Comments exported"),
                Err(err) => self.toast(ctx, format!("Export failed: {err}")),
            }
        }
    }

    // -- Panneau latéral ---------------------------------------------------




    /// Fenêtre « Propriétés de la page ».
    fn page_props_window(&mut self, ctx: &egui::Context) {
        let Some(number) = self.page_props else {
            return;
        };
        let Some(tab) = self.tabs.get(self.active) else {
            self.page_props = None;
            return;
        };
        let total = tab.total();
        if number == 0 || number > total {
            self.page_props = None;
            return;
        }
        let view = tab.doc.page_view(number as u32);
        let (dw, dh) = tab.doc.page_size(number as u32);
        let boxes = tab.doc.page_boxes(number as u32);
        let mut open = true;
        self.show_modal(
            ctx,
            "modal-page-props",
            "Page properties",
            320.0,
            &mut open,
            |_app, ui| {
                let mm = |pt: f32| pt * 25.4 / 72.0;
                let mut row = |label: &str, value: String| {
                    ui.horizontal(|ui| {
                        ui.label(section(label));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(value));
                        });
                    });
                };
                row("Page", format!("{number} of {total}"));
                row(
                    "Visible box",
                    format!("{:.0} × {:.0} pt", view.width, view.height),
                );
                row(
                    "Paper size",
                    format!("{:.1} × {:.1} mm", mm(view.width), mm(view.height)),
                );
                row("Displayed", format!("{dw:.0} × {dh:.0} pt"));
                row("Rotation", format!("{}°", view.rotation));
                row("Origin", format!("({:.0}, {:.0})", view.x, view.y));

                // Boîtes déclarées (Media / Crop / Trim / Bleed), quand elles
                // existent : Media est le support, Crop la zone visible.
                let mut any = false;
                for (label, rect) in [
                    ("Media box", boxes.media),
                    ("Crop box", boxes.crop),
                    ("Trim box", boxes.trim),
                    ("Bleed box", boxes.bleed),
                ] {
                    if let Some([x1, y1, x2, y2]) = rect {
                        row(
                            label,
                            format!(
                                "{:.0} {:.0} {:.0} {:.0} pt",
                                x1,
                                y1,
                                (x2 - x1).abs(),
                                (y2 - y1).abs()
                            ),
                        );
                        any = true;
                    }
                }
                if !any {
                    ui.label(
                        RichText::new(t("No page boxes declared"))
                            .size(LABEL)
                            .color(pal().muted),
                    );
                }
            },
        );
        if !open {
            self.page_props = None;
        }
    }


    /// Fenêtre « Organiser les pages » : grille des pages avec rotation,
    /// suppression, extraction, découpage et insertion. Les opérations
    /// s'appliquent immédiatement au document en mémoire ; rien n'est écrit
    /// sur le disque tant que l'utilisateur n'enregistre pas.
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
            FormKind::Button => {
                let mut field = NewFormField::text(page as u32, rect, name);
                field.kind = FormFieldKind::Button;
                field.value = Some("Button".to_string());
                field
            }
            FormKind::Date => {
                let mut field = NewFormField::text(page as u32, rect, name);
                field.kind = FormFieldKind::Date;
                field
            }
            FormKind::Barcode => {
                let mut field = NewFormField::text(page as u32, rect, name);
                field.kind = FormFieldKind::Barcode;
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
            // La boîte tracée est en repère affiché, le champ s'écrit en repère
            // page.
            let rect = youpdf_core::geom::rect_to_page(&tab.doc, page, rect);
            // Les noms de champ sont des identifiants dans tout le document :
            // deux champs ne peuvent pas partager le même.
            // Les noms sont des identifiants uniques : un `contains` sur un vecteur
            // est O(k) par candidat, O(k²) au total. Un HashSet ramène la
            // recherche à O(1).
            let mut existing: std::collections::HashSet<&str> =
                std::collections::HashSet::with_capacity(tab.forms.len());
            for form in &tab.forms {
                existing.insert(form.name.as_str());
            }
            let mut num = tab.forms.len() + 1;
            while existing.contains(format!("Field{num}").as_str()) {
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


    // -- Barre d'outils flottante ------------------------------------------

    /// Sélectionne l'image posée sous le point, s'il y en a une.
    ///
    /// L'image est retenue par son nom de ressource et sa boîte en repère
    /// page : le canevas la replace à l'écran à chaque trame, sans relire le
    /// flux de contenu.
    fn select_image_at(&mut self, page: usize, point: [f32; 2]) {
        let found = self.tabs.get(self.active).and_then(|tab| {
            let images = tab.doc.extract_images(page as u32).ok()?;
            // `image_rect` est en repère page ; le clic arrive en repère
            // affiché. Sur une page tournée les deux ne coïncident pas.
            let view = tab.doc.page_view(page as u32);
            images.iter().enumerate().find_map(|(index, info)| {
                let rect = view.display_rect(tab.doc.image_rect(info).ok().flatten()?);
                let [x1, y1, x2, y2] = rect;
                let inside = point[0] >= x1 && point[0] <= x2 && point[1] >= y1 && point[1] <= y2;
                inside.then(|| SelectedImage {
                    page,
                    index,
                    name: info.resource_name.clone(),
                    rect,
                })
            })
        });
        if found.is_some() {
            self.image_alpha = 1.0;
        }
        self.image_crop = None;
        self.selected_image = found;
    }

    /// Relit la boîte de l'image sélectionnée après une modification.
    fn refresh_selected_image(&mut self) {
        let Some(selected) = self.selected_image.clone() else {
            return;
        };
        let info = self.tabs.get(self.active).and_then(|tab| {
            let view = tab.doc.page_view(selected.page as u32);
            tab.doc
                .extract_images(selected.page as u32)
                .ok()
                .and_then(|list| {
                    list.into_iter()
                        .find(|info| info.resource_name == selected.name)
                })
                .map(|info| tab.doc.image_rect(&info).ok().flatten().map(|rect| view.display_rect(rect)))
        });
        match info {
            Some(Some(rect)) => {
                if let Some(current) = self.selected_image.as_mut() {
                    current.rect = rect;
                }
            }
            Some(None) => {}
            None => {
                // L'image n'est plus sur la page : la cible n'a plus d'objet.
                self.selected_image = None;
                self.image_crop = None;
                self.inspector_target = None;
            }
        }
    }

    /// Applique un glissé-déplacer sur l'image sélectionnée.
    ///
    /// L'offset est en coordonnées écran ; on le convertit en points de
    /// page (repère PDF, y vers le haut) avant d'écrire la matrice. Le geste
    /// ne réencode aucun pixel : la translation est appliquée à `(e, f)`.
    fn commit_image_move(
        &mut self,
        selected: &SelectedImage,
        offset: [f32; 2],
        scale: f32,
        ctx: &egui::Context,
    ) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(info) = tab
            .doc
            .extract_images(selected.page as u32)
            .ok()
            .and_then(|list| list.into_iter().find(|i| i.resource_name == selected.name))
        else {
            return;
        };
        let dx = offset[0] / scale.max(0.001);
        let dy = -offset[1] / scale.max(0.001);
        let matrix = match tab.doc.image_matrix(&info) {
            Ok(Some(m)) => m,
            _ => [1.0, 0.0, 0.0, 1.0, dx, dy],
        };
        let mut moved = matrix;
        moved[4] += dx;
        moved[5] += dy;
        match tab.doc.set_image_matrix(&info, moved) {
            Ok(()) => {
                self.mark_document_changed();
                self.refresh_selected_image();
                self.toast(ctx, "Image moved");
            }
            Err(err) => self.toast(ctx, format!("Move failed: {err}")),
        }
    }

    /// Applique un redimensionnement à l'image sélectionnée : la matrice
    /// `(a, d)` est étirée et `(e, f)` est déplacée pour que le coin tenu
    /// reste collé au curseur. Les pixels ne sont pas réencodés.
    fn commit_image_scale(
        &mut self,
        selected: &SelectedImage,
        matrix: [f32; 6],
        ctx: &egui::Context,
    ) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(info) = tab
            .doc
            .extract_images(selected.page as u32)
            .ok()
            .and_then(|list| list.into_iter().find(|i| i.resource_name == selected.name))
        else {
            return;
        };
        match tab.doc.set_image_matrix(&info, matrix) {
            Ok(()) => {
                self.mark_document_changed();
                self.refresh_selected_image();
                self.toast(ctx, "Image resized");
            }
            Err(err) => self.toast(ctx, format!("Resize failed: {err}")),
        }
    }

    /// Fait tourner l'image sélectionnée d'un angle libre, en degrés.
    ///
    /// L'angle est le sens horaire à l'écran, comme le geste de la poignée de
    /// rotation au centre de la boîte. Seule la matrice change : les pixels
    /// ne sont jamais réencodés, donc l'opération est réversible.
    fn commit_image_rotate(
        &mut self,
        selected: &SelectedImage,
        degrees: f32,
        ctx: &egui::Context,
    ) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let Some(info) = tab
            .doc
            .extract_images(selected.page as u32)
            .ok()
            .and_then(|list| list.into_iter().find(|i| i.resource_name == selected.name))
        else {
            return;
        };
        match tab.doc.rotate_image(&info, degrees) {
            Ok(()) => {
                self.mark_document_changed();
                self.refresh_selected_image();
                self.toast(ctx, "Image rotated");
            }
            Err(err) => self.toast(ctx, format!("Rotate failed: {err}")),
        }
    }

    /// Déplace un bloc de texte en réécrivant son emplacement cible.
    ///
    /// L'offset écran est converti en points de page (y inversé) et ajouté
    /// au rectangle du fragment. Le texte, la police et la tailre sont
    /// conservés — seul l'emplacement change.
    fn commit_text_block_move(
        &mut self,
        fragment: &TextFragment,
        offset: [f32; 2],
        scale: f32,
        ctx: &egui::Context,
    ) {
        let Some(tab) = self.tabs.get_mut(self.active) else { return };
        let dx = offset[0] / scale.max(0.001);
        let dy = -offset[1] / scale.max(0.001);
        let [x1, y1, x2, y2] = fragment.rect;
        let target = [x1 + dx, y1 + dy, x2 + dx, y2 + dy];
        let style = TextStyle::from_fragment(fragment);
        match tab.doc.replace_text_fragment_at(fragment, &fragment.text, target, None, &style) {
            Ok(()) => {
                self.mark_document_changed();
                self.toast(ctx, "Text block moved");
            }
            Err(err) => self.toast(ctx, format!("Move failed: {err}")),
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
        if let Some(tab) = self.tabs.get_mut(self.active) {
            // Le flush vient d'écrire les annotations dans les octets : une
            // image d'avant ne saurait plus les retirer sans les réintroduire.
            // Aplatir reste donc définitif pour l'historique.
            tab.history_base = None;
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
        let title = tab.title.clone();

        self.show_modal(ctx, "modal-properties", "Document properties", 0.0, &mut open, |_app, ui| {
            let mut row = |label: &str, value: &str| {
                ui.horizontal(|ui| {
                    ui.label(section(label));
                    ui.label(RichText::new(value).color(pal().icon_bright));
                });
            };
            row("File", &title);
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

    // -- IA ----------------------------------------------------------------

    /// Peint le panneau IA. Le texte du document n'est extrait qu'une fois par
    /// onglet, et seulement quand le panneau est ouvert.
    fn ai_window(&mut self, ctx: &egui::Context) {
        if !self.ai.open {
            return;
        }
        if self.ai.enabled() && self.ai_doc_tab != Some(self.active) {
            self.ai_doc_tab = Some(self.active);
            self.ai_doc_text = self
                .tabs
                .get(self.active)
                .map(|tab| extract_document_text(&tab.doc))
                .unwrap_or_default();
        }
        let (name, pages) = match self.tabs.get(self.active) {
            Some(tab) => (Some(tab.title.clone()), Some(tab.total())),
            None => (None, None),
        };
        self.ai.show(ctx, &self.ai_doc_text, name.as_deref(), pages);
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
        let page_annots = annots_to_page(&tab.doc, &tab.annots);
        if tab.doc.apply_annotations(&page_annots).is_err() {
            return None;
        }
        let outcome = write(tab);
        let _ = tab.doc.clear_youpdf_annotations();
        Some(outcome)
    }


    /// Barre de contrôle de la fenêtre de comparaison.
    fn compare_controls(&mut self, ui: &mut egui::Ui, pick: &mut bool, run: &mut bool) {
        let name = self
            .compare_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        ui.horizontal(|ui| {
            if ui.button(t("Choose a PDF…")).clicked() {
                *pick = true;
            }
            ui.label(
                RichText::new(
                    name.clone().unwrap_or_else(|| "No file chosen".to_owned()),
                )
                
                .color(if name.is_some() { pal().icon_bright } else { pal().muted }),
            );
        });
        ui.add_space(SPACE_M);
        ui.horizontal(|ui| {
            if ui.button(t("Compare with the open document")).clicked() {
                *run = true;
            }
            ui.add_space(10.0);
            egui::ComboBox::from_id_salt("compare-view")
                .selected_text(match self.compare_view {
                    CompareView::Report => t("Text report"),
                    CompareView::SideBySide => t("Side by side"),
                    CompareView::Overlay => t("Overlay"),
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.compare_view,
                        CompareView::Report,
                        t("Text report"),
                    );
                    ui.selectable_value(
                        &mut self.compare_view,
                        CompareView::SideBySide,
                        t("Side by side"),
                    );
                    ui.selectable_value(
                        &mut self.compare_view,
                        CompareView::Overlay,
                        t("Overlay"),
                    );
                });
            // Dans la vue superposée, le curseur règle l'opacité du second
            // document posé sur le premier.
            if self.compare_view == CompareView::Overlay {
                ui.add(
                    egui::Slider::new(&mut self.compare_blend, 0.0..=1.0)
                        .text(t("Opacity of B")),
                );
            }
            ui.add_space(10.0);
            if ui.checkbox(&mut self.compare_ignore_ws, t("Ignore whitespace")).changed() {
                self.compare_report = None;
                self.compare_tab = None;
            }
            if ui.checkbox(&mut self.compare_ignore_case, t("Ignore case")).changed() {
                self.compare_report = None;
                self.compare_tab = None;
            }
        });
    }


    /// Nombre de différences signalées sur une page, selon le côté du
    /// rapport (A ou B).
    fn diffs_on_page(&self, side_a: bool, page: usize) -> usize {
        let Some(report) = &self.compare_report else {
            return 0;
        };
        report
            .differences
            .iter()
            .filter(|d| {
                let p = if side_a { d.page_a } else { d.page_b };
                p == Some(page)
            })
            .count()
    }


    /// Fenêtre « Créer » : page vierge ou PDF à partir d'images.
    fn create_window(&mut self, ctx: &egui::Context) {
        if !self.create_open {
            return;
        }
        let mut open = true;
        let mut make_blank = false;
        let mut images = false;
        self.show_modal(ctx, "modal-create", "Create", 380.0, &mut open, |app, ui| {
            ui.label(section(t("Blank document")));
            ui.horizontal(|ui| {
                ui.label(section(t("Pages")));
                ui.add(egui::DragValue::new(&mut app.create_pages).range(1..=500));
                if ui.button(t("Create blank")).clicked() {
                    make_blank = true;
                }
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(section(t("From images")));
            ui.label(
                RichText::new(
                    "One page per image, in the order chosen. PNG and JPEG are read.",
                )
                .size(LABEL)
                .color(pal().muted),
            );
            ui.add_space(SPACE_S);
            if ui.button(t("Choose images…")).clicked() {
                images = true;
            }
        });

        if make_blank {
            match blank_document(self.create_pages.max(1), 595.0, 842.0) {
                Ok(doc) => {
                    self.push_document(doc, None);
                    self.create_open = false;
                }
                Err(err) => self.toast(ctx, format!("Create failed: {err}")),
            }
        }
        if images {
            self.import_images(ctx);
        }
        if !open {
            self.create_open = false;
        }
    }

    /// Nouveau document d'une page vierge.
    fn new_blank_page(&mut self, ctx: &egui::Context) {
        match blank_document(1, 595.0, 842.0) {
            Ok(doc) => self.push_document(doc, None),
            Err(err) => self.toast(ctx, format!("Create failed: {err}")),
        }
    }

    /// Nouveau document dont chaque page porte une image choisie.
    fn import_images(&mut self, ctx: &egui::Context) {
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

    /// Insère un autre PDF à la fin du document courant.
    fn combine_active(&mut self, ctx: &egui::Context) {
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
    fn print_active(&mut self, ctx: &egui::Context) {
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
    fn reveal_path(&self, path: &std::path::Path) {
        #[cfg(target_os = "macos")]
        let command = std::process::Command::new("open").arg("-R").arg(path).spawn();
        #[cfg(not(target_os = "macos"))]
        let command = std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(path))
            .spawn();
        let _ = command;
    }

    /// Révèle le fichier courant dans le gestionnaire de fichiers.
    fn reveal_active(&mut self, ctx: &egui::Context) {
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

    /// Demande le mot de passe d'un fichier protégé pour le déverrouiller.
    fn remove_protection(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
        else {
            return;
        };
        if !Document::file_is_encrypted(&path) {
            self.toast(ctx, "This file is not protected");
            return;
        }
        self.password_prompt = Some(PasswordPrompt {
            path,
            purpose: PasswordPurpose::RemoveProtection,
            value: String::new(),
            error: None,
        });
    }


    // -- Présentation et traitement par lots --------------------------------

    /// Entre en présentation plein écran sur la page courante.
    fn enter_slideshow(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() {
            self.toast(ctx, "Open a document first");
            return;
        }
        self.slideshow = true;
        self.panel = None;
        if !self.fullscreen {
            self.toggle_fullscreen(ctx);
        }
    }

    /// Sort de la présentation.
    fn exit_slideshow(&mut self, ctx: &egui::Context) {
        self.slideshow = false;
        if self.fullscreen {
            self.toggle_fullscreen(ctx);
        }
    }

    /// Présentation : fond noir, une page à la fois, flèches pour tourner.
    fn slideshow_view(&mut self, ctx: &egui::Context) {
        if self.tabs.is_empty() {
            self.exit_slideshow(ctx);
            return;
        }
        let (left, right, first, last, escape, quit) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft)
                    || i.key_pressed(egui::Key::ArrowUp)
                    || i.key_pressed(egui::Key::PageUp),
                i.key_pressed(egui::Key::ArrowRight)
                    || i.key_pressed(egui::Key::ArrowDown)
                    || i.key_pressed(egui::Key::PageDown)
                    || i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
                i.key_pressed(egui::Key::Escape),
                i.key_pressed(egui::Key::F5),
            )
        });
        if escape || quit {
            self.exit_slideshow(ctx);
            return;
        }
        let total = self.active_total() as usize;
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if first {
                tab.page = 1;
            }
            if last {
                tab.page = total.max(1);
            }
            if left {
                tab.page = tab.page.saturating_sub(1).max(1);
            }
            if right {
                tab.page = (tab.page + 1).min(total.max(1));
            }
            // Une page entière par écran : l'ajustement automatique s'en charge.
            tab.zoom = None;
            tab.zoom_fit_page = true;
        }
        CentralPanel::default()
            .frame(Frame::none().fill(Color32::BLACK))
            .show(ctx, |ui| {
                self.canvas(ui);
            });
        let page = self.tabs[self.active].page;
        Area::new(Id::new("slideshow-hud"))
            .fixed_pos(pos2(16.0, 16.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(Color32::from_black_alpha(160))
                    .rounding(Rounding::same(RADIUS_L))
                    .inner_margin(Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{page} / {total}"))
                                
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new(t("← → to turn, Esc to leave"))
                                .size(CAPTION)
                                .color(Color32::from_gray(170)),
                        );
                    });
            });
    }

    /// Applique le thème choisi : palette du chrome, puis réglages egui quand
    /// le thème a changé.
    fn sync_theme(&mut self, ctx: &egui::Context) {
        set_palette(self.theme.palette());
        if self.theme != self.applied_theme {
            self.applied_theme = self.theme;
            apply_theme(ctx, self.theme);
        }
    }

    /// Fond du canevas : la teinte du thème pour le réglage par défaut, le
    /// choix explicite du panneau Aspect sinon.
    fn canvas_background(&self) -> Color32 {
        match self.canvas_style {
            // 0 = Auto : le canevas suit le thème de l'application.
            0 => pal().canvas_bg,
            index => CANVAS_THEMES[(index - 1).min(CANVAS_THEMES.len() - 1)].1,
        }
    }


    /// Enregistre une copie déclarée conforme (PDF/A, PDF/X, PDF/E).
    fn conformance_save(&mut self, ctx: &egui::Context, level: Conformance) {
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
    fn image_pdf_save(&mut self, ctx: &egui::Context) {
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

    /// Reconstruit les signets à partir des titres du document.
    fn generate_bookmarks(&mut self, ctx: &egui::Context) {
        let style = OutlineStyle::default();
        let result = self.tabs.get_mut(self.active).map(|tab| {
            let generated = tab.doc.generate_outline(&style);
            if generated.is_ok() {
                tab.outline = tab.doc.outline();
            }
            generated
        });
        match result {
            Some(Ok(0)) => self.toast(ctx, "No headings found"),
            Some(Ok(count)) => {
                self.mark_document_changed();
                self.bookmark_collapsed.clear();
                self.toast(ctx, format!("{count} bookmark(s) created"));
            }
            Some(Err(err)) => self.toast(ctx, format!("Bookmarks failed: {err}")),
            None => {}
        }
    }

    /// Retire tous les signets du document.
    fn clear_bookmarks(&mut self, ctx: &egui::Context) {
        let result = self
            .tabs
            .get_mut(self.active)
            .map(|tab| tab.doc.clear_outline());
        match result {
            Some(Ok(count)) => {
                self.mark_document_changed();
                self.toast(ctx, format!("{count} bookmark(s) removed"));
            }
            Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
            None => {}
        }
    }


    /// Écrit un fichier d'extraction choisi par l'utilisateur.
    fn save_extract(&mut self, ctx: &egui::Context, name: &str, body: &str) {
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


    /// Surface d'accueil : accès direct aux fichiers récents.
    fn empty_state(&mut self, ui: &mut egui::Ui) {
        self.home_screen(ui);
    }


    /// Grille de cartes de l'écran d'accueil (onglets Récents / Favoris).
    #[allow(clippy::too_many_arguments)]
    fn home_grid(
        &mut self,
        ui: &mut egui::Ui,
        margin: f32,
        ctx: &egui::Context,
        now: i64,
        visible: &[PathBuf],
        open: &mut Option<PathBuf>,
        open_new: &mut Option<PathBuf>,
        reveal: &mut Option<PathBuf>,
        forget: &mut Option<PathBuf>,
        delete: &mut Option<PathBuf>,
        star: &mut Option<PathBuf>,
    ) {
        let usable = (ui.available_width() - 2.0 * margin).max(CARD_W);
        let columns = ((usable + GAP) / (CARD_W + GAP)).floor().max(1.0) as usize;
        let mut index = 0usize;
        while index < visible.len() {
            ui.horizontal(|ui| {
                ui.add_space(margin);
                for path in visible.iter().skip(index).take(columns) {
                    self.home_card(ui, ctx, now, path, open, open_new, reveal, forget, delete, star);
                    ui.add_space(GAP);
                }
            });
            index += columns;
            ui.add_space(GAP);
        }
    }



    /// Grille de raccourcis d'outils de l'onglet « Tools ».
    fn home_tools(&mut self, ui: &mut egui::Ui, margin: f32, tool: &mut Option<&'static str>) {
        const TOOLS: [(&str, &str); 12] = [
            ("Search", "search"),
            ("Thumbnails", "thumbs"),
            ("Organize", "organize"),
            ("Watermark", "watermark"),
            ("Compress", "compress"),
            ("Protect", "protect"),
            ("Crop", "crop"),
            ("OCR", "ocr"),
            ("Compare", "compare"),
            ("Measure", "measure"),
            ("Extract data", "extract"),
            ("Batch", "batch"),
        ];
        let usable = (ui.available_width() - 2.0 * margin).max(CELL_W);
        let columns = ((usable + GAP) / (CELL_W + GAP)).floor().max(1.0) as usize;
        let mut index = 0usize;
        while index < TOOLS.len() {
            ui.horizontal(|ui| {
                ui.add_space(margin);
                for (label, id) in TOOLS.iter().skip(index).take(columns) {
                    let (rect, resp) = ui.allocate_exact_size(vec2(CELL_W, CELL_H), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
                    }
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *label,
                        FontId::proportional(TITLE),
                        pal().icon_bright,
                    );
                    if resp.clicked() {
                        *tool = Some(id);
                    }
                    ui.add_space(GAP);
                }
            });
            index += columns;
            ui.add_space(GAP);
        }
    }

    /// Applique un raccourci d'outil choisi dans l'onglet « Tools ».
    fn run_home_tool(&mut self, id: &str) {
        match id {
            "search" => self.panel = Some(Panel::Search),
            "thumbs" => self.panel = Some(Panel::Thumbnails),
            "organize" => self.organize_open = true,
            "watermark" => self.watermark_open = true,
            "compress" => self.compress_open = true,
            "protect" => self.protect_open = true,
            "crop" => self.crop_open = true,
            "ocr" => self.ocr_open = true,
            "compare" => self.compare_open = true,
            "measure" => self.measure_open = true,
            "extract" => self.extract_open = true,
            "batch" => self.batch_open = true,
            _ => {}
        }
    }

    /// Vignette de la première page d'un fichier, mise en cache par `mtime`.
    ///
    /// Renvoie `None` pour un fichier illisible, chiffré, ou sans pdfium.
    ///
    /// ponytail: l'ouverture et le rendu se font sur le fil de l'interface.
    /// Acceptable pour une poignée de récents ; déporter sur un fil si la
    /// liste s'allonge ou si des fichiers lourds font scintiller l'accueil.
    fn home_thumbnail(&mut self, ctx: &egui::Context, path: &std::path::Path) -> Option<TextureHandle> {
        let meta = std::fs::metadata(path).ok()?;
        // Un fichier énorme n'entre pas dans une vignette : on l'ignore plutôt
        // que de le charger en entier au premier affichage.
        if meta.len() > 64 * 1024 * 1024 {
            return None;
        }
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)?;
        if let Some((cached, texture)) = self.home_thumbs.get(path) {
            if *cached == mtime {
                return Some(texture.clone());
            }
        }
        if !self.renderer.renders_real_content() {
            return None;
        }
        let doc = Document::open(path).ok()?;
        if doc.page_count() == 0 {
            return None;
        }
        let view = doc.page_view(1);
        let scale = (160.0 / view.width.max(1.0)).clamp(0.05, 0.5);
        let options = RenderOptions {
            scale,
            ..Default::default()
        };
        let bitmap = self.renderer.render_page(&doc, 1, &options).ok()?;
        let image = bitmap.to_egui_image();
        // Même fichier, contenu changé : on réécrit la texture existante
        // plutôt que d'en allouer une seconde puis de jeter la première.
        let handle = match self.home_thumbs.get(path) {
            Some((_, texture)) => {
                let mut texture = texture.clone();
                texture.set(image, TextureOptions::LINEAR);
                texture
            }
            None => ctx.load_texture(
                format!("home-thumb-{}", path.display()),
                image,
                TextureOptions::LINEAR,
            ),
        };
        // ponytail: borne simple — vider le cache s'il déborde. Un accueil
        // affiche une poignée de vignettes ; au-delà, les re-rendre coûte
        // moins qu'un cache qui grossit sans fin.
        if self.home_thumbs.len() > 128 {
            self.home_thumbs.clear();
        }
        self.home_thumbs
            .insert(path.to_path_buf(), (mtime, handle.clone()));
        Some(handle)
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

        egui::Window::new(t("Activity"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .anchor(Align2::RIGHT_TOP, vec2(-14.0, TITLE_H + 10.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(t("What YouPDF just did"))
                            .size(LABEL)
                            .color(pal().muted),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui.small_button("Clear").clicked() {
                            clear = true;
                        }
                    });
                });
                ui.add_space(SPACE_S);
                if self.activity.is_empty() {
                    ui.label(
                        RichText::new(t("Nothing yet."))
                            .size(BODY)
                            .color(pal().muted),
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
                            ui.label(RichText::new(text).size(BODY));
                            ui.label(caption(when));
                        });
                        ui.add_space(SPACE_XS);
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
                    .rounding(Rounding::same(RADIUS_L))
                    .inner_margin(Margin::symmetric(12.0, 6.0))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(RichText::new(&text).size(BODY).color(pal().icon_bright))
                                .extend(),
                        );
                    });
            });
    }


    fn handle_drops(&mut self, ctx: &egui::Context) {
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

impl eframe::App for YouPdfApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_drops(ctx);
        if self.active != self.last_active {
            self.last_active = self.active;
            self.reset_document_session();
        }
        self.ensure_annots();
        self.sync_theme(ctx);
        if self.slideshow {
            self.slideshow_view(ctx);
            return;
        }
        self.shortcuts(ctx);
        self.autosave_tick(ctx);
        self.title_bar(ctx);
        self.rail(ctx);
        self.side_panel(ctx);
        self.status_bar(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(self.canvas_background()))
            .show(ctx, |ui| {
                if self.home_open || self.tabs.is_empty() {
                    self.empty_state(ui);
                } else if self.organize_open {
                    self.organize_view(ui);
                } else {
                    self.scan_banner_ui(ui);
                    self.canvas(ui);
                }
            });
        self.toolbar(ctx);
        self.inspector(ctx);
        self.properties_window(ctx);
        self.close_confirm_window(ctx);
        self.export_window(ctx);
        self.watermark_window(ctx);
        self.header_footer_window(ctx);
        self.compress_window(ctx);
        self.protect_window(ctx);
        self.prefs_window(ctx);
        self.crop_window(ctx);
        self.tile_split_window(ctx);
        self.background_window(ctx);
        self.bates_window(ctx);
        self.compare_window(ctx);
        self.ocr_window(ctx);
        self.create_window(ctx);
        self.sign_window(ctx);
        self.stamp_window(ctx);
        self.signature_window(ctx);
        self.batch_window(ctx);
        self.extract_window(ctx);
        self.measure_window(ctx);
        self.password_window(ctx);
        self.draft_window(ctx);
        self.redact_window(ctx);
        self.page_props_window(ctx);
        self.activity_window(ctx);
        self.ai_window(ctx);
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

/// Réglages egui d'un thème. La palette et les visuels vivent dans
/// `youpdf_shared::theme` : un seul endroit définit les couleurs.
fn apply_theme(ctx: &egui::Context, theme: Theme) {
    theme.apply(ctx);
}

/// Extrait le texte du document pour le contexte IA.
///
/// Plafonné à 40 pages et 60 000 caractères : inutile d'envoyer un roman
/// entier au modèle à chaque question. Le plafond est volontairement large
/// pour couvrir un contrat ou un rapport typique.
fn extract_document_text(doc: &Document) -> String {
    const MAX_PAGES: usize = 40;
    const MAX_CHARS: usize = 60_000;

    let total = doc.page_count().min(MAX_PAGES);
    let mut out = String::new();
    for page in 1..=total as u32 {
        if let Ok(text) = doc.page_text(page) {
            out.push_str(&text.replace('\u{c}', "\n"));
            out.push('\n');
        }
    }
    if out.chars().count() > MAX_CHARS {
        out = out.chars().take(MAX_CHARS).collect();
    }
    out
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
    // Texte blanc sur l'accent ; la teinte du thème ailleurs, pour rester
    // lisible quand le thème est clair.
    let text_color = if primary || selected {
        Color32::WHITE
    } else {
        pal().icon_bright
    };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        FontId::proportional(BODY),
        text_color,
    );
    let width = 12.0
        + 15.0
        + 7.0
        + galley.size().x
        + if chevron { 15.0 } else { 0.0 }
        + 12.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 26.0), Sense::click());
    if primary || selected {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), pal().hover_bg);
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
        icon_chevron(ui.painter(), ch_rect, pal().muted);
    }
    resp
}

/// Ligne de menu cochable : la place de la coche est réservée même quand
/// l'entrée ne l'est pas, sinon les libellés d'un même groupe danseraient
/// d'une ligne à l'autre.
fn menu_row_checked(ui: &mut egui::Ui, label: &str, checked: bool) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), pal().hover_bg);
    }
    if checked {
        glyph(
            ui.painter(),
            Rect::from_center_size(pos2(rect.min.x + 18.0, rect.center().y), vec2(15.0, 15.0)),
            pal().accent,
            ph::CHECK,
        );
    }
    ui.painter().text(
        pos2(rect.min.x + 32.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(BODY),
        pal().icon_bright,
    );
    resp.clicked()
}

/// Séparateur de menu, aligné sur la largeur des lignes.
fn menu_separator(ui: &mut egui::Ui) {
    ui.add_space(SPACE_S);
    let y = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        y,
        Stroke::new(1.0_f32, pal().separator),
    );
    ui.add_space(SPACE_M);
}

/// Entrée qui ouvre un sous-menu au clic.
fn menu_submenu(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let text = RichText::new(format!("{label}  ▸")).size(BODY);
    ui.menu_button(text, |ui| {
        ui.set_min_width(180.0);
        add(ui);
    });
}

/// Ligne d'un sous-menu.
fn submenu_row(ui: &mut egui::Ui, label: &str, icon: Option<fn(&egui::Painter, egui::Rect, egui::Color32)>) -> bool {
    let desired_width = if icon.is_some() { 200.0 } else { 180.0 };
    let button = Button::new(RichText::new(label))
        .min_size(vec2(desired_width, 24.0));
    let resp = ui.add(button);
    // Draw icon if provided, positioned to the right of the label
    if let Some(draw_icon) = icon {
        let icon_size = 14.0; // Smaller icon for submenus
        let icon_x = resp.rect.min.x + resp.rect.width() - 10.0 - icon_size;
        let icon_rect = egui::Rect::from_min_max(
            egui::pos2(icon_x, resp.rect.min.y + (resp.rect.height() - icon_size) / 2.0),
            egui::pos2(icon_x + icon_size, resp.rect.max.y - (resp.rect.height() - icon_size) / 2.0),
        );
        draw_icon(ui.painter(), icon_rect, pal().icon_bright);
    }
    resp.clicked
}

/// Ligne de menu `Outils` : libellé à gauche, raccourci à droite.
fn menu_row(ui: &mut egui::Ui, label: &str, shortcut: &str, icon: Option<fn(&egui::Painter, egui::Rect, egui::Color32)>) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), pal().hover_bg);
    }
    let label_x = rect.min.x + 10.0;
    let shortcut_x = rect.max.x - 10.0;
    let icon_size = 16.0;
    let icon_x = label_x + 20.0 + icon_size;
    // Position icon only if provided and there's space
    if let Some(draw_icon) = icon {
        let icon_rect = egui::Rect::from_min_max(
            egui::pos2(icon_x, rect.min.y + (rect.height() - icon_size) / 2.0),
            egui::pos2(icon_x + icon_size, rect.max.y - (rect.height() - icon_size) / 2.0),
        );
        draw_icon(ui.painter(), icon_rect, pal().icon_bright);
    }
    ui.painter().text(
        pos2(label_x, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(BODY),
        pal().icon_bright,
    );
    ui.painter().text(
        pos2(shortcut_x, rect.center().y),
        Align2::RIGHT_CENTER,
        shortcut,
        FontId::proportional(LABEL),
        pal().muted,
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
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_L), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_L), pal().widget_bg);
    }
    let color = if selected {
        Color32::WHITE
    } else if resp.hovered() {
        pal().icon_bright
    } else {
        pal().icon
    };
    draw(ui.painter(), rect.shrink(9.0), color);
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
        ui.painter().rect_filled(rect, Rounding::same(RADIUS_M), pal().widget_bg);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, Rounding::same(RADIUS_M), Color32::from_rgb(0x33, 0x33, 0x33));
    }
    let color = if selected || resp.hovered() { pal().icon_bright } else { pal().icon };
    draw(ui.painter(), rect.shrink(8.0), color);
    resp
}

// --------------------------------------------------------------------------
// Icônes Phosphor (police d'icônes, licence MIT)
// --------------------------------------------------------------------------

/// Petit bouton d'icône de la barre d'état.
fn status_button(ui: &mut egui::Ui, glyph: &str, tip: &str) -> bool {
    let resp = ui
        .add(Button::new(RichText::new(glyph).size(TITLE).color(pal().icon)).frame(false))
        .on_hover_text(tip);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// Formats proposés à l'insertion d'une page blanche. `(0, 0)` reprend la
/// taille de la page voisine.
const BLANK_SIZES: [(&str, f32, f32); 7] = [
    ("Same as this page", 0.0, 0.0),
    ("A4 (595 × 842 pt)", 595.0, 842.0),
    ("A3 (842 × 1191 pt)", 842.0, 1191.0),
    ("A5 (420 × 595 pt)", 420.0, 595.0),
    ("Letter (612 × 792 pt)", 612.0, 792.0),
    ("Legal (612 × 1008 pt)", 612.0, 1008.0),
    ("Tabloid (792 × 1224 pt)", 792.0, 1224.0),
];

/// Opérations du menu contextuel des vignettes.
#[derive(Clone, Copy)]
enum PageAction {
    /// Rotation de `delta` degrés (multiple de 90).
    Rotate(i64),
    /// Page blanche `largeur × hauteur` insérée après la dernière page visée.
    Blank(f32, f32),
    /// Insertion des pages d'un autre PDF.
    InsertFile,
    /// Extraction vers un nouveau fichier.
    Extract,
    /// Duplication des pages visées.
    Duplicate,
    /// Suppression des pages visées.
    Delete,
    /// Déplacement `(de, vers)`.
    Move(usize, usize),
    /// Ouverture de la fenêtre des propriétés de la page.
    Properties(usize),
}

/// Modifications du plan demandées par le panneau Signets.
enum BookmarkRequest {
    /// Renomme l'entrée visée.
    Rename(String, String),
    /// Supprime l'entrée visée.
    Delete(String),
    /// Ajoute une entrée (page courante) sous le parent visé, ou à la racine.
    Add(Option<String>, Option<usize>),
    /// Change la page visée.
    SetDestination(String, Option<usize>),
    /// Déplace l'entrée `from` sous `parent`, à la position `index`
    /// (`None` = en dernier).
    Move {
        from: String,
        parent: Option<String>,
        index: Option<usize>,
    },
}

/// Bouton d'action de la fenêtre d'organisation : icône puis libellé.
fn organize_button(ui: &mut egui::Ui, icon: IconFn, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(116.0, 28.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        Rounding::same(RADIUS_M),
        if resp.hovered() { pal().hover_bg } else { pal().widget_bg },
    );
    let color = if resp.hovered() { pal().icon_bright } else { pal().icon };
    icon(
        ui.painter(),
        Rect::from_center_size(pos2(rect.min.x + 15.0, rect.center().y), vec2(14.0, 14.0)),
        color,
    );
    ui.painter().text(
        pos2(rect.min.x + 28.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(BODY),
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

/// Message centré d'un panneau vide.
fn panel_hint(ui: &mut egui::Ui, title: &str, detail: &str) {
    ui.add_space(14.0);
    ui.vertical_centered(|ui| {
        ui.label(section(title));
        ui.add_space(SPACE_S);
        ui.label(muted(detail, LABEL));
    });
}

/// Aligne un rectangle de champ sur les bords et centres des champs voisins.
///

/// Libellé anglais d'un type de champ de formulaire.
fn form_kind_label(kind: &FormFieldKind) -> String {
    kind.display_name().to_owned()
}

/// Libellé anglais d'un type d'annotation (alias de
/// [`youpdf_core::annotate::AnnotationKind::display_name`]).
fn annot_label(kind: AnnotationKind) -> &'static str {
    kind.display_name()
}
/// Action demandée par la liste des pièces jointes.
enum AttachmentAction {
    /// Ouvrir dans l'application par défaut du système.
    Open(usize),
    /// Enregistrer à l'emplacement choisi.
    SaveAs(usize),
    /// Retirer du document.
    Delete(usize),
}

/// Action demandée par la liste des commentaires.
enum CommentAction {
    /// Aller à la page visée.
    Jump(usize),
    /// Supprimer le commentaire et ses réponses.
    Delete(String),
    /// Changer l'état de revue.
    SetState(String, AnnotationState),
    /// Ouvrir la zone de réponse sous ce commentaire.
    Reply(String),
    /// Envoyer une réponse.
    Send(String, String),
    /// Exporter la liste.
    Export(CommentFormat),
}

/// Distance au présent, en clair (« 5 min », « 3 h », « 2 d »).
fn relative_time(when: &str, now: chrono::DateTime<chrono::Utc>) -> String {
    let Ok(then) = chrono::DateTime::parse_from_rfc3339(when) else {
        return when.get(..10).unwrap_or("").to_owned();
    };
    let seconds = (now - then.with_timezone(&chrono::Utc)).num_seconds();
    if seconds < 60 {
        return "now".to_owned();
    }
    let minutes = seconds / 60;
    if minutes < 60 {
        return format!("{minutes} min");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours} h");
    }
    let days = hours / 24;
    if days < 30 {
        return format!("{days} d");
    }
    let months = days / 30;
    if months < 12 {
        return format!("{months} mo");
    }
    format!("{} y", months / 12)
}


/// Icône du marqueur d'état.
fn comment_state_icon(state: AnnotationState) -> &'static str {
    match state {
        AnnotationState::None => ph::CHAT_CIRCLE,
        AnnotationState::Accepted => ph::CHECK_CIRCLE,
        AnnotationState::Rejected => ph::X_CIRCLE,
        AnnotationState::Cancelled => ph::PROHIBIT,
        AnnotationState::Completed => ph::CHECK,
    }
}

/// Couleur du marqueur d'état.
fn comment_state_color(state: AnnotationState) -> Color32 {
    match state {
        AnnotationState::None => pal().muted,
        AnnotationState::Accepted => Color32::from_rgb(72, 168, 104),
        AnnotationState::Rejected => Color32::from_rgb(206, 88, 84),
        AnnotationState::Cancelled => Color32::from_rgb(206, 158, 72),
        AnnotationState::Completed => pal().icon_bright,
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
        // Une réponse vit dans le fil de discussion, pas sur la page.
        if annot.page != page as usize || annot.reply_to.is_some() {
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
        // Une mise en évidence posée sur du texte porte un quadrilatère par
        // ligne ; sans quadrilatère, le rectangle fait la boîte.
        let quad_rect = |q: [f32; 8]| -> Rect {
            let points = [
                to_screen(q[0], q[1]),
                to_screen(q[2], q[3]),
                to_screen(q[4], q[5]),
                to_screen(q[6], q[7]),
            ];
            let mut rect = Rect::from_min_max(points[0], points[0]);
            for point in points.iter().skip(1) {
                rect = rect.union(Rect::from_min_max(*point, *point));
            }
            rect
        };
        let blocks: Vec<Rect> = if annot.quads.is_empty() {
            vec![r]
        } else {
            annot.quads.iter().map(|q| quad_rect(*q)).collect()
        };

        match annot.kind {
            AnnotationKind::Highlight => {
                for block in &blocks {
                    painter.rect_filled(*block, Rounding::same(RADIUS_XS), fill);
                }
            }
            AnnotationKind::Squiggly => {
                // Même ondulation que l'apparence enregistrée : deux crans
                // par motif, posés sur le bas de la boîte.
                let amp = (3.0 * scale).max(1.0);
                for block in &blocks {
                    let steps = ((block.width() / (4.0 * scale)).ceil() as usize * 2).max(4);
                    let pts: Vec<egui::Pos2> = (0..=steps)
                        .map(|i| {
                            let t = i as f32 / steps as f32;
                            pos2(
                                block.min.x + t * block.width(),
                                block.max.y - if i % 2 == 0 { amp * 0.6 } else { amp * 1.8 },
                            )
                        })
                        .collect();
                    painter.add(Shape::line(pts, Stroke::new(width, color)));
                }
            }
            AnnotationKind::Underline => {
                for block in &blocks {
                    painter.line_segment(
                        [pos2(block.min.x, block.max.y), pos2(block.max.x, block.max.y)],
                        Stroke::new(width, color),
                    );
                }
            }
            AnnotationKind::StrikeOut => {
                for block in &blocks {
                    painter.line_segment(
                        [
                            pos2(block.min.x, block.center().y),
                            pos2(block.max.x, block.center().y),
                        ],
                        Stroke::new(width, color),
                    );
                }
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
                painter.rect_filled(r, Rounding::same(RADIUS_TINY), fill);
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
                let lines: Vec<&str> = annot
                    .content
                    .lines()
                    .filter(|line| !line.is_empty())
                    .collect();
                let first = font.max(9.0);
                let leading = first * 1.15;
                let block = leading * lines.len() as f32;
                let top = r.center().y - block / 2.0 + leading / 2.0;
                for (index, line) in lines.iter().enumerate() {
                    let size = if index == 0 { first } else { first * 0.72 };
                    painter.text(
                        pos2(r.center().x, top + index as f32 * leading),
                        Align2::CENTER_CENTER,
                        *line,
                        FontId::proportional(size),
                        color,
                    );
                }
            }
            AnnotationKind::Circle => {
                painter.add(Shape::circle_stroke(
                    r.center(),
                    r.width().min(r.height()) / 2.0,
                    Stroke::new(width, color),
                ));
            }
            AnnotationKind::Polygon => {
                // Dessine le polygone à partir des points d'encre.
                let pts: Vec<egui::Pos2> = annot
                    .ink_points
                    .first()
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|p| to_screen(p[0], p[1]))
                    .collect();
                if pts.len() >= 3 {
                    // Ferme le polygone en reliant le dernier au premier.
                    let mut pts_closed = pts.clone();
                    pts_closed.push(pts[0]);
                    painter.add(Shape::convex_polygon(pts_closed, color, Stroke::NONE));
                }
            }
            // AnnotationKind::PolyLine => {
            //     // Dessine la polyligne ouverte.
            //     let pts: Vec<egui::Pos2> = annot
            //         .ink_points
            //         .first()
            //         .cloned()
            //         .unwrap_or_default()
            //         .iter()
            //         .map(|p| to_screen(p[0], p[1]))
            //         .collect();
            //     if pts.len() >= 2 {
            //         painter.add(Shape::line_strip(pts, Stroke::new(width, color)));
            //     }
            // }
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
            painter.rect_stroke(r.expand(3.0), 2.0, Stroke::new(1.5_f32, pal().accent));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use youpdf_core::TextChar;

    fn cachet(rows: &[Vec<u32>]) -> HashSet<u32> {
        rows.iter().flatten().copied().collect()
    }

    fn history_entry(marker: u8, len: usize) -> HistoryEntry {
        HistoryEntry {
            doc: Some(vec![marker; len]),
            annots: Vec::new(),
        }
    }

    fn un_onglet_vide() -> Tab {
        Tab {
            doc: blank_document(1, 200.0, 200.0).expect("page de test"),
            path: None,
            title: "test".to_owned(),
            page: 1,
            zoom: None,
            zoom_fit_page: true,
            cache: HashMap::new(),
            page_sizes: Vec::new(),
            last_bitmap: None,
            dirty: false,
            structural_dirty: false,
            annots: Vec::new(),
            baseline_annots: Vec::new(),
            annots_loaded: true,
            undo: Vec::new(),
            redo: Vec::new(),
            history_base: None,
            saved_hash: 0,
            outline: Vec::new(),
            forms: Vec::new(),
            attachments: Vec::new(),
            fragments: HashMap::new(),
            structure_loaded: false,
            pinned: false,
            protected: false,
        }
    }

    /// Un nouveau rendu du même palier réécrit la texture existante : même
    /// `TextureId`, et un delta de mise à jour à la place d'une allocation.
    ///
    /// `Context::load_texture` allouait une texture neuve à chaque rendu ;
    /// l'ancienne, faute de poignée, restait au GPU. Un zoom en fuyait une
    /// par palier et par page.
    #[test]
    fn un_rendu_reutilise_la_texture_du_palier() {
        let ctx = egui::Context::default();
        let renderer = NullRenderer;
        let mut tab = un_onglet_vide();
        let mut status = String::new();

        let premier = YouPdfApp::page_texture(
            &ctx, &renderer, &mut tab, 1, 10, 1.0, 1.0, true, false, &mut status,
        )
        .expect("premier rendu");
        let id = premier.id();
        assert!(status.is_empty(), "{status}");
        let textures_avant = ctx.tex_manager().read().num_allocated();

        // Même palier (10), mais 20 % plus grand que la rastérisation en
        // cache : elle est périmée, donc le rendu repart.
        let second = YouPdfApp::page_texture(
            &ctx, &renderer, &mut tab, 1, 10, 1.2, 1.0, true, false, &mut status,
        )
        .expect("second rendu");
        assert_eq!(second.id(), id, "la texture du palier est réutilisée");

        // Le rendu a bien eu lieu : le bitmap fait 240×240 et non 200×200.
        let delta = ctx.tex_manager().write().take_delta();
        let mises_a_jour: Vec<_> = delta.set.iter().filter(|(other, _)| *other == id).collect();
        assert_eq!(mises_a_jour.len(), 1, "un seul delta pour la texture");
        assert!(
            mises_a_jour[0].1.is_whole(),
            "le delta remplace l'image entière"
        );
        assert_eq!(mises_a_jour[0].1.image.size(), [240, 240]);
        assert!(
            !delta.free.contains(&id),
            "la texture réutilisée ne doit pas être libérée"
        );

        // Aucune texture n'a été allouée en plus : uniquement mise à jour.
        assert_eq!(
            ctx.tex_manager().read().num_allocated(),
            textures_avant,
            "le second rendu ne doit pas allouer de texture"
        );
    }

    #[test]
    fn history_drops_oldest_beyond_count_and_bytes() {
        let mut by_count: Vec<HistoryEntry> = (0..5).map(|n| history_entry(n, 1)).collect();
        history::trim_history_to(&mut by_count, 3, usize::MAX);
        assert_eq!(by_count.len(), 3);
        // Les plus récents restent, dans l'ordre.
        let markers: Vec<u8> = by_count
            .iter()
            .map(|entry| entry.doc.as_ref().unwrap()[0])
            .collect();
        assert_eq!(markers, vec![2, 3, 4]);

        let mut by_bytes = vec![history_entry(0, 4), history_entry(1, 4), history_entry(2, 4)];
        history::trim_history_to(&mut by_bytes, 10, 8);
        assert_eq!(by_bytes.len(), 2);
        assert_eq!(by_bytes[0].doc.as_ref().unwrap()[0], 1);
        assert_eq!(by_bytes[1].doc.as_ref().unwrap()[0], 2);
    }

    /// L'onglet actif suit son document à travers un réordonnancement.
    #[test]
    fn active_tab_follows_a_move() {
        // [A,B,C], active C (2). Déplacer A (0) vers 1 -> [B,A,C] : C reste à 2.
        assert_eq!(moved_active(2, 0, 1), 2);
        // [A,B,C], active A (0). Déplacer C (2) vers 0 -> [C,A,B] : A passe à 1.
        assert_eq!(moved_active(0, 2, 0), 1);
        // L'onglet actif lui-même est déplacé : il va là où on le dépose.
        assert_eq!(moved_active(1, 1, 2), 2);
        // [A,B,C], active A. Déplacer B (1) vers 0 -> [B,A,C] : A passe à 1.
        assert_eq!(moved_active(0, 1, 0), 1);
        // Déplacement sans effet sur les index voisins.
        assert_eq!(moved_active(0, 1, 2), 0);
    }

    /// La recherche de texte respecte la casse et retrouve une sous-chaîne.
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

    /// Bloc de texte de test : rectangle et corps suffisent.
    fn frag(text: &str, rect: [f32; 4], font_size: f32) -> TextFragment {
        TextFragment {
            page: 1,
            chars: Vec::new(),
            text: text.to_string(),
            rect,
            font: None,
            font_size,
            color: [0, 0, 0],
        }
    }

    /// Bloc de texte avec une boîte par caractère, comme pdfium.
    fn frag_chars(text: &str, rect: [f32; 4], font_size: f32) -> TextFragment {
        let mut fragment = frag(text, rect, font_size);
        let count = text.chars().count().max(1) as f32;
        let width = (rect[2] - rect[0]) / count;
        fragment.chars = text
            .chars()
            .enumerate()
            .map(|(index, ch)| TextChar {
                ch,
                rect: [
                    rect[0] + index as f32 * width,
                    rect[1],
                    rect[0] + (index + 1) as f32 * width,
                    rect[3],
                ],
            })
            .collect();
        fragment
    }

    /// La sélection suit l'ordre de lecture et joint les blocs correctement.
    #[test]
    fn selection_follows_reading_order() {
        // Deux lignes de deux blocs ; les blocs sont donnés dans le désordre.
        let frags = vec![
            frag("monde", [60.0, 82.0, 110.0, 94.0], 12.0),
            frag("bonjour", [10.0, 82.0, 55.0, 94.0], 12.0),
            frag("seconde", [10.0, 60.0, 60.0, 72.0], 12.0),
        ];

        assert_eq!(
            hit_runs(&frags, None),
            vec![(1, 0, 7), (0, 0, 5), (2, 0, 7)]
        );
        assert_eq!(hit_selection_text(&frags, None), "bonjour monde\nseconde");

        // Du premier bloc de la première ligne au bloc de la troisième.
        let span = Some((
            TextHit { fragment: 1, offset: 0 },
            TextHit { fragment: 2, offset: 7 },
        ));
        let runs = hit_runs(&frags, span);
        assert_eq!(runs.first().map(|run| run.0), Some(1));
        assert_eq!(runs.last().map(|run| run.0), Some(2));
        assert_eq!(hit_selection_text(&frags, span), "bonjour monde\nseconde");

        // Une sélection qui s'arrête dans le premier mot ne prend que lui.
        let span = Some((
            TextHit { fragment: 1, offset: 0 },
            TextHit { fragment: 1, offset: 7 },
        ));
        assert_eq!(hit_selection_text(&frags, span), "bonjour");
    }

    /// La sélection s'arrête au caractère, pas au bloc.
    #[test]
    fn selection_stops_mid_word() {
        let frags = vec![frag_chars("bonjour", [10.0, 82.0, 80.0, 94.0], 12.0)];
        let span = Some((
            TextHit { fragment: 0, offset: 1 },
            TextHit { fragment: 0, offset: 3 },
        ));
        assert_eq!(hit_selection_text(&frags, span), "on");
        let rects = hit_rects(&frags, span);
        assert_eq!(rects.len(), 1, "une seule ligne");
        assert!((rects[0][0] - 20.0).abs() < 0.01, "{rects:?}");
        assert!((rects[0][2] - 40.0).abs() < 0.01, "{rects:?}");
    }

    /// Une sélection qui traverse deux fragments coupe le premier et le second.
    #[test]
    fn selection_crosses_fragments_mid_word() {
        let frags = vec![
            frag_chars("ab", [10.0, 100.0, 30.0, 112.0], 12.0),
            frag_chars("cd", [40.0, 100.0, 60.0, 112.0], 12.0),
        ];
        let span = Some((
            TextHit { fragment: 0, offset: 1 },
            TextHit { fragment: 1, offset: 1 },
        ));
        assert_eq!(
            hit_runs(&frags, span),
            vec![(0, 1, 2), (1, 0, 1)]
        );
        assert_eq!(hit_selection_text(&frags, span), "b c");
    }

    /// Un point tombe sur la frontière de caractère la plus proche.
    #[test]
    fn text_hit_snaps_to_the_nearest_character_edge() {
        let frags = vec![frag_chars("abc", [10.0, 100.0, 40.0, 112.0], 12.0)];
        let hit = text_hit(&frags, [31.0, 106.0]).expect("un caractère");
        assert_eq!(hit, TextHit { fragment: 0, offset: 2 });
    }

    /// Une mise en évidence pose un quadrilatère par ligne de texte.
    #[test]
    fn markup_quads_un_par_ligne() {
        let frags = vec![
            frag("bonjour", [10.0, 82.0, 55.0, 94.0], 12.0),
            frag("monde", [60.0, 82.0, 110.0, 94.0], 12.0),
            frag("seconde", [10.0, 60.0, 60.0, 72.0], 12.0),
        ];
        let quads = hit_quads(&frags, None);

        assert_eq!(quads.len(), 2, "une ligne, puis l'autre");
        assert_eq!(
            quads[0],
            [10.0, 94.0, 110.0, 94.0, 10.0, 82.0, 110.0, 82.0]
        );
        assert_eq!(quads[1], [10.0, 72.0, 60.0, 72.0, 10.0, 60.0, 60.0, 60.0]);
    }

    /// Un point hors de tout bloc retombe sur le plus proche.
    #[test]
    fn selection_snaps_to_the_closest_block() {
        let frags = vec![
            frag("gauche", [10.0, 100.0, 60.0, 112.0], 12.0),
            frag("droite", [200.0, 100.0, 250.0, 112.0], 12.0),
        ];
        let hit = text_hit(&frags, [400.0, 40.0]).expect("un bloc");
        assert_eq!(hit.fragment, 1);
        let span = Some((TextHit { fragment: 0, offset: 0 }, hit));
        assert_eq!(hit_selection_text(&frags, span), "gauche droite");
    }

    /// Deux blocs presque à la même hauteur restent sur une seule ligne.
    #[test]
    fn same_line_blocks_join_with_a_space() {
        let frags = vec![
            frag("a", [10.0, 100.0, 20.0, 112.0], 12.0),
            frag("b", [30.0, 101.0, 40.0, 113.0], 12.0),
        ];
        assert_eq!(hit_selection_text(&frags, None), "a b");
    }

    /// Une sélection vide ou un index absent ne paniquent pas.
    #[test]
    fn empty_selection_is_empty() {
        assert!(hit_runs(&[], None).is_empty());
        assert_eq!(hit_selection_text(&[], None), "");
        let frags = vec![frag("seul", [0.0, 0.0, 10.0, 10.0], 10.0)];
        let hit = text_hit(&frags, [900.0, 900.0]).expect("le bloc unique");
        assert_eq!(hit.fragment, 0);
    }

    /// Des corps de police différents sur des lignes voisines ne doivent pas
    /// mélanger l'ordre de lecture.
    #[test]
    fn reading_order_survives_mixed_font_sizes() {
        let frags = vec![
            frag("corps", [10.0, 100.0, 60.0, 112.0], 12.0),
            frag("titre", [10.0, 118.0, 70.0, 134.0], 16.0),
            frag("bas", [10.0, 88.0, 55.0, 100.0], 12.0),
        ];
        assert_eq!(hit_selection_text(&frags, None), "titre\ncorps\nbas");
    }

    /// Deux corps différents sur une même ligne y restent.
    #[test]
    fn reading_order_keeps_one_line_together() {
        let frags = vec![
            frag("grand", [40.0, 98.0, 90.0, 118.0], 20.0),
            frag("petit", [10.0, 100.0, 35.0, 112.0], 12.0),
        ];
        assert_eq!(hit_selection_text(&frags, None), "petit grand");
    }








    /// L'horodatage relatif se lit par paliers.
    #[test]
    fn relative_time_follows_its_buckets() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-15T12:00:00Z")
            .expect("date de référence")
            .with_timezone(&chrono::Utc);

        assert_eq!(relative_time("2026-06-15T11:59:30Z", now), "now");
        assert_eq!(relative_time("2026-06-15T11:30:00Z", now), "30 min");
        assert_eq!(relative_time("2026-06-15T07:00:00Z", now), "5 h");
        assert_eq!(relative_time("2026-06-12T12:00:00Z", now), "3 d");
        // Date illisible : on retombe sur le jour seul.
        assert_eq!(relative_time("hier", now), "");
    }




    /// L'écran de démarrage suit la préférence enregistrée : `home` ouvre
    /// l'accueil, `blank` ouvre une page vierge et ferme l'accueil.
    #[test]
    fn startup_view_suit_la_preference() {
        let prefs = youpdf_ai::AppSettings::default();
        assert_eq!(prefs.startup, "home", "l'accueil par défaut");
        assert!(prefs.startup != "blank");
        assert_eq!(
            youpdf_ai::AppSettings {
                startup: "blank".to_string(),
                ..Default::default()
            }
            .startup,
            "blank"
        );
    }

    /// Un double-clic prend le mot entier, pas le caractère sous le curseur.
    #[test]
    fn expand_to_word_prend_le_mot_entier() {
        let frags = vec![frag("le monde entier", [10.0, 100.0, 130.0, 112.0], 12.0)];
        // Le curseur est sur « monde », caractère 4.
        let hit = TextHit {
            fragment: 0,
            offset: 4,
        };
        let (start, end) = expand_to_word(&frags, hit).expect("un mot");
        assert_eq!(start.offset, 3, "début de « monde »");
        assert_eq!(end.offset, 8, "fin de « monde »");
    }

    /// Un double-clic sur un mot à la fin du bloc s'arrête au bord.
    #[test]
    fn expand_to_word_eteint_le_dernier_mot() {
        let frags = vec![frag("bonjour", [10.0, 100.0, 70.0, 112.0], 12.0)];
        let hit = TextHit {
            fragment: 0,
            offset: 6,
        };
        let (start, end) = expand_to_word(&frags, hit).expect("un mot");
        assert_eq!(start.offset, 0);
        assert_eq!(end.offset, 7);
    }

    /// Un clic dans un espace prend le mot suivant, pas le précédent.
    #[test]
    fn expand_to_word_ignore_le_dernier_espace() {
        let frags = vec![frag("un deux", [10.0, 100.0, 70.0, 112.0], 12.0)];
        // L'offset 2 est l'espace entre « un » et « deux ».
        let hit = TextHit {
            fragment: 0,
            offset: 2,
        };
        let (start, end) = expand_to_word(&frags, hit).expect("un mot");
        assert_eq!(start.offset, 3, "début de « deux »");
        assert_eq!(end.offset, 7);
    }

    /// Un triple-clic prend la ligne entière, même quand elle est faite de
    /// plusieurs fragments.
    #[test]
    fn expand_to_line_prend_la_ligne_entiere() {
        let frags = vec![
            frag("bonjour", [10.0, 82.0, 55.0, 94.0], 12.0),
            frag("monde", [60.0, 82.0, 110.0, 94.0], 12.0),
            frag("seconde", [10.0, 60.0, 60.0, 72.0], 12.0),
        ];
        let hit = TextHit {
            fragment: 1,
            offset: 2,
        };
        let (start, end) = expand_to_line(&frags, hit).expect("une ligne");
        assert_eq!(start.fragment, 0, "début de la première ligne");
        assert_eq!(start.offset, 0);
        assert_eq!(end.fragment, 1, "fin de la première ligne");
        assert_eq!(end.offset, 5);
    }

    /// Un triple-clic ne déborde pas sur la ligne du dessous.
    #[test]
    fn expand_to_line_ignore_la_ligne_suivante() {
        let frags = vec![
            frag("bonjour", [10.0, 82.0, 55.0, 94.0], 12.0),
            frag("seconde", [10.0, 60.0, 60.0, 72.0], 12.0),
        ];
        let hit = TextHit {
            fragment: 1,
            offset: 1,
        };
        let (start, end) = expand_to_line(&frags, hit).expect("une ligne");
        assert_eq!(start.fragment, 1);
        assert_eq!(end.fragment, 1);
        assert_eq!(end.offset, 7);
    }

    /// La mise en évidence d'une sélection suit les lignes du texte : un
    /// quadrilatère par ligne, jamais un rectangle posé à l'aveugle.
    #[test]
    fn markup_suit_les_lignes_du_texte() {
        let frags = vec![
            frag_chars("bonjour", [10.0, 82.0, 55.0, 94.0], 12.0),
            frag_chars("monde", [60.0, 82.0, 110.0, 94.0], 12.0),
        ];
        let span = Some((
            TextHit {
                fragment: 0,
                offset: 0,
            },
            TextHit {
                fragment: 1,
                offset: 5,
            },
        ));
        let quads = hit_quads(&frags, span);
        // Une seule ligne visuelle : les deux fragments sont voisins.
        assert_eq!(quads.len(), 1, "{quads:?}");
        let quad = quads[0];
        // Le quadrilatère va du début de « bonjour » à la fin de « monde ».
        assert!(quad[0] <= 10.1 && quad[0] >= 9.9, "{quad:?}");
        assert!(quad[2] >= 109.9 && quad[2] <= 110.1, "{quad:?}");
    }

    /// Une sélection qui traverse deux lignes donne deux quadrilatères.
    #[test]
    fn markup_suit_chaque_ligne() {
        let frags = vec![
            frag_chars("bonjour", [10.0, 82.0, 55.0, 94.0], 12.0),
            frag_chars("monde", [10.0, 60.0, 55.0, 72.0], 12.0),
        ];
        let span = Some((
            TextHit {
                fragment: 0,
                offset: 0,
            },
            TextHit {
                fragment: 1,
                offset: 5,
            },
        ));
        let quads = hit_quads(&frags, span);
        assert_eq!(quads.len(), 2, "{quads:?}");
        // La ligne du bas est plus basse en repère page.
        assert!(quads[0][1] > quads[1][1], "{quads:?}");
    }

    /// Lien de test : rectangle et action.
    fn link(rect: [f32; 4], action: LinkAction) -> PageLink {
        PageLink {
            page: 1,
            rect,
            action,
        }
    }

    /// Un clic dans la boîte d'un lien le trouve.
    #[test]
    fn lien_sous_le_point_est_trouve() {
        let links = vec![link(
            [100.0, 700.0, 300.0, 720.0],
            LinkAction::Uri("https://example.org".to_owned()),
        )];
        let found = link_at(&links, [200.0, 710.0]).expect("le lien");
        assert_eq!(
            found.action,
            LinkAction::Uri("https://example.org".to_owned())
        );
    }

    /// Un clic à côté du lien ne le trouve pas.
    #[test]
    fn lien_hors_de_la_boite_est_ignore() {
        let links = vec![link(
            [100.0, 700.0, 300.0, 720.0],
            LinkAction::Uri("https://example.org".to_owned()),
        )];
        assert!(link_at(&links, [50.0, 710.0]).is_none());
        assert!(link_at(&links, [200.0, 650.0]).is_none());
    }

    /// Le lien du dessus l'emporte : c'est celui qu'on voit.
    #[test]
    fn lien_du_dessus_lemporte() {
        let links = vec![
            link(
                [100.0, 700.0, 300.0, 720.0],
                LinkAction::Uri("https://premier.example".to_owned()),
            ),
            link(
                [100.0, 700.0, 300.0, 720.0],
                LinkAction::Uri("https://second.example".to_owned()),
            ),
        ];
        let found = link_at(&links, [200.0, 710.0]).expect("un lien");
        assert_eq!(
            found.action,
            LinkAction::Uri("https://second.example".to_owned())
        );
    }

    /// La tolérance : un lien au pixel près reste touchable.
    #[test]
    fn lien_tolere_pres_du_bord() {
        let links = vec![link(
            [100.0, 700.0, 300.0, 720.0],
            LinkAction::Uri("https://example.org".to_owned()),
        )];
        // Deux points à l'intérieur du rectangle.
        assert!(link_at(&links, [102.0, 718.0]).is_some());
        // Un point juste à côté, dans la tolérance.
        assert!(link_at(&links, [98.5, 710.0]).is_some());
        // Un point plus loin, hors de la tolérance.
        assert!(link_at(&links, [90.0, 710.0]).is_none());
    }

    /// L'outil Lien du mode Édition ouvre bien « Ajouter un lien ».
    ///
    /// Les modes n'ont pas les mêmes outils : l'index 3 est « Comment » en
    /// mode Commentaire et « Lien » en mode Édition. Sans cette distinction,
    /// le clic demandait du texte.
    #[test]
    fn outil_lien_du_mode_edition_ouvre_un_lien() {
        assert_eq!(
            draft_kind_for(Some(1), Some(3)),
            DraftKind::Link,
            "l'outil Lien du mode Édition"
        );
        assert_eq!(
            draft_kind_for(Some(0), Some(3)),
            DraftKind::Note,
            "l'outil Comment du mode Commentaire"
        );
        // L'outil Lien du mode Commentaire reste un lien.
        assert_eq!(draft_kind_for(Some(0), Some(7)), DraftKind::Link);
    }
}
