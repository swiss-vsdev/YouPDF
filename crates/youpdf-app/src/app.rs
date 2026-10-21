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
    TextEdit, TextureHandle, TextureOptions, TopBottomPanel, ViewportCommand, CursorIcon,
};
use egui_phosphor::regular as ph;

mod batch_ui;
mod forms_ui;
mod decor_ui;
mod export_ui;
mod pages_ui;
mod prefs_ui;
mod organize_ui;

use youpdf_shared::{i18n, pal, set_palette, t, Language, Theme};
use crate::ai::AiPanel;
use crate::home::{self, HomeCategory, HomeView, ViewMode};
use crate::inspector::{
    arrow_x, image_controls, paint_arrow, place_inspector, text_style_row, AnnotationId,
    ImageAction, ImageId, InspectorTarget, TextBlockId, TextStyleUi, INSPECTOR_HEIGHT,
    INSPECTOR_WIDTH, PALETTE_HEIGHT, TEXT_INSPECTOR_HEIGHT,
};
use youpdf_core::compress::CompressionLevel;
use youpdf_core::render::NullRenderer;
use youpdf_core::ocr::{make_ocr, TesseractEngine};
use youpdf_core::{
    blank_document, document_from_images, export_comments, fold_case, Annotation, AnnotationKind,
    AnnotationState, AnnotationStyle, AttachmentInfo, Background, BandSlots, Bates, BatesPosition,
    CommentFormat, ComparisonReport, CompressionExtras, Conformance, CropMargins, DifferenceKind, Document,
    EncryptionOptions, EncryptionStrength, ExtractedTable, FieldProperties, FormDataFormat,
    FormField, FormFieldKind, HeaderFooter, ImageFormat, KeyValue, MeasureKind, Measurement,
    NumberStyle, OcrEngine, OcrMode, OfficeFormat, OutlineEntry, OutlineItem, OutlineStyle,
    ImageCrop, OutlineTarget, PageRange, PageBitmap, PatchFont, PdfiumRenderer, RedactionFill,
    RedactionMark, RedactionOverlay, RedactionReport, RenderOptions, Renderer, SearchIndex,
    SearchOptions, SearchResult, TextChar, TextFormat,
    TextFragment, TextStyle, Unit, Watermark, STAMP_NAMES, smooth_polyline,
};

use youpdf_core::annotate::BorderDash;

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
const STATUS_H: f32 = 26.0;
const RAIL_W: f32 = 52.0;
const PANEL_W: f32 = 210.0;
const PAGE_SPACING: f32 = 16.0;

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

/// Fractions de rognage d'un rectangle, exprimées dans le repère de l'image.
///
/// La matrice `[a b c d e f]` envoie le carré unité de l'image sur la page :
/// `(a, b)` est son axe horizontal, `(c, d)` son axe vertical, `(e, f)` son
/// coin bas-gauche. On inverse cette matrice sur les quatre coins du rectangle
/// à conserver, ce qui reste exact pour une image tournée par quarts de tour,
/// et approché pour une image au dessin incliné.
fn crop_fractions_in_image(matrix: [f32; 6], rect: [f32; 4]) -> ImageCrop {
    let [a, b, c, d, e, f] = matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-6 {
        return ImageCrop::default();
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for corner_x in [rect[0], rect[2]] {
        for corner_y in [rect[1], rect[3]] {
            let dx = corner_x - e;
            let dy = corner_y - f;
            let u = (d * dx - c * dy) / determinant;
            let v = (a * dy - b * dx) / determinant;
            min_x = min_x.min(u);
            max_x = max_x.max(u);
            min_y = min_y.min(v);
            max_y = max_y.max(v);
        }
    }
    ImageCrop {
        left: min_x.clamp(0.0, 1.0),
        right: (1.0 - max_x).clamp(0.0, 1.0),
        bottom: min_y.clamp(0.0, 1.0),
        top: (1.0 - max_y).clamp(0.0, 1.0),
    }
}

/// Deux rognages diffèrent-ils de plus d'un millième ?
///
/// Une poignée survolée est replacée sous le pointeur : sans ce seuil, la
/// simple approche du curseur déclencherait une écriture.
fn response_changed(previous: ImageCrop, next: ImageCrop) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() > 0.001;
    near(previous.left, next.left)
        || near(previous.top, next.top)
        || near(previous.right, next.right)
        || near(previous.bottom, next.bottom)
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

/// Position dans un fragment : caractère `offset`, entre `offset` et
/// `offset + 1`. `offset == longueur` désigne la fin du fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TextHit {
    fragment: usize,
    offset: usize,
}

/// Sélection de texte sur une page, au caractère près.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TextSel {
    /// Page concernée.
    page: u32,
    /// Ancre et tête du glissé ; `None` sélectionne tout le texte de la page.
    span: Option<(TextHit, TextHit)>,
}

/// Longueur d'un fragment en caractères sélectionnables.
fn hit_len(frag: &TextFragment) -> usize {
    if frag.chars.is_empty() {
        frag.text.chars().count()
    } else {
        frag.chars.len()
    }
}

/// Boîte d'un caractère. Sans boîtes du moteur, le caractère occupe une
/// part proportionnelle du rectangle du fragment.
fn char_box(frag: &TextFragment, index: usize) -> [f32; 4] {
    if let Some(item) = frag.chars.get(index) {
        return item.rect;
    }
    let count = frag.text.chars().count().max(1) as f32;
    let [x1, y1, x2, y2] = frag.rect;
    let width = (x2 - x1).max(0.1);
    let start = index as f32 / count;
    let end = (index + 1) as f32 / count;
    [x1 + width * start, y1, x1 + width * end, y2]
}

/// Caractère le plus proche d'un point, à la frontière près.
///
/// Une frontière est le bord gauche du premier caractère, le bord droit du
/// dernier, et le milieu de l'interstice entre deux caractères. La sélection
/// s'arrête donc au caractère sous le pointeur, sans prendre le bloc entier.
fn text_hit(frags: &[TextFragment], point: [f32; 2]) -> Option<TextHit> {
    let mut best: Option<(f32, TextHit)> = None;
    for (fragment, frag) in frags.iter().enumerate() {
        let len = hit_len(frag);
        let line_y = (frag.rect[1] + frag.rect[3]) * 0.5;
        for offset in 0..=len {
            let x = if len == 0 {
                frag.rect[0]
            } else if offset == 0 {
                char_box(frag, 0)[0]
            } else if offset >= len {
                char_box(frag, len - 1)[2]
            } else {
                let before = char_box(frag, offset - 1);
                let after = char_box(frag, offset);
                (before[2] + after[0]) * 0.5
            };
            let dx = point[0] - x;
            let dy = point[1] - line_y;
            let distance = dx * dx + dy * dy;
            if best.is_none_or(|(current, _)| distance < current) {
                best = Some((distance, TextHit { fragment, offset }));
            }
        }
    }
    best.map(|(_, hit)| hit)
}

/// Caractère du fragment à l'offset donné, ou `None` si l'offset est hors
/// du fragment.
fn char_at(frag: &TextFragment, offset: usize) -> Option<char> {
    if !frag.chars.is_empty() {
        return frag.chars.get(offset).map(|item| item.ch);
    }
    frag.text.chars().nth(offset)
}

/// Étend une position jusqu'aux bords du mot qui la contient.
///
/// Un mot est une suite de caractères non blancs. La position donnée est
/// ramenée dans le fragment si elle déborde, puis les bords sont cherchés de
/// part et d'autre.
fn expand_to_word(frags: &[TextFragment], hit: TextHit) -> Option<(TextHit, TextHit)> {
    let frag = frags.get(hit.fragment)?;
    let len = hit_len(frag);
    if len == 0 {
        return None;
    }
    let mut offset = hit.offset.min(len);

    // Un clic dans l'espace entre deux mots prend le mot suivant ; un clic
    // après le dernier mot prend le mot précédent.
    while offset < len && char_at(frag, offset).is_some_and(char::is_whitespace) {
        offset += 1;
    }
    if offset >= len {
        offset = hit.offset.min(len.saturating_sub(1));
        while offset > 0 && char_at(frag, offset).is_some_and(char::is_whitespace) {
            offset -= 1;
        }
    }
    if char_at(frag, offset).is_some_and(char::is_whitespace) {
        return None;
    }

    let mut start = offset;
    while start > 0 {
        match char_at(frag, start - 1) {
            Some(ch) if !ch.is_whitespace() => start -= 1,
            _ => break,
        }
    }
    let mut end = offset + 1;
    while end < len {
        match char_at(frag, end) {
            Some(ch) if !ch.is_whitespace() => end += 1,
            _ => break,
        }
    }
    Some((
        TextHit {
            fragment: hit.fragment,
            offset: start,
        },
        TextHit {
            fragment: hit.fragment,
            offset: end,
        },
    ))
}

/// Étend une position jusqu'aux bords de sa ligne visuelle.
///
/// Une ligne regroupe les fragments qui partagent la même ligne de base, à la
/// tolérance de corps de police employée partout ailleurs (60 % de la
/// hauteur). La sélection part du début de la ligne et finit à sa fin, dans
/// l'ordre de lecture.
fn expand_to_line(frags: &[TextFragment], hit: TextHit) -> Option<(TextHit, TextHit)> {
    let groups = line_groups(frags);
    let group = groups
        .iter()
        .find(|group| group.contains(&hit.fragment))?;
    let start = TextHit {
        fragment: group[0],
        offset: 0,
    };
    let last = *group.last()?;
    let end = TextHit {
        fragment: last,
        offset: hit_len(&frags[last]),
    };
    Some((start, end))
}

/// Plages `(fragment, début, fin)` couvertes par une sélection, caractère
/// exclu à la fin.
fn hit_runs(
    frags: &[TextFragment],
    span: Option<(TextHit, TextHit)>,
) -> Vec<(usize, usize, usize)> {
    let order = reading_order(frags);
    let full = |fragment: usize| (fragment, 0, hit_len(&frags[fragment]));
    let Some((anchor, head)) = span else {
        return order.iter().map(|index| full(*index)).collect();
    };
    let rank = |hit: TextHit| {
        order
            .iter()
            .position(|index| *index == hit.fragment)
            .unwrap_or(0)
    };
    let (mut start, mut end) = (anchor, head);
    if rank(start) > rank(end) || (rank(start) == rank(end) && start.offset > end.offset) {
        std::mem::swap(&mut start, &mut end);
    }
    let first = rank(start);
    let last = rank(end);
    let mut runs = Vec::new();
    for position in first..=last {
        let fragment = order[position];
        let length = hit_len(&frags[fragment]);
        let from = if position == first { start.offset } else { 0 }.min(length);
        let to = if position == last { end.offset } else { length }.min(length);
        if from < to {
            runs.push((fragment, from, to));
        }
    }
    runs
}

/// Texte d'une sélection au caractère près.
fn hit_selection_text(frags: &[TextFragment], span: Option<(TextHit, TextHit)>) -> String {
    let mut out = String::new();
    let mut previous_line: Option<f32> = None;
    for (fragment, start, end) in hit_runs(frags, span) {
        let frag = &frags[fragment];
        let line = (frag.rect[1] + frag.rect[3]) * 0.5;
        if let Some(previous) = previous_line {
            let height = frag.font_size.max(1.0);
            out.push(if (previous - line).abs() > height * 0.6 {
                '\n'
            } else {
                ' '
            });
        }
        let text: String = frag.text.chars().skip(start).take(end - start).collect();
        out.push_str(&text);
        previous_line = Some(line);
    }
    out
}

/// Rectangles d'une sélection, un par ligne. Les caractères d'une même ligne
/// sont réunis, y compris quand la sélection traverse deux fragments.
fn hit_rects(frags: &[TextFragment], span: Option<(TextHit, TextHit)>) -> Vec<[f32; 4]> {
    let mut lines: Vec<[f32; 4]> = Vec::new();
    for (fragment, start, end) in hit_runs(frags, span) {
        let frag = &frags[fragment];
        let height = frag.font_size.max(1.0);
        for index in start..end {
            let rect = char_box(frag, index);
            let center = (rect[1] + rect[3]) * 0.5;
            match lines.iter_mut().find(|line| {
                let line_center = (line[1] + line[3]) * 0.5;
                (line_center - center).abs() <= height * 0.6
            }) {
                Some(line) => {
                    line[0] = line[0].min(rect[0]);
                    line[1] = line[1].min(rect[1]);
                    line[2] = line[2].max(rect[2]);
                    line[3] = line[3].max(rect[3]);
                }
                None => lines.push(rect),
            }
        }
    }
    lines
}

/// Quadrilatères prêts pour `/QuadPoints` à partir de rectangles de lignes.
fn quads_from_rects(lines: &[[f32; 4]]) -> Vec<[f32; 8]> {
    lines
        .iter()
        .map(|line| {
            [
                line[0], line[3], line[2], line[3], line[0], line[1], line[2], line[1],
            ]
        })
        .collect()
}

/// Quadrilatères d'une sélection au caractère près.
fn hit_quads(frags: &[TextFragment], span: Option<(TextHit, TextHit)>) -> Vec<[f32; 8]> {
    quads_from_rects(&hit_rects(frags, span))
}

/// Ordre de lecture des blocs d'une page.
///
/// Deux passes plutôt qu'un comparateur : un comparateur « même ligne ? » n'est
/// pas transitif dès que les corps de police diffèrent (a ≈ b, b ≈ c mais
/// a < c), et un tri sur un ordre non transitif rend un résultat non
/// spécifié. On trie donc par ordonnée décroissante, puis on regroupe les
/// blocs d'une même ligne et on les trie de gauche à droite. Chaque groupe est
/// fermé, l'ordre est donc déterministe.
fn reading_order(frags: &[TextFragment]) -> Vec<usize> {
    line_groups(frags).into_iter().flatten().collect()
}

/// Répartit les blocs en lignes visuelles, de haut en bas.
///
/// Chaque groupe contient les fragments d'une même ligne de base, triés de
/// gauche à droite. Le regroupement est le même que celui du tri global, mais
/// isolé : c'est lui qui dit où commence et où finit une ligne.
fn line_groups(frags: &[TextFragment]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..frags.len()).collect();
    order.sort_by(|a, b| {
        line_center(&frags[*b])
            .partial_cmp(&line_center(&frags[*a]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut groups: Vec<Vec<usize>> = Vec::with_capacity(order.len());
    let mut start = 0;
    while start < order.len() {
        let reference = line_center(&frags[order[start]]);
        let tolerance = frags[order[start]].font_size.max(1.0) * 0.6;
        let mut end = start + 1;
        while end < order.len() {
            let size = frags[order[end]].font_size.max(1.0);
            let gap = (line_center(&frags[order[end]]) - reference).abs();
            if gap <= tolerance.max(size * 0.6) {
                end += 1;
            } else {
                break;
            }
        }
        let mut row: Vec<usize> = order[start..end].to_vec();
        row.sort_by(|a, b| {
            frags[*a].rect[0]
                .partial_cmp(&frags[*b].rect[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        groups.push(row);
        start = end;
    }
    groups
}

/// Ordonnée du centre d'un bloc, en repère affiché.
fn line_center(frag: &TextFragment) -> f32 {
    (frag.rect[1] + frag.rect[3]) * 0.5
}

/// Couleur du fond de page autour d'un bloc, en repère affiché.
///
/// Le masque de réécriture doit se fondre dans la page : blanc sur une page
/// blanche, beige sur une page beige. On échantillonne six points au bord du
/// bloc, à l'écart des glyphes, et on prend la médiane par canal — un point
/// isolé tombé sur un filet ou un glyphe voisin ne décide donc pas seul. Sans
/// bitmap de la page disponible, on répond blanc.
fn mask_color_in(tab: &Tab, page: usize, rect: [f32; 4]) -> [u8; 3] {
    const WHITE: [u8; 3] = [255, 255, 255];
    let Some((bitmap_page, scale, bitmap)) = tab.last_bitmap.as_ref() else {
        return WHITE;
    };
    if *bitmap_page != page as u32 || *scale <= 0.0 || bitmap.width == 0 || bitmap.height == 0 {
        return WHITE;
    }

    // Hauteur de la page en points du repère affiché.
    let page_h = bitmap.height as f32 / *scale;
    let [x1, y1, x2, y2] = rect;
    let mut samples: Vec<[u8; 3]> = Vec::with_capacity(6);

    // Deux rangées : juste au-dessus du bloc, juste en dessous.
    for y in [y2 + 2.0_f32, y1 - 2.0] {
        for t in [0.2_f32, 0.5, 0.8] {
            let x = x1 + (x2 - x1) * t;
            let px = (x * scale) as i64;
            let py = ((page_h - y) * scale) as i64;
            if px < 0 || py < 0 || px >= bitmap.width as i64 || py >= bitmap.height as i64 {
                continue;
            }
            let idx = (py as usize * bitmap.width as usize + px as usize) * 4;
            if idx + 2 < bitmap.pixels.len() {
                samples.push([
                    bitmap.pixels[idx],
                    bitmap.pixels[idx + 1],
                    bitmap.pixels[idx + 2],
                ]);
            }
        }
    }
    if samples.is_empty() {
        return WHITE;
    }

    let mut out = WHITE;
    for (channel, slot) in out.iter_mut().enumerate() {
        let mut values: Vec<u8> = samples.iter().map(|s| s[channel]).collect();
        values.sort_unstable();
        *slot = values[values.len() / 2];
    }
    out
}

/// Extrait et met en cache les blocs de texte d'une page, en repère affiché.
///
/// Répond `false` si le moteur n'a rien lu : l'appelant ne doit alors pas
/// proposer de sélection sur cette page.
fn extract_page_fragments(renderer: &dyn Renderer, tab: &mut Tab, page: u32) -> bool {
    match renderer.page_fragments(&tab.doc, page) {
        Ok(mut frags) => {
            for frag in &mut frags {
                frag.rect = rect_to_display(&tab.doc, page as usize, frag.rect);
                // Les boîtes de glyphes vivent dans le même repère que le
                // fragment : sans cette conversion, la sélection au caractère
                // serait décalée sur une page tournée ou recadrée.
                for item in &mut frag.chars {
                    item.rect = rect_to_display(&tab.doc, page as usize, item.rect);
                }
            }
            let usable = !frags.is_empty();
            tab.fragments.insert(page, frags);
            usable
        }
        Err(err) => {
            tracing::warn!(%err, page, "extraction du texte de la page");
            tab.fragments.insert(page, Vec::new());
            false
        }
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

/// Garde les états les plus récents dans les deux limites (nombre et octets).
fn trim_history(stack: &mut Vec<HistoryEntry>) {
    trim_history_to(stack, HISTORY_LIMIT, HISTORY_BYTES);
}

fn trim_history_to(stack: &mut Vec<HistoryEntry>, limit: usize, budget: usize) {
    while stack.len() > limit {
        stack.remove(0);
    }
    let mut total: usize = stack
        .iter()
        .filter_map(|entry| entry.doc.as_ref().map(Vec::len))
        .sum();
    while total > budget && stack.len() > 1 {
        let dropped = stack.remove(0);
        total -= dropped.doc.as_ref().map(Vec::len).unwrap_or(0);
    }
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
#[derive(Clone, Copy, Debug, PartialEq)]
enum ContentKind {
    Text,
    Image,
}

/// Element de contenu (texte ou image) sélectionné en mode Édition : le
/// cadre, les poignées de redimensionnement et de rotation.
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
#[derive(Clone, Debug)]
enum ContentDrag {
    Move { start: [f32; 2], offset: [f32; 2] },
    Scale { start: [f32; 2], original_matrix: [f32; 6] },
    Rotate { start_angle: f32, original_matrix: [f32; 6] },
}

/// Le curseur couramment utilisé par l'éditeur de texte.
#[allow(dead_code)]
const TEXT_CURSOR: CursorIcon = CursorIcon::Text;

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
    compare_report: Option<ComparisonReport>,
    /// Fenêtre « OCR » et ses réglages.
    ocr_open: bool,
    ocr_language: String,
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
    batch_files: Vec<PathBuf>,
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
    batch_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// Vrai tant qu'un lot tourne.
    batch_running: bool,
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
        install_fonts(&cc.egui_ctx);
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
            compare_report: None,
            ocr_open: false,
            ocr_language: "eng".to_owned(),
            ocr_mode: OcrMode::default(),
            ocr_report: None,
            ocr_probed: false,
            ocr_installed: false,
            ocr_languages: Vec::new(),
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
            batch_files: Vec::new(),
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
            .frame(
                Frame::window(&ctx.style())
                    .fill(pal().chrome_bg)
                    .rounding(Rounding::same(10.0)),
            )
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
                    .size(12.5)
                    .color(pal().icon_bright),
            );
            ui.label(
                RichText::new("Closing now would lose them.")
                    .size(11.0)
                    .color(pal().muted),
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

    /// Panneau Signets : plan du document.
    fn bookmarks_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_structure();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let tree = tab.doc.outline_tree();

        let mut generate = false;
        let mut expand_all = false;
        let mut collapse_all = false;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            if ui.small_button("Auto-Generate").clicked() {
                generate = true;
            }
            if ui.small_button("Expand all").clicked() {
                expand_all = true;
            }
            if ui.small_button("Collapse all").clicked() {
                collapse_all = true;
            }
        });
        ui.add_space(4.0);

        if generate {
            self.generate_bookmarks(ctx);
            return;
        }
        if expand_all {
            self.bookmark_collapsed.clear();
        }
        if collapse_all {
            self.bookmark_collapsed = bookmark_all_keys(&tree).into_iter().collect();
        }

        if tree.is_empty() {
            panel_hint(
                ui,
                "No bookmarks",
                "Auto-Generate reads the text, or add one from the current page",
            );
            return;
        }

        let current_page = self.tabs[self.active].page;
        let mut rename_key = self.bookmark_rename.take();
        let mut rename_text = std::mem::take(&mut self.bookmark_rename_text);
        let mut request_focus = std::mem::take(&mut self.bookmark_rename_focus);
        let mut collapsed = std::mem::take(&mut self.bookmark_collapsed);
        let mut drag_from = self.bookmark_drag.clone();
        let mut drag_stopped = false;
        let mut hover_target: Option<(String, bool)> = None;
        let mut request: Option<BookmarkRequest> = None;
        let mut jump = None;

        let rows = bookmark_rows(&tree, &collapsed);
        let pointer = ctx.pointer_interact_pos();

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                for (key, title, page, depth, has_children, is_collapsed) in &rows {
                    let indent = 8.0 + (*depth as f32 * 12.0).min(60.0);
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2((self.panel_w - 24.0).max(140.0), 24.0),
                        Sense::click_and_drag(),
                    );
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
                    }

                    // Chevron : replier/déplier ce signet.
                    let chevron_x = rect.min.x + indent;
                    if *has_children {
                        ui.painter().text(
                            pos2(chevron_x, rect.center().y),
                            Align2::LEFT_CENTER,
                            if *is_collapsed {
                                ph::CARET_RIGHT
                            } else {
                                ph::CARET_DOWN
                            },
                            FontId::proportional(11.0),
                            pal().muted,
                        );
                    }
                    let text_x = chevron_x + 14.0;

                    let renaming = rename_key.as_deref() == Some(key.as_str());
                    if renaming {
                        let edit_rect = Rect::from_min_max(
                            pos2(text_x, rect.min.y + 1.0),
                            pos2(rect.max.x - 34.0, rect.max.y - 1.0),
                        );
                        let edit = ui.put(
                            edit_rect,
                            TextEdit::singleline(&mut rename_text)
                                .font(FontId::proportional(12.0)),
                        );
                        if request_focus {
                            edit.request_focus();
                            request_focus = false;
                        }
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            rename_key = None;
                        } else if edit.lost_focus() {
                            // Le champ se ferme quoi qu'il arrive : sans cela,
                            // valider un titre inchangé le laisserait ouvert.
                            rename_key = None;
                            request =
                                Some(BookmarkRequest::Rename(key.clone(), rename_text.clone()));
                        }
                    } else {
                        ui.painter().text(
                            pos2(text_x, rect.center().y),
                            Align2::LEFT_CENTER,
                            shorten(title, 30),
                            FontId::proportional(12.0),
                            pal().icon_bright,
                        );
                    }
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 8.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(10.5),
                            pal().muted,
                        );
                    }

                    // Cible de dépôt : le bord droit fait du signet glissé un
                    // enfant, le reste en fait le frère suivant.
                    if drag_from.is_some() && pointer.map(|p| rect.contains(p)).unwrap_or(false) {
                        let as_child = pointer.map(|p| p.x > rect.max.x - 30.0).unwrap_or(false);
                        hover_target = Some((key.clone(), as_child));
                        ui.painter()
                            .rect_stroke(rect.shrink(1.0), 6.0, Stroke::new(1.5_f32, pal().accent));
                    }

                    if !renaming && resp.clicked() && !resp.double_clicked() {
                        let chevron_click = resp
                            .interact_pointer_pos()
                            .map(|p| *has_children && p.x < text_x)
                            .unwrap_or(false);
                        if chevron_click {
                            if *is_collapsed {
                                collapsed.remove(key);
                            } else {
                                collapsed.insert(key.clone());
                            }
                        } else {
                            jump = *page;
                        }
                    }
                    if !renaming && resp.double_clicked() {
                        rename_key = Some(key.clone());
                        rename_text = title.clone();
                        request_focus = true;
                    }
                    if resp.drag_started() {
                        drag_from = Some(key.clone());
                    }
                    if resp.drag_stopped() {
                        drag_stopped = true;
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Add Bookmark").clicked() {
                            request = Some(BookmarkRequest::Add(None, Some(current_page)));
                            ui.close_menu();
                        }
                        if ui.button("Add Sub-Bookmark").clicked() {
                            request =
                                Some(BookmarkRequest::Add(Some(key.clone()), Some(current_page)));
                            ui.close_menu();
                        }
                        if ui.button("Rename").clicked() {
                            rename_key = Some(key.clone());
                            rename_text = title.clone();
                            request_focus = true;
                            ui.close_menu();
                        }
                        if ui.button("Set Destination to This Page").clicked() {
                            request = Some(BookmarkRequest::SetDestination(
                                key.clone(),
                                Some(current_page),
                            ));
                            ui.close_menu();
                        }
                        if ui.button("Delete Bookmark").clicked() {
                            request = Some(BookmarkRequest::Delete(key.clone()));
                            ui.close_menu();
                        }
                    });
                    ui.add_space(2.0);
                }
            });

        // Fin du glissé : re-parent ou ré-ordonne.
        if drag_stopped {
            if let Some(from) = drag_from.take() {
                if let Some((target, as_child)) = hover_target {
                    let from_path = bookmark_path(&from);
                    let target_path = bookmark_path(&target);
                    if target != from && !bookmark_is_ancestor(&from_path, &target_path) {
                        let moved = if as_child {
                            BookmarkRequest::Move {
                                from,
                                parent: Some(target),
                                index: None,
                            }
                        } else {
                            let parent = if target_path.len() > 1 {
                                Some(bookmark_key(&target_path[..target_path.len() - 1]))
                            } else {
                                None
                            };
                            let index = target_path.last().map(|index| index + 1);
                            BookmarkRequest::Move {
                                from,
                                parent,
                                index,
                            }
                        };
                        request = Some(moved);
                    }
                }
            }
        }

        self.bookmark_rename = rename_key;
        self.bookmark_rename_text = rename_text;
        self.bookmark_rename_focus = request_focus;
        self.bookmark_collapsed = collapsed;
        self.bookmark_drag = drag_from;
        if let Some(page) = jump {
            self.jump = Some(page);
        }
        if let Some(request) = request {
            self.apply_bookmark_request(ctx, request);
        }
    }

    /// Applique une modification du plan (renommage, ajout, destination,
    /// suppression, déplacement) puis réécrit `/Outlines`.
    fn apply_bookmark_request(&mut self, ctx: &egui::Context, request: BookmarkRequest) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let mut tree = tab.doc.outline_tree();
        let mut changed = false;
        match request {
            BookmarkRequest::Rename(key, title) => {
                if let Some(entry) = bookmark_at_mut(&mut tree, &bookmark_path(&key)) {
                    if entry.title != title {
                        entry.title = title;
                        changed = true;
                    }
                }
            }
            BookmarkRequest::Delete(key) => {
                changed = bookmark_take(&mut tree, &bookmark_path(&key)).is_some();
            }
            BookmarkRequest::Add(parent, page) => {
                let entry = OutlineEntry {
                    title: "New bookmark".to_owned(),
                    page,
                    target: OutlineTarget::None,
                    children: Vec::new(),
                };
                match &parent {
                    Some(key) => {
                        if let Some(parent) = bookmark_at_mut(&mut tree, &bookmark_path(key)) {
                            parent.children.push(entry);
                            changed = true;
                        }
                    }
                    None => {
                        tree.push(entry);
                        changed = true;
                    }
                }
            }
            BookmarkRequest::SetDestination(key, page) => {
                if let Some(entry) = bookmark_at_mut(&mut tree, &bookmark_path(&key)) {
                    entry.page = page;
                    // La cible d'origine est remplacée : sans cela, la
                    // destination écrite resterait l'ancienne.
                    entry.target = OutlineTarget::None;
                    changed = true;
                }
            }
            BookmarkRequest::Move {
                from,
                parent,
                index,
            } => {
                let from_path = bookmark_path(&from);
                let parent_path = parent.as_deref().map(bookmark_path);
                // Un signet ne peut pas devenir le descendant de lui-même.
                let illegal = parent_path
                    .as_deref()
                    .map(|path| bookmark_is_ancestor(&from_path, path))
                    .unwrap_or(false);
                if !illegal {
                    // Longueur de la liste d'accueil avant tout retrait : c'est
                    // aussi le point d'insertion quand `index` est absent.
                    let dest_len = match &parent_path {
                        Some(path) => bookmark_at(&tree, path).map(|node| node.children.len()),
                        None => Some(tree.len()),
                    };
                    if let (Some(from_index), Some(dest_len)) =
                        (from_path.last().copied(), dest_len)
                    {
                        let from_parent = &from_path[..from_path.len() - 1];
                        let same_list = match &parent_path {
                            Some(path) => path.as_slice() == from_parent,
                            None => from_parent.is_empty(),
                        };
                        // `index` compte dans la liste d'avant retrait ; retirer
                        // un frère situé avant elle décale la cible de un.
                        let at =
                            bookmark_insert_index(index, dest_len, same_list, from_index);
                        if let Some(entry) = bookmark_take(&mut tree, &from_path) {
                            // Le parent visé a pu se décaler lui aussi si le
                            // signet retiré le précédait dans son propre niveau.
                            let shifted = parent_path
                                .as_deref()
                                .map(|path| bookmark_shift_path(path, &from_path));
                            match &shifted {
                                Some(path) => {
                                    if let Some(target) = bookmark_at_mut(&mut tree, path) {
                                        let at = at.min(target.children.len());
                                        target.children.insert(at, entry);
                                        changed = true;
                                    }
                                }
                                None => {
                                    let at = at.min(tree.len());
                                    tree.insert(at, entry);
                                    changed = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        if !changed {
            return;
        }

        let result = self.tabs.get_mut(self.active).map(|tab| {
            let written = tab.doc.write_outline(&tree);
            if written.is_ok() {
                tab.outline = tab.doc.outline();
            }
            written
        });
        match result {
            Some(Ok(_)) => {
                self.mark_document_changed();
                self.bookmark_rename = None;
                self.bookmark_drag = None;
                // Les chemins ont bougé : les replis mémorisés ne veulent plus
                // rien dire.
                self.bookmark_collapsed.clear();
            }
            Some(Err(err)) => self.toast(ctx, format!("Bookmark failed: {err}")),
            None => {}
        }
    }

    /// Panneau Formulaires : lecture et remplissage des champs.

    /// Panneau Pièces jointes : ajout, ouverture, extraction et retrait.
    fn attachments_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_structure();
        let count = self.tabs.get(self.active).map_or(0, |tab| tab.attachments.len());

        let mut add = false;
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            if ui.button(RichText::new("Add Attachment").size(11.0)).clicked() {
                add = true;
            }
            ui.label(
                RichText::new(format!("{count} file(s)"))
                    .size(10.0)
                    .color(pal().muted),
            );
        });
        ui.add_space(4.0);
        if add {
            self.add_attachment(ctx);
            return;
        }

        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.attachments.is_empty() {
            panel_hint(ui, "No attachments", "Use Add Attachment to embed a file");
            return;
        }
        let rows: Vec<(String, Option<usize>, Option<String>)> = tab
            .attachments
            .iter()
            .map(|item| (item.name.clone(), item.size, item.description.clone()))
            .collect();

        let panel_w = self.panel_w;
        let mut action: Option<AttachmentAction> = None;
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (idx, (name, size, description)) in rows.iter().enumerate() {
                    let (rect, resp) =
                        ui.allocate_exact_size(vec2(panel_w - 24.0, 44.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
                    }
                    let icon_rect = Rect::from_center_size(
                        pos2(rect.min.x + 16.0, rect.center().y),
                        vec2(15.0, 15.0),
                    );
                    icon_attach(ui.painter(), icon_rect, pal().icon_bright);
                    ui.painter().text(
                        pos2(rect.min.x + 30.0, rect.min.y + 13.0),
                        Align2::LEFT_CENTER,
                        shorten(name, 24),
                        FontId::proportional(12.0),
                        pal().icon_bright,
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
                        pal().muted,
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
                        if save_resp.hovered() { pal().icon_bright } else { pal().icon },
                    );
                    if save_resp.clicked() {
                        action = Some(AttachmentAction::SaveAs(idx));
                    } else if resp.double_clicked() {
                        action = Some(AttachmentAction::Open(idx));
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Open").clicked() {
                            action = Some(AttachmentAction::Open(idx));
                            ui.close_menu();
                        }
                        if ui.button("Save As…").clicked() {
                            action = Some(AttachmentAction::SaveAs(idx));
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Delete").clicked() {
                            action = Some(AttachmentAction::Delete(idx));
                            ui.close_menu();
                        }
                    });
                    ui.add_space(4.0);
                }
            });

        match action {
            Some(AttachmentAction::Open(idx)) => self.open_attachment(idx, ctx),
            Some(AttachmentAction::SaveAs(idx)) => self.save_attachment(idx, ctx),
            Some(AttachmentAction::Delete(idx)) => self.delete_attachment(idx, ctx),
            None => {}
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

    /// Panneau Aspect : fond du canevas, ombre, taille des vignettes.
    fn aspect_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Background").size(11.5).color(pal().muted));
        });
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(12.0);
            // « Auto » puis les quatre fonds nommés.
            let swatches: Vec<(&str, Color32)> = std::iter::once(("Auto", pal().canvas_bg))
                .chain(CANVAS_THEMES.iter().copied())
                .collect();
            for (idx, (name, color)) in swatches.into_iter().enumerate() {
                let selected = self.canvas_style == idx;
                let (rect, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
                ui.painter().rect_filled(rect, Rounding::same(6.0), color);
                if selected {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        7.0,
                        Stroke::new(1.5_f32, pal().accent),
                    );
                }
                if resp.clicked() {
                    self.canvas_style = idx;
                }
                resp.on_hover_text(name);
            }
        });

        ui.add_space(12.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(RichText::new("Page tint").size(11.5).color(pal().muted));
        });
        ui.add_space(6.0);
        // Aperçu : chaque pastille montre la teinte posée sur du blanc,
        // comme sur la page.
        ui.horizontal_wrapped(|ui| {
            ui.add_space(12.0);
            for (idx, (name, rgba)) in PAGE_TINTS.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::click());
                ui.painter()
                    .rect_filled(rect, Rounding::same(4.0), Color32::WHITE);
                ui.painter().rect_filled(
                    rect,
                    Rounding::same(4.0),
                    Color32::from_rgba_unmultiplied(rgba[0], rgba[1], rgba[2], rgba[3]),
                );
                if self.page_tint == idx {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        5.0,
                        Stroke::new(1.5_f32, pal().accent),
                    );
                }
                if resp.clicked() {
                    self.page_tint = idx;
                }
                resp.on_hover_text(*name);
            }
        });

        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            let mut border = self.page_border;
            if ui.checkbox(&mut border, "Page border").changed() {
                self.page_border = border;
            }
        });

        ui.add_space(8.0);
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
            ui.label(RichText::new("Thumbnails").size(11.5).color(pal().muted));
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.add_sized(
                vec2(self.panel_w - 40.0, 18.0),
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
                    .color(pal().muted),
            );
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(
                RichText::new("Colours come from the current tool presets.")
                    .size(10.5)
                    .color(pal().muted),
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
    #[allow(dead_code)]
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
            if annot.page == page && annot.kind == AnnotationKind::Ink {
                if annot.erase_ink_segment(from, to, RADIUS) > 0 {
                    changed = true;
                    if annot.ink_points.is_empty() {
                        continue;
                    }
                    annot.recompute_ink_bbox();
                }
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
            extract_page_fragments(&*self.renderer, tab, page);
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
            if !extract_page_fragments(&*self.renderer, tab, page) {
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
                fragment.rect = rect_to_page(&tab.doc, fragment.page, fragment.rect);
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
            Some(tab) => point_to_page(&tab.doc, page, point),
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
        // Le surligneur et le crayon se dessinent en transparence.
        let wants_opacity = tool == 1 || tool == 4;
        Area::new(Id::new("presets"))
            .anchor(Align2::CENTER_TOP, vec2(canvas_left / 2.0, top + 40.0))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                Frame::none()
                    .fill(pal().chrome_bg)
                    .rounding(Rounding::same(8.0))
                    .stroke(Stroke::new(1.0_f32, pal().separator))
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
                                ui.label(RichText::new("Width").size(11.0).color(pal().muted));
                                ui.add(
                                    egui::Slider::new(&mut self.annot_width, 0.5..=12.0)
                                        .fixed_decimals(1),
                                );
                            });
                        }
                        if tool == 10 {
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Eraser").size(11.0).color(pal().muted));
                                let stroke = self.eraser_mode == EraseMode::Stroke;
                                if ui.selectable_label(stroke, "Stroke").clicked() {
                                    self.eraser_mode = EraseMode::Stroke;
                                }
                                if ui.selectable_label(!stroke, "Segment").clicked() {
                                    self.eraser_mode = EraseMode::Segment;
                                }
                            });
                        }
                        if wants_opacity {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Opacity").size(11.0).color(pal().muted));
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
                                    .color(pal().muted),
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
            let mut results = index.search_with(&query, 200, options);
            // Les signets et les commentaires ne vivent pas dans le texte des
            // pages : on les ajoute à la main quand les filtres les demandent.
            let synonyms = self.bookmark_comment_matches(&query, options);
            results.extend(synonyms);
            self.results = results;
            let count = self.results.len();
            self.history.retain(|q| q != &query);
            self.history.insert(0, query);
            self.history.truncate(8);
            self.search_active = if count > 0 { Some(0) } else { None };
            self.focus_search_result();
            self.toast(ctx, format!("{count} result(s)"));
        }
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
        let needle = if options.case_sensitive {
            query.to_owned()
        } else {
            fold_case(query)
        };
        if needle.is_empty() {
            return out;
        }

        if self.search_bookmarks {
            for item in tab.doc.outline() {
                if let Some(pos) = find_query(&item.title, &needle, options.case_sensitive) {
                    out.push(SearchResult {
                        page: item.page.unwrap_or(1),
                        start: pos,
                        end: pos + needle.len(),
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
                if let Some(pos) = find_query(&annot.content, &needle, options.case_sensitive) {
                    out.push(SearchResult {
                        page: annot.page,
                        start: pos,
                        end: pos + needle.len(),
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
        let needle = fold_case(self.search_query.trim());
        if needle.is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
            return;
        };
        for fragment in fragments {
            // Les fragments sortent du moteur en repère page ; les surlignages
            // se dessinent en repère affiché.
            let rect = rect_to_display(&tab.doc, page, fragment.rect);
            self.search_rects
                .extend(match_rects(&fragment.text, rect, &needle));
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
                let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
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
                    // Même masque que la réécriture en place : la couleur du
                    // fond de la page quand elle est connue, blanc sinon.
                    let bg = mask_color_in(
                        tab,
                        page,
                        rect_to_display(&tab.doc, page, fragment.rect),
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
                let Ok(fragments) = self.renderer.page_fragments(&tab.doc, page as u32) else {
                    continue;
                };
                for fragment in fragments {
                    let rect = rect_to_display(&tab.doc, page, fragment.rect);
                    for rect in match_rects(&fragment.text, rect, &needle) {
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
                let handle = ctx.load_texture(
                    format!("p{page}s{step}"),
                    image,
                    TextureOptions::LINEAR,
                );
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

    fn title_bar(&mut self, ctx: &egui::Context) {
        TopBottomPanel::top("titlebar")
            .exact_height(TITLE_H)
            .frame(Frame::none().fill(pal().chrome_bg))
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
                    // Actions différées : la boucle emprunte `self.tabs` en
                    // lecture, elles s'appliquent après.
                    let mut tab_action: Option<TabAction> = None;
                    let mut drag_done = false;
                    let dragging = self.dragging_tab;
                    for (idx, tab) in self.tabs.iter().enumerate() {
                        let selected = idx == self.active;
                        let color = if selected { pal().icon_bright } else { pal().muted };
                        let mut label = if tab.dirty {
                            format!("{} •", shorten(&tab.title, 22))
                        } else {
                            shorten(&tab.title, 24)
                        };
                        if tab.pinned {
                            label = format!("{} {label}", ph::PUSH_PIN);
                        }
                        let galley = ui.painter().layout_no_wrap(
                            label,
                            FontId::proportional(12.5),
                            color,
                        );
                        let width = if selected {
                            (galley.size().x + 56.0).max(170.0)
                        } else {
                            // 44 pt réservent la place du bouton de fermeture,
                            // affiché dès le survol.
                            galley.size().x + 44.0
                        };
                        let width = width.clamp(120.0, 220.0);
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(width, 30.0), Sense::click_and_drag());

                        if selected {
                            ui.painter().rect_filled(rect, Rounding::same(8.0), pal().widget_bg);
                        } else if resp.hovered() {
                            ui.painter().rect_filled(
                                rect,
                                Rounding::same(8.0),
                                Color32::from_rgb(0x33, 0x33, 0x33),
                            );
                        }
                        if dragging.is_some() && resp.contains_pointer() {
                            ui.painter().vline(
                                rect.left() - 5.0,
                                rect.y_range(),
                                Stroke::new(2.0_f32, pal().accent),
                            );
                        }
                        ui.painter().galley(
                            pos2(rect.min.x + 14.0, rect.center().y - galley.size().y / 2.0),
                            galley,
                            color,
                        );

                        let show_close = selected || resp.hovered();
                        if show_close {
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
                                if x_resp.hovered() { pal().icon_bright } else { pal().muted },
                            );
                            if x_resp.clicked() {
                                close = Some(idx);
                            }
                        }

                        if resp.clicked() {
                            select = Some(idx);
                        }
                        if resp.middle_clicked() {
                            close = Some(idx);
                        }
                        if resp.drag_started() && !tab.pinned {
                            self.dragging_tab = Some(idx);
                        }
                        if dragging.is_some() && resp.contains_pointer() {
                            self.drag_target = Some(idx);
                        }
                        if resp.drag_stopped() {
                            drag_done = true;
                        }

                        resp.context_menu(|ui| {
                            if ui.button("Close Tab").clicked() {
                                tab_action = Some(TabAction::Close(idx));
                                ui.close_menu();
                            }
                            if ui.button("Close Other Tabs").clicked() {
                                tab_action = Some(TabAction::CloseOthers(idx));
                                ui.close_menu();
                            }
                            if ui.button("Close Tabs to the Right").clicked() {
                                tab_action = Some(TabAction::CloseRight(idx));
                                ui.close_menu();
                            }
                            if ui.button("Close All Tabs").clicked() {
                                tab_action = Some(TabAction::CloseAll);
                                ui.close_menu();
                            }
                            ui.separator();
                            let pin_label = if tab.pinned { "Unpin Tab" } else { "Pin Tab" };
                            if ui.button(pin_label).clicked() {
                                tab_action = Some(TabAction::TogglePin(idx));
                                ui.close_menu();
                            }
                            if ui.button("Duplicate Tab").clicked() {
                                tab_action = Some(TabAction::Duplicate(idx));
                                ui.close_menu();
                            }
                            ui.separator();
                            if ui.button("Reveal in Finder").clicked() {
                                tab_action = Some(TabAction::Reveal(idx));
                                ui.close_menu();
                            }
                            if ui.button("Copy File Path").clicked() {
                                tab_action = Some(TabAction::CopyPath(idx));
                                ui.close_menu();
                            }
                        });
                        ui.add_space(10.0);
                    }

                    // Dépôt d'un glissé : réordonne si la cible a changé.
                    if drag_done {
                        if let Some(from) = self.dragging_tab.take() {
                            let to = self.drag_target.take().unwrap_or(from);
                            self.move_tab(from, to);
                        }
                    }

                    if let Some(action) = tab_action {
                        self.apply_tab_action(ctx, action);
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
                            .circle_filled(rect.center(), 10.5, pal().accent);
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

    /// Liste des commentaires : filtres, tri, réponses et export.
    fn comments_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        self.ensure_annots();
        self.ensure_comment_quotes();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        if tab.annots.is_empty() {
            ui.add_space(14.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No annotations yet")
                        .size(12.0)
                        .color(pal().muted),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new("Use Comment mode to add one")
                        .size(11.0)
                        .color(pal().muted),
                );
            });
            return;
        }

        let panel_w = self.panel_w;
        let quotes = &self.comment_quotes;
        let now = chrono::Utc::now();

        // L'état de l'interface est relu en fin de fonction : les fermetures
        // de peinture ne peuvent pas emprunter `self`.
        let mut filter = self.comment_filter();
        let mut sort = self.comment_sort;
        let reply_open = self.comment_reply.clone();
        let mut reply_text = self.comment_reply_text.clone();
        let mut action: Option<CommentAction> = None;

        // Types, auteurs et couleurs réellement présents : proposer un filtre
        // qui ne peut rien retenir n'apporte rien.
        let mut kinds: Vec<AnnotationKind> = tab.annots.iter().map(|a| a.kind).collect();
        kinds.sort_by_key(|kind| kind.pdf_subtype());
        kinds.dedup();
        let mut authors: Vec<String> = tab.annots.iter().map(|a| a.author.clone()).collect();
        authors.sort();
        authors.dedup();
        let mut colors: Vec<[u8; 3]> = tab.annots.iter().map(|a| a.style.color).collect();
        colors.sort_unstable();
        colors.dedup();

        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.menu_button(RichText::new("Export Comments").size(11.0), |ui| {
                for format in CommentFormat::ALL {
                    if ui.button(format.display_name()).clicked() {
                        action = Some(CommentAction::Export(format));
                        ui.close_menu();
                    }
                }
            });
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            ui.add_sized(
                [panel_w - 32.0, 20.0],
                TextEdit::singleline(&mut filter.query).hint_text("Search comments"),
            );
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(4.0);
            egui::ComboBox::from_id_salt("comment-kind")
                .width(70.0)
                .selected_text(filter.kind.map_or("All types", annot_label))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(filter.kind.is_none(), "All types")
                        .clicked()
                    {
                        filter.kind = None;
                    }
                    for kind in &kinds {
                        if ui
                            .selectable_label(filter.kind == Some(*kind), annot_label(*kind))
                            .clicked()
                        {
                            filter.kind = Some(*kind);
                        }
                    }
                });
            egui::ComboBox::from_id_salt("comment-author")
                .width(76.0)
                .selected_text(filter.author.as_deref().unwrap_or("All authors"))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(filter.author.is_none(), "All authors")
                        .clicked()
                    {
                        filter.author = None;
                    }
                    for author in &authors {
                        if ui
                            .selectable_label(filter.author.as_deref() == Some(author.as_str()), author)
                            .clicked()
                        {
                            filter.author = Some(author.clone());
                        }
                    }
                });
            egui::ComboBox::from_id_salt("comment-color")
                .width(74.0)
                .selected_text(filter.color.map_or_else(|| "All colors".to_owned(), hex_color))
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(filter.color.is_none(), "All colors")
                        .clicked()
                    {
                        filter.color = None;
                    }
                    for color in &colors {
                        let label = RichText::new(hex_color(*color)).color(Color32::from_rgb(
                            color[0], color[1], color[2],
                        ));
                        if ui
                            .selectable_label(filter.color == Some(*color), label)
                            .clicked()
                        {
                            filter.color = Some(*color);
                        }
                    }
                });
            egui::ComboBox::from_id_salt("comment-state")
                .width(78.0)
                .selected_text(filter.state.display_name())
                .show_ui(ui, |ui| {
                    for state in CommentStateFilter::ALL {
                        if ui
                            .selectable_label(filter.state == state, state.display_name())
                            .clicked()
                        {
                            filter.state = state;
                        }
                    }
                });
        });
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.add_space(4.0);
            ui.label(RichText::new("Sort").size(10.0).color(pal().muted));
            for (choice, name) in [
                (CommentSort::Page, "Page"),
                (CommentSort::Date, "Date"),
                (CommentSort::Author, "Author"),
            ] {
                if ui
                    .selectable_label(sort == choice, RichText::new(name).size(10.0))
                    .clicked()
                {
                    sort = choice;
                }
            }
        });
        ui.add_space(2.0);
        ui.separator();

        let rows = comment_rows(&tab.annots, &filter, sort);
        if rows.is_empty() {
            ui.add_space(10.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new("No comment matches")
                        .size(11.0)
                        .color(pal().muted),
                );
            });
        }

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);
                for row in &rows {
                    let card_w = (panel_w - 16.0 - row.depth as f32 * 14.0).max(80.0);
                    ui.horizontal(|ui| {
                        ui.add_space(4.0 + row.depth as f32 * 14.0);
                        ui.vertical(|ui| {
                            let response = ui
                                .scope(|ui| {
                                    ui.set_width(card_w);
                                    ui.horizontal(|ui| {
                                        // Avatar : l'initiale de l'auteur.
                                        let (rect, _) = ui
                                            .allocate_exact_size(vec2(20.0, 20.0), Sense::hover());
                                        ui.painter().circle_filled(
                                            rect.center(),
                                            9.0,
                                            pal().widget_bg,
                                        );
                                        ui.painter().text(
                                            rect.center(),
                                            Align2::CENTER_CENTER,
                                            author_initial(&row.author),
                                            FontId::proportional(10.0),
                                            pal().icon_bright,
                                        );
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    RichText::new(&row.author).size(11.5).strong(),
                                                );
                                                ui.label(
                                                    RichText::new(relative_time(
                                                        &row.created_at,
                                                        now,
                                                    ))
                                                    .size(9.5)
                                                    .color(pal().muted),
                                                );
                                            });
                                            ui.horizontal(|ui| {
                                                let (swatch, _) = ui.allocate_exact_size(
                                                    vec2(8.0, 8.0),
                                                    Sense::hover(),
                                                );
                                                ui.painter().rect_filled(
                                                    swatch,
                                                    Rounding::same(2.0),
                                                    Color32::from_rgb(
                                                        row.color[0],
                                                        row.color[1],
                                                        row.color[2],
                                                    ),
                                                );
                                                ui.label(
                                                    RichText::new(format!(
                                                        "{} · p.{}",
                                                        annot_label(row.kind),
                                                        row.page
                                                    ))
                                                    .size(10.0)
                                                    .color(pal().icon_bright),
                                                );
                                            });
                                        });
                                    });
                                    // Texte visé par le surlignage.
                                    if let Some(quote) = quotes.get(&row.id) {
                                        if !quote.is_empty() && is_markup(row.kind) {
                                            ui.label(
                                                RichText::new(format!("“{quote}”"))
                                                    .size(10.0)
                                                    .italics()
                                                    .color(pal().muted),
                                            );
                                        }
                                    }
                                    if !row.content.is_empty() {
                                        ui.label(RichText::new(&row.content).size(11.0));
                                    }
                                    ui.horizontal(|ui| {
                                        ui.menu_button(
                                            RichText::new(format!(
                                                "{} {}",
                                                comment_state_icon(row.state),
                                                row.state.display_name()
                                            ))
                                            .size(9.5)
                                            .color(comment_state_color(row.state)),
                                            |ui| {
                                                for state in AnnotationState::ALL {
                                                    if ui
                                                        .selectable_label(
                                                            row.state == state,
                                                            state.display_name(),
                                                        )
                                                        .clicked()
                                                    {
                                                        action = Some(CommentAction::SetState(
                                                            row.id.clone(),
                                                            state,
                                                        ));
                                                        ui.close_menu();
                                                    }
                                                }
                                            },
                                        );
                                        if ui
                                            .button(RichText::new("Reply").size(9.5))
                                            .clicked()
                                        {
                                            action =
                                                Some(CommentAction::Reply(row.id.clone()));
                                        }
                                    });
                                    if reply_open.as_deref() == Some(row.id.as_str()) {
                                        ui.horizontal(|ui| {
                                            let edit = ui.add_sized(
                                                [(card_w - 56.0).max(60.0), 20.0],
                                                TextEdit::singleline(&mut reply_text)
                                                    .hint_text("Reply…"),
                                            );
                                            // Entrée valide la saisie ; la
                                            // perte de focus seule (clic
                                            // ailleurs) n'envoie rien.
                                            let entered = edit.lost_focus()
                                                && !reply_text.trim().is_empty()
                                                && ui.input(|i| {
                                                    i.key_pressed(egui::Key::Enter)
                                                });
                                            let send = ui
                                                .button(RichText::new("Send").size(9.5))
                                                .clicked();
                                            if send || entered {
                                                action = Some(CommentAction::Send(
                                                    row.id.clone(),
                                                    reply_text.clone(),
                                                ));
                                            }
                                        });
                                    }
                                })
                                .response;
                            // Carte cliquable : saut vers la page visée.
                            let card = response.interact(Sense::click());
                            if card.clicked() && action.is_none() {
                                action = Some(CommentAction::Jump(row.page));
                            }
                            card.context_menu(|ui| {
                                if ui.button("Reply").clicked() {
                                    action = Some(CommentAction::Reply(row.id.clone()));
                                    ui.close_menu();
                                }
                                if ui.button("Delete").clicked() {
                                    action = Some(CommentAction::Delete(row.id.clone()));
                                    ui.close_menu();
                                }
                            });
                        });
                    });
                    ui.add_space(4.0);
                }
            });

        // Les filtres et la réponse en cours repartent dans l'application…
        self.comment_search = filter.query.clone();
        self.comment_kind = filter.kind;
        self.comment_author = filter.author.clone();
        self.comment_color = filter.color;
        self.comment_state = filter.state;
        self.comment_sort = sort;
        self.comment_reply = reply_open;
        self.comment_reply_text = reply_text;
        // …puis l'action demandée a le dernier mot.
        match action {
            Some(CommentAction::Jump(page)) => self.jump = Some(page),
            Some(CommentAction::Delete(id)) => self.remove_comment_thread(&id, ctx),
            Some(CommentAction::SetState(id, state)) => self.set_comment_state(&id, state),
            Some(CommentAction::Reply(id)) => {
                // Recliquer sur « Reply » referme la zone de saisie.
                if self.comment_reply.as_deref() == Some(id.as_str()) {
                    self.comment_reply = None;
                } else {
                    self.comment_reply = Some(id);
                }
                self.comment_reply_text.clear();
            }
            Some(CommentAction::Send(parent, text)) => self.send_comment_reply(&parent, &text, ctx),
            Some(CommentAction::Export(format)) => self.export_comments_dialog(format, ctx),
            None => {}
        }
    }

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
                || !is_markup(annot.kind)
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
        let mut doomed: Vec<String> = vec![id.to_owned()];
        if let Some(tab) = self.tabs.get_mut(self.active) {
            // Fermeture transitive par largeur : chaque réponse trouvée
            // étend la liste à explorer.
            let mut cursor = 0;
            while cursor < doomed.len() {
                let parent = doomed[cursor].clone();
                cursor += 1;
                for annot in &tab.annots {
                    if annot.reply_to.as_deref() == Some(parent.as_str())
                        && !doomed.contains(&annot.id)
                    {
                        doomed.push(annot.id.clone());
                    }
                }
            }
            let before = tab.annots.len();
            if before == doomed.len().min(before) && !tab.annots.iter().any(|a| a.id == id) {
                return;
            }
            let before_state = tab.annots.clone();
            tab.record_annots(before_state);
            tab.annots.retain(|annot| !doomed.contains(&annot.id));
            if tab.annots.len() != before {
                tab.dirty = true;
            }
        }
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

    fn side_panel(&mut self, ctx: &egui::Context) {
        let Some(panel) = self.panel else {
            return;
        };
        let panel_resp = SidePanel::left("content")
            .default_width(self.panel_w)
            .resizable(true)
            .width_range(180.0..=450.0)
            .frame(Frame::none().fill(pal().canvas_bg).stroke(Stroke::new(1.0_f32, pal().separator)))
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
                                if resp.hovered() { pal().icon_bright } else { pal().icon },
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
                            let active = self.search_case
                                || self.search_word
                                || self.search_bookmarks
                                || self.search_comments;
                            icon_funnel(
                                ui.painter(),
                                rect.shrink(7.0),
                                if resp.hovered() || active { pal().icon_bright } else { pal().icon },
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
                                            .color(pal().muted),
                                    );
                                    ui.add_space(4.0);
                                    refilter |= ui
                                        .checkbox(&mut self.search_case, "Case sensitive")
                                        .changed();
                                    refilter |= ui
                                        .checkbox(&mut self.search_word, "Whole words")
                                        .changed();
                                    refilter |= ui
                                        .checkbox(&mut self.search_bookmarks, "Include bookmarks")
                                        .changed();
                                    refilter |= ui
                                        .checkbox(&mut self.search_comments, "Include comments")
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
                    Stroke::new(1.0_f32, pal().separator),
                );
                ui.add_space(10.0);
                match panel {
                    Panel::Thumbnails => self.thumbnails(ui),
                    Panel::Search => self.search_ui(ctx, ui),
                    Panel::Comments => self.comments_panel(ctx, ui),
                    Panel::Bookmarks => self.bookmarks_panel(ctx, ui),
                    Panel::Forms => self.forms_panel(ctx, ui),
                    Panel::Attachments => self.attachments_panel(ctx, ui),
                    Panel::Aspect => self.aspect_panel(ui),
                }
            });
        // La largeur réelle suit le glissé ; les panneaux s'y ajustent à la
        // trame suivante.
        self.panel_w = panel_resp.response.rect.width();
    }

    fn thumbnails(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            // Plus aucun document : la sélection ne doit pas survivre à
            // l'ouverture suivante.
            self.thumb_selected.clear();
            self.thumb_owner = usize::MAX;
            return;
        }
        let active = self.active;
        let ctx = ui.ctx().clone();
        let ppp = ctx.pixels_per_point();
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
        let total = self.tabs[active].total();
        let mut zoom = self.thumb_pt;
        let mut selected = std::mem::take(&mut self.thumb_selected);
        // Changer d'onglet change de document : les mêmes numéros de page
        // viseraient d'autres pages.
        if self.thumb_owner != active {
            self.thumb_owner = active;
            selected.clear();
        }
        // Un document plus court (suppression, nouvelle ouverture) laisse des
        // pages fantômes dans la sélection.
        selected.retain(|page| *page >= 1 && *page <= total);
        let mut drag_from = self.thumb_drag;
        let mut action: Option<PageAction> = None;
        let mut jump = None;

        // Curseur en bas du panneau : largeur des vignettes, de 80 à 240 pt.
        egui::TopBottomPanel::bottom("thumb_zoom")
            .frame(Frame::none().fill(pal().canvas_bg))
            .show_inside(ui, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(12.0);
                    ui.add_sized(
                        vec2((self.panel_w - 40.0).max(80.0), 18.0),
                        egui::Slider::new(&mut zoom, 80.0..=240.0).show_value(false),
                    );
                });
                ui.add_space(6.0);
            });

        let tab = &mut self.tabs[active];
        let pointer = ctx.pointer_interact_pos();
        let mut drop_target = None;
        let mut drag_stopped = false;

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let tile_width = ui.available_width().max(80.0);
                for page in 1..=total as u32 {
                    let (wp, hp) = tab.page_size_cached(page);
                    let scale = zoom / wp.max(1.0);
                    let size = vec2(wp * scale, hp * scale);
                    let (rect, resp) = ui.allocate_exact_size(
                        vec2(tile_width, size.y + 28.0),
                        Sense::click_and_drag(),
                    );
                    let picked = selected.contains(&(page as usize));
                    let img_rect = Rect::from_center_size(
                        pos2(rect.center().x, rect.min.y + 8.0 + size.y / 2.0),
                        size,
                    );

                    if picked {
                        ui.painter().rect_filled(
                            img_rect.expand(5.0),
                            Rounding::same(4.0),
                            pal().thumb_tint,
                        );
                    } else if resp.hovered() {
                        ui.painter().rect_filled(
                            img_rect.expand(5.0),
                            Rounding::same(4.0),
                            Color32::from_rgb(0x3C, 0x3C, 0x3C),
                        );
                    }

                    if let Some(tex) = Self::page_texture(
                        &ctx,
                        renderer,
                        tab,
                        page,
                        THUMBNAIL_STEP,
                        scale,
                        ppp,
                        true,
                        false,
                        status,
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
                            img_rect.expand(5.0),
                            4.0,
                            Stroke::new(1.0_f32, pal().accent),
                        );
                    }
                    // Repère de la page affichée.
                    if page as usize == tab.page {
                        ui.painter().circle_filled(
                            pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                            3.0,
                            pal().accent,
                        );
                    }
                    ui.painter().text(
                        pos2(rect.center().x, img_rect.max.y + 14.0),
                        Align2::CENTER_CENTER,
                        page.to_string(),
                        FontId::proportional(11.0),
                        if picked { pal().accent } else { pal().muted },
                    );

                    // Cible du glissé : la vignette sous le curseur.
                    if drag_from.is_some() && pointer.map(|p| rect.contains(p)).unwrap_or(false) {
                        drop_target = Some(page as usize);
                        ui.painter().rect_stroke(
                            img_rect.expand(5.0),
                            4.0,
                            Stroke::new(2.0_f32, pal().accent),
                        );
                    }
                    if resp.drag_started() {
                        drag_from = Some(page as usize);
                    }
                    if resp.drag_stopped() {
                        drag_stopped = true;
                    }
                    if resp.clicked() {
                        let mods = ui.input(|i| i.modifiers);
                        if mods.shift {
                            let anchor = selected
                                .iter()
                                .next_back()
                                .copied()
                                .unwrap_or(tab.page.max(1));
                            let (lo, hi) = (anchor.min(page as usize), anchor.max(page as usize));
                            for number in lo..=hi {
                                selected.insert(number);
                            }
                        } else if mods.command || mods.ctrl {
                            if !selected.remove(&(page as usize)) {
                                selected.insert(page as usize);
                            }
                        } else {
                            selected.clear();
                            selected.insert(page as usize);
                            jump = Some(page as usize);
                        }
                    }
                    resp.context_menu(|ui| {
                        // Le menu agit sur toute la sélection ; un clic droit
                        // sur une page hors sélection la sélectionne d'abord.
                        if !selected.contains(&(page as usize)) {
                            selected.clear();
                            selected.insert(page as usize);
                        }
                        if ui.button("Rotate Clockwise").clicked() {
                            action = Some(PageAction::Rotate(90));
                            ui.close_menu();
                        }
                        if ui.button("Rotate Counter-Clockwise").clicked() {
                            action = Some(PageAction::Rotate(-90));
                            ui.close_menu();
                        }
                        ui.separator();
                        ui.menu_button("Insert Blank Page After", |ui| {
                            for (label, width, height) in BLANK_SIZES {
                                if ui.button(label).clicked() {
                                    action = Some(PageAction::Blank(width, height));
                                    ui.close_menu();
                                }
                            }
                        });
                        if ui.button("Insert from File…").clicked() {
                            action = Some(PageAction::InsertFile);
                            ui.close_menu();
                        }
                        if ui.button("Extract Selected Pages…").clicked() {
                            action = Some(PageAction::Extract);
                            ui.close_menu();
                        }
                        if ui.button("Duplicate Pages").clicked() {
                            action = Some(PageAction::Duplicate);
                            ui.close_menu();
                        }
                        ui.separator();
                        if ui.button("Page Properties…").clicked() {
                            action = Some(PageAction::Properties(page as usize));
                            ui.close_menu();
                        }
                        if ui.button("Delete Pages").clicked() {
                            action = Some(PageAction::Delete);
                            ui.close_menu();
                        }
                        ui.label(
                            RichText::new(format!("{} page(s) selected", selected.len()))
                                .size(11.0)
                                .color(pal().muted),
                        );
                    });

                    ui.add_space(6.0);
                }
            });

        // Fin du glissé : la vignette survolée devient la nouvelle position.
        // `drag_stopped` vient de la vignette qui a porté le glissé : un
        // relâchement ailleurs (curseur sorti de la fenêtre) ne déplace rien.
        if drag_stopped {
            if let Some(from) = drag_from.take() {
                if let Some(to) = drop_target {
                    if to != from {
                        action = Some(PageAction::Move(from, to));
                    }
                }
            }
        }

        self.thumb_selected = selected;
        self.thumb_pt = zoom;
        self.thumb_drag = drag_from;
        if let Some(page) = jump {
            if let Some(tab) = self.tabs.get_mut(active) {
                tab.page = page;
            }
            self.jump = Some(page);
        }
        if let Some(action) = action {
            self.run_page_action(&ctx, active, action);
        }
    }

    /// Applique l'action choisie dans le menu contextuel des vignettes.
    fn run_page_action(&mut self, ctx: &egui::Context, active: usize, action: PageAction) {
        if self.tabs.is_empty() {
            return;
        }
        let total = self.tabs[active].total();
        // Pages visées : la sélection, ou la page affichée si rien n'est coché.
        let mut pages: Vec<usize> = self.thumb_selected.iter().copied().collect();
        if pages.is_empty() {
            pages = vec![self.tabs[active].page.clamp(1, total)];
        }
        let last = pages.iter().copied().max().unwrap_or(1);

        if let PageAction::Properties(page) = action {
            self.page_props = Some(page);
            return;
        }

        let doc = &mut self.tabs[active].doc;
        let outcome: Result<Option<String>, String> = match action {
            PageAction::Rotate(delta) => doc
                .rotate_pages(&pages, delta)
                .map(|count| Some(format!("{count} page(s) rotated")))
                .map_err(|err| format!("Rotation failed: {err}")),
            PageAction::Blank(mut width, mut height) => {
                if width <= 0.0 || height <= 0.0 {
                    let (dw, dh) = doc.page_size(last as u32);
                    width = dw;
                    height = dh;
                }
                doc.insert_blank_page_after(last, width, height)
                    .map(|_| Some(format!("Blank page inserted after page {last}")))
                    .map_err(|err| format!("Insert failed: {err}"))
            }
            PageAction::InsertFile => match rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file()
            {
                Some(path) => doc
                    .insert_pages_after(last, &path)
                    .map(|count| Some(format!("{count} page(s) inserted after page {last}")))
                    .map_err(|err| format!("Insert failed: {err}")),
                None => Ok(None),
            },
            PageAction::Extract => match rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .set_file_name("extract.pdf")
                .save_file()
            {
                Some(path) => doc
                    .extract_pages(&pages, &path)
                    .map(|_| {
                        Some(format!(
                            "{} page(s) extracted to {}",
                            pages.len(),
                            path.file_name().unwrap_or_default().to_string_lossy()
                        ))
                    })
                    .map_err(|err| format!("Extract failed: {err}")),
                None => Ok(None),
            },
            PageAction::Duplicate => doc
                .duplicate_pages(&pages)
                .map(|count| Some(format!("{count} page(s) duplicated")))
                .map_err(|err| format!("Duplicate failed: {err}")),
            PageAction::Delete => {
                if pages.len() >= total {
                    Err("Keep at least one page".to_owned())
                } else {
                    doc.delete_pages(&pages)
                        .map(|count| Some(format!("{count} page(s) removed")))
                        .map_err(|err| format!("Delete failed: {err}"))
                }
            }
            PageAction::Move(from, to) => doc
                .move_page(from, to)
                .map(|_| Some(format!("Page {from} moved to position {to}")))
                .map_err(|err| format!("Move failed: {err}")),
            PageAction::Properties(_) => Ok(None),
        };

        let structural = matches!(
            action,
            PageAction::Rotate(_)
                | PageAction::Blank(_, _)
                | PageAction::InsertFile
                | PageAction::Duplicate
                | PageAction::Delete
                | PageAction::Move(_, _)
        );
        let message = match outcome {
            Ok(message) => {
                if structural && message.is_some() {
                    // Les numéros de page changent : la sélection ne suit plus.
                    self.thumb_selected.clear();
                    self.mark_document_changed();
                    if let Some(tab) = self.tabs.get_mut(active) {
                        let count = tab.doc.page_count().max(1);
                        if tab.page > count {
                            tab.page = count;
                        }
                    }
                }
                message
            }
            Err(message) => Some(message),
        };
        if let Some(message) = message {
            self.toast(ctx, message);
        }
    }

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
                        ui.label(RichText::new(label).size(12.0).color(pal().muted));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(value).size(12.0));
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
                        RichText::new("No page boxes declared")
                            .size(11.0)
                            .color(pal().muted),
                    );
                }
            },
        );
        if !open {
            self.page_props = None;
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
                if resp.hovered() { pal().icon_bright } else { pal().icon },
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
                vec2(self.panel_w - 26.0, 28.0),
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
                    vec2(self.panel_w - 26.0, 28.0),
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
                                if resp.hovered() { pal().icon_bright } else { pal().icon },
                            );
                            if resp.clicked() {
                                clear_history = true;
                            }
                        });
                    });
                    ui.add_space(6.0);
                    for (idx, query) in self.history.iter().enumerate() {
                        let (rect, resp) =
                            ui.allocate_exact_size(vec2(self.panel_w - 24.0, 28.0), Sense::click());
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
                        ui.painter().text(
                            pos2(rect.min.x + 10.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            shorten(query, 22),
                            FontId::proportional(11.5),
                            pal().icon_bright,
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
                            if x_resp.hovered() { pal().icon_bright } else { pal().muted },
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
                                    if resp.hovered() { pal().icon_bright } else { pal().icon },
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
                            ui.allocate_exact_size(vec2(self.panel_w - 24.0, 28.0), Sense::click());
                        if resp.hovered() || active == Some(idx) {
                            ui.painter().rect_filled(
                                rect,
                                Rounding::same(6.0),
                                if active == Some(idx) {
                                    pal().hover_bg
                                } else {
                                    pal().widget_bg
                                },
                            );
                        }
                        ui.painter().text(
                            pos2(rect.min.x + 8.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            format!("#{}", result.page),
                            FontId::proportional(11.5),
                            pal().accent,
                        );
                        ui.painter().text(
                            pos2(rect.min.x + 40.0, rect.center().y),
                            Align2::LEFT_CENTER,
                            shorten(result.context.trim(), 24),
                            FontId::proportional(11.0),
                            pal().muted,
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
            let rect = rect_to_page(&tab.doc, page, rect);
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

    /// Fenêtre de caviardage, en deux temps.
    ///
    /// Première phase : le marquage. On choisit l'apparence des zones — le
    /// remplissage, la couleur, le texte superposé. Deuxième phase : la
    /// confirmation, qui dit honnêtement ce que le moteur va faire.
    fn redact_window(&mut self, ctx: &egui::Context) {
        if !self.redact_modal {
            return;
        }
        let areas = self.redact_marks.len();
        let mut confirm = false;
        let mut cancel = false;
        let mut advance = false;
        let mut keep_open = true;
        let phase = self.redact_phase;
        self.show_modal(ctx, "modal-redact", "Redaction", 420.0, &mut keep_open, |app, ui| {
            if phase == RedactPhase::Marking {
                ui.label(
                    RichText::new(format!("{areas} area(s) marked."))
                        .size(12.5)
                        .color(pal().icon_bright),
                );
                ui.add_space(8.0);

                ui.label(RichText::new("Fill").size(11.0).color(pal().muted));
                ui.horizontal(|ui| {
                    ui.selectable_value(
                        &mut app.redact_fill,
                        RedactionFill::Black,
                        "Black",
                    );
                    ui.selectable_value(
                        &mut app.redact_fill,
                        RedactionFill::White,
                        "White",
                    );
                    ui.selectable_value(
                        &mut app.redact_fill,
                        RedactionFill::Custom(app.redact_custom_color),
                        "Custom",
                    );
                });
                if matches!(app.redact_fill, RedactionFill::Custom(_)) {
                    let mut color = Color32::from_rgb(
                        app.redact_custom_color[0],
                        app.redact_custom_color[1],
                        app.redact_custom_color[2],
                    );
                    ui.horizontal(|ui| {
                        ui.color_edit_button_srgba(&mut color);
                        if ui.button("Use colour").clicked() {
                            app.redact_custom_color = [color.r(), color.g(), color.b()];
                            app.redact_fill = RedactionFill::Custom(app.redact_custom_color);
                        }
                    });
                }

                ui.add_space(8.0);
                ui.label(RichText::new("Overlay text").size(11.0).color(pal().muted));
                ui.add(
                    egui::TextEdit::singleline(&mut app.redact_overlay_text)
                        .hint_text("[REDACTED]")
                        .desired_width(f32::INFINITY),
                );
                if !app.redact_overlay_text.is_empty() {
                    ui.horizontal(|ui| {
                        for font in PatchFont::ALL {
                            ui.selectable_value(
                                &mut app.redact_overlay_font,
                                font,
                                font.display_name(),
                            );
                        }
                        ui.add(
                            egui::DragValue::new(&mut app.redact_overlay_size)
                                .range(6.0..=72.0)
                                .suffix(" pt"),
                        );
                    });
                }

                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Continue").color(Color32::WHITE))
                                .fill(pal().accent),
                        )
                        .clicked()
                    {
                        advance = true;
                    }
                });
            } else {
                ui.label(
                    RichText::new(format!("{areas} area(s) will be redacted permanently."))
                        .size(12.5)
                        .color(pal().icon_bright),
                );
                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "The content under the marked areas is destroyed, not hidden, and cannot be recovered. The rest of each page keeps its selectable text, but a page whose content cannot be sliced cleanly may still be rasterized. Annotations overlapping a marked area are deleted.",
                    )
                    .size(11.0)
                    .color(pal().muted),
                );
                ui.add_space(8.0);
                ui.checkbox(
                    &mut app.redact_sanitize,
                    "Also remove metadata, XMP, JavaScript and attachments",
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("Back").clicked() {
                        app.redact_phase = RedactPhase::Marking;
                    }
                    if ui
                        .add(
                            egui::Button::new(RichText::new("Apply").color(Color32::WHITE))
                                .fill(pal().redact_red),
                        )
                        .clicked()
                    {
                        confirm = true;
                    }
                });
            }
        });
        if cancel || !keep_open {
            self.redact_modal = false;
        }
        if advance {
            self.redact_phase = RedactPhase::Confirming;
        }
        if confirm {
            self.apply_redactions(ctx);
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

    /// Applique une action de l'inspecteur d'image.
    fn image_action(&mut self, action: ImageAction, ctx: &egui::Context) {
        let Some(selected) = self.selected_image.clone() else {
            return;
        };
        let info = self.tabs.get(self.active).and_then(|tab| {
            tab.doc
                .extract_images(selected.page as u32)
                .ok()
                .and_then(|list| {
                    list.into_iter()
                        .find(|info| info.resource_name == selected.name)
                })
        });
        let Some(info) = info else {
            self.selected_image = None;
            self.inspector_target = None;
            return;
        };
        match action {
            ImageAction::Replace => {
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
                let replaced = self
                    .tabs
                    .get_mut(self.active)
                    .map(|tab| tab.doc.replace_image_pixels(&info, &pixels, width, height));
                match replaced {
                    Some(Ok(())) => {
                        self.mark_document_changed();
                        self.toast(ctx, "Image replaced");
                    }
                    Some(Err(err)) => self.toast(ctx, format!("Replace failed: {err}")),
                    None => {}
                }
            }
            ImageAction::Extract => {
                let Some(path) = rfd::FileDialog::new()
                    .set_file_name("image.png")
                    .save_file()
                else {
                    return;
                };
                let png = self
                    .tabs
                    .get(self.active)
                    .map(|tab| tab.doc.extract_image_png(&info));
                match png {
                    Some(Ok(bytes)) => match std::fs::write(&path, &bytes) {
                        Ok(()) => self.toast(ctx, "Image exported"),
                        Err(err) => self.toast(ctx, format!("Export failed: {err}")),
                    },
                    Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                    None => {}
                }
            }
            ImageAction::Transform(transform) => {
                let done = self
                    .tabs
                    .get_mut(self.active)
                    .map(|tab| tab.doc.transform_image(&info, transform));
                match done {
                    Some(Ok(())) => self.mark_document_changed(),
                    Some(Err(err)) => self.toast(ctx, format!("Image failed: {err}")),
                    None => {}
                }
            }
            ImageAction::StartCrop => {
                self.image_crop = Some(ImageCrop::default());
            }
            ImageAction::ApplyCrop => {
                let crop = self.image_crop.take().unwrap_or_default();
                if crop.is_empty() {
                    return;
                }
                // Les poignées vivent sur la boîte **affichée** ; le cœur
                // rogne selon les axes de l'image. Les deux repères diffèrent
                // sur une page tournée, et plus encore après une rotation de
                // l'image : on passe par la matrice de dessin.
                let crop = self
                    .tabs
                    .get(self.active)
                    .and_then(|tab| {
                        let matrix = tab.doc.image_matrix(&info).ok().flatten()?;
                        let page_box = tab.doc.image_rect(&info).ok().flatten()?;
                        let view = tab.doc.page_view(selected.page as u32);
                        let [dx1, dy1, dx2, dy2] = view.display_rect(page_box);
                        let (dw, dh) = ((dx2 - dx1).max(1.0), (dy2 - dy1).max(1.0));
                        let kept = [
                            dx1 + crop.left * dw,
                            dy1 + crop.bottom * dh,
                            dx2 - crop.right * dw,
                            dy2 - crop.top * dh,
                        ];
                        let [kx1, ky1, kx2, ky2] = view.page_rect(kept);
                        Some(crop_fractions_in_image(matrix, [kx1, ky1, kx2, ky2]))
                    })
                    .unwrap_or(crop);
                let done = self
                    .tabs
                    .get_mut(self.active)
                    .map(|tab| tab.doc.crop_image(&info, crop));
                match done {
                    Some(Ok(())) => self.mark_document_changed(),
                    Some(Err(err)) => self.toast(ctx, format!("Crop failed: {err}")),
                    None => {}
                }
            }
            ImageAction::CancelCrop => {
                self.image_crop = None;
            }
            ImageAction::Opacity(alpha) => {
                let done = self
                    .tabs
                    .get_mut(self.active)
                    .map(|tab| tab.doc.set_image_opacity(&info, alpha));
                match done {
                    Some(Ok(())) => self.mark_document_changed(),
                    Some(Err(err)) => self.toast(ctx, format!("Opacity failed: {err}")),
                    None => {}
                }
            }
            ImageAction::Delete => {
                let done = self
                    .tabs
                    .get_mut(self.active)
                    .map(|tab| tab.doc.delete_image(&info));
                match done {
                    Some(Ok(())) => {
                        self.mark_document_changed();
                        self.toast(ctx, "Image deleted");
                    }
                    Some(Err(err)) => self.toast(ctx, format!("Delete failed: {err}")),
                    None => {}
                }
            }
        }
        self.refresh_selected_image();
    }

    /// Inspecteur d'attributs flottant, posé au-dessus de la sélection.
    ///
    /// Le placement vient de [`place_inspector`] : jamais hors de la fenêtre,
    /// flèche pointant vers l'élément. Le cadre suit la taille réelle de son
    /// contenu : elle est mesurée à la trame et sert à la suivante, sinon un
    /// texte long déborderait du fond.
    fn inspector(&mut self, ctx: &egui::Context) {
        let Some(target) = self.inspector_target.clone() else {
            return;
        };
        if self.home_open || self.tabs.is_empty() {
            return;
        }
        let annot = match &target {
            InspectorTarget::AnnotationElement { id, .. } => self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.annots.get(id.0))
                .cloned(),
            _ => None,
        };
        // Taille de la trame précédente ; à défaut, une estimation par type de
        // cible, large plutôt que courte : un panneau qui se croit plus haut
        // qu'il n'est laisse un vide, plus bas il recouvre son élément.
        let fallback = if matches!(target, InspectorTarget::TextElement { .. }) {
            vec2(INSPECTOR_WIDTH, TEXT_INSPECTOR_HEIGHT)
        } else {
            vec2(INSPECTOR_WIDTH, INSPECTOR_HEIGHT)
        };
        let mut size = self.inspector_size.unwrap_or(fallback);
        if let InspectorTarget::TextElement { .. } = &target {
            if self.inspector_text_ui.palette_open {
                size.y += PALETTE_HEIGHT;
            }
        }
        let placement = place_inspector(target.bounds(), ctx.screen_rect(), size);
        let mut measured: Option<egui::Vec2> = None;
        let mut style_changed = false;
        let mut close = false;
        let mut delete = false;
        let mut delete_id = None;
        let mut image_action = None;
        Area::new(Id::new("inspector"))
            .fixed_pos(placement.rect.min)
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let body = Frame::none()
                    .fill(pal().widget_bg)
                    .stroke(Stroke::new(1.0_f32, pal().separator))
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(8.0, 6.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(target.headline())
                                    .size(11.5)
                                    .color(pal().icon_bright),
                            );
                            ui.label(
                                RichText::new(format!("p. {}", target.page()))
                                    .size(10.5)
                                    .color(pal().muted),
                            );
                            if let Some(annot) = &annot {
                                ui.label(
                                    RichText::new(shorten(&annot.author, 10))
                                        .size(10.5)
                                        .color(pal().muted),
                                );
                                if ui.small_button("Delete").clicked() {
                                    delete = true;
                                    delete_id = Some(annot.id.clone());
                                }
                            }
                            if ui.small_button("Close").clicked() {
                                close = true;
                            }
                        });
                        // Contrôles de texte : ils agissent sur le bloc en
                        // cours de réécriture, qui porte la mise en forme.
                        if matches!(target, InspectorTarget::TextElement { .. }) {
                            if let Some(draft) = self.edit_text.as_mut() {
                                style_changed = text_style_row(
                                    ui,
                                    &mut draft.style,
                                    &mut self.inspector_text_ui,
                                );
                            }
                        }
                        // Contrôles de l'image sélectionnée.
                        if matches!(target, InspectorTarget::ImageElement { .. }) {
                            image_action = image_controls(
                                ui,
                                &mut self.image_alpha,
                                self.image_crop.is_some(),
                            );
                        }
                    });
                let rect = body.response.rect;
                measured = Some(rect.size());
                paint_arrow(
                    ui.painter(),
                    rect,
                    arrow_x(target.bounds(), rect),
                    placement.arrow_up,
                    pal().widget_bg,
                    pal().separator,
                );
            });
        self.inspector_size = measured;
        if let Some(action) = image_action {
            self.image_action(action, ctx);
        }
        if style_changed {
            // L'aperçu sur la page suit le style : un repaint immédiat évite
            // de croire que le réglage n'a rien fait.
            ctx.request_repaint();
        }
        if delete {
            // C'est bien l'annotation visée qui part, pas celle que la trame a
            // sélectionnée entre-temps.
            if let Some(id) = delete_id {
                let idx = self
                    .tabs
                    .get(self.active)
                    .and_then(|tab| tab.annots.iter().position(|a| a.id == id));
                if let Some(idx) = idx {
                    self.selected_annot = idx;
                    self.remove_annot(idx);
                }
            }
            close = true;
        }
        if close {
            self.has_annot_selection = false;
            self.inspector_target = None;
        }
    }

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
            + if self.panel.is_some() { self.panel_w } else { 0.0 };

        // `Outils ▾` à gauche.
        Area::new(Id::new("tools"))
            .fixed_pos(pos2(canvas_left + 10.0, top))
            .order(Order::Foreground)
            .show(ctx, |ui| {
                let inner = Frame::none()
                    .fill(pal().widget_bg)
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(10.0, 5.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                            icon_tools(ui.painter(), rect, pal().icon_bright);
                            ui.add_space(6.0);
                            ui.label(RichText::new(t("Tools")).size(12.5).color(pal().icon_bright));
                            ui.add_space(4.0);
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
                        .rounding(Rounding::same(8.0))
                        .stroke(Stroke::new(1.0_f32, pal().separator))
                        .inner_margin(Margin::symmetric(4.0, 4.0))
                        .show(ui, |ui| {
                            for (idx, (label, shortcut, entry)) in TOOLS_MENU.iter().enumerate() {
                                // Deux séparateurs : après les quatre modes, et
                                // avant la conversion et l'entretien.
                                if idx == 4 || idx == 11 {
                                    ui.add_space(4.0);
                                    let sep = ui.cursor().top();
                                    ui.painter().hline(
                                        ui.max_rect().x_range(),
                                        sep,
                                        Stroke::new(1.0_f32, pal().separator),
                                    );
                                    ui.add_space(4.0);
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
                        .rounding(Rounding::same(8.0))
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
                                                    RichText::new(label).size(12.0),
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
                                                RichText::new("Fullscreen").size(12.0),
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
                                                    RichText::new(theme.label()).size(12.0),
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
                    ui.label(RichText::new(label).size(12.0).color(pal().muted));
                    ui.label(RichText::new(value).size(12.0).color(pal().icon_bright));
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



    /// Fenêtre « Comparer des PDF » : différences de texte entre deux fichiers.
    fn compare_window(&mut self, ctx: &egui::Context) {
        if !self.compare_open {
            return;
        }
        let mut open = true;
        let mut pick = false;
        let mut run = false;
        let mut jump: Option<usize> = None;
        let name = self
            .compare_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        egui::Window::new("Compare PDFs")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(560.0, 440.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Choose a PDF…").clicked() {
                        pick = true;
                    }
                    ui.label(
                        RichText::new(
                            name.clone().unwrap_or_else(|| "No file chosen".to_owned()),
                        )
                        .size(12.0)
                        .color(if name.is_some() { pal().icon_bright } else { pal().muted }),
                    );
                });
                ui.add_space(6.0);
                if ui.button("Compare with the open document").clicked() {
                    run = true;
                }
                if let Some(report) = &self.compare_report {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.label(
                        RichText::new(format!(
                            "{} page(s) vs {} page(s) — {} difference(s), \
                             {} identical line(s)",
                            report.pages_a,
                            report.pages_b,
                            report.differences.len(),
                            report.identical_lines
                        ))
                        .size(12.0)
                        .color(pal().icon_bright),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        for (label, color) in [
                            ("Inserted", ADDED_GREEN),
                            ("Deleted", REMOVED_RED),
                            ("Modified", MODIFIED_AMBER),
                        ] {
                            ui.label(
                                RichText::new(format!("\u{25a0} {label}"))
                                    .size(10.5)
                                    .color(color),
                            );
                        }
                    });
                    ui.add_space(4.0);
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for diff in report.differences.iter().take(300) {
                            let (tag, color) = match diff.kind {
                                DifferenceKind::Added => ("+", ADDED_GREEN),
                                DifferenceKind::Removed => ("-", REMOVED_RED),
                                DifferenceKind::Modified => ("~", MODIFIED_AMBER),
                                DifferenceKind::PageAdded => ("+ page", ADDED_GREEN),
                                DifferenceKind::PageRemoved => ("- page", REMOVED_RED),
                            };
                            let text = diff
                                .after
                                .clone()
                                .or_else(|| diff.before.clone())
                                .unwrap_or_default();
                            let page = diff.page_b.or(diff.page_a).unwrap_or(0);
                            if ui
                                .selectable_label(
                                    false,
                                    RichText::new(format!("{tag}  p{page}  {text}"))
                                        .size(11.0)
                                        .color(color),
                                )
                                .clicked()
                            {
                                jump = Some(page);
                            }
                        }
                    });
                }
            });

        if let Some(page) = jump {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if page >= 1 {
                    tab.page = page.min(tab.total());
                }
            }
        }
        if pick {
            self.compare_path = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_file();
            self.compare_report = None;
        }
        if run {
            match self.compare_path.clone() {
                Some(path) => match Document::open(&path) {
                    Ok(other) => {
                        let report = self
                            .tabs
                            .get(self.active)
                            .map(|tab| tab.doc.compare_with(&other));
                        match report {
                            Some(Ok(report)) => {
                                let identical = report.is_identical();
                                self.compare_report = Some(report);
                                if identical {
                                    self.toast(ctx, "The two documents match");
                                }
                            }
                            Some(Err(err)) => self.toast(ctx, format!("Compare failed: {err}")),
                            None => self.toast(ctx, "Open a document first"),
                        }
                    }
                    Err(err) => self.toast(ctx, format!("Open failed: {err}")),
                },
                None => self.toast(ctx, "Choose a PDF to compare with"),
            }
        }
        if !open {
            self.compare_open = false;
        }
    }

    /// Fenêtre « OCR » : rend le document consultable par Tesseract.
    fn ocr_window(&mut self, ctx: &egui::Context) {
        if !self.ocr_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        // Tesseract n'est interrogé qu'une fois : `available_languages` lance
        // un processus, et le refaire à chaque trame ferait clignoter la
        // fenêtre tout en lançant un `tesseract --list-langs` par image.
        if !self.ocr_probed {
            let engine = TesseractEngine::detect();
            self.ocr_installed = engine.display_name().contains("système");
            self.ocr_languages = engine.available_languages().unwrap_or_default();
            self.ocr_probed = true;
        }
        let installed = self.ocr_installed;
        let languages = self.ocr_languages.clone();
        self.show_modal(ctx, "modal-ocr", "OCR", 430.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(if installed {
                    "Tesseract (system)"
                } else {
                    "Tesseract (not installed)"
                })
                .size(12.0)
                .color(if installed { pal().icon_bright } else { pal().redact_red }),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Language").size(12.0).color(pal().muted));
                ui.add(TextEdit::singleline(&mut app.ocr_language).desired_width(90.0));
                if !languages.is_empty() {
                    let mut picked: Option<String> = None;
                    egui::ComboBox::from_id_salt("ocr-lang")
                        .selected_text(app.ocr_language.clone())
                        .show_ui(ui, |ui| {
                            for lang in languages.iter().take(300) {
                                if ui
                                    .selectable_label(app.ocr_language == *lang, lang)
                                    .clicked()
                                {
                                    picked = Some(lang.clone());
                                }
                            }
                        });
                    if let Some(lang) = picked {
                        app.ocr_language = lang;
                    }
                }
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Output").size(12.0).color(pal().muted));
            for &(mode, label, detail) in OcrMode::ALL {
                if ui
                    .selectable_label(app.ocr_mode == mode, format!("{label} — {detail}"))
                    .clicked()
                {
                    app.ocr_mode = mode;
                }
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "Recognition rewrites the document: each page becomes an image plus \
                     an invisible text layer. It runs on the interface thread and can \
                     take a while on a long scan.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(6.0);
            if ui
                .add_enabled(installed, egui::Button::new("Recognise"))
                .clicked()
            {
                run = true;
            }
            if let Some(report) = &app.ocr_report {
                ui.add_space(6.0);
                ui.label(RichText::new(report).size(11.0).color(pal().icon_bright));
            }
        });

        if run {
            let language = self.ocr_language.clone();
            let mode = self.ocr_mode;
            let result = {
                let engine = TesseractEngine::detect();
                self.tabs.get(self.active).map(|tab| {
                    make_ocr(&tab.doc, &*self.renderer, &engine, &language, mode)
                })
            };
            match result {
                Some(Ok(doc)) => {
                    let pages = doc.page_count();
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.doc = doc;
                    }
                    self.mark_document_changed();
                    let report = format!("{pages} page(s) recognised");
                    self.ocr_report = Some(report.clone());
                    self.toast(ctx, format!("OCR done: {report}"));
                }
                Some(Err(err)) => {
                    let message = format!("{err}");
                    self.ocr_report = Some(message.clone());
                    self.toast(ctx, format!("OCR failed: {message}"));
                }
                None => self.toast(ctx, "Open a document first"),
            }
        }
        if !open {
            self.ocr_open = false;
        }
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
            ui.label(RichText::new("Blank document").size(12.0).color(pal().muted));
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pages").size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.create_pages).range(1..=500));
                if ui.button("Create blank").clicked() {
                    make_blank = true;
                }
            });
            ui.add_space(10.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(RichText::new("From images").size(12.0).color(pal().muted));
            ui.label(
                RichText::new(
                    "One page per image, in the order chosen. PNG and JPEG are read.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(4.0);
            if ui.button("Choose images…").clicked() {
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

    /// Fenêtre d'invite de mot de passe (ouverture ou déprotection).
    fn password_window(&mut self, ctx: &egui::Context) {
        if self.password_prompt.is_none() {
            return;
        }
        let mut open = true;
        let mut submit = false;
        let mut cancel = false;
        let (title, name, purpose) = {
            let prompt = self.password_prompt.as_ref().unwrap();
            let title = match prompt.purpose {
                PasswordPurpose::Open => "Password required",
                PasswordPurpose::RemoveProtection => "Remove protection",
            };
            let name = prompt
                .path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "This file".to_owned());
            (title, name, prompt.purpose)
        };
        self.show_modal(ctx, "modal-password", title, 340.0, &mut open, |app, ui| {
            let prompt = app.password_prompt.as_mut().unwrap();
            ui.label(
                RichText::new(format!("{name} is protected. Enter its password."))
                    .size(12.0)
                    .color(pal().icon_bright),
            );
            ui.add_space(6.0);
            let resp = ui.add(
                TextEdit::singleline(&mut prompt.value)
                    .password(true)
                    .desired_width(300.0)
                    .hint_text("Password"),
            );
            if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                submit = true;
            }
            if let Some(err) = &prompt.error {
                ui.add_space(4.0);
                ui.label(RichText::new(err).size(11.0).color(pal().redact_red));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() {
                    submit = true;
                }
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
            });
        });
        if !open {
            cancel = true;
        }
        if cancel {
            self.password_prompt = None;
            return;
        }
        if !submit {
            return;
        }
        let prompt = self.password_prompt.take().unwrap();
        match Document::open_protected(&prompt.path, &prompt.value) {
            Ok(doc) => match purpose {
                PasswordPurpose::Open => self.push_document(doc, Some(prompt.path)),
                PasswordPurpose::RemoveProtection => {
                    let stem = prompt
                        .path
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "document".to_owned());
                    if let Some(out) = rfd::FileDialog::new()
                        .add_filter("PDF", &["pdf"])
                        .set_file_name(format!("{stem}-unprotected.pdf"))
                        .save_file()
                    {
                        match doc.save_decrypted(&out) {
                            Ok(()) => {
                                let name = out
                                    .file_name()
                                    .map(|n| n.to_string_lossy().into_owned())
                                    .unwrap_or_default();
                                self.toast(ctx, format!("Unprotected copy written to {name}"));
                            }
                            Err(err) => {
                                self.toast(ctx, format!("Remove protection failed: {err}"));
                            }
                        }
                    }
                }
            },
            Err(err) => {
                let mut prompt = prompt;
                prompt.value.clear();
                prompt.error = Some(format!("{err}"));
                self.password_prompt = Some(prompt);
            }
        }
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
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(10.0, 6.0))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{page} / {total}"))
                                .size(12.0)
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new("← → to turn, Esc to leave")
                                .size(10.5)
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

    /// Fenêtre « Mesure » : nature, échelle, unité, activation.
    fn measure_window(&mut self, ctx: &egui::Context) {
        if !self.measure_open {
            return;
        }
        let mut open = true;
        let mut clear = false;
        self.show_modal(ctx, "modal-measure", "Measure", 340.0, &mut open, |app, ui| {
            ui.checkbox(&mut app.measure_active, "Draw on the page");
            ui.add_space(6.0);
            ui.label(RichText::new("Kind").size(12.0).color(pal().muted));
            for kind in [
                MeasureKind::Distance,
                MeasureKind::Perimeter,
                MeasureKind::Area,
            ] {
                if ui
                    .selectable_label(app.measure_kind == kind, kind.display_name())
                    .clicked()
                {
                    app.measure_kind = kind;
                }
            }
            ui.add_space(6.0);
            egui::Grid::new("measure-grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Scale").size(12.0).color(pal().muted));
                    ui.add(
                        egui::DragValue::new(&mut app.measure_scale)
                            .range(0.01..=10_000.0)
                            .speed(1.0),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Unit").size(12.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut app.measure_unit).desired_width(80.0),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Known").size(12.0).color(pal().muted));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::DragValue::new(&mut app.measure_known)
                                .range(0.0001..=1_000_000.0)
                                .speed(0.1),
                        );
                        if ui
                            .add_enabled(
                                app.measure_last_len.is_some(),
                                egui::Button::new("Calibrate"),
                            )
                            .on_hover_text(
                                "Use the last drawn distance as the known real distance",
                            )
                            .clicked()
                        {
                            if let Some(length) = app.measure_last_len {
                                app.measure_scale = length / app.measure_known.max(0.0001);
                            }
                        }
                    });
                    ui.end_row();
                });
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Scale is drawing points per real unit: 72 with unit \"in\" reads inches \
                     at 1:1.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(4.0);
            ui.label(
                RichText::new(
                    "Distance follows the drag; perimeter and area take the dragged \
                     rectangle.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(8.0);
            if ui.button("Remove all measurements").clicked() {
                clear = true;
            }
        });
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_measurements());
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {count} measurement(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.measure_open = false;
            // Sans la fenêtre, l'outil n'a plus de réglages visibles : le
            // laisser actif bloquerait tous les autres outils du canevas.
            self.measure_active = false;
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

    /// Fenêtre « Extraire les données » : tableaux et paires clé/valeur.
    fn extract_window(&mut self, ctx: &egui::Context) {
        if !self.extract_open {
            return;
        }
        let mut open = true;
        let mut scan = false;
        let mut export_tables = false;
        let mut export_pairs = false;
        egui::Window::new("Extract data")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(560.0, 460.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(560.0);
                ui.horizontal(|ui| {
                    if ui.button("Scan the document").clicked() {
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
                ui.add_space(6.0);
                ui.label(
                    RichText::new(format!(
                        "{} table(s), {} key/value pair(s)",
                        self.extract_tables.len(),
                        self.extract_pairs.len()
                    ))
                    .size(11.0)
                    .color(pal().muted),
                );
                ui.add_space(6.0);
                ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                    for (idx, table) in self.extract_tables.iter().enumerate() {
                        ui.label(
                            RichText::new(format!("Table {} — page {}", idx + 1, table.page))
                                .size(11.5)
                                .color(pal().icon_bright),
                        );
                        for row in &table.rows {
                            ui.label(
                                RichText::new(row.join("  |  ")).size(11.0).color(pal().icon_bright),
                            );
                        }
                        ui.add_space(6.0);
                    }
                    for pair in &self.extract_pairs {
                        ui.label(
                            RichText::new(format!("p{}  {} = {}", pair.page, pair.key, pair.value))
                                .size(11.0)
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
                    let line: Vec<String> = row.iter().map(|cell| csv_cell(cell)).collect();
                    body.push_str(&line.join(","));
                    body.push('\n');
                }
            }
            self.save_extract(ctx, "tables.csv", &body);
        }
        if export_pairs {
            let mut body = String::from("key,value\n");
            for pair in &self.extract_pairs {
                body.push_str(&format!("{},{}", csv_cell(&pair.key), csv_cell(&pair.value)));
                body.push('\n');
            }
            self.save_extract(ctx, "pairs.csv", &body);
        }
        if !open {
            self.extract_open = false;
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

    /// Fenêtre « Traitement par lots » : plusieurs fichiers, mêmes opérations.
    fn canvas(&mut self, ui: &mut egui::Ui) {
        if self.tabs.is_empty() {
            self.empty_state(ui);
            return;
        }

        // Pincement / ⌘-molette = zoom (egui agrège les deux).
        let zoom_delta = ui.input(|i| i.zoom_delta());
        if (zoom_delta - 1.0).abs() > 0.0001 {
            self.zoom_dirty = true;
            self.zoom_by(zoom_delta);
        }
        // Horodatage du geste : il gèle la rastérisation, on étire le palier
        // voisin, puis le rendu net arrive une fois la main levée.
        if self.zoom_dirty {
            self.zoom_dirty = false;
            self.zoom_moved_at = ui.input(|i| i.time);
        }
        let now = ui.input(|i| i.time);
        if now - self.zoom_moved_at < ZOOM_SETTLE_SECS + 0.15 {
            // La trame nette est encore due : on la programme.
            ui.ctx().request_repaint();
        }
        let zoom_settled = now - self.zoom_moved_at >= ZOOM_SETTLE_SECS;
        // Défilement : même mécanique que le zoom. La molette et le
        // trackpad signalent le geste tout de suite ; le glissement de la
        // barre de défilement est repéré plus bas, d'après le mouvement des
        // pages. Tant que le geste dure, on rasterise à basse résolution.
        if ui.input(|i| i.raw_scroll_delta.length() > 0.0) {
            self.scroll_moved_at = now;
        }
        let scroll_settled = now - self.scroll_moved_at >= SCROLL_SETTLE_SECS;
        let fast = !zoom_settled || !scroll_settled;
        if fast {
            // La trame nette est encore due : on la programme.
            ui.ctx().request_repaint();
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
        // Couleur du fond sous le bloc réécrit : lue avant que `self.tabs` ne
        // soit emprunté, `self` n'est plus accessible dans la boucle.
        let mask_bg = match draft.as_ref() {
            Some(d) => self.mask_color(d.fragment.page, d.fragment.rect),
            None => [255, 255, 255],
        };
        let mut edit_commit = false;
        let mut edit_cancel = false;
        // Sélection de texte : c'est l'état de lecture. Elle est active quand
        // aucun mode n'est ouvert, ou dans le mode Commentaire sans outil ou
        // avec l'outil Sélection. Aucun de ces états ne consomme le glissé.
        // Les autres modes et les outils qui dessinent gardent le leur.
        let select_text_tool = matches!(self.mode, None | Some(0))
            && matches!(self.annot_tool, None | Some(0));
        // Une mise en évidence se pose sur le texte : son glissé sélectionne,
        // il ne dessine pas de rectangle.
        let markup_kind = match action {
            ToolAction::DragRect(kind) if kind.is_markup() => Some(kind),
            _ => None,
        };
        let text_drag = select_text_tool || markup_kind.is_some();
        let mut sel = self.text_sel;
        let mut sel_copy = false;
        let mut sel_all = false;
        let renderer = &*self.renderer;
        let status = &mut self.render_status;
        let last_fit = &mut self.last_fit;
        let page_layout = self.page_layout;
        let tab = &mut self.tabs[active];
        let total = tab.total();
        let sense = if action == ToolAction::None && !select_text_tool {
            Sense::click()
        } else {
            Sense::click_and_drag()
        };
        let annot_width = self.annot_width;
        let eraser_mode = self.eraser_mode;
        let ink_current = &mut self.ink_current;
        let drag_from = &mut self.drag_from;
        let drag_to = &mut self.drag_to;
        let mut pending_rect: Option<(usize, AnnotationKind, [f32; 4])> = None;
        let mut pending_markup: Option<(usize, AnnotationKind, Vec<[f32; 8]>)> = None;
        let mut pending_ink: Option<(usize, Vec<[f32; 2]>)> = None;
        let mut pending_click: Option<(usize, [f32; 2])> = None;
        let mut pending_hit: Option<(usize, [f32; 2])> = None;
        let mut pending_erase_segment: Option<(usize, [f32; 2], [f32; 2])> = None;
        let mut pending_callout: Option<(usize, [f32; 2], [f32; 2], [f32; 2])> = None;
        let mut pending_edit: Option<(usize, [f32; 2])> = None;
        let mut pending_image: Option<(usize, [f32; 2])> = None;
        let mut pending_redact: Option<(usize, [f32; 4])> = None;
        let mut pending_form: Option<(usize, [f32; 4], FormKind)> = None;
        let mut pending_form_select: Option<(usize, [f32; 4])> = None;
        let mut pending_measure: Option<(usize, [f32; 2], [f32; 2])> = None;
        let redact_marks = &self.redact_marks;
        let search_rects = &self.search_rects;
        let search_rects_page = self.search_rects_page;
        let selected_annot = self.selected_annot;
        let has_selection = self.has_annot_selection;
        let show_shadow = self.page_shadow;
        let tint = PAGE_TINTS[self.page_tint.min(PAGE_TINTS.len() - 1)].1;
        let tint_color = Color32::from_rgba_unmultiplied(tint[0], tint[1], tint[2], tint[3]);
        let tint_on = tint[3] > 0;
        let show_border = self.page_border;
        // Le clic qui vise l'inspecteur ne doit pas valider la saisie en cours :
        // sans ce garde, le premier clic sur un réglage rendrait le focus du
        // champ, clavier compris, avant même d'avoir pu le régler.
        let inspector_hovered = self.inspector_target.as_ref().is_some_and(|target| {
            let size = self
                .inspector_size
                .unwrap_or(vec2(INSPECTOR_WIDTH, TEXT_INSPECTOR_HEIGHT));
            let rect = place_inspector(target.bounds(), ui.ctx().screen_rect(), size).rect;
            ui.ctx()
                .pointer_latest_pos()
                .is_some_and(|pos| rect.expand(6.0).contains(pos))
        });
        // L'inspecteur suit la sélection : la trame du canevas le calcule, la
        // barre d'outils (dessinée après) le pose.
        let mode = self.mode;
        let form_highlight = self.form_highlight;
        let form_editing = self.form_editing;
        let form_hover = self.form_hover;
        let form_selection = self.form_selection.clone();
        let mut form_draft = self.form_edit.take();
        let mut form_commit = false;
        let mut form_cancel = false;
        let mut inspector_target: Option<InspectorTarget> = None;
        // L'image sélectionnée et le rognage en cours sont lus une fois : la
        // trame du canevas emprunte les onglets, elle ne peut pas toucher à
        // `self` en même temps.
        let selected_image = self.selected_image.clone();
        let crop_draft = self.image_crop;
        let mut crop_next: Option<ImageCrop> = None;

        // Réserve pour les barres flottantes : les pages commencent sous la
        // barre d'outils, sinon la première page passe dessous.
        ui.add_space(CANVAS_TOP_GAP);
        // Position écran de la première page : sa variation d'une trame à
        // l'autre trahit un défilement que la molette n'a pas signalé
        // (barre de défilement tenue, inertie).
        let mut first_page_y: Option<f32> = None;

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
                        .map(|page| tab.page_size_cached(*page))
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
                if let Some((_, _, page_rect, _, _)) = placed.first() {
                    first_page_y = Some(page_rect.min.y);
                }

                // Une seule page rastérisée par trame, et seulement une fois
                // le zoom et le défilement au repos : la page courante
                // d'abord, puis les autres pages visibles au fil des trames
                // suivantes. La comparaison porte sur le palier *manquant ou
                // trop grossier*, d'où `needs_render` sans négation.
                let pending_render: Option<(u32, i32, f32)> = if zoom_settled && scroll_settled {
                    let mut visible: Vec<(u32, i32, f32)> = placed
                        .iter()
                        .filter(|(_, _, page_rect, _, _)| clip.intersects(*page_rect))
                        .map(|(page, _, _, _, scale)| (*page, render_step(*scale, ppp), *scale * ppp))
                        .collect();
                    visible.sort_by_key(|(page, _, _)| if *page == tab.page as u32 { 0 } else { 1 });
                    visible
                        .into_iter()
                        .find(|(page, step, target)| tab.needs_render(*page, *step, *target))
                } else {
                    None
                };

                for (page, rect, page_rect, row_id, scale) in placed {
                    let size = page_rect.size();
                    let resp = ui.interact(rect, row_id.with(page), sense);

                    // Curseur de surlignage : l'outil de surbrillance est actif
                    // et le pointeur est sur la page. egui n'a pas de curseur
                    // « marqueur » natif, la croix de visée sert de surligneur.
                    // On interroge la position du pointeur et non
                    // `resp.hovered()` : pendant le glissé de surlignage, le
                    // widget n'est plus « survolé ».
                    if markup_kind.is_some() {
                        if let Some(pos) = ctx.pointer_hover_pos() {
                            if rect.contains(pos) {
                                ui.ctx().set_cursor_icon(CursorIcon::Crosshair);
                            }
                        }
                    }

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

                        let step = render_step(scale, ppp);
                        let wanted = pending_render == Some((page, step, scale * ppp));
                        if let Some(tex) = Self::page_texture(
                            &ctx, renderer, tab, page, step, scale, ppp, wanted, fast, status,
                        ) {
                            ui.painter().image(
                                tex.id(),
                                page_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }

                        // Teinte et filet se posent sur la page rendue, sous
                        // tout ce que l'interface ajoute ensuite.
                        if tint_on {
                            ui.painter().rect_filled(page_rect, Rounding::ZERO, tint_color);
                        }
                        if show_border {
                            // Gris moyen plutôt que la couleur de séparation
                            // du thème : le filet doit se lire aussi bien sur
                            // une page blanche que contre un canevas sombre.
                            ui.painter().rect_stroke(
                                page_rect,
                                Rounding::ZERO,
                                Stroke::new(
                                    1.0_f32,
                                    Color32::from_rgba_unmultiplied(0x80, 0x80, 0x88, 170),
                                ),
                            );
                        }

                        // Sélection de texte en cours.
                        if let Some(s) = sel.as_ref() {
                            if s.page == page {
                                if let Some(frags) = tab.fragments.get(&page) {
                                    for [x1, y1, x2, y2] in hit_rects(frags, s.span) {
                                        let r = Rect::from_min_max(
                                            pos2(
                                                page_rect.min.x + x1 * scale,
                                                page_rect.max.y - y2 * scale,
                                            ),
                                            pos2(
                                                page_rect.min.x + x2 * scale,
                                                page_rect.max.y - y1 * scale,
                                            ),
                                        );
                                        ui.painter().rect_filled(
                                            r.expand(1.0),
                                            Rounding::same(2.0),
                                            Color32::from_rgba_unmultiplied(
                                                pal().accent.r(),
                                                pal().accent.g(),
                                                pal().accent.b(),
                                                90,
                                            ),
                                        );
                                    }
                                }
                            }
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
                            // Phase 1 : le marquage est visible mais la page
                            // n'est pas encore caviardée. Le rouge signale
                            // « à supprimer », pas « déjà supprimé ».
                            ui.painter().rect_filled(
                                rect,
                                Rounding::ZERO,
                                Color32::from_rgba_unmultiplied(
                                    pal().redact_red.r(),
                                    pal().redact_red.g(),
                                    pal().redact_red.b(),
                                    110,
                                ),
                            );
                            ui.painter().rect_stroke(
                                rect,
                                Rounding::ZERO,
                                Stroke::new(2.0_f32, pal().redact_red),
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
                                    pal().search_match,
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

                        // Champs de formulaire : teinte bleue quand le
                        // surlignage est actif, plus le champ en cours
                        // d'édition.
                        if form_highlight || form_editing.is_some() {
                            for (index, field) in tab.forms.iter().enumerate() {
                                if field.page != Some(page as usize) || field.rect == [0.0; 4] {
                                    continue;
                                }
                                let [x1, y1, x2, y2] = field.rect;
                                let rect = Rect::from_min_max(
                                    pos2(
                                        page_rect.min.x + x1.min(x2) * scale,
                                        page_rect.max.y - y1.max(y2) * scale,
                                    ),
                                    pos2(
                                        page_rect.min.x + x1.max(x2) * scale,
                                        page_rect.max.y - y1.min(y2) * scale,
                                    ),
                                );
                                let selected = form_selection.contains(&index);
                                let active = form_editing == Some(index)
                                    || form_hover == Some(index)
                                    || selected;
                                ui.painter().rect_filled(
                                    rect,
                                    Rounding::same(2.0),
                                    Color32::from_rgba_unmultiplied(
                                        70,
                                        130,
                                        220,
                                        if active { 70 } else { 34 },
                                    ),
                                );
                                ui.painter().rect_stroke(
                                    rect,
                                    Rounding::same(2.0),
                                    Stroke::new(
                                        if selected { 2.0 } else if active { 1.5 } else { 1.0 },
                                        if selected {
                                            Color32::from_rgb(230, 120, 40)
                                        } else {
                                            Color32::from_rgb(70, 130, 220)
                                        },
                                    ),
                                );
                            }
                        }

                        // Éditeur en place du champ de formulaire touché.
                        if let Some((idx, buffer)) = form_draft.as_mut() {
                            if let Some(field) = tab.forms.get(*idx) {
                                if field.page == Some(page as usize) && field.rect != [0.0; 4] {
                                    let [x1, y1, x2, y2] = field.rect;
                                    let rect = Rect::from_min_max(
                                        pos2(
                                            page_rect.min.x + x1.min(x2) * scale,
                                            page_rect.max.y - y1.max(y2) * scale,
                                        ),
                                        pos2(
                                            page_rect.min.x + x1.max(x2) * scale,
                                            page_rect.max.y - y1.min(y2) * scale,
                                        ),
                                    );
                                    ui.painter().rect_filled(
                                        rect.expand(1.0),
                                        Rounding::same(2.0),
                                        Color32::WHITE,
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        Rounding::same(2.0),
                                        Stroke::new(1.5_f32, pal().accent),
                                    );
                                    let single_line = !field.multiline;
                                    if matches!(
                                        field.kind,
                                        FormFieldKind::Choice | FormFieldKind::MultiChoice
                                    ) {
                                        egui::ComboBox::from_id_salt(("form-choice", *idx))
                                            .width(rect.width())
                                            .selected_text(if buffer.is_empty() {
                                                "Select…".to_string()
                                            } else {
                                                buffer.clone()
                                            })
                                            .show_ui(ui, |ui| {
                                                for option in &field.options {
                                                    if ui
                                                        .selectable_label(buffer == option, option)
                                                        .clicked()
                                                    {
                                                        *buffer = option.clone();
                                                        form_commit = true;
                                                    }
                                                }
                                            });
                                        ui.painter().rect_stroke(
                                            rect,
                                            Rounding::same(2.0),
                                            Stroke::new(1.5_f32, pal().accent),
                                        );
                                    } else {
                                        let edit = if single_line {
                                            TextEdit::singleline(buffer)
                                        } else {
                                            TextEdit::multiline(buffer)
                                        };
                                        let resp = ui.put(
                                            rect.shrink(3.0),
                                            edit.frame(false).desired_width(f32::INFINITY),
                                        );
                                        if ui.memory(|m| m.focused().is_none()) {
                                            resp.request_focus();
                                        }
                                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                            form_cancel = true;
                                        } else if (single_line
                                            && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                            || (resp.lost_focus() && !inspector_hovered)
                                        {
                                            form_commit = true;
                                        }
                                    }
                                }
                            }
                        }
                        if mode == Some(0) {
                            if let Some(idx) = selected {
                                if let Some(annot) = tab.annots.get(idx) {
                                    // Une réponse vit dans le fil, pas sur la
                                    // page : elle n'est pas dessinée, donc pas
                                    // d'inspecteur non plus.
                                    if annot.page == page as usize && annot.reply_to.is_none() {
                                        let screen = Rect::from_two_pos(
                                            pos2(
                                                page_rect.min.x + annot.rect[0] * scale,
                                                page_rect.max.y - annot.rect[3] * scale,
                                            ),
                                            pos2(
                                                page_rect.min.x + annot.rect[2] * scale,
                                                page_rect.max.y - annot.rect[1] * scale,
                                            ),
                                        );
                                        inspector_target = Some(InspectorTarget::AnnotationElement {
                                            page: annot.page,
                                            id: AnnotationId(idx),
                                            bounds: screen,
                                        });
                                    }
                                }
                            }
                        }

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
                                // Un seul masque, au moins aussi grand que le
                                // bloc d'origine *et* que le champ : sinon les
                                // jambages dépassent au-dessus et en dessous,
                                // et une réécriture plus longue que l'original
                                // déborde du blanc. La couleur suit la page.
                                let height =
                                    (block.height().max(d.style.size * scale) * 1.3)
                                        .max(20.0);
                                let field = Rect::from_min_size(
                                    pos2(block.min.x - 3.0, block.center().y - height / 2.0),
                                    vec2(block.width().max(140.0) + 6.0, height),
                                );
                                let mask = block.expand(2.0).union(field);
                                ui.painter().rect_filled(
                                    mask,
                                    Rounding::same(2.0),
                                    Color32::from_rgb(mask_bg[0], mask_bg[1], mask_bg[2]),
                                );
                                ui.painter().rect_stroke(
                                    field,
                                    2.0,
                                    Stroke::new(1.0_f32, pal().accent),
                                );
                                let ink = Color32::from_rgb(
                                    d.style.color[0],
                                    d.style.color[1],
                                    d.style.color[2],
                                );
                                // Courier est la seule des trois familles que
                                // l'interface sait rendre : egui n'embarque
                                // pas de police à empattements.
                                let font_id = match d.style.font {
                                    PatchFont::Courier => {
                                        FontId::monospace((d.style.size * scale).clamp(8.0, 96.0))
                                    }
                                    _ => {
                                        FontId::proportional((d.style.size * scale).clamp(8.0, 96.0))
                                    }
                                };
                                let edit_rect = field.shrink2(vec2(3.0, 0.0));
                                // Un paragraphe (texte déjà coupé) se réécrit
                                // en plusieurs lignes, Entrée y insère un saut ;
                                // une ligne seule se valide avec Entrée.
                                let single_line = d.single_line;
                                let field_edit = if single_line {
                                    TextEdit::singleline(&mut d.buffer)
                                } else {
                                    TextEdit::multiline(&mut d.buffer)
                                };
                                let resp = ui.put(
                                    edit_rect,
                                    field_edit
                                        .frame(false)
                                        .font(font_id.clone())
                                        .text_color(ink),
                                );
                                // Le curseur se pose entre les glyphes sous
                                // le clic d'origine : le champ n'existait pas
                                // encore à ce moment-là, egui ne peut pas le
                                // placer seul.
                                if let Some(x) = d.caret_at.take() {
                                    // Le curseur d'un paragraphe se place à
                                    // deux dimensions : ici on ne sait viser
                                    // qu'une abscisse, donc seulement une
                                    // ligne. `TextEdit` dessine son texte à sa
                                    // marge interne, 4 points à gauche, 2 en
                                    // haut.
                                    if single_line {
                                        let text_origin = edit_rect.min + vec2(4.0, 2.0);
                                        let galley = ui.fonts(|fonts| {
                                            fonts.layout_no_wrap(d.buffer.clone(), font_id, ink)
                                        });
                                        let local = vec2(
                                            (page_rect.min.x + x * scale - text_origin.x)
                                                .clamp(0.0, galley.size().x),
                                            galley.size().y / 2.0,
                                        );
                                        let index = galley.cursor_from_pos(local).ccursor.index;
                                        let mut state = TextEdit::load_state(
                                            ui.ctx(),
                                            resp.id,
                                        )
                                        .unwrap_or_default();
                                        state.cursor.set_char_range(Some(
                                            egui::text_selection::CCursorRange::one(
                                                egui::text::CCursor::new(index),
                                            ),
                                        ));
                                        state.store(ui.ctx(), resp.id);
                                    }
                                }
                                // Aperçu des traits : le texte écrit dans le
                                // PDF les portera, autant les voir ici.
                                if d.style.underline || d.style.strikeout {
                                    let rule = (d.style.size * scale * 0.07).max(0.5);
                                    if d.style.underline {
                                        ui.painter().line_segment(
                                            [
                                                pos2(field.min.x, field.max.y - 3.0),
                                                pos2(field.max.x, field.max.y - 3.0),
                                            ],
                                            Stroke::new(rule, ink),
                                        );
                                    }
                                    if d.style.strikeout {
                                        let y = field.center().y;
                                        ui.painter().line_segment(
                                            [pos2(field.min.x, y), pos2(field.max.x, y)],
                                            Stroke::new(rule, ink),
                                        );
                                    }
                                }
                                // Inspecteur du bloc en cours de réécriture.
                                if mode == Some(1) {
                                    let block_id = tab
                                        .fragments
                                        .get(&page)
                                        .and_then(|frags| {
                                            frags.iter().position(|frag| {
                                                frag.rect == d.fragment.rect
                                                    && frag.text == d.fragment.text
                                            })
                                        })
                                        .unwrap_or(0);
                                    inspector_target =
                                        Some(InspectorTarget::TextElement {
                                            page: d.fragment.page,
                                            id: TextBlockId(block_id),
                                            bounds: field,
                                        });
                                }
                                if d.fresh {
                                    resp.request_focus();
                                    d.fresh = false;
                                }
                                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                    edit_cancel = true;
                                } else if (single_line
                                    && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                                    || (resp.lost_focus() && !inspector_hovered)
                                {
                                    edit_commit = true;
                                }
                            }
                        }
                    }

                    // Image sélectionnée : cadre, poignées de rognage, et
                    // inspecteur posé au-dessus.
                    if let Some(image) = &selected_image {
                        if image.page == page as usize {
                            let [x1, y1, x2, y2] = image.rect;
                            let box_screen = Rect::from_two_pos(
                                pos2(
                                    page_rect.min.x + x1 * scale,
                                    page_rect.max.y - y2 * scale,
                                ),
                                pos2(
                                    page_rect.min.x + x2 * scale,
                                    page_rect.max.y - y1 * scale,
                                ),
                            );
                            ui.painter().rect_stroke(
                                box_screen,
                                Rounding::same(2.0),
                                Stroke::new(1.5_f32, pal().accent),
                            );
                            inspector_target = Some(InspectorTarget::ImageElement {
                                page: image.page,
                                id: ImageId(image.index),
                                bounds: box_screen,
                            });
                            if let Some(crop) = crop_draft {
                                // Zone conservée : les poignées vivent sur ses
                                // coins, le reste de l'image est assombri.
                                let kept = Rect::from_two_pos(
                                    pos2(
                                        box_screen.min.x + crop.left * box_screen.width(),
                                        box_screen.min.y + crop.top * box_screen.height(),
                                    ),
                                    pos2(
                                        box_screen.max.x - crop.right * box_screen.width(),
                                        box_screen.max.y - crop.bottom * box_screen.height(),
                                    ),
                                );
                                let shade = Color32::from_black_alpha(110);
                                for outside in [
                                    Rect::from_min_max(box_screen.min, pos2(box_screen.max.x, kept.min.y)),
                                    Rect::from_min_max(pos2(box_screen.min.x, kept.max.y), box_screen.max),
                                    Rect::from_min_max(pos2(box_screen.min.x, kept.min.y), pos2(kept.min.x, kept.max.y)),
                                    Rect::from_min_max(pos2(kept.max.x, kept.min.y), pos2(box_screen.max.x, kept.max.y)),
                                ] {
                                    ui.painter().rect_filled(outside, Rounding::ZERO, shade);
                                }
                                ui.painter().rect_stroke(
                                    kept,
                                    Rounding::ZERO,
                                    Stroke::new(1.0_f32, pal().accent),
                                );
                                let mut dragged = crop;
                                for (handle, corner) in [
                                    ("YouPDF crop tl", kept.left_top()),
                                    ("YouPDF crop tr", kept.right_top()),
                                    ("YouPDF crop bl", kept.left_bottom()),
                                    ("YouPDF crop br", kept.right_bottom()),
                                ] {
                                    let centre = ui.painter().round_pos_to_pixels(corner);
                                    let response = ui.interact(
                                        Rect::from_center_size(centre, vec2(12.0, 12.0)),
                                        Id::new(handle),
                                        Sense::drag(),
                                    );
                                    ui.painter().rect_filled(
                                        Rect::from_center_size(centre, vec2(8.0, 8.0)),
                                        Rounding::same(2.0),
                                        if response.hovered() || response.dragged() {
                                            pal().accent
                                        } else {
                                            pal().icon_bright
                                        },
                                    );
                                    // Seul un glissé déplace la poignée : un
                                    // simple survol ne doit pas écrire dans
                                    // le document.
                                    if !response.dragged() {
                                        continue;
                                    }
                                    let Some(pointer) = response.interact_pointer_pos() else {
                                        continue;
                                    };
                                    let fx = ((pointer.x - box_screen.min.x)
                                        / box_screen.width())
                                    .clamp(0.0, 1.0);
                                    let fy = ((pointer.y - box_screen.min.y)
                                        / box_screen.height())
                                    .clamp(0.0, 1.0);
                                    match handle {
                                        "YouPDF crop tl" => {
                                            dragged.left = fx;
                                            dragged.top = fy;
                                        }
                                        "YouPDF crop tr" => {
                                            dragged.right = 1.0 - fx;
                                            dragged.top = fy;
                                        }
                                        "YouPDF crop bl" => {
                                            dragged.left = fx;
                                            dragged.bottom = 1.0 - fy;
                                        }
                                        _ => {
                                            dragged.right = 1.0 - fx;
                                            dragged.bottom = 1.0 - fy;
                                        }
                                    }
                                }
                                // Les bords opposés ne doivent pas se croiser.
                                if dragged.left + dragged.right > 0.9 {
                                    let excess = dragged.left + dragged.right - 0.9;
                                    if dragged.left > dragged.right {
                                        dragged.left -= excess;
                                    } else {
                                        dragged.right -= excess;
                                    }
                                }
                                if dragged.top + dragged.bottom > 0.9 {
                                    let excess = dragged.top + dragged.bottom - 0.9;
                                    if dragged.top > dragged.bottom {
                                        dragged.top -= excess;
                                    } else {
                                        dragged.bottom -= excess;
                                    }
                                }
                                if response_changed(crop, dragged) {
                                    crop_next = Some(dragged);
                                }
                            }
                        }
                    }

                    // Repères de la page : le même calcul sert aux outils
                    // d'annotation et à la sélection de texte.
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

                    // Sélection de texte : c'est l'état de lecture, donc elle
                    // ne dépend pas de l'outil. Elle est active quand aucun
                    // outil n'est armé, et avec l'outil Sélection du mode
                    // Commentaire. Les outils qui dessinent gardent leur
                    // glissé.
                    if text_drag {
                        if resp.drag_started() {
                            if !tab.fragments.contains_key(&page) {
                                extract_page_fragments(renderer, tab, page);
                            }
                            if let Some(point) = resp.interact_pointer_pos() {
                                let point = to_pdf(point);
                                let hit = tab
                                    .fragments
                                    .get(&page)
                                    .and_then(|frags| text_hit(frags, point));
                                if let Some(hit) = hit {
                                    sel = Some(TextSel {
                                        page,
                                        span: Some((hit, hit)),
                                    });
                                }
                            }
                        } else if resp.dragged() {
                            if let Some(point) = resp.interact_pointer_pos() {
                                let point = to_pdf(point);
                                let hit = tab
                                    .fragments
                                    .get(&page)
                                    .and_then(|frags| text_hit(frags, point));
                                if let (Some(s), Some(hit)) = (sel.as_mut(), hit) {
                                    if s.page == page {
                                        if let Some((_, head)) = s.span.as_mut() {
                                            *head = hit;
                                        }
                                    }
                                }
                            }
                        } else if resp.drag_stopped() {
                            // Fin du glissé : avec une mise en évidence armée,
                            // la sélection devient une annotation posée sur les
                            // lignes touchées.
                            if let Some(kind) = markup_kind {
                                if let Some(frags) = tab.fragments.get(&page) {
                                    let quads =
                                        hit_quads(frags, sel.and_then(|s| s.span));
                                    if !quads.is_empty() {
                                        pending_markup =
                                            Some((page as usize, kind, quads));
                                    }
                                }
                                sel = None;
                            }
                        }
                        if resp.clicked() {
                            // Un double-clic prend le mot sous le pointeur, un
                            // triple-clic la ligne entière. Un clic simple
                            // efface la sélection.
                            let expand = if resp.triple_clicked() {
                                Some(3)
                            } else if resp.double_clicked() {
                                Some(2)
                            } else {
                                None
                            };
                            match expand {
                                Some(clicks) => {
                                    if !tab.fragments.contains_key(&page) {
                                        extract_page_fragments(renderer, tab, page);
                                    }
                                    let point = resp.interact_pointer_pos().map(to_pdf);
                                    let span = point
                                        .and_then(|point| {
                                            tab.fragments.get(&page).and_then(|frags| {
                                                text_hit(frags, point).and_then(|hit| {
                                                    if clicks == 2 {
                                                        expand_to_word(frags, hit)
                                                    } else {
                                                        expand_to_line(frags, hit)
                                                    }
                                                })
                                            })
                                        });
                                    sel = span.map(|(start, end)| TextSel {
                                        page,
                                        span: Some((start, end)),
                                    });
                                }
                                None => sel = None,
                            }
                        }
                        let on_page = sel.map(|s| s.page == page).unwrap_or(false);
                        // Un glissé explicite ouvre l'inspecteur ; « tout
                        // sélectionner » (sans ancre) couvrirait la page
                        // entière et n'apprendrait rien.
                        let explicit_sel = sel.map(|s| s.span.is_some()).unwrap_or(false);
                        if on_page && explicit_sel && markup_kind.is_none() {
                            // Inspecteur de la sélection de texte : enveloppe
                            // des blocs sélectionnés.
                            if let Some(frags) = tab.fragments.get(&page) {
                                let runs = hit_runs(frags, sel.and_then(|s| s.span));
                                let mut union: Option<Rect> = None;
                                for (index, _, _) in runs.iter().copied() {
                                    let frag = &frags[index];
                                    let rect = Rect::from_two_pos(
                                        to_screen([frag.rect[0], frag.rect[1]]),
                                        to_screen([frag.rect[2], frag.rect[3]]),
                                    );
                                    union = Some(match union {
                                        Some(previous) => previous.union(rect),
                                        None => rect,
                                    });
                                }
                                if let Some(bounds) = union {
                                    let range = (
                                        runs.first().map(|run| run.0).unwrap_or(0),
                                        runs.last().map(|run| run.0).unwrap_or(0),
                                    );
                                    inspector_target = Some(InspectorTarget::TextSelection {
                                        page: page as usize,
                                        range,
                                        bounds,
                                    });
                                }
                            }
                        }
                        if on_page {
                            resp.context_menu(|ui| {
                                if ui.button("Copy").clicked() {
                                    sel_copy = true;
                                    ui.close_menu();
                                }
                                if ui.button("Select all").clicked() {
                                    sel_all = true;
                                    ui.close_menu();
                                }
                            });
                        }
                    }

                    // Interaction des outils d'annotation.
                    if action != ToolAction::None {
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
                                        Stroke::new(1.5_f32, pal().accent)
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
                            // Une mise en évidence passe par la sélection de
                            // texte ; seul un rectangle dessiné arrive ici.
                            ToolAction::DragRect(_) if markup_kind.is_some() => {}
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
                                        Stroke::new(1.5_f32, pal().accent),
                                    );
                                }
                            }
                            ToolAction::Callout => {
                                // Mode callout : rien à faire ici, le traitement
                                // se fait via pending_callout dans le gestionnaire
                                // d'événements plus bas.
                            }
                            ToolAction::Ink => {
                                if resp.drag_started() {
                                    ink_current.clear();
                                }
                                if resp.dragged() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        let point = to_pdf(p);
                                        // Deux points plus proches qu'un point
                                        // PDF ne sont que du tremblement.
                                        let far_enough = ink_current.last().map_or(true, |last| {
                                            (point[0] - last[0]).hypot(point[1] - last[1]) >= 1.0
                                        });
                                        if far_enough {
                                            ink_current.push(point);
                                        }
                                    }
                                }
                                if resp.drag_stopped() {
                                    let smooth = smooth_polyline(ink_current, 4);
                                    ink_current.clear();
                                    pending_ink = Some((page as usize, smooth));
                                }
                                if ink_current.len() > 1 {
                                    let pts: Vec<egui::Pos2> =
                                        ink_current.iter().map(|p| to_screen(*p)).collect();
                                    ui.painter().add(Shape::line(
                                        pts,
                                        Stroke::new(annot_width * scale.max(0.4), pal().accent),
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
                                        Stroke::new(1.5_f32, pal().redact_red),
                                    );
                                }
                            }
                            ToolAction::Measure => {
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
                                        // Maj contraint le tracé à l'axe
                                        // dominant, comme une règle.
                                        let (a, b) = if ui.input(|i| i.modifiers.shift) {
                                            if (b[0] - a[0]).abs() >= (b[1] - a[1]).abs() {
                                                (a, [b[0], a[1]])
                                            } else {
                                                (a, [a[0], b[1]])
                                            }
                                        } else {
                                            (a, b)
                                        };
                                        pending_measure = Some((page as usize, a, b));
                                    }
                                }
                                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                    let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                    ui.painter().rect_stroke(
                                        rect,
                                        Rounding::ZERO,
                                        Stroke::new(1.5_f32, pal().accent),
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
                                        let rect = [
                                            a[0].min(b[0]),
                                            a[1].min(b[1]),
                                            a[0].max(b[0]),
                                            a[1].max(b[1]),
                                        ];
                                        let (rect, _) = snap_form_rect(
                                            rect,
                                            &tab.forms,
                                            page as usize,
                                            6.0 / scale,
                                        );
                                        pending_form = Some((page as usize, rect, kind));
                                    }
                                }
                                if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                    let raw = [
                                        a[0].min(b[0]),
                                        a[1].min(b[1]),
                                        a[0].max(b[0]),
                                        a[1].max(b[1]),
                                    ];
                                    let (snapped, guides) =
                                        snap_form_rect(raw, &tab.forms, page as usize, 6.0 / scale);
                                    let rect = Rect::from_two_pos(
                                        pos2(
                                            page_rect.min.x + snapped[0] * scale,
                                            page_rect.max.y - snapped[3] * scale,
                                        ),
                                        pos2(
                                            page_rect.min.x + snapped[2] * scale,
                                            page_rect.max.y - snapped[1] * scale,
                                        ),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        Rounding::same(2.0),
                                        Stroke::new(1.5_f32, pal().accent),
                                    );
                                    // Guides d'alignement rouges.
                                    for (vertical, at) in guides {
                                        let stroke =
                                            Stroke::new(1.0_f32, Color32::from_rgb(220, 60, 60));
                                        if vertical {
                                            let x = page_rect.min.x + at * scale;
                                            ui.painter().line_segment(
                                                [
                                                    pos2(x, page_rect.min.y),
                                                    pos2(x, page_rect.max.y),
                                                ],
                                                stroke,
                                            );
                                        } else {
                                            let y = page_rect.max.y - at * scale;
                                            ui.painter().line_segment(
                                                [
                                                    pos2(page_rect.min.x, y),
                                                    pos2(page_rect.max.x, y),
                                                ],
                                                stroke,
                                            );
                                        }
                                    }
                                }
                            }
                            ToolAction::Erase => {
                                if eraser_mode == EraseMode::Segment {
                                    if resp.drag_started() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            *drag_from = Some(to_pdf(p));
                                            *drag_to = Some(to_pdf(p));
                                        }
                                    }
                                    if resp.dragged() {
                                        if let Some(p) = resp.interact_pointer_pos() {
                                            *drag_to = Some(to_pdf(p));
                                        }
                                    }
                                    if resp.drag_stopped() {
                                        if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                            pending_erase_segment = Some((page as usize, a, b));
                                        }
                                        *drag_from = None;
                                        *drag_to = None;
                                    }
                                    // Aperçu du balayage de la gomme.
                                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                        ui.painter().line_segment(
                                            [to_screen(a), to_screen(b)],
                                            Stroke::new(6.0_f32, pal().accent.gamma_multiply(0.5)),
                                        );
                                    }
                                } else if resp.clicked() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        pending_hit = Some((page as usize, to_pdf(p)));
                                    }
                                }
                            }
                            ToolAction::Select => {
                                // Mode Préparer un formulaire : un glissé
                                // encadre plusieurs champs, un clic en vise un.
                                if mode == Some(2) {
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
                                            let rect = [
                                                a[0].min(b[0]),
                                                a[1].min(b[1]),
                                                a[0].max(b[0]),
                                                a[1].max(b[1]),
                                            ];
                                            pending_form_select = Some((page as usize, rect));
                                        }
                                    }
                                    if let (Some(a), Some(b)) = (*drag_from, *drag_to) {
                                        let rect = Rect::from_two_pos(to_screen(a), to_screen(b));
                                        ui.painter().rect_filled(
                                            rect,
                                            Rounding::same(2.0),
                                            Color32::from_rgba_unmultiplied(70, 130, 220, 30),
                                        );
                                        ui.painter().rect_stroke(
                                            rect,
                                            Rounding::same(2.0),
                                            Stroke::new(1.0_f32, Color32::from_rgb(70, 130, 220)),
                                        );
                                    }
                                }
                                if resp.clicked() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        pending_hit = Some((page as usize, to_pdf(p)));
                                    }
                                }
                            }
                            ToolAction::None => {
                                // Mode lecture : un clic peut viser un champ
                                // de formulaire.
                                if resp.clicked() {
                                    if let Some(p) = resp.interact_pointer_pos() {
                                        pending_hit = Some((page as usize, to_pdf(p)));
                                    }
                                }
                            }
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

                // La page demandée est à jour : la trame suivante s'occupe de
                // la suivante. Si son rendu a échoué, `needs_render` reste vrai
                // et on ne redemande rien — sinon on tournerait à vide.
                if let Some((page, step, target)) = pending_render {
                    if !tab.needs_render(page, step, target) {
                        ctx.request_repaint();
                    }
                }

                // ponytail: cache borné par nettoyage grossier, LRU plus tard.
                if tab.cache.len() > 40 {
                    let current = tab.page as i64;
                    tab.cache
                        .retain(|key, _| (key.0 as i64 - current).abs() < 10);
                }
            });

        // Le contenu a glissé : la première page n'est plus à la même place.
        // Cela couvre le défilement à la barre, que la molette ne signale pas.
        if let (Some(y), Some(last_y)) = (first_page_y, self.last_page_y) {
            if (y - last_y).abs() > 0.5 {
                self.scroll_moved_at = now;
            }
        }
        self.last_page_y = first_page_y;

        self.edit_text = draft;
        self.form_edit = form_draft;
        if form_cancel {
            self.form_edit = None;
            self.form_editing = None;
        } else if form_commit {
            self.commit_form_edit();
        }
        self.text_sel = sel;
        self.inspector_target = inspector_target;
        if let Some(crop) = crop_next {
            self.image_crop = Some(crop);
        }
        if edit_cancel {
            self.edit_text = None;
        } else if edit_commit {
            self.replace_fragment(&ctx);
        }

        if sel_all {
            self.select_page_text();
        } else if sel_copy {
            self.copy_selection(&ctx);
        }

        if let Some((page, kind, rect)) = pending_rect {
            self.add_rect_annot(page, kind, rect);
        }
        if let Some((page, kind, quads)) = pending_markup {
            self.add_markup_annot(page, kind, quads);
        }
        if let Some((page, points)) = pending_ink {
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
        if let Some((page, rect)) = pending_form_select {
            self.select_form_fields_in(page, rect);
        }
        if let Some((page, rect)) = pending_redact {
            if (rect[2] - rect[0]).abs() > 2.0 && (rect[3] - rect[1]).abs() > 2.0 {
                // L'apparence est figée au moment du marquage : changer le
                // remplissage ensuite ne touche que les zones nouvelles.
                let fill = self.redact_fill;
                let overlay = self.overlay_for(fill);
                self.redact_marks.push(RedactionMark {
                    page,
                    rect,
                    fill,
                    overlay,
                });
            }
        }
        if let Some((page, from, to)) = pending_measure {
            // Le glissé est en repère affiché ; la mesure s'écrit en repère page.
            let (from, to) = match self.tabs.get(self.active) {
                Some(tab) => (
                    point_to_page(&tab.doc, page, from),
                    point_to_page(&tab.doc, page, to),
                ),
                None => (from, to),
            };
            let low = [from[0].min(to[0]), from[1].min(to[1])];
            let high = [from[0].max(to[0]), from[1].max(to[1])];
            let points = match self.measure_kind {
                // Un glissé donne deux points : la distance va de l'un à
                // l'autre, le périmètre et l'aire prennent le rectangle.
                MeasureKind::Distance => vec![from, to],
                MeasureKind::Perimeter => vec![
                    low,
                    [high[0], low[1]],
                    high,
                    [low[0], high[1]],
                    low,
                ],
                MeasureKind::Area => vec![low, [high[0], low[1]], high, [low[0], high[1]]],
            };
            let drawing_len = ((from[0] - to[0]).powi(2) + (from[1] - to[1]).powi(2)).sqrt();
            self.measure_last_len = Some(drawing_len);
            let measurement = Measurement {
                kind: self.measure_kind,
                points,
                scale: self.measure_scale,
                unit: self.measure_unit.clone(),
            };
            let page = page as u32;
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_measurement(page, &measurement));
            match result {
                Some(Ok(())) => {
                    self.mark_document_changed();
                    self.toast(&ctx, "Measurement added");
                }
                Some(Err(err)) => self.toast(&ctx, format!("Measure failed: {err}")),
                None => {}
            }
        }
        if let Some((page, from, to)) = pending_erase_segment {
            self.erase_ink_segment(page, from, to, &ctx);
        }
        if let Some((page, p1, p2, p3)) = pending_callout {
            // Création de l'annotation Callout à partir des 3 points collectés.
            let mut annot = Annotation::new(page, AnnotationKind::Callout, [
                p1[0].min(p2[0]).min(p3[0]),
                p1[1].min(p2[1]).min(p3[1]),
                (p1[0].max(p2[0]).max(p3[0]) - p1[0].min(p2[0]).min(p3[0])).max(20.0),
                (p1[1].max(p2[1]).max(p3[1]) - p1[1].min(p2[1]).min(p3[1])).max(20.0),
            ]);
            annot.style = self.annot_style(AnnotationKind::Callout);
            annot.content = format!(
                "Callout: arrow from ({:.1}, {:.1}) to knee ({:.1}, {:.1}), text at ({:.1}, {:.1})",
                p1[0], p1[1], p2[0], p2[1], p3[0], p3[1]
            );
            annot.ink_points = vec![vec![p1, p2, p3]];
            annot.recompute_ink_bbox();
            self.push_annot(annot);
        }
        if let Some((page, point)) = pending_hit {
            // Vérifier d'abord si un champ de formulaire a été touché (mode Lecture).
            if matches!(self.mode, None | Some(0) | Some(2)) {
                // La liste des champs n'est chargée qu'à la demande : sans
                // cela, un clic sur un formulaire ne trouve rien tant que le
                // panneau n'a pas été ouvert.
                self.ensure_structure();
                if let Some(form_idx) = self.form_hit_test(page, point) {
                    // En mode Préparer, le clic sélectionne le champ (Maj :
                    // ajoute ou retire de la sélection).
                    if self.mode == Some(2) {
                        self.toggle_form_selection(form_idx);
                    } else {
                    let field = self.tabs[self.active].forms[form_idx].clone();
                    match &field.kind {
                        FormFieldKind::Text => {
                            self.form_editing = Some(form_idx);
                            self.form_edit =
                                Some((form_idx, field.value.clone().unwrap_or_default()));
                        }
                        FormFieldKind::Checkbox => {
                            let checked = field
                                .value
                                .as_deref()
                                .is_some_and(|value| value != "Off")
                                || field
                                    .state
                                    .as_deref()
                                    .is_some_and(|state| state != "Off");
                            let on_state = field
                                .states
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "Yes".to_string());
                            let new_value = if checked { "Off" } else { on_state.as_str() };
                            let filled = if let Some(tab) = self.tabs.get_mut(self.active) {
                                let filled = tab.doc.fill_form_field(&field, new_value).is_ok();
                                tab.forms = tab.doc.read_form_fields();
                                filled
                            } else {
                                false
                            };
                            if filled {
                                self.mark_document_changed();
                            }
                            self.form_editing = None;
                        }
                        FormFieldKind::Radio => {
                            let on_state = field
                                .states
                                .first()
                                .cloned()
                                .unwrap_or_else(|| "Yes".to_string());
                            let filled = if let Some(tab) = self.tabs.get_mut(self.active) {
                                let filled =
                                    tab.doc.fill_form_field(&field, &on_state).is_ok();
                                // Le groupe partage `/V` : relire tous les
                                // boutons remet leurs `/AS` à jour.
                                tab.forms = tab.doc.read_form_fields();
                                filled
                            } else {
                                false
                            };
                            if filled {
                                self.mark_document_changed();
                            }
                            self.form_editing = None;
                        }
                        _ => {
                            self.form_editing = Some(form_idx);
                            self.form_edit =
                                Some((form_idx, field.value.clone().unwrap_or_default()));
                        }
                    }
                    }
                }
            }
            match action {
                ToolAction::Select => {
                    if let Some(idx) = self.hit_test(page, point) {
                        self.selected_annot = idx;
                        self.has_annot_selection = true;
                        self.selected_image = None;
                    } else {
                        self.has_annot_selection = false;
                        // En mode Édition, le clic à vide cherche une image :
                        // c'est ainsi qu'on ouvre l'inspecteur d'image.
                        if self.mode == Some(1) {
                            self.select_image_at(page, point);
                        } else {
                            self.selected_image = None;
                        }
                    }
                }
                ToolAction::Callout => {
                    // Mode callout : 3 clics pour poser la flèche + texte.
                    if pending_callout.is_none() {
                        // Premier clic : mémoriser le point de départ.
                        pending_callout = Some((page, point, [0.0, 0.0], [0.0, 0.0]));
                    } else if pending_callout.as_ref().unwrap().1 == [0.0, 0.0] {
                        // Deuxième clic : mémoriser le genou.
                        let mut cp = pending_callout.take().unwrap();
                        cp.1 = point;
                        pending_callout = Some(cp);
                    } else if pending_callout.as_ref().unwrap().2 == [0.0, 0.0] {
                        // Troisième clic : mémoriser le coin du texte, créer l'annotation.
                        let mut cp = pending_callout.take().unwrap();
                        cp.2 = point;
                        // Création de l'annotation Callout.
                        let mut annot = Annotation::new(cp.0, AnnotationKind::Callout, [
                            cp.1[0].min(cp.2[0]).max(cp.1[0]).min(cp.2[0]),
                            cp.1[1].min(cp.2[1]).max(cp.1[1]).min(cp.2[1]),
                            (cp.1[0].max(cp.2[0]) - cp.1[0].min(cp.2[0])).max(20.0),
                            (cp.1[1].max(cp.2[1]) - cp.1[1].min(cp.2[1])).max(20.0),
                        ]);
                        annot.style = self.annot_style(AnnotationKind::Callout);
                        annot.content = format!(
                            "Callout: arrow from ({:.1}, {:.1}) to knee ({:.1}, {:.1}), text at ({:.1}, {:.1})",
                            cp.1[0], cp.1[1], cp.2[0], cp.2[1], cp.2[0], cp.2[1]
                        );
                        annot.ink_points = vec![vec![cp.1, cp.2, cp.2]];
                        annot.recompute_ink_bbox();
                        self.push_annot(annot);
                    }
                }
                _ => {}
            }
        }
    }

    /// Surface d'accueil : accès direct aux fichiers récents.
    fn empty_state(&mut self, ui: &mut egui::Ui) {
        self.home_screen(ui);
    }

    /// Écran d'accueil : recherche, onglets Récents / Favoris / Outils, et
    /// grille de vignettes ou liste. Remplace l'ancienne liste de récents.
    fn home_screen(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        let mut open_new: Option<PathBuf> = None;
        let mut reveal: Option<PathBuf> = None;
        let mut forget: Option<PathBuf> = None;
        let mut delete: Option<PathBuf> = None;
        let mut star: Option<PathBuf> = None;
        let mut open_dialog = false;
        let mut clear = false;
        let mut tool: Option<&'static str> = None;

        let now = ui.input(|i| i.time) as i64;

        // Liste affichée selon l'onglet.
        let paths: Vec<PathBuf> = match self.home.category {
            HomeCategory::Recents => self.recents.clone(),
            HomeCategory::Starred => self.stars.clone(),
            HomeCategory::Tools => Vec::new(),
        };
        let visible: Vec<PathBuf> = paths
            .into_iter()
            .filter(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.home.matches(&name)
            })
            .collect();

        let ctx = ui.ctx().clone();
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // En-tête.
                ui.add_space(34.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("YouPDF").size(30.0).strong());
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new("Free, local-first PDF editing")
                            .size(12.5)
                            .color(pal().muted),
                    );
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("or drop a file anywhere")
                            .size(11.0)
                            .color(pal().muted),
                    );
                });

                // Barre d'actions : recherche, catégories, vue.
                let full = ui.available_width();
                let margin = ((full - 720.0) * 0.5).max(16.0);
                ui.horizontal(|ui| {
                    ui.add_space(margin);
                    ui.add_sized(
                        vec2(240.0, 28.0),
                        TextEdit::singleline(&mut self.home.search_filter)
                            .hint_text("Search file…"),
                    );
                    if ui
                        .add_sized(
                            vec2(96.0, 28.0),
                            Button::new(RichText::new("Open File").size(12.5)).fill(pal().accent),
                        )
                        .clicked()
                    {
                        open_dialog = true;
                    }
                    if ui
                        .add_sized(vec2(76.0, 28.0), Button::new(RichText::new("Batch").size(12.5)))
                        .clicked()
                    {
                        tool = Some("batch");
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(margin);
                        let toggle = ui.add_sized(
                            vec2(30.0, 26.0),
                            Button::new(
                                RichText::new(match self.home.view_mode {
                                    ViewMode::Grid => "▦",
                                    ViewMode::List => "☰",
                                })
                                .size(13.0),
                            ),
                        );
                        let toggle = toggle.on_hover_text(self.home.view_mode.label());
                        if toggle.clicked() {
                            self.home.view_mode = self.home.view_mode.toggled();
                        }
                        ui.add_space(10.0);
                        for category in HomeCategory::ALL.into_iter().rev() {
                            let selected = self.home.category == category;
                            if ui
                                .selectable_label(selected, RichText::new(category.label()).size(12.5))
                                .clicked()
                            {
                                self.home.category = category;
                            }
                            ui.add_space(6.0);
                        }
                    });
                });
                ui.add_space(12.0);

                ui.horizontal(|ui| {
                    ui.add_space(margin);
                    ui.label(
                        RichText::new(match self.home.category {
                            HomeCategory::Recents => "Recent files",
                            HomeCategory::Starred => "Starred files",
                            HomeCategory::Tools => "Tools",
                        })
                        .size(13.0)
                        .strong(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(margin);
                        if self.home.category == HomeCategory::Recents
                            && !self.recents.is_empty()
                            && ui.small_button("Clear").clicked()
                        {
                            clear = true;
                        }
                    });
                });
                ui.add_space(10.0);

                if self.home.category == HomeCategory::Tools {
                    // Raccourcis d'outils.
                    self.home_tools(ui, margin, &mut tool);
                } else if visible.is_empty() {
                    ui.horizontal(|ui| {
                        ui.add_space(margin);
                        ui.label(
                            RichText::new(if self.home.category == HomeCategory::Starred {
                                "No starred files yet. Use the star on a card."
                            } else {
                                "No recent files. Open a PDF to get started."
                            })
                            .size(12.0)
                            .color(pal().muted),
                        );
                    });
                } else if self.home.view_mode == ViewMode::Grid {
                    self.home_grid(
                        ui, margin, &ctx, now, &visible, &mut open, &mut open_new, &mut reveal,
                        &mut forget, &mut delete, &mut star,
                    );
                } else {
                    self.home_list(
                        ui, margin, now, &visible, &mut open, &mut open_new, &mut reveal,
                        &mut forget, &mut delete, &mut star,
                    );
                }
                ui.add_space(30.0);
            });

        if open_dialog {
            self.open_dialog();
        }
        if clear {
            self.recents.clear();
            save_recents(&self.recents);
        }
        if let Some(path) = star {
            home::toggle_star(&mut self.stars, &path);
            home::save_stars(&self.stars);
        }
        if let Some(path) = forget {
            self.recents.retain(|old| old != &path);
            self.home_thumbs.remove(&path);
            save_recents(&self.recents);
        }
        if let Some(path) = reveal {
            self.reveal_path(&path);
        }
        if let Some(path) = delete {
            self.recents.retain(|old| old != &path);
            self.stars.retain(|old| old != &path);
            self.home_thumbs.remove(&path);
            save_recents(&self.recents);
            home::save_stars(&self.stars);
            if std::fs::remove_file(&path).is_ok() {
                self.toast(&ctx, "File deleted");
            } else {
                self.toast(&ctx, "Could not delete the file");
            }
        }
        if let Some(path) = open_new {
            if path.is_file() {
                self.open_path_new_tab(path);
            } else {
                self.toast(&ctx, "File not found");
            }
            self.home_open = false;
        } else if let Some(path) = open {
            if path.is_file() {
                self.open_path(path);
            } else {
                self.toast(&ctx, "File not found");
            }
            self.home_open = false;
        }
        if let Some(id) = tool {
            self.run_home_tool(id);
        }
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
        const CARD_W: f32 = 160.0;
        const GAP: f32 = 12.0;
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

    /// Une carte de l'écran d'accueil : vignette, nom, taille, ancienneté, étoile.
    #[allow(clippy::too_many_arguments)]
    fn home_card(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        now: i64,
        path: &std::path::Path,
        open: &mut Option<PathBuf>,
        open_new: &mut Option<PathBuf>,
        reveal: &mut Option<PathBuf>,
        forget: &mut Option<PathBuf>,
        delete: &mut Option<PathBuf>,
        star: &mut Option<PathBuf>,
    ) {
        const CARD_W: f32 = 160.0;
        const CARD_H: f32 = 200.0;
        let (rect, resp) = ui.allocate_exact_size(vec2(CARD_W, CARD_H), Sense::click());
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, Rounding::same(8.0), pal().widget_bg);
        }

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        let exists = path.is_file();
        let starred = home::is_star(&self.stars, path);

        // Vignette de la première page, ou glyphe de repli.
        let thumb_rect = Rect::from_min_size(rect.min, vec2(CARD_W, 130.0));
        let texture = if exists {
            self.home_thumbnail(ctx, path)
        } else {
            None
        };
        match texture {
            Some(texture) => {
                ui.painter().image(
                    texture.id(),
                    thumb_rect.shrink(6.0),
                    Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                let icon = Rect::from_center_size(thumb_rect.center(), vec2(46.0, 58.0));
                icon_page(ui.painter(), icon, pal().muted);
            }
        }

        // Bas de carte : nom, taille, ancienneté.
        let text_x = rect.min.x + 10.0;
        ui.painter().text(
            pos2(text_x, rect.min.y + 146.0),
            Align2::LEFT_CENTER,
            shorten(&name, 20),
            FontId::proportional(12.0),
            if exists { pal().icon_bright } else { pal().muted },
        );
        ui.painter().text(
            pos2(text_x, rect.min.y + 165.0),
            Align2::LEFT_CENTER,
            if exists {
                let size = std::fs::metadata(path)
                    .map(|m| format_size(m.len() as usize))
                    .unwrap_or_else(|_| "—".to_owned());
                let age = std::fs::metadata(path)
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| home::relative_time(d.as_secs() as i64, now))
                    .unwrap_or_else(|| "—".to_owned());
                format!("{size} · {age}")
            } else {
                "File not found".to_owned()
            },
            FontId::proportional(10.0),
            pal().muted,
        );

        // Étoile.
        let star_rect = Rect::from_center_size(
            pos2(rect.max.x - 16.0, rect.min.y + 148.0),
            vec2(20.0, 20.0),
        );
        let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
        ui.painter().text(
            star_rect.center(),
            Align2::CENTER_CENTER,
            if starred { "★" } else { "☆" },
            FontId::proportional(13.0),
            if starred { pal().accent } else { pal().muted },
        );
        if star_resp.clicked() {
            *star = Some(path.to_path_buf());
        }

        if star_resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if !star_resp.clicked() && resp.clicked() {
            *open = Some(path.to_path_buf());
        }

        resp.context_menu(|ui| {
            if ui.button("Open").clicked() {
                *open = Some(path.to_path_buf());
                ui.close_menu();
            }
            if ui.button("Open in New Tab").clicked() {
                *open_new = Some(path.to_path_buf());
                ui.close_menu();
            }
            if ui
                .button(if starred { "Remove Star" } else { "Star" })
                .clicked()
            {
                *star = Some(path.to_path_buf());
                ui.close_menu();
            }
            if ui.button("Reveal in Finder").clicked() {
                *reveal = Some(path.to_path_buf());
                ui.close_menu();
            }
            if ui.button("Remove from Recents").clicked() {
                *forget = Some(path.to_path_buf());
                ui.close_menu();
            }
            if ui.button("Delete File…").clicked() {
                *delete = Some(path.to_path_buf());
                ui.close_menu();
            }
        });
    }

    /// Liste compacte de l'écran d'accueil.
    #[allow(clippy::too_many_arguments)]
    fn home_list(
        &mut self,
        ui: &mut egui::Ui,
        margin: f32,
        now: i64,
        visible: &[PathBuf],
        open: &mut Option<PathBuf>,
        open_new: &mut Option<PathBuf>,
        reveal: &mut Option<PathBuf>,
        forget: &mut Option<PathBuf>,
        delete: &mut Option<PathBuf>,
        star: &mut Option<PathBuf>,
    ) {
        for path in visible {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.to_string_lossy().into_owned());
            let parent = path
                .parent()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            let exists = path.is_file();
            let starred = home::is_star(&self.stars, path);

            ui.horizontal(|ui| {
                ui.add_space(margin);
                let width = (ui.available_width() - margin).max(200.0);
                let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), Sense::click());
                if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect, Rounding::same(8.0), pal().widget_bg);
                }
                ui.painter().text(
                    pos2(rect.min.x + 14.0, rect.min.y + 15.0),
                    Align2::LEFT_CENTER,
                    shorten(&name, 46),
                    FontId::proportional(12.5),
                    if exists { pal().icon_bright } else { pal().muted },
                );
                let detail = if exists {
                    let size = std::fs::metadata(path)
                        .map(|m| format_size(m.len() as usize))
                        .unwrap_or_else(|_| "—".to_owned());
                    let age = std::fs::metadata(path)
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| home::relative_time(d.as_secs() as i64, now))
                        .unwrap_or_else(|| "—".to_owned());
                    format!("{} · {size} · {age}", shorten(&parent, 60))
                } else {
                    "File not found".to_owned()
                };
                ui.painter().text(
                    pos2(rect.min.x + 14.0, rect.min.y + 32.0),
                    Align2::LEFT_CENTER,
                    detail,
                    FontId::proportional(10.5),
                    pal().muted,
                );

                let star_rect = Rect::from_center_size(
                    pos2(rect.max.x - 46.0, rect.center().y),
                    vec2(20.0, 20.0),
                );
                let star_resp = ui.interact(star_rect, resp.id.with("star"), Sense::click());
                ui.painter().text(
                    star_rect.center(),
                    Align2::CENTER_CENTER,
                    if starred { "★" } else { "☆" },
                    FontId::proportional(13.0),
                    if starred { pal().accent } else { pal().muted },
                );
                if star_resp.clicked() {
                    *star = Some(path.clone());
                }

                let forget_rect = Rect::from_center_size(
                    pos2(rect.max.x - 20.0, rect.center().y),
                    vec2(20.0, 20.0),
                );
                let forget_resp = ui.interact(forget_rect, resp.id.with("forget"), Sense::click());
                ui.painter().text(
                    forget_rect.center(),
                    Align2::CENTER_CENTER,
                    if forget_resp.hovered() { ph::X } else { "" },
                    FontId::proportional(12.0),
                    pal().icon,
                );
                if forget_resp.clicked() {
                    *forget = Some(path.clone());
                } else if resp.clicked() {
                    *open = Some(path.clone());
                }

                resp.context_menu(|ui| {
                    if ui.button("Open").clicked() {
                        *open = Some(path.clone());
                        ui.close_menu();
                    }
                    if ui.button("Open in New Tab").clicked() {
                        *open_new = Some(path.clone());
                        ui.close_menu();
                    }
                    if ui.button("Reveal in Finder").clicked() {
                        *reveal = Some(path.clone());
                        ui.close_menu();
                    }
                    if ui.button("Remove from Recents").clicked() {
                        *forget = Some(path.clone());
                        ui.close_menu();
                    }
                    if ui.button("Delete File…").clicked() {
                        *delete = Some(path.clone());
                        ui.close_menu();
                    }
                });
            });
            ui.add_space(4.0);
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
        const CELL_W: f32 = 160.0;
        const CELL_H: f32 = 64.0;
        const GAP: f32 = 12.0;
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
                            .rect_filled(rect, Rounding::same(8.0), pal().widget_bg);
                    }
                    ui.painter().text(
                        rect.center(),
                        Align2::CENTER_CENTER,
                        *label,
                        FontId::proportional(13.0),
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
        let handle = ctx.load_texture(
            format!("home-thumb-{}", path.display()),
            bitmap.to_egui_image(),
            TextureOptions::LINEAR,
        );
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
                            .color(pal().muted),
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
                            ui.label(RichText::new(text).size(11.5));
                            ui.label(RichText::new(when).size(10.0).color(pal().muted));
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
    /// Barre d'état inférieure : navigation de page, disposition, zoom,
    /// diaporama, dimensions de page et état de protection.
    ///
    /// Remplace l'ancienne barre de zoom flottante : un seul endroit porte les
    /// réglages de vue, comme dans UPDF.
    fn status_bar(&mut self, ctx: &egui::Context) {
        if self.home_open || self.tabs.is_empty() {
            return;
        }
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let page = tab.page;
        let total = tab.total();
        let zoom_fit_page = tab.zoom_fit_page;
        let current_zoom = tab.zoom;
        let zoom_label = current_zoom
            .map(|z| format!("{:.0}%", z * 100.0))
            .unwrap_or_else(|| {
                if zoom_fit_page {
                    "Fit page".to_owned()
                } else {
                    "Fit width".to_owned()
                }
            });
        let layout = self.page_layout;
        let protected = tab.protected;
        let engine = self.renderer_label;
        let view = tab.doc.page_view(page as u32);
        let (view_w, view_h) = view.display_size();
        let size_label = page_size_label(view_w, view_h);

        let mut goto: Option<usize> = None;
        let mut delta = 0.0_f32;
        let mut layout_choice: Option<PageLayout> = None;
        let mut zoom_choice: Option<ZoomMode> = None;
        let mut slideshow = false;

        TopBottomPanel::bottom("statusbar")
            .exact_height(STATUS_H)
            .frame(Frame::none().fill(pal().chrome_bg))
            .show(ctx, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.add_space(6.0);

                    // Navigation de page.
                    if status_button(ui, ph::CARET_DOUBLE_LEFT, "First page") {
                        goto = Some(1);
                    }
                    if status_button(ui, ph::CARET_LEFT, "Previous page") {
                        if page > 1 {
                            goto = Some(page - 1);
                        }
                    }
                    // Le champ reprend la page courante tant qu'il n'a pas le
                    // focus ; sinon une saisie en cours serait écrasée.
                    let page_field = Id::new("status-page-entry");
                    if !ui.memory(|m| m.has_focus(page_field)) {
                        self.page_entry = page.to_string();
                    }
                    let field = ui.add_sized(
                        vec2(34.0, 18.0),
                        TextEdit::singleline(&mut self.page_entry)
                            .id(page_field)
                            .hint_text("1"),
                    );
                    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        goto = self.page_entry.trim().parse::<usize>().ok();
                    }
                    ui.label(
                        RichText::new(format!("of {total}"))
                            .size(11.0)
                            .color(pal().muted),
                    );
                    if status_button(ui, ph::CARET_RIGHT, "Next page") {
                        if page < total {
                            goto = Some(page + 1);
                        }
                    }
                    if status_button(ui, ph::CARET_DOUBLE_RIGHT, "Last page") {
                        goto = Some(total);
                    }

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Disposition des pages.
                    let layout_resp = ui
                        .add(
                            Button::new(
                                RichText::new(ph::SQUARES_FOUR).size(13.0).color(pal().icon),
                            )
                            .frame(false),
                        )
                        .on_hover_text(format!("Layout: {}", layout.label()));
                    if layout_resp.clicked() {
                        ui.memory_mut(|m| m.toggle_popup(Id::new("layout_menu")));
                    }
                    egui::popup_above_or_below_widget(
                        ui,
                        Id::new("layout_menu"),
                        &layout_resp,
                        egui::AboveOrBelow::Above,
                        egui::PopupCloseBehavior::CloseOnClick,
                        |ui| {
                            ui.set_min_width(170.0);
                            for choice in PageLayout::ALL {
                                let mark = if choice == layout { ph::CHECK } else { "" };
                                if menu_row(ui, choice.label(), mark, None) {
                                    layout_choice = Some(choice);
                                }
                            }
                        },
                    );

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Zoom.
                    if status_button(ui, ph::MINUS, "Zoom out") {
                        delta = -1.0;
                    }
                    let text = if zoom_fit_page {
                        format!("{zoom_label} · page")
                    } else {
                        zoom_label.clone()
                    };
                    let zoom_resp = ui
                        .add(
                            Button::new(RichText::new(&text).size(11.0).color(pal().muted))
                                .frame(false),
                        )
                        .on_hover_text("Zoom presets");
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
                                let selected =
                                    zoom_choice_matches(*mode, current_zoom, zoom_fit_page);
                                if menu_row(ui, label, if selected { ph::CHECK } else { "" }, None) {
                                    zoom_choice = Some(*mode);
                                }
                            }
                        },
                    );
                    if status_button(ui, ph::PLUS, "Zoom in") {
                        delta = 1.0;
                    }

                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);

                    if status_button(ui, ph::PLAY, "Slideshow (F5)") {
                        slideshow = true;
                    }

                    // Graphe de droite : dimensions et protection.
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(6.0);
                        let (lock_glyph, lock_tip, lock_color) = if protected {
                            (ph::LOCK_SIMPLE, "Protected document", pal().accent)
                        } else {
                            (ph::LOCK_OPEN, "No restrictions", pal().muted)
                        };
                        ui.add(
                            Button::new(RichText::new(lock_glyph).size(12.0).color(lock_color))
                                .frame(false),
                        )
                        .on_hover_text(lock_tip);
                        ui.add_space(4.0);
                        ui.label(RichText::new(size_label).size(11.0).color(pal().muted));
                        ui.add_space(6.0);
                        ui.label(RichText::new(engine).size(10.0).color(pal().muted));
                    });
                });
            });

        if let Some(target) = goto {
            if (1..=total).contains(&target) {
                self.jump = Some(target);
            } else {
                self.toast(ctx, format!("Page out of range (1–{total})"));
            }
        }
        if let Some(choice) = layout_choice {
            self.set_page_layout(ctx, choice);
        }
        if let Some(choice) = zoom_choice {
            self.set_zoom_mode(choice);
        }
        if delta < 0.0 {
            self.zoom_by(1.0 / 1.1);
        } else if delta > 0.0 {
            self.zoom_by(1.1);
        }
        if slideshow {
            self.enter_slideshow(ctx);
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
                            egui::Label::new(RichText::new(&text).size(11.5).color(pal().icon_bright))
                                .extend(),
                        );
                    });
            });
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Comma)) {
            self.prefs_open = true;
            return;
        }
        // Une saisie en cours (recherche, nom de champ, remplacement) a la
        // priorité : sinon Retour arrière efface l'annotation sélectionnée au
        // lieu d'un caractère.
        let typing = ctx.wants_keyboard_input();
        // ⌘Z / ⇧⌘Z : l'historique de l'onglet actif. Un champ de saisie au
        // premier plan garde son propre annuler/refaire.
        if !typing && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Z)) {
            if ctx.input(|i| i.modifiers.shift) {
                self.redo(ctx);
            } else {
                self.undo(ctx);
            }
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) && !typing {
            self.enter_slideshow(ctx);
            return;
        }
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
        // Copie et « tout sélectionner » portent sur le texte de la page : un
        // champ de saisie au premier plan garde la priorité (garde `typing`).
        let (copy_text, select_all) = ctx.input(|i| {
            (
                i.key_pressed(egui::Key::C) && i.modifiers.command && !i.modifiers.shift,
                i.key_pressed(egui::Key::A) && i.modifiers.command && !i.modifiers.shift,
            )
        });
        if !typing && copy_text {
            self.copy_selection(ctx);
            return;
        }
        if !typing && select_all {
            self.select_page_text();
            return;
        }
        if cmd && find {
            self.search_step(if shift { -1 } else { 1 });
            return;
        }
        // Raccourcis Ctrl+1..0 du menu « Outils ». Ils sont lus avant le
        // garde `cmd` ci-dessous : sinon toutes les combinaisons Ctrl seules
        // seraient ignorées.
        let ctrl_tool = ctx.input(|i| {
            if !i.modifiers.ctrl || i.modifiers.command {
                return None;
            }
            const DIGITS: [egui::Key; 10] = [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
                egui::Key::Num6,
                egui::Key::Num7,
                egui::Key::Num8,
                egui::Key::Num9,
                egui::Key::Num0,
            ];
            DIGITS.iter().position(|key| i.key_pressed(*key))
        });
        if let Some(index) = ctrl_tool {
            match index {
                0..=3 => {
                    self.mode = Some(index);
                    self.annot_tool = None;
                    self.preset_open = false;
                    self.text_sel = None;
                }
                4 => self.organize_open = true,
                5 => self.crop_open = true,
                6 => self.tile_split_open = true,
                7 => self.watermark_open = true,
                8 => self.background_open = true,
                _ => self.header_footer_open = true,
            }
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

/// Polices de l'application : Phosphor (icônes) puis Hack en repli.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    // Ubuntu-Light n'a pas ⇧ (U+21E7) : Hack le fournit en repli proportionnel.
    if let Some(keys) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        keys.push("Hack".to_owned());
    }
    ctx.set_fonts(fonts);
}

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
        FontId::proportional(12.5),
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
        ui.painter().rect_filled(rect, Rounding::same(6.0), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(6.0), pal().hover_bg);
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
        ui.painter().rect_filled(rect, Rounding::same(6.0), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(6.0), pal().hover_bg);
    }
    let icon_rect = Rect::from_center_size(
        pos2(rect.min.x + 13.0, rect.center().y),
        vec2(15.0, 15.0),
    );
    draw(
        ui.painter(),
        icon_rect,
        if primary || selected {
            Color32::WHITE
        } else {
            pal().icon
        },
    );
    if chevron {
        let ch_rect = Rect::from_center_size(
            pos2(rect.max.x - 11.0, rect.center().y),
            vec2(8.0, 8.0),
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
        FontId::proportional(12.5),
        pal().icon_bright,
    );
    resp.clicked()
}

/// Séparateur de menu, aligné sur la largeur des lignes.
fn menu_separator(ui: &mut egui::Ui) {
    ui.add_space(4.0);
    let y = ui.cursor().top();
    ui.painter().hline(
        ui.max_rect().x_range(),
        y,
        Stroke::new(1.0_f32, pal().separator),
    );
    ui.add_space(6.0);
}

/// Entrée qui ouvre un sous-menu au clic.
fn menu_submenu(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    let text = RichText::new(format!("{label}  ▸")).size(12.5);
    ui.menu_button(text, |ui| {
        ui.set_min_width(180.0);
        add(ui);
    });
}

/// Ligne d'un sous-menu.
fn submenu_row(ui: &mut egui::Ui, label: &str, icon: Option<fn(&egui::Painter, egui::Rect, egui::Color32)>) -> bool {
    let desired_width = if icon.is_some() { 200.0 } else { 180.0 };
    let button = Button::new(RichText::new(label).size(12.0))
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
        FontId::proportional(12.5),
        pal().icon_bright,
    );
    ui.painter().text(
        pos2(shortcut_x, rect.center().y),
        Align2::RIGHT_CENTER,
        shortcut,
        FontId::proportional(11.0),
        pal().muted,
    );
    resp.clicked()
}

/// Échappe une cellule CSV selon RFC 4180 : guillemets si la valeur contient
/// une virgule, un guillemet ou un saut de ligne, guillemets internes doublés.
fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

/// Un lot à traiter : les fichiers, le dossier de sortie et les opérations
/// cochées.


/// Recadre les annotations du repère affiché (celui du canevas) vers le repère
/// page, rotation et boîte visible comprises.
///
/// L'interface dessine et sélectionne en repère affiché ; le PDF s'écrit en
/// repère page. Sans cette conversion, une annotation posée sur une page
/// tournée ou recadrée atterrit au mauvais endroit dans le fichier.
fn annots_to_page(doc: &Document, annots: &[Annotation]) -> Vec<Annotation> {
    let mut out = annots.to_vec();
    for ann in &mut out {
        let view = doc.page_view(ann.page as u32);
        ann.rect = view.page_rect(ann.rect);
        for stroke in &mut ann.ink_points {
            for point in stroke.iter_mut() {
                *point = view.page_point(*point);
            }
        }
        for quad in &mut ann.quads {
            for point in quad.chunks_exact_mut(2) {
                let converted = view.page_point([point[0], point[1]]);
                point[0] = converted[0];
                point[1] = converted[1];
            }
        }
    }
    out
}

/// L'inverse : les annotations lues dans le PDF reviennent en repère affiché.
fn annots_to_display(doc: &Document, mut annots: Vec<Annotation>) -> Vec<Annotation> {
    for ann in &mut annots {
        let view = doc.page_view(ann.page as u32);
        ann.rect = view.display_rect(ann.rect);
        for stroke in &mut ann.ink_points {
            for point in stroke.iter_mut() {
                *point = view.display_point(*point);
            }
        }
        for quad in &mut ann.quads {
            for point in quad.chunks_exact_mut(2) {
                let converted = view.display_point([point[0], point[1]]);
                point[0] = converted[0];
                point[1] = converted[1];
            }
        }
    }
    annots
}

/// Recadre un rectangle du repère affiché vers le repère page.
fn rect_to_page(doc: &Document, page: usize, rect: [f32; 4]) -> [f32; 4] {
    doc.page_view(page as u32).page_rect(rect)
}

/// Recadre un point du repère affiché vers le repère page.
fn point_to_page(doc: &Document, page: usize, point: [f32; 2]) -> [f32; 2] {
    doc.page_view(page as u32).page_point(point)
}

/// Recadre un rectangle du repère page vers le repère affiché.
fn rect_to_display(doc: &Document, page: usize, rect: [f32; 4]) -> [f32; 4] {
    doc.page_view(page as u32).display_rect(rect)
}

/// Bouton-icône 38×38 avec fond sélection/hover.
fn rail_button(
    ui: &mut egui::Ui,
    selected: bool,
    draw: IconFn,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(38.0, 38.0), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, Rounding::same(8.0), pal().accent);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(8.0), pal().widget_bg);
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
        ui.painter().rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
    }
    let color = if resp.hovered() { pal().icon_bright } else { pal().icon };
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
        ui.painter().rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(rect, Rounding::same(6.0), Color32::from_rgb(0x33, 0x33, 0x33));
    }
    let color = if selected || resp.hovered() { pal().icon_bright } else { pal().icon };
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

/// Petit bouton d'icône de la barre d'état.
fn status_button(ui: &mut egui::Ui, glyph: &str, tip: &str) -> bool {
    let resp = ui
        .add(Button::new(RichText::new(glyph).size(13.0).color(pal().icon)).frame(false))
        .on_hover_text(tip);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.clicked()
}

/// Libellé lisible d'un format de page : nom usuel si connu, sinon la taille.
fn page_size_label(width: f32, height: f32) -> String {
    let near = |a: f32, b: f32| (a - b).abs() < 3.0;
    if near(width, 595.0) && near(height, 842.0) {
        "A4 portrait".to_owned()
    } else if near(width, 842.0) && near(height, 595.0) {
        "A4 landscape".to_owned()
    } else if near(width, 612.0) && near(height, 792.0) {
        "Letter portrait".to_owned()
    } else if near(width, 792.0) && near(height, 612.0) {
        "Letter landscape".to_owned()
    } else {
        format!("{width:.0} × {height:.0} pt")
    }
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

/// Clé de chemin d'un signet : indices de frères, séparés par `/`.
fn bookmark_key(path: &[usize]) -> String {
    path.iter()
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Chemin d'indices décrit par une clé de chemin.
fn bookmark_path(key: &str) -> Vec<usize> {
    key.split('/')
        .filter_map(|part| part.parse::<usize>().ok())
        .collect()
}

/// Position d'insertion d'un signet déplacé, dans la liste d'accueil.
///
/// `index` compte dans la liste d'*avant* retrait, `dest_len` en est la
/// longueur avant retrait. Retirer un frère situé avant la cible décale
/// celle-ci de un : sans cet ajustement, un déplacement vers le bas atterrit
/// un cran trop loin.
fn bookmark_insert_index(
    index: Option<usize>,
    dest_len: usize,
    same_list: bool,
    from_index: usize,
) -> usize {
    let mut at = index.unwrap_or(dest_len);
    if same_list && from_index < at {
        at -= 1;
    }
    at
}

/// Décale un chemin quand un frère qui le précède vient d'être retiré.
///
/// Seul le niveau du nœud retiré compte, et seulement pour ses propres frères :
/// la cible doit partager la liste parente du nœud retiré, sinon rien ne bouge.
/// Un `removed` préfixe de `path` (nœud visé dans le sous-arbre retiré) est
/// écarté par l'appelant avant d'arriver ici.
fn bookmark_shift_path(path: &[usize], removed: &[usize]) -> Vec<usize> {
    let mut out = path.to_vec();
    let depth = removed.len();
    // Trop court pour être un frère : c'est un ancêtre, jamais décalé.
    if depth == 0 || path.len() < depth {
        return out;
    }
    // Une autre liste parente : le retrait ne la concerne pas.
    if path[..depth - 1] != removed[..depth - 1] {
        return out;
    }
    if path[depth - 1] > removed[depth - 1] {
        out[depth - 1] -= 1;
    }
    out
}

/// Entrée visée par un chemin de frères.
fn bookmark_at<'a>(tree: &'a [OutlineEntry], path: &[usize]) -> Option<&'a OutlineEntry> {
    let (index, rest) = path.split_first()?;
    let entry = tree.get(*index)?;
    if rest.is_empty() {
        Some(entry)
    } else {
        bookmark_at(&entry.children, rest)
    }
}

/// Entrée visée par un chemin de frères, en écriture.
fn bookmark_at_mut<'a>(
    tree: &'a mut [OutlineEntry],
    path: &[usize],
) -> Option<&'a mut OutlineEntry> {
    let (index, rest) = path.split_first()?;
    let entry = tree.get_mut(*index)?;
    if rest.is_empty() {
        Some(entry)
    } else {
        bookmark_at_mut(&mut entry.children, rest)
    }
}

/// Retire l'entrée visée de l'arbre et la renvoie.
fn bookmark_take(tree: &mut Vec<OutlineEntry>, path: &[usize]) -> Option<OutlineEntry> {
    let (index, rest) = path.split_first()?;
    if rest.is_empty() {
        if *index < tree.len() {
            return Some(tree.remove(*index));
        }
        return None;
    }
    let entry = tree.get_mut(*index)?;
    bookmark_take(&mut entry.children, rest)
}

/// Vrai si `path` est `other` ou l'un de ses ancêtres.
fn bookmark_is_ancestor(path: &[usize], other: &[usize]) -> bool {
    other.len() >= path.len() && other[..path.len()] == *path
}

/// Une ligne du panneau Signets : `(clé, titre, page, profondeur, a des
/// enfants, replié)`.
type BookmarkRow = (String, String, Option<usize>, usize, bool, bool);

/// Lignes visibles d'un arbre de signets.
fn bookmark_rows(tree: &[OutlineEntry], collapsed: &BTreeSet<String>) -> Vec<BookmarkRow> {
    fn walk(
        entries: &[OutlineEntry],
        prefix: &mut Vec<usize>,
        collapsed: &BTreeSet<String>,
        rows: &mut Vec<BookmarkRow>,
    ) {
        for (index, entry) in entries.iter().enumerate() {
            prefix.push(index);
            let key = bookmark_key(prefix);
            let folded = collapsed.contains(&key);
            rows.push((
                key,
                entry.title.clone(),
                entry.page,
                prefix.len() - 1,
                !entry.children.is_empty(),
                folded,
            ));
            if !folded {
                walk(&entry.children, prefix, collapsed, rows);
            }
            prefix.pop();
        }
    }
    let mut rows = Vec::new();
    walk(tree, &mut Vec::new(), collapsed, &mut rows);
    rows
}

/// Toutes les clés de chemin d'un arbre (pour « Collapse all »).
fn bookmark_all_keys(tree: &[OutlineEntry]) -> Vec<String> {
    fn walk(entries: &[OutlineEntry], prefix: &mut Vec<usize>, out: &mut Vec<String>) {
        for (index, entry) in entries.iter().enumerate() {
            prefix.push(index);
            out.push(bookmark_key(prefix));
            walk(&entry.children, prefix, out);
            prefix.pop();
        }
    }
    let mut out = Vec::new();
    walk(tree, &mut Vec::new(), &mut out);
    out
}

/// Bouton d'action de la fenêtre d'organisation : icône puis libellé.
fn organize_button(ui: &mut egui::Ui, icon: IconFn, label: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(116.0, 28.0), Sense::click());
    ui.painter().rect_filled(
        rect,
        Rounding::same(6.0),
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

/// Cherche `needle` dans `hay`, selon la casse demandée.
///
/// `needle` est déjà plié si la recherche est insensible à la casse ;
/// `fold_case` préservant la longueur en octets, l'index trouvé reste valable
/// dans la chaîne d'origine.
fn find_query(hay: &str, needle: &str, case_sensitive: bool) -> Option<usize> {
    if case_sensitive {
        hay.find(needle)
    } else {
        fold_case(hay).find(needle)
    }
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
        ui.label(RichText::new(title).size(12.0).color(pal().muted));
        ui.add_space(4.0);
        ui.label(RichText::new(detail).size(11.0).color(pal().muted));
    });
}

/// Aligne un rectangle de champ sur les bords et centres des champs voisins.
///
/// Renvoie le rectangle ajusté et les guides à dessiner : `(vertical, position)`,
/// en repère page. Un écart inférieur à `tol` (en points) déclenche l'aimant.
fn snap_form_rect(
    rect: [f32; 4],
    fields: &[FormField],
    page: usize,
    tol: f32,
) -> ([f32; 4], Vec<(bool, f32)>) {
    let mut out = rect;
    let mut guides = Vec::new();
    let neighbors: Vec<[f32; 4]> = fields
        .iter()
        .filter(|field| field.page == Some(page + 1) && field.rect != [0.0; 4])
        .map(|field| field.rect)
        .collect();
    if neighbors.is_empty() || tol <= 0.0 {
        return (out, guides);
    }
    // Candidats verticaux : gauche, centre, droite.
    let mut best_x: Option<(f32, f32)> = None;
    for neighbor in &neighbors {
        let nx = [
            neighbor[0].min(neighbor[2]),
            (neighbor[0] + neighbor[2]) / 2.0,
            neighbor[0].max(neighbor[2]),
        ];
        let moving = [(out[0], 0usize), ((out[0] + out[2]) / 2.0, 1), (out[2], 2)];
        for (value, _) in moving {
            for target in nx {
                let delta = target - value;
                if delta.abs() <= tol && best_x.map_or(true, |(d, _)| delta.abs() < d.abs()) {
                    best_x = Some((delta, target));
                }
            }
        }
    }
    if let Some((delta, target)) = best_x {
        out[0] += delta;
        out[2] += delta;
        guides.push((true, target));
    }
    let mut best_y: Option<(f32, f32)> = None;
    for neighbor in &neighbors {
        let ny = [
            neighbor[1].min(neighbor[3]),
            (neighbor[1] + neighbor[3]) / 2.0,
            neighbor[1].max(neighbor[3]),
        ];
        let moving = [(out[1], 0usize), ((out[1] + out[3]) / 2.0, 1), (out[3], 2)];
        for (value, _) in moving {
            for target in ny {
                let delta = target - value;
                if delta.abs() <= tol && best_y.map_or(true, |(d, _)| delta.abs() < d.abs()) {
                    best_y = Some((delta, target));
                }
            }
        }
    }
    if let Some((delta, target)) = best_y {
        out[1] += delta;
        out[3] += delta;
        guides.push((false, target));
    }
    (out, guides)
}

/// Libellé anglais d'un type de champ de formulaire.
fn form_kind_label(kind: &FormFieldKind) -> String {    match kind {
        FormFieldKind::Text => "Text field".to_owned(),
        FormFieldKind::Checkbox => "Checkbox".to_owned(),
        FormFieldKind::Radio => "Radio".to_owned(),
        FormFieldKind::Choice => "Dropdown".to_owned(),
        FormFieldKind::MultiChoice => "List".to_owned(),
        FormFieldKind::Signature => "Signature".to_owned(),
        FormFieldKind::Button => "Button".to_owned(),
        FormFieldKind::Date => "Date field".to_owned(),
        FormFieldKind::Barcode => "Barcode".to_owned(),
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
        AnnotationKind::Polygon => "Polygon",
        AnnotationKind::PolyLine => "Polyligne",
        AnnotationKind::Ink => "Ink",
        AnnotationKind::Stamp => "Stamp",
        AnnotationKind::Link => "Link",
        AnnotationKind::Callout => "Callout",
        AnnotationKind::FileAttachment => "Attachment",
    }
}

/// Tri de la liste des commentaires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentSort {
    /// Par page, puis par date.
    Page,
    /// Du plus récent au plus ancien.
    Date,
    /// Par auteur, puis par page.
    Author,
}

/// Filtre par état de revue (« coché »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommentStateFilter {
    /// Tous les commentaires.
    All,
    /// Ceux qui n'ont pas encore été relus.
    Unchecked,
    /// Un état précis.
    State(AnnotationState),
}

impl CommentStateFilter {
    /// Les choix du menu, dans l'ordre.
    const ALL: [Self; 6] = [
        Self::All,
        Self::Unchecked,
        Self::State(AnnotationState::Accepted),
        Self::State(AnnotationState::Rejected),
        Self::State(AnnotationState::Cancelled),
        Self::State(AnnotationState::Completed),
    ];

    /// Libellé anglais pour l'interface.
    fn display_name(self) -> &'static str {
        match self {
            Self::All => "All status",
            Self::Unchecked => "Unchecked",
            Self::State(state) => state.display_name(),
        }
    }

    /// Vrai quand l'état de revue passe le filtre.
    fn matches(self, state: AnnotationState) -> bool {
        match self {
            Self::All => true,
            Self::Unchecked => state == AnnotationState::None,
            Self::State(wanted) => state == wanted,
        }
    }
}

/// Filtres de la liste des commentaires.
#[derive(Debug, Clone)]
struct CommentFilter {
    kind: Option<AnnotationKind>,
    author: Option<String>,
    color: Option<[u8; 3]>,
    state: CommentStateFilter,
    /// Recherche plein texte, insensible à la casse.
    query: String,
}

impl CommentFilter {
    /// Vrai quand le commentaire passe tous les filtres.
    fn matches(&self, annotation: &Annotation) -> bool {
        if let Some(kind) = self.kind {
            if annotation.kind != kind {
                return false;
            }
        }
        if let Some(author) = &self.author {
            if &annotation.author != author {
                return false;
            }
        }
        if let Some(color) = self.color {
            if annotation.style.color != color {
                return false;
            }
        }
        if !self.state.matches(annotation.state) {
            return false;
        }
        let query = self.query.trim();
        if !query.is_empty() {
            let query = query.to_lowercase();
            let haystack = format!("{} {}", annotation.content, annotation.author).to_lowercase();
            if !haystack.contains(&query) {
                return false;
            }
        }
        true
    }
}

/// Une ligne de la liste des commentaires, prête à peindre.
struct CommentRow {
    /// Indice dans `Tab::annots`.
    index: usize,
    id: String,
    page: usize,
    kind: AnnotationKind,
    content: String,
    author: String,
    created_at: String,
    color: [u8; 3],
    state: AnnotationState,
    /// 0 pour un commentaire, 1 et plus pour une réponse imbriquée.
    depth: usize,
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

/// Suffixe unique (horloge + processus) des fichiers extraits.
fn unique_stamp() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or(0);
    format!("{millis}-{}", std::process::id())
}

/// Durée de conservation d'une extraction de pièce jointe temporaire.
const ATTACHMENT_STAGING_MAX_AGE: std::time::Duration =
    std::time::Duration::from_secs(24 * 60 * 60);

/// Extrait une pièce jointe dans un dossier temporaire privé et unique.
///
/// Les pièces jointes peuvent être confidentielles : le dossier n'appartient
/// qu'à son propriétaire. `create_dir` et `create_new` refusent un chemin déjà
/// occupé, donc un lien symbolique planté à l'avance ne détourne ni le dossier
/// ni l'écriture du fichier.
fn stage_attachment(bytes: &[u8], file_name: &str) -> std::io::Result<std::path::PathBuf> {
    let dir = std::env::temp_dir().join(format!("youpdf-attach-{}", unique_stamp()));
    std::fs::create_dir(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let path = dir.join(file_name);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    std::io::Write::write_all(&mut file, bytes)?;
    Ok(path)
}

/// Supprime les extractions de pièces jointes plus vieilles que `max_age`.
///
/// Seuls les dossiers `youpdf-attach-*` créés par [`stage_attachment`] sont
/// visés : les autres fichiers temporaires de l'application restent en place.
fn prune_attachment_staging(max_age: std::time::Duration) -> usize {
    let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if !name.starts_with("youpdf-attach-") {
            continue;
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let Ok(age) = std::time::SystemTime::now().duration_since(modified) else {
            continue;
        };
        if age < max_age {
            continue;
        }
        let path = entry.path();
        let outcome = if metadata.is_dir() {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_file(&path)
        };
        if outcome.is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Nom de fichier sûr pour l'extraction d'une pièce jointe.
///
/// Un nom hostile (`../../etc/passwd`, `/absolu`) ne doit pas sortir du
/// dossier temporaire : tout caractère hors `[A-Za-z0-9._-]` devient `_`, et
/// un nom vide ou fait de points retombe sur un nom par défaut.
fn safe_attachment_name(name: &str, index: usize) -> String {
    let safe: String = name
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let safe = safe.trim_start_matches('.');
    if safe.is_empty() {
        format!("attachment-{index}")
    } else {
        safe.to_owned()
    }
}

/// Vrai pour les annotations de marquage de texte, seules à porter une
/// citation (« le texte visé »).
fn is_markup(kind: AnnotationKind) -> bool {
    matches!(
        kind,
        AnnotationKind::Highlight
            | AnnotationKind::Underline
            | AnnotationKind::StrikeOut
            | AnnotationKind::Squiggly
    )
}

/// Indices des commentaires, dans l'ordre de tri demandé.
fn comment_order(annots: &[Annotation], sort: CommentSort) -> Vec<usize> {
    let mut order: Vec<usize> = (0..annots.len()).collect();
    match sort {
        CommentSort::Page => order.sort_by_key(|index| {
            (
                annots[*index].page,
                annots[*index].created_at.clone(),
                annots[*index].id.clone(),
            )
        }),
        CommentSort::Date => order.sort_by(|a, b| {
            annots[*b]
                .created_at
                .cmp(&annots[*a].created_at)
                .then(annots[*a].page.cmp(&annots[*b].page))
                .then(annots[*a].id.cmp(&annots[*b].id))
        }),
        CommentSort::Author => order.sort_by(|a, b| {
            annots[*a]
                .author
                .to_lowercase()
                .cmp(&annots[*b].author.to_lowercase())
                .then(annots[*a].page.cmp(&annots[*b].page))
                .then(annots[*a].id.cmp(&annots[*b].id))
        }),
    }
    order
}

/// Construit les lignes affichées : filtres, tri, puis réponses imbriquées
/// sous leur parent.
///
/// Une réponse dont le parent est filtré remonte à la racine : elle reste
/// visible plutôt que d'être perdue. Un commentaire sans racine (réponse
/// circulaire) est posé à plat, pour la même raison.
fn comment_rows(annots: &[Annotation], filter: &CommentFilter, sort: CommentSort) -> Vec<CommentRow> {
    use std::collections::HashSet;

    let included: HashSet<String> = annots
        .iter()
        .filter(|annotation| filter.matches(annotation))
        .map(|annotation| annotation.id.clone())
        .collect();
    let order = comment_order(annots, sort);

    let mut rows = Vec::new();
    let mut emitted: HashSet<String> = HashSet::new();
    for index in &order {
        let annotation = &annots[*index];
        if !included.contains(annotation.id.as_str()) {
            continue;
        }
        let orphan = annotation
            .reply_to
            .as_deref()
            .map_or(true, |parent| !included.contains(parent));
        if orphan {
            append_thread(annots, &order, &included, *index, 0, &mut rows, &mut emitted);
        }
    }
    // Réponses circulaires : jamais atteintes depuis une racine.
    for index in &order {
        let annotation = &annots[*index];
        if included.contains(annotation.id.as_str()) && !emitted.contains(annotation.id.as_str()) {
            append_thread(annots, &order, &included, *index, 0, &mut rows, &mut emitted);
        }
    }
    rows
}

/// Ajoute un commentaire puis ses réponses, récursivement.
fn append_thread(
    annots: &[Annotation],
    order: &[usize],
    included: &std::collections::HashSet<String>,
    index: usize,
    depth: usize,
    rows: &mut Vec<CommentRow>,
    emitted: &mut std::collections::HashSet<String>,
) {
    let annotation = &annots[index];
    // `emitted` rompt aussi les cycles : un commentaire déjà posé n'y revient
    // jamais.
    if !emitted.insert(annotation.id.clone()) {
        return;
    }
    rows.push(CommentRow {
        index,
        id: annotation.id.clone(),
        page: annotation.page,
        kind: annotation.kind,
        content: annotation.content.clone(),
        author: annotation.author.clone(),
        created_at: annotation.created_at.clone(),
        color: annotation.style.color,
        state: annotation.state,
        depth,
    });
    for child in order {
        let reply = &annots[*child];
        if reply.reply_to.as_deref() != Some(annotation.id.as_str())
            || !included.contains(reply.id.as_str())
        {
            continue;
        }
        append_thread(annots, order, included, *child, depth + 1, rows, emitted);
    }
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

/// Initiale d'auteur, pour l'avatar de la carte.
fn author_initial(author: &str) -> String {
    author
        .chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned())
}

/// Couleur d'affichage `#RRGGBB`.
fn hex_color(color: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
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
                    painter.rect_filled(*block, Rounding::same(2.0), fill);
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

    fn cachet(rows: &[Vec<u32>]) -> HashSet<u32> {
        rows.iter().flatten().copied().collect()
    }

    fn history_entry(marker: u8, len: usize) -> HistoryEntry {
        HistoryEntry {
            doc: Some(vec![marker; len]),
            annots: Vec::new(),
        }
    }

    #[test]
    fn history_drops_oldest_beyond_count_and_bytes() {
        let mut by_count: Vec<HistoryEntry> = (0..5).map(|n| history_entry(n, 1)).collect();
        trim_history_to(&mut by_count, 3, usize::MAX);
        assert_eq!(by_count.len(), 3);
        // Les plus récents restent, dans l'ordre.
        let markers: Vec<u8> = by_count
            .iter()
            .map(|entry| entry.doc.as_ref().unwrap()[0])
            .collect();
        assert_eq!(markers, vec![2, 3, 4]);

        let mut by_bytes = vec![history_entry(0, 4), history_entry(1, 4), history_entry(2, 4)];
        trim_history_to(&mut by_bytes, 10, 8);
        assert_eq!(by_bytes.len(), 2);
        assert_eq!(by_bytes[0].doc.as_ref().unwrap()[0], 1);
        assert_eq!(by_bytes[1].doc.as_ref().unwrap()[0], 2);
    }

    /// L'aimant d'un champ prend le bord voisin et signale le guide.
    #[test]
    fn snap_form_rect_aligns_to_neighbor_edge() {
        let neighbor = FormField {
            object_id: (1, 0),
            widget_id: (1, 0),
            name: "a".to_string(),
            kind: FormFieldKind::Text,
            value: None,
            options: Vec::new(),
            read_only: false,
            tooltip: String::new(),
            required: false,
            multiline: false,
            page: Some(1),
            rect: [100.0, 100.0, 200.0, 140.0],
            states: Vec::new(),
            state: None,
        };
        // Bord gauche à 2 pt de celui du voisin : il doit coller.
        let (rect, guides) =
            snap_form_rect([102.0, 200.0, 130.0, 220.0], &[neighbor], 0, 6.0);
        assert!((rect[0] - 100.0).abs() < 1e-4, "{rect:?}");
        assert!((rect[2] - 128.0).abs() < 1e-4, "{rect:?}");
        assert!(guides.iter().any(|(vertical, at)| *vertical && (*at - 100.0).abs() < 1e-4));
        // Aucun voisin à portée : rien ne bouge.
        let (far, guides) =
            snap_form_rect([400.0, 400.0, 450.0, 420.0], &[], 0, 6.0);
        assert_eq!(far, [400.0, 400.0, 450.0, 420.0]);
        assert!(guides.is_empty());
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

    /// Les lignes du panneau Signets suivent les replis et la profondeur.
    #[test]
    fn bookmark_rows_respect_folding() {
        let tree = vec![OutlineEntry {
            title: "Chapitre 1".to_owned(),
            page: Some(1),
            target: OutlineTarget::None,
            children: vec![OutlineEntry {
                title: "Section 1.1".to_owned(),
                page: Some(2),
                target: OutlineTarget::None,
                children: Vec::new(),
            }],
        }];

        let rows = bookmark_rows(&tree, &BTreeSet::new());
        assert_eq!(
            rows.iter()
                .map(|row| (row.0.as_str(), row.1.as_str(), row.3, row.4))
                .collect::<Vec<_>>(),
            vec![("0", "Chapitre 1", 0, true), ("0/0", "Section 1.1", 1, false)]
        );

        let folded: BTreeSet<String> = ["0".to_owned()].into_iter().collect();
        let rows = bookmark_rows(&tree, &folded);
        assert_eq!(rows.len(), 1, "un parent replié cache ses enfants");
        assert!(rows[0].5, "la ligne se sait repliée");
    }

    /// Déplacer un signet vers le bas ne doit pas atterrir un cran trop loin :
    /// la cible est comptée dans la liste d'avant retrait.
    #[test]
    fn bookmark_insert_index_compensates_the_removal() {
        // [A,B,C], déposer A (0) après B (index cible 2) -> insertion à 1.
        assert_eq!(bookmark_insert_index(Some(2), 3, true, 0), 1);
        // Déposer C (2) après A (index cible 1) : rien ne se décale -> 1.
        assert_eq!(bookmark_insert_index(Some(1), 3, true, 2), 1);
        // Parents différents : aucun ajustement.
        assert_eq!(bookmark_insert_index(Some(2), 3, false, 0), 2);
        // Sans index : en fin de liste, ajusté du retrait dans la même liste.
        assert_eq!(bookmark_insert_index(None, 3, true, 0), 2);
        assert_eq!(bookmark_insert_index(None, 3, false, 0), 3);
    }

    /// Le chemin du parent visé se décale quand le signet retiré le précède.
    #[test]
    fn bookmark_shift_path_follows_the_removal() {
        // Retirer le frère 0 décale tout ce qui le suit d'un cran.
        assert_eq!(bookmark_shift_path(&[1, 2], &[0]), vec![0, 2]);
        // Retirer un frère d'un sous-niveau décale ses suivants au même niveau.
        assert_eq!(bookmark_shift_path(&[0, 3], &[0, 2]), vec![0, 2]);
        // Même racine, sous-arbre différent : rien ne bouge.
        assert_eq!(bookmark_shift_path(&[1, 0], &[0, 2]), vec![1, 0]);
        // Un ancêtre du nœud visé n'est pas décalé.
        assert_eq!(bookmark_shift_path(&[0], &[0, 2]), vec![0]);
    }

    /// Déplacer un signet le retire de son ancien parent et le pose au bon
    /// endroit ; on ne peut pas le déposer dans sa propre descendance.
    #[test]
    fn bookmark_move_reparents_and_reorders() {
        let mut tree = vec![
            OutlineEntry {
                title: "A".to_owned(),
                page: None,
                target: OutlineTarget::None,
                children: vec![OutlineEntry {
                    title: "A.1".to_owned(),
                    page: None,
                    target: OutlineTarget::None,
                    children: Vec::new(),
                }],
            },
            OutlineEntry {
                title: "B".to_owned(),
                page: None,
                target: OutlineTarget::None,
                children: Vec::new(),
            },
        ];

        let entry = bookmark_take(&mut tree, &bookmark_path("0/0")).expect("prise");
        assert_eq!(entry.title, "A.1");
        tree.push(entry);
        assert_eq!(
            tree.iter().map(|e| e.title.as_str()).collect::<Vec<_>>(),
            vec!["A", "B", "A.1"]
        );

        assert!(bookmark_is_ancestor(&bookmark_path("0"), &bookmark_path("0/0")));
        assert!(!bookmark_is_ancestor(&bookmark_path("0"), &bookmark_path("1")));
        assert_eq!(bookmark_key(&[1, 2]), "1/2");
        assert_eq!(bookmark_path("1/2"), vec![1, 2]);

        assert_eq!(bookmark_all_keys(&tree).len(), 3);
    }

    /// Les formats usuels portent un nom ; les autres la taille brute.
    #[test]
    fn page_size_label_names_common_formats() {
        assert_eq!(page_size_label(595.0, 842.0), "A4 portrait");
        assert_eq!(page_size_label(842.0, 595.0), "A4 landscape");
        assert_eq!(page_size_label(612.0, 792.0), "Letter portrait");
        assert_eq!(page_size_label(1000.0, 500.0), "1000 × 500 pt");
    }

    /// La recherche de texte respecte la casse et retrouve une sous-chaîne.
    #[test]
    fn find_query_matches_with_and_without_case() {
        assert_eq!(find_query("Hello World", "world", false), Some(6));
        assert_eq!(find_query("Hello World", "world", true), None);
        assert_eq!(find_query("accents ÉglIsE", "église", false), Some(8));
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

    /// Commentaire de test, avec un identifiant et un auteur choisis.
    fn comment(id: &str, page: usize, author: &str, when: &str) -> Annotation {
        let mut annot = Annotation::new(page, AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0]);
        annot.id = id.to_owned();
        annot.author = author.to_owned();
        annot.created_at = when.to_owned();
        annot
    }

    fn filter_all() -> CommentFilter {
        CommentFilter {
            kind: None,
            author: None,
            color: None,
            state: CommentStateFilter::All,
            query: String::new(),
        }
    }

    /// Les réponses s'imbriquent sous leur parent, dans l'ordre du fil.
    #[test]
    fn comment_rows_nest_replies_under_their_parent() {
        let mut reply = comment("r1", 1, "Bob", "2026-01-02T00:00:00Z");
        reply.reply_to = Some("p1".to_owned());
        let mut deep = comment("r2", 1, "Bob", "2026-01-03T00:00:00Z");
        deep.reply_to = Some("r1".to_owned());
        let annots = vec![comment("p1", 1, "Alice", "2026-01-01T00:00:00Z"), reply, deep];

        let rows = comment_rows(&annots, &filter_all(), CommentSort::Page);

        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["p1", "r1", "r2"]);
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[2].depth, 2);
    }

    /// Une réponse dont le parent est filtré remonte à la racine : elle
    /// reste visible.
    #[test]
    fn comment_rows_promote_an_orphaned_reply() {
        let mut child = comment("r1", 1, "Bob", "2026-01-02T00:00:00Z");
        child.reply_to = Some("p1".to_owned());
        let annots = vec![comment("p1", 1, "Alice", "2026-01-01T00:00:00Z"), child];

        let filter = CommentFilter {
            author: Some("Bob".to_owned()),
            ..filter_all()
        };
        let rows = comment_rows(&annots, &filter, CommentSort::Page);

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "r1");
        assert_eq!(rows[0].depth, 0);
    }

    /// Une réponse circulaire ne disparaît pas de la liste.
    #[test]
    fn comment_rows_survive_a_reply_cycle() {
        let mut a = comment("a", 1, "Alice", "2026-01-01T00:00:00Z");
        a.reply_to = Some("b".to_owned());
        let mut b = comment("b", 1, "Bob", "2026-01-02T00:00:00Z");
        b.reply_to = Some("a".to_owned());

        let rows = comment_rows(&[a, b], &filter_all(), CommentSort::Page);

        assert_eq!(rows.len(), 2, "les deux commentaires apparaissent");
    }

    /// Les filtres de type, d'auteur et d'état se cumulent.
    #[test]
    fn comment_filter_stacks_criteria() {
        let mut accepted = comment("a", 1, "Alice", "2026-01-01T00:00:00Z");
        accepted.state = AnnotationState::Accepted;
        accepted.content = "ligne trouvée".to_owned();
        let mut other = comment("b", 1, "Bob", "2026-01-02T00:00:00Z");
        other.kind = AnnotationKind::Note;
        let annots = vec![accepted, other];

        let filter = CommentFilter {
            kind: Some(AnnotationKind::Highlight),
            author: Some("Alice".to_owned()),
            state: CommentStateFilter::State(AnnotationState::Accepted),
            query: "trouvée".to_owned(),
            ..filter_all()
        };
        let rows = comment_rows(&annots, &filter, CommentSort::Page);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "a");

        // Le même filtre, une case à côté : plus rien ne passe.
        let missed = CommentFilter {
            query: "absente".to_owned(),
            ..filter
        };
        assert!(comment_rows(&annots, &missed, CommentSort::Page).is_empty());

        // « Unchecked » ne retient que l'état nul.
        let unchecked = CommentFilter {
            state: CommentStateFilter::Unchecked,
            ..filter_all()
        };
        let rows = comment_rows(&annots, &unchecked, CommentSort::Page);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "b");
    }

    /// Le tri par date va du plus récent au plus ancien.
    #[test]
    fn comment_date_sort_is_newest_first() {
        let annots = vec![
            comment("vieux", 1, "Alice", "2026-01-01T00:00:00Z"),
            comment("neuf", 3, "Bob", "2026-03-01T00:00:00Z"),
        ];
        let rows = comment_rows(&annots, &filter_all(), CommentSort::Date);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["neuf", "vieux"]);
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

    /// L'avatar reprend la première lettre de l'auteur, en capitale.
    #[test]
    fn author_initial_is_uppercased() {
        assert_eq!(author_initial("alice"), "A");
        assert_eq!(author_initial("Bob"), "B");
        assert_eq!(author_initial(""), "?");
    }

    /// Une image droite : le rognage affiché est aussi le rognage de l'image.
    #[test]
    fn crop_fractions_follow_an_upright_image() {
        // Boîte de 200×200 posée en (150, 300), pixels vers le haut.
        let crop = crop_fractions_in_image(
            [200.0, 0.0, 0.0, 200.0, 150.0, 300.0],
            [200.0, 400.0, 350.0, 500.0],
        );
        // Le rectangle gardé va de x=200 à 350 : 25 % de la largeur part à
        // gauche, rien à droite. Il va de y=400 à 500 : la moitié basse de
        // l'image s'en va.
        assert!((crop.left - 0.25).abs() < 1e-4, "{crop:?}");
        assert!(crop.right.abs() < 1e-4, "{crop:?}");
        assert!((crop.bottom - 0.5).abs() < 1e-4, "{crop:?}");
        assert!(crop.top.abs() < 1e-4, "{crop:?}");
    }

    /// Image tournée d'un quart de tour horaire : l'axe horizontal affiché est
    /// l'axe vertical de l'image, dans l'autre sens.
    #[test]
    fn crop_fractions_follow_a_quarter_turned_image() {
        // Matrice produite par `RotateCw` : `(a, b) = (0, -200)` descend,
        // `(c, d) = (200, 0)` va vers la droite.
        let matrix = [0.0, -200.0, 200.0, 0.0, 150.0, 500.0];
        // On garde la moitié haute de l'affichage.
        let crop = crop_fractions_in_image(matrix, [150.0, 400.0, 350.0, 500.0]);
        assert!(crop.left.abs() < 1e-4, "{crop:?}");
        assert!((crop.right - 0.5).abs() < 1e-4, "{crop:?}");
        assert!(crop.top.abs() < 1e-4 && crop.bottom.abs() < 1e-4, "{crop:?}");
        // Et la moitié basse donne l'autre moitié de l'image.
        let crop = crop_fractions_in_image(matrix, [150.0, 300.0, 350.0, 400.0]);
        assert!((crop.left - 0.5).abs() < 1e-4, "{crop:?}");
        assert!(crop.right.abs() < 1e-4, "{crop:?}");
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
}
