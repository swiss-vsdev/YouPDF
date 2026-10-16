//! Ouverture, lecture et sauvegarde de documents PDF.
//!
//! Cette couche encapsule `lopdf`. Le reste du programme ne manipule
//! jamais `lopdf` directement : cela permet de changer de backend
//! (par ex. vers une implémentation maison) sans impacter l'UI ni l'IA.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use youpdf_shared::{Error, Result};

use crate::metadata::DocumentMetadata;

/// Un document PDF chargé en mémoire.
pub struct Document {
    /// Instance bas niveau `lopdf`.
    pub(crate) inner: lopdf::Document,

    /// Chemin d'origine, si le document vient d'un fichier.
    pub path: Option<PathBuf>,

    /// Métadonnées extraites à l'ouverture.
    metadata: DocumentMetadata,

    /// Compteur d'invalidations : incrémenté à chaque accès mutable.
    mutation: Cell<u64>,

    /// Sérialisation courante, réutilisée tant que le document n'a pas été
    /// modifié. Le rendu en a besoin sous forme d'octets ; la refaire à
    /// chaque page rendue coûte O(taille du document) par page, donc
    /// O(n²) pour parcourir un document de n pages.
    snapshot: RefCell<Option<(u64, Rc<Vec<u8>>)>>,
}

impl Document {
    /// Ouvre un PDF depuis le disque.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let inner = lopdf::Document::load(&path)
            .map_err(|e| Error::Pdf(format!("{} : {e}", path.display())))?;

        let metadata = DocumentMetadata::from_lopdf(&inner);
        tracing::info!(pages = inner.get_pages().len(), "PDF chargé");

        Ok(Self {
            inner,
            path: Some(path),
            metadata,
            mutation: Cell::new(0),
            snapshot: RefCell::new(None),
        })
    }

    /// Ouvre un PDF depuis un tampon mémoire.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let inner =
            lopdf::Document::load_mem(bytes).map_err(|e| Error::Pdf(format!("mémoire : {e}")))?;
        let metadata = DocumentMetadata::from_lopdf(&inner);
        Ok(Self {
            inner,
            path: None,
            metadata,
            mutation: Cell::new(0),
            snapshot: RefCell::new(None),
        })
    }

    /// Sauvegarde le document à l'emplacement indiqué.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        self.inner
            .save(path)
            .map_err(|e| Error::Pdf(format!("sauvegarde {} : {e}", path.display())))?;
        // `lopdf::Document::save` peut normaliser le document (longueurs de
        // flux, renumérotation) : la sérialisation en cache n'est plus sûre.
        self.mutation.set(self.mutation.get().wrapping_add(1));
        self.snapshot.borrow_mut().take();
        Ok(())
    }

    /// Sérialise le document en mémoire.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        Ok(self.snapshot()?.as_ref().clone())
    }

    /// Sérialisation courante, partagée (coût de copie constant).
    ///
    /// Le résultat est recalculé uniquement si le document a été modifié
    /// depuis la dernière demande.
    pub(crate) fn snapshot(&self) -> Result<Rc<Vec<u8>>> {
        let generation = self.mutation.get();
        if let Some((gen, bytes)) = self.snapshot.borrow().as_ref() {
            if *gen == generation {
                return Ok(Rc::clone(bytes));
            }
        }

        // `save_to` prend `&mut self` : on sérialise sur une copie, comme
        // auparavant, pour garder une méthode accessible par `&self`.
        let mut buf = Vec::new();
        self.inner
            .clone()
            .save_to(&mut buf)
            .map_err(|e| Error::Pdf(format!("sérialisation : {e}")))?;
        let bytes = Rc::new(buf);
        *self.snapshot.borrow_mut() = Some((generation, Rc::clone(&bytes)));
        Ok(bytes)
    }

    /// Enveloppe un document `lopdf` déjà chargé (copies internes).
    pub(crate) fn from_lopdf(inner: lopdf::Document) -> Self {
        let metadata = DocumentMetadata::from_lopdf(&inner);
        Self {
            inner,
            path: None,
            metadata,
            mutation: Cell::new(0),
            snapshot: RefCell::new(None),
        }
    }

    /// Signale une modification : invalide la sérialisation en cache.
    pub(crate) fn touch(&mut self) {
        self.mutation.set(self.mutation.get().wrapping_add(1));
        self.snapshot.borrow_mut().take();
    }

    /// Identifiant du nœud racine de l'arbre des pages.
    pub(crate) fn pages_root_id(&self) -> Result<lopdf::ObjectId> {
        let catalog = self
            .inner
            .catalog()
            .map_err(|e| Error::Pdf(format!("catalogue : {e}")))?;
        let pages = catalog
            .get(b"Pages")
            .map_err(|e| Error::Pdf(format!("racine des pages : {e}")))?;
        lopdf::Object::as_reference(pages)
            .map_err(|e| Error::Pdf(format!("racine des pages : {e}")))
    }

    /// Résout un numéro de page (1-indexé) en identifiant d'objet lopdf.
    pub(crate) fn page_id(&self, page_number: u32) -> Result<lopdf::ObjectId> {
        self.inner
            .get_pages()
            .get(&page_number)
            .copied()
            .ok_or_else(|| Error::NotFound(format!("page {page_number} introuvable")))
    }

    /// Nombre de pages du document.
    pub fn page_count(&self) -> usize {
        self.inner.get_pages().len()
    }

    /// Métadonnées (titre, auteur, dates…).
    pub fn metadata(&self) -> &DocumentMetadata {
        &self.metadata
    }

    /// Extrait le texte brut d'une page (1-indexé, comme dans la spec PDF).
    ///
    /// Retourne une chaîne vide si la page ne contient pas de texte extractible
    /// (par exemple un scan non océrisé).
    pub fn page_text(&self, page_number: u32) -> Result<String> {
        self.inner
            .extract_text(&[page_number])
            .map_err(|e| Error::Pdf(format!("extraction de la page {page_number} : {e}")))
    }

    /// Extrait le texte de toutes les pages, séparé par un saut de page.
    pub fn all_text(&self) -> Result<String> {
        let total = self.page_count() as u32;
        let mut out = String::new();
        for n in 1..=total {
            if let Ok(txt) = self.page_text(n) {
                out.push_str(&txt);
                out.push_str("\n\u{c}\n");
            }
        }
        Ok(out)
    }

    /// Dimensions (largeur, hauteur) d'une page en points PDF (1/72 pouce).
    ///
    /// Résout `/CropBox` puis `/MediaBox` en remontant la chaîne des
    /// `/Parent` : ces clés sont fréquemment héritées du nœud `Pages`
    /// (documents produits par `cupsfilter`, par exemple).
    ///
    /// Retourne `(595.0, 842.0)` (A4) si aucune boîte exploitable n'existe.
    ///
    /// La taille **d'affichage** est renvoyée : `/Rotate` (hérité, comme les
    /// boîtes) est appliqué, donc une page A4 tournée à 90° mesure 842×595.
    /// C'est ce que produit aussi le rendu pdfium, qui applique la rotation.
    pub fn page_size(&self, page_number: u32) -> (f32, f32) {
        let Ok(page_id) = self.page_id(page_number) else {
            return (595.0, 842.0);
        };

        let mut rotation = 0_i64;
        let mut rotation_known = false;
        let mut current = Some(page_id);
        for _ in 0..16 {
            let Some(id) = current else { break };
            let Ok(dict) = self.inner.get_dictionary(id) else {
                break;
            };

            if !rotation_known {
                if let Ok(object) = dict.get_deref(b"Rotate", &self.inner) {
                    rotation = object.as_i64().unwrap_or(0).rem_euclid(360);
                    rotation_known = true;
                }
            }

            for key in [b"CropBox".as_slice(), b"MediaBox".as_slice()] {
                if let Ok(object) = dict.get_deref(key, &self.inner) {
                    if let Ok(array) = object.as_array() {
                        if array.len() >= 4 {
                            let num =
                                |i: usize| -> f32 { array[i].as_float().unwrap_or(0.0) };
                            let w = (num(2) - num(0)).abs();
                            let h = (num(3) - num(1)).abs();
                            if w > 0.0 && h > 0.0 {
                                return if matches!(rotation, 90 | 270) { (h, w) } else { (w, h) };
                            }
                        }
                    }
                }
            }

            current = dict
                .get(b"Parent")
                .ok()
                .and_then(|parent| parent.as_reference().ok());
        }

        if matches!(rotation, 90 | 270) {
            (842.0, 595.0)
        } else {
            (595.0, 842.0)
        }
    }

    /// Chemin d'origine, si connu.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Nom de fichier suggéré pour l'affichage dans un onglet.
    pub fn display_name(&self) -> String {
        self.path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or("Sans titre")
            .to_string()
    }

    /// Accès contrôlé au document `lopdf` sous-jacent.
    ///
    /// invalide le cache de sérialisation : toute modification passe par ici.
    pub(crate) fn inner_mut(&mut self) -> &mut lopdf::Document {
        self.mutation.set(self.mutation.get().wrapping_add(1));
        self.snapshot.borrow_mut().take();
        &mut self.inner
    }

    pub(crate) fn inner(&self) -> &lopdf::Document {
        &self.inner
    }
}
