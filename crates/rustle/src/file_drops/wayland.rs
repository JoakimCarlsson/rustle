//! Files dropped on a Wayland window, heard through a `wl_data_device` on a connection of our
//! own that shares the window's display, since winit does not take part in drag and drop there.
//!
//! The device hears the drags that cross any surface of this client. While a drag is over a
//! window, it is accepted only where the window has a file drop area, so the desktop shows
//! whether letting go there would do anything. On a drop the `text/uri-list` the source offers
//! is read and its local files go to the area under the point.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{OsString, c_void};
use std::io::Read;
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use rustix::event::{PollFd, PollFlags, Timespec};
use rustle_geometry::Offset;
use wayland_client::backend::{Backend, ObjectId, WaylandError};
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_data_device::{self, WlDataDevice};
use wayland_client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use wayland_client::protocol::wl_data_offer::{self, WlDataOffer};
use wayland_client::protocol::wl_registry::{self, WlRegistry};
use wayland_client::protocol::wl_seat::WlSeat;
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, delegate_noop, event_created_child,
};
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};

use crate::app::{App, WindowState};

/// The type a drag offers files as: a list of URIs, one per line.
const URI_LIST: &str = "text/uri-list";

/// How long the source of a drop has to hand over its files before the drop is given up.
const READ_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the listener waits for the compositor at a time before it looks whether to stop.
const LISTEN_SLICE: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 250_000_000,
};

thread_local! {
    /// The device that hears drags, once the first window has opened.
    static DROPS: RefCell<Opened> = const { RefCell::new(Opened::Untried) };
}

/// Whether the device that hears drags is there.
enum Opened {
    /// No window was open yet to find the display through.
    Untried,
    /// The windows are not on Wayland, or the compositor offers no drag and drop.
    Absent,
    /// It is open.
    Open(Box<Drops>),
}

/// Hears what the desktop said of drags over the windows of `app` since this was last called,
/// opening the device that hears them the first time a window is open.
pub(super) fn poll(app: &mut App) {
    DROPS.with(|drops| {
        let mut drops = drops.borrow_mut();
        if matches!(*drops, Opened::Untried) {
            let Some(state) = app.window.as_ref() else {
                return;
            };
            *drops = match state.window.display_handle().map(|handle| handle.as_raw()) {
                Ok(RawDisplayHandle::Wayland(display)) => {
                    Drops::open(display.display, Arc::clone(&app.wake))
                        .map_or(Opened::Absent, |drops| Opened::Open(Box::new(drops)))
                }
                _ => Opened::Absent,
            };
        }
        if let Opened::Open(drops) = &mut *drops {
            drops.poll(app);
        }
    });
}

/// Stops hearing drags and waits for the listener to end, so that nothing reads the display
/// once the event loop closes it.
pub(super) fn close() {
    let _ = DROPS.try_with(|drops| {
        if let Ok(mut drops) = drops.try_borrow_mut() {
            *drops = Opened::Absent;
        }
    });
}

/// A thread that reads what the compositor sends while the event loop sleeps and wakes the loop
/// with `wake` when some of it is for the device on `connection`, until `stop` is set.
///
/// winit reads the display only to serve its own objects and does not wake the loop for events
/// of others, so without it a drag would be heard only once something else happened.
fn listen(connection: Connection, wake: Arc<dyn Fn() + Send + Sync>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::Relaxed) {
        let heard = match connection.prepare_read() {
            Some(guard) => {
                let readable = {
                    let fd = guard.connection_fd();
                    let mut fds = [PollFd::new(&fd, PollFlags::IN)];
                    rustix::event::poll(&mut fds, Some(&LISTEN_SLICE)).is_ok_and(|ready| ready > 0)
                };
                if !readable {
                    continue;
                }
                match guard.read() {
                    Ok(count) => count,
                    Err(WaylandError::Io(error))
                        if error.kind() == std::io::ErrorKind::WouldBlock =>
                    {
                        0
                    }
                    Err(_) => return,
                }
            }
            None => match connection.backend().dispatch_inner_queue() {
                Ok(count) => count,
                Err(_) => return,
            },
        };
        if heard > 0 {
            wake();
        }
    }
}

/// The thread [`listen`]ing for the device, and what tells it to stop.
struct Listener {
    /// Set to make the thread stop.
    stop: Arc<AtomicBool>,
    /// The thread, joined when the listener is dropped.
    thread: Option<JoinHandle<()>>,
}

impl Listener {
    /// Starts listening on `connection`, waking the loop with `wake`.
    fn start(connection: Connection, wake: Arc<dyn Fn() + Send + Sync>) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = Arc::clone(&stop);
        let thread = std::thread::Builder::new()
            .name("rustle-file-drops".to_owned())
            .spawn(move || listen(connection, wake, stopping))
            .ok()?;
        Some(Self {
            stop,
            thread: Some(thread),
        })
    }
}

impl Drop for Listener {
    /// Tells the thread to stop and waits for it.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A connection of our own on the Wayland display `display` the windows are on.
///
/// The display must stay open for as long as the connection lives, which it does: winit keeps
/// it open while its event loop runs, and the connection lives on the event loop's thread.
fn share(display: NonNull<c_void>) -> Connection {
    // SAFETY: winit keeps the display open while its event loop runs.
    let backend = unsafe { Backend::from_foreign_display(display.as_ptr().cast()) };
    Connection::from_backend(backend)
}

/// The `wl_surface` of the window `state`, as the address of its proxy, when it is on Wayland.
fn surface_of(state: &WindowState) -> Option<usize> {
    match state.window.window_handle().ok()?.as_raw() {
        RawWindowHandle::Wayland(handle) => Some(handle.surface.as_ptr() as usize),
        _ => None,
    }
}

/// The slot of the window of `app` whose `wl_surface` proxy is at `surface`.
fn slot_of_surface(app: &App, surface: usize) -> Option<usize> {
    (0..app.slots.len()).find(|&index| app.slot_window(index).and_then(surface_of) == Some(surface))
}

/// The local files a `text/uri-list` names: its `file:` URIs, decoded, skipping comments and
/// URIs of anything else.
pub(super) fn file_paths(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.strip_prefix("file://"))
        .filter_map(|rest| rest.find('/').map(|slash| &rest[slash..]))
        .map(|path| PathBuf::from(OsString::from_vec(percent_decoded(path))))
        .collect()
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

/// What the device heard, in the order it heard it.
enum Happening {
    /// A drag came over the surface whose proxy is at `surface`, at `at`, offering `offer`.
    Enter {
        /// The serial to accept the drag with.
        serial: u32,
        /// The address of the `wl_surface` proxy the drag is over.
        surface: usize,
        /// Where on the surface, in logical pixels.
        at: Offset,
        /// What the drag offers, or nothing when it comes from a drag of this client's own.
        offer: Option<WlDataOffer>,
    },
    /// The drag moved to this point on the surface.
    Motion(Offset),
    /// The drag left the surface, or was called off.
    Leave,
    /// The drag was let go over the surface.
    Drop,
}

/// What the events of the connection tell.
#[derive(Default)]
struct Heard {
    /// What the device heard since it was last looked at.
    happenings: Vec<Happening>,
    /// The types each offer still alive offers its data as.
    types: HashMap<ObjectId, Vec<String>>,
}

/// The drag over one of the windows now.
struct Dragged {
    /// What it offers.
    offer: Option<WlDataOffer>,
    /// The serial to accept it with.
    serial: u32,
    /// The slot of the window it is over, if it is one of ours.
    slot: Option<usize>,
    /// Where it is on that window, in logical pixels.
    at: Offset,
    /// Whether it was last told it would be taken where it is, once it was told anything.
    answered: Option<bool>,
}

/// The device that hears drags and the connection it is made on.
struct Drops {
    /// Our connection to the compositor, on the windows' display.
    connection: Connection,
    /// The queue the events of our objects arrive on.
    queue: EventQueue<Heard>,
    /// What those events tell.
    heard: Heard,
    /// The device itself, kept so that it goes on hearing.
    _device: WlDataDevice,
    /// The drag over one of the windows now, if any.
    dragged: Option<Dragged>,
    /// The thread that wakes the loop when the device hears something. Declared last, so it
    /// is stopped after the device and before the connection's last handle goes.
    _listener: Option<Listener>,
}

impl Drops {
    /// A device for the first seat of the Wayland display `display`, which wakes the loop with
    /// `wake` when it hears something; `None` when the compositor offers no seat or no drag and
    /// drop.
    fn open(display: NonNull<c_void>, wake: Arc<dyn Fn() + Send + Sync>) -> Option<Self> {
        let connection = share(display);
        let (globals, mut queue) = registry_queue_init::<Heard>(&connection).ok()?;
        let handle = queue.handle();
        let seat: WlSeat = globals.bind(&handle, 1..=1, ()).ok()?;
        let manager: WlDataDeviceManager = globals.bind(&handle, 3..=3, ()).ok()?;
        let device = manager.get_data_device(&seat, &handle, ());
        let mut heard = Heard::default();
        let _ = queue.roundtrip(&mut heard);
        let listener = Listener::start(connection.clone(), wake);
        Some(Self {
            connection,
            queue,
            heard,
            _device: device,
            dragged: None,
            _listener: listener,
        })
    }

    /// Acts on what the device heard since this was last called.
    fn poll(&mut self, app: &mut App) {
        let _ = self.queue.dispatch_pending(&mut self.heard);
        for happening in std::mem::take(&mut self.heard.happenings) {
            self.hear(app, happening);
        }
        let _ = self.connection.flush();
    }

    /// Acts on one `happening`.
    fn hear(&mut self, app: &mut App, happening: Happening) {
        match happening {
            Happening::Enter {
                serial,
                surface,
                at,
                offer,
            } => {
                self.forget();
                self.dragged = Some(Dragged {
                    offer,
                    serial,
                    slot: slot_of_surface(app, surface),
                    at,
                    answered: None,
                });
                self.answer(app);
            }
            Happening::Motion(at) => {
                if let Some(dragged) = &mut self.dragged {
                    dragged.at = at;
                }
                self.answer(app);
            }
            Happening::Leave => self.forget(),
            Happening::Drop => self.drop_files(app),
        }
    }

    /// Tells the source of the drag whether it would be taken where it is now, when that
    /// changed: it is when it offers files and is over a file drop area.
    fn answer(&mut self, app: &mut App) {
        let Some(dragged) = &mut self.dragged else {
            return;
        };
        let Some(offer) = &dragged.offer else {
            return;
        };
        let files = self
            .heard
            .types
            .get(&offer.id())
            .is_some_and(|types| types.iter().any(|kind| kind == URI_LIST));
        let taken = files
            && dragged
                .slot
                .is_some_and(|slot| app.takes_files_at(slot, dragged.at));
        if dragged.answered == Some(taken) {
            return;
        }
        dragged.answered = Some(taken);
        if taken {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
            offer.accept(dragged.serial, Some(URI_LIST.to_owned()));
        } else {
            offer.accept(dragged.serial, None);
        }
    }

    /// Reads the files of the drag let go now and hands them to the window it was over, when it
    /// was over a file drop area.
    fn drop_files(&mut self, app: &mut App) {
        let Some(dragged) = self.dragged.take() else {
            return;
        };
        let Some(offer) = dragged.offer else {
            return;
        };
        if dragged.answered == Some(true)
            && let Some(slot) = dragged.slot
            && let Some(list) = self.read(&offer)
        {
            offer.finish();
            let _ = self.connection.flush();
            app.files_dropped(slot, dragged.at, file_paths(&list));
        }
        self.heard.types.remove(&offer.id());
        offer.destroy();
    }

    /// The `text/uri-list` `offer` holds, read from its source; `None` when it could not be read
    /// in time.
    fn read(&self, offer: &WlDataOffer) -> Option<String> {
        let (mut reader, writer) = std::io::pipe().ok()?;
        offer.receive(URI_LIST.to_owned(), writer.as_fd());
        self.connection.flush().ok()?;
        drop(writer);
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = reader.read_to_end(&mut bytes);
            let _ = sender.send(bytes);
        });
        let bytes = receiver.recv_timeout(READ_TIMEOUT).ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Lets go of the drag over the windows, if any.
    fn forget(&mut self) {
        if let Some(Dragged {
            offer: Some(offer), ..
        }) = self.dragged.take()
        {
            self.heard.types.remove(&offer.id());
            offer.destroy();
        }
    }
}

impl Dispatch<WlRegistry, GlobalListContents> for Heard {
    /// Ignores globals coming and going: the device binds what it needs once.
    fn event(
        _: &mut Self,
        _: &WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDevice, ()> for Heard {
    /// Records the drags crossing this client's surfaces, and lets go of the clipboard
    /// selections it is told of, which the system clipboard reads on its own.
    fn event(
        heard: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => heard.happenings.push(Happening::Enter {
                serial,
                surface: surface.id().as_ptr() as usize,
                at: Offset::new(x as f32, y as f32),
                offer: id,
            }),
            wl_data_device::Event::Motion { x, y, .. } => heard
                .happenings
                .push(Happening::Motion(Offset::new(x as f32, y as f32))),
            wl_data_device::Event::Leave => heard.happenings.push(Happening::Leave),
            wl_data_device::Event::Drop => heard.happenings.push(Happening::Drop),
            wl_data_device::Event::Selection { id: Some(offer) } => {
                heard.types.remove(&offer.id());
                offer.destroy();
            }
            _ => {}
        }
    }

    event_created_child!(Heard, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, ()),
    ]);
}

impl Dispatch<WlDataOffer, ()> for Heard {
    /// Records each type an offer offers its data as.
    fn event(
        heard: &mut Self,
        offer: &WlDataOffer,
        event: wl_data_offer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            heard.types.entry(offer.id()).or_default().push(mime_type);
        }
    }
}

delegate_noop!(Heard: ignore WlSeat);
delegate_noop!(Heard: ignore WlDataDeviceManager);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_paths_decodes_local_files_and_skips_the_rest() {
        let list = "# a comment\r\nfile:///home/me/a%20b.txt\r\nfile://host/srv/c.rs\r\nhttps://example.com/x\r\n\r\nfile:///t%C3%A5rta\n";
        assert_eq!(
            file_paths(list),
            vec![
                PathBuf::from("/home/me/a b.txt"),
                PathBuf::from("/srv/c.rs"),
                PathBuf::from("/tårta"),
            ]
        );
    }

    #[test]
    fn percent_decoded_keeps_stray_percent_signs() {
        assert_eq!(percent_decoded("100%-%zz%4"), b"100%-%zz%4".to_vec());
        assert_eq!(percent_decoded("%41%2f"), b"A/".to_vec());
    }
}
