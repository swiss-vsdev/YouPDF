//! Polices d'interface : SF Pro (Apple), lue sur le système au démarrage.
//!
//! Rien n'est embarqué (licences, taille) : les fichiers sont lus à
//! l'exécution. Toute erreur — absent, tronqué, format non géré par ab_glyph —
//! est ignorée et laisse le repli Ubuntu-Light d'egui en place, sans panique.

use egui::{FontData, FontDefinitions, FontFamily};

/// Polices Apple, par ordre de préférence.
const SF: &[&str] = &["/System/Library/Fonts/SFNS.ttf"];
const PINGFANG: &[&str] = &["/System/Library/Fonts/PingFang.ttc"];
const HELVETICA: &[&str] = &[
    "/System/Library/Fonts/HelveticaNeue.ttc",
    "/System/Library/Fonts/Helvetica.ttc",
];

/// Installe la famille proportionnelle. Monospace n'est pas touché : Hack
/// reste la police du code.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);

    for (name, paths) in [("SF", SF), ("PingFang", PINGFANG), ("Helvetica", HELVETICA)] {
        if let Some(data) = load(paths) {
            fonts.font_data.insert(name.to_owned(), data);
        }
    }

    // SF d'abord, Ubuntu-Light en repli garanti, puis PingFang/Helvetica,
    // enfin Phosphor (icônes), les emojis et Hack — dernier car seul à
    // fournir ⇧ (U+21E7) et divers symboles.
    let mut family = Vec::new();
    for name in ["SF", "Ubuntu-Light", "PingFang", "Helvetica"] {
        if fonts.font_data.contains_key(name) {
            family.push(name.to_owned());
        }
    }
    for name in ["phosphor", "NotoEmoji-Regular", "emoji-icon-font", "Hack"] {
        if fonts.font_data.contains_key(name) {
            family.push(name.to_owned());
        }
    }
    if !family.is_empty() {
        fonts.families.insert(FontFamily::Proportional, family);
    }

    ctx.set_fonts(fonts);
}

fn load(paths: &[&str]) -> Option<FontData> {
    paths.iter().find_map(|path| {
        let bytes = std::fs::read(path).ok()?;
        if renderable(&bytes) {
            Some(FontData::from_owned(bytes))
        } else {
            None
        }
    })
}

/// Vérifie qu'ab_glyph saura parser le fichier et dessiner ses glyphes.
///
/// Sans ce filtre, une police sans table de contours (par ex. PingFang UI, qui
/// n'a que les tables Apple `hvgl`/`cidg`) serait acceptée par epaint puis
/// rendue vide, et un fichier tronqué ferait paniquer `set_fonts`.
fn renderable(data: &[u8]) -> bool {
    const TTCF: u32 = 0x7474_6366;
    const TRUE: u32 = 0x7472_7565;
    const OTTO: u32 = 0x4f54_544f;
    const TTF: u32 = 0x0001_0000;

    let face = match be32(data, 0) {
        Some(TTF | TRUE | OTTO) => 0,
        Some(TTCF) if be32(data, 8).unwrap_or(0) > 0 => be32(data, 12).unwrap_or(0) as usize,
        _ => return false,
    };

    let num_tables = match be16(data, face + 4) {
        Some(n) if n > 0 && n <= 4096 => n as usize,
        _ => return false,
    };

    let (mut head, mut hhea, mut maxp, mut cmap) = (false, false, false, false);
    let (mut glyf, mut loca, mut cff) = (false, false, false);
    for i in 0..num_tables {
        let rec = face + 12 + i * 16;
        let (tag, off, len) = match (be32(data, rec), be32(data, rec + 8), be32(data, rec + 12)) {
            (Some(t), Some(o), Some(l)) => (t, o as usize, l as usize),
            _ => return false,
        };
        // Les tables d'une collection sont adressées depuis le début du fichier.
        match off.checked_add(len) {
            Some(end) if end <= data.len() => {}
            _ => return false,
        }
        match tag {
            0x6865_6164 => head = true,
            0x6868_6561 => hhea = true,
            0x6d61_7870 => maxp = true,
            0x636d_6170 => cmap = true,
            0x676c_7966 => glyf = true,
            0x6c6f_6361 => loca = true,
            0x4346_4620 => cff = true,
            _ => {}
        }
    }
    head && hhea && maxp && cmap && (cff || (glyf && loca))
}

fn be16(data: &[u8], off: usize) -> Option<u16> {
    let b = data.get(off..off.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn be32(data: &[u8], off: usize) -> Option<u32> {
    let b = data.get(off..off.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
