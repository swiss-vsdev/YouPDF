//! Export d'une liste de commentaires (Markdown, CSV, XFDF, FDF).

use lopdf::{dictionary, Dictionary, Object};

use youpdf_shared::{Error, Result};

use super::render::pdf_date;
use super::{Annotation, AnnotationKind, AnnotationState, CommentFormat};

/// Exporte une liste de commentaires vers `format`.
///
/// L'ordre reçu est conservé : l'appelant l'a déjà trié. Les réponses
/// (`reply_to`) sont indentées en Markdown et portent leur lien `/IRT`
/// ailleurs.
pub fn export_comments(annotations: &[Annotation], format: CommentFormat) -> Result<Vec<u8>> {
    match format {
        CommentFormat::Markdown => Ok(export_comments_markdown(annotations).into_bytes()),
        CommentFormat::Csv => Ok(export_comments_csv(annotations).into_bytes()),
        CommentFormat::Xfdf => Ok(export_comments_xfdf(annotations).into_bytes()),
        CommentFormat::Fdf => export_comments_fdf(annotations),
    }
}

/// Nom du sous-type PDF, stable et lisible par les autres outils.
fn comment_type_name(annotation: &Annotation) -> String {
    String::from_utf8_lossy(annotation.kind.pdf_subtype()).to_string()
}

/// Jour seul de l'horodatage (`AAAA-MM-JJ`).
fn comment_day(annotation: &Annotation) -> &str {
    annotation.created_at.get(..10).unwrap_or("")
}

/// Date PDF (`D:AAAAMMJJHHmmSSZ`) depuis l'horodatage interne.
///
/// Les formats d'échange de commentaires attendent une date PDF, pas une
/// chaîne RFC 3339.
fn comment_pdf_date(annotation: &Annotation) -> String {
    chrono::DateTime::parse_from_rfc3339(&annotation.created_at)
        .map(|date| pdf_date(date.with_timezone(&chrono::Utc)))
        .unwrap_or_else(|_| annotation.created_at.clone())
}

fn export_comments_markdown(annotations: &[Annotation]) -> String {
    let mut out = String::from("# Comments\n");
    for annotation in annotations {
        out.push('\n');
        if annotation.reply_to.is_some() {
            // Une réponse se place sous son commentaire parent.
            out.push_str(&format!(
                "#### ↳ Reply · {} · {}\n",
                annotation.author,
                comment_day(annotation)
            ));
        } else {
            out.push_str(&format!(
                "## p.{} · {} · {} · {}\n",
                annotation.page,
                comment_type_name(annotation),
                annotation.author,
                comment_day(annotation)
            ));
            if annotation.state != AnnotationState::None {
                out.push_str(&format!("\n**Status:** {}\n", annotation.state.display_name()));
            }
        }
        if !annotation.content.is_empty() {
            out.push('\n');
            out.push_str(&annotation.content);
            out.push('\n');
        }
    }
    out
}

fn export_comments_csv(annotations: &[Annotation]) -> String {
    use crate::util::csv_escape;

    let mut out = String::from("page,type,author,date,state,reply_to,content\n");
    for annotation in annotations {
        out.push_str(&format!(
            "{},{},{},{},{},{},{}\n",
            annotation.page,
            csv_escape(&comment_type_name(annotation)),
            csv_escape(&annotation.author),
            csv_escape(comment_day(annotation)),
            csv_escape(annotation.state.display_name()),
            csv_escape(annotation.reply_to.as_deref().unwrap_or("")),
            csv_escape(&annotation.content),
        ));
    }
    out
}

/// Nom d'élément XFDF associé à un type d'annotation.
fn xfdf_element(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::Highlight => "highlight",
        AnnotationKind::Underline => "underline",
        AnnotationKind::StrikeOut => "strikeout",
        AnnotationKind::Squiggly => "squiggly",
        AnnotationKind::Note => "text",
        AnnotationKind::FreeText => "freetext",
        AnnotationKind::Square => "square",
        AnnotationKind::Circle => "circle",
        AnnotationKind::Line | AnnotationKind::Arrow => "line",
        AnnotationKind::Polygon => "polygon",
        AnnotationKind::PolyLine => "polyline",
        AnnotationKind::Callout => "callout",
        AnnotationKind::Ink => "ink",
        AnnotationKind::Stamp => "stamp",
        AnnotationKind::Link => "link",
        AnnotationKind::FileAttachment => "fileattachment",
    }
}

fn export_comments_xfdf(annotations: &[Annotation]) -> String {
    use crate::util::escape_xml;

    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <xfdf xmlns=\"http://ns.adobe.com/xfdf/\" xml:space=\"preserve\">\n<annots>\n",
    );
    for annotation in annotations {
        let element = xfdf_element(annotation.kind);
        let color = format!(
            "#{:02X}{:02X}{:02X}",
            annotation.style.color[0], annotation.style.color[1], annotation.style.color[2]
        );
        out.push_str(&format!(
            "<{element} page=\"{}\" rect=\"{:.2},{:.2},{:.2},{:.2}\" color=\"{color}\" \
             date=\"{}\" title=\"{}\" name=\"{}\" subject=\"{}\"",
            annotation.page.saturating_sub(1),
            annotation.rect[0],
            annotation.rect[1],
            annotation.rect[2],
            annotation.rect[3],
            escape_xml(&comment_pdf_date(annotation)),
            escape_xml(&annotation.author),
            escape_xml(&annotation.id),
            escape_xml(&comment_type_name(annotation)),
        ));
        if annotation.state != AnnotationState::None {
            out.push_str(&format!(" state=\"{}\"", annotation.state.display_name()));
        }
        if let Some(parent) = &annotation.reply_to {
            out.push_str(&format!(" inreplyto=\"{}\"", escape_xml(parent)));
        }
        out.push('>');
        if !annotation.content.is_empty() {
            out.push_str(&format!(
                "<contents>{}</contents>",
                escape_xml(&annotation.content)
            ));
        }
        out.push_str(&format!("</{element}>\n"));
    }
    out.push_str("</annots>\n</xfdf>\n");
    out
}

/// Dictionnaire FDF d'une annotation (sans `/IRT`, posé au second passage).
fn fdf_annot_dict(annotation: &Annotation) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Type", Object::Name(b"Annot".to_vec()));
    d.set("Subtype", Object::Name(annotation.kind.pdf_subtype().to_vec()));
    d.set(
        "Rect",
        Object::Array(vec![
            Object::Real(annotation.rect[0]),
            Object::Real(annotation.rect[1]),
            Object::Real(annotation.rect[2]),
            Object::Real(annotation.rect[3]),
        ]),
    );
    // `/Page` est l'indice de page, base zéro, comme dans tout FDF.
    d.set(
        "Page",
        Object::Integer(annotation.page.saturating_sub(1) as i64),
    );
    d.set("NM", Object::string_literal(annotation.id.as_str()));
    d.set("T", Object::string_literal(annotation.author.as_str()));
    // `/M` est une date PDF, comme dans `to_pdf_dict`.
    d.set("M", Object::string_literal(comment_pdf_date(annotation)));
    if !annotation.content.is_empty() {
        d.set("Contents", Object::string_literal(annotation.content.as_str()));
    }
    if annotation.state != AnnotationState::None {
        d.set("StateModel", Object::Name(b"Review".to_vec()));
        d.set("State", Object::Name(annotation.state.pdf_name().to_vec()));
    }
    d
}

fn export_comments_fdf(annotations: &[Annotation]) -> Result<Vec<u8>> {
    let mut doc = lopdf::Document::with_version("1.2");

    // Les annotations sont des objets indirects : `/IRT` peut alors viser
    // l'objet parent plutôt que de le recopier.
    let mut ids: Vec<(String, Option<String>, lopdf::ObjectId)> = Vec::new();
    for annotation in annotations {
        let id = doc.add_object(Object::Dictionary(fdf_annot_dict(annotation)));
        ids.push((annotation.id.clone(), annotation.reply_to.clone(), id));
    }
    for (name, reply, id) in &ids {
        let Some(parent) = reply else { continue };
        let Some((_, _, parent_id)) = ids.iter().find(|(other, _, _)| other == parent) else {
            continue;
        };
        if name == parent {
            continue;
        }
        if let Ok(dict) = doc.get_dictionary_mut(*id) {
            dict.set("IRT", Object::Reference(*parent_id));
        }
    }

    let mut fdf = Dictionary::new();
    fdf.set(
        "Annots",
        Object::Array(ids.iter().map(|(_, _, id)| Object::Reference(*id)).collect()),
    );
    let root = doc.add_object(Object::Dictionary(
        dictionary! { "FDF" => Object::Dictionary(fdf) },
    ));
    doc.trailer.set("Root", Object::Reference(root));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes)
        .map_err(|e| Error::Pdf(format!("export FDF : {e}")))?;
    // Un FDF est un PDF dont seul l'en-tête change ; les lecteurs refusent
    // un conteneur étiqueté « PDF ».
    if bytes.starts_with(b"%PDF-1.2") {
        bytes[..8].copy_from_slice(b"%FDF-1.2");
    }
    Ok(bytes)
}
