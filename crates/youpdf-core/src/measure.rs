//! Outils de mesure : distance, périmètre, aire.
//!
//! ## Approche retenue
//!
//! Comme le filigrane de [`crate::watermark`], la mesure est écrite
//! **dans le flux de contenu** de la page : elle s'imprime, se convertit en
//! image et reste visible dans n'importe quel lecteur. Une annotation ne
//! donnerait qu'un objet décrit au `/Annots`, que certains lecteurs
//! n'affichent pas.
//!
//! Chaque mesure reçoit un flux de contenu supplémentaire, posé à la fin de
//! `/Contents` et marqué par la clé `/YouPdfMeasure` : c'est ce marquage qui
//! permet de retirer ensuite toutes les mesures avec
//! [`Document::clear_measurements`], sans toucher aux autres décorations.
//!
//! La police du libellé est la ressource `/YouPdfF1` (Helvetica
//! /WinAnsiEncoding) posée par [`crate::watermark`], partagée avec les
//! filigranes et les numéros de page.

use lopdf::{Dictionary, Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::encode_pdf_literal;

/// Clé marquant un flux de contenu posé par ce module.
const MEASURE_MARKER: &str = "YouPdfMeasure";

/// Nom de ressource de la police posée par le module filigrane.
const FONT_NAME: &str = "YouPdfF1";

/// Couleur d'accent fixe des tracés de mesure (RVB 0–1).
const ACCENT: [f32; 3] = [0.10, 0.45, 0.90];

/// Demi-côté des marqueurs carrés d'extrémité, en points.
const MARKER_HALF: f32 = 2.0;

/// Taille de police du libellé, en points.
const LABEL_SIZE: f32 = 9.0;

/// Nature d'une mesure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasureKind {
    /// Longueur d'un segment.
    Distance,
    /// Longueur d'un chemin ouvert.
    Perimeter,
    /// Aire d'un polygone fermé.
    Area,
}

impl MeasureKind {
    /// Nom affiché.
    pub fn display_name(self) -> &'static str {
        match self {
            MeasureKind::Distance => "Distance",
            MeasureKind::Perimeter => "Perimeter",
            MeasureKind::Area => "Area",
        }
    }

    /// Nombre minimal de points pour que la mesure ait un sens.
    fn min_points(self) -> usize {
        match self {
            MeasureKind::Distance | MeasureKind::Perimeter => 2,
            MeasureKind::Area => 3,
        }
    }
}

/// Mesure posée sur une page.
#[derive(Debug, Clone)]
pub struct Measurement {
    pub kind: MeasureKind,
    /// Points du tracé, dans le repère de la page.
    pub points: Vec<[f32; 2]>,
    /// Échelle : unités de dessin par unité réelle.
    pub scale: f32,
    /// Unité affichée (`mm`, `in`, `pt`…).
    pub unit: String,
}

/// Longueur d'un segment.
fn seg_len(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

/// Valeur de la mesure, en unités de dessin.
fn measure_value(kind: MeasureKind, points: &[[f32; 2]]) -> f32 {
    match kind {
        MeasureKind::Distance => seg_len(points[0], points[1]),
        MeasureKind::Perimeter => points.windows(2).map(|w| seg_len(w[0], w[1])).sum(),
        MeasureKind::Area => {
            let n = points.len();
            let mut acc = 0.0_f32;
            for (i, p) in points.iter().enumerate() {
                let q = points[(i + 1) % n];
                acc += p[0] * q[1] - q[0] * p[1];
            }
            (acc / 2.0).abs()
        }
    }
}

/// Libellé affiché : valeur réelle (dessin / échelle, dessin / échelle² pour
/// une aire), un chiffre après la virgule, suivi de l'unité. Exemple : `12.3 mm`.
fn measure_label(m: &Measurement) -> String {
    let scale = if m.scale > 0.0 { m.scale } else { 1.0 };
    // Une aire est en unités de dessin au carré : elle se convertit par le
    // carré de l'échelle, sans quoi elle est annoncée `scale` fois trop grande.
    let divisor = if m.kind == MeasureKind::Area {
        scale * scale
    } else {
        scale
    };
    let value = measure_value(m.kind, &m.points) / divisor;
    format!("{value:.1} {}", m.unit)
}

/// Flux de contenu d'une mesure : tracé, marqueurs carrés et libellé.
fn measure_ops(m: &Measurement, label: &str) -> Vec<u8> {
    let [r, g, b] = ACCENT;
    let mut out = Vec::new();
    out.extend_from_slice(b"q\n");

    // Tracé : segment, polyligne ouverte, ou polygone fermé.
    out.extend_from_slice(format!("{r:.3} {g:.3} {b:.3} RG\n1.5 w\n").as_bytes());
    if let Some(first) = m.points.first() {
        out.extend_from_slice(format!("{:.2} {:.2} m\n", first[0], first[1]).as_bytes());
        for p in &m.points[1..] {
            out.extend_from_slice(format!("{:.2} {:.2} l\n", p[0], p[1]).as_bytes());
        }
        if m.kind == MeasureKind::Area {
            out.extend_from_slice(b"h\n");
        }
        out.extend_from_slice(b"S\n");
    }

    // Marqueurs carrés pleins à chaque sommet.
    out.extend_from_slice(format!("{r:.3} {g:.3} {b:.3} rg\n").as_bytes());
    for p in &m.points {
        out.extend_from_slice(
            format!(
                "{:.2} {:.2} {:.2} {:.2} re\nf\n",
                p[0] - MARKER_HALF,
                p[1] - MARKER_HALF,
                MARKER_HALF * 2.0,
                MARKER_HALF * 2.0
            )
            .as_bytes(),
        );
    }

    // Libellé, posé près du barycentre du tracé.
    if !m.points.is_empty() {
        let n = m.points.len() as f32;
        let cx = m.points.iter().map(|p| p[0]).sum::<f32>() / n;
        let cy = m.points.iter().map(|p| p[1]).sum::<f32>() / n;
        out.extend_from_slice(
            format!(
                "BT\n/{FONT_NAME} {LABEL_SIZE:.2} Tf\n{r:.3} {g:.3} {b:.3} rg\n\
                 1 0 0 1 {:.2} {:.2} Tm\n",
                cx + MARKER_HALF + 2.0,
                cy + MARKER_HALF + 2.0
            )
            .as_bytes(),
        );
        out.push(b'(');
        out.extend_from_slice(&encode_pdf_literal(label));
        out.extend_from_slice(b") Tj\nET\n");
    }

    out.extend_from_slice(b"Q\n");
    out
}

impl Document {
    /// Ajoute une mesure à une page (1-indexée).
    ///
    /// Le tracé est écrit dans un flux de contenu marqué `/YouPdfMeasure`,
    /// posé à la fin de `/Contents`. La valeur affichée est
    /// `longueur / scale` (ou `aire / scale²` pour une aire) suivie de
    /// `unit`, arrondie à une décimale.
    pub fn add_measurement(&mut self, page: u32, measurement: &Measurement) -> Result<()> {
        if measurement.points.len() < measurement.kind.min_points() {
            return Err(Error::InvalidInput(format!(
                "{} : au moins {} points requis",
                measurement.kind.display_name(),
                measurement.kind.min_points()
            )));
        }
        if page == 0 || page as usize > self.page_count() {
            return Err(Error::InvalidInput(format!("page {page} hors bornes")));
        }
        let page_id = self
            .page_id(page)
            .map_err(|e| Error::InvalidInput(format!("page {page} : {e}")))?;

        let label = measure_label(measurement);
        let ops = measure_ops(measurement, &label);

        let mut dict = Dictionary::new();
        dict.set(MEASURE_MARKER, Object::Boolean(true));
        let stream_id = self.inner_mut().add_object(Stream::new(dict, ops));

        self.wm_ensure_font(page_id)?;
        self.wm_append_content(page_id, stream_id)?;

        tracing::debug!(page, kind = measurement.kind.display_name(), "mesure ajoutée");
        Ok(())
    }

    /// Retire toutes les mesures posées par YouPDF.
    ///
    /// Seuls les flux marqués `/YouPdfMeasure` sont retirés : le contenu
    /// d'origine de la page et les autres décorations restent en place.
    /// Renvoie le nombre de flux retirés.
    pub fn clear_measurements(&mut self) -> Result<usize> {
        self.wm_clear_marked(MEASURE_MARKER)
    }
}
