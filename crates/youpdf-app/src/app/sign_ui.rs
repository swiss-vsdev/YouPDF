//! Fenêtre « Signer » : certificat, apparence et validation des signatures.
//!
//! Le moteur PKCS#7 vit dans `youpdf-core::signature` ; cette fenêtre est la
//! seule chose qui manquait pour s'en servir — jusqu'ici `sign_open` était
//! déclaré et mis à `true` par le menu Outils, mais aucune fenêtre ne
//! s'affichait et aucune fonction de signature n'était appelée.

use super::*;
use youpdf_shared::t;
use youpdf_core::signature::{CertificateRequest, SignOptions, SigningIdentity};

/// Vert du bandeau « signature valide ».
const VALID_GREEN: Color32 = Color32::from_rgb(52, 148, 92);
/// Rouge du bandeau « signature invalide ».
const INVALID_RED: Color32 = Color32::from_rgb(196, 62, 62);

/// Ce que l'application sait d'un certificat chargé.
///
/// `SigningIdentity` n'est ni `Clone` ni sérialisable ; on en garde donc les
/// champs qui s'affichent, à côté de l'identité elle-même, prête à signer.
pub(super) struct LoadedCertificate {
    identity: SigningIdentity,
    common_name: String,
    issuer: String,
    valid_from: String,
    valid_to: String,
    self_signed: bool,
}

impl LoadedCertificate {
    fn new(identity: SigningIdentity) -> Self {
        Self {
            common_name: identity.common_name(),
            issuer: identity.issuer(),
            valid_from: identity.valid_from(),
            valid_to: identity.valid_to(),
            self_signed: identity.self_signed(),
            identity,
        }
    }
}

/// Réglages de la fenêtre « Signer », conservés entre deux ouvertures.
pub(super) struct SignUi {
    /// Certificat chargé : importé PKCS#12 ou créé ici.
    certificate: Option<LoadedCertificate>,
    /// Mot de passe du fichier PKCS#12 en cours d'import.
    pkcs12_password: String,
    /// Champs du certificat auto-signé.
    request: CertificateRequest,
    /// Apparence et placement de la signature.
    options: SignOptions,
    /// Dernier retour de validation, une entrée par signature trouvée.
    signatures: Vec<youpdf_core::signature::SignatureInfo>,
    /// Message d'erreur du dernier essai, affiché sous les boutons.
    error: Option<String>,
    /// Sous-vue : 0 = signature, 1 = certificat.
    tab: u8,
}

impl Default for SignUi {
    fn default() -> Self {
        Self {
            certificate: None,
            pkcs12_password: String::new(),
            request: CertificateRequest::default(),
            options: SignOptions {
                field_name: "Signature1".into(),
                ..Default::default()
            },
            signatures: Vec::new(),
            error: None,
            tab: 0,
        }
    }
}

impl YouPdfApp {
    /// Fenêtre « Signer ».
    pub(super) fn sign_window(&mut self, ctx: &egui::Context) {
        if !self.sign_open {
            return;
        }
        let mut open = true;
        self.show_modal(ctx, "modal-sign", "Signatures", 470.0, &mut open, |app, ui| {
            ui.horizontal(|ui| {
                for (value, label) in [(0u8, "Sign"), (1, "Certificates")] {
                    if ui.selectable_label(app.sign.tab == value, label).clicked() {
                        app.sign.tab = value;
                    }
                }
            });
            ui.separator();
            if app.sign.tab == 0 {
                app.sign_panel(ctx, ui);
            } else {
                app.certificate_panel(ctx, ui);
            }
            if let Some(error) = app.sign.error.clone() {
                ui.add_space(4.0);
                ui.label(RichText::new(error).size(11.0).color(INVALID_RED));
            }
        });
        if !open {
            self.sign_open = false;
        }
    }

    /// Onglet « Signer » : certificat courant, apparence, bouton de signature.
    fn sign_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let pages = self.tabs.get(self.active).map_or(0, |tab| tab.total());
        ui.label(RichText::new(t("Certificate")).size(11.0).color(pal().muted));
        match &self.sign.certificate {
            Some(cert) => {
                ui.label(
                    RichText::new(format!(
                        "{} — {} to {}",
                        cert.common_name, cert.valid_from, cert.valid_to
                    ))
                    .size(12.0)
                    .color(pal().icon_bright),
                );
                let issuer = if cert.self_signed {
                    "self-signed".to_string()
                } else {
                    format!("issued by {}", cert.issuer)
                };
                ui.label(RichText::new(issuer).size(10.5).color(pal().muted));
            }
            None => {
                ui.label(
                    RichText::new(t("No certificate loaded — see the Certificates tab."))
                        .size(11.0)
                        .color(pal().muted),
                );
            }
        }

        ui.add_space(8.0);
        ui.label(RichText::new(t("Appearance")).size(11.0).color(pal().muted));
        ui.horizontal(|ui| {
            ui.label(t("Page"));
            ui.add(egui::DragValue::new(&mut self.sign.options.page).range(1..=pages.max(1)));
        });
        ui.horizontal(|ui| {
            ui.label(t("Name"));
            ui.add(TextEdit::singleline(&mut self.sign.options.name).desired_width(140.0));
            ui.label(t("Reason"));
            ui.add(TextEdit::singleline(&mut self.sign.options.reason).desired_width(140.0));
        });
        ui.horizontal(|ui| {
            ui.label(t("Location"));
            ui.add(TextEdit::singleline(&mut self.sign.options.location).desired_width(140.0));
        });

        ui.add_space(6.0);
        ui.label(RichText::new(t("Signature box")).size(11.0).color(pal().muted));
        ui.horizontal(|ui| {
            for (label, index) in [("X1", 0usize), ("Y1", 1), ("X2", 2), ("Y2", 3)] {
                ui.label(label);
                ui.add(
                    egui::DragValue::new(&mut self.sign.options.rect[index])
                        .speed(1.0)
                        .range(0.0..=2000.0),
                );
            }
        });

        ui.add_space(10.0);
        let ready = self.sign.certificate.is_some() && pages > 0;
        if ui
            .add_enabled(ready, egui::Button::new("Sign document"))
            .clicked()
        {
            self.apply_signature(ctx);
        }
        if !ready {
            let why = if pages == 0 {
                "Open a document first"
            } else {
                "Load a certificate first"
            };
            ui.label(RichText::new(why).size(10.5).color(pal().muted));
        }

        ui.separator();
        if ui.button(t("Check signatures")).clicked() {
            self.refresh_signature_status();
        }
        self.signature_status(ui);
    }

    /// Bandeau de validation : vert si la signature tient, rouge sinon.
    fn signature_status(&self, ui: &mut egui::Ui) {
        if self.sign.signatures.is_empty() {
            ui.label(
                RichText::new(t("No signature found in this document"))
                    .size(11.0)
                    .color(pal().muted),
            );
            return;
        }
        for info in &self.sign.signatures {
            let (colour, headline) = if info.valid {
                (VALID_GREEN, "All digital signatures are valid")
            } else {
                (INVALID_RED, "A signature was modified after signing")
            };
            ui.label(RichText::new(headline).size(11.5).color(colour));
            let field = if info.field.is_empty() {
                "unnamed field"
            } else {
                info.field.as_str()
            };
            let signer = if info.signer.is_empty() {
                "unknown signer"
            } else {
                info.signer.as_str()
            };
            let when = info.signed_at.as_deref().unwrap_or("no date");
            ui.label(
                RichText::new(format!("{field} · {signer} · {when}"))
                    .size(10.5)
                    .color(pal().muted),
            );
            if !info.valid {
                ui.label(
                    RichText::new(format!(
                        "Covers the whole document: {}",
                        if info.covers_whole_document { "yes" } else { "no" }
                    ))
                    .size(10.0)
                    .color(pal().muted),
                );
            }
            if let Some(error) = &info.error {
                ui.label(RichText::new(error).size(10.0).color(INVALID_RED));
            }
        }
    }

    /// Onglet « Certificats » : import PKCS#12 et création auto-signée.
    fn certificate_panel(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.label(RichText::new(t("Import a PKCS#12 file")).size(11.0).color(pal().muted));
        ui.horizontal(|ui| {
            ui.label(t("Password"));
            ui.add(
                TextEdit::singleline(&mut self.sign.pkcs12_password)
                    .password(true)
                    .desired_width(160.0),
            );
            if ui.button(t("Choose .p12…")).clicked() {
                self.import_pkcs12(ctx);
            }
        });

        ui.add_space(10.0);
        ui.label(
            RichText::new(t("Or create a self-signed certificate"))
                .size(11.0)
                .color(pal().muted),
        );
        ui.horizontal(|ui| {
            ui.label(t("Name"));
            ui.add(TextEdit::singleline(&mut self.sign.request.common_name).desired_width(150.0));
            ui.label(t("Org"));
            ui.add(TextEdit::singleline(&mut self.sign.request.organization).desired_width(120.0));
        });
        ui.horizontal(|ui| {
            ui.label(t("Email"));
            ui.add(TextEdit::singleline(&mut self.sign.request.email).desired_width(150.0));
            ui.label(t("Country"));
            ui.add(
                TextEdit::singleline(&mut self.sign.request.country)
                    .char_limit(2)
                    .desired_width(40.0),
            );
            ui.label(t("Years"));
            ui.add(egui::DragValue::new(&mut self.sign.request.years).range(1..=30));
        });
        if ui.button(t("Create self-signed…")).clicked() {
            self.create_self_signed();
        }
    }

    /// Importe l'identité d'un fichier `.p12` / `.pfx` choisi par l'utilisateur.
    fn import_pkcs12(&mut self, ctx: &egui::Context) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PKCS#12", &["p12", "pfx"])
            .pick_file()
        else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(err) => {
                self.sign.error = Some(format!("Cannot read {}: {err}", path.display()));
                return;
            }
        };
        match SigningIdentity::from_pkcs12(&bytes, &self.sign.pkcs12_password) {
            Ok(identity) => {
                self.sign.error = None;
                self.sign.certificate = Some(LoadedCertificate::new(identity));
                self.toast(ctx, "Certificate loaded".to_string());
            }
            Err(err) => self.sign.error = Some(format!("PKCS#12 failed: {err}")),
        }
    }

    /// Crée un certificat auto-signé à partir des champs du panneau.
    fn create_self_signed(&mut self) {
        if self.sign.request.common_name.trim().is_empty() {
            self.sign.error = Some("A name is required for a new certificate".to_string());
            return;
        }
        match SigningIdentity::create_self_signed(&self.sign.request) {
            Ok(identity) => {
                self.sign.error = None;
                self.sign.certificate = Some(LoadedCertificate::new(identity));
            }
            Err(err) => self.sign.error = Some(format!("Certificate failed: {err}")),
        }
    }

    /// Signe le document courant et le remplace par le résultat signé.
    fn apply_signature(&mut self, ctx: &egui::Context) {
        let Some(cert) = self.sign.certificate.as_ref() else {
            return;
        };
        let options = self.sign.options.clone();
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        match tab.doc.sign(&cert.identity, &options) {
            Ok(bytes) => match Document::from_bytes(&bytes) {
                Ok(document) => {
                    if let Some(tab) = self.tabs.get_mut(self.active) {
                        tab.doc = document;
                    }
                    self.sign.error = None;
                    self.mark_document_changed();
                    self.refresh_signature_status();
                    self.toast(ctx, "Document signed".to_string());
                }
                Err(err) => self.sign.error = Some(format!("Signed bytes unreadable: {err}")),
            },
            Err(err) => self.sign.error = Some(format!("Signing failed: {err}")),
        }
    }

    /// Relit les signatures du document courant.
    fn refresh_signature_status(&mut self) {
        self.sign.signatures = self
            .tabs
            .get(self.active)
            .and_then(|tab| tab.doc.to_bytes().ok())
            .map(|bytes| youpdf_core::signature::validate_signatures(&bytes))
            .unwrap_or_default();
    }
}
