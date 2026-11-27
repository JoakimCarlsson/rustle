use std::cell::Cell;
use std::rc::Rc;

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{Border, Color, DrawCommand, LayerEffect};
use rustle_text::{TextMeasurer, TextSpan};
use rustle_unit::Constraints;

use super::ContentScale;
use super::measure::measure_node;
use super::measured::{Measured, Paint};
use super::regions::{
    ButtonRegion, CursorRegion, DisplayList, DragRegion, DragSourceRegion, DropTargetRegion,
    FileDropRegion, FocusRegion, HitRegion, HoverRegion, ImeCaretRegion, InteractionRegion,
    KeyRegion, LongPressRegion, PinchRegion, PointerInputRegion, PointerMoveRegion, PopupRegion,
    PressRegion, ScrollRegion, SemanticsNode, TaggedNode, intersect,
};
use crate::drag_and_drop::{DragAndDropSourceRegion, DragAndDropTargetRegion};
use crate::input::{Focus, PointerButton};
use crate::interaction::{Interaction, PointerState, resolve};
use crate::modifier::{Fill, FocusedFn};
use crate::popup::{PopupPlacement, popup_position};
use crate::scroll::{ScrollState, ScrollbarVisibility, thumb};
use crate::semantics::{Role, Semantics};
use crate::{Node, TextEditing};

#[path = "reuse.rs"]
mod reuse;

pub(super) use reuse::Reuse;

/// How the edges of laid-out areas are placed on the pixel grid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PixelRounding {
    /// Areas are drawn where layout put them, at fractional positions if need be.
    #[default]
    None,
    /// Every area's edges are rounded to whole logical pixels: its origin is rounded, and its
    /// size becomes the distance between its rounded edges, so neighbours still meet. The
    /// rounding is of each area's unrounded place in the window, so it never adds up down
    /// the tree. Text keeps its measured width, so it wraps as it was measured.
    Logical,
    /// As `Logical`, to whole physical pixels at [`DisplayOptions::scale`].
    Physical,
}

/// How [`display_list_with`] paints what it laid out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DisplayOptions {
    /// Whether and to what edges are rounded.
    pub rounding: PixelRounding,
    /// Physical pixels per logical pixel, for [`PixelRounding::Physical`].
    pub scale: f32,
}

impl Default for DisplayOptions {
    /// No rounding, at a scale of one.
    fn default() -> Self {
        Self {
            rounding: PixelRounding::None,
            scale: 1.0,
        }
    }
}

impl DisplayOptions {
    /// How many grid steps make a logical pixel, or `None` when nothing is rounded.
    fn grid(self) -> Option<f32> {
        match self.rounding {
            PixelRounding::None => None,
            PixelRounding::Logical => Some(1.0),
            PixelRounding::Physical if self.scale.is_finite() && self.scale > 0.0 => {
                Some(self.scale)
            }
            PixelRounding::Physical => Some(1.0),
        }
    }
}

/// Lays out `root` within `constraints`.
pub fn display_list(
    root: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
) -> DisplayList {
    display_list_with(root, constraints, text, DisplayOptions::default())
}

/// Lays out `root` within `constraints` like [`display_list`], painting as `options` says,
/// such as with edges rounded to whole pixels.
pub fn display_list_with(
    root: &Node,
    constraints: Constraints,
    text: &mut dyn TextMeasurer,
    options: DisplayOptions,
) -> DisplayList {
    LaidOut::new(root, constraints, text, options).paint(PointerState::default())
}

/// A tree measured once and kept, so it can be painted again when only the
/// [`Interaction`] state of its areas changed, without composing or measuring.
///
/// [`display_list_with`] measures and paints in one go; a host that resolves interaction
/// state (see [`Modifier::draw_behind_interactive`](crate::Modifier::draw_behind_interactive))
/// keeps this and calls [`paint`](Self::paint) when
/// [`DisplayList::interactions_changed`] says so.
pub struct LaidOut {
    measured: Measured,
    /// The size of the window, which `on_placed` handlers are told.
    window: Size,
    /// How many grid steps make a logical pixel when edges are rounded to them.
    grid: Option<f32>,
}

impl LaidOut {
    /// Measures `root` within `constraints`, measuring text with `text`, to be painted as
    /// `options` says.
    pub fn new(
        root: &Node,
        constraints: Constraints,
        text: &mut dyn TextMeasurer,
        options: DisplayOptions,
    ) -> Self {
        let measured = measure_node(root, constraints, text);
        let window = Size::new(
            if constraints.max_width.is_finite() {
                constraints.max_width
            } else {
                measured.size.width
            },
            if constraints.max_height.is_finite() {
                constraints.max_height
            } else {
                measured.size.height
            },
        );
        Self {
            measured,
            window,
            grid: options.grid(),
        }
    }

    /// Paints the measured tree into a display list, drawing each interactive area in the
    /// state `pointer` and the focus held now put it in.
    ///
    /// Whether something painted later covers an area is only known once everything is
    /// painted, so an area that turns out to be covered where the pointer is gets painted a
    /// second time; with no interactive area, or none covered, the tree is painted once.
    pub fn paint(&self, pointer: PointerState) -> DisplayList {
        self.paint_reusing(pointer, DisplayList::default())
    }

    /// Paints the measured tree as [`paint`](Self::paint) does, reusing what `previous`, the
    /// list painted last, holds of the subtrees kept from the tree it was painted from.
    ///
    /// A subtree laid out again is painted again. One whose layout was kept is reused, moved
    /// to where it is now, when it is painted in the same circumstances (the alpha, focus,
    /// interaction group and clips around it, the window and pixel grid), nothing its drawing
    /// read changed, and its interactive areas are in the same state; the unchanged subtrees
    /// inside a subtree painted again are reused in turn. The list is the same as painting
    /// everything would make. Drawing closures in a reused subtree do not run, so one that
    /// reads something outside composition (not state) is only seen to change by
    /// [`paint`](Self::paint).
    pub fn paint_reusing(&self, pointer: PointerState, previous: DisplayList) -> DisplayList {
        let list = self.paint_in(pointer, None, previous);
        if list.interactions.is_empty() || !list.interactions_changed(pointer) {
            return list;
        }
        let resolved = list.resolve_interactions(pointer);
        self.paint_in(pointer, Some(resolved), list)
    }

    /// Paints the tree once, with the interactive areas in the states `resolved` gives, in
    /// paint order, or in the state worked out as they are painted when it is `None`, reusing
    /// what `previous` holds of unchanged subtrees.
    fn paint_in(
        &self,
        pointer: PointerState,
        resolved: Option<Vec<Interaction>>,
        previous: DisplayList,
    ) -> DisplayList {
        let mut painter = Painter::new(self.window, self.grid, pointer, resolved);
        painter.previous = reuse::Previous::new(previous);
        painter.paint(&self.measured, Offset::default());
        painter.paint_deferred();
        painter.finish()
    }
}

/// Something to paint after the rest of the tree, because it has a z-index.
struct Deferred<'a> {
    z_index: i32,
    origin: Offset,
    alpha: f32,
    /// The clips around it where it was declared, outermost first, which cut it
    /// as well unless it is unclipped.
    clips: Vec<(Rect, Option<[f32; 4]>)>,
    /// The semantics it was declared inside, innermost last, so what it describes is still
    /// their child.
    parents: Vec<usize>,
    /// How many enclosing semantics drop every description inside them.
    clearing: usize,
    /// The focus of the nearest focusable around it where it was declared.
    focus: Option<Focus>,
    /// The focused flag given from outside around it where it was declared.
    focused_flag: Option<FocusedFn>,
    /// The visible rectangle of the interaction group around it where it was declared.
    group: Option<Rect>,
    /// The key handlers it was declared inside, as indices into the list, outermost first,
    /// which see the keys of what holds focus in it.
    key_scopes: Vec<usize>,
    /// The bounds of the layout it was declared in, the anchor of a popup.
    anchor: Option<Rect>,
    /// The areas that hear pointer events it was declared inside, innermost last.
    pointer_event_parents: Vec<usize>,
    measured: &'a Measured,
}

/// Paints the transient measured tree into an independently owned display list.
struct Painter<'a> {
    /// The drag-end handler of the area being painted, for the drag handler inside it.
    pending_end: Option<Rc<dyn Fn()>>,
    /// Raised areas, painted over the rest in order of z-index.
    deferred: Vec<Deferred<'a>>,
    /// How opaque everything painted now is, from the alpha modifiers around it.
    alpha: f32,
    /// How many enclosing semantics merge the text inside into themselves.
    merging: usize,
    /// How many enclosing semantics drop every description inside them.
    clearing: usize,
    /// The semantics being painted inside, as indices into the list, innermost last.
    parents: Vec<usize>,
    /// The text found inside each merging semantics, innermost last.
    merged_text: Vec<Vec<String>>,
    /// The described areas whose label was made by merging the text inside them.
    merged_labels: Vec<usize>,
    list: DisplayList,
    /// The clips in effect, innermost last, cutting hit and scroll regions.
    clips: Vec<Rect>,
    /// The corner radii of each of `clips`, if it is rounded.
    clip_radii: Vec<Option<[f32; 4]>>,
    /// How large the window is.
    window: Size,
    /// How many grid steps make a logical pixel when edges are rounded to them.
    grid: Option<f32>,
    /// The focus of the nearest focusable around what is being painted.
    focus: Option<Focus>,
    /// The focus a [`focus_requester`](crate::Modifier::focus_requester) around what is being
    /// painted gives the first focus target inside it, until one takes it.
    requester: Option<Focus>,
    /// The scrolling areas around what is being painted, as indices into the list, outermost
    /// first.
    scroll_parents: Vec<usize>,
    /// The focus-change observers around what is being painted still waiting for the first
    /// focus target inside them, outermost first; the first one painted takes them all.
    focus_observers: Vec<Rc<dyn Fn(bool)>>,
    /// The key handlers around what is being painted, as indices into the list, outermost
    /// first.
    key_scopes: Vec<usize>,
    /// The focused flag given from outside around what is being painted.
    focused_flag: Option<FocusedFn>,
    /// The visible rectangle of the nearest interaction group around what is being painted.
    group: Option<Rect>,
    /// Where the pointer is and where it went down, for interactive areas.
    pointer: PointerState,
    /// The state of each interactive area in paint order, when already resolved.
    resolved: Option<Vec<Interaction>>,
    /// The unrounded bounds of the layouts being painted inside, in the window, innermost
    /// last: the anchor of a popup declared in them.
    anchors: Vec<Rect>,
    /// The areas that hear pointer events being painted inside, as indices into the list,
    /// innermost last.
    pointer_event_parents: Vec<usize>,
    /// The list painted before, whose unchanged subtrees are reused.
    previous: Option<reuse::Previous>,
    /// What this paint keeps of its subtrees for the next one.
    kept: reuse::Kept,
    /// Whether a clip cut an area of the subtree being painted.
    cut: Cell<bool>,
    /// Whether a drag handler in the subtree being painted took a drag-end handler.
    used_end: Cell<bool>,
    /// The union of the areas of the subtree being painted that were cut to the clips.
    extent: Cell<Option<Rect>>,
    /// Whether the subtree being painted draws something that changes without its state
    /// changing, or reaches out of itself, so it cannot be reused.
    volatile: bool,
}

impl<'a> Painter<'a> {
    /// A painter of a window of size `window`, rounding edges to `grid` steps a logical pixel
    /// if given, with interactive areas in the state `pointer` puts them in, or in the states
    /// `resolved` gives in paint order.
    fn new(
        window: Size,
        grid: Option<f32>,
        pointer: PointerState,
        resolved: Option<Vec<Interaction>>,
    ) -> Self {
        Self {
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
            window,
            grid,
            focus: None,
            requester: None,
            scroll_parents: Vec::new(),
            focus_observers: Vec::new(),
            key_scopes: Vec::new(),
            focused_flag: None,
            group: None,
            pointer,
            resolved,
            anchors: Vec::new(),
            pointer_event_parents: Vec::new(),
            previous: None,
            kept: reuse::Kept::default(),
            cut: Cell::new(false),
            used_end: Cell::new(false),
            extent: Cell::new(None),
            volatile: false,
        }
    }

    /// The list painted, keeping what the next paint can reuse. What is left of the list
    /// painted before is retired, to be dropped once the frame is shown.
    fn finish(mut self) -> DisplayList {
        if let Some(previous) = self.previous.take() {
            crate::retire(previous);
        }
        self.list.reuse = self.kept.into_reuse();
        self.list
    }

    /// Where an area whose unrounded place is `exact` and size is `size` is drawn: as it is,
    /// or with its edges rounded to the grid, the size being the distance between them.
    fn snap(&self, exact: Offset, size: Size) -> (Offset, Size) {
        let Some(grid) = self.grid else {
            return (exact, size);
        };
        let round = |value: f32| (value * grid).round() / grid;
        let origin = Offset::new(round(exact.x), round(exact.y));
        let far = Offset::new(round(exact.x + size.width), round(exact.y + size.height));
        (origin, Size::new(far.x - origin.x, far.y - origin.y))
    }

    /// Paints the popup `inner` where `placement`'s provider puts it against the innermost
    /// layout around it (or `rect`, its own place, at the root), and notes it in the list.
    /// Its size is the extent of what it holds, since a popup takes no room where it is. As
    /// a window of its own does in Compose, a popup takes the pointer events that land on it,
    /// so they do not reach what lies beneath; a focusable one also takes those that land
    /// outside it, every button included, and one that is not focusable lets them through.
    fn paint_popup(&mut self, placement: &PopupPlacement, inner: &'a Measured, rect: Rect) {
        let anchor = self.anchors.last().copied().unwrap_or(rect);
        let size = content_extent(inner);
        let at = popup_position(
            placement.provider.as_ref(),
            placement.properties.clipping_enabled,
            anchor,
            self.window,
            size,
        );
        let (origin, size) = self.snap(at, size);
        if placement.properties.focusable {
            self.swallow_pointer(Rect::new(Offset::default(), self.window));
        }
        self.swallow_pointer(Rect::new(origin, size));
        self.list.popups.push(PopupRegion {
            rect: Rect::new(origin, size),
            properties: placement.properties,
            on_dismiss_request: placement.on_dismiss_request.clone(),
        });
        self.paint(inner, at);
    }

    /// Notes `rect` as an area that takes the presses, clicks and drags landing on it, the
    /// events of every pointer button and those heard through
    /// [`on_pointer_event`](crate::Modifier::on_pointer_event) and
    /// [`pointer_input`](crate::Modifier::pointer_input), and does nothing with them, so they
    /// do not reach what was painted beneath it.
    fn swallow_pointer(&mut self, rect: Rect) {
        for button in [
            PointerButton::Primary,
            PointerButton::Secondary,
            PointerButton::Middle,
            PointerButton::Back,
            PointerButton::Forward,
        ] {
            self.list.buttons.push(ButtonRegion {
                rect,
                origin: rect.origin,
                button,
                on_event: Rc::new(|_| {}),
            });
        }
        self.list
            .pointer_events
            .push(crate::pointer::PointerEventRegion {
                rect,
                origin: rect.origin,
                parent: None,
                depth: 0,
                event_type: crate::pointer::PointerEventType::Unknown,
                pass: crate::pointer::PointerEventPass::Main,
                on_event: Rc::new(|_| {}),
            });
        self.list.pointer_inputs.push(PointerInputRegion {
            rect,
            origin: rect.origin,
            on_event: Rc::new(|_| false),
        });
        let buttons = self.list.buttons.len();
        self.list.regions.push(HitRegion {
            rect,
            corner_radius: 0.0,
            feedback: false,
            hand: false,
            on_click: Rc::new(|| {}),
            buttons_inside: buttons..buttons,
        });
        self.list.presses.push(PressRegion {
            rect,
            origin: rect.origin,
            on_press: Rc::new(|_| {}),
            view_configuration: crate::ViewConfiguration::default(),
        });
        self.list.drags.push(DragRegion {
            on_end: None,
            rect,
            origin: rect.origin,
            on_drag: Rc::new(|_| {}),
            takes_pointer: false,
            inputs_before: self.list.pointer_inputs.len(),
        });
    }

    /// `distance` rounded to the grid when edges are rounded, as Compose scrolls by whole
    /// pixels: content scrolled to an end whose length is not whole then sits on the grid
    /// as the areas around it do.
    fn on_grid(&self, distance: f32) -> f32 {
        self.grid
            .map_or(distance, |grid| (distance * grid).round() / grid)
    }

    /// Paints what was raised with a z-index, lowest first, over what is already painted.
    fn paint_deferred(&mut self) {
        while !self.deferred.is_empty() {
            let mut raised = std::mem::take(&mut self.deferred);
            raised.sort_by_key(|raised| raised.z_index);
            for raised in raised {
                self.alpha = raised.alpha;
                self.clips.clear();
                self.clip_radii.clear();
                for (rect, radii) in &raised.clips {
                    match radii {
                        Some(radii) => self.push_rounded_clip(*rect, *radii),
                        None => self.push_clip(*rect),
                    }
                }
                self.parents = raised.parents;
                self.clearing = raised.clearing;
                self.focus = raised.focus;
                self.focused_flag = raised.focused_flag;
                self.group = raised.group;
                let merging = self.merging_node();
                if merging.is_some() {
                    self.merging += 1;
                    self.merged_text.push(Vec::new());
                }
                let first = self.list.focusables.len();
                self.key_scopes = raised.key_scopes;
                self.anchors = raised.anchor.into_iter().collect();
                self.pointer_event_parents = raised.pointer_event_parents;
                self.paint(raised.measured, raised.origin);
                let painted = first..self.list.focusables.len();
                if !painted.is_empty() {
                    for index in std::mem::take(&mut self.key_scopes) {
                        self.list.key_handlers[index]
                            .raised_focusables
                            .push(painted.clone());
                    }
                }
                if let Some(index) = merging {
                    self.merging -= 1;
                    let text = self.merged_text.pop().unwrap_or_default();
                    self.add_merged_text(index, text);
                }
                for _ in &raised.clips {
                    self.pop_clip();
                }
            }
        }
    }

    /// The innermost enclosing described area that merges the text inside it, if any, among
    /// the semantics being painted inside.
    fn merging_node(&self) -> Option<usize> {
        (self.clearing == 0)
            .then(|| {
                self.parents
                    .iter()
                    .rev()
                    .copied()
                    .find(|index| self.list.semantics[*index].semantics.merge_descendants)
            })
            .flatten()
    }

    /// Gives the area at `index`, which merges its content, the `text` found in content that
    /// was painted after it was finished: it becomes the label, or is added to a label made
    /// by merging.
    fn add_merged_text(&mut self, index: usize, text: Vec<String>) {
        if text.is_empty() {
            return;
        }
        let merged_labels = &self.merged_labels;
        let node = &mut self.list.semantics[index].semantics;
        match &mut node.label {
            None => node.label = Some(text.join(" ")),
            Some(label) if merged_labels.contains(&index) => {
                label.push(' ');
                label.push_str(&text.join(" "));
            }
            Some(_) => {}
        }
    }

    /// `rect` cut to what the clips leave visible, noting `rect` in the extent of the subtree
    /// being painted, and whether the clips cut it.
    fn visible(&self, rect: Rect) -> Rect {
        let visible = self
            .clips
            .iter()
            .fold(rect, |rect, clip| intersect(rect, *clip));
        self.extent.set(reuse::union(self.extent.get(), Some(rect)));
        let apart = |a: f32, b: f32| (a - b).abs() > 1e-3;
        if apart(visible.origin.x, rect.origin.x)
            || apart(visible.origin.y, rect.origin.y)
            || apart(visible.size.width, rect.size.width)
            || apart(visible.size.height, rect.size.height)
        {
            self.cut.set(true);
        }
        visible
    }

    /// `color` as faded by the alpha modifiers in effect.
    fn faded(&self, color: Color) -> Color {
        Color {
            a: (f32::from(color.a) * self.alpha).round() as u8,
            ..color
        }
    }

    /// Draws `fill` over `rect`, faded by the alpha modifiers in effect.
    fn paint_fill(&mut self, fill: &Fill, rect: Rect) {
        let command = match fill {
            Fill::Radial {
                from,
                to,
                center,
                radius,
                corner_radius,
            } => DrawCommand::RadialGradient {
                rect,
                center: *center,
                radius: *radius,
                from: self.faded(*from),
                to: self.faded(*to),
                corner_radius: *corner_radius,
            },
            Fill::Sweep {
                from,
                to,
                center,
                start,
                corner_radius,
            } => DrawCommand::SweepGradient {
                rect,
                center: *center,
                start: *start,
                from: self.faded(*from),
                to: self.faded(*to),
                corner_radius: *corner_radius,
            },
            Fill::Shader {
                brush,
                corner_radius,
            } => {
                let shader = DrawCommand::Shader {
                    rect,
                    corner_radius: *corner_radius,
                    brush: brush.clone(),
                };
                if self.alpha < 1.0 {
                    let effect = LayerEffect {
                        alpha: self.alpha,
                        ..LayerEffect::default()
                    };
                    self.list
                        .commands
                        .push(DrawCommand::PushEffect { rect, effect });
                    self.list.commands.push(shader);
                    DrawCommand::PopEffect
                } else {
                    shader
                }
            }
            Fill::BackdropBlur {
                blur,
                corner_radius,
            } => DrawCommand::BackdropBlur {
                rect,
                corner_radius: *corner_radius,
                blur: *blur,
            },
            Fill::DropShadow {
                dx,
                dy,
                blur,
                color,
                corner_radius,
            } => DrawCommand::Shadow {
                rect: Rect::new(
                    Offset::new(rect.origin.x + dx, rect.origin.y + dy),
                    rect.size,
                ),
                corner_radius: *corner_radius,
                blur: *blur,
                color: self.faded(*color),
            },
            Fill::ShadowOf {
                shadow,
                corner_radius,
            } => {
                let spread = shadow.spread;
                let grown = Rect::new(
                    Offset::new(
                        rect.origin.x + shadow.offset.x - spread,
                        rect.origin.y + shadow.offset.y - spread,
                    ),
                    Size::new(
                        (rect.size.width + 2.0 * spread).max(0.0),
                        (rect.size.height + 2.0 * spread).max(0.0),
                    ),
                );
                let command = DrawCommand::Shadow {
                    rect: grown,
                    corner_radius: (corner_radius + spread).max(0.0),
                    blur: shadow.radius.max(0.0),
                    color: self.faded(shadow.effective_color()),
                };
                if shadow.blend_mode == rustle_graphics::BlendMode::SrcOver {
                    command
                } else {
                    let reach = shadow.radius.max(0.0) * 2.0;
                    let bounds = Rect::new(
                        Offset::new(grown.origin.x - reach, grown.origin.y - reach),
                        Size::new(
                            grown.size.width + 2.0 * reach,
                            grown.size.height + 2.0 * reach,
                        ),
                    );
                    self.list.commands.push(DrawCommand::PushEffect {
                        rect: bounds,
                        effect: rustle_graphics::LayerEffect {
                            blend_mode: shadow.blend_mode,
                            ..rustle_graphics::LayerEffect::default()
                        },
                    });
                    self.list.commands.push(command);
                    DrawCommand::PopEffect
                }
            }
        };
        self.list.commands.push(command);
    }

    fn push_clip(&mut self, rect: Rect) {
        self.clips.push(rect);
        self.clip_radii.push(None);
        self.list.commands.push(DrawCommand::PushClip { rect });
    }

    fn push_rounded_clip(&mut self, rect: Rect, radii: [f32; 4]) {
        self.clips.push(rect);
        self.clip_radii.push(Some(radii));
        self.list
            .commands
            .push(DrawCommand::PushRoundedClip { rect, radii });
    }

    fn pop_clip(&mut self) {
        self.clips.pop();
        self.clip_radii.pop();
        self.list.commands.push(DrawCommand::PopClip);
    }

    /// Draws the overlay scrollbar of a scrolling area over its content, and
    /// registers the strip along its edge that reveals and drags it.
    fn paint_scrollbar(&mut self, rect: Rect, state: &ScrollState, vertical: bool, offset: f32) {
        const MARGIN: f32 = 2.0;
        let style = state.scrollbar();
        let viewport = if vertical {
            rect.size.height
        } else {
            rect.size.width
        };
        let track = viewport - 2.0 * MARGIN;
        if style.visibility == ScrollbarVisibility::Hidden
            || state.content_length() <= viewport + 0.5
            || track < style.min_length
        {
            return;
        }
        let hit = style.hit_thickness.max(style.thickness + 2.0 * MARGIN);
        // The strip along the end of the scrolling axis, and its place on that axis.
        let strip = if vertical {
            Rect::new(
                Offset::new(rect.origin.x + rect.size.width - hit, rect.origin.y),
                Size::new(hit, rect.size.height),
            )
        } else {
            Rect::new(
                Offset::new(rect.origin.x, rect.origin.y + rect.size.height - hit),
                Size::new(rect.size.width, hit),
            )
        };
        let alpha = state.scrollbar_alpha();
        if alpha > 0.01 {
            let (start, length) = thumb(
                offset,
                state.content_length(),
                viewport,
                track,
                style.min_length,
            );
            let thickness = style.thickness;
            let color = state.scrollbar_color();
            let color = Color {
                a: (f32::from(color.a) * alpha).round() as u8,
                ..color
            };
            let thumb_rect = if vertical {
                Rect::new(
                    Offset::new(
                        rect.origin.x + rect.size.width - MARGIN - thickness,
                        rect.origin.y + MARGIN + start,
                    ),
                    Size::new(thickness, length),
                )
            } else {
                Rect::new(
                    Offset::new(
                        rect.origin.x + MARGIN + start,
                        rect.origin.y + rect.size.height - MARGIN - thickness,
                    ),
                    Size::new(length, thickness),
                )
            };
            self.list.commands.push(DrawCommand::Rect {
                rect: thumb_rect,
                color: self.faded(color),
                corner_radius: style.corner_radius.min(thickness / 2.0),
                border: None,
            });
        }
        let visible = self.visible(strip);
        let hover_state = state.clone();
        self.list.hovers.push(HoverRegion {
            rect: visible,
            on_hover: Rc::new(move |hovering| hover_state.set_scrollbar_hovered(hovering)),
            covered_after: self.list.regions.len(),
        });
        // Only a bar that can be seen can be grabbed, so hidden ones leave the content its pointer.
        if alpha > 0.05 {
            let (drag_state, end_state) = (state.clone(), state.clone());
            self.list.drags.push(DragRegion {
                on_end: Some(Rc::new(move || end_state.end_thumb_drag())),
                rect: visible,
                origin: strip.origin,
                takes_pointer: true,
                inputs_before: self.list.pointer_inputs.len(),
                on_drag: Rc::new(move |position| {
                    let along = if vertical { position.y } else { position.x };
                    drag_state.drag_thumb(along - MARGIN, track);
                }),
            });
        }
    }

    /// Adds a description of `rect` inside the semantics being painted and returns its index.
    fn push_semantics(&mut self, rect: Rect, semantics: Semantics) -> usize {
        let index = self.list.semantics.len();
        self.list.semantics.push(SemanticsNode {
            rect: self.visible(rect),
            semantics,
            parent: self.parents.last().copied(),
            on_click: None,
        });
        index
    }

    /// Draws what editing shows over rich text laid out as `layout` at `origin`: the input
    /// method's composition underlined in `color`, then the caret, and reports where text
    /// input goes when `editing` has an `ime_caret`. `text` is the joined text of the spans,
    /// since `editing` counts characters and `layout` bytes.
    fn paint_rich_editing(
        &mut self,
        editing: &TextEditing,
        layout: &rustle_text::TextLayout,
        text: &str,
        origin: Offset,
        color: Color,
    ) {
        let byte_of = |index: usize| byte_at(text, index);
        if let Some((start, end)) = editing.underline
            && start < end
        {
            for rect in layout.selection_rects(byte_of(start), byte_of(end)) {
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(
                            origin.x + rect.origin.x,
                            origin.y + rect.origin.y + rect.size.height - 2.0,
                        ),
                        Size::new(rect.size.width, 1.0),
                    ),
                    color: self.faded(color),
                    corner_radius: 0.0,
                    border: None,
                });
            }
        }
        if let Some(caret) = editing.ime_caret {
            let from = editing
                .underline
                .map_or(caret, |(start, _)| start)
                .min(caret);
            let start = layout.caret(byte_of(from), editing.caret_affinity);
            let at = layout.caret(byte_of(caret), editing.caret_affinity);
            let left = if start.y == at.y { start.x } else { 0.0 };
            self.list.ime_area = Some(self.visible(Rect::new(
                Offset::new(origin.x + left, origin.y + at.y),
                Size::new((at.x - left).max(1.0), at.height),
            )));
        }
        if let Some(caret) = editing.caret {
            let at = layout.caret(byte_of(caret), editing.caret_affinity);
            self.list.commands.push(DrawCommand::Rect {
                rect: Rect::new(
                    Offset::new(origin.x + at.x - 0.75, origin.y + at.y),
                    Size::new(1.5, at.height),
                ),
                color: self.faded(editing.caret_color),
                corner_radius: 0.0,
                border: None,
            });
        }
    }

    /// Describes text as a label of its own, or gives it to the merging semantics around
    /// it, or drops it when they clear what is inside.
    fn describe_text(&mut self, rect: Rect, text: String) {
        if self.clearing > 0 {
            return;
        }
        match self.merged_text.last_mut() {
            Some(merged) => merged.push(text),
            None => {
                self.push_semantics(rect, Semantics::new(Role::Label).label(text));
            }
        }
    }

    /// Paints `measured` once at `exact`, its unrounded place, deferring raised descendants
    /// and consuming rich spans. With pixel rounding on, the area is drawn at its rounded
    /// place while its children are placed from the unrounded one, so rounding never adds up.
    fn paint(&mut self, measured: &'a Measured, exact: Offset) {
        let (origin, size) = self.snap(exact, measured.size);
        let rect = Rect::new(origin, size);
        match &measured.paint {
            Paint::Text {
                content,
                style,
                editing,
            } => {
                if editing.is_none() {
                    self.describe_text(rect, content.clone());
                }
                // The shaped text, when it is being edited: wrapped lines, selection and caret come from it.
                let edit_layout = editing
                    .as_ref()
                    .and_then(|(editing, _)| editing.layout.borrow().clone());
                let byte_of = |index: usize| {
                    content
                        .char_indices()
                        .nth(index)
                        .map_or(content.len(), |(byte, _)| byte)
                };
                if let Some((editing, _)) = editing
                    && let Some(layout) = &edit_layout
                    && let Some((start, end)) = editing.selection
                    && start != end
                {
                    for rect in layout.selection_rects(byte_of(start), byte_of(end)) {
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
                                rect.size,
                            ),
                            color: self.faded(editing.selection_color),
                            corner_radius: 0.0,
                            border: None,
                        });
                    }
                }
                self.list.commands.push(DrawCommand::Text {
                    x: origin.x,
                    y: origin.y,
                    width: measured.size.width,
                    content: content.clone(),
                    color: self.faded(style.color),
                    font_size: style.font_size,
                    line_height: style.line_height,
                    font_weight: style.font_weight.0,
                    font_family: style.font_family,
                    align: style.align,
                    max_lines: style.max_lines,
                    italic: style.italic,
                    letter_spacing: style.letter_spacing,
                    ligatures: style.ligatures,
                    wrap: style.wrap,
                });
                if style.underline || style.strikethrough {
                    let layout = match &edit_layout {
                        Some(layout) => Rc::clone(layout),
                        None => Rc::new(rustle_text::layout_spans(
                            &[TextSpan::new(content.as_str())],
                            style,
                            Some(measured.size.width + 1.0),
                        )),
                    };
                    let faded = |color| self.faded(color);
                    let lines = layout.decoration_commands(
                        origin,
                        style.underline,
                        style.strikethrough,
                        &faded,
                    );
                    self.list.commands.extend(lines);
                }
                // An input method's composition is underlined.
                if let Some((editing, boundaries)) = editing
                    && let Some((start, end)) = editing.underline
                    && start < end
                    && end < boundaries.len()
                {
                    let rects = match &edit_layout {
                        Some(layout) => layout.selection_rects(byte_of(start), byte_of(end)),
                        None => (boundaries[start].line..=boundaries[end].line)
                            .filter_map(|line| {
                                let on_line = |index: &usize| boundaries[*index].line == line;
                                let first = (start..=end).find(on_line)?;
                                let last = (start..=end).rev().find(on_line)?;
                                Some(Rect::new(
                                    Offset::new(
                                        boundaries[first].x,
                                        line as f32 * style.line_height,
                                    ),
                                    Size::new(
                                        boundaries[last].x - boundaries[first].x,
                                        style.line_height,
                                    ),
                                ))
                            })
                            .collect(),
                    };
                    for rect in rects {
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(
                                    origin.x + rect.origin.x,
                                    origin.y + rect.origin.y + rect.size.height - 2.0,
                                ),
                                Size::new(rect.size.width, 1.0),
                            ),
                            color: self.faded(style.color),
                            corner_radius: 0.0,
                            border: None,
                        });
                    }
                }
                // Where an input method puts its candidate popup: the composition up to the caret.
                if let Some((editing, boundaries)) = editing
                    && let Some(caret) = editing.ime_caret
                    && caret < boundaries.len()
                {
                    let from = editing
                        .underline
                        .map_or(caret, |(start, _)| start)
                        .min(caret);
                    let place = |index: usize| match &edit_layout {
                        Some(layout) => {
                            let at = layout.caret(byte_of(index), editing.caret_affinity);
                            (at.x, at.y, at.height)
                        }
                        None => (
                            boundaries[index].x,
                            boundaries[index].line as f32 * style.line_height,
                            style.line_height,
                        ),
                    };
                    let ((from_x, from_y, _), (x, y, height)) = (place(from), place(caret));
                    let left = if from_y == y { from_x } else { 0.0 };
                    self.list.ime_area = Some(self.visible(Rect::new(
                        Offset::new(origin.x + left, origin.y + y),
                        Size::new((x - left).max(1.0), height),
                    )));
                }
                if let Some((editing, boundaries)) = editing
                    && let Some(caret) = editing.caret
                {
                    let (x, y, height) = match &edit_layout {
                        Some(layout) => {
                            let at = layout.caret(byte_of(caret), editing.caret_affinity);
                            (at.x, at.y, at.height)
                        }
                        None => (
                            boundaries[caret].x,
                            boundaries[caret].line as f32 * style.line_height,
                            style.line_height,
                        ),
                    };
                    self.list.commands.push(DrawCommand::Rect {
                        rect: Rect::new(
                            Offset::new(origin.x + x - 0.75, origin.y + y),
                            Size::new(1.5, height),
                        ),
                        color: self.faded(editing.caret_color),
                        corner_radius: 0.0,
                        border: None,
                    });
                }
            }
            Paint::RichText {
                spans,
                style,
                editing,
            } => {
                let mut spans = spans.clone();
                let label: String = spans.iter().map(|span| span.text.as_str()).collect();
                let edit_layout = editing
                    .as_ref()
                    .and_then(|editing| editing.layout.borrow().clone());
                let byte_of = |index: usize| byte_at(&label, index);
                if let Some(editing) = editing
                    && let Some(layout) = &edit_layout
                    && let Some((start, end)) = editing.selection
                    && start != end
                {
                    for rect in layout.selection_rects(byte_of(start), byte_of(end)) {
                        self.list.commands.push(DrawCommand::Rect {
                            rect: Rect::new(
                                Offset::new(origin.x + rect.origin.x, origin.y + rect.origin.y),
                                rect.size,
                            ),
                            color: self.faded(editing.selection_color),
                            corner_radius: 0.0,
                            border: None,
                        });
                    }
                }
                // Backgrounds, underlines and strikethroughs are rectangles from the shaped layout.
                let decorated = style.underline
                    || style.strikethrough
                    || spans.iter().any(|span| {
                        span.underline || span.strikethrough || span.background.is_some()
                    });
                let layout = decorated.then(|| {
                    rustle_text::layout_styled(&spans, style, Some(measured.size.width + 1.0))
                });
                if let Some(layout) = &layout {
                    let faded = |color| self.faded(color);
                    let backgrounds = layout.background_commands(origin, &faded);
                    self.list.commands.extend(backgrounds);
                }
                for span in &mut spans {
                    span.color = self.faded(span.color);
                }
                self.list.commands.push(DrawCommand::RichText {
                    x: origin.x,
                    y: origin.y,
                    width: measured.size.width,
                    spans,
                    font_size: style.font_size,
                    line_height: style.line_height,
                    align: style.align,
                    max_lines: style.max_lines,
                    wrap: style.wrap,
                });
                if let Some(layout) = &layout {
                    let faded = |color| self.faded(color);
                    let lines = layout.decoration_commands(origin, false, false, &faded);
                    self.list.commands.extend(lines);
                }
                if let Some(editing) = editing
                    && let Some(layout) = &edit_layout
                {
                    self.paint_rich_editing(editing, layout, &label, origin, style.color);
                }
                self.describe_text(rect, label);
            }
            Paint::Canvas { draw } => {
                for command in draw(size) {
                    self.list
                        .commands
                        .push(command.translated(origin.x, origin.y).faded(self.alpha));
                }
            }
            Paint::Image { image, scale } => {
                if self.merging == 0 && self.clearing == 0 {
                    self.push_semantics(rect, Semantics::new(Role::Image));
                }
                let (width, height) = (image.width() as f32, image.height() as f32);
                let fit = (size.width / width).min(size.height / height);
                let cover = (size.width / width).max(size.height / height);
                let drawn = match scale {
                    ContentScale::Stretch => size,
                    ContentScale::Fit => Size::new(width * fit, height * fit),
                    ContentScale::Crop => Size::new(width * cover, height * cover),
                };
                let at = Offset::new(
                    origin.x + (size.width - drawn.width) / 2.0,
                    origin.y + (size.height - drawn.height) / 2.0,
                );
                let crop = *scale == ContentScale::Crop;
                if crop {
                    self.push_clip(rect);
                }
                self.list.commands.push(DrawCommand::Image {
                    rect: Rect::new(at, drawn),
                    image: image.clone(),
                    corner_radius: 0.0,
                    opacity: self.alpha,
                });
                if crop {
                    self.pop_clip();
                }
            }
            Paint::Shared(shared) => self.paint_shared(shared, exact),
            Paint::Children(children) => {
                self.anchors.push(Rect::new(exact, measured.size));
                for (offset, child) in children {
                    self.paint(child, Offset::new(exact.x + offset.x, exact.y + offset.y));
                }
                self.anchors.pop();
            }
            Paint::Background {
                color,
                corner_radius,
                inner,
            } => {
                self.list.commands.push(DrawCommand::Rect {
                    rect,
                    color: self.faded(*color),
                    corner_radius: *corner_radius,
                    border: None,
                });
                self.paint(inner, exact);
            }
            Paint::Draw { draw, inner } => {
                let (commands, content) = draw(size);
                let count = commands.len();
                for (index, command) in commands.into_iter().enumerate() {
                    if content == Some(index) {
                        self.paint(inner, exact);
                    }
                    self.list
                        .commands
                        .push(command.translated(origin.x, origin.y).faded(self.alpha));
                }
                if content == Some(count) {
                    self.paint(inner, exact);
                }
            }
            Paint::Interactive { draw, inner } => {
                let index = self.list.interactions.len();
                let focus = self.focus.clone().or_else(|| chain_focus(inner));
                let visible = self.visible(rect);
                let interaction = self
                    .resolved
                    .as_ref()
                    .and_then(|resolved| resolved.get(index).copied())
                    .unwrap_or_else(|| {
                        resolve(
                            visible,
                            focus.as_ref(),
                            self.focused_flag.as_ref().map(|focused| focused()),
                            self.group,
                            self.pointer,
                            false,
                        )
                    });
                self.list.interactions.push(InteractionRegion {
                    rect: visible,
                    focus,
                    focused: self.focused_flag.clone(),
                    group: self.group,
                    covered_after: 0,
                    interaction,
                });
                let (commands, content) = draw(size, interaction);
                let count = commands.len();
                for (at, command) in commands.into_iter().enumerate() {
                    if content == Some(at) {
                        self.paint(inner, exact);
                    }
                    self.list
                        .commands
                        .push(command.translated(origin.x, origin.y).faded(self.alpha));
                }
                if content == Some(count) {
                    self.paint(inner, exact);
                }
                self.list.interactions[index].covered_after = self.list.regions.len();
            }
            Paint::InteractionFocused { focused, inner } => {
                let around = self.focused_flag.replace(Rc::clone(focused));
                self.paint(inner, exact);
                self.focused_flag = around;
            }
            Paint::KeepInWindow { inner } => {
                let inside = |start: f32, length: f32, room: f32| {
                    let back = (room - (start + length)).min(0.0);
                    back.max(-start)
                };
                let shift = Offset::new(
                    inside(rect.origin.x, rect.size.width, self.window.width),
                    inside(rect.origin.y, rect.size.height, self.window.height),
                );
                self.volatile = true;
                self.paint(inner, Offset::new(exact.x + shift.x, exact.y + shift.y));
            }
            Paint::InWindow { at, inner } => {
                self.volatile = true;
                self.paint(inner, *at);
            }
            Paint::Popup { placement, inner } => {
                self.volatile = true;
                self.paint_popup(placement, inner, Rect::new(exact, measured.size));
            }
            Paint::InteractionGroup { inner } => {
                let around = self.group.replace(self.visible(rect));
                self.paint(inner, exact);
                self.group = around;
            }
            Paint::Fill { fill, inner } => {
                self.paint_fill(fill, rect);
                self.paint(inner, exact);
            }
            Paint::Effect { effect, inner } => {
                let faded = LayerEffect {
                    alpha: effect.alpha * self.alpha,
                    ..*effect
                };
                self.list.commands.push(DrawCommand::PushEffect {
                    rect,
                    effect: faded,
                });
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::PopEffect);
            }
            Paint::Gradient {
                from,
                to,
                angle,
                corner_radius,
                inner,
            } => {
                self.list.commands.push(DrawCommand::Gradient {
                    rect,
                    from: self.faded(*from),
                    to: self.faded(*to),
                    angle: *angle,
                    corner_radius: *corner_radius,
                });
                self.paint(inner, exact);
            }
            Paint::Ring {
                color,
                thickness,
                start,
                sweep,
                inner,
            } => {
                self.paint(inner, exact);
                let side = size.width.min(size.height);
                self.list.commands.push(DrawCommand::Arc {
                    rect: Rect::new(
                        Offset::new(
                            origin.x + (size.width - side) / 2.0,
                            origin.y + (size.height - side) / 2.0,
                        ),
                        Size::new(side, side),
                    ),
                    color: self.faded(*color),
                    thickness: *thickness,
                    start: *start,
                    sweep: *sweep,
                });
            }
            Paint::DragEnd { on_end, inner } => {
                // Applies to the drag handler declared next, inside this.
                self.pending_end = Some(Rc::clone(on_end));
                self.paint(inner, exact);
                self.pending_end = None;
            }
            Paint::DragSource {
                payload,
                button,
                touch_slop,
                inner,
            } => {
                let start = self.list.commands.len();
                let index = self.list.drag_sources.len();
                self.list.drag_sources.push(DragSourceRegion {
                    rect: self.visible(rect),
                    button: *button,
                    payload: Rc::clone(payload),
                    commands: start..start,
                    touch_slop: *touch_slop,
                });
                self.paint(inner, exact);
                self.list.drag_sources[index].commands.end = self.list.commands.len();
            }
            Paint::DropTarget { on_drop, inner } => {
                self.list.drop_targets.push(DropTargetRegion {
                    rect: self.visible(rect),
                    on_drop: Rc::clone(on_drop),
                });
                self.paint(inner, exact);
            }
            Paint::DragAndDropSource {
                transfer_data,
                touch_slop,
                inner,
            } => {
                self.list
                    .drag_and_drop_sources
                    .push(DragAndDropSourceRegion {
                        rect: self.visible(rect),
                        origin,
                        transfer_data: Rc::clone(transfer_data),
                        touch_slop: *touch_slop,
                    });
                self.paint(inner, exact);
            }
            Paint::DragAndDropTarget {
                should_start,
                target,
                inner,
            } => {
                self.list
                    .drag_and_drop_targets
                    .push(DragAndDropTargetRegion {
                        rect: self.visible(rect),
                        should_start: Rc::clone(should_start),
                        target: Rc::clone(target),
                    });
                self.paint(inner, exact);
            }
            Paint::FileDrop { on_drop, inner } => {
                self.list.file_drops.push(FileDropRegion {
                    rect: self.visible(rect),
                    on_drop: Rc::clone(on_drop),
                });
                self.paint(inner, exact);
            }
            Paint::PointerInput { on_event, inner } => {
                self.list.pointer_inputs.push(PointerInputRegion {
                    rect: self.visible(rect),
                    origin,
                    on_event: Rc::clone(on_event),
                });
                self.paint(inner, exact);
            }
            Paint::PointerEvent {
                event_type,
                pass,
                on_event,
                inner,
            } => {
                let index = self.list.pointer_events.len();
                self.list
                    .pointer_events
                    .push(crate::pointer::PointerEventRegion {
                        rect: self.visible(rect),
                        origin,
                        parent: self.pointer_event_parents.last().copied(),
                        depth: self.pointer_event_parents.len(),
                        event_type: *event_type,
                        pass: *pass,
                        on_event: Rc::clone(on_event),
                    });
                self.pointer_event_parents.push(index);
                self.paint(inner, exact);
                self.pointer_event_parents.pop();
            }
            Paint::TextInput { session, inner } => {
                let index = self.list.text_inputs.len();
                let first = self.list.focusables.len();
                self.list
                    .text_inputs
                    .push(crate::text_input::TextInputRegion {
                        rect: self.visible(rect),
                        origin,
                        focus: self.focus.clone(),
                        session: session.clone(),
                    });
                self.paint(inner, exact);
                if self.list.text_inputs[index].focus.is_none() {
                    self.list.text_inputs[index].focus = self
                        .list
                        .focusables
                        .get(first)
                        .map(|region| region.focus.clone());
                }
            }
            Paint::Pinch { on_pinch, inner } => {
                self.list.pinches.push(PinchRegion {
                    rect: self.visible(rect),
                    on_pinch: Rc::clone(on_pinch),
                });
                self.paint(inner, exact);
            }
            Paint::BlocksPointer(inner) => {
                self.list.blockers.push(self.visible(rect));
                self.paint(inner, exact);
            }
            Paint::PointerMove { on_move, inner } => {
                let index = self.list.pointer_moves.len();
                self.list.pointer_moves.push(PointerMoveRegion {
                    rect: self.visible(rect),
                    origin,
                    on_move: Rc::clone(on_move),
                    covered_after: 0,
                });
                self.paint(inner, exact);
                self.list.pointer_moves[index].covered_after = self.list.regions.len();
            }
            Paint::LongPress {
                on_long_press,
                view_configuration,
                inner,
            } => {
                self.list.long_presses.push(LongPressRegion {
                    rect: self.visible(rect),
                    on_long_press: Rc::clone(on_long_press),
                    view_configuration: *view_configuration,
                });
                self.paint(inner, exact);
            }
            Paint::Cursor { icon, inner } => {
                self.list.cursors.push(CursorRegion {
                    rect: self.visible(rect),
                    icon: *icon,
                });
                self.paint(inner, exact);
            }
            Paint::ImeCaret { rect: caret, inner } => {
                let index = self.list.ime_carets.len();
                let first = self.list.focusables.len();
                self.list.ime_carets.push(ImeCaretRegion {
                    rect: self.visible(Rect::new(
                        Offset::new(origin.x + caret.origin.x, origin.y + caret.origin.y),
                        caret.size,
                    )),
                    focus: self.focus.clone(),
                });
                self.paint(inner, exact);
                if self.list.ime_carets[index].focus.is_none() {
                    self.list.ime_carets[index].focus = self
                        .list
                        .focusables
                        .get(first)
                        .map(|region| region.focus.clone());
                }
            }
            Paint::Hover { on_hover, inner } => {
                let index = self.list.hovers.len();
                self.list.hovers.push(HoverRegion {
                    rect: self.visible(rect),
                    on_hover: Rc::clone(on_hover),
                    covered_after: 0,
                });
                self.paint(inner, exact);
                self.list.hovers[index].covered_after = self.list.regions.len();
            }
            Paint::Placed { on_placed, inner } => {
                on_placed(rect, self.window);
                self.kept
                    .note(reuse::PlacedCall::Placed(Rc::clone(on_placed), rect));
                self.paint(inner, exact);
            }
            Paint::PlacedVisible { on_visible, inner } => {
                let visible = self.visible(rect);
                on_visible(visible);
                self.kept
                    .note(reuse::PlacedCall::Visible(Rc::clone(on_visible), visible));
                self.paint(inner, exact);
            }
            Paint::FocusChanged {
                on_focus_changed,
                inner,
            } => {
                let waiting = self.focus_observers.len();
                self.focus_observers.push(Rc::clone(on_focus_changed));
                self.paint(inner, exact);
                self.focus_observers.truncate(waiting);
            }
            Paint::TestTag { tag, inner } => {
                let index = self.list.test_tags.len();
                let first = self.list.focusables.len();
                self.list.test_tags.push(TaggedNode {
                    tag: Rc::clone(tag),
                    bounds: rect,
                    visible: self.visible(rect),
                    focusable: None,
                });
                self.paint(inner, exact);
                let around = self.focus.as_ref().and_then(|focus| {
                    self.list
                        .focusables
                        .iter()
                        .position(|region| region.focus.id() == focus.id())
                });
                self.list.test_tags[index].focusable = (self.list.focusables.len() > first)
                    .then_some(first)
                    .or(around);
            }
            Paint::ZIndex {
                z_index,
                clipped,
                inner,
            } => {
                let clips = if *clipped {
                    self.clips
                        .iter()
                        .copied()
                        .zip(self.clip_radii.iter().copied())
                        .collect()
                } else {
                    Vec::new()
                };
                self.volatile = true;
                self.deferred.push(Deferred {
                    z_index: *z_index,
                    origin: exact,
                    alpha: self.alpha,
                    clips,
                    parents: self.parents.clone(),
                    clearing: self.clearing,
                    focus: self.focus.clone(),
                    focused_flag: self.focused_flag.clone(),
                    group: self.group,
                    key_scopes: self.key_scopes.clone(),
                    anchor: self.anchors.last().copied(),
                    pointer_event_parents: self.pointer_event_parents.clone(),
                    measured: inner,
                });
            }
            Paint::BorderSide {
                width,
                color,
                inner,
            } => {
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(origin.x + size.width - width, origin.y),
                        Size::new(*width, size.height),
                    ),
                    color: self.faded(*color),
                    corner_radius: 0.0,
                    border: None,
                });
            }
            Paint::BorderBottom {
                width,
                color,
                inner,
            } => {
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::Rect {
                    rect: Rect::new(
                        Offset::new(origin.x, origin.y + size.height - width),
                        Size::new(size.width, *width),
                    ),
                    color: self.faded(*color),
                    corner_radius: 0.0,
                    border: None,
                });
            }
            Paint::GraphicsLayer {
                alpha,
                rotation,
                scale,
                inner,
            } => {
                self.list.commands.push(DrawCommand::PushLayer {
                    rect,
                    alpha: *alpha,
                    rotation: *rotation,
                    scale: *scale,
                });
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::PopLayer);
            }
            Paint::RotateWith { degrees, inner } => {
                self.list.commands.push(DrawCommand::PushLayer {
                    rect,
                    alpha: 1.0,
                    rotation: degrees().to_radians(),
                    scale: 1.0,
                });
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::PopLayer);
            }
            Paint::Alpha { alpha, inner } => {
                let before = self.alpha;
                self.alpha *= alpha;
                self.paint(inner, exact);
                self.alpha = before;
            }
            Paint::Border {
                width,
                color,
                corner_radius,
                inner,
            } => {
                self.paint(inner, exact);
                self.list.commands.push(DrawCommand::Rect {
                    rect,
                    color: Color::TRANSPARENT,
                    corner_radius: *corner_radius,
                    border: Some(Border {
                        width: *width,
                        color: self.faded(*color),
                    }),
                });
            }
            Paint::Shadow {
                elevation,
                corner_radius,
                inner,
            } => {
                self.list.commands.push(DrawCommand::Shadow {
                    rect: Rect::new(Offset::new(origin.x, origin.y + elevation / 2.0), size),
                    corner_radius: *corner_radius,
                    blur: *elevation,
                    color: self.faded(Color::rgba(0, 0, 0, 110)),
                });
                self.paint(inner, exact);
            }
            Paint::Clip(inner) => {
                self.push_clip(rect);
                self.paint(inner, exact);
                self.pop_clip();
            }
            Paint::ClipRounded { radii, inner } => {
                self.push_rounded_clip(rect, *radii);
                self.paint(inner, exact);
                self.pop_clip();
            }
            Paint::Scroll {
                state,
                vertical,
                offset,
                inner,
            } => {
                self.push_clip(rect);
                self.scroll_parents.push(self.list.scrolls.len());
                self.list.scrolls.push(ScrollRegion {
                    rect: self.visible(rect),
                    state: state.clone(),
                    vertical: *vertical,
                });
                let shift = self.on_grid(*offset);
                let inside = if *vertical {
                    Offset::new(exact.x, exact.y - shift)
                } else {
                    Offset::new(exact.x - shift, exact.y)
                };
                self.paint(inner, inside);
                self.scroll_parents.pop();
                self.volatile = true;
                self.paint_scrollbar(rect, state, *vertical, *offset);
                self.pop_clip();
            }
            Paint::Drag { on_drag, inner } => {
                if self.pending_end.is_some() {
                    self.used_end.set(true);
                }
                self.list.drags.push(DragRegion {
                    on_end: self.pending_end.clone(),
                    rect: self.visible(rect),
                    origin,
                    on_drag: Rc::clone(on_drag),
                    takes_pointer: false,
                    inputs_before: self.list.pointer_inputs.len(),
                });
                self.paint(inner, exact);
            }
            Paint::Semantics { semantics, inner } => {
                let regions = self.list.regions.len();
                let described = (self.clearing == 0).then(|| {
                    let index = self.push_semantics(rect, semantics.clone());
                    self.parents.push(index);
                    index
                });
                let merges = described.is_some() && semantics.merge_descendants;
                if merges {
                    self.merging += 1;
                    self.merged_text.push(Vec::new());
                }
                if semantics.clear_descendants {
                    self.clearing += 1;
                }
                self.paint(inner, exact);
                if semantics.clear_descendants {
                    self.clearing -= 1;
                }
                let Some(index) = described else {
                    return;
                };
                self.parents.pop();
                if merges {
                    self.merging -= 1;
                    let text = self.merged_text.pop().unwrap_or_default();
                    if self.list.semantics[index].semantics.label.is_none() && !text.is_empty() {
                        self.merged_labels.push(index);
                    }
                    self.add_merged_text(index, text);
                }
                self.list.semantics[index].on_click = self.list.regions[regions..]
                    .iter()
                    .find(|region| region.rect == self.list.semantics[index].rect)
                    .map(|region| Rc::clone(&region.on_click));
            }
            Paint::Press {
                on_press,
                view_configuration,
                inner,
            } => {
                self.list.presses.push(PressRegion {
                    rect: self.visible(rect),
                    origin,
                    on_press: Rc::clone(on_press),
                    view_configuration: *view_configuration,
                });
                self.paint(inner, exact);
            }
            Paint::Button {
                button,
                on_event,
                inner,
            } => {
                self.list.buttons.push(ButtonRegion {
                    rect: self.visible(rect),
                    origin,
                    button: *button,
                    on_event: Rc::clone(on_event),
                });
                self.paint(inner, exact);
            }
            Paint::KeyEvent {
                on_key,
                preview,
                inner,
            } => {
                let index = self.list.key_handlers.len();
                let first = self.list.focusables.len();
                self.list.key_handlers.push(KeyRegion {
                    rect: self.visible(rect),
                    focusables: first..first,
                    raised_focusables: Vec::new(),
                    preview: *preview,
                    on_key: Rc::clone(on_key),
                });
                self.key_scopes.push(index);
                self.paint(inner, exact);
                self.key_scopes.pop();
                self.list.key_handlers[index].focusables.end = self.list.focusables.len();
            }
            Paint::FocusRequester { focus, inner } => {
                let around = self.requester.replace(focus.clone());
                self.paint(inner, exact);
                self.requester = around;
            }
            Paint::Focusable {
                focus,
                on_key,
                takes_press,
                pin,
                inner,
            } => {
                let focus = &self.requester.take().unwrap_or_else(|| focus.clone());
                let on_focus_changed = std::mem::take(&mut self.focus_observers);
                self.list.focusables.push(FocusRegion {
                    rect: self.visible(rect),
                    focus: focus.clone(),
                    on_key: Rc::clone(on_key),
                    takes_press: *takes_press,
                    on_focus_changed,
                    bounds: rect,
                    scrolls: self.scroll_parents.clone(),
                    pin: pin.clone(),
                });
                let around = self.focus.replace(focus.clone());
                self.paint(inner, exact);
                self.focus = around;
            }
            Paint::Clickable {
                on_click,
                feedback,
                hand,
                inner,
            } => {
                let corner_radius = match &inner.paint {
                    Paint::Background { corner_radius, .. } => *corner_radius,
                    _ => 0.0,
                };
                let index = self.list.regions.len();
                let buttons = self.list.buttons.len();
                self.list.regions.push(HitRegion {
                    rect: self.visible(rect),
                    corner_radius,
                    feedback: *feedback,
                    hand: *hand,
                    on_click: Rc::clone(on_click),
                    buttons_inside: buttons..buttons,
                });
                self.paint(inner, exact);
                self.list.regions[index].buttons_inside.end = self.list.buttons.len();
            }
        }
    }
}

/// How large what `measured` holds is: for a layout, which for a popup takes no room, the
/// furthest its children reach; for anything else its own size.
fn content_extent(measured: &Measured) -> Size {
    let mut at = measured;
    loop {
        at = match &at.paint {
            Paint::Shared(shared) => shared,
            Paint::Children(children) => {
                let far = children.iter().fold(measured.size, |far, (offset, child)| {
                    Size::new(
                        far.width.max(offset.x + child.size.width),
                        far.height.max(offset.y + child.size.height),
                    )
                });
                return far;
            }
            _ => return at.size,
        };
    }
}

/// The focus of the first focusable in the chain of modifiers `measured` starts, looking
/// through the wrappers of one node but not into its children.
fn chain_focus(measured: &Measured) -> Option<Focus> {
    let mut at = measured;
    loop {
        at = match &at.paint {
            Paint::Focusable { focus, .. } => return Some(focus.clone()),
            Paint::FocusRequester { focus, inner } => {
                return chain_focus(inner).map(|_| focus.clone());
            }
            Paint::Shared(shared) => shared,
            Paint::Background { inner, .. }
            | Paint::Draw { inner, .. }
            | Paint::Interactive { inner, .. }
            | Paint::InteractionFocused { inner, .. }
            | Paint::InteractionGroup { inner }
            | Paint::KeepInWindow { inner }
            | Paint::InWindow { inner, .. }
            | Paint::Popup { inner, .. }
            | Paint::Fill { inner, .. }
            | Paint::Effect { inner, .. }
            | Paint::Gradient { inner, .. }
            | Paint::Ring { inner, .. }
            | Paint::Cursor { inner, .. }
            | Paint::ImeCaret { inner, .. }
            | Paint::Hover { inner, .. }
            | Paint::Placed { inner, .. }
            | Paint::PlacedVisible { inner, .. }
            | Paint::FocusChanged { inner, .. }
            | Paint::TestTag { inner, .. }
            | Paint::LongPress { inner, .. }
            | Paint::PointerMove { inner, .. }
            | Paint::FileDrop { inner, .. }
            | Paint::DragEnd { inner, .. }
            | Paint::DragSource { inner, .. }
            | Paint::DropTarget { inner, .. }
            | Paint::DragAndDropSource { inner, .. }
            | Paint::DragAndDropTarget { inner, .. }
            | Paint::PointerInput { inner, .. }
            | Paint::PointerEvent { inner, .. }
            | Paint::TextInput { inner, .. }
            | Paint::Pinch { inner, .. }
            | Paint::BlocksPointer(inner)
            | Paint::Alpha { inner, .. }
            | Paint::GraphicsLayer { inner, .. }
            | Paint::RotateWith { inner, .. }
            | Paint::BorderBottom { inner, .. }
            | Paint::BorderSide { inner, .. }
            | Paint::Border { inner, .. }
            | Paint::Shadow { inner, .. }
            | Paint::Clip(inner)
            | Paint::ClipRounded { inner, .. }
            | Paint::Clickable { inner, .. }
            | Paint::Semantics { inner, .. }
            | Paint::Press { inner, .. }
            | Paint::Button { inner, .. }
            | Paint::KeyEvent { inner, .. }
            | Paint::Drag { inner, .. } => inner,
            _ => return None,
        };
    }
}

/// The byte offset in `text` of the character at `index`, or the end of `text` past its last.
fn byte_at(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

#[cfg(test)]
#[path = "paint_tests.rs"]
mod tests;
