//! Design-system-agnostic building blocks: layouts and basic elements.
//!
//! Everything here is a composable built on the primitives of `rustle-ui`.

use rustle_ui::{Node, collect, emit};

/// Emits a text element into the current scope.
pub fn text(content: impl Into<String>) {
    emit(Node::Text(content.into()));
}

/// Emits a column whose children are emitted by `content`.
pub fn column(content: impl FnOnce()) {
    emit(Node::Column(collect(content)));
}
