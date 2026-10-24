//! Suppression d'un commentaire et de toute sa descendance de réponses.

use crate::Annotation;

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