//! Ripple: how a pressed or hovered area answers the pointer, as Compose Material's `ripple`
//! and `RippleConfiguration`.

use rustle_animation::{remember_animatable, tween};
use rustle_foundation::Modifier;
use rustle_foundation::{PointerButton, PointerPhase};
use rustle_geometry::Offset;
use rustle_graphics::Color;
use rustle_runtime::{CompositionLocal, launched_effect, mutable_state_of};

use crate::shape::{Shape, ShapeModifier};

/// How ripples look: their color and how strongly hovering and pressing tint the area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RippleConfiguration {
    /// The color of the ripple, or `None` for the color of the content it is over.
    pub color: Option<Color>,
    /// The opacity of the tint while the pointer is over the area, from `0.0` to `1.0`.
    pub hovered_alpha: f32,
    /// The opacity of the ripple while the area is pressed, from `0.0` to `1.0`.
    pub pressed_alpha: f32,
}

impl Default for RippleConfiguration {
    fn default() -> Self {
        Self {
            color: None,
            hovered_alpha: 0.08,
            pressed_alpha: 0.10,
        }
    }
}

static LOCAL_RIPPLE_CONFIGURATION: CompositionLocal<Option<RippleConfiguration>> =
    CompositionLocal::new(|| Some(RippleConfiguration::default()));

/// The ripple configuration in effect, as Compose's `LocalRippleConfiguration`: `None` when
/// ripples are turned off.
pub fn ripple_configuration() -> Option<RippleConfiguration> {
    LOCAL_RIPPLE_CONFIGURATION.current()
}

/// Runs `content` with `configuration` as the ripple configuration, or with ripples turned off
/// for `None`.
pub fn provide_ripple_configuration<R>(
    configuration: Option<RippleConfiguration>,
    content: impl FnOnce() -> R,
) -> R {
    LOCAL_RIPPLE_CONFIGURATION.provide(configuration, content)
}

/// Modifiers that answer the pointer with a ripple.
pub trait RippleModifier: Sized {
    /// Runs `on_click` when the area is pressed and released, and shows a ripple that spreads from
    /// where it was pressed, clipped to `shape`, drawn in `content_color` unless the
    /// [`RippleConfiguration`] gives another. Hovering tints the area. Put it before the
    /// background so the ripple is drawn over it.
    ///
    /// Call it while composing: it remembers the state of the ripple.
    #[track_caller]
    fn ripple_clickable(
        self,
        shape: Shape,
        content_color: Color,
        on_click: impl Fn() + 'static,
    ) -> Self;
}

impl RippleModifier for Modifier {
    #[track_caller]
    fn ripple_clickable(
        self,
        shape: Shape,
        content_color: Color,
        on_click: impl Fn() + 'static,
    ) -> Self {
        let Some(configuration) = ripple_configuration() else {
            return self.clickable_plain(on_click);
        };
        let color = configuration.color.unwrap_or(content_color);
        let hovered = mutable_state_of(false);
        let down = mutable_state_of(false);
        let origin = mutable_state_of(Offset::new(0.0, 0.0));
        let presses = mutable_state_of(0_u32);
        let spread = remember_animatable(0.0_f32);
        let fade = remember_animatable(0.0_f32);

        let (grow, pressed) = (spread.clone(), presses.get());
        let started = fade.clone();
        launched_effect(pressed, move |scope| async move {
            if pressed == 0 {
                return;
            }
            grow.snap_to(0.0);
            started.snap_to(1.0);
            grow.animate_to(&scope, 1.0, tween(300)).await;
        });
        let (vanish, released) = (fade.clone(), !down.get());
        launched_effect((presses.get(), released), move |scope| async move {
            if released && pressed > 0 {
                vanish.animate_to(&scope, 0.0, tween(200)).await;
            }
        });

        let (spread_now, fade_now) = (spread.value(), fade.value());
        let (is_hovered, is_down, at) = (hovered.get(), down.get(), origin.get());
        let (set_hover, set_down, set_origin, count) = (
            hovered.clone(),
            down.clone(),
            origin.clone(),
            presses.clone(),
        );

        self.on_hover(move |over| set_hover.set(over))
            .on_pointer_button(PointerButton::Primary, move |event| match event.phase {
                PointerPhase::Press => {
                    set_origin.set(event.position);
                    count.update(|n| *n += 1);
                    set_down.set(true);
                }
                PointerPhase::Release | PointerPhase::Click => set_down.set(false),
                PointerPhase::Drag => {}
            })
            .clickable_plain(on_click)
            .clip_shape(shape)
            .draw_with_content(move |canvas| {
                canvas.draw_content();
                let (w, h) = (canvas.width(), canvas.size().height);
                if is_hovered && !is_down {
                    canvas.rounded_rect(
                        0.0,
                        0.0,
                        w,
                        h,
                        shape.radius(),
                        faded(color, configuration.hovered_alpha),
                    );
                }
                if pressed > 0 && fade_now > 0.0 {
                    let farthest = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
                        .into_iter()
                        .map(|(x, y)| ((x - at.x).powi(2) + (y - at.y).powi(2)).sqrt())
                        .fold(0.0, f32::max);
                    let radius = farthest * spread_now;
                    canvas.rounded_rect(
                        at.x - radius,
                        at.y - radius,
                        2.0 * radius,
                        2.0 * radius,
                        radius,
                        faded(color, configuration.pressed_alpha * fade_now),
                    );
                }
            })
    }
}

/// `color` at `alpha` of its own opacity.
fn faded(color: Color, alpha: f32) -> Color {
    Color::rgba(
        color.r,
        color.g,
        color.b,
        (f32::from(color.a) * alpha.clamp(0.0, 1.0)) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustle_ui::composition;

    /// Composes `content` once and returns what it made.
    fn compose<T: 'static>(content: impl Fn() -> T + 'static) -> T {
        let out = std::rc::Rc::new(std::cell::RefCell::new(None));
        let kept = std::rc::Rc::clone(&out);
        let mut composition = composition(move || {
            *kept.borrow_mut() = Some(content());
        });
        composition.compose();
        out.take().expect("the root composed")
    }

    /// Ripples are on by default and a subtree can change or turn them off.
    #[test]
    fn configuration_nests_and_turns_off() {
        let (outer, custom, off) = compose(|| {
            let custom = RippleConfiguration {
                pressed_alpha: 0.5,
                ..RippleConfiguration::default()
            };
            (
                ripple_configuration(),
                provide_ripple_configuration(Some(custom), ripple_configuration),
                provide_ripple_configuration(None, ripple_configuration),
            )
        });
        assert_eq!(outer, Some(RippleConfiguration::default()));
        assert_eq!(custom.map(|c| c.pressed_alpha), Some(0.5));
        assert_eq!(off, None);
    }

    /// Fading keeps the color and scales its opacity.
    #[test]
    fn fading_scales_opacity() {
        let faded = faded(Color::rgb(10, 20, 30), 0.5);
        assert_eq!((faded.r, faded.g, faded.b, faded.a), (10, 20, 30, 127));
    }
}
