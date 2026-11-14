//! Draws secondary windows without a window of their own, through `preview_window`, and checks
//! the picture: its size, which window it shows and the appearance it uses.
//!
//! Each test skips itself when the machine has no GPU adapter.

use std::path::PathBuf;

use rustle::prelude::*;
use rustle::{WindowOptions, WindowTheme, preview, preview_window, window};
use rustle_graphics::ImageData;

/// Whether a GPU adapter can be had, which the pictures need.
fn have_gpu() -> bool {
    rustle_wgpu::Renderer::offscreen(8, 8).is_ok()
}

/// Where the test writes its picture, inside the build directory.
fn output(name: &str) -> PathBuf {
    let directory = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    std::fs::create_dir_all(&directory).unwrap();
    directory.join(format!("{name}.png"))
}

/// Decodes the PNG at `path` and removes it.
fn decoded(path: &std::path::Path) -> ImageData {
    let image = ImageData::decode(&std::fs::read(path).unwrap()).unwrap();
    std::fs::remove_file(path).unwrap();
    image
}

/// The RGBA bytes of `image` at (`x`, `y`).
fn pixel(image: &ImageData, x: u32, y: u32) -> [u8; 4] {
    let start = ((y * image.width() + x) * 4) as usize;
    image.pixels()[start..start + 4].try_into().unwrap()
}

/// A window of `size` filled with `color`.
fn filled(title: &str, size: (f32, f32), color: Color) {
    window(
        WindowOptions::new(title).with_size(size.0, size.1),
        || {},
        move || {
            stack_with(Modifier::new().fill_max_size().background(color), || {});
        },
    );
}

#[test]
fn the_picture_is_as_large_as_the_windows_size_option() {
    if !have_gpu() {
        return;
    }
    let path = output("window_size");
    preview_window(
        || filled("One", (120.0, 80.0), Color::rgb(255, 0, 0)),
        0,
        true,
        &path,
    )
    .unwrap();
    let image = decoded(&path);
    assert_eq!((image.width(), image.height()), (120, 80));
    assert_eq!(&pixel(&image, 60, 40)[..3], [255, 0, 0]);
}

#[test]
fn the_index_counts_windows_in_the_order_they_opened() {
    if !have_gpu() {
        return;
    }
    let root = || {
        filled("One", (64.0, 64.0), Color::rgb(255, 0, 0));
        filled("Two", (96.0, 48.0), Color::rgb(0, 0, 255));
    };
    let first = output("window_first");
    let second = output("window_second");
    preview_window(root, 0, true, &first).unwrap();
    preview_window(root, 1, true, &second).unwrap();
    let (first, second) = (decoded(&first), decoded(&second));
    assert_eq!((first.width(), first.height()), (64, 64));
    assert_eq!((second.width(), second.height()), (96, 48));
    assert_eq!(&pixel(&first, 32, 32)[..3], [255, 0, 0]);
    assert_eq!(&pixel(&second, 48, 24)[..3], [0, 0, 255]);
}

#[test]
fn a_window_that_was_not_opened_is_an_error() {
    if !have_gpu() {
        return;
    }
    let path = output("window_missing");
    let none = preview_window(|| {}, 0, true, &path);
    assert!(none.is_err());
    let beyond = preview_window(
        || filled("One", (32.0, 32.0), Color::rgb(0, 0, 0)),
        1,
        true,
        &path,
    );
    assert!(beyond.is_err());
    assert!(!path.exists());
}

#[test]
fn windows_do_not_carry_over_between_previews() {
    if !have_gpu() {
        return;
    }
    let path = output("window_carry");
    preview_window(
        || filled("One", (32.0, 32.0), Color::rgb(0, 0, 0)),
        0,
        true,
        &path,
    )
    .unwrap();
    decoded(&path);
    assert!(preview_window(|| {}, 0, true, &path).is_err());
}

#[test]
fn a_window_theme_overrides_the_appearance_asked_for() {
    if !have_gpu() {
        return;
    }
    let path = output("window_theme");
    let root = || {
        window(
            WindowOptions::new("Themed")
                .with_size(40.0, 40.0)
                .with_theme(WindowTheme::Light),
            || {},
            || {
                let dark = is_system_in_dark_theme();
                let color = if dark {
                    Color::rgb(255, 0, 0)
                } else {
                    Color::rgb(0, 255, 0)
                };
                stack_with(Modifier::new().fill_max_size().background(color), || {});
            },
        );
    };
    preview_window(root, 0, true, &path).unwrap();
    assert_eq!(&pixel(&decoded(&path), 20, 20)[..3], [0, 255, 0]);
}

#[test]
fn without_a_theme_the_window_uses_the_appearance_asked_for() {
    if !have_gpu() {
        return;
    }
    let root = || {
        window(
            WindowOptions::new("Plain").with_size(40.0, 40.0),
            || {},
            || {
                let color = if is_system_in_dark_theme() {
                    Color::rgb(255, 0, 0)
                } else {
                    Color::rgb(0, 255, 0)
                };
                stack_with(Modifier::new().fill_max_size().background(color), || {});
            },
        );
    };
    for (dark, expected) in [(true, [255, 0, 0]), (false, [0, 255, 0])] {
        let path = output("window_dark");
        preview_window(root, 0, dark, &path).unwrap();
        assert_eq!(&pixel(&decoded(&path), 20, 20)[..3], expected);
    }
}

#[test]
fn the_root_previews_without_its_windows() {
    if !have_gpu() {
        return;
    }
    let path = output("window_root");
    preview(
        || {
            filled("Other", (32.0, 32.0), Color::rgb(0, 0, 255));
            stack_with(
                Modifier::new()
                    .fill_max_size()
                    .background(Color::rgb(255, 0, 0)),
                || {},
            );
        },
        (50, 30),
        true,
        &path,
    )
    .unwrap();
    let image = decoded(&path);
    assert_eq!((image.width(), image.height()), (50, 30));
    assert_eq!(&pixel(&image, 25, 15)[..3], [255, 0, 0]);
}

#[test]
fn a_window_that_closes_itself_still_previews() {
    if !have_gpu() {
        return;
    }
    let path = output("window_closes_itself");
    preview_window(
        || {
            window(
                WindowOptions::new("Gone")
                    .with_size(20.0, 20.0)
                    .with_closes_itself(true),
                || {},
                || {
                    stack_with(
                        Modifier::new()
                            .fill_max_size()
                            .background(Color::rgb(9, 9, 9)),
                        || {},
                    )
                },
            );
        },
        0,
        true,
        &path,
    )
    .unwrap();
    assert_eq!(decoded(&path).width(), 20);
}
