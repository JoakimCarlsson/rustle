//! A cached layout of a restartable block is measured again when fonts change.
//!
//! Kept in its own test binary: registering a font changes a process-wide epoch that every
//! cached layout in the process sees.

use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::Size;
use rustle_text::{LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle, register_font};
use rustle_ui::{Content, Modifier, Node, composition, display_list, emit, restartable};
use rustle_unit::Constraints;

/// A measurer that counts its calls.
struct Counting(Rc<Cell<usize>>);

impl TextMeasurer for Counting {
    /// Counts the call and returns eight pixels per character.
    fn measure(&mut self, content: &str, _: &TextStyle, _: f32) -> Size {
        self.0.set(self.0.get() + 1);
        Size::new(content.chars().count() as f32 * 8.0, 16.0)
    }

    /// Measures the joined text of `spans`.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, width: f32) -> Size {
        let content: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.measure(&content, style, width)
    }

    /// Returns one empty line; nothing here reads it.
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

/// Registering a font makes the next layout measure text again, once.
#[test]
fn registering_a_font_invalidates_cached_layouts() {
    let mut composition = composition(|| {
        restartable((), || {
            emit(Node {
                modifier: Modifier::new(),
                content: Content::Text {
                    content: "cached".to_string(),
                    style: TextStyle::default(),
                    editing: None,
                },
            });
        });
    });
    composition.compose();
    let calls = Rc::new(Cell::new(0));
    let mut measurer = Counting(Rc::clone(&calls));
    let constraints = Constraints::loose(Size::new(200.0, 100.0));
    display_list(composition.output().unwrap(), constraints, &mut measurer);
    display_list(composition.output().unwrap(), constraints, &mut measurer);
    assert_eq!(calls.get(), 1);
    register_font(Vec::new());
    display_list(composition.output().unwrap(), constraints, &mut measurer);
    assert_eq!(calls.get(), 2);
    display_list(composition.output().unwrap(), constraints, &mut measurer);
    assert_eq!(calls.get(), 2);
}
