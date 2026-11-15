//! Drawing a composable once, without a window.

use std::rc::Rc;

use rustle_foundation::{Focus, Key, KeyEvent, Modifiers};
use rustle_geometry::{Offset, Size};
use rustle_graphics::ImageData;
use rustle_ui::{
    Composition, DisplayList, PointerId, PointerInput, PointerInputPhase, PointerKind,
    PointerSample, PressEvent,
};
use rustle_ui::{KeyCode, PhysicalKey, PointerButton, PointerEvent, PointerPhase, Preedit};
use rustle_unit::Constraints;
use rustle_wgpu::Renderer;

use crate::window::REGISTRY;

/// Composes `root` and draws it once, with no window, into a PNG at `path`.
///
/// The picture is `width` by `height` pixels and uses the dark or the light
/// appearance. State, effects and tasks that wait on frames are not run
/// beyond the first composition. This is the way to see a composable without
/// opening a window, from a build script, a tool or another program.
///
/// `RUSTLE_SNAPSHOT_SCALE` draws it at a scale factor, such as `2`: the layout
/// stays `width` by `height` logical pixels and the picture is that many times
/// larger, as a window on a HiDPI screen draws it.
pub fn preview(
    root: impl FnMut() + 'static,
    (width, height): (u32, u32),
    dark: bool,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    snapshot(root, dark, Some((width, height)), None, path)
}

/// Composes `root` and draws the secondary window it opens with
/// [`window`](crate::window), with no window of its own, into a PNG at `path`.
///
/// `index` counts the windows the first composition opened, in the order they were opened,
/// from 0. The picture is as large as the window's `size` option and uses the dark or the
/// light appearance unless the window's `theme` says otherwise. It fails when root opens
/// fewer windows than that.
pub fn preview_window(
    root: impl FnMut() + 'static,
    index: usize,
    dark: bool,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    snapshot(root, dark, None, Some(index), path)
}

/// The latest content of the secondary window at `index`, in the order they were opened.
fn window_tree(index: usize) -> Option<rustle_ui::Node> {
    REGISTRY.with(|r| r.borrow().entries.values().nth(index)?.tree.clone())
}

/// The `size` option of the secondary window at `index`, in whole pixels.
fn window_size(index: usize) -> Option<(u32, u32)> {
    REGISTRY.with(|r| {
        let size = r.borrow().entries.values().nth(index)?.options.size;
        Some((size.0.max(1.0) as u32, size.1.max(1.0) as u32))
    })
}

/// The scale factor snapshots are drawn at: `RUSTLE_SNAPSHOT_SCALE`, such as `2` or `1.5`,
/// or 1 when it is unset or not a positive number. The layout keeps its logical size and
/// the picture has that many physical pixels per logical pixel, as on a HiDPI screen.
fn snapshot_scale() -> f32 {
    std::env::var("RUSTLE_SNAPSHOT_SCALE")
        .ok()
        .and_then(|value| value.trim().parse::<f32>().ok())
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0)
}

/// Composes `root` and draws it, or its secondary window `window`, off screen, into a PNG at
/// `path`. Without a `size` the window's own size is used.
pub(crate) fn snapshot(
    mut root: impl FnMut() + 'static,
    dark: bool,
    size: Option<(u32, u32)>,
    window: Option<usize>,
    path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    REGISTRY.with(|r| r.borrow_mut().entries.clear());
    let mut composition = rustle_ui::composition(move || {
        rustle_foundation::LOCAL_SYSTEM_DARK.provide(dark, &mut root);
    });
    composition.compose();
    let (width, height) = match (size, window) {
        (Some(size), _) => size,
        (None, Some(index)) => window_size(index).ok_or("the root opened no such window")?,
        (None, None) => (1280, 800),
    };
    let scale = snapshot_scale();
    let mut renderer = Renderer::offscreen(
        (width as f32 * scale).round().max(1.0) as u32,
        (height as f32 * scale).round().max(1.0) as u32,
    )?;
    let constraints = Constraints::loose(Size::new(width as f32, height as f32));
    // Laying out can change state, such as scrolling a caret into view, which
    // composes and lays out again, as the event loop does. A few rounds settle it.
    let settle = |composition: &mut Composition,
                  renderer: &mut Renderer|
     -> Result<DisplayList, Box<dyn std::error::Error>> {
        let mut display = None;
        for _ in 0..8 {
            let tree = match window {
                Some(index) => window_tree(index).ok_or("the window left the composition")?,
                None => composition
                    .output()
                    .cloned()
                    .ok_or("nothing was composed")?,
            };
            display = Some(rustle_ui::display_list(&tree, constraints, renderer));
            if !composition.recompose() {
                break;
            }
        }
        display.ok_or_else(|| "nothing was laid out".into())
    };
    let mut display = settle(&mut composition, &mut renderer)?;

    // Replays input against the laid-out UI, step by step, as the event loop
    // would handle it: RUSTLE_SNAPSHOT_INPUT="click:x,y;move:x,y;type:text;key:Backspace".
    //
    // Steps: `click:x,y`, `rclick:x,y` (secondary), `mclick:x,y` (middle), `backclick:x,y`,
    // `forwardclick:x,y`; `move:x,y`; `type:text`; `key:Name` (logical: a letter, or Backspace,
    // Delete, Enter, Tab, Escape, Left, Right, Up, Down, Home, End, PageUp, PageDown, Insert,
    // PrintScreen, ContextMenu, Pause, CapsLock, F1 to F35 ...); `code:KeyA` (a physical key, by
    // its `KeyCode` name, such as KeyA, F5 or ArrowLeft); `release:Name` (a key release);
    // `mod:ctrl+shift` (the modifiers held for the steps after, from ctrl, shift, alt, meta;
    // `mod:none` or `mod:` releases them); `preedit:text` (an input method composing `text` at
    // the caret of the focused field; `preedit:` ends it, `commit:text` types the final text);
    // `wait:ms` (runs `ms` milliseconds of frames, to catch an animation midway).
    // Each other step is followed by RUSTLE_SNAPSHOT_STEP_MS of frames, 1000 unless set; use 0
    // to look at the state right after the step, then `wait:ms`.
    let mut focused: Option<Focus> = None;
    let mut modifiers = Modifiers::default();
    let mut clock = std::time::Duration::ZERO;
    let steps = std::env::var("RUSTLE_SNAPSHOT_INPUT").unwrap_or_default();
    let step_ms: u64 = std::env::var("RUSTLE_SNAPSHOT_STEP_MS")
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(1000);
    for step in steps.split(';').filter(|step| !step.is_empty()) {
        let (kind, argument) = step.split_once(':').unwrap_or((step, ""));
        match kind {
            "click" | "rclick" | "mclick" | "backclick" | "forwardclick" => {
                let button = match kind {
                    "click" => PointerButton::Primary,
                    "rclick" => PointerButton::Secondary,
                    "mclick" => PointerButton::Middle,
                    "backclick" => PointerButton::Back,
                    _ => PointerButton::Forward,
                };
                let (x, y) = argument.split_once(',').ok_or("click needs x,y")?;
                let at = Offset::new(x.trim().parse()?, y.trim().parse()?);
                if button == PointerButton::Primary {
                    let next = display.focus_at(at).map(|region| region.focus.clone());
                    if focused.as_ref().map(Focus::id) != next.as_ref().map(Focus::id) {
                        if let Some(old) = &focused {
                            old.set_focused(false);
                            old.set_preedit(None);
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
                            button,
                            modifiers,
                        });
                    }
                    if let Some(region) = display.hit_test(at) {
                        (Rc::clone(&region.on_click))();
                    }
                }
                if let Some(region) = display.button_at(at, button) {
                    let on_event = Rc::clone(&region.on_event);
                    let event = |phase| PointerEvent {
                        button,
                        phase,
                        position: Offset::new(at.x - region.origin.x, at.y - region.origin.y),
                        window_position: at,
                        modifiers,
                        click_count: 1,
                    };
                    for phase in [
                        PointerPhase::Press,
                        PointerPhase::Release,
                        PointerPhase::Click,
                    ] {
                        on_event(&event(phase));
                    }
                }
            }
            "mod" => {
                modifiers = Modifiers::default();
                for name in argument.split('+').map(str::trim).filter(|n| !n.is_empty()) {
                    match name {
                        "ctrl" => modifiers.ctrl = true,
                        "shift" => modifiers.shift = true,
                        "alt" => modifiers.alt = true,
                        "meta" | "super" => modifiers.meta = true,
                        "none" => {}
                        other => return Err(format!("unknown modifier {other}").into()),
                    }
                }
                continue;
            }
            "preedit" | "commit" => {
                if let Some(focus) = &focused {
                    let cursor = argument.len();
                    focus.set_preedit((kind == "preedit" && !argument.is_empty()).then(|| {
                        Preedit {
                            text: argument.to_string(),
                            cursor: Some((cursor, cursor)),
                        }
                    }));
                    if kind == "commit" {
                        let event = KeyEvent::typed(argument, modifiers);
                        display.dispatch_key(Some(focus), &event);
                    }
                } else {
                    eprintln!("rustle: no focused area to receive {step}");
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
            "drag" => {
                let numbers: Vec<f32> = argument
                    .split(',')
                    .map(|number| number.trim().parse())
                    .collect::<Result<_, _>>()?;
                let [x1, y1, x2, y2] = numbers[..] else {
                    return Err("drag needs x1,y1,x2,y2".into());
                };
                replay_drag(
                    &display,
                    Offset::new(x1, y1),
                    Offset::new(x2, y2),
                    modifiers,
                );
            }
            "type" | "key" | "code" | "release" => {
                let mut physical_key = PhysicalKey::Unidentified;
                let key = match kind {
                    "type" => Key::Character(argument.to_string()),
                    "code" => {
                        let code = KeyCode::from_name(argument)
                            .ok_or_else(|| format!("unknown key code {argument}"))?;
                        physical_key = PhysicalKey::Code(code);
                        key_for_code(code)
                    }
                    _ => logical_key(argument).ok_or_else(|| format!("unknown key {argument}"))?,
                };
                if let Some(code) = KeyCode::from_name(argument) {
                    physical_key = PhysicalKey::Code(code);
                }
                let event = KeyEvent {
                    text: (kind == "type").then(|| argument.to_string()),
                    physical_key,
                    pressed: kind != "release",
                    modifiers,
                    ..KeyEvent::press(key, modifiers)
                };
                if !display.dispatch_key(focused.as_ref(), &event) {
                    // The defaults the event loop would do.
                    if event.pressed && event.key == Key::Escape {
                        if let Some(old) = focused.take() {
                            old.set_focused(false);
                        }
                    } else if focused.is_none() && display.key_handlers.is_empty() {
                        eprintln!("rustle: no focused area to receive {step}");
                    }
                }
            }
            "wait" => {
                let milliseconds: u64 = argument.trim().parse()?;
                advance_frames(&mut composition, &mut clock, milliseconds);
                display = settle(&mut composition, &mut renderer)?;
                continue;
            }
            other => return Err(format!("unknown input step {other}").into()),
        }
        advance_frames(&mut composition, &mut clock, step_ms);
        display = settle(&mut composition, &mut renderer)?;
    }
    let (width, height, pixels) = renderer.snapshot(&display.commands, scale)?;
    let image = ImageData::from_rgba(width, height, pixels).ok_or("bad snapshot size")?;
    std::fs::write(path, image.encode_png()?)?;
    Ok(())
}

/// Drags the mouse from `from` to `to` over the areas that watch the raw pointer stream
/// (`Modifier::pointer_input` and the gestures built on it): a press, ten moves 16 ms apart
/// and a release, delivered to the topmost such area under `from`.
fn replay_drag(display: &DisplayList, from: Offset, to: Offset, modifiers: Modifiers) {
    let Some(region) = display.pointer_inputs_at(from).next() else {
        eprintln!("rustle: no pointer-input area at {from:?} for a drag");
        return;
    };
    let on_event = Rc::clone(&region.on_event);
    let origin = region.origin;
    let began = std::time::Instant::now();
    let steps = 10;
    let event = |phase, at: Offset, step: u32| {
        let sample = PointerSample {
            id: PointerId::MOUSE,
            position: Offset::new(at.x - origin.x, at.y - origin.y),
            window_position: at,
            pressure: 0.5,
        };
        PointerInput {
            id: PointerId::MOUSE,
            kind: PointerKind::Mouse,
            phase,
            button: PointerButton::Primary,
            position: sample.position,
            window_position: at,
            pressure: 0.5,
            scroll_delta: Offset::default(),
            modifiers,
            pointers: if phase == PointerInputPhase::Up {
                Vec::new()
            } else {
                vec![sample]
            },
            time: began + std::time::Duration::from_millis(16 * u64::from(step)),
        }
    };
    on_event(&event(PointerInputPhase::Down, from, 0));
    for step in 1..=steps {
        let t = step as f32 / steps as f32;
        let at = Offset::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t);
        on_event(&event(PointerInputPhase::Move, at, step));
    }
    on_event(&event(PointerInputPhase::Up, to, steps));
}

/// Runs `milliseconds` of frames, about 16 ms apart, moving `clock` along, so animations and
/// tasks that wait on frames progress as they would on screen. The last frame is shorter when
/// `milliseconds` is not a multiple of 16.
fn advance_frames(
    composition: &mut Composition,
    clock: &mut std::time::Duration,
    milliseconds: u64,
) {
    let end = *clock + std::time::Duration::from_millis(milliseconds);
    while *clock < end {
        *clock = (*clock + std::time::Duration::from_millis(16)).min(end);
        composition.run_frame(*clock);
        composition.recompose();
    }
}

/// The logical key a replay step names.
fn logical_key(name: &str) -> Option<Key> {
    Some(match name {
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Enter" => Key::Enter,
        "Tab" => Key::Tab,
        "Escape" => Key::Escape,
        "Left" | "ArrowLeft" => Key::ArrowLeft,
        "Right" | "ArrowRight" => Key::ArrowRight,
        "Up" | "ArrowUp" => Key::ArrowUp,
        "Down" | "ArrowDown" => Key::ArrowDown,
        "Home" => Key::Home,
        "End" => Key::End,
        "PageUp" => Key::PageUp,
        "PageDown" => Key::PageDown,
        "Insert" => Key::Insert,
        "PrintScreen" => Key::PrintScreen,
        "ContextMenu" => Key::ContextMenu,
        "Pause" => Key::Pause,
        "Help" => Key::Help,
        "Clear" => Key::Clear,
        "CapsLock" => Key::CapsLock,
        "NumLock" => Key::NumLock,
        "ScrollLock" => Key::ScrollLock,
        "Shift" => Key::Shift,
        "Control" => Key::Control,
        "Alt" => Key::Alt,
        "Super" => Key::Super,
        "Space" => Key::Character(" ".to_string()),
        function if function.starts_with('F') && function.len() > 1 => {
            Key::F(function[1..].parse().ok()?)
        }
        single if single.chars().count() == 1 => Key::Character(single.to_string()),
        _ => return None,
    })
}

/// The logical key a physical key types on a US layout, for replays.
fn key_for_code(code: KeyCode) -> Key {
    let name = code.name();
    if let Some(letter) = name.strip_prefix("Key") {
        return Key::Character(letter.to_lowercase());
    }
    if let Some(digit) = name.strip_prefix("Digit") {
        return Key::Character(digit.to_string());
    }
    match name {
        "Space" => Key::Character(" ".to_string()),
        "ShiftLeft" | "ShiftRight" => Key::Shift,
        "ControlLeft" | "ControlRight" => Key::Control,
        "AltLeft" | "AltRight" => Key::Alt,
        "SuperLeft" | "SuperRight" => Key::Super,
        other => logical_key(other).unwrap_or(Key::Other),
    }
}
