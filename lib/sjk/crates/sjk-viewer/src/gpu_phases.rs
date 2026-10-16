//! Per-pass GPU timing of the main frame: timestamps between the encoder's sections,
//! resolved on a sampled frame. They are printed with the frame-budget report
//! (`SJK_FRAME_BUDGET` or `SJK_GPU_PHASES`), and kept for the low-FPS help
//! ([`crate::fps_help`]) while it watches. Neither on, nothing is encoded.
use std::cell::{Cell, RefCell};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const CAPACITY: u32 = 32;

/// Frames between sampled frames; the readback of one must finish before the next.
const INTERVAL: u32 = 32;

/// Timestamp features when the adapter has them; requesting them costs nothing.
pub(crate) fn features(adapter: &wgpu::Adapter) -> wgpu::Features {
    let wanted = wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS;
    if adapter.features().contains(wanted) {
        wanted
    } else {
        wgpu::Features::empty()
    }
}

pub(crate) struct Profiler {
    set: wgpu::QuerySet,
    resolve: wgpu::Buffer,
    read: wgpu::Buffer,
    nanoseconds_per_tick: f32,
    frame: Cell<u32>,
    active: Cell<bool>,
    index: Cell<u32>,
    names: RefCell<Vec<&'static str>>,
    pending: Cell<bool>,
    mapped: Arc<AtomicBool>,
    /// Requested by environment: every sampled frame is printed.
    print: bool,
    /// The low-FPS help wants samples.
    watch: Cell<bool>,
    /// The last sampled frame's times, until taken.
    sample: Cell<Option<Sample>>,
}

/// One sampled frame's GPU time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    /// The whole main frame, milliseconds.
    pub(crate) frame_ms: f32,
    /// SJK's real-time lighting: the sections named `light-...`.
    pub(crate) light_ms: f32,
}

impl Profiler {
    /// Present when the device has timestamps.
    pub(crate) fn new(
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
    ) -> Option<Self> {
        let print = std::env::var_os("SJK_FRAME_BUDGET").is_some()
            || std::env::var_os("SJK_GPU_PHASES").is_some();
        let supported = device.features().contains(
            wgpu::Features::TIMESTAMP_QUERY | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS,
        );
        if !supported {
            return None;
        }
        let set = device.create_query_set(&wgpu::QuerySetDescriptor {
            label: Some("SJK gpu phases"),
            ty: wgpu::QueryType::Timestamp,
            count: CAPACITY,
        });
        let size = u64::from(CAPACITY) * 8;
        let resolve = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK gpu phases resolve"),
            size,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let read = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK gpu phases read"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        Some(Self {
            set,
            resolve,
            read,
            nanoseconds_per_tick: queue.get_timestamp_period(),
            frame: Cell::new(0),
            active: Cell::new(false),
            index: Cell::new(0),
            names: RefCell::new(Vec::with_capacity(CAPACITY as usize)),
            pending: Cell::new(false),
            mapped: Arc::new(AtomicBool::new(false)),
            print,
            watch: Cell::new(false),
            sample: Cell::new(None),
        })
    }

    /// Sample frames for [`Self::take_sample`] (`true`) or stop.
    pub(crate) fn watch(&self, on: bool) {
        self.watch.set(on);
        if !on {
            self.sample.set(None);
        }
    }

    /// The last sampled frame's times, once.
    pub(crate) fn take_sample(&self) -> Option<Sample> {
        self.sample.take()
    }

    /// Start a frame; every `INTERVAL`th frame without a readback in flight is sampled,
    /// while printing or watched.
    pub(crate) fn begin(&self, encoder: &mut wgpu::CommandEncoder) {
        let frame = self.frame.get().wrapping_add(1);
        self.frame.set(frame);
        let active = (self.print || self.watch.get())
            && frame.is_multiple_of(INTERVAL)
            && !self.pending.get();
        self.active.set(active);
        if !active {
            return;
        }
        self.index.set(0);
        self.names.borrow_mut().clear();
        self.stamp(encoder);
    }

    /// End the section named `name` here.
    pub(crate) fn mark(&self, encoder: &mut wgpu::CommandEncoder, name: &'static str) {
        if !self.active.get() || self.index.get() >= CAPACITY {
            return;
        }
        self.names.borrow_mut().push(name);
        self.stamp(encoder);
    }

    fn stamp(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.write_timestamp(&self.set, self.index.get());
        self.index.set(self.index.get() + 1);
    }

    /// Resolve the sampled frame's stamps into the readback buffer; call before finishing.
    pub(crate) fn finish(&self, encoder: &mut wgpu::CommandEncoder) {
        if !self.active.get() || self.index.get() < 2 {
            return;
        }
        let count = self.index.get();
        encoder.resolve_query_set(&self.set, 0..count, &self.resolve, 0);
        encoder.copy_buffer_to_buffer(&self.resolve, 0, &self.read, 0, u64::from(count) * 8);
        self.pending.set(true);
        self.mapped.store(false, Ordering::Release);
        self.active.set(false);
    }

    /// Map the readback after the sampled frame was submitted; the callback lands later.
    pub(crate) fn after_submit(&self) {
        if !self.pending.get() || self.index.get() < 2 {
            return;
        }
        // `active` is already false; a second call while pending must not map twice.
        if self.index.get() == u32::MAX {
            return;
        }
        let mapped = self.mapped.clone();
        let count = self.index.get();
        self.index.set(u32::MAX);
        self.read
            .slice(..u64::from(count) * 8)
            .map_async(wgpu::MapMode::Read, move |result| {
                if result.is_ok() {
                    mapped.store(true, Ordering::Release);
                }
            });
    }

    /// Print or keep the sampled frame once its readback has landed; safe to call every
    /// frame.
    pub(crate) fn report(&self) {
        if !self.pending.get() || !self.mapped.load(Ordering::Acquire) {
            return;
        }
        let names = self.names.borrow();
        let count = names.len() + 1;
        let stamps: Vec<u64> = {
            let view = self
                .read
                .slice(..(count as u64) * 8)
                .get_mapped_range()
                .expect("mapped readback range");
            bytemuck::cast_slice::<u8, u64>(&view).to_vec()
        };
        self.read.unmap();
        self.pending.set(false);
        let ms = |a: u64, b: u64| {
            b.saturating_sub(a) as f64 * f64::from(self.nanoseconds_per_tick) * 1e-6
        };
        let light_ms = names
            .iter()
            .enumerate()
            .filter(|(_, name)| name.starts_with("light-"))
            .map(|(i, _)| ms(stamps[i], stamps[i + 1]))
            .sum::<f64>();
        if self.watch.get() {
            self.sample.set(Some(Sample {
                frame_ms: ms(stamps[0], stamps[count - 1]) as f32,
                light_ms: light_ms as f32,
            }));
        }
        if !self.print {
            return;
        }
        let mut line = format!(
            "gpu-phases frame={} total_ms={:.3}",
            self.frame.get(),
            ms(stamps[0], stamps[count - 1])
        );
        for (i, name) in names.iter().enumerate() {
            line.push_str(&format!(" {name}={:.3}", ms(stamps[i], stamps[i + 1])));
        }
        eprintln!("{line}");
    }
}
