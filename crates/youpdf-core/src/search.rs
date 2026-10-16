//! Recherche plein texte dans un document PDF.
//!
//! ## Fonctionnement
//!
//! L'index est construit **une fois** à la demande : le texte de toutes
//! les pages est extrait et stocké en mémoire. Les recherches suivantes
//! se font alors en temps linéaire sur cette chaîne, sans réextraire.
//!
//! Pour un PDF de 500 pages (≈ 2 Mo de texte), la recherche prend
//! quelques millisecondes, ce qui reste imperceptible pour l'utilisateur.
//!
//! ## Limites
//!
//! Cet index n'est **pas** un index inversé (pas de tokens, pas de
//! stemming). La recherche est une simple correspondance de sous-chaînes,
//! insensible à la casse dans la limite de [`fold_case`]. C'est suffisant
//! pour un usage interactif, et cela évite de charger un moteur de
//! recherche plein texte lourd.

use serde::{Deserialize, Serialize};

use youpdf_shared::Result;

use crate::document::Document;

/// Un résultat de recherche.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Numéro de page (1-indexé).
    pub page: usize,
    /// Position du début de la correspondance dans le texte de la page.
    pub start: usize,
    /// Position de fin (exclusive).
    pub end: usize,
    /// Extrait de contexte autour de la correspondance (pour affichage).
    pub context: String,
}

/// Options de recherche.
#[derive(Debug, Clone, Copy, Default)]
pub struct SearchOptions {
    /// Distinguer majuscules et minuscules.
    pub case_sensitive: bool,
    /// N'accepter que des mots entiers : la correspondance ne doit pas
    /// être collée à un caractère alphanumérique.
    pub whole_word: bool,
}

/// Index de recherche d'un document.
///
/// Le champ `text` contient le texte concaténé de toutes les pages,
/// séparé par des sauts de page `\u{000C}`. On stocke également un
/// vecteur `page_offsets` qui donne la position de début de chaque
/// page dans `text`, ce qui permet de retrouver rapidement la page
/// d'une correspondance.
pub struct SearchIndex {
    text: String,
    page_offsets: Vec<usize>,
    page_count: usize,
    /// Version insensible à la casse de `text`, **à longueur d'octets
    /// identique** (voir [`fold_case`]). Les positions trouvées dedans
    /// sont donc directement valables dans `text`.
    folded: String,
}

/// Plie la casse **en préservant la longueur en octets**.
///
/// `str::to_lowercase` ne peut pas être utilisé ici : il allonge certains
/// caractères (`İ` → 3 octets, `ẛ` idem), ce qui décalerait toutes les
/// positions renvoyées par rapport au texte d'origine. `to_ascii_lowercase`
/// conservait bien les indices, mais ignorait totalement les accents —
/// chercher `eglise` ne trouvait ni `église` ni `EGLISE`.
///
/// Ce pliage est **multi-lingue et sûr en indices** : seul l'ASCII change,
/// plus les caractères non-ASCII dont la minuscule a exactement la même
/// taille en UTF-8 (le cas courant : É→é, Ç→ç, Ñ→ñ, Ü→ü, Ω→ω…). Un
/// caractère sans équivalent de même taille est laissé tel quel, ce qui ne
/// peut que rendre une correspondance *plus stricte*, jamais décaler.
pub fn fold_case(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii() {
                return c.to_ascii_lowercase();
            }
            let folded = c.to_lowercase().next().unwrap_or(c);
            if folded.len_utf8() == c.len_utf8() {
                folded
            } else {
                c
            }
        })
        .collect()
}

impl SearchIndex {
    /// Construit l'index en extrayant le texte de toutes les pages.
    ///
    /// Le coût est proportionnel au nombre de pages. Pour un document
    /// volumineux (500+ pages), compter quelques centaines de ms.
    pub fn build(doc: &Document) -> Result<Self> {
        let mut text = String::new();
        let mut page_offsets = Vec::new();
        let page_count = doc.page_count();

        for n in 1..=page_count as u32 {
            page_offsets.push(text.len());
            match doc.page_text(n) {
                Ok(page_text) => text.push_str(&page_text),
                Err(e) => {
                    tracing::debug!(page = n, error = %e, "page ignorée dans l'index");
                }
            }
            // Séparateur de page : le form-feed est peu probable dans du
            // texte réel et sert de marqueur net.
            text.push('\u{000C}');
        }

        let folded = fold_case(&text);

        Ok(Self {
            text,
            folded,
            page_offsets,
            page_count,
        })
    }

    /// Nombre de pages indexées.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Nombre total de caractères indexés.
    pub fn char_count(&self) -> usize {
        self.text.len()
    }

    /// Indique si l'index est vide (aucun texte extractible).
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// Recherche insensible à la casse toutes les occurrences de `query`.
    ///
    /// `max_results` limite le nombre de résultats retournés (0 = illimité).
    /// Les résultats sont triés par ordre d'apparition dans le document.
    pub fn search(&self, query: &str, max_results: usize) -> Vec<SearchResult> {
        self.search_with(query, max_results, SearchOptions::default())
    }

    /// Comme [`Self::search`], avec les filtres de [`SearchOptions`].
    pub fn search_with(
        &self,
        query: &str,
        max_results: usize,
        options: SearchOptions,
    ) -> Vec<SearchResult> {
        let query = query.trim();
        if query.is_empty() {
            return Vec::new();
        }

        // Les deux côtés sont pliés par `fold_case`, qui garantit une
        // longueur d'octets inchangée : les indices trouvés dans `folded`
        // restent valables dans `text`. Le pliage ne touche que la casse,
        // pas les accents (« EGLISE » trouve « eglise », « ÉGLISE » trouve
        // « église », mais « EGLISE » ne trouve pas « église ») : c'est le
        // comportement des lecteurs PDF grand public, et cela évite de
        // faire correspondre « côte » et « coté ».
        //
        // Les deux appels à `fold_case` sont O(n) mais **une seule fois
        // par index** : `folded` est précalculé dans `build`, sinon chaque
        // frappe au clavier replierait les 2 Mo du document.
        // En mode sensible à la casse, la comparaison se fait sur le texte
        // brut : les indices restent ceux de `text`, donc valables partout.
        let (haystack, needle): (&str, String) = if options.case_sensitive {
            (self.text.as_str(), query.to_owned())
        } else {
            (self.folded.as_str(), fold_case(query))
        };
        let mut results = Vec::new();
        let mut search_from = 0;
        while search_from < haystack.len() {
            let Some(found) = haystack[search_from..].find(&needle) else {
                break;
            };
            let start = search_from + found;
            let end = start + needle.len();

            // On vérifie que les bornes tombent sur des frontières de
            // caractères UTF-8 valides. `find` renvoie toujours des
            // indices valides, mais l'ajout de `needle.len()` doit
            // être vérifié.
            if !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
                search_from = end.max(start + 1);
                continue;
            }

            if options.whole_word && !self.is_whole_word(start, end) {
                search_from = end.max(start + 1);
                continue;
            }

            let page = self.page_for_offset(start);
            let context = self.extract_context(start, end);

            results.push(SearchResult {
                page,
                start,
                end,
                context,
            });

            if max_results > 0 && results.len() >= max_results {
                break;
            }
            search_from = end.max(start + 1);
        }

        results
    }

    /// Vrai si `start..end` n'est pas collé à un caractère alphanumérique.
    fn is_whole_word(&self, start: usize, end: usize) -> bool {
        let before = self.text[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_alphanumeric());
        let after = self.text[end..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphanumeric());
        !before && !after
    }

    /// Retourne le numéro de page (1-indexé) contenant `offset`.
    ///
    /// Implémentation : recherche binaire dans `page_offsets`. O(log n).
    fn page_for_offset(&self, offset: usize) -> usize {
        match self.page_offsets.binary_search(&offset) {
            Ok(idx) => idx + 1,
            Err(idx) => idx, // idx est le nombre de pages avec offset < offset
        }
    }

    /// Extrait un extrait de contexte de `radius` caractères autour d'une
    /// correspondance, en respectant les frontières UTF-8.
    fn extract_context(&self, start: usize, end: usize) -> String {
        const RADIUS: usize = 60;

        let ctx_start = self.previous_char_boundary(start.saturating_sub(RADIUS));
        let ctx_end = self.next_char_boundary((end + RADIUS).min(self.text.len()));

        let mut out = String::new();
        if ctx_start < start {
            out.push('…');
        }
        out.push_str(self.text[ctx_start..ctx_end].replace('\n', " ").trim());
        if ctx_end < self.text.len() {
            out.push('…');
        }
        out
    }

    fn previous_char_boundary(&self, mut idx: usize) -> usize {
        while idx > 0 && !self.text.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn next_char_boundary(&self, mut idx: usize) -> usize {
        while idx < self.text.len() && !self.text.is_char_boundary(idx) {
            idx += 1;
        }
        idx
    }

    /// Accès direct au texte indexé (utile pour l'IA).
    pub fn text(&self) -> &str {
        &self.text
    }
}
