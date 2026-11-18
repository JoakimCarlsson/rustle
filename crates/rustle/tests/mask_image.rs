//! Lays out [`mask_image`] in an [`Embedded`] interface and checks the mask command it draws:
//! its size, where it is and its color.

use rustle::prelude::*;
use rustle::{Embedded, Rect, mask_image};
use rustle_graphics::DrawCommand;

/// The masks `root` draws at 200 by 200 logical pixels, with their rectangles and colors.
fn masks(root: impl FnMut() + 'static) -> Vec<(Rect, Color)> {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((200, 200), 1.0);
    embedded.update();
    embedded
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Mask { rect, color, .. } => Some((*rect, *color)),
            _ => None,
        })
        .collect()
}

/// A mask of `width` by `height` pixels, all covered.
fn solid(width: u32, height: u32) -> ImageData {
    ImageData::from_alpha(width, height, &vec![255; (width * height) as usize]).unwrap()
}

/// Without a size the mask is as many logical pixels as it has pixels.
#[test]
fn a_mask_is_its_pixel_size_by_default() {
    let drawn = masks(|| {
        column_with(Modifier::new().padding(10.0), || {
            mask_image(Modifier::new(), &solid(12, 8), Color::rgb(1, 2, 3));
        });
    });
    assert_eq!(drawn.len(), 1);
    let (rect, color) = drawn[0];
    assert_eq!((rect.origin.x, rect.origin.y), (10.0, 10.0));
    assert_eq!((rect.size.width, rect.size.height), (12.0, 8.0));
    assert_eq!(color, Color::rgb(1, 2, 3));
}

/// A size given by the modifier wins, and the mask is stretched over it.
#[test]
fn a_mask_takes_the_size_its_modifier_gives() {
    let drawn = masks(|| {
        mask_image(
            Modifier::new().size(16.0, 16.0),
            &solid(32, 32),
            Color::rgb(9, 9, 9),
        );
    });
    assert_eq!(drawn.len(), 1);
    assert_eq!(
        (drawn[0].0.size.width, drawn[0].0.size.height),
        (16.0, 16.0)
    );
}
