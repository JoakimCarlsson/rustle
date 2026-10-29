//! Procedural macros for Rustle.

use proc_macro::TokenStream;

/// Marks a function as a composable.
///
/// For now this is a pass-through: the function is emitted unchanged.
/// Recomposition and state tracking will hook in here later.
#[proc_macro_attribute]
pub fn composable(_attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}
