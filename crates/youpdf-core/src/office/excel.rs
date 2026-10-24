//! Export Excel (`.xlsx`) : feuilles, colonnes, valeurs de cellule, styles.

use super::*;

/// Le contenu d'une cellule, avec son type déduit du texte.
#[derive(Clone)]
pub(crate) enum CellValue {
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
pub(crate) fn excel_parts_flow(pages: &[Vec<String>]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
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
pub(crate) fn excel_parts_exact(pages: &[OfficePage]) -> (Vec<Part>, Vec<(String, Vec<u8>)>) {
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
pub(crate) fn excel_column_boundaries(lines: &[OfficeLine]) -> Vec<f32> {
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
pub(crate) fn excel_column_threshold(lines: &[OfficeLine]) -> f32 {
    let mut sizes: Vec<f32> = lines.iter().map(|line| line.size).collect();
    if sizes.is_empty() {
        return 6.0;
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = sizes[sizes.len() / 2];
    (median * 0.6).max(1.0)
}

/// Indice de la colonne d'un fragment.
pub(crate) fn excel_column_of(boundaries: &[f32], x: f32, threshold: f32) -> usize {
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
pub(crate) fn excel_cell_value(text: &str) -> CellValue {
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
pub(crate) fn excel_parse_number(text: &str) -> Option<f64> {
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
pub(crate) fn excel_date_serial(text: &str) -> Option<f64> {
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
    if !(1900..=9999).contains(&year) {
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
pub(crate) fn days_from_civil(year: i32, month: i32, day: i32) -> Option<i64> {
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
pub(crate) fn merge_cells(first: CellValue, second: CellValue) -> CellValue {
    match (first, second) {
        (CellValue::Text(a), CellValue::Text(b)) => CellValue::Text(format!("{a} {b}")),
        (CellValue::Number(a), CellValue::Number(b)) => CellValue::Number(a + b),
        (a, _) => a,
    }
}

/// Nombre écrit dans une cellule : point décimal, pas de notation
/// scientifique (Excel ne l'accepte pas dans `<v>`).
pub(crate) fn excel_number(value: f64) -> String {
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
pub(crate) fn excel_cols(widths: &[usize]) -> String {
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
pub(crate) fn excel_sheet(rows: &str, cols: &str, views: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{views}{cols}<sheetData>{rows}</sheetData></worksheet>"#
    )
}

/// `xl/sharedStrings.xml` : les chaînes partagées.
pub(crate) fn excel_shared_strings(strings: &str, count: usize) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{count}" uniqueCount="{count}">{strings}</sst>"#
    )
}

/// `xl/styles.xml` : un format de date (`numFmtId="14"`), référencé par
/// l'index `1` des cellules de date.
pub(crate) fn excel_styles() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><numFmts count="1"><numFmt numFmtId="14" formatCode="yyyy\-mm\-dd"/></numFmts><fonts count="1"><font><sz val="11"/><name val="Calibri"/></font></fonts><fills count="1"><fill><patternFill patternType="none"/></fill></fills><borders count="1"><border><left/><right/><top/><bottom/><diagonal/></border></borders><cellStyleXfs count="1"><xf numFmtId="0" fontId="0" fillId="0" borderId="0"/></cellStyleXfs><cellXfs count="2"><xf numFmtId="0" fontId="0" fillId="0" borderId="0" xfId="0"/><xf numFmtId="14" fontId="0" fillId="0" borderId="0" xfId="0" applyNumberFormat="1"/></cellXfs></styleSheet>"#
        .to_string()
}

/// Lettres d'une colonne Excel (0 → A, 26 → AA).
pub(crate) fn column_letters(mut column: usize) -> String {
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
pub(crate) fn excel_content_types(styles: bool) -> String {
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
pub(crate) fn excel_workbook() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
        .to_string()
}

/// `xl/_rels/workbook.xml.rels` : feuille, chaînes partagées et styles.
pub(crate) fn excel_workbook_rels(styles: bool) -> String {
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
