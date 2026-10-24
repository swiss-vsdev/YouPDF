//! Interpréteur de flux de contenu : géométrie des opérateurs.
//!
//! Le moteur de caviardage a besoin de savoir **où** chaque instruction
//! dessine, afin de supprimer celles qui tombent dans une zone à effacer.
//! Ce module parcourt donc le flux de contenu en maintenant l'état
//! graphique (CTM, épaisseur de trait) et l'état de texte (matrices, corps,
//! espacements), puis renvoie la boîte englobante de ce qui est réellement
//! peint.
//!
//! Repère : tout est exprimé en **repère page** (espace utilisateur du PDF,
//! origine en bas à gauche du MediaBox, `/Rotate` non appliqué). C'est au
//! appelant de convertir les rectangles affichés par l'utilisateur avant
//! de les comparer aux boîtes produites ici.

use lopdf::Object;

/// Matrice 2×3 `[a b c d e f]` au format PDF.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Matrix {
    pub const IDENTITY: Matrix = Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// Produit matriciel `self × other` : `self` s'applique d'abord.
    ///
    /// C'est la convention du PDF : `cm` multiplie par la gauche, d'où
    /// `ctm = m.then(ctm)` — l'opérant est appliqué avant la matrice déjà
    /// en vigueur.
    pub fn then(self, other: Matrix) -> Matrix {
        Matrix {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    /// Transforme un point par la matrice.
    pub fn apply(self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }

    /// Longueur de l'axe X transformé (facteur d'échelle horizontal).
    pub fn scale_x(self) -> f32 {
        (self.a * self.a + self.b * self.b).sqrt()
    }

    /// Longueur de l'axe Y transformé (facteur d'échelle vertical).
    pub fn scale_y(self) -> f32 {
        (self.c * self.c + self.d * self.d).sqrt()
    }

    /// Matrice inverse, `None` si elle n'est pas inversible.
    ///
    /// Utilisée par l'appelant pour ramener un point en repère utilisateur.
    #[allow(dead_code)]
    pub fn inverse(self) -> Option<Matrix> {
        let det = self.a * self.d - self.b * self.c;
        if !det.is_finite() || det.abs() < 1e-9 {
            return None;
        }
        let inv = 1.0 / det;
        let a = self.d * inv;
        let b = -self.b * inv;
        let c = -self.c * inv;
        let d = self.a * inv;
        Some(Matrix {
            a,
            b,
            c,
            d,
            e: -(a * self.e + c * self.f),
            f: -(b * self.e + d * self.f),
        })
    }
}

/// Boîte englobante dans le repère page, `[x1, y1, x2, y2]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl BBox {
    pub const EMPTY: BBox = BBox {
        x1: f32::INFINITY,
        y1: f32::INFINITY,
        x2: f32::NEG_INFINITY,
        y2: f32::NEG_INFINITY,
    };

    /// Boîte dégénérée réduite à un point.
    #[allow(dead_code)]
    pub fn from_point(x: f32, y: f32) -> Self {
        BBox {
            x1: x,
            y1: y,
            x2: x,
            y2: y,
        }
    }

    /// Boîte depuis un rectangle `[x1, y1, x2, y2]`.
    ///
    /// Les coordonnées sont normalisées (`x1 <= x2`, `y1 <= y2`) : le
    /// rectangle peut venir d'un `/Rect` dont les coins sont dans le désordre.
    pub fn from_rect(rect: [f32; 4]) -> Self {
        let (x1, y1, x2, y2) = (rect[0], rect[1], rect[2], rect[3]);
        BBox {
            x1: x1.min(x2),
            y1: y1.min(y2),
            x2: x1.max(x2),
            y2: y1.max(y2),
        }
    }

    /// La boîte ne contient-elle aucune surface exploitable ?
    pub fn is_empty(&self) -> bool {
        if !(self.x1.is_finite()
            && self.y1.is_finite()
            && self.x2.is_finite()
            && self.y2.is_finite())
        {
            return true;
        }
        self.x1 > self.x2 || self.y1 > self.y2
    }

    /// Agrandit la boîte pour inclure le point donné.
    pub fn extend(&mut self, x: f32, y: f32) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        self.x1 = self.x1.min(x);
        self.y1 = self.y1.min(y);
        self.x2 = self.x2.max(x);
        self.y2 = self.y2.max(y);
    }

    /// Dilate la boîte d'une marge sur les quatre côtés.
    pub fn inflate(&mut self, margin: f32) {
        if self.is_empty() {
            return;
        }
        self.x1 -= margin;
        self.y1 -= margin;
        self.x2 += margin;
        self.y2 += margin;
    }

    /// La boîte est-elle entièrement contenue dans `other` ?
    pub fn inside(&self, other: &BBox) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x1 >= other.x1 && self.y1 >= other.y1 && self.x2 <= other.x2 && self.y2 <= other.y2
    }

    /// La boîte chevauche-t-elle `other` ?
    ///
    /// Le contact simple (bord contre bord) ne compte **pas** : seul un
    /// recouvrement d'aire strictement positive est un chevauchement, sinon
    /// des formes jointives seraient supprimées à tort.
    pub fn overlaps(&self, other: &BBox) -> bool {
        if self.is_empty() || other.is_empty() {
            return false;
        }
        self.x1 < other.x2 && other.x1 < self.x2 && self.y1 < other.y2 && other.y1 < self.y2
    }

    /// La boîte sous forme de rectangle `[x1, y1, x2, y2]`.
    #[allow(dead_code)]
    pub fn as_rect(&self) -> [f32; 4] {
        [self.x1, self.y1, self.x2, self.y2]
    }
}
/// Nature de ce qu'une instruction dessine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Text,
    Path,
    Image,
    /// Réservé aux instructions qui dessinent sans être localisables.
    #[allow(dead_code)]
    Other,
}

/// Résultat de l'analyse d'une instruction.
#[derive(Debug, Clone)]
pub struct OpGeometry {
    pub kind: OpKind,
    pub bbox: BBox,
    pub filled: bool,
    pub stroked: bool,
    /// Vrai quand le tracé est un rectangle plein, donc découpable.
    #[allow(dead_code)]
    pub plain_rect: bool,
}

/// État graphique suivi au fil du flux.
#[derive(Debug, Clone)]
pub struct GraphicsState {
    pub ctm: Matrix,
    pub line_width: f32,
}

impl Default for GraphicsState {
    fn default() -> Self {
        Self {
            ctm: Matrix::IDENTITY,
            line_width: 1.0,
        }
    }
}

/// État de texte suivi au fil du flux.
#[derive(Debug, Clone)]
pub struct TextState {
    pub matrix: Matrix,
    pub line_matrix: Matrix,
    pub size: f32,
    pub hscale: f32,
    pub vscale: f32,
    pub char_spacing: f32,
    pub word_spacing: f32,
    pub leading: f32,
    pub font: Option<String>,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            matrix: Matrix::IDENTITY,
            line_matrix: Matrix::IDENTITY,
            size: 12.0,
            hscale: 1.0,
            vscale: 1.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            leading: 0.0,
            font: None,
        }
    }
}
/// Interpréteur de flux de contenu.
pub struct Interpreter {
    pub stack: Vec<GraphicsState>,
    pub state: GraphicsState,
    pub text: TextState,
    pub path: Vec<(f32, f32)>,
    pub current: (f32, f32),
    pub path_open: bool,
    /// Dernier rectangle construit par `re`, en repère utilisateur (avant
    /// CTM). Conservé pour permettre à l'appelant de découper un
    /// rectangle plein en morceaux plus petits.
    pub last_rect: Option<[f32; 4]>,
}

impl Interpreter {
    pub fn new() -> Self {
        Interpreter {
            stack: Vec::new(),
            state: GraphicsState::default(),
            text: TextState::default(),
            path: Vec::new(),
            current: (0.0, 0.0),
            path_open: false,
            last_rect: None,
        }
    }

    /// CTM courante.
    #[allow(dead_code)]
    pub fn ctm(&self) -> Matrix {
        self.state.ctm
    }

    /// Matrice de texte effective : matrice de texte × CTM.
    #[allow(dead_code)]
    pub fn text_matrix(&self) -> Matrix {
        self.text.matrix.then(self.state.ctm)
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

/// Largeur moyenne d'un caractère, en millièmes de cadratin.
///
/// Les largeurs exactes des polices standard-14 ne sont **pas** dans le
/// fichier PDF : il faudrait le tableau `/Widths` de la police, que lopdf
/// n'expose pas de façon utilisable ici. On se rabat donc sur une
/// estimation fondée sur le nom de la ressource (le nom de police est
/// utilisé comme substitut du `/BaseFont`).
pub fn average_width(base_font: &str) -> f32 {
    let lower = base_font.to_ascii_lowercase();
    if lower.contains("courier") || lower.contains("mono") {
        0.6
    } else if lower.contains("times") || lower.contains("serif") {
        0.46
    } else {
        0.52
    }
}

/// Largeur estimée d'une chaîne, en points de texte.
pub fn estimate_width(text: &str, size: f32, base_font: &str) -> f32 {
    let count = text.chars().count();
    count as f32 * average_width(base_font) * size
}

/// Traduit une instruction `cm` en matrice.
pub fn matrix_of(operands: &[Object]) -> Option<Matrix> {
    if operands.len() < 6 {
        return None;
    }
    let mut values = [0.0f32; 6];
    for (slot, operand) in values.iter_mut().zip(operands.iter()) {
        *slot = number_of(operand)?;
    }
    Some(Matrix {
        a: values[0],
        b: values[1],
        c: values[2],
        d: values[3],
        e: values[4],
        f: values[5],
    })
}

/// Matrice de translation.
pub fn translation(tx: f32, ty: f32) -> Matrix {
    Matrix {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: tx,
        f: ty,
    }
}

/// Lit un opérande numérique (`Real` ou `Integer`).
pub fn number_of(object: &Object) -> Option<f32> {
    match object {
        Object::Real(v) => Some(*v),
        Object::Integer(v) => Some(*v as f32),
        _ => None,
    }
}
mod interp;
pub use interp::KNOWN_OPERATORS;

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::Operation;

    #[test]
    fn produit_de_matrices_ordre_pdf() {
        // Une translation suivie d'une mise à l'échelle.
        let t = translation(10.0, 20.0);
        let s = Matrix {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 2.0,
            e: 0.0,
            f: 0.0,
        };
        // `s.then(t)` applique l'échelle d'abord, puis la translation.
        let m = s.then(t);
        assert_eq!(m.apply(1.0, 1.0), (12.0, 22.0));
        // `t.then(s)` applique la translation d'abord : l'échelle double la
        // translation.
        let m2 = t.then(s);
        assert_eq!(m2.apply(1.0, 1.0), (22.0, 42.0));
        assert_eq!(m2.e, 20.0);
    }

    #[test]
    fn inverse_et_aller_retour() {
        let m = Matrix {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 4.0,
            e: 5.0,
            f: 6.0,
        };
        let inv = m.inverse();
        assert!(inv.is_some());
        if let Some(inv) = inv {
            let p = m.apply(3.0, 7.0);
            let q = inv.apply(p.0, p.1);
            assert!((q.0 - 3.0).abs() < 1e-4);
            assert!((q.1 - 7.0).abs() < 1e-4);
        }
        // Matrice dégénérée : pas d'inverse.
        let singular = Matrix {
            a: 0.0,
            b: 0.0,
            c: 0.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        };
        assert!(singular.inverse().is_none());
    }

    #[test]
    fn bbox_normalise_et_chevauchement() {
        let b = BBox::from_rect([10.0, 20.0, 0.0, 5.0]);
        assert_eq!(b.as_rect(), [0.0, 5.0, 10.0, 20.0]);
        assert!(!b.is_empty());
        // Simple contact de bord : ce n'est *pas* un chevauchement.
        let other = BBox::from_rect([10.0, 5.0, 20.0, 20.0]);
        assert!(!b.overlaps(&other));
        // Recouvrement partiel : là, oui.
        let other = BBox::from_rect([9.0, 5.0, 20.0, 20.0]);
        assert!(b.overlaps(&other));
        assert!(BBox::from_rect([1.0, 6.0, 2.0, 7.0]).inside(&b));
        assert!(!b.inside(&other));
    }

    #[test]
    fn largeurs_estimees() {
        assert!((average_width("CourierNew") - 0.6).abs() < 1e-6);
        assert!((average_width("Times-Roman") - 0.46).abs() < 1e-6);
        assert!((average_width("Helvetica") - 0.52).abs() < 1e-6);
        let w = estimate_width("abc", 10.0, "Helvetica");
        assert!((w - 3.0 * 0.52 * 10.0).abs() < 1e-4);
    }

    #[test]
    fn texte_et_chemin_renseignent_leur_boite() {
        let mut interp = Interpreter::new();
        let tj = Operation::new(
            "Tj",
            vec![Object::String(
                b"Hello".to_vec(),
                lopdf::StringFormat::Literal,
            )],
        );
        let geometry = interp.interpret(&tj);
        assert!(geometry.is_some());
        if let Some(g) = geometry {
            assert_eq!(g.kind, OpKind::Text);
            assert!(g.filled);
            assert!(!g.plain_rect);
            // 5 caractères à 0.52 em, corps 12 : environ 31 points de large.
            assert!(g.bbox.x2 - g.bbox.x1 > 30.0);
        }

        let mut interp = Interpreter::new();
        interp.interpret(&Operation::new(
            "re",
            vec![
                Object::Integer(10),
                Object::Integer(20),
                Object::Integer(30),
                Object::Integer(40),
            ],
        ));
        assert_eq!(interp.last_rect, Some([10.0, 20.0, 40.0, 60.0]));
        let painted = interp.interpret(&Operation::new("f", vec![]));
        assert!(painted.is_some());
        if let Some(g) = painted {
            assert_eq!(g.kind, OpKind::Path);
            assert!(g.filled);
            assert!(!g.stroked);
            assert_eq!(g.bbox.as_rect(), [10.0, 20.0, 40.0, 60.0]);
        }
    }

    #[test]
    fn cm_presmultiplie_la_ctm() {
        let mut interp = Interpreter::new();
        interp.interpret(&Operation::new(
            "cm",
            vec![
                Object::Integer(2),
                Object::Integer(0),
                Object::Integer(0),
                Object::Integer(2),
                Object::Integer(10),
                Object::Integer(20),
            ],
        ));
        let p = interp.ctm().apply(1.0, 1.0);
        assert!((p.0 - 12.0).abs() < 1e-4);
        assert!((p.1 - 22.0).abs() < 1e-4);
    }
}
