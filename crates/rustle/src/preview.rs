//! Drawing a composable once, without a window.

use std::rc::Rc;

use rustle_foundation::{Focus, Key, KeyEvent, Modifiers};
use rustle_geometry::{Offset, Size};
use rustle_graphics::ImageData;
use rustle_ui::{Composition, DisplayList, PressEvent};
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;

/// Composes `root` and draws it once, with no window, into a PNG at `path`.
///
/// The picture is `width` by `height` pixels and uses the dark or the light
/// appearance. State, effects and tasks that wait on frames are not run
/// beyond the first composition. This is the way to see a composable without
/// opening a window, from a build script, a tool or another program.
pub fn preview(
    root: impl FnMut() + 'static,
    (width, height): (u32, u32),
    dark: bool,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    snapshot(root, dark, (width, height), path)
}

/// Composes `root` and draws it once, off screen, into a PNG at `path`.
pub(crate) fn snapshot(
    mut root: impl FnMut() + 'static,
    dark: bool,
    (width, height): (u32, u32),
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut composition = rustle_ui::composition(move || {
        rustle_foundation::LOCAL_SYSTEM_DARK.provide(dark, &mut root);
    });
    let mut renderer = Renderer::offscreen(width, height)?;
    composition.compose();
    let constraints = Constraints::loose(Size::new(width as f32, height as f32));
    // Laying out can change state, such as scrolling a caret into view, which
    // composes and lays out again, as the event loop does. A few rounds settle it.
    let settle = |composition: &mut Composition,
                  renderer: &mut Renderer|
     -> Result<DisplayList, Box<dyn std::error::Error>> {
        let mut display = None;
        for _ in 0..8 {
            let tree = composition.output().ok_or("nothing was composed")?;
            display = Some(rustle_ui::display_list(tree, constraints, renderer));
            if !composition.recompose() {
                break;
            }
        }
        display.ok_or_else(|| "nothing was laid out".into())
    };
    let mut display = settle(&mut composition, &mut renderer)?;

    // Replays input against the laid-out UI, step by step, as the event loop
    // would handle it: RUSTLE_SNAPSHOT_INPUT="click:x,y;move:x,y;type:text;key:Backspace".
    let mut focused: Option<Focus> = None;
    let mut clock = std::time::Duration::ZERO;
    let steps = std::env::var("RUSTLE_SNAPSHOT_INPUT").unwrap_or_default();
    for step in steps.split(';').filter(|step| !step.is_empty()) {
        let (kind, argument) = step.split_once(':').unwrap_or((step, ""));
        match kind {
            "click" => {
                let (x, y) = argument.split_once(',').ok_or("click needs x,y")?;
                let at = Offset::new(x.trim().parse()?, y.trim().parse()?);
                let next = display.focus_at(at).map(|region| region.focus.clone());
                if focused.as_ref().map(Focus::id) != next.as_ref().map(Focus::id) {
                    if let Some(old) = &focused {
                        old.set_focused(false);
                    }
                    if let Some(new) = &next {
                        new.set_focused(true);
                    }
                    focused = next;
                }
                if let Some(region) = display.press_at(at) {
                    let position = Offset::new(at.x - region.origin.x, at.y - region.origin.y);
                    (Rc::clone(&region.on_press))(PressEvent {
                        position,
                        click_count: 1,
                    });
                }
                if let Some(region) = display.hit_test(at) {
                    (Rc::clone(&region.on_click))();
                }
            }
            "move" => {
                // Puts the pointer at x,y: hover and pointer-move handlers hear about it.
                let (x, y) = argument.split_once(',').ok_or("move needs x,y")?;
                let at = Offset::new(x.trim().parse()?, y.trim().parse()?);
                for region in display.hovers_at(at) {
                    (Rc::clone(&region.on_hover))(true);
                }
                for region in display.pointer_moves_at(at) {
                    let local = Offset::new(at.x - region.origin.x, at.y - region.origin.y);
                    (Rc::clone(&region.on_move))(Some(local));
                }
            }
            "type" | "key" => {
                let key = if kind == "type" {
                    Key::Character(argument.to_string())
                } else {
                    match argument {
                        "Backspace" => Key::Backspace,
                        "Delete" => Key::Delete,
                        "Enter" => Key::Enter,
                        "Tab" => Key::Tab,
                        "Left" => Key::ArrowLeft,
                        "Right" => Key::ArrowRight,
                        "Up" => Key::ArrowUp,
                        "Down" => Key::ArrowDown,
                        "Home" => Key::Home,
                        "End" => Key::End,
                        other => return Err(format!("unknown key {other}").into()),
                    }
                };
                let handler = focused.as_ref().and_then(|focus| {
                    display
                        .focusables
                        .iter()
                        .find(|region| region.focus.id() == focus.id())
                        .map(|region| Rc::clone(&region.on_key))
                });
                if let Some(handler) = handler {
                    handler(&KeyEvent {
                        key,
                        text: (kind == "type").then(|| argument.to_string()),
                        modifiers: Modifiers::default(),
                        pressed: true,
                    });
                } else {
                    eprintln!("rustle: no focused area to receive {step}");
                }
            }
            other => return Err(format!("unknown input step {other}").into()),
        }
        // Let animations and tasks that wait on frames run to their end: a second of frames.
        for _ in 0..60 {
            clock += std::time::Duration::from_millis(16);
            composition.run_frame(clock);
            composition.recompose();
        }
        display = settle(&mut composition, &mut renderer)?;
    }
    let (width, height, pixels) = renderer.snapshot(&display.commands, 1.0)?;
    let image = ImageData::from_rgba(width, height, pixels).ok_or("bad snapshot size")?;
    std::fs::write(path, image.encode_png()?)?;
    Ok(())
}
