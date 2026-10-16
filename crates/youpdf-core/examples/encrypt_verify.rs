//! Chiffre un PDF réel et le fait relire par un lecteur tiers.
//!
//! `lopdf` ne sait pas déchiffrer la révision 6 : ce binaire sert de
//! vérification *externe* (poppler, qpdf…) de la conformité ISO 32000 de
//! `protect::encrypt`, là où les tests unitaires ne voient que la structure.
//!
//! ```sh
//! cargo run --release --example encrypt_verify -- in.pdf aes256 /tmp/out.pdf
//! pdfinfo    -upw user /tmp/out.pdf   # doit annoncer « algorithm:AES-256 »
//! pdftotext  -upw user /tmp/out.pdf - # doit restituer le texte
//! pdftotext  -upw WRONG  /tmp/out.pdf - # doit refuser
//! ```
use youpdf_core::{Document, EncryptionOptions, EncryptionStrength, Permissions};

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let (src, kind, out) = match args.as_slice() {
        [s, k, o] => (s.clone(), k.clone(), o.clone()),
        _ => {
            eprintln!("usage : encrypt_verify <source.pdf> <aes256|rc4> <sortie.pdf>");
            std::process::exit(2);
        }
    };

    let mut doc = Document::open(&src).expect("ouverture du PDF source");
    let strength = match kind.as_str() {
        "rc4" => EncryptionStrength::Rc4_128,
        "aes256" => EncryptionStrength::Aes256,
        other => {
            eprintln!("algorithme inconnu : {other} (attendu aes256 ou rc4)");
            std::process::exit(2);
        }
    };

    doc.encrypt(&EncryptionOptions {
        user_password: "user".into(),
        owner_password: "owner".into(),
        permissions: Permissions::all(),
        strength,
    })
    .expect("chiffrement");

    doc.save(&out).expect("sauvegarde");
    println!("{kind} : {out} écrit — vérifiez avec `pdfinfo -upw user {out}`");
}
