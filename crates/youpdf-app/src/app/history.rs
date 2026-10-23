//! Aides de purge de l'historique d'annulation.

use super::{HistoryEntry, HISTORY_BYTES, HISTORY_LIMIT};

/// Garde les états les plus récents dans les deux limites (nombre et octets).
pub(super) fn trim_history(stack: &mut Vec<HistoryEntry>) {
    trim_history_to(stack, HISTORY_LIMIT, HISTORY_BYTES);
}

pub(super) fn trim_history_to(stack: &mut Vec<HistoryEntry>, limit: usize, budget: usize) {
    // `remove(0)` déplace tous les éléments suivants : O(n) par appel,
    // donc O(n²) pour vider l'historique. `drain` est O(n) une seule fois.
    let excess = stack.len().saturating_sub(limit);
    if excess > 0 {
        stack.drain(..excess);
    }
    // Taille cumulée des documents, de l'entrée la plus ancienne à la plus
    // récente : on retire une tranche de tête jusqu'à ce que le budget
    // tienne, sans repasser par `remove(0)` à chaque entrée.
    let sizes: Vec<usize> = stack
        .iter()
        .filter_map(|entry| entry.doc.as_ref().map(Vec::len))
        .collect();
    let mut total: usize = sizes.iter().sum();
    let mut drop_count = 0;
    while total > budget && drop_count < stack.len().saturating_sub(1) {
        total -= sizes[drop_count];
        drop_count += 1;
    }
    if drop_count > 0 {
        stack.drain(..drop_count);
    }
}
