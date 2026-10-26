//! Réordonnancement de pages : entrelacement et échange de plages.
use lopdf::Object;
use std::ops::Range;
use youpdf_shared::{Error, Result};

use crate::document::Document;

/// Applique un nouvel ordre de pages.
///
/// `order` est une permutation des indices de page 0-indexés dans l'ordre
/// documentaire. Les pages sont toutes rattachées à la racine pour que l'ordre
/// soit explicite, les nœuds intermédiaires étant vidés.
///
/// ponytail: aplatit l'arbre `/Pages` ; un document à milliers de pages gagne
/// une réécriture en place des `/Kids` par sous-arbre plutôt qu'un aplatissage.
fn apply_order(doc: &mut Document, order: &[usize]) -> Result<()> {
    let ids: Vec<lopdf::ObjectId> = doc.page_ids().into_values().collect();
    if order.len() != ids.len() || order.iter().any(|&i| i >= ids.len()) {
        return Err(Error::InvalidInput("ordre de pages invalide".into()));
    }
    let root = doc.pages_root_id()?;

    // Vider les nœuds intermédiaires : chaque page ne pendre qu'à la racine.
    let mut branches = vec![root];
    let mut seen = 0;
    while seen < branches.len() {
        let node = branches[seen];
        seen += 1;
        for child in doc.kids_of(node) {
            if let Object::Reference(id) = child {
                if doc.is_branch(id) {
                    branches.push(id);
                }
            }
        }
    }
    for node in branches.iter().skip(1) {
        doc.set_kids(*node, Vec::new())?;
    }

    let new_ids: Vec<lopdf::ObjectId> = order.iter().map(|&i| ids[i]).collect();
    doc.set_kids(
        root,
        new_ids.iter().copied().map(Object::Reference).collect(),
    )?;
    doc.set_pages_parent(root, &new_ids);
    doc.refresh_page_tree()?;
    doc.touch();
    Ok(())
}

/// Entrelace les pages paires et impaires.
///
/// `even_first == true` : [0,2,4,…,1,3,5,…] — sinon l'inverse.
pub fn interleave_pages(doc: &mut Document, even_first: bool) -> Result<()> {
    let total = doc.page_count();
    let alternation = |start: usize| (start..total).step_by(2);
    let order: Vec<usize> = if even_first {
        alternation(0).chain(alternation(1)).collect()
    } else {
        alternation(1).chain(alternation(0)).collect()
    };
    apply_order(doc, &order)
}

/// Échange deux plages disjointes : avant A, puis B, puis ce qui les séparait,
/// puis A, puis après B.
pub fn exchange_ranges(doc: &mut Document, range_a: Range<usize>, range_b: Range<usize>) -> Result<()> {
    let total = doc.page_count();
    if range_a.end > range_b.start || range_b.end > total {
        return Err(Error::InvalidInput(
            "les plages doivent être disjointes et comprise dans le document".into(),
        ));
    }
    let mut order: Vec<usize> = (0..range_a.start).collect();
    order.extend(range_b.start..range_b.end);
    order.extend(range_a.end..range_b.start);
    order.extend(range_a);
    order.extend(range_b.end..total);
    apply_order(doc, &order)
}
