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

/// Vrai si `text[start..end]` n'est collé à aucun caractère alphanumérique.
///
/// C'est la règle du « mot entier » : `cat` ne correspond pas dans
/// `catalog` ni dans `scat`. Partagée avec l'application, qui surligne et
/// remplace les correspondances trouvées par l'index.
pub fn is_whole_word(text: &str, start: usize, end: usize) -> bool {
    let before = text[..start]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric());
    let after = text[end..]
        .chars()
        .next()
        .is_some_and(|c| c.is_alphanumeric());
    !before && !after
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

    /// Nombre de mots, selon la même règle que `str::split_whitespace`.
    ///
    /// Un « mot » est un séquence de caractères non-blancs délimitée par des
    /// espaces ou des sauts de ligne. Les form-feeds (marqueurs de page) sont
    /// aussi des séparateurs.
    pub fn word_count(&self) -> usize {
        self.text.split_whitespace().count()
    }

    /// Nombre de caractères CJK (idéogrammes), hors ponctuation.
    ///
    /// ponytail: couvre les blocs principaux ; ajouter Hiragana/Katakana/Hangul
    /// si la détection multilingue doit être exhaustive.
    pub fn cjk_count(&self) -> usize {
        self.text.chars().filter(|c| is_cjk(*c)).count()
    }

    /// Nombre d'espaces (blancs) dans le texte indexé.
    pub fn space_count(&self) -> usize {
        self.text.chars().filter(|c| c.is_whitespace()).count()
    }

    /// Statistiques agrégées pour l'affichage dans l'UI.
    pub fn stats(&self) -> DocumentStats {
        DocumentStats::from_index(self)
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
        is_whole_word(&self.text, start, end)
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

/// Positions `(début, fin)` des occurrences de `needle` dans `text`.
///
/// C'est la règle de correspondance de la recherche, appliquée à une ligne
/// de fragment : `needle` est le texte **brut** de la requête (non plié), et
/// [`SearchOptions`] décide de la casse et du mot entier. Les positions sont
/// des indices d'octets dans `text` : le pliage de [`fold_case`] préserve la
/// longueur en octets.
pub fn query_matches(text: &str, needle: &str, options: SearchOptions) -> Vec<(usize, usize)> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }
    let (haystack, needle): (std::borrow::Cow<'_, str>, std::borrow::Cow<'_, str>) =
        if options.case_sensitive {
            (text.into(), needle.into())
        } else {
            (fold_case(text).into(), fold_case(needle).into())
        };
    let mut out = Vec::new();
    let mut cursor = 0usize;
    while let Some(offset) = haystack[cursor..].find(needle.as_ref()) {
        let start = cursor + offset;
        let end = start + needle.len();
        // Le pliage préserve la longueur, mais `find` peut tomber sur une
        // frontière valide sans que `end` le soit : on ne découpe jamais
        // au milieu d'un caractère.
        if !text.is_char_boundary(start) || !text.is_char_boundary(end) {
            cursor = end.max(start + 1);
            continue;
        }
        // Comme l'index : un mot entier se juge sur le texte d'origine.
        if options.whole_word && !is_whole_word(text, start, end) {
            cursor = end.max(start + 1);
            continue;
        }
        out.push((start, end));
        cursor = end.max(start + 1);
    }
    out
}

/// Sous-rectangles d'une ligne couvrant chaque occurrence de `needle`.
///
/// Le texte d'un fragment est une ligne : la position de la correspondance
/// dans la chaîne donne sa place proportionnelle dans la largeur du
/// rectangle. C'est une approximation (police proportionnelle), suffisante
/// pour surligner ou marquer une correspondance.
///
/// `needle` est le texte brut de la requête : la casse et le mot entier sont
/// ceux de `options`, exactement comme pour les résultats de l'index.
pub fn match_rects(
    text: &str,
    rect: [f32; 4],
    needle: &str,
    options: SearchOptions,
) -> Vec<[f32; 4]> {
    let total = text.len().max(1) as f32;
    let width = rect[2] - rect[0];
    let mut out = Vec::new();
    for (start, end) in query_matches(text, needle, options) {
        let before = start as f32 / total;
        let hit = ((end - start) as f32 / total).min(1.0 - before);
        let left = rect[0] + width * before;
        let right = (rect[0] + width * (before + hit)).max(left + 2.0);
        out.push([left, rect[1], right, rect[3]]);
    }
    out
}

/// Remplace toutes les occurrences de `needle` dans `text`.
///
/// `needle` est le texte brut de la requête ; `options` applique les mêmes
/// règles que [`match_rects`] et que l'index de recherche.
pub fn replace_occurrences(
    text: &str,
    needle: &str,
    replacement: &str,
    options: SearchOptions,
) -> String {
    let hits = query_matches(text, needle, options);
    if hits.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut cursor = 0usize;
    for (start, end) in hits {
        out.push_str(&text[cursor..start]);
        out.push_str(replacement);
        cursor = end;
    }
    out.push_str(&text[cursor..]);
    out
}

/// Première occurrence de `needle` dans `hay`, avec les filtres de `options`.
///
/// Les signets et les commentaires ne passent pas par l'index : sans ce
/// détour par [`query_matches`], une recherche « mot entier » y trouverait
/// encore des sous-chaînes que la page, elle, refuse.
pub fn find_query(hay: &str, needle: &str, options: SearchOptions) -> Option<(usize, usize)> {
    query_matches(hay, needle, options).first().copied()
}


/// Vrai si `c` est un caractère CJK unifié.
///
/// ponytail: blocs principaux uniquement ; Hiragana/Katakana/Hangul ne sont
/// pas comptés. Ajouter les plages si la détection doit être exhaustive.
fn is_cjk(c: char) -> bool {
    let cp = c as u32;
    (0x4E00..=0x9FFF).contains(&cp)
        || (0x3400..=0x4DBF).contains(&cp)
        || (0x20000..=0x2A6DF).contains(&cp)
        || (0x2A700..=0x2EBEF).contains(&cp)
}

/// Agrégat des statistiques de texte pour l'affichage dans l'UI.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DocumentStats {
    /// Nombre de pages dans le document.
    pub pages: usize,
    /// Mots au sens `split_whitespace`.
    pub words: usize,
    /// Caractères CJK (idéogrammes).
    pub cjk: usize,
    /// Blancs (`char::is_whitespace`).
    pub spaces: usize,
    /// Tous les caractères (UTF-8) indexés.
    pub total_chars: usize,
}

impl DocumentStats {
    /// Construit les stats depuis un index existant.
    pub fn from_index(index: &SearchIndex) -> Self {
        Self {
            pages: index.page_count(),
            words: index.word_count(),
            cjk: index.cjk_count(),
            spaces: index.space_count(),
            total_chars: index.char_count(),
        }
    }
}


#[cfg(test)]
mod match_tests {
    use super::{find_query, match_rects, query_matches, replace_occurrences, SearchOptions};

    #[test]
    fn find_query_matches_with_and_without_case() {
        let insensitive = SearchOptions::default();
        let sensitive = SearchOptions {
            case_sensitive: true,
            whole_word: false,
        };
        let whole = SearchOptions {
            case_sensitive: false,
            whole_word: true,
        };
        assert_eq!(find_query("Hello World", "world", insensitive), Some((6, 11)));
        assert_eq!(find_query("Hello World", "world", sensitive), None);
        assert_eq!(find_query("accents ÉglIsE", "église", insensitive), Some((8, 15)));
        // Les signets et les commentaires suivent le même filtre « mot
        // entier » que les pages.
        assert_eq!(find_query("Worldwide", "world", whole), None);
        assert_eq!(find_query("Hello World", "world", whole), Some((6, 11)));
    }

    /// Les correspondances d'un fragment suivent la casse et le mot entier :
    /// les mêmes règles que l'index, sans quoi le surlignage mentirait.

    #[test]
    fn query_matches_honours_case_and_whole_word() {
        let insensitive = SearchOptions::default();
        let sensitive = SearchOptions {
            case_sensitive: true,
            whole_word: false,
        };
        let whole = SearchOptions {
            case_sensitive: false,
            whole_word: true,
        };
        assert_eq!(
            query_matches("cat Cat catalog", "cat", insensitive),
            vec![(0, 3), (4, 7), (8, 11)]
        );
        assert_eq!(
            query_matches("cat Cat catalog", "cat", sensitive),
            vec![(0, 3), (8, 11)]
        );
        // « catalog » n'est pas un mot : seules les occurrences isolées
        // comptent, mais « Cat » en est une.
        assert_eq!(
            query_matches("cat Cat catalog", "cat", whole),
            vec![(0, 3), (4, 7)]
        );
        assert_eq!(query_matches("a scat here", "cat", whole), Vec::new());
        // Une requête vide ne correspond à rien, même contre du vide.
        assert!(query_matches("abc", "   ", insensitive).is_empty());
    }

    /// Le remplacement applique les mêmes filtres que le surlignage.

    #[test]
    fn replace_occurrences_honours_case_and_whole_word() {
        let insensitive = SearchOptions::default();
        let sensitive = SearchOptions {
            case_sensitive: true,
            whole_word: false,
        };
        let whole = SearchOptions {
            case_sensitive: false,
            whole_word: true,
        };
        assert_eq!(
            replace_occurrences("cat Cat catalog", "cat", "dog", insensitive),
            "dog dog dogalog"
        );
        assert_eq!(
            replace_occurrences("cat Cat catalog", "cat", "dog", sensitive),
            "dog Cat dogalog"
        );
        assert_eq!(
            replace_occurrences("cat Cat catalog", "cat", "dog", whole),
            "dog dog catalog"
        );
        // Aucune occurrence : la chaîne revient intacte.
        assert_eq!(
            replace_occurrences("scatter", "cat", "dog", whole),
            "scatter"
        );
    }

    /// Les rectangles de surlignage se placent proportionnellement dans la
    /// ligne et ne marquent pas les correspondances refusées par les filtres.

    #[test]
    fn match_rects_places_only_accepted_matches() {
        let rect = [0.0, 10.0, 110.0, 20.0]; // 11 caractères → 10 pt chacun
        let insensitive = SearchOptions::default();
        let whole = SearchOptions {
            case_sensitive: false,
            whole_word: true,
        };
        let sensitive = SearchOptions {
            case_sensitive: true,
            whole_word: false,
        };

        let all = match_rects("cat cat cat", rect, "cat", insensitive);
        assert_eq!(all.len(), 3);
        let lefts: Vec<f32> = all.iter().map(|r| r[0]).collect();
        for (got, want) in lefts.iter().zip([0.0, 40.0, 80.0]) {
            assert!((got - want).abs() < 0.5, "gauche {got} ≠ {want}");
        }
        // Chaque rectangle couvre la largeur de la correspondance, et garde
        // la hauteur de la ligne.
        for r in &all {
            assert!((r[2] - r[0] - 30.0).abs() < 0.5, "largeur {}", r[2] - r[0]);
            assert_eq!(r[1], rect[1]);
            assert_eq!(r[3], rect[3]);
        }

        // « catalog » : le mot entier refuse la correspondance.
        assert!(match_rects("catalog", rect, "cat", whole).is_empty());
        // Sensible à la casse : « Cat » ne compte pas.
        assert_eq!(match_rects("cat Cat", rect, "cat", sensitive).len(), 1);
        // Requête vide : aucun rectangle.
        assert!(match_rects("cat", rect, " ", insensitive).is_empty());
    }
}
