//! Lays an [`Embedded`] interface out with and without pixel rounding and checks where an
//! area at a fractional place is drawn.

use rustle::prelude::*;
use rustle::{Embedded, PixelRounding, Rect};
use rustle_graphics::DrawCommand;

/// A root drawing one 5.3 by 5.3 square offset to (10.4, 0.6).
fn square() {
    canvas(
        Modifier::new()
            .absolute_offset(10.4, 0.6)
            .size(5.3, 5.3)
            .background(Color::rgba(10, 20, 30, 255)),
        |_| {},
    );
}

/// The rectangle of the square as `embedded` draws it now.
fn drawn(embedded: &Embedded) -> Rect {
    embedded
        .commands()
        .iter()
        .find_map(|command| match command {
            DrawCommand::Rect { rect, .. } => Some(*rect),
            _ => None,
        })
        .expect("the square is drawn")
}

/// Rounding is off until the host turns it on, then edges land on whole pixels.
#[test]
fn the_host_turns_pixel_rounding_on() {
    let mut embedded = Embedded::without_renderer(square, || {});
    embedded.resize((200, 200), 2.0);
    assert_eq!(embedded.pixel_rounding(), PixelRounding::None);
    assert_eq!(drawn(&embedded).origin.x, 10.4);
    embedded.set_pixel_rounding(PixelRounding::Logical);
    let rect = drawn(&embedded);
    assert_eq!((rect.origin.x, rect.origin.y), (10.0, 1.0));
    assert_eq!((rect.size.width, rect.size.height), (6.0, 5.0));
    assert!(embedded.update(), "a new rounding is drawn");
    embedded.set_pixel_rounding(PixelRounding::Physical);
    let rect = drawn(&embedded);
    assert_eq!((rect.origin.x, rect.origin.y), (10.5, 0.5));
}
