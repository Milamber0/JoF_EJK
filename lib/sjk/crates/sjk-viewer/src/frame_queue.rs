//! The renderer's queue handle: uploads are recorded per submission and applied by
//! whoever submits next.
//!
//! wgpu applies `Queue::write_buffer` and `Queue::write_texture` at the next
//! `Queue::submit`, ahead of that submission's command buffers. Frames are submitted on
//! a submit thread (see `frame_split`) while the render thread already records the next
//! frame, so a write made for frame N+1 must not land in frame N's submission.
//! [`FrameQueue`] keeps wgpu's semantics exactly: every write is recorded in call order
//! and replayed immediately before the submission it belongs to, either the frame's own
//! (on the submit thread) or an out-of-band [`FrameQueue::submit`], which first waits
//! for frames in flight. Recording copies bytes into storage reused across frames, so a
//! steady-state frame neither allocates nor creates wgpu staging buffers on the render
//! thread.
use std::ops::Range;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

/// Writes at least this large are applied at once when no frame is in flight, so a
/// load-time upload does not leave the recording storage permanently large.
const DIRECT_WRITE_BYTES: usize = 1 << 20;
/// Recording storage above this size is trimmed after it has been applied.
const RETAINED_BYTES: usize = 16 << 20;

/// One recorded write; its bytes live in [`Writes::bytes`].
enum Op {
    Buffer {
        buffer: wgpu::Buffer,
        offset: wgpu::BufferAddress,
        bytes: Range<usize>,
    },
    Texture {
        texture: wgpu::Texture,
        mip_level: u32,
        origin: wgpu::Origin3d,
        aspect: wgpu::TextureAspect,
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
        bytes: Range<usize>,
    },
}

/// Writes recorded for one submission, in call order; storage is reused.
#[derive(Default)]
pub(crate) struct Writes {
    bytes: Vec<u8>,
    ops: Vec<Op>,
}

impl Writes {
    /// Replay every write on `queue` in call order, then clear, keeping capacity.
    pub(crate) fn apply(&mut self, queue: &wgpu::Queue) {
        for op in self.ops.drain(..) {
            match op {
                Op::Buffer {
                    buffer,
                    offset,
                    bytes,
                } => queue.write_buffer(&buffer, offset, &self.bytes[bytes]),
                Op::Texture {
                    texture,
                    mip_level,
                    origin,
                    aspect,
                    layout,
                    size,
                    bytes,
                } => queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level,
                        origin,
                        aspect,
                    },
                    &self.bytes[bytes],
                    layout,
                    size,
                ),
            }
        }
        self.bytes.clear();
        if self.bytes.capacity() > RETAINED_BYTES {
            self.bytes.shrink_to(RETAINED_BYTES);
        }
    }

    fn record(&mut self, data: &[u8]) -> Range<usize> {
        let start = self.bytes.len();
        self.bytes.extend_from_slice(data);
        start..self.bytes.len()
    }
}

struct Inner {
    queue: wgpu::Queue,
    /// Writes waiting for the next submission.
    recording: Mutex<Writes>,
    /// Frames handed to the submit thread and not yet submitted.
    in_flight: Mutex<u32>,
    idle: Condvar,
}

/// Shared handle to the device queue; clones share one recording.
#[derive(Clone)]
pub(crate) struct FrameQueue {
    inner: Arc<Inner>,
}

impl FrameQueue {
    pub(crate) fn new(queue: wgpu::Queue) -> Self {
        Self {
            inner: Arc::new(Inner {
                queue,
                recording: Mutex::new(Writes::default()),
                in_flight: Mutex::new(0),
                idle: Condvar::new(),
            }),
        }
    }

    /// The device queue, for calls that neither write existing resources nor submit
    /// (for example creating a texture with initial data or reading the timestamp
    /// period). Writes and submissions go through [`FrameQueue`] instead.
    pub(crate) fn raw(&self) -> &wgpu::Queue {
        &self.inner.queue
    }

    /// `wgpu::Queue::write_buffer`, applied ahead of the next submission.
    pub(crate) fn write_buffer(
        &self,
        buffer: &wgpu::Buffer,
        offset: wgpu::BufferAddress,
        data: &[u8],
    ) {
        if let Some(mut writes) = self.direct(data.len()) {
            writes.apply(&self.inner.queue);
            self.inner.queue.write_buffer(buffer, offset, data);
            return;
        }
        let mut writes = self.recording();
        let bytes = writes.record(data);
        writes.ops.push(Op::Buffer {
            buffer: buffer.clone(),
            offset,
            bytes,
        });
    }

    /// `wgpu::Queue::write_texture`, applied ahead of the next submission.
    pub(crate) fn write_texture(
        &self,
        texture: wgpu::TexelCopyTextureInfo<'_>,
        data: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        size: wgpu::Extent3d,
    ) {
        if let Some(mut writes) = self.direct(data.len()) {
            writes.apply(&self.inner.queue);
            self.inner.queue.write_texture(texture, data, layout, size);
            return;
        }
        let mut writes = self.recording();
        let bytes = writes.record(data);
        writes.ops.push(Op::Texture {
            texture: texture.texture.clone(),
            mip_level: texture.mip_level,
            origin: texture.origin,
            aspect: texture.aspect,
            layout,
            size,
            bytes,
        });
    }

    /// Submit outside the frame pipeline: wait until frames in flight are submitted, then
    /// apply the writes recorded so far ahead of `buffers`, as wgpu would have.
    pub(crate) fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(
        &self,
        buffers: I,
    ) -> wgpu::SubmissionIndex {
        self.wait_idle();
        let mut writes = self.recording();
        writes.apply(&self.inner.queue);
        self.inner.queue.submit(buffers)
    }

    pub(crate) fn get_timestamp_period(&self) -> f32 {
        self.inner.queue.get_timestamp_period()
    }

    /// Block until every frame handed to the submit thread has been submitted.
    pub(crate) fn wait_idle(&self) {
        let mut in_flight = lock(&self.inner.in_flight);
        while *in_flight > 0 {
            in_flight = self
                .inner
                .idle
                .wait(in_flight)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Take the writes recorded for the frame being handed off; `spare` (already
    /// applied) becomes the recording storage for the next frame.
    pub(crate) fn take_writes(&self, spare: &mut Writes) {
        std::mem::swap(&mut *self.recording(), spare);
    }

    /// A frame was handed to the submit thread.
    pub(crate) fn begin_flight(&self) {
        *lock(&self.inner.in_flight) += 1;
    }

    /// A handed-off frame has been submitted.
    pub(crate) fn end_flight(&self) {
        let mut in_flight = lock(&self.inner.in_flight);
        *in_flight = in_flight.saturating_sub(1);
        self.inner.idle.notify_all();
    }

    /// Large writes with no frame in flight: the recording, ready to be flushed ahead
    /// of a direct write so wgpu still sees every write in call order.
    fn direct(&self, len: usize) -> Option<MutexGuard<'_, Writes>> {
        (len >= DIRECT_WRITE_BYTES && *lock(&self.inner.in_flight) == 0).then(|| self.recording())
    }

    fn recording(&self) -> MutexGuard<'_, Writes> {
        lock(&self.inner.recording)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
