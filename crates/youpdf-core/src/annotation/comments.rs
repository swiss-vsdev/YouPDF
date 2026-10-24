//! Suppression d'un commentaire et de toute sa descendance de réponses.

use crate::{Annotation, AnnotationKind, AnnotationState};

/// Supprime un commentaire et toute sa descendance de réponses (closure
/// transitive par `reply_to`) de la liste d'annotations passée.
///
/// Retourne la liste des identifiants supprimés (le thread entier), pour que
/// l'appelant puisse nettoyer les structures UI dépendantes (citations, etc.).
///
/// Utilise un `HashSet` de clés pour éviter la scan O(a·k) d'un `contains` sur
/// un `Vec` : chaque réponse trouvée étend la liste à explorer, et le set
/// empêche les doublons sans re-scanne le vecteur.
pub fn remove_comment_thread(annots: &mut Vec<Annotation>, id: &str) -> Vec<String> {
    let mut doomed: Vec<String> = vec![id.to_owned()];
    let mut doomed_set: std::collections::HashSet<String> = std::collections::HashSet::new();
    doomed_set.insert(id.to_owned());
    let mut cursor = 0;
    while cursor < doomed.len() {
        let parent = doomed[cursor].clone();
        cursor += 1;
        for annot in annots.iter() {
            if annot.reply_to.as_deref() == Some(parent.as_str()) && !doomed_set.insert(annot.id.clone()) {
                doomed.push(annot.id.clone());
            }
        }
    }
    annots.retain(|annot| !doomed_set.contains(annot.id.as_str()));
    doomed
}


/// Tri de la liste des commentaires.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentSort {
    /// Par page, puis par date.
    Page,
    /// Du plus récent au plus ancien.
    Date,
    /// Par auteur, puis par page.
    Author,
}

/// Filtre par état de revue (« coché »).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentStateFilter {
    /// Tous les commentaires.
    All,
    /// Ceux qui n'ont pas encore été relus.
    Unchecked,
    /// Un état précis.
    State(AnnotationState),
}

impl CommentStateFilter {
    /// Les choix du menu, dans l'ordre.
    pub const ALL: [Self; 6] = [
        Self::All,
        Self::Unchecked,
        Self::State(AnnotationState::Accepted),
        Self::State(AnnotationState::Rejected),
        Self::State(AnnotationState::Cancelled),
        Self::State(AnnotationState::Completed),
    ];

    /// Libellé anglais pour l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::All => "All status",
            Self::Unchecked => "Unchecked",
            Self::State(state) => state.display_name(),
        }
    }

    /// Vrai quand l'état de revue passe le filtre.
    pub fn matches(self, state: AnnotationState) -> bool {
        match self {
            Self::All => true,
            Self::Unchecked => state == AnnotationState::None,
            Self::State(wanted) => state == wanted,
        }
    }
}

/// Filtres de la liste des commentaires.
#[derive(Debug, Clone)]
pub struct CommentFilter {
    pub kind: Option<AnnotationKind>,
    pub author: Option<String>,
    pub color: Option<[u8; 3]>,
    pub state: CommentStateFilter,
    /// Recherche plein texte, insensible à la casse.
    pub query: String,
}

impl CommentFilter {
    /// Vrai quand le commentaire passe tous les filtres.
    pub fn matches(&self, annotation: &Annotation) -> bool {
        if let Some(kind) = self.kind {
            if annotation.kind != kind {
                return false;
            }
        }
        if let Some(author) = &self.author {
            if &annotation.author != author {
                return false;
            }
        }
        if let Some(color) = self.color {
            if annotation.style.color != color {
                return false;
            }
        }
        if !self.state.matches(annotation.state) {
            return false;
        }
        let query = self.query.trim();
        if !query.is_empty() {
            let query = query.to_lowercase();
            let haystack = format!("{} {}", annotation.content, annotation.author).to_lowercase();
            if !haystack.contains(&query) {
                return false;
            }
        }
        true
    }
}

/// Une ligne de la liste des commentaires, prête à peindre.
pub struct CommentRow {
    /// Indice dans la liste d'annotations passée à [`comment_rows`].
    pub index: usize,
    pub id: String,
    pub page: usize,
    pub kind: AnnotationKind,
    pub content: String,
    pub author: String,
    pub created_at: String,
    pub color: [u8; 3],
    pub state: AnnotationState,
    /// 0 pour un commentaire, 1 et plus pour une réponse imbriquée.
    pub depth: usize,
}

/// Indices des commentaires, dans l'ordre de tri demandé.
pub fn comment_order(annots: &[Annotation], sort: CommentSort) -> Vec<usize> {
    let mut order: Vec<usize> = (0..annots.len()).collect();
    match sort {
        CommentSort::Page => order.sort_by_key(|index| {
            (
                annots[*index].page,
                annots[*index].created_at.clone(),
                annots[*index].id.clone(),
            )
        }),
        CommentSort::Date => order.sort_by(|a, b| {
            annots[*b]
                .created_at
                .cmp(&annots[*a].created_at)
                .then(annots[*a].page.cmp(&annots[*b].page))
                .then(annots[*a].id.cmp(&annots[*b].id))
        }),
        CommentSort::Author => order.sort_by(|a, b| {
            annots[*a]
                .author
                .to_lowercase()
                .cmp(&annots[*b].author.to_lowercase())
                .then(annots[*a].page.cmp(&annots[*b].page))
                .then(annots[*a].id.cmp(&annots[*b].id))
        }),
    }
    order
}

/// Construit les lignes affichées : filtres, tri, puis réponses imbriquées
/// sous leur parent.
///
/// Une réponse dont le parent est filtré remonte à la racine : elle reste
/// visible plutôt que d'être perdue. Un commentaire sans racine (réponse
/// circulaire) est posé à plat, pour la même raison.
pub fn comment_rows(annots: &[Annotation], filter: &CommentFilter, sort: CommentSort) -> Vec<CommentRow> {
    use std::collections::HashSet;

    let included: HashSet<String> = annots
        .iter()
        .filter(|annotation| filter.matches(annotation))
        .map(|annotation| annotation.id.clone())
        .collect();
    let order = comment_order(annots, sort);

    let mut rows = Vec::new();
    let mut emitted: HashSet<String> = HashSet::new();
    for index in &order {
        let annotation = &annots[*index];
        if !included.contains(annotation.id.as_str()) {
            continue;
        }
        let orphan = annotation
            .reply_to
            .as_deref()
            .map_or(true, |parent| !included.contains(parent));
        if orphan {
            append_thread(annots, &order, &included, *index, 0, &mut rows, &mut emitted);
        }
    }
    // Réponses circulaires : jamais atteintes depuis une racine.
    for index in &order {
        let annotation = &annots[*index];
        if included.contains(annotation.id.as_str()) && !emitted.contains(annotation.id.as_str()) {
            append_thread(annots, &order, &included, *index, 0, &mut rows, &mut emitted);
        }
    }
    rows
}

/// Ajoute un commentaire puis ses réponses, récursivement.
pub fn append_thread(
    annots: &[Annotation],
    order: &[usize],
    included: &std::collections::HashSet<String>,
    index: usize,
    depth: usize,
    rows: &mut Vec<CommentRow>,
    emitted: &mut std::collections::HashSet<String>,
) {
    let annotation = &annots[index];
    // `emitted` rompt aussi les cycles : un commentaire déjà posé n'y revient
    // jamais.
    if !emitted.insert(annotation.id.clone()) {
        return;
    }
    rows.push(CommentRow {
        index,
        id: annotation.id.clone(),
        page: annotation.page,
        kind: annotation.kind,
        content: annotation.content.clone(),
        author: annotation.author.clone(),
        created_at: annotation.created_at.clone(),
        color: annotation.style.color,
        state: annotation.state,
        depth,
    });
    for child in order {
        let reply = &annots[*child];
        if reply.reply_to.as_deref() != Some(annotation.id.as_str())
            || !included.contains(reply.id.as_str())
        {
            continue;
        }
        append_thread(annots, order, included, *child, depth + 1, rows, emitted);
    }
}


#[cfg(test)]
mod rows_tests {
    use super::{CommentFilter, CommentSort, CommentStateFilter, comment_rows};
    use crate::{Annotation, AnnotationKind, AnnotationState};

/// Commentaire de test, avec un identifiant et un auteur choisis.
fn comment(id: &str, page: usize, author: &str, when: &str) -> Annotation {
    let mut annot = Annotation::new(page, AnnotationKind::Highlight, [0.0, 0.0, 10.0, 10.0]);
    annot.id = id.to_owned();
    annot.author = author.to_owned();
    annot.created_at = when.to_owned();
    annot
}

fn filter_all() -> CommentFilter {
    CommentFilter {
        kind: None,
        author: None,
        color: None,
        state: CommentStateFilter::All,
        query: String::new(),
    }
}

/// Les réponses s'imbriquent sous leur parent, dans l'ordre du fil.
#[test]
fn comment_rows_nest_replies_under_their_parent() {
    let mut reply = comment("r1", 1, "Bob", "2026-01-02T00:00:00Z");
    reply.reply_to = Some("p1".to_owned());
    let mut deep = comment("r2", 1, "Bob", "2026-01-03T00:00:00Z");
    deep.reply_to = Some("r1".to_owned());
    let annots = vec![comment("p1", 1, "Alice", "2026-01-01T00:00:00Z"), reply, deep];

    let rows = comment_rows(&annots, &filter_all(), CommentSort::Page);

    let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
    assert_eq!(ids, vec!["p1", "r1", "r2"]);
    assert_eq!(rows[0].depth, 0);
    assert_eq!(rows[1].depth, 1);
    assert_eq!(rows[2].depth, 2);
}

/// Une réponse dont le parent est filtré remonte à la racine : elle
/// reste visible.
#[test]
fn comment_rows_promote_an_orphaned_reply() {
    let mut child = comment("r1", 1, "Bob", "2026-01-02T00:00:00Z");
    child.reply_to = Some("p1".to_owned());
    let annots = vec![comment("p1", 1, "Alice", "2026-01-01T00:00:00Z"), child];

    let filter = CommentFilter {
        author: Some("Bob".to_owned()),
        ..filter_all()
    };
    let rows = comment_rows(&annots, &filter, CommentSort::Page);

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "r1");
    assert_eq!(rows[0].depth, 0);
}

/// Une réponse circulaire ne disparaît pas de la liste.
#[test]
fn comment_rows_survive_a_reply_cycle() {
    let mut a = comment("a", 1, "Alice", "2026-01-01T00:00:00Z");
    a.reply_to = Some("b".to_owned());
    let mut b = comment("b", 1, "Bob", "2026-01-02T00:00:00Z");
    b.reply_to = Some("a".to_owned());

    let rows = comment_rows(&[a, b], &filter_all(), CommentSort::Page);

    assert_eq!(rows.len(), 2, "les deux commentaires apparaissent");
}

/// Les filtres de type, d'auteur et d'état se cumulent.
    #[test]
    fn comment_filter_stacks_criteria() {
        let mut accepted = comment("a", 1, "Alice", "2026-01-01T00:00:00Z");
        accepted.state = AnnotationState::Accepted;
        accepted.content = "ligne trouvée".to_owned();
        let mut other = comment("b", 1, "Bob", "2026-01-02T00:00:00Z");
        other.kind = AnnotationKind::Note;
        let annots = vec![accepted, other];

        let filter = CommentFilter {
            kind: Some(AnnotationKind::Highlight),
            author: Some("Alice".to_owned()),
            state: CommentStateFilter::State(AnnotationState::Accepted),
            query: "trouvée".to_owned(),
            ..filter_all()
        };
        let rows = comment_rows(&annots, &filter, CommentSort::Page);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "a");

        // Le même filtre, une case à côté : plus rien ne passe.
        let missed = CommentFilter {
            query: "absente".to_owned(),
            ..filter
        };
        assert!(comment_rows(&annots, &missed, CommentSort::Page).is_empty());

        // « Unchecked » ne retient que l'état nul.
        let unchecked = CommentFilter {
            state: CommentStateFilter::Unchecked,
            ..filter_all()
        };
        let rows = comment_rows(&annots, &unchecked, CommentSort::Page);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "b");
    }

/// Le tri par date va du plus récent au plus ancien.
    #[test]
    fn comment_date_sort_is_newest_first() {
        let annots = vec![
            comment("vieux", 1, "Alice", "2026-01-01T00:00:00Z"),
            comment("neuf", 3, "Bob", "2026-03-01T00:00:00Z"),
        ];
        let rows = comment_rows(&annots, &filter_all(), CommentSort::Date);
        let ids: Vec<&str> = rows.iter().map(|row| row.id.as_str()).collect();
        assert_eq!(ids, vec!["neuf", "vieux"]);
    }
}
