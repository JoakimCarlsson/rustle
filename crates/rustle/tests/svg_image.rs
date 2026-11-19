//! Lays out [`svg_image`] and [`svg_icon`] in an [`Embedded`] interface and checks the SVG
//! command each draws: its size, where it is, how it fits and its tint.

use rustle::prelude::*;
use rustle::{ContentScale, Embedded, Rect, SvgPainter, svg_icon, svg_image};
use rustle_graphics::DrawCommand;

/// A document 20 wide and 10 tall.
const WIDE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="#00ff00"/></svg>"##;

/// The SVG documents `root` draws at 200 by 200 logical pixels, with their rectangles and tints.
fn drawn(root: impl FnMut() + 'static) -> Vec<(Rect, Option<Color>)> {
    let mut embedded = Embedded::without_renderer(root, || {});
    embedded.resize((200, 200), 1.0);
    embedded.update();
    embedded
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::Svg { rect, tint, .. } => Some((*rect, *tint)),
            _ => None,
        })
        .collect()
}

/// Without a size the document is its own size, in its own colors.
#[test]
fn an_svg_image_is_its_own_size_by_default() {
    let painter = SvgPainter::from_str(WIDE).unwrap();
    let drawn = drawn(move || {
        column_with(Modifier::new().padding(10.0), || {
            svg_image(Modifier::new(), &painter, ContentScale::Fit, None);
        });
    });
    assert_eq!(drawn.len(), 1);
    let (rect, tint) = drawn[0];
    assert_eq!((rect.origin.x, rect.origin.y), (10.0, 10.0));
    assert_eq!((rect.size.width, rect.size.height), (20.0, 10.0));
    assert_eq!(tint, None);
}

/// A size given by the modifier wins, and the document is fitted in it or stretched over it.
#[test]
fn an_svg_image_fills_its_size_as_its_scale_says() {
    let painter = SvgPainter::from_str(WIDE).unwrap();
    let fitted = painter.clone();
    let drawn_fit = drawn(move || {
        svg_image(
            Modifier::new().size(40.0, 40.0),
            &fitted,
            ContentScale::Fit,
            None,
        );
    });
    let rect = drawn_fit[0].0;
    assert_eq!((rect.origin.x, rect.origin.y), (0.0, 10.0));
    assert_eq!((rect.size.width, rect.size.height), (40.0, 20.0));

    let drawn_stretched = drawn(move || {
        svg_image(
            Modifier::new().size(40.0, 40.0),
            &painter,
            ContentScale::Stretch,
            None,
        );
    });
    let rect = drawn_stretched[0].0;
    assert_eq!((rect.size.width, rect.size.height), (40.0, 40.0));
}

/// An icon is the size it is given, the document fitted and centered in it, and tinted.
#[test]
fn an_svg_icon_is_square_and_tinted() {
    let painter = SvgPainter::from_str(WIDE).unwrap();
    let drawn = drawn(move || {
        svg_icon(Modifier::new(), &painter, 16.0, Some(Color::rgb(1, 2, 3)));
    });
    assert_eq!(drawn.len(), 1);
    let (rect, tint) = drawn[0];
    assert_eq!((rect.origin.x, rect.origin.y), (0.0, 4.0));
    assert_eq!((rect.size.width, rect.size.height), (16.0, 8.0));
    assert_eq!(tint, Some(Color::rgb(1, 2, 3)));
}
