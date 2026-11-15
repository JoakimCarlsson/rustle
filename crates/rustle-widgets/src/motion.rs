//! Animated visibility, crossfades, pagers and swipe to dismiss.

use rustle_animation::{animate_float_as_state, animate_value, remember_animatable, tween};
use rustle_foundation::{
    Alignment, Arrangement, Modifier, ScrollState, reveal, row_arranged, row_with, spacer,
    stack_with,
};
use rustle_runtime::{key, launched_effect, remember};

use crate::theme::color_scheme;

/// Emits `content` while `visible`, fading and expanding it in and collapsing
/// and fading it out. The content leaves the composition, and forgets its
/// state, once it has finished disappearing.
#[track_caller]
pub fn animated_visibility(visible: bool, content: impl FnOnce()) {
    let progress = animate_float_as_state(if visible { 1.0 } else { 0.0 }, tween(220)).get();
    if visible || progress > 0.001 {
        reveal(progress, Modifier::new().alpha(progress), content);
    }
}

/// Emits `content` for `target`, and when `target` changes fades the old
/// content out as the new fades in.
#[track_caller]
pub fn crossfade<T: Clone + PartialEq + std::hash::Hash + 'static>(
    target: T,
    content: impl Fn(&T),
) {
    let current = remember(|| target.clone());
    let previous = remember(|| None::<T>);
    let progress = remember_animatable(1.0_f32);
    if current.get() != target {
        previous.set(Some(current.get()));
        current.set(target.clone());
        progress.snap_to(0.0);
    }

    let (fade, outgoing) = (progress.clone(), previous.clone());
    launched_effect(target.clone(), move |scope| async move {
        fade.animate_to(&scope, 1.0, tween(250)).await;
        outgoing.set(None);
    });

    let faded = progress.value();
    let old = previous.get();
    stack_with(Modifier::new(), || {
        if let Some(old) = &old {
            stack_with(Modifier::new().alpha(1.0 - faded), || content(old));
        }
        let alpha = if old.is_some() { faded } else { 1.0 };
        stack_with(Modifier::new().alpha(alpha), || content(&current.get()));
    });
}

/// Scrolls `state` to `target` over time. Run it from a `launched_effect`.
pub async fn animate_scroll_to(
    scope: &rustle_runtime::TaskScope,
    state: &ScrollState,
    target: f32,
    spec: rustle_animation::AnimationSpec,
) {
    let from = state.offset();
    let state = state.clone();
    animate_value(scope, from, target, spec, move |offset| {
        state.scroll_to(offset)
    })
    .await;
}

/// What a pager remembers: which page it is on, and where it is scrolled.
#[derive(Clone)]
pub struct PagerState {
    scroll: ScrollState,
    /// Where the last drag event was, to turn positions into movement.
    last_x: std::rc::Rc<std::cell::Cell<Option<f32>>>,
    /// The page to settle on, and a count that changes to ask for a settle.
    settle: rustle_runtime::State<(usize, u64)>,
}

impl PagerState {
    /// How wide a page is: the pager's width as of the latest layout.
    fn page_width(&self) -> f32 {
        match self.scroll.viewport_length() {
            width if width > 0.0 => width,
            _ => 400.0,
        }
    }

    /// The page nearest to what is showing.
    pub fn current_page(&self) -> usize {
        (self.scroll.offset() / self.page_width()).round().max(0.0) as usize
    }

    /// Slides to `page`.
    pub fn animate_to_page(&self, page: usize) {
        self.settle
            .update(|(target, requests)| (*target, *requests) = (page, *requests + 1));
    }
}

/// Remembers a [`PagerState`] at this call site.
#[track_caller]
pub fn remember_pager_state() -> PagerState {
    rustle_runtime::group(|| PagerState {
        scroll: rustle_foundation::remember_scroll_state(),
        last_x: remember(std::rc::Rc::default).get(),
        settle: remember(|| (0, 0)),
    })
}

/// Emits `count` pages side by side, each as wide as the pager, that can be
/// swiped between. `page` emits the page at an index. Size it with `modifier`.
#[track_caller]
pub fn horizontal_pager(
    modifier: Modifier,
    state: &PagerState,
    count: usize,
    page: impl Fn(usize),
) {
    rustle_runtime::group(|| {
        let width = state.page_width();
        let max_page = count.saturating_sub(1);

        // Settling on a page slides there, and a swipe that ends asks for it.
        let (settle, scroll) = (state.settle.get(), state.scroll.clone());
        launched_effect(settle.1, move |scope| async move {
            let target = settle.0.min(max_page) as f32 * width;
            animate_scroll_to(&scope, &scroll, target, tween(260)).await;
        });

        let (drag, release) = (state.clone(), state.clone());
        row_with(
            // The drag handlers sit outside the scrolling, so positions are
            // measured from a corner that stays put while the pages move.
            modifier
                .clip()
                .on_drag_end(move || {
                    release.last_x.set(None);
                    let nearest = release.current_page().min(max_page);
                    release.animate_to_page(nearest);
                })
                .on_drag(move |at| {
                    if let Some(last) = drag.last_x.get() {
                        drag.scroll.scroll_by(last - at.x);
                    }
                    drag.last_x.set(Some(at.x));
                })
                .horizontal_scroll(&state.scroll),
            || {
                for index in 0..count {
                    key(index, || {
                        stack_with(Modifier::new().width(width).fill_max_height(), || {
                            page(index)
                        });
                    });
                }
            },
        );
    });
}

/// Emits a row of dots, `count` of them, with the one at `current` filled.
#[track_caller]
pub fn page_indicator(count: usize, current: usize) {
    let theme = color_scheme();
    row_arranged(
        Modifier::new(),
        Arrangement::SpacedBy(8.0),
        Alignment::Center,
        || {
            for index in 0..count {
                key(index, || {
                    let color = if index == current {
                        theme.primary
                    } else {
                        theme.outline
                    };
                    spacer(
                        Modifier::new()
                            .size(8.0, 8.0)
                            .background_rounded(color, 4.0),
                    );
                });
            }
        },
    );
}

/// Emits `content` that can be swiped sideways away; past a third of its width it
/// is dismissed, calling `on_dismiss`, and otherwise it slides back.
#[track_caller]
pub fn swipe_to_dismiss(on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    rustle_runtime::group(|| {
        let offset = remember(|| 0.0_f32);
        let last_x = remember(std::rc::Rc::<std::cell::Cell<Option<f32>>>::default).get();
        let width = remember(|| std::rc::Rc::new(std::cell::Cell::new(300.0_f32))).get();
        let release = remember(|| 0_u64);

        let (settle, position, size) = (release.get(), offset.clone(), width.get());
        let on_dismiss = std::rc::Rc::new(on_dismiss);
        let dismiss = std::rc::Rc::clone(&on_dismiss);
        launched_effect(settle, move |scope| async move {
            if settle == 0 {
                return;
            }
            let from = position.get();
            let gone = from.abs() > size / 3.0;
            let to = if gone { from.signum() * size } else { 0.0 };
            let moving = position.clone();
            animate_value(&scope, from, to, tween(180), move |value| moving.set(value)).await;
            if gone {
                dismiss();
            }
        });

        let (drag, end) = (
            (offset.clone(), std::rc::Rc::clone(&last_x)),
            (release.clone(), last_x),
        );
        let measured = std::rc::Rc::clone(&width);
        let fade = 1.0 - (offset.get().abs() / width.get()).min(1.0);
        stack_with(
            // The drag handlers sit outside the offset, so the pointer is
            // measured from a corner that stays put while the content moves.
            Modifier::new()
                .on_size_changed(move |size| measured.set(size.width.max(1.0)))
                .on_drag_end(move || {
                    end.1.set(None);
                    end.0.update(|requests| *requests += 1);
                })
                .on_drag(move |at| {
                    if let Some(last) = drag.1.get() {
                        drag.0.update(|offset| *offset += at.x - last);
                    }
                    drag.1.set(Some(at.x));
                })
                .offset(offset.get(), 0.0)
                .alpha(fade),
            content,
        );
    });
}
