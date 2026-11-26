//! Instance data of a whole frame in one persistent vertex buffer per kind of primitive,
//! grown by doubling, instead of a new buffer for every layer.

use std::ops::Range;

/// The smallest buffer an [`InstanceBuffer`] makes, in bytes.
const MINIMUM: u64 = 16 * 1024;

/// Instances of one kind for a frame: gathered on the CPU while the frame's layers are
/// prepared, then written into a vertex buffer kept between frames in one go.
pub(crate) struct InstanceBuffer {
    /// The name the buffer is made with, for debugging tools.
    label: &'static str,
    /// The buffer, made when first needed and replaced by one twice as large when outgrown.
    buffer: Option<wgpu::Buffer>,
    /// The size of `buffer` in bytes.
    capacity: u64,
    /// The bytes of this frame's instances, in the order they were pushed.
    staged: Vec<u8>,
}

impl InstanceBuffer {
    /// An empty buffer that names what it makes `label`.
    pub(crate) fn new(label: &'static str) -> Self {
        Self {
            label,
            buffer: None,
            capacity: 0,
            staged: Vec::new(),
        }
    }

    /// Starts a frame: the instances of the last one are forgotten.
    pub(crate) fn begin_frame(&mut self) {
        self.staged.clear();
    }

    /// Adds `bytes` to the frame's instances and returns where they lie in the buffer, in
    /// bytes, once [`flush`](Self::flush) has written it.
    pub(crate) fn push(&mut self, bytes: &[u8]) -> Range<u64> {
        let start = self.staged.len() as u64;
        self.staged.extend_from_slice(bytes);
        start..self.staged.len() as u64
    }

    /// Writes the frame's instances into the buffer with `queue`, first making a buffer
    /// twice as large (or more) on `device` if they no longer fit.
    pub(crate) fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.staged.is_empty() {
            return;
        }
        let needed = self.staged.len() as u64;
        if self.buffer.is_none() || needed > self.capacity {
            let mut capacity = self.capacity.max(MINIMUM);
            while capacity < needed {
                capacity *= 2;
            }
            self.buffer = Some(device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: capacity,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }));
            self.capacity = capacity;
        }
        if let Some(buffer) = &self.buffer {
            queue.write_buffer(buffer, 0, &self.staged);
        }
    }

    /// The bytes at `range` of the buffer, as written by the last [`flush`](Self::flush).
    pub(crate) fn slice(&self, range: Range<u64>) -> Option<wgpu::BufferSlice<'_>> {
        self.buffer.as_ref().map(|buffer| buffer.slice(range))
    }
}
