//! A page of the text features: rich spans, ligatures, bidirectional text, wrapped
//! text areas, the glyph-level layout API on a canvas, selection across several
//! blocks, and the font functions.
//!
//! Run it with `cargo run --release --example text`, or draw it to a picture with
//! `RUSTLE_SNAPSHOT=out.png RUSTLE_SNAPSHOT_SIZE=1000x1500 cargo run --example text`.

use rustle::prelude::*;
use rustle::{
    Affinity, SelectionPoint, TextWrap, basic_text_area, basic_text_area_no_wrap, font_families,
    layout_spans, register_font, remember_selection_state, selection_container_with,
};
use rustle_geometry::Offset;

const MUTED: Color = Color::rgb(150, 150, 165);
const PANEL: Color = Color::rgb(28, 28, 36);
const ACCENT: Color = Color::rgb(120, 170, 255);

fn heading(label: &str) {
    text_styled(
        Modifier::new().padding_xy(0.0, 6.0),
        label,
        TextStyle::default().font_size(12.0).color(MUTED),
    );
}

fn rich_spans() -> Vec<TextSpan> {
    vec![
        TextSpan::new("Rich text: "),
        TextSpan::new("italic").italic(),
        TextSpan::new(" "),
        TextSpan::new("underline").underline(),
        TextSpan::new(" "),
        TextSpan::new("strikethrough").strikethrough(),
        TextSpan::new(" "),
        TextSpan::new("highlighted")
            .background(Color::rgba(255, 200, 60, 90))
            .color(Color::rgb(255, 230, 150)),
        TextSpan::new(" "),
        TextSpan::new("BIG").font_size(28.0).color(ACCENT).bold(),
        TextSpan::new(" tight").letter_spacing(-0.6),
        TextSpan::new(" wide").letter_spacing(4.0),
        TextSpan::new(" mono")
            .font_family(FontFamily::Monospace)
            .underline()
            .italic(),
    ]
}

#[composable]
fn spans_section() {
    heading("RICH SPANS");
    rich_text_with(Modifier::new().width(900.0), rich_spans());
    let big = TextStyle::default()
        .font_size(34.0)
        .font_family(FontFamily::named("Liberation Sans"));
    heading("LIGATURES (fi fl ffi) ON, THEN OFF");
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(40.0),
        Alignment::Start,
        || {
            text_styled(Modifier::new(), "office fluffy find", big);
            text_styled(Modifier::new(), "office fluffy find", big.ligatures(false));
        },
    );
}

#[composable]
fn bidi_section() {
    heading("BIDIRECTIONAL");
    let style = TextStyle::default().font_size(20.0);
    LOCAL_TEXT_STYLE.provide(style, || {
        text("Hello \u{5e9}\u{5dc}\u{5d5}\u{5dd} \u{5e2}\u{5d5}\u{5dc}\u{5dd} and \u{645}\u{631}\u{62d}\u{628}\u{627} \u{628}\u{627}\u{644}\u{639}\u{627}\u{644}\u{645} end 123");
        text_with(
            Modifier::new().width(420.0),
            "\u{5d6}\u{5d4}\u{5d5} \u{5de}\u{5e9}\u{5e4}\u{5d8} \u{5d1}\u{5e2}\u{5d1}\u{5e8}\u{5d9}\u{5ea} \u{5d0}\u{5e8}\u{5d5}\u{5da} \u{5e9}\u{5de}\u{5ea}\u{5d7}\u{5d9}\u{5dc} \u{5dc}\u{5e9}\u{5d1}\u{5d5}\u{5e8} \u{5d1}\u{5e9}\u{5d5}\u{5e8}\u{5d4} \u{5e0}\u{5d5}\u{5e1}\u{5e4}\u{5ea} \u{5db}\u{5d3}\u{5d9} \u{5dc}\u{5d4}\u{5e8}\u{5d0}\u{5d5}\u{5ea}",
        );
    });
}

const LONG: &str = "Soft wrapping breaks this paragraph at the width of the field, so the caret can go up and down by visual line. Home and End stay on the visual line.\nA second paragraph after a hard line break, with a_very_long_unbreakable_token_that_has_to_wrap_by_character_when_it_is_wider_than_a_line.";

#[composable]
fn areas_section() {
    heading("WRAPPED TEXT AREA (click it), THEN NO-WRAP");
    let wrapped = remember_text_field_state(LONG);
    let plain = remember_text_field_state(LONG);
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(24.0),
        Alignment::Start,
        || {
            basic_text_area(
                Modifier::new()
                    .size(420.0, 150.0)
                    .background(PANEL)
                    .padding(8.0),
                &wrapped,
            );
            basic_text_area_no_wrap(
                Modifier::new()
                    .size(300.0, 150.0)
                    .background(PANEL)
                    .padding(8.0),
                &plain,
            );
        },
    );
}

#[composable]
fn canvas_section() {
    heading("GLYPH-LEVEL LAYOUT ON A CANVAS: LINE METRICS, SELECTION, CARET, HIT TEST");
    canvas(Modifier::new().size(620.0, 190.0).background(PANEL), |c| {
        let style = TextStyle::default().font_size(17.0);
        let spans = rich_spans();
        let width = c.width() - 24.0;
        let layout = layout_spans(&spans, &style, Some(width));
        // Line boxes and baselines.
        for line in layout.lines() {
            c.stroke_rect(
                12.0,
                12.0 + line.top,
                line.width.max(1.0),
                line.height,
                0.0,
                1.0,
                Color::rgba(255, 255, 255, 40),
            );
            c.rect(
                12.0,
                12.0 + line.baseline,
                width,
                1.0,
                Color::rgba(255, 90, 90, 90),
            );
        }
        c.selection(12.0, 12.0, &layout, 6, 40, Color::rgba(74, 96, 214, 140));
        c.draw_rich_text(12.0, 12.0, &spans, &style, Some(width));
        c.caret(
            12.0,
            12.0,
            &layout,
            52,
            Affinity::Downstream,
            1.5,
            Color::rgb(255, 255, 255),
        );
        // Hit testing: which offset is under a point, drawn as a marker on its caret.
        let probe = Offset::new(200.0, 40.0);
        let (offset, side) = layout.hit_test_with_affinity(probe);
        c.circle(
            12.0 + probe.x,
            12.0 + probe.y,
            3.0,
            Color::rgb(255, 200, 60),
        );
        c.caret(
            12.0,
            12.0,
            &layout,
            offset,
            side,
            2.0,
            Color::rgb(255, 200, 60),
        );
        let size = layout.size();
        let report = format!(
            "{} lines, {:.0} x {:.0}; probe at (200, 40) hits offset {offset}",
            layout.line_count(),
            size.width,
            size.height
        );
        let small = TextStyle::default()
            .font_size(12.0)
            .color(MUTED)
            .wrap(TextWrap::None);
        c.draw_text(12.0, 150.0, &report, &small, None);
        let caret = layout.caret(52, Affinity::Downstream);
        c.draw_text(
            12.0,
            168.0,
            &format!(
                "caret at offset 52: x {:.1} y {:.1} line {}",
                caret.x, caret.y, caret.line
            ),
            &small,
            None,
        );
    });
}

#[composable]
fn selection_section() {
    heading("SELECTION ACROSS BLOCKS (selection_container)");
    let state = remember_selection_state();
    let preset = state.clone();
    disposable_effect((), move || {
        preset.select(
            SelectionPoint {
                block: 0,
                offset: 12,
            },
            SelectionPoint {
                block: 2,
                offset: 24,
            },
        );
        || {}
    });
    selection_container_with(
        Modifier::new().width(620.0).background(PANEL).padding(12.0),
        &state,
        || {
            selectable_text(
                Modifier::new(),
                "First block: plain selectable text that is long enough to wrap onto a second line in this box.",
            );
            spacer(Modifier::new().height(8.0));
            rich_text(vec![
                TextSpan::new("Second block is "),
                TextSpan::new("rich").bold().color(ACCENT),
                TextSpan::new(" text with "),
                TextSpan::new("underline").underline(),
                TextSpan::new("."),
            ]);
            spacer(Modifier::new().height(8.0));
            selectable_text(
                Modifier::new(),
                "Third block: the selection runs through all three.",
            );
        },
    );
    let copied = state.selected_text().unwrap_or_default();
    text_styled(
        Modifier::new().padding_xy(0.0, 6.0).width(620.0),
        format!("selected_text(): {copied:?}"),
        TextStyle::default().font_size(12.0).color(MUTED),
    );
}

#[composable]
fn fonts_section() {
    heading("FONTS");
    let families = font_families();
    // Registering bytes at run time: a file stands in for bundled `include_bytes!` data.
    let registered = remember(|| {
        std::fs::read("/usr/share/fonts/TTF/DejaVuSans.ttf")
            .map(register_font)
            .unwrap_or_default()
    });
    text_styled(
        Modifier::new().width(900.0),
        format!(
            "{} families installed, e.g. {}. register_font returned {:?}.",
            families.len(),
            families
                .iter()
                .take(6)
                .cloned()
                .collect::<Vec<_>>()
                .join(", "),
            registered.get(),
        ),
        TextStyle::default().font_size(12.0).color(MUTED),
    );
    for name in [
        "DejaVu Serif",
        "Hack",
        "Liberation Mono",
        "Noto Naskh Arabic",
    ] {
        text_styled(
            Modifier::new(),
            format!("{name}: The quick brown fox jumps over the lazy dog"),
            TextStyle::default()
                .font_family(FontFamily::named(name))
                .font_size(16.0),
        );
    }
}

#[composable]
fn app() {
    column_with(Modifier::new().fill_max_size().padding(24.0), || {
        spans_section();
        bidi_section();
        areas_section();
        canvas_section();
        selection_section();
        fonts_section();
    });
}

fn main() {
    rustle::run(app);
}
