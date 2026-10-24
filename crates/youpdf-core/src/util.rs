//! Aides partagées : compression zlib et échappement de texte structuré.
//!
//! Ces fonctions étaient dupliquées dans `watermark`, `create`, `pdfa`,
//! `office`, `forms` et `convert`. Elles vivent ici pour qu'une correction
//! (échappement, bornes de compression) profite à tous les appelants.

use std::io::{Read, Write};

use youpdf_shared::{Error, Result};

/// Compresse `data` en zlib (`/FlateDecode`).
pub fn flate_compress(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::write::ZlibEncoder;
    use flate2::Compression;

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data)
        .map_err(|e| Error::Internal(format!("compression zlib : {e}")))?;
    encoder
        .finish()
        .map_err(|e| Error::Internal(format!("compression zlib : {e}")))
}

/// Décompresse un flux zlib (`/FlateDecode`).
pub fn flate_decompress(data: &[u8]) -> Result<Vec<u8>> {
    use flate2::read::ZlibDecoder;

    let mut decoder = ZlibDecoder::new(data);
    let mut out = Vec::new();
    decoder
        .read_to_end(&mut out)
        .map_err(|e| Error::Internal(format!("décompression zlib : {e}")))?;
    Ok(out)
}

/// Échappe un texte pour un contenu XML (`&`, `<`, `>`, `"`, `'`).
pub fn escape_xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Inverse de [`escape_xml`].
///
/// `&amp;` est traité en dernier : sinon `&amp;lt;` deviendrait `<`.
pub fn unescape_xml(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Encode un champ CSV selon la RFC 4180 : guillemets uniquement si le champ
/// contient `"`, `,`, `\n` ou `\r`, guillemets internes doublés.
pub fn csv_escape(text: &str) -> String {
    if text.contains(['"', ',', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flate_round_trips() {
        let data = b"the quick brown fox ".repeat(20);
        let packed = flate_compress(&data).unwrap();
        assert!(!packed.is_empty());
        assert_eq!(flate_decompress(&packed).unwrap(), data);
    }

    #[test]
    fn flate_handles_empty_input() {
        let packed = flate_compress(b"").unwrap();
        assert_eq!(flate_decompress(&packed).unwrap(), b"");
    }

    #[test]
    fn xml_escape_round_trips() {
        let raw = "a & b < c > d \" e ' f";
        assert_eq!(unescape_xml(&escape_xml(raw)), raw);
    }

    #[test]
    fn ampersand_unescapes_last() {
        // `&amp;lt;` doit redevenir `&lt;`, pas `<`.
        assert_eq!(unescape_xml("&amp;lt;"), "&lt;");
    }

    #[test]
    fn csv_quotes_only_when_needed() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b"), "\"a,b\"");
        assert_eq!(csv_escape("say \"hi\""), "\"say \"\"hi\"\"\"");
    }
}

// ---------------------------------------------------------------------------
// Libellés lisibles : formats de page usuels et tailles en octets.
// ---------------------------------------------------------------------------

/// Libellé lisible d'un format de page : nom usuel si connu, sinon la taille.
pub fn page_size_label(width: f32, height: f32) -> String {
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




/// Taille lisible : octets sous 1 Ko, puis Ko et Mo.
pub fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}


#[cfg(test)]
mod label_tests {
    use super::{format_size, page_size_label};


/// Les formats usuels portent un nom ; les autres la taille brute.
#[test]
fn page_size_label_names_common_formats() {
    assert_eq!(page_size_label(595.0, 842.0), "A4 portrait");
    assert_eq!(page_size_label(842.0, 595.0), "A4 landscape");
    assert_eq!(page_size_label(612.0, 792.0), "Letter portrait");
    assert_eq!(page_size_label(1000.0, 500.0), "1000 × 500 pt");
}

    #[test]
    fn format_size_scales_units() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(2048), "2.0 KB");
        assert_eq!(format_size(5 * 1024 * 1024), "5.0 MB");
    }
}
