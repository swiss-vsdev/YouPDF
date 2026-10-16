//! Compression et optimisation du document.
//!
//! `Document::compress` ré-écrit le document à un emplacement et renvoie
//! les tailles avant/après, en octets. Trois opérations sont effectuées,
//! via `lopdf` 0.34 :
//!
//! - recompression des flux en `FlateDecode` (`Document::compress`),
//! - suppression des objets que l'arbre ne référence plus
//!   (`Document::prune_objects`),
//! - resserrement des identifiants d'objets (`Document::renumber_objects`).
//!
//! ## Ce que cette compression ne fait **pas**
//!
//! - Pas d'*object streams* : `lopdf` 0.34 sait les **lire**, pas les
//!   écrire, donc le fichier écrit reste en objets « classiques ».
//! - Pas de rééchantillonnage ni de recompression des images : une image
//!   déjà en JPEG ou JPEG2000 est conservée telle quelle.
//! - Pas de sous-ensemble de polices (font subsetting), pas de
//!   déduplication des ressources de page.
//!
//! Le gain dépend donc du document ; un PDF déjà optimisé peut maigrir peu
//! ou même grossir légèrement.

use std::path::Path;

use youpdf_shared::Result;

use crate::document::Document;

/// Bilan d'une compression : tailles avant et après, en octets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionReport {
    /// Taille de la sérialisation en mémoire **avant** optimisation.
    pub before_bytes: usize,
    /// Taille du fichier écrit **après** optimisation.
    pub after_bytes: usize,
}

impl CompressionReport {
    /// Octets économisés (positif) ou ajoutés (négatif).
    pub fn saved_bytes(&self) -> isize {
        self.before_bytes as isize - self.after_bytes as isize
    }
}

impl Document {
    /// Compresse le document et l'écrit à `path`.
    ///
    /// Retourne les tailles avant/après. Voir l'en-tête du module pour les
    /// opérations réellement effectuées et celles qui ne le sont pas : la
    /// taille peut augmenter sur un document déjà optimisé.
    pub fn compress(&mut self, path: impl AsRef<Path>) -> Result<CompressionReport> {
        let path = path.as_ref();
        let before_bytes = self.to_bytes()?.len();

        let inner = self.inner_mut();
        inner.prune_objects();
        inner.renumber_objects();
        inner.compress();

        self.save(path)?;

        let after_bytes = std::fs::metadata(path)?.len() as usize;
        Ok(CompressionReport {
            before_bytes,
            after_bytes,
        })
    }
}
