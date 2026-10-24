// ────────────────────────────────────────────────────────────────────────
// Construction du flux de contenu
// ────────────────────────────────────────────────────────────────────────
/// Largeur approchée d'un texte : l'Helvetica vaut ~0.5 cadratin par
/// caractère. Suffisant pour centrer, impossible pour ajuster au point.
use super::*;
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.5
}

/// Écrit un littéral PDF `(…)`, encodé en WinAnsi pour la police posée.
pub(crate) fn push_literal(out: &mut Vec<u8>, text: &str) {
    out.push(b'(');
    out.extend_from_slice(&encode_pdf_literal(text));
    out.push(b')');
}

/// Écrit un texte posé en (x, y).
pub(crate) fn push_text_line(out: &mut Vec<u8>, size: f32, x: f32, y: f32, text: &str) {
    out.extend_from_slice(
        format!(
            "BT\n/{WM_FONT_NAME} {size:.2} Tf\n0 0 0 rg\n1 0 0 1 {x:.2} {y:.2} Tm\n"
        )
        .as_bytes(),
    );
    push_literal(out, text);
    out.extend_from_slice(b" Tj\nET\n");
}

/// Écrit une ligne gauche / centré / droite sur une même base.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_three(
    out: &mut Vec<u8>,
    size: f32,
    margin: f32,
    y: f32,
    width: f32,
    left: &str,
    center: &str,
    right: &str,
) {
    if !left.is_empty() {
        push_text_line(out, size, margin, y, left);
    }
    if !center.is_empty() {
        push_text_line(out, size, (width - text_width(center, size)) / 2.0, y, center);
    }
    if !right.is_empty() {
        push_text_line(
            out,
            size,
            (width - margin - text_width(right, size)).max(margin),
            y,
            right,
        );
    }
}

/// Dessine une occurrence du filigrane, tournée d'un angle donné.
pub(crate) fn paint_watermark_text(
    out: &mut Vec<u8>,
    wm: &Watermark,
    size: f32,
    cos: f32,
    sin: f32,
    x: f32,
    y: f32,
) {
    let [r, g, b] = wm.color;
    out.extend_from_slice(
        format!(
            "BT\n/{WM_FONT_NAME} {size:.2} Tf\n{:.3} {:.3} {:.3} rg\n\
             {cos:.4} {sin:.4} {:.4} {cos:.4} {x:.2} {y:.2} Tm\n",
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            -sin
        )
        .as_bytes(),
    );
    push_literal(out, &wm.text);
    out.extend_from_slice(b" Tj\nET\n");
}

/// Flux de contenu d'un filigrane : centré, ou répété en trame.
pub(crate) fn watermark_ops(wm: &Watermark, width: f32, height: f32) -> Vec<u8> {
    let size = wm.font_size.clamp(4.0, 400.0);
    let radians = wm.rotation_deg.to_radians();
    let (cos, sin) = (radians.cos(), radians.sin());

    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");
    // L'état graphique porte l'opacité : sans lui le texte serait opaque.
    if wm.opacity < 1.0 {
        out.extend_from_slice(format!("/{WM_GS_NAME} gs\n").as_bytes());
    }

    if wm.tiled {
        let gap = wm.tile_gap.clamp(0.0, 600.0);
        let mut step_x = text_width(&wm.text, size) + gap;
        let mut step_y = size + gap;
        // Une page démesurée — affiche, plan de ville, ou boîte héritée d'un
        // fichier hostile — demanderait des centaines de milliers
        // d'opérations. L'espacement s'élargit plutôt que d'abandonner la
        // trame, qui doit continuer à couvrir la page.
        const MAX_TILES: f32 = 400.0;
        let estimated = (width / step_x + 1.0) * (height / step_y + 1.0);
        if estimated > MAX_TILES {
            let factor = (estimated / MAX_TILES).sqrt();
            step_x *= factor;
            step_y *= factor;
        }
        let mut y = size;
        while y < height + step_y {
            let mut x = 0.0_f32;
            while x < width + step_x {
                paint_watermark_text(&mut out, wm, size, cos, sin, x, y);
                x += step_x;
            }
            y += step_y;
        }
    } else {
        let text_w = text_width(&wm.text, size);
        const MARGIN: f32 = 24.0;
        let x = match wm.position {
            WatermarkPosition::TopLeft
            | WatermarkPosition::MiddleLeft
            | WatermarkPosition::BottomLeft => MARGIN,
            WatermarkPosition::TopCenter
            | WatermarkPosition::Center
            | WatermarkPosition::BottomCenter => ((width - text_w) / 2.0).max(2.0),
            WatermarkPosition::TopRight
            | WatermarkPosition::MiddleRight
            | WatermarkPosition::BottomRight => (width - text_w - MARGIN).max(2.0),
        };
        let y = match wm.position {
            WatermarkPosition::TopLeft
            | WatermarkPosition::TopCenter
            | WatermarkPosition::TopRight => (height - size - MARGIN).max(2.0),
            WatermarkPosition::MiddleLeft
            | WatermarkPosition::Center
            | WatermarkPosition::MiddleRight => height / 2.0 - size * 0.35,
            WatermarkPosition::BottomLeft
            | WatermarkPosition::BottomCenter
            | WatermarkPosition::BottomRight => MARGIN,
        };
        paint_watermark_text(
            &mut out,
            wm,
            size,
            cos,
            sin,
            x + wm.offset_x,
            y + wm.offset_y,
        );
    }

    out.extend_from_slice(b"Q\n");
    out
}

/// Flux de contenu de l'en-tête et du pied d'une page.
///
/// Renvoie `None` quand il n'y a rien à écrire : la page est alors laissée
/// intacte, ce qui rend la numérotation « sauf première page » exacte.
pub(crate) fn header_footer_ops(
    hf: &HeaderFooter,
    page: u32,
    total: u32,
    width: f32,
    height: f32,
) -> Option<Vec<u8>> {
    // Numérotation : absente avant `start_page`, et sur la première page si
    // `skip_first_page` est actif.
    let number = match hf.page_numbers {
        Some(style) if page >= hf.start_page.max(1) && !(hf.skip_first_page && page == 1) => {
            let label = page - hf.start_page.max(1) + 1;
            Some(style.render(label, total))
        }
        _ => None,
    };

    // Macros : {page}, {pages} (total) et {label} (numéro courant).
    let label = page
        .saturating_sub(hf.start_page.max(1))
        .saturating_add(1)
        .to_string();
    let expand = |text: &str| {
        text.replace("{page}", &page.to_string())
            .replace("{pages}", &total.to_string())
            .replace("{total}", &total.to_string())
            .replace("{label}", &label)
    };

    // Le numéro rejoint le centre du pied de page.
    let footer_center = expand(&hf.footer.center);
    let footer_center = match (&number, footer_center.trim()) {
        (Some(n), "") => n.clone(),
        (Some(n), c) => format!("{c} {n}"),
        (None, _) => footer_center.trim().to_string(),
    };
    let header_left = expand(&hf.header.left);
    let header_center = expand(&hf.header.center);
    let header_right = expand(&hf.header.right);
    let footer_left = expand(&hf.footer.left);
    let footer_right = expand(&hf.footer.right);

    let header_band = !header_left.trim().is_empty()
        || !header_center.trim().is_empty()
        || !header_right.trim().is_empty();
    let footer_band = !footer_left.trim().is_empty()
        || !footer_center.trim().is_empty()
        || !footer_right.trim().is_empty()
        || number.is_some();
    if !header_band && !footer_band {
        return None;
    }

    let size = hf.font_size.clamp(4.0, 72.0);
    let margin = hf.margin.clamp(0.0, width.min(height) / 3.0);

    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");
    if header_band {
        push_three(
            &mut out,
            size,
            margin,
            height - margin - size,
            width,
            &header_left,
            &header_center,
            &header_right,
        );
    }
    if footer_band {
        push_three(
            &mut out,
            size,
            margin,
            margin,
            width,
            &footer_left,
            &footer_center,
            &footer_right,
        );
    }
    out.extend_from_slice(b"Q\n");
    Some(out)
}

/// Flux de contenu d'un aplat de couleur, sur toute la boîte visible.
pub(crate) fn background_color_ops(bg: &Background, view: PageView) -> Vec<u8> {
    let [r, g, b] = bg.color;
    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");
    if bg.opacity < 1.0 {
        out.extend_from_slice(format!("/{WM_GS_NAME} gs\n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "{:.3} {:.3} {:.3} rg\n{:.2} {:.2} {:.2} {:.2} re f\n",
            r as f32 / 255.0,
            g as f32 / 255.0,
            b as f32 / 255.0,
            view.x,
            view.y,
            view.width,
            view.height
        )
        .as_bytes(),
    );
    out.extend_from_slice(b"Q\n");
    out
}

/// Flux de contenu d'une image d'arrière-plan, recadrée sur la boîte visible.
///
/// L'image couvre la boîte (rapport d'échelle le plus grand des deux axes),
/// multipliée par `scale` : `scale > 1` agrandit et rogne, `scale < 1`
/// laisse une marge. Le rectangle de recadrage garde l'image dans la boîte.
pub(crate) fn background_image_ops(view: PageView, img_w: f32, img_h: f32, scale: f32) -> Vec<u8> {
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let cover = (view.width / img_w).max(view.height / img_h) * scale;
    let w = img_w * cover;
    let h = img_h * cover;
    let x = view.x + (view.width - w) / 2.0;
    let y = view.y + (view.height - h) / 2.0;
    format!(
        "q\n{:.2} {:.2} {:.2} {:.2} re W n\n\
         {w:.2} 0 0 {h:.2} {x:.2} {y:.2} cm\n/{BG_XOBJECT_NAME} Do\nQ\n",
        view.x, view.y, view.width, view.height
    )
    .into_bytes()
}

/// Flux de contenu d'un numéro Bates, placé selon la position choisie.
pub(crate) fn bates_ops(b: &Bates, label: &str, view: PageView) -> Vec<u8> {
    let size = b.font_size.clamp(4.0, 72.0);
    let tw = text_width(label, size);
    let margin = b.margin.clamp(0.0, view.width.min(view.height) / 3.0);
    let left = view.x + margin;
    let right = (view.x + view.width - margin - tw).max(left);
    let center = view.x + (view.width - tw) / 2.0;
    let top = view.y + view.height - margin - size;
    let bottom = view.y + margin;

    let (x, y) = match b.position {
        BatesPosition::TopLeft => (left, top),
        BatesPosition::TopCenter => (center, top),
        BatesPosition::TopRight => (right, top),
        BatesPosition::BottomLeft => (left, bottom),
        BatesPosition::BottomCenter => (center, bottom),
        BatesPosition::BottomRight => (right, bottom),
    };

    let [r, g, bl] = b.color;
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "q\n{:.3} {:.3} {:.3} rg\nBT\n/{WM_FONT_NAME} {size:.2} Tf\n\
             1 0 0 1 {x:.2} {y:.2} Tm\n",
            r as f32 / 255.0,
            g as f32 / 255.0,
            bl as f32 / 255.0
        )
        .as_bytes(),
    );
    push_literal(&mut out, label);
    out.extend_from_slice(b" Tj\nET\nQ\n");
    out
}

/// Construit le flux XObject image d'un arrière-plan depuis un fichier.
///
/// Un JPEG est recopié tel quel (`/DCTDecode`) ; tout autre format est
/// décodé en RGB puis compressé en zlib. Renvoie `(flux, largeur, hauteur)`.
pub(crate) fn background_image_stream(path: &std::path::Path) -> Result<(Stream, u32, u32)> {
    let raw = std::fs::read(path)?;
    let (width, height, data, filter) = if raw.starts_with(&[0xFF, 0xD8]) {
        let (w, h) = image::image_dimensions(path)
            .map_err(|e| Error::InvalidInput(format!("{} : {e}", path.display())))?;
        (w, h, raw, "DCTDecode")
    } else {
        let (pixels, w, h) = load_image_rgb(path)?;
        (w, h, flate_compress(&pixels)?, "FlateDecode")
    };

    let dict = dictionary! {
        "Type" => Object::Name(b"XObject".to_vec()),
        "Subtype" => Object::Name(b"Image".to_vec()),
        "Width" => Object::Integer(width as i64),
        "Height" => Object::Integer(height as i64),
        "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
        "BitsPerComponent" => Object::Integer(8),
        "Filter" => Object::Name(filter.as_bytes().to_vec()),
    };
    Ok((Stream::new(dict, data), width, height))
}

/// Dictionnaire des flux posés par ce module (clé de repérage).
pub(crate) fn marked_dict() -> Dictionary {
    marked_dict_with(WM_MARKER)
}

/// Dictionnaire marquant un flux par la clé de repérage `key`.
pub(crate) fn marked_dict_with(key: &str) -> Dictionary {
    let mut dict = Dictionary::new();
    dict.set(key, Object::Boolean(true));
    dict
}

/// Chiffres romains minuscules. `0` n'a pas d'écriture romaine : on le
// rend tel quel plutôt que de produire une chaîne vide.
pub(crate) fn to_roman(mut n: u32) -> String {
    if n == 0 {
        return "0".to_string();
    }
    const TABLE: [(u32, &str); 13] = [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ];
    let mut out = String::new();
    for (value, letters) in TABLE {
        while n >= value {
            out.push_str(letters);
            n -= value;
        }
    }
    out
}

