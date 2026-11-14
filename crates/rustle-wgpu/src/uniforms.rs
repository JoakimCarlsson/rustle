//! Per-frame uniform data in a few large buffers, instead of one buffer per draw.

use std::cell::{Cell, RefCell};

/// The size of each buffer the arena makes.
const CHUNK: u64 = 256 * 1024;

/// A place in the arena a block of uniform data was written to.
#[derive(Clone)]
pub(crate) struct Slot {
    /// Which of the arena's buffers holds the data.
    pub(crate) chunk: usize,
    pub(crate) buffer: wgpu::Buffer,
    /// Where in the buffer the data starts; a multiple of the device's uniform alignment.
    pub(crate) offset: u64,
}

/// Uniform data for one frame at a time. Blocks are written with the queue, so they are in
/// place when the frame's commands run, and the buffers are written over by the next frame.
pub(crate) struct Arena {
    chunks: RefCell<Vec<wgpu::Buffer>>,
    chunk: Cell<usize>,
    cursor: Cell<u64>,
    alignment: u64,
}

impl Arena {
    /// An arena with no buffers yet, for `device`.
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        Self {
            chunks: RefCell::new(Vec::new()),
            chunk: Cell::new(0),
            cursor: Cell::new(0),
            alignment: u64::from(device.limits().min_uniform_buffer_offset_alignment).max(16),
        }
    }

    /// Starts a frame: the data of the last one may be written over.
    pub(crate) fn begin_frame(&self) {
        self.chunk.set(0);
        self.cursor.set(0);
    }

    /// Writes `bytes` (at most one buffer long) and says where.
    pub(crate) fn push(&self, device: &wgpu::Device, queue: &wgpu::Queue, bytes: &[u8]) -> Slot {
        let length = (bytes.len() as u64).div_ceil(self.alignment) * self.alignment;
        if self.cursor.get() + length > CHUNK {
            self.chunk.set(self.chunk.get() + 1);
            self.cursor.set(0);
        }
        let mut chunks = self.chunks.borrow_mut();
        while chunks.len() <= self.chunk.get() {
            chunks.push(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("uniforms"),
                size: CHUNK,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
        }
        let offset = self.cursor.get();
        let buffer = chunks[self.chunk.get()].clone();
        queue.write_buffer(&buffer, offset, bytes);
        self.cursor.set(offset + length);
        Slot {
            chunk: self.chunk.get(),
            buffer,
            offset,
        }
    }
}
