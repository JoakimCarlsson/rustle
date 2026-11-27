//! Whole-frame benchmarks: recording a frame's draw commands and waiting for the GPU to draw
//! them into a texture, with no readback and no presentation, as a window's frame costs
//! before it is shown. A list of rows and a screen of highlighted code are drawn still,
//! scrolled a few pixels a frame (a trackpad), flicked, and at a window size that changes
//! every frame. It needs a GPU adapter and skips its benchmarks when there is none.
//!
//! ```sh
//! CARGO_TARGET_DIR=build cargo bench -p rustle-bench --bench frame
//! ```

use rustle::prelude::*;
use rustle::{Embedded, FontFamily, TextSpan};
use rustle_bench::bench_sequence;
use rustle_graphics::DrawCommand;
use rustle_wgpu::Renderer;

/// The window the frames are drawn in, in physical pixels.
const WINDOW: (u32, u32) = (1600, 1000);

/// How many frames each benchmark measures.
const FRAMES: usize = 300;

/// The format of the texture frames are drawn into, the one windows use.
const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Bgra8Unorm;

/// The words lines of code are made of, with the colour each is highlighted in.
const TOKENS: [(&str, (u8, u8, u8)); 8] = [
    ("fn", (198, 120, 221)),
    ("render_frame", (97, 175, 239)),
    ("(", (171, 178, 191)),
    ("commands", (224, 108, 117)),
    (": &[DrawCommand],", (229, 192, 123)),
    ("scale", (224, 108, 117)),
    (") -> Result<bool> {", (171, 178, 191)),
    ("// keeps what it drew", (92, 99, 112)),
];

/// Emits a list of 600 rows, each a rounded background, a title, a subtitle and a trailing
/// value, moved up by `scrolled` logical pixels.
fn rows(scrolled: f32) {
    column_with(Modifier::new().offset(0.0, -scrolled), || {
        for index in 0..600 {
            row_with(
                Modifier::new()
                    .fill_max_width()
                    .height(40.0)
                    .background_rounded(Color::rgb(30, 30, 36), 6.0)
                    .padding(8.0),
                || {
                    text(format!("Item {index}"));
                    spacer(Modifier::new().weight(1.0));
                    text("subtitle goes here");
                    text(format!("{}", index * 7 % 100));
                },
            );
        }
    });
}

/// Emits a screen of 2,000 lines of highlighted code, each a run of coloured tokens in a
/// monospace font behind a line number, moved up by `scrolled` logical pixels.
fn code(scrolled: f32) {
    let first = (scrolled / 18.0) as usize;
    column_with(
        Modifier::new()
            .fill_max_size()
            .background(Color::rgb(24, 26, 31)),
        || {
            column_with(
                Modifier::new().offset(0.0, first as f32 * 18.0 - scrolled),
                || {
                    for line in first..first + 60 {
                        row_with(Modifier::new().height(18.0).fill_max_width(), || {
                            text_with(Modifier::new().width(48.0), format!("{}", line + 1));
                            let spans: Vec<TextSpan> = (0..12)
                                .map(|token| {
                                    let (word, (r, g, b)) = TOKENS[(line + token) % TOKENS.len()];
                                    TextSpan::new(format!("{word} "))
                                        .color(Color::rgb(r, g, b))
                                        .font_family(FontFamily::Monospace)
                                })
                                .collect();
                            rich_text(spans);
                        });
                    }
                },
            );
        },
    );
}

/// The draw commands `content` emits for each frame, scrolled `step` logical pixels a frame,
/// in a window `size` gives for the frame.
fn frames(
    content: fn(f32),
    step: f32,
    size: impl Fn(usize) -> (u32, u32),
) -> Vec<((u32, u32), Vec<DrawCommand>)> {
    (0..FRAMES)
        .map(|frame| {
            let scrolled = frame as f32 * step;
            let mut embedded = Embedded::without_renderer(move || content(scrolled), || {});
            let size = size(frame);
            embedded.resize(size, 1.0);
            embedded.update();
            (size, embedded.commands().into_owned())
        })
        .collect()
}

/// A device and queue of the default adapter, or `None` when there is no adapter.
fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter =
        pollster(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    pollster(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()
}

/// Runs `future` to its end on this thread, for wgpu's requests, which finish at once.
fn pollster<F: std::future::Future>(future: F) -> F::Output {
    let waker = std::task::Waker::noop();
    let mut context = std::task::Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}

/// A texture of `size` frames are drawn into.
fn target(device: &wgpu::Device, size: (u32, u32)) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&wgpu::TextureViewDescriptor::default())
}

/// Measures drawing each of `frames` in turn, under `name`.
fn measure(name: &str, frames: &[((u32, u32), Vec<DrawCommand>)]) {
    let Some((device, queue)) = device() else {
        eprintln!("frame benchmarks skipped: no GPU adapter");
        return;
    };
    let mut renderer = Renderer::with_device(device.clone(), queue.clone(), FORMAT, WINDOW);
    let mut views: Vec<((u32, u32), wgpu::TextureView)> = Vec::new();
    for (size, _) in frames {
        if !views.iter().any(|(made, _)| made == size) {
            views.push((*size, target(&device, *size)));
        }
    }
    let mut draw = |frame: usize| {
        let (size, commands) = &frames[frame % frames.len()];
        let view = &views.iter().find(|(made, _)| made == size).expect("made").1;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        renderer
            .render_into(
                &mut encoder,
                view,
                *size,
                commands,
                1.0,
                Some(wgpu::Color::BLACK),
            )
            .expect("drawn");
        queue.submit(Some(encoder.finish()));
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("finished");
    };
    for warm in 0..3 {
        draw(warm);
    }
    bench_sequence(name, frames.len(), || (), |(), frame| draw(frame));
}

/// Runs every frame benchmark.
fn main() {
    let still = |_| WINDOW;
    let resizing = |frame: usize| {
        (
            WINDOW.0 - (frame as u32 % 200) * 3,
            WINDOW.1 - (frame as u32 % 150) * 2,
        )
    };
    for (name, content, step, size) in [
        (
            "frame/rows_still",
            rows as fn(f32),
            0.0,
            &still as &dyn Fn(usize) -> (u32, u32),
        ),
        ("frame/rows_trackpad", rows, 8.0, &still),
        ("frame/rows_flick", rows, 120.0, &still),
        ("frame/rows_resizing", rows, 0.0, &resizing),
        ("frame/code_still", code, 0.0, &still),
        ("frame/code_trackpad", code, 8.0, &still),
        ("frame/code_flick", code, 400.0, &still),
        ("frame/code_resizing", code, 0.0, &resizing),
    ] {
        if rustle_bench::selected(name) {
            measure(name, &frames(content, step, size));
        }
    }
}
