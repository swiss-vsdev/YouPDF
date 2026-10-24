//! Export Word (`.docx`) : parties OOXML et images embarquées.

use super::*;

/// Les médias embarqués par Word et leurs poses, page par page.
///
/// Le premier vecteur associe chaque image à son nom dans `word/media/`, le
/// second donne, pour chaque page, les images qui y sont posées.
pub(crate) type WordMedia = (Vec<(String, Vec<u8>)>, Vec<Vec<WordImage>>);

/// Assemble les parties du paquet Word en mode « flux ».
///
/// Les images sont ajoutées à la fin de chaque page : en flux, elles ne
/// peuvent pas être placées au milieu du texte sans briser la lecture.
pub(crate) fn word_parts_flow(pages: &[OfficePage], text_pages: &[Vec<String>]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
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
pub(crate) fn word_flow_body(page: &OfficePage, fallback: Option<&Vec<String>>) -> String {
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
pub(crate) fn word_run(run: &OfficeRun) -> String {
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
pub(crate) fn word_media(pages: &[OfficePage]) -> WordMedia {
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
pub(crate) struct WordImage {
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
pub(crate) fn word_parts_exact(pages: &[OfficePage]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
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
pub(crate) fn word_frame_paragraph(run: &OfficeRun, page_height: f32) -> String {
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
pub(crate) fn word_positioned_picture(image: &WordImage, page_height: f32) -> String {
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
pub(crate) fn word_document(body: &str) -> String {
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
pub(crate) fn word_styles() -> String {
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
pub(crate) fn image_rel_id(index: usize) -> usize {
    index + 9
}

/// `word/_rels/document.xml.rels` : styles, puis une relation par image.
pub(crate) fn document_rels(images: usize) -> String {
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
pub(crate) fn word_picture_paragraph(index: usize, width: u32, height: u32) -> String {
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
pub(crate) fn word_content_types(images: bool) -> String {
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
