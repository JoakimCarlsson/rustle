//! Modifiers: an ordered chain of changes to how an element is sized, drawn and laid out.

use std::rc::Rc;

use rustle_graphics::Color;
use rustle_unit::Dp;

#[derive(Clone)]
pub(crate) enum Element {
    Padding {
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
    },
    Size {
        width: Option<f32>,
        height: Option<f32>,
    },
    FillMax {
        width: bool,
        height: bool,
    },
    Background {
        color: Color,
        corner_radius: f32,
    },
    Clickable(Rc<dyn Fn()>),
}

/// A chain of modifiers, applied from the first call (outermost) to the last.
///
/// Order matters: `padding` before `background` leaves a margin outside the
/// background, and after it pads inside.
#[derive(Clone, Default)]
pub struct Modifier {
    pub(crate) elements: Vec<Element>,
}

impl Modifier {
    pub fn new() -> Self {
        Self::default()
    }

    fn then(mut self, element: Element) -> Self {
        self.elements.push(element);
        self
    }

    /// Adds the same space on all sides.
    pub fn padding(self, all: impl Into<Dp>) -> Self {
        let all = all.into();
        self.padding_xy(all, all)
    }

    /// Adds `horizontal` space left and right and `vertical` space above and below.
    pub fn padding_xy(self, horizontal: impl Into<Dp>, vertical: impl Into<Dp>) -> Self {
        let (horizontal, vertical) = (horizontal.into().0, vertical.into().0);
        self.then(Element::Padding {
            left: horizontal,
            top: vertical,
            right: horizontal,
            bottom: vertical,
        })
    }

    /// Requests an exact size, within what the parent allows.
    pub fn size(self, width: impl Into<Dp>, height: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: Some(width.into().0),
            height: Some(height.into().0),
        })
    }

    /// Requests an exact width, within what the parent allows.
    pub fn width(self, width: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: Some(width.into().0),
            height: None,
        })
    }

    /// Requests an exact height, within what the parent allows.
    pub fn height(self, height: impl Into<Dp>) -> Self {
        self.then(Element::Size {
            width: None,
            height: Some(height.into().0),
        })
    }

    /// Takes all the width the parent offers, when it is bounded.
    pub fn fill_max_width(self) -> Self {
        self.then(Element::FillMax {
            width: true,
            height: false,
        })
    }

    /// Takes all the height the parent offers, when it is bounded.
    pub fn fill_max_height(self) -> Self {
        self.then(Element::FillMax {
            width: false,
            height: true,
        })
    }

    /// Takes all the space the parent offers, when it is bounded.
    pub fn fill_max_size(self) -> Self {
        self.then(Element::FillMax {
            width: true,
            height: true,
        })
    }

    /// Runs `on_click` when the area is pressed and released with the primary button.
    ///
    /// The area is whatever the modifiers before this one add up to, so
    /// `padding` before `clickable` makes the padding clickable too.
    pub fn clickable(self, on_click: impl Fn() + 'static) -> Self {
        self.then(Element::Clickable(Rc::new(on_click)))
    }

    /// Fills the area with `color`.
    pub fn background(self, color: Color) -> Self {
        self.background_rounded(color, 0.0)
    }

    /// Fills the area with `color`, with corners rounded to `corner_radius`.
    pub fn background_rounded(self, color: Color, corner_radius: impl Into<Dp>) -> Self {
        self.then(Element::Background {
            color,
            corner_radius: corner_radius.into().0,
        })
    }
}
