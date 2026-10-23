use youpdf_shared::Result;

use crate::document::Document;
use crate::util::{csv_escape, escape_xml, unescape_xml};

use super::{FormDataFormat, FormField};

impl FormDataFormat {
    /// Nom lisible pour l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            FormDataFormat::Fdf => "FDF",
            FormDataFormat::Xfdf => "XFDF",
            FormDataFormat::Csv => "CSV",
            FormDataFormat::Json => "JSON",
        }
    }

    /// Extension de fichier, sans le point.
    pub fn extension(self) -> &'static str {
        match self {
            FormDataFormat::Fdf => "fdf",
            FormDataFormat::Xfdf => "xfdf",
            FormDataFormat::Csv => "csv",
            FormDataFormat::Json => "json",
        }
    }
}

impl Document {
    /// Sérialise chaque couple nom/valeur de champ.
    ///
    /// Le résultat est en octets : le FDF est un conteneur d'octets dont les
    /// littéraux sont en WinAnsi, une chaîne UTF-8 n'y suffit pas.
    pub fn export_form_data(&self, format: FormDataFormat) -> Result<Vec<u8>> {
        // Un groupe radio apparaît une fois par widget : l'export ne garde
        // qu'une ligne par champ.
        let mut seen = std::collections::HashSet::new();
        let pairs: Vec<(String, String)> = self
            .read_form_fields()
            .into_iter()
            .filter(|field| seen.insert(field.object_id))
            .map(|f| (f.name, f.value.unwrap_or_default()))
            .collect();

        let out = match format {
            FormDataFormat::Csv => export_csv(&pairs).into_bytes(),
            FormDataFormat::Json => export_json(&pairs).into_bytes(),
            FormDataFormat::Xfdf => export_xfdf(&pairs).into_bytes(),
            FormDataFormat::Fdf => export_fdf(&pairs),
        };
        Ok(out)
    }

    /// Lit des couples nom/valeur et les écrit dans les champs. Renvoie le
    /// nombre de valeurs réellement appliquées.
    pub fn import_form_data(&mut self, format: FormDataFormat, data: &[u8]) -> Result<usize> {
        let pairs = match format {
            FormDataFormat::Csv => parse_csv(&String::from_utf8_lossy(data)),
            FormDataFormat::Json => parse_json(&String::from_utf8_lossy(data)),
            FormDataFormat::Xfdf => parse_xfdf(&String::from_utf8_lossy(data)),
            FormDataFormat::Fdf => parse_fdf(data),
        };

        // Les champs sont lus une seule fois : avant la boucle, chaque paire
        // ne cherchait son champ par scan linéaire, ce qui refaisait le
        // parcours de tout l'AcroForm pour chaque entrée (O(k·f)). Un
        // index par nom ramène la recherche à O(1).
        let fields = self.read_form_fields();
        let mut by_name: std::collections::HashMap<&str, &FormField> =
            std::collections::HashMap::with_capacity(fields.len());
        for field in &fields {
            by_name.entry(field.name.as_str()).or_insert(field);
        }

        let mut applied = 0usize;
        for (name, value) in pairs {
            let Some(field) = by_name.get(name.as_str()) else {
                // Nom inconnu : ignoré, ce n'est pas une erreur.
                continue;
            };
            self.set_field_value(field, &value)?;
            applied += 1;
        }
        if applied > 0 {
            self.touch();
        }
        Ok(applied)
    }
}

fn export_csv(pairs: &[(String, String)]) -> String {
    let mut out = String::from("name,value\n");
    for (name, value) in pairs {
        out.push_str(&csv_escape(name));
        out.push(',');
        out.push_str(&csv_escape(value));
        out.push('\n');
    }
    out
}

/// Échappe une chaîne pour un littéral JSON (sans `serde_json`).
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn export_json(pairs: &[(String, String)]) -> String {
    let mut out = String::from("{\"fields\":[");
    for (index, (name, value)) in pairs.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str("{\"name\":\"");
        out.push_str(&json_escape(name));
        out.push_str("\",\"value\":\"");
        out.push_str(&json_escape(value));
        out.push_str("\"}");
    }
    out.push_str("]}");
    out
}

fn export_xfdf(pairs: &[(String, String)]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xfdf xmlns=\"http://ns.adobe.com/xfdf/\"><fields>",
    );
    for (name, value) in pairs {
        out.push_str("<field name=\"");
        out.push_str(&escape_xml(name));
        out.push_str("\"><value>");
        out.push_str(&escape_xml(value));
        out.push_str("</value></field>");
    }
    out.push_str("</fields></xfdf>");
    out
}

/// Encode une valeur en littéral PDF `(…)`, prêt à insérer.
///
/// Le résultat reste en octets : les littéraux FDF sont en WinAnsi, et passer
/// par une `String` remplaçait chaque octet ≥ 0x80 par U+FFFD.
fn pdf_literal(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 2);
    out.push(b'(');
    out.extend_from_slice(&crate::edit::encode_pdf_literal(s));
    out.push(b')');
    out
}

fn export_fdf(pairs: &[(String, String)]) -> Vec<u8> {
    let mut out = Vec::from(&b"%FDF-1.2\n1 0 obj\n<< /FDF << /Fields [\n"[..]);
    for (name, value) in pairs {
        out.extend_from_slice(b"<< /T ");
        out.extend_from_slice(&pdf_literal(name));
        out.extend_from_slice(b" /V ");
        out.extend_from_slice(&pdf_literal(value));
        out.extend_from_slice(b" >>\n");
    }
    out.extend_from_slice(b"] >>\n>>\nendobj\ntrailer\n<< /Root 1 0 R >>\n%%EOF\n");
    out
}

/// Lit un littéral PDF `(…)` en partant de `start`, renvoie le texte décodé
/// et l'index suivant la parenthèse fermante.
fn read_pdf_literal(data: &[u8], start: usize) -> (String, usize) {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut i = start;
    while i < data.len() {
        let b = data[i];
        match b {
            b'\\' => {
                i += 1;
                if i >= data.len() {
                    break;
                }
                let e = data[i];
                match e {
                    b'n' => out.push(b'\n'),
                    b'r' => out.push(b'\r'),
                    b't' => out.push(b'\t'),
                    b'b' => out.push(0x08),
                    b'f' => out.push(0x0C),
                    b'0'..=b'7' => {
                        let mut val = (e - b'0') as u32;
                        let mut k = 0;
                        while k < 2 && i + 1 < data.len() && (b'0'..=b'7').contains(&data[i + 1]) {
                            i += 1;
                            val = val * 8 + (data[i] - b'0') as u32;
                            k += 1;
                        }
                        out.push(val as u8);
                    }
                    other => out.push(other),
                }
                i += 1;
            }
            b'(' => {
                // La parenthèse ouvrante du littéral n'appartient pas au
                // contenu ; celles imbriquées, si.
                if depth > 0 {
                    out.push(b'(');
                }
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    i += 1;
                    break;
                }
                out.push(b')');
                i += 1;
            }
            _ => {
                out.push(b);
                i += 1;
            }
        }
    }
    (crate::structure::decode_pdf_text(&out), i)
}

/// Lecteur tolérant de FDF : trouve les couples `/T` / `/V`.
fn parse_fdf(data: &[u8]) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut pending: Option<String> = None;
    let mut i = 0;
    while i < data.len() {
        if data[i] == b'/' && i + 1 < data.len() && (data[i + 1] == b'T' || data[i + 1] == b'V') {
            let key = data[i + 1];
            let mut j = i + 2;
            while j < data.len() && data[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < data.len() && data[j] == b'(' {
                let (text, next) = read_pdf_literal(data, j);
                if key == b'T' {
                    pending = Some(text);
                } else if let Some(name) = pending.take() {
                    pairs.push((name, text));
                }
                i = next;
                continue;
            }
        }
        i += 1;
    }
    pairs
}

fn parse_csv(data: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = data.chars().collect();
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_quotes {
            if c == '"' {
                if i + 1 < chars.len() && chars[i + 1] == '"' {
                    field.push('"');
                    i += 2;
                } else {
                    in_quotes = false;
                    i += 1;
                }
            } else {
                field.push(c);
                i += 1;
            }
        } else {
            match c {
                '"' => {
                    in_quotes = true;
                    i += 1;
                }
                ',' => {
                    record.push(std::mem::take(&mut field));
                    i += 1;
                }
                '\r' => i += 1,
                '\n' => {
                    record.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut record));
                    i += 1;
                }
                _ => {
                    field.push(c);
                    i += 1;
                }
            }
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        rows.push(record);
    }

    // La première ligne est l'en-tête `name,value`.
    rows.into_iter()
        .skip(1)
        .filter_map(|row| match row.len() {
            0 => None,
            1 => Some((row[0].clone(), String::new())),
            _ => Some((row[0].clone(), row[1].clone())),
        })
        .collect()
}

/// Lit une chaîne JSON en partant de `start` (le guillemet ouvrant).
fn read_json_string(chars: &[char], start: usize) -> (String, usize) {
    let mut out = String::new();
    let mut i = start + 1;
    while i < chars.len() {
        match chars[i] {
            '"' => {
                i += 1;
                break;
            }
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                match chars[i] {
                    'n' => out.push('\n'),
                    'r' => out.push('\r'),
                    't' => out.push('\t'),
                    'b' => out.push('\u{8}'),
                    'f' => out.push('\u{c}'),
                    'u' => {
                        let mut code = 0u32;
                        let mut k = 0;
                        while k < 4 && i + 1 < chars.len() {
                            let Some(d) = chars[i + 1].to_digit(16) else {
                                break;
                            };
                            code = code * 16 + d;
                            i += 1;
                            k += 1;
                        }
                        if let Some(c) = char::from_u32(code) {
                            out.push(c);
                        }
                    }
                    other => out.push(other),
                }
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, i)
}

/// Lecteur tolérant de JSON : associe les clés `name` et `value`.
fn parse_json(data: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = data.chars().collect();
    let mut pairs = Vec::new();
    let mut pending: Option<String> = None;
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '"' {
            i += 1;
            continue;
        }
        let (key, after_key) = read_json_string(&chars, i);
        let mut j = after_key;
        while j < chars.len() && chars[j].is_ascii_whitespace() {
            j += 1;
        }
        let mut consumed = after_key;
        if j < chars.len() && chars[j] == ':' {
            j += 1;
            while j < chars.len() && chars[j].is_ascii_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j] == '"' {
                let (value, after_value) = read_json_string(&chars, j);
                match key.as_str() {
                    "name" => pending = Some(value),
                    "value" => {
                        if let Some(name) = pending.take() {
                            pairs.push((name, value));
                        }
                    }
                    _ => {}
                }
                consumed = after_value;
            }
        }
        i = consumed;
    }
    pairs
}

fn parse_xfdf(data: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut rest = data;
    while let Some(pos) = rest.find("name=\"") {
        rest = &rest[pos + 6..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let name = unescape_xml(&rest[..end]);
        rest = &rest[end + 1..];
        if let Some(vstart) = rest.find("<value>") {
            let after = &rest[vstart + 7..];
            if let Some(vend) = after.find("</value>") {
                pairs.push((name, unescape_xml(&after[..vend])));
                rest = &after[vend + 8..];
            }
        }
    }
    pairs
}
