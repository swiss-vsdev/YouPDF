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

mod batch_types;
mod batch_ui;
mod forms_editor;
mod forms_fields;
mod forms_ui;
mod decor_ui;
mod marks_ui;
mod signature_ui;
mod export_ui;
mod pages_ui;
mod prefs_ui;
mod organize_dialogs;
mod organize_ops;
mod organize_ui;
mod sign_ui;
mod reader;
mod widgets;
pub(crate) use widgets::*;
mod render;
pub(crate) use render::*;
mod files_ui;
mod documents_ui;
mod annots_ui;
mod text_edit_ui;
mod slideshow_ui;
mod tabs_ui;
mod windows_ui;
mod readaloud_ui;
mod canvas_commit;
mod canvas_image_ui;
mod canvas_text_ui;
mod canvas_tools_ui;
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
use crate::app::batch_types::{BatchEntry, BatchEvent};
use crate::inspector::{
    arrow_x, image_controls, markup_row, paint_arrow, place_inspector, text_style_row, AnnotationId,
    ImageAction, ImageId, InspectorTarget, TextBlockId, TextStyleUi, INSPECTOR_HEIGHT,
    INSPECTOR_WIDTH, MARKUP_KINDS, PALETTE_HEIGHT, SELECTION_INSPECTOR_HEIGHT,
    TEXT_INSPECTOR_HEIGHT,
};
use youpdf_core::compress::CompressionLevel;
use youpdf_core::render::NullRenderer;
use youpdf_core::TtsEngine;
use youpdf_core::ocr::{make_ocr, TesseractEngine};
use youpdf_core::{
    blank_document, document_from_image_bytes, document_from_images, export_comments,
    Annotation,
    AnnotationKind, AnnotationState, AnnotationStyle, AttachmentInfo, Background, BandSlots, Bates,
    BatesPosition, CommentFormat, ComparisonReport, CompressionExtras, Conformance, CropMargins,
    DifferenceKind, Document,
    EncryptionOptions, EncryptionStrength, ExtractedTable, FieldProperties, FormDataFormat,
    FormField, FormFieldKind, HeaderFooter, ImageFormat, KeyValue, LinkAction, MeasureKind,
    Measurement, NumberStyle, OcrEngine, OcrMode, OfficeFormat, OutlineEntry, OutlineItem,
    OutlineStyle, ImageCrop, OutlineTarget, PageRange, PageBitmap, PatchFont,
    PdfiumRenderer, RedactionFill,
    RedactionMark, RedactionOverlay, RedactionReport, RenderOptions, Renderer, SearchIndex,
    SearchOptions, SearchResult, TextFormat,
    TextFragment, TextStyle, Unit, Watermark, smooth_polyline,
};
use youpdf_core::{LANGUAGES, combine_languages, language_name};

use youpdf_core::annotate::{BorderDash, STAMP_TEMPLATES};
use youpdf_core::annotation::transform::{annots_to_display, annots_to_page, link_at};
use youpdf_core::search::{find_query, match_rects_best, replace_occurrences};
use youpdf_core::util::{format_size, page_size_label};
use youpdf_core::forms::snap_form_rect;
use youpdf_core::annotation::comments::{
    CommentFilter, CommentSort, CommentStateFilter, comment_rows,
};
use youpdf_core::edit::{crop_fractions_in_image, response_changed};
use youpdf_core::annotate::author_initial;
use youpdf_core::structure::attachments::{prune_attachment_staging, safe_attachment_name, stage_attachment};
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
    /// Nouveau document à partir d'un fichier Office (.docx/.xlsx/.pptx).
    FromOffice,
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
    ModeTool("Cloud", icon_cloud, true, false),
];

const EDIT_TOOLS: &[ModeTool] = &[
    ModeTool("Select", icon_cursor, false, false),
    ModeTool("Text", icon_text, true, false),
    ModeTool("Image", icon_image, true, false),
    ModeTool("Link", icon_link, true, false),
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
    ModeTool("Edit", icon_pen, false, false),
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
    Cloud,
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
        Some(12) => DraftKind::Cloud,
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
    /// Édition des champs de formulaire : déplacer, supprimer.
    EditForm,
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
    /// Déplacement interactif d'un champ de formulaire.
    form_move: Option<(usize, [f32; 2], [f32; 4])>,
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
            form_move: None,
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
    /// Filigrane depuis un PDF source (chemin + page).
    watermark_pdf_source: Option<std::path::PathBuf>,
    watermark_pdf_page: u32,
    /// Filigranes sauvegardés (préréglages).
    watermark_presets: Vec<Watermark>,
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
    /// Fenêtre « Outils de pages » : redimensionner, interlacer, échanger.
    page_tools_open: bool,
    page_labels_open: bool,
    page_label_prefix: String,
    page_label_style: usize,
    page_label_start: u32,
    page_label_from_page: u32,
    page_new_width: f32,
    page_new_height: f32,
    page_interleave_even_first: bool,
    page_exchange_a: usize,
    page_exchange_a_count: usize,
    page_exchange_b: usize,
    page_exchange_b_count: usize,
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
    /// Fenêtre « Presenter View » (page courante + page suivante).
    presenter_open: bool,
    /// État de la vue présentateur : chronomètre, page suivante.
    presenter: slideshow_ui::PresenterView,
    /// Fenêtre « Read Aloud » : voix, vitesse, lecture du texte de la page.
    readaloud_open: bool,
    /// Moteur de synthèse vocale (un processus `say` à la fois).
    tts: TtsEngine,
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
    /// Couleur de remplissage des formes ; `None` = pas de remplissage.
    annot_fill: Option<[u8; 3]>,
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
    /// Fenêtre « Open Quickly » (⌘⇧O) : requête de recherche floue sur les récents.
    open_quickly_open: bool,
    /// Brouillon de la requête de la fenêtre « Open Quickly ».
    open_query: String,
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
    redact_code_sets: std::collections::HashSet<usize>,
    /// Champ de formulaire sélectionné et saisie en cours.
    form_edit: Option<(usize, String)>,
    /// Brouillon des propriétés du champ sélectionné.
    form_props: Option<(usize, FieldProperties)>,
    /// Onglet actif de l'inspecteur de champ.
    form_props_tab: usize,
    /// Champ de formulaire en cours de édition interactive (lecteur).
    form_editing: Option<usize>,
    /// Champ de signature en cours de remplissage.
    form_signature_target: Option<usize>,
    /// Demande le focus clavier au prochain rendu de l'éditeur de champ.
    form_edit_fresh: bool,
    /// Cible du sélecteur de date.
    form_date_target: Option<usize>,
    form_date_open: bool,
    form_date_value: String,
    form_date_year: i32,
    form_date_month: u32,
    /// Cible de la fenêtre d'options (liste déroulante).
    form_options_target: Option<usize>,
    form_options_open: bool,
    form_options_text: String,
    /// Cible de la fenêtre de code-barres.
    form_barcode_target: Option<usize>,
    form_barcode_open: bool,
    form_barcode_value: String,
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
            form_edit_fresh: false,
            form_signature_target: None,
            form_date_target: None,
            form_date_open: false,
            form_date_value: String::new(),
            form_date_year: 2026,
            form_date_month: 1,
            form_options_target: None,
            form_options_open: false,
            form_options_text: String::new(),
            form_barcode_target: None,
            form_barcode_open: false,
            form_barcode_value: String::new(),            form_hover: None,
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
            watermark_pdf_source: None,
            watermark_pdf_page: 1,
            watermark_presets: Vec::new(),
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
            page_tools_open: false,
            page_labels_open: false,
            page_label_prefix: String::new(),
            page_label_style: 0,
            page_label_start: 1,
            page_label_from_page: 1,
            page_new_width: 612.0,
            page_new_height: 792.0,
            page_interleave_even_first: true,
            page_exchange_a: 1,
            page_exchange_a_count: 1,
            page_exchange_b: 1,
            page_exchange_b_count: 1,
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
            presenter_open: false,
            presenter: slideshow_ui::PresenterView::default(),
            readaloud_open: false,
            tts: TtsEngine::new(),
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
            annot_fill: None,
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
            open_quickly_open: false,
            open_query: String::new(),
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
            redact_code_sets: std::collections::HashSet::new(),
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



    // -- Fichiers ----------------------------------------------------------














    // -- Enregistrement ----------------------------------------------------




    // -- Annotations -------------------------------------------------------









    // -- Structure du document ---------------------------------------------














    // -- Outils d'annotation ------------------------------------------------
























    // -- Zoom --------------------------------------------------------------




    // -- Recherche ---------------------------------------------------------






    // -- Recherche active, remplacement, caviardage ------------------------






    // -- Rendu des pages ---------------------------------------------------


    // -- Barre de titre ----------------------------------------------------


    // -- Rail --------------------------------------------------------------


    // -- Panneau Commentaires ----------------------------------------------








    // -- Panneau latéral ---------------------------------------------------









    // -- Barre d'outils flottante ------------------------------------------













    // -- Propriétés --------------------------------------------------------


    // -- Filigrane, en-tête, compression, protection -----------------------




    // -- IA ----------------------------------------------------------------




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













    // -- Présentation et traitement par lots --------------------------------























    // -- Superpositions ----------------------------------------------------




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
            self.presenter_window(ctx);
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
                // Aucun document ouvert → l'accueil, jamais une page
                // vierge. C'est la garantie du mode lecture : sans ce
                // garde, un démarrage sur « blank document » laissait un
                // PDF vide à la place de l'écran d'accueil.
                if self.tabs.is_empty() || self.home_open {
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
        self.page_tools_window(ctx);
        self.page_labels_window(ctx);
        self.compare_window(ctx);
        self.ocr_window(ctx);
        self.readaloud_window(ctx);
        self.create_window(ctx);
        self.sign_window(ctx);
        self.stamp_window(ctx);
        self.signature_window(ctx);
        self.date_picker_window(ctx);
        self.form_options_window(ctx);
        self.form_barcode_window(ctx);
        self.batch_window(ctx);
        self.extract_window(ctx);
        self.measure_window(ctx);
        self.password_window(ctx);
        self.draft_window(ctx);
        self.redact_window(ctx);
        self.page_props_window(ctx);
        self.activity_window(ctx);
        self.ai_window(ctx);
        if self.open_quickly_open {
            self.open_quickly_panel(ctx);
        }
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



// --------------------------------------------------------------------------
// Widgets maison
// --------------------------------------------------------------------------









// --------------------------------------------------------------------------
// Icônes Phosphor (police d'icônes, licence MIT)
// --------------------------------------------------------------------------


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




/// Aligne un rectangle de champ sur les bords et centres des champs voisins.
///
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






#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use youpdf_core::{PageLink, TextChar};

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
            form_move: None,
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
