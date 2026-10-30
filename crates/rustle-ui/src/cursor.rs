//! The mouse cursor shapes an area can ask for.

/// The shape of the mouse cursor, chosen for an area with [`Modifier::cursor`](crate::Modifier::cursor).
///
/// These are the shapes every desktop platform has. The window layer maps them to the
/// platform's own cursors.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum CursorIcon {
    /// The ordinary arrow.
    #[default]
    Default,
    /// A pointing hand, for something that can be clicked.
    Pointer,
    /// A text insertion beam.
    Text,
    /// A crosshair.
    Crosshair,
    /// An open hand, for something that can be grabbed.
    Grab,
    /// A closed hand, for something being dragged.
    Grabbing,
    /// Something that cannot be done here.
    NotAllowed,
    /// Something is busy.
    Wait,
    /// Move in any direction.
    Move,
    /// Resize left and right.
    EwResize,
    /// Resize up and down.
    NsResize,
    /// Resize along the diagonal from the top right to the bottom left.
    NeswResize,
    /// Resize along the diagonal from the top left to the bottom right.
    NwseResize,
    /// Resize a column.
    ColResize,
    /// Resize a row.
    RowResize,
}
