use std::rc::Rc;

use rustle_geometry::{Offset, Size};
use rustle_ui::{Children, Layout, LayoutResult, Modifier, layout};
use rustle_unit::Constraints;

/// Emits `content` in an area that is `size` (width, height) when given and
/// as large as the content when not, within what the parent allows. Whatever
/// the content measures at its natural size is passed to `on_natural` at each
/// layout, so a caller can animate `size` toward it. What overflows is cut off.
#[track_caller]
pub fn sized_box(
    modifier: Modifier,
    size: Option<(f32, f32)>,
    on_natural: impl Fn((f32, f32)) + 'static,
    content: impl FnOnce(),
) {
    layout(
        modifier.clip(),
        SizedBox {
            size,
            on_natural: Rc::new(on_natural),
        },
        content,
    );
}

/// Measures children at their natural size, reports it, and takes a size it was given.
struct SizedBox {
    size: Option<(f32, f32)>,
    on_natural: Rc<dyn Fn((f32, f32))>,
}

impl Layout for SizedBox {
    fn measure(&self, children: &mut Children, constraints: Constraints) -> LayoutResult {
        let loose = Constraints {
            min_width: 0.0,
            min_height: 0.0,
            ..constraints
        };
        let mut natural = Size::default();
        for index in 0..children.len() {
            let child = children.measure(index, loose);
            natural.width = natural.width.max(child.width);
            natural.height = natural.height.max(child.height);
        }
        (self.on_natural)((natural.width, natural.height));
        let shown = self
            .size
            .map_or(natural, |(width, height)| Size::new(width, height));
        LayoutResult {
            size: constraints.constrain(shown),
            positions: vec![Offset::default(); children.len()],
        }
    }
}
