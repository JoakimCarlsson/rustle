//! What is drawn over the display list each frame: hover feedback and carried items.

use rustle_geometry::{Offset, Rect};
use rustle_graphics::{Border, Color, DrawCommand};
use rustle_ui::DisplayList;

use crate::app::Carrying;
use crate::pointer::same_area;

/// What is drawn on top while something is carried: the drop target under the
/// pointer lit up, the place it came from dimmed, and a lifted, translucent copy
/// of it following the pointer.
pub(crate) fn carrying_overlay(
    display: &DisplayList,
    carrying: Option<&Carrying>,
    pointer: Option<Offset>,
) -> Option<Vec<DrawCommand>> {
    let carrying = carrying.filter(|carrying| carrying.active)?;
    let pointer = pointer?;
    let mut out = Vec::new();
    // Every place it can be dropped is outlined; the one it would land on is filled.
    let over = display.drop_target_at(pointer).map(|target| target.rect);
    for target in &display.drop_targets {
        let hot = over.is_some_and(|rect| same_area(rect, target.rect));
        out.push(DrawCommand::Rect {
            rect: target.rect,
            color: Color::rgba(59, 130, 246, if hot { 40 } else { 0 }),
            corner_radius: 10.0,
            border: Some(Border {
                width: if hot { 2.0 } else { 1.0 },
                color: Color::rgba(59, 130, 246, if hot { 255 } else { 110 }),
            }),
        });
    }
    out.push(DrawCommand::Rect {
        rect: carrying.rect,
        color: Color::rgba(128, 128, 128, 70),
        corner_radius: 8.0,
        border: Some(Border {
            width: 1.0,
            color: Color::rgba(128, 128, 128, 140),
        }),
    });
    let (dx, dy) = (pointer.x - carrying.began.x, pointer.y - carrying.began.y);
    let lifted = Rect::new(
        Offset::new(carrying.rect.origin.x + dx, carrying.rect.origin.y + dy),
        carrying.rect.size,
    );
    out.push(DrawCommand::Shadow {
        rect: lifted,
        corner_radius: 8.0,
        blur: 18.0,
        color: Color::rgba(0, 0, 0, 90),
    });
    out.push(DrawCommand::PushLayer {
        rect: lifted,
        alpha: 0.9,
        rotation: 0.03,
        scale: 1.04,
    });
    if let Some(commands) = display.commands.get(carrying.commands.clone()) {
        out.extend(
            commands
                .iter()
                .cloned()
                .map(|command| command.translated(dx, dy)),
        );
    }
    out.push(DrawCommand::PopLayer);
    Some(out)
}

/// The tint drawn over the clickable region under the pointer: lighter while
/// hovering, darker while pressed. Nothing is drawn if the button went down on
/// a different region.
pub(crate) fn feedback(
    display: &DisplayList,
    pointer: Option<Offset>,
    pressed: &Option<Rect>,
) -> Option<DrawCommand> {
    let region = display.hit_test(pointer?)?;
    if !region.feedback {
        return None;
    }
    let is_pressed = pressed.is_some_and(|pressed| same_area(pressed, region.rect));
    if pressed.is_some() && !is_pressed {
        return None;
    }
    Some(DrawCommand::Rect {
        rect: region.rect,
        color: if is_pressed {
            Color::rgba(0, 0, 0, 70)
        } else {
            Color::rgba(255, 255, 255, 30)
        },
        corner_radius: region.corner_radius,
        border: None,
    })
}
