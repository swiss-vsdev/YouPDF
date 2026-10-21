//! Organisation des pages : rotation, suppression, insertion, extraction,
//! découpe et nettoyage des métadonnées.
//!
//! Toutes les opérations travaillent sur l'arbre des pages `/Pages` du
//! catalogue : les références sont retirées ou ajoutées dans les tableaux
//! `/Kids`, et les compteurs `/Count` sont recalculés en remontant.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use lopdf::{Dictionary, Object, ObjectId, Stream};
use youpdf_shared::{Error, Result};

use crate::document::Document;

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

    /// Recalcule `/Count` et retire les références condamnées d'un nœud.
    fn prune_node(&mut self, node_id: ObjectId, doomed: &HashSet<ObjectId>) -> Result<usize> {
        let children: Vec<Object> = self
            .inner
            .get_dictionary(node_id)
            .ok()
            .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
            .unwrap_or_default();

        let mut kept: Vec<Object> = Vec::with_capacity(children.len());
        let mut count = 0usize;
        for child in children {
            let Object::Reference(child_id) = child else {
                kept.push(child);
                count += 1;
                continue;
            };
            if doomed.contains(&child_id) {
                continue;
            }
            let is_branch = self
                .inner
                .get_dictionary(child_id)
                .map(|dict| {
                    dict.get(b"Type")
                        .and_then(Object::as_name)
                        .map(|name| name == b"Pages")
                        .unwrap_or(false)
                })
                .unwrap_or(false);
            if is_branch {
                let sub = self.prune_node(child_id, doomed)?;
                if sub == 0 {
                    continue;
                }
                count += sub;
            } else {
                count += 1;
            }
            kept.push(child);
        }

        let dict = self
            .inner
            .get_dictionary_mut(node_id)
            .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?;
        dict.set("Kids", Object::Array(kept));
        dict.set("Count", Object::Integer(count as i64));
        Ok(count)
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

    /// Insère des références dans le tableau `/Kids` qui contient `anchor`.
    ///
    /// Sans ancre, les pages passent en tête. Si l'ancre est absente du
    /// document, elles finissent en fin de nœud racine. La recherche descend
    /// dans **tous** les sous-arbres avant de conclure à l'absence : une ancre
    /// présente dans un second sous-arbre ne doit pas être confondue avec une
    /// ancre manquante, sinon la page atterrit dans le mauvais sous-arbre.
    fn insert_kids(
        &mut self,
        node_id: ObjectId,
        anchor: Option<ObjectId>,
        new_pages: &[ObjectId],
    ) -> Result<usize> {
        if let Some(anchor_id) = anchor {
            if self.insert_kids_after(node_id, anchor_id, new_pages)? {
                return Ok(new_pages.len());
            }
        } else {
            let mut kids: Vec<Object> = new_pages
                .iter()
                .map(|id| Object::Reference(*id))
                .collect();
            kids.extend(self.kids_of(node_id));
            self.set_kids(node_id, kids)?;
            self.set_pages_parent(node_id, new_pages);
            return Ok(new_pages.len());
        }

        // Ancre absente du document : ajouter en fin de nœud racine.
        let mut kids = self.kids_of(node_id);
        kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
        self.set_kids(node_id, kids)?;
        self.set_pages_parent(node_id, new_pages);
        Ok(new_pages.len())
    }

    /// Insère `new_pages` juste après `anchor_id`, dans le nœud qui le contient
    /// ou dans l'un de ses sous-arbres. Renvoie `true` si l'ancre est trouvée.
    fn insert_kids_after(
        &mut self,
        node_id: ObjectId,
        anchor_id: ObjectId,
        new_pages: &[ObjectId],
    ) -> Result<bool> {
        let children = self.kids_of(node_id);
        if let Some(position) = children
            .iter()
            .position(|child| matches!(child, Object::Reference(id) if *id == anchor_id))
        {
            let mut kids = children;
            let tail = kids.split_off(position + 1);
            kids.extend(new_pages.iter().map(|id| Object::Reference(*id)));
            kids.extend(tail);
            self.set_kids(node_id, kids)?;
            self.set_pages_parent(node_id, new_pages);
            return Ok(true);
        }
        for child in children {
            let Object::Reference(child_id) = child else {
                continue;
            };
            if !self.is_branch(child_id) {
                continue;
            }
            if self.insert_kids_after(child_id, anchor_id, new_pages)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Enfants `/Kids` d'un nœud, ou tableau vide.
    fn kids_of(&self, node_id: ObjectId) -> Vec<Object> {
        self.inner
            .get_dictionary(node_id)
            .ok()
            .and_then(|node| node.get(b"Kids").and_then(Object::as_array).ok().cloned())
            .unwrap_or_default()
    }

    /// Écrit le tableau `/Kids` d'un nœud.
    fn set_kids(&mut self, node_id: ObjectId, kids: Vec<Object>) -> Result<()> {
        let dict = self
            .inner
            .get_dictionary_mut(node_id)
            .map_err(|e| Error::Pdf(format!("arbre des pages : {e}")))?;
        dict.set("Kids", Object::Array(kids));
        Ok(())
    }

    /// Vrai si le nœud est un nœud intermédiaire `/Pages`.
    fn is_branch(&self, node_id: ObjectId) -> bool {
        self.inner
            .get_dictionary(node_id)
            .map(|dict| {
                dict.get(b"Type")
                    .and_then(Object::as_name)
                    .map(|name| name == b"Pages")
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    /// Nœud dont le tableau `/Kids` contient directement cette page.
    fn kids_node_of(&self, page_id: ObjectId) -> Option<ObjectId> {
        let root = self.pages_root_id().ok()?;
        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            for child in self.kids_of(node_id) {
                let Object::Reference(child_id) = child else {
                    continue;
                };
                if child_id == page_id {
                    return Some(node_id);
                }
                if self.is_branch(child_id) {
                    stack.push(child_id);
                }
            }
        }
        None
    }

    /// Rattache des pages au nœud qui les reçoit.
    ///
    /// Un `/Parent` figé sur la racine alors que les pages descendent dans un
    /// sous-arbre rend l'arbre incohérent : les lecteurs qui suivent
    /// `/Parent` pour résoudre les boîtes héritées lisent le mauvais nœud.
    fn set_pages_parent(&mut self, node_id: ObjectId, new_pages: &[ObjectId]) {
        for id in new_pages {
            if let Ok(Object::Dictionary(dict)) = self.inner.get_object_mut(*id) {
                dict.set("Parent", Object::Reference(node_id));
            }
        }
    }

    /// Copie profonde d'une page étrangère, identifiants remappés.
    fn copy_foreign_page(
        &mut self,
        foreign: &lopdf::Document,
        page_id: ObjectId,
        map: &mut HashMap<ObjectId, ObjectId>,
    ) -> Result<ObjectId> {
        if let Some(new_id) = map.get(&page_id) {
            return Ok(*new_id);
        }
        let placeholder = self.inner.add_object(Object::Null);
        map.insert(page_id, placeholder);
        let object = foreign
            .get_object(page_id)
            .map_err(|e| Error::Pdf(format!("page source : {e}")))?
            .clone();
        let copied = self.copy_foreign_object(foreign, &object, map)?;
        *self
            .inner
            .get_object_mut(placeholder)
            .map_err(|e| Error::Pdf(format!("copie : {e}")))? = copied;
        Ok(placeholder)
    }

    /// Copie profonde d'un objet, en suivant les références.
    fn copy_foreign_object(
        &mut self,
        foreign: &lopdf::Document,
        object: &Object,
        map: &mut HashMap<ObjectId, ObjectId>,
    ) -> Result<Object> {
        Ok(match object {
            Object::Reference(id) => {
                Object::Reference(self.copy_foreign_page(foreign, *id, map)?)
            }
            Object::Array(items) => {
                let mut copied = Vec::with_capacity(items.len());
                for item in items {
                    copied.push(self.copy_foreign_object(foreign, item, map)?);
                }
                Object::Array(copied)
            }
            Object::Dictionary(dict) => Object::Dictionary(self.copy_foreign_dict(foreign, dict, map)?),
            Object::Stream(stream) => Object::Stream(Stream::new(
                self.copy_foreign_dict(foreign, &stream.dict, map)?,
                stream.content.clone(),
            )),
            other => other.clone(),
        })
    }

    /// Copie profonde d'un dictionnaire ; `/Parent` est retiré des pages.
    fn copy_foreign_dict(
        &mut self,
        foreign: &lopdf::Document,
        dict: &Dictionary,
        map: &mut HashMap<ObjectId, ObjectId>,
    ) -> Result<Dictionary> {
        let is_page = dict
            .get(b"Type")
            .and_then(Object::as_name)
            .map(|name| name == b"Page")
            .unwrap_or(false);
        let mut copied = Dictionary::new();
        for (key, value) in dict.iter() {
            if is_page && key.as_slice() == b"Parent" {
                continue;
            }
            copied.set(key.clone(), self.copy_foreign_object(foreign, value, map)?);
        }
        Ok(copied)
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
        let mut written = Vec::new();
        let mut start = 1usize;
        while start <= total {
            let mut end = start;
            let mut last_ok = start;
            while end <= total {
                let mut copy = Document::from_lopdf(self.inner.clone());
                let keep: HashSet<usize> = (start..=end).collect();
                let remove: Vec<usize> = (1..=copy.page_count())
                    .filter(|number| !keep.contains(number))
                    .collect();
                if !remove.is_empty() {
                    copy.delete_pages(&remove)?;
                }
                let size = copy.to_bytes()?.len() as u64;
                if size <= max_bytes || end == start {
                    last_ok = end;
                    end += 1;
                } else {
                    break;
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

    /// Table des matières brute : identifiants de page dans l'ordre.
    pub fn page_ids(&self) -> BTreeMap<u32, ObjectId> {
        self.inner.get_pages()
    }
}
