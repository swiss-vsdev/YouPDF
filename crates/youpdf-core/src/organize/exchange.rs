use lopdf::Object;
use std::ops::Range;
use crate::document::Document;
use youpdf_shared::Result;

/// Interleave pages: even-indexed and odd-indexed pages alternate.
/// even_first=true: [0,2,4,...,1,3,5,...]
/// even_first=false: [1,3,5,...,0,2,4,...]
pub fn interleave_pages(doc: &mut Document, even_first: bool) -> Result<()> {
    let total = doc.page_count();

    let even: Vec<usize> = (0..total).filter(|&i| i % 2 == 0).collect();
    let odd: Vec<usize> = (0..total).filter(|&i| i % 2 == 1).collect();

    let result: Vec<usize> = if even_first {
        // even_first=true: [0,2,4,...,1,3,5,...]
        let mut r = Vec::new();
        let mut e = 0;
        let mut o = 0;
        while e < even.len() || o < odd.len() {
            if e < even.len() {
                r.push(even[e]);
                e += 1;
            }
            if o < odd.len() {
                r.push(odd[o]);
                o += 1;
            }
        }
        r
    } else {
        // even_first=false: [1,3,5,...,0,2,4,...]
        let mut r = Vec::new();
        let mut o = 0;
        let mut e = 0;
        while o < odd.len() || e < even.len() {
            if o < odd.len() {
                r.push(odd[o]);
                o += 1;
            }
            if e < even.len() {
                r.push(even[e]);
                e += 1;
            }
        }
        r
    };

    doc.touch();
    Ok(())
}

/// Exchange two disjoint page ranges.
///
/// New order: everything before A, B, A, everything after B.
pub fn exchange_ranges(doc: &mut Document, range_a: Range<usize>, range_b: Range<usize>) -> Result<()> {
    let total = doc.page_count();

    // Valider que les plages sont disjointes et que range_a est avant range_b
    assert!(range_a.end <= range_b.start,
            "Les plages doivent être disjointes et range_a doit être avant range_b");

    let before_a: Vec<usize> = (0..range_a.start).collect();
    let a_pages: Vec<usize> = (range_a.start..range_a.end).collect();
    let b_pages: Vec<usize> = (range_b.start..range_b.end).collect();
    let after: Vec<usize> = (range_b.end..total).collect();

    // Concaténer les vecteurs
    let mut new_order = before_a;
    new_order.extend(b_pages);
    new_order.extend(a_pages);
    new_order.extend(after);

    doc.touch();
    Ok(())
}