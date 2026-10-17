//! Tests de non-régression des correctifs de l'audit.
//!
//! Chaque test échouait sur le code d'origine.
use lopdf::{Dictionary, Object, Stream, dictionary};
use youpdf_core::{Document, SignaturePlacement};

/// PDF minimal dont la page déclare `/F1` = Courier.
fn doc_with_f1(label: &str) -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(label.as_bytes().to_vec()),
    });
    let cid = doc.add_object(Stream::new(
        Dictionary::new(),
        b"BT /F1 24 Tf 100 700 Td (ORIGINAL) Tj ET".to_vec(),
    ));
    let pid = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => cid,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => Object::Reference(font) } },
    });
    let pages = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![pid.into()]),
        "Count" => 1,
    });
    let cat = doc.add_object(dictionary! {
    "Type" => Object::Name(b"Catalog".to_vec()), "Pages" => pages });
    doc.trailer.set("Root", Object::Reference(cat));
    // /Info : doit être chiffré comme tout le reste.
    let info = doc.add_object(dictionary! {
        "Title" => Object::String(b"SECRET-TITLE-MARKER".to_vec(), lopdf::StringFormat::Literal),
    });
    doc.trailer.set("Info", Object::Reference(info));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

fn base_font_of(doc: &lopdf::Document, resource: &str) -> Option<String> {
    let page_id = *doc.get_pages().values().next()?;
    let page = doc.get_dictionary(page_id).ok()?;
    let res = page
        .get(b"Resources")
        .and_then(|o| o.as_dict())
        .ok()?
        .get(b"Font")
        .and_then(|o| o.as_dict())
        .ok()?
        .clone();
    let id = res.get(resource.as_bytes()).ok()?.as_reference().ok()?;
    let obj = doc.get_object(id).ok()?;
    let name = obj.as_dict().ok()?.get(b"BaseFont").ok()?;
    let name = name.as_name().ok()?;
    Some(String::from_utf8_lossy(name).into_owned())
}

/// **CVE-like** : le nom alternatif de la signature est saisi par un tiers.
/// Un `)` non échappé ferme le littéral et permet d'ajouter des opérateurs
/// (texte arbitraire, changement de police…) au flux de la page signée.
#[test]
fn signature_label_cannot_inject_operators() {
    let mut doc = Document::from_bytes(&doc_with_f1("Courier")).unwrap();
    doc.place_visual_signature(&SignaturePlacement {
        page: 1,
        rect: [100.0, 100.0, 320.0, 140.0],
        image_bytes: None,
        // `<…>` n'était pas échappé non plus : pas besoin de parenthèse.
        text_alternative: ") Tj /F1 30 Tf 60 400 Td <494E4A4543544544> Tj".into(),
        signer_name: "Jane Doe".into(),
    })
    .unwrap();

    let bytes = doc.to_bytes().unwrap();
    let d = lopdf::Document::load_mem(&bytes).unwrap();
    let page = d
        .get_dictionary(*d.get_pages().values().next().unwrap())
        .unwrap();
    let mut text = String::new();
    let ids: Vec<Object> = match page.get(b"Contents").unwrap() {
        Object::Reference(r) => vec![Object::Reference(*r)],
        Object::Array(a) => a.clone(),
        _ => vec![],
    };
    for o in ids {
        if let Ok(Object::Reference(id)) = o.as_reference().map(Object::Reference) {
            if let Ok(Object::Stream(s)) = d.get_object(id) {
                text.push_str(&String::from_utf8_lossy(&s.content));
            }
        }
    }
    // L'opérateur doit rester **à l'intérieur** de la chaîne : jamais exécutable.
    assert!(
        !text.contains("<494E4A4543544544> Tj"),
        "injection d'opérateur possible dans le flux :\n{text}"
    );
    assert!(
        text.contains("\\) Tj /F1 30 Tf"),
        "le libellé doit être affiché littéralement, pas supprimé"
    );
}

/// L'ajout d'une signature ne doit pas réécrire la police `/F1` existante :
/// elle désigne une police légitime du document (ici Courier).
#[test]
fn signature_does_not_clobber_page_font() {
    let mut doc = Document::from_bytes(&doc_with_f1("Courier")).unwrap();
    doc.place_visual_signature(&SignaturePlacement {
        page: 1,
        rect: [100.0, 100.0, 320.0, 140.0],
        image_bytes: None,
        text_alternative: "Jane Doe".into(),
        signer_name: "Jane Doe".into(),
    })
    .unwrap();

    let d = lopdf::Document::load_mem(&doc.to_bytes().unwrap()).unwrap();
    assert_eq!(
        base_font_of(&d, "F1").as_deref(),
        Some("Courier"),
        "la police /F1 du document a été remplacée par la signature"
    );
    assert_eq!(
        base_font_of(&d, "YouPdfF1").as_deref(),
        Some("Helvetica"),
        "la police dédiée à la signature devrait être ajoutée sous un nom propre"
    );
}

/// Les polices standard sont déclarées `WinAnsiEncoding` : un littéral doit
/// contenir **un octet par caractère**. En UTF-8, « é » pèse deux octets et
/// s'affiche « Ã© ».
#[test]
fn patched_text_is_winansi_not_utf8() {
    use youpdf_core::encode_pdf_literal;

    assert_eq!(encode_pdf_literal("café"), b"caf\xe9");
    assert_eq!(encode_pdf_literal("cœur"), b"c\x9cur");
    assert_eq!(encode_pdf_literal("10 €"), b"10 \x80");
    // Hors WinAnsi : replacement explicite, pas d'octet arbitraire.
    assert_eq!(encode_pdf_literal("你好"), b"??");
    // Échappement structurel.
    assert_eq!(encode_pdf_literal("a(b)c\\d"), b"a\\(b\\)c\\\\d");

    // Et le flux écrit dans le document ne contient pas de UTF-8.
    let mut doc = Document::from_bytes(&doc_with_f1("Helvetica")).unwrap();
    let frags = doc.extract_text_fragments(1).expect("fragments");
    assert!(!frags.is_empty(), "aucun fragment extrait");
    doc.replace_text_fragment(&frags[0], "café", None).unwrap();
    let bytes = doc.to_bytes().unwrap();
    assert!(
        bytes.windows(4).any(|w| w == b"caf\xe9"),
        "le texte remplacé devrait être encodé en WinAnsi"
    );
    assert!(
        !bytes.windows(6).any(|w| w == b"caf\xc3\xa9"),
        "du UTF-8 s'est glissé dans un littéral WinAnsi (→ « cafÃ© »)"
    );
}

/// Le patch de réécriture doit poser la couleur demandée pour le fond et
/// reposer explicitement celle du texte : sans cela, il hérite de la couleur
/// de remplissage laissée par le contenu de la page (blanc par défaut), ce qui
/// rend le texte réécrit illisible sur une page teintée ou masque mal le fond.
#[test]
fn replace_text_paints_the_requested_colours() {
    let mut doc = Document::from_bytes(&doc_with_f1("Helvetica")).unwrap();
    let frags = doc.extract_text_fragments(1).expect("fragments");
    assert!(!frags.is_empty(), "aucun fragment extrait");
    let mut fragment = frags[0].clone();
    fragment.color = [10, 20, 30];

    doc.replace_text_fragment(&fragment, "test", Some([0.5, 0.25, 1.0]))
        .unwrap();

    let bytes = doc.to_bytes().unwrap();
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("0.50 0.25 1.00 rg"),
        "le fond demandé n'apparaît pas dans le patch"
    );
    assert!(
        text.contains("0.039 0.078 0.118 rg"),
        "la couleur du texte n'est pas reposée explicitement"
    );
}

/// AES-256 (révision 6) : le hash renforcé est en **AES-128-CBC**, la
/// condition d'arrêt regarde le dernier octet de `E`, et `/Length` des flux
/// doit suivre la taille chiffrée. Sans ces trois points, le fichier produit
/// est illisible par les lecteurs du marché.
#[test]
fn aes256_output_is_readable_and_metadata_encrypted() {
    let bytes = doc_with_f1("Helvetica");
    let mut doc = Document::from_bytes(&bytes).unwrap();
    assert_eq!(
        doc.metadata().title.as_deref(),
        Some("SECRET-TITLE-MARKER"),
        "le PDF de test devrait porter un /Title"
    );
    doc.encrypt(&youpdf_core::EncryptionOptions {
        user_password: "user".into(),
        owner_password: "owner".into(),
        permissions: youpdf_core::Permissions::all(),
        strength: youpdf_core::EncryptionStrength::Aes256,
    })
    .expect("chiffrement AES-256");

    let out = doc.to_bytes().unwrap();
    assert!(
        !out.windows(b"SECRET-TITLE-MARKER".len())
            .any(|w| w == b"SECRET-TITLE-MARKER"),
        "les métadonnées /Info fuient en clair dans le fichier chiffré"
    );

    // lopdf ne gère pas la révision 6 côté lecture ; `scripts/verify-encryption.sh`
    // (poppler) valide la lecture réelle avec mot de passe.
    assert!(out.starts_with(b"%PDF-"));
    assert!(out.windows(6).any(|w| w == b"/AESV3"));
    assert!(out.windows(6).any(|w| w == b"/Perms"));
}

/// `/Length` d'un flux doit refléter la taille **chiffrée** (IV + corps +
/// bourrage). Sinon le lecteur lit un tronçon et échoue (« Missing
/// 'endstream' or incorrect stream length »).
#[test]
fn encrypted_stream_length_matches_content() {
    let bytes = doc_with_f1("Helvetica");
    let mut doc = Document::from_bytes(&bytes).unwrap();
    doc.encrypt(&youpdf_core::EncryptionOptions {
        user_password: "user".into(),
        owner_password: "owner".into(),
        permissions: youpdf_core::Permissions::all(),
        strength: youpdf_core::EncryptionStrength::Aes256,
    })
    .unwrap();
    let out = doc.to_bytes().unwrap();

    // Chaque « stream … endstream » doit avoir la longueur annoncée.
    // Attention à rester en **octets** : un `String::from_utf8_lossy`
    // gonflerait les octets binaires invalides en U+FFFD (3 octets) et
    // fausserait toute mesure.
    fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
        hay[from..]
            .windows(needle.len())
            .position(|w| w == needle)
            .map(|p| p + from)
    }

    let mut ok = 0;
    let mut pos = 0;
    while let Some(i) = find(&out, b"stream", pos) {
        if out[..i].ends_with(b"end") {
            pos = i + 6;
            continue; // « endstream »
        }
        let mut start = i + "stream".len();
        while matches!(out.get(start), Some(b'\n') | Some(b'\r')) {
            start += 1;
        }
        let Some(rel_end) = find(&out, b"endstream", start) else {
            break;
        };
        let mut end = rel_end;
        if out.get(end.wrapping_sub(1)) == Some(&b'\n') {
            end -= 1;
        }
        let declared = out[..start]
            .rsplit(|b| *b == b'/')
            .find(|s| s.starts_with(b"Length"))
            .and_then(|s| {
                let digits: Vec<u8> = s["Length".len()..]
                    .iter()
                    .copied()
                    .skip_while(|b| *b == b' ' || *b == b'\r' || *b == b'\n')
                    .take_while(|b| b.is_ascii_digit())
                    .collect();
                String::from_utf8(digits).ok()?.parse::<usize>().ok()
            });
        if let Some(n) = declared {
            assert_eq!(
                n,
                end - start,
                "/Length ({n}) ≠ contenu réel ({}) pour le flux à l'offset {start}",
                end - start
            );
            ok += 1;
        }
        pos = end + "endstream".len();
    }
    assert!(ok > 0, "aucun flux vérifié : test inopérant");
}
