//! Turns the vendored Phosphor SVGs into constants of path data, one per icon.
//!
//! A constant that nothing uses is not compiled into the program.

use std::path::PathBuf;

use proc_macro::{TokenStream, TokenTree};
use svgtypes::{SimplePathSegment, SimplifyingPathParser};

const WEIGHTS: [&str; 5] = ["thin", "light", "regular", "bold", "fill"];

/// `icons!(regular)`: one `pub const` per icon of that weight, named in SCREAMING_SNAKE_CASE.
///
/// The weight is one of `thin`, `light`, `regular`, `bold` and `fill`. A constant
/// nothing uses is not compiled into the program.
#[proc_macro]
pub fn icons(input: TokenStream) -> TokenStream {
    match expand(input) {
        Ok(tokens) => tokens,
        Err(message) => format!("compile_error!({message:?})").parse().unwrap(),
    }
}

fn expand(input: TokenStream) -> Result<TokenStream, String> {
    let weight = match input.into_iter().next() {
        Some(TokenTree::Ident(weight)) => weight.to_string(),
        _ => return Err("expected a weight, like `icons!(regular)`".into()),
    };
    if !WEIGHTS.contains(&weight.as_str()) {
        return Err(format!(
            "unknown weight `{weight}`; use thin, light, regular, bold or fill"
        ));
    }
    let directory: PathBuf = [env!("CARGO_MANIFEST_DIR"), "assets", "phosphor", &weight]
        .iter()
        .collect();
    let mut files: Vec<_> = std::fs::read_dir(&directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(|entry| entry.ok()?.path().into())
        .collect::<Vec<PathBuf>>();
    files.sort();
    let suffix = format!("-{weight}");
    let mut code = String::new();
    for file in files {
        let Some(stem) = file.file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let name = if weight == "regular" {
            stem
        } else {
            stem.strip_suffix(suffix.as_str()).unwrap_or(stem)
        };
        let svg = std::fs::read_to_string(&file)
            .map_err(|error| format!("{}: {error}", file.display()))?;
        let (commands, count) = commands(&svg).map_err(|error| format!("{stem}: {error}"))?;
        let size = view_box_size(&svg).unwrap_or(256.0);
        let mut constant = name.replace('-', "_").to_uppercase();
        if constant.starts_with(|c: char| c.is_ascii_digit()) {
            constant.insert(0, '_');
        }
        code.push_str(&format!(
            "pub const {constant}: ::rustle_icons::Icon = ::rustle_icons::__private::Path::from_static(\
             &[{commands}] as &[::rustle_icons::__private::PathCommand; {count}], {size:?}_f32, {size:?}_f32, \
             ::rustle_icons::__private::FillRule::NonZero);\n"
        ));
    }
    code.parse()
        .map_err(|_| "could not build the icons".to_string())
}

/// The path commands of every `d` attribute in `svg`, as Rust source, and how many.
fn commands(svg: &str) -> Result<(String, usize), String> {
    let (mut out, mut count) = (String::new(), 0);
    let mut rest = svg;
    while let Some(at) = rest.find(" d=\"") {
        rest = &rest[at + 4..];
        let end = rest.find('"').ok_or("unterminated path data")?;
        for segment in SimplifyingPathParser::from(&rest[..end]) {
            let segment = segment.map_err(|error| error.to_string())?;
            let command = match segment {
                SimplePathSegment::MoveTo { x, y } => format!("MoveTo({}, {})", f(x), f(y)),
                SimplePathSegment::LineTo { x, y } => format!("LineTo({}, {})", f(x), f(y)),
                SimplePathSegment::Quadratic { x1, y1, x, y } => {
                    format!("QuadTo({}, {}, {}, {})", f(x1), f(y1), f(x), f(y))
                }
                SimplePathSegment::CurveTo {
                    x1,
                    y1,
                    x2,
                    y2,
                    x,
                    y,
                } => format!(
                    "CubicTo({}, {}, {}, {}, {}, {})",
                    f(x1),
                    f(y1),
                    f(x2),
                    f(y2),
                    f(x),
                    f(y)
                ),
                SimplePathSegment::ClosePath => "Close".to_string(),
            };
            out.push_str("::rustle_icons::__private::PathCommand::");
            out.push_str(&command);
            out.push(',');
            count += 1;
        }
        rest = &rest[end..];
    }
    if count == 0 {
        return Err("no path in the icon".into());
    }
    Ok((out, count))
}

fn f(value: f64) -> String {
    format!("{:?}_f32", value as f32)
}

/// The width of the `viewBox`, which is the size of the icon's box.
fn view_box_size(svg: &str) -> Option<f32> {
    let start = svg.find("viewBox=\"")? + 9;
    let end = svg[start..].find('"')? + start;
    svg[start..end].split_whitespace().nth(2)?.parse().ok()
}
