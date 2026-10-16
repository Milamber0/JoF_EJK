//! Per-view indirect draw lists for world passes that need no per-material state.
//!
//! The light pre-pass and the receiver pass draw every visible light-buffered surface with
//! one of three pipelines (by cull mode) and nothing else per material. Their visible
//! ranges are gathered once per view into `DrawIndexedIndirectArgs`, uploaded once, and
//! each pass replays them with one `multi_draw_indexed_indirect` per cull mode: the render
//! thread records, and the encode workers validate, three calls instead of hundreds.
//!
//! Queue writes all land ahead of the frame's commands, so every view of a frame takes
//! its own region of the buffer; a frame that outgrows it draws directly, as before.
//! `SJK_INDIRECT_DRAWS=0` keeps direct draws for same-binary comparisons.
use std::cell::{Cell, RefCell};
use std::ops::Range;

const ARGS: u64 = 20;

/// Cull-mode buckets: front, back, none (`Material::light_buffered`).
pub(super) const BUCKETS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub camera: u64,
    pub area: u64,
    pub source: Option<usize>,
    pub pvs: bool,
}

/// One view's uploaded list: per bucket the byte offset and draw count.
#[derive(Clone, Copy)]
pub(super) struct View {
    pub offsets: [u64; BUCKETS],
    pub counts: [u32; BUCKETS],
}

/// Consecutive draws of a colour pass that share a pipeline: one multi-draw.
#[derive(Clone, Copy)]
pub(super) struct Run {
    pub pipeline: usize,
    pub offset: u64,
    pub count: u32,
}

pub(super) struct Lists {
    pub(super) buffer: wgpu::Buffer,
    capacity: u64,
    /// Bytes of the buffer already given to this frame's views.
    used: Cell<u64>,
    current: Cell<Option<(Key, View)>>,
    scratch: RefCell<[Vec<u8>; BUCKETS]>,
    runs: RefCell<(Vec<u8>, Vec<Run>)>,
    enabled: bool,
}

impl Lists {
    /// `draws` is the map's static draw count: no view can list more.
    pub(super) fn new(device: &wgpu::Device, draws: usize) -> Self {
        // Main view, its mirrors and their receiver frusta each take a region.
        let capacity = (draws.max(1) as u64 * ARGS * 8).min(64 * 1024 * 1024);
        Self {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("SJK world indirect draws"),
                size: capacity,
                usage: wgpu::BufferUsages::INDIRECT
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::STORAGE,
                mapped_at_creation: false,
            }),
            capacity,
            used: Cell::new(0),
            current: Cell::new(None),
            scratch: RefCell::new(std::array::from_fn(|_| {
                Vec::with_capacity(draws * ARGS as usize)
            })),
            // A material has a few stages; four per draw never reallocates in practice.
            runs: RefCell::new((
                Vec::with_capacity(draws * ARGS as usize * 4),
                Vec::with_capacity(256),
            )),
            enabled: std::env::var_os("SJK_INDIRECT_DRAWS").is_none_or(|value| value != "0"),
        }
    }

    /// Start a frame: every region is free again.
    pub(super) fn begin_frame(&self) {
        self.used.set(0);
        self.current.set(None);
    }

    /// A colour pass's draws in traversal order, each naming its stage record through the
    /// first instance. `build` pushes (pipeline, index range, record); consecutive draws of
    /// one pipeline become one run. `None`: off, or the frame's regions are exhausted.
    pub(super) fn runs(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        build: impl FnOnce(&mut dyn FnMut(usize, Range<u32>, u32)),
    ) -> Option<std::cell::Ref<'_, [Run]>> {
        if !self.enabled {
            return None;
        }
        {
            let mut guard = self.runs.borrow_mut();
            let (bytes, runs) = &mut *guard;
            bytes.clear();
            runs.clear();
            let start = self.used.get();
            build(&mut |pipeline, range, record| {
                let offset = start + bytes.len() as u64;
                let args = [range.end - range.start, 1, range.start, 0, record];
                bytes.extend_from_slice(bytemuck::cast_slice(&args));
                match runs.last_mut() {
                    Some(run) if run.pipeline == pipeline => run.count += 1,
                    _ => runs.push(Run {
                        pipeline,
                        offset,
                        count: 1,
                    }),
                }
            });
            if start + bytes.len() as u64 > self.capacity {
                return None;
            }
            if !bytes.is_empty() {
                queue.write_buffer(&self.buffer, start, bytes);
            }
            self.used.set(start + bytes.len() as u64);
        }
        Some(std::cell::Ref::map(self.runs.borrow(), |(_, runs)| {
            runs.as_slice()
        }))
    }

    /// The uploaded list for `key`, built from `ranges` (bucket, index range) on first use
    /// in this view. `None`: indirect draws are off or the frame's regions are exhausted.
    pub(super) fn view(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        key: Key,
        ranges: impl FnOnce(&mut dyn FnMut(usize, Range<u32>)),
    ) -> Option<View> {
        if !self.enabled {
            return None;
        }
        if let Some((current, view)) = self.current.get() {
            if current == key {
                return Some(view);
            }
        }
        let mut scratch = self.scratch.borrow_mut();
        for bucket in scratch.iter_mut() {
            bucket.clear();
        }
        ranges(&mut |bucket, range| {
            let args = [range.end - range.start, 1, range.start, 0, 0];
            scratch[bucket].extend_from_slice(bytemuck::cast_slice(&args));
        });
        let mut view = View {
            offsets: [0; BUCKETS],
            counts: [0; BUCKETS],
        };
        let mut offset = self.used.get();
        let total: u64 = scratch.iter().map(|bucket| bucket.len() as u64).sum();
        if offset + total > self.capacity {
            return None;
        }
        for (index, bucket) in scratch.iter().enumerate() {
            view.offsets[index] = offset;
            view.counts[index] = (bucket.len() as u64 / ARGS) as u32;
            if !bucket.is_empty() {
                queue.write_buffer(&self.buffer, offset, bucket);
            }
            offset += bucket.len() as u64;
        }
        self.used.set(offset);
        self.current.set(Some((key, view)));
        Some(view)
    }
}

impl super::Runtime {
    /// Reset before any secondary or main view records indirect draws. Queue uploads
    /// precede execution, so resetting between views would overwrite earlier commands.
    pub(crate) fn begin_world_frame(&self) {
        if let Some(lists) = &self.indirect {
            lists.begin_frame();
        }
    }

    /// Map-lifetime argument storage for current-frame mirror visibility.
    pub(crate) fn mirror_arguments(&self) -> Option<&wgpu::Buffer> {
        self.indirect.as_ref().map(|lists| &lists.buffer)
    }
    /// Start a disjoint interval; a preceding view's cached list cannot be suppressed.
    pub(crate) fn begin_mirror_commands(&self) -> u64 {
        self.indirect.as_ref().map_or(0, |lists| {
            lists.current.set(None);
            lists.used.get()
        })
    }
    /// End of the argument interval recorded for this mirror.
    pub(crate) fn mirror_command_cursor(&self) -> u64 {
        self.indirect.as_ref().map_or(0, |lists| lists.used.get())
    }
}
