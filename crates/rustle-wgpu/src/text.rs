//! Text: shaping with glyphon, drawing it, and measuring it for layout.

use std::rc::Rc;

use glyphon::cosmic_text::{Align, Ellipsize, EllipsizeHeightLimit, Hinting};
use glyphon::{
    Attrs, Buffer, Color, Family, FontSystem, Metrics, Shaping, TextArea, TextBounds, TextRenderer,
    Weight, Wrap,
};
use rustle_geometry::Size;
use rustle_graphics::{FontFamily, StyledSpan, TextAlign};
use rustle_text::{TextMeasurer, TextStyle};

use crate::{Renderer, Result};

impl Renderer {
    pub(super) fn prepare_text(
        &mut self,
        index: usize,
        texts: &[Shaped],
        scale_factor: f32,
        target: (u32, u32),
    ) -> Result<()> {
        while self.text_renderers.len() <= index {
            self.text_renderers.push(TextRenderer::new(
                &mut self.atlas,
                &self.device,
                wgpu::MultisampleState::default(),
                None,
            ));
        }
        let bounds = TextBounds {
            left: 0,
            top: 0,
            right: target.0 as i32,
            bottom: target.1 as i32,
        };
        let areas = texts.iter().map(|text| TextArea {
            buffer: &text.buffer,
            left: (text.position.0 * scale_factor).round(),
            top: (text.position.1 * scale_factor).round(),
            scale: scale_factor,
            bounds,
            default_color: text.color,
            custom_glyphs: &[],
        });
        self.text_renderers[index].prepare(
            &self.device,
            &self.queue,
            &mut self.font_system,
            &mut self.atlas,
            &self.viewport,
            areas,
            &mut self.swash_cache,
        )?;
        Ok(())
    }
}

pub(super) struct Shaped {
    pub(super) position: (f32, f32),
    pub(super) color: Color,
    pub(super) buffer: Rc<Buffer>,
}
impl TextMeasurer for Renderer {
    fn measure(&mut self, content: &str, style: &TextStyle, max_width: f32) -> Size {
        let buffer = shape(
            &mut self.font_system,
            Setting {
                content: Content::Plain {
                    text: content,
                    font_weight: legible_weight(style.font_weight.0, style.color),
                    font_family: style.font_family,
                },
                font_size: style.font_size,
                line_height: style.line_height,
                align: TextAlign::Start,
                max_lines: style.max_lines,
            },
            max_width.is_finite().then_some(max_width),
        );
        // A trailing line break leaves an empty last line the shaper does not report.
        let breaks = content.matches('\n').count() as u32 + 1;
        Self::size_of(&buffer, style.line_height, breaks)
    }

    fn measure_spans(&mut self, spans: &[StyledSpan], style: &TextStyle, max_width: f32) -> Size {
        let buffer = shape(
            &mut self.font_system,
            Setting {
                content: Content::Rich(spans),
                font_size: style.font_size,
                line_height: style.line_height,
                align: TextAlign::Start,
                max_lines: style.max_lines,
            },
            max_width.is_finite().then_some(max_width),
        );
        Self::size_of(&buffer, style.line_height, 0)
    }
}

impl Renderer {
    fn size_of(buffer: &Buffer, line_height: f32, at_least: u32) -> Size {
        let (width, lines) = buffer
            .layout_runs()
            .fold((0.0_f32, 0_u32), |(width, lines), run| {
                (width.max(run.line_w), lines + 1)
            });
        Size::new(width, lines.max(at_least).max(1) as f32 * line_height)
    }
}

/// How a piece of text is set.
pub(super) struct Setting<'a> {
    pub(super) content: Content<'a>,
    pub(super) font_size: f32,
    pub(super) line_height: f32,
    pub(super) align: TextAlign,
    pub(super) max_lines: Option<usize>,
}

/// What a piece of text says.
pub(super) enum Content<'a> {
    /// One run in one style.
    Plain {
        text: &'a str,
        font_weight: u16,
        font_family: FontFamily,
    },
    /// Runs in their own styles.
    Rich(&'a [StyledSpan]),
}

/// Whether text in `color` sits on a light surface.
fn is_dark(color: rustle_graphics::Color) -> bool {
    let luminance =
        0.2126 * f32::from(color.r) + 0.7152 * f32::from(color.g) + 0.0722 * f32::from(color.b);
    luminance < 110.0
}

/// Dark text is drawn pure black, the strongest contrast antialiasing allows.
pub(super) fn legible_color(color: rustle_graphics::Color) -> Color {
    if is_dark(color) {
        Color::rgba(0, 0, 0, color.a)
    } else {
        Color::rgba(color.r, color.g, color.b, color.a)
    }
}

/// Dark text on a light surface blends thinner than light text on a dark one,
/// so it is set one step heavier to stay as readable.
pub(super) fn legible_weight(weight: u16, color: rustle_graphics::Color) -> u16 {
    if is_dark(color) && weight < 600 {
        600
    } else {
        weight
    }
}

fn family(family: FontFamily) -> Family<'static> {
    match family {
        FontFamily::SansSerif => Family::SansSerif,
        FontFamily::Serif => Family::Serif,
        FontFamily::Monospace => Family::Monospace,
        FontFamily::Named(name) => Family::Name(name),
    }
}

/// Shapes text in logical pixels, wrapping at `width` if there is one.
pub(super) fn shape(
    font_system: &mut FontSystem,
    setting: Setting<'_>,
    width: Option<f32>,
) -> Buffer {
    let mut buffer = Buffer::new(
        font_system,
        Metrics::new(setting.font_size, setting.line_height),
    );
    // Snap glyph advances to whole pixels so strokes land on the pixel grid.
    buffer.set_hinting(Hinting::Enabled);
    buffer.set_size(width, None);
    buffer.set_wrap(Wrap::Word);
    buffer.set_ellipsize(match setting.max_lines {
        Some(lines) => Ellipsize::End(EllipsizeHeightLimit::Lines(lines)),
        None => Ellipsize::None,
    });
    let alignment = match setting.align {
        TextAlign::Start => None,
        TextAlign::Left => Some(Align::Left),
        TextAlign::Center => Some(Align::Center),
        TextAlign::Right => Some(Align::Right),
    };
    match setting.content {
        Content::Plain {
            text,
            font_weight,
            font_family,
        } => buffer.set_text(
            text,
            &Attrs::new()
                .family(family(font_family))
                .weight(Weight(font_weight)),
            Shaping::Advanced,
            alignment,
        ),
        Content::Rich(spans) => buffer.set_rich_text(
            spans.iter().map(|span| {
                (
                    span.text.as_str(),
                    Attrs::new()
                        .family(family(span.font_family))
                        .weight(Weight(legible_weight(span.font_weight, span.color)))
                        .color(Color::rgba(
                            span.color.r,
                            span.color.g,
                            span.color.b,
                            span.color.a,
                        )),
                )
            }),
            &Attrs::new(),
            Shaping::Advanced,
            alignment,
        ),
    }
    buffer.shape_until_scroll(font_system, false);
    buffer
}
