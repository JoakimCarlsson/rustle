use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A picture: RGBA pixels, cheap to clone, identified so renderers can cache its texture.
#[derive(Clone)]
pub struct ImageData {
    id: u64,
    width: u32,
    height: u32,
    /// Straight-alpha RGBA, 8 bits per channel, row by row from the top.
    pixels: Arc<[u8]>,
}

impl ImageData {
    /// A picture from raw RGBA pixels, or `None` if `pixels` is not `width * height * 4` bytes.
    pub fn from_rgba(width: u32, height: u32, pixels: Vec<u8>) -> Option<Self> {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        (pixels.len() == width as usize * height as usize * 4).then(|| Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            width,
            height,
            pixels: pixels.into(),
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

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ImageData({}x{}, id {})",
            self.width, self.height, self.id
        )
    }
}

impl PartialEq for ImageData {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
