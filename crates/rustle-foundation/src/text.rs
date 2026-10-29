use rustle_graphics::ImageData;
use rustle_runtime::CompositionLocal;
use rustle_text::{TextSpan, TextStyle};
use rustle_ui::{Content, ContentScale, Modifier, Node, emit};

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

/// Emits a line of text with `modifier`, set in `style`.
pub fn text_styled(modifier: Modifier, content: impl Into<String>, style: TextStyle) {
    emit(Node {
        modifier,
        content: Content::Text {
            content: content.into(),
            style,
            editing: None,
        },
    });
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

/// Emits a line of text made of runs in their own colors, weights or fonts,
/// set in the current [`LOCAL_TEXT_STYLE`].
#[track_caller]
pub fn rich_text(spans: Vec<TextSpan>) {
    rich_text_with(Modifier::new(), spans);
}

/// Emits rich text with `modifier`.
#[track_caller]
pub fn rich_text_with(modifier: Modifier, spans: Vec<TextSpan>) {
    emit(Node {
        modifier,
        content: Content::RichText {
            spans,
            style: LOCAL_TEXT_STYLE.current(),
        },
    });
}
