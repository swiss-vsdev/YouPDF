use youpdf_core::organize::labels::{get_label, read_from_pdf, set_label, write_to_pdf, NumStyle, PageLabel};

#[test]
fn roman_upper_correct() {
    use crate::organize::labels::to_roman;
    assert_eq!(to_roman(1), "I");
    assert_eq!(to_roman(4), "IV");
    assert_eq!(to_roman(5), "V");
    assert_eq!(to_roman(9), "IX");
    assert_eq!(to_roman(10), "X");
    assert_eq!(to_roman(1999), "MCMXCIX");
}

#[test]
fn letters_wrap_at_z() {
    use crate::organize::labels::to_letters;
    assert_eq!(to_letters(1), "A");
    assert_eq!(to_letters(26), "Z");
    assert_eq!(to_letters(27), "AA");
    assert_eq!(to_letters(52), "AZ");
}

#[test]
fn label_for_page_sparse() {
    let labels = vec![
        PageLabel {
            prefix: "".to_string(),
            style: NumStyle::Decimal,
            start: 1,
        },
        PageLabel {
            prefix: "A-".to_string(),
            style: NumStyle::LettersUpper,
            start: 11,
        },
    ];
    assert_eq!(get_label(&labels, 1), "1");
    assert_eq!(get_label(&labels, 5), "5");
    assert_eq!(get_label(&labels, 11), "A-A");
    assert_eq!(get_label(&labels, 15), "A-E");
}

#[test]
fn get_label_no_segments() {
    let labels: Vec<PageLabel> = vec![];
    assert_eq!(get_label(&labels, 1), "");
    assert_eq!(get_label(&labels, 100), "");
}

#[test]
fn roundtrip_pdf() {
    use crate::document::Document;

    // Create a blank document
    let mut doc = Document::from_bytes(b"%PDF-1.4\n% minimal\n%%EOF")
        .expect("blank doc");

    // Write some labels
    let labels = vec![
        PageLabel {
            prefix: "".to_string(),
            style: NumStyle::Decimal,
            start: 1,
        },
        PageLabel {
            prefix: "A-".to_string(),
            style: NumStyle::LettersUpper,
            start: 11,
        },
    ];
    let result = write_to_pdf(&mut doc, &labels);
    assert!(result.is_ok(), "write_to_pdf should succeed");

    // Read back
    let read = read_from_pdf(&doc);
    assert!(result.is_ok());
    // Compare structural equality (order may differ, so sort-compare)
    assert_eq!(read.len(), labels.len());
    for (a, b) in read.iter().zip(labels.iter()) {
        assert_eq!(a.prefix, b.prefix);
        assert_eq!(a.style, b.style);
        assert_eq!(a.start, b.start);
    }
}