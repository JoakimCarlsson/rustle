use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_runtime::Subcomposition;
use rustle_text::TextMeasurer;
use rustle_unit::Constraints;

use super::intrinsic::{Query, intrinsic_modified};
use super::measure::measure_modified;
use super::measured::Measured;
use super::{Children, Layout, LayoutResult};
use crate::modifier::Element;
use crate::{Modifier, Node};

/// How [`Modifier::width_intrinsic`] and [`Modifier::height_intrinsic`] size an element.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntrinsicSize {
    /// The smallest size at which the content is not cut off, such as the
    /// width of the longest word of a text.
    Min,
    /// The size the content would take if it had all the room it wanted, such
    /// as the width of a text on one line.
    Max,
}

/// Where a custom layout puts what it measured and how large it makes itself.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// The size of the element, clamped into the constraints it was given.
    pub size: Size,
    /// Where the measured content goes, from the element's top-left corner.
    pub offset: Offset,
}

impl Placement {
    /// A placement of `size` that puts the content at `offset`.
    pub fn new(size: Size, offset: Offset) -> Self {
        Self { size, offset }
    }
}

/// The part of an element that comes after a [`Modifier::layout`] in the
/// chain: what the custom layout measures and places.
pub struct Measurable<'a> {
    pub(super) rest: &'a [Element],
    pub(super) node: &'a Node,
    pub(super) text: &'a mut dyn TextMeasurer,
    pub(super) measured: Option<Measured>,
}

impl Measurable<'_> {
    /// Measures the content within `constraints` and returns its size.
    /// Measuring again replaces the earlier result.
    pub fn measure(&mut self, constraints: Constraints) -> Size {
        let measured = measure_modified(self.rest, self.node, constraints, self.text);
        let size = measured.size;
        self.measured = Some(measured);
        size
    }

    /// The smallest width the content can take without being cut off, when it
    /// is `height` tall (`f32::INFINITY` for any height).
    pub fn min_intrinsic_width(&mut self, height: f32) -> f32 {
        self.intrinsic(Query::MinWidth, height)
    }

    /// The width the content would take with all the room it wanted, when it
    /// is `height` tall.
    pub fn max_intrinsic_width(&mut self, height: f32) -> f32 {
        self.intrinsic(Query::MaxWidth, height)
    }

    /// The smallest height the content can take without being cut off, when
    /// it is `width` wide (`f32::INFINITY` for any width).
    pub fn min_intrinsic_height(&mut self, width: f32) -> f32 {
        self.intrinsic(Query::MinHeight, width)
    }

    /// The height the content would take with all the room it wanted, when it
    /// is `width` wide.
    pub fn max_intrinsic_height(&mut self, width: f32) -> f32 {
        self.intrinsic(Query::MaxHeight, width)
    }

    /// Answers `query` for the content without measuring it.
    fn intrinsic(&mut self, query: Query, extent: f32) -> f32 {
        intrinsic_modified(self.rest, self.node, query, extent, self.text)
    }
}

/// The scope a [`subcompose_layout`] measures in: it composes children while
/// measuring, so what they are can depend on the constraints.
pub struct SubcomposeScope<'a, 'b> {
    children: &'a mut Children<'b>,
    host: &'a SubcomposeHost,
}

impl SubcomposeScope<'_, '_> {
    /// Composes `content` and returns the indices of the nodes it emitted.
    ///
    /// What `content` remembers belongs to `slot` and is kept between
    /// measurements, so give each distinct piece of content its own slot. A
    /// slot that is not composed during a measurement is disposed of, which
    /// runs its cleanups.
    pub fn subcompose(&mut self, slot: impl Hash, content: impl FnOnce()) -> Vec<usize> {
        let mut hasher = DefaultHasher::new();
        slot.hash(&mut hasher);
        let key = hasher.finish();
        let composition = self.host.slot(key);
        let nodes = composition.compose(|| crate::collect(content));
        self.children.append(nodes)
    }

    /// Measures child `index`, as returned by [`SubcomposeScope::subcompose`],
    /// within `constraints` and returns its size.
    pub fn measure(&mut self, index: usize, constraints: Constraints) -> Size {
        self.children.measure(index, constraints)
    }

    /// The smallest width child `index` can take without being cut off, when it is `height` tall.
    pub fn min_intrinsic_width(&mut self, index: usize, height: f32) -> f32 {
        self.children.min_intrinsic_width(index, height)
    }

    /// The width child `index` would take with all the room it wanted, when it is `height` tall.
    pub fn max_intrinsic_width(&mut self, index: usize, height: f32) -> f32 {
        self.children.max_intrinsic_width(index, height)
    }

    /// The smallest height child `index` can take without being cut off, when it is `width` wide.
    pub fn min_intrinsic_height(&mut self, index: usize, width: f32) -> f32 {
        self.children.min_intrinsic_height(index, width)
    }

    /// The height child `index` would take with all the room it wanted, when it is `width` wide.
    pub fn max_intrinsic_height(&mut self, index: usize, width: f32) -> f32 {
        self.children.max_intrinsic_height(index, width)
    }
}

/// What a [`subcompose_layout`] decided: its size and where the children it
/// measured go. Children left out are neither placed nor painted.
pub struct SubcomposeResult {
    pub size: Size,
    /// A child index, as returned by [`SubcomposeScope::subcompose`], and its
    /// position from the layout's top-left corner. Later entries paint over earlier ones.
    pub placements: Vec<(usize, Offset)>,
}

/// The compositions of the slots of one [`subcompose_layout`], kept between passes.
struct SubcomposeHost {
    template: Subcomposition,
    slots: RefCell<HashMap<u64, Rc<Subcomposition>>>,
    used: RefCell<HashSet<u64>>,
}

impl SubcomposeHost {
    /// Creates a host tied to the composition being composed.
    fn new() -> Self {
        Self {
            template: Subcomposition::new(),
            slots: RefCell::default(),
            used: RefCell::default(),
        }
    }

    /// Refreshes the composition locals every slot sees.
    fn capture(&self) {
        self.template.capture();
        for slot in self.slots.borrow().values() {
            slot.capture();
        }
    }

    /// The composition of the slot `key`, created when new, and marks it used.
    fn slot(&self, key: u64) -> Rc<Subcomposition> {
        self.used.borrow_mut().insert(key);
        Rc::clone(
            self.slots
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::new(self.template.fork())),
        )
    }

    /// Forgets the slots not used by the measurement that just ran.
    fn prune(&self) {
        let used = self.used.borrow();
        let removed: Vec<_> = {
            let mut slots = self.slots.borrow_mut();
            let stale: Vec<u64> = slots
                .keys()
                .copied()
                .filter(|key| !used.contains(key))
                .collect();
            stale.iter().filter_map(|key| slots.remove(key)).collect()
        };
        drop(used);
        drop(removed);
    }
}

/// The measure closure of a [`subcompose_layout`].
type SubcomposeMeasure = dyn Fn(&mut SubcomposeScope, Constraints) -> SubcomposeResult;

/// A layout policy whose measure composes its children.
struct SubcomposeLayout {
    host: Rc<SubcomposeHost>,
    measure: Rc<SubcomposeMeasure>,
}

impl Layout for SubcomposeLayout {
    /// Runs the measure closure, then drops the slots it did not compose.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        self.host.used.borrow_mut().clear();
        let result = {
            let mut scope = SubcomposeScope {
                children,
                host: &self.host,
            };
            (self.measure)(&mut scope, constraints)
        };
        self.host.prune();
        let mut positions = vec![Offset::default(); children.len()];
        for (index, offset) in result.placements {
            positions[index] = offset;
        }
        LayoutResult {
            size: result.size,
            positions,
        }
    }
}

/// Emits an element whose children are composed while it measures, so which
/// children exist can depend on the constraints it is given or on what other
/// children measured to. This is Compose's `SubcomposeLayout`.
///
/// `measure` composes content with [`SubcomposeScope::subcompose`], measures
/// the nodes it got and returns their places in a [`SubcomposeResult`].
#[track_caller]
pub fn subcompose_layout(
    modifier: Modifier,
    measure: impl Fn(&mut SubcomposeScope, Constraints) -> SubcomposeResult + 'static,
) {
    let host = rustle_runtime::remember(|| Rc::new(SubcomposeHost::new())).get();
    host.capture();
    crate::layout(
        modifier,
        SubcomposeLayout {
            host,
            measure: Rc::new(measure),
        },
        || {},
    );
}
