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
        grid: None,
        focus: None,
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

/// The rectangle of the background drawn for an area offset to (10.4, 0.6) and sized 5.3 by
/// 5.3, painted with `options`.
fn offset_background(options: DisplayOptions) -> Rect {
    let root = Node {
        modifier: Modifier::new()
            .absolute_offset(10.4, 0.6)
            .size(5.3, 5.3)
            .background(Color::rgba(10, 20, 30, 255)),
        content: Content::Canvas {
            draw: Rc::new(|_| Vec::new()),
        },
    };
    let list = display_list_with(
        &root,
        Constraints::loose(Size::new(100.0, 100.0)),
        &mut FixedText,
        options,
    );
    list.commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the background is drawn")
}

/// Leaves areas at their fractional places unless rounding is asked for.
#[test]
fn pixel_rounding_is_off_by_default() {
    let rect = offset_background(DisplayOptions::default());
    assert_eq!(rect.origin, Offset::new(10.4, 0.6));
    assert_eq!(rect.size, Size::new(5.3, 5.3));
}

/// Rounds an area's edges to whole logical pixels, its size being the distance between them.
#[test]
fn logical_rounding_places_edges_on_whole_pixels() {
    let rect = offset_background(DisplayOptions {
        rounding: PixelRounding::Logical,
        scale: 2.0,
    });
    assert_eq!(rect.origin, Offset::new(10.0, 1.0));
    assert_eq!(rect.size, Size::new(6.0, 5.0));
}

/// Rounds an area's edges to whole physical pixels at the scale given.
#[test]
fn physical_rounding_places_edges_on_device_pixels() {
    let rect = offset_background(DisplayOptions {
        rounding: PixelRounding::Physical,
        scale: 2.0,
    });
    assert_eq!(rect.origin, Offset::new(10.5, 0.5));
    assert_eq!(rect.size, Size::new(5.0, 5.5));
}

/// Places children from their parent's unrounded place, so rounding does not add up.
#[test]
fn rounding_does_not_accumulate_down_the_tree() {
    let mut node = Node {
        modifier: Modifier::new()
            .size(1.0, 1.0)
            .background(Color::rgba(1, 2, 3, 255)),
        content: Content::Canvas {
            draw: Rc::new(|_| Vec::new()),
        },
    };
    for _ in 0..5 {
        node = Node {
            modifier: Modifier::new().absolute_offset(0.4, 0.0),
            content: Content::Layout {
                policy: Rc::new(crate::Overlay::default()),
                children: Rc::new(vec![node]),
            },
        };
    }
    let list = display_list_with(
        &node,
        Constraints::loose(Size::new(100.0, 100.0)),
        &mut FixedText,
        DisplayOptions {
            rounding: PixelRounding::Logical,
            scale: 1.0,
        },
    );
    let rect = list
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the background is drawn");
    assert_eq!(rect.origin.x, 2.0);
}

/// Editing details for rich text with only the caret, composition and input place given.
fn composing(
    caret: Option<usize>,
    underline: Option<(usize, usize)>,
    ime_caret: Option<usize>,
) -> TextEditing {
    TextEditing {
        caret,
        selection: None,
        underline,
        ime_caret,
        caret_color: Color::rgba(1, 2, 3, 255),
        selection_color: Color::TRANSPARENT,
        caret_affinity: rustle_text::Affinity::Downstream,
        boundaries: Rc::new(RefCell::new(Vec::new())),
        layout: Rc::new(RefCell::new(None)),
        on_layout: None,
    }
}

/// Lays out rich text "abcd" edited as `editing`.
fn edited_rich_text(editing: TextEditing) -> DisplayList {
    let root = Node {
        modifier: Modifier::new(),
        content: Content::RichText {
            spans: vec![TextSpan::new("ab"), TextSpan::new("cd")],
            style: TextStyle::default(),
            editing: Some(editing),
        },
    };
    display_list(
        &root,
        Constraints::loose(Size::new(100.0, 100.0)),
        &mut FixedText,
    )
}

/// Draws the caret of edited rich text as the same thin bar plain text gets.
#[test]
fn rich_text_draws_its_caret() {
    let list = edited_rich_text(composing(Some(2), None, None));
    let [
        DrawCommand::RichText { .. },
        DrawCommand::Rect { rect, color, .. },
    ] = list.commands.as_slice()
    else {
        panic!("expected rich text followed by its caret");
    };
    assert_eq!(
        *rect,
        Rect::new(Offset::new(15.25, 0.0), Size::new(1.5, 16.0))
    );
    assert_eq!(*color, Color::rgba(1, 2, 3, 255));
    assert_eq!(list.ime_area, None);
}

/// Underlines an input method's composition in rich text and reports where input goes.
#[test]
fn rich_text_underlines_a_composition_and_reports_the_input_area() {
    let list = edited_rich_text(composing(None, Some((1, 3)), Some(3)));
    let [DrawCommand::RichText { .. }, DrawCommand::Rect { rect, .. }] = list.commands.as_slice()
    else {
        panic!("expected rich text followed by the composition's underline");
    };
    assert_eq!(
        *rect,
        Rect::new(Offset::new(8.0, 14.0), Size::new(16.0, 1.0))
    );
    assert_eq!(
        list.ime_area,
        Some(Rect::new(Offset::new(8.0, 0.0), Size::new(16.0, 16.0)))
    );
    assert_eq!(list.focused_ime_area(), list.ime_area);
}

/// Composes a 50 by 20 canvas with `modifier`, given the focus remembered for it, and
/// returns the focus and the laid-out list.
fn focused_canvas(
    modifier: impl Fn(&crate::Focus) -> Modifier + 'static,
) -> (crate::Focus, DisplayList) {
    let slot = Rc::new(RefCell::new(None));
    let kept = Rc::clone(&slot);
    let mut composition = crate::composition(move || {
        let focus = crate::remember_focus();
        *kept.borrow_mut() = Some(focus.clone());
        crate::canvas(modifier(&focus).size(50.0, 20.0), |_| {});
    });
    composition.compose();
    let tree = composition.output().cloned().expect("the root composed");
    let list = display_list(
        &tree,
        Constraints::loose(Size::new(100.0, 100.0)),
        &mut FixedText,
    );
    let focus = slot.borrow().clone().expect("a focus was remembered");
    (focus, list)
}

/// Reports an area's own input place only while the focusable around it holds focus.
#[test]
fn an_ime_caret_counts_while_its_focus_is_held() {
    let caret = Rect::new(Offset::new(4.0, 2.0), Size::new(1.0, 12.0));
    let (focus, list) = focused_canvas(move |focus| {
        Modifier::new()
            .padding(10.0)
            .focusable(focus, |_| false)
            .ime_caret(caret)
    });
    assert_eq!(list.ime_area, None);
    assert_eq!(list.focused_ime_area(), None);
    focus.set_focused(true);
    assert_eq!(
        list.focused_ime_area(),
        Some(Rect::new(Offset::new(14.0, 12.0), Size::new(1.0, 12.0)))
    );
    focus.set_focused(false);
    assert_eq!(list.focused_ime_area(), None);
}

/// Takes the focus of the first focusable inside the area when none is around it.
#[test]
fn an_ime_caret_outside_its_focusable_uses_the_one_inside() {
    let caret = Rect::new(Offset::new(4.0, 2.0), Size::new(1.0, 12.0));
    let (focus, list) =
        focused_canvas(move |focus| Modifier::new().ime_caret(caret).focusable(focus, |_| false));
    focus.set_focused(true);
    assert_eq!(list.focused_ime_area(), Some(caret));
}
