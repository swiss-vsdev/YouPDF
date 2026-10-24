//! Organisation des pages : rotation, suppression, insertion, extraction,
//! découpe et nettoyage des métadonnées.
//!
//! Toutes les opérations travaillent sur l'arbre des pages `/Pages` du
//! catalogue : les références sont retirées ou ajoutées dans les tableaux
//! `/Kids`, et les compteurs `/Count` sont recalculés en remontant.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use lopdf::{Dictionary, Object, ObjectId, Stream};
use youpdf_shared::{Error, Result};

use crate::document::Document;

mod merge;
mod split;
mod tree;


impl Document {
    /// Fait pivoter les pages indiquées de `delta` degrés (90 = horaire).
    pub fn rotate_pages(&mut self, pages: &[usize], delta: i64) -> Result<usize> {
        if delta % 90 != 0 {
            return Err(Error::Pdf("rotation multiple de 90° attendue".into()));
        }
        let mut rotated = 0usize;
        for number in pages {
            let Ok(id) = self.page_id(*number as u32) else {
                continue;
            };
            let Ok(dict) = self.inner.get_dictionary_mut(id) else {
                continue;
            };
            let current = dict
                .get(b"Rotate")
                .and_then(Object::as_i64)
                .unwrap_or(0);
            let angle = (((current + delta) % 360) + 360) % 360;
            dict.set("Rotate", Object::Integer(angle));
            rotated += 1;
        }
        if rotated > 0 {
            self.touch();
        }
        Ok(rotated)
    }

    /// Supprime les pages indiquées (numéros 1-indexés).
    pub fn delete_pages(&mut self, pages: &[usize]) -> Result<usize> {
        let existing = self.inner.get_pages();
        let doomed: HashSet<ObjectId> = pages
            .iter()
            .filter_map(|number| existing.get(&(*number as u32)).copied())
            .collect();
        if doomed.is_empty() {
            return Ok(0);
        }
        if doomed.len() >= existing.len() {
            return Err(Error::Pdf(
                "au moins une page doit rester dans le document".into(),
            ));
        }
        let root = self.pages_root_id()?;
        self.prune_node(root, &doomed)?;
        for id in &doomed {
            self.inner.objects.remove(id);
        }
        self.touch();
        Ok(doomed.len())
    }
    /// Découpe chaque page visée en `rows × cols` tuiles, chacune devenant
    /// une page du document à la taille de la tuile.
    ///
    /// Chaque tuile garde les ressources de la page d'origine et reçoit un
    /// flux de contenu translaté pour que son coin bas-gauche tombe en (0, 0).
    /// Les pages d'origine sont retirées. Renvoie le nombre de pages créées.
    pub fn split_tiles(&mut self, pages: &[usize], rows: usize, cols: usize) -> Result<usize> {
        self.split_tiles_with_gap(pages, rows, cols, 0.0)
    }

    /// Découpe comme [`Document::split_tiles`], en laissant `gap` points
    /// blancs entre les tuiles.
    ///
    /// L'écart est réparti sur les `cols - 1` colonnes et les `rows - 1`
    /// lignes : les tuiles rétrécissent pour laisser la place du blanc. Un
    /// écart trop grand pour la page est ramené à ce qui tient.
    pub fn split_tiles_with_gap(
        &mut self,
        pages: &[usize],
        rows: usize,
        cols: usize,
        gap: f32,
    ) -> Result<usize> {
        let rows = rows.clamp(1, 20);
        let cols = cols.clamp(1, 20);
        if rows == 1 && cols == 1 {
            return Ok(0);
        }
        let total = self.page_count();
        let mut targets: Vec<usize> = pages
            .iter()
            .copied()
            .filter(|page| *page >= 1 && *page <= total)
            .collect();
        targets.sort_unstable();
        targets.dedup();
        if targets.is_empty() {
            return Err(Error::InvalidInput("aucune page à découper".to_string()));
        }

        // Collecte avant toute mutation : la suppression décalerait les
        // numéros de page.
        #[allow(dead_code)]
        struct Source {
            number: usize,
            id: ObjectId,
            view: crate::document::PageView,
            resources: Object,
            content: Vec<u8>,
            rotate: Option<Object>,
        }
        let mut sources = Vec::new();
        for number in &targets {
            let page_id = self.page_id(*number as u32)?;
            let view = self.page_view(*number as u32);
            let dict = self
                .inner
                .get_dictionary(page_id)
                .map_err(|e| Error::Pdf(format!("page {number} : {e}")))?;
            let resources = dict
                .get(b"Resources")
                .ok()
                .cloned()
                .unwrap_or_else(|| Object::Dictionary(lopdf::Dictionary::new()));
            let rotate = dict.get(b"Rotate").ok().cloned();
            let content = self
                .inner
                .get_page_content(page_id)
                .map_err(|e| Error::Pdf(format!("page {number} : {e}")))?;
            sources.push(Source {
                number: *number,
                id: page_id,
                view,
                resources,
                content,
                rotate,
            });
        }

        let first = targets[0];
        let mut created = Vec::new();
        for source in &sources {
            let view = source.view;
            // L'écart blanc est retiré de la place disponible avant de
            // partager le reste : les tuiles gardent la même taille et le
            // blanc se retrouve entre elles. L'écart est plafonné pour que
            // chaque tuile garde au moins 1 pt de côté.
            let gap_x = if cols > 1 {
                let room = (view.width - cols as f32).max(0.0) / (cols as f32 - 1.0);
                gap.max(0.0).min(room)
            } else {
                0.0
            };
            let gap_y = if rows > 1 {
                let room = (view.height - rows as f32).max(0.0) / (rows as f32 - 1.0);
                gap.max(0.0).min(room)
            } else {
                0.0
            };
            let (tw, th) = (
                (view.width - gap_x * (cols as f32 - 1.0)) / cols as f32,
                (view.height - gap_y * (rows as f32 - 1.0)) / rows as f32,
            );
            for row in 0..rows {
                for col in 0..cols {
                    let x0 = view.x + col as f32 * (tw + gap_x);
                    // `row` se lit de haut en bas, comme l'aperçu.
                    let y0 = view.y + (rows - 1 - row) as f32 * (th + gap_y);
                    let mut page = lopdf::Dictionary::new();
                    page.set("Type", Object::Name(b"Page".to_vec()));
                    page.set(
                        "MediaBox",
                        Object::Array(vec![
                            Object::Integer(0),
                            Object::Integer(0),
                            Object::Real(tw),
                            Object::Real(th),
                        ]),
                    );
                    page.set("Resources", source.resources.clone());
                    if let Some(rotate) = &source.rotate {
                        page.set("Rotate", rotate.clone());
                    }
                    let mut stream = Vec::new();
                    stream.extend_from_slice(
                        format!("q 1 0 0 1 {:.4} {:.4} cm\n", -x0, -y0).as_bytes(),
                    );
                    stream.extend_from_slice(&source.content);
                    stream.extend_from_slice(b"\nQ");
                    page.set(
                        "Contents",
                        Object::Stream(lopdf::Stream::new(
                            lopdf::Dictionary::new(),
                            stream,
                        )),
                    );
                    created.push(self.inner.add_object(Object::Dictionary(page)));
                }
            }
        }

        // Les tuiles prennent la place du premier page visée, puis les
        // originaux disparaissent.
        let root = self.pages_root_id()?;
        let anchor = if first == 1 {
            None
        } else {
            self.inner.get_pages().get(&((first - 1) as u32)).copied()
        };
        self.insert_kids(root, anchor, &created)?;
        let doomed: HashSet<ObjectId> = sources.iter().map(|source| source.id).collect();
        self.prune_node(root, &doomed)?;
        for id in &doomed {
            self.inner.objects.remove(id);
        }
        self.refresh_page_tree()?;
        self.touch();
        Ok(created.len())
    }

    /// Insère une page blanche juste après la page `after`
    /// (`after == 0` place la page en tête).
    ///
    /// Le format demandé (`width` × `height` en points) devient sa boîte
    /// visible : une page ajoutée au milieu d'un document hétérogène garde la
    /// taille qu'on lui donne, elle n'hérite pas de ses voisines.
    pub fn insert_blank_page_after(
        &mut self,
        after: usize,
        width: f32,
        height: f32,
    ) -> Result<usize> {
        if width <= 0.0 || height <= 0.0 {
            return Err(Error::Pdf("format de page invalide".into()));
        }
        let mut page = Dictionary::new();
        page.set("Type", Object::Name(b"Page".to_vec()));
        page.set(
            "MediaBox",
            Object::Array(vec![
                Object::Integer(0),
                Object::Integer(0),
                Object::Real(width),
                Object::Real(height),
            ]),
        );
        page.set("Resources", Object::Dictionary(Dictionary::new()));
        page.set(
            "Contents",
            Object::Stream(Stream::new(Dictionary::new(), Vec::new())),
        );
        let id = self.inner.add_object(Object::Dictionary(page));

        let root = self.pages_root_id()?;
        let page_count = self.inner.get_pages().len();
        let anchor = if after == 0 {
            None
        } else {
            self.inner
                .get_pages()
                .get(&(after.min(page_count) as u32))
                .copied()
        };
        self.insert_kids(root, anchor, &[id])?;
        self.refresh_page_tree()?;
        self.touch();
        Ok(1)
    }

    /// Duplique les pages indiquées, chaque copie placée juste après l'original.
    ///
    /// Les copies sont des objets neufs : modifier la copie ne touche pas
    /// l'original, et l'ordre de sélection n'a pas d'importance.
    pub fn duplicate_pages(&mut self, pages: &[usize]) -> Result<usize> {
        let existing = self.inner.get_pages();
        let mut selected: Vec<(u32, ObjectId)> = pages
            .iter()
            .filter_map(|number| {
                let number = *number as u32;
                existing.get(&number).map(|id| (number, *id))
            })
            .collect();
        if selected.is_empty() {
            return Ok(0);
        }
        selected.sort_by_key(|(number, _)| *number);
        selected.dedup_by_key(|(number, _)| *number);

        // Un instantané suffit pour copier toutes les pages : la copie a
        // besoin de lire la version stable du document.
        let source = self.inner.clone();
        let root = self.pages_root_id()?;
        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut inserted = 0usize;
        for (_, original) in &selected {
            let copy = self.copy_foreign_page(&source, *original, &mut map)?;
            inserted += self.insert_kids(root, Some(*original), &[copy])?;
        }
        self.refresh_page_tree()?;
        self.touch();
        Ok(inserted)
    }

    /// Déplace la page `from` vers la position `to` (numéros 1-indexés).
    ///
    /// La page est retirée de son nœud puis réinsérée à sa nouvelle place.
    /// C'est la même référence qui se déplace, pas une copie : les entrées du
    /// plan, les destinations et les liens qui pointent sur cette page
    /// continuent de la viser, et aucun objet ne devient orphelin.
    pub fn move_page(&mut self, from: usize, to: usize) -> Result<()> {
        if from == 0 || to == 0 || from == to {
            return Ok(());
        }
        let page_count = self.inner.get_pages().len();
        if from > page_count || to > page_count {
            return Ok(());
        }
        let Some(original) = self.inner.get_pages().get(&(from as u32)).copied() else {
            return Ok(());
        };
        let Some(holder) = self.kids_node_of(original) else {
            return Ok(());
        };

        // Retirer la page : les numéros qui la suivaient se décalent de un.
        let mut kids = self.kids_of(holder);
        kids.retain(|child| !matches!(child, Object::Reference(id) if *id == original));
        self.set_kids(holder, kids)?;

        // La déposer avant la page qui occupe désormais la position visée.
        let root = self.pages_root_id()?;
        let anchor = if to <= 1 {
            None
        } else {
            self.inner
                .get_pages()
                .get(&((to - 1) as u32))
                .copied()
        };
        self.insert_kids(root, anchor, &[original])?;
        self.refresh_page_tree()?;
        self.touch();
        Ok(())
    }

    /// Recalcule les compteurs de l'arbre des pages.
    pub fn refresh_page_tree(&mut self) -> Result<usize> {
        let root = self.pages_root_id()?;
        let count = self.prune_node(root, &HashSet::new())?;
        self.touch();
        Ok(count)
    }

    /// Insère les pages d'un autre PDF après la page `after`
    /// (`after == 0` place les pages en tête, `after >= page_count` en fin).
    pub fn insert_pages_after(&mut self, after: usize, source: &Path) -> Result<usize> {
        let foreign = lopdf::Document::load(source)
            .map_err(|e| Error::Pdf(format!("{} : {e}", source.display())))?;
        let numbers: Vec<u32> = foreign.get_pages().keys().copied().collect();
        if numbers.is_empty() {
            return Err(Error::Pdf(format!(
                "{} ne contient aucune page",
                source.display()
            )));
        }

        let mut map: HashMap<ObjectId, ObjectId> = HashMap::new();
        let mut copied: Vec<ObjectId> = Vec::with_capacity(numbers.len());
        for number in &numbers {
            let Some(page_id) = foreign.get_pages().get(number).copied() else {
                continue;
            };
            copied.push(self.copy_foreign_page(&foreign, page_id, &mut map)?);
        }

        let root = self.pages_root_id()?;

        // `after >= page_count` veut dire « à la fin ». Viser le début
        // collait les pages en tête, à l'opposé de la demande.
        let page_count = self.inner.get_pages().len();
        let anchor = if after == 0 {
            None
        } else if after >= page_count {
            self.inner.get_pages().get(&(page_count as u32)).copied()
        } else {
            self.inner.get_pages().get(&(after as u32)).copied()
        };
        let inserted = self.insert_kids(root, anchor, &copied)?;
        self.refresh_page_tree()?;
        Ok(inserted)
    }

    /// Écrit une copie du document ne contenant que `pages` (1-indexées).
    pub fn extract_pages(&self, pages: &[usize], path: &Path) -> Result<()> {
        let mut copy = Document::from_lopdf(self.inner.clone());
        let keep: HashSet<usize> = pages.iter().copied().collect();
        let remove: Vec<usize> = (1..=copy.page_count())
            .filter(|number| !keep.contains(number))
            .collect();
        if !remove.is_empty() {
            copy.delete_pages(&remove)?;
        }
        copy.save(path)
    }

    /// Remplace les pages `pages` par le contenu d'un autre PDF.
    ///
    /// Les pages de `source` prennent la place du premier page visée ; la
    /// sélection est supprimée avant l'insertion pour garder les numéros
    /// cohérents. Renvoie le nombre de pages insérées.
    pub fn replace_pages(&mut self, pages: &[usize], source: &Path) -> Result<usize> {
        if pages.is_empty() {
            return Err(Error::InvalidInput(
                "aucune page à remplacer".to_string(),
            ));
        }
        let first = pages.iter().copied().min().unwrap_or(1);
        let total = self.page_count();
        let mut remove: Vec<usize> = pages
            .iter()
            .copied()
            .filter(|page| *page >= 1 && *page <= total)
            .collect();
        remove.sort_unstable();
        remove.dedup();
        self.delete_pages(&remove)?;
        // Les pages supprimées libèrent `remove.len()` emplacements : la
        // première page visée prend la place laissée libre avant elle.
        let after = first.saturating_sub(1).min(self.page_count());
        self.insert_pages_after(after, source)
    }

    /// Table des matières brute : identifiants de page dans l'ordre.
    pub fn page_ids(&self) -> BTreeMap<u32, ObjectId> {
        self.inner.get_pages()
    }
}
