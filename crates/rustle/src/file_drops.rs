//! Files dragged onto a window from other applications, through winit's drag and drop: while
//! a drag that offers files is over a window, it is accepted only where the window has a file
//! drop area, so the desktop shows whether letting go there would do anything, and on a drop
//! the files go to the area under the point.

use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

use rustle_geometry::Offset;
use winit::data_transfer::{DataTransferId, TypeHint, TypedData};
use winit::dpi::PhysicalPosition;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, AsyncRequestSerial, DndAction};
use winit::window::WindowId;

use crate::app::App;

/// A drag over one of the windows, from the moment it enters until its files are handed over
/// or it leaves.
pub(crate) struct FileDrag {
    /// What winit calls the drag.
    id: DataTransferId,
    /// The window it is over.
    window: WindowId,
    /// Whether it offers files.
    files: bool,
    /// Where it is in the window, in logical pixels, once the platform said.
    at: Option<Offset>,
    /// Whether the desktop was last told that the drag would be taken where it is.
    accepted: Option<bool>,
    /// The request for its files after it was let go, once made.
    fetch: Option<AsyncRequestSerial>,
    /// Its files as they arrived, while they are still being read.
    received: Option<Arc<dyn TypedData>>,
}

impl App {
    /// Follows a drag over the window `window`, in slot `index`, through `event`. Returns
    /// whether `event` was about a drag.
    pub(crate) fn file_drag_event(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        index: usize,
        window: WindowId,
        event: &WindowEvent,
    ) -> bool {
        match event {
            WindowEvent::DragEntered { id, position } => {
                let files = event_loop
                    .data_transfer(*id)
                    .is_ok_and(|transfer| transfer.has_type(&TypeHint::UriList));
                self.file_drag = Some(FileDrag {
                    id: *id,
                    window,
                    files,
                    at: None,
                    accepted: None,
                    fetch: None,
                    received: None,
                });
                self.drag_moved(event_loop, index, *id, *position);
            }
            WindowEvent::DragPosition { id, position, .. } => {
                self.drag_moved(event_loop, index, *id, Some(*position));
            }
            WindowEvent::DragDropped { id, .. } => self.drag_let_go(event_loop, *id),
            WindowEvent::DragLeft { id } => {
                if self
                    .file_drag
                    .as_ref()
                    .is_some_and(|drag| drag.id == *id && drag.fetch.is_none())
                {
                    self.file_drag = None;
                }
            }
            WindowEvent::DataTransferReceived { id, serial, value } => {
                self.files_received(*id, *serial, value);
            }
            _ => return false,
        }
        true
    }

    /// Moves the drag `id` to `position`, in physical pixels in the window in slot `index`, and
    /// tells the desktop whether it would be taken there.
    fn drag_moved(
        &mut self,
        event_loop: &dyn ActiveEventLoop,
        index: usize,
        id: DataTransferId,
        position: Option<PhysicalPosition<f64>>,
    ) {
        let scale_factor = self
            .slot_window(index)
            .map_or(1.0, |state| state.window.scale_factor());
        let Some(drag) = self.file_drag.as_mut().filter(|drag| drag.id == id) else {
            return;
        };
        if let Some(position) = position {
            drag.at = Some(Offset::new(
                (position.x / scale_factor) as f32,
                (position.y / scale_factor) as f32,
            ));
        }
        let (files, at) = (drag.files, drag.at);
        let taken = files && at.is_some_and(|at| self.takes_files_at(index, at));
        let Some(drag) = self.file_drag.as_mut() else {
            return;
        };
        if drag.accepted == Some(taken) {
            return;
        }
        drag.accepted = Some(taken);
        let actions: &[DndAction] = if taken { &[DndAction::Copy] } else { &[] };
        let _ = event_loop.set_valid_dnd_actions(id, actions);
    }

    /// Asks for the files of the drag `id`, let go now, when it was over a file drop area.
    fn drag_let_go(&mut self, event_loop: &dyn ActiveEventLoop, id: DataTransferId) {
        let Some(drag) = self.file_drag.as_mut().filter(|drag| drag.id == id) else {
            return;
        };
        drag.fetch = (drag.accepted == Some(true))
            .then(|| event_loop.fetch_data_transfer(id, &TypeHint::UriList).ok())
            .flatten();
        if drag.fetch.is_none() {
            self.file_drag = None;
        }
    }

    /// Hands the files of the drag `id` to the area it was let go over, once `value`, which
    /// answers the request `serial`, can be read.
    fn files_received(
        &mut self,
        id: DataTransferId,
        serial: AsyncRequestSerial,
        value: &Arc<dyn TypedData>,
    ) {
        let Some(drag) = self.file_drag.as_mut().filter(|drag| drag.id == id) else {
            return;
        };
        if drag.fetch == Some(serial) {
            drag.received = Some(Arc::clone(value));
        }
        let Some(received) = drag.received.clone() else {
            return;
        };
        let uris = match received.try_as_uris() {
            Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Deadlock) => {
                return;
            }
            uris => uris.unwrap_or_default(),
        };
        let Some(drag) = self.file_drag.take() else {
            return;
        };
        if let (Some(index), Some(at)) = (self.slot_of(drag.window), drag.at) {
            self.files_dropped(
                index,
                at,
                uris.iter().filter_map(|uri| file_path(uri)).collect(),
            );
        }
    }

    /// Whether the window in slot `index` has a file drop area at `at` that input may reach now.
    pub(crate) fn takes_files_at(&mut self, index: usize, at: Offset) -> bool {
        if self.blocking_modal(index).is_some() {
            return false;
        }
        self.activate(index);
        self.ui.display.file_drop_at(at).is_some()
    }

    /// Hands `paths`, dropped at `at` on the window in slot `index`, to the file drop area there.
    pub(crate) fn files_dropped(&mut self, index: usize, at: Offset, paths: Vec<PathBuf>) {
        if !self.takes_files_at(index, at) {
            return;
        }
        for path in paths {
            self.ui.file_dropped_at(at, path);
        }
        self.apply_focus_request();
        self.apply_effects();
    }
}

/// The file a `file:` URI names, decoded; `None` for URIs of anything else.
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.trim().strip_prefix("file://")?;
    let (host, path) = rest.split_at(rest.find('/')?);
    path_of(host, percent_decoded(path))
}

/// The path the decoded path of a `file:` URI stands for. The host is left out, as desktops
/// name the machine itself there.
#[cfg(unix)]
fn path_of(_host: &str, bytes: Vec<u8>) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

/// The path the decoded path of a `file:` URI on `host` stands for: `/C:/a/b` is `C:\a\b`, and
/// a path on another machine is a UNC path, `\\host\share`.
#[cfg(not(unix))]
fn path_of(host: &str, bytes: Vec<u8>) -> Option<PathBuf> {
    let path = String::from_utf8(bytes).ok()?.replace('/', "\\");
    let drive = path
        .as_bytes()
        .get(1..3)
        .is_some_and(|rest| rest[0].is_ascii_alphabetic() && rest[1] == b':');
    Some(PathBuf::from(match host {
        "" | "localhost" if drive => path[1..].to_owned(),
        "" | "localhost" => path,
        host => format!(r"\\{host}{path}"),
    }))
}

/// The bytes `text` stands for once each `%` followed by two hexadecimal digits is replaced by
/// the byte they name.
fn percent_decoded(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u8::from_str_radix(digits, 16).ok());
        match escaped {
            Some(byte) => {
                decoded.push(byte);
                index += 3;
            }
            None => {
                decoded.push(bytes[index]);
                index += 1;
            }
        }
    }
    decoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn file_path_decodes_local_files_and_skips_the_rest() {
        let uris = [
            "file:///home/me/a%20b.txt",
            "file://host/srv/c.rs",
            "https://example.com/x",
            "file:///t%C3%A5rta",
        ];
        let paths: Vec<PathBuf> = uris.iter().filter_map(|uri| file_path(uri)).collect();
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/home/me/a b.txt"),
                PathBuf::from("/srv/c.rs"),
                PathBuf::from("/tårta"),
            ]
        );
    }

    #[cfg(windows)]
    #[test]
    fn file_path_turns_drive_uris_into_windows_paths() {
        assert_eq!(
            file_path("file:///C:/Users/me/a%20b.txt"),
            Some(PathBuf::from(r"C:\Users\me\a b.txt"))
        );
        assert_eq!(file_path("https://example.com/x"), None);
    }

    #[test]
    fn percent_decoded_keeps_stray_percent_signs() {
        assert_eq!(percent_decoded("100%-%zz%4"), b"100%-%zz%4".to_vec());
        assert_eq!(percent_decoded("%41%2f"), b"A/".to_vec());
    }
}
