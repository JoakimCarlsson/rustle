//! [Phosphor](https://phosphoricons.com) icons as vector paths.
//!
//! ```ignore
//! use rustle_icons::phosphor;
//!
//! let house = phosphor::HOUSE;
//! let arrow = phosphor::ARROW_UP_RIGHT;
//! let bell = phosphor::bold::BELL; // needs the `bold` feature
//! ```
//!
//! Every icon is a constant, so a misspelt name does not compile and an editor
//! can complete it. Icons that are never used are not in the program. The
//! regular weight is always there; `thin`, `light`, `bold` and `fill` are
//! features, each a module of the same names inside `phosphor`. Phosphor is MIT
//! licensed; its license is in `LICENSE-PHOSPHOR`, and has to travel with
//! copies of the icons.

// The generated constants name this crate by path, including from inside it.
extern crate self as rustle_icons;

/// An icon: a vector outline.
pub type Icon = rustle_graphics::Path;

#[doc(hidden)]
pub mod __private {
    pub use rustle_graphics::{FillRule, Path, PathCommand};
}

/// Every Phosphor icon, as constants named like the icons on phosphoricons.com.
#[allow(clippy::all, missing_docs)]
pub mod phosphor {
    rustle_icons_macros::icons!(regular);

    #[cfg(feature = "thin")]
    pub mod thin {
        rustle_icons_macros::icons!(thin);
    }
    #[cfg(feature = "light")]
    pub mod light {
        rustle_icons_macros::icons!(light);
    }
    #[cfg(feature = "bold")]
    pub mod bold {
        rustle_icons_macros::icons!(bold);
    }
    #[cfg(feature = "fill")]
    pub mod fill {
        rustle_icons_macros::icons!(fill);
    }
}

#[cfg(test)]
mod tests {
    use super::phosphor;

    #[test]
    fn embeds_an_icon() {
        let house = phosphor::HOUSE;
        assert_eq!(house.size(), (256.0, 256.0));
        assert!(house.commands().len() > 10);
        assert!(house.static_key().is_some());
    }

    #[test]
    fn has_names_with_dashes_and_digits() {
        assert!(!phosphor::ARROW_UP_RIGHT.commands().is_empty());
    }
}
