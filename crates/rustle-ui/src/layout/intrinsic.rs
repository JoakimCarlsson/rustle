use rustle_text::{StyledSpan, TextMeasurer};

use super::Children;
use crate::modifier::Element;
use crate::{Content, Node};

/// Which intrinsic measurement is asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Query {
    MinWidth,
    MaxWidth,
    MinHeight,
    MaxHeight,
}

impl Query {
    /// Whether the answer is a width.
    pub(super) fn is_width(self) -> bool {
        matches!(self, Self::MinWidth | Self::MaxWidth)
    }
}

/// Answers `query` for `node` with all its modifiers applied. `extent` is the
/// size on the other axis: the height for a width query, the width for a height query.
pub(super) fn intrinsic_node(
    node: &Node,
    query: Query,
    extent: f32,
    text: &mut dyn TextMeasurer,
) -> f32 {
    intrinsic_modified(&node.modifier.elements, node, query, extent, text)
}

/// Applies the modifier `elements` from the outermost in, then asks the node itself.
pub(super) fn intrinsic_modified(
    elements: &[Element],
    node: &Node,
    query: Query,
    extent: f32,
    text: &mut dyn TextMeasurer,
) -> f32 {
    let Some((element, rest)) = elements.split_first() else {
        return intrinsic_content(node, query, extent, text);
    };
    match element {
        Element::Padding {
            left,
            top,
            right,
            bottom,
        } => {
            let (own, cross) = if query.is_width() {
                (left + right, top + bottom)
            } else {
                (top + bottom, left + right)
            };
            intrinsic_modified(rest, node, query, (extent - cross).max(0.0), text) + own
        }
        Element::Size { width, height } => {
            let (own, cross) = if query.is_width() {
                (width, height)
            } else {
                (height, width)
            };
            match own {
                Some(length) => *length,
                None => intrinsic_modified(rest, node, query, cross.unwrap_or(extent), text),
            }
        }
        Element::SizeIn {
            min_width,
            max_width,
            min_height,
            max_height,
        } => {
            let (min, max, cross_min, cross_max) = if query.is_width() {
                (min_width, max_width, min_height, max_height)
            } else {
                (min_height, max_height, min_width, max_width)
            };
            let extent = extent
                .max(cross_min.unwrap_or(0.0))
                .min(cross_max.unwrap_or(f32::INFINITY));
            let length = intrinsic_modified(rest, node, query, extent, text);
            length
                .max(min.unwrap_or(0.0))
                .min(max.unwrap_or(f32::INFINITY))
        }
        Element::AspectRatio(ratio) if *ratio > 0.0 && extent.is_finite() => {
            if query.is_width() {
                extent * ratio
            } else {
                extent / ratio
            }
        }
        _ => intrinsic_modified(rest, node, query, extent, text),
    }
}

/// Answers `query` for what the node itself holds.
fn intrinsic_content(node: &Node, query: Query, extent: f32, text: &mut dyn TextMeasurer) -> f32 {
    match &node.content {
        Content::Text { content, style, .. } => match query {
            Query::MinWidth => text.measure(content, style, 0.0).width,
            Query::MaxWidth => text.measure(content, style, f32::INFINITY).width,
            Query::MinHeight | Query::MaxHeight => text.measure(content, style, extent).height,
        },
        Content::RichText { spans, style, .. } => {
            let spans: Vec<StyledSpan> = spans.iter().map(|span| span.resolve(style)).collect();
            match query {
                Query::MinWidth => text.measure_spans(&spans, style, 0.0).width,
                Query::MaxWidth => text.measure_spans(&spans, style, f32::INFINITY).width,
                Query::MinHeight | Query::MaxHeight => {
                    text.measure_spans(&spans, style, extent).height
                }
            }
        }
        Content::Canvas { .. } => 0.0,
        Content::Kept(kept) => intrinsic_node(&kept.node, query, extent, text),
        Content::Scope(scope) => {
            scope.record_read();
            let node = scope
                .output::<Node>()
                .expect("a restartable block has not run");
            intrinsic_node(&node, query, extent, text)
        }
        Content::Image { image, .. } => {
            if query.is_width() {
                image.width() as f32
            } else {
                image.height() as f32
            }
        }
        Content::Layout { policy, children } => {
            let mut scope = Children {
                nodes: children,
                extra: Vec::new(),
                measured: children.iter().map(|_| None).collect(),
                text,
            };
            match query {
                Query::MinWidth => policy.min_intrinsic_width(&mut scope, extent),
                Query::MaxWidth => policy.max_intrinsic_width(&mut scope, extent),
                Query::MinHeight => policy.min_intrinsic_height(&mut scope, extent),
                Query::MaxHeight => policy.max_intrinsic_height(&mut scope, extent),
            }
        }
    }
}
