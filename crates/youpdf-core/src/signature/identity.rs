//! Identité de signature : import PKCS#12 / PEM et certificat auto-signé.

use std::io::Write;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::time::Duration as StdDuration;

use chrono::Utc;
use const_oid::ObjectIdentifier;
use der::asn1::OctetString;
use der::{Any, Decode, Encode};
use rsa::pkcs1v15::{Signature as RsaSignature, SigningKey};
use rsa::pkcs8::{DecodePrivateKey, EncodePublicKey};
use rsa::signature::Keypair;
use rsa::RsaPrivateKey;
use sha2::Sha256;
use spki::SubjectPublicKeyInfoOwned;
use x509_cert::builder::{Builder, CertificateBuilder, Profile};
use x509_cert::name::Name;
use x509_cert::serial_number::SerialNumber;
use x509_cert::time::{Time as CertTime, Validity};
use x509_cert::Certificate;

use youpdf_shared::{Error, Result};

use crate::signature::sign::{OID_CERT_BAG, OID_KEY_BAG, OID_SHROUDED_KEY_BAG, OID_X509_CERT};
use crate::signature::util::dn_value;
use crate::signature::{CertificateRequest, DerExt, SigningIdentity};

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

pub(super) fn common_name_of(certificate: &Certificate) -> Option<String> {
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
