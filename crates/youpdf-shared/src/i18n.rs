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
        // Additional tools and actions.
        "Search text" => "Rechercher du texte",
        "Search file…" => "Rechercher un fichier",
        "Search comments" => "Rechercher des commentaires",
        // Inspector and style controls.
        "Font" => "Police",
        "Size" => "Taille",
        "Style" => "Style",
        "Colour" => "Couleur",
        "Opacity" => "Opacité",
        "Bold" => "Gras",
        "Italic" => "Italique",
        "Underline" => "Souligner",
        "Strikethrough" => "Barré",
        "Alignment" => "Alignement",
        "Justify" => "Justifier",
        "Line spacing" => "Espacement des lignes",
        "Character spacing" => "Espacement des caractères",
        // Form field labels.
        "Name" => "Nom",
        "Tooltip" => "Infobulle",
        "Width" => "Largeur",
        "Height" => "Hauteur",
        "Border style" => "Style de bordure",
        "Font size" => "Taille de police",
        "Default value" => "Valeur par défaut",
        "Calculation" => "Calcul",
        "Format" => "Format",
        "Align" => "Aligner",
        "Sum / Product / Average" => "Somme / Produit / Moyenne",
        // Watermark and background.
        "Page tint" => "Teinte de page",
        "Page border" => "Bord de page",
        "Scale" => "Échelle",
        "Add watermark" => "Ajouter un filigrane",
        "Remove watermark" => "Supprimer le filigrane",
        // Header & footer.
        "Macros" => "Macros",
        "Page" => "Page",
        "Pages" => "Pages",
        "Date" => "Date",
        "Time" => "Heure",
        "File name" => "Nom du fichier",
        "Path" => "Chemin",
        "File path" => "Chemin du fichier",
        // Bates numbering.
        "Prefix" => "Préfixe",
        "Suffix" => "Suffixe",
        "Start number" => "Numéro de départ",
        "Step" => "Pas",
        // Measure.
        "Distance" => "Distance",
        "Perimeter" => "Périmètre",
        "Area" => "Superficie",
        "Calibrate" => "Calibrer",
        "Unit" => "Unité",
        // Convert.
        "Office" => "Bureautique",
        "Text" => "Texte",
        "Images" => "Images",
        "Long image" => "Image longue",
        // OCR.
        "Language" => "Langue",
        "Output mode" => "Mode de sortie",
        "Editable" => "Modifiable",
        "Searchable" => "Recherchable",
        "Text & Pictures" => "Texte et images",
        // Create.
        "Blank document" => "Document vierge",
        "From template" => "À partir d'un modèle",
        // Compare.
        "Choose a PDF…" => "Choisir un PDF…",
        "Compare with the open document" => "Comparer avec le document ouvert",
        // Protect.
        "Remove protection" => "Retirer la protection",
        "Password" => "Mot de passe",
        "Permission" => "Permission",
        "Printing" => "Impression",
        "Changes allowed" => "Modifications autorisées",
        "Extract pages" => "Extraire les pages",
        "Commenting and form filling" => "Commentaires et remplissage de formulaires",
        "Any except extracting pages" => "Sauf extraction de pages",
        "Screen" => "Écran",
        "Ebook" => "Livre électronique",
        "Print" => "Impression",
        // Mode pills.
        "Edit" => "Modifier",
        "Form" => "Formulaire",
        // Additional common labels.
        "OK" => "OK",
        "Select all" => "Tout sélectionner",
        // UI strings wired during the full localization pass.
        "#" => "#",
        "A field needs a name" => "A field needs a name",
        "AI" => "IA",
        "AI assistant" => "Assistant IA",
        "API key" => "Clé API",
        "Accessibility" => "Accessibilité",
        "Activity" => "Activité",
        "Add" => "Ajouter",
        "Add Attachment" => "Ajouter une pièce jointe",
        "Add Bookmark" => "Ajouter un signet",
        "Add PDFs…" => "Ajouter des PDF…",
        "Add Sub-Bookmark" => "Ajouter un sous-signet",
        "Add folder…" => "Ajouter un dossier…",
        "Add the date" => "Ajouter la date",
        "All authors" => "Tous les auteurs",
        "All colors" => "Toutes les couleurs",
        "All types" => "Tous les types",
        "Allow custom text" => "Autoriser le texte personnalisé",
        "Allowed actions" => "Actions autorisées",
        "Also" => "Également",
        "Also remove" => "Retirer aussi",
        "Angle" => "Angle",
        "Appearance" => "Apparence",
        "Apply the crop" => "Appliquer le recadrage",
        "Assemble" => "Assembler",
        "Author" => "Auteur",
        "Auto-crop margins" => "Recadrer les marges",
        "Auto-save" => "Enregistrement automatique",
        "Back" => "Retour",
        "Base URL" => "URL de base",
        "Batch" => "Batch",
        "Batch processing" => "Traitement par lots",
        "Bates prefix" => "Bates prefix",
        "Behind page content" => "Behind page content",
        "Blank A4 page" => "Page A4 vierge",
        "Bookmarks" => "Signets",
        "Border" => "Bordure",
        "Border colour" => "Couleur de bordure",
        "Box" => "Box",
        "Bring your own provider. No telemetry. Nothing leaves this machine unless you press a button." => "Bring your own provider. No telemetry. Nothing leaves this machine unless you press a button.",
        "Calculate script" => "Script de calcul",
        "Cancel the crop" => "Annuler le recadrage",
        "Case sensitive" => "Respecter la casse",
        "Centre" => "Centrer",
        "Certificate" => "Certificat",
        "Check signatures" => "Vérifier les signatures",
        "Choose .p12…" => "Choose .p12…",
        "Choose an image…" => "Choisir une image…",
        "Choose file and replace…" => "Choisir un fichier et remplacer…",
        "Choose folder and split…" => "Choisir un dossier et découper…",
        "Choose images…" => "Choisir des images…",
        "Choose image…" => "Choisir une image…",
        "Clear" => "Effacer",
        "Close All Tabs" => "Fermer tous les onglets",
        "Close Other Tabs" => "Fermer les autres onglets",
        "Close Tab" => "Fermer l’onglet",
        "Close Tabs to the Right" => "Fermer les onglets à droite",
        "Closing now would lose them." => "Closing now would lose them.",
        "Color applies to new text" => "Color applies to new text",
        "Colours come from the current tool presets." => "Colours come from the current tool presets.",
        "Combine every file into one PDF" => "Combine every file into one PDF",
        "Compare PDFs" => "Comparer des PDF",
        // Vue côte à côte de deux documents.
        "Document A" => "Document A",
        "Document B" => "Document B",
        "First" => "Première",
        "Prev" => "Précédente",
        "Next" => "Suivante",
        "Last" => "Dernière",
        "Text report" => "Rapport de texte",
        "Side by side" => "Côte à côte",
        "Ignore whitespace" => "Ignorer les espaces",
        "Ignore case" => "Ignorer la casse",
        "Continue" => "Continuer",
        "Convert" => "Convertir",
        "Copy" => "Copier",
        "Copy File Path" => "Copier le chemin du fichier",
        "Copy text" => "Copier le texte",
        "Country" => "Pays",
        "Create blank" => "Créer un document vierge",
        "Create self-signed…" => "Créer un certificat auto-signé…",
        "Crop the image" => "Recadrer l’image",
        "Custom…" => "Personnalisé…",
        "Default layout" => "Disposition par défaut",
        "Default zoom" => "Zoom par défaut",
        "Delete" => "Supprimer",
        "Delete Bookmark" => "Supprimer le signet",
        "Delete File…" => "Supprimer le fichier…",
        "Delete Pages" => "Supprimer les pages",
        "Delete the image" => "Supprimer l’image",
        "Digits" => "Chiffres",
        "Discard and close" => "Abandonner et fermer",
        "Done" => "Terminé",
        "Draw on the page" => "Dessiner sur la page",
        "Duplicate Pages" => "Dupliquer les pages",
        "Duplicate Tab" => "Dupliquer l’onglet",
        "Email" => "E-mail",
        "Embedded images" => "Images intégrées",
        "Enable AI features" => "Activer les fonctions d’IA",
        "Enable the AI layer" => "Activer la couche d’IA",
        "Encryption" => "Chiffrement",
        "Eraser" => "Gomme",
        "Estimate" => "Estimation",
        "Export" => "Exporter",
        "Export Comments" => "Exporter les commentaires",
        "Export the image as PNG" => "Export the image as PNG",
        "Export the pages as PNG (150 dpi)" => "Export the pages as PNG (150 dpi)",
        "Export the text (plain text)" => "Export the text (plain text)",
        "Extract Selected Pages…" => "Extraire les pages sélectionnées…",
        "Extract data" => "Extraire les données",
        "Extract…" => "Extract…",
        "Fill" => "Remplir",
        "Fill forms" => "Remplir des formulaires",
        "Forget" => "Oublier",
        "Format script" => "Script de format",
        "Free, local-first PDF editing" => "Free, local-first PDF editing",
        "From Clipboard" => "From Clipboard",
        "From PDF file…" => "From PDF file…",
        "From images" => "From images",
        "Fullscreen" => "Plein écran",
        "Gap" => "Écart",
        "General" => "Général",
        "Grey" => "Gris",
        "Header/footer" => "En-tête et pied de page",
        "Highlight fields" => "Surligner les champs",
        "History" => "Historique",
        "Image quality" => "Qualité des images",
        "Image scale" => "Échelle des images",
        "Import" => "Importer",
        "Import a PKCS#12 file" => "Importer un fichier PKCS#12",
        "Include bookmarks" => "Inclure les signets",
        "Include comments" => "Inclure les commentaires",
        "Insert from File…" => "Insérer depuis un fichier…",
        "Insert pages" => "Insérer des pages",
        "Insert pages from…" => "Insérer des pages depuis…",
        "Kind" => "Type",
        "Known" => "Known",
        "Left" => "Gauche",
        "List models" => "Lister les modèles",
        "Location" => "Lieu",
        "Margin" => "Marge",
        "Measure unit" => "Unité de mesure",
        "Metadata and XMP" => "Métadonnées et XMP",
        "Model" => "Modèle",
        "Modify" => "Modifier",
        "Multiline" => "Multiligne",
        "No annotations yet" => "No annotations yet",
        "No certificate loaded — see the Certificates tab." => "No certificate loaded — see the Certificates tab.",
        "No comment matches" => "No comment matches",
        "No page boxes declared" => "No page boxes declared",
        "No signature found in this document" => "No signature found in this document",
        "None" => "Aucun",
        "Notation" => "Notation",
        "Nothing saved yet." => "Rien d’enregistré pour l’instant.",
        "Nothing yet." => "Rien pour l’instant.",
        "OCR language" => "Langue d’OCR",
        "Off" => "Désactivé",
        "Offset" => "Décalage",
        "Ollama runs locally and needs no API key." => "Ollama runs locally and needs no API key.",
        "On" => "Activé",
        "On startup" => "Au démarrage",
        "One file per top-level bookmark" => "One file per top-level bookmark",
        "Open File" => "Ouvrir un fichier",
        "Open in New Tab" => "Ouvrir dans un nouvel onglet",
        "Open password" => "Mot de passe d’ouverture",
        "Operations, applied in this order" => "Opérations, appliquées dans cet ordre",
        "Or create a self-signed certificate" => "Or create a self-signed certificate",
        "Org" => "Organisation",
        "Output" => "Sortie",
        "Output folder" => "Dossier de sortie",
        "Output name" => "Nom de sortie",
        "Overlay text" => "Overlay text",
        "PNG or JPEG, drawn at a quarter of the page width." => "PNG or JPEG, drawn at a quarter of the page width.",
        "Page Properties…" => "Propriétés de la page…",
        "Page numbers" => "Numéros de page",
        "Page shadow" => "Ombre de page",
        "Page thumbnails" => "Vignettes de page",
        "Perform OCR" => "Lancer l’OCR",
        "Permissions password" => "Mot de passe des permissions",
        "Position" => "Position",
        "Properties" => "Propriétés",
        "Provider" => "Fournisseur",
        "Queue" => "File d’attente",
        "Read only" => "Lecture seule",
        "Reason" => "Motif",
        "Recognise text (OCR, searchable)" => "Recognise text (OCR, searchable)",
        "Remove all measurements" => "Supprimer toutes les mesures",
        "Remove from Recents" => "Retirer des récents",
        "Remove from the document" => "Retirer du document",
        "Remove pages" => "Supprimer des pages",
        "Remove protection from a file…" => "Retirer la protection d’un fichier…",
        "Rename" => "Renommer",
        "Replace all" => "Tout remplacer",
        "Replace pages" => "Remplacer les pages",
        "Replace the image, keeping its place" => "Replace the image, keeping its place",
        "Reply" => "Répondre",
        "Required" => "Obligatoire",
        "Reset crop" => "Réinitialiser le recadrage",
        "Resolution" => "Résolution",
        "Reveal in Finder" => "Afficher dans le Finder",
        "Right" => "Droite",
        "Rotate Clockwise" => "Pivoter à droite",
        "Rotate Counter-Clockwise" => "Pivoter à gauche",
        "Run JavaScript on click" => "Run JavaScript on click",
        "Save As…" => "Enregistrer sous…",
        "Save and close" => "Enregistrer et fermer",
        "Save compressed copy…" => "Enregistrer une copie compressée…",
        "Save encrypted copy…" => "Enregistrer une copie chiffrée…",
        "Scan the document" => "Analyser le document",
        "Search options" => "Options de recherche",
        "Segment" => "Segment",
        "Send" => "Envoyer",
        "Send each file to the system printer" => "Send each file to the system printer",
        "Set Destination to This Page" => "Définir la destination sur cette page",
        "Signature box" => "Zone de signature",
        "Skip first page" => "Ignorer la première page",
        "Sort" => "Trier",
        "Sort options" => "Options de tri",
        "Source" => "Source",
        "Source fields, comma separated" => "Source fields, comma separated",
        "Split" => "Découper",
        "Split document" => "Découper le document",
        "Stamp" => "Tampon",
        "Start at" => "Commencer à",
        "Stroke" => "Trait",
        "Task" => "Tâche",
        "Text colour" => "Couleur du texte",
        "The pages leave the file. Undo brings them back." => "The pages leave the file. Undo brings them back.",
        "The stamp is placed at the centre of the current page." => "The stamp is placed at the centre of the current page.",
        "Thumbnails" => "Vignettes",
        "Tile gap" => "Tile gap",
        "Tiled" => "Tiled",
        "Translate to" => "Traduire en",
        "Use Comment mode to add one" => "Use Comment mode to add one",
        "Use colour" => "Use colour",
        "Use this signature" => "Utiliser cette signature",
        "What YouPDF just did" => "What YouPDF just did",
        "Whole words" => "Mots entiers",
        "Working…" => "En cours…",
        "Write a compressed copy" => "Écrire une copie compressée",
        "Years" => "Années",
        "YouPDF" => "YouPDF",
        "Zoom presets" => "Préréglages de zoom",
        "at" => "at",
        "from" => "from",
        "or drop a file anywhere" => "ou déposez un fichier n’importe où",
        "start" => "start",
        "then from" => "then from",
        "to" => "to",
        "with pages from a PDF" => "with pages from a PDF",
        "← → to turn, Esc to leave" => "← → pour tourner, Échap pour quitter",
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

    /// Lit `"key" => "value",` hors de la zone match.
    fn parse_entry(line: &str) -> Option<(&str, &str)> {
        let line = line.trim();
        let (_, after_key) = line.split_once('"')?;
        let after = after_key.strip_prefix('"')?;
        let value = after.split_once('"')?.0;
        Some((after_key.split_once('"').unwrap_or(("", "")).0, value))
    }

    #[test]
    fn dictionnaire_sans_doublon_et_sans_vide() {
        let src = include_str!("i18n.rs");
        let mut seen = std::collections::HashSet::new();
        let mut empty = Vec::new();
        let mut in_match = false;
        for line in src.lines() {
            if line.contains("_ => key") {
                in_match = true;
                continue;
            }
            if !in_match {
                continue;
            }
            if let Some((key, value)) = parse_entry(line) {
                assert!(seen.insert(key), "clé en duplicate : {}", key);
                if value.is_empty() {
                    empty.push(key);
                }
            }
        }
        assert!(empty.is_empty(), "valeurs vides : {:?}", empty);
    }
}
