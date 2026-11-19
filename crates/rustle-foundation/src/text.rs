use rustle_graphics::{Color, ImageData};
use rustle_runtime::CompositionLocal;
use rustle_text::{TextSpan, TextStyle};
use rustle_ui::{Content, ContentScale, Modifier, Node, canvas, emit};

use crate::ellipsized::text_ellipsized;
use crate::selection::{in_selection_container, selectable_modifier};

/// The style `text` is set in. Provide another value to restyle everything below.
pub static LOCAL_TEXT_STYLE: CompositionLocal<TextStyle> =
    CompositionLocal::new(TextStyle::default);

/// Whether the system is set to a dark appearance. Provided by the host and
/// kept up to date as the setting changes.
pub static LOCAL_SYSTEM_DARK: CompositionLocal<bool> = CompositionLocal::new(|| true);

/// Whether the system is set to a dark appearance.
pub fn is_system_in_dark_theme() -> bool {
    LOCAL_SYSTEM_DARK.current()
}

/// Emits a line of text in the current [`LOCAL_TEXT_STYLE`].
pub fn text(content: impl Into<String>) {
    text_with(Modifier::new(), content);
}

/// Emits a line of text with `modifier`, in the current [`LOCAL_TEXT_STYLE`].
pub fn text_with(modifier: Modifier, content: impl Into<String>) {
    text_styled(modifier, content, LOCAL_TEXT_STYLE.current());
}

/// Emits a line of text with `modifier`, set in `style`. Inside a
/// [`selection_container`](crate::selection_container) it takes part in the selection.
pub fn text_styled(modifier: Modifier, content: impl Into<String>, style: TextStyle) {
    let content: String = content.into();
    let modifier = selectable_modifier(modifier, content.as_str());
    emit(Node {
        modifier,
        content: Content::Text {
            content,
            style,
            editing: None,
        },
    });
}

/// How text that does not fit its box shows, as Compose's `TextOverflow`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextOverflow {
    /// What does not fit is cut off at the box.
    #[default]
    Clip,
    /// A single line that does not fit ends early with an ellipsis.
    Ellipsis,
}

/// Emits `content` set in `style` with `modifier`, overflowing its box as `overflow` says:
/// Compose's `BasicText(text, modifier, style, overflow, maxLines)`, the line limit being
/// the style's `max_lines`.
///
/// With [`TextOverflow::Ellipsis`] a style of at most one line, for content of one line,
/// is set as [`text_ellipsized`] sets it; any other text is set as [`text_styled`] sets it.
/// Inside a [`selection_container`](crate::selection_container) it takes part in the
/// selection, text cut short with the part that shows.
#[track_caller]
pub fn basic_text(
    modifier: Modifier,
    content: impl Into<String>,
    style: TextStyle,
    overflow: TextOverflow,
) {
    let content: String = content.into();
    if overflow == TextOverflow::Ellipsis && style.max_lines == Some(1) && !content.contains('\n') {
        text_ellipsized(modifier, content, style);
        return;
    }
    text_styled(modifier, content, style);
}

/// Emits a picture, sized to its pixels unless `modifier` or the space available says otherwise.
/// `scale` says how it fills an area of a different shape.
#[track_caller]
pub fn image(modifier: Modifier, image: &ImageData, scale: ContentScale) {
    emit(Node {
        modifier,
        content: Content::Image {
            image: image.clone(),
            scale,
        },
    });
}

/// Emits a coverage mask, such as an icon the program rasterised itself, filled with `color`.
/// Only the alpha of `mask`'s pixels is used (see [`ImageData::from_alpha`]).
///
/// It is as many logical pixels as `mask` has pixels unless `modifier` sizes it, and the mask
/// is stretched over the area. A mask rasterised at the area's size times the scale factor is
/// drawn texel for texel from a whole physical pixel, so icons stay sharp.
#[track_caller]
pub fn mask_image(modifier: Modifier, mask: &ImageData, color: Color) {
    let natural = (mask.width() as f32, mask.height() as f32);
    let mask = mask.clone();
    canvas(modifier.size(natural.0, natural.1), move |canvas| {
        let (width, height) = (canvas.width(), canvas.height());
        canvas.mask(&mask, 0.0, 0.0, width, height, color);
    });
}

/// Emits text made of runs in their own colors, weights, fonts, slants, sizes and
/// letter spacing, with underlines, strikethroughs and backgrounds, set in the
/// current [`LOCAL_TEXT_STYLE`]. Long lines wrap at the width available.
#[track_caller]
pub fn rich_text(spans: Vec<TextSpan>) {
    rich_text_with(Modifier::new(), spans);
}

/// Emits rich text with `modifier`. Inside a
/// [`selection_container`](crate::selection_container) it takes part in the selection.
#[track_caller]
pub fn rich_text_with(modifier: Modifier, spans: Vec<TextSpan>) {
    let style = LOCAL_TEXT_STYLE.current();
    let modifier = if in_selection_container() {
        let text: String = spans.iter().map(|span| span.text.as_str()).collect();
        selectable_modifier(modifier, text)
    } else {
        modifier
    };
    emit(Node {
        modifier,
        content: Content::RichText {
            spans,
            style,
            editing: None,
        },
    });
}
