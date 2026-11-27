//! Headless render tests: draw commands into a picture and compare pixels at probe points.
//!
//! Each test skips itself when the machine has no GPU adapter.

use rustle_geometry::{Offset, Rect, Size};
use rustle_graphics::{
    BlendMode, Color, ColorFilter, DrawCommand, FontFamily, ImageData, ImageKey, LayerEffect, Path,
    PathCommand, PixelFormat, PixelRect, ShaderBrush, SvgPainter, TextAlign, TextWrap,
};

use crate::Renderer;

/// The color the renderer clears to, as the picture holds it.
const BACKGROUND: [u8; 4] = [24, 24, 30, 255];

/// A picture a test drew, with probes at pixel positions.
struct Picture {
    width: u32,
    pixels: Vec<u8>,
}

impl Picture {
    /// The RGBA bytes at (`x`, `y`).
    fn at(&self, x: u32, y: u32) -> [u8; 4] {
        let start = ((y * self.width + x) * 4) as usize;
        self.pixels[start..start + 4].try_into().unwrap()
    }

    /// Asserts the pixel at (`x`, `y`) is within `tolerance` of `expected` on every channel.
    #[track_caller]
    fn expect(&self, x: u32, y: u32, expected: [u8; 4], tolerance: u8) {
        let actual = self.at(x, y);
        let close = actual
            .iter()
            .zip(expected)
            .all(|(a, e)| a.abs_diff(e) <= tolerance);
        assert!(
            close,
            "pixel ({x}, {y}) is {actual:?}, expected {expected:?} within {tolerance}"
        );
    }
}

/// A renderer of `size` square, or `None` without a GPU.
fn renderer(size: u32) -> Option<Renderer> {
    Renderer::offscreen(size, size).ok()
}

/// Draws `commands` at scale 1 with `renderer` and returns the picture.
fn draw(renderer: &mut Renderer, commands: &[DrawCommand]) -> Picture {
    let (width, _, pixels) = renderer.snapshot(commands, 1.0).unwrap();
    Picture { width, pixels }
}

/// A rectangle from its edges.
fn area(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(Offset::new(x, y), Size::new(width, height))
}

/// A filled rectangle command.
fn fill(rect: Rect, color: Color) -> DrawCommand {
    DrawCommand::Rect {
        rect,
        color,
        corner_radius: 0.0,
        border: None,
    }
}

const RED: Color = Color::rgb(255, 0, 0);
const GREEN: Color = Color::rgb(0, 255, 0);
const BLUE: Color = Color::rgb(0, 0, 255);

#[test]
fn a_rectangle_is_drawn_where_it_is() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(&mut renderer, &[fill(area(8.0, 8.0, 16.0, 16.0), RED)]);
    picture.expect(16, 16, [255, 0, 0, 255], 2);
    picture.expect(4, 4, BACKGROUND, 2);
    picture.expect(40, 40, BACKGROUND, 2);
}

#[test]
fn a_transparent_renderer_leaves_what_is_not_drawn_see_through() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    renderer.set_transparent(true);
    let picture = draw(&mut renderer, &[fill(area(8.0, 8.0, 16.0, 16.0), RED)]);
    picture.expect(16, 16, [255, 0, 0, 255], 2);
    picture.expect(40, 40, [0, 0, 0, 0], 2);
    renderer.set_transparent(false);
    let picture = draw(&mut renderer, &[]);
    picture.expect(40, 40, BACKGROUND, 2);
}

#[test]
fn a_shader_that_does_not_compile_is_skipped_and_the_frame_goes_on() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let broken = ShaderBrush::new(
        "fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32> { return nonsense; }",
    );
    let good = ShaderBrush::new(
        "fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32> { return vec4<f32>(0.0, 0.0, 1.0, 1.0); }",
    );
    let commands = [
        DrawCommand::Shader {
            rect: area(0.0, 0.0, 32.0, 32.0),
            corner_radius: 0.0,
            brush: broken,
        },
        DrawCommand::Shader {
            rect: area(32.0, 32.0, 32.0, 32.0),
            corner_radius: 0.0,
            brush: good,
        },
        fill(area(32.0, 0.0, 16.0, 16.0), RED),
    ];
    for _ in 0..2 {
        let picture = draw(&mut renderer, &commands);
        picture.expect(16, 16, BACKGROUND, 2);
        picture.expect(48, 48, [0, 0, 255, 255], 2);
        picture.expect(40, 8, [255, 0, 0, 255], 2);
    }
}

#[test]
fn a_shader_is_drawn_with_its_uniforms() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let brush = ShaderBrush::new(
        "fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32> { return vec4<f32>(uniforms.params[0].x, uv.x, 0.0, 1.0); }",
    )
    .with_params(&[1.0]);
    let picture = draw(
        &mut renderer,
        &[DrawCommand::Shader {
            rect: area(0.0, 0.0, 64.0, 64.0),
            corner_radius: 0.0,
            brush,
        }],
    );
    picture.expect(2, 32, [255, 10, 0, 255], 12);
    picture.expect(62, 32, [255, 250, 0, 255], 12);
}

#[test]
fn a_rounded_clip_cuts_corners_and_keeps_the_middle() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushRoundedClip {
                rect: area(8.0, 8.0, 48.0, 48.0),
                radii: [16.0; 4],
            },
            fill(area(0.0, 0.0, 64.0, 64.0), RED),
            DrawCommand::PopClip,
        ],
    );
    picture.expect(32, 32, [255, 0, 0, 255], 2);
    picture.expect(9, 9, BACKGROUND, 2);
    picture.expect(32, 9, [255, 0, 0, 255], 2);
    picture.expect(4, 32, BACKGROUND, 2);
    picture.expect(60, 60, BACKGROUND, 2);
}

#[test]
fn rounded_clips_nest() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushRoundedClip {
                rect: area(0.0, 0.0, 64.0, 64.0),
                radii: [8.0; 4],
            },
            DrawCommand::PushRoundedClip {
                rect: area(16.0, 16.0, 32.0, 32.0),
                radii: [16.0; 4],
            },
            fill(area(0.0, 0.0, 64.0, 64.0), RED),
            DrawCommand::PopClip,
            fill(area(0.0, 0.0, 8.0, 8.0), GREEN),
            DrawCommand::PopClip,
        ],
    );
    picture.expect(32, 32, [255, 0, 0, 255], 2);
    picture.expect(17, 17, BACKGROUND, 2);
    picture.expect(6, 6, [0, 255, 0, 255], 2);
    picture.expect(0, 0, BACKGROUND, 2);
    picture.expect(60, 60, BACKGROUND, 2);
}

#[test]
fn a_rectangular_clip_inside_a_rounded_clip_applies_both() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushRoundedClip {
                rect: area(0.0, 0.0, 64.0, 64.0),
                radii: [32.0; 4],
            },
            DrawCommand::PushClip {
                rect: area(0.0, 0.0, 32.0, 64.0),
            },
            fill(area(0.0, 0.0, 64.0, 64.0), RED),
            DrawCommand::PopClip,
            DrawCommand::PopClip,
        ],
    );
    picture.expect(16, 32, [255, 0, 0, 255], 2);
    picture.expect(48, 32, BACKGROUND, 2);
    picture.expect(1, 1, BACKGROUND, 2);
}

#[test]
fn a_layer_fades_everything_inside_it_as_one() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushLayer {
                rect: area(0.0, 0.0, 64.0, 64.0),
                alpha: 0.5,
                rotation: 0.0,
                scale: 1.0,
            },
            fill(area(0.0, 0.0, 40.0, 40.0), RED),
            fill(area(20.0, 20.0, 40.0, 40.0), BLUE),
            DrawCommand::PopLayer,
        ],
    );
    let overlap = picture.at(30, 30);
    assert!(overlap[2] > 100 && overlap[0] < 60, "{overlap:?}");
    let only_red = picture.at(5, 5);
    assert!(only_red[0] > 100 && only_red[0] < 200, "{only_red:?}");
}

#[test]
fn an_effect_nests_inside_a_rounded_clip_and_a_layer() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushRoundedClip {
                rect: area(0.0, 0.0, 64.0, 64.0),
                radii: [24.0; 4],
            },
            DrawCommand::PushLayer {
                rect: area(0.0, 0.0, 64.0, 64.0),
                alpha: 1.0,
                rotation: 0.0,
                scale: 1.0,
            },
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect {
                    color_filter: Some(ColorFilter::saturation(0.0)),
                    ..LayerEffect::default()
                },
            },
            fill(area(0.0, 0.0, 64.0, 64.0), RED),
            DrawCommand::PopEffect,
            DrawCommand::PopLayer,
            DrawCommand::PopClip,
        ],
    );
    let middle = picture.at(32, 32);
    assert!(
        middle[0].abs_diff(middle[1]) < 4 && middle[0] > 20,
        "{middle:?}"
    );
    picture.expect(1, 1, BACKGROUND, 2);
}

#[test]
fn an_effect_inside_an_effect_composes_both() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect {
                    alpha: 0.5,
                    ..LayerEffect::default()
                },
            },
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect {
                    alpha: 0.5,
                    ..LayerEffect::default()
                },
            },
            fill(area(0.0, 0.0, 64.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::PopEffect,
            DrawCommand::PopEffect,
        ],
    );
    let pixel = picture.at(32, 32);
    assert!(
        pixel[0].abs_diff(81) < 8,
        "quarter strength in stored values: {pixel:?}"
    );
}

#[test]
fn a_blend_mode_combines_with_what_is_below() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 64.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect {
                    blend_mode: BlendMode::Multiply,
                    ..LayerEffect::default()
                },
            },
            fill(area(0.0, 0.0, 32.0, 64.0), RED),
            DrawCommand::PopEffect,
        ],
    );
    picture.expect(16, 32, [255, 0, 0, 255], 3);
    picture.expect(48, 32, [255, 255, 255, 255], 3);
}

#[test]
fn a_blurred_layer_spreads_and_is_cut_to_its_rectangle() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 40.0, 64.0),
                effect: LayerEffect {
                    blur: 4.0,
                    ..LayerEffect::default()
                },
            },
            fill(area(20.0, 20.0, 24.0, 24.0), Color::rgb(255, 255, 255)),
            DrawCommand::PopEffect,
        ],
    );
    let center = picture.at(32, 32);
    assert!(center[0] > 230, "{center:?}");
    let edge = picture.at(20, 32);
    assert!(edge[0] > 90 && edge[0] < 220, "the edge is soft: {edge:?}");
    let outside_inside_rect = picture.at(12, 32);
    assert!(outside_inside_rect[0] > 24, "{outside_inside_rect:?}");
    picture.expect(50, 32, BACKGROUND, 2);
}

#[test]
fn a_blur_at_the_window_edge_does_not_darken_it() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect {
                    blur: 6.0,
                    ..LayerEffect::default()
                },
            },
            fill(area(0.0, 0.0, 64.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::PopEffect,
        ],
    );
    for (x, y) in [(0, 0), (63, 63), (0, 32), (32, 0), (63, 32), (32, 63)] {
        picture.expect(x, y, [255, 255, 255, 255], 6);
    }
}

#[test]
fn a_backdrop_blur_softens_what_is_below_and_keeps_the_rest() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 32.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::BackdropBlur {
                rect: area(16.0, 16.0, 32.0, 32.0),
                corner_radius: 0.0,
                blur: 4.0,
            },
        ],
    );
    let seam = picture.at(32, 32);
    assert!(seam[0] > 70 && seam[0] < 200, "{seam:?}");
    picture.expect(8, 8, [255, 255, 255, 255], 2);
    picture.expect(56, 56, BACKGROUND, 2);
    picture.expect(17, 20, [255, 255, 255, 255], 40);
}

#[test]
fn a_backdrop_blur_in_a_rounded_clip_is_cut_to_it() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 32.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::PushRoundedClip {
                rect: area(16.0, 16.0, 32.0, 32.0),
                radii: [16.0; 4],
            },
            DrawCommand::BackdropBlur {
                rect: area(16.0, 16.0, 32.0, 32.0),
                corner_radius: 16.0,
                blur: 4.0,
            },
            DrawCommand::PopClip,
        ],
    );
    picture.expect(17, 17, [255, 255, 255, 255], 2);
    picture.expect(46, 17, BACKGROUND, 2);
}

#[test]
fn a_backdrop_blur_next_to_the_window_edge_keeps_the_edge_color() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 64.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::BackdropBlur {
                rect: area(0.0, 0.0, 32.0, 32.0),
                corner_radius: 0.0,
                blur: 6.0,
            },
        ],
    );
    picture.expect(0, 0, [255, 255, 255, 255], 6);
    picture.expect(31, 31, [255, 255, 255, 255], 6);
}

#[test]
fn many_rounded_clips_and_effects_draw_correctly() {
    let Some(mut renderer) = renderer(128) else {
        return;
    };
    let mut commands = Vec::new();
    for column in 0..8 {
        for row in 0..8 {
            let rect = area(
                column as f32 * 16.0 + 1.0,
                row as f32 * 16.0 + 1.0,
                14.0,
                14.0,
            );
            commands.push(DrawCommand::PushRoundedClip {
                rect,
                radii: [4.0; 4],
            });
            commands.push(DrawCommand::PushEffect {
                rect,
                effect: LayerEffect {
                    alpha: 0.5,
                    ..LayerEffect::default()
                },
            });
            commands.push(fill(rect, GREEN));
            commands.push(DrawCommand::PopEffect);
            commands.push(DrawCommand::PopClip);
        }
    }
    let picture = draw(&mut renderer, &commands);
    for column in 0..8 {
        for row in 0..8 {
            let (x, y) = (column * 16 + 8, row * 16 + 8);
            let pixel = picture.at(x, y);
            assert!(pixel[1] > 100 && pixel[0] < 30, "({x}, {y}) {pixel:?}");
            picture.expect(column * 16 + 1, row * 16 + 1, BACKGROUND, 6);
        }
    }
}

#[test]
fn a_layer_texture_is_reused_between_frames() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let commands = [
        DrawCommand::PushLayer {
            rect: area(0.0, 0.0, 32.0, 32.0),
            alpha: 1.0,
            rotation: 0.0,
            scale: 1.0,
        },
        fill(area(0.0, 0.0, 32.0, 32.0), RED),
        DrawCommand::PopLayer,
    ];
    draw(&mut renderer, &commands);
    let made = renderer.layer_textures_made();
    for _ in 0..4 {
        draw(&mut renderer, &commands);
    }
    assert_eq!(renderer.layer_textures_made(), made);
}

#[test]
fn a_backdrop_blur_inside_a_rounded_clip_blurs_what_is_below_the_clip() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 32.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::PushRoundedClip {
                rect: area(8.0, 8.0, 48.0, 48.0),
                radii: [8.0; 4],
            },
            DrawCommand::BackdropBlur {
                rect: area(8.0, 8.0, 48.0, 48.0),
                corner_radius: 8.0,
                blur: 4.0,
            },
            DrawCommand::PopClip,
        ],
    );
    let left = picture.at(31, 32);
    let right = picture.at(32, 32);
    assert!(
        left[0] < 240 && right[0] > 40,
        "the seam is soft: {left:?} {right:?}"
    );
}

#[test]
fn a_backdrop_blur_inside_an_effect_blurs_what_is_below_it() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 32.0, 64.0), Color::rgb(255, 255, 255)),
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect::default(),
            },
            DrawCommand::BackdropBlur {
                rect: area(8.0, 8.0, 48.0, 48.0),
                corner_radius: 0.0,
                blur: 4.0,
            },
            DrawCommand::PopEffect,
        ],
    );
    let left = picture.at(31, 32);
    assert!(left[0] < 240, "the seam is soft: {left:?}");
}

/// A line of white text at `x`, `y`.
fn words(x: f32, y: f32) -> DrawCommand {
    DrawCommand::Text {
        x,
        y,
        width: 100.0,
        content: "Rustle".into(),
        color: Color::rgb(255, 255, 255),
        font_size: 20.0,
        line_height: 24.0,
        font_weight: 700,
        font_family: FontFamily::SansSerif,
        align: TextAlign::Start,
        max_lines: Some(1),
        italic: false,
        letter_spacing: 0.0,
        ligatures: true,
        wrap: TextWrap::default(),
    }
}

/// How many pixels of `picture` inside `area` differ from `other` by more than `tolerance`.
fn differing(
    picture: &Picture,
    other: &Picture,
    area: (u32, u32, u32, u32),
    tolerance: u8,
) -> usize {
    let mut count = 0;
    for y in area.1..area.1 + area.3 {
        for x in area.0..area.0 + area.2 {
            let close = picture
                .at(x, y)
                .iter()
                .zip(other.at(x, y))
                .all(|(a, b)| a.abs_diff(b) <= tolerance);
            count += usize::from(!close);
        }
    }
    count
}

#[test]
fn what_a_clip_holds_is_drawn_where_it_would_be_without_the_clip() {
    let Some(mut renderer) = renderer(128) else {
        return;
    };
    let star = Path::new(
        vec![
            PathCommand::MoveTo(0.0, 0.0),
            PathCommand::LineTo(20.0, 0.0),
            PathCommand::LineTo(10.0, 20.0),
            PathCommand::Close,
        ],
        20.0,
        20.0,
        Default::default(),
    );
    let picture_data = ImageData::from_rgba(
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
        ],
    )
    .unwrap();
    let content = |commands: &mut Vec<DrawCommand>| {
        commands.push(fill(area(60.0, 60.0, 50.0, 50.0), Color::rgb(40, 90, 200)));
        commands.push(words(66.0, 64.0));
        commands.push(DrawCommand::Path {
            path: star.clone(),
            rect: area(70.0, 90.0, 20.0, 20.0),
            color: GREEN,
        });
        commands.push(DrawCommand::Image {
            image: picture_data.clone(),
            rect: area(94.0, 90.0, 12.0, 12.0),
            corner_radius: 0.0,
            opacity: 1.0,
        });
        commands.push(DrawCommand::Line {
            from: (62.0, 106.0),
            to: (92.0, 106.0),
            width: 2.0,
            color: RED,
        });
        commands.push(DrawCommand::Shader {
            rect: area(94.0, 104.0, 10.0, 4.0),
            corner_radius: 0.0,
            brush: ShaderBrush::new(
                "fn shade(uv: vec2<f32>, position: vec2<f32>) -> vec4<f32> { return vec4<f32>(1.0, 0.0, 1.0, 1.0); }",
            ),
        });
    };
    let mut plain = Vec::new();
    content(&mut plain);
    let mut clipped = vec![DrawCommand::PushRoundedClip {
        rect: area(56.0, 56.0, 60.0, 60.0),
        radii: [6.0; 4],
    }];
    content(&mut clipped);
    clipped.push(DrawCommand::PopClip);
    let mut layered = vec![DrawCommand::PushLayer {
        rect: area(56.0, 56.0, 60.0, 60.0),
        alpha: 1.0,
        rotation: 0.0,
        scale: 1.0,
    }];
    content(&mut layered);
    layered.push(DrawCommand::PopLayer);
    let expected = draw(&mut renderer, &plain);
    let inside = (60, 60, 50, 50);
    for commands in [&clipped, &layered] {
        let picture = draw(&mut renderer, commands);
        assert_eq!(differing(&picture, &expected, inside, 3), 0);
    }
}

#[test]
fn a_clipped_layer_away_from_the_origin_holds_all_it_draws_at_scale_two() {
    let Some(mut renderer) = renderer(128) else {
        return;
    };
    let commands = [
        DrawCommand::PushRoundedClip {
            rect: area(20.0, 20.0, 30.0, 30.0),
            radii: [10.0; 4],
        },
        fill(area(0.0, 0.0, 128.0, 128.0), RED),
        words(24.0, 24.0),
        DrawCommand::PopClip,
    ];
    let (width, _, pixels) = renderer.snapshot(&commands, 2.0).unwrap();
    let picture = Picture { width, pixels };
    picture.expect(60, 92, [255, 0, 0, 255], 2);
    picture.expect(42, 42, BACKGROUND, 2);
    picture.expect(110, 70, BACKGROUND, 2);
    picture.expect(70, 110, BACKGROUND, 2);
    let text = (40..100).any(|x| picture.at(x, 64)[1] > 200);
    assert!(text, "the text is drawn inside the clip");
}

#[test]
fn a_large_blur_is_smooth() {
    let Some(mut renderer) = renderer(128) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 128.0, 128.0),
                effect: LayerEffect {
                    blur: 20.0,
                    ..LayerEffect::default()
                },
            },
            fill(area(0.0, 0.0, 64.0, 128.0), Color::rgb(255, 255, 255)),
            DrawCommand::PopEffect,
        ],
    );
    let mut last = 255;
    for x in 30..100 {
        let value = picture.at(x, 64)[0];
        assert!(
            value <= last.max(value) && last.saturating_sub(value) < 16,
            "step at {x}: {last} to {value}"
        );
        last = value;
    }
}

#[test]
fn a_blurred_effect_inside_a_clip_is_cut_by_the_clip() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[
            DrawCommand::PushClip {
                rect: area(0.0, 0.0, 32.0, 64.0),
            },
            DrawCommand::PushEffect {
                rect: area(0.0, 0.0, 64.0, 64.0),
                effect: LayerEffect {
                    blur: 3.0,
                    ..LayerEffect::default()
                },
            },
            fill(area(8.0, 8.0, 48.0, 48.0), Color::rgb(255, 255, 255)),
            DrawCommand::PopEffect,
            DrawCommand::PopClip,
        ],
    );
    picture.expect(48, 32, BACKGROUND, 2);
    let inside = picture.at(24, 32);
    assert!(inside[0] > 240, "{inside:?}");
}

/// A picture of one color, `size` pixels square, as a generation of `key`.
fn live_frame(key: ImageKey, size: u32, color: [u8; 4]) -> ImageData {
    let pixels: Vec<u8> = (0..size * size).flat_map(|_| color).collect();
    ImageData::keyed(key, size, size, pixels).unwrap()
}

/// `image` drawn over the square from (8, 8) to (40, 40).
fn picture_at_8(image: ImageData) -> DrawCommand {
    DrawCommand::Image {
        rect: area(8.0, 8.0, 32.0, 32.0),
        image,
        corner_radius: 0.0,
        opacity: 1.0,
    }
}

#[test]
fn a_keyed_picture_keeps_one_texture_and_shows_its_newest_generation() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let key = ImageKey::new();
    let frames = [
        (4, [255, 0, 0, 255]),
        (4, [0, 255, 0, 255]),
        (8, [0, 0, 255, 255]),
        (8, [255, 255, 255, 255]),
    ];
    for (size, color) in frames {
        let picture = draw(&mut renderer, &[picture_at_8(live_frame(key, size, color))]);
        picture.expect(24, 24, color, 2);
        picture.expect(50, 50, BACKGROUND, 2);
        assert_eq!(
            renderer.images.texture_count(),
            1,
            "one texture for the key"
        );
    }
    let still = ImageData::from_rgba(2, 2, vec![255; 16]).unwrap();
    draw(&mut renderer, &[picture_at_8(still)]);
    assert_eq!(
        renderer.images.texture_count(),
        2,
        "a still picture has its own"
    );
}

/// A coverage mask of `size` pixels square: full coverage, except the left half at `edge`.
fn coverage(size: u32, edge: u8) -> ImageData {
    let bytes: Vec<u8> = (0..size * size)
        .map(|index| if index % size < size / 2 { edge } else { 255 })
        .collect();
    ImageData::from_alpha(size, size, &bytes).unwrap()
}

#[test]
fn a_mask_is_filled_with_its_color_by_coverage() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let commands = [DrawCommand::Mask {
        rect: area(8.0, 8.0, 16.0, 16.0),
        mask: coverage(16, 0),
        color: GREEN,
    }];
    let picture = draw(&mut renderer, &commands);
    picture.expect(20, 16, [0, 255, 0, 255], 2);
    picture.expect(10, 16, BACKGROUND, 2);
    picture.expect(40, 40, BACKGROUND, 2);

    let half = [DrawCommand::Mask {
        rect: area(8.0, 8.0, 16.0, 16.0),
        mask: coverage(16, 255),
        color: Color::rgba(255, 0, 0, 128),
    }];
    let picture = draw(&mut renderer, &half);
    let [r, g, b, _] = picture.at(16, 16);
    assert!(
        r > 120 && g < BACKGROUND[1] && b < BACKGROUND[2],
        "the color's alpha fades the mask: {:?}",
        [r, g, b]
    );
}

/// A 4 by 4 SVG document: red over its top half, nothing below.
const TOP_HALF_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="4" height="2" fill="#ff0000"/></svg>"##;

/// A 4 by 4 SVG document in red stripes one unit wide: the first and third columns.
const STRIPES_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4"><rect width="1" height="4" fill="#ff0000"/><rect x="2" width="1" height="4" fill="#ff0000"/></svg>"##;

#[test]
fn an_svg_is_drawn_in_its_colors_or_tinted() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let painter = SvgPainter::from_str(TOP_HALF_SVG).unwrap();
    let own = [DrawCommand::Svg {
        rect: area(8.0, 8.0, 16.0, 16.0),
        painter: painter.clone(),
        tint: None,
        opacity: 1.0,
    }];
    let picture = draw(&mut renderer, &own);
    picture.expect(16, 12, [255, 0, 0, 255], 2);
    picture.expect(16, 20, BACKGROUND, 2);

    let tinted = [DrawCommand::Svg {
        rect: area(8.0, 8.0, 16.0, 16.0),
        painter,
        tint: Some(GREEN),
        opacity: 1.0,
    }];
    let picture = draw(&mut renderer, &tinted);
    picture.expect(16, 12, [0, 255, 0, 255], 2);
    picture.expect(16, 20, BACKGROUND, 2);
}

#[test]
fn an_svg_is_drawn_at_its_physical_size() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let commands = [DrawCommand::Svg {
        rect: area(10.0, 10.0, 4.0, 4.0),
        painter: SvgPainter::from_str(STRIPES_SVG).unwrap(),
        tint: None,
        opacity: 1.0,
    }];
    let (width, _, pixels) = renderer.snapshot(&commands, 2.0).unwrap();
    let picture = Picture { width, pixels };
    for (column, red) in [true, true, false, false, true, true, false, false]
        .into_iter()
        .enumerate()
    {
        let expected = if red { [255, 0, 0, 255] } else { BACKGROUND };
        picture.expect(20 + column as u32, 24, expected, 2);
    }
}

/// A coverage mask of `size` pixels square in stripes: every even column covered, every odd
/// one not, so sampling between texels shows as grey.
fn stripes(size: u32) -> ImageData {
    let bytes: Vec<u8> = (0..size * size)
        .map(|index| {
            if (index % size).is_multiple_of(2) {
                255
            } else {
                0
            }
        })
        .collect();
    ImageData::from_alpha(size, size, &bytes).unwrap()
}

/// The stripes expected from `start` on one row of `picture`, one per pixel.
#[track_caller]
fn expect_stripes(picture: &Picture, start: u32, count: u32, y: u32) {
    for column in 0..count {
        let expected = if column.is_multiple_of(2) {
            [255, 0, 0, 255]
        } else {
            BACKGROUND
        };
        picture.expect(start + column, y, expected, 2);
    }
}

#[test]
fn a_mask_at_its_pixel_size_lands_on_whole_pixels() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let commands = [DrawCommand::Mask {
        rect: area(10.4, 10.4, 4.0, 4.0),
        mask: stripes(4),
        color: RED,
    }];
    let picture = draw(&mut renderer, &commands);
    expect_stripes(&picture, 10, 4, 12);
    picture.expect(14, 12, BACKGROUND, 2);

    let sharp = [DrawCommand::Mask {
        rect: area(10.3, 10.3, 4.0, 4.0),
        mask: stripes(8),
        color: RED,
    }];
    let (width, _, pixels) = renderer.snapshot(&sharp, 2.0).unwrap();
    let picture = Picture { width, pixels };
    expect_stripes(&picture, 21, 8, 24);
    picture.expect(20, 24, BACKGROUND, 2);
}

#[test]
fn a_picture_at_its_pixel_size_lands_on_whole_pixels() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let commands = [DrawCommand::Image {
        rect: area(10.6, 10.6, 4.0, 4.0),
        image: stripes(4),
        corner_radius: 0.0,
        opacity: 1.0,
    }];
    let picture = draw(&mut renderer, &commands);
    for column in 0..4_u32 {
        let expected = if column.is_multiple_of(2) {
            [255, 255, 255, 255]
        } else {
            BACKGROUND
        };
        picture.expect(11 + column, 12, expected, 2);
    }
}

/// Renderers blend colors as they are stored, in sRGB-encoded values, not in linear light:
/// half-transparent white over black lands halfway in stored values.
#[test]
fn colors_blend_as_they_are_stored() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    renderer.set_transparent(false);
    let picture = draw(
        &mut renderer,
        &[
            fill(area(0.0, 0.0, 64.0, 64.0), Color::rgb(0, 0, 0)),
            fill(area(0.0, 0.0, 64.0, 64.0), Color::rgba(255, 255, 255, 128)),
        ],
    );
    let pixel = picture.at(32, 32);
    assert!(
        pixel[0].abs_diff(128) < 4,
        "halfway in stored values: {pixel:?}"
    );
}

/// A shadow is a Gaussian blur of its rounded rectangle, so a corner is lighter than a side
/// the same distance out.
#[test]
fn a_shadow_lightens_its_corners() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    renderer.set_transparent(true);
    let picture = draw(
        &mut renderer,
        &[DrawCommand::Shadow {
            rect: area(16.0, 16.0, 32.0, 32.0),
            corner_radius: 0.0,
            blur: 8.0,
            color: Color::rgb(0, 0, 0),
        }],
    );
    let (side, corner) = (picture.at(32, 51)[3], picture.at(50, 50)[3]);
    assert!(corner < side, "side {side}, corner {corner}");
}

#[test]
fn svg_text_is_set_with_the_text_engines_fonts() {
    let _metrics = crate::TextMetrics::new();
    if rustle_text::font_families().is_empty() {
        return;
    }
    let painter = SvgPainter::from_str(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="60" height="30"><text x="2" y="24" font-size="24" font-family="sans-serif" fill="#000000">Hg</text></svg>"##,
    )
    .unwrap();
    let image = painter.rasterize(60, 30).unwrap();
    assert!(image.pixels().chunks(4).any(|pixel| pixel[3] > 0));
}

/// Text limited to one line draws only its first line, though it holds a line break.
#[test]
fn a_one_line_text_draws_no_second_line() {
    let Some(mut renderer) = renderer(128) else {
        return;
    };
    let said = |content: &str| match words(8.0, 8.0) {
        DrawCommand::Text {
            x,
            y,
            width,
            color,
            font_size,
            line_height,
            font_weight,
            font_family,
            align,
            max_lines,
            italic,
            letter_spacing,
            ligatures,
            wrap,
            ..
        } => DrawCommand::Text {
            x,
            y,
            width,
            content: content.into(),
            color,
            font_size,
            line_height,
            font_weight,
            font_family,
            align,
            max_lines,
            italic,
            letter_spacing,
            ligatures,
            wrap,
        },
        other => other,
    };
    let one = draw(&mut renderer, &[said("Rustle")]);
    let broken = draw(&mut renderer, &[said("Rustle\nRustle")]);
    assert_eq!(differing(&one, &broken, (0, 0, 128, 128), 8), 0);
}

/// A BGRA picture of one color, given as RGBA, `size` pixels square, as a generation of `key`.
fn live_bgra(key: ImageKey, size: u32, color: [u8; 4]) -> ImageData {
    let [red, green, blue, alpha] = color;
    let pixels: Vec<u8> = (0..size * size)
        .flat_map(|_| [blue, green, red, alpha])
        .collect();
    ImageData::keyed_as(key, size, size, pixels, PixelFormat::Bgra8).unwrap()
}

#[test]
fn a_bgra_picture_shows_its_colors_where_they_belong() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let key = ImageKey::new();
    let red = [230, 20, 10, 255];
    let picture = draw(&mut renderer, &[picture_at_8(live_bgra(key, 8, red))]);
    picture.expect(24, 24, red, 2);
    let rgba = live_frame(key, 8, [10, 20, 230, 255]);
    let picture = draw(&mut renderer, &[picture_at_8(rgba)]);
    picture.expect(24, 24, [10, 20, 230, 255], 2);
}

#[test]
fn a_changed_rectangle_is_all_that_is_written_over_the_generation_it_changed_from() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    let key = ImageKey::new();
    let first = live_bgra(key, 32, [255, 0, 0, 255]);
    let previous = first.generation();
    draw(&mut renderer, &[picture_at_8(first)]);
    let green =
        live_bgra(key, 32, [0, 255, 0, 255]).changed_since(previous, PixelRect::new(0, 0, 16, 32));
    let picture = draw(&mut renderer, &[picture_at_8(green.clone())]);
    picture.expect(12, 24, [0, 255, 0, 255], 2);
    picture.expect(36, 24, [255, 0, 0, 255], 2);
    let blue =
        live_bgra(key, 32, [0, 0, 255, 255]).changed_since(previous, PixelRect::new(0, 0, 1, 1));
    let picture = draw(&mut renderer, &[picture_at_8(blue)]);
    picture.expect(12, 24, [0, 0, 255, 255], 2);
    picture.expect(36, 24, [0, 0, 255, 255], 2);
}

/// The time `run` takes on average over `rounds` calls after one to warm up, in milliseconds.
fn average_ms(rounds: u32, mut run: impl FnMut()) -> f64 {
    run();
    let started = std::time::Instant::now();
    for _ in 0..rounds {
        run();
    }
    started.elapsed().as_secs_f64() * 1000.0 / f64::from(rounds)
}

/// Times writing a new generation of a live picture into its texture at 1080p and 4K: whole
/// and RGBA, whole and BGRA, and only a caret's rectangle, then a tenth of the rows, as BGRA.
/// Run with `cargo test -p rustle-wgpu --release -- --ignored --nocapture live_picture_upload_cost`.
#[test]
#[ignore = "a measurement, not a check"]
fn live_picture_upload_cost() {
    let Some(mut renderer) = renderer(64) else {
        return;
    };
    for (name, width, height) in [("1080p", 1920u32, 1080u32), ("4K", 3840, 2160)] {
        let pixels: std::sync::Arc<[u8]> = vec![128u8; (width * height * 4) as usize].into();
        let key = ImageKey::new();
        let mut timed = |format: PixelFormat, rect: Option<PixelRect>| {
            let mut previous =
                ImageData::keyed_as(key, width, height, pixels.clone(), format).unwrap();
            renderer
                .images
                .prepare(&renderer.device, &renderer.queue, &previous);
            average_ms(30, || {
                let mut image =
                    ImageData::keyed_as(key, width, height, pixels.clone(), format).unwrap();
                if let Some(rect) = rect {
                    image = image.changed_since(previous.generation(), rect);
                }
                renderer
                    .images
                    .prepare(&renderer.device, &renderer.queue, &image);
                renderer.queue.submit([]);
                let _ = renderer.device.poll(wgpu::PollType::wait_indefinitely());
                previous = image;
            })
        };
        let whole_rgba = timed(PixelFormat::Rgba8, None);
        let whole_bgra = timed(PixelFormat::Bgra8, None);
        let caret = timed(PixelFormat::Bgra8, Some(PixelRect::new(400, 300, 2, 40)));
        let tenth = timed(
            PixelFormat::Bgra8,
            Some(PixelRect::new(0, height / 2, width, height / 10)),
        );
        println!(
            "{name}: whole RGBA {whole_rgba:.3} ms, whole BGRA {whole_bgra:.3} ms, caret {caret:.3} ms, tenth of rows {tenth:.3} ms"
        );
    }
}

/// Lines of a code view: each cut to its own row, with a background and text.
fn code_lines(count: usize, isolate: bool) -> Vec<DrawCommand> {
    let mut commands = Vec::new();
    for line in 0..count {
        let y = line as f32 * 24.0;
        let row = area(0.0, y, 200.0, 24.0);
        if isolate {
            commands.push(DrawCommand::PushLayer {
                rect: row,
                alpha: 1.0,
                rotation: 0.0,
                scale: 1.0,
            });
        }
        commands.push(DrawCommand::PushClip { rect: row });
        let shade = if line % 2 == 0 { RED } else { BLUE };
        commands.push(fill(area(0.0, y, 200.0, 24.0), shade));
        commands.push(words(4.0, y));
        commands.push(DrawCommand::PopClip);
        if isolate {
            commands.push(DrawCommand::PopLayer);
        }
    }
    commands
}

#[test]
fn clipped_lines_that_do_not_overlap_share_one_layer() {
    let Some(mut renderer) = renderer(256) else {
        return;
    };
    let layers = renderer.build_layers(&code_lines(10, false), 1.0);
    assert!(layers <= 1, "{layers} layers for one layer of lines");
    let shared = draw(&mut renderer, &code_lines(10, false));
    let Some(mut reference) = self::renderer(256) else {
        return;
    };
    let isolated = draw(&mut reference, &code_lines(10, true));
    assert_eq!(differing(&shared, &isolated, (0, 0, 256, 256), 2), 0);
}

#[test]
fn a_rectangle_over_earlier_text_still_paints_over_it() {
    let Some(mut renderer) = renderer(128) else {
        return;
    };
    let picture = draw(
        &mut renderer,
        &[words(8.0, 8.0), fill(area(0.0, 0.0, 128.0, 48.0), RED)],
    );
    for x in (8..80).step_by(4) {
        picture.expect(x, 20, [255, 0, 0, 255], 2);
    }
}
