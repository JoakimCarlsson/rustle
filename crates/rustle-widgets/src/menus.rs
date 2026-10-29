//! Dropdown menus, context menus and the select popup.

use std::cell::Cell;
use std::rc::Rc;

use rustle_foundation::{
    Alignment, Arrangement, LOCAL_TEXT_STYLE, Modifier, Role, Semantics, column, column_with,
    row_arranged, spacer, stack_with, text, text_styled,
};
use rustle_geometry::{Rect, Size};
use rustle_icons::{Icon, phosphor};
use rustle_runtime::key;

use crate::card;
use crate::icons::icon;
use crate::popup;
use crate::select_field;
use crate::theme::{theme, with_text_color};

/// Emits a menu of `content` below what was emitted before it, while `expanded`.
/// Emit it right after the control that opens it, in the same column or stack.
#[track_caller]
pub fn dropdown_menu(expanded: bool, on_dismiss: impl Fn() + 'static, content: impl FnOnce()) {
    dropdown_menu_at(expanded, on_dismiss, (0.0, 4.0), None, content);
}

/// Like [`dropdown_menu`], moved `offset` from where it would start and, if
/// `width` is given, exactly that wide.
#[track_caller]
pub fn dropdown_menu_at(
    expanded: bool,
    on_dismiss: impl Fn() + 'static,
    offset: (f32, f32),
    width: Option<f32>,
    content: impl FnOnce(),
) {
    if !expanded {
        return;
    }
    popup(on_dismiss, || {
        card(
            match width {
                Some(width) => Modifier::new().width(width),
                None => Modifier::new().width_in(180.0, 360.0),
            }
            .offset(offset.0, offset.1)
            // The card takes clicks, so they do not dismiss the menu.
            .clickable_plain(|| {}),
            || {
                column_with(Modifier::new().padding_xy(0.0, 8.0), content);
            },
        );
    });
}

/// Emits one entry of a menu showing `label` that runs `on_click` when clicked.
#[track_caller]
pub fn dropdown_menu_item(label: &str, on_click: impl Fn() + 'static) {
    let theme = theme();
    stack_with(
        Modifier::new()
            .fill_max_width()
            .semantics(
                Semantics::new(Role::Button)
                    .label(label.to_string())
                    .merging_descendants(),
            )
            .clickable(on_click)
            .padding_xy(16.0, 10.0),
        || with_text_color(theme.on_surface, || text(label.to_string())),
    );
}

/// One entry of a menu.
#[derive(Clone)]
pub struct MenuItem {
    label: String,
    icon: Option<Icon>,
    on_select: Rc<dyn Fn()>,
    destructive: bool,
    checked: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>, on_select: impl Fn() + 'static) -> Self {
        Self {
            label: label.into(),
            icon: None,
            on_select: Rc::new(on_select),
            destructive: false,
            checked: false,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Draws the entry as a warning, for deleting things.
    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Marks the entry with a check, for the current choice.
    pub fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// Emits `trigger`, which is given a function that opens a menu of `items`
/// below it. Choosing an item closes the menu and runs the item.
#[track_caller]
pub fn menu(trigger: impl FnOnce(Rc<dyn Fn()>), items: Vec<MenuItem>) {
    rustle_runtime::group(|| {
        let theme = theme();
        let open = rustle_runtime::remember(|| false);
        let opener = open.clone();
        let dismiss = open.clone();
        // Where the trigger was last drawn, so the menu can stay inside the window.
        let placed = rustle_runtime::remember(|| Rc::new(Cell::new(None::<(Rect, Size)>))).get();
        let report = Rc::clone(&placed);
        let width = placed
            .get()
            .map_or(200.0, |(rect, _)| rect.size.width.clamp(200.0, 360.0));
        let height = items.len() as f32 * 36.0 + 16.0;
        let offset = placed.get().map_or((0.0, 4.0), |(rect, window)| {
            let across = if rect.origin.x + width > window.width - 8.0 {
                rect.size.width - width
            } else {
                0.0
            };
            let down = if rect.origin.y + rect.size.height + height + 8.0 > window.height {
                -(rect.size.height + height + 4.0)
            } else {
                4.0
            };
            (across, down)
        });
        column(|| {
            stack_with(
                Modifier::new().on_placed(move |rect, window| report.set(Some((rect, window)))),
                || trigger(Rc::new(move || opener.set(true))),
            );
            crate::dropdown_menu_at(
                open.get(),
                move || dismiss.set(false),
                offset,
                Some(width),
                || {
                    for (index, item) in items.iter().enumerate() {
                        let (item, close) = (item.clone(), open.clone());
                        key(index, || {
                            let color = if item.destructive {
                                theme.danger
                            } else {
                                theme.on_surface
                            };
                            row_arranged(
                                Modifier::new()
                                    .fill_max_width()
                                    .semantics(
                                        Semantics::new(Role::Button)
                                            .label(item.label.clone())
                                            .merging_descendants(),
                                    )
                                    .clickable({
                                        let (handler, close) =
                                            (Rc::clone(&item.on_select), close.clone());
                                        move || {
                                            close.set(false);
                                            handler();
                                        }
                                    })
                                    .padding_xy(14.0, 8.0),
                                Arrangement::SpacedBy(10.0),
                                Alignment::Center,
                                || {
                                    if let Some(glyph) = item.icon {
                                        icon(glyph, 15.0, color);
                                    }
                                    let style = LOCAL_TEXT_STYLE.current().color(color);
                                    text_styled(Modifier::new(), item.label.clone(), style);
                                    if item.checked {
                                        spacer(Modifier::new().weight(1.0));
                                        icon(phosphor::CHECK, 14.0, theme.on_surface_muted);
                                    }
                                },
                            );
                        });
                    }
                },
            );
        });
    });
}

/// Emits a box showing the `selected` one of `options`; clicking it opens a menu of all of them,
/// and choosing one calls `on_change` with its index.
#[track_caller]
pub fn select(
    modifier: Modifier,
    options: Vec<String>,
    selected: usize,
    on_change: impl Fn(usize) + 'static,
) {
    let on_change = Rc::new(on_change);
    let value = options.get(selected).cloned().unwrap_or_default();
    let items = options
        .iter()
        .enumerate()
        .map(|(index, option)| {
            let on_change = Rc::clone(&on_change);
            MenuItem::new(option.clone(), move || on_change(index)).checked(index == selected)
        })
        .collect();
    menu(|open| select_field(modifier, &value, move || open()), items);
}
