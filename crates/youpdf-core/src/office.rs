//! Export PDF → Word (`.docx`), Excel (`.xlsx`) et PowerPoint (`.pptx`).
//!
//! Les parties OOXML (Office Open XML) sont écrites à la main puis
//! empaquetées avec la crate `zip`. Aucune dépendance `docx-rs` : chaque
//! format reste autonome et la surface de dépendances est minimale.
//!
//! ## Ce que l'export transporte
//!
//! Deux mises en page sont proposées, choisies par [`OfficeLayout`] :
//!
//! - **Flow** : le texte est reconstruit en paragraphes éditables. Word
//!   regroupe les lignes en paragraphes et applique les styles de titre,
//!   Excel pose une ligne de tableau par ligne de texte, PowerPoint empile
//!   les paragraphes dans des zones de texte.
//! - **Exact** : chaque fragment garde sa position mesurée sur la page.
//!   Word écrit des cadres ancrés sur la page, Excel reconstruit un tableau
//!   à partir de l'alignement des fragments, PowerPoint pose un objet par
//!   fragment.
//!
//! Les images sont toujours embarquées (`word/media/`, `ppt/media/`) ; en
//! mode Exact, elles sont replacées à leur position d'origine.

use std::io::Write;
use std::path::Path;

use lopdf::{Dictionary, Object};
use youpdf_shared::{Error, Result};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::document::Document;
use crate::util::escape_xml;

/// Format Office proposé à l'export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfficeFormat {
    /// Word (`.docx`).
    Word,
    /// Excel (`.xlsx`).
    Excel,
    /// PowerPoint (`.pptx`).
    PowerPoint,
}

impl OfficeFormat {
    /// Nom affiché dans l'interface.
    pub fn display_name(&self) -> &'static str {
        match self {
            OfficeFormat::Word => "Word (.docx)",
            OfficeFormat::Excel => "Excel (.xlsx)",
            OfficeFormat::PowerPoint => "PowerPoint (.pptx)",
        }
    }

    /// Extension de fichier suggérée (sans point).
    pub fn extension(&self) -> &'static str {
        match self {
            OfficeFormat::Word => "docx",
            OfficeFormat::Excel => "xlsx",
            OfficeFormat::PowerPoint => "pptx",
        }
    }

    /// Tous les formats, dans l'ordre d'affichage.
    pub const ALL: [OfficeFormat; 3] = [
        OfficeFormat::Word,
        OfficeFormat::Excel,
        OfficeFormat::PowerPoint,
    ];
}

/// Mode de reconstruction de la page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OfficeLayout {
    /// Paragraphes fluides, reconstruits à partir des fragments.
    #[default]
    Flow,
    /// Chaque fragment est placé à ses coordonnées PDF mesurées.
    Exact,
}

/// Une partie du paquet OOXML : chemin dans l'archive, contenu XML.
type Part = (String, String);

/// Un fragment de texte, avec son style résolu en nom de police Office.
#[derive(Debug, Clone)]
struct OfficeRun {
    /// Texte du fragment, nettoyé des caractères interdits en XML.
    text: String,
    /// Boîte `[x1, y1, x2, y2]` en coordonnées PDF (origine en bas à gauche).
    rect: [f32; 4],
    /// Corps en points.
    font_size: f32,
    /// Couleur RGB.
    color: [u8; 3],
    /// Famille de police Office.
    font: &'static str,
    /// Gras.
    bold: bool,
    /// Italique.
    italic: bool,
}

/// Une image embarquée, avec sa boîte sur la page quand elle est connue.
struct OfficeImage {
    /// Octets PNG.
    png: Vec<u8>,
    /// Largeur en pixels.
    width: u32,
    /// Hauteur en pixels.
    height: u32,
    /// Boîte `[x1, y1, x2, y2]` sur la page, si la matrice est connue.
    rect: Option<[f32; 4]>,
}

/// Une page reconstruite : sa taille, son texte et ses images.
struct OfficePage {
    /// Largeur en points.
    width: f32,
    /// Hauteur en points.
    height: f32,
    /// Fragments de texte, dans l'ordre de lecture.
    runs: Vec<OfficeRun>,
    /// Images de la page.
    images: Vec<OfficeImage>,
}

/// Une ligne de texte : fragments alignés sur une même base.
#[derive(Clone)]
struct OfficeLine {
    /// Fragments de la ligne, de gauche à droite.
    runs: Vec<OfficeRun>,
    /// Bord supérieur de la ligne (coordonnées PDF).
    top: f32,
    /// Bord inférieur de la ligne (coordonnées PDF).
    bottom: f32,
    /// Corps le plus grand de la ligne.
    size: f32,
}

/// Un paragraphe : des lignes consécutives partageant le même style.
struct OfficeParagraph {
    /// Lignes du paragraphe, de haut en bas.
    lines: Vec<OfficeLine>,
    /// Niveau de titre (1, 2 ou 3) si le paragraphe est un titre.
    heading: Option<u8>,
}

impl Document {
    /// Exporte la couche texte du document vers un fichier Office.
    ///
    /// Les pages sans texte extractible (scan non océrisé) produisent une
    /// section vide plutôt qu'une erreur : un document mixte s'exporte
    /// quand même.
    pub fn export_office(&self, path: impl AsRef<Path>, format: OfficeFormat) -> Result<()> {
        self.export_office_layout(path, format, OfficeLayout::Flow)
    }

    /// Exporte vers Office en choisissant la mise en page.
    ///
    /// `Exact` repositionne chaque fragment à ses coordonnées mesurées ;
    /// `Flow` reconstruit un document éditable. Les deux modes transportent
    /// les images de la page.
    pub fn export_office_layout(
        &self,
        path: impl AsRef<Path>,
        format: OfficeFormat,
        layout: OfficeLayout,
    ) -> Result<()> {
        let path = path.as_ref();
        let (parts, media) = match format {
            OfficeFormat::Word => match layout {
                OfficeLayout::Flow => word_parts_flow(&self.office_model(), &self.office_text_lines()),
                OfficeLayout::Exact => word_parts_exact(&self.office_model()),
            },
            OfficeFormat::Excel => match layout {
                OfficeLayout::Flow => excel_parts_flow(&self.office_text_lines()),
                OfficeLayout::Exact => excel_parts_exact(&self.office_model()),
            },
            OfficeFormat::PowerPoint => powerpoint_parts(&self.office_model(), layout),
        };
        write_zip_with_media(path, &parts, &media)
    }

    /// Reconstitue chaque page : fragments stylés, images et taille.
    ///
    /// Le décodeur interne ne connaît pas les tables de largeur : la boîte
    /// d'un fragment reste indicative. Elle suffit à un placement « au plus
    /// proche », pas à une reproduction point près.
    fn office_model(&self) -> Vec<OfficePage> {
        let total = self.page_count() as u32;
        let mut pages = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let (width, height) = self.page_size(number);
            let mut runs = Vec::new();
            if let Ok(fragments) = self.extract_text_fragments(number) {
                for fragment in fragments {
                    let text = sanitize_xml(fragment.text.trim());
                    if text.is_empty() {
                        continue;
                    }
                    let (font, bold, italic) =
                        self.fragment_font(number, fragment.font.as_deref());
                    runs.push(OfficeRun {
                        text,
                        rect: fragment.rect,
                        font_size: fragment.font_size.max(1.0),
                        color: fragment.color,
                        font,
                        bold,
                        italic,
                    });
                }
            }
            // Ordre de lecture : du haut vers le bas, puis de gauche à droite.
            runs.sort_by(|a, b| {
                b.rect[3]
                    .partial_cmp(&a.rect[3])
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(
                        a.rect[0]
                            .partial_cmp(&b.rect[0])
                            .unwrap_or(std::cmp::Ordering::Equal),
                    )
            });

            let mut images = Vec::new();
            for info in self.extract_images(number).unwrap_or_default() {
                let Ok(png) = self.extract_image_png(&info) else {
                    continue;
                };
                images.push(OfficeImage {
                    png,
                    width: info.width,
                    height: info.height,
                    rect: self.image_rect(&info).ok().flatten(),
                });
            }

            pages.push(OfficePage {
                width,
                height,
                runs,
                images,
            });
        }
        pages
    }

    /// Lignes de texte brutes par page, pour les mises en page fluides.
    fn office_text_lines(&self) -> Vec<Vec<String>> {
        let total = self.page_count() as u32;
        let mut pages = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let text = self.page_text(number).unwrap_or_default();
            let text = text.replace('\u{c}', "\n");
            pages.push(text.split('\n').map(sanitize_xml).collect());
        }
        pages
    }

    /// Dictionnaire `/Font` des ressources d'une page.
    fn page_font_dict(&self, page: u32) -> Option<Dictionary> {
        let page_id = self.page_id(page).ok()?;
        let resources = self.page_resources(page_id).ok()?;
        match resources.get(b"Font") {
            Ok(Object::Dictionary(dict)) => Some(dict.clone()),
            Ok(Object::Reference(id)) => self.inner().get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    /// Nom de base (`/BaseFont`) de la police de ressource donnée.
    fn base_font_name(&self, page: u32, resource: &str) -> Option<String> {
        let fonts = self.page_font_dict(page)?;
        let object = fonts.get(resource.as_bytes()).ok()?;
        let id = object.as_reference().ok()?;
        let dict = self.inner().get_dictionary(id).ok()?;
        let base = dict.get(b"BaseFont").ok()?.as_name().ok()?;
        Some(String::from_utf8_lossy(base).into_owned())
    }

    /// Police Office correspondant à une police PDF, avec son gras et son
    /// italique.
    fn fragment_font(&self, page: u32, resource: Option<&str>) -> (&'static str, bool, bool) {
        let base = resource
            .and_then(|name| self.base_font_name(page, name))
            .unwrap_or_default();
        office_font(&base)
    }
}

/// Écrit une archive ZIP mêlant parties XML et parties binaires.
fn write_zip_with_media(
    path: &Path,
    parts: &[Part],
    media: &[(String, Vec<u8>)],
) -> Result<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in parts.iter() {
        zip.start_file(name.as_str(), options)
            .map_err(|e| Error::Pdf(format!("écriture de {name} : {e}")))?;
        zip.write_all(content.as_bytes())?;
    }
    for (name, content) in media.iter() {
        zip.start_file(name.as_str(), options)
            .map_err(|e| Error::Pdf(format!("écriture de {name} : {e}")))?;
        zip.write_all(content)?;
    }
    zip.finish()
        .map_err(|e| Error::Pdf(format!("finalisation de l'archive Office : {e}")))?;
    Ok(())
}

/// Police Office correspondant à un nom de base PDF, avec son gras et son
/// italique.
///
/// Le nom PDF est une sous-chaîne : `ABCDEF+Helvetica-Bold` doit donner
/// « Arial » en gras. Le repli sur Arial évite un document qui s'ouvre avec
/// une police de substitution.
fn office_font(base: &str) -> (&'static str, bool, bool) {
    let lower = base.to_ascii_lowercase();
    let bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
    let italic = lower.contains("italic")
        || lower.contains("oblique")
        || lower.contains("italicized");
    let family = if lower.contains("times") || lower.contains("serif") || lower.contains("roman") {
        "Times New Roman"
    } else if lower.contains("courier") || lower.contains("mono") {
        "Courier New"
    } else if lower.contains("helvetica")
        || lower.contains("arial")
        || lower.contains("calibri")
        || lower.contains("sans")
    {
        "Arial"
    } else {
        "Arial"
    };
    (family, bold, italic)
}

/// Échelle une valeur vers un entier, sans jamais descendre sous `floor`.
fn scale_to_i64(value: f32, scale: f32, floor: i64) -> i64 {
    if !value.is_finite() {
        return floor;
    }
    let scaled = (value * scale).round();
    if scaled < floor as f32 {
        floor
    } else {
        scaled as i64
    }
}

/// Couleur RGB en hexadécimal majuscule, pour `w:color` et `a:srgbClr`.
fn hex_color(color: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

// ---------------------------------------------------------------------------
// Reconstruction du texte : lignes, paragraphes, titres
// ---------------------------------------------------------------------------

/// Regroupe les fragments en lignes de texte.
///
/// Deux fragments partagent une ligne quand leurs bases sont proches : la
/// tolérance suit le corps, car c'est lui qui donne l'échelle verticale du
/// texte. Le bord inférieur (`rect[1]`) sert de ligne de base : c'est
/// ainsi que le décodeur interne construit ses boîtes.
fn group_lines(runs: &[OfficeRun]) -> Vec<OfficeLine> {
    let mut lines: Vec<OfficeLine> = Vec::new();
    for run in runs {
        let baseline = run.rect[1];
        // Une ligne existe déjà si une base est à moins d'un demi-corps.
        let found = lines.iter_mut().find(|line| {
            (line.bottom - baseline).abs() <= line.size.max(run.font_size) * 0.5
        });
        match found {
            Some(line) => {
                line.top = line.top.max(run.rect[3]);
                line.bottom = line.bottom.min(run.rect[1]);
                line.size = line.size.max(run.font_size);
                line.runs.push(run.clone());
            }
            None => lines.push(OfficeLine {
                runs: vec![run.clone()],
                top: run.rect[3],
                bottom: run.rect[1],
                size: run.font_size,
            }),
        }
    }
    // Ordre de lecture : de haut en bas, puis de gauche à droite.
    for line in lines.iter_mut() {
        line.runs.sort_by(|a, b| {
            a.rect[0]
                .partial_cmp(&b.rect[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    lines.sort_by(|a, b| {
        b.top.partial_cmp(&a.top).unwrap_or(std::cmp::Ordering::Equal)
    });
    lines
}

/// Corps de texte dominant d'un ensemble de lignes.
///
/// Le mode est préféré à la médiane : un document est surtout composé de
/// son corps, et une médiane sur deux lignes titrerait à tort le texte
/// plus petit. À égalité, le plus petit corps gagne — c'est lui qui
/// ressemble à du corps.
fn body_size(lines: &[OfficeLine]) -> f32 {
    let mut sizes: Vec<u32> = lines.iter().map(|line| (line.size * 10.0) as u32).collect();
    if sizes.is_empty() {
        return 11.0;
    }
    sizes.sort_unstable();
    let mut best = sizes[0];
    let mut best_count = 0usize;
    let mut current = sizes[0];
    let mut count = 0usize;
    for size in sizes {
        if size == current {
            count += 1;
        } else {
            if count > best_count {
                best = current;
                best_count = count;
            }
            current = size;
            count = 1;
        }
    }
    if count > best_count {
        best = current;
    }
    (best.max(10) as f32) / 10.0
}

/// Regroupe les lignes en paragraphes et repère les titres.
///
/// Un paragraphe change quand l'écart vertical entre deux lignes dépasse
/// environ une fois et demie le corps : en dessous, c'est un simple retour
/// à la ligne dans le même paragraphe.
fn group_paragraphs(lines: &[OfficeLine], body: f32) -> Vec<OfficeParagraph> {
    let mut paragraphs: Vec<OfficeParagraph> = Vec::new();
    for line in lines {
        let starts_new = match paragraphs.last() {
            None => true,
            Some(previous) => {
                let last = previous.lines.last();
                let gap = match last {
                    Some(last) => last.bottom - line.top,
                    None => 0.0,
                };
                gap > body.max(line.size) * 1.6
            }
        };
        if starts_new {
            paragraphs.push(OfficeParagraph {
                lines: vec![line.clone()],
                heading: None,
            });
        } else if let Some(previous) = paragraphs.last_mut() {
            previous.lines.push(line.clone());
        }
    }

    // Titre : la ligne dépasse nettement le corps du document.
    for paragraph in paragraphs.iter_mut() {
        let size = paragraph
            .lines
            .iter()
            .map(|line| line.size)
            .fold(0.0f32, f32::max);
        if body <= 0.0 {
            continue;
        }
        let ratio = size / body;
        paragraph.heading = if ratio >= 1.6 {
            Some(1)
        } else if ratio >= 1.3 {
            Some(2)
        } else if ratio >= 1.15 {
            Some(3)
        } else {
            None
        };
    }
    paragraphs
}

// ---------------------------------------------------------------------------
// Word (.docx)
// ---------------------------------------------------------------------------

/// Assemble les parties du paquet Word en mode « flux ».
///
/// Les images sont ajoutées à la fin de chaque page : en flux, elles ne
/// peuvent pas être placées au milieu du texte sans briser la lecture.
fn word_parts_flow(pages: &[OfficePage], text_pages: &[Vec<String>]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
    let (media, placements) = word_media(pages);
    let count = media.len();

    let mut body = String::new();
    for (index, page) in pages.iter().enumerate() {
        if index > 0 {
            body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
        }
        body.push_str(&word_flow_body(page, text_pages.get(index)));
        // Images : à la fin de la page, dans l'ordre d'embarquement.
        for placement in placements.get(index).into_iter().flatten() {
            body.push_str(&word_picture_paragraph(
                placement.index,
                placement.width,
                placement.height,
            ));
        }
    }

    let parts = vec![
        (
            "[Content_Types].xml".to_string(),
            word_content_types(count > 0),
        ),
        ("_rels/.rels".to_string(), root_rels("word/document.xml")),
        ("word/document.xml".to_string(), word_document(&body)),
        ("word/styles.xml".to_string(), word_styles()),
        (
            "word/_rels/document.xml.rels".to_string(),
            document_rels(count),
        ),
    ];
    (parts, media)
}

/// Corps du document Word en mode « flux ».
///
/// Les fragments sont regroupés en lignes puis en paragraphes : un
/// paragraphe contient un `<w:r>` par ligne, séparés par un `<w:br/>`. Un
/// titre reçoit un style `Heading1`…`Heading3`, défini dans
/// `word/styles.xml`.
fn word_flow_body(page: &OfficePage, fallback: Option<&Vec<String>>) -> String {
    let lines = group_lines(&page.runs);
    let mut body = String::new();

    if lines.is_empty() {
        // Aucun fragment exploitable : on retombe sur le texte brut, qui
        // au moins garde l'ordre de lecture.
        for line in fallback.into_iter().flatten() {
            if line.is_empty() {
                body.push_str("<w:p/>");
                continue;
            }
            body.push_str(&format!(
                r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                escape_xml(line)
            ));
        }
        if body.is_empty() {
            body.push_str("<w:p/>");
        }
        return body;
    }

    let body_size = body_size(&lines);
    for paragraph in group_paragraphs(&lines, body_size) {
        body.push_str("<w:p>");
        if let Some(level) = paragraph.heading {
            body.push_str(&format!(r#"<w:pPr><w:pStyle w:val="Heading{level}"/></w:pPr>"#));
        }
        for (index, line) in paragraph.lines.iter().enumerate() {
            if index > 0 {
                body.push_str("<w:r><w:br/></w:r>");
            }
            for run in &line.runs {
                body.push_str(&word_run(run));
            }
        }
        body.push_str("</w:p>");
    }
    body
}

/// Un run Word : style du texte (police, corps, couleur, gras, italique).
fn word_run(run: &OfficeRun) -> String {
    let half_points = scale_to_i64(run.font_size, 2.0, 2);
    format!(
        r#"<w:r><w:rPr><w:rFonts w:ascii="{font}" w:hAnsi="{font}" w:cs="{font}"/>{bold}{italic}<w:color w:val="{color}"/><w:sz w:val="{half_points}"/><w:szCs w:val="{half_points}"/></w:rPr><w:t xml:space="preserve">{text}</w:t></w:r>"#,
        font = run.font,
        bold = if run.bold { "<w:b/>" } else { "" },
        italic = if run.italic { "<w:i/>" } else { "" },
        color = hex_color(run.color),
        half_points = half_points,
        text = escape_xml(&run.text),
    )
}

/// Embarque les images des pages et renvoie, pour chaque page, la liste des
/// images posées (indice global, largeur, hauteur).
fn word_media(pages: &[OfficePage]) -> (Vec<(String, Vec<u8>)>, Vec<Vec<WordImage>>) {
    let mut media = Vec::new();
    let mut placements: Vec<Vec<WordImage>> = Vec::new();
    let mut index = 0usize;
    for page in pages {
        let mut placed = Vec::new();
        for image in &page.images {
            index += 1;
            media.push((format!("word/media/image{index}.png"), image.png.clone()));
            placed.push(WordImage {
                index,
                width: image.width,
                height: image.height,
                rect: image.rect,
            });
        }
        placements.push(placed);
    }
    (media, placements)
}

/// Une image Word : son indice global et sa taille.
struct WordImage {
    /// Indice global (1-based), qui donne son `rId`.
    index: usize,
    /// Largeur en pixels.
    width: u32,
    /// Hauteur en pixels.
    height: u32,
    /// Boîte sur la page, si elle est connue.
    rect: Option<[f32; 4]>,
}

/// Assemble les parties du paquet Word en mode « mise en page exacte ».
///
/// Chaque fragment devient un paragraphe porteur d'un `w:framePr` : c'est
/// le mécanisme standard de Word pour un texte positionné par rapport à la
/// page. On ne passe pas par `w:txbxContent` (zone de texte VML/DrawingML) :
/// cette forme est bien plus fragile et Word la réécrit à l'ouverture.
fn word_parts_exact(pages: &[OfficePage]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
    let (media, placements) = word_media(pages);
    let count = media.len();

    let mut body = String::new();
    for (index, page) in pages.iter().enumerate() {
        if index > 0 {
            body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
        }
        for run in &page.runs {
            body.push_str(&word_frame_paragraph(run, page.height));
        }
        // Images : à leur position mesurée, dans un cadre ancré sur la page.
        for image in placements.get(index).into_iter().flatten() {
            body.push_str(&word_positioned_picture(image, page.height));
        }
        if page.runs.is_empty() && page.images.is_empty() {
            body.push_str("<w:p/>");
        }
    }

    let parts = vec![
        (
            "[Content_Types].xml".to_string(),
            word_content_types(count > 0),
        ),
        ("_rels/.rels".to_string(), root_rels("word/document.xml")),
        ("word/document.xml".to_string(), word_document(&body)),
        ("word/styles.xml".to_string(), word_styles()),
        (
            "word/_rels/document.xml.rels".to_string(),
            document_rels(count),
        ),
    ];
    (parts, media)
}

/// Paragraphe Word positionné par un cadre ancré sur la page.
///
/// Le repère PDF a son origine en bas à gauche ; Word ancre ses cadres
/// depuis le haut. D'où la conversion de `y`.
fn word_frame_paragraph(run: &OfficeRun, page_height: f32) -> String {
    let x = scale_to_i64(run.rect[0], 20.0, 0);
    let y = scale_to_i64(page_height - run.rect[3], 20.0, 0);
    let width = scale_to_i64(run.rect[2] - run.rect[0], 20.0, 1);
    let height = scale_to_i64(run.rect[3] - run.rect[1], 20.0, 1);
    format!(
        r#"<w:p><w:pPr><w:framePr w:w="{width}" w:h="{height}" w:hAnchor="page" w:vAnchor="page" w:x="{x}" w:y="{y}" w:wrap="none" w:hRule="exact"/></w:pPr>{run}</w:p>"#,
        run = word_run(run),
    )
}

/// Image Word positionnée par un cadre ancré sur la page.
fn word_positioned_picture(image: &WordImage, page_height: f32) -> String {
    const EMU_PER_PIXEL: u64 = 9_525;
    let natural_cx = (image.width.max(1) as u64) * EMU_PER_PIXEL;
    let natural_cy = (image.height.max(1) as u64) * EMU_PER_PIXEL;

    // La boîte posée sur la page est la référence ; à défaut, la taille
    // naturelle de l'image sert de boîte.
    let (x_pt, y_top_pt, w_pt, h_pt) = match image.rect {
        Some([x1, y1, x2, y2]) => (x1, y2, (x2 - x1).max(1.0), (y2 - y1).max(1.0)),
        None => (0.0, page_height, 0.0, 0.0),
    };
    // La taille affichée est celle de la boîte ; on la convertit en EMU
    // pour le dessin, mais on garde le ratio de l'image.
    let cx = if w_pt > 0.0 {
        (w_pt * 12_700.0).round().max(1.0) as u64
    } else {
        natural_cx
    };
    let cy = if h_pt > 0.0 {
        (h_pt * 12_700.0).round().max(1.0) as u64
    } else {
        natural_cy
    };

    let x = scale_to_i64(x_pt, 20.0, 0);
    let y = scale_to_i64(page_height - y_top_pt, 20.0, 0);
    let frame_w = scale_to_i64(w_pt, 20.0, 1);
    let frame_h = scale_to_i64(h_pt, 20.0, 1);

    format!(
        r#"<w:p><w:pPr><w:framePr w:w="{frame_w}" w:h="{frame_h}" w:hAnchor="page" w:vAnchor="page" w:x="{x}" w:y="{y}" w:wrap="none"/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{id}" name="Picture {id}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{id}" name="image{id}.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId{rel}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
        id = image.index,
        rel = image_rel_id(image.index),
    )
}

/// `word/document.xml` : le corps déjà construit, plus la section.
fn word_document(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1417" w:bottom="1417" w:left="1417"/></w:sectPr></w:body></w:document>"#
    )
}

/// `word/styles.xml` : style `Normal` et les trois niveaux de titre.
///
/// Les titres sont définis explicitement plutôt que laissés au modèle par
/// défaut : Word n'appliquerait alors aucune taille, et la hiérarchie
/// reconstruite serait invisible.
fn word_styles() -> String {
    let mut styles = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style>"#,
    );
    for (level, size) in [(1u8, 36u32), (2, 30), (3, 26)] {
        styles.push_str(&format!(
            r#"<w:style w:type="paragraph" w:styleId="Heading{level}"><w:name w:val="heading {level}"/><w:basedOn w:val="Normal"/><w:next w:val="Normal"/><w:qFormat/><w:pPr><w:outlineLvl w:val="{outline}"/><w:spacing w:before="240" w:after="120"/></w:pPr><w:rPr><w:b/><w:sz w:val="{size}"/><w:szCs w:val="{size}"/></w:rPr></w:style>"#,
            level = level,
            outline = level - 1,
            size = size,
        ));
    }
    styles.push_str("</w:styles>");
    styles
}

/// Relation `rId` de la n-ième image (les styles occupent `rId1`).
fn image_rel_id(index: usize) -> usize {
    index + 9
}

/// `word/_rels/document.xml.rels` : styles, puis une relation par image.
fn document_rels(images: usize) -> String {
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#,
    );
    for index in 1..=images {
        rels.push_str(&format!(
            r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image{index}.png"/>"#,
            image_rel_id(index)
        ));
    }
    rels.push_str("</Relationships>");
    rels
}

/// Paragraphe contenant une image en ligne, à six pouces de large au plus.
fn word_picture_paragraph(index: usize, width: u32, height: u32) -> String {
    const EMU_PER_PIXEL: u64 = 9_525;
    const MAX_WIDTH_EMU: u64 = 5_486_400; // 6 pouces
    let natural = (width.max(1) as u64) * EMU_PER_PIXEL;
    let scale = (MAX_WIDTH_EMU as f64 / natural as f64).min(1.0);
    let cx = (natural as f64 * scale) as u64;
    let cy = ((height.max(1) as u64) * EMU_PER_PIXEL) as f64 * scale;
    let cy = cy as u64;
    format!(
        r#"<w:p><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{cx}" cy="{cy}"/><wp:docPr id="{index}" name="Picture {index}"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:nvPicPr><pic:cNvPr id="{index}" name="image{index}.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId{rel}"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
        rel = image_rel_id(index)
    )
}

/// `[Content_Types].xml` d'un document Word.
fn word_content_types(images: bool) -> String {
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    if images {
        types.push_str(r#"<Default Extension="png" ContentType="image/png"/>"#);
    }
    types.push_str(r#"<Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#);
    types
}

// ---------------------------------------------------------------------------
// Excel (.xlsx)
// ---------------------------------------------------------------------------

/// Le contenu d'une cellule, avec son type déduit du texte.
#[derive(Clone)]
enum CellValue {
    /// Nombre, écrit sans attribut de type.
    Number(f64),
    /// Date, écrite en numéro de série Excel avec un format de date.
    Date(f64),
    /// Booléen.
    Boolean(bool),
    /// Texte, écrit comme chaîne partagée.
    Text(String),
}

/// Assemble les parties du paquet Excel en mode « flux » : une ligne par
/// ligne de texte, tout en colonne A.
fn excel_parts_flow(pages: &[Vec<String>]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
    let mut rows = String::new();
    let mut shared = String::new();
    let mut shared_count = 0usize;
    let mut row_index = 0usize;

    for page in pages {
        for line in page {
            row_index += 1;
            if line.is_empty() {
                rows.push_str(&format!("<row r=\"{row_index}\"/>"));
                continue;
            }
            rows.push_str(&format!(
                "<row r=\"{row_index}\"><c r=\"A{row_index}\" t=\"s\"><v>{shared_count}</v></c></row>"
            ));
            shared.push_str(r#"<si><t xml:space="preserve">"#);
            shared.push_str(&escape_xml(line));
            shared.push_str("</t></si>");
            shared_count += 1;
        }
    }

    let parts = vec![
        ("[Content_Types].xml".to_string(), excel_content_types(false)),
        ("_rels/.rels".to_string(), root_rels("xl/workbook.xml")),
        ("xl/workbook.xml".to_string(), excel_workbook()),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            excel_workbook_rels(false),
        ),
        ("xl/worksheets/sheet1.xml".to_string(), excel_sheet(&rows, "", "")),
        (
            "xl/sharedStrings.xml".to_string(),
            excel_shared_strings(&shared, shared_count),
        ),
    ];
    (parts, Vec::new())
}

/// Assemble les parties du paquet Excel en mode « mise en page exacte ».
///
/// Les colonnes sont détectées par la position des fragments, les lignes par
/// leur base. Les cellules reçoivent un type : nombre, date ou texte.
fn excel_parts_exact(pages: &[OfficePage]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
    let mut rows = String::new();
    let mut shared = String::new();
    let mut shared_count = 0usize;
    let mut row_index = 0usize;

    // Largeurs de colonne, en caractères Excel : la plus longue cellule de
    // la colonne, plus deux caractères de marge.
    let mut widths: Vec<usize> = Vec::new();
    // Vrai si la première ligne ressemble à un en-tête.
    let mut frozen = false;

    for page in pages {
        let lines = group_lines(&page.runs);
        if lines.is_empty() {
            continue;
        }
        // Frontières de colonnes : les abscisses de départ des fragments,
        // regroupées quand elles sont trop proches.
        let boundaries = excel_column_boundaries(&lines);
        if boundaries.is_empty() {
            continue;
        }
        let threshold = excel_column_threshold(&lines);

        let mut page_rows: Vec<Vec<Option<CellValue>>> = Vec::new();
        for line in &lines {
            let mut cells: Vec<Option<CellValue>> = vec![None; boundaries.len()];
            for run in &line.runs {
                let column = excel_column_of(&boundaries, run.rect[0], threshold);
                let value = excel_cell_value(&run.text);
                let slot = &mut cells[column];
                // Deux fragments dans la même cellule : ils sont concaténés.
                *slot = Some(match slot.take() {
                    None => value,
                    Some(previous) => merge_cells(previous, value),
                });
            }
            page_rows.push(cells);
        }

        // Un en-tête : la première ligne est entièrement textuelle et les
        // lignes suivantes contiennent des nombres.
        if !frozen && page_rows.len() > 1 {
            let header_text = page_rows[0]
                .iter()
                .all(|cell| matches!(cell, Some(CellValue::Text(_)) | None))
                && page_rows[0].iter().any(|cell| matches!(cell, Some(CellValue::Text(_))));
            let body_numeric = page_rows[1..]
                .iter()
                .any(|row| row.iter().any(|cell| matches!(cell, Some(CellValue::Number(_)))));
            frozen = header_text && body_numeric;
        }

        for cells in page_rows {
            row_index += 1;
            rows.push_str(&format!("<row r=\"{row_index}\">"));
            for (column, cell) in cells.iter().enumerate() {
                let Some(cell) = cell else { continue };
                let reference = format!("{}{}", column_letters(column), row_index);
                match cell {
                    CellValue::Number(value) => {
                        rows.push_str(&format!(
                            "<c r=\"{reference}\"><v>{}</v></c>",
                            excel_number(*value)
                        ));
                    }
                    CellValue::Date(value) => {
                        rows.push_str(&format!(
                            "<c r=\"{reference}\" s=\"1\"><v>{}</v></c>",
                            excel_number(*value)
                        ));
                    }
                    CellValue::Boolean(value) => {
                        rows.push_str(&format!(
                            "<c r=\"{reference}\" t=\"b\"><v>{}</v></c>",
                            if *value { 1 } else { 0 }
                        ));
                    }
                    CellValue::Text(text) => {
                        rows.push_str(&format!(
                            "<c r=\"{reference}\" t=\"s\"><v>{shared_count}</v></c>"
                        ));
                        shared.push_str(r#"<si><t xml:space="preserve">"#);
                        shared.push_str(&escape_xml(text));
                        shared.push_str("</t></si>");
                        shared_count += 1;
                        // La largeur se mesure sur le texte affiché.
                        let width = text.chars().count() + 2;
                        while widths.len() <= column {
                            widths.push(0);
                        }
                        widths[column] = widths[column].max(width);
                    }
                }
            }
            rows.push_str("</row>");
        }
    }

    // Les largeurs sont posées après coup : on connaît la cellule la plus
    // large de chaque colonne seulement à la fin de la lecture.
    let cols = excel_cols(&widths);
    let views = if frozen {
        r#"<sheetViews><sheetView workbookViewId="0"><pane ySplit="1" topLeftCell="A2" activePane="bottomLeft" state="frozen"/></sheetView></sheetViews>"#
            .to_string()
    } else {
        String::new()
    };

    let parts = vec![
        (
            "[Content_Types].xml".to_string(),
            excel_content_types(true),
        ),
        ("_rels/.rels".to_string(), root_rels("xl/workbook.xml")),
        ("xl/workbook.xml".to_string(), excel_workbook()),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            excel_workbook_rels(true),
        ),
        (
            "xl/worksheets/sheet1.xml".to_string(),
            excel_sheet(&rows, &cols, &views),
        ),
        (
            "xl/sharedStrings.xml".to_string(),
            excel_shared_strings(&shared, shared_count),
        ),
        ("xl/styles.xml".to_string(), excel_styles()),
    ];
    (parts, Vec::new())
}

/// Frontières de colonnes : abscisses de départ des fragments, regroupées.
///
/// Une colonne commence quand une abscisse est séparée de toutes les
/// colonnes connues par plus que le seuil. Le regroupement se fait sur
/// toutes les lignes de la page : une colonne n'est repérée que si elle
/// existe partout, pas seulement sur une ligne.
fn excel_column_boundaries(lines: &[OfficeLine]) -> Vec<f32> {
    let mut starts: Vec<f32> = Vec::new();
    for line in lines {
        for run in &line.runs {
            starts.push(run.rect[0]);
        }
    }
    if starts.is_empty() {
        return Vec::new();
    }
    starts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let threshold = excel_column_threshold(lines);

    let mut boundaries: Vec<f32> = Vec::new();
    for start in starts {
        match boundaries.last() {
            Some(last) if (start - last).abs() <= threshold => {}
            _ => boundaries.push(start),
        }
    }
    boundaries
}

/// Écart minimal entre deux colonnes, déduit de la taille des glyphes.
///
/// Un demi-cadratin est la largeur typique d'un caractère : un écart
/// inférieur signifie que les deux abscisses désignent la même colonne.
fn excel_column_threshold(lines: &[OfficeLine]) -> f32 {
    let mut sizes: Vec<f32> = lines.iter().map(|line| line.size).collect();
    if sizes.is_empty() {
        return 6.0;
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sizes[sizes.len() / 2];
    (median * 0.6).max(1.0)
}

/// Indice de la colonne d'un fragment.
fn excel_column_of(boundaries: &[f32], x: f32, threshold: f32) -> usize {
    let mut best = 0usize;
    let mut best_distance = f32::INFINITY;
    for (index, boundary) in boundaries.iter().enumerate() {
        let distance = (x - boundary).abs();
        if distance < best_distance {
            best_distance = distance;
            best = index;
        }
    }
    // Un fragment très à droite de la dernière colonne appartient à cette
    // colonne-ci : la table n'a pas de colonne supplémentaire.
    if best_distance > threshold && !boundaries.is_empty() {
        return boundaries.len() - 1;
    }
    best
}

/// Déduit le type d'une cellule de son texte.
///
/// Un nombre est reconnu par sa seule grammaire (chiffres, séparateurs,
/// virgule décimale) : un texte qui contient des lettres reste du texte,
/// même s'il commence par un chiffre.
fn excel_cell_value(text: &str) -> CellValue {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return CellValue::Text(String::new());
    }
    if trimmed.eq_ignore_ascii_case("true") {
        return CellValue::Boolean(true);
    }
    if trimmed.eq_ignore_ascii_case("false") {
        return CellValue::Boolean(false);
    }
    if let Some(serial) = excel_date_serial(trimmed) {
        return CellValue::Date(serial);
    }
    if let Some(number) = excel_parse_number(trimmed) {
        return CellValue::Number(number);
    }
    CellValue::Text(trimmed.to_string())
}

/// Nombre reconnu dans un texte, ou rien.
///
/// Les séparateurs de milliers sont tolérés : `1 234,56` et `1,234.56`
/// valent `1234.56`. Un texte qui contient autre chose que des chiffres et
/// des séparateurs est refusé.
fn excel_parse_number(text: &str) -> Option<f64> {
    let mut body = text.trim();
    if body.is_empty() {
        return None;
    }
    let mut sign = String::new();
    if let Some(rest) = body.strip_prefix('-') {
        sign.push('-');
        body = rest;
    } else if let Some(rest) = body.strip_prefix('+') {
        body = rest;
    }

    // Le dernier séparateur rencontré est le séparateur décimal s'il est
    // suivi d'un ou deux chiffres : `1,234.56` et `1.234,56`.
    let chars: Vec<char> = body.chars().collect();
    let mut decimal_index: Option<usize> = None;
    for (index, c) in chars.iter().enumerate() {
        if matches!(c, '.' | ',' | ' ' | '\u{a0}' | '\'') {
            let rest: String = chars[index + 1..].iter().collect();
            let digits: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            if digits.len() == rest.chars().filter(|c| !c.is_whitespace()).count()
                && !digits.is_empty()
                && digits.len() <= 2
                && index + 1 < chars.len()
            {
                decimal_index = Some(index);
            }
        }
    }

    let mut integral = String::new();
    let mut fraction = String::new();
    for (index, c) in chars.iter().enumerate() {
        if !c.is_ascii_digit() {
            if matches!(c, '.' | ',' | ' ' | '\u{a0}' | '\'') {
                continue;
            }
            return None;
        }
        if Some(index) == decimal_index {
            continue;
        }
        if decimal_index.is_some() && index > decimal_index.unwrap_or(0) {
            fraction.push(*c);
        } else {
            integral.push(*c);
        }
    }
    if integral.is_empty() {
        return None;
    }
    let value = if fraction.is_empty() {
        integral.parse::<f64>().ok()?
    } else {
        format!("{integral}.{fraction}").parse::<f64>().ok()?
    };
    if !value.is_finite() {
        return None;
    }
    let signed = format!("{sign}{value}");
    signed.parse::<f64>().ok()
}

/// Numéro de série Excel d'une date, ou rien.
///
/// Les formats usuels sont reconnus : `AAAA-MM-JJ`, `JJ/MM/AAAA`,
/// `JJ-MM-AAAA`, `JJ.MM.AAAA`. Excel compte les jours depuis le
/// 30/12/1899 (le décalage historique de la bugue de 1900 est absorbé
/// dans cette base).
fn excel_date_serial(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    let parts: Vec<&str> = trimmed.split(['-', '/', '.']).collect();
    let (year, month, day) = match parts.as_slice() {
        [a, b, c] if a.len() == 4 => {
            let year: i32 = a.parse().ok()?;
            let month: i32 = b.parse().ok()?;
            let day: i32 = c.parse().ok()?;
            (year, month, day)
        }
        [a, b, c] => {
            let day: i32 = a.parse().ok()?;
            let month: i32 = b.parse().ok()?;
            let year: i32 = c.parse().ok()?;
            (year, month, day)
        }
        _ => return None,
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    if year < 1900 || year > 9999 {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    // La base d'Excel est le 30/12/1899.
    Some((days - days_from_civil(1899, 12, 30)?) as f64)
}

/// Nombre de jours depuis le 1er janvier 1970, en calendrier civil.
///
/// L'algorithme d'Hinnant donne un jour de la semaine pour n'importe quelle
/// date du calendrier grégorien, sans table de jours par mois.
fn days_from_civil(year: i32, month: i32, day: i32) -> Option<i64> {
    let year = year as i64;
    let month = month as i64;
    let day = day as i64;
    if !(1..=12).contains(&month) {
        return None;
    }
    let shifted = if month <= 2 { year - 1 } else { year };
    let era = if shifted >= 0 { shifted } else { shifted - 399 } / 400;
    let year_of_era = shifted - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// Concatène deux fragments tombant dans la même cellule.
fn merge_cells(first: CellValue, second: CellValue) -> CellValue {
    match (first, second) {
        (CellValue::Text(a), CellValue::Text(b)) => CellValue::Text(format!("{a} {b}")),
        (CellValue::Number(a), CellValue::Number(b)) => CellValue::Number(a + b),
        (a, _) => a,
    }
}

/// Nombre écrit dans une cellule : point décimal, pas de notation
/// scientifique (Excel ne l'accepte pas dans `<v>`).
fn excel_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let rounded = (value * 1_000_000.0).round() / 1_000_000.0;
    if rounded == rounded.trunc() {
        format!("{}", rounded as i64)
    } else {
        let mut text = format!("{rounded:.6}");
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        text
    }
}

/// Élément `<cols>` : largeur de chaque colonne, en caractères Excel.
fn excel_cols(widths: &[usize]) -> String {
    if widths.is_empty() {
        return String::new();
    }
    let mut out = String::from("<cols>");
    for (index, width) in widths.iter().enumerate() {
        let width = (*width).clamp(3, 255);
        out.push_str(&format!(
            r#"<col min="{min}" max="{max}" width="{width}" customWidth="1"/>"#,
            min = index + 1,
            max = index + 1,
        ));
    }
    out.push_str("</cols>");
    out
}

/// `xl/worksheets/sheet1.xml` : feuille avec ses colonnes, ses vues et ses
/// lignes.
fn excel_sheet(rows: &str, cols: &str, views: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{views}{cols}<sheetData>{rows}</sheetData></worksheet>"#
    )
}

/// `xl/sharedStrings.xml` : les chaînes partagées.
fn excel_shared_strings(strings: &str, count: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{count}" uniqueCount="{count}">{strings}</sst>"#
    )
}

/// `xl/styles.xml` : un format de date (`numFmtId="14"`), référencé par
/// l'index `1` des cellules de date.
fn excel_styles() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="14" formatCode="yyyy\-mm\-dd"/></numFmts><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs></styleSheet>"#
        .to_string()
}

/// Lettres d'une colonne Excel (0 → A, 26 → AA).
fn column_letters(mut column: usize) -> String {
    let mut out = String::new();
    loop {
        out.insert(0, (b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    out
}

/// `[Content_Types].xml` d'un classeur Excel.
fn excel_content_types(styles: bool) -> String {
    let mut types = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>"#,
    );
    if styles {
        types.push_str(r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#);
    }
    types.push_str("</Types>");
    types
}

/// `xl/workbook.xml` : une feuille nommée `Sheet1`.
fn excel_workbook() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        .to_string()
}

/// `xl/_rels/workbook.xml.rels` : feuille, chaînes partagées et styles.
fn excel_workbook_rels(styles: bool) -> String {
    let mut rels = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/>"#,
    );
    if styles {
        rels.push_str(r#"<Relationship Id="rId3" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/>"#);
    }
    rels.push_str("</Relationships>");
    rels
}

// ---------------------------------------------------------------------------
// PowerPoint (.pptx)
// ---------------------------------------------------------------------------

/// Assemble les parties du paquet PowerPoint : une diapositive par page.
///
/// La taille de la diapositive suit celle de la page PDF : une capture
/// d'écran 16:9 ne doit pas être aplatie dans un format 4:3.
fn powerpoint_parts(pages: &[OfficePage], layout: OfficeLayout) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
    // Un PDF a toujours au moins une page ; on garantit la même chose ici.
    let fallback = vec![OfficePage {
        width: 595.0,
        height: 842.0,
        runs: Vec::new(),
        images: Vec::new(),
    }];
    let pages: &[OfficePage] = if pages.is_empty() { &fallback } else { pages };
    let slide_count = pages.len();

    // Taille de la diapositive : celle de la première page.
    let (cx, cy) = powerpoint_slide_size(pages[0].width, pages[0].height);

    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let mut slides = Vec::new();
    let mut image_index = 0usize;
    for (number, page) in pages.iter().enumerate() {
        let mut rels = String::from(
            r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/>"#,
        );
        let mut pictures = Vec::new();
        for image in &page.images {
            image_index += 1;
            media.push((format!("ppt/media/image{image_index}.png"), image.png.clone()));
            pictures.push(PptPicture {
                index: image_index,
                width: image.width,
                height: image.height,
                rect: image.rect,
            });
            rels.push_str(&format!(
                r#"<Relationship Id="rId{}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="../media/image{image_index}.png"/>"#,
                image_index + 1
            ));
        }
        rels.push_str("</Relationships>");
        slides.push((
            slide(page, layout, &pictures),
            rels,
        ));
        let _ = number;
    }

    let mut parts = vec![
        (
            "[Content_Types].xml".to_string(),
            powerpoint_content_types(slide_count, image_index > 0),
        ),
        (
            "_rels/.rels".to_string(),
            root_rels("ppt/presentation.xml"),
        ),
        (
            "ppt/presentation.xml".to_string(),
            powerpoint_presentation(slide_count, cx, cy),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            powerpoint_presentation_rels(slide_count),
        ),
        (
            "ppt/slideMasters/slideMaster1.xml".to_string(),
            slide_master(),
        ),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_string(),
            slide_master_rels(),
        ),
        (
            "ppt/slideLayouts/slideLayout1.xml".to_string(),
            slide_layout(),
        ),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".to_string(),
            slide_layout_rels(),
        ),
        ("ppt/theme/theme1.xml".to_string(), theme()),
    ];

    for (index, (slide_xml, rels_xml)) in slides.iter().enumerate() {
        let number = index + 1;
        parts.push((format!("ppt/slides/slide{number}.xml"), slide_xml.clone()));
        parts.push((
            format!("ppt/slides/_rels/slide{number}.xml.rels"),
            rels_xml.clone(),
        ));
    }

    (parts, media)
}

/// Une image à poser sur une diapositive.
struct PptPicture {
    /// Indice global de l'image (1-based).
    index: usize,
    /// Largeur en pixels.
    width: u32,
    /// Hauteur en pixels.
    height: u32,
    /// Boîte `[x1, y1, x2, y2]` sur la page, si elle est connue.
    rect: Option<[f32; 4]>,
}

/// Taille de la diapositive en EMU (1 pt = 12 700 EMU).
fn powerpoint_slide_size(width: f32, height: f32) -> (u64, u64) {
    let cx = (width.max(1.0) * 12_700.0).round().max(1.0) as u64;
    let cy = (height.max(1.0) * 12_700.0).round().max(1.0) as u64;
    (cx, cy)
}

/// `ppt/slides/slideN.xml` : le contenu de la diapositive.
///
/// En mode Exact, chaque fragment a sa propre forme de texte, posée à ses
/// coordonnées. En mode flux, les paragraphes sont empilés dans une zone
/// unique, lisible comme du texte courant.
fn slide(page: &OfficePage, layout: OfficeLayout, pictures: &[PptPicture]) -> String {
    let mut shapes = String::new();
    let mut shape_id = 2u32;

    // Les images d'abord : elles passent derrière le texte.
    for picture in pictures {
        shapes.push_str(&ppt_picture(picture, page, shape_id));
        shape_id += 1;
    }

    match layout {
        OfficeLayout::Exact => {
            for run in &page.runs {
                shapes.push_str(&ppt_text_shape(run, page, shape_id));
                shape_id += 1;
            }
        }
        OfficeLayout::Flow => {
            let lines = group_lines(&page.runs);
            let body = if lines.is_empty() {
                "<a:p/>".to_string()
            } else {
                let body_size = body_size(&lines);
                let mut out = String::new();
                for paragraph in group_paragraphs(&lines, body_size) {
                    out.push_str("<a:p>");
                    for (index, line) in paragraph.lines.iter().enumerate() {
                        if index > 0 {
                            out.push_str("<a:br/>");
                        }
                        for run in &line.runs {
                            out.push_str(&ppt_run(run));
                        }
                    }
                    out.push_str("</a:p>");
                }
                out
            };
            shapes.push_str(&ppt_flow_shape(&body, page, shape_id));
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}

/// Un run DrawingML : texte, corps, couleur et police.
fn ppt_run(run: &OfficeRun) -> String {
    let hundredths = scale_to_i64(run.font_size, 100.0, 100);
    format!(
        r#"<a:r><a:rPr lang="fr-FR" sz="{hundredths}" b="{bold}" i="{italic}" dirty="0"><a:solidFill><a:srgbClr val="{color}"/></a:solidFill><a:latin typeface="{font}"/><a:cs typeface="{font}"/></a:rPr><a:t>{text}</a:t></a:r>"#,
        hundredths = hundredths,
        bold = if run.bold { 1 } else { 0 },
        italic = if run.italic { 1 } else { 0 },
        color = hex_color(run.color),
        font = run.font,
        text = escape_xml(&run.text),
    )
}

/// Forme de texte positionnée sur un fragment.
fn ppt_text_shape(run: &OfficeRun, page: &OfficePage, shape_id: u32) -> String {
    // Le repère PDF a son origine en bas à gauche ; PowerPoint mesure
    // depuis le haut de la diapositive.
    let x = scale_to_i64(run.rect[0], 12_700.0, 0);
    let y = scale_to_i64(page.height - run.rect[3], 12_700.0, 0);
    let cx = scale_to_i64(run.rect[2] - run.rect[0], 12_700.0, 1);
    let cy = scale_to_i64(run.rect[3] - run.rect[1], 12_700.0, 1);
    let body = format!("<a:p>{}</a:p>", ppt_run(run));
    ppt_text_body(shape_id, x, y, cx, cy, &body)
}

/// Zone de texte empilée, en mode flux.
fn ppt_flow_shape(body: &str, page: &OfficePage, shape_id: u32) -> String {
    let (cx, cy) = powerpoint_slide_size(page.width, page.height);
    let cx = cx as i64;
    let cy = cy as i64;
    ppt_text_body(shape_id, 0, 0, cx, cy, body)
}

/// Corps commun des formes de texte.
fn ppt_text_body(shape_id: u32, x: i64, y: i64, cx: i64, cy: i64, body: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{shape_id}" name="Text {shape_id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="none" lIns="0" tIns="0" rIns="0" bIns="0"><a:noAutofit/></a:bodyPr><a:lstStyle/>{body}</p:txBody></p:sp>"#
    )
}

/// Image posée sur la diapositive, à sa position mesurée.
fn ppt_picture(picture: &PptPicture, page: &OfficePage, shape_id: u32) -> String {
    const EMU_PER_PIXEL: u64 = 9_525;
    let natural_cx = (picture.width.max(1) as u64) * EMU_PER_PIXEL;
    let natural_cy = (picture.height.max(1) as u64) * EMU_PER_PIXEL;

    let (x_pt, y_top_pt, w_pt, h_pt) = match picture.rect {
        Some([x1, y1, x2, y2]) => (x1, y2, (x2 - x1).max(1.0), (y2 - y1).max(1.0)),
        None => (0.0, page.height, 0.0, 0.0),
    };
    let cx = if w_pt > 0.0 {
        (w_pt * 12_700.0).round().max(1.0) as u64
    } else {
        natural_cx
    };
    let cy = if h_pt > 0.0 {
        (h_pt * 12_700.0).round().max(1.0) as u64
    } else {
        natural_cy
    };
    let x = scale_to_i64(x_pt, 12_700.0, 0);
    let y = scale_to_i64(page.height - y_top_pt, 12_700.0, 0);

    format!(
        r#"<p:pic><p:nvPicPr><p:cNvPr id="{shape_id}" name="Picture {shape_id}"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId{rel}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
        rel = picture.index + 1,
    )
}

/// `[Content_Types].xml` d'une présentation PowerPoint.
fn powerpoint_content_types(slide_count: usize, images: bool) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#,
    );
    if images {
        out.push_str(r#"<Default Extension="png" ContentType="image/png"/>"#);
    }
    out.push_str(r#"<Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#);
    for number in 1..=slide_count {
        out.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
    }
    out.push_str("</Types>");
    out
}

/// `ppt/presentation.xml` : liste du masque et des diapositives.
///
/// La taille de la diapositive est celle de la page PDF : c'est elle qui
/// donne le format de la présentation.
fn powerpoint_presentation(slide_count: usize, cx: u64, cy: u64) -> String {
    let mut slides = String::new();
    for number in 1..=slide_count {
        let id = 255 + number;
        let rel = number + 1;
        slides.push_str(&format!("<p:sldId id=\"{id}\" r:id=\"rId{rel}\"/>"));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{slides}</p:sldIdLst><p:sldSz cx="{cx}" cy="{cy}"/><p:notesSz cx="{cx}" cy="{cy}"/></p:presentation>"#
    )
}

/// `ppt/_rels/presentation.xml.rels` : masque puis diapositives.
fn powerpoint_presentation_rels(slide_count: usize) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="slideMasters/slideMaster1.xml"/>"#,
    );
    for number in 1..=slide_count {
        let rel = number + 1;
        out.push_str(&format!(
            "<Relationship Id=\"rId{rel}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{number}.xml\"/>"
        ));
    }
    out.push_str("</Relationships>");
    out
}

/// `ppt/slideMasters/slideMaster1.xml`.
fn slide_master() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#
        .to_string()
}

/// `ppt/slideMasters/_rels/slideMaster1.xml.rels` : disposition + thème.
fn slide_master_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#
        .to_string()
}

/// `ppt/slideLayouts/slideLayout1.xml` : disposition vide.
fn slide_layout() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#
        .to_string()
}

/// `ppt/slideLayouts/_rels/slideLayout1.xml.rels` : lien vers le masque.
fn slide_layout_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
        .to_string()
}

/// `ppt/theme/theme1.xml` : thème « Office » minimal mais complet.
fn theme() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="105000"/><a:tint val="67000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="103000"/><a:tint val="73000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="102000"/><a:satMod val="109000"/><a:tint val="62000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:satMod val="103000"/><a:lumMod val="102000"/><a:tint val="94000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:satMod val="110000"/><a:lumMod val="100000"/><a:shade val="100000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="99000"/><a:satMod val="121000"/><a:shade val="78000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/><a:shade val="98000"/><a:lumMod val="102000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:tint val="98000"/><a:satMod val="130000"/><a:shade val="90000"/><a:lumMod val="103000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="63000"/><a:satMod val="120000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
        .to_string()
}

// ---------------------------------------------------------------------------
// Communs
// ---------------------------------------------------------------------------

/// `_rels/.rels` : relation racine vers le document principal du paquet.
fn root_rels(target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{target}"/></Relationships>"#
    )
}

/// Retire les caractères interdits en XML 1.0 (contrôles C0 sauf tabulation,
/// saut de ligne et retour chariot).
fn sanitize_xml(text: &str) -> String {
    text.chars().filter(|&c| is_xml_char(c)).collect()
}

/// Vrai si `c` est autorisé dans un document XML 1.0.
fn is_xml_char(c: char) -> bool {
    c == '\t' || c == '\n' || c == '\r' || c >= ' '
}

