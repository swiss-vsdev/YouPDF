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
use youpdf_shared::{pal, set_palette, Theme};
use crate::ai::AiPanel;
use crate::home::{self, HomeCategory, HomeView, ViewMode};
use youpdf_core::compress::CompressionLevel;
use youpdf_core::render::NullRenderer;
use youpdf_core::ocr::{make_searchable, TesseractEngine};
use youpdf_core::{
    blank_document, document_from_images, fold_case, Annotation, AnnotationKind, AnnotationStyle,
    AttachmentInfo, Background, Bates, BatesPosition, ComparisonReport, Conformance, CropMargins,
    DifferenceKind, Document, EncryptionOptions, EncryptionStrength, ExtractedTable,
    FieldProperties, FormDataFormat, FormField, FormFieldKind, HeaderFooter, ImageFormat, KeyValue,
    MeasureKind, Measurement, NumberStyle, OcrEngine, OcrMode, OfficeFormat, OutlineItem,
    OutlineStyle, PageRange, PageBitmap,
    PdfiumRenderer, RedactionRegion, RenderOptions, Renderer, SearchIndex, SearchOptions,
    SearchResult, TextFormat, TextFragment, Watermark, STAMP_NAMES,
};

// --------------------------------------------------------------------------
// Palette
// --------------------------------------------------------------------------

/// Fonds proposés dans le panneau Aspect.
const CANVAS_STYLES: [Color32; 4] = [
    Color32::from_rgb(0x30, 0x30, 0x30),
    Color32::from_rgb(0x1E, 0x1E, 0x1E),
    Color32::from_rgb(0x4A, 0x4A, 0x4A),
    Color32::from_rgb(0xD8, 0xD8, 0xD8),
];

const TITLE_H: f32 = 40.0;
const STATUS_H: f32 = 26.0;
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
    /// Les images embarquées du document, extraites telles quelles.
    Embedded,
    /// Document bureautique (Word, Excel, PowerPoint).
    Office,
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

/// Sélection de texte sur une page, en repère affiché.
#[derive(Clone, Copy, Debug, PartialEq)]
struct TextSel {
    /// Page concernée.
    page: u32,
    /// Ancre et tête du glissé ; `None` sélectionne tout le texte de la page.
    span: Option<([f32; 2], [f32; 2])>,
}

/// Indices des blocs couverts par une sélection, dans l'ordre de lecture.
///
/// L'ordre est celui d'un lecteur : lignes de haut en bas, puis blocs de
/// gauche à droite. La sélection prend tout ce qui se trouve entre le bloc de
/// l'ancre et celui de la tête. Le découpage s'arrête au bloc, pas au
/// caractère : l'extraction estime les rectangles (`0,5 × corps` par
/// caractère), un découpage plus fin afficherait une précision trompeuse.
fn selected_indices(frags: &[TextFragment], span: Option<([f32; 2], [f32; 2])>) -> Vec<usize> {
    let order = reading_order(frags);
    if order.is_empty() {
        return order;
    }
    let Some((anchor, head)) = span else {
        return order;
    };
    let start = nearest_fragment(frags, anchor);
    let end = nearest_fragment(frags, head);
    let start = order.iter().position(|i| *i == start).unwrap_or(0);
    let end = order.iter().position(|i| *i == end).unwrap_or(start);
    let (low, high) = if start <= end {
        (start, end)
    } else {
        (end, start)
    };
    order[low..=high].to_vec()
}

/// Texte d'une sélection, blocs réunis dans l'ordre de lecture.
///
/// Deux blocs d'une même ligne sont séparés par une espace, deux lignes par un
/// saut de ligne.
fn selection_text(frags: &[TextFragment], span: Option<([f32; 2], [f32; 2])>) -> String {
    let mut out = String::new();
    let mut previous_line: Option<f32> = None;
    for index in selected_indices(frags, span) {
        let frag = &frags[index];
        let line = (frag.rect[1] + frag.rect[3]) * 0.5;
        if let Some(previous) = previous_line {
            let height = frag.font_size.max(1.0);
            out.push(if (previous - line).abs() > height * 0.6 {
                '\n'
            } else {
                ' '
            });
        }
        out.push_str(&frag.text);
        previous_line = Some(line);
    }
    out
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
    let mut order: Vec<usize> = (0..frags.len()).collect();
    order.sort_by(|a, b| {
        line_center(&frags[*b])
            .partial_cmp(&line_center(&frags[*a]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut out: Vec<usize> = Vec::with_capacity(order.len());
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
        out.extend(row);
        start = end;
    }
    out
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

/// Index du bloc le plus proche d'un point (repère affiché).
fn nearest_fragment(frags: &[TextFragment], point: [f32; 2]) -> usize {
    let mut best = 0;
    let mut best_d = f32::INFINITY;
    for (index, frag) in frags.iter().enumerate() {
        let d = rect_distance(frag.rect, point);
        if d < best_d {
            best_d = d;
            best = index;
        }
    }
    best
}

/// Distance d'un point à un rectangle ; zéro s'il est dedans.
fn rect_distance(rect: [f32; 4], point: [f32; 2]) -> f32 {
    let dx = (rect[0] - point[0]).max(0.0).max(point[0] - rect[2]);
    let dy = (rect[1] - point[1]).max(0.0).max(point[1] - rect[3]);
    (dx * dx + dy * dy).sqrt()
}

/// Extrait et met en cache les blocs de texte d'une page, en repère affiché.
///
/// Répond `false` si le moteur n'a rien lu : l'appelant ne doit alors pas
/// proposer de sélection sur cette page.
fn extract_page_fragments(tab: &mut Tab, page: u32) -> bool {
    match tab.doc.extract_text_fragments(page) {
        Ok(mut frags) => {
            for frag in &mut frags {
                frag.rect = rect_to_display(&tab.doc, page as usize, frag.rect);
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
    ("Split pages", "^7", ToolsEntry::Organize),
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
    /// Glissé d'une mesure (distance, périmètre, aire).
    Measure,
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
    /// Piles d'annulation/rétablissement (instantanés des annotations).
    undo: Vec<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
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
    /// Nature de l'export : texte, images ou document bureautique.
    export_mode: ExportMode,
    /// Format d'image choisi.
    export_image_format: ImageFormat,
    /// Résolution d'export, en points par pouce.
    export_dpi: f32,
    /// Format bureautique choisi.
    export_office_format: OfficeFormat,
    /// Fenêtre « Compresser » : dernier bilan affiché.
    compress_open: bool,
    compress_report: Option<String>,
    /// Niveau de compression d'image choisi.
    compress_level: CompressionLevel,
    /// Fenêtre « Protéger » et ses réglages.
    protect_open: bool,
    protect: EncryptionOptions,
    /// Fenêtre « Recadrage » et ses marges.
    crop_open: bool,
    crop: CropMargins,
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
    /// Brouillon des propriétés du champ sélectionné.
    form_props: Option<(usize, FieldProperties)>,
    /// Format choisi pour l'échange de données de formulaire.
    form_data_format: FormDataFormat,
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
    /// Un réglage de zoom vient d'être demandé : le canevas horodate le
    /// geste et diffère le rendu net tant que l'utilisateur bouge encore.
    zoom_dirty: bool,
    /// Instant du dernier changement de zoom (voir `ZOOM_SETTLE_SECS`).
    zoom_moved_at: f64,
}

impl YouPdfApp {
    pub fn new(cc: &eframe::CreationContext<'_>, initial: Option<PathBuf>) -> Self {
        install_fonts(&cc.egui_ctx);
        // La configuration est lue une seule fois : le thème en vient, et le
        // panneau IA reçoit le même objet au lieu de relire le fichier.
        let settings = youpdf_ai::AppSettings::load().unwrap_or_default();
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
            compress_open: false,
            compress_report: None,
            compress_level: CompressionLevel::Ebook,
            protect_open: false,
            protect: EncryptionOptions::default(),
            crop_open: false,
            crop: CropMargins::default(),
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
            dragging_tab: None,
            drag_target: None,
            home: HomeView::default(),
            stars: home::load_stars(),
            home_thumbs: HashMap::new(),
            ai: AiPanel::with_settings(settings),
            ai_doc_tab: None,
            ai_doc_text: String::new(),
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
            form_props: None,
            form_data_format: FormDataFormat::Fdf,
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
            zoom_dirty: false,
            zoom_moved_at: 0.0,
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
        let protected = path
            .as_deref()
            .map(Document::file_is_encrypted)
            .unwrap_or(false);
        let annots = annots_to_display(&doc, doc.read_annotations());
        if !annots.is_empty() && doc.clear_youpdf_annotations().is_err() {
            tracing::warn!("purge des annotations impossible");
        }
        let baseline_annots = annots.clone();
        self.tabs.push(Tab {
            doc,
            path: path.clone(),
            title: title.clone(),
            page: 1,
            zoom: None,
            zoom_fit_page: false,
            cache: HashMap::new(),
            last_bitmap: None,
            dirty: false,
            structural_dirty: false,
            annots,
            baseline_annots,
            annots_loaded: true,
            undo: Vec::new(),
            redo: Vec::new(),
            outline: Vec::new(),
            forms: Vec::new(),
            attachments: Vec::new(),
            fragments: HashMap::new(),
            structure_loaded: false,
            pinned: false,
            protected,
        });
        self.active = self.tabs.len() - 1;
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
        self.redact_regions.clear();
        self.form_edit = None;
        self.selected_annot = 0;
        self.has_annot_selection = false;
        self.text_sel = None;
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

    /// Fenêtre « Exporter » : choix du format de sortie.
    fn export_window(&mut self, ctx: &egui::Context) {
        if !self.export_open {
            return;
        }
        let mut open = true;
        let mut run = false;
        self.show_modal(ctx, "modal-export", "Export", 380.0, &mut open, |app, ui| {
            ui.label(RichText::new("Text").size(12.0).color(pal().muted));
            for format in [
                TextFormat::Plain,
                TextFormat::Markdown,
                TextFormat::Html,
                TextFormat::Csv,
                TextFormat::Xml,
                TextFormat::Rtf,
            ] {
                if ui
                    .selectable_label(
                        app.export_mode == ExportMode::Text && app.export_format == format,
                        format.display_name(),
                    )
                    .clicked()
                {
                    app.export_mode = ExportMode::Text;
                    app.export_format = format;
                }
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Images").size(12.0).color(pal().muted));
            for (format, label) in [
                (ImageFormat::Png, "PNG (one file per page)"),
                (ImageFormat::Jpeg, "JPEG (one file per page)"),
            ] {
                if ui
                    .selectable_label(
                        app.export_mode == ExportMode::Image
                            && app.export_image_format == format,
                        label,
                    )
                    .clicked()
                {
                    app.export_mode = ExportMode::Image;
                    app.export_image_format = format;
                }
            }
            if app.export_mode == ExportMode::Image {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Resolution").size(12.0).color(pal().muted));
                    ui.add(
                        egui::DragValue::new(&mut app.export_dpi)
                            .range(36.0..=600.0)
                            .suffix(" dpi"),
                    );
                });
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Embedded images").size(12.0).color(pal().muted));
            if ui
                .selectable_label(
                    app.export_mode == ExportMode::Embedded,
                    "PNG (one file per embedded image)",
                )
                .clicked()
            {
                app.export_mode = ExportMode::Embedded;
            }
            ui.add_space(6.0);
            ui.label(RichText::new("Office").size(12.0).color(pal().muted));
            for format in OfficeFormat::ALL {
                if ui
                    .selectable_label(
                        app.export_mode == ExportMode::Office
                            && app.export_office_format == format,
                        format.display_name(),
                    )
                    .clicked()
                {
                    app.export_mode = ExportMode::Office;
                    app.export_office_format = format;
                }
            }
            ui.add_space(8.0);
            ui.label(
                RichText::new(match app.export_mode {
                    ExportMode::Image => {
                        "Each page is rendered at the chosen resolution and written into a \
                         folder as page-0001, page-0002…"
                    }
                    ExportMode::Office => {
                        "The text layer is rebuilt as an Office document: one paragraph or \
                         row per line, one slide per page. Images, tables and the layout are \
                         not reconstructed."
                    }
                    ExportMode::Embedded => {
                        "Each image embedded in the document is written as a PNG. Rendered \
                         page pictures are not included — use Images for those."
                    }
                    ExportMode::Text => {
                        "Only the text layer is exported: images, tables and the page layout \
                         are not rebuilt, and pages without extractable text come out empty."
                    }
                })
                .size(11.0)
                .color(pal().muted),
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

    /// Écrit la couche texte du document dans le format choisi, les pages en
    /// images, ou un document bureautique.
    fn export_active(&mut self, ctx: &egui::Context) {
        if self.export_mode == ExportMode::Embedded {
            let Some(directory) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let result = self.tabs.get(self.active).map(|tab| {
                let mut written = 0usize;
                let mut skipped = 0usize;
                for page in 1..=tab.total() as u32 {
                    let images = tab.doc.extract_images(page).unwrap_or_default();
                    for (index, info) in images.iter().enumerate() {
                        match tab.doc.extract_image_png(info) {
                            Ok(bytes) => {
                                let name =
                                    format!("page{page:04}-image{:02}.png", index + 1);
                                if std::fs::write(directory.join(name), bytes).is_ok() {
                                    written += 1;
                                } else {
                                    skipped += 1;
                                }
                            }
                            Err(_) => skipped += 1,
                        }
                    }
                }
                (written, skipped)
            });
            if let Some((written, skipped)) = result {
                let name = directory
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "the folder".to_owned());
                self.toast(
                    ctx,
                    format!("{written} embedded image(s) written to {name} ({skipped} skipped)"),
                );
            }
            return;
        }
        if self.export_mode == ExportMode::Office {
            let format = self.export_office_format;
            let stem = self
                .active_stem()
                .or_else(|| self.tabs.get(self.active).map(|tab| tab.title.clone()))
                .unwrap_or_else(|| "document".to_owned());
            let Some(path) = rfd::FileDialog::new()
                .add_filter(format.display_name(), &[format.extension()])
                .set_file_name(format!("{stem}.{}", format.extension()))
                .save_file()
            else {
                return;
            };
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_office(&path, format));
            match result {
                Some(Ok(())) => {
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "the file".to_owned());
                    self.toast(ctx, format!("Exported {name}"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
        if self.export_mode == ExportMode::Image {
            let format = self.export_image_format;
            let dpi = self.export_dpi;
            let Some(directory) = rfd::FileDialog::new().pick_folder() else {
                return;
            };
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_images(&directory, format, dpi));
            match result {
                Some(Ok(paths)) => {
                    let name = directory
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "the folder".to_owned());
                    self.toast(ctx, format!("{} image(s) written to {name}", paths.len()));
                }
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
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
                tab.annots = annots_to_display(&tab.doc, tab.doc.read_annotations());
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
            // Revenir à l'état enregistré doit éteindre « non enregistré ».
            tab.dirty = tab.structural_dirty || tab.annots != tab.baseline_annots;
            self.toast(ctx, "Undone");
        }
    }

    fn redo(&mut self, ctx: &egui::Context) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if let Some(next) = tab.redo.pop() {
            tab.undo.push(std::mem::replace(&mut tab.annots, next));
            tab.dirty = tab.structural_dirty || tab.annots != tab.baseline_annots;
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
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + indent, rect.center().y),
                        Align2::LEFT_CENTER,
                        shorten(title, 30),
                        FontId::proportional(12.0),
                        pal().icon_bright,
                    );
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 10.0, rect.center().y),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(10.5),
                            pal().muted,
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

        // Échange de données de formulaire (FDF, XFDF, CSV, JSON).
        let mut import = false;
        let mut export = false;
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("form-data-format")
                .width(80.0)
                .selected_text(self.form_data_format.display_name())
                .show_ui(ui, |ui| {
                    for format in [
                        FormDataFormat::Fdf,
                        FormDataFormat::Xfdf,
                        FormDataFormat::Csv,
                        FormDataFormat::Json,
                    ] {
                        if ui
                            .selectable_label(
                                self.form_data_format == format,
                                format.display_name(),
                            )
                            .clicked()
                        {
                            self.form_data_format = format;
                        }
                    }
                });
            if ui.button("Import").clicked() {
                import = true;
            }
            if ui.button("Export").clicked() {
                export = true;
            }
        });
        ui.add_space(4.0);

        if import {
            let format = self.form_data_format;
            if let Some(path) = rfd::FileDialog::new()
                .add_filter(format.display_name(), &[format.extension()])
                .pick_file()
            {
                match std::fs::read(&path) {
                    Ok(data) => {
                        let result = self
                            .tabs
                            .get_mut(self.active)
                            .map(|tab| tab.doc.import_form_data(format, &data));
                        match result {
                            Some(Ok(count)) => {
                                self.mark_document_changed();
                                self.ensure_structure();
                                self.toast(ctx, format!("{count} field value(s) imported"));
                            }
                            Some(Err(err)) => self.toast(ctx, format!("Import failed: {err}")),
                            None => {}
                        }
                    }
                    Err(err) => self.toast(ctx, format!("Import failed: {err}")),
                }
            }
            return;
        }
        if export {
            let format = self.form_data_format;
            let result = self
                .tabs
                .get(self.active)
                .map(|tab| tab.doc.export_form_data(format));
            match result {
                Some(Ok(data)) => {
                    let stem = self.active_stem().unwrap_or_else(|| "form".to_owned());
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(format.display_name(), &[format.extension()])
                        .set_file_name(format!("{stem}.{}", format.extension()))
                        .save_file()
                    {
                        match std::fs::write(&path, data) {
                            Ok(()) => self.toast(ctx, "Form data exported"),
                            Err(err) => self.toast(ctx, format!("Export failed: {err}")),
                        }
                    }
                }
                Some(Err(err)) => self.toast(ctx, format!("Export failed: {err}")),
                None => {}
            }
            return;
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(6.0);
                for (idx, (name, kind, page, read_only, value)) in rows.iter().enumerate() {
                    let (rect, resp) = ui
                        .allocate_exact_size(vec2(PANEL_W - 24.0, 40.0), Sense::click());
                    if resp.hovered() {
                        ui.painter()
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 12.0),
                        Align2::LEFT_CENTER,
                        shorten(name, 26),
                        FontId::proportional(12.0),
                        pal().icon_bright,
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
                        pal().muted,
                    );
                    if let Some(page) = page {
                        ui.painter().text(
                            pos2(rect.max.x - 10.0, rect.min.y + 12.0),
                            Align2::RIGHT_CENTER,
                            format!("p.{page}"),
                            FontId::proportional(10.0),
                            pal().muted,
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
            self.form_props = self
                .tabs
                .get(self.active)
                .and_then(|tab| tab.forms.get(idx))
                .map(|field| {
                    (
                        idx,
                        FieldProperties {
                            name: field.name.clone(),
                            tooltip: field.tooltip.clone(),
                            required: field.required,
                            read_only: field.read_only,
                            multiline: field.multiline,
                            options: field.options.clone(),
                        },
                    )
                });
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
            .fill(pal().widget_bg)
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(format!("{}  ({})", field.name, form_kind_label(&field.kind)))
                        .size(11.5)
                        .color(pal().icon_bright),
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

        // Propriétés du champ : nom, aide, drapeaux, options.
        let mut props = self
            .form_props
            .take()
            .filter(|(i, _)| *i == idx)
            .map(|(_, props)| props)
            .unwrap_or_else(|| FieldProperties {
                name: field.name.clone(),
                tooltip: field.tooltip.clone(),
                required: field.required,
                read_only: field.read_only,
                multiline: field.multiline,
                options: field.options.clone(),
            });
        let mut apply_props = false;
        let choice = matches!(
            field.kind,
            FormFieldKind::Choice | FormFieldKind::MultiChoice
        );
        Frame::none()
            .fill(pal().widget_bg)
            .rounding(Rounding::same(8.0))
            .inner_margin(Margin::symmetric(10.0, 8.0))
            .show(ui, |ui| {
                ui.label(RichText::new("Properties").size(11.5).color(pal().icon_bright));
                ui.add_space(4.0);
                ui.add_sized(
                    vec2(PANEL_W - 46.0, 24.0),
                    TextEdit::singleline(&mut props.name).hint_text("Name"),
                );
                ui.add_sized(
                    vec2(PANEL_W - 46.0, 24.0),
                    TextEdit::singleline(&mut props.tooltip).hint_text("Tooltip"),
                );
                ui.checkbox(&mut props.required, "Required");
                ui.checkbox(&mut props.read_only, "Read only");
                if matches!(field.kind, FormFieldKind::Text) {
                    ui.checkbox(&mut props.multiline, "Multiline");
                }
                if choice {
                    let mut options = props.options.join(", ");
                    if ui
                        .add_sized(
                            vec2(PANEL_W - 46.0, 24.0),
                            TextEdit::singleline(&mut options)
                                .hint_text("Options, comma separated"),
                        )
                        .changed()
                    {
                        props.options = options
                            .split(',')
                            .map(|item| item.trim().to_owned())
                            .filter(|item| !item.is_empty())
                            .collect();
                    }
                }
                ui.add_space(4.0);
                if ui.button("Apply properties").clicked() {
                    apply_props = true;
                }
            });
        ui.add_space(6.0);

        if apply_props {
            if let Some(tab) = self.tabs.get_mut(self.active) {
                if let Some(field) = tab.forms.get(idx).cloned() {
                    match tab.doc.update_form_field(&field, &props) {
                        Ok(()) => {
                            if let Some(entry) = tab.forms.get_mut(idx) {
                                entry.name = props.name.clone();
                                entry.tooltip = props.tooltip.clone();
                                entry.required = props.required;
                                entry.read_only = props.read_only;
                                entry.multiline = props.multiline;
                                entry.options = props.options.clone();
                            }
                            tab.dirty = true;
                            tab.structural_dirty = true;
                            tab.invalidate_render();
                            self.toast(ctx, format!("Field {} updated", props.name));
                        }
                        Err(err) => self.toast(ctx, format!("Field update failed: {err}")),
                    }
                }
            }
            self.form_props = None;
        } else {
            self.form_props = Some((idx, props));
        }

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
                    tab.structural_dirty = true;
                    // La page est déjà rendue avec l'ancienne valeur.
                    tab.invalidate_render();
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
            ui.label(RichText::new("Background").size(11.5).color(pal().muted));
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
                        Stroke::new(1.5_f32, pal().accent),
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
            ui.label(RichText::new("Thumbnails").size(11.5).color(pal().muted));
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
                Ok(mut frags) => {
                    // Les fragments sortent du moteur en repère page ; tout le
                    // canevas raisonne en repère affiché.
                    for frag in &mut frags {
                        frag.rect = rect_to_display(&tab.doc, page as usize, frag.rect);
                    }
                    tab.fragments.insert(page, frags);
                }
                Err(err) => {
                    tracing::warn!(%err, page, "extraction du texte de la page");
                    tab.fragments.insert(page, Vec::new());
                }
            }
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
            if !extract_page_fragments(tab, page) {
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
            .map(|frags| selection_text(frags, sel.span))
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
                    .replace_text_fragment(&fragment, &draft.buffer, background)
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
        let wants_opacity = tool == 1;
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
                                    egui::Slider::new(&mut self.annot_width, 0.5..=8.0)
                                        .fixed_decimals(1),
                                );
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
            tab.dirty = true;
            tab.structural_dirty = true;
            tab.invalidate_render();
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
                let Ok(fragments) = tab.doc.extract_text_fragments(page as u32) else {
                    continue;
                };
                for fragment in fragments {
                    let rect = rect_to_display(&tab.doc, page, fragment.rect);
                    for rect in match_rects(&fragment.text, rect, &needle) {
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
            // malgré le geste en cours.
        }

        let options = RenderOptions {
            scale: target_scale,
            ..Default::default()
        };
        match renderer.render_page(&tab.doc, page, &options) {
            Ok(bitmap) => {
                tracing::info!(
                    "render page {page} step {step}: bitmap {}x{} (scale {:.3})",
                    bitmap.width,
                    bitmap.height,
                    logical_scale * ppp
                );
                let image = bitmap.to_egui_image();
                // La page en cours d'affichage garde ses pixels : la
                // réécriture y lit la couleur du fond. Une vignette rendue à
                // une échelle minuscule ne doit pas écraser cette référence.
                if page as usize == tab.page && step != THUMBNAIL_STEP {
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
                        scale: target_scale,
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
                            .rect_filled(rect, Rounding::same(6.0), pal().widget_bg);
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
                        pal().icon_bright,
                    );
                    ui.painter().text(
                        pos2(rect.min.x + 28.0, rect.min.y + 28.0),
                        Align2::LEFT_CENTER,
                        shorten(text, 26),
                        FontId::proportional(11.0),
                        pal().muted,
                    );
                    ui.painter().text(
                        pos2(rect.max.x - 10.0, rect.min.y + 12.0),
                        Align2::RIGHT_CENTER,
                        when.get(..10).unwrap_or(""),
                        FontId::proportional(9.5),
                        pal().muted,
                    );
                    let x_rect = Rect::from_center_size(
                        pos2(rect.max.x - 16.0, rect.max.y - 12.0),
                        vec2(16.0, 16.0),
                    );
                    let x_resp = ui.interact(x_rect, Id::new(("annot-x", *idx)), Sense::click());
                    icon_close(
                        ui.painter(),
                        x_rect,
                        if x_resp.hovered() { pal().icon_bright } else { pal().muted },
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
                            let active = self.search_case || self.search_word;
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
                            pal().thumb_tint,
                        );
                    } else if resp.hovered() {
                        ui.painter().rect_filled(
                            img_rect.expand(5.0),
                            Rounding::same(4.0),
                            Color32::from_rgb(0x3C, 0x3C, 0x3C),
                        );
                    }

                    if let Some(tex) =
                        Self::page_texture(&ctx, renderer, tab, page, THUMBNAIL_STEP, scale, ppp, true, status)
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
                            Stroke::new(1.0_f32, pal().accent),
                        );
                    }
                    ui.painter().text(
                        pos2(rect.center().x, img_rect.max.y + 14.0),
                        Align2::CENTER_CENTER,
                        page.to_string(),
                        FontId::proportional(11.0),
                        if selected { pal().accent } else { pal().muted },
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
                            ui.allocate_exact_size(vec2(PANEL_W - 24.0, 28.0), Sense::click());
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
                            ui.allocate_exact_size(vec2(PANEL_W - 24.0, 28.0), Sense::click());
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
                        .color(pal().muted),
                    );
                    if ui.small_button("Select all").clicked() {
                        selected = (1..=total).collect();
                    }
                    if ui.small_button("Clear").clicked() {
                        selected.clear();
                    }
                    ui.add_space(8.0);
                    ui.label(RichText::new("Split every").size(11.0).color(pal().muted));
                    ui.add(egui::DragValue::new(&mut split_per).range(1..=200).speed(0.2));
                    ui.label(RichText::new("page(s)").size(11.0).color(pal().muted));
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
                                        pal().thumb_tint
                                    } else {
                                        Color32::from_rgb(0x28, 0x28, 0x28)
                                    },
                                );
                                if let Some(tex) = Self::page_texture(
                                    ctx,
                                    renderer,
                                    tab,
                                    page,
                                    THUMBNAIL_STEP,
                                    scale,
                                    ppp,
                                    true,
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
                                        img_rect.expand(3.0),
                                        3.0,
                                        Stroke::new(1.5_f32, pal().accent),
                                    );
                                }
                                ui.painter().text(
                                    pos2(rect.center().x, img_rect.max.y + 14.0),
                                    Align2::CENTER_CENTER,
                                    page.to_string(),
                                    FontId::proportional(11.0),
                                    if picked { pal().accent } else { pal().muted },
                                );
                                if page as usize == current {
                                    ui.painter().circle_filled(
                                        pos2(rect.min.x + 8.0, rect.min.y + 8.0),
                                        3.0,
                                        pal().accent,
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

    /// Fenêtre de confirmation du caviardage.
    fn redact_window(&mut self, ctx: &egui::Context) {
        if !self.redact_modal {
            return;
        }
        let areas = self.redact_regions.len();
        let mut confirm = false;
        let mut cancel = false;
        let mut keep_open = true;
        self.show_modal(ctx, "modal-redact", "Apply redaction", 360.0, &mut keep_open, |app, ui| {
            ui.label(
                RichText::new(format!("{areas} area(s) will be blacked out permanently."))
                    .size(12.5)
                    .color(pal().icon_bright),
            );
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Redaction replaces each marked page with an image: the text                          underneath is destroyed, not hidden, and cannot be recovered.                          Annotations overlapping a marked area are deleted.",
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
                if ui.button("Cancel").clicked() {
                    cancel = true;
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
        });
        if cancel || !keep_open {
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
                    .fill(pal().widget_bg)
                    .rounding(Rounding::same(8.0))
                    .inner_margin(Margin::symmetric(10.0, 5.0))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(15.0, 15.0), Sense::hover());
                            icon_tools(ui.painter(), rect, pal().icon_bright);
                            ui.add_space(6.0);
                            ui.label(RichText::new("Tools").size(12.5).color(pal().icon_bright));
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
                                if menu_row(ui, label, shortcut) {
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

        // Menu `…` : enregistrer, exporter, propriétés.
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
                                Stroke::new(1.0_f32, pal().separator),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("File").size(10.0).color(pal().muted));
                            });
                            if menu_row(ui, "New blank page", "") {
                                action = Some(MenuAction::NewBlank);
                            }
                            if menu_row(ui, "From images…", "") {
                                action = Some(MenuAction::FromImages);
                            }
                            if menu_row(ui, "Combine with a PDF…", "") {
                                action = Some(MenuAction::Combine);
                            }
                            if menu_row(ui, "Print…", "⌘P") {
                                action = Some(MenuAction::Print);
                            }
                            if menu_row(ui, "Reveal in Finder", "") {
                                action = Some(MenuAction::Reveal);
                            }
                            if menu_row(ui, "Remove protection…", "") {
                                action = Some(MenuAction::RemoveProtection);
                            }
                            ui.add_space(4.0);
                            let sep = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, pal().separator),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Document").size(10.0).color(pal().muted));
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
                            if menu_row(ui, "Crop pages…", "") {
                                action = Some(MenuAction::Crop);
                            }
                            if menu_row(ui, "Background…", "") {
                                action = Some(MenuAction::Background);
                            }
                            if menu_row(ui, "Bates numbering…", "") {
                                action = Some(MenuAction::Bates);
                            }
                            if menu_row(ui, "Compare PDFs…", "") {
                                action = Some(MenuAction::Compare);
                            }
                            if menu_row(ui, "OCR…", "") {
                                action = Some(MenuAction::Ocr);
                            }
                            if menu_row(ui, "Create…", "") {
                                action = Some(MenuAction::Create);
                            }
                            if menu_row(ui, "Batch processing…", "") {
                                action = Some(MenuAction::Batch);
                            }
                            if menu_row(ui, "Extract data…", "") {
                                action = Some(MenuAction::Extract);
                            }
                            if menu_row(ui, "Generate bookmarks", "") {
                                action = Some(MenuAction::GenerateOutline);
                            }
                            if menu_row(ui, "Clear bookmarks", "") {
                                action = Some(MenuAction::ClearOutline);
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Save as").size(10.0).color(pal().muted));
                            });
                            for level in [
                                Conformance::PdfA2b,
                                Conformance::PdfX4,
                                Conformance::PdfE1,
                            ] {
                                if menu_row(ui, level.display_name(), "") {
                                    action = Some(MenuAction::Conformance(level));
                                }
                            }
                            if menu_row(ui, "Image PDF…", "") {
                                action = Some(MenuAction::ImagePdf);
                            }
                            ui.add_space(4.0);
                            let sep = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, pal().separator),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("View").size(10.0).color(pal().muted));
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
                            if menu_row(ui, "Slideshow", "F5") {
                                action = Some(MenuAction::Slideshow);
                            }
                            ui.add_space(4.0);
                            ui.horizontal(|ui| {
                                ui.add_space(10.0);
                                ui.label(RichText::new("Theme").size(10.0).color(pal().muted));
                            });
                            for theme in Theme::ALL {
                                if menu_row_checked(ui, theme.label(), self.theme == theme) {
                                    action = Some(MenuAction::Theme(theme));
                                }
                            }
                            ui.add_space(4.0);
                            if menu_row_checked(ui, "AI features…", self.ai.enabled()) {
                                action = Some(MenuAction::AiEnabled);
                            }
                            ui.add_space(4.0);
                            let sep = ui.cursor().top();
                            ui.painter().hline(
                                ui.max_rect().x_range(),
                                sep,
                                Stroke::new(1.0_f32, pal().separator),
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

    /// Fenêtre « Filigrane ».
    fn watermark_window(&mut self, ctx: &egui::Context) {
        if !self.watermark_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        self.show_modal(ctx, "modal-watermark", "Watermark", 360.0, &mut open, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Text").size(12.0).color(pal().muted));
                ui.add(TextEdit::singleline(&mut app.watermark.text).desired_width(230.0));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Size").size(12.0).color(pal().muted));
                ui.add(
                    egui::DragValue::new(&mut app.watermark.font_size).range(6.0..=200.0),
                );
                ui.label(RichText::new("Angle").size(12.0).color(pal().muted));
                ui.add(
                    egui::DragValue::new(&mut app.watermark.rotation_deg).range(-90.0..=90.0),
                );
                ui.checkbox(&mut app.watermark.tiled, "Tiled");
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Opacity").size(12.0).color(pal().muted));
                ui.add(
                    egui::Slider::new(&mut app.watermark.opacity, 0.05..=1.0).show_value(false),
                );
                ui.label(
                    RichText::new(format!("{:.0} %", app.watermark.opacity * 100.0))
                        .size(12.0)
                        .color(pal().icon_bright),
                );
                ui.label(RichText::new("Grey").size(12.0).color(pal().muted));
                let mut grey = app.watermark.color[0];
                if ui
                    .add(egui::Slider::new(&mut grey, 0..=255).show_value(false))
                    .changed()
                {
                    app.watermark.color = [grey, grey, grey];
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pages").size(12.0).color(pal().muted));
                let mut all = app.watermark_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.watermark_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.watermark_from.as_mut() {
                    ui.label(RichText::new("then from").size(12.0).color(pal().muted));
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
        self.show_modal(ctx, "modal-header-footer", "Header & footer", 460.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Each band has its own three slots; the page number joins the \
                     footer centre.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(4.0);
            for (label, slots) in [
                ("Header", &mut app.header_footer.header),
                ("Footer", &mut app.header_footer.footer),
            ] {
                ui.label(RichText::new(label).size(11.5).color(pal().icon_bright));
                egui::Grid::new(format!("hf-grid-{label}"))
                    .num_columns(3)
                    .spacing([8.0, 4.0])
                    .show(ui, |ui| {
                        ui.label(RichText::new("Left").size(11.0).color(pal().muted));
                        ui.label(RichText::new("Centre").size(11.0).color(pal().muted));
                        ui.label(RichText::new("Right").size(11.0).color(pal().muted));
                        ui.end_row();
                        ui.add(TextEdit::singleline(&mut slots.left).desired_width(140.0));
                        ui.add(TextEdit::singleline(&mut slots.center).desired_width(140.0));
                        ui.add(TextEdit::singleline(&mut slots.right).desired_width(140.0));
                        ui.end_row();
                    });
                ui.add_space(4.0);
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Page numbers").size(12.0).color(pal().muted));
                for (text, style) in [
                    ("None", None),
                    ("1", Some(NumberStyle::Numeric)),
                    ("1 / n", Some(NumberStyle::NumericTotal)),
                    ("i", Some(NumberStyle::Roman)),
                ] {
                    if ui
                        .selectable_label(app.header_footer.page_numbers == style, text)
                        .clicked()
                    {
                        app.header_footer.page_numbers = style;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Start at").size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.header_footer.start_page).range(1..=total));
                ui.checkbox(&mut app.header_footer.skip_first_page, "Skip first page");
                ui.label(RichText::new("Size").size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.header_footer.font_size).range(4.0..=36.0));
                ui.label(RichText::new("Margin").size(12.0).color(pal().muted));
                ui.add(egui::DragValue::new(&mut app.header_footer.margin).range(0.0..=120.0));
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pages").size(12.0).color(pal().muted));
                let mut all = app.header_footer_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.header_footer_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.header_footer_from.as_mut() {
                    ui.label(RichText::new("then from").size(12.0).color(pal().muted));
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
        self.show_modal(ctx, "modal-compress", "Compress", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Streams are deflated and unreachable objects are dropped.                          Fonts are not subset, so a text-only file barely shrinks.",
                )
                .size(12.0)
                .color(pal().muted),
            );
            ui.add_space(8.0);
            ui.label(RichText::new("Image quality").size(12.0).color(pal().muted));
            for level in CompressionLevel::ALL {
                if ui
                    .selectable_label(app.compress_level == level, level.display_name())
                    .clicked()
                {
                    app.compress_level = level;
                }
            }
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "Images are resampled to the chosen resolution and re-encoded as JPEG.                          A stream is replaced only when the result is smaller.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            if let Some(report) = app.compress_report.clone() {
                ui.add_space(6.0);
                ui.label(RichText::new(report).size(12.0).color(pal().icon_bright));
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
        let level = self.compress_level;
        let result =
            self.with_annotations_applied(|tab| tab.doc.compress_with(&path, &level.options()));
        match result {
            Some(Ok(report)) => {
                let before = report.before_bytes as f64 / 1024.0;
                let after = report.after_bytes as f64 / 1024.0;
                self.compress_report = Some(format!(
                    "{before:.0} KB → {after:.0} KB ({} byte(s) saved, {} image(s) resampled)",
                    report.saved_bytes(),
                    report.images_resampled
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
        self.show_modal(ctx, "modal-protect", "Protect", 420.0, &mut open, |app, ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Encryption").size(12.0).color(pal().muted));
                for strength in [EncryptionStrength::Aes256, EncryptionStrength::Rc4_128] {
                    if ui
                        .selectable_label(
                            app.protect.strength == strength,
                            strength.display_name(),
                        )
                        .clicked()
                    {
                        app.protect.strength = strength;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Open password").size(12.0).color(pal().muted));
                ui.add(
                    TextEdit::singleline(&mut app.protect.user_password)
                        .password(true)
                        .desired_width(190.0),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Permissions password").size(12.0).color(pal().muted));
                ui.add(
                    TextEdit::singleline(&mut app.protect.owner_password)
                        .password(true)
                        .desired_width(150.0),
                );
            });
            ui.add_space(6.0);
            ui.label(RichText::new("Allowed actions").size(12.0).color(pal().muted));
            egui::Grid::new("protect-perms")
                .num_columns(3)
                .spacing([14.0, 4.0])
                .show(ui, |ui| {
                    ui.checkbox(&mut app.protect.permissions.print, "Print");
                    ui.checkbox(
                        &mut app.protect.permissions.print_high_quality,
                        "High-quality print",
                    );
                    ui.checkbox(&mut app.protect.permissions.copy, "Copy text");
                    ui.end_row();
                    ui.checkbox(&mut app.protect.permissions.modify, "Modify");
                    ui.checkbox(&mut app.protect.permissions.annotate, "Comment");
                    ui.checkbox(&mut app.protect.permissions.fill_forms, "Fill forms");
                    ui.end_row();
                    ui.checkbox(&mut app.protect.permissions.assemble, "Assemble");
                    ui.checkbox(&mut app.protect.permissions.accessibility, "Accessibility");
                    ui.end_row();
                });
            ui.add_space(8.0);
            ui.label(
                RichText::new(
                    "The copy is encrypted on disk; the document in the window stays open                          and unencrypted.",
                )
                .size(11.0)
                .color(pal().muted),
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

    // -- Fonctionnalités de document ---------------------------------------

    /// Fenêtre « Recadrage » : marges retirées des quatre bords.
    fn crop_window(&mut self, ctx: &egui::Context) {
        if !self.crop_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut reset = false;
        self.show_modal(ctx, "modal-crop", "Crop pages", 340.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "Margins removed from every edge, in points (72 pt = 1 inch). \
                     Negative values grow the page.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(6.0);
            egui::Grid::new("crop-grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    for (label, value) in [
                        ("Left", &mut app.crop.left),
                        ("Right", &mut app.crop.right),
                        ("Top", &mut app.crop.top),
                        ("Bottom", &mut app.crop.bottom),
                    ] {
                        ui.label(RichText::new(label).size(12.0).color(pal().muted));
                        ui.add(
                            egui::DragValue::new(value).range(-500.0..=500.0).speed(0.5),
                        );
                        ui.end_row();
                    }
                });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Apply").clicked() {
                    apply = true;
                }
                if ui.button("Reset crop").clicked() {
                    reset = true;
                }
            });
        });

        let pages: Vec<usize> = (1..=self.active_total() as usize).collect();
        if apply {
            let margins = self.crop;
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.crop_pages(&pages, margins));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("{count} page(s) cropped"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Crop failed: {err}")),
                None => {}
            }
        }
        if reset {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.reset_crop(&pages));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Crop removed on {count} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Reset failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.crop_open = false;
        }
    }

    /// Fenêtre « Arrière-plan » : aplat de couleur ou image derrière la page.
    fn background_window(&mut self, ctx: &egui::Context) {
        if !self.background_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let mut choose = false;
        let total = self.active_total();
        let image_name = self
            .background
            .image
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        self.show_modal(ctx, "modal-background", "Background", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "The background is painted behind the page content and prints with \
                     the document.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Colour").size(12.0).color(pal().muted));
                let [mut r, mut g, mut b] = app.background.color;
                let mut changed = false;
                changed |= ui
                    .add(egui::Slider::new(&mut r, 0..=255).show_value(false))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut g, 0..=255).show_value(false))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut b, 0..=255).show_value(false))
                    .changed();
                if changed {
                    app.background.color = [r, g, b];
                }
                let (rect, _) = ui.allocate_exact_size(vec2(24.0, 18.0), Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    Rounding::same(4.0),
                    Color32::from_rgb(r, g, b),
                );
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Opacity").size(12.0).color(pal().muted));
                ui.add(
                    egui::Slider::new(&mut app.background.opacity, 0.05..=1.0)
                        .show_value(false),
                );
            });
            ui.horizontal(|ui| {
                if ui.button("Choose an image…").clicked() {
                    choose = true;
                }
                ui.label(
                    RichText::new(
                        image_name
                            .clone()
                            .unwrap_or_else(|| "No image (colour fill)".to_owned()),
                    )
                    .size(11.0)
                    .color(pal().muted),
                );
                if app.background.image.is_some() && ui.small_button("Remove").clicked() {
                    app.background.image = None;
                }
            });
            if app.background.image.is_some() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Image scale").size(12.0).color(pal().muted));
                    ui.add(egui::Slider::new(&mut app.background.scale, 0.2..=2.0));
                });
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pages").size(12.0).color(pal().muted));
                let mut all = app.background_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.background_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.background_from.as_mut() {
                    ui.label(RichText::new("from").size(12.0).color(pal().muted));
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

        if choose {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Images", &["png", "jpg", "jpeg"])
                .pick_file()
            {
                self.background.image = Some(path);
            }
        }
        if apply {
            self.background.pages = Self::page_range_from(self.background_from);
            let bg = self.background.clone();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_background(&bg));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Background applied to {count} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Background failed: {err}")),
                None => {}
            }
        }
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_backgrounds());
            match result {
                Some(Ok(removed)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {removed} background(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.background_open = false;
        }
    }

    /// Fenêtre « Numérotation Bates ».
    fn bates_window(&mut self, ctx: &egui::Context) {
        if !self.bates_open {
            return;
        }
        let mut open = true;
        let mut apply = false;
        let mut clear = false;
        let total = self.active_total();
        self.show_modal(ctx, "modal-bates", "Bates numbering", 380.0, &mut open, |app, ui| {
            ui.label(
                RichText::new(
                    "A sequential stamp on every chosen page, for legal discovery and \
                     document control.",
                )
                .size(11.0)
                .color(pal().muted),
            );
            ui.add_space(6.0);
            egui::Grid::new("bates-grid")
                .num_columns(2)
                .spacing([8.0, 4.0])
                .show(ui, |ui| {
                    ui.label(RichText::new("Prefix").size(12.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut app.bates.prefix).desired_width(160.0),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Suffix").size(12.0).color(pal().muted));
                    ui.add(
                        TextEdit::singleline(&mut app.bates.suffix).desired_width(160.0),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Start at").size(12.0).color(pal().muted));
                    ui.add(egui::DragValue::new(&mut app.bates.start).range(0..=999_999));
                    ui.end_row();
                    ui.label(RichText::new("Digits").size(12.0).color(pal().muted));
                    ui.add(egui::DragValue::new(&mut app.bates.digits).range(1..=12));
                    ui.end_row();
                    ui.label(RichText::new("Font size").size(12.0).color(pal().muted));
                    ui.add(
                        egui::DragValue::new(&mut app.bates.font_size).range(4.0..=72.0),
                    );
                    ui.end_row();
                    ui.label(RichText::new("Margin").size(12.0).color(pal().muted));
                    ui.add(egui::DragValue::new(&mut app.bates.margin).range(0.0..=200.0));
                    ui.end_row();
                });
            ui.add_space(4.0);
            ui.label(RichText::new("Position").size(12.0).color(pal().muted));
            ui.horizontal_wrapped(|ui| {
                for (position, label) in BATES_POSITIONS {
                    if ui
                        .selectable_label(app.bates.position == position, label)
                        .clicked()
                    {
                        app.bates.position = position;
                    }
                }
            });
            ui.horizontal(|ui| {
                ui.label(RichText::new("Pages").size(12.0).color(pal().muted));
                let mut all = app.bates_from.is_none();
                if ui.checkbox(&mut all, format!("All ({total})")).changed() {
                    app.bates_from = if all { None } else { Some(1) };
                }
                if let Some(from) = app.bates_from.as_mut() {
                    ui.label(RichText::new("from").size(12.0).color(pal().muted));
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
            self.bates.pages = Self::page_range_from(self.bates_from);
            let bates = self.bates.clone();
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.add_bates(&bates));
            match result {
                Some(Ok(count)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Bates stamp applied to {count} page(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Bates failed: {err}")),
                None => {}
            }
        }
        if clear {
            let result = self
                .tabs
                .get_mut(self.active)
                .map(|tab| tab.doc.clear_bates());
            match result {
                Some(Ok(removed)) => {
                    self.mark_document_changed();
                    self.toast(ctx, format!("Removed {removed} Bates stamp(s)"));
                }
                Some(Err(err)) => self.toast(ctx, format!("Remove failed: {err}")),
                None => {}
            }
        }
        if !open {
            self.bates_open = false;
        }
    }

    /// Fenêtre « Comparer des PDF » : différences de texte entre deux fichiers.
    fn compare_window(&mut self, ctx: &egui::Context) {
        if !self.compare_open {
            return;
        }
        let mut open = true;
        let mut pick = false;
        let mut run = false;
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
                    ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for diff in report.differences.iter().take(300) {
                            let (tag, color) = match diff.kind {
                                DifferenceKind::Added => {
                                    ("+", Color32::from_rgb(0x7A, 0xC0, 0x7A))
                                }
                                DifferenceKind::Removed => ("-", pal().redact_red),
                                DifferenceKind::Modified => ("~", pal().accent),
                                DifferenceKind::PageAdded => {
                                    ("+ page", Color32::from_rgb(0x7A, 0xC0, 0x7A))
                                }
                                DifferenceKind::PageRemoved => ("- page", pal().redact_red),
                            };
                            let text = diff
                                .after
                                .clone()
                                .or_else(|| diff.before.clone())
                                .unwrap_or_default();
                            let page = diff.page_b.or(diff.page_a).unwrap_or(0);
                            ui.label(
                                RichText::new(format!("{tag}  p{page}  {text}"))
                                    .size(11.0)
                                    .color(color),
                            );
                        }
                    });
                }
            });

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
            let result = {
                let engine = TesseractEngine::detect();
                self.tabs.get(self.active).map(|tab| {
                    make_searchable(&tab.doc, &*self.renderer, &engine, &language)
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
        let style = self.canvas_style.min(CANVAS_STYLES.len() - 1);
        if style == 0 {
            pal().canvas_bg
        } else {
            CANVAS_STYLES[style]
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
        let result = self
            .tabs
            .get_mut(self.active)
            .map(|tab| tab.doc.generate_outline(&style));
        match result {
            Some(Ok(count)) => {
                self.mark_document_changed();
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
    fn batch_window(&mut self, ctx: &egui::Context) {
        if !self.batch_open {
            return;
        }
        // Le lot tourne dans un fil de travail : on relève ses lignes, et on
        // redemande une trame tant qu'il tourne pour voir la liste avancer.
        self.pump_batch();
        if self.batch_running {
            ctx.request_repaint();
        }
        let mut open = true;
        let mut add_files = false;
        let mut choose_dir = false;
        let mut clear = false;
        let mut run = false;
        let files = self.batch_files.len();
        let dir = self
            .batch_dir
            .as_ref()
            .map(|p| p.display().to_string());
        egui::Window::new("Batch processing")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(vec2(520.0, 470.0))
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_max_width(520.0);
                ui.label(RichText::new("Files").size(12.0).color(pal().muted));
                ui.horizontal(|ui| {
                    if ui.button("Add PDFs…").clicked() {
                        add_files = true;
                    }
                    if ui.button("Clear").clicked() {
                        clear = true;
                    }
                    ui.label(RichText::new(format!("{files} file(s)")).size(11.0).color(pal().muted));
                });
                ScrollArea::vertical().max_height(130.0).show(ui, |ui| {
                    for path in &self.batch_files {
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        ui.label(RichText::new(name).size(11.0).color(pal().icon_bright));
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Output folder").size(12.0).color(pal().muted));
                ui.horizontal(|ui| {
                    if ui.button("Choose…").clicked() {
                        choose_dir = true;
                    }
                    ui.label(
                        RichText::new(dir.clone().unwrap_or_else(|| "Not chosen".to_owned()))
                            .size(11.0)
                            .color(pal().muted),
                    );
                });
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.label(
                    RichText::new("Operations, applied in this order").size(12.0).color(pal().muted),
                );
                ui.checkbox(&mut self.batch_text, "Export the text (plain text)");
                ui.checkbox(&mut self.batch_images, "Export the pages as PNG (150 dpi)");
                ui.checkbox(&mut self.batch_compress, "Write a compressed copy");
                ui.checkbox(
                    &mut self.batch_protect,
                    "Write a protected copy (current settings)",
                );
                ui.checkbox(
                    &mut self.batch_flatten,
                    "Write a copy with the annotations flattened",
                );
                ui.add_space(8.0);
                if ui
                    .add_enabled(
                        files > 0 && self.batch_dir.is_some() && !self.batch_running,
                        egui::Button::new(if self.batch_running {
                            "Running…"
                        } else {
                            "Run"
                        }),
                    )
                    .clicked()
                {
                    run = true;
                }
                if !self.batch_report.is_empty() {
                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ScrollArea::vertical().max_height(150.0).show(ui, |ui| {
                        ui.label(
                            RichText::new(self.batch_report.clone())
                                .size(11.0)
                                .color(pal().icon_bright),
                        );
                    });
                }
            });

        if add_files {
            if let Some(paths) = rfd::FileDialog::new()
                .add_filter("PDF", &["pdf"])
                .pick_files()
            {
                for path in paths {
                    if !self.batch_files.contains(&path) {
                        self.batch_files.push(path);
                    }
                }
            }
        }
        if clear {
            self.batch_files.clear();
            self.batch_report.clear();
        }
        if choose_dir {
            self.batch_dir = rfd::FileDialog::new().pick_folder();
        }
        if run {
            self.run_batch(ctx);
        }
        if !open {
            self.batch_open = false;
        }
    }

    /// Lance le lot dans un fil de travail.
    ///
    /// Chaque fichier est traité en séquence : les opérations écrivent des
    /// copies dans le dossier de sortie, l'original n'est jamais touché. Le lot
    /// ouvre et réécrit des documents entiers ; le faire dans la trame d'egui
    /// gelait l'interface pour toute sa durée. Le fil envoie une ligne par
    /// fichier, que la fenêtre consomme au fil de l'eau.
    fn run_batch(&mut self, ctx: &egui::Context) {
        if self.batch_running {
            return;
        }
        let Some(dir) = self.batch_dir.clone() else {
            return;
        };
        if let Err(err) = std::fs::create_dir_all(&dir) {
            self.toast(ctx, format!("Batch failed: {err}"));
            return;
        }
        let job = BatchJob {
            files: self.batch_files.clone(),
            dir,
            text: self.batch_text,
            images: self.batch_images,
            compress: self.batch_compress,
            protect: self.batch_protect,
            flatten: self.batch_flatten,
            protect_options: self.protect.clone(),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || job.run(&tx));
        self.batch_rx = Some(rx);
        self.batch_running = true;
        self.batch_report.clear();
    }

    /// Consomme les lignes arrivées du fil de travail.
    fn pump_batch(&mut self) {
        let Some(rx) = self.batch_rx.as_ref() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(line) => {
                    self.batch_report.push_str(&line);
                    self.batch_report.push('\n');
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.batch_rx = None;
                    self.batch_running = false;
                    break;
                }
            }
        }
    }

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
        let mut pending_measure: Option<(usize, [f32; 2], [f32; 2])> = None;
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

                // Une seule page rastérisée par trame, et seulement une fois
                // le zoom au repos : la page courante d'abord, puis les
                // autres pages visibles au fil des trames suivantes. La
                // comparaison porte sur le palier *manquant ou trop grossier*,
                // d'où `needs_render` sans négation.
                let pending_render: Option<(u32, i32, f32)> = if zoom_settled {
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
                            &ctx, renderer, tab, page, step, scale, ppp, wanted, status,
                        ) {
                            ui.painter().image(
                                tex.id(),
                                page_rect,
                                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                                Color32::WHITE,
                            );
                        }

                        // Sélection de texte en cours.
                        if let Some(s) = sel.as_ref() {
                            if s.page == page {
                                if let Some(frags) = tab.fragments.get(&page) {
                                    for index in selected_indices(frags, s.span) {
                                        let [x1, y1, x2, y2] = frags[index].rect;
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
                            ui.painter().rect_filled(
                                rect,
                                Rounding::ZERO,
                                Color32::from_black_alpha(190),
                            );
                            ui.painter().rect_stroke(
                                rect,
                                Rounding::ZERO,
                                Stroke::new(1.5_f32, pal().redact_red),
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
                                    (block.height().max(d.fragment.font_size * scale) * 1.3)
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
                    if select_text_tool {
                        if resp.drag_started() {
                            if !tab.fragments.contains_key(&page) {
                                extract_page_fragments(tab, page);
                            }
                            if let Some(point) = resp.interact_pointer_pos() {
                                let point = to_pdf(point);
                                sel = Some(TextSel {
                                    page,
                                    span: Some((point, point)),
                                });
                            }
                        } else if resp.dragged() {
                            if let (Some(s), Some(point)) =
                                (sel.as_mut(), resp.interact_pointer_pos())
                            {
                                if s.page == page {
                                    if let Some((_, head)) = s.span.as_mut() {
                                        *head = to_pdf(point);
                                    }
                                }
                            }
                        }
                        if resp.clicked() {
                            sel = None;
                        }
                        let on_page = sel.map(|s| s.page == page).unwrap_or(false);
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
                                        Stroke::new(1.5_f32, pal().accent),
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

        self.edit_text = draft;
        self.text_sel = sel;
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
                    if forget_resp.hovered() { "✕" } else { "" },
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
                                let mark = if choice == layout { "✓" } else { "" };
                                if menu_row(ui, choice.label(), mark) {
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
                                if menu_row(ui, label, if selected { "✓" } else { "" }) {
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
        // Une saisie en cours (recherche, nom de champ, remplacement) a la
        // priorité : sinon Retour arrière efface l'annotation sélectionnée au
        // lieu d'un caractère.
        let typing = ctx.wants_keyboard_input();
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
        self.title_bar(ctx);
        self.rail(ctx);
        self.side_panel(ctx);
        self.status_bar(ctx);
        CentralPanel::default()
            .frame(Frame::none().fill(self.canvas_background()))
            .show(ctx, |ui| {
                if self.home_open || self.tabs.is_empty() {
                    self.empty_state(ui);
                } else {
                    self.canvas(ui);
                }
            });
        self.toolbar(ctx);
        self.properties_window(ctx);
        self.close_confirm_window(ctx);
        self.export_window(ctx);
        self.watermark_window(ctx);
        self.header_footer_window(ctx);
        self.compress_window(ctx);
        self.protect_window(ctx);
        self.crop_window(ctx);
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
        self.organize_window(ctx);
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

/// Police d'icônes Phosphor (MIT) ajoutée à la famille proportionnelle.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
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

/// Ligne de menu `Outils` : libellé à gauche, raccourci à droite.
fn menu_row(ui: &mut egui::Ui, label: &str, shortcut: &str) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(210.0, 26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, Rounding::same(5.0), pal().hover_bg);
    }
    ui.painter().text(
        pos2(rect.min.x + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        FontId::proportional(12.5),
        pal().icon_bright,
    );
    ui.painter().text(
        pos2(rect.max.x - 10.0, rect.center().y),
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
struct BatchJob {
    files: Vec<PathBuf>,
    dir: PathBuf,
    text: bool,
    images: bool,
    compress: bool,
    protect: bool,
    flatten: bool,
    protect_options: EncryptionOptions,
}

impl BatchJob {
    /// Traite chaque fichier et envoie une ligne par fichier, puis un bilan.
    ///
    /// Tourne dans un fil de travail : `Document` n'est pas `Send`, il est donc
    /// créé et détruit ici, et seules des `String` repartent vers l'interface.
    fn run(self, tx: &std::sync::mpsc::Sender<String>) {
        let mut done = 0usize;
        let mut failed = 0usize;
        for path in &self.files {
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "document".to_owned());
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let mut doc = match Document::open(path) {
                Ok(doc) => doc,
                Err(err) => {
                    failed += 1;
                    let _ = tx.send(format!("{name}: open failed — {err}"));
                    continue;
                }
            };
            let mut errors: Vec<String> = Vec::new();
            if self.text {
                let out = self.dir.join(format!("{stem}.txt"));
                match doc.export_text(TextFormat::Plain) {
                    Ok(body) => {
                        if let Err(err) = std::fs::write(&out, body) {
                            errors.push(format!("text: {err}"));
                        }
                    }
                    Err(err) => errors.push(format!("text: {err}")),
                }
            }
            if self.images {
                let out = self.dir.join(format!("{stem}-pages"));
                match std::fs::create_dir_all(&out) {
                    Ok(()) => {
                        if let Err(err) = doc.export_images(&out, ImageFormat::Png, 150.0) {
                            errors.push(format!("images: {err}"));
                        }
                    }
                    Err(err) => errors.push(format!("images: {err}")),
                }
            }
            if self.compress {
                let out = self.dir.join(format!("{stem}-compressed.pdf"));
                if let Err(err) = doc.compress(&out) {
                    errors.push(format!("compress: {err}"));
                }
            }
            if self.protect {
                let out = self.dir.join(format!("{stem}-protected.pdf"));
                if let Err(err) = doc.save_protected(&out, &self.protect_options) {
                    errors.push(format!("protect: {err}"));
                }
            }
            if self.flatten {
                let out = self.dir.join(format!("{stem}-flat.pdf"));
                match doc.flatten_annotations() {
                    Ok(_) => {
                        if let Err(err) = doc.save(&out) {
                            errors.push(format!("flatten: {err}"));
                        }
                    }
                    Err(err) => errors.push(format!("flatten: {err}")),
                }
            }
            if errors.is_empty() {
                done += 1;
                let _ = tx.send(format!("{name}: ok"));
            } else {
                failed += 1;
                let _ = tx.send(format!("{name}: {}", errors.join("; ")));
            }
        }
        let _ = tx.send(format!("\n{done} done, {failed} failed"));
    }
}

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

    /// Les formats usuels portent un nom ; les autres la taille brute.
    #[test]
    fn page_size_label_names_common_formats() {
        assert_eq!(page_size_label(595.0, 842.0), "A4 portrait");
        assert_eq!(page_size_label(842.0, 595.0), "A4 landscape");
        assert_eq!(page_size_label(612.0, 792.0), "Letter portrait");
        assert_eq!(page_size_label(1000.0, 500.0), "1000 × 500 pt");
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
            text: text.to_string(),
            rect,
            font: None,
            font_size,
            color: [0, 0, 0],
        }
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

        // Tout sélectionner parcourt de haut en bas puis de gauche à droite.
        let all = selected_indices(&frags, None);
        assert_eq!(all, vec![1, 0, 2]);
        assert_eq!(selection_text(&frags, None), "bonjour monde\nseconde");

        // Un glissé du premier bloc de la première ligne au second de la
        // troisième ligne couvre bien tout.
        let span = Some(([30.0, 88.0], [30.0, 66.0]));
        assert_eq!(selected_indices(&frags, span), vec![1, 0, 2]);

        // Un glissé qui s'arrête dans la première ligne s'y arrête.
        let span = Some(([30.0, 88.0], [30.0, 88.0]));
        assert_eq!(selected_indices(&frags, span), vec![1]);
        assert_eq!(selection_text(&frags, span), "bonjour");
    }

    /// Un point hors de tout bloc retombe sur le plus proche.
    #[test]
    fn selection_snaps_to_the_closest_block() {
        let frags = vec![
            frag("gauche", [10.0, 100.0, 60.0, 112.0], 12.0),
            frag("droite", [200.0, 100.0, 250.0, 112.0], 12.0),
        ];
        // Le point est à droite du second bloc, sous la ligne.
        let span = Some(([20.0, 106.0], [400.0, 40.0]));
        assert_eq!(selected_indices(&frags, span), vec![0, 1]);
        assert_eq!(selection_text(&frags, span), "gauche droite");
    }

    /// Deux blocs presque à la même hauteur restent sur une seule ligne.
    #[test]
    fn same_line_blocks_join_with_a_space() {
        let frags = vec![
            frag("a", [10.0, 100.0, 20.0, 112.0], 12.0),
            frag("b", [30.0, 101.0, 40.0, 113.0], 12.0),
        ];
        assert_eq!(selection_text(&frags, None), "a b");
    }

    /// Une sélection vide ou un index absent ne paniquent pas.
    #[test]
    fn empty_selection_is_empty() {
        assert!(selected_indices(&[], None).is_empty());
        assert_eq!(selection_text(&[], None), "");
        // Bloc unique : un point très loin retombe dessus.
        let frags = vec![frag("seul", [0.0, 0.0, 10.0, 10.0], 10.0)];
        let span = Some(([900.0, 900.0], [-50.0, -50.0]));
        assert_eq!(selected_indices(&frags, span), vec![0]);
    }

    /// Des corps de police différents sur des lignes voisines ne doivent pas
    /// mélanger l'ordre de lecture (comparateur non transitif d'autrefois).
    #[test]
    fn reading_order_survives_mixed_font_sizes() {
        let frags = vec![
            frag("corps", [10.0, 100.0, 60.0, 112.0], 12.0),
            frag("titre", [10.0, 118.0, 70.0, 134.0], 16.0),
            frag("bas", [10.0, 88.0, 55.0, 100.0], 12.0),
        ];
        assert_eq!(selection_text(&frags, None), "titre\ncorps\nbas");
    }

    /// Deux corps différents sur une même ligne y restent.
    #[test]
    fn reading_order_keeps_one_line_together() {
        let frags = vec![
            frag("grand", [40.0, 98.0, 90.0, 118.0], 20.0),
            frag("petit", [10.0, 100.0, 35.0, 112.0], 12.0),
        ];
        assert_eq!(selection_text(&frags, None), "petit grand");
    }
}
