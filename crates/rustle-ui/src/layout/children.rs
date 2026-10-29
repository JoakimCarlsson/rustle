use rustle_geometry::Size;
use rustle_text::TextMeasurer;
use rustle_unit::Constraints;

use super::BoxAlignment;
use super::measure::measure_node;
use super::measured::Measured;
use crate::Node;
use crate::modifier::Element;

/// The children of the node being measured.
pub struct Children<'a> {
    pub(super) nodes: &'a [Node],
    pub(super) measured: Vec<Option<Measured>>,
    pub(super) text: &'a mut dyn TextMeasurer,
}

impl Children<'_> {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The `weight` modifier of child `index`, if it has one.
    pub fn weight(&self, index: usize) -> Option<f32> {
        self.nodes[index]
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Weight(weight) => Some(*weight),
                _ => None,
            })
    }

    /// The `align` modifier of child `index`, if it has one.
    pub fn alignment(&self, index: usize) -> Option<BoxAlignment> {
        self.nodes[index]
            .modifier
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Align(alignment) => Some(*alignment),
                _ => None,
            })
    }

    /// Measures child `index` within `constraints` and returns its size.
    /// Measuring a child again replaces the earlier result.
    pub fn measure(&mut self, index: usize, constraints: Constraints) -> Size {
        let measured = measure_node(&self.nodes[index], constraints, self.text);
        let size = measured.size;
        self.measured[index] = Some(measured);
        size
    }
}
