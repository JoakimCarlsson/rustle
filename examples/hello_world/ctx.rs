//! State shared by every page: where the user is, what is open, and the data
//! the pages show and edit.

use std::time::Duration;

use rustle::prelude::*;

pub const PAGES: [&str; 13] = [
    "Dashboard",
    "Overview",
    "Components",
    "Projects",
    "Team",
    "Data Library",
    "Reports",
    "Word Assistant",
    "Settings",
    "Get Help",
    "Automations",
    "Gallery",
    "Motion",
];

/// What is shown over the page, if anything.
#[derive(Clone, PartialEq)]
pub enum Overlay {
    None,
    Search,
    NewDocument,
    Detail(usize),
    Alert,
    Report,
    Pending,
    Transfer,
    Logout,
    Invite,
}

#[derive(Clone)]
pub struct Doc {
    pub id: usize,
    pub header: String,
    pub kind: String,
    pub done: bool,
    pub target: u32,
    pub limit: u32,
    pub reviewer: String,
}

#[derive(Clone)]
pub struct Member {
    pub name: String,
    pub email: String,
    pub role: usize,
}

#[derive(Clone)]
pub struct Item {
    pub name: String,
    pub detail: String,
    pub starred: bool,
}

pub const ROLES: [&str; 3] = ["Owner", "Member", "Viewer"];
pub const REVIEWERS: [&str; 3] = ["Eddie Lake", "Jamik Tashpulatov", "Sofia Davis"];
pub const KINDS: [&str; 4] = ["Narrative", "Cover page", "Table of contents", "Appendix"];

#[derive(Clone)]
pub struct Ctx {
    pub page: State<usize>,
    pub collapsed: State<bool>,
    pub more: State<bool>,
    pub signed_in: State<bool>,
    /// None follows the system.
    pub dark: State<Option<bool>>,
    pub overlay: State<Overlay>,
    pub toast: State<Option<(u64, String)>>,
    pub toasts: State<u64>,
    pub docs: StateList<Doc>,
    pub next_doc: State<usize>,
    pub members: StateList<Member>,
    pub notices: StateList<(String, bool)>,
    /// Balances of the three accounts the transfer form moves money between.
    pub balances: State<[f64; 3]>,
    /// The transfer waiting for confirmation: amount, from, to.
    pub transfer: State<(f64, usize, usize)>,
    /// Projects, library files and reports, in that order.
    pub collections: [StateList<Item>; 3],
}

#[composable]
pub fn remember_ctx() -> Ctx {
    let docs = [
        ("Cover page", "Cover page", false, 18, 5, 0),
        ("Table of contents", "Table of contents", true, 29, 24, 0),
        ("Executive summary", "Narrative", true, 10, 13, 0),
        ("Technical approach", "Narrative", true, 27, 23, 1),
        ("Design", "Narrative", false, 2, 16, 1),
        ("Capabilities", "Narrative", false, 20, 8, 1),
        (
            "Integration with existing systems",
            "Narrative",
            false,
            19,
            21,
            2,
        ),
        ("Innovation and Advantages", "Narrative", true, 25, 11, 2),
    ];
    Ctx {
        page: remember_saveable("page", || 0usize),
        collapsed: remember(|| false),
        more: remember(|| false),
        signed_in: remember(|| true),
        dark: remember(|| None),
        overlay: remember(|| Overlay::None),
        toast: remember(|| None),
        toasts: remember(|| 0),
        docs: remember_state_list(move || {
            docs.iter()
                .enumerate()
                .map(|(id, (header, kind, done, target, limit, reviewer))| Doc {
                    id,
                    header: header.to_string(),
                    kind: kind.to_string(),
                    done: *done,
                    target: *target,
                    limit: *limit,
                    reviewer: REVIEWERS[*reviewer].to_string(),
                })
                .collect()
        }),
        next_doc: remember(|| 8),
        members: remember_state_list(|| {
            [
                ("Sofia Davis", "m@example.com", 0),
                ("Jackson Lee", "p@example.com", 1),
                ("Isabella Nguyen", "i@example.com", 1),
            ]
            .iter()
            .map(|(name, email, role)| Member {
                name: name.to_string(),
                email: email.to_string(),
                role: *role,
            })
            .collect()
        }),
        notices: remember_state_list(|| {
            vec![
                ("Eddie Lake commented on Design".to_string(), false),
                ("Cover page is ready for review".to_string(), false),
                ("Weekly report was generated".to_string(), true),
            ]
        }),
        balances: remember(|| [12_450.0, 42_100.0, 3_280.55]),
        transfer: remember(|| (0.0, 0, 1)),
        collections: [
            items(&[
                ("Website redesign", "Design \u{2022} due Jun 12"),
                ("Mobile app launch", "Engineering \u{2022} due Jul 3"),
                ("Q3 planning", "Operations \u{2022} due Jun 28"),
                ("Brand refresh", "Marketing \u{2022} due Aug 15"),
            ]),
            items(&[
                ("customers.csv", "2.4 MB \u{2022} updated today"),
                ("pricing-2024.xlsx", "380 KB \u{2022} updated yesterday"),
                ("brand-guidelines.pdf", "5.1 MB \u{2022} updated Mar 2"),
                ("research-notes.md", "12 KB \u{2022} updated Feb 18"),
            ]),
            items(&[
                ("Weekly summary", "Generated Monday"),
                ("Revenue by region", "Generated Jun 1"),
                ("Customer retention", "Generated May 24"),
            ]),
        ],
    }
}

impl Ctx {
    pub fn go(&self, page: usize) {
        self.page.set(page);
    }

    pub fn open(&self, overlay: Overlay) {
        self.overlay.set(overlay);
    }

    pub fn close(&self) {
        self.overlay.set(Overlay::None);
    }

    /// Shows `message` for a few seconds.
    pub fn say(&self, message: impl Into<String>) {
        let id = self.toasts.get() + 1;
        self.toasts.set(id);
        self.toast.set(Some((id, message.into())));
    }

    pub fn add_doc(&self, header: &str, kind: &str) -> usize {
        let id = self.next_doc.get();
        self.next_doc.set(id + 1);
        self.docs.push(Doc {
            id,
            header: header.to_string(),
            kind: kind.to_string(),
            done: false,
            target: 0,
            limit: 10,
            reviewer: REVIEWERS[0].to_string(),
        });
        id
    }

    pub fn doc(&self, id: usize) -> Option<Doc> {
        self.docs.to_vec().into_iter().find(|doc| doc.id == id)
    }

    pub fn change_doc(&self, id: usize, change: impl FnOnce(&mut Doc)) {
        let mut docs = self.docs.to_vec();
        if let Some(doc) = docs.iter_mut().find(|doc| doc.id == id) {
            change(doc);
        }
        self.docs.update(|list| *list = docs);
    }

    pub fn delete_docs(&self, ids: &[usize]) {
        self.docs
            .update(|list| list.retain(|doc| !ids.contains(&doc.id)));
    }

    pub fn unread(&self) -> usize {
        self.notices
            .to_vec()
            .iter()
            .filter(|(_, read)| !read)
            .count()
    }
}

/// Clears the toast again a few seconds after it appeared.
#[composable]
pub fn toast_timer(ctx: &Ctx) {
    let toast = ctx.toast.clone();
    launched_effect(ctx.toast.get().map(|(id, _)| id), move |scope| async move {
        if toast.get().is_some() {
            scope.delay(Duration::from_millis(2800)).await;
            toast.set(None);
        }
    });
}

#[composable]
fn items(seed: &[(&str, &str)]) -> StateList<Item> {
    remember_state_list(|| {
        seed.iter()
            .map(|(name, detail)| Item {
                name: name.to_string(),
                detail: detail.to_string(),
                starred: false,
            })
            .collect()
    })
}
