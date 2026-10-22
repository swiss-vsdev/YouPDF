//! Chiffrement AES-256 (révision 6).
//!
//! Le fichier écrit doit porter un dictionnaire `/Encrypt` (`/V 5`, `/R 6`,
//! crypt filter `AESV3`), différer de la version en clair, et garder son
//! contenu illisible pour qui n'a pas le mot de passe. Le document en
//! mémoire, lui, doit rester en clair.

use std::path::PathBuf;

use youpdf_core::protect::{EncryptionOptions, EncryptionStrength, Permissions};
use youpdf_core::Document;

/// Une page blanche 612×792 avec un peu de texte.
fn page_blanche() -> Vec<u8> {
    use lopdf::{dictionary, Document as LopdfDoc, Object, Stream};

    let mut doc = LopdfDoc::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    });
    let content = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 24 Tf 60 700 Td (YouPDF) Tj ET".to_vec(),
    ));
    let page = doc.add_object(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => content,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
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

fn options() -> EncryptionOptions {
    EncryptionOptions {
        user_password: "mot-de-passe".to_string(),
        owner_password: "proprietaire".to_string(),
        permissions: Permissions::read_only(),
        strength: EncryptionStrength::Aes256,
    }
}

fn chemin(nom: &str) -> PathBuf {
    std::env::temp_dir().join(format!("youpdf-protect-{nom}-{}.pdf", std::process::id()))
}

/// Vrai si `foin` contient la suite d'octets `motif`.
fn contient(foin: &[u8], motif: &[u8]) -> bool {
    foin.windows(motif.len()).any(|f| f == motif)
}

/// Contenu brut du flux de la première page (chiffré, si le fichier l'est).
fn contenu_page_1(bytes: &[u8]) -> Vec<u8> {
    let doc = lopdf::Document::load_mem(bytes).expect("relecture");
    for (_num, id) in doc.get_pages() {
        let page = doc.get_dictionary(id).expect("page");
        if let Ok(contents) = page.get(b"Contents") {
            match contents {
                lopdf::Object::Reference(id) => {
                    if let Ok(stream) = doc.get_object(*id).and_then(|o| o.as_stream()) {
                        return stream.content.clone();
                    }
                }
                lopdf::Object::Array(arr) => {
                    for item in arr {
                        if let Ok(id) = item.as_reference() {
                            if let Ok(stream) = doc.get_object(id).and_then(|o| o.as_stream()) {
                                return stream.content.clone();
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
    Vec::new()
}

#[test]
fn chiffrer_ecrit_un_dictionnaire_encrypt() {
    let doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let clair = doc.to_bytes().expect("sérialisation en clair");

    let path = chemin("dict");
    doc.save_protected(&path, &options()).expect("chiffrement");
    let chiffre = std::fs::read(&path).expect("lecture");
    let _ = std::fs::remove_file(&path);

    assert!(chiffre != clair, "le fichier chiffré doit différer du clair");
    assert!(contient(&chiffre, b"/Encrypt"), "dictionnaire /Encrypt absent");
    assert!(contient(&chiffre, b"/V 5"), "version de chiffrement /V 5 attendue");
    assert!(contient(&chiffre, b"/R 6"), "révision /R 6 attendue");
    assert!(contient(&chiffre, b"/AESV3"), "crypt filter AESV3 attendu");
}

#[test]
fn chiffrer_ne_salie_pas_l_etat_en_memoire() {
    let doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let clair = doc.to_bytes().expect("clair");

    let path = chemin("etat");
    doc.save_protected(&path, &options()).expect("chiffrement");
    let _ = std::fs::remove_file(&path);

    let apres = doc.to_bytes().expect("clair après chiffrement");
    assert_eq!(apres, clair, "le document en mémoire doit rester en clair");
}

#[test]
fn sans_mot_de_passe_le_contenu_reste_illisible() {
    let doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let clair = doc.to_bytes().expect("clair");

    let path = chemin("illisible");
    doc.save_protected(&path, &options()).expect("chiffrement");
    let chiffre = std::fs::read(&path).expect("lecture");
    let _ = std::fs::remove_file(&path);

    // En clair, le contenu de page porte le marqueur ; chiffré, non.
    assert!(contient(&clair, b"YouPDF"), "marqueur absent du clair");
    assert!(
        !contient(&chiffre, b"YouPDF"),
        "le contenu ne doit pas rester lisible sans mot de passe"
    );

    // Le fichier se relit en structure, mais se sait chiffré : une
    // réouverture sans mot de passe n'aboutit pas à un contenu exploitable.
    let relu = lopdf::Document::load_mem(&chiffre).expect("relecture structurelle");
    assert!(relu.is_encrypted(), "le fichier doit se savoir chiffré");
    assert!(
        !contient(&contenu_page_1(&chiffre), b"YouPDF"),
        "le flux de contenu ne doit pas être en clair"
    );
}

/// Un fichier RC4-128 se rouvre avec son mot de passe ; un fichier AES-256
/// ne peut pas l'être, et l'erreur doit le dire : `lopdf` ne déchiffre que
/// les révisions 2 et 3.
#[test]
fn rouvrir_explique_que_l_aes_256_est_illisible() {
    let doc = Document::from_bytes(&page_blanche()).expect("ouverture");

    let path_rc4 = chemin("rc4");
    let mut options_rc4 = options();
    options_rc4.strength = EncryptionStrength::Rc4_128;
    doc.save_protected(&path_rc4, &options_rc4)
        .expect("chiffrement RC4");
    let relu = Document::open_protected(&path_rc4, "mot-de-passe");
    let _ = std::fs::remove_file(&path_rc4);
    assert!(
        relu.is_ok(),
        "un fichier RC4-128 doit se rouvrir avec son mot de passe"
    );

    let path_aes = chemin("aes");
    doc.save_protected(&path_aes, &options())
        .expect("chiffrement AES");
    let message = match Document::open_protected(&path_aes, "mot-de-passe") {
        Ok(_) => panic!("un fichier AES-256 ne peut pas être rouvert par lopdf"),
        Err(erreur) => erreur.to_string(),
    };
    let _ = std::fs::remove_file(&path_aes);
    assert!(
        !message.contains("mot de passe refusé"),
        "l'erreur ne doit pas accuser le mot de passe : {message}"
    );
    assert!(
        message.contains("AES-256"),
        "l'erreur doit nommer la cause : {message}"
    );
}

/// Un mauvais mot de passe sur un fichier RC4 doit toujours être signalé
/// comme tel : le message ne vaut que si les deux cas sont distincts.
#[test]
fn un_mauvais_mot_de_passe_reste_signale_comme_tel() {
    let doc = Document::from_bytes(&page_blanche()).expect("ouverture");
    let mut options = options();
    options.strength = EncryptionStrength::Rc4_128;

    let path = chemin("mauvais-mot-de-passe");
    doc.save_protected(&path, &options)
        .expect("chiffrement RC4");
    let message = match Document::open_protected(&path, "mauvais") {
        Ok(_) => panic!("le mauvais mot de passe doit être refusé"),
        Err(erreur) => erreur.to_string(),
    };
    let _ = std::fs::remove_file(&path);
    assert!(
        message.contains("mot de passe refusé"),
        "un mauvais mot de passe doit le dire : {message}"
    );
}

/// Le choix affiché dans la fenêtre « Protéger » doit dire ce que l'app sait
/// faire : seul le RC4 se rouvre ici, l'AES-256 s'ouvre ailleurs.
#[test]
fn le_libelle_du_chiffrement_annonce_ce_que_l_app_sait_rouvrir() {
    assert!(
        EncryptionStrength::Rc4_128
            .display_name()
            .contains("reopenable"),
        "RC4-128 doit se dire rouvrable : {}",
        EncryptionStrength::Rc4_128.display_name()
    );
    assert!(
        EncryptionStrength::Aes256
            .display_name()
            .contains("another reader"),
        "AES-256 doit dire qu'il s'ouvre ailleurs : {}",
        EncryptionStrength::Aes256.display_name()
    );
}
