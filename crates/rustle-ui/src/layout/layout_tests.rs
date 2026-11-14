//! Tests for custom layouts, intrinsic sizes and subcomposition.

use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_text::{Cluster, LineLayout, StyledSpan, TextLayout, TextMeasurer, TextStyle};
use rustle_unit::Constraints;

use super::measure::measure_node;
use super::{IntrinsicSize, Overlay, Placement, SubcomposeResult, subcompose_layout};
use crate::{Content, Modifier, Node, composition, emit};

/// A deterministic single-line measurer with eight logical pixels per character.
struct FixedText;

impl TextMeasurer for FixedText {
    /// Returns the size of `content`, ignoring wrapping and style for this fixture.
    fn measure(&mut self, content: &str, _: &TextStyle, _: f32) -> Size {
        Size::new(content.chars().count() as f32 * 8.0, 16.0)
    }

    /// Returns the combined size of `spans` using the fixture's character advance.
    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, width: f32) -> Size {
        let content: String = spans.iter().map(|span| span.text.as_str()).collect();
        self.measure(&content, style, width)
    }

    /// Shapes `spans` into one empty line; nothing here reads it.
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
            clusters: Vec::<Cluster>::new(),
        };
        TextLayout::new(String::new(), spans.to_vec(), style.font_size, vec![line])
    }
}

/// A text node with `modifier`.
fn label(modifier: Modifier, content: &str) -> Node {
    Node {
        modifier,
        content: Content::Text {
            content: content.to_string(),
            style: TextStyle::default(),
            editing: None,
        },
    }
}

/// A stack of `children` with `modifier`.
fn stack(modifier: Modifier, children: Vec<Node>) -> Node {
    Node {
        modifier,
        content: Content::Layout {
            policy: Rc::new(Overlay::default()),
            children: Rc::new(children),
        },
    }
}

/// Measures `node` loosely within a 400 by 400 window.
fn measure(node: &Node) -> Size {
    let constraints = Constraints::loose(Size::new(400.0, 400.0));
    measure_node(node, constraints, &mut FixedText).size
}

/// A layout modifier decides the size and where the content goes.
#[test]
fn layout_modifier_sizes_and_places_content() {
    let modifier = Modifier::new().layout(|measurable, constraints| {
        let size = measurable.measure(constraints);
        Placement::new(
            Size::new(size.width + 10.0, size.height + 20.0),
            Offset::new(10.0, 20.0),
        )
    });
    assert_eq!(measure(&label(modifier, "abcd")), Size::new(42.0, 36.0));
}

/// An intrinsic width modifier sizes a stack to its widest child.
#[test]
fn intrinsic_width_takes_the_widest_child() {
    let children = vec![
        label(Modifier::new().fill_max_width(), "ab"),
        label(Modifier::new(), "abcdef"),
    ];
    let wide = stack(Modifier::new(), children.clone());
    assert_eq!(measure(&wide).width, 400.0);
    let intrinsic = stack(
        Modifier::new().width_intrinsic(IntrinsicSize::Max),
        children,
    );
    assert_eq!(measure(&intrinsic).width, 48.0);
}

/// Intrinsic size looks through padding and fixed sizes.
#[test]
fn intrinsic_size_includes_padding() {
    let node = stack(
        Modifier::new()
            .height_intrinsic(IntrinsicSize::Max)
            .padding(4.0),
        vec![label(Modifier::new(), "abc")],
    );
    assert_eq!(measure(&node), Size::new(32.0, 24.0));
}

/// Content composed during measurement is measured, kept between measurements and disposed of.
#[test]
fn subcomposed_children_are_measured() {
    let mut composition = composition(|| {
        subcompose_layout(Modifier::new(), |scope, constraints| {
            let wide = constraints.max_width > 100.0;
            let indices = scope.subcompose(0_u8, move || {
                emit(label(
                    Modifier::new(),
                    if wide { "wide!" } else { "narrow" },
                ));
            });
            let size = scope.measure(indices[0], constraints);
            SubcomposeResult {
                size,
                placements: vec![(indices[0], Offset::default())],
            }
        });
    });
    composition.compose();
    let root = composition.output().expect("composed");
    assert_eq!(measure(root), Size::new(40.0, 16.0));
    assert_eq!(measure(root), Size::new(40.0, 16.0));
    let narrow = Constraints::loose(Size::new(50.0, 50.0));
    assert_eq!(
        measure_node(root, narrow, &mut FixedText).size,
        Size::new(48.0, 16.0)
    );
}
