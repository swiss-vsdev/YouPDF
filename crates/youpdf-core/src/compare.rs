//! Comparaison de deux documents PDF.
//!
//! ## Approche
//!
//! Comparaison **au niveau du texte** :
//!
//! 1. Extraction du texte de chaque page de chaque document.
//! 2. Alignement ligne par ligne (algorithme de plus longue sous-séquence
//!    commune, LCS).
//! 3. Classement des différences : ajout, suppression, modification.
//!
//! ## Limites
//!
//! - Ne détecte pas les changements d'image sans changement de texte.
//! - Ne détecte pas les changements de mise en forme à texte identique.
//! - Compare page à page dans l'ordre : une insertion de page décale
//!   tout le reste. Un vrai alignement structurel serait nécessaire
//!   pour aller plus loin.

use serde::{Deserialize, Serialize};

use youpdf_shared::Result;

use crate::document::Document;

/// Type de différence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DifferenceKind {
    /// Ligne présente uniquement dans le second document.
    Added,
    /// Ligne présente uniquement dans le premier document.
    Removed,
    /// Ligne modifiée.
    Modified,
    /// Page entière ajoutée.
    PageAdded,
    /// Page entière supprimée.
    PageRemoved,
}

/// Une différence entre les deux documents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Difference {
    pub kind: DifferenceKind,
    /// Numéro de page dans le document A (1-indexé), si applicable.
    pub page_a: Option<usize>,
    /// Numéro de page dans le document B (1-indexé), si applicable.
    pub page_b: Option<usize>,
    /// Ligne de contexte (avant).
    pub before: Option<String>,
    /// Ligne de contexte (après).
    pub after: Option<String>,
}

/// Rapport de comparaison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonReport {
    /// Nombre de pages dans A.
    pub pages_a: usize,
    /// Nombre de pages dans B.
    pub pages_b: usize,
    /// Différences détectées.
    pub differences: Vec<Difference>,
    /// Nombre de lignes identiques.
    pub identical_lines: usize,
}

impl ComparisonReport {
    /// Indique si les deux documents sont identiques au niveau du texte.
    pub fn is_identical(&self) -> bool {
        self.differences.is_empty()
    }

    /// Compte les différences par type.
    pub fn count(&self, kind: DifferenceKind) -> usize {
        self.differences.iter().filter(|d| d.kind == kind).count()
    }
}

impl Document {
    /// Compare ce document avec un autre.
    pub fn compare_with(&self, other: &Document) -> Result<ComparisonReport> {
        self.compare_with_options(other, false, false)
    }

    /// Compare ce document avec un autre, avec options.
    ///
    /// `ignore_whitespace` écarte les lignes vides et les différences
    /// d'espaces ; `ignore_case` compare en minuscules. Les deux options
    /// ne changent pas le classement des pages, seulement le texte comparé.
    pub fn compare_with_options(
        &self,
        other: &Document,
        ignore_whitespace: bool,
        ignore_case: bool,
    ) -> Result<ComparisonReport> {
        let pages_a = self.page_count();
        let pages_b = other.page_count();

        let mut differences = Vec::new();
        let mut identical = 0usize;

        // Nombre de pages communes à comparer ligne à ligne.
        let common = pages_a.min(pages_b);

        for page in 1..=common {
            let text_a = self.page_text(page as u32).unwrap_or_default();
            let text_b = other.page_text(page as u32).unwrap_or_default();

            let lines_a = normalize_lines(&text_a, ignore_whitespace, ignore_case);
            let lines_b = normalize_lines(&text_b, ignore_whitespace, ignore_case);

            compare_lines(
                &lines_a,
                &lines_b,
                page,
                page,
                &mut differences,
                &mut identical,
            );
        }

        // Pages en plus dans A.
        for page in (common + 1)..=pages_a {
            differences.push(Difference {
                kind: DifferenceKind::PageRemoved,
                page_a: Some(page),
                page_b: None,
                before: Some(format!("Page {page}")),
                after: None,
            });
        }
        // Pages en plus dans B.
        for page in (common + 1)..=pages_b {
            differences.push(Difference {
                kind: DifferenceKind::PageAdded,
                page_a: None,
                page_b: Some(page),
                before: None,
                after: Some(format!("Page {page}")),
            });
        }

        Ok(ComparisonReport {
            pages_a,
            pages_b,
            differences,
            identical_lines: identical,
        })
    }
}

// ────────────────────────────────────────────────────────────────────────
// Algorithme LCS simplifié
// ────────────────────────────────────────────────────────────────────────

/// Compare deux listes de lignes et remplit `differences`.
///
/// Utilise une table LCS pour identifier les zones communes et les
/// zones divergentes. Les zones divergentes produisent des paires
/// Removed / Added, que l'on fusionne en `Modified` quand elles sont
/// adjacentes (heuristique : une ligne supprimée suivie d'une ligne
/// ajoutée au même endroit = modification).
fn compare_lines(
    a: &[String],
    b: &[String],
    page_a: usize,
    page_b: usize,
    differences: &mut Vec<Difference>,
    identical: &mut usize,
) {
    if a.len().saturating_mul(b.len()) > MAX_LCS_CELLS {
        compare_lines_positional(a, b, page_a, page_b, differences, identical);
        return;
    }
    let lcs = compute_lcs(a, b);
    let mut ia = 0usize;
    let mut ib = 0usize;

    // Zone de divergence courante (pour fusionner en Modified).
    let mut pending_removed: Vec<String> = Vec::new();
    let mut pending_added: Vec<String> = Vec::new();

    while ia < a.len() || ib < b.len() {
        if ia < a.len() && ib < b.len() && a[ia] == b[ib] {
            // Zone commune : on vide d'abord les pendings.
            flush_pending(
                &mut pending_removed,
                &mut pending_added,
                page_a,
                page_b,
                differences,
            );

            *identical += 1;
            ia += 1;
            ib += 1;
        } else if lcs[ia][ib] == 0 {
            // Ligne présente en b mais pas en a → ajout.
            if ib < b.len() {
                pending_added.push(b[ib].clone());
                ib += 1;
            } else if ia < a.len() {
                pending_removed.push(a[ia].clone());
                ia += 1;
            }
        } else if ia < a.len() && (ib >= b.len() || lcs[ia][ib] == lcs[ia + 1][ib]) {
            pending_removed.push(a[ia].clone());
            ia += 1;
        } else if ib < b.len() {
            pending_added.push(b[ib].clone());
            ib += 1;
        } else {
            break;
        }
    }

    flush_pending(
        &mut pending_removed,
        &mut pending_added,
        page_a,
        page_b,
        differences,
    );
}

/// Vide les zones de divergence accumulées en produisant des différences.
///
/// Fusionne Removed + Added adjacents en Modified quand les tailles
/// sont comparables.
fn flush_pending(
    removed: &mut Vec<String>,
    added: &mut Vec<String>,
    page_a: usize,
    page_b: usize,
    out: &mut Vec<Difference>,
) {
    if removed.is_empty() && added.is_empty() {
        return;
    }

    // Fusion en Modified si les deux listes ont des tailles voisines.
    if !removed.is_empty() && !added.is_empty() {
        let ratio = removed.len() as f32 / added.len() as f32;
        if (0.5..=2.0).contains(&ratio) {
            let max = removed.len().max(added.len());
            for i in 0..max {
                out.push(Difference {
                    kind: DifferenceKind::Modified,
                    page_a: Some(page_a),
                    page_b: Some(page_b),
                    before: removed.get(i).cloned(),
                    after: added.get(i).cloned(),
                });
            }
            removed.clear();
            added.clear();
            return;
        }
    }

    for line in removed.drain(..) {
        out.push(Difference {
            kind: DifferenceKind::Removed,
            page_a: Some(page_a),
            page_b: Some(page_b),
            before: Some(line),
            after: None,
        });
    }
    for line in added.drain(..) {
        out.push(Difference {
            kind: DifferenceKind::Added,
            page_a: Some(page_a),
            page_b: Some(page_b),
            before: None,
            after: Some(line),
        });
    }
}

/// Nombre maximal de cellules de la table LCS.
///
/// La table est `(n + 1) × (m + 1)` : deux pages très denses feraient
/// grimper l'allocation quadratiquement (une page de 10 000 lignes en face
/// d'une autre = 800 Mo). Au-delà du plafond, on retombe sur la comparaison
/// positionnelle, linéaire.
const MAX_LCS_CELLS: usize = 4_000_000;

/// Repli linéaire quand la table LCS serait trop grande.
///
/// Les lignes sont appariées par position ; le surplus d'un côté est
/// signalé comme ajouté ou retiré. Moins fin qu'un LCS, mais borné.
fn compare_lines_positional(
    a: &[String],
    b: &[String],
    page_a: usize,
    page_b: usize,
    differences: &mut Vec<Difference>,
    identical: &mut usize,
) {
    let common = a.len().min(b.len());
    for i in 0..common {
        if a[i] == b[i] {
            *identical += 1;
        } else {
            differences.push(Difference {
                kind: DifferenceKind::Modified,
                page_a: Some(page_a),
                page_b: Some(page_b),
                before: Some(a[i].clone()),
                after: Some(b[i].clone()),
            });
        }
    }
    for line in &a[common..] {
        differences.push(Difference {
            kind: DifferenceKind::Removed,
            page_a: Some(page_a),
            page_b: Some(page_b),
            before: Some(line.clone()),
            after: None,
        });
    }
    for line in &b[common..] {
        differences.push(Difference {
            kind: DifferenceKind::Added,
            page_a: Some(page_a),
            page_b: Some(page_b),
            before: None,
            after: Some(line.clone()),
        });
    }
}

/// Calcule la table LCS entre deux listes de lignes.
///
/// `lcs[i][j]` = longueur de la plus longue sous-séquence commune
/// entre `a[i..]` et `b[j..]`.
fn compute_lcs(a: &[String], b: &[String]) -> Vec<Vec<usize>> {
    let n = a.len();
    let m = b.len();
    let mut table = vec![vec![0usize; m + 1]; n + 1];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if a[i] == b[j] {
                1 + table[i + 1][j + 1]
            } else {
                table[i + 1][j].max(table[i][j + 1])
            };
        }
    }

    table
}

/// Découpe un texte en lignes comparables, selon les options de comparaison.
///
/// Les lignes vides sont toujours écartées. `ignore_whitespace` retire les
/// espaces de début et de fin de chaque ligne, `ignore_case` les met en
/// minuscules. Les deux options ne changent pas le classement des pages,
/// seulement le texte comparé.
fn normalize_lines(text: &str, ignore_whitespace: bool, ignore_case: bool) -> Vec<String> {
    text.lines()
        .map(|s| {
            if ignore_whitespace {
                s.trim_matches(|c: char| c.is_whitespace())
            } else {
                s.trim()
            }
        })
        .filter(|s| !s.is_empty())
        .map(|s| {
            if ignore_case {
                s.to_lowercase()
            } else {
                s.to_string()
            }
        })
        .collect()
}
