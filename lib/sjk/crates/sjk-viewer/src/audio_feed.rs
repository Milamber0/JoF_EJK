//! Pushing into the queues the audio callback drains, without waiting forever on a
//! callback that has stopped.
//!
//! The output device's callback empties the command and registration queues at every
//! mix block. A stream can stop calling it for good: on Windows cpal's WASAPI thread
//! returns when its device goes away (`AUDCLNT_E_DEVICE_INVALIDATED`), as when a
//! headset or Bluetooth speaker switches itself off after a while of silence (a window
//! muted in the background plays nothing), a monitor's speakers go with the display
//! asleep or the device's format is changed. The queues then never drain again. The
//! render thread used to wait for room without end, so the game stopped answering a
//! few seconds later and Alt+Tab never brought it back.
use ringbuf::traits::Producer;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long a full queue waits for the callback before its stream counts as stopped:
/// dozens of mix blocks (256 samples, under 6 ms) and device periods (10 ms for WASAPI).
pub(super) const PATIENCE: Duration = Duration::from_millis(250);
/// The decoder's pause between attempts once a full queue has outlasted [`PATIENCE`],
/// so a stopped stream does not keep a core spinning.
const DECODER_RETRY: Duration = Duration::from_millis(10);

/// What became of a pushed item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Pushed {
    Sent,
    /// The queue stayed full: the item is gone. `stopped` is true for the push that
    /// found the callback stopped, false for the ones dropped after it.
    Dropped {
        stopped: bool,
    },
}

/// A producer's view of whether the callback still drains its queue.
#[derive(Debug, Default)]
pub(super) struct Feed {
    stalled: bool,
}

impl Feed {
    /// Push `item`, waiting up to `patience` for room while the queue is full, as a
    /// burst used to wait. A queue still full after that belongs to a stopped stream:
    /// the item is dropped and the feed stalls. A stalled feed drops at once instead of
    /// waiting again, so the render thread waits on a stopped callback once, not at
    /// every command. Room in the queue again (the callback is back) ends the stall.
    pub(super) fn push<T>(
        &mut self,
        producer: &mut impl Producer<Item = T>,
        item: T,
        patience: Duration,
    ) -> Pushed {
        let mut item = match producer.try_push(item) {
            Ok(()) => {
                self.stalled = false;
                return Pushed::Sent;
            }
            Err(item) => item,
        };
        if self.stalled {
            return Pushed::Dropped { stopped: false };
        }
        let deadline = Instant::now() + patience;
        loop {
            std::thread::yield_now();
            item = match producer.try_push(item) {
                Ok(()) => return Pushed::Sent,
                Err(item) => item,
            };
            if Instant::now() >= deadline {
                self.stalled = true;
                return Pushed::Dropped { stopped: true };
            }
        }
    }

    /// The callback has not drained this queue since it was last found full.
    #[cfg(test)]
    pub(super) fn stalled(&self) -> bool {
        self.stalled
    }
}

/// Push `item`, waiting while the queue is full for as long as it takes, as the decoder
/// must: a sound's bank index is the handle the render thread already handed out, so a
/// registration is never dropped. Returns false, dropping it, once `closing` is set, so
/// dropping the output never waits on a stopped callback. After `patience` it sleeps
/// between attempts instead of spinning.
pub(super) fn push_until_closed<T>(
    producer: &mut impl Producer<Item = T>,
    mut item: T,
    closing: &AtomicBool,
    patience: Duration,
) -> bool {
    let started = Instant::now();
    loop {
        item = match producer.try_push(item) {
            Ok(()) => return true,
            Err(item) => item,
        };
        if closing.load(Ordering::Acquire) {
            return false;
        }
        if started.elapsed() < patience {
            std::thread::yield_now();
        } else {
            std::thread::sleep(DECODER_RETRY);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ringbuf::{HeapRb, traits::*};
    use std::sync::Arc;
    use std::sync::mpsc::channel;

    const SHORT: Duration = Duration::from_millis(20);

    #[test]
    fn a_draining_queue_takes_every_item() {
        let (mut producer, mut consumer) = HeapRb::<u32>::new(4).split();
        let mut feed = Feed::default();
        for value in 0..32 {
            assert_eq!(feed.push(&mut producer, value, SHORT), Pushed::Sent);
            assert_eq!(consumer.try_pop(), Some(value));
        }
        assert!(!feed.stalled());
    }

    #[test]
    fn a_stopped_callback_is_waited_for_once_then_dropped_at_once() {
        // Nothing drains the queue, as after the device went away.
        let (mut producer, _consumer) = HeapRb::<u32>::new(4).split();
        let mut feed = Feed::default();
        for value in 0..4 {
            assert_eq!(feed.push(&mut producer, value, SHORT), Pushed::Sent);
        }
        let started = Instant::now();
        assert_eq!(
            feed.push(&mut producer, 4, SHORT),
            Pushed::Dropped { stopped: true }
        );
        assert!(started.elapsed() >= SHORT);
        assert!(feed.stalled());
        // A frame's hundreds of commands no longer wait at all: with an hour's
        // patience each, a wait would fail this test.
        let started = Instant::now();
        for value in 0..10_000 {
            assert_eq!(
                feed.push(&mut producer, value, Duration::from_secs(3_600)),
                Pushed::Dropped { stopped: false }
            );
        }
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn a_callback_that_drains_again_ends_the_stall() {
        let (mut producer, mut consumer) = HeapRb::<u32>::new(2).split();
        let mut feed = Feed::default();
        feed.push(&mut producer, 0, SHORT);
        feed.push(&mut producer, 1, SHORT);
        assert_eq!(
            feed.push(&mut producer, 2, SHORT),
            Pushed::Dropped { stopped: true }
        );
        assert_eq!(consumer.try_pop(), Some(0));
        assert_eq!(feed.push(&mut producer, 3, SHORT), Pushed::Sent);
        assert!(!feed.stalled());
    }

    #[test]
    fn a_burst_waits_for_a_slow_callback() {
        let (mut producer, mut consumer) = HeapRb::<u32>::new(2).split();
        let mut feed = Feed::default();
        feed.push(&mut producer, 0, PATIENCE);
        feed.push(&mut producer, 1, PATIENCE);
        let reader = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            consumer.try_pop()
        });
        // Generous patience, so a loaded machine cannot fail it.
        let patience = Duration::from_secs(10);
        assert_eq!(feed.push(&mut producer, 2, patience), Pushed::Sent);
        assert_eq!(reader.join().unwrap(), Some(0));
        assert!(!feed.stalled());
    }

    #[test]
    fn the_decoder_stops_waiting_when_the_output_closes() {
        let (mut producer, consumer) = HeapRb::<u32>::new(1).split();
        let closing = Arc::new(AtomicBool::new(false));
        let (done, finished) = channel();
        let decoder = {
            let closing = Arc::clone(&closing);
            std::thread::spawn(move || {
                let first = push_until_closed(&mut producer, 0, &closing, SHORT);
                // The queue is full and nothing drains it.
                let second = push_until_closed(&mut producer, 1, &closing, SHORT);
                let _ = done.send((first, second));
            })
        };
        // Still waiting, well past its patience: a registration is not dropped.
        assert!(finished.recv_timeout(SHORT * 4).is_err());
        closing.store(true, Ordering::Release);
        let result = finished.recv_timeout(Duration::from_secs(10));
        assert_eq!(result, Ok((true, false)));
        decoder.join().unwrap();
        drop(consumer);
    }
}
