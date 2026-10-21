//! Tests du moteur de caviardage à deux phases.
//!
//! Le moteur réécrit le flux de contenu au lieu de rasteriser la page :
//! le texte situé hors des zones caviardées doit rester sélectionnable.
//! C'est la propriété centrale, et c'est celle qu'un simple « le fichier
//! s'ouvre » ne vérifie pas.

use lopdf::{dictionary, Dictionary, Object, Stream};
use youpdf_core::{
    Document, PatchFont, RedactionFill, RedactionMark, RedactionOverlay, RedactionReport,
};

/// Un PDF de deux lignes : l'une à caviarder, l'autre à préserver.
fn deux_textes() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let cid = doc.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 24 Tf 100 700 Td (CONFIDENTIEL) Tj 0 -100 Td (PUBLIQUE) Tj ET".to_vec(),
    ));
    let pid = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => cid,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![pid.into()]),
        "Count" => 1,
    });
    let cat = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(cat));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("écriture");
    buf
}

/// Le texte extrait de la page.
fn texte_page(doc: &Document, page: u32) -> String {
    doc.page_text(page).unwrap_or_default()
}

/// Le flux de contenu brut de la page, relu depuis le fichier sérialisé.
fn contenu(doc: &Document) -> String {
    let bytes = doc.to_bytes().expect("sérialisation");
    let raw = lopdf::Document::load_mem(&bytes).expect("relecture");
    let page = raw.get_pages().values().copied().next().expect("page");
    String::from_utf8_lossy(&raw.get_page_content(page).expect("contenu")).into_owned()
}

/// Un rectangle qui couvre la ligne « CONFIDENTIEL » (y ≈ 700 en repère page).
fn zone_confidentiel() -> [f32; 4] {
    [90.0, 690.0, 400.0, 740.0]
}

fn marque(rect: [f32; 4]) -> RedactionMark {
    RedactionMark {
        page: 1,
        rect,
        fill: RedactionFill::Black,
        overlay: None,
    }
}

/// Le texte hors de la zone survit au caviardage.
#[test]
fn le_texte_hors_zone_survit() {
    let mut doc = Document::from_bytes(&deux_textes()).expect("ouverture");
    let report = doc
        .redact_marks(&[marque(zone_confidentiel())], &youpdf_core::render::NullRenderer)
        .expect("caviardage");

    assert_eq!(report.pages_sliced, 1, "la page doit être découpée");
    assert_eq!(report.pages_rasterized, 0, "aucune rasterisation nécessaire");
    assert!(report.text_ops_removed >= 1, "au moins une instruction retirée");

    let texte = texte_page(&doc, 1);
    assert!(
        !texte.contains("CONFIDENTIEL"),
        "le texte caviardé doit disparaître, obtenu : {texte:?}"
    );
    assert!(
        texte.contains("PUBLIQUE"),
        "le texte hors zone doit survivre, obtenu : {texte:?}"
    );
}

/// Le texte caviardé ne doit plus être dans le flux de contenu.
#[test]
fn le_texte_caviarde_quitte_le_flux() {
    let mut doc = Document::from_bytes(&deux_textes()).expect("ouverture");
    doc.redact_marks(&[marque(zone_confidentiel())], &youpdf_core::render::NullRenderer)
        .expect("caviardage");

    let flux = contenu(&doc);
    assert!(
        !flux.contains("CONFIDENTIEL"),
        "la chaîne caviardée doit quitter le flux, obtenu : {flux}"
    );
    assert!(
        flux.contains("PUBLIQUE"),
        "l'autre chaîne doit rester, obtenu : {flux}"
    );
}

/// Une zone qui ne touche rien ne détruit pas la page.
#[test]
fn une_zone_loin_de_tout_ne_ravage_pas() {
    let mut doc = Document::from_bytes(&deux_textes()).expect("ouverture");
    doc.redact_marks(
        &[marque([10.0, 10.0, 80.0, 50.0])],
        &youpdf_core::render::NullRenderer,
    )
    .expect("caviardage");

    let texte = texte_page(&doc, 1);
    assert!(
        texte.contains("CONFIDENTIEL") && texte.contains("PUBLIQUE"),
        "rien ne devait être détruit, obtenu : {texte:?}"
    );
}

/// Le texte superposé est écrit dans le flux à la place du contenu effacé.
#[test]
fn le_texte_superpose_est_ecrit() {
    let mut doc = Document::from_bytes(&deux_textes()).expect("ouverture");
    let mark = RedactionMark {
        page: 1,
        rect: zone_confidentiel(),
        fill: RedactionFill::Black,
        overlay: Some(RedactionOverlay {
            text: "[REDACTED]".to_string(),
            font: PatchFont::Helvetica,
            size: 12.0,
            color: [255, 255, 255],
        }),
    };
    doc.redact_marks(&[mark], &youpdf_core::render::NullRenderer)
        .expect("caviardage");

    let flux = contenu(&doc);
    assert!(
        flux.contains("[REDACTED]"),
        "le texte superposé doit apparaître, obtenu : {flux}"
    );
}

/// Sans marque, rien ne se passe et le document est intact.
#[test]
fn aucune_marque_ne_touche_rien() {
    let mut doc = Document::from_bytes(&deux_textes()).expect("ouverture");
    let report: RedactionReport = doc
        .redact_marks(&[], &youpdf_core::render::NullRenderer)
        .expect("caviardage");

    assert_eq!(report.pages_sliced, 0);
    assert_eq!(report.pages_rasterized, 0);
    assert_eq!(report.annotations_removed, 0);
    assert_eq!(report.text_ops_removed, 0);

    let texte = texte_page(&doc, 1);
    assert!(texte.contains("CONFIDENTIEL"), "le document doit être intact");
}

/// Le caviardage est destructif : le texte ne doit pas survivre dans le
/// flux, même si l'on cherche la chaîne dans le fichier entier.
#[test]
fn la_chaine_caviardee_absent_du_fichier() {
    let mut doc = Document::from_bytes(&deux_textes()).expect("ouverture");
    doc.redact_marks(&[marque(zone_confidentiel())], &youpdf_core::render::NullRenderer)
        .expect("caviardage");

    let bytes = doc.to_bytes().expect("sérialisation");
    let haystack = String::from_utf8_lossy(&bytes);
    assert!(
        !haystack.contains("CONFIDENTIEL"),
        "la chaîne caviardée ne doit plus être dans le fichier"
    );
}
