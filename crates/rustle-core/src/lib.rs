//! Renderer-independent core of Rustle: composition, layout and the display list.

use std::cell::RefCell;

mod composition;
mod state;

pub use composition::{Composition, remember};
pub use state::State;

/// A node of the UI tree produced by composition.
#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Column(Vec<Node>),
    Text(String),
}

/// A backend-independent drawing instruction produced by layout.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawCommand {
    Text { x: f32, y: f32, content: String },
}

/// Font size of all text, in logical pixels.
pub const FONT_SIZE: f32 = 18.0;
/// Height reserved for one line of text, in logical pixels.
pub const LINE_HEIGHT: f32 = 24.0;
/// Vertical gap between the children of a column, in logical pixels.
pub const SPACING: f32 = 8.0;
/// Offset of the root UI from the top-left corner, in logical pixels.
pub const PADDING: f32 = 16.0;

thread_local! {
    /// One entry per open scope, each holding the children emitted so far.
    static SCOPES: RefCell<Vec<Vec<Node>>> = const { RefCell::new(Vec::new()) };
}

/// Restores the scope stack to `depth` on drop, so a panic inside a
/// composable cannot leave stale scopes behind.
struct ScopeGuard {
    depth: usize,
}

impl Drop for ScopeGuard {
    fn drop(&mut self) {
        SCOPES.with(|scopes| scopes.borrow_mut().truncate(self.depth));
    }
}

/// Runs `content` in a fresh scope and returns the nodes it emitted.
fn collect(content: impl FnOnce()) -> Vec<Node> {
    let depth = SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        scopes.push(Vec::new());
        scopes.len() - 1
    });
    let _guard = ScopeGuard { depth };
    content();
    SCOPES
        .with(|scopes| scopes.borrow_mut().pop())
        .expect("composition scope stack was corrupted")
}

fn emit(node: Node) {
    let node = SCOPES.with(|scopes| match scopes.borrow_mut().last_mut() {
        Some(children) => {
            children.push(node);
            None
        }
        None => Some(node),
    });
    assert!(
        node.is_none(),
        "composables can only be called during composition"
    );
}

/// Emits a text element into the current scope.
pub fn text(content: impl Into<String>) {
    emit(Node::Text(content.into()));
}

/// Emits a column whose children are emitted by `content`.
pub fn column(content: impl FnOnce()) {
    emit(Node::Column(collect(content)));
}

/// Runs a root composable and returns the resulting UI tree.
///
/// A single root element is returned as is; anything else is wrapped in a column.
pub fn compose(root: impl FnOnce()) -> Node {
    let mut children = collect(root);
    if children.len() == 1 {
        children.remove(0)
    } else {
        Node::Column(children)
    }
}

/// Lays out the tree and returns drawing commands in paint order.
pub fn layout(root: &Node) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    place(root, PADDING, PADDING, &mut commands);
    commands
}

/// Places `node` with its top-left corner at (`x`, `y`) and returns its height.
fn place(node: &Node, x: f32, y: f32, commands: &mut Vec<DrawCommand>) -> f32 {
    match node {
        Node::Text(content) => {
            commands.push(DrawCommand::Text {
                x,
                y,
                content: content.clone(),
            });
            LINE_HEIGHT
        }
        Node::Column(children) => {
            let mut offset = 0.0;
            for (i, child) in children.iter().enumerate() {
                if i > 0 {
                    offset += SPACING;
                }
                offset += place(child, x, y + offset, commands);
            }
            offset
        }
    }
}
