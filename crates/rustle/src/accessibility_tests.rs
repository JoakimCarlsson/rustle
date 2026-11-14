//! Tests of the accessibility tree built from a display list: nesting, reading order,
//! roles, states, actions and the semantics of the built-in widgets.

use std::cell::Cell;
use std::rc::Rc;

use accesskit::{Action, Live, Node, NodeId, Role, Toggled, TreeUpdate};
use rustle_foundation::{
    Modifier, Role as UiRole, Semantics, TextFieldState, column_with, remember_focus,
    remember_text_field_state, stack_with, text,
};
use rustle_ui::{DisplayList, LiveRegion, remember_scroll_state};
use rustle_widgets::{
    alert_dialog, button, checkbox, circular_progress_indicator, dialog, linear_progress_indicator,
    radio_button, slider, switch, tab_row, text_field, title,
};

use crate::accessibility::{index_of, node_id, tree};
use crate::testing::display_of;

/// The tree of `display` at scale 1 with nothing focused.
fn built(display: &DisplayList) -> TreeUpdate {
    tree(display, 1.0, None)
}

/// The node with `id` in `update`.
fn node(update: &TreeUpdate, id: NodeId) -> &Node {
    &update
        .nodes
        .iter()
        .find(|(node, _)| *node == id)
        .unwrap_or_else(|| panic!("no node {id:?}"))
        .1
}

/// The window node's children, as the roles of the nodes.
fn top_roles(update: &TreeUpdate) -> Vec<Role> {
    let window = update.tree.as_ref().unwrap().root;
    node(update, window)
        .children()
        .iter()
        .map(|child| node(update, *child).role())
        .collect()
}

/// The ids of every node with `role`, in the order the display list described them.
fn with_role(update: &TreeUpdate, role: Role) -> Vec<NodeId> {
    update
        .nodes
        .iter()
        .filter(|(_, node)| node.role() == role)
        .map(|(id, _)| *id)
        .collect()
}

/// Emits an empty described area.
fn area(semantics: Semantics) {
    stack_with(Modifier::new().size(10.0, 10.0).semantics(semantics), || {});
}

#[test]
fn the_window_is_the_root_and_holds_what_is_described_outside_anything() {
    let display = display_of(|| {
        text("one");
        text("two");
    });
    let update = built(&display);
    let root = update.tree.as_ref().unwrap().root;
    assert_eq!(node(&update, root).role(), Role::Window);
    assert_eq!(top_roles(&update), [Role::Label, Role::Label]);
    assert_eq!(
        node(&update, root).children(),
        [node_id(0), node_id(1)].as_slice()
    );
    assert_eq!(node(&update, node_id(0)).label(), Some("one"));
}

#[test]
fn node_ids_map_back_to_described_areas() {
    for index in [0, 1, 7, 300] {
        assert_eq!(index_of(node_id(index)), Some(index));
    }
    assert_eq!(index_of(NodeId(0)), None);
}

#[test]
fn text_runs_map_back_to_the_field_they_belong_to() {
    let display = display_of(|| {
        let state = remember_text_field_state("héllo");
        text_field(Modifier::new().width(200.0), &state, "name");
    });
    let update = built(&display);
    let field = with_role(&update, Role::TextInput)[0];
    let run = node(&update, field).children()[0];
    assert_eq!(node(&update, run).role(), Role::TextRun);
    assert_eq!(index_of(run), index_of(field));
}

#[test]
fn areas_described_inside_others_are_their_children() {
    let display = display_of(|| {
        stack_with(
            Modifier::new().semantics(Semantics::new(UiRole::Group).label("outer")),
            || {
                text("inside");
                stack_with(
                    Modifier::new().semantics(Semantics::new(UiRole::Group).label("inner")),
                    || text("deep"),
                );
            },
        );
        text("after");
    });
    let update = built(&display);
    let outer = with_role(&update, Role::Group)[0];
    let inner = with_role(&update, Role::Group)[1];
    assert_eq!(top_roles(&update), [Role::Group, Role::Label]);
    let children = node(&update, outer).children();
    assert_eq!(children.len(), 2);
    assert_eq!(node(&update, children[0]).label(), Some("inside"));
    assert_eq!(children[1], inner);
    let deep = node(&update, inner).children();
    assert_eq!(node(&update, deep[0]).label(), Some("deep"));
}

#[test]
fn merged_text_leaves_a_button_with_no_children() {
    let display = display_of(|| button("Save", || {}));
    let update = built(&display);
    let button = with_role(&update, Role::Button)[0];
    assert_eq!(node(&update, button).label(), Some("Save"));
    assert!(node(&update, button).children().is_empty());
    assert_eq!(update.nodes.len(), 2);
}

#[test]
fn cleared_descendants_are_not_in_the_tree() {
    let display = display_of(|| {
        stack_with(
            Modifier::new().clear_and_set_semantics(Semantics::new(UiRole::Image).label("chart")),
            || {
                text("1");
                text("2");
            },
        );
    });
    let update = built(&display);
    assert_eq!(update.nodes.len(), 2);
    assert_eq!(top_roles(&update), [Role::Image]);
}

#[test]
fn traversal_index_orders_siblings_and_keeps_paint_order_among_equals() {
    let display = display_of(|| {
        area(Semantics::new(UiRole::Button).label("a"));
        area(
            Semantics::new(UiRole::Button)
                .label("b")
                .traversal_index(2.0),
        );
        area(Semantics::new(UiRole::Button).label("c"));
        area(
            Semantics::new(UiRole::Button)
                .label("d")
                .traversal_index(-1.0),
        );
        area(
            Semantics::new(UiRole::Button)
                .label("e")
                .traversal_index(2.0),
        );
    });
    let update = built(&display);
    let root = update.tree.as_ref().unwrap().root;
    let order: Vec<_> = node(&update, root)
        .children()
        .iter()
        .map(|id| node(&update, *id).label().unwrap().to_string())
        .collect();
    assert_eq!(order, ["d", "a", "c", "b", "e"]);
}

#[test]
fn traversal_index_orders_children_inside_a_parent() {
    let display = display_of(|| {
        stack_with(
            Modifier::new().semantics(Semantics::new(UiRole::Group)),
            || {
                area(
                    Semantics::new(UiRole::Button)
                        .label("late")
                        .traversal_index(5.0),
                );
                area(
                    Semantics::new(UiRole::Button)
                        .label("early")
                        .traversal_index(1.0),
                );
            },
        );
    });
    let update = built(&display);
    let group = with_role(&update, Role::Group)[0];
    let labels: Vec<_> = node(&update, group)
        .children()
        .iter()
        .map(|id| node(&update, *id).label().unwrap().to_string())
        .collect();
    assert_eq!(labels, ["early", "late"]);
}

#[test]
fn headings_have_a_level() {
    let display = display_of(|| {
        title("Welcome", 24.0);
        area(Semantics::new(UiRole::Label).label("Details").heading(3));
        text("body");
    });
    let update = built(&display);
    let headings = with_role(&update, Role::Heading);
    assert_eq!(headings.len(), 2);
    assert_eq!(node(&update, headings[0]).level(), Some(2));
    assert_eq!(node(&update, headings[1]).level(), Some(3));
    assert_eq!(node(&update, headings[1]).label(), Some("Details"));
}

#[test]
fn live_regions_are_polite_or_assertive() {
    let display = display_of(|| {
        area(
            Semantics::new(UiRole::Label)
                .label("status")
                .live_region(LiveRegion::Polite),
        );
        area(
            Semantics::new(UiRole::Label)
                .label("error")
                .live_region(LiveRegion::Assertive),
        );
        area(Semantics::new(UiRole::Label).label("plain"));
    });
    let update = built(&display);
    assert_eq!(node(&update, node_id(0)).live(), Some(Live::Polite));
    assert_eq!(node(&update, node_id(1)).live(), Some(Live::Assertive));
    assert_eq!(node(&update, node_id(2)).live(), None);
}

#[test]
fn actions_follow_what_the_area_can_do() {
    let display = display_of(|| {
        let focus = remember_focus();
        area(Semantics::new(UiRole::Label).label("inert"));
        stack_with(
            Modifier::new()
                .size(10.0, 10.0)
                .semantics(Semantics::new(UiRole::Button).label("click").focus(&focus))
                .clickable(|| {}),
            || {},
        );
        area(
            Semantics::new(UiRole::TextInput)
                .on_set_value(|_| {})
                .on_scroll(|_| {})
                .on_set_text_selection(|_, _| {})
                .custom_action("Archive", || {}),
        );
    });
    let update = built(&display);
    let inert = node(&update, node_id(0));
    for action in [
        Action::Click,
        Action::Focus,
        Action::SetValue,
        Action::ScrollDown,
        Action::CustomAction,
    ] {
        assert!(!inert.supports_action(action), "{action:?}");
    }
    let clickable = node(&update, node_id(1));
    assert!(clickable.supports_action(Action::Click));
    assert!(clickable.supports_action(Action::Focus));
    assert!(!clickable.supports_action(Action::SetValue));
    let rich = node(&update, node_id(2));
    for action in [
        Action::SetValue,
        Action::ScrollUp,
        Action::ScrollDown,
        Action::ScrollLeft,
        Action::ScrollRight,
        Action::SetTextSelection,
        Action::CustomAction,
    ] {
        assert!(rich.supports_action(action), "{action:?}");
    }
    assert!(!rich.supports_action(Action::Click));
    assert_eq!(rich.custom_actions().len(), 1);
    assert_eq!(rich.custom_actions()[0].description, "Archive");
    assert_eq!(rich.custom_actions()[0].id, 0);
}

#[test]
fn a_click_action_runs_the_click_handler() {
    let clicks = Rc::new(Cell::new(0));
    let seen = Rc::clone(&clicks);
    let display = display_of(move || {
        let seen = Rc::clone(&seen);
        button("Go", move || seen.set(seen.get() + 1));
    });
    let on_click = display.semantics[0].on_click.clone().expect("clickable");
    on_click();
    assert_eq!(clicks.get(), 1);
}

#[test]
fn bounds_are_in_physical_pixels() {
    let display = display_of(|| {
        stack_with(
            Modifier::new()
                .size(40.0, 20.0)
                .semantics(Semantics::new(UiRole::Group)),
            || {},
        );
    });
    let update = tree(&display, 2.0, None);
    let bounds = node(&update, node_id(0)).bounds().unwrap();
    assert_eq!((bounds.x1 - bounds.x0, bounds.y1 - bounds.y0), (80.0, 40.0));
}

#[test]
fn focus_is_the_focused_area_or_the_window() {
    let focus = Rc::new(std::cell::RefCell::new(None));
    let kept = Rc::clone(&focus);
    let display = display_of(move || {
        let focus = remember_focus();
        *kept.borrow_mut() = Some(focus.clone());
        stack_with(
            Modifier::new()
                .size(10.0, 10.0)
                .semantics(Semantics::new(UiRole::Button).label("field").focus(&focus)),
            || {},
        );
    });
    let focus = focus.borrow().clone().unwrap();
    let root = tree(&display, 1.0, None).tree.unwrap().root;
    assert_eq!(tree(&display, 1.0, None).focus, root);
    assert_eq!(tree(&display, 1.0, Some(&focus)).focus, node_id(0));
}

#[test]
fn toggles_values_and_labels_are_reported() {
    let display = display_of(|| {
        area(Semantics::new(UiRole::CheckBox).toggled(true).label("on"));
        area(Semantics::new(UiRole::Switch).toggled(false).value("off"));
    });
    let update = built(&display);
    let on = node(&update, node_id(0));
    assert_eq!(on.toggled(), Some(Toggled::True));
    assert_eq!(on.label(), Some("on"));
    let off = node(&update, node_id(1));
    assert_eq!(off.toggled(), Some(Toggled::False));
    assert_eq!(off.value(), Some("off"));
}

#[test]
fn a_selection_counts_characters_not_bytes() {
    let display = display_of(|| {
        area(
            Semantics::new(UiRole::TextInput)
                .value("a😀é")
                .text_selection(1, 3),
        );
    });
    let update = built(&display);
    let field = node(&update, node_id(0));
    let selection = field.text_selection().unwrap();
    assert_eq!(selection.anchor.character_index, 1);
    assert_eq!(selection.focus.character_index, 3);
    let run = node(&update, selection.anchor.node);
    assert_eq!(run.role(), Role::TextRun);
    assert_eq!(run.value(), Some("a😀é"));
    assert_eq!(run.character_lengths(), [1, 4, 2].as_slice());
    assert_eq!(field.children()[0], selection.anchor.node);
}

#[test]
fn a_selection_past_the_end_is_kept_inside_the_text() {
    let display = display_of(|| {
        area(
            Semantics::new(UiRole::TextInput)
                .value("ab")
                .text_selection(0, 9),
        );
    });
    let update = built(&display);
    let selection = node(&update, node_id(0)).text_selection().unwrap();
    assert_eq!(selection.focus.character_index, 2);
}

#[test]
fn a_text_field_is_a_text_input_with_value_focus_and_selection() {
    let kept = Rc::new(std::cell::RefCell::new(None::<TextFieldState>));
    let keep = Rc::clone(&kept);
    let display = display_of(move || {
        let state = remember_text_field_state("hello");
        *keep.borrow_mut() = Some(state.clone());
        text_field(Modifier::new().width(200.0), &state, "name");
    });
    let update = built(&display);
    let field = node(&update, with_role(&update, Role::TextInput)[0]);
    assert_eq!(field.value(), Some("hello"));
    assert!(field.supports_action(Action::Focus));
    assert!(field.supports_action(Action::SetValue));
    assert!(field.supports_action(Action::SetTextSelection));
    let (anchor, caret) = {
        let selection = field.text_selection().unwrap();
        (
            selection.anchor.character_index,
            selection.focus.character_index,
        )
    };
    assert_eq!(anchor, caret);
}

#[test]
fn a_button_is_a_button_with_its_caption_and_a_click() {
    let display = display_of(|| button("Open", || {}));
    let update = built(&display);
    let button = node(&update, with_role(&update, Role::Button)[0]);
    assert_eq!(button.label(), Some("Open"));
    assert!(button.supports_action(Action::Click));
}

#[test]
fn a_checkbox_reports_whether_it_is_checked() {
    let display = display_of(|| {
        checkbox(true, |_| {});
        checkbox(false, |_| {});
    });
    let update = built(&display);
    let boxes = with_role(&update, Role::CheckBox);
    assert_eq!(node(&update, boxes[0]).toggled(), Some(Toggled::True));
    assert_eq!(node(&update, boxes[1]).toggled(), Some(Toggled::False));
    assert!(node(&update, boxes[0]).supports_action(Action::Click));
    assert!(node(&update, boxes[0]).supports_action(Action::SetValue));
}

#[test]
fn a_switch_and_a_radio_button_report_their_state() {
    let display = display_of(|| {
        switch(true, |_| {});
        radio_button(false, || {});
        radio_button(true, || {});
    });
    let update = built(&display);
    let switches = with_role(&update, Role::Switch);
    assert_eq!(node(&update, switches[0]).toggled(), Some(Toggled::True));
    let radios = with_role(&update, Role::RadioButton);
    assert_eq!(node(&update, radios[0]).toggled(), Some(Toggled::False));
    assert_eq!(node(&update, radios[1]).toggled(), Some(Toggled::True));
    assert!(node(&update, radios[0]).supports_action(Action::Click));
}

#[test]
fn a_slider_reports_its_value_and_accepts_a_new_one() {
    let display = display_of(|| slider(200.0, 0.25, |_| {}));
    let update = built(&display);
    let slider = node(&update, with_role(&update, Role::Slider)[0]);
    assert_eq!(slider.value(), Some("25%"));
    assert!(slider.supports_action(Action::SetValue));
}

#[test]
fn progress_indicators_report_their_progress() {
    let display = display_of(|| {
        linear_progress_indicator(100.0, 0.5);
        circular_progress_indicator(24.0, 0.8);
    });
    let update = built(&display);
    let bars = with_role(&update, Role::ProgressIndicator);
    assert_eq!(node(&update, bars[0]).value(), Some("50%"));
    assert_eq!(node(&update, bars[1]).value(), Some("80%"));
}

#[test]
fn tabs_are_tabs_with_their_labels() {
    let display = display_of(|| tab_row(1, &["One", "Two", "Three"], |_| {}));
    let update = built(&display);
    let tabs = with_role(&update, Role::Tab);
    assert_eq!(tabs.len(), 3);
    assert_eq!(node(&update, tabs[1]).label(), Some("Two"));
    assert!(node(&update, tabs[1]).supports_action(Action::Click));
}

#[test]
fn a_dialog_is_a_dialog_that_holds_its_content() {
    let display = display_of(|| {
        stack_with(Modifier::new().fill_max_size(), || {
            text("behind");
            dialog(|| {}, || text("Are you sure?"));
        });
    });
    let update = built(&display);
    let dialog = with_role(&update, Role::Dialog)[0];
    let inside: Vec<_> = node(&update, dialog)
        .children()
        .iter()
        .map(|id| node(&update, *id).label().unwrap_or_default().to_string())
        .collect();
    assert!(inside.contains(&"Are you sure?".to_string()), "{inside:?}");
    assert!(!inside.contains(&"behind".to_string()));
}

#[test]
fn an_alert_dialog_is_named_by_its_title() {
    let display = display_of(|| {
        stack_with(Modifier::new().fill_max_size(), || {
            alert_dialog(
                "Delete file",
                "This cannot be undone.",
                "Delete",
                || {},
                || {},
            );
        });
    });
    let update = built(&display);
    let dialog = node(&update, with_role(&update, Role::Dialog)[0]);
    assert_eq!(dialog.label(), Some("Delete file"));
}

#[test]
fn rows_scrolled_out_of_sight_have_no_area_left() {
    let display = display_of(|| {
        let state = remember_scroll_state();
        column_with(
            Modifier::new().size(100.0, 32.0).vertical_scroll(&state),
            || {
                for index in 0..10 {
                    text(format!("row {index}"));
                }
            },
        );
    });
    let update = built(&display);
    let shown = with_role(&update, Role::Label)
        .into_iter()
        .filter(|id| {
            let bounds = node(&update, *id).bounds().unwrap();
            bounds.x1 > bounds.x0 && bounds.y1 > bounds.y0
        })
        .count();
    assert!(shown > 0 && shown < 10, "{shown} rows have an area");
}
