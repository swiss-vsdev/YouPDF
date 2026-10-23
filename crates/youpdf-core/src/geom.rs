//! Conversion de rectangles et points entre repères page et affiché.

use crate::document::Document;

/// Recadre un rectangle du repère affiché vers le repère page.
pub fn rect_to_page(doc: &Document, page: usize, rect: [f32; 4]) -> [f32; 4] {
    doc.page_view(page as u32).page_rect(rect)
}

/// Recadre un point du repère affiché vers le repère page.
pub fn point_to_page(doc: &Document, page: usize, point: [f32; 2]) -> [f32; 2] {
    doc.page_view(page as u32).page_point(point)
}

/// Recadre un rectangle du repère page vers le repère affiché.
pub fn rect_to_display(doc: &Document, page: usize, rect: [f32; 4]) -> [f32; 4] {
    doc.page_view(page as u32).display_rect(rect)
}