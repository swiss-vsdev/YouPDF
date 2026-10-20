//! Traductions de l'interface.
//!
//! La clé d'une traduction est son texte anglais : une chaîne non traduite
//! reste lisible au lieu de disparaître derrière une clé technique. Les
//! chaînes dynamiques (noms de fichiers, messages d'erreur du système) ne
//! passent pas par ici.

use std::cell::Cell;

/// Langue d'interface disponible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// Anglais (langue de référence).
    #[default]
    English,
    /// Français.
    French,
}

impl Language {
    /// Toutes les langues proposées, dans l'ordre du sélecteur.
    pub const ALL: [Language; 2] = [Language::English, Language::French];

    /// Code de langue stocké dans la configuration.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::French => "fr",
        }
    }

    /// Nom affiché dans le sélecteur, dans la langue elle-même.
    pub fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::French => "Français",
        }
    }

    /// Langue correspondant à un code de configuration.
    pub fn from_code(code: &str) -> Self {
        match code {
            "fr" => Language::French,
            _ => Language::English,
        }
    }
}

thread_local! {
    static CURRENT: Cell<Language> = const { Cell::new(Language::English) };
}

/// Langue active.
pub fn language() -> Language {
    CURRENT.with(Cell::get)
}

/// Change la langue active.
pub fn set_language(language: Language) {
    CURRENT.with(|cell| cell.set(language));
}

/// Traduit une chaîne anglaise.
///
/// Une chaîne absente du dictionnaire revient telle quelle : l'interface reste
/// complète pendant que les traductions arrivent.
pub fn t(key: &'static str) -> &'static str {
    if language() == Language::English {
        return key;
    }
    match key {
        // Menus et sections.
        "File" => "Fichier",
        "Edit" => "Édition",
        "View" => "Affichage",
        "Tools" => "Outils",
        "Help" => "Aide",
        "Preferences…" => "Préférences…",
        "Export…" => "Exporter…",
        "Save" => "Enregistrer",
        "Save as…" => "Enregistrer sous…",
        "Close" => "Fermer",
        "Cancel" => "Annuler",
        "Apply" => "Appliquer",
        "Reset" => "Réinitialiser",
        "Remove" => "Retirer",
        "Choose…" => "Choisir…",
        "Open" => "Ouvrir",
        // Outils.
        "Comment" => "Commentaire",
        "Edit PDF" => "Modifier le PDF",
        "Prepare form" => "Préparer un formulaire",
        "Redact" => "Caviarder",
        "Organize pages" => "Organiser les pages",
        "Crop pages" => "Recadrer les pages",
        "Split pages" => "Découper les pages",
        "Watermark" => "Filigrane",
        "Background" => "Arrière-plan",
        "Header & footer" => "En-tête et pied de page",
        "Bates numbering" => "Numérotation Bates",
        "Measure" => "Mesurer",
        "Convert…" => "Convertir…",
        "OCR…" => "OCR…",
        "Create…" => "Créer…",
        "Compare PDFs…" => "Comparer des PDF…",
        "Protect…" => "Protéger…",
        "Reduce file size…" => "Réduire la taille…",
        "Flatten annotations" => "Aplatir les annotations",
        // Préférences.
        "General" => "Général",
        "Appearance" => "Apparence",
        "On startup" => "Au démarrage",
        "Default zoom" => "Zoom par défaut",
        "Default layout" => "Disposition par défaut",
        "Author" => "Auteur",
        "Measure unit" => "Unité de mesure",
        "Auto-save" => "Enregistrement automatique",
        "Language" => "Langue",
        "Disabled" => "Désactivé",
        _ => key,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anglais_est_identite() {
        set_language(Language::English);
        assert_eq!(t("Watermark"), "Watermark");
        assert_eq!(t("Unknown string"), "Unknown string");
    }

    #[test]
    fn francais_traduit_et_retombe_sur_la_cle() {
        set_language(Language::French);
        assert_eq!(t("Watermark"), "Filigrane");
        assert_eq!(t("Not translated"), "Not translated");
        set_language(Language::English);
    }

    #[test]
    fn code_aller_retour() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), language);
        }
    }
}
