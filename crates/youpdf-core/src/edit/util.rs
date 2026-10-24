//! Outils partagés : lecture d'une image disque, encodage WinAnsi d'un littéral.

use youpdf_shared::{Error, Result};

/// Décode une image disque (PNG ou JPEG) en pixels RGB 8 bits.
///
/// Rend `(pixels, largeur, hauteur)`, prêt pour
/// [`Document::insert_image_bitmap`](crate::Document::insert_image_bitmap).
pub fn load_image_rgb(
    path: impl AsRef<std::path::Path>,
) -> Result<(Vec<u8>, u32, u32)> {
    let path = path.as_ref();
    let mut reader = image::ImageReader::open(path)
        .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
    // Plafonds explicites : une image « bombe » (dimensions énormes annoncées
    // dans l'en-tête) est refusée au décodage plutôt qu'après allocation.
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(20_000);
    limits.max_image_height = Some(20_000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader
        .with_guessed_format()
        .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?
        .decode()
        .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
    let rgb = image.to_rgb8();
    let (width, height) = rgb.dimensions();
    Ok((rgb.into_raw(), width, height))
}

/// Échappe une chaîne et l'encode en octets WinAnsi pour un littéral PDF `(…)`.
///
/// La page de codes est celle de la police posée (`WinAnsiEncoding`) : il
/// faut donc écrire **un octet** par caractère. Renvoyer une `String` et la
/// convertir en UTF-8 produisait deux octets pour chaque accentué — « café »
/// sortait en `caf\xc3\xa9`, que le lecteur affiche « cafÃ© ».
pub fn encode_pdf_literal(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' => out.extend_from_slice(b"\\("),
            ')' => out.extend_from_slice(b"\\)"),
            '\\' => out.extend_from_slice(b"\\\\"),
            '\n' | '\r' | '\t' => out.push(b' '),
            c if (c as u32) < 0x80 => out.push(c as u8),
            // Hors Latin-1/CP1252 (CJK, cyrillique …) : la police standard
            // n'a pas de glyphe garanti → point d'interrogation plutôt
            // qu'un octet arbitraire rendu avec un glyphe au hasard.
            c => out.push(winansi_byte(c).unwrap_or(b'?')),
        }
    }
    out
}

/// Convertit un caractère en octet WinAnsi, avec les écarts usuels entre
/// Latin-1 et CP1252 (guillemets, tirets, points de suspension…).
pub(crate) fn winansi_byte_pub(c: char) -> Option<u8> {
    winansi_byte(c)
}

fn winansi_byte(c: char) -> Option<u8> {
    const CP1252: [(char, u8); 27] = [
        ('\u{20AC}', 0x80), ('\u{201A}', 0x82), ('\u{0192}', 0x83), ('\u{201E}', 0x84),
        ('\u{2026}', 0x85), ('\u{2020}', 0x86), ('\u{2021}', 0x87), ('\u{02C6}', 0x88),
        ('\u{2030}', 0x89), ('\u{0160}', 0x8A), ('\u{2039}', 0x8B), ('\u{0152}', 0x8C),
        ('\u{017D}', 0x8E), ('\u{2018}', 0x91), ('\u{2019}', 0x92), ('\u{201C}', 0x93),
        ('\u{201D}', 0x94), ('\u{2022}', 0x95), ('\u{2013}', 0x96), ('\u{2014}', 0x97),
        ('\u{02DC}', 0x98), ('\u{2122}', 0x99), ('\u{0161}', 0x9A), ('\u{203A}', 0x9B),
        ('\u{0153}', 0x9C), ('\u{017E}', 0x9E), ('\u{0178}', 0x9F),
    ];
    if let Some((_, b)) = CP1252.iter().find(|(ch, _)| *ch == c) {
        return Some(*b);
    }
    let u = c as u32;
    if (0xA0..=0xFF).contains(&u) {
        Some(u as u8)
    } else {
        None
    }
}
