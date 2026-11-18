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

/// A picture: RGBA pixels, cheap to clone, identified so renderers can cache its texture.
#[derive(Clone)]
pub struct ImageData {
    id: u64,
    key: Option<ImageKey>,
    width: u32,
    height: u32,
    /// Straight-alpha RGBA, 8 bits per channel, row by row from the top.
    pixels: Arc<[u8]>,
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
        let pixels = pixels.into();
        (pixels.len() == width as usize * height as usize * 4).then(|| Self {
            id: next_id(),
            key: Some(key),
            width,
            height,
            pixels,
        })
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
        let buffer = image::RgbaImage::from_raw(self.width, self.height, self.pixels.to_vec())
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

    /// The straight-alpha RGBA pixels, 8 bits per channel, row by row from the top.
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

    /// A mask from coverage bytes is white with that alpha, and its size is checked.
    #[test]
    fn a_mask_from_coverage_keeps_the_coverage_as_alpha() {
        let mask = ImageData::from_alpha(2, 1, &[0, 200]).unwrap();
        assert_eq!(mask.pixels(), &[255, 255, 255, 0, 255, 255, 255, 200]);
        assert!(ImageData::from_alpha(2, 2, &[0; 3]).is_none());
    }
}
