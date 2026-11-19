//! Indications: how a component shows its interactions, as Compose's `Indication` and
//! `IndicationNodeFactory` describe it.

use std::rc::Rc;

use rustle_runtime::CompositionLocal;

use crate::interaction_source::InteractionSource;
use crate::modifier::Modifier;

/// Makes what draws an indication for one component: Compose's `IndicationNodeFactory`.
///
/// `create` is given the component's interactions and returns the modifier that draws them,
/// typically a [`draw_with_content`](Modifier::draw_with_content) that draws the content and
/// then a hover or press overlay, or a focus ring, from the states the source gives. Reading
/// those states inside the drawing closure draws the area again when they change, without
/// composing.
///
/// ```ignore
/// struct Overlay;
///
/// impl IndicationNodeFactory for Overlay {
///     fn create(&self, interaction_source: &InteractionSource) -> Modifier {
///         let pressed = interaction_source.collect_is_pressed_as_state();
///         let hovered = interaction_source.collect_is_hovered_as_state();
///         Modifier::new().draw_with_content(move |canvas| {
///             canvas.draw_content();
///             let alpha = if pressed.get() { 40 } else if hovered.get() { 20 } else { 0 };
///             canvas.rect(0.0, 0.0, canvas.width(), canvas.height(), Color::rgba(0, 0, 0, alpha));
///         })
///     }
/// }
/// ```
pub trait IndicationNodeFactory {
    /// The modifier that shows the interactions `interaction_source` reports.
    fn create(&self, interaction_source: &InteractionSource) -> Modifier;
}

/// How components show their interactions: Compose's `Indication`, a shared
/// [`IndicationNodeFactory`]. Cheap to clone; two are equal when they are the same factory.
#[derive(Clone)]
pub struct Indication(Rc<dyn IndicationNodeFactory>);

impl Indication {
    /// An indication made by `factory`.
    pub fn new(factory: impl IndicationNodeFactory + 'static) -> Self {
        Self(Rc::new(factory))
    }

    /// The modifier that shows the interactions `interaction_source` reports.
    pub fn create(&self, interaction_source: &InteractionSource) -> Modifier {
        self.0.create(interaction_source)
    }
}

impl PartialEq for Indication {
    /// Whether both are the same factory.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl std::fmt::Debug for Indication {
    /// Names the type only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Indication")
    }
}

/// The indication components use when they are not given one: Compose's `LocalIndication`.
/// None by default, so components draw only what they draw themselves.
pub static LOCAL_INDICATION: CompositionLocal<Option<Indication>> = CompositionLocal::new(|| None);
