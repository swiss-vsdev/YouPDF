//! Chirurgie du flux de contenu : suppression ciblée des instructions.
//!
//! Ce module réécrit le flux de contenu d'une page en retirant les
//! instructions qui tombent dans une zone caviardée. Le reste de la page
//! reste sélectionnable et cherchable.

use lopdf::content::{Content, Operation};
use lopdf::Object;

use crate::document::Document;
use crate::edit::encode_pdf_literal;
use crate::redact::geometry::{BBox, Interpreter, OpKind, KNOWN_OPERATORS};
use crate::redact::{RedactionFill, RedactionOverlay};

use youpdf_shared::{Error, Result};

/// Résultat de l'analyse d'un flux de contenu.
#[derive(Debug, Clone, Default)]
pub struct SliceOutcome {
    /// Instructions conservées, réécrites.
    pub kept: Vec<Operation>,
    /// Instructions de texte supprimées.
    pub text_ops_removed: usize,
    /// Instructions vectorielles supprimées.
    pub vector_ops_removed: usize,
    /// Vrai si le flux ne peut pas être découpé proprement et exige une
    /// rasterisation de la page.
    pub needs_raster: bool,
    /// Images dont la zone recoupe une zone à effacer.
    pub images_touched: Vec<TouchedImage>,
    /// Opérateurs rencontrés que l'interpréteur ne sait pas traiter.
    pub unsupported: Vec<String>,
}

/// Une image dont la boîte de placement recoupe une zone à effacer.
#[derive(Debug, Clone)]
pub struct TouchedImage {
    /// Nom de ressource, sans le `/` initial.
    pub resource_name: String,
    /// Boîte de placement dans le repère page.
    pub bbox: BBox,
}

/// Analyse les flux de contenu d'une page et retire ce qui tombe dans les
/// zones.
///
/// `regions` sont les zones à effacer, en repère **page**.
pub(crate) fn slice_page(
    doc: &Document,
    page_id: lopdf::ObjectId,
    regions: &[BBox],
) -> Result<SliceOutcome> {
    let mut outcome = SliceOutcome::default();
    let stream_ids = doc.inner().get_page_contents(page_id);

    'outer: for id in stream_ids {
        let obj = match doc.inner().get_object(id) {
            Ok(o) => o,
            Err(e) => return Err(Error::Pdf(format!("page : {e}"))),
        };
        let stream = match obj {
            Object::Stream(s) => s.clone(),
            _ => continue,
        };

        let bytes = match stream.get_plain_content() {
            Ok(b) => b,
            Err(_) => {
                outcome.needs_raster = true;
                break 'outer;
            }
        };

        let content = match Content::decode(&bytes) {
            Ok(c) => c,
            Err(_) => {
                outcome.needs_raster = true;
                break 'outer;
            }
        };

        let mut interp = Interpreter::new();
        for operation in content.operations.iter() {
            let geometry = interp.interpret(operation);

            if !is_known_operator(&operation.operator) {
                outcome.needs_raster = true;
                outcome.unsupported.push(operation.operator.clone());
                break 'outer;
            }

            match geometry {
                None => {
                    outcome.kept.push(operation.clone());
                }
                Some(g) => match g.kind {
                    OpKind::Text => {
                        let mut drop = false;
                        for region in regions {
                            if g.bbox.overlaps(region) {
                                drop = true;
                                break;
                            }
                        }
                        if drop {
                            outcome.text_ops_removed += 1;
                        } else {
                            outcome.kept.push(operation.clone());
                        }
                    }
                    OpKind::Path => {
                        let filled = g.filled;
                        let stroked = g.stroked;
                        if !filled && !stroked {
                            outcome.kept.push(operation.clone());
                            continue;
                        }
                        let partial = regions
                            .iter()
                            .any(|r| g.bbox.overlaps(r) && !g.bbox.inside(r));
                        let inside = regions.iter().any(|r| g.bbox.inside(r));
                        if partial {
                            outcome.needs_raster = true;
                            break 'outer;
                        } else if inside {
                            outcome.vector_ops_removed += 1;
                        } else {
                            outcome.kept.push(operation.clone());
                        }
                    }
                    OpKind::Image => {
                        outcome.kept.push(operation.clone());

                        let resource_name = operation
                            .operands
                            .first()
                            .and_then(|o| o.as_name().ok())
                            .map(|n| String::from_utf8_lossy(n).to_string())
                            .unwrap_or_default();

                        let subtype = xobject_subtype(doc, page_id, resource_name.as_bytes());
                        match subtype.as_deref() {
                            Some(b"Form") => {
                                outcome.needs_raster = true;
                                break 'outer;
                            }
                            Some(b"Image") => {
                                for region in regions {
                                    if g.bbox.overlaps(region) {
                                        outcome.images_touched.push(TouchedImage {
                                            resource_name: resource_name.clone(),
                                            bbox: g.bbox,
                                        });
                                        break;
                                    }
                                }
                            }
                            _ => {
                                outcome.needs_raster = true;
                                break 'outer;
                            }
                        }
                    }
                    OpKind::Other => {
                        outcome.kept.push(operation.clone());
                    }
                },
            }
        }

        if outcome.needs_raster {
            break 'outer;
        }
    }

    Ok(outcome)
}

/// Réécrit le flux de contenu d'une page.
pub(crate) fn write_back(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    kept: &[Operation],
) -> Result<()> {
    let encoded = Content {
        operations: kept.to_vec(),
    }
    .encode()
    .map_err(|e| Error::Pdf(format!("encodage du flux : {e}")))?;

    let ids = doc.inner().get_page_contents(page_id);

    let mut first = true;
    for id in ids {
        let obj = doc.inner().get_object(id).cloned();
        if let Ok(Object::Stream(mut stream)) = obj {
            if first {
                stream.set_plain_content(encoded.clone());
                first = false;
            } else {
                stream.set_plain_content(Vec::new());
            }
            doc.inner_mut().set_object(id, Object::Stream(stream));
        }
    }

    Ok(())
}

/// Remplace le tableau `/Annots` d'une page.
pub(crate) fn set_annots(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    kept: &[lopdf::ObjectId],
) -> Result<()> {
    let page_obj = doc
        .inner_mut()
        .get_object_mut(page_id)
        .map_err(|e| Error::Pdf(format!("page : {e}")))?;
    let dict = page_obj
        .as_dict_mut()
        .map_err(|e| Error::Pdf(format!("page : {e}")))?;

    if kept.is_empty() {
        dict.remove(b"Annots");
    } else {
        dict.set(
            "Annots",
            Object::Array(kept.iter().map(|id| Object::Reference(*id)).collect()),
        );
    }

    Ok(())
}

/// Opérateurs que l'interpréteur sait traiter.
pub(crate) fn is_known_operator(operator: &str) -> bool {
    KNOWN_OPERATORS.contains(&operator)
}

/// Les instructions qui peintent une zone caviardée et son texte superposé.
///
/// Le flux est produit **en repère page** : les coordonnées ont déjà été
/// converties par l'appelant.
pub(crate) fn overlay_operations(
    doc: &mut Document,
    page_id: lopdf::ObjectId,
    region: &BBox,
    fill: RedactionFill,
    overlay: Option<&RedactionOverlay>,
) -> Result<Vec<Operation>> {
    let mut operations = Vec::new();

    // Le remplissage : un rectangle plein à la couleur demandée.
    let color = match fill {
        RedactionFill::Black => [0.0, 0.0, 0.0],
        RedactionFill::White => [1.0, 1.0, 1.0],
        RedactionFill::Custom(rgb) => [
            f32::from(rgb[0]) / 255.0,
            f32::from(rgb[1]) / 255.0,
            f32::from(rgb[2]) / 255.0,
        ],
    };
    operations.push(Operation {
        operator: "q".to_string(),
        operands: Vec::new(),
    });
    operations.push(Operation {
        operator: "rg".to_string(),
        operands: color.iter().map(|c| Object::Real(*c)).collect(),
    });
    operations.push(Operation {
        operator: "re".to_string(),
        operands: vec![
            Object::Real(region.x1),
            Object::Real(region.y1),
            Object::Real(region.x2 - region.x1),
            Object::Real(region.y2 - region.y1),
        ],
    });
    operations.push(Operation {
        operator: "f".to_string(),
        operands: Vec::new(),
    });
    operations.push(Operation {
        operator: "Q".to_string(),
        operands: Vec::new(),
    });

    // Le texte superposé, posé dans la zone. Il est centré verticalement
    // dans la boîte : c'est là que l'œil va le chercher.
    let Some(overlay) = overlay else {
        return Ok(operations);
    };
    if overlay.text.trim().is_empty() {
        return Ok(operations);
    }
    let size = if overlay.size.is_finite() && overlay.size > 0.0 {
        overlay.size
    } else {
        12.0
    };
    let (_, font_name) = doc.ensure_patch_font_named(page_id, overlay.font.base_font(false, false))?;

    let text = encode_pdf_literal(&overlay.text);
    let text_object = Object::String(text, lopdf::StringFormat::Literal);
    let ink = crate::redact::geometry::estimate_width(&overlay.text, size, "Helvetica");
    let x = region.x1 + ((region.x2 - region.x1) - ink) / 2.0;
    let y = region.y1 + ((region.y2 - region.y1) - size * 0.7) / 2.0;

    operations.push(Operation {
        operator: "q".to_string(),
        operands: Vec::new(),
    });
    operations.push(Operation {
        operator: "BT".to_string(),
        operands: Vec::new(),
    });
    operations.push(Operation {
        operator: "rg".to_string(),
        operands: overlay
            .color
            .iter()
            .map(|c| Object::Real(f32::from(*c) / 255.0))
            .collect(),
    });
    operations.push(Operation {
        operator: "Tf".to_string(),
        operands: vec![
            Object::Name(font_name.into_bytes()),
            Object::Real(size),
        ],
    });
    operations.push(Operation {
        operator: "Td".to_string(),
        operands: vec![Object::Real(x), Object::Real(y)],
    });
    operations.push(Operation {
        operator: "Tj".to_string(),
        operands: vec![text_object],
    });
    operations.push(Operation {
        operator: "ET".to_string(),
        operands: Vec::new(),
    });
    operations.push(Operation {
        operator: "Q".to_string(),
        operands: Vec::new(),
    });

    Ok(operations)
}

/// Résout le sous-type d'un XObject (Image/Form) dans les ressources de la
/// page.
fn xobject_subtype(
    doc: &Document,
    page_id: lopdf::ObjectId,
    name: &[u8],
) -> Option<Vec<u8>> {
    let (res_opt, res_ids) = doc.inner().get_page_resources(page_id).ok()?;

    if let Some(res) = res_opt {
        if let Some(st) = lookup_xobject_subtype(doc, res, name) {
            return Some(st);
        }
    }
    for rid in res_ids {
        if let Ok(res) = doc.inner().get_dictionary(rid) {
            if let Some(st) = lookup_xobject_subtype(doc, res, name) {
                return Some(st);
            }
        }
    }
    None
}

fn lookup_xobject_subtype(
    doc: &Document,
    res: &lopdf::Dictionary,
    name: &[u8],
) -> Option<Vec<u8>> {
    let xobj = res.get(b"XObject").ok()?;
    let xobj_dict = match xobj {
        Object::Dictionary(d) => d,
        Object::Reference(id) => doc.inner().get_object(*id).ok()?.as_dict().ok()?,
        _ => return None,
    };
    let val = xobj_dict.get(name).ok()?;
    let stream_id = match val {
        Object::Reference(id) => *id,
        _ => return None,
    };
    let d = doc.inner().get_dictionary(stream_id).ok()?;
    let st = d.get(b"Subtype").ok()?;
    Some(st.as_name().ok()?.to_vec())
}

