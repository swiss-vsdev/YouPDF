//! Géométrie de sélection de texte au caractère près.
//!
//! Déplacé depuis `app.rs` : sélection (clic mot / clic ligne / glissé),
//! ordre de lecture, rectangles et quadrilatères de surbrillance. Aucun
//! egui ici, donc testable sans interface.

use crate::edit::TextFragment;

/// Position dans un fragment : caractère `offset`, entre `offset` et
/// `offset + 1`. `offset == longueur` désigne la fin du fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TextHit {
    pub fragment: usize,
    pub offset: usize,
}

/// Longueur d'un fragment en caractères sélectionnables.
pub fn hit_len(frag: &TextFragment) -> usize {
    if frag.chars.is_empty() {
        frag.text.chars().count()
    } else {
        frag.chars.len()
    }
}

/// Boîte d'un caractère. Sans boîtes du moteur, le caractère occupe une
/// part proportionnelle du rectangle du fragment.
pub fn char_box(frag: &TextFragment, index: usize) -> [f32; 4] {
    if let Some(item) = frag.chars.get(index) {
        return item.rect;
    }
    let count = frag.text.chars().count().max(1) as f32;
    let [x1, y1, x2, y2] = frag.rect;
    let width = (x2 - x1).max(0.1);
    let start = index as f32 / count;
    let end = (index + 1) as f32 / count;
    [x1 + width * start, y1, x1 + width * end, y2]
}

/// Caractère le plus proche d'un point, à la frontière près.
///
/// Une frontière est le bord gauche du premier caractère, le bord droit du
/// dernier, et le milieu de l'interstice entre deux caractères. La sélection
/// s'arrête donc au caractère sous le pointeur, sans prendre le bloc entier.
pub fn text_hit(frags: &[TextFragment], point: [f32; 2]) -> Option<TextHit> {
    let mut best: Option<(f32, TextHit)> = None;
    for (fragment, frag) in frags.iter().enumerate() {
        let len = hit_len(frag);
        let line_y = (frag.rect[1] + frag.rect[3]) * 0.5;
        for offset in 0..=len {
            let x = if len == 0 {
                frag.rect[0]
            } else if offset == 0 {
                char_box(frag, 0)[0]
            } else if offset >= len {
                char_box(frag, len - 1)[2]
            } else {
                let before = char_box(frag, offset - 1);
                let after = char_box(frag, offset);
                (before[2] + after[0]) * 0.5
            };
            let dx = point[0] - x;
            let dy = point[1] - line_y;
            let distance = dx * dx + dy * dy;
            if best.map_or(true, |(current, _)| distance < current) {
                best = Some((distance, TextHit { fragment, offset }));
            }
        }
    }
    best.map(|(_, hit)| hit)
}

/// Caractère du fragment à l'offset donné, ou `None` si l'offset est hors
/// du fragment.
pub fn char_at(frag: &TextFragment, offset: usize) -> Option<char> {
    if !frag.chars.is_empty() {
        return frag.chars.get(offset).map(|item| item.ch);
    }
    frag.text.chars().nth(offset)
}

/// Étend une position jusqu'aux bords du mot qui la contient.
///
/// Un mot est une suite de caractères non blancs. La position donnée est
/// ramenée dans le fragment si elle déborde, puis les bords sont cherchés de
/// part et d'autre.
pub fn expand_to_word(frags: &[TextFragment], hit: TextHit) -> Option<(TextHit, TextHit)> {
    let frag = frags.get(hit.fragment)?;
    let len = hit_len(frag);
    if len == 0 {
        return None;
    }
    let mut offset = hit.offset.min(len);

    // Un clic dans l'espace entre deux mots prend le mot suivant ; un clic
    // après le dernier mot prend le mot précédent.
    while offset < len && char_at(frag, offset).is_some_and(char::is_whitespace) {
        offset += 1;
    }
    if offset >= len {
        offset = hit.offset.min(len.saturating_sub(1));
        while offset > 0 && char_at(frag, offset).is_some_and(char::is_whitespace) {
            offset -= 1;
        }
    }
    if char_at(frag, offset).is_some_and(char::is_whitespace) {
        return None;
    }

    let mut start = offset;
    while start > 0 {
        match char_at(frag, start - 1) {
            Some(ch) if !ch.is_whitespace() => start -= 1,
            _ => break,
        }
    }
    let mut end = offset + 1;
    while end < len {
        match char_at(frag, end) {
            Some(ch) if !ch.is_whitespace() => end += 1,
            _ => break,
        }
    }
    Some((
        TextHit {
            fragment: hit.fragment,
            offset: start,
        },
        TextHit {
            fragment: hit.fragment,
            offset: end,
        },
    ))
}

/// Étend une position jusqu'aux bords de sa ligne visuelle.
///
/// Une ligne regroupe les fragments qui partagent la même ligne de base, à la
/// tolérance de corps de police employée partout ailleurs (60 % de la
/// hauteur). La sélection part du début de la ligne et finit à sa fin, dans
/// l'ordre de lecture.
pub fn expand_to_line(frags: &[TextFragment], hit: TextHit) -> Option<(TextHit, TextHit)> {
    let groups = line_groups(frags);
    let group = groups
        .iter()
        .find(|group| group.contains(&hit.fragment))?;
    let start = TextHit {
        fragment: group[0],
        offset: 0,
    };
    let last = *group.last()?;
    let end = TextHit {
        fragment: last,
        offset: hit_len(&frags[last]),
    };
    Some((start, end))
}

/// Plages `(fragment, début, fin)` couvertes par une sélection, caractère
/// exclu à la fin.
pub fn hit_runs(
    frags: &[TextFragment],
    span: Option<(TextHit, TextHit)>,
) -> Vec<(usize, usize, usize)> {
    let order = reading_order(frags);
    let full = |fragment: usize| (fragment, 0, hit_len(&frags[fragment]));
    let Some((anchor, head)) = span else {
        return order.iter().map(|index| full(*index)).collect();
    };
    let rank = |hit: TextHit| {
        order
            .iter()
            .position(|index| *index == hit.fragment)
            .unwrap_or(0)
    };
    let (mut start, mut end) = (anchor, head);
    if rank(start) > rank(end) || (rank(start) == rank(end) && start.offset > end.offset) {
        std::mem::swap(&mut start, &mut end);
    }
    let first = rank(start);
    let last = rank(end);
    let mut runs = Vec::new();
    for (position, &fragment) in order.iter().enumerate().take(last + 1).skip(first) {
        let length = hit_len(&frags[fragment]);
        let from = if position == first { start.offset } else { 0 }.min(length);
        let to = if position == last { end.offset } else { length }.min(length);
        if from < to {
            runs.push((fragment, from, to));
        }
    }
    runs
}

/// Texte d'une sélection au caractère près.
pub fn hit_selection_text(frags: &[TextFragment], span: Option<(TextHit, TextHit)>) -> String {
    let mut out = String::new();
    let mut previous_line: Option<f32> = None;
    for (fragment, start, end) in hit_runs(frags, span) {
        let frag = &frags[fragment];
        let line = (frag.rect[1] + frag.rect[3]) * 0.5;
        if let Some(previous) = previous_line {
            let height = frag.font_size.max(1.0);
            out.push(if (previous - line).abs() > height * 0.6 {
                '\n'
            } else {
                ' '
            });
        }
        let text: String = frag.text.chars().skip(start).take(end - start).collect();
        out.push_str(&text);
        previous_line = Some(line);
    }
    out
}

/// Rectangles d'une sélection, un par ligne. Les caractères d'une même ligne
/// sont réunis, y compris quand la sélection traverse deux fragments.
pub fn hit_rects(frags: &[TextFragment], span: Option<(TextHit, TextHit)>) -> Vec<[f32; 4]> {
    let mut lines: Vec<[f32; 4]> = Vec::new();
    for (fragment, start, end) in hit_runs(frags, span) {
        let frag = &frags[fragment];
        let height = frag.font_size.max(1.0);
        for index in start..end {
            let rect = char_box(frag, index);
            let center = (rect[1] + rect[3]) * 0.5;
            match lines.iter_mut().find(|line| {
                let line_center = (line[1] + line[3]) * 0.5;
                (line_center - center).abs() <= height * 0.6
            }) {
                Some(line) => {
                    line[0] = line[0].min(rect[0]);
                    line[1] = line[1].min(rect[1]);
                    line[2] = line[2].max(rect[2]);
                    line[3] = line[3].max(rect[3]);
                }
                None => lines.push(rect),
            }
        }
    }
    lines
}

/// Quadrilatères prêts pour `/QuadPoints` à partir de rectangles de lignes.
pub fn quads_from_rects(lines: &[[f32; 4]]) -> Vec<[f32; 8]> {
    lines
        .iter()
        .map(|line| {
            [
                line[0], line[3], line[2], line[3], line[0], line[1], line[2], line[1],
            ]
        })
        .collect()
}

/// Quadrilatères d'une sélection au caractère près.
pub fn hit_quads(frags: &[TextFragment], span: Option<(TextHit, TextHit)>) -> Vec<[f32; 8]> {
    quads_from_rects(&hit_rects(frags, span))
}

/// Ordre de lecture des blocs d'une page.
///
/// Deux passes plutôt qu'un comparateur : un comparateur « même ligne ? » n'est
/// pas transitif dès que les corps de police diffèrent (a ≈ b, b ≈ c mais
/// a < c), et un tri sur un ordre non transitif rend un résultat non
/// spécifié. On trie donc par ordonnée décroissante, puis on regroupe les
/// blocs d'une même ligne et on les trie de gauche à droite. Chaque groupe est
/// fermé, l'ordre est donc déterministe.
pub fn reading_order(frags: &[TextFragment]) -> Vec<usize> {
    line_groups(frags).into_iter().flatten().collect()
}

/// Répartit les blocs en lignes visuelles, de haut en bas.
///
/// Chaque groupe contient les fragments d'une même ligne de base, triés de
/// gauche à droite. Le regroupement est le même que celui du tri global, mais
/// isolé : c'est lui qui dit où commence et où finit une ligne.
pub fn line_groups(frags: &[TextFragment]) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..frags.len()).collect();
    order.sort_by(|a, b| {
        line_center(&frags[*b])
            .partial_cmp(&line_center(&frags[*a]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut groups: Vec<Vec<usize>> = Vec::with_capacity(order.len());
    let mut start = 0;
    while start < order.len() {
        let reference = line_center(&frags[order[start]]);
        let tolerance = frags[order[start]].font_size.max(1.0) * 0.6;
        let mut end = start + 1;
        while end < order.len() {
            let size = frags[order[end]].font_size.max(1.0);
            let gap = (line_center(&frags[order[end]]) - reference).abs();
            if gap <= tolerance.max(size * 0.6) {
                end += 1;
            } else {
                break;
            }
        }
        let mut row: Vec<usize> = order[start..end].to_vec();
        row.sort_by(|a, b| {
            frags[*a].rect[0]
                .partial_cmp(&frags[*b].rect[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        groups.push(row);
        start = end;
    }
    groups
}

/// Ordonnée du centre d'un bloc, en repère affiché.
pub fn line_center(frag: &TextFragment) -> f32 {
    (frag.rect[1] + frag.rect[3]) * 0.5
}
