//! Text with styles, paragraph styles, links and annotations on ranges of it: Compose's
//! `AnnotatedString`, built with [`build_annotated_string`] as Compose's
//! `buildAnnotatedString` builds one.

use std::fmt;
use std::rc::Rc;

use rustle_graphics::{Color, FontFamily, TextAlign};

use crate::{FontWeight, TextSpan, TextStyle};

/// A length in text, Compose's `TextUnit`: logical pixels that follow the text size setting,
/// or a multiple of the font size of the text it is used in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TextUnit {
    /// Scaled pixels, Compose's `sp`: logical pixels, as text sizes are given in.
    Sp(f32),
    /// A multiple of the font size of the text around, Compose's `em`.
    Em(f32),
}

impl TextUnit {
    /// The length in logical pixels in text set `font_size` logical pixels tall.
    pub fn to_px(self, font_size: f32) -> f32 {
        match self {
            Self::Sp(value) => value,
            Self::Em(value) => value * font_size,
        }
    }
}

/// How a range of text is set, Compose's `SpanStyle`. Anything left unset comes from the
/// styles around it, and in the end from the text style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpanStyle {
    /// The color of the letters.
    pub color: Option<Color>,
    /// The size of the letters, in logical pixels.
    pub font_size: Option<f32>,
    /// How heavy the strokes are.
    pub font_weight: Option<FontWeight>,
    /// Slanted letterforms, Compose's `FontStyle.Italic`.
    pub italic: Option<bool>,
    /// The family the letters are taken from.
    pub font_family: Option<FontFamily>,
    /// Extra space after every character, in logical pixels.
    pub letter_spacing: Option<f32>,
    /// A fill behind the range, as tall as its line.
    pub background: Option<Color>,
    /// A line under the range, Compose's `TextDecoration.Underline`.
    pub underline: Option<bool>,
    /// A line through the range, Compose's `TextDecoration.LineThrough`.
    pub strikethrough: Option<bool>,
    /// Whether the font's ligatures are used.
    pub ligatures: Option<bool>,
}

impl SpanStyle {
    /// A style that sets nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the color of the letters.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Sets the size of the letters, in logical pixels.
    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = Some(font_size);
        self
    }

    /// Sets how heavy the strokes are.
    pub fn font_weight(mut self, font_weight: FontWeight) -> Self {
        self.font_weight = Some(font_weight);
        self
    }

    /// Sets the letters in bold.
    pub fn bold(self) -> Self {
        self.font_weight(FontWeight::BOLD)
    }

    /// Slants the letters.
    pub fn italic(mut self) -> Self {
        self.italic = Some(true);
        self
    }

    /// Sets the family the letters are taken from.
    pub fn font_family(mut self, font_family: FontFamily) -> Self {
        self.font_family = Some(font_family);
        self
    }

    /// Sets the extra space after every character, in logical pixels.
    pub fn letter_spacing(mut self, letter_spacing: f32) -> Self {
        self.letter_spacing = Some(letter_spacing);
        self
    }

    /// Fills behind the range with `color`.
    pub fn background(mut self, color: Color) -> Self {
        self.background = Some(color);
        self
    }

    /// Draws a line under the range, or takes away one drawn by a style around it.
    pub fn underline(mut self, underline: bool) -> Self {
        self.underline = Some(underline);
        self
    }

    /// Draws a line through the range, or takes away one drawn by a style around it.
    pub fn strikethrough(mut self, strikethrough: bool) -> Self {
        self.strikethrough = Some(strikethrough);
        self
    }

    /// Turns the font's ligatures on or off.
    pub fn ligatures(mut self, ligatures: bool) -> Self {
        self.ligatures = Some(ligatures);
        self
    }

    /// This style with what `other` sets put over it, Compose's `SpanStyle.merge`.
    pub fn merge(&self, other: &SpanStyle) -> SpanStyle {
        SpanStyle {
            color: other.color.or(self.color),
            font_size: other.font_size.or(self.font_size),
            font_weight: other.font_weight.or(self.font_weight),
            italic: other.italic.or(self.italic),
            font_family: other.font_family.or(self.font_family),
            letter_spacing: other.letter_spacing.or(self.letter_spacing),
            background: other.background.or(self.background),
            underline: other.underline.or(self.underline),
            strikethrough: other.strikethrough.or(self.strikethrough),
            ligatures: other.ligatures.or(self.ligatures),
        }
    }

    /// A run of `text` set in this style.
    pub fn span(&self, text: impl Into<String>) -> TextSpan {
        TextSpan {
            text: text.into(),
            color: self.color,
            font_weight: self.font_weight,
            font_family: self.font_family,
            italic: self.italic,
            underline: self.underline,
            strikethrough: self.strikethrough,
            background: self.background,
            font_size: self.font_size,
            letter_spacing: self.letter_spacing,
            ligatures: self.ligatures,
        }
    }
}

/// How a paragraph is set, Compose's `ParagraphStyle`: a range with one is laid out as a
/// paragraph of its own, below the text before it, as if a line break ended the text before.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ParagraphStyle {
    /// How the paragraph's lines line up across its width.
    pub text_align: Option<TextAlign>,
    /// The height of each of the paragraph's lines.
    pub line_height: Option<TextUnit>,
}

impl ParagraphStyle {
    /// A paragraph style that sets nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Lines the paragraph's lines up across its width as `text_align` says.
    pub fn text_align(mut self, text_align: TextAlign) -> Self {
        self.text_align = Some(text_align);
        self
    }

    /// Makes each of the paragraph's lines `line_height` tall.
    pub fn line_height(mut self, line_height: TextUnit) -> Self {
        self.line_height = Some(line_height);
        self
    }

    /// This style with what `other` sets put over it, Compose's `ParagraphStyle.merge`.
    pub fn merge(&self, other: &ParagraphStyle) -> ParagraphStyle {
        ParagraphStyle {
            text_align: other.text_align.or(self.text_align),
            line_height: other.line_height.or(self.line_height),
        }
    }

    /// `style` with what this paragraph style sets put over it.
    pub fn apply(&self, style: TextStyle) -> TextStyle {
        let style = match self.text_align {
            Some(align) => style.align(align),
            None => style,
        };
        match self.line_height {
            Some(height) => style.line_height(height.to_px(style.font_size)),
            None => style,
        }
    }
}

/// What a link pressed is told: the link, Compose's `LinkInteractionListener`.
pub type LinkInteractionListener = Rc<dyn Fn(&LinkAnnotation)>;

/// The styles of a link as the pointer and the keyboard deal with it, Compose's
/// `TextLinkStyles`. Each is put over the link's text when it applies: `style` always, then
/// `focused_style` while the link holds keyboard focus, `hovered_style` while the pointer is
/// over it and `pressed_style` while it is pressed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextLinkStyles {
    /// The style of the link at rest.
    pub style: Option<SpanStyle>,
    /// Put over the link while it holds keyboard focus.
    pub focused_style: Option<SpanStyle>,
    /// Put over the link while the pointer is over it.
    pub hovered_style: Option<SpanStyle>,
    /// Put over the link while it is pressed.
    pub pressed_style: Option<SpanStyle>,
}

impl TextLinkStyles {
    /// Links styled `style` at rest, and no differently when focused, hovered or pressed.
    pub fn new(style: SpanStyle) -> Self {
        Self {
            style: Some(style),
            ..Self::default()
        }
    }

    /// Puts `style` over the link while it holds keyboard focus.
    pub fn focused_style(mut self, style: SpanStyle) -> Self {
        self.focused_style = Some(style);
        self
    }

    /// Puts `style` over the link while the pointer is over it.
    pub fn hovered_style(mut self, style: SpanStyle) -> Self {
        self.hovered_style = Some(style);
        self
    }

    /// Puts `style` over the link while it is pressed.
    pub fn pressed_style(mut self, style: SpanStyle) -> Self {
        self.pressed_style = Some(style);
        self
    }

    /// The style the link is drawn in when `focused`, `hovered` and `pressed` say how it is
    /// being dealt with.
    pub fn resolve(&self, focused: bool, hovered: bool, pressed: bool) -> SpanStyle {
        [
            (true, &self.style),
            (focused, &self.focused_style),
            (hovered, &self.hovered_style),
            (pressed, &self.pressed_style),
        ]
        .into_iter()
        .filter_map(|(applies, style)| style.as_ref().filter(|_| applies))
        .fold(SpanStyle::new(), |merged, style| merged.merge(style))
    }
}

/// A range of text that can be pressed, Compose's `LinkAnnotation`. A link with a listener
/// tells it when pressed; a [`Url`](Self::Url) without one opens its address through the
/// uri handler in effect.
#[derive(Clone)]
pub enum LinkAnnotation {
    /// A link to an address, Compose's `LinkAnnotation.Url`.
    Url {
        /// The address.
        url: String,
        /// How the link is drawn.
        styles: Option<TextLinkStyles>,
        /// Told when the link is pressed, in place of opening the address.
        listener: Option<LinkInteractionListener>,
    },
    /// A link the application acts on itself, Compose's `LinkAnnotation.Clickable`.
    Clickable {
        /// What the application knows the link by.
        tag: String,
        /// How the link is drawn.
        styles: Option<TextLinkStyles>,
        /// Told when the link is pressed.
        listener: Option<LinkInteractionListener>,
    },
}

impl LinkAnnotation {
    /// A link to `url`, opened through the uri handler in effect when pressed.
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url {
            url: url.into(),
            styles: None,
            listener: None,
        }
    }

    /// A link known by `tag` that tells `listener` when it is pressed.
    pub fn clickable(tag: impl Into<String>, listener: impl Fn(&LinkAnnotation) + 'static) -> Self {
        Self::Clickable {
            tag: tag.into(),
            styles: None,
            listener: Some(Rc::new(listener)),
        }
    }

    /// This link drawn with `styles`.
    pub fn with_styles(self, styles: TextLinkStyles) -> Self {
        match self {
            Self::Url { url, listener, .. } => Self::Url {
                url,
                styles: Some(styles),
                listener,
            },
            Self::Clickable { tag, listener, .. } => Self::Clickable {
                tag,
                styles: Some(styles),
                listener,
            },
        }
    }

    /// This link telling `listener` when pressed, in place of what it does by itself.
    pub fn with_listener(self, listener: impl Fn(&LinkAnnotation) + 'static) -> Self {
        let listener: Option<LinkInteractionListener> = Some(Rc::new(listener));
        match self {
            Self::Url { url, styles, .. } => Self::Url {
                url,
                styles,
                listener,
            },
            Self::Clickable { tag, styles, .. } => Self::Clickable {
                tag,
                styles,
                listener,
            },
        }
    }

    /// How the link is drawn.
    pub fn styles(&self) -> Option<&TextLinkStyles> {
        match self {
            Self::Url { styles, .. } | Self::Clickable { styles, .. } => styles.as_ref(),
        }
    }

    /// What the link tells when pressed.
    pub fn listener(&self) -> Option<&LinkInteractionListener> {
        match self {
            Self::Url { listener, .. } | Self::Clickable { listener, .. } => listener.as_ref(),
        }
    }
}

impl PartialEq for LinkAnnotation {
    /// Links are equal when they lead to the same place, styled alike, telling the same
    /// listener.
    fn eq(&self, other: &Self) -> bool {
        let same_listener = match (self.listener(), other.listener()) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        let same_place = match (self, other) {
            (Self::Url { url: a, .. }, Self::Url { url: b, .. }) => a == b,
            (Self::Clickable { tag: a, .. }, Self::Clickable { tag: b, .. }) => a == b,
            _ => false,
        };
        same_place && same_listener && self.styles() == other.styles()
    }
}

impl fmt::Debug for LinkAnnotation {
    /// Writes the link's kind, address or tag and styles, and whether it has a listener.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (kind, place) = match self {
            Self::Url { url, .. } => ("Url", url),
            Self::Clickable { tag, .. } => ("Clickable", tag),
        };
        formatter
            .debug_struct(kind)
            .field("place", place)
            .field("styles", &self.styles())
            .field("listener", &self.listener().is_some())
            .finish()
    }
}

/// Where a placeholder sits on its line, Compose's `PlaceholderVerticalAlign`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PlaceholderVerticalAlign {
    /// Its bottom on the line's baseline.
    #[default]
    AboveBaseline,
    /// Its top at the top of the line.
    Top,
    /// Its bottom at the bottom of the line.
    Bottom,
    /// In the middle of the line.
    Center,
    /// Its top at the top of the letters before it.
    TextTop,
    /// Its bottom at the bottom of the letters before it.
    TextBottom,
    /// In the middle of the letters before it.
    TextCenter,
}

/// Room kept in a line of text for something that is not text, Compose's `Placeholder`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placeholder {
    /// How wide it is.
    pub width: TextUnit,
    /// How tall it is. A line holding a placeholder taller than the line grows to fit it.
    pub height: TextUnit,
    /// Where it sits on its line.
    pub placeholder_vertical_align: PlaceholderVerticalAlign,
}

impl Placeholder {
    /// Room `width` wide and `height` tall, placed on its line as `placeholder_vertical_align`
    /// says.
    pub fn new(
        width: TextUnit,
        height: TextUnit,
        placeholder_vertical_align: PlaceholderVerticalAlign,
    ) -> Self {
        Self {
            width,
            height,
            placeholder_vertical_align,
        }
    }
}

/// Something set on the bytes from `start` to `end` of an [`AnnotatedString`], Compose's
/// `AnnotatedString.Range`.
#[derive(Debug, Clone, PartialEq)]
pub struct AnnotatedRange<T> {
    /// What is set.
    pub item: T,
    /// The first byte it covers.
    pub start: usize,
    /// The byte after the last it covers.
    pub end: usize,
    /// The tag of a string annotation; empty for anything else.
    pub tag: String,
}

impl<T> AnnotatedRange<T> {
    /// `item` set on the bytes from `start` to `end`.
    pub fn new(item: T, start: usize, end: usize) -> Self {
        Self {
            item,
            start,
            end,
            tag: String::new(),
        }
    }

    /// Whether it covers any of the bytes from `start` to `end`, or sits at `start` when it
    /// covers nothing.
    pub fn intersects(&self, start: usize, end: usize) -> bool {
        if self.start == self.end {
            return (start..=end).contains(&self.start);
        }
        self.start < end && start < self.end
    }
}

/// Text with styles, paragraph styles, links and string annotations set on ranges of it,
/// Compose's `AnnotatedString`. Ranges are in bytes of the text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AnnotatedString {
    /// What it says.
    text: String,
    /// The span styles, in the order they were set; later ones go over earlier ones.
    span_styles: Vec<AnnotatedRange<SpanStyle>>,
    /// The paragraph styles, in the order they were set.
    paragraph_styles: Vec<AnnotatedRange<ParagraphStyle>>,
    /// The links, in the order they were set.
    links: Vec<AnnotatedRange<LinkAnnotation>>,
    /// The string annotations, in the order they were set.
    annotations: Vec<AnnotatedRange<String>>,
}

impl AnnotatedString {
    /// `text` with nothing set on it.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    /// What it says.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// How many bytes it has.
    pub fn len(&self) -> usize {
        self.text.len()
    }

    /// Whether it says nothing.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The span styles, in the order they were set.
    pub fn span_styles(&self) -> &[AnnotatedRange<SpanStyle>] {
        &self.span_styles
    }

    /// The paragraph styles, in the order they were set.
    pub fn paragraph_styles(&self) -> &[AnnotatedRange<ParagraphStyle>] {
        &self.paragraph_styles
    }

    /// The links that cover any of the bytes from `start` to `end`, Compose's
    /// `getLinkAnnotations`.
    pub fn get_link_annotations(
        &self,
        start: usize,
        end: usize,
    ) -> Vec<AnnotatedRange<LinkAnnotation>> {
        self.links
            .iter()
            .filter(|range| range.intersects(start, end))
            .cloned()
            .collect()
    }

    /// The string annotations tagged `tag` that cover any of the bytes from `start` to `end`,
    /// Compose's `getStringAnnotations(tag, start, end)`.
    pub fn get_string_annotations(
        &self,
        tag: &str,
        start: usize,
        end: usize,
    ) -> Vec<AnnotatedRange<String>> {
        self.annotations
            .iter()
            .filter(|range| range.tag == tag && range.intersects(start, end))
            .cloned()
            .collect()
    }

    /// Whether any string annotation tagged `tag` covers any of the bytes from `start` to
    /// `end`, Compose's `hasStringAnnotations`.
    pub fn has_string_annotations(&self, tag: &str, start: usize, end: usize) -> bool {
        self.annotations
            .iter()
            .any(|range| range.tag == tag && range.intersects(start, end))
    }

    /// The span styles merged over every stretch of the bytes from `start` to `end` where
    /// they do not change, together with `extra` ranges put over them, as runs of text.
    pub fn spans_in(
        &self,
        start: usize,
        end: usize,
        extra: &[AnnotatedRange<SpanStyle>],
    ) -> Vec<TextSpan> {
        let ranges: Vec<&AnnotatedRange<SpanStyle>> = self
            .span_styles
            .iter()
            .chain(extra)
            .filter(|range| range.start < end && start < range.end)
            .collect();
        let mut cuts: Vec<usize> = ranges
            .iter()
            .flat_map(|range| [range.start, range.end])
            .chain([start, end])
            .filter(|cut| (start..=end).contains(cut))
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        cuts.windows(2)
            .filter(|pair| pair[0] < pair[1])
            .map(|pair| {
                let style = ranges
                    .iter()
                    .filter(|range| range.start <= pair[0] && pair[1] <= range.end)
                    .fold(SpanStyle::new(), |merged, range| merged.merge(&range.item));
                style.span(&self.text[pair[0]..pair[1]])
            })
            .collect()
    }

    /// The paragraphs the text is laid out in, as Compose's `MultiParagraph` lays it out:
    /// each range with a paragraph style is one, and the stretches between them are one each,
    /// as byte ranges with the paragraph style merged over each. Text with no paragraph
    /// styles is one paragraph.
    pub fn paragraphs(&self) -> Vec<AnnotatedRange<ParagraphStyle>> {
        let length = self.text.len();
        let mut cuts: Vec<usize> = self
            .paragraph_styles
            .iter()
            .flat_map(|range| [range.start, range.end])
            .chain([0, length])
            .filter(|cut| *cut <= length)
            .collect();
        cuts.sort_unstable();
        cuts.dedup();
        let paragraphs: Vec<AnnotatedRange<ParagraphStyle>> = cuts
            .windows(2)
            .filter(|pair| pair[0] < pair[1])
            .map(|pair| {
                let style = self
                    .paragraph_styles
                    .iter()
                    .filter(|range| range.start <= pair[0] && pair[1] <= range.end)
                    .fold(ParagraphStyle::new(), |merged, range| {
                        merged.merge(&range.item)
                    });
                AnnotatedRange::new(style, pair[0], pair[1])
            })
            .collect();
        if paragraphs.is_empty() {
            return vec![AnnotatedRange::new(ParagraphStyle::new(), 0, 0)];
        }
        paragraphs
    }
}

impl From<&str> for AnnotatedString {
    /// `text` with nothing set on it.
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

impl From<String> for AnnotatedString {
    /// `text` with nothing set on it.
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

/// What a builder has set, while its range may still be open.
#[derive(Debug, Clone)]
enum Pending {
    /// A span style.
    Span(SpanStyle),
    /// A paragraph style.
    Paragraph(ParagraphStyle),
    /// A link.
    Link(Box<LinkAnnotation>),
    /// A string annotation with its tag.
    Annotation(String, String),
}

/// Builds an [`AnnotatedString`], Compose's `AnnotatedString.Builder`: text is appended, and
/// styles, links and annotations are pushed to cover what is appended until they are popped,
/// or added on ranges of what is there.
#[derive(Debug, Clone, Default)]
pub struct AnnotatedStringBuilder {
    /// What it says so far.
    text: String,
    /// What was set, with where each range starts and, once popped or added, ends.
    pending: Vec<(Pending, usize, Option<usize>)>,
    /// The places in `pending` of the ranges pushed and not popped, the latest last.
    open: Vec<usize>,
}

/// A style a builder can push: a [`SpanStyle`] or a [`ParagraphStyle`], as Compose's
/// `pushStyle` and `withStyle` take either.
pub trait AnnotationStyle {
    /// Pushes this style onto `builder` and returns the index [`AnnotatedStringBuilder::pop_to`]
    /// takes to pop it.
    fn push_onto(self, builder: &mut AnnotatedStringBuilder) -> usize;
}

impl AnnotationStyle for SpanStyle {
    /// Pushes this span style.
    fn push_onto(self, builder: &mut AnnotatedStringBuilder) -> usize {
        builder.push(Pending::Span(self))
    }
}

impl AnnotationStyle for ParagraphStyle {
    /// Pushes this paragraph style.
    fn push_onto(self, builder: &mut AnnotatedStringBuilder) -> usize {
        builder.push(Pending::Paragraph(self))
    }
}

impl AnnotatedStringBuilder {
    /// A builder holding nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many bytes it holds so far, Compose's `length`.
    pub fn length(&self) -> usize {
        self.text.len()
    }

    /// Appends `text`, covered by every range pushed and not popped.
    pub fn append(&mut self, text: &str) {
        self.text.push_str(text);
    }

    /// Appends `text` with everything set on it, moved to where it lands.
    pub fn append_annotated(&mut self, text: &AnnotatedString) {
        let shift = self.text.len();
        self.text.push_str(&text.text);
        let moved = |start: usize, end: usize| (start + shift, Some(end + shift));
        for range in &text.span_styles {
            let (start, end) = moved(range.start, range.end);
            self.pending
                .push((Pending::Span(range.item.clone()), start, end));
        }
        for range in &text.paragraph_styles {
            let (start, end) = moved(range.start, range.end);
            self.pending
                .push((Pending::Paragraph(range.item), start, end));
        }
        for range in &text.links {
            let (start, end) = moved(range.start, range.end);
            self.pending
                .push((Pending::Link(Box::new(range.item.clone())), start, end));
        }
        for range in &text.annotations {
            let (start, end) = moved(range.start, range.end);
            let annotation = Pending::Annotation(range.tag.clone(), range.item.clone());
            self.pending.push((annotation, start, end));
        }
    }

    /// Opens `pending` at the end of the text and returns its index for [`Self::pop_to`].
    fn push(&mut self, pending: Pending) -> usize {
        self.pending.push((pending, self.text.len(), None));
        self.open.push(self.pending.len() - 1);
        self.open.len() - 1
    }

    /// Covers what is appended from now on with `style` until it is popped, and returns its
    /// index for [`Self::pop_to`]: Compose's `pushStyle`.
    pub fn push_style(&mut self, style: impl AnnotationStyle) -> usize {
        style.push_onto(self)
    }

    /// Makes what is appended from now on the link `link` until it is popped, and returns
    /// its index for [`Self::pop_to`]: Compose's `pushLink`.
    pub fn push_link(&mut self, link: LinkAnnotation) -> usize {
        self.push(Pending::Link(Box::new(link)))
    }

    /// Annotates what is appended from now on with `annotation` under `tag` until it is
    /// popped, and returns its index for [`Self::pop_to`]: Compose's `pushStringAnnotation`.
    pub fn push_string_annotation(
        &mut self,
        tag: impl Into<String>,
        annotation: impl Into<String>,
    ) -> usize {
        self.push(Pending::Annotation(tag.into(), annotation.into()))
    }

    /// Ends the range pushed last at the end of the text, Compose's `pop`.
    ///
    /// # Panics
    ///
    /// When no range is open.
    pub fn pop(&mut self) {
        let index = self.open.pop().expect("nothing is pushed to pop");
        self.pending[index].2 = Some(self.text.len());
    }

    /// Ends the ranges pushed since and including the one `index` names, Compose's `pop(index)`.
    ///
    /// # Panics
    ///
    /// When `index` names no open range.
    pub fn pop_to(&mut self, index: usize) {
        assert!(index < self.open.len(), "{index} names no open range");
        while self.open.len() > index {
            self.pop();
        }
    }

    /// Sets `style` on the bytes from `start` to `end`, Compose's `addStyle`.
    pub fn add_style(&mut self, style: SpanStyle, start: usize, end: usize) {
        self.pending.push((Pending::Span(style), start, Some(end)));
    }

    /// Sets the paragraph style `style` on the bytes from `start` to `end`, Compose's
    /// `addStyle` for a paragraph style.
    pub fn add_paragraph_style(&mut self, style: ParagraphStyle, start: usize, end: usize) {
        self.pending
            .push((Pending::Paragraph(style), start, Some(end)));
    }

    /// Makes the bytes from `start` to `end` the link `link`, Compose's `addLink`.
    pub fn add_link(&mut self, link: LinkAnnotation, start: usize, end: usize) {
        self.pending
            .push((Pending::Link(Box::new(link)), start, Some(end)));
    }

    /// Annotates the bytes from `start` to `end` with `annotation` under `tag`, Compose's
    /// `addStringAnnotation`.
    pub fn add_string_annotation(
        &mut self,
        tag: impl Into<String>,
        annotation: impl Into<String>,
        start: usize,
        end: usize,
    ) {
        let annotation = Pending::Annotation(tag.into(), annotation.into());
        self.pending.push((annotation, start, Some(end)));
    }

    /// Runs `block` with `style` covering what it appends, and returns what it returns:
    /// Compose's `withStyle`, for a [`SpanStyle`] or a [`ParagraphStyle`].
    pub fn with_style<R>(
        &mut self,
        style: impl AnnotationStyle,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let index = self.push_style(style);
        let result = block(self);
        self.pop_to(index);
        result
    }

    /// Runs `block` with what it appends being the link `link`, and returns what it returns:
    /// Compose's `withLink`.
    pub fn with_link<R>(&mut self, link: LinkAnnotation, block: impl FnOnce(&mut Self) -> R) -> R {
        let index = self.push_link(link);
        let result = block(self);
        self.pop_to(index);
        result
    }

    /// Runs `block` with what it appends annotated with `annotation` under `tag`, and returns
    /// what it returns: Compose's `withAnnotation`.
    pub fn with_annotation<R>(
        &mut self,
        tag: impl Into<String>,
        annotation: impl Into<String>,
        block: impl FnOnce(&mut Self) -> R,
    ) -> R {
        let index = self.push_string_annotation(tag, annotation);
        let result = block(self);
        self.pop_to(index);
        result
    }

    /// The annotated string built so far, ranges still open ending at the end of the text:
    /// Compose's `toAnnotatedString`.
    pub fn to_annotated_string(&self) -> AnnotatedString {
        let length = self.text.len();
        let mut built = AnnotatedString::new(self.text.clone());
        for (pending, start, end) in &self.pending {
            let (start, end) = (*start, end.unwrap_or(length).min(length));
            match pending {
                Pending::Span(style) => {
                    built
                        .span_styles
                        .push(AnnotatedRange::new(style.clone(), start, end))
                }
                Pending::Paragraph(style) => built
                    .paragraph_styles
                    .push(AnnotatedRange::new(*style, start, end)),
                Pending::Link(link) => {
                    built
                        .links
                        .push(AnnotatedRange::new((**link).clone(), start, end))
                }
                Pending::Annotation(tag, annotation) => built.annotations.push(AnnotatedRange {
                    item: annotation.clone(),
                    start,
                    end,
                    tag: tag.clone(),
                }),
            }
        }
        built
    }
}

/// Builds an annotated string by running `block` on a builder, Compose's
/// `buildAnnotatedString`.
pub fn build_annotated_string(block: impl FnOnce(&mut AnnotatedStringBuilder)) -> AnnotatedString {
    let mut builder = AnnotatedStringBuilder::new();
    block(&mut builder);
    builder.to_annotated_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Styles pushed cover what is appended until popped, and nest.
    #[test]
    fn pushed_styles_cover_what_is_appended_until_popped() {
        let red = Color::rgb(255, 0, 0);
        let text = build_annotated_string(|builder| {
            builder.append("plain ");
            builder.with_style(SpanStyle::new().bold(), |builder| {
                builder.append("bold ");
                builder.with_style(SpanStyle::new().color(red), |builder| builder.append("red"));
            });
            builder.append(" end");
        });
        assert_eq!(text.text(), "plain bold red end");
        assert_eq!(
            text.span_styles(),
            &[
                AnnotatedRange::new(SpanStyle::new().bold(), 6, 14),
                AnnotatedRange::new(SpanStyle::new().color(red), 11, 14),
            ]
        );
        let spans = text.spans_in(0, text.len(), &[]);
        let texts: Vec<&str> = spans.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, ["plain ", "bold ", "red", " end"]);
        assert_eq!(spans[2].color, Some(red));
        assert_eq!(spans[2].font_weight, Some(FontWeight::BOLD));
        assert_eq!(spans[3].font_weight, None);
    }

    /// Ranges left open end at the end of the text, and `pop_to` ends every range pushed
    /// since the one it names.
    #[test]
    fn open_ranges_end_at_the_end_and_pop_to_ends_those_after() {
        let mut builder = AnnotatedStringBuilder::new();
        let outer = builder.push_style(SpanStyle::new().italic());
        builder.push_string_annotation("note", "a");
        builder.append("ab");
        builder.pop_to(outer);
        builder.append("c");
        builder.push_style(SpanStyle::new().bold());
        builder.append("d");
        let text = builder.to_annotated_string();
        assert_eq!(text.span_styles()[0].end, 2);
        assert_eq!(text.span_styles()[1].start, 3);
        assert_eq!(text.span_styles()[1].end, 4);
        assert_eq!(text.get_string_annotations("note", 0, 4)[0].end, 2);
        assert!(text.get_string_annotations("note", 2, 4).is_empty());
        assert!(text.has_string_annotations("note", 1, 2));
    }

    /// Paragraph styles split the text into paragraphs, the stretches between them being
    /// paragraphs of their own, and `with_style` takes either kind of style.
    #[test]
    fn paragraph_styles_split_the_text_into_paragraphs() {
        let tall = ParagraphStyle::new().line_height(TextUnit::Sp(30.0));
        let text = build_annotated_string(|builder| {
            builder.append("first");
            builder.with_style(tall, |builder| builder.append("second"));
            builder.append("third");
        });
        let paragraphs = text.paragraphs();
        let bounds: Vec<(usize, usize)> = paragraphs.iter().map(|p| (p.start, p.end)).collect();
        assert_eq!(bounds, [(0, 5), (5, 11), (11, 16)]);
        assert_eq!(paragraphs[1].item.line_height, Some(TextUnit::Sp(30.0)));
        assert_eq!(paragraphs[0].item, ParagraphStyle::new());
        let style = paragraphs[1].item.apply(TextStyle::default());
        assert_eq!(style.line_height, 30.0);
        assert_eq!(AnnotatedString::new("one").paragraphs().len(), 1);
    }

    /// Links keep their range, and appending an annotated string moves what is set on it.
    #[test]
    fn links_keep_their_range_and_appended_text_moves() {
        let inner = build_annotated_string(|builder| {
            builder.with_link(LinkAnnotation::url("https://example.com"), |builder| {
                builder.append("site")
            });
        });
        let text = build_annotated_string(|builder| {
            builder.append("see ");
            builder.append_annotated(&inner);
        });
        let links = text.get_link_annotations(0, text.len());
        assert_eq!(links.len(), 1);
        assert_eq!((links[0].start, links[0].end), (4, 8));
        assert!(text.get_link_annotations(0, 3).is_empty());
        assert!(
            matches!(&links[0].item, LinkAnnotation::Url { url, .. } if url == "https://example.com")
        );
    }

    /// A link's styles go over one another as it is focused, hovered and pressed.
    #[test]
    fn link_styles_go_over_one_another() {
        let blue = Color::rgb(0, 0, 255);
        let red = Color::rgb(255, 0, 0);
        let styles = TextLinkStyles::new(SpanStyle::new().color(blue).underline(true))
            .hovered_style(SpanStyle::new().color(red))
            .pressed_style(SpanStyle::new().underline(false));
        assert_eq!(styles.resolve(false, false, false).color, Some(blue));
        assert_eq!(styles.resolve(false, true, false).color, Some(red));
        let pressed = styles.resolve(false, true, true);
        assert_eq!(pressed.underline, Some(false));
        assert_eq!(pressed.color, Some(red));
    }

    /// Text units turn into logical pixels, `em` by the font size.
    #[test]
    fn text_units_turn_into_pixels() {
        assert_eq!(TextUnit::Sp(12.0).to_px(20.0), 12.0);
        assert_eq!(TextUnit::Em(1.5).to_px(20.0), 30.0);
    }
}
