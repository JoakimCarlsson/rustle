//! A small set of stroked icons, drawn on a canvas so they take any size and color.

use rustle_foundation::{Canvas, Modifier, canvas};
use rustle_graphics::Color;

/// An icon in the set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Home,
    Dashboard,
    Activity,
    BarChart,
    Folder,
    File,
    User,
    Users,
    Settings,
    Search,
    Plus,
    Check,
    X,
    ChevronDown,
    ChevronUp,
    ChevronRight,
    ChevronLeft,
    ChevronsUpDown,
    ArrowRight,
    ArrowUpRight,
    TrendingUp,
    TrendingDown,
    MoreVertical,
    MoreHorizontal,
    Calendar,
    Bell,
    CreditCard,
    Menu,
    PanelLeft,
    Mail,
    Grip,
    Wallet,
    Layers,
    Help,
    Circle,
    Lock,
    Receipt,
    Send,
}

/// Emits `icon`, `size` square, stroked in `color`.
#[track_caller]
pub fn icon(icon: Icon, size: f32, color: Color) {
    canvas(Modifier::new().size(size, size), move |canvas| {
        let k = size / 24.0;
        let width = (2.0 * k).max(1.3);
        draw(canvas, icon, k, width, color);
    });
}

fn draw(canvas: &mut Canvas, icon: Icon, k: f32, width: f32, color: Color) {
    let p = |x: f32, y: f32| (x * k, y * k);
    let line = |canvas: &mut Canvas, points: &[(f32, f32)]| {
        let scaled: Vec<(f32, f32)> = points.iter().map(|(x, y)| p(*x, *y)).collect();
        canvas.polyline(&scaled, width, color);
    };
    let ring = |canvas: &mut Canvas, x: f32, y: f32, radius: f32| {
        canvas.stroke_circle(x * k, y * k, radius * k, width, color);
    };
    let frame = |canvas: &mut Canvas, x: f32, y: f32, w: f32, h: f32, r: f32| {
        canvas.stroke_rect(x * k, y * k, w * k, h * k, r * k, width, color);
    };
    let dot = |canvas: &mut Canvas, x: f32, y: f32| {
        canvas.circle(x * k, y * k, (1.4 * k).max(0.9), color);
    };
    match icon {
        Icon::Home => {
            line(canvas, &[(3.0, 11.0), (12.0, 3.0), (21.0, 11.0)]);
            line(
                canvas,
                &[(5.0, 9.5), (5.0, 20.0), (19.0, 20.0), (19.0, 9.5)],
            );
            line(
                canvas,
                &[(10.0, 20.0), (10.0, 14.0), (14.0, 14.0), (14.0, 20.0)],
            );
        }
        Icon::Dashboard => {
            frame(canvas, 3.0, 3.0, 7.0, 9.0, 1.5);
            frame(canvas, 14.0, 3.0, 7.0, 5.0, 1.5);
            frame(canvas, 14.0, 12.0, 7.0, 9.0, 1.5);
            frame(canvas, 3.0, 16.0, 7.0, 5.0, 1.5);
        }
        Icon::Activity => line(
            canvas,
            &[
                (22.0, 12.0),
                (18.0, 12.0),
                (15.0, 21.0),
                (9.0, 3.0),
                (6.0, 12.0),
                (2.0, 12.0),
            ],
        ),
        Icon::BarChart => {
            line(canvas, &[(12.0, 20.0), (12.0, 10.0)]);
            line(canvas, &[(18.0, 20.0), (18.0, 4.0)]);
            line(canvas, &[(6.0, 20.0), (6.0, 16.0)]);
        }
        Icon::Folder => line(
            canvas,
            &[
                (3.0, 6.0),
                (9.0, 6.0),
                (11.0, 9.0),
                (21.0, 9.0),
                (21.0, 19.0),
                (3.0, 19.0),
                (3.0, 6.0),
            ],
        ),
        Icon::File => {
            line(
                canvas,
                &[
                    (14.0, 2.5),
                    (6.0, 2.5),
                    (5.0, 3.5),
                    (5.0, 20.5),
                    (6.0, 21.5),
                    (18.0, 21.5),
                    (19.0, 20.5),
                    (19.0, 8.0),
                    (14.0, 2.5),
                ],
            );
            line(canvas, &[(14.0, 2.5), (14.0, 8.0), (19.0, 8.0)]);
        }
        Icon::User => {
            ring(canvas, 12.0, 8.0, 4.0);
            line(
                canvas,
                &[
                    (4.5, 21.0),
                    (4.5, 18.5),
                    (7.5, 15.0),
                    (16.5, 15.0),
                    (19.5, 18.5),
                    (19.5, 21.0),
                ],
            );
        }
        Icon::Users => {
            ring(canvas, 9.0, 8.0, 3.5);
            line(
                canvas,
                &[
                    (2.5, 20.0),
                    (2.5, 18.0),
                    (5.0, 15.0),
                    (13.0, 15.0),
                    (15.5, 18.0),
                    (15.5, 20.0),
                ],
            );
            ring(canvas, 17.5, 8.5, 2.8);
            line(
                canvas,
                &[(18.0, 15.0), (20.5, 16.0), (21.5, 18.0), (21.5, 20.0)],
            );
        }
        Icon::Settings => {
            ring(canvas, 12.0, 12.0, 3.0);
            ring(canvas, 12.0, 12.0, 7.5);
            for tick in 0..8 {
                let angle = tick as f32 * std::f32::consts::FRAC_PI_4;
                let (sin, cos) = angle.sin_cos();
                line(
                    canvas,
                    &[
                        (12.0 + sin * 7.5, 12.0 - cos * 7.5),
                        (12.0 + sin * 10.5, 12.0 - cos * 10.5),
                    ],
                );
            }
        }
        Icon::Search => {
            ring(canvas, 11.0, 11.0, 7.0);
            line(canvas, &[(16.5, 16.5), (21.0, 21.0)]);
        }
        Icon::Plus => {
            line(canvas, &[(12.0, 5.0), (12.0, 19.0)]);
            line(canvas, &[(5.0, 12.0), (19.0, 12.0)]);
        }
        Icon::Check => line(canvas, &[(4.5, 12.5), (9.5, 17.5), (19.5, 6.5)]),
        Icon::X => {
            line(canvas, &[(6.0, 6.0), (18.0, 18.0)]);
            line(canvas, &[(18.0, 6.0), (6.0, 18.0)]);
        }
        Icon::ChevronDown => line(canvas, &[(6.0, 9.0), (12.0, 15.0), (18.0, 9.0)]),
        Icon::ChevronUp => line(canvas, &[(6.0, 15.0), (12.0, 9.0), (18.0, 15.0)]),
        Icon::ChevronRight => line(canvas, &[(9.0, 6.0), (15.0, 12.0), (9.0, 18.0)]),
        Icon::ChevronLeft => line(canvas, &[(15.0, 6.0), (9.0, 12.0), (15.0, 18.0)]),
        Icon::ChevronsUpDown => {
            line(canvas, &[(7.0, 9.0), (12.0, 4.0), (17.0, 9.0)]);
            line(canvas, &[(7.0, 15.0), (12.0, 20.0), (17.0, 15.0)]);
        }
        Icon::ArrowRight => {
            line(canvas, &[(4.0, 12.0), (20.0, 12.0)]);
            line(canvas, &[(14.0, 6.0), (20.0, 12.0), (14.0, 18.0)]);
        }
        Icon::ArrowUpRight => {
            line(canvas, &[(7.0, 17.0), (17.0, 7.0)]);
            line(canvas, &[(8.0, 7.0), (17.0, 7.0), (17.0, 16.0)]);
        }
        Icon::TrendingUp => {
            line(
                canvas,
                &[(2.5, 17.0), (9.0, 10.5), (13.0, 14.5), (21.5, 6.0)],
            );
            line(canvas, &[(15.0, 6.0), (21.5, 6.0), (21.5, 12.5)]);
        }
        Icon::TrendingDown => {
            line(
                canvas,
                &[(2.5, 7.0), (9.0, 13.5), (13.0, 9.5), (21.5, 18.0)],
            );
            line(canvas, &[(15.0, 18.0), (21.5, 18.0), (21.5, 11.5)]);
        }
        Icon::MoreVertical => {
            dot(canvas, 12.0, 5.0);
            dot(canvas, 12.0, 12.0);
            dot(canvas, 12.0, 19.0);
        }
        Icon::MoreHorizontal => {
            dot(canvas, 5.0, 12.0);
            dot(canvas, 12.0, 12.0);
            dot(canvas, 19.0, 12.0);
        }
        Icon::Calendar => {
            frame(canvas, 3.0, 5.0, 18.0, 16.0, 2.5);
            line(canvas, &[(3.0, 10.0), (21.0, 10.0)]);
            line(canvas, &[(8.0, 3.0), (8.0, 7.0)]);
            line(canvas, &[(16.0, 3.0), (16.0, 7.0)]);
        }
        Icon::Bell => {
            line(
                canvas,
                &[
                    (6.0, 17.0),
                    (6.0, 11.0),
                    (9.0, 6.0),
                    (15.0, 6.0),
                    (18.0, 11.0),
                    (18.0, 17.0),
                ],
            );
            line(canvas, &[(4.0, 17.0), (20.0, 17.0)]);
            line(canvas, &[(10.0, 20.5), (14.0, 20.5)]);
        }
        Icon::CreditCard => {
            frame(canvas, 2.0, 5.0, 20.0, 14.0, 2.5);
            line(canvas, &[(2.0, 10.0), (22.0, 10.0)]);
        }
        Icon::Menu => {
            line(canvas, &[(4.0, 6.0), (20.0, 6.0)]);
            line(canvas, &[(4.0, 12.0), (20.0, 12.0)]);
            line(canvas, &[(4.0, 18.0), (20.0, 18.0)]);
        }
        Icon::PanelLeft => {
            frame(canvas, 3.0, 4.0, 18.0, 16.0, 2.5);
            line(canvas, &[(9.0, 4.0), (9.0, 20.0)]);
        }
        Icon::Mail => {
            frame(canvas, 2.5, 4.5, 19.0, 15.0, 2.5);
            line(canvas, &[(3.0, 7.0), (12.0, 13.5), (21.0, 7.0)]);
        }
        Icon::Grip => {
            for (x, y) in [
                (9.0, 5.5),
                (15.0, 5.5),
                (9.0, 12.0),
                (15.0, 12.0),
                (9.0, 18.5),
                (15.0, 18.5),
            ] {
                dot(canvas, x, y);
            }
        }
        Icon::Wallet => {
            frame(canvas, 3.0, 6.0, 18.0, 14.0, 2.5);
            line(canvas, &[(3.0, 9.5), (17.0, 9.5)]);
            dot(canvas, 17.0, 14.5);
        }
        Icon::Layers => {
            line(
                canvas,
                &[
                    (12.0, 3.0),
                    (21.0, 8.0),
                    (12.0, 13.0),
                    (3.0, 8.0),
                    (12.0, 3.0),
                ],
            );
            line(canvas, &[(3.0, 12.5), (12.0, 17.5), (21.0, 12.5)]);
            line(canvas, &[(3.0, 16.5), (12.0, 21.5), (21.0, 16.5)]);
        }
        Icon::Help => {
            ring(canvas, 12.0, 12.0, 9.0);
            line(
                canvas,
                &[
                    (9.5, 9.5),
                    (10.5, 7.8),
                    (13.5, 7.8),
                    (14.5, 9.8),
                    (12.0, 12.0),
                    (12.0, 13.5),
                ],
            );
            dot(canvas, 12.0, 16.5);
        }
        Icon::Circle => ring(canvas, 12.0, 12.0, 9.0),
        Icon::Lock => {
            frame(canvas, 5.0, 11.0, 14.0, 10.0, 2.5);
            line(
                canvas,
                &[
                    (8.0, 11.0),
                    (8.0, 8.0),
                    (10.0, 5.0),
                    (14.0, 5.0),
                    (16.0, 8.0),
                    (16.0, 11.0),
                ],
            );
        }
        Icon::Receipt => {
            line(
                canvas,
                &[
                    (5.0, 3.0),
                    (5.0, 21.0),
                    (8.0, 19.0),
                    (12.0, 21.0),
                    (16.0, 19.0),
                    (19.0, 21.0),
                    (19.0, 3.0),
                    (5.0, 3.0),
                ],
            );
            line(canvas, &[(9.0, 9.0), (15.0, 9.0)]);
            line(canvas, &[(9.0, 13.0), (15.0, 13.0)]);
        }
        Icon::Send => {
            line(canvas, &[(21.0, 3.0), (10.0, 14.0)]);
            line(
                canvas,
                &[
                    (21.0, 3.0),
                    (14.5, 21.0),
                    (10.0, 14.0),
                    (3.0, 9.5),
                    (21.0, 3.0),
                ],
            );
        }
    }
}
