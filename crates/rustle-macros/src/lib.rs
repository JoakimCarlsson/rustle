//! Procedural macros for Rustle.

use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, parse_macro_input, parse_quote};

/// Marks a function as a composable.
///
/// The body runs in a group identified by the call site, so remembered state
/// and effects inside it belong to that call. The function is marked
/// `#[track_caller]` for this, and must be called during composition.
///
/// `#[composable(skippable)]` also skips the body when it is called again with
/// equal arguments and none of the state it read has changed. Every argument
/// must then be `Clone + PartialEq + 'static`, so references cannot be taken.
#[proc_macro_attribute]
pub fn composable(attribute: TokenStream, item: TokenStream) -> TokenStream {
    let skippable = attribute.to_string().trim() == "skippable";
    let mut function = parse_macro_input!(item as ItemFn);
    let body = &function.block;
    function.block = if skippable {
        let arguments: Vec<_> = function
            .sig
            .inputs
            .iter()
            .filter_map(|input| match input {
                FnArg::Typed(typed) => match &*typed.pat {
                    Pat::Ident(ident) => Some(ident.ident.clone()),
                    _ => None,
                },
                FnArg::Receiver(_) => None,
            })
            .collect();
        parse_quote!({
            ::rustle::skippable(( #( #arguments.clone(), )* ), move || #body)
        })
    } else {
        parse_quote!({ ::rustle::group(move || #body) })
    };
    function.attrs.push(parse_quote!(#[track_caller]));
    quote!(#function).into()
}
