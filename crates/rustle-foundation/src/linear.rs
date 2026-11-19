use rustle_geometry::{Offset, Size};
use rustle_ui::{BoxAlignment, Children, Layout, LayoutResult, Modifier, Overlay, is_rtl, layout};

use rustle_unit::Constraints;

/// Emits a column that places the children of `content` top to bottom.
#[track_caller]
pub fn column(content: impl FnOnce()) {
    column_with(Modifier::new(), content);
}

/// Emits a column with `modifier`.
#[track_caller]
pub fn column_with(modifier: Modifier, content: impl FnOnce()) {
    layout(
        modifier,
        Linear::new(true, Arrangement::Start, Alignment::Start),
        content,
    );
}

/// Emits a row that places the children of `content` left to right.
#[track_caller]
pub fn row(content: impl FnOnce()) {
    row_with(Modifier::new(), content);
}

/// Emits a row with `modifier`.
#[track_caller]
pub fn row_with(modifier: Modifier, content: impl FnOnce()) {
    layout(
        modifier,
        Linear::new(false, Arrangement::Start, Alignment::Start),
        content,
    );
}

/// Emits a stack that places the children of `content` on top of each other.
///
/// This is Compose's `Box`, which Rust reserves as a keyword.
#[track_caller]
pub fn stack(content: impl FnOnce()) {
    stack_with(Modifier::new(), content);
}

/// Emits a stack with `modifier`, putting children without their own `align` at `alignment`.
#[track_caller]
pub fn stack_aligned(modifier: Modifier, alignment: BoxAlignment, content: impl FnOnce()) {
    layout(
        modifier,
        Overlay {
            alignment,
            rtl: is_rtl(),
            propagate_min_constraints: false,
        },
        content,
    );
}

/// Emits a stack whose children get its minimum size as theirs, as Compose's
/// `Box(modifier, contentAlignment, propagateMinConstraints = true)`: a child of a stack
/// given a fixed or least size fills it, as the content of a `Surface` does.
#[track_caller]
pub fn stack_propagating(modifier: Modifier, alignment: BoxAlignment, content: impl FnOnce()) {
    layout(
        modifier,
        Overlay {
            alignment,
            rtl: is_rtl(),
            propagate_min_constraints: true,
        },
        content,
    );
}

/// Emits a stack with `modifier`.
#[track_caller]
pub fn stack_with(modifier: Modifier, content: impl FnOnce()) {
    stack_aligned(modifier, BoxAlignment::default(), content);
}

/// Emits empty space sized by `modifier`, such as `Modifier::new().height(8.0)`.
#[track_caller]
pub fn spacer(modifier: Modifier) {
    stack_with(modifier, || {});
}

/// How a row or column spreads its children along its main axis when it has room to spare.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Arrangement {
    Start,
    Center,
    End,
    /// The first child at the start, the last at the end, equal gaps between.
    SpaceBetween,
    /// Equal gaps between children, half of one at both ends.
    SpaceAround,
    /// Equal gaps between children and at both ends.
    SpaceEvenly,
    /// A fixed gap between children, starting at the start: Compose's `Arrangement.spacedBy(space)`.
    SpacedBy(f32),
    /// A fixed gap between children, the group placed by the alignment in the room left over:
    /// Compose's `Arrangement.spacedBy(space, alignment)`.
    SpacedByAligned(f32, Alignment),
}

/// Where a row or column puts each child across its main axis.
///
/// A child placed with [`Modifier::align`] goes where the cross part of its alignment says
/// instead (the vertical part in a row, the horizontal part in a column).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Alignment {
    Start,
    Center,
    End,
}

/// Emits a column with `arrangement` down the column and `alignment` across it.
#[track_caller]
pub fn column_arranged(
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    content: impl FnOnce(),
) {
    layout(modifier, Linear::new(true, arrangement, alignment), content);
}

/// Emits a row with `arrangement` along the row and `alignment` across it.
#[track_caller]
pub fn row_arranged(
    modifier: Modifier,
    arrangement: Arrangement,
    alignment: Alignment,
    content: impl FnOnce(),
) {
    layout(
        modifier,
        Linear::new(false, arrangement, alignment),
        content,
    );
}

/// Where the first of `count` children starts and the extra gap between
/// neighbours when `free` space is left over along the main axis.
pub(crate) fn arrange(arrangement: Arrangement, free: f32, count: usize) -> (f32, f32) {
    match arrangement {
        Arrangement::Start => (0.0, 0.0),
        Arrangement::Center => (free / 2.0, 0.0),
        Arrangement::End => (free, 0.0),
        Arrangement::SpaceBetween if count > 1 => (0.0, free / (count - 1) as f32),
        Arrangement::SpaceBetween => (0.0, 0.0),
        Arrangement::SpaceAround if count > 0 => {
            let gap = free / count as f32;
            (gap / 2.0, gap)
        }
        Arrangement::SpaceAround => (0.0, 0.0),
        Arrangement::SpaceEvenly => {
            let gap = free / (count + 1) as f32;
            (gap, gap)
        }
        Arrangement::SpacedBy(spacing) => (0.0, spacing),
        Arrangement::SpacedByAligned(spacing, alignment) => {
            let start = match alignment {
                Alignment::Start => 0.0,
                Alignment::Center => free / 2.0,
                Alignment::End => free,
            };
            (start, spacing)
        }
    }
}

/// A column or a row.
struct Linear {
    vertical: bool,
    arrangement: Arrangement,
    alignment: Alignment,
    /// Mirrors rows, and which side a column's start alignment is.
    rtl: bool,
}

impl Linear {
    fn new(vertical: bool, arrangement: Arrangement, alignment: Alignment) -> Self {
        Self {
            vertical,
            arrangement,
            alignment,
            rtl: is_rtl(),
        }
    }

    /// The fixed gap between neighbours that the arrangement asks for.
    fn gap(&self) -> f32 {
        match self.arrangement {
            Arrangement::SpacedBy(spacing) | Arrangement::SpacedByAligned(spacing, _) => spacing,
            _ => 0.0,
        }
    }

    /// Where child `index` goes across the main axis: its own `align` modifier, else the
    /// layout's alignment.
    fn alignment_of(&self, children: &Children, index: usize) -> Alignment {
        let Some(own) = children.alignment(index) else {
            return self.alignment;
        };
        let (horizontal, vertical) = match own {
            BoxAlignment::TopStart => (Alignment::Start, Alignment::Start),
            BoxAlignment::TopCenter => (Alignment::Center, Alignment::Start),
            BoxAlignment::TopEnd => (Alignment::End, Alignment::Start),
            BoxAlignment::CenterStart => (Alignment::Start, Alignment::Center),
            BoxAlignment::Center => (Alignment::Center, Alignment::Center),
            BoxAlignment::CenterEnd => (Alignment::End, Alignment::Center),
            BoxAlignment::BottomStart => (Alignment::Start, Alignment::End),
            BoxAlignment::BottomCenter => (Alignment::Center, Alignment::End),
            BoxAlignment::BottomEnd => (Alignment::End, Alignment::End),
        };
        if self.vertical { horizontal } else { vertical }
    }

    /// Splits a size into (main, cross) lengths.
    fn split(&self, size: Size) -> (f32, f32) {
        if self.vertical {
            (size.height, size.width)
        } else {
            (size.width, size.height)
        }
    }

    fn join(&self, main: f32, cross: f32) -> Size {
        if self.vertical {
            Size::new(cross, main)
        } else {
            Size::new(main, cross)
        }
    }

    fn constraints(&self, main: (f32, f32), cross_max: f32) -> Constraints {
        let (width, height) = if self.vertical {
            ((0.0, cross_max), main)
        } else {
            (main, (0.0, cross_max))
        };
        Constraints {
            min_width: width.0,
            max_width: width.1,
            min_height: height.0,
            max_height: height.1,
        }
    }
}

impl Linear {
    /// The intrinsic length along the main axis (the children's sum, plus any
    /// fixed spacing) or across it (the largest child), for one of the four
    /// intrinsic queries. `extent` is the length on the other axis.
    fn intrinsic(
        &self,
        children: &mut Children,
        width_query: bool,
        minimum: bool,
        extent: f32,
    ) -> f32 {
        let spacing = self.gap();
        let along = width_query != self.vertical;
        let count = children.len();
        let lengths = (0..count).map(|index| match (width_query, minimum) {
            (true, true) => children.min_intrinsic_width(index, extent),
            (true, false) => children.max_intrinsic_width(index, extent),
            (false, true) => children.min_intrinsic_height(index, extent),
            (false, false) => children.max_intrinsic_height(index, extent),
        });
        if along {
            lengths.sum::<f32>() + spacing * count.saturating_sub(1) as f32
        } else {
            lengths.fold(0.0, f32::max)
        }
    }
}

impl Layout for Linear {
    fn min_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        self.intrinsic(children, true, true, height)
    }

    fn max_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        self.intrinsic(children, true, false, height)
    }

    fn min_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        self.intrinsic(children, false, true, width)
    }

    fn max_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        self.intrinsic(children, false, false, width)
    }

    /// Measures the children without a weight first, in order, each within what is left of the
    /// bound; then the weighted children share what is left of the bound, or of the least length
    /// when there is no bound, as Compose's rows and columns share it, each exactly as long as
    /// its share when it fills and at most that long otherwise.
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let (main_min, cross_min) =
            self.split(Size::new(constraints.min_width, constraints.min_height));
        let (main_max, cross_max) =
            self.split(Size::new(constraints.max_width, constraints.max_height));
        let count = children.len();
        let spacing = self.gap();
        let mut mains = vec![0.0_f32; count];
        let mut crosses = vec![0.0_f32; count];
        let mut used = spacing * count.saturating_sub(1) as f32;

        let mut total_weight = 0.0;
        for index in 0..count {
            if let Some(weight) = children.weight(index) {
                total_weight += weight.max(0.0);
                continue;
            }
            let room = (main_max - used).max(0.0);
            let size = children.measure(index, self.constraints((0.0, room), cross_max));
            (mains[index], crosses[index]) = self.split(size);
            used += mains[index];
        }

        if total_weight > 0.0 {
            let target = if main_max.is_finite() {
                main_max
            } else {
                main_min
            };
            let space = (target - used).max(0.0);
            for index in 0..count {
                let Some(weight) = children.weight(index) else {
                    continue;
                };
                let share = space * weight.max(0.0) / total_weight;
                let least = if children.weight_fills(index) {
                    share
                } else {
                    0.0
                };
                let size = children.measure(index, self.constraints((least, share), cross_max));
                (mains[index], crosses[index]) = self.split(size);
                used += mains[index];
            }
        }

        let main_size = used.max(main_min).min(main_max);
        let cross_size = crosses
            .iter()
            .fold(cross_min, |widest, cross| widest.max(*cross))
            .min(cross_max.max(cross_min));

        let free = (main_size - used).max(0.0);
        let (start, gap) = arrange(self.arrangement, free, count);

        let mut position = start;
        let mut positions = Vec::with_capacity(count);
        for index in 0..count {
            // In a column the cross axis is horizontal, so start and end swap when mirrored.
            let alignment = match (
                self.alignment_of(children, index),
                self.vertical && self.rtl,
            ) {
                (Alignment::Start, true) => Alignment::End,
                (Alignment::End, true) => Alignment::Start,
                (other, _) => other,
            };
            let across = match alignment {
                Alignment::Start => 0.0,
                Alignment::Center => (cross_size - crosses[index]) / 2.0,
                Alignment::End => cross_size - crosses[index],
            };
            let (x, y) = if self.vertical {
                (across, position)
            } else if self.rtl {
                // A mirrored row starts at its right edge.
                (main_size - position - mains[index], across)
            } else {
                (position, across)
            };
            positions.push(Offset::new(x, y));
            position += mains[index] + gap;
        }

        LayoutResult {
            size: self.join(main_size, cross_size),
            positions,
        }
    }
}
