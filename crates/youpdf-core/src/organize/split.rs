//! Découpe de document : par taille, par signets, tous N pages, et assainissement.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use lopdf::Object;

use youpdf_shared::{Error, Result};

use crate::document::Document;

impl Document {
    /// Découpe en fichiers ne dépassant pas `max_bytes`.
    ///
    /// La taille est mesurée sur un document reconstruit, pas estimée : un
    /// groupe trop gros est réduit page par page jusqu'à tenir.
    pub fn split_by_size(&self, directory: &Path, max_bytes: u64) -> Result<Vec<PathBuf>> {
        let total = self.page_count();
        if total == 0 {
            return Err(Error::Pdf("document vide".into()));
        }
        let stem = self
            .path
            .as_ref()
            .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "document".to_owned());

        // Recherche binaire du nombre maximal de pages par fichier :
        // la taille est mesurée sur un document reconstruit, pas estimée.
        // Un balayage linéaire clones et sérialise le document entier pour
        // chaque candidat → O(n²) ; la recherche binaire fait O(n log n).
        let mut written = Vec::new();
        let mut start = 1usize;
        while start <= total {
            let mut lo = start;
            let mut hi = total;
            let mut last_ok = start; // au moins la page courante
            while lo <= hi {
                let mid = lo + (hi - lo) / 2;
                let pages: Vec<usize> = (start..=mid).collect();
                let size = self.measure_pages(&pages)?.len() as u64;
                if size <= max_bytes {
                    last_ok = mid;
                    lo = mid + 1;
                } else {
                    if mid == start {
                        last_ok = start;
                    }
                    hi = mid - 1;
                }
            }
            let pages: Vec<usize> = (start..=last_ok).collect();
            let target = directory.join(format!("{stem} p{start}-{last_ok}.pdf"));
            self.extract_pages(&pages, &target)?;
            written.push(target);
            start = last_ok + 1;
        }
        Ok(written)
    }

    /// Taille en octets d'un document ne contenant que `pages` (1-indexées).
    ///
    /// Construit une copie partielle et la sérialise : c'est la même
    /// opération que `split_by_size` utilise pour mesurer, extraite pour
    /// éviter de dupliquer le clone dans la recherche binaire.
    pub(crate) fn measure_pages(&self, pages: &[usize]) -> Result<Vec<u8>> {
        let mut copy = Document::from_lopdf(self.inner.clone());
        let keep: HashSet<usize> = pages.iter().copied().collect();
        let remove: Vec<usize> = (1..=copy.page_count())
            .filter(|number| !keep.contains(number))
            .collect();
        if !remove.is_empty() {
            copy.delete_pages(&remove)?;
        }
        copy.to_bytes()
    }

    /// Découpe aux pages des signets de premier niveau.
    pub fn split_by_bookmarks(&self, directory: &Path) -> Result<Vec<PathBuf>> {
        let total = self.page_count();
        let mut cuts: Vec<usize> = self
            .outline()
            .into_iter()
            .filter(|item| item.level == 0)
            .filter_map(|item| item.page)
            .filter(|page| *page >= 1 && *page <= total)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        if cuts.is_empty() {
            return Err(Error::InvalidInput(
                "aucun signet de premier niveau".to_string(),
            ));
        }
        if cuts.first().copied() != Some(1) {
            cuts.insert(0, 1);
        }
        let stem = self
            .path
            .as_ref()
            .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "document".to_owned());
        let mut written = Vec::new();
        for (index, start) in cuts.iter().enumerate() {
            let end = cuts
                .get(index + 1)
                .map(|next| next.saturating_sub(1))
                .unwrap_or(total);
            if end < *start {
                continue;
            }
            let pages: Vec<usize> = (*start..=end).collect();
            let target = directory.join(format!("{stem} p{start}-{end}.pdf"));
            self.extract_pages(&pages, &target)?;
            written.push(target);
        }
        Ok(written)
    }

    /// Découpe le document en fichiers de `per_file` pages maximum.
    pub fn split(&self, directory: &Path, per_file: usize) -> Result<Vec<PathBuf>> {
        let per_file = per_file.max(1);
        let total = self.page_count();
        if total == 0 {
            return Err(Error::Pdf("document vide".into()));
        }
        let stem = self
            .path
            .as_ref()
            .and_then(|path| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
            .unwrap_or_else(|| "document".to_owned());
        let mut written = Vec::new();
        let mut start = 1usize;
        while start <= total {
            let end = (start + per_file - 1).min(total);
            let pages: Vec<usize> = (start..=end).collect();
            let name = format!("{stem} p{start}-{end}.pdf");
            let target = directory.join(name);
            self.extract_pages(&pages, &target)?;
            written.push(target);
            start = end + 1;
        }
        Ok(written)
    }

    /// Retire métadonnées, XMP, JavaScript et actions automatiques.
    ///
    /// Les pièces jointes et les destinations nommées ne font pas partie du
    /// lot : elles vivent elles aussi sous `/Names`, et les supprimer ferait
    /// disparaître des fichiers joints sans rapport avec le nettoyage.
    pub fn sanitize(&mut self) -> Result<usize> {
        let mut removed = 0usize;

        if let Ok(catalog) = self.inner.catalog_mut() {
            for key in [
                b"Metadata".as_slice(),
                b"OpenAction".as_slice(),
                b"AA".as_slice(),
            ] {
                if catalog.remove(key).is_some() {
                    removed += 1;
                }
            }
        }

        // Le JavaScript vit sous `/Names`, à côté de ce qu'on garde.
        let names_id = self
            .inner
            .catalog()
            .ok()
            .and_then(|catalog| catalog.get(b"Names").ok())
            .and_then(|names| names.as_reference().ok());
        match names_id {
            Some(id) => {
                if let Ok(names) = self.inner.get_dictionary_mut(id) {
                    if names.remove(b"JavaScript").is_some() {
                        removed += 1;
                    }
                }
            }
            None => {
                if let Ok(catalog) = self.inner.catalog_mut() {
                    if let Ok(Object::Dictionary(names)) = catalog.get_mut(b"Names") {
                        if names.remove(b"JavaScript").is_some() {
                            removed += 1;
                        }
                    }
                }
            }
        }

        if let Some(info) = self
            .inner
            .trailer
            .get(b"Info")
            .ok()
            .and_then(|object| Object::as_reference(object).ok())
        {
            if let Ok(dict) = self.inner.get_dictionary_mut(info) {
                let keys: Vec<Vec<u8>> = dict.iter().map(|(key, _)| key.clone()).collect();
                for key in keys {
                    if key.as_slice() == b"Producer" {
                        continue;
                    }
                    dict.remove(&key);
                    removed += 1;
                }
            }
        }

        self.touch();
        Ok(removed)
    }
}
