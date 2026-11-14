//! Regression tests for transferring rich text from measurement to owned drawing commands.

use std::cell::{Cell, RefCell};

use rustle_text::{Cluster, LineLayout, StyledSpan, TextLayout, TextStyle};

use super::*;
use crate::{Content, Modifier, TextEditing};

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

    /// Shapes `spans` into one line with one cluster per Unicode character.
    fn layout(&mut self, spans: &[StyledSpan], style: &TextStyle, _: f32) -> TextLayout {
        let content: String = spans.iter().map(|span| span.text.as_str()).collect();
        let clusters: Vec<_> = content
            .char_indices()
            .enumerate()
            .map(|(index, (start, character))| Cluster {
                start,
                end: start + character.len_utf8(),
                x: index as f32 * 8.0,
                width: 8.0,
                rtl: false,
                span: 0,
            })
            .collect();
        let line = LineLayout {
            start: 0,
            end: content.len(),
            top: 0.0,
            height: 16.0,
            baseline: 12.0,
            width: clusters.len() as f32 * 8.0,
            rtl: false,
            hard_break: true,
            clusters,
        };
        TextLayout::new(content, spans.to_vec(), style.font_size, vec![line])
    }
}

/// Paints a measured tree once, including all deferred descendants.
fn paint_once(measured: &Measured) -> DisplayList {
    let mut painter = Painter {
        pending_end: None,
        deferred: Vec::new(),
        alpha: 1.0,
        merging: 0,
        clearing: 0,
        parents: Vec::new(),
        merged_text: Vec::new(),
        merged_labels: Vec::new(),
        list: DisplayList::default(),
        clips: Vec::new(),
        clip_radii: Vec::new(),
        window: measured.size,
    };
    painter.paint(measured, Offset::default());
    painter.paint_deferred();
    painter.list
}

/// Ensures painting rich text copies the measured spans, so a cached measurement paints again.
#[test]
fn rich_text_paints_again_from_the_same_measurement() {
    let style = TextStyle::default();
    let spans = vec![
        TextSpan::new("hello ").resolve(&style),
        TextSpan::new("世界").bold().italic().resolve(&style),
    ];
    let expected = spans.clone();
    let measured = Measured {
        size: Size::new(64.0, 16.0),
        paint: Paint::RichText {
            spans,
            style,
            editing: None,
        },
    };
    let first = paint_once(&measured);
    let second = paint_once(&measured);
    for list in [&first, &second] {
        let [DrawCommand::RichText { spans, .. }] = list.commands.as_slice() else {
            panic!("expected one rich text command");
        };
        assert_eq!(spans, &expected);
    }
}

/// Preserves padded placement, alpha, clips and metadata for clipped and unclipped raised text.
#[test]
fn deferred_rich_text_preserves_modifiers_and_offscreen_metadata() {
    for clipped in [true, false] {
        let placed = Rc::new(RefCell::new(Vec::new()));
        let recorded = Rc::clone(&placed);
        let modifier = Modifier::new()
            .size(20.0, 20.0)
            .clip_rounded(3.0)
            .padding(4.0)
            .absolute_offset(30.0, 0.0)
            .alpha(0.5);
        let modifier = if clipped {
            modifier.z_index(2)
        } else {
            modifier.z_index_unclipped(2)
        };
        let color = Color::rgba(20, 40, 60, 200);
        let root = Node {
            modifier: modifier.on_placed(move |rect, window| {
                recorded.borrow_mut().push((rect, window));
            }),
            content: Content::RichText {
                spans: vec![TextSpan::new("outside").color(color)],
                style: TextStyle::default(),
                editing: None,
            },
        };
        let list = display_list(
            &root,
            Constraints::loose(Size::new(100.0, 100.0)),
            &mut FixedText,
        );
        drop(root);
        let expected_rect = Rect::new(Offset::new(34.0, 4.0), Size::new(12.0, 12.0));
        assert_eq!(
            *placed.borrow(),
            vec![(expected_rect, Size::new(100.0, 100.0))]
        );
        assert_eq!(list.commands.len(), if clipped { 5 } else { 3 });
        assert!(matches!(
            list.commands[0],
            DrawCommand::PushRoundedClip { .. }
        ));
        assert!(matches!(list.commands[1], DrawCommand::PopClip));
        let command = &list.commands[if clipped { 3 } else { 2 }];
        let DrawCommand::RichText {
            x, y, width, spans, ..
        } = command
        else {
            panic!("expected deferred rich text");
        };
        assert_eq!((*x, *y, *width), (34.0, 4.0, 12.0));
        assert_eq!(spans[0].text, "outside");
        assert_eq!(spans[0].color, Color::rgba(20, 40, 60, 100));
        assert_eq!(list.semantics.len(), 1);
        assert_eq!(
            list.semantics[0].semantics.label.as_deref(),
            Some("outside")
        );
        if clipped {
            assert_eq!(list.semantics[0].rect.size.width, 0.0);
            assert!(matches!(list.commands[4], DrawCommand::PopClip));
        } else {
            assert_eq!(list.semantics[0].rect, expected_rect);
        }
    }
}

/// Keeps Unicode selection byte conversion and layout callbacks intact across repeated layout.
#[test]
fn selected_rich_text_can_be_laid_out_again() {
    let callbacks = Rc::new(Cell::new(0));
    let recorded = Rc::clone(&callbacks);
    let selection_color = Color::rgba(10, 20, 30, 80);
    let editing = TextEditing {
        caret: None,
        selection: Some((1, 2)),
        underline: None,
        ime_caret: None,
        caret_color: Color::rgb(0, 0, 0),
        selection_color,
        caret_affinity: rustle_text::Affinity::Downstream,
        boundaries: Rc::new(RefCell::new(Vec::new())),
        layout: Rc::new(RefCell::new(None)),
        on_layout: Some(Rc::new(move || recorded.set(recorded.get() + 1))),
    };
    let boundaries = Rc::clone(&editing.boundaries);
    let root = Node {
        modifier: Modifier::new(),
        content: Content::RichText {
            spans: vec![TextSpan::new("é"), TextSpan::new("界x")],
            style: TextStyle::default(),
            editing: Some(editing),
        },
    };
    for _ in 0..2 {
        let list = display_list(
            &root,
            Constraints::loose(Size::new(100.0, 100.0)),
            &mut FixedText,
        );
        let [
            DrawCommand::Rect { rect, color, .. },
            DrawCommand::RichText { spans, .. },
        ] = list.commands.as_slice()
        else {
            panic!("expected selection followed by rich text");
        };
        assert_eq!(
            *rect,
            Rect::new(Offset::new(8.0, 0.0), Size::new(8.0, 16.0))
        );
        assert_eq!(*color, selection_color);
        assert_eq!(spans[0].text, "é");
        assert_eq!(spans[1].text, "界x");
        assert_eq!(list.semantics[0].semantics.label.as_deref(), Some("é界x"));
    }
    assert_eq!(callbacks.get(), 2);
    assert_eq!(boundaries.borrow().len(), 4);
}
