//! Chiffrement par mot de passe.
//!
//! ## Deux niveaux disponibles
//!
//! - **RC4-128 (révision 3)** : largement compatible, mais RC4 est
//!   considéré faible aujourd'hui. Recommandé uniquement pour la
//!   compatibilité avec de vieux lecteurs.
//! - **AES-256 (révision 6)** : standard actuel (PDF 2.0). C'est le
//!   choix par défaut recommandé par YouPDF.
//!
//! ## Algorithmes
//!
//! - Révision 3 : ISO 32000-1 §7.6.3.3 (MD5, RC4).
//! - Révision 6 : ISO 32000-2 §7.6.4.3.4 (SHA-256/384/512, AES-256-CBC).
//!
//! L'implémentation AES-256 suit l'algorithme 2.B de la spécification,
//! qui effectue un durcissement itératif sur la clé. C'est complexe
//! mais nécessaire pour être conforme.

use std::path::Path;

use aes::Aes256;
use cbc::cipher::{block_padding::NoPadding, BlockDecryptMut, BlockEncryptMut, KeyIvInit};
use cbc::{Decryptor, Encryptor};
use lopdf::{Dictionary, Document as LopdfDoc, Object, ObjectId};
use md5::{Digest as Md5Digest, Md5};
use sha2::{Sha256, Sha384, Sha512};

use youpdf_shared::{Error, Result};

use crate::document::Document;

/// Niveau de chiffrement souhaité.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EncryptionStrength {
    /// RC4 128 bits (révision 3). Compatible, faible — mais réouvrable ici.
    Rc4_128,
    /// AES 256 bits (révision 6). Standard actuel, fort — mais `lopdf` ne le
    /// déchiffre pas : le fichier produit s'ouvre dans un autre lecteur.
    #[default]
    Aes256,
}

impl EncryptionStrength {
    pub fn display_name(&self) -> &'static str {
        match self {
            EncryptionStrength::Rc4_128 => "RC4-128 (reopenable here)",
            // L'AES-256 est le plus fort, mais `lopdf` ne sait pas le
            // déchiffrer : le fichier produit s'ouvre dans un autre lecteur,
            // pas dans celui-ci.
            EncryptionStrength::Aes256 => "AES-256 (strongest, reopen in another reader)",
        }
    }
}

/// Permissions accordées par le document protégé.
#[derive(Debug, Clone, Copy)]
pub struct Permissions {
    pub print: bool,
    pub modify: bool,
    pub copy: bool,
    pub annotate: bool,
    pub fill_forms: bool,
    pub accessibility: bool,
    pub assemble: bool,
    pub print_high_quality: bool,
}

impl Permissions {
    pub fn all() -> Self {
        Self {
            print: true,
            modify: true,
            copy: true,
            annotate: true,
            fill_forms: true,
            accessibility: true,
            assemble: true,
            print_high_quality: true,
        }
    }

    pub fn read_only() -> Self {
        Self {
            print: true,
            modify: false,
            copy: false,
            annotate: false,
            fill_forms: false,
            accessibility: true,
            assemble: false,
            print_high_quality: true,
        }
    }

    /// Convertit en entier 32 bits selon la spec PDF.
    fn to_pdf_flags(self) -> i32 {
        let mut flags: i32 = 0;
        if self.print {
            flags |= 0x0004;
        }
        if self.modify {
            flags |= 0x0008;
        }
        if self.copy {
            flags |= 0x0010;
        }
        if self.annotate {
            flags |= 0x0020;
        }
        if self.fill_forms {
            flags |= 0x0100;
        }
        if self.accessibility {
            flags |= 0x0200;
        }
        if self.assemble {
            flags |= 0x0400;
        }
        if self.print_high_quality {
            flags |= 0x0800;
        }
        flags |= 0xFFFFF0C0u32 as i32;
        flags
    }
}

/// Options de chiffrement.
#[derive(Debug, Clone)]
pub struct EncryptionOptions {
    pub user_password: String,
    pub owner_password: String,
    pub permissions: Permissions,
    /// Niveau de chiffrement à utiliser.
    pub strength: EncryptionStrength,
}

impl Default for EncryptionOptions {
    fn default() -> Self {
        Self {
            user_password: String::new(),
            owner_password: String::new(),
            permissions: Permissions::all(),
            strength: EncryptionStrength::Aes256,
        }
    }
}

// ────────────────────────────────────────────────────────────────────────
// Constantes communes
// ────────────────────────────────────────────────────────────────────────

/// Chaîne de remplissage officielle (ISO 32000-1, tableau 21).
/// Utilisée par les révisions 3 et 4.
const PAD: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

const KEY_LEN_R3: usize = 16; // RC4-128

// ────────────────────────────────────────────────────────────────────────
// Point d'entrée
// ────────────────────────────────────────────────────────────────────────

impl Document {
    /// Chiffre le document avec les options fournies.
    pub fn encrypt(&mut self, opts: &EncryptionOptions) -> Result<()> {
        match opts.strength {
            EncryptionStrength::Rc4_128 => self.encrypt_rc4_128(opts),
            EncryptionStrength::Aes256 => self.encrypt_aes_256(opts),
        }
    }

    /// Chiffre le document et l'écrit à `path`.
    ///
    /// Le chiffrement s'applique à une **copie** : le document en mémoire
    /// reste en clair et éditable, l'état de travail n'est pas contaminé
    /// par les objets chiffrés ni par le dictionnaire `/Encrypt`.
    pub fn save_protected(
        &self,
        path: impl AsRef<Path>,
        opts: &EncryptionOptions,
    ) -> Result<()> {
        let mut copie = Document::from_lopdf(self.inner.clone());
        copie.encrypt(opts)?;
        copie.save(path)
    }

    /// Vrai si le fichier porte un dictionnaire `/Encrypt`.
    ///
    /// Un fichier illisible renvoie `false` : l'appelant enchaînera sur une
    /// ouverture normale, qui remontera l'erreur de lecture.
    pub fn file_is_encrypted(path: impl AsRef<Path>) -> bool {
        lopdf::Document::load(path.as_ref())
            .map(|doc| doc.is_encrypted())
            .unwrap_or(false)
    }

    /// Ouvre un PDF en le déchiffrant quand il est protégé.
    ///
    /// Le mot de passe peut être le mot de passe utilisateur ou celui du
    /// propriétaire : `lopdf` accepte les deux.
    ///
    /// `lopdf` ne sait déchiffrer que les révisions 2 et 3 (RC4). Un fichier
    /// en AES-256 (révision 6) — que `encrypt` sait écrire — est donc
    /// illisible ici : l'erreur le dit, plutôt que de laisser croire à un
    /// mot de passe erroné.
    pub fn open_protected(path: impl AsRef<Path>, password: &str) -> Result<Self> {
        let path = path.as_ref();
        let mut inner = lopdf::Document::load(path)
            .map_err(|e| Error::Pdf(format!("{} : {e}", path.display())))?;
        if inner.is_encrypted() {
            inner
                .decrypt(password.as_bytes())
                .map_err(|e| Error::Pdf(decrypt_error(e)))?;
        }
        let mut doc = Document::from_lopdf(inner);
        doc.path = Some(path.to_path_buf());
        Ok(doc)
    }

    /// Écrit une copie en clair du document, sans `/Encrypt`.
    ///
    /// Sert à « Enlever la protection » : la copie reste modifiable et
    /// n'exige plus de mot de passe. L'original n'est jamais réécrit.
    pub fn save_decrypted(&self, path: impl AsRef<Path>) -> Result<()> {
        let mut copie = Document::from_lopdf(self.inner.clone());
        copie.inner_mut().trailer.remove(b"Encrypt");
        copie.save(path)
    }
}

/// Traduit une erreur de déchiffrement en un message qui ne confond pas un
/// algorithme non supporté avec un mot de passe erroné.
///
/// `lopdf` ne déchiffre que les révisions 2 et 3. Un fichier en AES-256
/// échoue donc sur l'algorithme, jamais sur le mot de passe : dire « mot de
/// passe refusé » enverrait l'utilisateur sur la mauvaise piste.
fn decrypt_error(error: lopdf::Error) -> String {
    match error {
        lopdf::Error::Decryption(
            lopdf::encryption::DecryptionError::UnsupportedEncryption
            | lopdf::encryption::DecryptionError::InvalidKeyLength
            | lopdf::encryption::DecryptionError::InvalidRevision,
        ) => "chiffrement AES-256 non lisible par l'application \
             (seul RC4-128 peut être rouvert ici) : ouvrez le fichier avec un lecteur qui gère l'AES-256"
            .to_owned(),
        other => format!("mot de passe refusé : {other}"),
    }
}

// ────────────────────────────────────────────────────────────────────────
// RC4-128 (révision 3)
// ────────────────────────────────────────────────────────────────────────

impl Document {
    fn encrypt_rc4_128(&mut self, opts: &EncryptionOptions) -> Result<()> {
        let user_pwd = pad_password(&opts.user_password);
        let owner_pwd = pad_password(&opts.owner_password);
        let doc_id = generate_id(16);

        let permissions = opts.permissions.to_pdf_flags();
        let o_value = r3_owner_entry(&owner_pwd, &user_pwd);
        let key = r3_encryption_key(&user_pwd, &o_value, permissions, &doc_id);
        let u_value = r3_user_entry(&key, &doc_id);

        encrypt_objects_r3(self.inner_mut(), &key)?;

        let encrypt_dict = build_encrypt_dict_r3(&o_value, &u_value, permissions);
        let encrypt_id = self
            .inner_mut()
            .add_object(Object::Dictionary(encrypt_dict));

        let trailer = &mut self.inner_mut().trailer;
        trailer.set("Encrypt", Object::Reference(encrypt_id));
        trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(doc_id.clone(), lopdf::StringFormat::Hexadecimal),
                Object::String(doc_id, lopdf::StringFormat::Hexadecimal),
            ]),
        );

        tracing::info!("chiffrement RC4-128 (révision 3) appliqué");
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────
// AES-256 (révision 6)
// ────────────────────────────────────────────────────────────────────────
//
// Implémentation selon ISO 32000-2, algorithmes 2.A (révision 5) et
// 2.B (révision 6). La révision 6 introduit un durcissement itératif
// de la clé basé sur AES-CBC et SHA-256/384/512, conçu pour être lent
// et coûteux en mémoire.
//
// Cette implémentation suit fidèlement la spécification, mais elle est
// complexe et mérite des tests avant usage en production.

impl Document {
    fn encrypt_aes_256(&mut self, opts: &EncryptionOptions) -> Result<()> {
        let user_pwd_bytes = opts.user_password.as_bytes();
        let owner_pwd_bytes = opts.owner_password.as_bytes();
        let doc_id = generate_id(16);

        // 1. Générer 32 octets aléatoires : la clé de chiffrement du fichier.
        let file_key = generate_id(32);
        let file_key_arr: [u8; 32] = file_key[..32].try_into().unwrap();

        // Les mots de passe sont tronqués à 127 octets (ISO 32000-2, §7.6.4.3).
        let user_pwd_bytes = &user_pwd_bytes[..user_pwd_bytes.len().min(127)];
        let owner_pwd_bytes = &owner_pwd_bytes[..owner_pwd_bytes.len().min(127)];

        // 2. Hash de validation + sels (algorithmes 2.A/2.B, PDF 2.0).
        let user_validation_salt = generate_id(8);
        let user_hash = hash_r6(user_pwd_bytes, &user_validation_salt, &[]);

        let user_key_salt = generate_id(8);
        let user_key = hash_r6(user_pwd_bytes, &user_key_salt, &[]);

        // 3. /U et /O = hash (32) + sel de validation (8) + sel de clé (8).
        let mut u_value = Vec::with_capacity(48);
        u_value.extend_from_slice(&user_hash);
        u_value.extend_from_slice(&user_validation_salt);
        u_value.extend_from_slice(&user_key_salt);
        debug_assert_eq!(u_value.len(), 48);

        // L'entrée du propriétaire est indexée sur /U ENTIER (48 octets),
        // pas sur les seuls 32 octets du hash.
        let owner_validation_salt = generate_id(8);
        let owner_hash = hash_r6(owner_pwd_bytes, &owner_validation_salt, &u_value);

        let owner_key_salt = generate_id(8);
        let owner_key = hash_r6(owner_pwd_bytes, &owner_key_salt, &u_value);

        let mut o_value = Vec::with_capacity(48);
        o_value.extend_from_slice(&owner_hash);
        o_value.extend_from_slice(&owner_validation_salt);
        o_value.extend_from_slice(&owner_key_salt);
        debug_assert_eq!(o_value.len(), 48);

        // 4. /UE et /OE : la clé du fichier, chiffrée AES-256-CBC sans
        // padding (IV nul), sous la clé dérivée des sels et mots de passe.
        let ue_value = aes_256_cbc_encrypt_no_padding(&user_key, &file_key_arr);
        let oe_value = aes_256_cbc_encrypt_no_padding(&owner_key, &file_key_arr);
        debug_assert_eq!(ue_value.len(), 32);
        debug_assert_eq!(oe_value.len(), 32);

        // 5. Chiffrer tous les objets avec AES-256-CBC.
        let permissions = opts.permissions.to_pdf_flags();
        encrypt_objects_aes256(self.inner_mut(), &file_key)?;

        // 6. Construire le dictionnaire /Encrypt.
        let perms_value = compute_perms(&file_key_arr, permissions, true);
        let encrypt_dict = build_encrypt_dict_aes(
            &o_value,
            &u_value,
            &ue_value,
            &oe_value,
            &perms_value,
            permissions,
        );
        let encrypt_id = self
            .inner_mut()
            .add_object(Object::Dictionary(encrypt_dict));

        let trailer = &mut self.inner_mut().trailer;
        trailer.set("Encrypt", Object::Reference(encrypt_id));
        trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(doc_id.clone(), lopdf::StringFormat::Hexadecimal),
                Object::String(doc_id, lopdf::StringFormat::Hexadecimal),
            ]),
        );

        tracing::info!("chiffrement AES-256 (révision 6) appliqué");
        Ok(())
    }
}

// ────────────────────────────────────────────────────────────────────────
// RC4 : dérivation
// ────────────────────────────────────────────────────────────────────────

fn pad_password(password: &str) -> [u8; 32] {
    let bytes = password.as_bytes();
    let mut out = [0u8; 32];
    let take = bytes.len().min(32);
    out[..take].copy_from_slice(&bytes[..take]);
    if take < 32 {
        out[take..].copy_from_slice(&PAD[..32 - take]);
    }
    out
}

fn r3_owner_entry(owner_pwd: &[u8; 32], user_pwd: &[u8; 32]) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(owner_pwd);
    let mut hash: [u8; 16] = hasher.finalize().into();
    for _ in 0..50 {
        let mut h = Md5::new();
        h.update(hash);
        hash = h.finalize().into();
    }
    let mut result = rc4(&hash, user_pwd);
    for i in 1..=19u8 {
        let mut k = hash;
        for b in k.iter_mut() {
            *b ^= i;
        }
        result = rc4(&k, &result);
    }
    result
}

fn r3_encryption_key(
    user_pwd: &[u8; 32],
    o_value: &[u8],
    permissions: i32,
    doc_id: &[u8],
) -> [u8; KEY_LEN_R3] {
    let mut hasher = Md5::new();
    hasher.update(user_pwd);
    hasher.update(o_value);
    hasher.update(permissions.to_le_bytes());
    hasher.update(doc_id);
    let mut hash: [u8; 16] = hasher.finalize().into();
    for _ in 0..50 {
        let mut h = Md5::new();
        h.update(&hash[..KEY_LEN_R3]);
        hash = h.finalize().into();
    }
    let mut key = [0u8; KEY_LEN_R3];
    key.copy_from_slice(&hash[..KEY_LEN_R3]);
    key
}

fn r3_user_entry(key: &[u8; KEY_LEN_R3], doc_id: &[u8]) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(PAD);
    hasher.update(doc_id);
    let hash: [u8; 16] = hasher.finalize().into();
    let mut result = rc4(key, &hash);
    for i in 1..=19u8 {
        let mut k = *key;
        for b in k.iter_mut() {
            *b ^= i;
        }
        result = rc4(&k, &result);
    }
    result.resize(32, 0);
    result
}

fn r3_object_key(base_key: &[u8; KEY_LEN_R3], obj_num: u32, gen_num: u16) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(base_key);
    hasher.update([
        (obj_num & 0xFF) as u8,
        ((obj_num >> 8) & 0xFF) as u8,
        ((obj_num >> 16) & 0xFF) as u8,
    ]);
    hasher.update([(gen_num & 0xFF) as u8, ((gen_num >> 8) & 0xFF) as u8]);
    let hash: [u8; 16] = hasher.finalize().into();
    let len = (KEY_LEN_R3 + 5).min(16);
    hash[..len].to_vec()
}

// ────────────────────────────────────────────────────────────────────────
// RC4 : application
// ────────────────────────────────────────────────────────────────────────

fn encrypt_objects_r3(doc: &mut LopdfDoc, key: &[u8; KEY_LEN_R3]) -> Result<()> {
    for id in encryption_target_ids(doc) {
        let obj_key = r3_object_key(key, id.0, id.1);
        if let Some(obj) = doc.objects.get_mut(&id) {
            encrypt_object_rc4(obj, &obj_key);
        }
    }
    encrypt_info_inline(doc, |obj| {
        // En RC4, l'en-tête d'objet utilise l'identifiant (0,0) pour un /Info inline.
        let obj_key = r3_object_key(key, 0, 0);
        encrypt_object_rc4(obj, &obj_key);
    });
    Ok(())
}

/// Objets à chiffrer : tout le catalogue, y compris `/Info` qui n'est pas
/// atteint depuis `/Root` et doit donc être ajouté explicitement.
///
///`doc.objects` est balayé **une seule fois** : lister les identifiants puis
/// rechiffrer `/Info` à part le chiffrait deux fois (RC4 étant symétrique,
/// le second passage annulait le premier et laissait le titre, l'auteur et
/// les dates en clair dans le fichier final).
fn encryption_target_ids(doc: &LopdfDoc) -> Vec<ObjectId> {
    let mut ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    if let Ok(info_ref) = doc.trailer.get(b"Info").and_then(|o| o.as_reference()) {
        if !ids.contains(&info_ref) {
            ids.push(info_ref);
        }
    }
    ids
}

/// Chiffre un `/Info` **direct** (dictionnaire inline dans le trailer),
/// cas qu'un balayage par `ObjectId` ne couvre pas.
fn encrypt_info_inline(doc: &mut LopdfDoc, mut enc: impl FnMut(&mut Object)) {
    if let Ok(Object::Dictionary(dict)) = doc.trailer.get_mut(b"Info") {
        for (_k, v) in dict.iter_mut() {
            enc(v);
        }
    }
}

fn encrypt_object_rc4(obj: &mut Object, key: &[u8]) {
    match obj {
        Object::String(bytes, _) => {
            *bytes = rc4(key, bytes);
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                encrypt_object_rc4(item, key);
            }
        }
        Object::Dictionary(dict) => {
            for (_k, v) in dict.iter_mut() {
                encrypt_object_rc4(v, key);
            }
        }
        Object::Stream(stream) => {
            for (_k, v) in stream.dict.iter_mut() {
                encrypt_object_rc4(v, key);
            }
            stream.content = rc4(key, &stream.content);
        }
        _ => {}
    }
}

fn build_encrypt_dict_r3(o_value: &[u8], u_value: &[u8], permissions: i32) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Filter", Object::Name(b"Standard".to_vec()));
    d.set("V", Object::Integer(2));
    d.set("R", Object::Integer(3));
    d.set("Length", Object::Integer(128));
    d.set("P", Object::Integer(permissions as i64));
    d.set(
        "O",
        Object::String(o_value.to_vec(), lopdf::StringFormat::Literal),
    );
    d.set(
        "U",
        Object::String(u_value.to_vec(), lopdf::StringFormat::Literal),
    );
    d
}

// ────────────────────────────────────────────────────────────────────────
// AES-256 : dérivation et durcissement (algorithme 2.B)
// ────────────────────────────────────────────────────────────────────────

/// Calcule le hash de mot de passe selon l'algorithme 2.B de la spec.
///
/// Cette fonction est **volontairement lente** : le durcissement
/// itératif jusqu'à au moins 64 tours de SHA-384/512 est conçu pour
/// ralentir les attaques par force brute.
fn hash_r6(password: &[u8], salt: &[u8], extra: &[u8]) -> [u8; 32] {
    // 1. K = SHA-256(password + salt + extra)
    let mut hasher = Sha256::new();
    hasher.update(password);
    hasher.update(salt);
    hasher.update(extra);
    let mut k: Vec<u8> = hasher.finalize().to_vec();

    // 2. Durcissement itératif (algorithme 2.B).
    let mut round = 0u32;
    let mut last_e_byte: u8 = 0;
    while round < 64 || last_e_byte as u32 > round - 32 {
        // K1 = (password + K + extra) répété 64 fois.
        let mut k1 = Vec::with_capacity((password.len() + k.len() + extra.len()) * 64);
        for _ in 0..64 {
            k1.extend_from_slice(password);
            k1.extend_from_slice(&k);
            k1.extend_from_slice(extra);
        }

        // E = AES-128-CBC(K[0..16], IV = K[16..32], K1) sans padding.
        let e = aes_128_cbc_encrypt_nopad(&k[..16], &k[16..32], &k1);

        // E mod 3 sur les 16 premiers octets -> taille du hash.
        let sum: u32 = e[..16].iter().map(|&b| b as u32).sum();
        let hash = match sum % 3 {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };

        k = hash;
        last_e_byte = *e.last().unwrap();
        round += 1;
    }

    // 3. Retourner les 32 premiers octets.
    let mut out = [0u8; 32];
    out.copy_from_slice(&k[..32]);
    out
}

/// AES-256-CBC **sans padding** (IV nul) : format exigé pour les entrées
/// /UE et /OE de la révision 6, où la clé de fichier fait exactement
/// 32 octets (deux blocs AES).
fn aes_256_cbc_encrypt_no_padding(key: &[u8; 32], data: &[u8]) -> Vec<u8> {
    use aes::cipher::{BlockEncryptMut, KeyIvInit};
    type Enc = cbc::Encryptor<aes::Aes256>;

    assert_eq!(data.len() % 16, 0, "données non alignées sur un bloc AES");
    let mut buf = data.to_vec();
    let data_len = buf.len();
    let zero_iv = [0u8; 16];
    Enc::new(key.into(), &zero_iv.into())
        .encrypt_padded_mut::<aes::cipher::block_padding::NoPadding>(&mut buf, data_len)
        .expect("chiffrement AES-256-CBC sans padding");
    buf
}


/// AES-128-CBC sans padding, IV fourni : utilisé par l'algorithme 2.B,
/// qui chiffre K1 avec les 128 **premiers** bits de K et un IV non nul.
fn aes_128_cbc_encrypt_nopad(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    type Aes128Enc = cbc::Encryptor<aes::Aes128>;
    assert_eq!(key.len(), 16);
    assert_eq!(iv.len(), 16);
    assert_eq!(data.len() % 16, 0, "K1 doit être un multiple du bloc AES");
    let mut buf = data.to_vec();
    let len = buf.len();
    Aes128Enc::new(key.into(), iv.into())
        .encrypt_padded_mut::<NoPadding>(&mut buf, len)
        .expect("chiffrement AES-128-CBC (algorithme 2.B)");
    buf
}

/// Calcule la valeur /Perms (algorithme 10, PDF 2.0) : 16 octets chiffrés
/// AES-256-CBC IV-nul sous la clé de fichier. Absent de ce dictionnaire,
/// plusieurs lecteurs refusent le document.
fn compute_perms(file_key: &[u8; 32], p_value: i32, encrypt_metadata: bool) -> Vec<u8> {
    let mut k = [0u8; 16];
    k[..4].copy_from_slice(&(p_value as u32).to_le_bytes());
    k[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    k[8] = if encrypt_metadata { b'T' } else { b'F' };
    k[9..12].copy_from_slice(b"adb");
    // Octets 13-16 : aléatoires.
    let rnd = generate_id(4);
    k[12..16].copy_from_slice(&rnd);
    aes_256_cbc_encrypt_no_padding(file_key, &k)
}

/// AES-256-CBC brut, avec padding PKCS#7.
fn aes_256_cbc_encrypt_raw(key: &[u8], iv: &[u8], data: &[u8]) -> Vec<u8> {
    // Padding PKCS#7 pour aligner sur 16 octets.
    let pad_len = 16 - (data.len() % 16);
    let pad_len = if pad_len == 0 { 16 } else { pad_len };
    let mut padded = data.to_vec();
    padded.extend(std::iter::repeat(pad_len as u8).take(pad_len));

    let mut key_arr = [0u8; 32];
    let key_arr_len = key.len().min(32);
    key_arr[..key_arr_len].copy_from_slice(&key[..key_arr_len]);

    let mut iv_arr = [0u8; 16];
    iv_arr.copy_from_slice(&iv[..16]);

    let encryptor = Encryptor::<Aes256>::new(&key_arr.into(), &iv_arr.into());
    encryptor.encrypt_padded_vec_mut::<NoPadding>(&padded)
}

/// AES-256-CBC brut (déchiffrement), avec retrait du padding PKCS#7.
#[allow(dead_code)]
fn aes_256_cbc_decrypt_raw(key: &[u8; 32], iv: &[u8; 16], data: &[u8]) -> Vec<u8> {
    let decryptor = Decryptor::<Aes256>::new(key.into(), iv.into());
    let mut buf = data.to_vec();
    match decryptor.decrypt_padded_mut::<NoPadding>(&mut buf) {
        Ok(plain) => {
            // Retrait du padding PKCS#7.
            if let Some(&last) = plain.last() {
                let pad = last as usize;
                if pad > 0 && pad <= 16 && plain.len() >= pad {
                    plain[..plain.len() - pad].to_vec()
                } else {
                    plain.to_vec()
                }
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    }
}

// ────────────────────────────────────────────────────────────────────────
// AES-256 : application aux objets
// ────────────────────────────────────────────────────────────────────────

fn encrypt_objects_aes256(doc: &mut LopdfDoc, file_key: &[u8]) -> Result<()> {
    for id in encryption_target_ids(doc) {
        if let Some(obj) = doc.objects.get_mut(&id) {
            encrypt_object_aes(obj, file_key);
        }
    }
    encrypt_info_inline(doc, |obj| encrypt_object_aes(obj, file_key));
    Ok(())
}

fn encrypt_object_aes(obj: &mut Object, key: &[u8]) {
    match obj {
        Object::String(bytes, _) => {
            *bytes = aes_cbc_iv_prefix(key, bytes);
        }
        Object::Array(arr) => {
            for item in arr.iter_mut() {
                encrypt_object_aes(item, key);
            }
        }
        Object::Dictionary(dict) => {
            for (_k, v) in dict.iter_mut() {
                encrypt_object_aes(v, key);
            }
        }
        Object::Stream(stream) => {
            for (_k, v) in stream.dict.iter_mut() {
                encrypt_object_aes(v, key);
            }
            stream.content = aes_cbc_iv_prefix(key, &stream.content);
            // `lopdf` écrit /Length tel quel : sans mise à jour, le lecteur
            // lit un tronçon trop court et le fichier devient illisible
            // (« Missing 'endstream' or incorrect stream length »).
            stream.dict.set("Length", Object::Integer(stream.content.len() as i64));
        }
        _ => {}
    }
}

/// Chiffre avec un IV aléatoire, préfixé aux données (format PDF AES).
fn aes_cbc_iv_prefix(key: &[u8], data: &[u8]) -> Vec<u8> {
    let iv = generate_id(16);
    let mut out = iv.clone();
    if key.len() < 32 {
        // Clé plus courte : on étend par zéro (ne devrait pas arriver).
        let mut k = [0u8; 32];
        k[..key.len()].copy_from_slice(key);
        out.extend(aes_256_cbc_encrypt_raw(&k, &iv, data));
    } else {
        out.extend(aes_256_cbc_encrypt_raw(&key[..32], &iv, data));
    }
    out
}

fn build_encrypt_dict_aes(
    o_value: &[u8],
    u_value: &[u8],
    ue_value: &[u8],
    oe_value: &[u8],
    perms_value: &[u8],
    permissions: i32,
) -> Dictionary {
    let mut d = Dictionary::new();
    d.set("Filter", Object::Name(b"Standard".to_vec()));
    d.set("V", Object::Integer(5));
    d.set("R", Object::Integer(6));
    d.set("Length", Object::Integer(256));
    d.set("P", Object::Integer(permissions as i64));
    d.set(
        "O",
        Object::String(o_value.to_vec(), lopdf::StringFormat::Literal),
    );
    d.set(
        "U",
        Object::String(u_value.to_vec(), lopdf::StringFormat::Literal),
    );
    d.set(
        "UE",
        Object::String(ue_value.to_vec(), lopdf::StringFormat::Literal),
    );
    d.set(
        "OE",
        Object::String(oe_value.to_vec(), lopdf::StringFormat::Literal),
    );
    // /Perms valide les droits annoncés ; son absence fait rejeter le
    // document par plusieurs lecteurs (dont Aperçu et Aperçu Quick Look).
    d.set(
        "Perms",
        Object::String(perms_value.to_vec(), lopdf::StringFormat::Literal),
    );
    // Crypt filter par défaut : AESV3.
    d.set(
        "CF",
        Object::Dictionary({
            let mut cf = Dictionary::new();
            let mut std_cf = Dictionary::new();
            std_cf.set("CFM", Object::Name(b"AESV3".to_vec()));
            std_cf.set("Length", Object::Integer(32));
            cf.set("StdCF", Object::Dictionary(std_cf));
            cf
        }),
    );
    d.set("StmF", Object::Name(b"StdCF".to_vec()));
    d.set("StrF", Object::Name(b"StdCF".to_vec()));
    d
}

// ────────────────────────────────────────────────────────────────────────
// RC4 brut
// ────────────────────────────────────────────────────────────────────────

fn rc4(key: &[u8], data: &[u8]) -> Vec<u8> {
    if key.is_empty() {
        return data.to_vec();
    }
    let mut s: [u8; 256] = [0; 256];
    for (i, byte) in s.iter_mut().enumerate() {
        *byte = i as u8;
    }
    let mut j: u8 = 0;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let mut i: u8 = 0;
    let mut j: u8 = 0;
    let mut out = Vec::with_capacity(data.len());
    for &byte in data {
        i = i.wrapping_add(1);
        j = j.wrapping_add(s[i as usize]);
        s.swap(i as usize, j as usize);
        let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
        out.push(byte ^ k);
    }
    out
}

// ────────────────────────────────────────────────────────────────────────
// Générateur d'identifiant / sel
// ────────────────────────────────────────────────────────────────────────

/// Génère `len` octets avec le CSPRNG du système.
///
/// Sécurité : pas de repli silencieux. Si l'entropie système est
/// indisponible, on panique plutôt que de produire des clés ou des sels
/// prévisibles.
fn generate_id(len: usize) -> Vec<u8> {
    let mut out = vec![0u8; len];
    getrandom::getrandom(&mut out)
        .expect("CSPRNG système indisponible : arrêt plutôt que des clés faibles");
    out
}
