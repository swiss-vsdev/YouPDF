//! Unités de longueur.
//!
//! Le PDF travaille en points (1/72 de pouce). L'interface, elle, laisse
//! l'utilisateur saisir des millimètres, des centimètres ou des pouces. Ce
//! module est la charnière : une valeur d'interface est toujours convertie
//! en points avant d'atteindre le moteur.

/// Une unité de longueur utilisable dans l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Unit {
    /// Point PDF (1/72 de pouce).
    #[default]
    Point,
    /// Millimètre.
    Millimeter,
    /// Centimètre.
    Centimeter,
    /// Pouce.
    Inch,
}

impl Unit {
    /// Toutes les unités proposées, dans l'ordre du sélecteur.
    pub const ALL: [Unit; 4] = [
        Unit::Point,
        Unit::Millimeter,
        Unit::Centimeter,
        Unit::Inch,
    ];

    /// Abréviation affichée dans l'interface.
    pub fn suffix(self) -> &'static str {
        match self {
            Unit::Point => "pt",
            Unit::Millimeter => "mm",
            Unit::Centimeter => "cm",
            Unit::Inch => "in",
        }
    }

    /// Nombre de points dans une unité.
    pub fn to_points(self, value: f32) -> f32 {
        match self {
            Unit::Point => value,
            Unit::Millimeter => value * 72.0 / 25.4,
            Unit::Centimeter => value * 72.0 / 2.54,
            Unit::Inch => value * 72.0,
        }
    }

    /// Convertit des points vers l'unité.
    pub fn from_points(self, points: f32) -> f32 {
        match self {
            Unit::Point => points,
            Unit::Millimeter => points * 25.4 / 72.0,
            Unit::Centimeter => points * 2.54 / 72.0,
            Unit::Inch => points / 72.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aller_retour_preserve_la_valeur() {
        for unit in Unit::ALL {
            for value in [0.0_f32, 1.0, 12.5, -3.0] {
                let back = unit.from_points(unit.to_points(value));
                assert!(
                    (back - value).abs() < 1e-3,
                    "{unit:?} {value} -> {back}"
                );
            }
        }
    }

    #[test]
    fn une_pouce_vaut_72_points() {
        assert!((Unit::Inch.to_points(1.0) - 72.0).abs() < 1e-4);
        assert!((Unit::Millimeter.to_points(25.4) - 72.0).abs() < 1e-3);
    }
}
