//! Persistent workers with bounded, ownership-transferring SPSC queues; no shared mutable actors.
use super::{Actor, Task};
use ringbuf::{HeapCons, HeapProd, HeapRb, traits::*};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::{self, JoinHandle};

// A scheduling capacity, not a game/entity limit. Excess jobs execute on the caller.
const QUEUED: usize = 64;
const MAX_LANES: usize = 8;

struct Worker {
    send: HeapProd<Task>,
    receive: HeapCons<Task>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    pending: usize,
}

impl Worker {
    fn new(index: usize) -> std::io::Result<Self> {
        let (send, mut jobs) = HeapRb::<Task>::new(QUEUED).split();
        let (mut results, receive) = HeapRb::<Task>::new(QUEUED).split();
        let stop = Arc::new(AtomicBool::new(false));
        let stopping = stop.clone();
        let handle = thread::Builder::new()
            .name(format!("actor-eval-{index}"))
            .stack_size(8 * 1024 * 1024)
            .spawn(move || {
                loop {
                    let mut caller = None;
                    while let Some(mut task) = jobs.try_pop() {
                        // Return ownership even if malformed model data exposes an evaluator panic.
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            task.evaluate();
                        }));
                        if result.is_err() {
                            task.error =
                                Some(sjk_model::ModelError::invalid(0, "actor worker panicked"));
                        }
                        caller.get_or_insert_with(|| task.caller.clone());
                        assert!(
                            results.try_push(task).is_ok(),
                            "bounded result queue overflow"
                        );
                    }
                    // One completion notification for the drained batch, not one per actor.
                    if let Some(caller) = caller {
                        caller.unpark();
                    }
                    if stopping.load(Ordering::Acquire) {
                        break;
                    }
                    // A token received between the empty check and park prevents a lost wakeup.
                    thread::park();
                }
            })?;
        Ok(Self {
            send,
            receive,
            stop,
            handle: Some(handle),
            pending: 0,
        })
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.handle.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
    }
}

/// Map-lifetime worker pool. Default lanes include the caller and never exceed eight.
pub(crate) struct Pool {
    workers: Vec<Worker>,
}

impl Pool {
    /// Sample machine parallelism only at construction; failure degrades transactionally to serial.
    pub(crate) fn new() -> Self {
        Self::with_threads(thread::available_parallelism().map_or(1, usize::from))
    }

    /// Explicit lane count for deterministic scaling evidence; not a cvar or per-frame setting.
    pub(crate) fn with_threads(threads: usize) -> Self {
        let count = threads.clamp(1, MAX_LANES) - 1;
        let mut workers = Vec::with_capacity(count);
        for index in 0..count {
            match Worker::new(index) {
                Ok(worker) => workers.push(worker),
                Err(error) => {
                    eprintln!("actor workers unavailable, using serial evaluation: {error}");
                    workers.clear();
                    break;
                }
            }
        }
        Self { workers }
    }

    /// Effective execution lanes, including the render thread.
    pub(crate) fn threads(&self) -> usize {
        self.workers.len() + 1
    }

    /// Evaluate each actor once, then return all ownership before fixed-order application.
    pub(crate) fn evaluate<A: Actor>(&mut self, actors: &mut [A], time: i64) {
        let active = actors
            .iter_mut()
            .map(|actor| actor.evaluation().0.requested.is_some())
            .filter(|active| *active)
            .count();
        if active <= 1 || self.workers.is_empty() {
            for actor in actors {
                let (slot, animation, config) = actor.evaluation();
                slot.serial(animation, config, time);
            }
            return;
        }
        let caller = thread::current();
        let lanes = self.threads().min(active);
        let mut ordinal = 0;
        // Dispatch first; caller-owned slots are evaluated while workers run, not before dispatch.
        for (index, actor) in actors.iter_mut().enumerate() {
            let (slot, animation, config) = actor.evaluation();
            let Some(state) = slot.requested else {
                continue;
            };
            let lane = ordinal % lanes;
            ordinal += 1;
            if lane == 0 {
                continue;
            }
            let worker = &mut self.workers[lane - 1];
            if worker.pending == QUEUED {
                continue;
            }
            let task = Task {
                index,
                animator: slot.inner.take().expect("owned slot"),
                animation: animation.clone(),
                config: config.clone(),
                state,
                time,
                error: None,
                caller: caller.clone(),
            };
            match worker.send.try_push(task) {
                Ok(()) => {
                    worker.pending += 1;
                }
                Err(task) => {
                    slot.inner = Some(task.animator);
                }
            }
        }
        // Enqueue each lane's whole batch before waking it. Queues retain ownership,
        // and unpark tokens cover the empty-check/park race exactly as before.
        for worker in &self.workers {
            if worker.pending != 0 {
                worker.handle.as_ref().unwrap().thread().unpark();
            }
        }
        for actor in actors.iter_mut() {
            let (slot, animation, config) = actor.evaluation();
            if slot.inner.is_some() {
                slot.serial(animation, config, time);
            }
        }
        for worker in &mut self.workers {
            while worker.pending != 0 {
                if let Some(task) = worker.receive.try_pop() {
                    let slot = actors[task.index].evaluation().0;
                    slot.inner = Some(task.animator);
                    slot.error = task.error;
                    worker.pending -= 1;
                } else {
                    // Wait for a completion token, never busy-wait through an empty queue.
                    thread::park();
                }
            }
        }
    }
}
