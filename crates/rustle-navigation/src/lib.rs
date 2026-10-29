//! Navigation between screens with a back stack.
//!
//! A [`NavController`] holds the stack of routes. [`nav_host`] shows the
//! screen of the route on top. A screen that is left, by navigating away or
//! popping back, leaves the composition and loses what it remembered.

use rustle_runtime::{State, group, key, remember};

/// The routes visited, oldest first, each with an id that tells repeated visits apart.
#[derive(Clone)]
struct Entry<R> {
    id: u64,
    route: R,
}

/// Navigates a back stack of routes of type `R`, typically an enum of screens.
#[derive(Clone)]
pub struct NavController<R> {
    stack: State<Vec<Entry<R>>>,
    next_id: State<u64>,
}

impl<R: Clone + 'static> NavController<R> {
    /// The route on top of the stack.
    pub fn current(&self) -> R {
        self.stack.with(|stack| {
            stack
                .last()
                .expect("the back stack is never empty")
                .route
                .clone()
        })
    }

    /// How many routes are on the stack, the first one included.
    pub fn depth(&self) -> usize {
        self.stack.with(Vec::len)
    }

    /// Whether [`NavController::pop_back`] has somewhere to go.
    pub fn can_go_back(&self) -> bool {
        self.depth() > 1
    }

    /// Shows `route` on top of the current one.
    pub fn navigate(&self, route: R) {
        let id = self.take_id();
        self.stack.update(|stack| stack.push(Entry { id, route }));
    }

    /// Shows `route` in place of the current one.
    pub fn replace(&self, route: R) {
        let id = self.take_id();
        self.stack.update(|stack| {
            stack.pop();
            stack.push(Entry { id, route });
        });
    }

    /// Clears the stack down to `route`, which becomes its only entry.
    pub fn reset(&self, route: R) {
        let id = self.take_id();
        self.stack.set(vec![Entry { id, route }]);
    }

    /// Goes back one screen. Returns false at the first screen, where there is nowhere to go.
    pub fn pop_back(&self) -> bool {
        if !self.can_go_back() {
            return false;
        }
        self.stack.update(|stack| {
            stack.pop();
        });
        true
    }

    fn take_id(&self) -> u64 {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        id
    }
}

/// Remembers a [`NavController`] at this call site, starting at `start`.
#[track_caller]
pub fn remember_nav_controller<R: Clone + 'static>(start: R) -> NavController<R> {
    group(|| NavController {
        stack: remember(|| {
            vec![Entry {
                id: 0,
                route: start,
            }]
        }),
        next_id: remember(|| 1),
    })
}

/// Emits the screen of the route on top of `controller`'s stack, by calling
/// `screen` with it. Each visit to a route is its own composition, so two
/// visits to the same route do not share state.
#[track_caller]
pub fn nav_host<R: Clone + 'static>(controller: &NavController<R>, screen: impl FnOnce(R)) {
    group(|| {
        let id = controller
            .stack
            .with(|stack| stack.last().expect("the back stack is never empty").id);
        let route = controller.current();
        key(id, || screen(route));
    });
}
