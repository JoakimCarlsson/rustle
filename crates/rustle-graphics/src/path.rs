use std::borrow::Cow;

/// One step of a vector outline, in the path's own units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    /// A quadratic curve: control point, then end point.
    QuadTo(f32, f32, f32, f32),
    /// A cubic curve: two control points, then end point.
    CubicTo(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Which parts of a self-overlapping outline count as inside.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FillRule {
    #[default]
    NonZero,
    EvenOdd,
}

/// A filled vector outline, such as an icon, drawn into a rectangle at any size.
///
/// Coordinates are in a `width` by `height` box of the path's own units, which
/// is stretched over the rectangle the path is drawn into. A path made of
/// `'static` data is free to clone and is cached by renderers.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    commands: Cow<'static, [PathCommand]>,
    width: f32,
    height: f32,
    fill_rule: FillRule,
}

impl Path {
    /// A path of constant data, such as an icon compiled into the program.
    pub const fn from_static(
        commands: &'static [PathCommand],
        width: f32,
        height: f32,
        fill_rule: FillRule,
    ) -> Self {
        Self {
            commands: Cow::Borrowed(commands),
            width,
            height,
            fill_rule,
        }
    }

    /// A path built at run time.
    pub fn new(commands: Vec<PathCommand>, width: f32, height: f32, fill_rule: FillRule) -> Self {
        Self {
            commands: Cow::Owned(commands),
            width,
            height,
            fill_rule,
        }
    }

    pub fn commands(&self) -> &[PathCommand] {
        &self.commands
    }

    /// The size of the box the coordinates are in.
    pub fn size(&self) -> (f32, f32) {
        (self.width, self.height)
    }

    pub fn fill_rule(&self) -> FillRule {
        self.fill_rule
    }

    /// Identifies constant data so renderers can cache what they build from it;
    /// a path built at run time has none.
    pub fn static_key(&self) -> Option<(usize, usize)> {
        match &self.commands {
            Cow::Borrowed(commands) => Some((commands.as_ptr() as usize, commands.len())),
            Cow::Owned(_) => None,
        }
    }
}
