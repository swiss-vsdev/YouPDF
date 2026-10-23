//! Tampons dynamiques et personnalisés.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::annotate::{
    dynamic_stamp_text, stamp_id_for_pdf_name, stamp_template, StampTemplate, STAMP_FONT_SIZE,
    STAMP_TEMPLATES,
};
use youpdf_core::{Annotation, AnnotationKind, Document};

/// Un PDF d'une page vierge, pour y poser des tampons.
fn blank() -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let content = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
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
    doc.save_to(&mut buf).expect("écriture du PDF de test");
    buf
}

/// Les douze modèles sont distincts et chacun porte un nom PDF non vide.
#[test]
fn les_modeles_sont_complets_et_distincts() {
    assert_eq!(STAMP_TEMPLATES.len(), 12);
    let mut ids: Vec<&str> = STAMP_TEMPLATES.iter().map(|t| t.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 12, "identifiants uniques");
    for template in STAMP_TEMPLATES {
        assert!(!template.label.is_empty(), "libellé de {}", template.id);
        assert!(
            !template.pdf_name.is_empty(),
            "nom PDF de {}",
            template.id
        );
    }
}

/// Un identifiant inconnu retombe sur le premier modèle au lieu de paniquer.
#[test]
fn un_identifiant_inconnu_retombe_sur_le_premier() {
    assert_eq!(stamp_template("nope").id, STAMP_TEMPLATES[0].id);
    assert_eq!(stamp_template("").id, STAMP_TEMPLATES[0].id);
}

/// Le texte dynamique empile libellé, auteur et date.
#[test]
fn le_texte_dynamique_empile_les_trois_lignes() {
    let template = stamp_template("approved");
    let text = dynamic_stamp_text(template, "Camille", "2026-10-23");
    assert_eq!(text, "Approved\nCamille\n2026-10-23");
}

/// Un auteur ou une date vide ne laisse pas de ligne creuse.
#[test]
fn les_champs_vides_ne_laissent_pas_de_ligne() {
    let template = stamp_template("approved");
    assert_eq!(dynamic_stamp_text(template, "", ""), "Approved");
    assert_eq!(dynamic_stamp_text(template, "  ", ""), "Approved");
    assert_eq!(
        dynamic_stamp_text(template, "Camille", " "),
        "Approved\nCamille"
    );
}

/// Un modèle non dynamique reste sur son libellé, même avec auteur et date.
#[test]
fn un_modele_fixe_ignore_auteur_et_date() {
    let template = stamp_template("sign_here");
    assert!(!template.dynamic);
    assert_eq!(
        dynamic_stamp_text(template, "Camille", "2026-10-23"),
        "Sign Here"
    );
}

/// Un tampon dynamique se régénère ; un tampon à texte libre non.
#[test]
fn seuls_les_tampons_dynamiques_se_regenerent() {
    let mut dynamic = Annotation::stamp(1, [10.0, 10.0, 200.0, 60.0], "confidential", "Ana", "2026-01-02");
    assert!(dynamic.is_dynamic_stamp());
    assert_eq!(dynamic.content, "Confidential\nAna\n2026-01-02");
    dynamic.refresh_stamp("Bo", "2026-03-04");
    assert_eq!(dynamic.content, "Confidential\nBo\n2026-03-04");

    let mut free = Annotation::stamp(1, [10.0, 10.0, 200.0, 60.0], "sign_here", "Ana", "2026-01-02");
    assert!(!free.is_dynamic_stamp());
    assert_eq!(free.content, "Sign Here");
    free.refresh_stamp("Bo", "2026-03-04");
    assert_eq!(free.content, "Sign Here");
}

/// Un tampon créé garde la couleur du modèle et sa taille de police.
#[test]
fn le_tampon_reprend_la_couleur_du_modele() {
    let annot = Annotation::stamp(1, [0.0, 0.0, 100.0, 40.0], "not_approved", "Ana", "2026-01-02");
    assert_eq!(annot.kind, AnnotationKind::Stamp);
    assert_eq!(annot.style.color, stamp_template("not_approved").color);
    assert_eq!(annot.style.font_size, STAMP_FONT_SIZE);
    assert_eq!(annot.author, "Ana");
    assert_eq!(annot.stamp.as_deref(), Some("not_approved"));
}

/// Le nom PDF et l'identifiant se retrouvent dans les deux sens.
#[test]
fn le_nom_pdf_se_retrouve_dans_l_identifiant() {
    for template in STAMP_TEMPLATES {
        let id = stamp_id_for_pdf_name(template.pdf_name).expect("identifiant retrouvé");
        assert_eq!(
            stamp_template(&id).pdf_name,
            template.pdf_name,
            "aller-retour pour {}",
            template.id
        );
    }
    assert!(stamp_id_for_pdf_name("Nonsense").is_none());
}

/// L'apparence d'un tampon dynamique porte une ligne par champ.
#[test]
fn l_apparence_dessine_une_ligne_par_champ() {
    let annot = Annotation::stamp(1, [50.0, 400.0, 300.0, 460.0], "approved", "Ana", "2026-01-02");
    let content = String::from_utf8(annot.appearance_content().expect("apparence"))
        .expect("utf8");
    assert_eq!(content.matches(" Tj").count(), 3, "trois lignes de texte");
    assert!(content.contains("(Approved)"));
    assert!(content.contains("(Ana)"));
    assert!(content.contains("2026-01-02"));
    assert!(content.contains(" re S"), "le cadre est tracé");
}

/// Le tampon traverse l'écriture et la relecture du document.
#[test]
fn le_tampon_traverse_le_document() {
    let mut doc = Document::from_bytes(&blank()).expect("ouverture");
    let annot = Annotation::stamp(1, [50.0, 400.0, 300.0, 460.0], "confidential", "Ana", "2026-01-02");
    doc.apply_annotations(&[annot]).expect("écriture");

    let back = doc.read_annotations();
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].kind, AnnotationKind::Stamp);
    assert_eq!(back[0].content, "Confidential\nAna\n2026-01-02");
    assert_eq!(back[0].stamp.as_deref(), Some("confidential"));
    assert!(back[0].is_dynamic_stamp());
}

/// Le `/Name` écrit est celui du modèle, pas son libellé affiché.
#[test]
fn le_nom_pdf_suit_le_modele() {
    let mut doc = Document::from_bytes(&blank()).expect("ouverture");
    let annot = Annotation::stamp(1, [50.0, 400.0, 300.0, 460.0], "void", "", "");
    doc.apply_annotations(&[annot]).expect("écriture");
    let back = doc.read_annotations();
    assert_eq!(back[0].stamp.as_deref(), Some("not_approved"));
}

/// Un auteur vide laisse le nom par défaut de l'annotation.
#[test]
fn un_auteur_vide_garde_le_nom_par_defaut() {
    let annot = Annotation::stamp(1, [0.0, 0.0, 100.0, 40.0], "final", "   ", "2026-01-02");
    assert_eq!(annot.author, "YouPDF");
    assert_eq!(annot.content, "Final\n2026-01-02");
}

/// Un modèle de test peut être construit par l'appelant : le type est
/// public et ses champs le sont aussi.
#[test]
fn le_type_de_modele_est_public() {
    let custom = StampTemplate {
        id: "custom",
        label: "Custom",
        pdf_name: "Draft",
        color: [1, 2, 3],
        dynamic: true,
    };
    assert_eq!(
        dynamic_stamp_text(&custom, "Ana", "2026-01-02"),
        "Custom\nAna\n2026-01-02"
    );
}
