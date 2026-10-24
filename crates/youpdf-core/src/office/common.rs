//! Outils partagés par les trois formats Office : écriture ZIP,
//! polices, couleurs, regroupement des lignes en paragraphes, échappement XML.

use super::*;

/// Écrit une archive ZIP mêlant parties XML et parties binaires.
pub(crate) fn write_zip_with_media(
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
pub(crate) fn office_font(base: &str) -> (&'static str, bool, bool) {
    let lower = base.to_ascii_lowercase();
    let bold = lower.contains("bold") || lower.contains("black") || lower.contains("heavy");
    let italic = lower.contains("italic")
        || lower.contains("oblique")
        || lower.contains("italicized");
    let family = if lower.contains("times") || lower.contains("serif") || lower.contains("roman") {
        "Times New Roman"
    } else if lower.contains("courier") || lower.contains("mono") {
        "Courier New"
    } else {
        // Helvetica, Arial, Calibri et les polices sans empattement rejoignent
        // le repli Arial.
        "Arial"
    };
    (family, bold, italic)
}

/// Échelle une valeur vers un entier, sans jamais descendre sous `floor`.
pub(crate) fn scale_to_i64(value: f32, scale: f32, floor: i64) -> i64 {
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
pub fn hex_color(color: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", color[0], color[1], color[2])
}

/// Regroupe les fragments en lignes de texte.
///
/// Deux fragments partagent une ligne quand leurs bases sont proches : la
/// tolérance suit le corps, car c'est lui qui donne l'échelle verticale du
/// texte. Le bord inférieur (`rect[1]`) sert de ligne de base : c'est
/// ainsi que le décodeur interne construit ses boîtes.
pub(crate) fn group_lines(runs: &[OfficeRun]) -> Vec<OfficeLine> {
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
pub(crate) fn body_size(lines: &[OfficeLine]) -> f32 {
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
pub(crate) fn group_paragraphs(lines: &[OfficeLine], body: f32) -> Vec<OfficeParagraph> {
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

/// `_rels/.rels` : relation racine vers le document principal du paquet.
pub(crate) fn root_rels(target: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{target}"/></Relationships>"#
    )
}

/// Retire les caractères interdits en XML 1.0 (contrôles C0 sauf tabulation,
/// saut de ligne et retour chariot).
pub(crate) fn sanitize_xml(text: &str) -> String {
    text.chars().filter(|&c| is_xml_char(c)).collect()
}

/// Vrai si `c` est autorisé dans un document XML 1.0.
pub(crate) fn is_xml_char(c: char) -> bool {
    c == '\t' || c == '\n' || c == '\r' || c >= ' '
}
