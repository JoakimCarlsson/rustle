//! Files dragged onto a window from other applications where winit does not report them,
//! which is on Wayland: the desktop's drag and drop is heard on a connection of our own, and
//! the files go to the file drop area under the point they are dropped at.

#[cfg(target_os = "linux")]
mod wayland;

#[cfg(target_os = "linux")]
use std::path::PathBuf;

#[cfg(target_os = "linux")]
use rustle_geometry::Offset;

use crate::app::App;

impl App {
    /// Hears what the desktop said of files dragged over and dropped on the windows since this
    /// was last called, where winit leaves that out.
    pub(crate) fn poll_file_drops(&mut self) {
        #[cfg(target_os = "linux")]
        wayland::poll(self);
    }

    /// Whether the window in slot `index` has a file drop area at `at` that input may reach now.
    #[cfg(target_os = "linux")]
    pub(crate) fn takes_files_at(&mut self, index: usize, at: Offset) -> bool {
        if self.blocking_modal(index).is_some() {
            return false;
        }
        self.activate(index);
        self.ui.display.file_drop_at(at).is_some()
    }

    /// Hands `paths`, dropped at `at` on the window in slot `index`, to the file drop area there.
    #[cfg(target_os = "linux")]
    pub(crate) fn files_dropped(&mut self, index: usize, at: Offset, paths: Vec<PathBuf>) {
        if !self.takes_files_at(index, at) {
            return;
        }
        for path in paths {
            self.ui.file_dropped_at(at, path);
        }
        self.ui.apply_focus_request();
        self.apply_effects();
    }
}

impl Drop for App {
    /// Stops hearing drags before the event loop, and the display it keeps open, go away.
    fn drop(&mut self) {
        #[cfg(target_os = "linux")]
        wayland::close();
    }
}
