//! Accès aux ressources d'une page et réécriture de la matrice `cm` qui pose
//! une image. Le rognage y puise ses bornes : pixels et matrice partent des
//! mêmes fractions, sinon ils divergent d'un pixel.

use lopdf::Object;

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::{ImageCrop, ImageInfo, ImageTransform};

impl Document {
    /// Ressources d'une page, héritées de ses parents s'il n'y en a pas,
    /// références résolues.
    ///
    /// Une page sans `/Resources` emprunte ceux de l'arbre : la copie rendue
    /// doit donc remonter la chaîne, sinon l'écriture d'un état graphique
    /// ferait disparaître les polices du document.
    pub(crate) fn page_resources(&self, page_id: (u32, u16)) -> Result<lopdf::Dictionary> {
        let mut current = page_id;
        // Garde-fou : un arbre corrompu ne doit pas boucler.
        for _ in 0..64 {
            let page = self
                .inner()
                .get_dictionary(current)
                .map_err(|e| Error::Pdf(format!("page : {e}")))?;
            match page.get(b"Resources") {
                Ok(Object::Dictionary(dict)) => return Ok(dict.clone()),
                Ok(Object::Reference(id)) => {
                    return self
                        .inner()
                        .get_dictionary(*id)
                        .cloned()
                        .map_err(|e| Error::Pdf(format!("ressources : {e}")));
                }
                _ => {}
            }
            match page.get(b"Parent").ok().and_then(|parent| parent.as_reference().ok()) {
                Some(parent) => current = parent,
                None => break,
            }
        }
        Err(Error::Pdf(
            "arbre de pages cyclique : ressources introuvables".to_owned(),
        ))
    }

    /// Dictionnaire d'une entrée de ressources, en résolvant une éventuelle
    /// référence indirecte.
    pub(crate) fn resolve_dict(&self, value: Option<&Object>) -> Option<lopdf::Dictionary> {
        match value? {
            Object::Dictionary(dict) => Some(dict.clone()),
            Object::Reference(id) => self.inner().get_dictionary(*id).ok().cloned(),
            _ => None,
        }
    }

    /// Écrit les ressources d'une page.
    pub(crate) fn set_page_resources(
        &mut self,
        page_id: (u32, u16),
        resources: lopdf::Dictionary,
    ) -> Result<()> {
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let dict = page
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        dict.set("Resources", Object::Dictionary(resources));
        Ok(())
    }

    /// Boîte occupée par une image sur la page, en coordonnées page.
    ///
    /// La boîte rendue est le rectangle englobant du carré unité transformé
    /// par la matrice qui précède le `Do` : une image tournée donne donc une
    /// boîte droite.
    pub fn image_rect(&self, info: &ImageInfo) -> Result<Option<[f32; 4]>> {
        let Some([a, b, c, d, e, f]) = self.image_matrix(info)? else {
            return Ok(None);
        };
        let corners = [(e, f), (e + a, f + b), (e + c, f + d), (e + a + c, f + b + d)];
        let x1 = corners.iter().map(|p| p.0).fold(f32::INFINITY, f32::min);
        let y1 = corners.iter().map(|p| p.1).fold(f32::INFINITY, f32::min);
        let x2 = corners.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max);
        let y2 = corners.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max);
        Ok(Some([x1, y1, x2, y2]))
    }

    /// Matrice `cm` qui dessine une image sur sa page.
    ///
    /// `(a, b)` va de la gauche vers la droite de l'image, `(c, d)` du bas vers
    /// le haut, `(e, f)` est son coin bas-gauche. `None` quand l'image est
    /// dessinée sans matrice propre (héritée d'un `q` extérieur), cas que ce
    /// module ne sait pas situer.
    pub fn image_matrix(&self, info: &ImageInfo) -> Result<Option<[f32; 6]>> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = info.resource_name.trim_start_matches('/').as_bytes().to_vec();
        for content_id in self.page_content_ids(page_id)? {
            let Ok(Object::Stream(stream)) = self.inner().get_object(content_id) else {
                continue;
            };
            let Ok(bytes) = stream.get_plain_content() else {
                continue;
            };
            let Ok(content) = lopdf::content::Content::decode(&bytes) else {
                continue;
            };
            for (index, operation) in content.operations.iter().enumerate() {
                if operation.operator != "Do" {
                    continue;
                }
                if operation
                    .operands
                    .first()
                    .and_then(|object| object.as_name().ok())
                    != Some(name.as_slice())
                {
                    continue;
                }
                let Some(position) = preceding_cm(&content.operations, index) else {
                    continue;
                };
                if let Some(matrix) = matrix_of(&content.operations[position].operands) {
                    return Ok(Some(matrix));
                }
            }
        }
        Ok(None)
    }


    /// Identifiants des flux de contenu d'une page.
    pub(crate) fn page_content_ids(&self, page_id: (u32, u16)) -> Result<Vec<(u32, u16)>> {
        let page = self
            .inner()
            .get_dictionary(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let mut ids = Vec::new();
        match page.get(b"Contents").map_err(|e| Error::Pdf(format!("contenu : {e}")))? {
            Object::Reference(id) => ids.push(*id),
            Object::Array(items) => {
                for item in items {
                    if let Ok(id) = item.as_reference() {
                        ids.push(id);
                    }
                }
            }
            _ => {}
        }
        Ok(ids)
    }

    /// Applique `edit` aux opérateurs qui dessinent l'image `resource_name`.
    ///
    /// Le flux de contenu est décodé, les `Do` de l'image sont passés à
    /// `edit` dans l'ordre inverse (pour que les indices restent valables),
    /// puis le flux est réécrit en clair. Rend vrai si un dessin a été
    /// trouvé.
    pub(crate) fn edit_image_draw(
        &mut self,
        page_id: (u32, u16),
        resource_name: &str,
        mut edit: impl FnMut(&mut Vec<lopdf::content::Operation>, usize),
    ) -> Result<bool> {
        let targets = self.page_content_ids(page_id)?;
        let name = resource_name.trim_start_matches('/').as_bytes().to_vec();
        let mut found = false;
        for content_id in targets {
            let Ok(Object::Stream(stream)) = self.inner().get_object(content_id) else {
                continue;
            };
            // `decompressed_content` rend un flux vide quand il n'y a pas de
            // filtre : `get_plain_content` gère les deux cas.
            let Ok(bytes) = stream.get_plain_content() else {
                continue;
            };
            let Ok(mut content) = lopdf::content::Content::decode(&bytes) else {
                continue;
            };
            let mut indices: Vec<usize> = content
                .operations
                .iter()
                .enumerate()
                .filter(|(_, operation)| {
                    operation.operator == "Do"
                        && operation
                            .operands
                            .first()
                            .and_then(|object| object.as_name().ok())
                            == Some(name.as_slice())
                })
                .map(|(index, _)| index)
                .collect();
            if indices.is_empty() {
                continue;
            }
            // De la fin vers le début : insérer ou retirer un opérateur ne
            // décale pas les indices encore à traiter.
            indices.reverse();
            for index in indices {
                edit(&mut content.operations, index);
                found = true;
            }
            let encoded = content
                .encode()
                .map_err(|e| Error::Pdf(format!("flux de page : {e}")))?;
            let stream = self
                .inner_mut()
                .get_object_mut(content_id)
                .map_err(|e| Error::Pdf(format!("flux de page : {e}")))?
                .as_stream_mut()
                .map_err(|e| Error::Pdf(format!("flux de page : {e}")))?;
            stream.set_plain_content(encoded);
        }
        Ok(found)
    }

    /// Déplace le flux de dessin d'une image vers le début (back) ou la fin
    /// (front) du tableau `Contents` de la page. Chaque image est posée dans
    /// son propre flux par `insert_image_bitmap` : déplacer le flux entier
    /// change l'ordre d'affichage sans toucher les autres images. L'opération
    /// est exacte.
    pub fn set_image_z_order(&mut self, info: &ImageInfo, front: bool) -> Result<()> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = info.resource_name.clone();
        let name_bytes = name.trim_start_matches('/').as_bytes().to_vec();

        // Trouve le flux de contenu qui contient le `Do` de l'image.
        let targets = self.page_content_ids(page_id)?;
        let mut stream_id: Option<(u32, u16)> = None;
        for content_id in &targets {
            let Ok(Object::Stream(stream)) = self.inner().get_object(*content_id) else {
                continue;
            };
            let Ok(bytes) = stream.get_plain_content() else {
                continue;
            };
            let Ok(content) = lopdf::content::Content::decode(&bytes) else {
                continue;
            };
            let has_do = content.operations.iter().any(|op| {
                op.operator == "Do"
                    && op
                        .operands
                        .first()
                        .and_then(|obj| obj.as_name().ok())
                        == Some(name_bytes.as_slice())
            });
            if has_do {
                stream_id = Some(*content_id);
                break;
            }
        }
        let Some(stream_id) = stream_id else {
            return Err(Error::NotFound(format!("dessin de l'image \"{name}\"")));
        };

        // Lit le tableau `Contents` de la page.
        let page = self
            .inner_mut()
            .get_object_mut(page_id)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?
            .as_dict_mut()
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let contents_obj = page
            .get_mut(b"Contents")
            .map_err(|e| Error::Pdf(format!("contenu : {e}")))?;
        let contents = match contents_obj {
            Object::Array(arr) => arr,
            _ => return Err(Error::Pdf("page sans tableau Contents".to_owned())),
        };
        let position = contents.iter().position(|obj| {
            matches!(obj, Object::Reference(id) if *id == stream_id)
        });
        let Some(position) = position else {
            return Err(Error::Pdf(format!(
                "flux de l'image {name} absent du tableau Contents"
            )));
        };
        let entry = contents.remove(position);
        if front {
            contents.push(entry);
        } else {
            contents.insert(0, entry);
        }
        Ok(())
    }

    /// Pose la matrice qui dessine une image sur sa page.
    ///
    /// Sert au déplacement et au redimensionnement d'un bloc image : l'origine
    /// se déplace, les vecteurs s'allongent, mais les pixels ne sont jamais
    /// réencodés. L'opération est donc exacte et réversible, à la différence
    /// d'une réécriture par patch.
    pub fn set_image_matrix(&mut self, info: &ImageInfo, matrix: [f32; 6]) -> Result<()> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = info.resource_name.clone();
        let found = self.edit_image_draw(page_id, &name, |operations, index| {
            if let Some(position) = preceding_cm(operations, index) {
                operations[position].operands = matrix_operands(matrix);
            }
        })?;
        if !found {
            return Err(Error::NotFound(format!("matrice de l'image {name}")));
        }
        Ok(())
    }

    /// Tourne une image d'un angle libre, en place.
    ///
    /// L'angle est en degrés, dans le sens horaire à l'écran (comme les
    /// rotations de l'inspecteur). Seule la matrice qui dessine l'image
    /// change : les vecteurs de base tournent autour du centre de la boîte et
    /// l'origine se replace pour que le centre ne bouge pas. Les pixels ne
    /// sont jamais réencodés, donc l'opération est réversible et sans perte.
    pub fn rotate_image(&mut self, info: &ImageInfo, degrees: f32) -> Result<()> {
        let found = self.rotate_image_matrix(
            info.page,
            &info.resource_name,
            degrees.to_radians(),
        )?;
        if !found {
            return Err(Error::NotFound(format!(
                "matrice de l'image {}",
                info.resource_name
            )));
        }
        Ok(())
    }

    fn rotate_image_matrix(
        &mut self,
        page: usize,
        resource_name: &str,
        angle: f32,
    ) -> Result<bool> {
        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = resource_name.to_owned();
        self.edit_image_draw(page_id, &name, |operations, index| {
            let Some(position) = preceding_cm(operations, index) else {
                return;
            };
            let Some([a, b, c, d, e, f]) = matrix_of(&operations[position].operands) else {
                return;
            };
            // Le repère du dessin : `(a, b)` va de gauche à droite de l'image,
            // `(c, d)` du bas vers le haut. `e, f` est le coin bas-gauche.
            // Les deux vecteurs de base sont tournés d'un même angle, dans le
            // sens horaire à l'écran : à 90°, `(a, b) -> (b, -a)`, ce qui
            // reproduit exactement le cas `RotateCw`.
            let (s, cos) = angle.sin_cos();
            // Rotation horaire à l'écran : à 90°, `(a, b) -> (b, -a)`, ce qui
            // reproduit exactement le cas `RotateCw`.
            let xa = a * cos + b * s;
            let xb = -a * s + b * cos;
            let ya = c * cos + d * s;
            let yb = -c * s + d * cos;
            // Centre conservé : l'origine se déplace de la moitié de l'écart.
            let ne = e + 0.5 * (a + c - xa - ya);
            let nf = f + 0.5 * (b + d - xb - yb);
            operations[position].operands = matrix_operands([xa, xb, ya, yb, ne, nf]);
        })
    }

    /// Réécrit la matrice qui dessine une image pour la tourner ou la
    /// retourner.
    ///
    /// La transformation est appliquée au *repère* : les vecteurs de base
    /// tournent autour du centre de la boîte, l'origine se replace pour que le
    /// centre ne bouge pas. Les pixels ne changent jamais, donc l'opération est
    /// réversible et sans perte.
    pub(crate) fn transform_image_matrix(
        &mut self,
        page: usize,
        resource_name: &str,
        transform: ImageTransform,
    ) -> Result<bool> {
        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = resource_name.to_owned();
        self.edit_image_draw(page_id, &name, |operations, index| {
            let Some(position) = preceding_cm(operations, index) else {
                return;
            };
            let Some([a, b, c, d, e, f]) = matrix_of(&operations[position].operands) else {
                return;
            };
            // Le repère du dessin : `(a, b)` va de gauche à droite de l'image,
            // `(c, d)` du bas vers le haut. `e, f` est le coin bas-gauche.
            let (xa, xb, ya, yb) = match transform {
                // Quart de tour horaire à l'écran : (x, y) devient (y, -x).
                ImageTransform::RotateCw => (b, -a, d, -c),
                ImageTransform::RotateCcw => (-b, a, -d, c),
                // Miroirs : on inverse le vecteur de l'axe concerné.
                ImageTransform::FlipHorizontal => (-a, -b, c, d),
                ImageTransform::FlipVertical => (a, b, -c, -d),
            };
            // Centre conservé : l'origine se déplace de la moitié de l'écart.
            let ne = e + 0.5 * (a + c - xa - ya);
            let nf = f + 0.5 * (b + d - xb - yb);
            operations[position].operands = matrix_operands([xa, xb, ya, yb, ne, nf]);
        })
    }

    /// Rétrécit la boîte de dessin d'une image du côté rogné, sans la
    /// déplacer : le coin bas-gauche local part de `left` / `bottom` et les
    /// vecteurs gardent la part conservée.
    pub(crate) fn crop_image_matrix(
        &mut self,
        page: usize,
        resource_name: &str,
        crop: ImageCrop,
    ) -> Result<bool> {
        let page_id = self
            .page_id(page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let name = resource_name.to_owned();
        self.edit_image_draw(page_id, &name, |operations, index| {
            let Some(position) = preceding_cm(operations, index) else {
                return;
            };
            let Some([a, b, c, d, e, f]) = matrix_of(&operations[position].operands) else {
                return;
            };
            let keep_x = (1.0 - crop.left - crop.right).max(0.01);
            let keep_y = (1.0 - crop.top - crop.bottom).max(0.01);
            let ne = e + a * crop.left + c * crop.bottom;
            let nf = f + b * crop.left + d * crop.bottom;
            operations[position].operands =
                matrix_operands([a * keep_x, b * keep_x, c * keep_y, d * keep_y, ne, nf]);
        })
    }
}
/// Opérateurs `cm` de la matrice `[a b c d e f]`.
fn matrix_operands(values: [f32; 6]) -> Vec<Object> {
    values
        .iter()
        .map(|value| Object::Real(*value))
        .collect()
}

/// Lit les six nombres d'un `cm`, ou rien si les opérandes sont inattendus.
fn matrix_of(operands: &[Object]) -> Option<[f32; 6]> {
    if operands.len() != 6 {
        return None;
    }
    let mut values = [0.0f32; 6];
    for (slot, operand) in values.iter_mut().zip(operands) {
        *slot = operand.as_float().ok()?;
    }
    Some(values)
}

/// Position du dernier `cm` avant un `Do`, sans remonter au-delà de la
/// dernière image ou du dernier `q`.
fn preceding_cm(operations: &[lopdf::content::Operation], index: usize) -> Option<usize> {
    for candidate in (0..index).rev() {
        match operations[candidate].operator.as_str() {
            "cm" => return Some(candidate),
            "q" | "Q" | "Do" => return None,
            _ => {}
        }
    }
    None
}

/// Bords conservés d'un rognage, en **pixels** : `(x0, y0, x1, y1)`.
///
/// Les pixels et la matrice du flux de page sont calculés à partir de ces
/// mêmes bornes, sinon ils divergeraient d'une fraction de pixel.
pub(crate) fn crop_bounds(width: u32, height: u32, crop: ImageCrop) -> (u32, u32, u32, u32) {
    let x0 = ((crop.left * width as f32).floor() as u32).min(width.saturating_sub(1));
    let x1 = ((width as f32 * (1.0 - crop.right)).ceil() as u32).clamp(x0 + 1, width);
    let y0 = ((crop.top * height as f32).floor() as u32).min(height.saturating_sub(1));
    let y1 = ((height as f32 * (1.0 - crop.bottom)).ceil() as u32).clamp(y0 + 1, height);
    (x0, y0, x1, y1)
}

/// Découpe les pixels selon des bornes en pixels.
pub(crate) fn crop_pixels(
    pixels: &[u8],
    width: u32,
    height: u32,
    (x0, y0, x1, y1): (u32, u32, u32, u32),
) -> (Vec<u8>, u32, u32) {
    let out_width = x1.saturating_sub(x0).max(1).min(width);
    let out_height = y1.saturating_sub(y0).max(1).min(height);
    let mut out = vec![0u8; out_width as usize * out_height as usize * 3];
    for y in 0..out_height {
        for x in 0..out_width {
            let sx = (x0 + x).min(width - 1);
            let sy = (y0 + y).min(height - 1);
            let source = (sy as usize * width as usize + sx as usize) * 3;
            let target = (y as usize * out_width as usize + x as usize) * 3;
            if source + 3 <= pixels.len() {
                out[target..target + 3].copy_from_slice(&pixels[source..source + 3]);
            }
        }
    }
    (out, out_width, out_height)
}
