//! Vector artwork that arrives as SVG text, such as an icon a program loads while it runs.

use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
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
/// Shapes, strokes, gradients, patterns, masks, clips, filters, text and pictures embedded as
/// `data:` URLs (PNG, JPEG, GIF, WebP and nested SVG) are drawn. Text is set with the fonts the
/// text engine loaded (the system's and those added with `register_font`), handed over by the
/// renderer with [`set_svg_fonts`]; a document with text loaded before any renderer existed,
/// or before a font it names was registered, is parsed again when loaded again afterwards.
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

/// Where a font the text of SVG documents may be set with comes from, as the text engine
/// loaded it. Given to [`set_svg_fonts`] by a renderer.
#[derive(Clone)]
pub enum SvgFontSource {
    /// A font file, read when a document needs it.
    File(PathBuf),
    /// The bytes of a font file held in memory, shared with the text engine.
    Data(Arc<dyn AsRef<[u8]> + Send + Sync>),
}

impl fmt::Debug for SvgFontSource {
    /// Writes the path of a file, and only the length of data held in memory.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => formatter.debug_tuple("File").field(path).finish(),
            Self::Data(data) => formatter
                .debug_tuple("Data")
                .field(&AsRef::<[u8]>::as_ref(&**data).len())
                .finish(),
        }
    }
}

/// The fonts SVG text is set with: where they come from, the families the generic names
/// stand for, the database made from them once a document needed it, and how many times
/// they were replaced.
#[derive(Default)]
struct SvgFonts {
    /// Counts the calls to [`set_svg_fonts`]; part of the identity of a document with text.
    generation: u64,
    /// Where the fonts come from.
    sources: Vec<SvgFontSource>,
    /// The families `sans-serif`, `serif` and `monospace` stand for.
    generic: [Option<String>; 3],
    /// The database made from `sources`, once a document with text needed it.
    database: Option<Arc<fontdb::Database>>,
}

/// The fonts SVG text is set with.
static FONTS: LazyLock<Mutex<SvgFonts>> = LazyLock::new(Default::default);

/// Hands the fonts the text engine loaded to SVG documents, so their text is set with the
/// same fonts as the rest of the interface: `sources` are the fonts, and `sans_serif`,
/// `serif` and `monospace` the families the generic names stand for when `sources` hold none
/// of the families desktop systems resolve them to.
///
/// As a browser does, a generic name (`sans-serif`, `serif`, `monospace`, `cursive`,
/// `fantasy`) is set in the family the system resolves it to: the first installed of the
/// usual desktop families (such as Inter, Noto Sans or DejaVu Sans for `sans-serif`). The
/// family of text that names none is usvg's default, Times New Roman, as a browser's
/// default font is a serif one.
///
/// Renderers call it when they load fonts; an application does not need to. Documents with
/// text loaded from now on are set with these fonts; ones loaded before keep the fonts they
/// were parsed with.
pub fn set_svg_fonts(
    sources: Vec<SvgFontSource>,
    sans_serif: Option<String>,
    serif: Option<String>,
    monospace: Option<String>,
) {
    let mut fonts = FONTS.lock().unwrap_or_else(PoisonError::into_inner);
    fonts.generation += 1;
    fonts.sources = sources;
    fonts.generic = [sans_serif, serif, monospace];
    fonts.database = None;
}

/// The generation of the fonts handed over with [`set_svg_fonts`].
fn font_generation() -> u64 {
    FONTS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .generation
}

/// The options documents with text are parsed with: the font database made from the fonts
/// handed over with [`set_svg_fonts`], made the first time it is needed, with its generic
/// families resolved as a browser resolves them.
fn text_options() -> resvg::usvg::Options<'static> {
    let mut fonts = FONTS.lock().unwrap_or_else(PoisonError::into_inner);
    let database = match &fonts.database {
        Some(database) => Arc::clone(database),
        None => {
            let mut database = fontdb::Database::new();
            let mut files = std::collections::HashSet::new();
            for source in &fonts.sources {
                match source {
                    SvgFontSource::File(path) => {
                        if files.insert(path.clone()) {
                            let _ = database.load_font_file(path);
                        }
                    }
                    SvgFontSource::Data(data) => {
                        database.load_font_source(fontdb::Source::Binary(Arc::clone(data)));
                    }
                }
            }
            let [sans_serif, serif, monospace] = fonts.generic.clone();
            if let Some(family) = installed(&database, SANS_SERIF).or(sans_serif) {
                database.set_sans_serif_family(family);
            }
            if let Some(family) = installed(&database, SERIF).or(serif) {
                database.set_serif_family(family);
            }
            if let Some(family) = installed(&database, MONOSPACE).or(monospace) {
                database.set_monospace_family(family);
            }
            if let Some(family) = installed(&database, CURSIVE) {
                database.set_cursive_family(family);
            }
            if let Some(family) = installed(&database, FANTASY) {
                database.set_fantasy_family(family);
            }
            let database = Arc::new(database);
            fonts.database = Some(Arc::clone(&database));
            database
        }
    };
    resvg::usvg::Options {
        fontdb: database,
        ..resvg::usvg::Options::default()
    }
}

/// The families desktop systems resolve `sans-serif` to, most wanted first, as a browser
/// sets SVG text named only by its generic family.
const SANS_SERIF: &[&str] = &[
    "Inter",
    "Noto Sans",
    "DejaVu Sans",
    "Liberation Sans",
    "Cantarell",
    "Segoe UI",
    "Helvetica",
    "Arial",
];

/// The families desktop systems resolve `serif` to, most wanted first.
const SERIF: &[&str] = &[
    "Noto Serif",
    "DejaVu Serif",
    "Liberation Serif",
    "Times New Roman",
    "Times",
];

/// The families desktop systems resolve `monospace` to, most wanted first.
const MONOSPACE: &[&str] = &[
    "JetBrains Mono",
    "Noto Sans Mono",
    "DejaVu Sans Mono",
    "Liberation Mono",
    "Menlo",
    "Consolas",
    "Courier New",
];

/// The families desktop systems resolve `cursive` to, most wanted first.
const CURSIVE: &[&str] = &["Comic Neue", "Comic Sans MS", "Apple Chancery"];

/// The families desktop systems resolve `fantasy` to, most wanted first.
const FANTASY: &[&str] = &["Impact", "Papyrus"];

/// The first of `preferred` that `database` holds a face of, or `None` when it holds none.
fn installed(database: &fontdb::Database, preferred: &[&str]) -> Option<String> {
    preferred
        .iter()
        .find(|wanted| {
            database
                .faces()
                .any(|face| face.families.iter().any(|(name, _)| name == *wanted))
        })
        .map(|family| (*family).to_owned())
}

/// Whether `bytes` may hold a text element, whose glyphs depend on the fonts: one of their
/// own, or an SVG document embedded as a `data:` URL, whose text cannot be seen without
/// decoding it.
fn has_text(bytes: &[u8]) -> bool {
    const NESTED: &[u8] = b"data:image/svg+xml";
    bytes.windows(5).any(|window| window == b"<text")
        || bytes
            .windows(NESTED.len())
            .any(|window| window.eq_ignore_ascii_case(NESTED))
}

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
        let text = has_text(bytes);
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        if text {
            font_generation().hash(&mut hasher);
        }
        let key = hasher.finish();
        let mut documents = DOCUMENTS.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(document) = documents.get(&key) {
            return Ok(Self {
                document: Arc::clone(document),
            });
        }
        let options = if text {
            text_options()
        } else {
            resvg::usvg::Options::default()
        };
        let tree =
            resvg::usvg::Tree::from_data(bytes, &options).map_err(|error| error.to_string())?;
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

    /// `bytes` in standard base64 with padding, for `data:` URLs.
    fn base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let word = (u32::from(chunk[0]) << 16)
                | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
                | u32::from(*chunk.get(2).unwrap_or(&0));
            for index in 0..4 {
                if index <= chunk.len() {
                    out.push(ALPHABET[((word >> (18 - 6 * index)) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    /// A picture embedded as a `data:` URL is drawn where the document puts it.
    #[test]
    fn draws_embedded_pictures() {
        let green = ImageData::from_rgba(2, 2, [0, 255, 0, 255].repeat(4)).unwrap();
        let png = base64(&green.encode_png().unwrap());
        let document = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image width="10" height="10" href="data:image/png;base64,{png}"/></svg>"#
        );
        let painter = SvgPainter::from_str(&document).unwrap();
        let image = painter.rasterize(10, 10).unwrap();
        assert_eq!(pixel(&image, 5, 5), [0, 255, 0, 255]);
    }

    /// Keeps the tests that hand fonts over from running at once, since the fonts are shared.
    static HANDING_OVER: Mutex<()> = Mutex::new(());

    /// The system's fonts handed over, with "Fallback Sans" for `sans-serif` when the system
    /// has none of the usual families.
    fn hand_over_system_fonts() -> Option<String> {
        let mut system = fontdb::Database::new();
        system.load_system_fonts();
        let wanted = installed(&system, SANS_SERIF);
        let sources = system
            .faces()
            .filter_map(|face| match &face.source {
                fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
                    Some(SvgFontSource::File(path.clone()))
                }
                fontdb::Source::Binary(_) => None,
            })
            .collect();
        set_svg_fonts(sources, Some("Fallback Sans".into()), None, None);
        wanted
    }

    /// `text` percent-encoded for a `data:` URL.
    fn percent_encoded(text: &str) -> String {
        text.bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() {
                    char::from(byte).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect()
    }

    /// The text of an SVG document embedded as a `data:` URL is set with the fonts handed
    /// over, though the outer document has no text element of its own.
    #[test]
    fn nested_documents_draw_their_text() {
        let _alone = HANDING_OVER.lock().unwrap_or_else(PoisonError::into_inner);
        hand_over_system_fonts();
        if fontdb_is_empty() {
            return;
        }
        let nested = |inner: &str| {
            format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="30"><image width="60" height="30" href="data:image/svg+xml,{}"/></svg>"#,
                percent_encoded(inner)
            )
        };
        let lettered = nested(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="30"><text x="2" y="24" font-size="24" font-family="sans-serif" fill="#000000">Hg</text></svg>"##,
        );
        let blank = nested(r#"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="30"/>"#);
        let drawn = SvgPainter::from_str(&lettered)
            .unwrap()
            .rasterize(60, 30)
            .unwrap();
        let empty = SvgPainter::from_str(&blank)
            .unwrap()
            .rasterize(60, 30)
            .unwrap();
        assert_ne!(drawn.pixels(), empty.pixels());
        assert!(drawn.pixels().chunks(4).any(|pixel| pixel[3] > 0));
    }

    /// Whether the fonts handed over hold no face, as on a machine without fonts.
    fn fontdb_is_empty() -> bool {
        text_options().fontdb.is_empty()
    }

    /// Each generic family is set in the first of the usual desktop families the fonts hold,
    /// or in the family handed over when they hold none.
    #[test]
    fn generic_families_follow_the_system() {
        let _alone = HANDING_OVER.lock().unwrap_or_else(PoisonError::into_inner);
        assert_eq!(installed(&fontdb::Database::new(), SANS_SERIF), None);
        let wanted = hand_over_system_fonts();
        let options = text_options();
        let expected = wanted.unwrap_or_else(|| "Fallback Sans".to_owned());
        assert_eq!(
            options.fontdb.family_name(&fontdb::Family::SansSerif),
            expected
        );
    }

    /// A document with text gets a new identity when other fonts are handed over, so it is
    /// set again with them; one without text keeps its identity.
    #[test]
    fn documents_with_text_follow_the_fonts() {
        let _alone = HANDING_OVER.lock().unwrap_or_else(PoisonError::into_inner);
        let text = r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><text x="0" y="15">Hi</text></svg>"#;
        let before = SvgPainter::from_str(text).unwrap();
        let shape = SvgPainter::from_str(HALF).unwrap();
        set_svg_fonts(Vec::new(), Some("Sans".into()), None, None);
        assert_ne!(SvgPainter::from_str(text).unwrap(), before);
        assert_eq!(SvgPainter::from_str(HALF).unwrap(), shape);
    }
}
