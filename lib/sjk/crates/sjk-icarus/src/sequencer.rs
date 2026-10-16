//! One entity's sequencer (`CSequencer`, `Sequencer.cpp`): the state it keeps while it
//! routes a script's blocks into sequences, and the parse streams it routes from.
//!
//! The routing itself is in [`crate::route`], the pre-processing of commands on their way
//! to the task manager in [`crate::prep`].

use std::collections::HashMap;

use crate::stream::BlockStream;
use crate::tasks::TaskManager;

/// A parse stream's id (a `bstream_t *`).
pub(crate) type StreamId = u32;

/// A parse stream (`bstream_t`): the block stream it reads — its own, or, for the
/// temporary one an affect routes through, another's, whose position it shares — and
/// the stream that was current before it.
#[derive(Debug)]
pub(crate) struct ParseStream {
    pub source: StreamSource,
    pub last: Option<StreamId>,
}

#[derive(Debug)]
pub(crate) enum StreamSource {
    Own(BlockStream),
    Shares(StreamId),
}

/// Every parse stream alive, by id.
#[derive(Debug, Default)]
pub(crate) struct Streams {
    map: HashMap<StreamId, ParseStream>,
    next: StreamId,
}

impl Streams {
    pub fn add(&mut self, stream: ParseStream) -> StreamId {
        let id = self.next;
        self.next = self.next.wrapping_add(1);
        self.map.insert(id, stream);
        id
    }

    pub fn remove(&mut self, id: StreamId) {
        self.map.remove(&id);
    }

    pub fn last(&self, id: StreamId) -> Option<StreamId> {
        self.map.get(&id).and_then(|stream| stream.last)
    }

    /// The block stream a parse stream reads, following a shared one to its owner.
    pub fn blocks(&mut self, id: StreamId) -> Option<&mut BlockStream> {
        let mut id = id;
        for _ in 0..8 {
            match &self.map.get(&id)?.source {
                StreamSource::Own(_) => break,
                StreamSource::Shares(owner) => id = *owner,
            }
        }
        match &mut self.map.get_mut(&id)?.source {
            StreamSource::Own(blocks) => Some(blocks),
            StreamSource::Shares(_) => None,
        }
    }
}

/// One entity's sequencer and task manager.
#[derive(Debug)]
pub(crate) struct Sequencer {
    pub serial: u64,
    /// `m_sequences`, in creation order.
    pub sequences: Vec<i32>,
    /// `m_taskSequences`: a task group to its body.
    pub task_sequences: HashMap<usize, i32>,
    /// `m_curSequence`.
    pub current: Option<i32>,
    /// `m_curGroup`: the task body being routed or run.
    pub current_group: Option<usize>,
    /// `m_curStream`.
    pub current_stream: Option<StreamId>,
    /// `m_numCommands`: commands pushed less commands popped, over every sequence.
    pub command_count: i32,
    /// `m_elseValid`: blocks left in which an `else` may follow an `if`.
    pub else_valid: i32,
    /// `m_elseOwner`: the sequence at whose front the `if` an `else` belongs to waits.
    pub else_owner: Option<i32>,
    /// `m_streamsCreated`.
    pub streams_created: Vec<StreamId>,
    pub tasks: TaskManager,
    /// The entity's task slots (`taskID[NUM_TIDS]`): -1 for none.
    pub slots: Vec<i32>,
}

impl Sequencer {
    pub fn new(serial: u64, task_slots: usize) -> Self {
        Self {
            serial,
            sequences: Vec::new(),
            task_sequences: HashMap::new(),
            current: None,
            current_group: None,
            current_stream: None,
            command_count: 0,
            else_valid: 0,
            else_owner: None,
            streams_created: Vec::new(),
            tasks: TaskManager::default(),
            slots: vec![-1; task_slots],
        }
    }
}
