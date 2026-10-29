//! Rustle's UI layer: the node tree, modifiers, composing into the tree, and layout.

use std::cell::RefCell;

use rustle_graphics::Color;
use rustle_text::TextStyle;

mod layout;
mod modifier;

pub use layout::{Children, DisplayList, HitRegion, Layout, LayoutResult, Overlay, display_list};
pub use modifier::Modifier;

/// A node of the UI tree produced by composition.
pub struct Node {
    pub modifier: Modifier,
    pub content: Content,
}

/// What a node is.
pub enum Content {
    /// One line of text.
    Text {
        content: String,
        color: Color,
        style: TextStyle,
    },
    /// Children arranged by a layout policy.
    Layout {
        policy: Box<dyn Layout>,
        children: Vec<Node>,
    },
}

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
pub fn collect(content: impl FnOnce()) -> Vec<Node> {
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

/// Emits `node` into the current scope.
///
/// # Panics
///
/// Panics when called outside composition.
pub fn emit(node: Node) {
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

/// Runs a root composable and returns the resulting UI tree.
///
/// A single root element is returned as is; anything else is placed on top of
/// each other by an [`Overlay`].
pub fn compose(root: impl FnOnce()) -> Node {
    let mut children = collect(root);
    if children.len() == 1 {
        children.remove(0)
    } else {
        Node {
            modifier: Modifier::new(),
            content: Content::Layout {
                policy: Box::new(Overlay),
                children,
            },
        }
    }
}

/// Emits a node arranged by `policy` whose children are emitted by `content`.
pub fn layout(modifier: Modifier, policy: impl Layout + 'static, content: impl FnOnce()) {
    emit(Node {
        modifier,
        content: Content::Layout {
            policy: Box::new(policy),
            children: collect(content),
        },
    });
}

/// A composition whose root produces a UI [`Node`] tree.
pub type Composition = rustle_runtime::Composition<Node>;

/// Creates a composition that composes `root` into a tree on every pass.
pub fn composition(mut root: impl FnMut() + 'static) -> Composition {
    Composition::new(move || compose(&mut root))
}
