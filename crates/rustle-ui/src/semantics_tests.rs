//! Tests of the semantic tree a display list carries: nesting, merging, clearing, headings,
//! live regions, actions and reading order.

use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::Size;
use rustle_graphics::{ImageData, StyledSpan};
use rustle_text::{LineLayout, TextLayout, TextMeasurer, TextStyle};
use rustle_unit::Constraints;

use super::*;

/// A measurer with eight pixels per character and one line.
struct Fixed;

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

/// Emits a text node.
fn label(value: &str) {
    emit(Node {
        modifier: Modifier::new(),
        content: Content::Text {
            content: value.to_string(),
            style: TextStyle::default(),
            editing: None,
        },
    });
}

/// Composes `content` as the child of a stack and lays it out in 300 by 200 pixels.
fn describe(content: impl Fn() + 'static) -> DisplayList {
    let mut composition = composition(move || {
        layout(Modifier::new(), Overlay::default(), &content);
    });
    composition.compose();
    display_list(
        composition.output().unwrap(),
        Constraints::loose(Size::new(300.0, 200.0)),
        &mut Fixed,
    )
}

/// The labels of the described areas, in order.
fn labels(list: &DisplayList) -> Vec<Option<String>> {
    list.semantics
        .iter()
        .map(|node| node.semantics.label.clone())
        .collect()
}

/// A stack with `semantics` over `content`.
fn described(semantics: Semantics, content: impl FnOnce()) {
    layout(
        Modifier::new().semantics(semantics),
        Overlay::default(),
        content,
    );
}

#[test]
fn text_is_a_label_of_its_own() {
    let list = describe(|| label("Hello"));
    assert_eq!(labels(&list), [Some("Hello".to_string())]);
    assert_eq!(list.semantics[0].semantics.role, Role::Label);
    assert_eq!(list.semantics[0].parent, None);
}

#[test]
fn an_area_described_inside_another_is_its_child() {
    let list = describe(|| {
        described(Semantics::new(Role::Group).label("outer"), || {
            label("one");
            described(Semantics::new(Role::Group).label("inner"), || label("two"));
        });
        label("after");
    });
    assert_eq!(
        labels(&list),
        ["outer", "one", "inner", "two", "after"].map(|label| Some(label.to_string()))
    );
    let parents: Vec<_> = list.semantics.iter().map(|node| node.parent).collect();
    assert_eq!(parents, [None, Some(0), Some(0), Some(2), None]);
}

#[test]
fn parents_come_before_their_children() {
    let list = describe(|| {
        described(Semantics::new(Role::Group), || {
            described(Semantics::new(Role::Group), || {
                described(Semantics::new(Role::Group), || label("deep"));
            });
        });
    });
    for (index, node) in list.semantics.iter().enumerate() {
        assert!(node.parent.is_none_or(|parent| parent < index));
    }
    assert_eq!(list.semantics.len(), 4);
}

#[test]
fn merging_descendants_makes_their_text_the_label() {
    let list = describe(|| {
        described(Semantics::new(Role::Button).merging_descendants(), || {
            label("Save");
            label("changes");
        });
    });
    assert_eq!(labels(&list), [Some("Save changes".to_string())]);
    assert_eq!(list.semantics[0].semantics.role, Role::Button);
}

#[test]
fn a_label_given_explicitly_is_kept_when_merging() {
    let list = describe(|| {
        described(
            Semantics::new(Role::Button)
                .label("Close")
                .merging_descendants(),
            || label("x"),
        );
    });
    assert_eq!(labels(&list), [Some("Close".to_string())]);
}

#[test]
fn merging_leaves_other_areas_alone() {
    let list = describe(|| {
        described(Semantics::new(Role::Button).merging_descendants(), || {
            label("inside")
        });
        label("outside");
    });
    assert_eq!(
        labels(&list),
        [Some("inside".to_string()), Some("outside".to_string())]
    );
    assert_eq!(list.semantics[1].parent, None);
}

#[test]
fn merging_swallows_areas_described_inside_it() {
    let list = describe(|| {
        described(Semantics::new(Role::Button).merging_descendants(), || {
            label("a");
            described(Semantics::new(Role::Image).label("icon"), || {});
            label("b");
        });
    });
    let button = &list.semantics[0];
    assert_eq!(button.semantics.label.as_deref(), Some("a b"));
}

#[test]
fn a_picture_inside_a_merge_is_not_described_apart() {
    let image = ImageData::from_rgba(1, 1, vec![0, 0, 0, 255]).unwrap();
    let list = describe(move || {
        let image = image.clone();
        described(
            Semantics::new(Role::Button).merging_descendants(),
            move || {
                emit(Node {
                    modifier: Modifier::new(),
                    content: Content::Image {
                        image: image.clone(),
                        scale: ContentScale::Fit,
                    },
                });
                label("Play");
            },
        );
    });
    assert_eq!(list.semantics.len(), 1);
}

#[test]
fn clearing_descendants_drops_everything_inside() {
    let list = describe(|| {
        described(
            Semantics::new(Role::Group)
                .label("card")
                .clearing_descendants(),
            || {
                label("hidden text");
                described(Semantics::new(Role::Button).label("hidden button"), || {});
            },
        );
        label("visible");
    });
    assert_eq!(
        labels(&list),
        [Some("card".to_string()), Some("visible".to_string())]
    );
}

#[test]
fn clearing_descendants_keeps_the_area_described_after_it() {
    let list = describe(|| {
        described(Semantics::new(Role::Group).clearing_descendants(), || {
            label("x")
        });
        described(Semantics::new(Role::Button).label("next"), || {});
    });
    let parents: Vec<_> = list.semantics.iter().map(|node| node.parent).collect();
    assert_eq!(parents, [None, None]);
}

#[test]
fn a_clear_and_set_description_reads_as_itself() {
    let list = describe(|| {
        layout(
            Modifier::new().clear_and_set_semantics(Semantics::new(Role::Image).label("chart")),
            Overlay::default(),
            || label("1 2 3"),
        );
    });
    assert_eq!(labels(&list), [Some("chart".to_string())]);
}

#[test]
fn headings_and_live_regions_are_carried() {
    let list = describe(|| {
        described(Semantics::new(Role::Label).label("Title").heading(2), || {});
        described(
            Semantics::new(Role::Label)
                .label("Saved")
                .live_region(LiveRegion::Assertive),
            || {},
        );
        described(Semantics::new(Role::Label).heading(0), || {});
    });
    assert_eq!(list.semantics[0].semantics.heading, Some(2));
    assert_eq!(
        list.semantics[1].semantics.live_region,
        Some(LiveRegion::Assertive)
    );
    assert_eq!(list.semantics[2].semantics.heading, Some(1));
}

#[test]
fn a_clickable_described_area_can_be_activated() {
    let clicked = Rc::new(Cell::new(0));
    let counter = Rc::clone(&clicked);
    let list = describe(move || {
        let counter = Rc::clone(&counter);
        layout(
            Modifier::new()
                .size(40.0, 20.0)
                .semantics(Semantics::new(Role::Button).label("Go"))
                .clickable(move || counter.set(counter.get() + 1)),
            Overlay::default(),
            || {},
        );
        described(Semantics::new(Role::Label).label("plain"), || {});
    });
    let on_click = list.semantics[0].on_click.clone().expect("a click action");
    on_click();
    assert_eq!(clicked.get(), 1);
    assert!(list.semantics[1].on_click.is_none());
}

#[test]
fn a_description_covers_its_area() {
    let list = describe(|| {
        layout(
            Modifier::new()
                .size(40.0, 20.0)
                .semantics(Semantics::new(Role::Group)),
            Overlay::default(),
            || {},
        );
    });
    let rect = list.semantics[0].rect;
    assert_eq!((rect.size.width, rect.size.height), (40.0, 20.0));
}

#[test]
fn traversal_indices_and_custom_actions_are_kept() {
    let ran = Rc::new(Cell::new(false));
    let flag = Rc::clone(&ran);
    let list = describe(move || {
        let flag = Rc::clone(&flag);
        described(Semantics::new(Role::Button).label("first"), || {});
        described(
            Semantics::new(Role::Button)
                .label("second")
                .traversal_index(-1.0)
                .custom_action("Archive", move || flag.set(true)),
            || {},
        );
    });
    assert_eq!(list.semantics[0].semantics.traversal_index, None);
    assert_eq!(list.semantics[1].semantics.traversal_index, Some(-1.0));
    let action = &list.semantics[1].semantics.custom_actions[0];
    assert_eq!(action.label, "Archive");
    (action.handler)();
    assert!(ran.get());
}

#[test]
fn raised_content_stays_inside_the_area_it_belongs_to() {
    let list = describe(|| {
        described(Semantics::new(Role::Group).label("owner"), || {
            layout(
                Modifier::new()
                    .z_index(5)
                    .semantics(Semantics::new(Role::Button).label("raised")),
                Overlay::default(),
                || {},
            );
        });
    });
    let raised = list
        .semantics
        .iter()
        .position(|node| node.semantics.label.as_deref() == Some("raised"))
        .unwrap();
    let owner = list
        .semantics
        .iter()
        .position(|node| node.semantics.label.as_deref() == Some("owner"))
        .unwrap();
    assert_eq!(list.semantics[raised].parent, Some(owner));
}

#[test]
fn text_inside_raised_content_still_merges_into_its_button() {
    let list = describe(|| {
        described(Semantics::new(Role::Button).merging_descendants(), || {
            layout(Modifier::new().z_index(3), Overlay::default(), || {
                label("raised caption")
            });
        });
    });
    assert_eq!(labels(&list), [Some("raised caption".to_string())]);
}
