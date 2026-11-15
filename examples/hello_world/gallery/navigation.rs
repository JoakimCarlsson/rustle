//! Navigation components.

use rustle::prelude::*;

use crate::ui::panel;

#[composable]
pub(super) fn navigation() {
    let theme = color_scheme();
    let page = remember(|| 2_usize);
    let tab = remember(|| 0_usize);
    panel(
        "Breadcrumb, tabs, pagination",
        "Where you are, and how to move.",
        || {
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(6.0),
                Alignment::Center,
                || {
                    muted_sized("Home", 13.0);
                    icon(phosphor::CARET_RIGHT, 13.0, theme.on_surface_variant);
                    muted_sized("Components", 13.0);
                    icon(phosphor::CARET_RIGHT, 13.0, theme.on_surface_variant);
                    title("Breadcrumb", 13.0);
                },
            );
            spacer(Modifier::new().height(16.0));
            let pick = tab.clone();
            tabs_list(
                tab.get(),
                &[("Account", None), ("Password", None), ("Billing", Some(2))],
                move |index| pick.set(index),
            );
            spacer(Modifier::new().height(8.0));
            stack_with(Modifier::new().height(24.0), || {
                muted_sized(
                    [
                        "Make changes to your account.",
                        "Change your password here.",
                        "Two invoices are due.",
                    ][tab.get()],
                    13.0,
                );
            });
            spacer(Modifier::new().height(10.0));
            row_arranged(
                Modifier::new(),
                Arrangement::SpacedBy(4.0),
                Alignment::Center,
                || {
                    let back = page.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Ghost,
                        "Previous",
                        Some(phosphor::CARET_LEFT),
                        None,
                        move || back.update(|p| *p = p.saturating_sub(1)),
                    );
                    for number in 0..5 {
                        key(number, || {
                            let pick = page.clone();
                            let active = page.get() == number;
                            stack_aligned(
                                Modifier::new()
                                    .size(34.0, 34.0)
                                    .semantics(
                                        Semantics::new(Role::Button)
                                            .label(format!("Page {}", number + 1)),
                                    )
                                    .clickable(move || pick.set(number))
                                    .border(
                                        1.0,
                                        if active {
                                            theme.outline
                                        } else {
                                            Color::TRANSPARENT
                                        },
                                        8.0,
                                    ),
                                BoxAlignment::Center,
                                || text((number + 1).to_string()),
                            );
                        });
                    }
                    let next = page.clone();
                    styled_button(
                        Modifier::new(),
                        ButtonStyle::Ghost,
                        "Next",
                        None,
                        Some(phosphor::CARET_RIGHT),
                        move || next.update(|p| *p = (*p + 1).min(4)),
                    );
                },
            );
        },
    );
}
