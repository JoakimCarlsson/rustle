//! The text engine a renderer installs, and the functions that reach it.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Mutex;

use crate::{StyledSpan, TextLayout, TextSpan, TextStyle};

/// Shapes text and knows the fonts. The renderer implements this and installs it
/// with [`install_text_engine`] when it is created.
pub trait TextEngine {
    /// `spans` shaped and broken into lines at `max_width` (no wrapping if `None`).
    fn layout(&self, spans: &[StyledSpan], style: &TextStyle, max_width: Option<f32>)
    -> TextLayout;

    /// The names of the installed and registered font families, sorted, without repeats.
    fn font_families(&self) -> Vec<String>;

    /// Adds the font file in `data` (TrueType, OpenType or a collection of them) and
    /// returns the family names it provides.
    fn register_font(&self, data: Vec<u8>) -> Vec<String>;
}

thread_local! {
    static ENGINE: RefCell<Option<Rc<dyn TextEngine>>> = const { RefCell::new(None) };
}

/// Font data registered before any renderer exists, which the renderer adds when it starts.
static PENDING: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Makes `engine` the one the text functions use on this thread. Renderers call this
/// when they are created.
pub fn install_text_engine(engine: Rc<dyn TextEngine>) {
    ENGINE.with(|slot| *slot.borrow_mut() = Some(engine));
}

/// The font data [`register_font`] queued because no renderer existed yet, taken from the queue.
/// Renderers call this after installing their engine.
pub fn take_pending_fonts() -> Vec<Vec<u8>> {
    std::mem::take(
        &mut *PENDING
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    )
}

fn engine() -> Option<Rc<dyn TextEngine>> {
    ENGINE.with(|slot| slot.borrow().clone())
}

/// Lays out `content` set in `style`, wrapped at `max_width` if there is one.
///
/// This is the glyph-level measurement an app uses to build its own editor: line
/// breaks and metrics, [`TextLayout::caret`] for where a caret is drawn and
/// [`TextLayout::hit_test`] for the offset under a point. It can be called from a
/// composable or a `canvas` closure. Before a renderer exists the layout is empty.
pub fn layout_text(content: &str, style: &TextStyle, max_width: Option<f32>) -> TextLayout {
    layout_spans(&[TextSpan::new(content)], style, max_width)
}

/// Like [`layout_text`] for rich text: runs that differ in color, weight, family,
/// slant, size and letter spacing, set one after another.
pub fn layout_spans(spans: &[TextSpan], style: &TextStyle, max_width: Option<f32>) -> TextLayout {
    let Some(engine) = engine() else {
        return TextLayout::empty();
    };
    let resolved: Vec<StyledSpan> = spans.iter().map(|span| span.resolve(style)).collect();
    engine.layout(&resolved, style, max_width)
}

/// Like [`layout_spans`] for spans whose style is already resolved.
pub fn layout_styled(
    spans: &[StyledSpan],
    style: &TextStyle,
    max_width: Option<f32>,
) -> TextLayout {
    match engine() {
        Some(engine) => engine.layout(spans, style, max_width),
        None => TextLayout::empty(),
    }
}

/// The names of the installed system font families and registered ones, sorted.
/// Empty before a renderer exists.
pub fn font_families() -> Vec<String> {
    engine()
        .map(|engine| engine.font_families())
        .unwrap_or_default()
}

/// Adds font data (a TrueType or OpenType file, for example from `include_bytes!`) so
/// its families can be chosen with `FontFamily::Named`. Returns the family names it
/// provides. Before a renderer exists the data is kept and added when it starts, and
/// the result is empty. Text shaped earlier is shaped again with the new font.
pub fn register_font(data: Vec<u8>) -> Vec<String> {
    match engine() {
        Some(engine) => engine.register_font(data),
        None => {
            PENDING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(data);
            Vec::new()
        }
    }
}
