use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A number not given out before, shared by picture ids and image keys.
fn next_id() -> u64 {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Names a picture that changes over time, such as the frames of a video or of a page drawn
/// elsewhere, so a renderer keeps one texture for it and writes each new picture into it.
///
/// Make one key per live picture with [`ImageKey::new`] and give it to every
/// [`ImageData::keyed`] made for that picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageKey(u64);

impl ImageKey {
    /// A key no other picture has.
    pub fn new() -> Self {
        Self(next_id())
    }

    /// The number that identifies the key.
    pub fn get(self) -> u64 {
        self.0
    }
}

impl Default for ImageKey {
    /// A key no other picture has, as [`ImageKey::new`].
    fn default() -> Self {
        Self::new()
    }
}

/// The order of the four bytes of each pixel of an [`ImageData`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    /// Red, green, blue, then alpha.
    #[default]
    Rgba8,
    /// Blue, green, red, then alpha, as many browsers, video decoders and window systems hand
    /// out pictures; a renderer takes it as it is, without reordering the bytes.
    Bgra8,
}

/// A rectangle of whole pixels inside a picture, from its top left corner.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PixelRect {
    /// The left edge, in pixels from the picture's left.
    pub x: u32,
    /// The top edge, in pixels from the picture's top.
    pub y: u32,
    /// The width, in pixels.
    pub width: u32,
    /// The height, in pixels.
    pub height: u32,
}

impl PixelRect {
    /// A rectangle from its left and top edges and its size.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Whether the rectangle covers no pixel.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// The part of the rectangle inside a picture `width` by `height` pixels.
    pub fn clamped(self, width: u32, height: u32) -> Self {
        let x = self.x.min(width);
        let y = self.y.min(height);
        Self {
            x,
            y,
            width: self.width.min(width - x),
            height: self.height.min(height - y),
        }
    }

    /// The smallest rectangle holding both this one and `other`; an empty one adds nothing.
    pub fn union(self, other: Self) -> Self {
        if self.is_empty() {
            return other;
        }
        if other.is_empty() {
            return self;
        }
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = (self.x + self.width).max(other.x + other.width);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self::new(x, y, right - x, bottom - y)
    }
}

/// A picture: RGBA or BGRA pixels, cheap to clone, identified so renderers can cache its texture.
#[derive(Clone)]
pub struct ImageData {
    id: u64,
    key: Option<ImageKey>,
    width: u32,
    height: u32,
    /// Straight alpha, 8 bits per channel in the order of `format`, row by row from the top.
    pixels: Arc<[u8]>,
    /// The order of each pixel's bytes.
    format: PixelFormat,
    /// The generation of the same key this picture differs from only inside the rectangle.
    change: Option<(u64, PixelRect)>,
}

impl ImageData {
    /// A picture from raw RGBA pixels, or `None` if `pixels` is not `width * height * 4` bytes.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        (pixels.len() == width as usize * height as usize * 4).then(|| Self {
            id: next_id(),
            key: None,
            width,
            height,
            pixels: pixels.into(),
            format: PixelFormat::Rgba8,
            change: None,
        })
    }

    /// One picture of the live picture `key`, from raw RGBA pixels, or `None` if `pixels` is
    /// not `width * height * 4` bytes.
    ///
    /// Each call makes a new generation of `key`: it has an [`id`](ImageData::id) of its own,
    /// so it is not equal to the pictures made before it, but a renderer draws every generation
    /// into the one texture it keeps for `key`. When a newer generation is drawn, the texture
    /// is written in place if the size is the same and made again if it changed, so a picture
    /// that changes every frame does not make a texture every frame. Only the newest generation
    /// drawn is kept: drawing two generations of one key in a frame shows the later in both
    /// places, and drawing an older generation again uploads it again.
    pub fn keyed(
        key: ImageKey,
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
    ) -> Option<Self> {
        Self::keyed_as(key, width, height, pixels, PixelFormat::Rgba8)
    }

    /// One picture of the live picture `key`, as [`ImageData::keyed`], from pixels whose bytes
    /// are in the order `format`, or `None` if `pixels` is not `width * height * 4` bytes.
    ///
    /// A renderer keeps a BGRA picture in a texture of that order, so pixels handed out as
    /// BGRA, such as a browser's paints, are uploaded without reordering a byte.
    pub fn keyed_as(
        key: ImageKey,
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
        format: PixelFormat,
    ) -> Option<Self> {
        let pixels = pixels.into();
        (pixels.len() == width as usize * height as usize * 4).then(|| Self {
            id: next_id(),
            key: Some(key),
            width,
            height,
            pixels,
            format,
            change: None,
        })
    }

    /// This picture, said to differ from the generation `previous` of its key only inside
    /// `rect`, so a renderer whose texture still holds `previous` writes only that rectangle
    /// into it.
    ///
    /// A renderer whose texture holds any other generation, or none, or one of another size or
    /// pixel format, writes the whole picture as it would without this, so a wrong `previous`
    /// costs an upload, never a wrong picture. `rect` is cut to the picture; an empty one says
    /// nothing changed. Pictures without a key are always written whole.
    pub fn changed_since(mut self, previous: u64, rect: PixelRect) -> Self {
        self.change = Some((previous, rect.clamped(self.width, self.height)));
        self
    }

    /// The generation this picture differs from only inside a rectangle, and that rectangle,
    /// when it was made with [`ImageData::changed_since`].
    pub fn change(&self) -> Option<(u64, PixelRect)> {
        self.change
    }

    /// The order of the bytes of each pixel.
    pub fn format(&self) -> PixelFormat {
        self.format
    }

    /// A coverage mask from one byte per pixel, or `None` if `coverage` is not
    /// `width * height` bytes: white pixels whose alpha is the coverage, for
    /// [`DrawCommand::Mask`](crate::DrawCommand::Mask), which takes only the alpha.
    pub fn from_alpha(width: u32, height: u32, coverage: &[u8]) -> Option<Self> {
        (coverage.len() == width as usize * height as usize).then(|| {
            let pixels = coverage
                .iter()
                .flat_map(|&alpha| [255, 255, 255, alpha])
                .collect::<Vec<u8>>();
            Self {
                id: next_id(),
                key: None,
                width,
                height,
                pixels: pixels.into(),
                format: PixelFormat::Rgba8,
                change: None,
            }
        })
    }

    /// Decodes a PNG or JPEG file's bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, String> {
        let decoded = image::load_from_memory(bytes)
            .map_err(|error| error.to_string())?
            .to_rgba8();
        let (width, height) = decoded.dimensions();
        Self::from_rgba(width, height, decoded.into_raw())
            .ok_or_else(|| "decoded image has the wrong size".to_string())
    }

    /// The picture as the bytes of a PNG file.
    pub fn encode_png(&self) -> Result<Vec<u8>, String> {
        let mut rgba = self.pixels.to_vec();
        if self.format == PixelFormat::Bgra8 {
            for pixel in rgba.as_chunks_mut::<4>().0 {
                pixel.swap(0, 2);
            }
        }
        let buffer = image::RgbaImage::from_raw(self.width, self.height, rgba)
            .ok_or_else(|| "pixels do not match the size".to_string())?;
        let mut bytes = std::io::Cursor::new(Vec::new());
        buffer
            .write_to(&mut bytes, image::ImageFormat::Png)
            .map_err(|error| error.to_string())?;
        Ok(bytes.into_inner())
    }

    /// Identifies this picture; clones share it.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The live picture this is a generation of, if it was made with [`ImageData::keyed`].
    pub fn key(&self) -> Option<ImageKey> {
        self.key
    }

    /// The generation of a keyed picture: it grows with every picture made, so a later
    /// generation of a key has a larger one. The same number as [`id`](ImageData::id).
    pub fn generation(&self) -> u64 {
        self.id
    }

    /// The width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The straight-alpha pixels, 8 bits per channel in the order [`ImageData::format`] gives
    /// (RGBA unless the picture was made as BGRA), row by row from the top.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl std::fmt::Debug for ImageData {
    /// The size and id, and the key of a keyed picture; never the pixels.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ImageData({}x{}, id {})",
            self.width, self.height, self.id
        )?;
        match self.key {
            Some(key) => write!(f, " of key {}", key.0),
            None => Ok(()),
        }
    }
}

impl PartialEq for ImageData {
    /// Pictures are equal when they are the same picture or clones of it.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Generations of one key are told apart, and the pixel count is checked.
    #[test]
    fn keyed_generations_are_distinct_pictures_of_one_key() {
        let key = ImageKey::new();
        let first = ImageData::keyed(key, 1, 1, vec![1, 2, 3, 4]).unwrap();
        let second = ImageData::keyed(key, 1, 1, vec![5, 6, 7, 8]).unwrap();
        assert_eq!((first.key(), second.key()), (Some(key), Some(key)));
        assert_ne!(first, second);
        assert!(second.generation() > first.generation());
        assert_eq!(first.clone(), first);
        assert_ne!(ImageKey::new(), key);
        assert!(ImageData::keyed(key, 2, 1, vec![0; 4]).is_none());
        assert_eq!(ImageData::from_rgba(1, 1, vec![0; 4]).unwrap().key(), None);
    }

    /// A BGRA picture keeps its byte order, and a change is cut to the picture.
    #[test]
    fn a_bgra_picture_and_its_change_are_kept() {
        let key = ImageKey::new();
        let image = ImageData::keyed_as(key, 2, 2, vec![0; 16], PixelFormat::Bgra8)
            .unwrap()
            .changed_since(7, PixelRect::new(1, 1, 5, 5));
        assert_eq!(image.format(), PixelFormat::Bgra8);
        assert_eq!(image.change(), Some((7, PixelRect::new(1, 1, 1, 1))));
        assert_eq!(
            ImageData::keyed(key, 1, 1, vec![0; 4]).unwrap().format(),
            PixelFormat::Rgba8
        );
        assert_eq!(
            PixelRect::new(0, 0, 1, 1).union(PixelRect::new(3, 2, 1, 1)),
            PixelRect::new(0, 0, 4, 3)
        );
        assert_eq!(
            PixelRect::default().union(PixelRect::new(3, 2, 1, 1)),
            PixelRect::new(3, 2, 1, 1)
        );
    }

    /// A BGRA picture is encoded with its colors where they belong.
    #[test]
    fn a_bgra_picture_encodes_as_its_colors() {
        let bgra = ImageData::keyed_as(
            ImageKey::new(),
            1,
            1,
            vec![10, 20, 30, 255],
            PixelFormat::Bgra8,
        )
        .unwrap();
        let decoded = ImageData::decode(&bgra.encode_png().unwrap()).unwrap();
        assert_eq!(decoded.pixels(), &[30, 20, 10, 255]);
    }

    /// A mask from coverage bytes is white with that alpha, and its size is checked.
    #[test]
    fn a_mask_from_coverage_keeps_the_coverage_as_alpha() {
        let mask = ImageData::from_alpha(2, 1, &[0, 200]).unwrap();
        assert_eq!(mask.pixels(), &[255, 255, 255, 0, 255, 255, 255, 200]);
        assert!(ImageData::from_alpha(2, 2, &[0; 3]).is_none());
    }
}
