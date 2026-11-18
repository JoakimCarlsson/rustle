//! Drives interfaces inside a host that owns the GPU device and the frame, with no window:
//! clicks through `Embedded`, draws into a texture the host made and checks the pixels.
//!
//! Covers what a host needs: pointer results that tell whether the interface used the event
//! (including areas that block the pointer), input methods, replacing the root, a saver of the
//! host's own, several interfaces on one thread, and a viewport the host draws into.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use rustle::prelude::*;
use rustle::{
    Embedded, ImageData, ImageKey, Key, KeyEvent, Modifiers, Offset, PointerButton, Rect, Saver,
    TextFieldState, basic_text_field, set_saver,
};
use rustle_wgpu::Renderer;

const SIZE: (u32, u32) = (480, 480);
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const HOST_BLUE: wgpu::Color = wgpu::Color {
    r: 0.0,
    g: 0.0,
    b: 1.0,
    a: 1.0,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Where the interface says its areas are, and what it was told.
#[derive(Default)]
struct Spots {
    panel: Cell<Rect>,
    plain: Cell<Rect>,
    field: Cell<Rect>,
    view: Cell<Rect>,
    view_reports: Cell<u32>,
    field_state: RefCell<Option<TextFieldState>>,
}

/// A saver backed by the host's own storage, here a map.
#[derive(Default)]
struct HostStorage(RefCell<HashMap<String, String>>);

impl Saver for HostStorage {
    fn load(&self, key: &str) -> Option<String> {
        self.0.borrow().get(key).cloned()
    }

    fn store(&self, key: &str, value: &str) {
        self.0.borrow_mut().insert(key.into(), value.into());
    }
}

/// A root with a button, a panel that blocks the pointer, one that does not, a text field and
/// a viewport.
fn dashboard(spots: Rc<Spots>, clicks: Rc<Cell<u32>>) -> impl FnMut() {
    move || {
        let count = remember_saveable("count", || 0_i32);
        let field = remember_text_field_state("");
        *spots.field_state.borrow_mut() = Some(field.clone());
        let (spots, clicks) = (Rc::clone(&spots), Rc::clone(&clicks));
        column_with(Modifier::new().padding(24.0), move || {
            text(format!("Count: {}", count.get()));
            let count = count.clone();
            let clicks = Rc::clone(&clicks);
            button("Increment", move || {
                count.update(|n| *n += 1);
                clicks.set(clicks.get() + 1);
            });
            let panel = Rc::clone(&spots);
            stack_with(
                Modifier::new()
                    .size(200.0, 60.0)
                    .on_placed(move |rect, _| panel.panel.set(rect))
                    .blocks_pointer(),
                || text("Panel"),
            );
            let plain = Rc::clone(&spots);
            stack_with(
                Modifier::new()
                    .size(200.0, 40.0)
                    .on_placed(move |rect, _| plain.plain.set(rect)),
                || text("Plain"),
            );
            let placed = Rc::clone(&spots);
            basic_text_field(
                Modifier::new()
                    .size(200.0, 30.0)
                    .on_placed(move |rect, _| placed.field.set(rect)),
                &field,
            );
            let view = Rc::clone(&spots);
            viewport(160.0, 90.0, move |rect| {
                view.view.set(rect);
                view.view_reports.set(view.view_reports.get() + 1);
            });
        });
    }
}

/// A different interface, as a reloaded library would provide.
fn replacement(clicks: Rc<Cell<u32>>) -> impl FnMut() {
    move || {
        let count = remember_saveable("count", || 0_i32);
        let clicks = Rc::clone(&clicks);
        column_with(Modifier::new().padding(24.0), move || {
            text(format!("Reloaded: {}", count.get()));
            let clicks = Rc::clone(&clicks);
            button("Again", move || clicks.set(clicks.get() + 1));
        });
    }
}

fn center(rect: Rect) -> Offset {
    Offset::new(
        rect.origin.x + rect.size.width / 2.0,
        rect.origin.y + rect.size.height / 2.0,
    )
}

fn far_corner(rect: Rect) -> Offset {
    Offset::new(
        rect.origin.x + rect.size.width - 3.0,
        rect.origin.y + rect.size.height - 3.0,
    )
}

fn click(ui: &mut Embedded, at: Offset) -> bool {
    ui.pointer_moved(at);
    let used = ui.pointer_button(PointerButton::Primary, true);
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    used
}

/// The first point in the top rows, left to right, where the pointer is over something.
fn first_interactive(ui: &mut Embedded, rows: u32) -> Option<Offset> {
    (0..rows)
        .step_by(4)
        .flat_map(|y| {
            (0..SIZE.0)
                .step_by(4)
                .map(move |x| Offset::new(x as f32, y as f32))
        })
        .find(|at| ui.pointer_moved(*at))
}

/// Draws `ui` over a texture the host cleared to blue and reads the pixels back.
fn draw(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    ui: &mut Embedded,
    size: (u32, u32),
) -> Result<Vec<u8>> {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("host target"),
        size: wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    ui.render_into(&mut encoder, &view, size, Some(HOST_BLUE))?;
    let padded = (size.0 * 4).div_ceil(256) * 256;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(padded) * u64::from(size.1),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: Some(size.1),
            },
        },
        wgpu::Extent3d {
            width: size.0,
            height: size.1,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (sender, receiver) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = sender.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely())?;
    receiver.recv()??;
    let mapped = slice.get_mapped_range()?;
    Ok(mapped
        .chunks(padded as usize)
        .flat_map(|row| row[..(size.0 * 4) as usize].iter().copied())
        .collect())
}

fn pixel(pixels: &[u8], width: u32, scale: f32, at: Offset) -> [u8; 4] {
    let (x, y) = ((at.x * scale) as usize, (at.y * scale) as usize);
    let start = (y * width as usize + x) * 4;
    [
        pixels[start],
        pixels[start + 1],
        pixels[start + 2],
        pixels[start + 3],
    ]
}

fn is_host_blue(pixel: [u8; 4]) -> bool {
    pixel == [0, 0, 255, 255]
}

/// A picture of one color, `size` pixels square, as a new generation of `key`.
fn live_frame(key: ImageKey, size: u32, color: [u8; 4]) -> Result<ImageData> {
    let pixels: Vec<u8> = (0..size * size).flat_map(|_| color).collect();
    Ok(ImageData::keyed(key, size, size, pixels).ok_or("the pixels fit the size")?)
}

/// A root that shows whichever picture `frame` holds, 64 logical pixels square at the top-left.
fn live_picture(frame: Rc<RefCell<ImageData>>) -> impl FnMut() {
    move || {
        image(
            Modifier::new().size(64.0, 64.0),
            &frame.borrow(),
            ContentScale::Stretch,
        )
    }
}

fn main() -> Result {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let renderer = || Renderer::with_device(device.clone(), queue.clone(), FORMAT, SIZE);

    let storage = Rc::new(HostStorage::default());
    set_saver(storage.clone());

    let spots = Rc::new(Spots::default());
    let clicks = Rc::new(Cell::new(0));
    let mut ui = Embedded::new(
        dashboard(Rc::clone(&spots), Rc::clone(&clicks)),
        renderer(),
        || {},
    );
    ui.resize(SIZE, 1.0);
    ui.update();

    // A click reaches the button, and the pointer is over the interface there.
    let panel = spots.panel.get();
    let on_button = first_interactive(&mut ui, panel.origin.y as u32)
        .ok_or("nothing in the interface reacts to the pointer")?;
    assert!(ui.pointer_button(PointerButton::Primary, true));
    ui.pointer_button(PointerButton::Primary, false);
    ui.update();
    assert_eq!(clicks.get(), 1, "the click reached the button");
    println!("clicked the button at {on_button:?}");
    assert!(!ui.pointer_moved(Offset::new(SIZE.0 as f32 - 1.0, SIZE.1 as f32 - 1.0)));
    assert_eq!(
        storage.load("count").as_deref(),
        Some("1"),
        "remember_saveable stored the value through the host's saver"
    );

    // An area that blocks the pointer counts as used where nothing in it reacts.
    let (blocked, open) = (far_corner(spots.panel.get()), far_corner(spots.plain.get()));
    assert!(
        ui.pointer_moved(blocked),
        "the panel's empty space is the interface's"
    );
    assert!(ui.pointer_button(PointerButton::Primary, true));
    ui.pointer_button(PointerButton::Primary, false);
    assert!(
        ui.wheel(Offset::new(0.0, 10.0)),
        "the wheel over the panel is used"
    );
    assert!(
        !ui.pointer_moved(open),
        "plain space goes through to the host"
    );
    assert!(!ui.pointer_button(PointerButton::Primary, true));
    ui.pointer_button(PointerButton::Primary, false);
    assert!(
        !ui.wheel(Offset::new(0.0, 10.0)),
        "the wheel over plain space is not"
    );
    println!("blocks_pointer: panel {blocked:?} used, plain {open:?} not");

    // An input method composes and commits in a text field, and says where its caret is.
    assert!(!ui.has_focus());
    assert!(click(&mut ui, center(spots.field.get())));
    assert!(ui.has_focus(), "clicking the field focuses it");
    let caret = ui.ime_area().ok_or("a focused field reports its caret")?;
    assert!(
        spots.field.get().origin.y <= caret.origin.y
            && caret.origin.y < spots.field.get().origin.y + spots.field.get().size.height,
        "the caret {caret:?} is inside the field {:?}",
        spots.field.get()
    );
    let field = spots
        .field_state
        .borrow()
        .clone()
        .ok_or("the field exists")?;
    ui.preedit("かな", Some((0, 6)));
    ui.update();
    assert_eq!(field.text(), "", "composing text is not in the value yet");
    assert!(ui.commit("日本"));
    ui.update();
    assert_eq!(field.text(), "日本");
    ui.preedit("", None);
    ui.update();
    assert!(
        ui.ime_area().is_some(),
        "the caret is still reported after a commit"
    );
    assert!(ui.key_event(KeyEvent::press(Key::Escape, Modifiers::default())));
    ui.update();
    assert!(!ui.has_focus() && ui.ime_area().is_none());
    println!(
        "input method: committed {:?}, caret at {caret:?}",
        field.text()
    );

    // A viewport reports its rectangle in logical pixels, and the host's content shows there.
    let view = spots.view.get();
    assert_eq!((view.size.width, view.size.height), (160.0, 90.0));
    let reports = spots.view_reports.get();
    ui.update();
    assert_eq!(
        spots.view_reports.get(),
        reports,
        "it only reports when it moves"
    );
    let pixels = draw(&device, &queue, &mut ui, SIZE)?;
    assert!(
        is_host_blue(pixel(&pixels, SIZE.0, 1.0, center(view))),
        "nothing is drawn in the viewport"
    );
    assert!(
        !is_host_blue(pixel(
            &pixels,
            SIZE.0,
            1.0,
            Offset::new(on_button.x + 20.0, on_button.y + 10.0)
        )),
        "the button is drawn over the host's frame"
    );
    ui.resize((SIZE.0 * 2, SIZE.1 * 2), 2.0);
    ui.update();
    assert_eq!(
        spots.view.get().size,
        view.size,
        "the rectangle is logical, whatever the scale"
    );
    let pixels = draw(&device, &queue, &mut ui, (SIZE.0 * 2, SIZE.1 * 2))?;
    assert!(is_host_blue(pixel(
        &pixels,
        SIZE.0 * 2,
        2.0,
        center(spots.view.get())
    )));
    ui.resize(SIZE, 1.0);
    ui.update();
    println!("viewport at {:?}", spots.view.get());

    // Another interface on the same thread, with a renderer of its own, is independent.
    let other_clicks = Rc::new(Cell::new(0));
    let mut other = Embedded::new(replacement(Rc::clone(&other_clicks)), renderer(), || {});
    other.resize(SIZE, 1.0);
    other.update();
    let at = first_interactive(&mut other, SIZE.1).ok_or("the second interface has a button")?;
    assert!(click(&mut other, at));
    assert_eq!(
        (other_clicks.get(), clicks.get()),
        (1, 1),
        "each has its own state"
    );
    assert!(
        draw(&device, &queue, &mut other, SIZE)?
            .chunks(4)
            .any(|p| !is_host_blue([p[0], p[1], p[2], p[3]]))
    );
    assert!(
        draw(&device, &queue, &mut ui, SIZE)?
            .chunks(4)
            .any(|p| !is_host_blue([p[0], p[1], p[2], p[3]]))
    );
    assert!(
        !rustle::layout_text("text", &TextStyle::default(), None)
            .lines()
            .is_empty(),
        "the shared text engine shapes text"
    );
    println!("two interfaces on one thread");

    // An interface laid out on this thread and drawn elsewhere hands over its commands.
    let outside_clicks = Rc::new(Cell::new(0));
    let mut outside = Embedded::without_renderer(replacement(Rc::clone(&outside_clicks)), || {});
    outside.resize(SIZE, 1.0);
    outside.update();
    assert!(
        !outside.commands().is_empty(),
        "laid out without a renderer"
    );
    let at = first_interactive(&mut outside, SIZE.1).ok_or("it has a button")?;
    assert!(click(&mut outside, at));
    assert_eq!(outside_clicks.get(), 1);
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
    assert!(
        outside
            .render_into(&mut encoder, &view, (1, 1), None)
            .is_err()
    );
    println!("an interface without a renderer lays out and hands over commands");

    // A live picture changes every frame under one key and is drawn with what it holds now.
    let key = ImageKey::new();
    let frame = Rc::new(RefCell::new(live_frame(key, 8, [255, 0, 0, 255])?));
    let mut live = Embedded::new(live_picture(Rc::clone(&frame)), renderer(), || {});
    live.resize(SIZE, 1.0);
    live.update();
    for (size, color) in [
        (8, [255, 0, 0, 255]),
        (8, [0, 255, 0, 255]),
        (16, [255, 255, 255, 255]),
    ] {
        *frame.borrow_mut() = live_frame(key, size, color)?;
        live.invalidate();
        live.update();
        let pixels = draw(&device, &queue, &mut live, SIZE)?;
        assert_eq!(
            pixel(&pixels, SIZE.0, 1.0, Offset::new(32.0, 32.0)),
            color,
            "the newest generation of the key is drawn"
        );
    }
    println!("a keyed picture showed three generations, one of a new size");

    // The root is replaced, keeping the renderer, and the new one starts from the host's storage.
    let again = Rc::new(Cell::new(0));
    ui.set_root(replacement(Rc::clone(&again)));
    ui.update();
    assert!(!ui.has_focus());
    let at = first_interactive(&mut ui, SIZE.1).ok_or("the new root has a button")?;
    assert!(click(&mut ui, at));
    assert_eq!((again.get(), clicks.get()), (1, 1), "the old root is gone");
    let pixels = draw(&device, &queue, &mut ui, SIZE)?;
    assert!(
        pixels
            .chunks(4)
            .any(|p| !is_host_blue([p[0], p[1], p[2], p[3]]))
    );
    println!("root replaced; clicked the new button at {at:?}");

    // An area that draws its own text says where input goes while it has focus.
    let mut terminal = Embedded::without_renderer(terminal, || {});
    terminal.resize(SIZE, 1.0);
    terminal.update();
    assert_eq!(terminal.ime_area(), None, "nothing has focus yet");
    assert!(click(&mut terminal, Offset::new(30.0, 30.0)));
    assert!(terminal.has_focus(), "a press focuses the terminal");
    assert_eq!(
        terminal.ime_area(),
        Some(TERMINAL_CARET),
        "the focused terminal reports its own caret"
    );
    println!("a custom text area reports its caret at {TERMINAL_CARET:?}");
    Ok(())
}

/// Where the terminal of [`terminal`] draws its caret, in the interface.
const TERMINAL_CARET: Rect = Rect {
    origin: Offset { x: 48.0, y: 32.0 },
    size: rustle::Size {
        width: 1.0,
        height: 16.0,
    },
};

/// A root with one area that draws its own text, as a terminal does, and takes focus.
fn terminal() {
    let focus = remember_focus();
    canvas(
        Modifier::new()
            .padding(16.0)
            .size(200.0, 100.0)
            .focusable(&focus, |_| false)
            .ime_caret(Rect::new(
                Offset::new(32.0, 16.0),
                rustle::Size::new(1.0, 16.0),
            )),
        |_| {},
    );
}
