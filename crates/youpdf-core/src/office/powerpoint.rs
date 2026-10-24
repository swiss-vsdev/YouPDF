//! Export PowerPoint (`.pptx`) : diapositives, formes de texte, images.

use super::*;

/// Assemble les parties du paquet PowerPoint : une diapositive par page.
///
/// La taille de la diapositive suit celle de la page PDF : une capture
/// d'écran 16:9 ne doit pas être aplatie dans un format 4:3.
pub(crate) fn powerpoint_parts(pages: &[OfficePage], layout: OfficeLayout) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
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
pub(crate) struct PptPicture {
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
pub(crate) fn powerpoint_slide_size(width: f32, height: f32) -> (u64, u64) {
    let cx = (width.max(1.0) * 12_700.0).round().max(1.0) as u64;
    let cy = (height.max(1.0) * 12_700.0).round().max(1.0) as u64;
    (cx, cy)
}

/// `ppt/slides/slideN.xml` : le contenu de la diapositive.
///
/// En mode Exact, chaque fragment a sa propre forme de texte, posée à ses
/// coordonnées. En mode flux, les paragraphes sont empilés dans une zone
/// unique, lisible comme du texte courant.
pub(crate) fn slide(page: &OfficePage, layout: OfficeLayout, pictures: &[PptPicture]) -> String {
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
pub(crate) fn ppt_run(run: &OfficeRun) -> String {
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
pub(crate) fn ppt_text_shape(run: &OfficeRun, page: &OfficePage, shape_id: u32) -> String {
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
pub(crate) fn ppt_flow_shape(body: &str, page: &OfficePage, shape_id: u32) -> String {
    let (cx, cy) = powerpoint_slide_size(page.width, page.height);
    let cx = cx as i64;
    let cy = cy as i64;
    ppt_text_body(shape_id, 0, 0, cx, cy, body)
}

/// Corps commun des formes de texte.
pub(crate) fn ppt_text_body(shape_id: u32, x: i64, y: i64, cx: i64, cy: i64, body: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{shape_id}" name="Text {shape_id}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{cx}" cy="{cy}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="none" lIns="0" tIns="0" rIns="0" bIns="0"><a:noAutofit/></a:bodyPr><a:lstStyle/>{body}</p:txBody></p:sp>"#
    )
}

/// Image posée sur la diapositive, à sa position mesurée.
pub(crate) fn ppt_picture(picture: &PptPicture, page: &OfficePage, shape_id: u32) -> String {
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
pub(crate) fn powerpoint_content_types(slide_count: usize, images: bool) -> String {
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
pub(crate) fn powerpoint_presentation(slide_count: usize, cx: u64, cy: u64) -> String {
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
pub(crate) fn powerpoint_presentation_rels(slide_count: usize) -> String {
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
pub(crate) fn slide_master() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldMaster xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst><p:sldLayoutId id="2147483649" r:id="rId1"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle/><p:bodyStyle/><p:otherStyle/></p:txStyles></p:sldMaster>"#
        .to_string()
}

/// `ppt/slideMasters/_rels/slideMaster1.xml.rels` : disposition + thème.
pub(crate) fn slide_master_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme" Target="../theme/theme1.xml"/></Relationships>"#
        .to_string()
}

/// `ppt/slideLayouts/slideLayout1.xml` : disposition vide.
pub(crate) fn slide_layout() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<p:sldLayout xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" type="blank" preserve="1"><p:cSld name="Blank"><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#
        .to_string()
}

/// `ppt/slideLayouts/_rels/slideLayout1.xml.rels` : lien vers le masque.
pub(crate) fn slide_layout_rels() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideMaster" Target="../slideMasters/slideMaster1.xml"/></Relationships>"#
        .to_string()
}

/// `ppt/theme/theme1.xml` : thème « Office » minimal mais complet.
pub(crate) fn theme() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office Theme"><a:themeElements><a:clrScheme name="Office"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1><a:accent2><a:srgbClr val="ED7D31"/></a:accent2><a:accent3><a:srgbClr val="A5A5A5"/></a:accent3><a:accent4><a:srgbClr val="FFC000"/></a:accent4><a:accent5><a:srgbClr val="5B9BD5"/></a:accent5><a:accent6><a:srgbClr val="70AD47"/></a:accent6><a:hlink><a:srgbClr val="0563C1"/></a:hlink><a:folHlink><a:srgbClr val="954F72"/></a:folHlink></a:clrScheme><a:fontScheme name="Office"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:lumMod val="110000"/><a:satMod val="105000"/><a:tint val="67000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:lumMod val="105000"/><a:satMod val="103000"/><a:tint val="73000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="102000"/><a:satMod val="109000"/><a:tint val="62000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:satMod val="103000"/><a:lumMod val="102000"/><a:tint val="94000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:satMod val="110000"/><a:lumMod val="100000"/><a:shade val="100000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:lumMod val="99000"/><a:satMod val="121000"/><a:shade val="78000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:prstDash val="solid"/><a:miter lim="800000"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst><a:outerShdw blurRad="57150" dist="19050" dir="5400000" algn="ctr" rotWithShape="0"><a:srgbClr val="000000"><a:alpha val="63000"/></a:srgbClr></a:outerShdw></a:effectLst></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst><a:solidFill><a:schemeClr val="phClr"/></a:solidFill><a:solidFill><a:schemeClr val="phClr"><a:tint val="95000"/><a:satMod val="170000"/></a:schemeClr></a:solidFill><a:gradFill rotWithShape="1"><a:gsLst><a:gs pos="0"><a:schemeClr val="phClr"><a:tint val="93000"/><a:satMod val="150000"/><a:shade val="98000"/><a:lumMod val="102000"/></a:schemeClr></a:gs><a:gs pos="50000"><a:schemeClr val="phClr"><a:tint val="98000"/><a:satMod val="130000"/><a:shade val="90000"/><a:lumMod val="103000"/></a:schemeClr></a:gs><a:gs pos="100000"><a:schemeClr val="phClr"><a:shade val="63000"/><a:satMod val="120000"/></a:schemeClr></a:gs></a:gsLst><a:lin ang="5400000" scaled="0"/></a:gradFill></a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#
        .to_string()
}
