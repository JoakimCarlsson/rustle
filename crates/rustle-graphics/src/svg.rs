//! Vector artwork that arrives as SVG text, such as an icon a program loads while it runs.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, LazyLock, Mutex, PoisonError};

use crate::image::ImageData;

/// The most bytes of drawn pictures kept between frames; the least recently drawn go first.
const RASTER_BUDGET: usize = 64 * 1024 * 1024;

/// The most parsed documents kept for loading the same text again.
const DOCUMENT_LIMIT: usize = 512;

/// The longest side, in pixels, an SVG document is drawn at.
const LONGEST_SIDE: u32 = 8192;

/// An SVG document ready to draw at any size, as Compose's `loadSvgPainter` gives:
/// load it with [`SvgPainter::from_str`] or [`SvgPainter::from_bytes`], then draw it with
/// `svg_image`, `svg_icon` or `Canvas::svg`, which give it the size to fill and, for an icon,
/// the color to tint it with.
///
/// Cheap to clone. The document is drawn on the CPU when a renderer meets it, at the area's
/// size in physical pixels, so it stays sharp at any scale; the pictures are kept by the
/// document's content and their pixel size, shared by every painter loaded from the same text,
/// and the least recently drawn are let go once they pass a budget. Painters loaded from the
/// same text are equal.
///
/// Shapes, strokes, gradients, patterns, masks, clips and filters are drawn; text and embedded
/// pictures in the document are not.
#[derive(Clone)]
pub struct SvgPainter {
    document: Arc<Document>,
}

/// A parsed document and what it is known by.
struct Document {
    /// A hash of the text it was parsed from: its identity in the caches.
    key: u64,
    /// The parsed document.
    tree: resvg::usvg::Tree,
}

/// A drawn picture of a document, and when it was last asked for.
struct Raster {
    /// The picture.
    image: ImageData,
    /// The tick of the last time it was asked for.
    used: u64,
}

/// The pictures drawn so far, by document and pixel size, with the bytes they hold and a clock.
#[derive(Default)]
struct Rasters {
    /// The pictures, by document key, width and height.
    pictures: HashMap<(u64, u32, u32), Raster>,
    /// The bytes all pictures hold.
    bytes: usize,
    /// Counts up each time a picture is asked for.
    tick: u64,
}

/// Documents parsed so far, by the hash of their text.
static DOCUMENTS: LazyLock<Mutex<HashMap<u64, Arc<Document>>>> = LazyLock::new(Default::default);

/// Pictures drawn so far.
static RASTERS: LazyLock<Mutex<Rasters>> = LazyLock::new(Default::default);

impl SvgPainter {
    /// Loads the SVG document `text`, or says why it is not one.
    ///
    /// Loading text loaded before reuses the document parsed then.
    ///
    /// # Errors
    ///
    /// The parser's message when `text` is not an SVG document it can read.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(text: &str) -> Result<Self, String> {
        Self::from_bytes(text.as_bytes())
    }

    /// Loads an SVG document from the bytes of an `.svg` file, or
    /// says why they are not one, as Compose's `loadSvgPainter(inputStream, density)`.
    ///
    /// Loading bytes loaded before reuses the document parsed then.
    ///
    /// # Errors
    ///
    /// The parser's message when `bytes` are not an SVG document it can read.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        let key = hasher.finish();
        let mut documents = DOCUMENTS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(document) = documents.get(&key) {
            return Ok(Self {
                document: Arc::clone(document),
            });
        }
        let tree = resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default())
            .map_err(|error| error.to_string())?;
        if documents.len() >= DOCUMENT_LIMIT {
            documents.retain(|_, document| Arc::strong_count(document) > 1);
        }
        let document = Arc::new(Document { key, tree });
        documents.insert(key, Arc::clone(&document));
        Ok(Self { document })
    }

    /// The size the document says it is, in logical pixels: its `width` and `height`, or its
    /// view box when it gives none, as Compose's `Painter.intrinsicSize`.
    pub fn intrinsic_size(&self) -> (f32, f32) {
        let size = self.document.tree.size();
        (size.width(), size.height())
    }

    /// The document drawn stretched over `width` by `height` pixels, kept for the next time
    /// that size is asked for; nothing when either side is zero or longer than 8192 pixels.
    ///
    /// The picture has straight alpha, so it can be drawn as it is or used as a coverage mask.
    pub fn rasterize(&self, width: u32, height: u32) -> Option<ImageData> {
        if width == 0 || height == 0 || width > LONGEST_SIDE || height > LONGEST_SIDE {
            return None;
        }
        let entry = (self.document.key, width, height);
        let mut rasters = RASTERS.lock().unwrap_or_else(PoisonError::into_inner);
        rasters.tick += 1;
        let tick = rasters.tick;
        if let Some(raster) = rasters.pictures.get_mut(&entry) {
            raster.used = tick;
            return Some(raster.image.clone());
        }
        let image = self.draw(width, height)?;
        rasters.bytes += image.pixels().len();
        rasters.pictures.insert(
            entry,
            Raster {
                image: image.clone(),
                used: tick,
            },
        );
        rasters.trim();
        Some(image)
    }

    /// The document drawn stretched over `width` by `height` pixels, or nothing when no pixel
    /// buffer of that size can be made.
    fn draw(&self, width: u32, height: u32) -> Option<ImageData> {
        let (natural_width, natural_height) = self.intrinsic_size();
        let mut canvas = resvg::tiny_skia::Pixmap::new(width, height)?;
        resvg::render(
            &self.document.tree,
            resvg::tiny_skia::Transform::from_scale(
                width as f32 / natural_width.max(f32::EPSILON),
                height as f32 / natural_height.max(f32::EPSILON),
            ),
            &mut canvas.as_mut(),
        );
        let pixels = canvas
            .pixels()
            .iter()
            .flat_map(|pixel| {
                let color = pixel.demultiply();
                [color.red(), color.green(), color.blue(), color.alpha()]
            })
            .collect();
        ImageData::from_rgba(width, height, pixels)
    }
}

impl Rasters {
    /// Lets the least recently asked for pictures go until those kept fit the budget.
    fn trim(&mut self) {
        while self.bytes > RASTER_BUDGET && self.pictures.len() > 1 {
            let Some(oldest) = self
                .pictures
                .iter()
                .min_by_key(|(_, raster)| raster.used)
                .map(|(entry, _)| *entry)
            else {
                return;
            };
            if let Some(raster) = self.pictures.remove(&oldest) {
                self.bytes -= raster.image.pixels().len();
            }
        }
    }
}

impl PartialEq for SvgPainter {
    /// Whether both were loaded from the same text.
    fn eq(&self, other: &Self) -> bool {
        self.document.key == other.document.key
    }
}

impl fmt::Debug for SvgPainter {
    /// Writes the document's identity and size, not its content.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (width, height) = self.intrinsic_size();
        formatter
            .debug_struct("SvgPainter")
            .field("key", &self.document.key)
            .field("width", &width)
            .field("height", &height)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 10 by 20 document: a red square over its top half.
    const HALF: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="20" viewBox="0 0 10 20"><rect width="10" height="10" fill="#ff0000"/></svg>"##;

    /// The pixel of `image` at `x`, `y`, as RGBA.
    fn pixel(image: &ImageData, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width() + x) * 4) as usize;
        image.pixels()[at..at + 4].try_into().unwrap()
    }

    /// The document's own size is read from its width and height.
    #[test]
    fn reads_the_intrinsic_size() {
        let painter = SvgPainter::from_str(HALF).unwrap();
        assert_eq!(painter.intrinsic_size(), (10.0, 20.0));
    }

    /// Text that is not SVG is refused with a reason.
    #[test]
    fn refuses_what_is_not_svg() {
        assert!(SvgPainter::from_str("not a picture").is_err());
        assert!(SvgPainter::from_bytes(&[0xff, 0x00]).is_err());
    }

    /// The document is stretched over the pixels asked for.
    #[test]
    fn draws_at_the_size_asked_for() {
        let painter = SvgPainter::from_str(HALF).unwrap();
        let image = painter.rasterize(40, 40).unwrap();
        assert_eq!((image.width(), image.height()), (40, 40));
        assert_eq!(pixel(&image, 20, 5), [255, 0, 0, 255]);
        assert_eq!(pixel(&image, 20, 35)[3], 0);
    }

    /// A size asked for again gives the picture drawn before, and painters of the same text
    /// share it.
    #[test]
    fn keeps_pictures_by_content_and_size() {
        let first = SvgPainter::from_str(HALF).unwrap();
        let second = SvgPainter::from_bytes(HALF.as_bytes()).unwrap();
        assert_eq!(first, second);
        let drawn = first.rasterize(12, 24).unwrap();
        assert_eq!(second.rasterize(12, 24).unwrap().id(), drawn.id());
        assert_ne!(first.rasterize(13, 24).unwrap().id(), drawn.id());
    }

    /// Nothing is drawn at an empty or oversized area.
    #[test]
    fn draws_nothing_at_an_empty_size() {
        let painter = SvgPainter::from_str(HALF).unwrap();
        assert!(painter.rasterize(0, 10).is_none());
        assert!(painter.rasterize(10, LONGEST_SIDE + 1).is_none());
    }
}
