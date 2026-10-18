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
