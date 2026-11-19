use rustle_geometry::Size;
use rustle_text::TextMeasurer;
use rustle_unit::Constraints;

use super::BoxAlignment;
use super::intrinsic::{Query, intrinsic_node};
use super::measure::measure_node;
use super::measured::{Measured, Paint};
use crate::Node;
use crate::modifier::Element;

/// The children of the node being measured.
pub struct Children<'a> {
    pub(super) nodes: &'a [Node],
    pub(super) extra: Vec<Node>,
    pub(super) measured: Vec<Option<Measured>>,
    pub(super) text: &'a mut dyn TextMeasurer,
}

impl Children<'_> {
    /// The number of children, counting those added by [`Children::append`].
    pub fn len(&self) -> usize {
        self.nodes.len() + self.extra.len()
    }

    /// Whether there are no children.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Child `index`, whether it was emitted with the node or appended while measuring.
    fn node(&self, index: usize) -> &Node {
        match self.nodes.get(index) {
            Some(node) => node,
            None => &self.extra[index - self.nodes.len()],
        }
    }

    /// Adds `nodes`, composed while measuring, after the existing children and
    /// returns their indices. A child that is never measured is not painted
    /// and takes no part in the layout's result.
    pub fn append(&mut self, nodes: Vec<Node>) -> Vec<usize> {
        let first = self.len();
        self.measured.extend(nodes.iter().map(|_| None));
        self.extra.extend(nodes);
        (first..self.len()).collect()
    }

    /// Leaves child `index` out: it takes no space and is not painted, and
    /// receives no input. For children that do not fit, such as the lines of a
    /// flow beyond its limit. A layout must measure or omit every child.
    pub fn omit(&mut self, index: usize) {
        self.measured[index] = Some(Measured {
            size: Size::default(),
            paint: Paint::Children(Vec::new()),
        });
    }

    /// The `weight` modifier of child `index`, if it has one.
    pub fn weight(&self, index: usize) -> Option<f32> {
        self.node(index)
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Weight { weight, .. } => Some(*weight),
                _ => None,
            })
    }

    /// Whether child `index` is made exactly as long as its weighted share, as Compose's
    /// `weight(weight, fill = true)` is, rather than at most that long; true without a weight.
    pub fn weight_fills(&self, index: usize) -> bool {
        self.node(index)
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Weight { fill, .. } => Some(*fill),
                _ => None,
            })
            .unwrap_or(true)
    }

    /// The `align` modifier of child `index`, if it has one.
    pub fn alignment(&self, index: usize) -> Option<BoxAlignment> {
        self.node(index)
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Align(alignment) => Some(*alignment),
                _ => None,
            })
    }

    /// Whether child `index` is sized to its stack with
    /// [`Modifier::match_parent_size`](crate::Modifier::match_parent_size).
    pub fn matches_parent_size(&self, index: usize) -> bool {
        self.node(index)
            .modifier
            .elements
            .iter()
            .any(|element| matches!(element, Element::MatchParentSize))
    }

    /// Measures child `index` within `constraints` and returns its size.
    /// Measuring a child again replaces the earlier result.
    pub fn measure(&mut self, index: usize, constraints: Constraints) -> Size {
        let node = match self.nodes.get(index) {
            Some(node) => node,
            None => &self.extra[index - self.nodes.len()],
        };
        let measured = measure_node(node, constraints, self.text);
        let size = measured.size;
        self.measured[index] = Some(measured);
        size
    }
}

impl Children<'_> {
    /// The smallest width child `index` can take without its content being
    /// cut off, when it is `height` tall (`f32::INFINITY` for any height).
    pub fn min_intrinsic_width(&mut self, index: usize, height: f32) -> f32 {
        self.intrinsic(index, Query::MinWidth, height)
    }

    /// The width child `index` would take if it had all the room it wanted,
    /// when it is `height` tall.
    pub fn max_intrinsic_width(&mut self, index: usize, height: f32) -> f32 {
        self.intrinsic(index, Query::MaxWidth, height)
    }

    /// The smallest height child `index` can take without its content being
    /// cut off, when it is `width` wide (`f32::INFINITY` for any width).
    pub fn min_intrinsic_height(&mut self, index: usize, width: f32) -> f32 {
        self.intrinsic(index, Query::MinHeight, width)
    }

    /// The height child `index` would take if it had all the room it wanted,
    /// when it is `width` wide.
    pub fn max_intrinsic_height(&mut self, index: usize, width: f32) -> f32 {
        self.intrinsic(index, Query::MaxHeight, width)
    }

    /// Answers `query` for child `index` without measuring or keeping anything.
    fn intrinsic(&mut self, index: usize, query: Query, extent: f32) -> f32 {
        let node = match self.nodes.get(index) {
            Some(node) => node,
            None => &self.extra[index - self.nodes.len()],
        };
        intrinsic_node(node, query, extent, self.text)
    }
}
