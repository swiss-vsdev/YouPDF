//! Édition des propriétés de champ et échange des données de formulaire.
//!
//! Ces tests construisent un document de formulaire comme
//! `tests/form_create.rs`, puis vérifient l'écriture des propriétés et les
//! quatre formats d'échange (FDF, XFDF, CSV, JSON).

use youpdf_core::forms::{FieldProperties, FormDataFormat, NewFormField};
use youpdf_core::{Document, FormFieldKind};

/// Les formats texte sortent en octets (le FDF est un conteneur d'octets) ;
/// les tests les relisent en texte.
fn as_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

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

/// Document avec des champs texte vides, dans l'ordre des noms.
fn doc_avec_champs(noms: &[&str]) -> Document {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    for (index, nom) in noms.iter().enumerate() {
        let y = 600.0 - index as f32 * 30.0;
        doc.create_form_field(&NewFormField::text(1, [72.0, y, 272.0, y + 20.0], *nom))
            .expect("création");
    }
    doc
}

/// Document construit à la main, avec des valeurs brutes (retours à la ligne,
/// caractères de contrôle) que `create_form_field` ne sait pas écrire.
fn pdf_avec_valeurs(champs: &[(&str, &[u8])]) -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream, StringFormat};

    let mut doc = LopdfDoc::with_version("1.7");
    let content = doc.add_object(Stream::new(dictionary! {}, b" ".to_vec()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! {},
    });
    let mut refs = Vec::new();
    for (name, value) in champs {
        let widget = doc.add_object(dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::string_literal(name.as_bytes().to_vec()),
            "V" => Object::String(value.to_vec(), StringFormat::Literal),
            "Rect" => Object::Array(vec![72.into(), 700.into(), 300.into(), 724.into()]),
            "P" => page,
            "F" => 4,
            "DA" => Object::string_literal(b"/Helv 12 Tf 0 g".to_vec()),
        });
        refs.push(Object::Reference(widget));
    }
    let fields = doc.add_object(Object::Array(refs));
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
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("écriture du PDF de test");
    out
}

/// Renommer un champ change `/T` et pose `/TU`.
#[test]
fn renommer_change_t_et_tu() {
    let mut doc = doc_avec_champs(&["Nom"]);
    let field = doc.read_form_fields().remove(0);

    let props = FieldProperties {
        name: "NomComplet".to_string(),
        tooltip: "Votre nom".to_string(),
        ..FieldProperties::default()
    };
    doc.update_form_field(&field, &props).expect("mise à jour");

    let fields = doc.read_form_fields();
    assert_eq!(fields[0].name, "NomComplet");

    let bytes = doc.to_bytes().expect("sérialisation");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("(NomComplet)"), "nouveau /T absent : {text}");
    assert!(text.contains("/TU(Votre nom)"), "nouveau /TU absent : {text}");
}

/// Un champ introuvable est une erreur de saisie.
#[test]
fn renommer_champ_absent_echoue() {
    let mut doc = doc_avec_champs(&["Nom"]);
    let field = doc.read_form_fields().remove(0);
    let absent = youpdf_core::FormField {
        name: "Fantome".to_string(),
        ..field
    };
    let props = FieldProperties {
        name: "Autre".to_string(),
        ..FieldProperties::default()
    };
    assert!(doc.update_form_field(&absent, &props).is_err());
}

/// Un champ de premier niveau garde le nom saisi, point compris : le tronquer
/// au dernier point renommerait « groupe.nom » en « nom ».
#[test]
fn renommer_premier_niveau_garde_le_point() {
    let mut doc = doc_avec_champs(&["Nom"]);
    let field = doc.read_form_fields().remove(0);
    let props = FieldProperties {
        name: "groupe.nom".to_string(),
        ..FieldProperties::default()
    };
    doc.update_form_field(&field, &props).expect("mise à jour");
    assert_eq!(doc.read_form_fields()[0].name, "groupe.nom");
}

/// Poser `Required` ne touche pas au bit de type déjà présent.
#[test]
fn drapeau_obligatoire_sans_ecraser() {
    let mut doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    doc.create_form_field(&NewFormField::choice(
        1,
        [72.0, 400.0, 220.0, 420.0],
        "Pays",
        &["France", "Suisse"],
    ))
    .expect("création");

    let field = doc.read_form_fields().remove(0);
    let props = FieldProperties {
        name: "Pays".to_string(),
        required: true,
        ..FieldProperties::default()
    };
    doc.update_form_field(&field, &props).expect("mise à jour");

    // 1 << 17 (liste) | 2 (obligatoire).
    let bytes = doc.to_bytes().expect("sérialisation");
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Ff 131074"), "drapeaux inattendus : {text}");
    assert_eq!(doc.read_form_fields()[0].kind, FormFieldKind::Choice);
}

/// Multiligne et lecture seule se posent puis s'effacent séparément.
#[test]
fn drapeaux_multiligne_et_lecture_seule() {
    let mut doc = doc_avec_champs(&["Texte"]);
    let field = doc.read_form_fields().remove(0);

    let pose = FieldProperties {
        name: "Texte".to_string(),
        required: true,
        read_only: true,
        multiline: true,
        ..FieldProperties::default()
    };
    doc.update_form_field(&field, &pose).expect("mise à jour");

    // 1 (lecture seule) | 2 (obligatoire) | 4096 (multiligne).
    let text = String::from_utf8_lossy(&doc.to_bytes().expect("sérialisation")).to_string();
    assert!(text.contains("/Ff 4099"), "drapeaux inattendus : {text}");

    let efface = FieldProperties {
        name: "Texte".to_string(),
        required: true,
        read_only: true,
        multiline: false,
        ..FieldProperties::default()
    };
    doc.update_form_field(&doc.read_form_fields().remove(0), &efface)
        .expect("mise à jour");
    let text = String::from_utf8_lossy(&doc.to_bytes().expect("sérialisation")).to_string();
    assert!(text.contains("/Ff 3"), "drapeaux inattendus : {text}");
}

/// CSV : en-tête puis une ligne par champ, citation RFC 4180.
#[test]
fn export_csv_cite() {
    let doc = Document::from_bytes(&pdf_avec_valeurs(&[
        ("champ1", b"a,b"),
        ("champ2", b"say \"hi\""),
        ("champ3", b"l1\nl2"),
    ]))
    .expect("ouverture");

    let csv = as_text(&doc.export_form_data(FormDataFormat::Csv).expect("export"));
    assert!(csv.starts_with("name,value\n"), "en-tête absent : {csv}");
    assert!(csv.contains("champ1,\"a,b\"\n"), "virgule non citée : {csv}");
    assert!(
        csv.contains("champ2,\"say \"\"hi\"\"\"\n"),
        "guillemets mal échappés : {csv}"
    );
    assert!(
        csv.contains("champ3,\"l1\nl2\"\n"),
        "retour à la ligne non cité : {csv}"
    );
}

/// JSON : échappement des guillemets, antislashs et caractères de contrôle.
#[test]
fn export_json_echappe() {
    let doc = Document::from_bytes(&pdf_avec_valeurs(&[
        ("a", b"quote\" back\\ slash"),
        ("b", b"tab\there"),
        ("c", b"ctrl\x01x"),
    ]))
    .expect("ouverture");

    let json = as_text(&doc.export_form_data(FormDataFormat::Json).expect("export"));
    assert!(json.starts_with("{\"fields\":["), "forme inattendue : {json}");
    assert!(json.contains("quote\\\" back\\\\ slash"), "échappement : {json}");
    assert!(json.contains("tab\\there"), "tabulation : {json}");
    assert!(json.contains("ctrl\\u0001x"), "contrôle : {json}");
}

/// XFDF : échappement XML des cinq entités.
#[test]
fn export_xfdf_echappe() {
    let doc = Document::from_bytes(&pdf_avec_valeurs(&[("a", b"a&b<c>d\"e'f")]))
        .expect("ouverture");

    let xfdf = as_text(&doc.export_form_data(FormDataFormat::Xfdf).expect("export"));
    assert!(
        xfdf.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"),
        "prologue absent : {xfdf}"
    );
    assert!(
        xfdf.contains("<value>a&amp;b&lt;c&gt;d&quot;e&apos;f</value>"),
        "entités XML : {xfdf}"
    );
}

/// FDF : conteneur réel et valeurs littérales.
#[test]
fn export_fdf_conteneur() {
    let mut doc = doc_avec_champs(&["Nom"]);
    let field = doc.read_form_fields().remove(0);
    doc.fill_form_field(&field, "Ada").expect("remplissage");

    let fdf = as_text(&doc.export_form_data(FormDataFormat::Fdf).expect("export"));
    assert!(fdf.starts_with("%FDF-1.2"), "en-tête FDF : {fdf}");
    assert!(fdf.contains("/FDF << /Fields ["), "corps FDF : {fdf}");
    assert!(fdf.contains("<< /T (Nom) /V (Ada) >>"), "champ FDF : {fdf}");
    assert!(fdf.contains("trailer\n<< /Root 1 0 R >>"), "trailer : {fdf}");
    assert!(fdf.trim_end().ends_with("%%EOF"), "EOF FDF : {fdf}");
}

/// Aller-retour export puis import pour les quatre formats.
#[test]
fn aller_retour_quatre_formats() {
    for format in [
        FormDataFormat::Fdf,
        FormDataFormat::Xfdf,
        FormDataFormat::Csv,
        FormDataFormat::Json,
    ] {
        let mut source = doc_avec_champs(&["Nom", "Ville"]);
        let fields = source.read_form_fields();
        source.fill_form_field(&fields[0], "Ada").expect("remplissage");
        source.fill_form_field(&fields[1], "Lyon").expect("remplissage");

        let data = source.export_form_data(format).expect("export");

        let mut cible = doc_avec_champs(&["Nom", "Ville"]);
        let applied = cible.import_form_data(format, &data).expect("import");
        assert_eq!(applied, 2, "{format:?} : mauvais compte");

        let values: Vec<Option<String>> = cible
            .read_form_fields()
            .into_iter()
            .map(|f| f.value)
            .collect();
        assert_eq!(
            values,
            vec![Some("Ada".to_string()), Some("Lyon".to_string())],
            "{format:?} : valeurs non conservées"
        );
    }
}

/// Le FDF est un conteneur d'octets : un accent doit survivre à l'aller-retour.
///
/// Le passer par une chaîne UTF-8 remplaçait chaque octet ≥ 0x80 par U+FFFD.
#[test]
fn fdf_conserve_les_accents() {
    let mut source = doc_avec_champs(&["Nom"]);
    let field = source.read_form_fields().remove(0);
    source.fill_form_field(&field, "Café").expect("remplissage");

    let fdf = source.export_form_data(FormDataFormat::Fdf).expect("export");
    assert!(
        fdf.contains(&0xE9),
        "l'octet WinAnsi du « é » a disparu : {fdf:?}"
    );

    let mut cible = doc_avec_champs(&["Nom"]);
    let applied = cible.import_form_data(FormDataFormat::Fdf, &fdf).expect("import");
    assert_eq!(applied, 1);
    assert_eq!(
        cible.read_form_fields()[0].value.as_deref(),
        Some("Café"),
        "accent perdu"
    );
}

/// Un nom inconnu est ignoré, les autres valeurs s'appliquent.
#[test]
fn import_ignore_nom_inconnu() {
    let mut doc = doc_avec_champs(&["Nom"]);
    let applied = doc
        .import_form_data(FormDataFormat::Csv, b"name,value\nInconnu,x\nNom,Ada\n")
        .expect("import");
    assert_eq!(applied, 1, "seul le champ connu doit s'appliquer");
    assert_eq!(doc.read_form_fields()[0].value.as_deref(), Some("Ada"));
}

/// Les noms affichés et les extensions suivent le format.
#[test]
fn noms_et_extensions() {
    assert_eq!(FormDataFormat::Fdf.display_name(), "FDF");
    assert_eq!(FormDataFormat::Xfdf.display_name(), "XFDF");
    assert_eq!(FormDataFormat::Csv.display_name(), "CSV");
    assert_eq!(FormDataFormat::Json.display_name(), "JSON");
    assert_eq!(FormDataFormat::Fdf.extension(), "fdf");
    assert_eq!(FormDataFormat::Xfdf.extension(), "xfdf");
    assert_eq!(FormDataFormat::Csv.extension(), "csv");
    assert_eq!(FormDataFormat::Json.extension(), "json");
}
