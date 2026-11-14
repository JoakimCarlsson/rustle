use rustle_geometry::{Offset, Size};
use rustle_ui::{Children, Layout, LayoutResult, Modifier, is_rtl, layout};
use rustle_unit::Constraints;

use crate::linear::{Arrangement, arrange};

/// How a [`flow_row_with`] or [`flow_column_with`] fills its lines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlowOptions {
    /// The gap between neighbours along a line.
    pub main_spacing: f32,
    /// The gap between lines.
    pub cross_spacing: f32,
    /// The most lines shown. Children that would start another line are left out.
    pub max_lines: Option<usize>,
    /// The most children in one line; a longer run wraps even if it would fit.
    pub max_items_per_line: Option<usize>,
    /// How the space left in a line is spread, when the container is bounded.
    /// `SpacedBy` is the same as `Start`: spacing is `main_spacing`.
    pub arrangement: Arrangement,
}

impl FlowOptions {
    /// Lines with `main_spacing` between children and `cross_spacing` between
    /// lines, no limits, children at the start of their line.
    pub fn new(main_spacing: f32, cross_spacing: f32) -> Self {
        Self {
            main_spacing,
            cross_spacing,
            max_lines: None,
            max_items_per_line: None,
            arrangement: Arrangement::Start,
        }
    }

    /// Shows at most `max_lines` lines.
    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.max_lines = Some(max_lines);
        self
    }

    /// Puts at most `max_items` children in a line.
    pub fn max_items_per_line(mut self, max_items: usize) -> Self {
        self.max_items_per_line = Some(max_items);
        self
    }

    /// Spreads the space left in each line by `arrangement`.
    pub fn arrangement(mut self, arrangement: Arrangement) -> Self {
        self.arrangement = arrangement;
        self
    }
}

/// Emits a row that fills with the children of `content` left to right and
/// starts a new line when the next one does not fit, with the given gaps.
#[track_caller]
pub fn flow_row(
    modifier: Modifier,
    horizontal_spacing: f32,
    vertical_spacing: f32,
    content: impl FnOnce(),
) {
    flow_row_with(
        modifier,
        FlowOptions::new(horizontal_spacing, vertical_spacing),
        content,
    );
}

/// Emits a flow row with `options`: a limit on lines or children per line, and
/// an arrangement for the space left in a line. Children with `weight` share
/// what is left of their line in proportion to it. In a right-to-left layout
/// lines start at the right edge.
#[track_caller]
pub fn flow_row_with(modifier: Modifier, options: FlowOptions, content: impl FnOnce()) {
    layout(
        modifier,
        Flow {
            vertical: false,
            options,
            rtl: is_rtl(),
        },
        content,
    );
}

/// Emits a column that fills with the children of `content` top to bottom and
/// starts a new column when the next one does not fit, with the given gaps.
#[track_caller]
pub fn flow_column(
    modifier: Modifier,
    vertical_spacing: f32,
    horizontal_spacing: f32,
    content: impl FnOnce(),
) {
    flow_column_with(
        modifier,
        FlowOptions::new(vertical_spacing, horizontal_spacing),
        content,
    );
}

/// Emits a flow column with `options`; see [`flow_row_with`]. Its main axis is
/// vertical, so `main_spacing` is between children in a column and
/// `cross_spacing` between columns. In a right-to-left layout the first
/// column is at the right edge.
#[track_caller]
pub fn flow_column_with(modifier: Modifier, options: FlowOptions, content: impl FnOnce()) {
    layout(
        modifier,
        Flow {
            vertical: true,
            options,
            rtl: is_rtl(),
        },
        content,
    );
}

/// Places children in lines, wrapping when the next does not fit the main axis.
struct Flow {
    vertical: bool,
    options: FlowOptions,
    rtl: bool,
}

impl Flow {
    /// Splits a size into (main, cross) lengths.
    fn split(&self, size: Size) -> (f32, f32) {
        if self.vertical {
            (size.height, size.width)
        } else {
            (size.width, size.height)
        }
    }

    /// Joins (main, cross) lengths into a size.
    fn join(&self, main: f32, cross: f32) -> Size {
        if self.vertical {
            Size::new(cross, main)
        } else {
            Size::new(main, cross)
        }
    }

    /// Breaks children of the given main lengths into lines of indices, none
    /// longer than `limit` unless a single child is.
    fn lines(&self, mains: &[f32], limit: f32) -> Vec<Vec<usize>> {
        let mut lines: Vec<Vec<usize>> = Vec::new();
        let mut used = 0.0;
        for (index, main) in mains.iter().enumerate() {
            let wrap = match lines.last() {
                None => true,
                Some(line) => {
                    used + self.options.main_spacing + main > limit
                        || self
                            .options
                            .max_items_per_line
                            .is_some_and(|most| line.len() >= most.max(1))
                }
            };
            if wrap {
                lines.push(vec![index]);
                used = *main;
            } else {
                lines
                    .last_mut()
                    .expect("a line was just checked")
                    .push(index);
                used += self.options.main_spacing + main;
            }
        }
        lines
    }

    /// The main length child `index` would like, by its intrinsic size.
    fn child_main(&self, children: &mut Children, index: usize, minimum: bool) -> f32 {
        match (self.vertical, minimum) {
            (true, true) => children.min_intrinsic_height(index, f32::INFINITY),
            (true, false) => children.max_intrinsic_height(index, f32::INFINITY),
            (false, true) => children.min_intrinsic_width(index, f32::INFINITY),
            (false, false) => children.max_intrinsic_width(index, f32::INFINITY),
        }
    }

    /// The summed cross length of the lines children break into at `limit`.
    fn cross_intrinsic(&self, children: &mut Children, limit: f32) -> f32 {
        let count = children.len();
        let mains: Vec<f32> = (0..count)
            .map(|index| self.child_main(children, index, false))
            .collect();
        let lines = self.lines(&mains, limit);
        let shown = self.options.max_lines.unwrap_or(usize::MAX);
        let mut total = 0.0;
        for (number, line) in lines.iter().take(shown).enumerate() {
            let tallest = line.iter().fold(0.0_f32, |tallest, index| {
                let cross = if self.vertical {
                    children.max_intrinsic_width(*index, mains[*index])
                } else {
                    children.max_intrinsic_height(*index, mains[*index])
                };
                tallest.max(cross)
            });
            total += tallest
                + if number > 0 {
                    self.options.cross_spacing
                } else {
                    0.0
                };
        }
        total
    }

    /// The intrinsic length along the main axis: the widest unbreakable child,
    /// or every child on one line.
    fn main_intrinsic(&self, children: &mut Children, minimum: bool) -> f32 {
        let count = children.len();
        let lengths = (0..count).map(|index| self.child_main(children, index, minimum));
        if minimum {
            lengths.fold(0.0, f32::max)
        } else {
            lengths.sum::<f32>() + self.options.main_spacing * count.saturating_sub(1) as f32
        }
    }
}

impl Layout for Flow {
    fn min_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        if self.vertical {
            self.cross_intrinsic(children, height)
        } else {
            self.main_intrinsic(children, true)
        }
    }

    fn max_intrinsic_width(&self, children: &mut Children, height: f32) -> f32 {
        if self.vertical {
            self.cross_intrinsic(children, height)
        } else {
            self.main_intrinsic(children, false)
        }
    }

    fn min_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        if self.vertical {
            self.main_intrinsic(children, true)
        } else {
            self.cross_intrinsic(children, width)
        }
    }

    fn max_intrinsic_height(&self, children: &mut Children, width: f32) -> f32 {
        if self.vertical {
            self.main_intrinsic(children, false)
        } else {
            self.cross_intrinsic(children, width)
        }
    }

    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let (main_max, _) = self.split(Size::new(constraints.max_width, constraints.max_height));
        let (main_min, cross_min) =
            self.split(Size::new(constraints.min_width, constraints.min_height));
        let loose = |main_limit: f32| {
            let (max_width, max_height) = if self.vertical {
                (f32::INFINITY, main_limit)
            } else {
                (main_limit, f32::INFINITY)
            };
            Constraints {
                min_width: 0.0,
                max_width,
                min_height: 0.0,
                max_height,
            }
        };
        let count = children.len();
        let mut sizes = Vec::with_capacity(count);
        for index in 0..count {
            let size = children.measure(index, loose(main_max));
            sizes.push(self.split(size));
        }
        let mains: Vec<f32> = sizes.iter().map(|(main, _)| *main).collect();
        let mut lines = self.lines(&mains, main_max);
        if let Some(most) = self.options.max_lines {
            for line in lines.split_off(most.min(lines.len())) {
                for index in line {
                    children.omit(index);
                }
            }
        }

        let mut positions = vec![Offset::default(); count];
        let mut any_weight = false;
        let mut cross_offset = 0.0_f32;
        let mut widest = 0.0_f32;
        let mut placed: Vec<(Vec<usize>, f32, f32)> = Vec::new();
        for line in &lines {
            let spacing = self.options.main_spacing * line.len().saturating_sub(1) as f32;
            let mut used = spacing + line.iter().map(|index| sizes[*index].0).sum::<f32>();
            let total_weight: f32 = line
                .iter()
                .filter_map(|index| children.weight(*index))
                .map(|weight| weight.max(0.0))
                .sum();
            if total_weight > 0.0 {
                any_weight = true;
                let leftover = if main_max.is_finite() {
                    (main_max - used).max(0.0)
                } else {
                    0.0
                };
                for index in line {
                    let Some(weight) = children.weight(*index) else {
                        continue;
                    };
                    let main = sizes[*index].0 + leftover * weight.max(0.0) / total_weight;
                    let cross_loose = (0.0, f32::INFINITY);
                    let (min_width, max_width, min_height, max_height) = if self.vertical {
                        (cross_loose.0, cross_loose.1, main, main)
                    } else {
                        (main, main, cross_loose.0, cross_loose.1)
                    };
                    let size = children.measure(
                        *index,
                        Constraints {
                            min_width,
                            max_width,
                            min_height,
                            max_height,
                        },
                    );
                    sizes[*index] = self.split(size);
                }
                used = spacing + line.iter().map(|index| sizes[*index].0).sum::<f32>();
            }
            let line_cross = line
                .iter()
                .fold(0.0_f32, |tallest, index| tallest.max(sizes[*index].1));
            placed.push((line.clone(), used, line_cross));
            widest = widest.max(used);
        }

        let fills = any_weight || self.options.arrangement != Arrangement::Start;
        let main_size = if fills && main_max.is_finite() {
            main_max
        } else {
            widest
        }
        .clamp(main_min, main_max.max(main_min));
        for (line, used, line_cross) in &placed {
            let free = if main_size.is_finite() {
                (main_size - used).max(0.0)
            } else {
                0.0
            };
            let (start, extra) = match self.options.arrangement {
                Arrangement::SpacedBy(_) => (0.0, 0.0),
                other => arrange(other, free, line.len()),
            };
            let mut main = start;
            for index in line {
                let (length, _) = sizes[*index];
                let along = if self.rtl && !self.vertical {
                    main_size - main - length
                } else {
                    main
                };
                let offset = self.join(along, cross_offset);
                positions[*index] = Offset::new(offset.width, offset.height);
                main += length + self.options.main_spacing + extra;
            }
            cross_offset += line_cross + self.options.cross_spacing;
        }
        let cross_size = (cross_offset
            - if placed.is_empty() {
                0.0
            } else {
                self.options.cross_spacing
            })
        .max(cross_min);
        if self.rtl && self.vertical {
            for line in lines.iter() {
                for index in line {
                    positions[*index].x = cross_size - positions[*index].x - sizes[*index].1;
                }
            }
        }
        LayoutResult {
            size: self.join(main_size, cross_size),
            positions,
        }
    }
}
