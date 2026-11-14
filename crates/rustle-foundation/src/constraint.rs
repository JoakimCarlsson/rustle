use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_ui::{Children, Layout, LayoutResult, Modifier, layout};
use rustle_unit::Constraints;

/// A child, guideline or barrier of a [`ConstraintSet`], to anchor others to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ref(usize);

/// How a child is sized along one axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Dimension {
    /// As large as its content wants.
    Wrap,
    /// Exactly this many logical pixels.
    Fixed(f32),
    /// All the space between its start and end anchors. With fewer than two
    /// anchors on the axis it is as large as its content.
    Fill,
}

/// Where a guideline lies along its axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GuidePosition {
    /// This far from the start (left or top) of the layout.
    Begin(f32),
    /// This far from the end (right or bottom) of the layout.
    End(f32),
    /// This fraction (0 to 1) of the way along the layout.
    Fraction(f32),
}

/// Which edge of the barrier's members it sits on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarrierDirection {
    /// The left-most left edge.
    Start,
    /// The right-most right edge.
    End,
    /// The top-most top edge.
    Top,
    /// The bottom-most bottom edge.
    Bottom,
}

/// How a chain spreads its members between its outer anchors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainStyle {
    /// Equal gaps between members and at both ends.
    Spread,
    /// The first member at the start, the last at the end, equal gaps between.
    SpreadInside,
    /// Members side by side, the group placed by the first member's bias.
    Packed,
}

/// The two axes a layout is solved on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

/// What an anchor is attached to.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Parent,
    Item(usize),
}

/// A point on another element that an edge of a child is tied to, with a margin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    target: Target,
    to_end: bool,
    margin: f32,
}

impl Anchor {
    /// The start (left or top) edge of the layout.
    pub const PARENT_START: Anchor = Anchor {
        target: Target::Parent,
        to_end: false,
        margin: 0.0,
    };

    /// The end (right or bottom) edge of the layout.
    pub const PARENT_END: Anchor = Anchor {
        target: Target::Parent,
        to_end: true,
        margin: 0.0,
    };

    /// The start (left or top) edge of `target`, or the position of a guideline or barrier.
    pub fn start_of(target: Ref) -> Self {
        Self {
            target: Target::Item(target.0),
            to_end: false,
            margin: 0.0,
        }
    }

    /// The end (right or bottom) edge of `target`, or the position of a guideline or barrier.
    pub fn end_of(target: Ref) -> Self {
        Self {
            target: Target::Item(target.0),
            to_end: true,
            margin: 0.0,
        }
    }

    /// Keeps `margin` logical pixels between the edge and the anchor.
    pub fn margin(mut self, margin: f32) -> Self {
        self.margin = margin;
        self
    }
}

/// How a child is constrained along one axis.
#[derive(Debug, Clone, Copy)]
struct AxisSpec {
    dimension: Dimension,
    start: Option<Anchor>,
    end: Option<Anchor>,
    bias: f32,
    weight: f32,
}

impl Default for AxisSpec {
    fn default() -> Self {
        Self {
            dimension: Dimension::Wrap,
            start: None,
            end: None,
            bias: 0.5,
            weight: 1.0,
        }
    }
}

/// The constraints of one child of a [`constraint_layout`].
#[derive(Debug, Clone, Copy, Default)]
pub struct ChildConstraints {
    horizontal: AxisSpec,
    vertical: AxisSpec,
}

impl ChildConstraints {
    /// Ties the child's start (left) edge to `anchor`.
    pub fn link_start(&mut self, anchor: Anchor) -> &mut Self {
        self.horizontal.start = Some(anchor);
        self
    }

    /// Ties the child's end (right) edge to `anchor`.
    pub fn link_end(&mut self, anchor: Anchor) -> &mut Self {
        self.horizontal.end = Some(anchor);
        self
    }

    /// Ties the child's top edge to `anchor`.
    pub fn link_top(&mut self, anchor: Anchor) -> &mut Self {
        self.vertical.start = Some(anchor);
        self
    }

    /// Ties the child's bottom edge to `anchor`.
    pub fn link_bottom(&mut self, anchor: Anchor) -> &mut Self {
        self.vertical.end = Some(anchor);
        self
    }

    /// Sets how the child is sized horizontally.
    pub fn width(&mut self, dimension: Dimension) -> &mut Self {
        self.horizontal.dimension = dimension;
        self
    }

    /// Sets how the child is sized vertically.
    pub fn height(&mut self, dimension: Dimension) -> &mut Self {
        self.vertical.dimension = dimension;
        self
    }

    /// Where the child sits between two horizontal anchors, from 0 (at the
    /// start) to 1 (at the end). The default is 0.5.
    pub fn horizontal_bias(&mut self, bias: f32) -> &mut Self {
        self.horizontal.bias = bias;
        self
    }

    /// Where the child sits between two vertical anchors, from 0 (at the top)
    /// to 1 (at the bottom). The default is 0.5.
    pub fn vertical_bias(&mut self, bias: f32) -> &mut Self {
        self.vertical.bias = bias;
        self
    }

    /// The share of the chain's free space a [`Dimension::Fill`] child gets
    /// horizontally, in proportion to the other members' weights. The default is 1.
    pub fn horizontal_weight(&mut self, weight: f32) -> &mut Self {
        self.horizontal.weight = weight;
        self
    }

    /// Like [`ChildConstraints::horizontal_weight`], vertically.
    pub fn vertical_weight(&mut self, weight: f32) -> &mut Self {
        self.vertical.weight = weight;
        self
    }

    /// The constraints along `axis`.
    fn spec(&self, axis: Axis) -> &AxisSpec {
        match axis {
            Axis::Horizontal => &self.horizontal,
            Axis::Vertical => &self.vertical,
        }
    }
}

/// What a [`ConstraintSet`] holds.
#[derive(Debug, Clone)]
enum Item {
    Child(ChildConstraints),
    Guideline {
        axis: Axis,
        position: GuidePosition,
    },
    Barrier {
        direction: BarrierDirection,
        members: Vec<usize>,
    },
}

/// A run of children tied to each other along one axis.
#[derive(Debug, Clone)]
struct Chain {
    axis: Axis,
    members: Vec<usize>,
    style: ChainStyle,
}

/// The constraints of a [`constraint_layout`]: its children, guidelines,
/// barriers and chains, and how they are tied together.
///
/// Children are declared with [`ConstraintSet::child`] in the order the
/// layout's content emits them, and are then anchored to the layout, to a
/// guideline or barrier, or to each other. Everything is solved one axis at a
/// time, so horizontal constraints never affect vertical ones, except that a
/// child's height is measured at the width it ends up with.
#[derive(Debug, Clone, Default)]
pub struct ConstraintSet {
    items: Vec<Item>,
    child_items: Vec<usize>,
    chains: Vec<Chain>,
}

impl ConstraintSet {
    /// An empty set.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares the next child of the layout's content and returns its reference.
    pub fn child(&mut self) -> Ref {
        self.items.push(Item::Child(ChildConstraints::default()));
        self.child_items.push(self.items.len() - 1);
        Ref(self.items.len() - 1)
    }

    /// Declares a vertical line children can be anchored to.
    pub fn vertical_guideline(&mut self, position: GuidePosition) -> Ref {
        self.items.push(Item::Guideline {
            axis: Axis::Horizontal,
            position,
        });
        Ref(self.items.len() - 1)
    }

    /// Declares a horizontal line children can be anchored to.
    pub fn horizontal_guideline(&mut self, position: GuidePosition) -> Ref {
        self.items.push(Item::Guideline {
            axis: Axis::Vertical,
            position,
        });
        Ref(self.items.len() - 1)
    }

    /// Declares a line at the `direction` edge of all of `members`, which
    /// moves as they do, for anchoring children that must clear all of them.
    pub fn barrier(&mut self, direction: BarrierDirection, members: &[Ref]) -> Ref {
        self.items.push(Item::Barrier {
            direction,
            members: members.iter().map(|member| member.0).collect(),
        });
        Ref(self.items.len() - 1)
    }

    /// Ties `members` into a horizontal chain: each sits between its
    /// neighbours, and the first's start anchor and the last's end anchor
    /// bound the whole chain. Other anchors of the members along the axis are ignored.
    pub fn horizontal_chain(&mut self, members: &[Ref], style: ChainStyle) {
        self.add_chain(Axis::Horizontal, members, style);
    }

    /// Ties `members` into a vertical chain, from the first's top anchor to
    /// the last's bottom anchor; see [`ConstraintSet::horizontal_chain`].
    pub fn vertical_chain(&mut self, members: &[Ref], style: ChainStyle) {
        self.add_chain(Axis::Vertical, members, style);
    }

    /// The constraints of `child`, to change.
    ///
    /// # Panics
    ///
    /// Panics if `child` is not a child declared with [`ConstraintSet::child`].
    pub fn constrain(&mut self, child: Ref) -> &mut ChildConstraints {
        match &mut self.items[child.0] {
            Item::Child(constraints) => constraints,
            _ => panic!("only children can be constrained"),
        }
    }

    /// Records a chain of `members` along `axis`.
    fn add_chain(&mut self, axis: Axis, members: &[Ref], style: ChainStyle) {
        if !members.is_empty() {
            self.chains.push(Chain {
                axis,
                members: members.iter().map(|member| member.0).collect(),
                style,
            });
        }
    }
}

/// Emits a layout that places the children of `content` by `set`, anchored to
/// the layout, to guidelines, barriers and each other, with chains.
///
/// The layout wraps what it holds, within its constraints; give it
/// `fill_max_size` or a size for anchors to its end edges to mean something.
/// Content beyond the children the set declares is placed at the top-left.
/// Anchors do not mirror in right-to-left layouts.
#[track_caller]
pub fn constraint_layout(modifier: Modifier, set: ConstraintSet, content: impl FnOnce()) {
    layout(modifier, ConstraintLayout { set: Rc::new(set) }, content);
}

/// Solves a [`ConstraintSet`] for the children it is given.
struct ConstraintLayout {
    set: Rc<ConstraintSet>,
}

/// Resolved `(start, length)` of every item along one axis.
type Solved = Vec<(f32, f32)>;

impl ConstraintLayout {
    /// The position an anchor gives to an edge, once its target is resolved.
    fn anchor_position(
        anchor: &Anchor,
        resolved: &[Option<(f32, f32)>],
        parent: f32,
        is_start: bool,
    ) -> Option<f32> {
        let base = match anchor.target {
            Target::Parent => {
                if anchor.to_end {
                    parent
                } else {
                    0.0
                }
            }
            Target::Item(item) => {
                let (start, length) = resolved[item]?;
                if anchor.to_end { start + length } else { start }
            }
        };
        Some(if is_start {
            base + anchor.margin
        } else {
            base - anchor.margin
        })
    }

    /// Resolves the start and length of every item along `axis` inside a
    /// layout `parent` long, given the lengths `natural` the children measured to.
    fn solve(&self, axis: Axis, parent: f32, natural: &[f32]) -> Solved {
        let set = &*self.set;
        let count = set.items.len();
        let mut resolved: Vec<Option<(f32, f32)>> = vec![None; count];
        let mut chained = vec![false; count];
        for chain in set.chains.iter().filter(|chain| chain.axis == axis) {
            for member in &chain.members {
                chained[*member] = true;
            }
        }
        for _ in 0..=count + set.chains.len() {
            let mut progressed = false;
            for index in 0..count {
                if resolved[index].is_some() {
                    continue;
                }
                let value = match &set.items[index] {
                    Item::Guideline {
                        axis: guide_axis,
                        position,
                    } if *guide_axis == axis => Some((
                        match position {
                            GuidePosition::Begin(distance) => *distance,
                            GuidePosition::End(distance) => parent - distance,
                            GuidePosition::Fraction(fraction) => parent * fraction,
                        },
                        0.0,
                    )),
                    Item::Barrier { direction, members }
                        if Self::barrier_axis(*direction) == axis =>
                    {
                        let edges: Option<Vec<(f32, f32)>> =
                            members.iter().map(|member| resolved[*member]).collect();
                        edges.map(|edges| {
                            let position = match direction {
                                BarrierDirection::Start | BarrierDirection::Top => edges
                                    .iter()
                                    .fold(f32::INFINITY, |least, (start, _)| least.min(*start)),
                                BarrierDirection::End | BarrierDirection::Bottom => edges
                                    .iter()
                                    .fold(f32::NEG_INFINITY, |most, (start, length)| {
                                        most.max(start + length)
                                    }),
                            };
                            let position = if position.is_finite() { position } else { 0.0 };
                            (position, 0.0)
                        })
                    }
                    Item::Child(child) if !chained[index] => {
                        Self::resolve_child(child.spec(axis), natural[index], &resolved, parent)
                    }
                    _ => None,
                };
                if value.is_some() {
                    resolved[index] = value;
                    progressed = true;
                }
            }
            for chain in set.chains.iter().filter(|chain| chain.axis == axis) {
                if resolved[chain.members[0]].is_some() {
                    continue;
                }
                if let Some(values) = self.resolve_chain(chain, natural, &resolved, parent) {
                    for (member, value) in chain.members.iter().zip(values) {
                        resolved[*member] = Some(value);
                    }
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }
        resolved
            .into_iter()
            .enumerate()
            .map(|(index, value)| value.unwrap_or((0.0, natural[index])))
            .collect()
    }

    /// The axis a barrier direction works along.
    fn barrier_axis(direction: BarrierDirection) -> Axis {
        match direction {
            BarrierDirection::Start | BarrierDirection::End => Axis::Horizontal,
            BarrierDirection::Top | BarrierDirection::Bottom => Axis::Vertical,
        }
    }

    /// The outer anchors of `spec`, or `None` while a target is unresolved.
    fn edges(
        spec: &AxisSpec,
        resolved: &[Option<(f32, f32)>],
        parent: f32,
    ) -> Option<(Option<f32>, Option<f32>)> {
        let start = match &spec.start {
            Some(anchor) => Some(Self::anchor_position(anchor, resolved, parent, true)?),
            None => None,
        };
        let end = match &spec.end {
            Some(anchor) => Some(Self::anchor_position(anchor, resolved, parent, false)?),
            None => None,
        };
        Some((start, end))
    }

    /// Places a child that is not in a chain, once its anchors are resolved.
    fn resolve_child(
        spec: &AxisSpec,
        natural: f32,
        resolved: &[Option<(f32, f32)>],
        parent: f32,
    ) -> Option<(f32, f32)> {
        let (start, end) = Self::edges(spec, resolved, parent)?;
        let length = match (spec.dimension, start, end) {
            (Dimension::Fill, Some(start), Some(end)) => (end - start).max(0.0),
            _ => natural,
        };
        let position = match (start, end) {
            (Some(start), Some(end)) => start + (end - start - length) * spec.bias,
            (Some(start), None) => start,
            (None, Some(end)) => end - length,
            (None, None) => 0.0,
        };
        Some((position, length))
    }

    /// Places the members of `chain` between its outer anchors, once they are resolved.
    fn resolve_chain(
        &self,
        chain: &Chain,
        natural: &[f32],
        resolved: &[Option<(f32, f32)>],
        parent: f32,
    ) -> Option<Vec<(f32, f32)>> {
        let specs: Vec<&AxisSpec> = chain
            .members
            .iter()
            .map(|member| match &self.set.items[*member] {
                Item::Child(child) => child.spec(chain.axis),
                _ => unreachable!("chains hold children"),
            })
            .collect();
        let start = match &specs[0].start {
            Some(anchor) => Some(Self::anchor_position(anchor, resolved, parent, true)?),
            None => None,
        };
        let end = match &specs[specs.len() - 1].end {
            Some(anchor) => Some(Self::anchor_position(anchor, resolved, parent, false)?),
            None => None,
        };
        let fills = |index: usize| matches!(specs[index].dimension, Dimension::Fill);
        let fixed: f32 = (0..specs.len())
            .filter(|index| !fills(*index))
            .map(|index| natural[chain.members[index]])
            .sum();
        let span = match (start, end) {
            (Some(start), Some(end)) => Some(end - start),
            _ => None,
        };
        let total_weight: f32 = (0..specs.len())
            .filter(|index| fills(*index))
            .map(|index| specs[index].weight.max(0.0))
            .sum();
        let lengths: Vec<f32> = (0..specs.len())
            .map(|index| match span {
                Some(span) if fills(index) && total_weight > 0.0 => {
                    (span - fixed).max(0.0) * specs[index].weight.max(0.0) / total_weight
                }
                _ => natural[chain.members[index]],
            })
            .collect();
        let total: f32 = lengths.iter().sum();
        let free = span.map_or(0.0, |span| (span - total).max(0.0));
        let origin = match (start, end) {
            (Some(start), _) => start,
            (None, Some(end)) => end - total,
            (None, None) => 0.0,
        };
        let count = lengths.len();
        let (first, gap) = match chain.style {
            ChainStyle::Spread => (free / (count + 1) as f32, free / (count + 1) as f32),
            ChainStyle::SpreadInside if count > 1 => (0.0, free / (count - 1) as f32),
            ChainStyle::SpreadInside => (free * specs[0].bias, 0.0),
            ChainStyle::Packed => (free * specs[0].bias, 0.0),
        };
        let mut position = origin + first;
        Some(
            lengths
                .iter()
                .map(|length| {
                    let placed = (position, *length);
                    position += length + gap;
                    placed
                })
                .collect(),
        )
    }

    /// Finds the length of a layout that wraps its children along `axis`, then
    /// solves in it. Where children end depends on the layout's length when
    /// they are anchored to its far edge or centered, so the length is
    /// searched for: the smallest one, no shorter than `range.0`, that holds
    /// every child and the margin it keeps from the far edge. Returns the solved items and the layout's length, kept
    /// within `range`.
    fn solve_wrapping(&self, axis: Axis, range: (f32, f32), natural: &[f32]) -> (Solved, f32) {
        let extent_at = |length: f32| {
            let solved = self.solve(axis, length, natural);
            self.set.child_items.iter().fold(0.0_f32, |extent, item| {
                let margin = match &self.set.items[*item] {
                    Item::Child(child) => match &child.spec(axis).end {
                        Some(Anchor {
                            target: Target::Parent,
                            to_end: true,
                            margin,
                        }) => *margin,
                        _ => 0.0,
                    },
                    _ => 0.0,
                };
                extent.max(solved[*item].0 + solved[*item].1 + margin)
            })
        };
        let mut length = range.0.max(0.0);
        for _ in 0..8 {
            let reached = extent_at(length);
            if reached <= length + 0.001 || !reached.is_finite() {
                break;
            }
            let next = extent_at(reached);
            let slope = (next - reached) / (reached - length);
            length = if slope < 0.999 {
                (reached - slope * length) / (1.0 - slope)
            } else {
                reached
            };
        }
        let length = length.max(range.0).min(range.1.max(range.0));
        (self.solve(axis, length, natural), length)
    }

    /// The constraints to measure child `item` at its own size.
    fn natural_constraints(
        &self,
        child: &ChildConstraints,
        constraints: Constraints,
    ) -> Constraints {
        let bounds = |spec: &AxisSpec, max: f32| match spec.dimension {
            Dimension::Fixed(length) => (length.min(max), length.min(max)),
            _ => (0.0, max),
        };
        let (min_width, max_width) = bounds(&child.horizontal, constraints.max_width);
        let (min_height, max_height) = bounds(&child.vertical, constraints.max_height);
        Constraints {
            min_width,
            max_width,
            min_height,
            max_height,
        }
    }
}

impl Layout for ConstraintLayout {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let set = &*self.set;
        let declared = set.child_items.len().min(children.len());
        let mut natural = vec![Size::default(); set.items.len()];
        for (index, item) in set.child_items.iter().take(declared).enumerate() {
            let Item::Child(child) = &set.items[*item] else {
                continue;
            };
            let at = self.natural_constraints(child, constraints);
            natural[*item] = children.measure(index, at);
        }
        let widths: Vec<f32> = natural.iter().map(|size| size.width).collect();
        let (horizontal, width) = self.solve_wrapping(
            Axis::Horizontal,
            (constraints.min_width, constraints.max_width),
            &widths,
        );
        for (index, item) in set.child_items.iter().take(declared).enumerate() {
            let Item::Child(child) = &set.items[*item] else {
                continue;
            };
            if child.horizontal.dimension == Dimension::Fill {
                let at = self.natural_constraints(child, constraints);
                let length = horizontal[*item].1;
                natural[*item] = children.measure(
                    index,
                    Constraints {
                        min_width: length,
                        max_width: length,
                        ..at
                    },
                );
            }
        }
        let heights: Vec<f32> = natural.iter().map(|size| size.height).collect();
        let (vertical, height) = self.solve_wrapping(
            Axis::Vertical,
            (constraints.min_height, constraints.max_height),
            &heights,
        );
        let mut positions = vec![Offset::default(); children.len()];
        for (index, item) in set.child_items.iter().take(declared).enumerate() {
            let Item::Child(child) = &set.items[*item] else {
                continue;
            };
            let fills = child.horizontal.dimension == Dimension::Fill
                || child.vertical.dimension == Dimension::Fill;
            if fills {
                let size = Size::new(horizontal[*item].1, vertical[*item].1);
                children.measure(index, Constraints::tight(size));
            }
            positions[index] = Offset::new(horizontal[*item].0, vertical[*item].0);
        }
        for index in declared..children.len() {
            children.measure(
                index,
                Constraints {
                    min_width: 0.0,
                    min_height: 0.0,
                    ..constraints
                },
            );
        }
        LayoutResult {
            size: Size::new(width, height),
            positions,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use rustle_geometry::Rect;
    use rustle_text::{StyledSpan, TextLayout, TextMeasurer, TextStyle};
    use rustle_ui::{composition, display_list};

    use super::*;

    /// A measurer for a tree without text.
    struct NoText;

    impl TextMeasurer for NoText {
        /// Text does not occur in these tests.
        fn measure(&mut self, _: &str, _: &TextStyle, _: f32) -> Size {
            Size::default()
        }

        /// Text does not occur in these tests.
        fn measure_spans(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> Size {
            Size::default()
        }

        /// Text does not occur in these tests.
        fn layout(&mut self, _: &[StyledSpan], _: &TextStyle, _: f32) -> TextLayout {
            unreachable!("no text")
        }
    }

    /// Lays out `set` over children of the given sizes in a 200 by 100 area and
    /// returns where each was placed.
    fn solve(set: ConstraintSet, sizes: &[(f32, f32)]) -> Vec<Rect> {
        let rects = Rc::new(RefCell::new(vec![Rect::default(); sizes.len()]));
        let sizes = sizes.to_vec();
        let out = Rc::clone(&rects);
        let mut composition = composition(move || {
            let set = set.clone();
            let sizes = sizes.clone();
            let out = Rc::clone(&out);
            constraint_layout(Modifier::new().size(200.0, 100.0), set, move || {
                for (index, (width, height)) in sizes.iter().enumerate() {
                    let out = Rc::clone(&out);
                    rustle_ui::canvas(
                        Modifier::new()
                            .size(*width, *height)
                            .on_globally_positioned(move |rect| out.borrow_mut()[index] = rect),
                        |_| {},
                    );
                }
            });
        });
        composition.compose();
        let root = composition.output().expect("composed");
        let window = Constraints::loose(Size::new(200.0, 100.0));
        display_list(root, window, &mut NoText);
        rects.take()
    }

    /// Anchors with margins, a fill child and bias place children where they should be.
    #[test]
    fn anchors_margins_and_fill() {
        let mut set = ConstraintSet::new();
        let first = set.child();
        let second = set.child();
        set.constrain(first)
            .link_start(Anchor::PARENT_START.margin(10.0))
            .link_top(Anchor::PARENT_START.margin(5.0));
        set.constrain(second)
            .link_start(Anchor::end_of(first).margin(10.0))
            .link_end(Anchor::PARENT_END.margin(20.0))
            .link_bottom(Anchor::PARENT_END)
            .width(Dimension::Fill);
        let rects = solve(set, &[(50.0, 20.0), (1.0, 30.0)]);
        assert_eq!((rects[0].origin.x, rects[0].origin.y), (10.0, 5.0));
        assert_eq!((rects[1].origin.x, rects[1].size.width), (70.0, 110.0));
        assert_eq!(rects[1].origin.y, 70.0);
    }

    /// A spread-inside chain puts the first member at the start and the last at the end.
    #[test]
    fn chain_spreads_inside() {
        let mut set = ConstraintSet::new();
        let members = [set.child(), set.child(), set.child()];
        set.constrain(members[0]).link_start(Anchor::PARENT_START);
        set.constrain(members[2]).link_end(Anchor::PARENT_END);
        set.horizontal_chain(&members, ChainStyle::SpreadInside);
        let rects = solve(set, &[(20.0, 10.0), (20.0, 10.0), (20.0, 10.0)]);
        assert_eq!(rects[0].origin.x, 0.0);
        assert_eq!(rects[1].origin.x, 90.0);
        assert_eq!(rects[2].origin.x, 180.0);
    }

    /// A barrier clears its tallest member and a guideline sits at its fraction.
    #[test]
    fn barrier_and_guideline() {
        let mut set = ConstraintSet::new();
        let tall = set.child();
        let short = set.child();
        let below = set.child();
        let barrier = set.barrier(BarrierDirection::Bottom, &[tall, short]);
        let guide = set.vertical_guideline(GuidePosition::Fraction(0.5));
        set.constrain(below)
            .link_top(Anchor::end_of(barrier))
            .link_start(Anchor::start_of(guide));
        let rects = solve(set, &[(10.0, 40.0), (10.0, 25.0), (10.0, 10.0)]);
        assert_eq!(rects[2].origin.y, 40.0);
        assert_eq!(rects[2].origin.x, 100.0);
    }
}
