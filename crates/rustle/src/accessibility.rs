//! Describes the UI to platform accessibility APIs through AccessKit.

use accesskit::{Action, Node, NodeId, Rect, Role, Toggled, TreeId, TreeInfo, TreeUpdate};
use rustle_ui::{DisplayList, Focus, Role as UiRole};

/// The window itself, parent of every described area.
const WINDOW: NodeId = NodeId(0);

/// The node of the described area at `index` in the display list.
pub(crate) fn node_id(index: usize) -> NodeId {
    NodeId(index as u64 + 1)
}

/// The index of the described area a node stands for.
pub(crate) fn index_of(id: NodeId) -> Option<usize> {
    (id.0 as usize).checked_sub(1)
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
pub(crate) fn tree(
    display: &DisplayList,
    scale_factor: f64,
    focused: Option<&Focus>,
) -> TreeUpdate {
    let mut window = Node::new(Role::Window);
    window.set_label("Rustle");
    let mut nodes = Vec::with_capacity(display.semantics.len() + 1);
    let mut children = Vec::with_capacity(display.semantics.len());
    let mut focus = WINDOW;

    for (index, described) in display.semantics.iter().enumerate() {
        let id = node_id(index);
        let semantics = &described.semantics;
        let mut node = Node::new(role(semantics.role));
        let rect = described.rect;
        node.set_bounds(Rect {
            x0: f64::from(rect.origin.x) * scale_factor,
            y0: f64::from(rect.origin.y) * scale_factor,
            x1: f64::from(rect.origin.x + rect.size.width) * scale_factor,
            y1: f64::from(rect.origin.y + rect.size.height) * scale_factor,
        });
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
        if let Some(owner) = &semantics.focus {
            node.add_action(Action::Focus);
            if focused.is_some_and(|focused| focused.id() == owner.id()) {
                focus = id;
            }
        }
        children.push(id);
        nodes.push((id, node));
    }

    window.set_children(children);
    nodes.push((WINDOW, window));
    TreeUpdate {
        nodes,
        tree: Some(TreeInfo::new(WINDOW)),
        tree_id: TreeId::ROOT,
        focus,
    }
}
