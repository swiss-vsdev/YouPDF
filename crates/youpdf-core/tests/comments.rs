//! Liste de commentaires : état de revue, réponses et export.

use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};
use youpdf_core::{
    export_comments, Annotation, AnnotationKind, AnnotationState, CommentFormat, Document,
};

/// Construit un PDF d'une page portant une ligne de texte.
fn text_pdf(text: &str) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut content: Vec<u8> = Vec::new();
    content.extend_from_slice(
        format!("BT\n/F1 12 Tf\n1 0 0 1 72 700 Tm\n({text}) Tj\nET\n").as_bytes(),
    );
    let stream = doc.add_object(Stream::new(dictionary! {}, content));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => dictionary! {
            "Font" => dictionary! {
                "F1" => dictionary! {
                    "Type" => Object::Name(b"Font".to_vec()),
                    "Subtype" => Object::Name(b"Type1".to_vec()),
                    "BaseFont" => Object::Name(b"Helvetica".to_vec()),
                }
            }
        },
        "Contents" => stream,
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

fn highlight(page: usize, content: &str) -> Annotation {
    let mut ann = Annotation::new(page, AnnotationKind::Highlight, [70.0, 695.0, 300.0, 712.0]);
    ann.content = content.to_owned();
    ann
}

/// L'état de revue traverse l'écriture et la relecture.
#[test]
fn etat_de_revue_traverse_le_document() {
    let mut doc = Document::from_bytes(&text_pdf("Bonjour")).expect("ouverture");
    let mut ann = highlight(1, "à revoir");
    ann.state = AnnotationState::Accepted;
    let mut rejected = highlight(1, "non");
    rejected.state = AnnotationState::Rejected;

    doc.apply_annotations(&[ann, rejected]).expect("écriture");
    let back = doc.read_annotations();

    assert_eq!(back.len(), 2);
    assert_eq!(back[0].state, AnnotationState::Accepted);
    assert_eq!(back[0].content, "à revoir");
    assert_eq!(back[1].state, AnnotationState::Rejected);
}

/// Une réponse pointe l'objet de son parent et se relit par son `/NM`.
#[test]
fn reponse_pointe_son_parent() {
    let mut doc = Document::from_bytes(&text_pdf("Bonjour")).expect("ouverture");
    let parent = highlight(1, "question ?");
    let mut reply = Annotation::new(1, AnnotationKind::Note, [280.0, 700.0, 300.0, 720.0]);
    reply.content = "réponse".to_owned();
    reply.reply_to = Some(parent.id.clone());

    doc.apply_annotations(&[parent.clone(), reply]).expect("écriture");
    let back = doc.read_annotations();

    let reply = back
        .iter()
        .find(|a| a.content == "réponse")
        .expect("la réponse est relue");
    assert_eq!(reply.reply_to.as_deref(), Some(parent.id.as_str()));
    // Le parent, lui, n'est pas une réponse.
    let parent = back
        .iter()
        .find(|a| a.content == "question ?")
        .expect("le parent est relu");
    assert_eq!(parent.reply_to, None);
}

/// Une réponse dont le parent a disparu ne garde pas de lien mort.
#[test]
fn reponse_orpheline_perd_son_lien() {
    let mut doc = Document::from_bytes(&text_pdf("Bonjour")).expect("ouverture");
    let mut reply = Annotation::new(1, AnnotationKind::Note, [280.0, 700.0, 300.0, 720.0]);
    reply.content = "orpheline".to_owned();
    reply.reply_to = Some("youpdf-parent-inexistant".to_owned());

    doc.apply_annotations(&[reply]).expect("écriture");
    let back = doc.read_annotations();

    assert_eq!(back.len(), 1);
    assert_eq!(back[0].reply_to, None);
}

/// La citation reprend le texte visé par le surlignage.
#[test]
fn citation_reprend_le_texte_du_rectangle() {
    let doc = Document::from_bytes(&text_pdf("PHRASE CIBLE")).expect("ouverture");
    let inside = doc.text_in_rect(1, [60.0, 690.0, 250.0, 715.0]);
    assert_eq!(inside, "PHRASE CIBLE");

    let outside = doc.text_in_rect(1, [400.0, 400.0, 500.0, 430.0]);
    assert!(outside.is_empty(), "hors du texte : {outside:?}");
}

/// L'export Markdown porte l'état et distingue les réponses.
#[test]
fn export_markdown_marque_les_reponses() {
    let parent = highlight(3, "phrase citée");
    let mut reply = Annotation::new(3, AnnotationKind::Note, [0.0, 0.0, 10.0, 10.0]);
    reply.content = "d'accord".to_owned();
    reply.reply_to = Some(parent.id.clone());

    let out = export_comments(&[parent, reply], CommentFormat::Markdown).expect("export");
    let text = String::from_utf8(out).expect("UTF-8");

    assert!(text.contains("# Comments"));
    assert!(text.contains("## p.3 · Highlight · YouPDF"));
    assert!(text.contains("#### ↳ Reply"));
    assert!(text.contains("phrase citée"));
    assert!(text.contains("d'accord"));
}

/// L'export CSV échappe les virgules et les guillemets.
#[test]
fn export_csv_echappe_le_texte() {
    let mut ann = highlight(1, "a, \"b\"");
    ann.author = "Alice, Bob".to_owned();
    ann.state = AnnotationState::Completed;

    let out = export_comments(&[ann], CommentFormat::Csv).expect("export");
    let text = String::from_utf8(out).expect("UTF-8");
    let line = text.lines().nth(1).expect("une ligne de données");

    assert!(line.starts_with("1,Highlight,"));
    assert!(line.contains("\"Alice, Bob\""));
    assert!(line.contains("Completed"));
    assert!(line.contains("\"a, \"\"b\"\"\""));
}

/// L'export XFDF porte le lien de réponse et le contenu.
#[test]
fn export_xfdf_porte_le_lien_de_reponse() {
    let parent = highlight(2, "parent");
    let mut reply = Annotation::new(2, AnnotationKind::Note, [0.0, 0.0, 10.0, 10.0]);
    reply.content = "enfant".to_owned();
    reply.reply_to = Some(parent.id.clone());

    let out = export_comments(&[parent.clone(), reply], CommentFormat::Xfdf).expect("export");
    let text = String::from_utf8(out).expect("UTF-8");

    assert!(text.contains("<xfdf xmlns=\"http://ns.adobe.com/xfdf/\""));
    assert!(text.contains("inreplyto=\""));
    assert!(text.contains(&format!("name=\"{}\"", parent.id)));
    assert!(text.contains("<contents>parent</contents>"));
    // Base zéro pour la page, comme dans tout XFDF.
    assert!(text.contains("page=\"1\""));
}

/// L'export FDF est un conteneur FDF valide dont les réponses se lient.
#[test]
fn export_fdf_est_un_conteneur_valide() {
    let parent = highlight(1, "parent");
    let mut reply = Annotation::new(1, AnnotationKind::Note, [0.0, 0.0, 10.0, 10.0]);
    reply.content = "enfant".to_owned();
    reply.reply_to = Some(parent.id.clone());

    let out = export_comments(&[parent, reply], CommentFormat::Fdf).expect("export");
    assert!(out.starts_with(b"%FDF-1.2"), "en-tête FDF attendu");

    // Un FDF est un PDF dont seul l'en-tête diffère : lopdf (qui n'accepte
    // que `%PDF`) relit le même octet stream une fois l'en-tête remis.
    let mut as_pdf = out.clone();
    as_pdf[..8].copy_from_slice(b"%PDF-1.2");
    let fdf = LopdfDoc::load_mem(&as_pdf).expect("le conteneur doit se relire");
    let root = fdf.trailer.get(b"Root").expect("Root").as_reference().expect("référence");
    let catalog = fdf.get_dictionary(root).expect("catalogue");
    let fdf_dict = catalog.get(b"FDF").expect("FDF").as_dict().expect("dictionnaire");
    let annots = fdf_dict.get(b"Annots").expect("Annots").as_array().expect("tableau");
    assert_eq!(annots.len(), 2);

    let mut with_irt = 0;
    for annot in annots {
        let id = annot.as_reference().expect("annotation indirecte");
        let dict = fdf.get_dictionary(id).expect("dictionnaire d'annotation");
        if dict.get(b"IRT").is_ok() {
            with_irt += 1;
        }
    }
    assert_eq!(with_irt, 1, "la réponse porte un /IRT");
}

/// PDF d'une page portant plusieurs lignes positionnées `(texte, x, y)`.
fn text_pdf_at(lines: &[(&str, f32, f32)]) -> Vec<u8> {
    let mut doc = LopdfDoc::with_version("1.7");
    let mut content: Vec<u8> = Vec::new();
    for (text, x, y) in lines {
        content.extend_from_slice(
            format!("BT\n/F1 12 Tf\n1 0 0 1 {x} {y} Tm\n({text}) Tj\nET\n").as_bytes(),
        );
    }
    let stream = doc.add_object(Stream::new(dictionary! {}, content));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => dictionary! {
            "Font" => dictionary! {
                "F1" => dictionary! {
                    "Type" => Object::Name(b"Font".to_vec()),
                    "Subtype" => Object::Name(b"Type1".to_vec()),
                    "BaseFont" => Object::Name(b"Helvetica".to_vec()),
                }
            }
        },
        "Contents" => stream,
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

/// Une citation sur deux lignes suit l'ordre de lecture et garde la coupure.
#[test]
fn citation_multi_lignes_suit_l_ordre_de_lecture() {
    // La seconde ligne vient en premier dans le flux de contenu.
    let pdf = text_pdf_at(&[("SECONDE", 72.0, 660.0), ("PREMIERE", 72.0, 700.0)]);
    let doc = Document::from_bytes(&pdf).expect("ouverture");

    let quote = doc.text_in_rect(1, [60.0, 650.0, 400.0, 715.0]);
    assert_eq!(quote, "PREMIERE\nSECONDE");
}

/// La lecture groupée n'analyse la page qu'une fois et rend un texte par
/// rectangle.
#[test]
fn citation_groupee_rend_un_texte_par_rectangle() {
    let pdf = text_pdf_at(&[("GAUCHE", 72.0, 700.0), ("DROITE", 300.0, 700.0)]);
    let doc = Document::from_bytes(&pdf).expect("ouverture");

    let quotes = doc.text_in_rects(
        1,
        &[[60.0, 690.0, 200.0, 715.0], [290.0, 690.0, 400.0, 715.0]],
    );
    assert_eq!(quotes, vec!["GAUCHE".to_owned(), "DROITE".to_owned()]);
}

/// Les exports datent les commentaires au format PDF, pas en RFC 3339.
#[test]
fn exports_datement_au_format_pdf() {
    let mut ann = highlight(1, "daté");
    ann.created_at = "2026-03-04T05:06:07Z".to_owned();

    let xfdf = export_comments(&[ann.clone()], CommentFormat::Xfdf).expect("export");
    let xfdf = String::from_utf8(xfdf).expect("UTF-8");
    assert!(
        xfdf.contains("date=\"D:20260304050607Z\""),
        "date FDF attendue dans le XFDF : {xfdf}"
    );
    assert!(!xfdf.contains("2026-03-04T05:06:07Z"));

    let fdf = export_comments(&[ann], CommentFormat::Fdf).expect("export");
    let text = String::from_utf8_lossy(&fdf);
    assert!(text.contains("(D:20260304050607Z)"), "date FDF attendue");
}
