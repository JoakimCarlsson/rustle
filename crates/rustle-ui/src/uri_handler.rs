//! Opening addresses: Compose's `UriHandler` and `LocalUriHandler`.

use std::fmt;
use std::rc::Rc;

use rustle_runtime::CompositionLocal;

/// Opens addresses, such as the one a link to a web page leads to: Compose's `UriHandler`.
/// Two handles are equal when they open addresses the same way.
#[derive(Clone)]
pub struct UriHandler {
    /// What opens an address.
    open: Rc<dyn Fn(&str)>,
}

impl UriHandler {
    /// A handler that opens an address by calling `open` with it.
    pub fn new(open: impl Fn(&str) + 'static) -> Self {
        Self {
            open: Rc::new(open),
        }
    }

    /// Opens `uri`, Compose's `openUri`.
    pub fn open_uri(&self, uri: &str) {
        (self.open)(uri);
    }
}

impl PartialEq for UriHandler {
    /// Handlers are equal when they are the same handler.
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.open, &other.open)
    }
}

impl fmt::Debug for UriHandler {
    /// Writes the handler's name alone.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UriHandler")
    }
}

thread_local! {
    /// The handler that opens nothing, shared so every default is equal to the others.
    static OPENS_NOTHING: UriHandler = UriHandler::new(|_| {});
}

/// The handler addresses are opened with below where it is provided: Compose's
/// `LocalUriHandler`. Windows and embedded interfaces provide one that hands addresses to the
/// system; outside them the default opens nothing.
pub static LOCAL_URI_HANDLER: CompositionLocal<UriHandler> =
    CompositionLocal::new(|| OPENS_NOTHING.with(UriHandler::clone));
