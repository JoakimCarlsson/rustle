use rustle_unit::Dp;

use super::Element;
use super::Modifier;
use crate::layout::BoxAlignment;
use crate::scroll::ScrollState;

impl Modifier {
    /// Adds the same space on all sides.
    pub fn padding(self, all: impl Into<Dp>) -> Self {
        let all = all.into();
        self.padding_xy(all, all)
    }

    /// Adds `horizontal` space left and right and `vertical` space above and below.
    pub fn padding_xy(self, horizontal: impl Into<Dp>, vertical: impl Into<Dp>) -> Self {
        let (horizontal, vertical) = (horizontal.into().0, vertical.into().0);
        self.padding_each(horizontal, vertical, horizontal, vertical)
    }

    /// Adds a different space on each side.
    pub fn padding_each(
        self,
        left: impl Into<Dp>,
        top: impl Into<Dp>,
        right: impl Into<Dp>,
        bottom: impl Into<Dp>,
    ) -> Self {
        self.then(Element::Padding {
            left: left.into().0,
            top: top.into().0,
            right: right.into().0,
            bottom: bottom.into().0,
        })
    }

    /// Adds a different space on each side, with `start` and `end` on the left
    /// and right in a left-to-right layout and swapped in a right-to-left one.
    pub fn padding_directional(
        self,
        start: impl Into<Dp>,
        top: impl Into<Dp>,
        end: impl Into<Dp>,
        bottom: impl Into<Dp>,
    ) -> Self {
        if crate::is_rtl() {
            self.padding_each(end, top, start, bottom)
        } else {
            self.padding_each(start, top, end, bottom)
        }
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

    /// Bounds the width to at least `min` and at most `max`.
    pub fn width_in(self, min: impl Into<Dp>, max: impl Into<Dp>) -> Self {
        self.then(Element::SizeIn {
            min_width: Some(min.into().0),
            max_width: Some(max.into().0),
            min_height: None,
            max_height: None,
        })
    }

    /// Bounds the height to at least `min` and at most `max`.
    pub fn height_in(self, min: impl Into<Dp>, max: impl Into<Dp>) -> Self {
        self.then(Element::SizeIn {
            min_width: None,
            max_width: None,
            min_height: Some(min.into().0),
            max_height: Some(max.into().0),
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

    /// Sizes the element to the largest `width / height = ratio` that fits.
    pub fn aspect_ratio(self, ratio: f32) -> Self {
        self.then(Element::AspectRatio(ratio))
    }

    /// Moves the element without changing the space it takes.
    pub fn offset(self, x: impl Into<Dp>, y: impl Into<Dp>) -> Self {
        self.then(Element::Offset {
            x: x.into().0,
            y: y.into().0,
        })
    }

    /// Shares the space left in a row or column with the other weighted
    /// children, in proportion to `weight`. Ignored anywhere else.
    pub fn weight(self, weight: f32) -> Self {
        self.then(Element::Weight(weight))
    }

    /// Places the element within a stack. Ignored anywhere else.
    pub fn align(self, alignment: BoxAlignment) -> Self {
        self.then(Element::Align(alignment))
    }

    /// Paints the area over everything with a lower `z_index`, whatever its
    /// place in the tree, and lets it receive input first. The default is 0.
    /// For popups, menus and tooltips that must not be covered by later siblings.
    ///
    /// The area is still cut to the clips (`clip`, `clip_rounded`, scrolling
    /// areas) around it, as if it had stayed where it is in the tree. Use
    /// [`Modifier::z_index_unclipped`] for something that must reach outside them.
    pub fn z_index(self, z_index: i32) -> Self {
        self.then(Element::ZIndex(z_index))
    }

    /// Like [`Modifier::z_index`], but the area is not cut to the clips around
    /// it. For popups, menus and dialogs that open inside a scrolling or
    /// clipped area and must be able to leave it.
    pub fn z_index_unclipped(self, z_index: i32) -> Self {
        self.then(Element::ZIndexUnclipped(z_index))
    }

    /// Cuts off whatever the content draws outside the area. The cut is
    /// rectangular, whatever rounding the background has.
    pub fn clip(self) -> Self {
        self.then(Element::Clip)
    }

    /// Cuts off whatever the content draws outside the area, which is rounded
    /// at every corner by `radius`. Unlike [`Modifier::clip`] the edge is
    /// antialiased, and it applies to everything inside: child backgrounds,
    /// pictures, text and outlines. Clips nest, and a rounded clip may sit
    /// inside a rectangular one and the other way round.
    ///
    /// The content is drawn to a texture first, so prefer [`Modifier::clip`]
    /// where the corners are not rounded. Pointer input is still cut to the
    /// rectangle.
    pub fn clip_rounded(self, radius: impl Into<Dp>) -> Self {
        let radius = radius.into().0;
        self.then(Element::ClipRounded([radius; 4]))
    }

    /// Like [`Modifier::clip_rounded`], with a radius for each corner: top
    /// left, top right, bottom right and bottom left.
    pub fn clip_rounded_corners(
        self,
        top_left: impl Into<Dp>,
        top_right: impl Into<Dp>,
        bottom_right: impl Into<Dp>,
        bottom_left: impl Into<Dp>,
    ) -> Self {
        self.then(Element::ClipRounded([
            top_left.into().0,
            top_right.into().0,
            bottom_right.into().0,
            bottom_left.into().0,
        ]))
    }

    /// Lets the content scroll vertically, however tall it is.
    pub fn vertical_scroll(self, state: &ScrollState) -> Self {
        self.then(Element::Scroll {
            state: state.clone(),
            vertical: true,
        })
    }

    /// Lets the content scroll horizontally, however wide it is.
    pub fn horizontal_scroll(self, state: &ScrollState) -> Self {
        self.then(Element::Scroll {
            state: state.clone(),
            vertical: false,
        })
    }
}
