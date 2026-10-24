//! Édition d'images posées sur une page : transformations, opacité, rognage.

use lopdf::{Object, Stream};

use youpdf_shared::{Error, Result};

use crate::document::Document;
use crate::edit::matrix::{crop_bounds, crop_pixels};
use crate::edit::ImageInfo;

/// Transformation géométrique appliquée aux pixels d'une image.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ImageTransform {
    /// Quart de tour dans le sens horaire.
    RotateCw,
    /// Quart de tour dans le sens antihoraire.
    RotateCcw,
    /// Miroir horizontal.
    FlipHorizontal,
    /// Miroir vertical.
    FlipVertical,
}

impl ImageTransform {
    /// Les quatre transformations, dans l'ordre de l'interface.
    pub const ALL: [ImageTransform; 4] = [
        ImageTransform::RotateCw,
        ImageTransform::RotateCcw,
        ImageTransform::FlipHorizontal,
        ImageTransform::FlipVertical,
    ];

    /// Libellé affiché dans l'interface.
    pub fn display_name(self) -> &'static str {
        match self {
            ImageTransform::RotateCw => "Rotate 90° CW",
            ImageTransform::RotateCcw => "Rotate 90° CCW",
            ImageTransform::FlipHorizontal => "Flip Horizontal",
            ImageTransform::FlipVertical => "Flip Vertical",
        }
    }

    /// Vrai pour les rotations, qui échangent largeur et hauteur.
    pub fn swaps_axes(self) -> bool {
        matches!(self, ImageTransform::RotateCw | ImageTransform::RotateCcw)
    }
}

/// Fractions retirées de chaque bord d'une image, entre 0 et 1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ImageCrop {
    /// Bord gauche.
    pub left: f32,
    /// Bord haut.
    pub top: f32,
    /// Bord droit.
    pub right: f32,
    /// Bord bas.
    pub bottom: f32,
}

impl ImageCrop {
    /// Ramène les fractions dans `[0, 1)` sans jamais vider l'image : les
    /// quatre bords sont réduits proportionnellement si leur somme atteint
    /// l'image entière.
    pub fn sanitised(mut self) -> Self {
        let clamp = |value: f32| {
            if value.is_finite() {
                value.clamp(0.0, 0.98)
            } else {
                0.0
            }
        };
        self.left = clamp(self.left);
        self.right = clamp(self.right);
        self.top = clamp(self.top);
        self.bottom = clamp(self.bottom);
        let width = self.left + self.right;
        if width >= 1.0 {
            self.left /= width;
            self.right /= width;
        }
        let height = self.top + self.bottom;
        if height >= 1.0 {
            self.top /= height;
            self.bottom /= height;
        }
        self
    }

    /// Vrai quand aucun bord n'est rogné.
    pub fn is_empty(&self) -> bool {
        self.left <= f32::EPSILON
            && self.top <= f32::EPSILON
            && self.right <= f32::EPSILON
            && self.bottom <= f32::EPSILON
    }
}

impl Document {
    /// Pixels RGB 8 bits d'une image embarquée, et ses dimensions.
    ///
    /// Passe par le décodeur commun : le flux peut être du JPEG (`DCTDecode`),
    /// du PNG ou des échantillons bruts en zlib.
    pub fn image_pixels(&self, info: &ImageInfo) -> Result<(Vec<u8>, u32, u32)> {
        let png = self.extract_image_png(info)?;
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .map_err(|e| Error::Pdf(format!("décodage image : {e}")))?;
        let rgb = decoded.to_rgb8();
        let (width, height) = rgb.dimensions();
        Ok((rgb.into_raw(), width, height))
    }

    /// Remplace les pixels d'une image en place.
    ///
    /// L'objet image garde son identifiant : le nom de ressource et la
    /// matrice posés dans le flux de la page restent valables, donc l'image
    /// ne bouge pas. Si les proportions changent, elle est étirée dans la
    /// même boîte, comme le fait un remplacement d'image dans un traitement
    /// de texte.
    pub fn replace_image_pixels(
        &mut self,
        info: &ImageInfo,
        pixels_rgb: &[u8],
        width: u32,
        height: u32,
    ) -> Result<()> {
        let mut stream = Self::build_image_stream(pixels_rgb, width, height)?;
        // Le masque de transparence et la table de décodage appartiennent à
        // l'image : les perdre rendrait un logo transparent opaque. `Decode` et
        // une `Mask` par plage de couleurs dépendent en revanche du nombre de
        // composantes : recopiés d'une image CMJN sur notre RGB, ils
        // décaleraient les couleurs.
        if let Ok(Object::Stream(previous)) = self.inner().get_object(info.object_id) {
            if let Ok(value) = previous.dict.get(b"SMask") {
                stream.dict.set("SMask", value.clone());
            }
            let was_rgb = previous
                .dict
                .get(b"ColorSpace")
                .ok()
                .and_then(|space| space.as_name().ok())
                .is_some_and(|name| name == b"DeviceRGB");
            if was_rgb {
                for key in [b"Mask".as_slice(), b"Decode"] {
                    if let Ok(value) = previous.dict.get(key) {
                        stream.dict.set(key, value.clone());
                    }
                }
            }
        }
        self.inner_mut()
            .set_object(info.object_id, Object::Stream(stream));
        Ok(())
    }

    /// Tourne ou retourne une image en place.
    ///
    /// Seule la matrice qui dessine l'image change, autour du centre de sa
    /// boîte : les pixels ne sont jamais réencodés, donc l'opération est sans
    /// perte et instantanée. Une rotation échange les côtés de la boîte si
    /// l'image n'est pas carrée, comme dans les autres lecteurs.
    pub fn transform_image(&mut self, info: &ImageInfo, transform: ImageTransform) -> Result<()> {
        let found = self.transform_image_matrix(info.page, &info.resource_name, transform)?;
        if !found {
            return Err(Error::NotFound(format!(
                "matrice de l'image {}",
                info.resource_name
            )));
        }
        Ok(())
    }

    /// Règle l'opacité d'une image, de 0 (invisible) à 1 (opaque).
    ///
    /// L'opacité passe par un `/ExtGState` : elle est donc réellement
    /// transparente sur la page, sans retouche des pixels.
    pub fn set_image_opacity(&mut self, info: &ImageInfo, alpha: f32) -> Result<()> {
        let alpha = if alpha.is_finite() {
            alpha.clamp(0.0, 1.0)
        } else {
            1.0
        };
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        // Un état graphique par image : deux images de la même page peuvent
        // avoir deux opacités différentes.
        let name = format!("YouPdfImAlpha{}", info.resource_name.replace('/', ""));
        use lopdf::dictionary;
        let mut resources = self.page_resources(page_id)?;
        let mut states = self
            .resolve_dict(resources.get(b"ExtGState").ok())
            .unwrap_or_default();
        // Un seul objet par image : régler l'opacité au curseur ne doit pas
        // semer un dictionnaire orphelin à chaque pas.
        let existing = states
            .get(name.as_bytes())
            .ok()
            .and_then(|object| object.as_reference().ok());
        let state = dictionary! {
            "Type" => Object::Name(b"ExtGState".to_vec()),
            "ca" => Object::Real(alpha),
            "CA" => Object::Real(alpha),
        };
        let state_id = match existing {
            Some(id) => {
                self.inner_mut().set_object(id, Object::Dictionary(state));
                id
            }
            None => self.inner_mut().add_object(Object::Dictionary(state)),
        };
        states.set(name.as_str(), Object::Reference(state_id));
        resources.set("ExtGState", Object::Dictionary(states));
        self.set_page_resources(page_id, resources)?;

        let resource_name = info.resource_name.clone();
        self.edit_image_draw(page_id, &resource_name, |operations, index| {
            // Un seul `/gs` par image : régler l'opacité au curseur ne doit pas
            // empiler un opérateur à chaque pas. On retire ceux déjà posés
            // juste avant le dessin (`cm` et `gs` forment le préambule).
            let mut stale = Vec::new();
            for candidate in (0..index).rev() {
                let operation = &operations[candidate];
                match operation.operator.as_str() {
                    "gs" => {
                        // Seulement nos états : un `/gs` posé par la page pour
                        // elle-même ne nous appartient pas.
                        let ours = operation
                            .operands
                            .first()
                            .and_then(|object| object.as_name().ok())
                            == Some(name.as_bytes());
                        if ours {
                            stale.push(candidate);
                        }
                    }
                    "cm" => continue,
                    _ => break,
                }
            }
            // Les indices sont décroissants : retirer du plus grand au plus
            // petit est le seul ordre qui ne décale pas les suivants.
            for position in &stale {
                operations.remove(*position);
            }
            // Le `/gs` doit précéder le `Do` : `cm` puis `gs` puis `Do`.
            operations.insert(
                index - stale.len(),
                lopdf::content::Operation::new(
                    "gs",
                    vec![Object::Name(name.as_bytes().to_vec())],
                ),
            );
        })?;
        Ok(())
    }

    /// Rogne une image : les pixels sont découpés et la boîte de dessin
    /// rétrécit d'autant, du côté rogné.
    ///
    /// C'est un vrai rognage : l'image ne bouge pas sur la page, elle perd
    /// seulement sa bordure. L'appelant peut rogner plusieurs fois de suite.
    pub fn crop_image(&mut self, info: &ImageInfo, crop: ImageCrop) -> Result<()> {
        let crop = crop.sanitised();
        if crop.is_empty() {
            return Ok(());
        }
        let (pixels, width, height) = self.image_pixels(info)?;
        // Les bords sont arrondis au pixel : la matrice reprend ces fractions
        // effectives, sinon la boîte et l'image découpée divergent d'un pixel.
        let bounds = crop_bounds(width, height, crop);
        if bounds.2 <= bounds.0 || bounds.3 <= bounds.1 {
            return Ok(());
        }
        let effective = ImageCrop {
            left: bounds.0 as f32 / width as f32,
            right: 1.0 - bounds.2 as f32 / width as f32,
            top: bounds.1 as f32 / height as f32,
            bottom: 1.0 - bounds.3 as f32 / height as f32,
        };
        let found = self.crop_image_matrix(info.page, &info.resource_name, effective)?;
        if !found {
            return Err(Error::NotFound(format!(
                "matrice de l'image {}",
                info.resource_name
            )));
        }
        let (pixels, width, height) = crop_pixels(&pixels, width, height, bounds);
        self.replace_image_pixels(info, &pixels, width, height)
    }

    /// Retire une image de la page : le `Do` disparaît du flux et la
    /// ressource est déliée. L'objet image reste dans le fichier jusqu'au
    /// prochain [`Document::purge_unreachable`].
    pub fn delete_image(&mut self, info: &ImageInfo) -> Result<()> {
        let page_id = self
            .page_id(info.page as u32)
            .map_err(|e| Error::Pdf(format!("page : {e}")))?;
        let resource_name = info.resource_name.clone();
        let found = self.edit_image_draw(page_id, &resource_name, |operations, index| {
            // Le préambule de l'image (`q` … `cm`) ne sert qu'à elle : il part
            // avec le dessin, et le `Q` qui le ferme aussi, sinon le flux
            // laisserait des opérateurs déséquilibrés.
            let mut remove_from = index;
            let mut balanced = false;
            for candidate in (0..index).rev() {
                match operations[candidate].operator.as_str() {
                    "cm" | "gs" => remove_from = candidate,
                    "q" => {
                        remove_from = candidate;
                        balanced = true;
                        break;
                    }
                    _ => break,
                }
            }
            let mut remove_to = index;
            // Le `Q` ne part qu'avec le `q` de l'image : sinon il fermerait le
            // groupe graphique de la page et la suite du flux serait peinte
            // avec le mauvais état.
            if balanced
                && operations
                    .get(index + 1)
                    .is_some_and(|operation| operation.operator == "Q")
            {
                remove_to = index + 1;
            }
            operations.drain(remove_from..=remove_to);
        })?;
        if !found {
            return Err(Error::NotFound(format!(
                "dessin de l'image {resource_name}"
            )));
        }

        let mut resources = self.page_resources(page_id)?;
        if let Some(mut xobjects) = self.resolve_dict(resources.get(b"XObject").ok()) {
            xobjects.remove(resource_name.as_bytes());
            resources.set("XObject", Object::Dictionary(xobjects));
        }
        self.set_page_resources(page_id, resources)?;
        Ok(())
    }
    /// Construit le flux d'une image RGB 8 bits non compressée.
    fn build_image_stream(pixels_rgb: &[u8], width: u32, height: u32) -> Result<Stream> {
        let expected = width as usize * height as usize * 3;
        if pixels_rgb.len() < expected {
            return Err(Error::InvalidInput(format!(
                "bitmap trop court : {} octets, attendu {expected}",
                pixels_rgb.len()
            )));
        }
        use lopdf::dictionary;
        let dict = dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(width as i64),
            "Height" => Object::Integer(height as i64),
            "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
        };
        Ok(Stream::new(dict, pixels_rgb[..expected].to_vec()))
    }
}
