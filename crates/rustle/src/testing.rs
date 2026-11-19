//! Helpers for tests that compose an interface and lay it out without a window or a GPU.

use rustle_geometry::Size;
use rustle_graphics::StyledSpan;
use rustle_text::{LineLayout, TextLayout, TextMeasurer, TextStyle};
use rustle_ui::{DisplayList, composition, display_list};
use rustle_unit::Constraints;

/// A measurer with eight pixels per character and a single line.
pub(crate) struct Fixed;

impl TextMeasurer for Fixed {
    /// Returns eight pixels per character.
    fn measure(&mut self, content: &str, _: &TextStyle, _: f32) -> Size {
        Size::new(content.chars().count() as f32 * 8.0, 16.0)
    }

    /// Measures the joined text of `spans`.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, width: f32) -> Size {
        let content: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.measure(&content, style, width)
    }

    /// Returns one empty line; nothing in these tests reads it.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, _: f32) -> TextLayout {
        let line = LineLayout {
            start: 0,
            end: 0,
            top: 0.0,
            height: 16.0,
            baseline: 12.0,
            width: 0.0,
            rtl: false,
            hard_break: true,
            clusters: Vec::new(),
        };
        TextLayout::new(String::new(), spans.to_vec(), style.font_size, vec![line])
    }
}

/// Composes `content` once and lays it out in 400 by 300 logical pixels.
pub(crate) fn display_of(content: impl FnMut() + 'static) -> DisplayList {
    let mut composition = composition(content);
    composition.compose();
    display_list(
        composition.output().expect("something composed"),
        Constraints::loose(Size::new(400.0, 300.0)),
        &mut Fixed,
    )
}

/// A headless interface of `width` by `height` logical pixels with `root` composed and laid
/// out in it.
pub(crate) fn embedded(width: u32, height: u32, root: impl FnMut() + 'static) -> crate::Embedded {
    let mut embedded = crate::Embedded::without_renderer(root, || {});
    embedded.resize((width, height), 1.0);
    embedded.update();
    embedded
}

/// Emits a stack with `modifier`, its children at its top-left corner.
#[track_caller]
pub(crate) fn stack_sized(modifier: rustle_ui::Modifier, content: impl FnOnce()) {
    rustle_foundation::stack_with(modifier, content);
}
