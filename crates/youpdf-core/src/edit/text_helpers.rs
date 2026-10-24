//! Aides internes à l'extraction : tokenisation du flux, accumulation des
//! fragments.

use crate::edit::TextFragment;

pub(crate) fn tokenize_content(content: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(content);
    let mut tokens = Vec::new();
    let mut chars = text.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }

        // Chaîne littérale (…)
        if c == '(' {
            chars.next();
            let mut depth = 1;
            let mut s = String::new();
            while let Some(&nc) = chars.peek() {
                chars.next();
                match nc {
                    '\\' => {
                        // Caractère d'échappement : on le consomme et on
                        // ajoute le suivant.
                        if let Some(&escaped) = chars.peek() {
                            chars.next();
                            match escaped {
                                'n' => s.push('\n'),
                                'r' => s.push('\r'),
                                't' => s.push('\t'),
                                other => s.push(other),
                            }
                        }
                    }
                    '(' => {
                        depth += 1;
                        s.push('(');
                    }
                    ')' => {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        s.push(')');
                    }
                    other => s.push(other),
                }
            }
            tokens.push(format!("({s})"));
            continue;
        }

        // Nom /X
        if c == '/' {
            chars.next();
            let mut s = String::from("/");
            while let Some(&nc) = chars.peek() {
                if nc.is_whitespace() || nc == '/' || nc == '(' || nc == '[' || nc == ']' {
                    break;
                }
                s.push(nc);
                chars.next();
            }
            tokens.push(s);
            continue;
        }

        // Délimiteur d'array : un jeton à part entière. Sans ce cas, le `[`
        // ou le `]` serait relu sans fin — la boucle extérieure le repère, la
        // boucle « Opérateur ou nombre » le rejette sans l'avancer.
        if c == '[' || c == ']' {
            chars.next();
            tokens.push(c.to_string());
            continue;
        }

        // Opérateur ou nombre
        let mut s = String::new();
        while let Some(&nc) = chars.peek() {
            if nc.is_whitespace() || nc == '(' || nc == '/' || nc == '[' || nc == ']' {
                break;
            }
            s.push(nc);
            chars.next();
        }
        if !s.is_empty() {
            tokens.push(s);
        }
    }

    tokens
}

/// Extrait le contenu d'un token de type `(…)` s'il y en a un.
pub(crate) fn extract_string_literal(token: &str) -> Option<String> {
    if token.starts_with('(') && token.ends_with(')') {
        Some(token[1..token.len() - 1].to_string())
    } else {
        None
    }
}

/// Longueur du dernier fragment poussé (pour l'avance horizontale).
pub(crate) fn current_text_len(fragments: &[TextFragment]) -> usize {
    fragments
        .last()
        .map(|f| f.text.chars().count())
        .unwrap_or(0)
}

/// Pousse ou accumule un fragment de texte.
#[allow(clippy::too_many_arguments)]
pub(crate) fn push_fragment(
    fragments: &mut Vec<TextFragment>,
    current: &mut Option<TextFragment>,
    text: String,
    page: usize,
    tx: f32,
    ty: f32,
    font_size: f32,
    font_name: Option<String>,
) {
    if text.trim().is_empty() {
        return;
    }

    match current {
        Some(frag) => {
            // On continue le fragment courant : le texte suit sur la
            // même ligne (même ty à la tolérance près).
            if (frag.rect[1] - ty).abs() < font_size * 0.3 {
                frag.text.push_str(&text);
                // On étend le rectangle.
                frag.rect[2] += font_size * 0.5 * text.chars().count() as f32;
            } else {
                fragments.push(frag.clone());
                *frag = new_fragment(text, page, tx, ty, font_size, font_name);
            }
        }
        None => {
            *current = Some(new_fragment(text, page, tx, ty, font_size, font_name));
        }
    }
}

pub(crate) fn new_fragment(
    text: String,
    page: usize,
    tx: f32,
    ty: f32,
    font_size: f32,
    font_name: Option<String>,
) -> TextFragment {
    let width = font_size * 0.5 * text.chars().count() as f32;
    TextFragment {
        page,
        chars: Vec::new(),
        text,
        rect: [tx, ty, tx + width, ty + font_size],
        font: font_name,
        font_size,
        color: [0, 0, 0],
    }
}
