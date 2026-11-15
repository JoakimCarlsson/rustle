//! Describes the UI to platform accessibility APIs through AccessKit.

use accesskit::{
    Action, ActionData, CustomAction, Live, Node, NodeId, Rect, Role, TextPosition, TextSelection,
    Toggled, TreeId, TreeInfo, TreeUpdate,
};
use rustle_ui::{DisplayList, Focus, LiveRegion, Role as UiRole, ScrollDirection};

use crate::app::App;

mod adapter;

pub(crate) use adapter::{Adapter, Request, Requests};

/// The window itself, parent of every described area.
const WINDOW: NodeId = NodeId(0);

/// The node of the described area at `index` in the display list.
pub(crate) fn node_id(index: usize) -> NodeId {
    NodeId(index as u64 + 1)
}

/// The first id of the text runs that carry the text of editable areas.
const TEXT_RUN_BASE: u64 = 1 << 32;

/// The node holding the text of the editable described area at `index`; it maps back to
/// the same index through [`index_of`], like the area's own node.
fn text_run_id(index: usize) -> NodeId {
    NodeId(TEXT_RUN_BASE + index as u64 + 1)
}

/// The index of the described area a node stands for, whether it is the area or its text run.
pub(crate) fn index_of(id: NodeId) -> Option<usize> {
    let raw = id.0 & (TEXT_RUN_BASE - 1);
    (raw as usize).checked_sub(1)
}

/// The window-space bounds of `rect` in physical pixels.
fn bounds(rect: rustle_geometry::Rect, scale_factor: f64) -> Rect {
    Rect {
        x0: f64::from(rect.origin.x) * scale_factor,
        y0: f64::from(rect.origin.y) * scale_factor,
        x1: f64::from(rect.origin.x + rect.size.width) * scale_factor,
        y1: f64::from(rect.origin.y + rect.size.height) * scale_factor,
    }
}

/// `ids` ordered by the traversal index of the areas they stand for, keeping paint order
/// among equals.
fn in_reading_order(display: &DisplayList, mut ids: Vec<usize>) -> Vec<usize> {
    let key = |index: &usize| {
        display.semantics[*index]
            .semantics
            .traversal_index
            .unwrap_or(0.0)
    };
    ids.sort_by(|a, b| key(a).total_cmp(&key(b)));
    ids
}

fn role(role: UiRole) -> Role {
    match role {
        UiRole::Label => Role::Label,
        UiRole::Button => Role::Button,
        UiRole::CheckBox => Role::CheckBox,
        UiRole::Switch => Role::Switch,
        UiRole::RadioButton => Role::RadioButton,
        UiRole::Slider => Role::Slider,
        UiRole::TextInput => Role::TextInput,
        UiRole::ProgressBar => Role::ProgressIndicator,
        UiRole::Image => Role::Image,
        UiRole::Dialog => Role::Dialog,
        UiRole::Tab => Role::Tab,
        UiRole::Group => Role::Group,
    }
}

/// The whole accessibility tree for `display`, with bounds in physical pixels.
///
/// Areas described inside others become their children, ordered by traversal index.
pub(crate) fn tree(
    display: &DisplayList,
    scale_factor: f64,
    focused: Option<&Focus>,
) -> TreeUpdate {
    let mut window = Node::new(Role::Window);
    window.set_label("Rustle");
    let count = display.semantics.len();
    let mut nodes = Vec::with_capacity(count + 1);
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); count];
    let mut top = Vec::new();
    let mut focus = WINDOW;

    for (index, described) in display.semantics.iter().enumerate() {
        match described.parent {
            Some(parent) => children[parent].push(index),
            None => top.push(index),
        }
    }

    for (index, described) in display.semantics.iter().enumerate() {
        let id = node_id(index);
        let semantics = &described.semantics;
        let mut node = Node::new(match semantics.heading {
            Some(_) => Role::Heading,
            None => role(semantics.role),
        });
        node.set_bounds(bounds(described.rect, scale_factor));
        if let Some(level) = semantics.heading {
            node.set_level(usize::from(level));
        }
        if let Some(live) = semantics.live_region {
            node.set_live(match live {
                LiveRegion::Polite => Live::Polite,
                LiveRegion::Assertive => Live::Assertive,
            });
        }
        if let Some(label) = &semantics.label {
            node.set_label(label.clone());
        }
        if let Some(value) = &semantics.value {
            node.set_value(value.clone());
        }
        if let Some(toggled) = semantics.toggled {
            node.set_toggled(if toggled {
                Toggled::True
            } else {
                Toggled::False
            });
        }
        if described.on_click.is_some() {
            node.add_action(Action::Click);
        }
        if semantics.on_scroll.is_some() {
            for action in [
                Action::ScrollUp,
                Action::ScrollDown,
                Action::ScrollLeft,
                Action::ScrollRight,
            ] {
                node.add_action(action);
            }
        }
        if semantics.on_set_value.is_some() {
            node.add_action(Action::SetValue);
        }
        if semantics.on_set_text_selection.is_some() {
            node.add_action(Action::SetTextSelection);
        }
        if !semantics.custom_actions.is_empty() {
            node.add_action(Action::CustomAction);
            node.set_custom_actions(
                semantics
                    .custom_actions
                    .iter()
                    .enumerate()
                    .map(|(position, action)| CustomAction {
                        id: position as i32,
                        description: action.label.clone(),
                    })
                    .collect::<Vec<_>>(),
            );
        }
        if let Some(owner) = &semantics.focus {
            node.add_action(Action::Focus);
            if focused.is_some_and(|focused| focused.id() == owner.id()) {
                focus = id;
            }
        }
        let mut child_ids: Vec<NodeId> = in_reading_order(display, children[index].clone())
            .into_iter()
            .map(node_id)
            .collect();
        if let (Some((anchor, caret)), Some(value)) = (semantics.text_selection, &semantics.value) {
            let run = text_run_id(index);
            let lengths: Vec<u8> = value.chars().map(|c| c.len_utf8() as u8).collect();
            let position = |at: usize| TextPosition {
                node: run,
                character_index: at.min(lengths.len()),
            };
            node.set_text_selection(TextSelection {
                anchor: position(anchor),
                focus: position(caret),
            });
            let mut text = Node::new(Role::TextRun);
            text.set_bounds(bounds(described.rect, scale_factor));
            text.set_value(value.clone());
            text.set_character_lengths(lengths);
            nodes.push((run, text));
            child_ids.insert(0, run);
        }
        node.set_children(child_ids);
        nodes.push((id, node));
    }

    window.set_children(
        in_reading_order(display, top)
            .into_iter()
            .map(node_id)
            .collect::<Vec<_>>(),
    );
    nodes.push((WINDOW, window));
    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(WINDOW)),
        tree_id: TreeId::ROOT,
        focus,
    }
}

impl App {
    /// Tells the platform what the latest layout means, if anything is listening.
    pub(crate) fn update_accessibility(&mut self) {
        let Some(state) = &mut self.window else {
            return;
        };
        let scale_factor = state.window.scale_factor();
        let (display, focused) = (&self.ui.display, self.ui.focused.as_ref());
        state
            .adapter
            .update_if_active(|| tree(display, scale_factor, focused));
    }

    /// Answers a request of the platform's accessibility API for the active window.
    pub(crate) fn accessibility_event(&mut self, event: Request) {
        use accesskit::Action;
        match event {
            Request::InitialTreeRequested => self.update_accessibility(),
            Request::ActionRequested(request) => {
                let described = index_of(request.target_node)
                    .and_then(|index| self.ui.display.semantics.get(index));
                match request.action {
                    Action::Click => {
                        if let Some(on_click) = described.and_then(|node| node.on_click.clone()) {
                            on_click();
                        }
                    }
                    Action::Focus => {
                        let focus = described.and_then(|node| node.semantics.focus.clone());
                        self.ui.set_focus(focus);
                    }
                    Action::ScrollUp
                    | Action::ScrollDown
                    | Action::ScrollLeft
                    | Action::ScrollRight => {
                        let direction = match request.action {
                            Action::ScrollUp => ScrollDirection::Up,
                            Action::ScrollDown => ScrollDirection::Down,
                            Action::ScrollLeft => ScrollDirection::Left,
                            _ => ScrollDirection::Right,
                        };
                        if let Some(on_scroll) =
                            described.and_then(|node| node.semantics.on_scroll.clone())
                        {
                            on_scroll(direction);
                        }
                    }
                    Action::SetValue => {
                        let value = match request.data {
                            Some(ActionData::Value(value)) => Some(value.to_string()),
                            Some(ActionData::NumericValue(value)) => Some(value.to_string()),
                            _ => None,
                        };
                        let handler =
                            described.and_then(|node| node.semantics.on_set_value.clone());
                        if let (Some(handler), Some(value)) = (handler, value) {
                            handler(value);
                        }
                    }
                    Action::SetTextSelection => {
                        let handler =
                            described.and_then(|node| node.semantics.on_set_text_selection.clone());
                        if let (Some(handler), Some(ActionData::SetTextSelection(selection))) =
                            (handler, request.data)
                        {
                            handler(
                                selection.anchor.character_index,
                                selection.focus.character_index,
                            );
                        }
                    }
                    Action::CustomAction => {
                        let action = match request.data {
                            Some(ActionData::CustomAction(position)) => {
                                usize::try_from(position).ok().and_then(|position| {
                                    described?.semantics.custom_actions.get(position)
                                })
                            }
                            _ => None,
                        };
                        if let Some(action) = action {
                            (action.handler.clone())();
                        }
                    }
                    _ => {}
                }
            }
            Request::AccessibilityDeactivated => {}
        }
    }
}
