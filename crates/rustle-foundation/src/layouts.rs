use rustle_geometry::{Offset, Size};
use rustle_ui::{
    Canvas, Children, Layout, LayoutResult, Modifier, SubcomposeResult, layout, subcompose_layout,
};
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
    layout(modifier.z_index_unclipped(1), Floating, content);
}

/// Emits `content` in a layer above everything else, like [`floating`], but
/// with its bottom-left corner where it would have started, so it opens
/// upward: for menus over a field near the bottom of the window.
#[track_caller]
pub fn floating_above(modifier: Modifier, content: impl FnOnce()) {
    layout(modifier.z_index_unclipped(1), FloatingAbove, content);
}

/// Measures children without limits, takes no space itself and puts each
/// child's bottom where it starts.
struct FloatingAbove;

impl Layout for FloatingAbove {
    fn measure(&self, children: &mut Children, _: Constraints) -> LayoutResult {
        let unbounded = Constraints {
            min_width: 0.0,
            max_width: f32::INFINITY,
            min_height: 0.0,
            max_height: f32::INFINITY,
        };
        let positions = (0..children.len())
            .map(|index| {
                let size = children.measure(index, unbounded);
                Offset::new(0.0, -size.height)
            })
            .collect();
        LayoutResult {
            size: Size::default(),
            positions,
        }
    }
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

/// Emits a stack whose content is composed while it measures, so it can
/// depend on the constraints: `content` gets the constraints the stack was
/// given, for choosing a layout that fits the room (Compose's
/// `BoxWithConstraints`).
///
/// The stack takes the size of the largest child, within its constraints.
/// What `content` remembers is kept between passes.
#[track_caller]
pub fn with_constraints(modifier: Modifier, content: impl Fn(Constraints) + 'static) {
    let content = std::rc::Rc::new(content);
    subcompose_layout(modifier, move |scope, constraints| {
        let content = std::rc::Rc::clone(&content);
        let indices = scope.subcompose(0_u8, move || content(constraints));
        let loose = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            ..constraints
        };
        let mut size = Size::default();
        for index in &indices {
            let child = scope.measure(*index, loose);
            size.width = size.width.max(child.width);
            size.height = size.height.max(child.height);
        }
        SubcomposeResult {
            size: constraints.constrain(size),
            placements: indices
                .into_iter()
                .map(|index| (index, Offset::default()))
                .collect(),
        }
    });
}
