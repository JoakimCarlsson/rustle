//! Rounded clips, a chat-style lazy list pinned to its end, and overlay scrollbars.
//!
//! Check it without a window with
//! `RUSTLE_SNAPSHOT=out.png RUSTLE_SNAPSHOT_SIZE=1100x760 cargo run --example graphics`.
//! `GRAPHICS_SCENE=scroll_to` shows the list after `scroll_to_item`, and
//! `GRAPHICS_SCENE=fade` leaves the scrollbar of the plain column to fade, for
//! trying the pointer with `RUSTLE_SNAPSHOT_INPUT="move:1081,250"`.
//! `GRAPHICS_SCENE=effects` shows custom shaders, sweep and radial gradients, blur,
//! color filters, blend modes, drop shadows, frosted glass and `draw_with_content`.

use std::time::Duration;

use rustle::prelude::*;
use rustle::{ItemAlign, ScrollbarStyle, ScrollbarVisibility, lazy_list_keyed};

/// A smooth diagonal picture, so a clip's edge shows against detail.
fn picture() -> ImageData {
    let side = 96_u32;
    let mut pixels = Vec::with_capacity((side * side * 4) as usize);
    for y in 0..side {
        for x in 0..side {
            let checker = ((x / 12 + y / 12) % 2) as u8 * 40;
            pixels.extend_from_slice(&[
                (x * 255 / side) as u8,
                (y * 255 / side) as u8,
                200 - checker,
                255,
            ]);
        }
    }
    ImageData::from_rgba(side, side, pixels).expect("pixels match the size")
}

fn label(content: &str) {
    text_with(Modifier::new().padding(8.0), content);
}

/// The showcase of clips.
fn clips(image: &ImageData) {
    column_with(Modifier::new().padding(12.0), || {
        row_with(Modifier::new(), || {
            // An image and a child fill, cut to a rounded rectangle.
            stack_with(
                Modifier::new()
                    .size(200.0, 200.0)
                    .padding(8.0)
                    .clip_rounded(40.0),
                || {
                    self::image_fill(image);
                    spacer(
                        Modifier::new()
                            .size(120.0, 120.0)
                            .offset(110.0, 110.0)
                            .background(Color::rgba(255, 90, 90, 200)),
                    );
                },
            );
            // A different radius on each corner.
            stack_with(
                Modifier::new()
                    .size(200.0, 200.0)
                    .padding(8.0)
                    .clip_rounded_corners(80.0, 8.0, 80.0, 8.0),
                || {
                    column_with(Modifier::new().fill_max_size(), || {
                        for (index, color) in [
                            Color::rgb(230, 80, 80),
                            Color::rgb(240, 190, 60),
                            Color::rgb(70, 190, 120),
                            Color::rgb(70, 130, 230),
                        ]
                        .into_iter()
                        .enumerate()
                        {
                            stack_with(
                                Modifier::new()
                                    .fill_max_width()
                                    .weight(1.0)
                                    .background(color),
                                || label(&format!("band {index}")),
                            );
                        }
                    });
                },
            );
            // A circle: the usual avatar.
            stack_with(
                Modifier::new()
                    .size(120.0, 120.0)
                    .padding(8.0)
                    .clip_rounded(60.0),
                || self::image_fill(image),
            );
        });
        row_with(Modifier::new(), || {
            // A rounded clip inside another, with a gradient crossing both.
            stack_with(
                Modifier::new()
                    .size(240.0, 200.0)
                    .padding(8.0)
                    .clip_rounded(60.0),
                || {
                    stack_with(
                        Modifier::new().fill_max_size().background_gradient(
                            Color::rgb(250, 120, 60),
                            Color::rgb(80, 60, 220),
                            35.0,
                            0.0,
                        ),
                        || {},
                    );
                    stack_with(
                        Modifier::new()
                            .size(150.0, 150.0)
                            .offset(70.0, 30.0)
                            .clip_rounded(30.0),
                        || self::image_fill(image),
                    );
                },
            );
            // A raised child (z_index) cut by the rounded clip around it, as
            // opposed to one that is allowed to leave it.
            stack_with(
                Modifier::new()
                    .size(200.0, 200.0)
                    .padding(8.0)
                    .background(Color::rgb(40, 40, 52))
                    .clip_rounded(40.0),
                || {
                    stack_with(
                        Modifier::new()
                            .size(160.0, 160.0)
                            .offset(90.0, 90.0)
                            .z_index(1)
                            .background(Color::rgb(90, 200, 160)),
                        || label("z_index, clipped"),
                    );
                },
            );
        });
    });
}

fn image_fill(image: &ImageData) {
    rustle::image(Modifier::new().fill_max_size(), image, ContentScale::Crop);
}

/// A chat of messages of different heights, pinned to its end.
fn chat(count: usize, scene: &str) {
    let state = remember_lazy_list_state();
    state.set_stick_to_end(scene != "scroll_to");
    state.scroll().set_scrollbar(ScrollbarStyle {
        visibility: ScrollbarVisibility::Always,
        ..ScrollbarStyle::default()
    });
    let done = remember(|| false);
    if scene == "scroll_to" && !done.get() {
        done.set(true);
        state.scroll_to_item(120, ItemAlign::Center);
    }
    column_with(
        Modifier::new()
            .size(380.0, 560.0)
            .padding(12.0)
            .background_rounded(Color::rgb(28, 28, 36), 24.0),
        || {
            label(&format!("{count} messages"));
            lazy_list_keyed(
                Modifier::new()
                    .fill_max_width()
                    .weight(1.0)
                    .clip_rounded(16.0),
                &state,
                count,
                |index| index,
                |index| {
                    let lines = 1 + index % 4;
                    let text_lines = (0..lines)
                        .map(|line| format!("message {index}, line {line}"))
                        .collect::<Vec<_>>()
                        .join("\n");
                    column_with(
                        Modifier::new()
                            .fill_max_width()
                            .padding(4.0)
                            .background_rounded(
                                if index % 2 == 0 {
                                    Color::rgb(52, 64, 100)
                                } else {
                                    Color::rgb(60, 60, 70)
                                },
                                10.0,
                            )
                            .padding(8.0),
                        || text(text_lines.clone()),
                    );
                },
            );
        },
    );
}

/// A plain scrolling column with its scrollbar showing.
fn scrolling(scene: &str) {
    let state = remember_scroll_state();
    // The default fades; "fade" keeps it, to try hovering and dragging.
    if scene != "fade" {
        state.set_scrollbar(ScrollbarStyle {
            visibility: ScrollbarVisibility::Always,
            ..ScrollbarStyle::default()
        });
    }
    column_with(
        Modifier::new()
            .size(220.0, 300.0)
            .background_rounded(Color::rgb(28, 28, 36), 16.0)
            .clip_rounded(16.0)
            .vertical_scroll(&state),
        || {
            for index in 0..40 {
                text_with(Modifier::new().padding(6.0), format!("row {index}"));
            }
        },
    );
    // Scrolled partway, to see the thumb away from the start.
    if scene != "fade" && state.offset() == 0.0 && state.viewport_length() > 0.0 {
        state.scroll_to(500.0);
    }
}

/// A moving plasma, written as the WGSL `shade` function a `ShaderBrush` runs.
const PLASMA: &str = "
fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32> {
    let t = uniforms.time * uniforms.params[0].x;
    let p = uv * 6.0;
    let v = sin(p.x + t) + sin(p.y * 1.3 - t) + sin((p.x + p.y) * 0.8 + t * 0.5);
    let r = 0.5 + 0.5 * sin(v * 1.6);
    let g = 0.5 + 0.5 * sin(v * 1.6 + 2.1);
    let b = 0.5 + 0.5 * sin(v * 1.6 + 4.2);
    return vec4<f32>(r, g, b, 1.0);
}
";

/// A tile with a caption under `modifier`'s content, for the effects scene.
fn tile(title: &str, content: impl FnOnce()) {
    column_with(Modifier::new().padding(8.0), || {
        content();
        text_with(Modifier::new().padding(4.0), title);
    });
}

/// The showcase of shaders, gradients and layer effects.
fn effects(image: &ImageData) {
    let seconds =
        infinite_float_repeating(0.0, 600.0, Duration::from_secs(600), Easing::Linear).get();
    column_with(Modifier::new().padding(12.0), || {
        row_with(Modifier::new(), || {
            tile("custom shader", || {
                spacer(
                    Modifier::new().size(180.0, 140.0).shader(
                        ShaderBrush::new(PLASMA)
                            .with_params(&[1.5])
                            .with_time(seconds),
                        24.0,
                    ),
                );
            });
            tile("radial gradient", || {
                spacer(
                    Modifier::new()
                        .size(180.0, 140.0)
                        .background_radial_gradient(
                            Color::rgb(255, 220, 120),
                            Color::rgb(120, 40, 160),
                            (0.35, 0.3),
                            140.0,
                            24.0,
                        ),
                );
            });
            tile("sweep gradient", || {
                spacer(
                    Modifier::new()
                        .size(140.0, 140.0)
                        .background_sweep_gradient(
                            Color::rgb(70, 130, 230),
                            Color::rgb(230, 80, 120),
                            (0.5, 0.5),
                            -90.0,
                            70.0,
                        ),
                );
            });
            tile("drop shadow", || {
                stack_with(Modifier::new().padding(24.0), || {
                    spacer(
                        Modifier::new()
                            .size(120.0, 80.0)
                            .drop_shadow(6.0, 14.0, 28.0, Color::rgba(0, 0, 0, 200), 20.0)
                            .background_rounded(Color::rgb(240, 240, 245), 20.0),
                    );
                });
            });
        });
        row_with(Modifier::new(), || {
            tile("blur(6)", || {
                stack_with(Modifier::new().size(180.0, 140.0).blur(6.0), || {
                    image_fill(image);
                });
            });
            tile("grayscale + tint", || {
                row_with(Modifier::new(), || {
                    stack_with(
                        Modifier::new()
                            .size(86.0, 140.0)
                            .color_filter(ColorFilter::saturation(0.0)),
                        || image_fill(image),
                    );
                    stack_with(
                        Modifier::new()
                            .size(86.0, 140.0)
                            .color_filter(ColorFilter::Tint(Color::rgba(255, 120, 40, 255))),
                        || image_fill(image),
                    );
                });
            });
            tile("multiply / screen", || {
                stack_with(
                    Modifier::new()
                        .size(180.0, 140.0)
                        .background(Color::rgb(230, 200, 90)),
                    || {
                        spacer(
                            Modifier::new()
                                .size(100.0, 100.0)
                                .offset(10.0, 20.0)
                                .blend_mode(BlendMode::Multiply)
                                .background_rounded(Color::rgb(60, 140, 240), 50.0),
                        );
                        spacer(
                            Modifier::new()
                                .size(100.0, 100.0)
                                .offset(70.0, 20.0)
                                .blend_mode(BlendMode::Screen)
                                .background_rounded(Color::rgb(220, 60, 90), 50.0),
                        );
                    },
                );
            });
            tile("frosted glass", || {
                stack_with(Modifier::new().size(180.0, 140.0), || {
                    image_fill(image);
                    spacer(
                        Modifier::new()
                            .size(120.0, 70.0)
                            .offset(30.0, 35.0)
                            .background_blur(10.0, 18.0)
                            .background_rounded(Color::rgba(255, 255, 255, 70), 18.0),
                    );
                });
            });
            tile("draw_with_content", || {
                text_with(
                    Modifier::new()
                        .size(180.0, 140.0)
                        .draw_with_content(|canvas| {
                            let (width, height) = (canvas.width(), canvas.height());
                            canvas.sweep_gradient(
                                0.0,
                                0.0,
                                width,
                                height,
                                16.0,
                                (0.5, 0.5),
                                0.0,
                                Color::rgb(90, 90, 200),
                                Color::rgb(200, 90, 90),
                            );
                            canvas.draw_content();
                            canvas.stroke_rect(
                                2.0,
                                2.0,
                                width - 4.0,
                                height - 4.0,
                                14.0,
                                2.0,
                                Color::rgb(255, 255, 255),
                            );
                        })
                        .padding(28.0),
                    "content",
                );
            });
        });
    });
}

fn app() {
    let scene = std::env::var("GRAPHICS_SCENE").unwrap_or_default();
    let image = remember(picture).get();
    if scene == "effects" {
        column_with(
            Modifier::new()
                .fill_max_size()
                .background(Color::rgb(18, 18, 24)),
            || effects(&image),
        );
        return;
    }
    row_with(
        Modifier::new()
            .fill_max_size()
            .background(Color::rgb(18, 18, 24))
            .padding(12.0),
        || {
            clips(&image);
            chat(300, &scene);
            scrolling(&scene);
        },
    );
}

fn main() {
    rustle::run(app);
}
