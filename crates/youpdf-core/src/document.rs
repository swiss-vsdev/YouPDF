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

/// Rotation ramenée à un multiple de 90 degrés, dans `0..360`.
fn normalize_rotation(degrees: i64) -> i32 {
    let quarters = (degrees as f32 / 90.0).round() as i64;
    (quarters.rem_euclid(4) * 90) as i32
}

/// Géométrie d'affichage d'une page.
///
/// Deux repères coexistent dans un PDF : celui de la page (origine en bas à
/// gauche de la boîte, `/Rotate` non appliqué), où sont écrits le contenu et
/// les `/Rect` des annotations, et celui que voit l'utilisateur (rotation
/// appliquée, boîte visible seule). Une copie du contenu de la page se fait
/// dans le second : sans passer par cette structure, les zones noires et
/// l'image se placent dans deux repères différents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageView {
    /// Coin inférieur gauche de la boîte visible, en points.
    pub x: f32,
    /// Coin inférieur gauche de la boîte visible, en points.
    pub y: f32,
    /// Largeur de la boîte visible, rotation non appliquée.
    pub width: f32,
    /// Hauteur de la boîte visible, rotation non appliquée.
    pub height: f32,
    /// Rotation d'affichage, en degrés : 0, 90, 180 ou 270.
    pub rotation: i32,
}

impl Default for PageView {
    fn default() -> Self {
        PageView {
            x: 0.0,
            y: 0.0,
            width: 595.0,
            height: 842.0,
            rotation: 0,
        }
    }
}

impl PageView {
    /// Taille affichée, rotation appliquée.
    pub fn display_size(&self) -> (f32, f32) {
        if matches!(self.rotation, 90 | 270) {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }

    /// Position d'un point du repère page dans la boîte visible, exprimée en
    /// fractions `0..1` de cette boîte (origine en bas à gauche).
    fn box_fraction(&self, x: f32, y: f32) -> (f32, f32) {
        let u = if self.width > 0.0 {
            (x - self.x) / self.width
        } else {
            0.0
        };
        let v = if self.height > 0.0 {
            (y - self.y) / self.height
        } else {
            0.0
        };
        (u.clamp(0.0, 1.0), v.clamp(0.0, 1.0))
    }

    /// Applique la rotation à une fraction de la boîte visible.
    fn rotate_fraction(&self, u: f32, v: f32) -> (f32, f32) {
        match self.rotation {
            90 => (v, 1.0 - u),
            180 => (1.0 - u, 1.0 - v),
            270 => (1.0 - v, u),
            _ => (u, v),
        }
    }

    /// Convertit un rectangle du repère page vers le repère affiché : origine
    /// en bas à gauche de la boîte visible, rotation appliquée.
    pub fn display_rect(&self, rect: [f32; 4]) -> [f32; 4] {
        let (u1, v1) = self.box_fraction(rect[0], rect[1]);
        let (u2, v2) = self.box_fraction(rect[2], rect[3]);
        let (du1, dv1) = self.rotate_fraction(u1, v1);
        let (du2, dv2) = self.rotate_fraction(u2, v2);
        let (width, height) = self.display_size();
        [
            du1.min(du2) * width,
            dv1.min(dv2) * height,
            du1.max(du2) * width,
            dv1.max(dv2) * height,
        ]
    }

    /// Matrice `cm` qui place une image plein cadre sur la page affichée.
    ///
    /// La rotation est incluse dans la matrice : `/Rotate` peut rester sur la
    /// page, donc les annotations conservées gardent leur repère.
    pub fn image_matrix(&self) -> [f32; 6] {
        let (width, height) = (self.width, self.height);
        let (x, y) = (self.x, self.y);
        match self.rotation {
            90 => [0.0, height, -width, 0.0, x + width, y],
            180 => [-width, 0.0, 0.0, -height, x + width, y + height],
            270 => [0.0, -height, width, 0.0, x, y + height],
            _ => [width, 0.0, 0.0, height, x, y],
        }
    }
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
        self.page_view(page_number).display_size()
    }

    /// Géométrie d'affichage d'une page : boîte visible et rotation.
    ///
    /// La boîte visible est le `/CropBox`, sinon le `/MediaBox`, hérité de
    /// l'arbre des pages si besoin ; elle ne part pas forcément de `(0, 0)`.
    /// La rotation reste séparée de la boîte : c'est ce que fait un lecteur,
    /// et c'est ce qui permet de replacer une image sans déformer la page.
    pub fn page_view(&self, page_number: u32) -> PageView {
        let Ok(page_id) = self.page_id(page_number) else {
            return PageView::default();
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
                    rotation = object.as_i64().unwrap_or(0);
                    rotation_known = true;
                }
            }

            for key in [b"CropBox".as_slice(), b"MediaBox".as_slice()] {
                if let Ok(object) = dict.get_deref(key, &self.inner) {
                    if let Ok(array) = object.as_array() {
                        if array.len() >= 4 {
                            let num = |i: usize| -> f32 {
                                array[i]
                                    .as_float()
                                    .or_else(|_| array[i].as_i64().map(|v| v as f32))
                                    .unwrap_or(0.0)
                            };
                            let (x, y) = (num(0), num(1));
                            let (width, height) = ((num(2) - x).abs(), (num(3) - y).abs());
                            if width > 0.0 && height > 0.0 {
                                return PageView {
                                    x: x.min(num(2)),
                                    y: y.min(num(3)),
                                    width,
                                    height,
                                    rotation: normalize_rotation(rotation),
                                };
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

        let rotation = normalize_rotation(rotation);
        let (width, height) = if matches!(rotation, 90 | 270) {
            (842.0, 595.0)
        } else {
            (595.0, 842.0)
        };
        PageView {
            x: 0.0,
            y: 0.0,
            width,
            height,
            rotation,
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
