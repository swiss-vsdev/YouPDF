//! Page labels round-trip and formatter tests.

use youpdf_core::organize::labels::{get_page_labels, set_page_labels, LabelStyle, PageLabel};
use youpdf_core::create::blank_document;
use std::collections::BTreeMap;

#[test]
fn test_round_trip_arabic() {
    let mut doc = blank_document(3, 612.0, 792.0).unwrap();
    let mut labels = BTreeMap::new();
    labels.insert(0, PageLabel::new(None::<String>, LabelStyle::Arabic, 1));
    labels.insert(2, PageLabel::new(Some("App-".to_string()), LabelStyle::Arabic, 1));
    set_page_labels(&mut doc, &labels).unwrap();
    let read = get_page_labels(&doc).unwrap();
    assert_eq!(read.len(), 2);
    assert_eq!(read.get(&0).unwrap().style, LabelStyle::Arabic);
    assert_eq!(read.get(&2).unwrap().prefix.as_deref(), Some("App-"));
}

#[test]
fn test_round_trip_roman() {
    let mut doc = blank_document(3, 612.0, 792.0).unwrap();
    let mut labels = BTreeMap::new();
    labels.insert(0, PageLabel::new(None::<String>, LabelStyle::RomanUpper, 1));
    set_page_labels(&mut doc, &labels).unwrap();
    let read = get_page_labels(&doc).unwrap();
    assert_eq!(read.get(&0).unwrap().style, LabelStyle::RomanUpper);
}

#[test]
fn test_remove_labels() {
    let mut doc = blank_document(3, 612.0, 792.0).unwrap();
    let mut labels = BTreeMap::new();
    labels.insert(0, PageLabel::new(None::<String>, LabelStyle::Arabic, 1));
    set_page_labels(&mut doc, &labels).unwrap();
    assert!(!get_page_labels(&doc).unwrap().is_empty());
    set_page_labels(&mut doc, &BTreeMap::new()).unwrap();
    assert!(get_page_labels(&doc).unwrap().is_empty());
}

#[test]
fn test_letter_increment() {
    assert_eq!(LabelStyle::LetterUpper.format(26), "Z");
    assert_eq!(LabelStyle::LetterUpper.format(27), "AA");
    assert_eq!(LabelStyle::LetterLower.format(52), "az");
    assert_eq!(LabelStyle::LetterUpper.format(53), "BA");
}