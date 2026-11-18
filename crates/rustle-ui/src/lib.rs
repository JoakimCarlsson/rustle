//! Rustle's UI layer: the node tree, modifiers, composing into the tree, and layout.

use std::cell::RefCell;
use std::rc::Rc;

use rustle_geometry::Size;
use rustle_graphics::{Color, DrawCommand, ImageData};
use rustle_text::{TextSpan, TextStyle};

mod canvas;
mod cursor;
mod gesture;
mod input;
mod interaction;
mod layout;
mod modifier;
mod scroll;
mod semantics;

#[cfg(test)]
mod restartable_tests;
#[cfg(test)]
mod semantics_tests;
#[cfg(test)]
mod skippable_tests;

pub use canvas::Canvas;
pub use cursor::CursorIcon;
pub use gesture::{
    AnchoredDraggableState, Axis, DraggableState, NestedScrollConnection, PointerId, PointerInput,
    PointerInputHandler, PointerInputPhase, PointerKind, PointerSample, ScrollableState,
    TransformableState, VelocityTracker, remember_anchored_draggable_state,
    remember_draggable_state, remember_scrollable_state, remember_transformable_state,
};
pub use input::{
    Clipboard, ClipboardImage, DropHandler, Focus, Key, KeyCode, KeyEvent, KeyHandler, KeyLocation,
    Modifiers, PhysicalKey, PointerButton, PointerEvent, PointerPhase, Preedit, PressEvent,
    clipboard, remember_focus, set_clipboard, take_focus_request,
};
pub use interaction::{Interaction, InteractionColors, PointerState};
pub use layout::ContentScale;
pub use layout::{
    BoxAlignment, ButtonRegion, Children, CursorRegion, DisplayList, DisplayOptions, DragRegion,
    DragSourceRegion, DropTargetRegion, FileDropRegion, FocusRegion, HitRegion, HoverRegion,
    ImeCaretRegion, InteractionRegion, IntrinsicSize, KeyRegion, LaidOut, Layout, LayoutResult,
    Measurable, Overlay, PixelRounding, Placement, PointerInputRegion, PressRegion, ScrollRegion,
    SemanticsNode, SubcomposeResult, SubcomposeScope, display_list, display_list_with,
    subcompose_layout,
};
pub use modifier::Modifier;
pub use scroll::{ScrollState, ScrollbarStyle, ScrollbarVisibility, remember_scroll_state};
pub use semantics::{CustomAction, LiveRegion, Role, ScrollDirection, Semantics};

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
    /// A range of characters to underline, as a start and end index: the input method's
    /// composition text, which is part of the content while it is being composed.
    pub underline: Option<(usize, usize)>,
    /// Set on the field input goes to: where its caret is, as a character index (shown or
    /// not, so it does not blink). The caret's place in the window is then reported in
    /// [`DisplayList::ime_area`] for the input method's popup, as a one-pixel-wide
    /// rectangle from the start of `underline` (or the caret) to the caret.
    pub ime_caret: Option<usize>,
    pub caret_color: Color,
    pub selection_color: Color,
    /// Which side of a soft wrap the caret is on, when it is at the end of a wrapped line.
    pub caret_affinity: rustle_text::Affinity,
    /// Filled in by layout: where each character boundary is, one more than
    /// there are characters, so a pointer position can be turned into a
    /// character index. Lines end at line breaks and where the text wraps.
    pub boundaries: Rc<RefCell<Vec<CharPosition>>>,
    /// Filled in by layout along with `boundaries`: the shaped text with its line
    /// breaks, for caret movement by visual line and for hit testing. Offsets in it are
    /// bytes, while the indices of this struct count characters.
    pub layout: Rc<RefCell<Option<Rc<rustle_text::TextLayout>>>>,
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
        /// Set to learn the text's layout and to show a selection, a caret and an input
        /// method's composition over it, and to report where text input goes, as for `Text`.
        editing: Option<TextEditing>,
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
    /// One node, produced by a restartable block (see [`restartable`]) that can be composed
    /// again alone. It lays out and paints as the node it last produced, and its layout is
    /// kept while nothing it depends on has changed.
    Scope(rustle_runtime::Scope),
    /// Children arranged by a layout policy.
    Layout {
        policy: Rc<dyn Layout>,
        children: Rc<Vec<Node>>,
    },
}

/// Children collected by a scope, sharing an unchanged cached batch until it is extended.
#[derive(Default)]
enum Scope {
    #[default]
    Empty,
    Owned(Vec<Node>),
    Shared(Rc<Vec<Node>>),
}

impl Scope {
    /// Returns the number of nodes currently collected in this scope.
    fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::Owned(children) => children.len(),
            Self::Shared(children) => children.len(),
        }
    }

    /// Returns mutable children, copying a shared batch only when this scope is extended.
    fn children_mut(&mut self) -> &mut Vec<Node> {
        if matches!(self, Self::Empty) {
            *self = Self::Owned(Vec::new());
        }
        match self {
            Self::Owned(children) => children,
            Self::Shared(children) => Rc::make_mut(children),
            Self::Empty => unreachable!(),
        }
    }

    /// Appends a cached batch, retaining its allocation when it is the scope's first output.
    fn append(&mut self, children: Rc<Vec<Node>>) {
        if children.is_empty() {
            return;
        }
        if matches!(self, Self::Empty) {
            *self = Self::Shared(children);
        } else {
            self.children_mut().extend(children.iter().cloned());
        }
    }

    /// Returns owned children, cloning only when another owner retains the cached batch.
    fn into_vec(self) -> Vec<Node> {
        match self {
            Self::Empty => Vec::new(),
            Self::Owned(children) => children,
            Self::Shared(children) => Rc::unwrap_or_clone(children),
        }
    }

    /// Returns shared children without copying a cached batch.
    fn into_shared(self) -> Rc<Vec<Node>> {
        match self {
            Self::Empty => Rc::new(Vec::new()),
            Self::Owned(children) => Rc::new(children),
            Self::Shared(children) => children,
        }
    }
}

thread_local! {
    /// One entry per open scope, each holding the children emitted so far.
    static SCOPES: RefCell<Vec<Scope>> = const { RefCell::new(Vec::new()) };
}

/// Restores the scope stack to `depth` on drop, so a panic inside a
/// composable cannot leave stale scopes behind.
struct ScopeGuard {
    depth: usize,
}

impl Drop for ScopeGuard {
    /// Restores the stack depth that preceded this collection, including during unwinding.
    fn drop(&mut self) {
        SCOPES.with(|scopes| scopes.borrow_mut().truncate(self.depth));
    }
}

/// Runs `content` in a fresh scope and returns its owned or shared children.
///
/// # Panics
///
/// Propagates a panic from `content`, restoring the previous scope stack.
fn collect_scope(content: impl FnOnce()) -> Scope {
    let depth = SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        scopes.push(Scope::Empty);
        scopes.len() - 1
    });
    let _guard = ScopeGuard { depth };
    content();
    SCOPES
        .with(|scopes| scopes.borrow_mut().pop())
        .expect("composition scope stack was corrupted")
}

/// Runs `content` in a fresh scope and returns the nodes it emitted.
///
/// # Panics
///
/// Propagates a panic from `content`, restoring the previous scope stack.
pub fn collect(content: impl FnOnce()) -> Vec<Node> {
    collect_scope(content).into_vec()
}

/// Emits `node` into the current scope.
///
/// # Panics
///
/// Panics when called outside composition.
pub fn emit(node: Node) {
    let node = SCOPES.with(|scopes| match scopes.borrow_mut().last_mut() {
        Some(children) => {
            children.children_mut().push(node);
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
    let children = collect_scope(root);
    if children.len() == 1 {
        children.into_vec().remove(0)
    } else {
        Node {
            modifier: Modifier::new(),
            content: Content::Layout {
                policy: Rc::new(Overlay::default()),
                children: children.into_shared(),
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
    let children = rustle_runtime::group(|| collect_scope(content).into_shared());
    emit(Node {
        modifier,
        content: Content::Layout {
            policy: Rc::new(policy),
            children,
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
/// Cached children are shared until surrounding output requires an owned list.
/// Effects inside a skipped block do not run again.
#[track_caller]
pub fn skippable<D: PartialEq + 'static>(deps: D, content: impl FnOnce()) {
    let nodes = rustle_runtime::memo(deps, || collect_scope(content).into_shared());
    let uncollected = SCOPES.with(|scopes| {
        let mut scopes = scopes.borrow_mut();
        if let Some(scope) = scopes.last_mut() {
            scope.append(nodes);
            None
        } else {
            Some(nodes)
        }
    });
    assert!(
        uncollected.is_none_or(|nodes| nodes.is_empty()),
        "composables can only be called during composition"
    );
}

/// Runs `content` like a block of composables, as one node that is composed again alone
/// when state it read changes.
///
/// The block remembers the state it read. When that changes, only this block runs
/// again, in place: the code around it does not run, and its layout is measured again
/// while the layout of everything else that did not change is reused. Calling it again
/// from running code with equal `deps` and unchanged reads skips it, like
/// [`skippable`]. Pass everything `content` takes from outside in `deps`.
///
/// It stands for a single node: when `content` emits exactly one, that is the node;
/// otherwise the nodes are placed over each other by an [`Overlay`], as a root is.
/// State read by the code around the block, rather than inside it, still composes
/// that code again.
///
/// # Panics
///
/// Panics when called outside composition.
#[track_caller]
pub fn restartable<D: PartialEq + 'static>(deps: D, content: impl Fn() + 'static) {
    let scope = rustle_runtime::restartable(deps, move || compose(&content));
    emit(Node {
        modifier: Modifier::new(),
        content: Content::Scope(scope),
    });
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
