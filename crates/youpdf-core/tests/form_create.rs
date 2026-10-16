//! Création de champs de formulaire.
//!
//! Un champ créé doit être lisible par n'importe quel lecteur : il lui faut
//! un widget dans `/Annots`, une entrée dans `/Fields` de `/AcroForm`, et une
//! apparence — sinon l'utilisateur ne voit qu'un rectangle vide, ou rien.

use youpdf_core::forms::NewFormField;
use youpdf_core::{Document, FormFieldKind, PdfiumRenderer, RenderOptions, Renderer};

/// Une page blanche 612×792.
fn page_blanche() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! {},
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Le champ texte créé se relit avec son nom, sa page et sa valeur.
#[test]
fn champ_texte_cree_puis_relu() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let mut field = NewFormField::text(1, [72.0, 600.0, 272.0, 620.0], "Nom");
    field.value = Some("Ada".to_string());
    doc.create_form_field(&field).expect("création");

    let fields = doc.read_form_fields();
    assert_eq!(fields.len(), 1, "un champ attendu");
    assert_eq!(fields[0].name, "Nom");
    assert_eq!(fields[0].kind, FormFieldKind::Text);
    assert_eq!(fields[0].page, Some(1));
    assert_eq!(fields[0].value.as_deref(), Some("Ada"));

    let bytes = doc.to_bytes().expect("sérialisation");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/AcroForm"), "pas de /AcroForm au catalogue");
    assert!(text.contains("/Widget"), "pas de widget");
    assert!(text.contains("NeedAppearances"), "pas de /NeedAppearances");
    assert!(text.contains("(Ada)"), "la valeur n'est pas dans l'apparence");
}

/// Une case à cocher porte ses deux états et son état d'apparence.
#[test]
fn case_a_cocher_a_deux_etats() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let mut field = NewFormField::checkbox(1, [72.0, 500.0, 92.0, 520.0], "Accepte");
    field.value = Some("Yes".to_string());
    doc.create_form_field(&field).expect("création");

    let bytes = doc.to_bytes().expect("sérialisation");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Off"), "état /Off absent");
    assert!(text.contains("/Yes"), "état /Yes absent");
    assert!(text.contains("/AS"), "état d'apparence absent");

    let fields = doc.read_form_fields();
    assert_eq!(fields[0].kind, FormFieldKind::Checkbox);
    assert_eq!(fields[0].value.as_deref(), Some("Yes"));
}

/// Une liste propose ses choix et se coche par défaut sur le premier.
#[test]
fn liste_garde_ses_choix() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let field =
        NewFormField::choice(1, [72.0, 400.0, 220.0, 420.0], "Pays", &["France", "Suisse"]);
    doc.create_form_field(&field).expect("création");

    let fields = doc.read_form_fields();
    assert_eq!(fields[0].kind, FormFieldKind::Choice);
    assert_eq!(fields[0].options, vec!["France", "Suisse"]);

    let bytes = doc.to_bytes().expect("sérialisation");
    assert!(String::from_utf8_lossy(&bytes).contains("(France)"));
}

/// Le remplissage d'un champ créé passe par le même chemin que les autres.
#[test]
fn champ_cree_puis_rempli() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.create_form_field(&NewFormField::text(1, [72.0, 600.0, 272.0, 620.0], "Ville"))
        .expect("création");
    let field = doc.read_form_fields().remove(0);
    doc.fill_form_field(&field, "Lyon").expect("remplissage");

    let fields = doc.read_form_fields();
    assert_eq!(fields[0].value.as_deref(), Some("Lyon"));

    // Le champ doit exister pour PDFium : la page se rend sans erreur et la
    // valeur reste lisible après un aller-retour sur disque.
    let bytes = doc.to_bytes().expect("sérialisation");
    let reopened = Document::from_bytes(&bytes).expect("réouverture");
    assert_eq!(
        reopened.read_form_fields()[0].value.as_deref(),
        Some("Lyon")
    );
    let renderer = PdfiumRenderer::load().expect("pdfium");
    renderer
        .render_page(&reopened, 1, &RenderOptions::default())
        .expect("rendu");
}

/// Un document dont `/Fields` est une référence garde ses champs existants.
///
/// C'est la forme qu'utilisent la plupart des producteurs : remplacer le
/// tableau par un neuf ferait disparaître les champs déjà déclarés.
#[test]
fn champs_existants_conserves_si_fields_indirect() {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let widget = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Annot".to_vec()),
        "Subtype" => Object::Name(b"Widget".to_vec()),
        "FT" => Object::Name(b"Tx".to_vec()),
        "T" => Object::string_literal(b"existant".to_vec()),
        "Rect" => Object::Array(vec![72.into(), 700.into(), 300.into(), 724.into()]),
        "P" => page,
        "F" => 4,
        "DA" => Object::string_literal(b"/Helv 12 Tf 0 g".to_vec()),
    });
    let fields = doc.add_object(Object::Array(vec![Object::Reference(widget)]));
    let acro = doc.add_object(dictionary! {
        "Fields" => Object::Reference(fields),
        "NeedAppearances" => true,
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![page.into()]),
        "Count" => 1,
    });
    let catalog = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => pages,
        "AcroForm" => Object::Reference(acro),
    });
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("écriture du PDF de test");

    let mut doc = Document::from_bytes(&bytes).expect("ouverture");
    assert_eq!(
        doc.read_form_fields().len(),
        1,
        "le champ existant n'est pas lu"
    );

    doc.create_form_field(&NewFormField::text(1, [72.0, 600.0, 272.0, 620.0], "nouveau"))
        .expect("création");

    let saved = doc.to_bytes().expect("sérialisation");
    let reopened = Document::from_bytes(&saved).expect("réouverture");
    let names: Vec<String> = reopened
        .read_form_fields()
        .into_iter()
        .map(|field| field.name)
        .collect();
    assert!(names.contains(&"existant".to_string()), "champ perdu : {names:?}");
    assert!(names.contains(&"nouveau".to_string()), "champ absent : {names:?}");
}

/// Une liste à choix multiple porte le bit 21, et non son voisin.
#[test]
fn multi_selection_drapeau() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let mut field = NewFormField::choice(1, [72.0, 400.0, 220.0, 420.0], "Liste", &["a", "b"]);
    field.kind = FormFieldKind::MultiChoice;
    doc.create_form_field(&field).expect("création");

    // 1 << 17 (liste déroulante) | 1 << 20 (choix multiple).
    let bytes = doc.to_bytes().expect("sérialisation");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Ff 1179648"), "drapeaux inattendus : {text}");
    assert_eq!(doc.read_form_fields()[0].kind, FormFieldKind::MultiChoice);
}
