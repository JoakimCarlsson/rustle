//! Content that animates as it changes: animated content, animated size and
//! elements shared between two places.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;

use rustle_animation::{AnimationSpec, animate_float_as_state, remember_animatable, tween};
use rustle_foundation::{Modifier, Placement, sized_box, stack_with};
use rustle_geometry::{Offset, Size};
use rustle_runtime::{State, group, launched_effect, remember};

/// How content comes in: any mix of a fade, a slide and a scale, from a
/// starting look to its normal one. Combine them with [`EnterTransition::and`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EnterTransition {
    fade_from: Option<f32>,
    slide_from: Option<(f32, f32)>,
    scale_from: Option<f32>,
}

impl EnterTransition {
    /// No effect: the content is just there.
    pub fn none() -> Self {
        Self::default()
    }

    /// Fades in from `initial_alpha` (0 to 1).
    pub fn fade_in(initial_alpha: f32) -> Self {
        Self {
            fade_from: Some(initial_alpha),
            ..Self::default()
        }
    }

    /// Slides in from `fraction` of its own width to the side (negative for
    /// the left).
    pub fn slide_in_horizontally(fraction: f32) -> Self {
        Self {
            slide_from: Some((fraction, 0.0)),
            ..Self::default()
        }
    }

    /// Slides in from `fraction` of its own height below (negative for above).
    pub fn slide_in_vertically(fraction: f32) -> Self {
        Self {
            slide_from: Some((0.0, fraction)),
            ..Self::default()
        }
    }

    /// Grows in from `initial_scale` of its size.
    pub fn scale_in(initial_scale: f32) -> Self {
        Self {
            scale_from: Some(initial_scale),
            ..Self::default()
        }
    }

    /// Both effects at once; where both set the same kind, `other` wins.
    pub fn and(self, other: Self) -> Self {
        Self {
            fade_from: other.fade_from.or(self.fade_from),
            slide_from: other.slide_from.or(self.slide_from),
            scale_from: other.scale_from.or(self.scale_from),
        }
    }

    /// Dresses `modifier` for the look at `progress`, 0 as it starts and 1
    /// once it has come in, for content of `size`.
    pub fn apply(&self, modifier: Modifier, progress: f32, size: (f32, f32)) -> Modifier {
        let remaining = 1.0 - progress;
        let alpha = self
            .fade_from
            .map_or(1.0, |from| from + (1.0 - from) * progress);
        let scale = self
            .scale_from
            .map_or(1.0, |from| from + (1.0 - from) * progress);
        let (x, y) = self.slide_from.unwrap_or((0.0, 0.0));
        modifier
            .offset(x * size.0 * remaining, y * size.1 * remaining)
            .graphics_layer(alpha, 0.0, scale)
    }
}

/// How content goes out: any mix of a fade, a slide and a scale, from its
/// normal look to a final one. Combine them with [`ExitTransition::and`].
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExitTransition {
    fade_to: Option<f32>,
    slide_to: Option<(f32, f32)>,
    scale_to: Option<f32>,
}

impl ExitTransition {
    /// No effect: the content is just gone once the change is over.
    pub fn none() -> Self {
        Self::default()
    }

    /// Fades out to `target_alpha` (0 to 1).
    pub fn fade_out(target_alpha: f32) -> Self {
        Self {
            fade_to: Some(target_alpha),
            ..Self::default()
        }
    }

    /// Slides out to `fraction` of its own width to the side (negative for the left).
    pub fn slide_out_horizontally(fraction: f32) -> Self {
        Self {
            slide_to: Some((fraction, 0.0)),
            ..Self::default()
        }
    }

    /// Slides out to `fraction` of its own height below (negative for above).
    pub fn slide_out_vertically(fraction: f32) -> Self {
        Self {
            slide_to: Some((0.0, fraction)),
            ..Self::default()
        }
    }

    /// Shrinks out to `target_scale` of its size.
    pub fn scale_out(target_scale: f32) -> Self {
        Self {
            scale_to: Some(target_scale),
            ..Self::default()
        }
    }

    /// Both effects at once; where both set the same kind, `other` wins.
    pub fn and(self, other: Self) -> Self {
        Self {
            fade_to: other.fade_to.or(self.fade_to),
            slide_to: other.slide_to.or(self.slide_to),
            scale_to: other.scale_to.or(self.scale_to),
        }
    }

    /// Dresses `modifier` for the look at `progress`, 0 as it starts to leave
    /// and 1 once it has gone, for content of `size`.
    pub fn apply(&self, modifier: Modifier, progress: f32, size: (f32, f32)) -> Modifier {
        let alpha = self.fade_to.map_or(1.0, |to| 1.0 + (to - 1.0) * progress);
        let scale = self.scale_to.map_or(1.0, |to| 1.0 + (to - 1.0) * progress);
        let (x, y) = self.slide_to.unwrap_or((0.0, 0.0));
        modifier
            .offset(x * size.0 * progress, y * size.1 * progress)
            .graphics_layer(alpha, 0.0, scale)
    }
}

/// What [`animated_content`] does when its target changes: how the new
/// content comes in, how the old goes out, and how long it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentTransform {
    pub enter: EnterTransition,
    pub exit: ExitTransition,
    /// The animation both run on.
    pub spec: AnimationSpec,
    /// Whether the area also animates its size between the two contents.
    pub size_transform: bool,
}

impl ContentTransform {
    /// A transform with `enter` and `exit`, over 300 ms, animating the size.
    pub fn new(enter: EnterTransition, exit: ExitTransition) -> Self {
        Self {
            enter,
            exit,
            spec: tween(300),
            size_transform: true,
        }
    }

    /// The same transform on `spec`.
    pub fn with_spec(self, spec: AnimationSpec) -> Self {
        Self { spec, ..self }
    }

    /// The same transform, with the size animated or not.
    pub fn with_size_transform(self, size_transform: bool) -> Self {
        Self {
            size_transform,
            ..self
        }
    }
}

impl Default for ContentTransform {
    fn default() -> Self {
        Self::new(EnterTransition::fade_in(0.0), ExitTransition::fade_out(0.0))
    }
}

/// Emits `content` for `target`, and when `target` changes runs the
/// [`ContentTransform`] that `transition` picks for the state left and the
/// state reached: the old content goes out as the new comes in.
#[track_caller]
pub fn animated_content<T: Clone + PartialEq + Hash + 'static>(
    target: T,
    transition: impl Fn(&T, &T) -> ContentTransform,
    content: impl Fn(&T),
) {
    group(|| {
        let current = remember(|| target.clone());
        let previous = remember(|| None::<T>);
        let transform = remember(ContentTransform::default);
        let progress = remember_animatable(1.0_f32);
        let incoming_size = remember(|| Rc::new(Cell::new((0.0_f32, 0.0_f32)))).get();
        let outgoing_size = remember(|| Rc::new(Cell::new((0.0_f32, 0.0_f32)))).get();
        if current.get() != target {
            let next = transition(&current.get(), &target);
            outgoing_size.set(incoming_size.get());
            previous.set(Some(current.get()));
            current.set(target.clone());
            transform.set(next);
            progress.snap_to(0.0);
        }

        let (run, outgoing, spec) = (progress.clone(), previous.clone(), transform.get().spec);
        launched_effect(target.clone(), move |scope| async move {
            if outgoing.get().is_none() {
                return;
            }
            run.animate_to(&scope, 1.0, spec).await;
            outgoing.set(None);
        });

        let now = progress.value();
        let old = previous.get();
        let transform = transform.get();
        let body = || {
            stack_with(Modifier::new(), || {
                if let Some(old) = &old {
                    let sized = Rc::clone(&outgoing_size);
                    let modifier = transform
                        .exit
                        .apply(
                            Modifier::new().layout(|content, constraints| {
                                content.measure(constraints);
                                Placement::new(Size::default(), Offset::default())
                            }),
                            now,
                            outgoing_size.get(),
                        )
                        .on_size_changed(move |size| sized.set((size.width, size.height)));
                    stack_with(modifier, || content(old));
                }
                let progress = if old.is_some() { now } else { 1.0 };
                let sized = Rc::clone(&incoming_size);
                let modifier = transform
                    .enter
                    .apply(Modifier::new(), progress, incoming_size.get())
                    .on_size_changed(move |size| sized.set((size.width, size.height)));
                stack_with(modifier, || content(&current.get()));
            });
        };
        if transform.size_transform {
            animate_content_size(Modifier::new(), transform.spec, body);
        } else {
            body();
        }
    });
}

/// Emits `content` in an area whose size follows the content's, gliding to
/// each new size as `spec` says instead of jumping. What does not fit while
/// it moves is cut off.
#[track_caller]
pub fn animate_content_size(modifier: Modifier, spec: AnimationSpec, content: impl FnOnce()) {
    group(|| {
        let natural: State<(f32, f32)> = remember(|| (0.0, 0.0));
        let (width, height) = natural.get();
        let measured = width > 0.0 || height > 0.0;
        let animated = measured.then(|| {
            (
                animate_float_as_state(width, spec).get(),
                animate_float_as_state(height, spec).get(),
            )
        });
        let report = natural.clone();
        sized_box(
            modifier,
            animated,
            move |found| {
                if report.get() != found {
                    report.set(found);
                }
            },
            content,
        );
    });
}

/// Where elements shared between places were last drawn, so an element that
/// appears somewhere new can glide from where its twin was. Make one with
/// [`remember_shared_elements`] and pass it to every [`shared_element`] that
/// should be able to hand over to another.
#[derive(Clone, Default)]
pub struct SharedElements {
    last_origin: Rc<RefCell<HashMap<u64, (f32, f32)>>>,
}

/// Remembers a [`SharedElements`] at this call site.
#[track_caller]
pub fn remember_shared_elements() -> SharedElements {
    remember(SharedElements::default).get()
}

/// Emits `content` as the element called `id`. When an element with the same
/// `id` was drawn somewhere else just before (it left the composition as this
/// one entered), this one starts at the other's position and glides to its
/// own over `spec`: a shared-element transition between two composables.
#[track_caller]
pub fn shared_element<K: Hash>(
    elements: &SharedElements,
    id: K,
    modifier: Modifier,
    spec: AnimationSpec,
    content: impl FnOnce(),
) {
    group(|| {
        let id = {
            let mut hasher = DefaultHasher::new();
            id.hash(&mut hasher);
            hasher.finish()
        };
        let gap = remember(|| (0.0_f32, 0.0_f32));
        let remaining = remember_animatable(0.0_f32);
        let launches = remember(|| 0_u32);
        let placed_once = remember(|| Rc::new(Cell::new(false))).get();

        let (glide, drive) = (remaining.clone(), launches.get());
        launched_effect(drive, move |scope| async move {
            if drive > 0 {
                glide.animate_to(&scope, 0.0, spec).await;
            }
        });

        let (x, y) = gap.get();
        let left = remaining.value();
        let (map, gap_state, remaining_state, launches_state) = (
            Rc::clone(&elements.last_origin),
            gap.clone(),
            remaining.clone(),
            launches.clone(),
        );
        let modifier = modifier
            .offset(x * left, y * left)
            .on_placed(move |rect, _| {
                let at = (rect.origin.x, rect.origin.y);
                if !placed_once.replace(true) {
                    let before = map.borrow().get(&id).copied();
                    if let Some(before) = before.filter(|before| *before != at) {
                        gap_state.set((before.0 - at.0, before.1 - at.1));
                        remaining_state.snap_to(1.0);
                        launches_state.update(|count| *count += 1);
                    }
                }
                map.borrow_mut().insert(id, at);
            });
        stack_with(modifier, content);
    });
}
