use rustle_geometry::{Offset, Size};
use rustle_ui::{Canvas, Children, Layout, LayoutResult, Modifier, is_rtl, layout};
use rustle_unit::Constraints;

/// Emits `content` stacked over a canvas that is drawn behind it and is exactly
/// as large as the content, for frames, glows and other decoration that must
/// follow the size of what it surrounds.
#[track_caller]
pub fn backdrop(modifier: Modifier, draw: impl Fn(&mut Canvas) + 'static, content: impl FnOnce()) {
    layout(modifier, Backdrop, || {
        rustle_ui::canvas(Modifier::new(), draw);
        content();
    });
}

/// Sizes itself to every child but the first, then makes the first as large.
struct Backdrop;

impl Layout for Backdrop {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let loose = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            ..constraints
        };
        let mut size = Size::default();
        for index in 1..children.len() {
            let child = children.measure(index, loose);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        let size = constraints.constrain(size);
        if !children.is_empty() {
            children.measure(
                0,
                Constraints {
                    min_width: size.width,
                    max_width: size.width,
                    min_height: size.height,
                    max_height: size.height,
                },
            );
        }
        LayoutResult {
            size,
            positions: vec![Offset::default(); children.len()],
        }
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
    layout(
        modifier,
        Flow {
            horizontal: horizontal_spacing,
            vertical: vertical_spacing,
            rtl: is_rtl(),
        },
        content,
    );
}

/// Places children in lines, wrapping when the next does not fit the width.
struct Flow {
    horizontal: f32,
    vertical: f32,
    rtl: bool,
}

impl Layout for Flow {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let child_constraints = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: constraints.max_width,
            max_height: f32::INFINITY,
        };
        let (mut x, mut y, mut line_height, mut width) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        let mut positions = Vec::with_capacity(children.len());
        let mut sizes = Vec::with_capacity(children.len());
        for index in 0..children.len() {
            let size = children.measure(index, child_constraints);
            sizes.push(size);
            if x > 0.0 && x + size.width > constraints.max_width {
                x = 0.0;
                y += line_height + self.vertical;
                line_height = 0.0;
            }
            positions.push(Offset::new(x, y));
            x += size.width + self.horizontal;
            line_height = line_height.max(size.height);
            width = width.max(x - self.horizontal);
        }
        if self.rtl {
            for (position, size) in positions.iter_mut().zip(&sizes) {
                position.x = width - position.x - size.width;
            }
        }
        LayoutResult {
            size: Size::new(width, y + line_height),
            positions,
        }
    }
}

/// Emits `content` showing only `fraction` (0 to 1) of its height, cut off
/// below. For collapsing and expanding things.
#[track_caller]
pub fn reveal(fraction: f32, modifier: Modifier, content: impl FnOnce()) {
    layout(
        modifier.clip(),
        Reveal {
            fraction: fraction.clamp(0.0, 1.0),
        },
        content,
    );
}

/// Measures children at their natural height and takes a fraction of it.
struct Reveal {
    fraction: f32,
}

impl Layout for Reveal {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let natural = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            max_width: constraints.max_width,
            max_height: f32::INFINITY,
        };
        let mut size = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, natural);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        LayoutResult {
            size: Size::new(size.width, size.height * self.fraction),
            positions: vec![Offset::default(); children.len()],
        }
    }
}

/// Emits `content` in a layer above everything else, as large as it likes,
/// taking no space where it is emitted: its top-left corner is where it would
/// have started. For popups, menus and tooltips. Position it with `align`.
#[track_caller]
pub fn floating(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier.z_index(1), Floating, content);
}

/// Measures children without limits and takes no space itself.
struct Floating;

impl Layout for Floating {
    fn measure(&self, children: &mut Children, _: Constraints) -> LayoutResult {
        let unbounded = Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        };
        for index in 0..children.len() {
            children.measure(index, unbounded);
        }
        LayoutResult {
            size: Size::default(),
            positions: vec![Offset::default(); children.len()],
        }
    }
}
