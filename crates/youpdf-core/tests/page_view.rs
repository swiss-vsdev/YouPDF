//! Conversions entre le repère affiché et le repère page.
//!
//! L'interface dessine en repère affiché (rotation et boîte visible
//! appliquées) ; le PDF écrit en repère page. Une erreur de conversion pose
//! une annotation, une image ou un champ au mauvais endroit dès que la page
//! est tournée ou recadrée.

use youpdf_core::document::PageView;

fn view(x: f32, y: f32, width: f32, height: f32, rotation: i32) -> PageView {
    PageView {
        x,
        y,
        width,
        height,
        rotation,
    }
}

fn assert_close(got: [f32; 4], want: [f32; 4]) {
    for i in 0..4 {
        assert!(
            (got[i] - want[i]).abs() < 0.01,
            "composante {i} : {got:?} != {want:?}"
        );
    }
}

fn assert_close2(got: [f32; 2], want: [f32; 2]) {
    for i in 0..2 {
        assert!(
            (got[i] - want[i]).abs() < 0.01,
            "composante {i} : {got:?} != {want:?}"
        );
    }
}

/// Sans rotation ni recadrage, les deux repères coïncident.
#[test]
fn identite_sans_rotation() {
    let v = view(0.0, 0.0, 612.0, 792.0, 0);
    let rect = [10.0, 20.0, 110.0, 70.0];
    assert_close(v.display_rect(rect), rect);
    assert_close(v.page_rect(rect), rect);
    assert_close2(v.page_point([10.0, 20.0]), [10.0, 20.0]);
    assert_close2(v.display_point([10.0, 20.0]), [10.0, 20.0]);
}

/// L'aller-retour revient au point de départ, pour les quatre rotations.
#[test]
fn aller_retour_sur_les_quatre_rotations() {
    for rotation in [0, 90, 180, 270] {
        let v = view(0.0, 0.0, 600.0, 800.0, rotation);
        let rect = [50.0, 100.0, 250.0, 300.0];
        let round = v.page_rect(v.display_rect(rect));
        assert_close(round, rect);
        let point = [123.0, 456.0];
        let round = v.page_point(v.display_point(point));
        assert!((round[0] - point[0]).abs() < 0.01);
        assert!((round[1] - point[1]).abs() < 0.01);
    }
}

/// Une page tournée à 90° échange largeur et hauteur à l'affichage, et le coin
/// bas-gauche du repère page part en haut à gauche de l'affichage.
#[test]
fn rotation_quatre_vingt_dix() {
    let v = view(0.0, 0.0, 600.0, 800.0, 90);
    assert_eq!(v.display_size(), (800.0, 600.0));
    let corner = v.display_point([0.0, 0.0]);
    assert_close2(corner, [0.0, 600.0]);
    // Et l'inverse ramène bien au coin d'origine.
    assert_close2(v.page_point([0.0, 600.0]), [0.0, 0.0]);
}

/// Le recadrage décale : un point du repère page hors de la boîte visible est
/// ramené sur le bord par la fraction bornée.
#[test]
fn recadrage_borne_la_fraction() {
    let v = view(100.0, 100.0, 200.0, 200.0, 0);
    let outside = v.display_point([0.0, 0.0]);
    assert!(outside[0].abs() < 0.01, "{outside:?}");
    assert!(outside[1].abs() < 0.01, "{outside:?}");
    let inside = v.page_point([50.0, 50.0]);
    assert!((inside[0] - 150.0).abs() < 0.01, "{inside:?}");
    assert!((inside[1] - 150.0).abs() < 0.01, "{inside:?}");
}
