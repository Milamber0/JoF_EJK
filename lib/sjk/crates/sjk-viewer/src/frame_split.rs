//! Finish and submit a frame's command encoders off the render thread.
//!
//! wgpu-core defers validation and backend encoding of every pass to
//! `CommandEncoder::finish`, so a single frame encoder serializes all of that work on
//! the render thread. A [`Splitter`] cuts the frame into several encoders at pass
//! boundaries: workers finish earlier cuts while the render thread keeps recording.
//! Command buffers are submitted in recording order in one `Queue::submit`, so the GPU
//! executes exactly the command stream a single encoder would have produced.
//!
//! The submission itself runs on a submit thread: it waits for the frame's cuts,
//! finishes the last encoder, applies the frame's recorded uploads (see `frame_queue`),
//! submits and presents, while the render thread already records the next frame. Two
//! frame batches alternate between the threads, so the render thread is never more
//! than one frame ahead and a frame's swapchain image is presented before the next one
//! is acquired.
//!
//! `SJK_FRAME_SPLIT=0` keeps the single encoder; any other number sets the worker count.
//! `SJK_SUBMIT_THREAD=0` submits on the render thread instead. Hand-off uses bounded
//! channels allocated once: a cut neither allocates nor takes a lock on the render
//! thread. Idle workers share one queue, so the next cut always goes to a free worker
//! instead of waiting behind a long one.
use crate::frame_queue::{FrameQueue, Writes};
use std::cell::{Cell, OnceCell, RefCell};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::sync::{Arc, Mutex, OnceLock};

/// Cuts beyond this many per frame stay in the current encoder. It is below every
/// channel's capacity, so neither a cut nor a worker's reply can block on a full queue.
const MAX_CUTS: usize = 28;
const CAPACITY: usize = 32;
const DEFAULT_WORKERS: usize = 6;

type Done = (usize, Option<wgpu::CommandBuffer>);
struct Job {
    order: usize,
    encoder: wgpu::CommandEncoder,
    reply: SyncSender<Done>,
}

/// Process-wide encode workers, started on first use. `None`: single-encoder frames.
fn workers() -> Option<&'static SyncSender<Job>> {
    static WORKERS: OnceLock<Option<SyncSender<Job>>> = OnceLock::new();
    WORKERS
        .get_or_init(|| {
            let wanted = match std::env::var("SJK_FRAME_SPLIT") {
                Ok(value) => value.parse::<usize>().ok()?,
                Err(_) => DEFAULT_WORKERS.min(
                    std::thread::available_parallelism()
                        .map_or(1, std::num::NonZero::get)
                        .saturating_sub(1),
                ),
            };
            if wanted == 0 {
                return None;
            }
            let (jobs, take) = sync_channel::<Job>(CAPACITY);
            let take = Arc::new(Mutex::new(take));
            for index in 0..wanted {
                let take = take.clone();
                std::thread::Builder::new()
                    .name(format!("sjk-encode-{index}"))
                    .spawn(move || work(&take))
                    .ok()?;
            }
            Some(jobs)
        })
        .as_ref()
}

fn work(jobs: &Mutex<Receiver<Job>>) {
    // Only idle workers contend for the queue: the lock is released before `finish`.
    loop {
        let job = jobs
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .recv();
        let Ok(Job {
            order,
            encoder,
            reply,
        }) = job
        else {
            return;
        };

        // A validation panic must reach the render thread instead of stranding its wait.
        let command =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| encoder.finish())).ok();

        if reply.send((order, command)).is_err() {
            return;
        }
    }
}

/// One frame's cuts, last encoder, uploads and swapchain image; reused frame to frame.
struct Batch {
    reply: SyncSender<Done>,
    done: Receiver<Done>,
    sent: usize,
    ordered: Vec<Option<wgpu::CommandBuffer>>,
    last: Option<wgpu::CommandEncoder>,
    present: Option<wgpu::SurfaceTexture>,
    writes: Writes,
}

impl Batch {
    /// Reply storage is allocated here, outside the frame loop.
    fn new() -> Self {
        let (reply, done) = sync_channel(CAPACITY);
        let mut ordered = Vec::with_capacity(MAX_CUTS + 1);
        ordered.resize_with(MAX_CUTS + 1, || None);
        Self {
            reply,
            done,
            sent: 0,
            ordered,
            last: None,
            present: None,
            writes: Writes::default(),
        }
    }

    /// Finish the last cut, gather earlier cuts in recording order, apply the frame's
    /// uploads, submit once and present.
    fn submit(&mut self, queue: &wgpu::Queue) {
        let last = self.last.take().expect("frame encoder handed off").finish();
        for _ in 0..std::mem::take(&mut self.sent) {
            let (order, command) = self.done.recv().expect("frame encode worker stopped");
            self.ordered[order] = Some(command.expect("frame encoder cut failed on its worker"));
        }
        self.writes.apply(queue);
        queue.submit(
            self.ordered
                .iter_mut()
                .filter_map(Option::take)
                .chain(Some(last)),
        );
        if let Some(frame) = self.present.take() {
            queue.present(frame);
        }
    }
}

/// The submit thread's ends of the hand-off.
struct Submitter {
    frames: SyncSender<Batch>,
    returned: Receiver<Option<Batch>>,
    in_flight: Cell<bool>,
}

impl Submitter {
    /// `None` when `SJK_SUBMIT_THREAD=0` or the thread cannot start: submit inline.
    fn start(queue: &FrameQueue) -> Option<Self> {
        if std::env::var("SJK_SUBMIT_THREAD").is_ok_and(|value| value == "0") {
            return None;
        }
        let (frames, take) = sync_channel::<Batch>(1);
        let (give, returned) = sync_channel(2);
        let queue = queue.clone();
        std::thread::Builder::new()
            .name("sjk-submit".into())
            .spawn(move || {
                for mut batch in take {
                    let submitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        batch.submit(queue.raw())
                    }))
                    .is_ok();
                    queue.end_flight();
                    if give.send(submitted.then_some(batch)).is_err() {
                        return;
                    }
                }
            })
            .ok()?;
        Some(Self {
            frames,
            returned,
            in_flight: Cell::new(false),
        })
    }
}

/// One renderer's frame cuts and submissions; owned by its frame pacer.
pub(crate) struct Splitter {
    workers: Option<&'static SyncSender<Job>>,
    /// The frame being recorded; empty only during a hand-off.
    current: RefCell<Option<Batch>>,
    /// A batch back from the submit thread, ready for the next frame.
    spare: RefCell<Option<Batch>>,
    submitter: OnceCell<Option<Submitter>>,
}

impl Splitter {
    /// Batch storage is allocated here, outside the frame loop.
    pub(crate) fn new() -> Self {
        Self {
            workers: workers(),
            current: RefCell::new(Some(Batch::new())),
            spare: RefCell::new(Some(Batch::new())),
            submitter: OnceCell::new(),
        }
    }

    /// End the current encoder here, between passes, and keep recording into a fresh one.
    pub(crate) fn cut(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        let Some(workers) = self.workers else {
            return;
        };
        let mut current = self.current.borrow_mut();
        let batch = current.as_mut().expect("frame batch");
        let order = batch.sent;
        if order >= MAX_CUTS {
            return;
        }
        let next = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("SJK frame encoder"),
        });
        let job = Job {
            order,
            encoder: std::mem::replace(encoder, next),
            reply: batch.reply.clone(),
        };
        workers.send(job).expect("frame encode worker stopped");
        batch.sent = order + 1;
    }

    /// Hand the frame (its last encoder, uploads and swapchain image) to the submit
    /// thread, or submit it here when there is none.
    pub(crate) fn submit(
        &self,
        queue: &FrameQueue,
        encoder: wgpu::CommandEncoder,
        present: Option<wgpu::SurfaceTexture>,
        timing: &mut crate::frame_pacing::budget::Timer,
    ) {
        let submitter = self.submitter.get_or_init(|| Submitter::start(queue));
        // At most one frame in flight: the previous one must be back first.
        self.wait_previous();
        timing.mark(crate::frame_pacing::budget::Phase::QueueSubmit);
        let mut batch = self.current.borrow_mut().take().expect("frame batch");
        batch.last = Some(encoder);
        batch.present = present;
        queue.take_writes(&mut batch.writes);
        let Some(submitter) = submitter else {
            batch.submit(queue.raw());
            *self.current.borrow_mut() = Some(batch);
            return;
        };
        queue.begin_flight();
        submitter
            .frames
            .send(batch)
            .expect("frame submit thread stopped");
        submitter.in_flight.set(true);
        let next = self.spare.borrow_mut().take().expect("spare frame batch");
        *self.current.borrow_mut() = Some(next);
    }

    /// Wait until the frame handed off last has been submitted and presented. Call
    /// before acquiring the next swapchain image and before work that must follow that
    /// submission.
    pub(crate) fn wait_previous(&self) {
        let Some(Some(submitter)) = self.submitter.get() else {
            return;
        };
        if !submitter.in_flight.replace(false) {
            return;
        }
        let batch = submitter
            .returned
            .recv()
            .expect("frame submit thread stopped")
            .expect("frame submission failed on the submit thread");
        *self.spare.borrow_mut() = Some(batch);
    }
}

impl Drop for Splitter {
    fn drop(&mut self) {
        // A frame still in flight completes before the renderer's resources go away.
        if let Some(Some(submitter)) = self.submitter.get()
            && submitter.in_flight.get()
        {
            let _ = submitter.returned.recv();
        }
    }
}
