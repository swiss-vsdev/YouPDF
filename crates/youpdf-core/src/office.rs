//! Export PDF → Word (`.docx`), Excel (`.xlsx`) et PowerPoint (`.pptx`).
//!
//! Les parties OOXML (Office Open XML) sont écrites à la main puis
//! empaquetées avec la crate `zip`. Aucune dépendance `docx-rs` : chaque
//! format reste autonome et la surface de dépendances est minimale.
//!
//! ## Ce que l'export transporte
//!
//! Seule la couche texte est exportée :
//!
//! - **Word** : un paragraphe par ligne, saut de page entre les pages PDF ;
//! - **Excel** : une ligne de feuille par ligne de texte ;
//! - **PowerPoint** : une diapositive par page PDF, chaque ligne dans un
//!   cadre de texte.
//!
//! Les images, les tableaux et la mise en page ne sont pas reconstruits.

use std::io::Write;
use std::path::Path;

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
    /// Paragraphes fluides, un par ligne (texte éditable).
    #[default]
    Flow,
    /// Chaque fragment est placé à ses coordonnées PDF (Word uniquement).
    Exact,
}

/// Une partie du paquet OOXML : chemin dans l'archive et contenu XML.
type Part = (String, String);

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
    /// `Exact` ne change que Word : les autres formats placent déjà leur
    /// contenu à des coordonnées mesurées.
    pub fn export_office_layout(
        &self,
        path: impl AsRef<Path>,
        format: OfficeFormat,
        layout: OfficeLayout,
    ) -> Result<()> {
        let path = path.as_ref();
        let pages = self.office_pages();
        let parts = match format {
            OfficeFormat::Word => match layout {
                OfficeLayout::Flow => {
                    let (parts, media) = word_parts_with_images(&pages, &self.office_images());
                    let owned: Vec<(String, Vec<u8>)> = parts
                        .into_iter()
                        .map(|(name, content)| (name, content.into_bytes()))
                        .collect();
                    return write_zip_with_media(path, &owned, &media);
                }
                OfficeLayout::Exact => word_parts_exact(&self.office_placed()),
            },
            OfficeFormat::Excel => match layout {
                OfficeLayout::Flow => excel_parts(&pages),
                OfficeLayout::Exact => excel_parts_positioned(&self.office_placed()),
            },
            OfficeFormat::PowerPoint => powerpoint_parts(&pages),
        };
        write_zip(path, &parts)
    }

    /// Images embarquées de chaque page, en PNG, avec leurs dimensions.
    fn office_images(&self) -> Vec<Vec<(Vec<u8>, u32, u32)>> {
        let total = self.page_count() as u32;
        let mut pages = Vec::new();
        for number in 1..=total {
            let mut images = Vec::new();
            for info in self.extract_images(number).unwrap_or_default() {
                if let Ok(png) = self.extract_image_png(&info) {
                    images.push((png, info.width, info.height));
                }
            }
            pages.push(images);
        }
        pages
    }

    /// Fragments positionnés de chaque page, pour la mise en page exacte.
    fn office_placed(&self) -> Vec<(f32, f32, Vec<(String, f32, f32, f32)>)> {
        let total = self.page_count() as u32;
        let mut pages = Vec::new();
        for number in 1..=total {
            let (width, height) = self.page_size(number);
            let mut runs = Vec::new();
            if let Ok(fragments) = self.extract_text_fragments(number) {
                for fragment in fragments {
                    let text = fragment.text.trim();
                    if text.is_empty() {
                        continue;
                    }
                    runs.push((
                        text.to_string(),
                        fragment.rect[0],
                        // Haut du cadre, depuis le haut de la page.
                        height - fragment.rect[3],
                        fragment.font_size.max(4.0),
                    ));
                }
            }
            runs.sort_by(|a, b| {
                a.2.partial_cmp(&b.2)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            });
            pages.push((width, height, runs));
        }
        pages
    }

    /// Lignes de texte par page, `\u{c}` normalisé en `\n` et caractères
    /// interdits en XML retirés.
    fn office_pages(&self) -> Vec<Vec<String>> {
        let total = self.page_count() as u32;
        let mut pages = Vec::with_capacity(total as usize);
        for number in 1..=total {
            let text = self.page_text(number).unwrap_or_default();
            let text = text.replace('\u{c}', "\n");
            pages.push(text.split('\n').map(sanitize_xml).collect());
        }
        pages
    }
}

/// Écrit une archive ZIP contenant chaque partie en DEFLATE.
fn write_zip(path: &Path, parts: &[Part]) -> Result<()> {
    let owned: Vec<(String, Vec<u8>)> = parts
        .iter()
        .map(|(name, content)| (name.clone(), content.as_bytes().to_vec()))
        .collect();
    write_zip_with_media(path, &owned, &[])
}

/// Écrit une archive ZIP mêlant parties XML et parties binaires.
fn write_zip_with_media(
    path: &Path,
    parts: &[(String, Vec<u8>)],
    media: &[(String, Vec<u8>)],
) -> Result<()> {
    let file = std::fs::File::create(path)?;
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, content) in parts.iter().chain(media.iter()) {
        zip.start_file(name.as_str(), options)
            .map_err(|e| Error::Pdf(format!("écriture de {name} : {e}")))?;
        zip.write_all(content)?;
    }
    zip.finish()
        .map_err(|e| Error::Pdf(format!("finalisation de l'archive Office : {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Word (.docx)
// ---------------------------------------------------------------------------

/// Assemble les parties du paquet Word.
#[allow(dead_code)]
fn word_parts(pages: &[Vec<String>]) -> Vec<Part> {
    vec![
        ("[Content_Types].xml".to_string(), word_content_types()),
        (
            "_rels/.rels".to_string(),
            root_rels("word/document.xml"),
        ),
        ("word/document.xml".to_string(), word_document(pages)),
        ("word/styles.xml".to_string(), word_styles()),
        (
            "word/_rels/document.xml.rels".to_string(),
            document_rels(),
        ),
    ]
}

/// Variante « mise en page exacte » : chaque fragment devient un cadre
/// positionné en twips (1 pt = 20 twips) depuis le coin haut-gauche.
fn word_parts_exact(pages: &[(f32, f32, Vec<(String, f32, f32, f32)>)]) -> Vec<Part> {
    vec![
        ("[Content_Types].xml".to_string(), word_content_types()),
        ("_rels/.rels".to_string(), root_rels("word/document.xml")),
        ("word/document.xml".to_string(), word_document_exact(pages)),
        ("word/styles.xml".to_string(), word_styles()),
        (
            "word/_rels/document.xml.rels".to_string(),
            document_rels(),
        ),
    ]
}

fn word_document_exact(pages: &[(f32, f32, Vec<(String, f32, f32, f32)>)]) -> String {
    let mut body = String::new();
    for (_width, _height, runs) in pages {
        for (text, x, y, size) in runs {
            let x_twips = (*x * 20.0).round().max(0.0) as i64;
            let y_twips = (*y * 20.0).round().max(0.0) as i64;
            let w_twips = ((text.chars().count() as f32 * size * 0.5) * 20.0)
                .round()
                .max(200.0) as i64;
            let h_twips = (*size * 1.4 * 20.0).round().max(200.0) as i64;
            let half_points = (*size * 2.0).round() as i64;
            body.push_str(&format!(
                r#"<w:p><w:pPr><w:framePr w:w="{w_twips}" w:h="{h_twips}" w:hAnchor="page" w:vAnchor="page" w:x="{x_twips}" w:y="{y_twips}" w:wrap="none"/></w:pPr><w:r><w:rPr><w:sz w:val="{half_points}"/></w:rPr><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
                escape_xml(text)
            ));
        }
        body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
    }
    let (width, height) = pages
        .first()
        .map(|(w, h, _)| (*w, *h))
        .unwrap_or((595.0, 842.0));
    let page_w = (width * 20.0).round() as i64;
    let page_h = (height * 20.0).round() as i64;
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="{page_w}" w:h="{page_h}"/><w:pgMar w:top="0" w:right="0" w:bottom="0" w:left="0"/></w:sectPr></w:body></w:document>"#
    )
}

/// Paquet Word avec les images embarquées de chaque page.
fn word_parts_with_images(
    pages: &[Vec<String>],
    images: &[Vec<(Vec<u8>, u32, u32)>],
) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
    let mut media = Vec::new();
    let mut placements: Vec<Vec<(usize, u32, u32)>> = Vec::new();
    let mut index = 0usize;
    for page in images {
        let mut placed = Vec::new();
        for (png, width, height) in page {
            index += 1;
            media.push((format!("word/media/image{index}.png"), png.clone()));
            placed.push((index, *width, *height));
        }
        placements.push(placed);
    }
    let count = index;
    let parts = vec![
        ("[Content_Types].xml".to_string(), word_content_types_with_images(count > 0)),
        ("_rels/.rels".to_string(), root_rels("word/document.xml")),
        (
            "word/document.xml".to_string(),
            word_document_with_images(pages, &placements),
        ),
        ("word/styles.xml".to_string(), word_styles()),
        (
            "word/_rels/document.xml.rels".to_string(),
            document_rels_with_images(count),
        ),
    ];
    (parts, media)
}

/// Relation `rId` de la n-ième image (les styles occupent `rId1`).
fn image_rel_id(index: usize) -> usize {
    index + 9
}

fn document_rels_with_images(images: usize) -> String {
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

fn word_document_with_images(
    pages: &[Vec<String>],
    images: &[Vec<(usize, u32, u32)>],
) -> String {
    let mut body = String::new();
    for (page_index, page) in pages.iter().enumerate() {
        if page_index > 0 {
            body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
        }
        for line in page {
            body.push_str(r#"<w:p><w:r><w:t xml:space="preserve">"#);
            body.push_str(&escape_xml(line));
            body.push_str("</w:t></w:r></w:p>");
        }
        if let Some(page_images) = images.get(page_index) {
            for (index, width, height) in page_images {
                body.push_str(&word_picture_paragraph(*index, *width, *height));
            }
        }
        if page.is_empty() && images.get(page_index).map(Vec::is_empty).unwrap_or(true) {
            body.push_str("<w:p/>");
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1417" w:bottom="1417" w:left="1417"/></w:sectPr></w:body></w:document>"#
    )
}

fn word_content_types_with_images(images: bool) -> String {
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

/// `[Content_Types].xml` d'un document Word.
fn word_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/></Types>"#
        .to_string()
}

/// `word/document.xml` : un paragraphe par ligne, saut de page entre les pages.
#[allow(dead_code)]
fn word_document(pages: &[Vec<String>]) -> String {
    let mut body = String::new();
    for (page_index, page) in pages.iter().enumerate() {
        if page_index > 0 {
            body.push_str(r#"<w:p><w:r><w:br w:type="page"/></w:r></w:p>"#);
        }
        if page.is_empty() {
            body.push_str("<w:p/>");
            continue;
        }
        for line in page {
            body.push_str(r#"<w:p><w:r><w:t xml:space="preserve">"#);
            body.push_str(&escape_xml(line));
            body.push_str("</w:t></w:r></w:p>");
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}<w:sectPr><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1417" w:right="1417" w:bottom="1417" w:left="1417"/></w:sectPr></w:body></w:document>"#
    )
}

/// `word/styles.xml` : style `Normal` minimal.
fn word_styles() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:cs="Calibri"/><w:sz w:val="22"/><w:szCs w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault/></w:docDefaults><w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/><w:qFormat/></w:style></w:styles>"#
        .to_string()
}

/// `word/_rels/document.xml.rels` : lien vers les styles.
fn document_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/></Relationships>"#
        .to_string()
}

// ---------------------------------------------------------------------------
// Excel (.xlsx)
// ---------------------------------------------------------------------------

/// Assemble les parties du paquet Excel : une feuille, une chaîne partagée par
/// ligne non vide.
fn excel_parts(pages: &[Vec<String>]) -> Vec<Part> {
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

    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows}</sheetData></worksheet>"#
    );
    let strings = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{shared_count}" uniqueCount="{shared_count}">{shared}</sst>"#
    );

    vec![
        ("[Content_Types].xml".to_string(), excel_content_types()),
        ("_rels/.rels".to_string(), root_rels("xl/workbook.xml")),
        ("xl/workbook.xml".to_string(), excel_workbook()),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            excel_workbook_rels(),
        ),
        ("xl/worksheets/sheet1.xml".to_string(), sheet),
        ("xl/sharedStrings.xml".to_string(), strings),
    ]
}

/// Détecte lignes et colonnes par alignement des fragments, puis écrit une
/// cellule par intersection.
fn excel_parts_positioned(pages: &[(f32, f32, Vec<(String, f32, f32, f32)>)]) -> Vec<Part> {
    // Lignes : regroupées par ordonnée, à la tolérance d'une demi-hauteur.
    let mut rows_out = String::new();
    let mut shared = String::new();
    let mut shared_count = 0usize;
    let mut row_index = 0usize;

    for (_, _, runs) in pages {
        let mut lines: Vec<(f32, Vec<&(String, f32, f32, f32)>)> = Vec::new();
        for run in runs {
            let y = run.2;
            match lines.iter_mut().find(|(line_y, _)| (line_y - y).abs() <= run.3 * 0.6) {
                Some((_, bucket)) => bucket.push(run),
                None => lines.push((y, vec![run])),
            }
        }
        lines.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

        for (_, bucket) in lines {
            row_index += 1;
            let mut cells: Vec<(f32, String)> = Vec::new();
            let mut sorted = bucket;
            sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            for run in sorted {
                match cells.last_mut() {
                    Some((x, text)) if (run.1 - *x).abs() <= 10.0 => {
                        if !text.is_empty() {
                            text.push(' ');
                        }
                        text.push_str(&run.0);
                    }
                    _ => cells.push((run.1, run.0.clone())),
                }
            }
            if cells.is_empty() {
                rows_out.push_str(&format!("<row r=\"{row_index}\"/>"));
                continue;
            }
            rows_out.push_str(&format!("<row r=\"{row_index}\">"));
            for (column, (_, text)) in cells.iter().enumerate() {
                let reference = format!("{}{}", column_letters(column), row_index);
                rows_out.push_str(&format!(
                    "<c r=\"{reference}\" t=\"s\"><v>{shared_count}</v></c>"
                ));
                shared.push_str(r#"<si><t xml:space="preserve">"#);
                shared.push_str(&escape_xml(text));
                shared.push_str("</t></si>");
                shared_count += 1;
            }
            rows_out.push_str("</row>");
        }
    }

    let sheet = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>{rows_out}</sheetData></worksheet>"#
    );
    let strings = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{shared_count}" uniqueCount="{shared_count}">{shared}</sst>"#
    );
    vec![
        ("[Content_Types].xml".to_string(), excel_content_types()),
        ("_rels/.rels".to_string(), root_rels("xl/workbook.xml")),
        ("xl/workbook.xml".to_string(), excel_workbook()),
        (
            "xl/_rels/workbook.xml.rels".to_string(),
            excel_workbook_rels(),
        ),
        ("xl/worksheets/sheet1.xml".to_string(), sheet),
        ("xl/sharedStrings.xml".to_string(), strings),
    ]
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
fn excel_content_types() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/><Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/></Types>"#
        .to_string()
}

/// `xl/workbook.xml` : une feuille nommée `Sheet1`.
fn excel_workbook() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        .to_string()
}

/// `xl/_rels/workbook.xml.rels` : feuille + chaînes partagées.
fn excel_workbook_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings" Target="sharedStrings.xml"/></Relationships>"#
        .to_string()
}

// ---------------------------------------------------------------------------
// PowerPoint (.pptx)
// ---------------------------------------------------------------------------

/// Assemble les parties du paquet PowerPoint : une diapositive par page.
fn powerpoint_parts(pages: &[Vec<String>]) -> Vec<Part> {
    // Un PDF a toujours au moins une page ; on garantit la même chose ici.
    let fallback = vec![Vec::new()];
    let pages: &[Vec<String>] = if pages.is_empty() { &fallback } else { pages };
    let slide_count = pages.len();

    let mut parts = vec![
        (
            "[Content_Types].xml".to_string(),
            powerpoint_content_types(slide_count),
        ),
        (
            "_rels/.rels".to_string(),
            root_rels("ppt/presentation.xml"),
        ),
        (
            "ppt/presentation.xml".to_string(),
            powerpoint_presentation(slide_count),
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

    for (index, page) in pages.iter().enumerate() {
        let number = index + 1;
        parts.push((format!("ppt/slides/slide{number}.xml"), slide(page)));
        parts.push((
            format!("ppt/slides/_rels/slide{number}.xml.rels"),
            slide_rels(),
        ));
    }

    parts
}

/// `[Content_Types].xml` d'une présentation PowerPoint.
fn powerpoint_content_types(slide_count: usize) -> String {
    let mut out = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/ppt/presentation.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"/><Override PartName="/ppt/slideMasters/slideMaster1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"/><Override PartName="/ppt/slideLayouts/slideLayout1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/><Override PartName="/ppt/theme/theme1.xml" ContentType="application/vnd.openxmlformats-officedocument.theme+xml"/>"#,
    );
    for number in 1..=slide_count {
        out.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
    }
    out.push_str("</Types>");
    out
}

/// `ppt/presentation.xml` : liste du masque et des diapositives.
fn powerpoint_presentation(slide_count: usize) -> String {
    let mut slides = String::new();
    for number in 1..=slide_count {
        let id = 255 + number;
        let rel = number + 1;
        slides.push_str(&format!("<p:sldId id=\"{id}\" r:id=\"rId{rel}\"/>"));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:presentation xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst>{slides}</p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/></p:presentation>"#
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

/// `ppt/slides/slideN.xml` : chaque ligne devient un paragraphe d'un cadre.
fn slide(lines: &[String]) -> String {
    let mut body = String::new();
    if lines.is_empty() {
        body.push_str("<a:p/>");
    } else {
        for line in lines {
            body.push_str("<a:p><a:r><a:t>");
            body.push_str(&escape_xml(line));
            body.push_str("</a:t></a:r></a:p>");
        }
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sld xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr><p:sp><p:nvSpPr><p:cNvPr id="2" name="Content"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="457200" y="457200"/><a:ext cx="8229600" cy="5943600"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr wrap="square"/><a:lstStyle/>{body}</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#
    )
}

/// `ppt/slides/_rels/slideN.xml.rels` : lien vers la disposition.
fn slide_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/></Relationships>"#
        .to_string()
}

/// `ppt/theme/theme1.xml` : thème « Office » minimal mais complet.
fn theme() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="105000"/><a:tint val="67000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="103000"/><a:tint val="73000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="109000"/><a:tint val="81000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:satMod val="103000"/><a:lumMod val="102000"/><a:tint val="94000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:satMod val="110000"/><a:lumMod val="100000"/><a:shade val="100000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="99000"/><a:satMod val="120000"/><a:shade val="78000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/><a:shade val="98000"/><a:lumMod val="102000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:tint val="98000"/><a:satMod val="130000"/><a:shade val="90000"/><a:lumMod val="103000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="63000"/><a:satMod val="120000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
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
