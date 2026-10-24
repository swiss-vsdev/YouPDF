//! Écriture d'objets PDF, dates et échappements partagés par la signature.

use youpdf_shared::Result;

pub(super) fn resolve_dict(
    document: &lopdf::Document,
    object: &lopdf::Object,
) -> Option<lopdf::Dictionary> {
    match object {
        lopdf::Object::Dictionary(dict) => Some(dict.clone()),
        lopdf::Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .cloned(),
        _ => None,
    }
}

pub(super) fn resolve_array(
    document: &lopdf::Document,
    object: &lopdf::Object,
) -> Option<Vec<lopdf::Object>> {
    match object {
        lopdf::Object::Array(array) => Some(array.clone()),
        lopdf::Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|object| object.as_array().ok())
            .cloned(),
        _ => None,
    }
}

pub(super) fn signature_field_name(document: &lopdf::Document, signature: u32) -> Option<String> {
    for object in document.objects.values() {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        let value = dict
            .get(b"V")
            .ok()
            .and_then(|object| object.as_reference().ok())
            .map(|(id, _)| id);
        if value != Some(signature) {
            continue;
        }
        if let Ok(lopdf::Object::String(bytes, _)) = dict.get(b"T") {
            return Some(String::from_utf8_lossy(bytes).into_owned());
        }
    }
    None
}

pub(super) fn pdf_string(object: Option<&lopdf::Object>) -> String {
    match object {
        Some(lopdf::Object::String(bytes, _)) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        _ => String::new(),
    }
}

/// Sérialise un objet PDF sans le wrapper `n 0 obj … endobj`.
pub(super) fn object_bytes(object: &lopdf::Object) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_pdf_object(&mut out, object);
    Ok(out)
}

fn write_pdf_object(out: &mut Vec<u8>, object: &lopdf::Object) {
    match object {
        lopdf::Object::Null => out.extend_from_slice(b"null"),
        lopdf::Object::Boolean(value) => {
            out.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        lopdf::Object::Integer(value) => out.extend_from_slice(value.to_string().as_bytes()),
        lopdf::Object::Real(value) => {
            let text = format!("{value}");
            out.extend_from_slice(if text.contains('.') { text } else { format!("{text}.0") }.as_bytes());
        }
        lopdf::Object::Name(name) => {
            out.push(b'/');
            for &byte in name {
                if b" \t\n\r\x0C()<>[]{}/%#".contains(&byte) || !(33..=126).contains(&byte) {
                    out.extend_from_slice(format!("#{byte:02X}").as_bytes());
                } else {
                    out.push(byte);
                }
            }
        }
        lopdf::Object::String(bytes, format) => match format {
            lopdf::StringFormat::Literal => {
                out.push(b'(');
                for &byte in bytes {
                    match byte {
                        b'(' | b')' | b'\\' => {
                            out.push(b'\\');
                            out.push(byte);
                        }
                        b'\r' => out.extend_from_slice(b"\\r"),
                        b'\n' => out.extend_from_slice(b"\\n"),
                        _ => out.push(byte),
                    }
                }
                out.push(b')');
            }
            lopdf::StringFormat::Hexadecimal => {
                out.push(b'<');
                out.extend_from_slice(hex_encode(bytes).as_bytes());
                out.push(b'>');
            }
        },
        lopdf::Object::Array(array) => {
            out.push(b'[');
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    out.push(b' ');
                }
                write_pdf_object(out, item);
            }
            out.push(b']');
        }
        lopdf::Object::Dictionary(dict) => write_pdf_dictionary(out, dict),
        lopdf::Object::Reference((id, generation)) => {
            out.extend_from_slice(format!("{id} {generation} R").as_bytes())
        }
        lopdf::Object::Stream(stream) => {
            let mut dict = stream.dict.clone();
            dict.set("Length", lopdf::Object::Integer(stream.content.len() as i64));
            write_pdf_dictionary(out, &dict);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&stream.content);
            out.extend_from_slice(b"\nendstream");
        }
    }
}

fn write_pdf_dictionary(out: &mut Vec<u8>, dict: &lopdf::Dictionary) {
    out.extend_from_slice(b"<<");
    for (key, value) in dict.iter() {
        out.push(b' ');
        out.push(b'/');
        for &byte in key {
            if b" \t\n\r\x0C()<>[]{}/%#".contains(&byte) || !(33..=126).contains(&byte) {
                out.extend_from_slice(format!("#{byte:02X}").as_bytes());
            } else {
                out.push(byte);
            }
        }
        out.push(b' ');
        write_pdf_object(out, value);
    }
    out.extend_from_slice(b" >>");
}

/// Littéral PDF échappé (nom, motif, lieu).
pub(super) fn literal_string(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 2);
    out.push(b'(');
    out.extend_from_slice(&escape_sig(value));
    out.push(b')');
    out
}

pub(super) fn signature_appearance_content(
    width: f64,
    height: f64,
    name: &str,
    reason: &str,
    date: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        format!("q 0.96 0.97 1 rg 0 0 {width:.2} {height:.2} re f Q\n").as_bytes(),
    );
    out.extend_from_slice(
        format!("q 0.15 0.35 0.75 RG 1.2 w 1 1 {:.2} {:.2} re S Q\n", width - 2.0, height - 2.0)
            .as_bytes(),
    );
    let text = |font_size: f64, y: f64, value: &str| {
        let mut line = Vec::new();
        line.extend_from_slice(
            format!("BT /Helv {font_size:.1} Tf 0.08 0.12 0.22 rg 6 {y:.2} Td ").as_bytes(),
        );
        line.extend_from_slice(&literal_string(value));
        line.extend_from_slice(b" Tj ET\n");
        line
    };
    out.extend(text(10.0, (height - 18.0).max(8.0), name));
    if !reason.trim().is_empty() {
        out.extend(text(8.0, (height / 2.0 - 3.0).max(7.0), reason));
    }
    out.extend(text(7.0, 5.0, date));
    out
}

pub(super) fn pdf_date_now() -> String {
    let now = chrono::Local::now();
    let offset = now.offset().local_minus_utc();
    let sign = if offset >= 0 { '+' } else { '-' };
    let minutes = offset.abs() / 60;
    format!(
        "D:{}{}{:02}'{:02}'",
        now.format("%Y%m%d%H%M%S"),
        sign,
        minutes / 60,
        minutes % 60
    )
}

pub(super) fn pdf_date_short() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

pub(super) fn last_startxref(bytes: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(bytes);
    let index = text.rfind("startxref")?;
    let digits: String = text[index + "startxref".len()..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

pub(super) fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

pub(super) fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

pub(super) fn dn_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, ',' | '+' | '=' | '\\' | '<' | '>' | ';' | '"'))
        .collect()
}

/// Échappe et encode en WinAnsi le libellé d'une signature.
///
/// Ce texte vient d'un champ saisi par un tiers (nom du signataire, motif
///…). Un `)` non échappé ferme le littéral PDF et permet d'ajouter des
/// opérateurs au flux de contenu ; `<` / `>` ne sont pas non plus échappés,
/// ce qui suffit à composer une chaîne hexadécimale valide donc à écrire du
/// texte arbitraire dans le document signé.
pub(super) fn escape_sig(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' | ')' | '<' | '>' | '\\' => {
                out.push(b'\\');
                out.push(c as u8);
            }
            '\n' | '\r' | '\t' | '\u{0}' => out.push(b' '),
            c if (c as u32) < 0x80 => out.push(c as u8),
            // Idem `encode_pdf_literal` : un octet WinAnsi, jamais du UTF-8.
            c => out.push(crate::edit::winansi_byte_pub(c).unwrap_or(b'?')),
        }
    }
    out
}
