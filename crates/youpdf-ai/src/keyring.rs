//! Stockage sécurisé des clés API dans le trousseau système.
//!
//! Sur macOS : Keychain.
//! Sur Windows : Credential Manager.
//! Sur Linux : Secret Service (gnome-keyring, KWallet…).
//!
//! **Les clés API ne sont jamais écrites dans un fichier de configuration.**
//! Elles ne transitent que par le trousseau système, et ne sont lues qu'au
//! moment de construire un provider.

use youpdf_shared::{Error, Result};

/// Nom du service utilisé dans le trousseau système.
const SERVICE_NAME: &str = "youpdf";

/// Encapsule les opérations sur le trousseau système.
pub struct SecretStore;

impl SecretStore {
    /// Construit la clé de stockage pour un fournisseur donné.
    ///
    /// Format : `<provider_id>:api_key`. Cela permet d'avoir plusieurs
    /// entrées pour un même fournisseur (clé API, token OAuth…).
    fn entry_name(provider_id: &str) -> String {
        format!("{provider_id}:api_key")
    }

    /// Enregistre (ou remplace) la clé API d'un fournisseur.
    pub fn set_api_key(provider_id: &str, api_key: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::entry_name(provider_id))
            .map_err(|e| Error::Config(format!("accès au trousseau : {e}")))?;
        entry
            .set_password(api_key)
            .map_err(|e| Error::Config(format!("écriture dans le trousseau : {e}")))?;
        tracing::debug!(provider = %provider_id, "clé API enregistrée dans le trousseau");
        Ok(())
    }

    /// Récupère la clé API d'un fournisseur, si elle existe.
    pub fn get_api_key(provider_id: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::entry_name(provider_id))
            .map_err(|e| Error::Config(format!("accès au trousseau : {e}")))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(Error::Config(format!("lecture du trousseau : {e}"))),
        }
    }

    /// Supprime la clé API d'un fournisseur.
    pub fn delete_api_key(provider_id: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE_NAME, &Self::entry_name(provider_id))
            .map_err(|e| Error::Config(format!("accès au trousseau : {e}")))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()), // rien à supprimer
            Err(e) => Err(Error::Config(format!("suppression du trousseau : {e}"))),
        }
    }
}
