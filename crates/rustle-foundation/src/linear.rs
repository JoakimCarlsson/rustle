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
    /// A fixed gap between children, starting at the start.
    SpacedBy(f32),
}

/// Where a row or column puts each child across its main axis.
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
        let spacing = match self.arrangement {
            Arrangement::SpacedBy(spacing) => spacing,
            _ => 0.0,
        };
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

    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let (main_min, cross_min) =
            self.split(Size::new(constraints.min_width, constraints.min_height));
        let (main_max, cross_max) =
            self.split(Size::new(constraints.max_width, constraints.max_height));
        let count = children.len();
        let spacing = match self.arrangement {
            Arrangement::SpacedBy(spacing) => spacing,
            _ => 0.0,
        };
        let mut mains = vec![0.0_f32; count];
        let mut crosses = vec![0.0_f32; count];
        let mut used = spacing * count.saturating_sub(1) as f32;

        // Children without a weight take what they ask for, in order.
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

        // Weighted children split what is left, when there is a bound to leave it.
        if total_weight > 0.0 {
            let space = if main_max.is_finite() {
                (main_max - used).max(0.0)
            } else {
                0.0
            };
            for index in 0..count {
                let Some(weight) = children.weight(index) else {
                    continue;
                };
                let share = space * weight.max(0.0) / total_weight;
                let size = children.measure(index, self.constraints((share, share), cross_max));
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
            let alignment = match (self.alignment, self.vertical && self.rtl) {
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
