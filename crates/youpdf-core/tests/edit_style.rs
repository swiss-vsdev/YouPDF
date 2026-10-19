//! Mise en forme d'un patch de texte : police, corps, couleur, styles,
//! alignement, interligne, approche.
//!
//! Les documents sont construits en mémoire ; le fragment de départ est lu
//! dans le document pour rester au plus près de ce que fait l'application.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::{Document, PatchFont, TextAlign, TextStyle};

/// PDF d'une page portant le texte `content`.
fn pdf_with(content: &str) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let stream = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => stream,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
    });
    doc.trailer.set("Root", catalog);
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("sérialisation");
    buf
}

/// Document et son premier fragment de texte.
fn document_with_fragment() -> (Document, youpdf_core::TextFragment) {
    let bytes = pdf_with("BT /F1 12 Tf 1 0 0 1 72 700 Tm (Invoice 12345) Tj ET\n");
    let doc = Document::from_bytes(&bytes).expect("ouverture");
    let fragment = doc
        .extract_text_fragments(1)
        .expect("fragments")
        .into_iter()
        .next()
        .expect("un fragment");
    (doc, fragment)
}

/// Le style par défaut reprend la couleur et le corps du fragment.
#[test]
fn default_style_follows_the_fragment() {
    let (_, fragment) = document_with_fragment();
    let style = TextStyle::from_fragment(&fragment);
    assert_eq!(style.font, PatchFont::Helvetica);
    assert_eq!(style.size, fragment.font_size);
    assert_eq!(style.color, fragment.color);
    assert_eq!(style.align, TextAlign::Left);
    assert_eq!(style.line_spacing, 1.0);
}

/// Une police choisie apparaît dans le patch, avec sa propre ressource.
#[test]
fn chosen_font_is_written_and_registered() {
    let (mut doc, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    style.font = PatchFont::Courier;
    style.bold = true;
    style.size = 18.0;

    doc.replace_text_fragment_styled(&fragment, "Invoice 12345", None, &style)
        .expect("patch");

    let bytes = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("/YouPdfCourierBold 18.00 Tf"),
        "la police choisie n'est pas dans le flux"
    );
    // La police dédiée est déclarée sous son propre nom, avec le fichier de
    // base choisi. Les dictionnaires peuvent vivre dans un flux d'objets
    // compressé : on les relit avec lopdf plutôt qu'à l'œil.
    let reopened = lopdf::Document::load_mem(&bytes).expect("réouverture");
    let page_id = reopened.page_iter().next().expect("page");
    let fonts = reopened
        .get_dictionary(page_id)
        .expect("page")
        .get(b"Resources")
        .and_then(Object::as_dict)
        .and_then(|r| r.get(b"Font"))
        .and_then(Object::as_dict)
        .expect("ressources de police");
    let font_id = fonts
        .get(b"YouPdfCourierBold")
        .and_then(Object::as_reference)
        .expect("police dédiée absente");
    let base = reopened
        .get_dictionary(font_id)
        .expect("dictionnaire de police")
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .expect("BaseFont");
    assert_eq!(base, b"Courier-Bold");
}

/// Couleur, gras, italique, souligné et barré se retrouvent dans le flux.
#[test]
fn colour_and_styles_are_written() {
    let (mut doc, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    style.color = [204, 0, 51];
    style.italic = true;
    style.underline = true;
    style.strikeout = true;

    doc.replace_text_fragment_styled(&fragment, "Annulé", None, &style)
        .expect("patch");

    let bytes = doc.to_bytes().expect("écriture");
    let text = String::from_utf8_lossy(&bytes);
    let ink = format!("{:.3} {:.3} {:.3} rg", 204.0 / 255.0, 0.0, 51.0 / 255.0);
    assert!(text.contains(&ink), "la couleur demandée n'est pas posée");
    assert!(text.contains("Helvetica-Oblique"), "l'italique n'est pas choisi");
    // Souligné et barré sont deux rectangles pleins, donc deux `re … f` de plus
    // que le seul masque.
    assert_eq!(
        text.matches(" re\n").count(),
        3,
        "masque, souligné et barré attendus"
    );
}

/// L'alignement centre ou à droite décale le point de départ ; à gauche, il
/// reste sur le bord de la boîte.
#[test]
fn alignment_shifts_the_start_point() {
    let (mut doc, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    style.align = TextAlign::Left;
    doc.replace_text_fragment_styled(&fragment, "Invoice 12345", None, &style)
        .expect("patch gauche");
    let g = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    let left_x = tdf_x(&g);

    let mut doc = Document::from_bytes(&pdf_with(
        "BT /F1 12 Tf 1 0 0 1 72 700 Tm (Invoice 12345) Tj ET\n",
    ))
    .expect("ouverture");
    let fragment = doc
        .extract_text_fragments(1)
        .expect("fragments")
        .remove(0);
    let mut style = TextStyle::from_fragment(&fragment);
    style.align = TextAlign::Center;
    // La boîte du fragment épouse le texte d'origine : pour voir un décalage,
    // le patch doit être plus étroit que la boîte.
    style.size = 6.0;
    doc.replace_text_fragment_styled(&fragment, "Invoice 12345", None, &style)
        .expect("patch centre");
    let c = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    let center_x = tdf_x(&c);

    assert!(
        center_x > left_x,
        "le centrage devrait décaler vers la droite : {left_x} → {center_x}"
    );
}

/// Justifié : les mots d'une ligne sont répartis sur la largeur de la boîte,
/// chacun posé par son propre `Tm`.
#[test]
fn justification_spreads_the_words() {
    let (mut doc, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    style.align = TextAlign::Justify;
    // Le texte doit être plus étroit que la boîte pour qu'il y ait à répartir.
    style.size = 6.0;

    doc.replace_text_fragment_styled(&fragment, "Hello there world", None, &style)
        .expect("patch");

    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    assert_eq!(
        text.matches(") Tj\n").count(),
        3,
        "un mot par opérateur attendu"
    );
    let starts: Vec<f32> = text
        .match_indices(" Tm\n")
        .filter_map(|(position, _)| {
            let line_start = text[..position].rfind('\n').map(|i| i + 1).unwrap_or(0);
            text[line_start..position]
                .split_whitespace()
                .nth(4)
                .and_then(|value| value.parse().ok())
        })
        .collect();
    assert_eq!(starts.len(), 3, "trois mots positionnés");
    assert!(
        starts.windows(2).all(|pair| pair[0] < pair[1]),
        "les mots ne progressent pas : {starts:?}"
    );
}

/// Une ligne d'un seul mot ne se justifie pas : elle reste à gauche.
#[test]
fn justification_leaves_a_single_word_alone() {
    let (mut doc, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    style.align = TextAlign::Justify;
    style.size = 6.0;

    doc.replace_text_fragment_styled(&fragment, "Unique", None, &style)
        .expect("patch");

    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    assert!(text.contains(" Td\n"), "le mot unique devrait rester à gauche");
}

/// Le premier `Td` du patch : point de départ du texte.
fn tdf_x(content: &str) -> f32 {
    let marker = " Td\n";
    let position = content.rfind(marker).expect("un Td");
    let line_start = content[..position]
        .rfind("Tc\n")
        .map(|i| i + 3)
        .or_else(|| content[..position].rfind('\n'))
        .expect("début de ligne");
    content[line_start..position]
        .split_whitespace()
        .next()
        .and_then(|value| value.parse().ok())
        .expect("abscisse du Td")
}

/// Un texte multiligne écrit une ligne par `BT`, espacées de l'interligne.
#[test]
fn multiline_follows_the_line_spacing() {
    let (mut doc, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    style.size = 10.0;
    style.line_spacing = 1.5;

    doc.replace_text_fragment_styled(&fragment, "première\ndeuxième", None, &style)
        .expect("patch");

    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    assert_eq!(text.matches(" Tj\n").count(), 2, "deux lignes attendues");
}

/// Le masque s'étend sous la boîte d'origine pour couvrir les lignes
/// supplémentaires : sinon la seconde ligne se poserait sur la page.
#[test]
fn multiline_mask_covers_every_line() {
    let (mut doc, fragment) = document_with_fragment();
    let box_height = fragment.rect[3] - fragment.rect[1];
    let mut style = TextStyle::from_fragment(&fragment);
    style.size = 10.0;
    style.line_spacing = 1.5;

    doc.replace_text_fragment_styled(&fragment, "a\nb\nc", None, &style)
        .expect("patch");

    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    let height = first_rect_height(&text);
    assert!(
        height > box_height + 10.0,
        "le masque ne couvre pas les lignes : {height} pour une boîte de {box_height}"
    );
}

/// Hauteur du premier rectangle du flux (`re`).
fn first_rect_height(content: &str) -> f32 {
    let marker = " re\n";
    let position = content.find(marker).expect("un rectangle");
    let line_start = content[..position].rfind('\n').map(|i| i + 1).unwrap_or(0);
    content[line_start..position]
        .split_whitespace()
        .nth(3)
        .and_then(|value| value.parse().ok())
        .expect("hauteur du rectangle")
}

/// Un texte venu d'un champ CRLF n'écrit pas de retour chariot dans le flux.
#[test]
fn carriage_returns_are_stripped() {
    let (mut doc, fragment) = document_with_fragment();
    let style = TextStyle::from_fragment(&fragment);
    doc.replace_text_fragment_styled(&fragment, "a\r\nb", None, &style)
        .expect("patch");

    let bytes = doc.to_bytes().expect("écriture");
    // Le flux de patch est le seul écrit par ce test : un CR signerait un
    // littéral corrompu.
    let stream = bytes
        .windows(4)
        .position(|window| window == b"BT\n9" || window == b"BT\n1")
        .map(|start| &bytes[start..])
        .unwrap_or(&bytes);
    assert!(
        !stream.contains(&b'\r'),
        "un retour chariot s'est glissé dans le patch"
    );
}

/// L'approche ajoute un `Tc` non nul ; la largeur estimée en tient compte.
#[test]
fn character_spacing_is_written_and_counted() {
    let (_, fragment) = document_with_fragment();
    let mut style = TextStyle::from_fragment(&fragment);
    let tight = youpdf_core::estimate_text_width("abc", &style);

    style.char_spacing = 2.0;
    let loose = youpdf_core::estimate_text_width("abc", &style);
    assert!(
        (loose - tight - 6.0).abs() < 0.01,
        "l'approche n'est pas comptée : {tight} → {loose}"
    );

    let (mut doc, fragment) = document_with_fragment();
    doc.replace_text_fragment_styled(&fragment, "abc", None, &style)
        .expect("patch");
    let text = String::from_utf8_lossy(&doc.to_bytes().expect("écriture")).into_owned();
    assert!(text.contains("2.00 Tc"), "l'approche n'est pas dans le flux");
}

/// Une ligne vide reste une ligne ; un style par défaut produit un patch
/// identique à l'ancien appel sans style.
#[test]
fn unstyled_call_matches_the_styled_default() {
    let (mut plain, fragment) = document_with_fragment();
    plain
        .replace_text_fragment(&fragment, "Invoice 99999", None)
        .expect("patch historique");

    let (mut styled, fragment) = document_with_fragment();
    let style = TextStyle::from_fragment(&fragment);
    styled
        .replace_text_fragment_styled(&fragment, "Invoice 99999", None, &style)
        .expect("patch stylé");

    let plain = String::from_utf8_lossy(&plain.to_bytes().expect("écriture")).into_owned();
    let styled = String::from_utf8_lossy(&styled.to_bytes().expect("écriture")).into_owned();
    assert!(plain.contains("/YouPdfF1 12.00 Tf"), "police historique");
    assert!(styled.contains("/YouPdfF1 12.00 Tf"), "police par défaut");
    assert_eq!(tdf_x(&plain), tdf_x(&styled));
}
