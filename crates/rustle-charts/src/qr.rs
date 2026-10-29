use qrcode::{Color as Module, QrCode};
use rustle_foundation::{Modifier, canvas};
use rustle_graphics::Color;

/// Emits a QR code for `data`, `size` square, with `dark` modules on a `light` background.
#[track_caller]
pub fn qr_code(size: f32, data: &str, dark: Color, light: Color) {
    let modules: Option<(usize, Vec<bool>)> = QrCode::new(data.as_bytes()).ok().map(|code| {
        (
            code.width(),
            code.to_colors()
                .iter()
                .map(|m| *m == Module::Dark)
                .collect(),
        )
    });
    canvas(Modifier::new().size(size, size), move |canvas| {
        canvas.rounded_rect(0.0, 0.0, size, size, 6.0, light);
        let Some((width, dark_modules)) = &modules else {
            return;
        };
        // A quiet border of two modules around the code.
        let cell = size / (*width as f32 + 4.0);
        for (index, is_dark) in dark_modules.iter().enumerate() {
            if *is_dark {
                let (column, row) = (index % width, index / width);
                canvas.rect(
                    (column as f32 + 2.0) * cell,
                    (row as f32 + 2.0) * cell,
                    cell + 0.4,
                    cell + 0.4,
                    dark,
                );
            }
        }
    });
}
