//! Extraction de données : tableaux et paires clé/valeur.
//!
//! ## Principe
//!
//! Tout repose sur les fragments de texte positionnés
//! ([`Document::extract_text_fragments`]) : chaque fragment porte un rectangle
//! `[x1, y1, x2, y2]` et une taille de police. Aucune dépendance externe,
//! aucun apprentissage : uniquement de la géométrie.
//!
//! ## Limites
//!
//! Les heuristiques sont volontairement simples et **best effort** :
//!
//! - Un « tableau » est une suite d'au moins trois lignes consécutives qui
//!   partagent au moins deux positions de colonne (x du premier fragment, à
//!   3 pt près).
//! - Les cellules sont délimitées par les trous horizontaux plus larges que
//!   `max(2,5 × avance moyenne des glyphes, 12 pt)`.
//! - Un « libellé : valeur » se détecte sur une cellule finissant par `:`
//!   ou contenant ` : ` ; la valeur est la cellule à droite, sinon la cellule
//!   de même rang sur la ligne suivante.
//!
//! Ces règles privilégient la **précision** (peu de faux positifs) sur le
//! rappel : un tableau non aligné n'est pas signalé, plutôt que de produire
//! une grille fantaisiste.

use youpdf_shared::Result;

use crate::document::Document;
use crate::edit::TextFragment;

/// Tableau repéré dans une page.
#[derive(Debug, Clone)]
pub struct ExtractedTable {
    pub page: u32,
    pub rows: Vec<Vec<String>>,
}

/// Paire clé/valeur repérée dans une page.
#[derive(Debug, Clone)]
pub struct KeyValue {
    pub page: u32,
    pub key: String,
    pub value: String,
}

/// Tolérance verticale minimale pour regrouper des fragments en une ligne.
const LINE_TOL: f32 = 3.0;
/// Tolérance horizontale pour considérer deux colonnes comme alignées.
const COLUMN_TOL: f32 = 3.0;
/// Nombre minimal de lignes consécutives pour parler de tableau.
const MIN_TABLE_ROWS: usize = 3;
/// Nombre minimal de colonnes partagées pour parler de tableau.
const MIN_TABLE_COLUMNS: usize = 2;
/// Écart horizontal minimal (pt) entre deux cellules.
const MIN_CELL_GAP: f32 = 12.0;
/// Facteur appliqué à l'avance moyenne des glyphes pour le seuil de cellule.
const GAP_FACTOR: f32 = 2.5;

impl Document {
    /// Repère les tableaux alignés des pages.
    ///
    /// Retourne un tableau par suite de lignes alignées d'au moins trois
    /// lignes et deux colonnes. L'ordre est celui des pages, puis celui des
    /// lignes (de haut en bas). Renvoie un vecteur vide si rien ne qualifie.
    pub fn extract_tables(&self) -> Result<Vec<ExtractedTable>> {
        let mut tables = Vec::new();

        for page in 1..=self.page_count() as u32 {
            let fragments = self.extract_text_fragments(page)?;
            let lines = group_lines(&fragments);

            for run in table_runs(&lines) {
                let rows: Vec<Vec<String>> = run
                    .iter()
                    .map(|&i| cells_of(&lines[i]).into_iter().map(|c| c.text).collect())
                    .collect();
                // Une suite de lignes vides n'est pas un tableau.
                if rows.iter().all(|row| row.iter().all(|c| c.trim().is_empty())) {
                    continue;
                }
                tables.push(ExtractedTable { page, rows });
            }
        }

        Ok(tables)
    }

    /// Repère les paires « libellé : valeur » des pages.
    ///
    /// Le libellé est le texte d'une cellule avant le `:`. La valeur est la
    /// cellule à droite sur la même ligne, sinon la cellule de même rang sur
    /// la ligne suivante (valeur sous le libellé, même colonne). Les entrées
    /// à libellé ou valeur vide sont ignorées. Ordre de lecture.
    pub fn extract_key_values(&self) -> Result<Vec<KeyValue>> {
        let mut out = Vec::new();

        for page in 1..=self.page_count() as u32 {
            let fragments = self.extract_text_fragments(page)?;
            let lines = group_lines(&fragments);

            for (line_index, line) in lines.iter().enumerate() {
                let cells = cells_of(line);

                for (cell_index, cell) in cells.iter().enumerate() {
                    let Some((key, inline)) = split_key(&cell.text) else {
                        continue;
                    };
                    if key.is_empty() {
                        continue;
                    }

                    let mut value = inline.unwrap_or_default();

                    if value.is_empty() {
                        if let Some(next) = cells.get(cell_index + 1) {
                            value = next.text.trim().to_string();
                        }
                    }
                    if value.is_empty() {
                        if let Some(next_line) = lines.get(line_index + 1) {
                            if let Some(next) = cells_of(next_line).get(cell_index) {
                                value = next.text.trim().to_string();
                            }
                        }
                    }

                    let value = value.trim().to_string();
                    if value.is_empty() {
                        continue;
                    }
                    out.push(KeyValue { page, key, value });
                }
            }
        }

        Ok(out)
    }
}

// ────────────────────────────────────────────────────────────────────────
// Structures internes
// ────────────────────────────────────────────────────────────────────────

/// Fragment réduit à ce dont l'extraction a besoin.
#[derive(Debug, Clone)]
struct Frag {
    x: f32,
    width: f32,
    text: String,
}

/// Une ligne de texte : fragments triés de gauche à droite.
#[derive(Debug, Clone)]
struct Line {
    /// Ordonnée de référence (celle du premier fragment de la ligne).
    y: f32,
    frags: Vec<Frag>,
}

/// Une cellule : position de départ et texte concaténé.
#[derive(Debug, Clone)]
struct Cell {
    x: f32,
    text: String,
}

// ────────────────────────────────────────────────────────────────────────
// Aides internes
// ────────────────────────────────────────────────────────────────────────

/// Tolérance verticale pour la ligne, élargie avec la taille de police.
fn line_tolerance(font_size: f32) -> f32 {
    LINE_TOL.max(font_size * 0.3)
}

/// Regroupe les fragments d'une page en lignes, de haut en bas.
fn group_lines(fragments: &[TextFragment]) -> Vec<Line> {
    let mut sorted: Vec<&TextFragment> = fragments.iter().collect();
    sorted.sort_by(|a, b| {
        b.rect[1]
            .total_cmp(&a.rect[1])
            .then_with(|| a.rect[0].total_cmp(&b.rect[0]))
    });

    let mut lines: Vec<Line> = Vec::new();

    for fragment in sorted {
        let frag = Frag {
            x: fragment.rect[0],
            width: (fragment.rect[2] - fragment.rect[0]).max(0.0),
            text: fragment.text.clone(),
        };

        match lines.last_mut() {
            Some(line)
                if (line.y - fragment.rect[1]).abs() <= line_tolerance(fragment.font_size) =>
            {
                line.frags.push(frag);
            }
            _ => lines.push(Line {
                y: fragment.rect[1],
                frags: vec![frag],
            }),
        }
    }

    for line in &mut lines {
        line.frags.sort_by(|a, b| a.x.total_cmp(&b.x));
    }

    lines
}

/// Découpe une ligne en cellules séparées par des trous horizontaux.
fn cells_of(line: &Line) -> Vec<Cell> {
    let threshold = cell_gap_threshold(line);
    let mut cells: Vec<Cell> = Vec::new();
    let mut previous_end = 0.0_f32;

    for (index, frag) in line.frags.iter().enumerate() {
        if index == 0 {
            cells.push(Cell {
                x: frag.x,
                text: frag.text.clone(),
            });
        } else {
            let gap = frag.x - previous_end;
            if gap > threshold {
                cells.push(Cell {
                    x: frag.x,
                    text: frag.text.clone(),
                });
            } else if let Some(last) = cells.last_mut() {
                if gap > 0.0 {
                    last.text.push(' ');
                }
                last.text.push_str(&frag.text);
            }
        }
        previous_end = frag.x + frag.width;
    }

    cells
}

/// Seuil d'écart horizontal entre deux cellules d'une ligne.
///
/// `max(2,5 × avance moyenne des glyphes, 12 pt)`. L'avance moyenne est le
/// rapport entre la largeur totale et le nombre de caractères de la ligne.
fn cell_gap_threshold(line: &Line) -> f32 {
    let total_width: f32 = line.frags.iter().map(|f| f.width).sum();
    let total_chars: usize = line.frags.iter().map(|f| f.text.chars().count()).sum();
    let advance = if total_chars > 0 {
        total_width / total_chars as f32
    } else {
        0.0
    };
    (GAP_FACTOR * advance).max(MIN_CELL_GAP)
}

/// Repère les suites de lignes formant un tableau.
///
/// Un tableau est une suite d'au moins [`MIN_TABLE_ROWS`] lignes consécutives
/// partageant au moins [`MIN_TABLE_COLUMNS`] positions de colonne. Les
/// colonnes de référence sont celles de la première ligne de la suite.
fn table_runs(lines: &[Line]) -> Vec<Vec<usize>> {
    let mut runs: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut columns: Vec<f32> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let line_columns: Vec<f32> = cells_of(line).into_iter().map(|c| c.x).collect();

        if current.is_empty() {
            columns = line_columns;
            current.push(index);
            continue;
        }

        let shared = line_columns
            .iter()
            .filter(|x| columns.iter().any(|column| (column - **x).abs() <= COLUMN_TOL))
            .count();

        if shared >= MIN_TABLE_COLUMNS {
            current.push(index);
        } else {
            if current.len() >= MIN_TABLE_ROWS {
                runs.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
            columns = line_columns;
            current.push(index);
        }
    }

    if current.len() >= MIN_TABLE_ROWS {
        runs.push(current);
    }

    runs
}

/// Sépare un libellé de sa valeur.
///
/// Reconnaît `Libellé :` (valeur ailleurs) et `Libellé : valeur` (valeur dans
/// la même cellule). Renvoie `None` si le texte n'est pas un libellé.
fn split_key(text: &str) -> Option<(String, Option<String>)> {
    let trimmed = text.trim();

    if let Some(index) = trimmed.find(" : ") {
        let key = trimmed[..index].trim().to_string();
        let value = trimmed[index + 3..].trim().to_string();
        return Some((key, Some(value)));
    }

    if let Some(rest) = trimmed.strip_suffix(':') {
        return Some((rest.trim().to_string(), None));
    }

    None
}
