//! Rognage d'une image posée : fractions dans le repère de l'image.

use crate::edit::ImageCrop;

/// Fractions de rognage d'un rectangle, exprimées dans le repère de l'image.
///
/// La matrice `[a b c d e f]` envoie le carré unité de l'image sur la page :
/// `(a, b)` est son axe horizontal, `(c, d)` son axe vertical, `(e, f)` son
/// coin bas-gauche. On inverse cette matrice sur les quatre coins du rectangle
/// à conserver, ce qui reste exact pour une image tournée par quarts de tour,
/// et approché pour une image au dessin incliné.
pub fn crop_fractions_in_image(matrix: [f32; 6], rect: [f32; 4]) -> ImageCrop {
    let [a, b, c, d, e, f] = matrix;
    let determinant = a * d - b * c;
    if determinant.abs() < 1e-6 {
        return ImageCrop::default();
    }
    let mut min_x = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    for corner_x in [rect[0], rect[2]] {
        for corner_y in [rect[1], rect[3]] {
            let dx = corner_x - e;
            let dy = corner_y - f;
            let u = (d * dx - c * dy) / determinant;
            let v = (a * dy - b * dx) / determinant;
            min_x = min_x.min(u);
            max_x = max_x.max(u);
            min_y = min_y.min(v);
            max_y = max_y.max(v);
        }
    }
    ImageCrop {
        left: min_x.clamp(0.0, 1.0),
        right: (1.0 - max_x).clamp(0.0, 1.0),
        bottom: min_y.clamp(0.0, 1.0),
        top: (1.0 - max_y).clamp(0.0, 1.0),
    }
}

/// Deux rognages diffèrent-ils de plus d'un millième ?
///
/// Une poignée survolée est replacée sous le pointeur : sans ce seuil, la
/// simple approche du curseur déclencherait une écriture.
pub fn response_changed(previous: ImageCrop, next: ImageCrop) -> bool {
    let near = |a: f32, b: f32| (a - b).abs() > 0.001;
    near(previous.left, next.left)
        || near(previous.top, next.top)
        || near(previous.right, next.right)
        || near(previous.bottom, next.bottom)
}


#[cfg(test)]
mod crop_tests {
    use super::crop_fractions_in_image;

/// Une image droite : le rognage affiché est aussi le rognage de l'image.
#[test]
fn crop_fractions_follow_an_upright_image() {
    // Boîte de 200×200 posée en (150, 300), pixels vers le haut.
    let crop = crop_fractions_in_image(
        [200.0, 0.0, 0.0, 200.0, 150.0, 300.0],
        [200.0, 400.0, 350.0, 500.0],
    );
    // Le rectangle gardé va de x=200 à 350 : 25 % de la largeur part à
    // gauche, rien à droite. Il va de y=400 à 500 : la moitié basse de
    // l'image s'en va.
    assert!((crop.left - 0.25).abs() < 1e-4, "{crop:?}");
    assert!(crop.right.abs() < 1e-4, "{crop:?}");
    assert!((crop.bottom - 0.5).abs() < 1e-4, "{crop:?}");
    assert!(crop.top.abs() < 1e-4, "{crop:?}");
}

/// Image tournée d'un quart de tour horaire : l'axe horizontal affiché est
/// l'axe vertical de l'image, dans l'autre sens.
#[test]
fn crop_fractions_follow_a_quarter_turned_image() {
    // Matrice produite par `RotateCw` : `(a, b) = (0, -200)` descend,
    // `(c, d) = (200, 0)` va vers la droite.
    let matrix = [0.0, -200.0, 200.0, 0.0, 150.0, 500.0];
    // On garde la moitié haute de l'affichage.
    let crop = crop_fractions_in_image(matrix, [150.0, 400.0, 350.0, 500.0]);
    assert!(crop.left.abs() < 1e-4, "{crop:?}");
    assert!((crop.right - 0.5).abs() < 1e-4, "{crop:?}");
    assert!(crop.top.abs() < 1e-4 && crop.bottom.abs() < 1e-4, "{crop:?}");
    // Et la moitié basse donne l'autre moitié de l'image.
    let crop = crop_fractions_in_image(matrix, [150.0, 300.0, 350.0, 400.0]);
    assert!((crop.left - 0.5).abs() < 1e-4, "{crop:?}");
    assert!(crop.right.abs() < 1e-4, "{crop:?}");
}
}
