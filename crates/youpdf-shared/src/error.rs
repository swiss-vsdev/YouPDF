//! Type d'erreur unifié pour tout YouPDF.
//!
//! On utilise un seul `enum` plutôt qu'une hiérarchie par crate : cela
//! simplifie les signatures et permet à l'UI d'afficher un message adapté
//! selon la catégorie, sans avoir à connaître les détails internes.

use thiserror::Error;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Error)]
pub enum Error {
    /// Erreur d'entrée/sortie (fichiers, réseau bas niveau…).
    #[error("erreur d'entrée/sortie : {0}")]
    Io(#[from] std::io::Error),

    /// Erreur issue du moteur PDF (parsing, structure invalide…).
    #[error("erreur PDF : {0}")]
    Pdf(String),

    /// Erreur renvoyée par un fournisseur d'IA.
    #[error("erreur du fournisseur IA : {0}")]
    Ai(String),

    /// Configuration invalide ou absente.
    #[error("erreur de configuration : {0}")]
    Config(String),

    /// L'utilisateur a demandé quelque chose d'irréalisable en l'état.
    #[error("non pris en charge : {0}")]
    NotSupported(String),

    /// Ressource introuvable.
    #[error("introuvable : {0}")]
    NotFound(String),

    /// Entrée utilisateur invalide.
    #[error("entrée invalide : {0}")]
    InvalidInput(String),

    /// Erreur interne (bug, invariant cassé…). À utiliser avec parcimonie.
    #[error("erreur interne : {0}")]
    Internal(String),
}

impl Error {
    /// Indique si l'erreur est susceptible d'être transitoire (réseau, timeout…).
    pub fn is_transient(&self) -> bool {
        matches!(self, Error::Io(_) | Error::Ai(_))
    }

    /// Message court destiné à être affiché dans la barre d'état.
    pub fn user_message(&self) -> String {
        match self {
            Error::Io(e) => format!("Problème d'accès au fichier : {e}"),
            Error::Pdf(m) => format!("Fichier PDF illisible : {m}"),
            Error::Ai(m) => format!("L'IA n'a pas répondu : {m}"),
            Error::Config(m) => format!("Configuration invalide : {m}"),
            Error::NotSupported(m) => format!("Non pris en charge : {m}"),
            Error::NotFound(m) => format!("Élément introuvable : {m}"),
            Error::InvalidInput(m) => format!("Saisie invalide : {m}"),
            Error::Internal(m) => format!("Erreur interne : {m}"),
        }
    }
}
