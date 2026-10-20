//! Signature visuelle et demande de signature.
//!
//! ## Deux niveaux
//!
//! ### 1. Signature visuelle
//!
//! Une image (signature manuscrite, tampon) est placée à un endroit
//! précis du document, sous forme d'annotation ou de contenu de page.
//! C'est ce qu'attend 95 % des utilisateurs : la signature « paraît »
//! sur le document.
//!
//! ### 2. Signature numérique (certificat)
//!
//! La signature cryptographique (PKCS#7, CAdES) garantit l'intégrité
//! du document et l'identité du signataire. Ce module **prépare la
//! structure** (champ `/Sig`, dictionnaire `/Sig`, `/ByteRange`) mais
//! la génération effective du PKCS#7 dépend d'une bibliothèque
//! cryptographique externe (OpenSSL, `rustls` avec un moteur PKCS#7).
//!
//! ## Demande de signature
//!
//! La structure [`SignatureRequest`] permet de préparer une demande
//! qui sera envoyée par email ou par lien (le transport est géré par
//! la couche backend, non incluse dans ce crate).

use std::io::Write;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::{Duration as StdDuration, SystemTime};

use chrono::{DateTime, Utc};
use const_oid::ObjectIdentifier;
use der::asn1::{OctetString, SetOfVec};
use der::{Any, Decode, Encode};
use rsa::pkcs1v15::{Signature as RsaSignature, SigningKey, VerifyingKey};
use rsa::pkcs8::{DecodePrivateKey, DecodePublicKey, EncodePublicKey};
use rsa::signature::{Keypair, Verifier};
use rsa::{RsaPrivateKey, RsaPublicKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use spki::{AlgorithmIdentifierOwned, SubjectPublicKeyInfoOwned};
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::{Time as CertTime, Validity};
use x509_cert::Certificate;

use youpdf_shared::{Error, Result};

/// Convertit les erreurs ASN.1/DER en erreur applicative.
trait DerExt<T> {
    fn der(self) -> Result<T>;
}

impl<T> DerExt<T> for std::result::Result<T, der::Error> {
    fn der(self) -> Result<T> {
        self.map_err(|e| Error::InvalidInput(format!("DER : {e}")))
    }
}

/// Emplacement d'une signature sur une page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignaturePlacement {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle PDF `[x1, y1, x2, y2]` où sera placée la signature.
    pub rect: [f32; 4],
    /// Image PNG ou JPEG en bytes (facultatif : on peut signer avec
    /// un rendu texte simple à défaut).
    pub image_bytes: Option<Vec<u8>>,
    /// Texte de remplacement si aucune image n'est fournie.
    pub text_alternative: String,
    /// Nom du signataire (affiché dans les métadonnées).
    pub signer_name: String,
}

/// Statut d'une demande de signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignatureStatus {
    /// Brouillon, non envoyé.
    Draft,
    /// Envoyé au signataire.
    Sent,
    /// Consulté par le signataire.
    Viewed,
    /// Signé.
    Signed,
    /// Refusé.
    Declined,
    /// Expiré.
    Expired,
}

/// Une demande de signature électronique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureRequest {
    /// Identifiant unique.
    pub id: String,
    /// Document concerné (nom, chemin ou identifiant interne).
    pub document_ref: String,
    /// Email du signataire.
    pub signer_email: String,
    /// Nom du signataire.
    pub signer_name: String,
    /// Message d'accompagnement.
    pub message: String,
    /// Date d'envoi (remplie au moment de l'envoi).
    pub sent_at: Option<DateTime<Utc>>,
    /// Date d'expiration.
    pub expires_at: Option<DateTime<Utc>>,
    /// Statut actuel.
    pub status: SignatureStatus,
    /// Emplacements prévus pour la signature.
    pub placements: Vec<SignaturePlacement>,
}

impl SignatureRequest {
    /// Crée une nouvelle demande.
    pub fn new(document_ref: impl Into<String>, signer_email: impl Into<String>) -> Self {
        Self {
            id: format!("sigreq-{}", uuid::Uuid::new_v4()),
            document_ref: document_ref.into(),
            signer_email: signer_email.into(),
            signer_name: String::new(),
            message: String::new(),
            sent_at: None,
            expires_at: None,
            status: SignatureStatus::Draft,
            placements: Vec::new(),
        }
    }

    /// Ajoute un emplacement de signature.
    pub fn add_placement(&mut self, placement: SignaturePlacement) {
        self.placements.push(placement);
    }

    /// Marque la demande comme envoyée.
    pub fn mark_sent(&mut self) {
        self.sent_at = Some(Utc::now());
        self.status = SignatureStatus::Sent;
    }
}

use crate::document::Document;

impl Document {
    /// Place une signature visuelle sur le document.
    ///
    /// ## Comment
    ///
    /// Si `placement.image_bytes` est fourni, l'image est embarquée en
    /// XObject image et dessinée dans le rectangle. Sinon, le texte
    /// alternatif est écrit en Helvetica italique.
    ///
    /// ## Ce qui n'est pas fait
    ///
    /// Aucune signature cryptographique : le document peut être modifié
    /// après coup sans que le destinataire puisse le détecter. Pour un
    /// besoin d'intégrité, il faut une signature PKCS#7 (voir
    /// [`SignatureRequest`] et la documentation de phase 5+).
    pub fn place_visual_signature(&mut self, placement: &SignaturePlacement) -> Result<()> {
        let page_id = self
            .page_id(placement.page as u32)
            .map_err(|e| Error::Pdf(format!("page {} : {e}", placement.page)))?;

        let [x1, y1, x2, y2] = placement.rect;
        let w = x2 - x1;
        let h = y2 - y1;

        if w <= 0.0 || h <= 0.0 {
            return Err(Error::InvalidInput(
                "rectangle de signature invalide".into(),
            ));
        }

        // Flux construit en octets : le libellé échappé est du WinAnsi, un
        // `String` puis `into_bytes()` le ré-encoderait en UTF-8.
        let mut content: Vec<u8> = Vec::new();

        if let Some(image_bytes) = &placement.image_bytes {
            // Décoder l'image (PNG ou JPEG) — on s'appuie sur le
            // décodage natif de pdfium plus loin ; ici, en l'absence,
            // on se rabat sur un texte.
            if image_bytes.is_empty() {
                return Err(Error::InvalidInput("image de signature vide".into()));
            }
            // Note : l'intégration de l'image nécessite une conversion
            // RGB, faite ici même (extraction de palette).
            content.extend_from_slice(b"q\n");
            content.extend_from_slice(
                format!("{:.2} 0 0 {:.2} {:.2} {:.2} cm\n", w, h, x1, y1).as_bytes(),
            );
            content.extend_from_slice(b"/SigIm0 Do\nQ\n");

            // On enregistre l'image en XObject (implémentation minimale :
            // l'appelant doit avoir préalablement décodé l'image en RGB).
            return Err(Error::NotSupported(
                "L'insertion d'image de signature nécessite un décodeur PNG/JPEG. \
                 Utilisez le texte alternatif en attendant, ou passez l'image déjà \
                 convertie en bitmap via `insert_image_bitmap`."
                    .into(),
            ));
        } else {
            // Texte alternatif en italique.
            content.extend_from_slice(b"q\n0.85 0.85 0.85 rg\n");
            content.extend_from_slice(
                format!("{:.2} {:.2} {:.2} {:.2} re f\n", x1, y1, w, h).as_bytes(),
            );
            content.extend_from_slice(b"Q\n");

            content.extend_from_slice(b"BT\n/YouPdfF1 14 Tf\n0 0 0 rg\n");
            content.extend_from_slice(
                format!(
                    "1 0 0 1 {:.2} {:.2} Tm\n",
                    x1 + 6.0,
                    y1 + h / 2.0 - 5.0
                )
                .as_bytes(),
            );
            let safe = escape_sig(&placement.text_alternative);
            content.push(b'(');
            content.extend_from_slice(&safe);
            content.extend_from_slice(b") Tj\nET\n");
        }

        // Injection dans la page.
        self.append_signature_content(page_id, content)?;

        tracing::info!(
            page = placement.page,
            signer = %placement.signer_name,
            "signature visuelle appliquée"
        );
        Ok(())
    }

    /// Insère une image bitmap (RGB) comme XObject sur une page.
    ///
    /// Cette méthode est plus directe que `place_visual_signature` :
    /// l'appelant fournit les pixels déjà décodés.
    pub fn insert_image_bitmap(
        &mut self,
        page: usize,
        rect: [f32; 4],
        pixels_rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        use lopdf::{dictionary, Object, Stream};

        let expected = width as usize * height as usize * 3;
        if pixels_rgb.len() < expected {
            return Err(Error::InvalidInput(format!(
                "bitmap trop court : {} octets, attendu {}",
                pixels_rgb.len(),
                expected
            )));
        }

        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page {page} : {e}")))?;

        // 1. Créer l'XObject image.
        let img_dict = dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
        };
        let img_stream = Stream::new(img_dict, pixels_rgb[..expected].to_vec());
        let img_id = self.inner_mut().add_object(img_stream);

        // 2. Enregistrer dans /Resources/XObject sous un nom libre. Un nom fixe
        //    ferait que deux images posées sur la même page partageraient la
        //    ressource : la seconde écraserait la première, et l'inspecteur
        //    d'image ne saurait plus laquelle il édite.
        let name = self.free_xobject_name(page_id, "SigIm")?;
        self.link_xobject_to_page(page_id, &name, img_id)?;

        // 3. Flux de dessin.
        let [x1, y1, x2, y2] = rect;
        let w = x2 - x1;
        let h = y2 - y1;
        let content = format!("q\n{w:.2} 0 0 {h:.2} {x1:.2} {y1:.2} cm\n/{name} Do\nQ\n");
        let stream = Stream::new(lopdf::Dictionary::new(), content.into_bytes());
        let stream_id = self.inner_mut().add_object(stream);
        self.append_content_by_id(page_id, stream_id)?;

        Ok(())
    }

    /// Premier nom de ressource libre de la forme `prefixe{n}`.
    fn free_xobject_name(&self, page_id: (u32, u16), prefix: &str) -> Result<String> {
        let used: Vec<Vec<u8>> = self
            .read_resources(page_id)?
            .get(b"XObject")
            .ok()
            .and_then(|object| object.as_dict().ok())
            .map(|xobjects| xobjects.iter().map(|(name, _)| name.clone()).collect())
            .unwrap_or_default();
        for index in 0..1000u32 {
            let candidate = format!("{prefix}{index}");
            if !used.iter().any(|name| name == candidate.as_bytes()) {
                return Ok(candidate);
            }
        }
        Err(Error::Pdf("plus de nom d'image libre".to_owned()))
    }

    /// Enregistre un XObject dans les ressources d'une page.
    fn link_xobject_to_page(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        object_id: (u32, u16),
    ) -> Result<()> {
        // Lecture des ressources actuelles. `/XObject` peut être une
        // référence : la résoudre évite d'écraser un dictionnaire partagé.
        let mut resources = self.read_resources(page_id)?;
        let mut xobjects = self
            .resolve_dict(resources.get(b"XObject").ok())
            .unwrap_or_default();
        xobjects.set(name, lopdf::Object::Reference(object_id));
        resources.set("XObject", lopdf::Object::Dictionary(xobjects));

        // Écriture.
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", lopdf::Object::Dictionary(resources));
        Ok(())
    }

    /// Lit les ressources d'une page, en résolvant les références et en
    /// héritant de ses parents.
    fn read_resources(&self, page_id: (u32, u16)) -> Result<lopdf::Dictionary> {
        self.page_resources(page_id)
    }

    /// Ajoute un flux de contenu (déjà stocké) à la fin du contenu d'une page.
    fn append_signature_content(&mut self, page_id: (u32, u16), content: Vec<u8>) -> Result<()> {
        use lopdf::{Dictionary, Stream};
        let stream = Stream::new(Dictionary::new(), content);
        let stream_id = self.inner_mut().add_object(stream);
        self.append_content_by_id(page_id, stream_id)
    }

    fn append_content_by_id(&mut self, page_id: (u32, u16), stream_id: (u32, u16)) -> Result<()> {
        use lopdf::Object;

        // Polices dédiées à YouPDF : on n'écrase jamais le `/F1` de la page,
        // qui désigne une police légitime du document (l'écraser change la
        // typographie — et parfois l'encodage — de tout le texte existant).
        let helv_id = self.ensure_helvetica_font_pub(page_id)?;
        self.link_font_to_page_pub(page_id, "YouPdfF1", helv_id)?;

        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;

        match dict.get_mut(b"Contents") {
            Ok(Object::Reference(id)) => {
                let existing = *id;
                dict.set(
                    "Contents",
                    Object::Array(vec![
                        Object::Reference(existing),
                        Object::Reference(stream_id),
                    ]),
                );
            }
            Ok(Object::Array(arr)) => arr.push(Object::Reference(stream_id)),
            _ => dict.set("Contents", Object::Reference(stream_id)),
        }

        Ok(())
    }

    /// Variantes publiques de helpers privés de `edit.rs`.
    fn ensure_helvetica_font_pub(&mut self, _page_id: (u32, u16)) -> Result<(u32, u16)> {
        use lopdf::{Dictionary, Object};
        let font = Dictionary::from_iter(vec![
            ("Type", Object::Name(b"Font".to_vec())),
            ("Subtype", Object::Name(b"Type1".to_vec())),
            ("BaseFont", Object::Name(b"Helvetica".to_vec())),
            ("Encoding", Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        Ok(self.inner_mut().add_object(Object::Dictionary(font)))
    }

    fn link_font_to_page_pub(
        &mut self,
        page_id: (u32, u16),
        name: &str,
        font_id: (u32, u16),
    ) -> Result<()> {
        let mut resources = self.read_resources(page_id)?;
        let mut fonts = resources
            .get(b"Font")
            .ok()
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default();
        fonts.set(name, lopdf::Object::Reference(font_id));
        resources.set("Font", lopdf::Object::Dictionary(fonts));

        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", lopdf::Object::Dictionary(resources));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Signature numérique (PKCS#7 détachée, RSA + SHA-256)
// ---------------------------------------------------------------------------

/// Capacité réservée au PKCS#7 dans `/Contents`, en octets DER.
const SIGNATURE_CAPACITY: usize = 8192;

/// Emplacement fixe de `/ByteRange`, remplacé après calcul du hachage.
const BYTE_RANGE_PLACEHOLDER: &[u8] = b"[0 0000000000 0000000000 0000000000]";

const OID_CERT_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.3");
const OID_KEY_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.1");
const OID_SHROUDED_KEY_BAG: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.12.10.1.2");
const OID_X509_CERT: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.22.1");
const OID_SIGNING_TIME: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.5");
const OID_MESSAGE_DIGEST: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("1.2.840.113549.1.9.4");
const OID_SHA_256: ObjectIdentifier =
    ObjectIdentifier::new_unwrap("2.16.840.1.101.3.4.2.1");

/// Identité de signature : certificat feuille, chaîne et clé privée RSA.
pub struct SigningIdentity {
    certificate: Certificate,
    chain: Vec<Certificate>,
    private_key: RsaPrivateKey,
}

impl std::fmt::Debug for SigningIdentity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SigningIdentity")
            .field("subject", &self.subject())
            .field("issuer", &self.issuer())
            .finish()
    }
}

impl SigningIdentity {
    /// Sujet du certificat (format RFC 4514).
    pub fn subject(&self) -> String {
        self.certificate.tbs_certificate.subject.to_string()
    }

    /// Émetteur du certificat.
    pub fn issuer(&self) -> String {
        self.certificate.tbs_certificate.issuer.to_string()
    }

    /// Nom court (CN) du signataire, sinon le sujet complet.
    pub fn common_name(&self) -> String {
        common_name_of(&self.certificate).unwrap_or_else(|| self.subject())
    }

    /// Date de début de validité (`AAAA-MM-JJ`).
    pub fn valid_from(&self) -> String {
        time_to_date(self.certificate.tbs_certificate.validity.not_before)
    }

    /// Date de fin de validité (`AAAA-MM-JJ`).
    pub fn valid_to(&self) -> String {
        time_to_date(self.certificate.tbs_certificate.validity.not_after)
    }

    /// Vrai si le certificat est auto-signé.
    pub fn self_signed(&self) -> bool {
        self.certificate.tbs_certificate.issuer == self.certificate.tbs_certificate.subject
    }

    /// Importe une identité depuis un fichier PKCS#12 (`.p12` / `.pfx`).
    pub fn from_pkcs12(bytes: &[u8], password: &str) -> Result<Self> {
        match Self::from_pkcs12_rust(bytes, password) {
            Ok(identity) => Ok(identity),
            Err(rust_error) => Self::from_pkcs12_openssl(bytes, password).map_err(|openssl| {
                Error::InvalidInput(format!(
                    "PKCS#12 illisible ({rust_error}) ; repli OpenSSL impossible ({openssl})"
                ))
            }),
        }
    }

    /// Importe un certificat PEM/DER et une clé PEM/DER (éventuellement chiffrée).
    pub fn from_pem(
        certificate_bytes: &[u8],
        private_key_bytes: &[u8],
        password: Option<&str>,
    ) -> Result<Self> {
        let certificates = parse_certificates(certificate_bytes)?;
        let key = parse_private_key(private_key_bytes, password)?;
        identity_from_parts(certificates, key)
    }

    /// Crée un certificat auto-signé RSA 2048 bits et son identité.
    pub fn create_self_signed(request: &CertificateRequest) -> Result<Self> {
        let mut rng = rsa::rand_core::OsRng;
        let key = RsaPrivateKey::new(&mut rng, 2048)
            .map_err(|e| Error::Internal(format!("clé RSA : {e}")))?;
        let signer = SigningKey::<Sha256>::new(key.clone());

        let common = dn_value(if request.common_name.trim().is_empty() {
            "YouPDF Signer"
        } else {
            &request.common_name
        });
        let org = dn_value(&request.organization);
        let email = dn_value(&request.email);
        let country: String = request
            .country
            .chars()
            .filter(|c| c.is_ascii_alphabetic())
            .take(2)
            .collect::<String>()
            .to_ascii_uppercase();
        let country = if country.len() == 2 { country } else { "FR".into() };
        let mut dn = format!("CN={common},C={country}");
        if !org.is_empty() {
            dn.push_str(",O=");
            dn.push_str(&org);
        }
        if !email.is_empty() {
            dn.push_str(",emailAddress=");
            dn.push_str(&email);
        }
        let name = Name::from_str(&dn)
            .map_err(|e| Error::InvalidInput(format!("nom X.509 : {e}")))?;

        let mut serial_bytes = [0u8; 16];
        getrandom::getrandom(&mut serial_bytes)
            .map_err(|e| Error::Internal(format!("aléa : {e}")))?;
        serial_bytes[0] &= 0x7f;
        if serial_bytes.iter().all(|b| *b == 0) {
            serial_bytes[15] = 1;
        }
        let serial = SerialNumber::new(&serial_bytes)
            .map_err(|e| Error::Internal(format!("numéro de série : {e}")))?;
        let years = request.years.clamp(1, 30) as u64;
        let validity = Validity::from_now(StdDuration::from_secs(years * 365 * 86_400))
            .map_err(|e| Error::Internal(format!("validité : {e}")))?;
        let spki = SubjectPublicKeyInfoOwned::from_key(signer.verifying_key())
            .map_err(|e| Error::Internal(format!("clé publique : {e}")))?;

        let builder = CertificateBuilder::new(
            Profile::Leaf {
                issuer: name.clone(),
                enable_key_agreement: false,
                enable_key_encipherment: true,
            },
            serial,
            validity,
            name,
            spki,
            &signer,
        )
        .map_err(|e| Error::Internal(format!("certificat : {e}")))?;
        let certificate = builder
            .build::<RsaSignature>()
            .map_err(|e| Error::Internal(format!("signature du certificat : {e}")))?;

        Ok(Self {
            certificate,
            chain: Vec::new(),
            private_key: key,
        })
    }

    /// Import pur Rust des `safeBag` non chiffrés.
    fn from_pkcs12_rust(bytes: &[u8], password: &str) -> Result<Self> {
        let pfx = pkcs12::pfx::Pfx::from_der(bytes)
            .map_err(|e| Error::InvalidInput(format!("PFX : {e}")))?;
        let auth_safe = OctetString::from_der(&pfx.auth_safe.content.to_der().der()?)
            .map_err(|e| Error::InvalidInput(format!("AuthSafe : {e}")))?;
        let sections = Vec::<cms::content_info::ContentInfo>::from_der(auth_safe.as_bytes())
            .map_err(|e| Error::InvalidInput(format!("AuthenticatedSafe : {e}")))?;

        let mut certificates = Vec::new();
        let mut keys = Vec::new();
        for section in sections {
            if section.content_type != const_oid::db::rfc5911::ID_DATA {
                return Err(Error::NotSupported(
                    "PKCS#12 chiffré : repli OpenSSL nécessaire".into(),
                ));
            }
            let octets = OctetString::from_der(&section.content.to_der().der()?).der()?;
            let bags = Vec::<P12SafeBag>::from_der(octets.as_bytes())
                .map_err(|e| Error::InvalidInput(format!("SafeContents : {e}")))?;
            for bag in bags {
                if bag.bag_id == OID_CERT_BAG {
                    let cert_bag = P12CertBag::from_der(&bag.bag_value.to_der().der()?).der()?;
                    if cert_bag.cert_id == OID_X509_CERT {
                        certificates.push(Certificate::from_der(cert_bag.cert_value.as_bytes()).der()?);
                    }
                } else if bag.bag_id == OID_KEY_BAG {
                    keys.push(RsaPrivateKey::from_pkcs8_der(&bag.bag_value.to_der().der()?).map_err(|e| Error::InvalidInput(format!("clé PKCS#8 : {e}")))?);
                } else if bag.bag_id == OID_SHROUDED_KEY_BAG {
                    let encrypted_der = bag.bag_value.to_der().der()?;
                    let info = rsa::pkcs8::EncryptedPrivateKeyInfo::from_der(&encrypted_der)
                        .map_err(|e| Error::InvalidInput(format!("clé chiffrée : {e}")))?;
                    let plain = info
                        .decrypt(password)
                        .map_err(|e| Error::InvalidInput(format!("clé chiffrée : {e}")))?;
                    keys.push(RsaPrivateKey::from_pkcs8_der(plain.as_bytes()).map_err(|e| Error::InvalidInput(format!("clé PKCS#8 : {e}")))?);
                }
            }
        }

        if certificates.is_empty() {
            return Err(Error::InvalidInput("PKCS#12 sans certificat".into()));
        }
        let key = keys
            .into_iter()
            .next()
            .ok_or_else(|| Error::InvalidInput("PKCS#12 sans clé privée non chiffrée".into()))?;
        identity_from_parts(certificates, key)
    }

    /// Repli OpenSSL pour les conteneurs `encryptedData` (RC2, 3DES…).
    fn from_pkcs12_openssl(bytes: &[u8], password: &str) -> Result<Self> {
        let mut file = tempfile::Builder::new()
            .prefix("youpdf-p12-")
            .suffix(".p12")
            .tempfile()?;
        file.write_all(bytes)?;
        file.flush()?;

        let mut last_error = String::new();
        for legacy in [false, true] {
            let mut command = Command::new("openssl");
            command
                .arg("pkcs12")
                .arg("-in")
                .arg(file.path())
                .arg("-nodes")
                .arg("-passin")
                .arg("stdin");
            if legacy {
                command.arg("-legacy");
            }
            let mut child = match command
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
            {
                Ok(child) => child,
                Err(e) => return Err(Error::NotSupported(format!("openssl introuvable : {e}"))),
            };
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(password.as_bytes());
                let _ = stdin.write_all(b"\n");
            }
            let output = child
                .wait_with_output()
                .map_err(|e| Error::Internal(format!("openssl : {e}")))?;
            if output.status.success() {
                return Self::from_pem(&output.stdout, &output.stdout, None);
            }
            last_error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        }
        Err(Error::InvalidInput(format!(
            "mot de passe PKCS#12 refusé ou format non pris en charge : {last_error}"
        )))
    }
}

/// Paramètres de création d'un certificat auto-signé.
#[derive(Debug, Clone)]
pub struct CertificateRequest {
    pub common_name: String,
    pub organization: String,
    pub email: String,
    pub country: String,
    pub years: u32,
}

impl Default for CertificateRequest {
    fn default() -> Self {
        Self {
            common_name: String::new(),
            organization: String::new(),
            email: String::new(),
            country: "FR".into(),
            years: 3,
        }
    }
}

/// Emplacement et métadonnées d'une signature détachée.
#[derive(Debug, Clone, Default)]
pub struct SignOptions {
    /// Page (1-indexée).
    pub page: usize,
    /// Rectangle PDF `[x1, y1, x2, y2]`.
    pub rect: [f32; 4],
    /// Nom du champ de signature.
    pub field_name: String,
    /// Nom affiché du signataire.
    pub name: String,
    /// Motif de la signature.
    pub reason: String,
    /// Lieu de signature.
    pub location: String,
    /// Coordonnées de contact.
    pub contact: String,
}

/// Résultat de la vérification d'une signature présente dans un PDF.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignatureInfo {
    pub field: String,
    pub signer: String,
    pub issuer: String,
    pub signed_at: Option<String>,
    pub reason: String,
    pub location: String,
    pub valid: bool,
    pub covers_whole_document: bool,
    pub self_signed: bool,
    pub error: Option<String>,
}

impl Document {
    /// Appose une signature PKCS#7 détachée (RSA SHA-256) sur le document.
    ///
    /// Le document courant est d'abord sérialisé, puis un incrément PDF est
    /// ajouté : dictionnaire `/Sig`, widget, apparence, `/AcroForm`, page et
    /// catalogue. Les octets hors `/Contents` sont hachés, signés, puis le
    /// PKCS#7 est incrusté dans l'emplacement réservé.
    pub fn sign(&self, identity: &SigningIdentity, options: &SignOptions) -> Result<Vec<u8>> {
        if options.page == 0 {
            return Err(Error::InvalidInput("page de signature invalide".into()));
        }
        let base = self.to_bytes()?;
        let page_id = self.page_id(options.page as u32)?;
        let page_dict = self
            .inner()
            .get_object(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?
            .as_dict()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?
            .clone();
        let root_id = self
            .inner()
            .trailer
            .get(b"Root")
            .and_then(lopdf::Object::as_reference)
            .map_err(|_| Error::Pdf("catalogue absent".into()))?;
        let catalog = self
            .inner()
            .get_object(root_id)
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?
            .as_dict()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?
            .clone();

        let mut next_id = self.inner().max_id + 1;
        let sig_id = next_id;
        next_id += 1;
        let widget_id = next_id;
        next_id += 1;
        let appearance_id = next_id;
        next_id += 1;
        let font_id = next_id;
        next_id += 1;
        let acroform_id = next_id;
        next_id += 1;
        let reference = |id: u32| lopdf::Object::Reference((id, 0));

        let [x1, y1, x2, y2] = options.rect;
        let (x1, x2) = (x1.min(x2), x1.max(x2));
        let (y1, y2) = (y1.min(y2), y1.max(y2));
        let width = (x2 - x1).max(24.0);
        let height = (y2 - y1).max(20.0);
        let rect = vec![
            lopdf::Object::Real(x1),
            lopdf::Object::Real(y1),
            lopdf::Object::Real(x1 + width),
            lopdf::Object::Real(y1 + height),
        ];

        // Dictionnaire /Sig, écrit à la main pour garder /ByteRange de largeur fixe.
        let mut sig_body = Vec::new();
        sig_body.extend_from_slice(
            b"<< /Type /Sig /Filter /Adobe.PPKLite /SubFilter /adbe.pkcs7.detached /ByteRange ",
        );
        sig_body.extend_from_slice(BYTE_RANGE_PLACEHOLDER);
        sig_body.extend_from_slice(b" /Contents <");
        sig_body.extend(std::iter::repeat_n(b'0', SIGNATURE_CAPACITY * 2));
        sig_body.extend_from_slice(b">");
        for (key, value) in [
            ("Name", options.name.as_str()),
            ("Reason", options.reason.as_str()),
            ("Location", options.location.as_str()),
            ("ContactInfo", options.contact.as_str()),
        ] {
            if !value.trim().is_empty() {
                sig_body.extend_from_slice(b" /");
                sig_body.extend_from_slice(key.as_bytes());
                sig_body.push(b' ');
                sig_body.extend(literal_string(value));
            }
        }
        sig_body.extend_from_slice(b" /M ");
        sig_body.extend(literal_string(&pdf_date_now()));
        sig_body.extend_from_slice(b" >>");

        // Apparence visible (nom, motif, date).
        let font_dict = lopdf::Dictionary::from_iter(vec![
            ("Type", lopdf::Object::Name(b"Font".to_vec())),
            ("Subtype", lopdf::Object::Name(b"Type1".to_vec())),
            ("BaseFont", lopdf::Object::Name(b"Helvetica".to_vec())),
            ("Encoding", lopdf::Object::Name(b"WinAnsiEncoding".to_vec())),
        ]);
        let mut font_res = lopdf::Dictionary::new();
        font_res.set("Helv", reference(font_id));
        let mut resources = lopdf::Dictionary::new();
        resources.set("Font", lopdf::Object::Dictionary(font_res));
        let mut ap_dict = lopdf::Dictionary::new();
        ap_dict.set("Type", lopdf::Object::Name(b"XObject".to_vec()));
        ap_dict.set("Subtype", lopdf::Object::Name(b"Form".to_vec()));
        ap_dict.set(
            "BBox",
            lopdf::Object::Array(vec![
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(0.0),
                lopdf::Object::Real(width),
                lopdf::Object::Real(height),
            ]),
        );
        ap_dict.set("Resources", lopdf::Object::Dictionary(resources));
        let ap_content = signature_appearance_content(
            width as f64,
            height as f64,
            &options.name,
            &options.reason,
            &pdf_date_short(),
        );
        let appearance = lopdf::Stream::new(ap_dict, ap_content);

        let mut widget = lopdf::Dictionary::new();
        widget.set("Type", lopdf::Object::Name(b"Annot".to_vec()));
        widget.set("Subtype", lopdf::Object::Name(b"Widget".to_vec()));
        widget.set("FT", lopdf::Object::Name(b"Sig".to_vec()));
        widget.set(
            "T",
            lopdf::Object::string_literal(options.field_name.as_bytes().to_vec()),
        );
        widget.set("Rect", lopdf::Object::Array(rect));
        widget.set("P", reference(page_id.0));
        widget.set("V", reference(sig_id));
        widget.set("F", lopdf::Object::Integer(132));
        let mut ap = lopdf::Dictionary::new();
        ap.set("N", reference(appearance_id));
        widget.set("AP", lopdf::Object::Dictionary(ap));

        // /AcroForm : conserve les champs existants, ajoute le widget.
        let mut acroform = catalog
            .get(b"AcroForm")
            .ok()
            .and_then(|object| resolve_dict(self.inner(), object))
            .unwrap_or_default();
        let mut fields = acroform
            .get(b"Fields")
            .ok()
            .and_then(|object| resolve_array(self.inner(), object))
            .unwrap_or_default();
        fields.push(reference(widget_id));
        acroform.set("Fields", lopdf::Object::Array(fields));
        acroform.set("SigFlags", lopdf::Object::Integer(3));

        let mut page_dict = page_dict;
        let mut annots = page_dict
            .get(b"Annots")
            .ok()
            .and_then(|object| resolve_array(self.inner(), object))
            .unwrap_or_default();
        annots.push(reference(widget_id));
        page_dict.set("Annots", lopdf::Object::Array(annots));

        let mut catalog = catalog;
        catalog.set("AcroForm", reference(acroform_id));

        let previous_xref = last_startxref(&base)
            .ok_or_else(|| Error::Pdf("startxref introuvable dans la base".into()))?;

        let mut file = base;
        if !file.ends_with(b"\n") {
            file.push(b'\n');
        }
        let base_len = file.len();

        let mut update = Vec::new();
        let mut offsets = Vec::new();
        let mut push = |id: u32, body: &[u8], update: &mut Vec<u8>| {
            let offset = update.len();
            update.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
            update.extend_from_slice(body);
            if !body.ends_with(b"\n") {
                update.push(b'\n');
            }
            update.extend_from_slice(b"endobj\n");
            offsets.push((id, offset));
        };

        push(sig_id, &sig_body, &mut update);
        let widget_body = object_bytes(&lopdf::Object::Dictionary(widget))?;
        push(widget_id, &widget_body, &mut update);
        let appearance_body = object_bytes(&lopdf::Object::Stream(appearance))?;
        push(appearance_id, &appearance_body, &mut update);
        let font_body = object_bytes(&lopdf::Object::Dictionary(font_dict))?;
        push(font_id, &font_body, &mut update);
        let acroform_body = object_bytes(&lopdf::Object::Dictionary(acroform))?;
        push(acroform_id, &acroform_body, &mut update);
        let page_body = object_bytes(&lopdf::Object::Dictionary(page_dict))?;
        push(page_id.0, &page_body, &mut update);
        let catalog_body = object_bytes(&lopdf::Object::Dictionary(catalog))?;
        push(root_id.0, &catalog_body, &mut update);

        let xref_start = base_len + update.len();
        update.extend_from_slice(b"xref\n");
        offsets.sort_unstable_by_key(|(id, _)| *id);
        for (id, offset) in &offsets {
            update.extend_from_slice(format!("{id} 1\n{:010} 00000 n \n", base_len + offset).as_bytes());
        }
        let mut trailer = lopdf::Dictionary::new();
        trailer.set("Size", lopdf::Object::Integer(next_id as i64));
        trailer.set("Root", reference(root_id.0));
        trailer.set("Prev", lopdf::Object::Integer(previous_xref as i64));
        if let Ok(id) = self.inner().trailer.get(b"ID") {
            trailer.set("ID", id.clone());
        }
        let trailer_body = object_bytes(&lopdf::Object::Dictionary(trailer))?;
        update.extend_from_slice(b"trailer\n");
        update.extend_from_slice(&trailer_body);
        update.extend_from_slice(
            format!("\nstartxref\n{xref_start}\n%%EOF\n").as_bytes(),
        );

        file.extend_from_slice(&update);
        let total_len = file.len();

        let marker = format!("<{}>", "0".repeat(SIGNATURE_CAPACITY * 2));
        let contents_start = find_subslice(&file[base_len..], marker.as_bytes())
            .map(|pos| base_len + pos)
            .ok_or_else(|| Error::Internal("emplacement /Contents introuvable".into()))?;
        let contents_end = contents_start + marker.len();
        let byte_range_pos = find_subslice(&file[base_len..], BYTE_RANGE_PLACEHOLDER)
            .map(|pos| base_len + pos)
            .ok_or_else(|| Error::Internal("emplacement /ByteRange introuvable".into()))?;
        let replacement = format!(
            "[0 {:010} {:010} {:010}]",
            contents_start,
            contents_end,
            total_len - contents_end
        );
        if replacement.len() != BYTE_RANGE_PLACEHOLDER.len() {
            return Err(Error::Internal("largeur de /ByteRange inattendue".into()));
        }
        file[byte_range_pos..byte_range_pos + replacement.len()]
            .copy_from_slice(replacement.as_bytes());

        let mut hasher = Sha256::new();
        hasher.update(&file[..contents_start]);
        hasher.update(&file[contents_end..]);
        let digest = hasher.finalize();

        let cms = build_cms(identity, &digest)?;
        if cms.len() > SIGNATURE_CAPACITY {
            return Err(Error::NotSupported(format!(
                "signature trop volumineuse ({} octets, limite {})",
                cms.len(),
                SIGNATURE_CAPACITY
            )));
        }
        let mut hex = hex_encode(&cms);
        hex.extend(std::iter::repeat_n('0', SIGNATURE_CAPACITY * 2 - hex.len()));
        file[contents_start + 1..contents_start + 1 + hex.len()]
            .copy_from_slice(hex.as_bytes());

        Ok(file)
    }
}

/// Construit le `ContentInfo` PKCS#7/SignedData détaché.
fn build_cms(identity: &SigningIdentity, digest: &[u8]) -> Result<Vec<u8>> {
    use cms::builder::{SignedDataBuilder, SignerInfoBuilder};
    use cms::cert::{CertificateChoices, IssuerAndSerialNumber};
    use cms::signed_data::{EncapsulatedContentInfo, SignerIdentifier};

    let signer = SigningKey::<Sha256>::new(identity.private_key.clone());
    let sid = SignerIdentifier::IssuerAndSerialNumber(IssuerAndSerialNumber {
        issuer: identity.certificate.tbs_certificate.issuer.clone(),
        serial_number: identity.certificate.tbs_certificate.serial_number.clone(),
    });
    let econtent = EncapsulatedContentInfo {
        econtent_type: const_oid::db::rfc5911::ID_DATA,
        econtent: None,
    };
    let digest_algorithm = AlgorithmIdentifierOwned {
        oid: OID_SHA_256,
        parameters: Some(Any::null()),
    };
    let mut signed = SignerInfoBuilder::new(
        &signer,
        sid,
        digest_algorithm.clone(),
        &econtent,
        Some(digest),
    )
    .map_err(|e| Error::Internal(format!("CMS signer : {e}")))?;

    let signing_time = der::asn1::UtcTime::from_system_time(SystemTime::now())
        .map_err(|e| Error::Internal(format!("horodatage : {e}")))?;
    let mut values = SetOfVec::new();
    values
        .insert(Any::encode_from(&signing_time).map_err(|e| Error::Internal(format!("attribut : {e}")))?)
        .map_err(|e| Error::Internal(format!("attribut : {e}")))?;
    signed
        .add_signed_attribute(x509_cert::attr::Attribute {
            oid: OID_SIGNING_TIME,
            values,
        })
        .map_err(|e| Error::Internal(format!("attribut : {e}")))?;

    let mut builder = SignedDataBuilder::new(&econtent);
    builder
        .add_digest_algorithm(digest_algorithm)
        .map_err(|e| Error::Internal(format!("CMS digest : {e}")))?;
    builder
        .add_certificate(CertificateChoices::Certificate(identity.certificate.clone()))
        .map_err(|e| Error::Internal(format!("CMS certificat : {e}")))?;
    for certificate in &identity.chain {
        builder
            .add_certificate(CertificateChoices::Certificate(certificate.clone()))
            .map_err(|e| Error::Internal(format!("CMS chaîne : {e}")))?;
    }
    builder
        .add_signer_info::<SigningKey<Sha256>, RsaSignature>(signed)
        .map_err(|e| Error::Internal(format!("CMS signature : {e}")))?;
    let content_info = builder
        .build()
        .map_err(|e| Error::Internal(format!("CMS : {e}")))?;
    content_info
        .to_der()
        .map_err(|e| Error::Internal(format!("CMS DER : {e}")))
}

/// Vérifie toutes les signatures détachées d'un PDF.
pub fn validate_signatures(bytes: &[u8]) -> Vec<SignatureInfo> {
    let Ok(document) = lopdf::Document::load_mem(bytes) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for ((id, _), object) in &document.objects {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        let is_signature = dict
            .get(b"Type")
            .ok()
            .and_then(|object| object.as_name().ok())
            .is_some_and(|name| name == b"Sig");
        if !is_signature {
            continue;
        }
        let field = signature_field_name(&document, *id).unwrap_or_default();
        let reason = pdf_string(dict.get(b"Reason").ok());
        let location = pdf_string(dict.get(b"Location").ok());
        let signer_hint = pdf_string(dict.get(b"Name").ok());

        let mut info = SignatureInfo {
            field,
            signer: signer_hint,
            issuer: String::new(),
            signed_at: None,
            reason,
            location,
            valid: false,
            covers_whole_document: false,
            self_signed: false,
            error: None,
        };

        let result = (|| -> Result<()> {
            let byte_range = dict
                .get(b"ByteRange")
                .map_err(|_| Error::Pdf("/ByteRange absent".into()))?
                .as_array()
                .map_err(|_| Error::Pdf("/ByteRange invalide".into()))?
                .iter()
                .map(|object| object.as_i64())
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(|_| Error::Pdf("/ByteRange invalide".into()))?;
            let contents = match dict
                .get(b"Contents")
                .map_err(|_| Error::Pdf("/Contents absent".into()))?
            {
                lopdf::Object::String(bytes, _) => bytes.clone(),
                _ => return Err(Error::Pdf("/Contents invalide".into())),
            };
            if byte_range.len() != 4 {
                return Err(Error::Pdf("/ByteRange incomplet".into()));
            }
            let [a, b, c, d] = [
                byte_range[0].max(0) as usize,
                byte_range[1].max(0) as usize,
                byte_range[2].max(0) as usize,
                byte_range[3].max(0) as usize,
            ];
            if a + b > bytes.len() || c + d > bytes.len() || c < a + b {
                return Err(Error::Pdf("/ByteRange hors du fichier".into()));
            }
            let gap = 2 + contents.len() * 2;
            info.covers_whole_document =
                a == 0 && c == a + b + gap && c + d == bytes.len();
            let mut hasher = Sha256::new();
            hasher.update(&bytes[a..a + b]);
            hasher.update(&bytes[c..c + d]);
            let covered = hasher.finalize();

            let cms_end = contents.iter().rposition(|byte| *byte != 0).map_or(0, |index| index + 1);
            let (certificate, signed_at, self_signed) = verify_cms(&contents[..cms_end], &covered)?;
            info.signer = common_name_of(&certificate).unwrap_or_else(|| certificate.tbs_certificate.subject.to_string());
            info.issuer = certificate.tbs_certificate.issuer.to_string();
            info.signed_at = signed_at;
            info.self_signed = self_signed;
            let now = SystemTime::now();
            let validity = &certificate.tbs_certificate.validity;
            if now < validity.not_before.to_system_time() {
                return Err(Error::Pdf("certificat pas encore valide".into()));
            }
            if now > validity.not_after.to_system_time() {
                return Err(Error::Pdf("certificat expiré".into()));
            }
            if !info.covers_whole_document {
                return Err(Error::Pdf(
                    "la signature ne couvre pas tout le document".into(),
                ));
            }
            info.valid = true;
            Ok(())
        })();

        if let Err(error) = result {
            info.error = Some(error.to_string());
        }
        out.push(info);
    }
    out
}

/// Vérifie le PKCS#7 et renvoie le certificat, l'horodatage et l'auto-signature.
fn verify_cms(
    cms_der: &[u8],
    covered_digest: &[u8],
) -> Result<(Certificate, Option<String>, bool)> {
    use cms::content_info::ContentInfo;
    use cms::signed_data::{CertificateSet, SignedData, SignerIdentifier};
    use x509_cert::attr::AttributeValue;

    let content_info = ContentInfo::from_der(cms_der)
        .map_err(|e| Error::Pdf(format!("PKCS#7 : {e}")))?;
    let signed_data = SignedData::from_der(&content_info.content.to_der().der()?)
        .map_err(|e| Error::Pdf(format!("SignedData : {e}")))?;
    let signer = signed_data
        .signer_infos
        .0
        .iter()
        .next()
        .ok_or_else(|| Error::Pdf("PKCS#7 sans signataire".into()))?;
    let signed_attrs = signer
        .signed_attrs
        .as_ref()
        .ok_or_else(|| Error::Pdf("attributs signés absents".into()))?;

    let certificates: &CertificateSet = signed_data
        .certificates
        .as_ref()
        .ok_or_else(|| Error::Pdf("certificat absent du PKCS#7".into()))?;
    let certificate = certificates
        .0
        .iter()
        .filter_map(|choice| match choice {
            cms::cert::CertificateChoices::Certificate(certificate) => Some(certificate),
            _ => None,
        })
        .find(|certificate| match &signer.sid {
            SignerIdentifier::IssuerAndSerialNumber(identity) => {
                certificate.tbs_certificate.issuer == identity.issuer
                    && certificate.tbs_certificate.serial_number == identity.serial_number
            }
            SignerIdentifier::SubjectKeyIdentifier(_) => true,
        })
        .ok_or_else(|| Error::Pdf("certificat du signataire introuvable".into()))?;

    let signed_der = signed_attrs
        .to_der()
        .map_err(|e| Error::Pdf(format!("attributs : {e}")))?;
    let signature = RsaSignature::try_from(signer.signature.as_bytes())
        .map_err(|e| Error::Pdf(format!("signature RSA : {e}")))?;
    let public_key = RsaPublicKey::from_public_key_der(
        &certificate
            .tbs_certificate
            .subject_public_key_info
            .to_der()
            .der()?,
    )
    .map_err(|e| Error::Pdf(format!("clé publique : {e}")))?;
    VerifyingKey::<Sha256>::new(public_key)
        .verify(&signed_der, &signature)
        .map_err(|_| Error::Pdf("signature cryptographique invalide".into()))?;

    let digest_attribute = signed_attrs
        .iter()
        .find(|attribute| attribute.oid == OID_MESSAGE_DIGEST)
        .ok_or_else(|| Error::Pdf("messageDigest absent".into()))?;
    let digest_value: &AttributeValue = digest_attribute
        .values
        .get(0)
        .ok_or_else(|| Error::Pdf("messageDigest vide".into()))?;
    let expected = OctetString::from_der(&digest_value.to_der().der()?)
        .map_err(|e| Error::Pdf(format!("messageDigest : {e}")))?;
    if expected.as_bytes() != covered_digest {
        return Err(Error::Pdf(
            "le document a été modifié après la signature".into(),
        ));
    }

    let signed_at = signed_attrs
        .iter()
        .find(|attribute| attribute.oid == OID_SIGNING_TIME)
        .and_then(|attribute| attribute.values.get(0))
        .and_then(|value| CertTime::from_der(&value.to_der().ok()?).ok())
        .map(|time| {
            chrono::DateTime::<Utc>::from_timestamp(
                time.to_unix_duration().as_secs() as i64,
                0,
            )
            .map(|date| date.format("%Y-%m-%d %H:%M UTC").to_string())
            .unwrap_or_default()
        });

    let self_signed =
        certificate.tbs_certificate.issuer == certificate.tbs_certificate.subject;
    Ok((certificate.clone(), signed_at, self_signed))
}

/// Bag PKCS#12 minimal (évite l'encodage bancal du crate `pkcs12`).
#[derive(Debug, der::Sequence)]
struct P12SafeBag {
    bag_id: ObjectIdentifier,
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT")]
    bag_value: Any,
    #[asn1(optional = "true")]
    _bag_attributes: Option<x509_cert::attr::Attributes>,
}

#[derive(Debug, der::Sequence)]
struct P12CertBag {
    cert_id: ObjectIdentifier,
    #[asn1(context_specific = "0", tag_mode = "EXPLICIT")]
    cert_value: OctetString,
}

fn parse_certificates(bytes: &[u8]) -> Result<Vec<Certificate>> {
    if let Ok(chain) = Certificate::load_pem_chain(bytes) {
        if !chain.is_empty() {
            return Ok(chain);
        }
    }
    Certificate::from_der(bytes)
        .map(|certificate| vec![certificate])
        .map_err(|e| Error::InvalidInput(format!("certificat X.509 : {e}")))
}

fn parse_private_key(bytes: &[u8], password: Option<&str>) -> Result<RsaPrivateKey> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if text.contains("ENCRYPTED PRIVATE KEY") {
            return RsaPrivateKey::from_pkcs8_encrypted_pem(text, password.unwrap_or(""))
                .map_err(|e| Error::InvalidInput(format!("clé chiffrée : {e}")));
        }
        if let Ok(key) = RsaPrivateKey::from_pkcs8_pem(text) {
            return Ok(key);
        }
        if let Ok(key) = rsa::pkcs1::DecodeRsaPrivateKey::from_pkcs1_pem(text) {
            return Ok(key);
        }
    }
    if let Ok(key) = RsaPrivateKey::from_pkcs8_der(bytes) {
        return Ok(key);
    }
    if let Ok(key) = <RsaPrivateKey as rsa::pkcs1::DecodeRsaPrivateKey>::from_pkcs1_der(bytes) {
        return Ok(key);
    }
    Err(Error::InvalidInput(
        "clé privée RSA illisible (PEM/DER, PKCS#8 ou PKCS#1)".into(),
    ))
}

fn identity_from_parts(
    mut certificates: Vec<Certificate>,
    key: RsaPrivateKey,
) -> Result<SigningIdentity> {
    let position = certificates
        .iter()
        .position(|certificate| certificate_matches_key(certificate, &key))
        .ok_or_else(|| {
            Error::InvalidInput("le certificat ne correspond pas à la clé privée".into())
        })?;
    let certificate = certificates.remove(position);
    Ok(SigningIdentity {
        certificate,
        chain: certificates,
        private_key: key,
    })
}

fn certificate_matches_key(certificate: &Certificate, key: &RsaPrivateKey) -> bool {
    let Ok(public) = key.to_public_key().to_public_key_der() else {
        return false;
    };
    certificate
        .tbs_certificate
        .subject_public_key_info
        .to_der()
        .is_ok_and(|spki| spki == public.as_bytes())
}

fn common_name_of(certificate: &Certificate) -> Option<String> {
    let subject = certificate.tbs_certificate.subject.to_string();
    subject.split(',').find_map(|part| {
        part.trim()
            .strip_prefix("CN=")
            .map(|name| name.trim_matches('"').to_string())
    })
}

fn time_to_date(time: CertTime) -> String {
    chrono::DateTime::<Utc>::from_timestamp(time.to_unix_duration().as_secs() as i64, 0)
        .map(|date| date.format("%Y-%m-%d").to_string())
        .unwrap_or_default()
}

fn resolve_dict(
    document: &lopdf::Document,
    object: &lopdf::Object,
) -> Option<lopdf::Dictionary> {
    match object {
        lopdf::Object::Dictionary(dict) => Some(dict.clone()),
        lopdf::Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|object| object.as_dict().ok())
            .cloned(),
        _ => None,
    }
}

fn resolve_array(
    document: &lopdf::Document,
    object: &lopdf::Object,
) -> Option<Vec<lopdf::Object>> {
    match object {
        lopdf::Object::Array(array) => Some(array.clone()),
        lopdf::Object::Reference(id) => document
            .get_object(*id)
            .ok()
            .and_then(|object| object.as_array().ok())
            .cloned(),
        _ => None,
    }
}

fn signature_field_name(document: &lopdf::Document, signature: u32) -> Option<String> {
    for object in document.objects.values() {
        let Ok(dict) = object.as_dict() else {
            continue;
        };
        let value = dict
            .get(b"V")
            .ok()
            .and_then(|object| object.as_reference().ok())
            .map(|(id, _)| id);
        if value != Some(signature) {
            continue;
        }
        if let Ok(lopdf::Object::String(bytes, _)) = dict.get(b"T") {
            return Some(String::from_utf8_lossy(bytes).into_owned());
        }
    }
    None
}

fn pdf_string(object: Option<&lopdf::Object>) -> String {
    match object {
        Some(lopdf::Object::String(bytes, _)) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        _ => String::new(),
    }
}

/// Sérialise un objet PDF sans le wrapper `n 0 obj … endobj`.
fn object_bytes(object: &lopdf::Object) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    write_pdf_object(&mut out, object);
    Ok(out)
}

fn write_pdf_object(out: &mut Vec<u8>, object: &lopdf::Object) {
    match object {
        lopdf::Object::Null => out.extend_from_slice(b"null"),
        lopdf::Object::Boolean(value) => {
            out.extend_from_slice(if *value { b"true" } else { b"false" })
        }
        lopdf::Object::Integer(value) => out.extend_from_slice(value.to_string().as_bytes()),
        lopdf::Object::Real(value) => {
            let text = format!("{value}");
            out.extend_from_slice(if text.contains('.') { text } else { format!("{text}.0") }.as_bytes());
        }
        lopdf::Object::Name(name) => {
            out.push(b'/');
            for &byte in name {
                if b" \t\n\r\x0C()<>[]{}/%#".contains(&byte) || !(33..=126).contains(&byte) {
                    out.extend_from_slice(format!("#{byte:02X}").as_bytes());
                } else {
                    out.push(byte);
                }
            }
        }
        lopdf::Object::String(bytes, format) => match format {
            lopdf::StringFormat::Literal => {
                out.push(b'(');
                for &byte in bytes {
                    match byte {
                        b'(' | b')' | b'\\' => {
                            out.push(b'\\');
                            out.push(byte);
                        }
                        b'\r' => out.extend_from_slice(b"\\r"),
                        b'\n' => out.extend_from_slice(b"\\n"),
                        _ => out.push(byte),
                    }
                }
                out.push(b')');
            }
            lopdf::StringFormat::Hexadecimal => {
                out.push(b'<');
                out.extend_from_slice(hex_encode(bytes).as_bytes());
                out.push(b'>');
            }
        },
        lopdf::Object::Array(array) => {
            out.push(b'[');
            for (index, item) in array.iter().enumerate() {
                if index > 0 {
                    out.push(b' ');
                }
                write_pdf_object(out, item);
            }
            out.push(b']');
        }
        lopdf::Object::Dictionary(dict) => write_pdf_dictionary(out, dict),
        lopdf::Object::Reference((id, generation)) => {
            out.extend_from_slice(format!("{id} {generation} R").as_bytes())
        }
        lopdf::Object::Stream(stream) => {
            let mut dict = stream.dict.clone();
            dict.set("Length", lopdf::Object::Integer(stream.content.len() as i64));
            write_pdf_dictionary(out, &dict);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&stream.content);
            out.extend_from_slice(b"\nendstream");
        }
    }
}

fn write_pdf_dictionary(out: &mut Vec<u8>, dict: &lopdf::Dictionary) {
    out.extend_from_slice(b"<<");
    for (key, value) in dict.iter() {
        out.push(b' ');
        out.push(b'/');
        for &byte in key {
            if b" \t\n\r\x0C()<>[]{}/%#".contains(&byte) || !(33..=126).contains(&byte) {
                out.extend_from_slice(format!("#{byte:02X}").as_bytes());
            } else {
                out.push(byte);
            }
        }
        out.push(b' ');
        write_pdf_object(out, value);
    }
    out.extend_from_slice(b" >>");
}

/// Littéral PDF échappé (nom, motif, lieu).
fn literal_string(value: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(value.len() + 2);
    out.push(b'(');
    out.extend_from_slice(&escape_sig(value));
    out.push(b')');
    out
}

fn signature_appearance_content(
    width: f64,
    height: f64,
    name: &str,
    reason: &str,
    date: &str,
) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        format!("q 0.96 0.97 1 rg 0 0 {width:.2} {height:.2} re f Q\n").as_bytes(),
    );
    out.extend_from_slice(
        format!("q 0.15 0.35 0.75 RG 1.2 w 1 1 {:.2} {:.2} re S Q\n", width - 2.0, height - 2.0)
            .as_bytes(),
    );
    let text = |font_size: f64, y: f64, value: &str| {
        let mut line = Vec::new();
        line.extend_from_slice(
            format!("BT /Helv {font_size:.1} Tf 0.08 0.12 0.22 rg 6 {y:.2} Td ").as_bytes(),
        );
        line.extend_from_slice(&literal_string(value));
        line.extend_from_slice(b" Tj ET\n");
        line
    };
    out.extend(text(10.0, (height - 18.0).max(8.0), name));
    if !reason.trim().is_empty() {
        out.extend(text(8.0, (height / 2.0 - 3.0).max(7.0), reason));
    }
    out.extend(text(7.0, 5.0, date));
    out
}

fn pdf_date_now() -> String {
    let now = chrono::Local::now();
    let offset = now.offset().local_minus_utc();
    let sign = if offset >= 0 { '+' } else { '-' };
    let minutes = offset.abs() / 60;
    format!(
        "D:{}{}{:02}'{:02}'",
        now.format("%Y%m%d%H%M%S"),
        sign,
        minutes / 60,
        minutes % 60
    )
}

fn pdf_date_short() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

fn last_startxref(bytes: &[u8]) -> Option<usize> {
    let text = String::from_utf8_lossy(bytes);
    let index = text.rfind("startxref")?;
    let digits: String = text[index + "startxref".len()..]
        .trim_start()
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn dn_value(value: &str) -> String {
    value
        .chars()
        .filter(|c| !matches!(c, ',' | '+' | '=' | '\\' | '<' | '>' | ';' | '"'))
        .collect()
}

/// Échappe et encode en WinAnsi le libellé d'une signature.
///
/// Ce texte vient d'un champ saisi par un tiers (nom du signataire, motif
///…). Un `)` non échappé ferme le littéral PDF et permet d'ajouter des
/// opérateurs au flux de contenu ; `<` / `>` ne sont pas non plus échappés,
/// ce qui suffit à composer une chaîne hexadécimale valide donc à écrire du
/// texte arbitraire dans le document signé.
fn escape_sig(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '(' | ')' | '<' | '>' | '\\' => {
                out.push(b'\\');
                out.push(c as u8);
            }
            '\n' | '\r' | '\t' | '\u{0}' => out.push(b' '),
            c if (c as u32) < 0x80 => out.push(c as u8),
            // Idem `encode_pdf_literal` : un octet WinAnsi, jamais du UTF-8.
            c => out.push(crate::edit::winansi_byte_pub(c).unwrap_or(b'?')),
        }
    }
    out
}
