//! Rustle's UI layer: the node tree, modifiers, composing into the tree, and layout.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_geometry::Size;
use rustle_graphics::{Color, DrawCommand, ImageData};
use rustle_text::{TextSpan, TextStyle};

mod canvas;
mod input;
mod layout;
mod modifier;
mod scroll;
mod semantics;

pub use canvas::Canvas;
pub use input::{
    Clipboard, DropHandler, Focus, Key, KeyEvent, KeyHandler, Modifiers, PressEvent, clipboard,
    remember_focus, set_clipboard, take_focus_request,
};
pub use layout::ContentScale;
pub use layout::{
    BoxAlignment, Children, DisplayList, DragRegion, DragSourceRegion, DropTargetRegion,
    FileDropRegion, FocusRegion, HitRegion, HoverRegion, Layout, LayoutResult, Overlay,
    PressRegion, ScrollRegion, SemanticsNode, display_list,
};
pub use modifier::Modifier;
pub use scroll::{ScrollState, remember_scroll_state};
pub use semantics::{Role, Semantics};

/// Which way layouts run: the start of a row is on the left in `Ltr` and on the right in `Rtl`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    #[default]
    Ltr,
    Rtl,
}

/// The layout direction in effect. Provide `Rtl` below some point to mirror
/// rows, stacks, flows and start-aligned columns there.
pub static LOCAL_LAYOUT_DIRECTION: rustle_runtime::CompositionLocal<LayoutDirection> =
    rustle_runtime::CompositionLocal::new(LayoutDirection::default);

/// Whether layouts currently run right to left.
pub fn is_rtl() -> bool {
    LOCAL_LAYOUT_DIRECTION.current() == LayoutDirection::Rtl
}

/// A node of the UI tree produced by composition. Cloning is cheap: layout
/// children and policies are shared.
#[derive(Clone)]
pub struct Node {
    pub modifier: Modifier,
    pub content: Content,
}

/// Where the boundary before a character is: its x from the start of its
/// line, and which line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CharPosition {
    pub x: f32,
    pub line: u32,
}

/// What an editable piece of text shows besides the text itself.
#[derive(Clone)]
pub struct TextEditing {
    /// The caret, as a character index, drawn when present.
    pub caret: Option<usize>,
    /// The selected characters, as a start and end index.
    pub selection: Option<(usize, usize)>,
    pub caret_color: Color,
    pub selection_color: Color,
    /// Filled in by layout: where each character boundary is, one more than
    /// there are characters, so a pointer position can be turned into a
    /// character index. Lines end at line breaks and do not wrap.
    pub boundaries: Rc<RefCell<Vec<CharPosition>>>,
    /// Called after layout has filled in `boundaries`, for example to scroll the caret into view.
    pub on_layout: Option<Rc<dyn Fn()>>,
}

/// What a node is.
#[derive(Clone)]
pub enum Content {
    /// Text, with caret and selection if it is being edited.
    Text {
        content: String,
        style: TextStyle,
        editing: Option<TextEditing>,
    },
    /// Text in runs of different styles.
    RichText {
        spans: Vec<TextSpan>,
        style: TextStyle,
    },
    /// Whatever `draw` paints, given the size the node is laid out at.
    Canvas {
        draw: Rc<dyn Fn(Size) -> Vec<DrawCommand>>,
    },
    /// A picture, sized to its pixels unless constrained.
    Image {
        image: ImageData,
        scale: ContentScale,
    },
    /// Children arranged by a layout policy.
    Layout {
        policy: Rc<dyn Layout>,
        children: Rc<Vec<Node>>,
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
                policy: Rc::new(Overlay::default()),
                children: Rc::new(children),
            },
        }
    }
}

/// Emits a node arranged by `policy` whose children are emitted by `content`.
///
/// `content` runs in a group identified by the call site, so what it
/// remembers belongs to this call.
#[track_caller]
pub fn layout(modifier: Modifier, policy: impl Layout + 'static, content: impl FnOnce()) {
    let children = rustle_runtime::group(|| collect(content));
    emit(Node {
        modifier,
        content: Content::Layout {
            policy: Rc::new(policy),
            children: Rc::new(children),
        },
    });
}

/// Runs `content` like a block of composables, but skips it when it can.
///
/// When `deps` equals what it was last given and none of the state `content`
/// read has changed, `content` does not run, what it remembered is kept, and
/// the nodes it emitted last time are emitted again. Use it for a part of the
/// tree that is expensive to compose and often unchanged, passing everything
/// it takes from outside in `deps`.
///
/// Anything in `content` that reads a composition local stops it being skipped.
#[track_caller]
pub fn skippable<D: PartialEq + 'static>(deps: D, content: impl FnOnce()) {
    let nodes = rustle_runtime::memo(deps, || collect(content));
    for node in nodes {
        emit(node);
    }
}

/// Emits an area drawn by `draw`, which paints shapes, lines and text in the
/// area's own coordinates. It has no size of its own: give it one with
/// `modifier`, such as `Modifier::new().size(200.0, 80.0)` or `fill_max_width`.
#[track_caller]
pub fn canvas(modifier: Modifier, draw: impl Fn(&mut Canvas) + 'static) {
    emit(Node {
        modifier,
        content: Content::Canvas {
            draw: Rc::new(move |size| {
                let mut canvas = Canvas::new(size);
                draw(&mut canvas);
                canvas.finish()
            }),
        },
    });
}

/// A composition whose root produces a UI [`Node`] tree.
pub type Composition = rustle_runtime::Composition<Node>;

/// Creates a composition that composes `root` into a tree on every pass.
pub fn composition(mut root: impl FnMut() + 'static) -> Composition {
    Composition::new(move || compose(&mut root))
}
