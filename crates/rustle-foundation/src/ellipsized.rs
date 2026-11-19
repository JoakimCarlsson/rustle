//! A line of text cut short at its box with an ellipsis, as Compose's `Text` with
//! `maxLines = 1`, `softWrap = false` and `overflow = TextOverflow.Ellipsis` draws it.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_text::{TextStyle, ellipsize, layout_text};
use rustle_ui::{Children, Layout, LayoutResult, Modifier, canvas, layout};
use rustle_unit::Constraints;

use crate::selection::{in_selection_container, taking_part};

/// Lays out a line cut short at its box: as wide as its words when they fit, else as wide as
/// it may be, one line tall, its drawing filling it.
struct Ellipsized {
    /// How wide the whole line is.
    natural: f32,
    /// How tall a line is.
    height: f32,
}

impl Layout for Ellipsized {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let size = Size::new(
            self.natural
                .ceil()
                .min(constraints.max_width)
                .max(constraints.min_width),
            self.height
                .min(constraints.max_height)
                .max(constraints.min_height),
        );
        for index in 0..children.len() {
            children.measure(index, Constraints::tight(size));
        }
        LayoutResult {
            size,
            positions: vec![Offset::new(0.0, 0.0); children.len()],
        }
    }

    fn min_intrinsic_width(&self, _: &mut Children, _: f32) -> f32 {
        0.0
    }

    fn max_intrinsic_width(&self, _: &mut Children, _: f32) -> f32 {
        self.natural.ceil()
    }

    fn min_intrinsic_height(&self, _: &mut Children, _: f32) -> f32 {
        self.height
    }

    fn max_intrinsic_height(&self, _: &mut Children, _: f32) -> f32 {
        self.height
    }
}

/// Emits one line of `content` set in `style`, as wide as it is or as its box allows, and cut
/// short when it does not fit: the most of it that fits with an ellipsis after it, the spaces before the
/// ellipsis left out (see [`ellipsize`]). The line is one line of `style` tall. Inside a
/// [`selection_container`](crate::selection_container) the part that shows, without the
/// ellipsis, takes part in the selection, as Compose selects the visible text of a `Text`
/// cut short.
#[track_caller]
pub fn text_ellipsized(modifier: Modifier, content: impl Into<String>, style: TextStyle) {
    let content: String = content.into();
    let style = TextStyle {
        max_lines: None,
        ..style
    };
    let natural = layout_text(&content, &style, None).size().width;
    let height = style.line_height;
    let cut: RefCell<Option<(f32, String)>> = RefCell::new(None);
    let modifier = if in_selection_container() {
        taking_part(modifier, visible_part(content.clone(), style))
    } else {
        modifier
    };
    layout(modifier, Ellipsized { natural, height }, move || {
        canvas(Modifier::new().fill_max_size(), move |canvas| {
            let room = canvas.width();
            let mut held = cut.borrow_mut();
            if held.as_ref().is_none_or(|(width, _)| *width != room) {
                *held = Some((room, ellipsize(&content, &style, room, 1.0)));
            }
            if let Some((_, words)) = held.as_ref() {
                canvas.draw_text(0.0, 0.0, words, &style, None);
            }
        });
    });
}

/// What of `content`, set in `style` and cut short at the width of the rectangle it is
/// placed at, shows, without the ellipsis, with its shaped text: kept for the last width.
fn visible_part(content: String, style: TextStyle) -> crate::selection::Shaped {
    let held: RefCell<Option<(f32, crate::selection::ShapedText)>> = RefCell::new(None);
    Rc::new(move |rect: Rect| {
        let room = rect.size.width;
        let mut held = held.borrow_mut();
        if held.as_ref().is_none_or(|(width, _)| *width != room) {
            let words = ellipsize(&content, &style, room, 1.0);
            let shown = match words == content {
                true => words.as_str(),
                false => words.strip_suffix('\u{2026}').unwrap_or(&words),
            };
            let layout = Rc::new(layout_text(shown, &style, None));
            *held = Some((room, (Rc::from(shown), layout)));
        }
        held.as_ref().map(|(_, shaped)| shaped.clone())
    })
}
