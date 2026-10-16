//! Event-maintained looping-sound state from codemp cgame.
//!
//! Snapshot `loopSound` values are re-declared by the entity every frame. In
//! contrast, event loops are retained in `centity_t::loopingSound[8]` by
//! `CG_S_AddRealLoopingSound`/`CG_S_AddLoopingSound` and replayed each frame by
//! `CG_S_UpdateLoopingSounds` (`codemp/cgame/cg_ents.c:109-110,141-192,
//! 202-285`). The sound backend still receives ordinary frame loops; its
//! `S_ClearLoopingSounds` only clears that 32-entry frame list
//! (`codemp/client/snd_dma.cpp:1870-1884,1918-1974`).

use sjk_audio::{ChannelId, PlayRequest, SoundHandle, SourceId};
use sjk_protocol::{EntityState, Snapshot};

use crate::sound_events::normal_attenuation;

const MAX_ENTITIES: usize = 1_024;
const MAX_LOOPS_PER_ENTITY: usize = 8; // codemp/cgame/cg_local.h:319
const MAX_FRAME_LOOPS: usize = 32; // codemp/client/snd_dma.cpp:213

/// Event family responsible for an event-maintained loop/control action.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LegacyMaintainedEvent {
    PlayDoorLoopSound,
    StartLoopingSound,
    StopLoopingSound,
    TrackedGeneralSound,
    MuteSound,
}

impl LegacyMaintainedEvent {
    /// Stable parity-ledger label.
    pub const fn name(self) -> &'static str {
        match self {
            Self::PlayDoorLoopSound => "EV_PLAYDOORLOOPSOUND",
            Self::StartLoopingSound => "EV_STARTLOOPINGSOUND",
            Self::StopLoopingSound => "EV_STOPLOOPINGSOUND",
            Self::TrackedGeneralSound => "EV_GENERAL_SOUND_TRACKED",
            Self::MuteSound => "EV_MUTE_SOUND",
        }
    }
}

/// State change produced by one maintained-sound event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyMaintainedActionKind {
    Start,
    Stop,
    Mute,
}

/// Allocation-free diagnostic/control result for one event.
#[derive(Clone, Copy, Debug)]
pub struct LegacyMaintainedAction {
    /// Protocol event family that requested the state change.
    pub event: LegacyMaintainedEvent,
    /// Start, stop, or combined mute operation.
    pub kind: LegacyMaintainedActionKind,
    /// Entity whose cgame loop/voice state is affected.
    pub source: SourceId,
    /// Voice channel to kill for a mute operation.
    pub channel: Option<ChannelId>,
    /// Adapter catalog index selected by a start operation.
    pub sound: Option<u16>,
    /// Decoded engine-bank handle, if registration succeeded.
    pub handle: Option<SoundHandle>,
    /// Persistent slots created or cleared by this action.
    pub affected_slots: u8,
}

/// One persistent cgame slot translated into the backend's current frame.
#[derive(Clone, Copy, Debug)]
pub struct LegacyMaintainedLoopDecision {
    /// Event family that originally created this persistent slot.
    pub event: LegacyMaintainedEvent,
    /// Adapter catalog index retained in the slot.
    pub sound: Option<u16>,
    /// Decoded engine-bank handle, if available.
    pub handle: Option<SoundHandle>,
    /// Engine-generic spatial loop request for this frame.
    pub request: PlayRequest,
    /// Source velocity; event-maintained codemp loops use zero velocity.
    pub velocity: [f32; 3],
}

#[derive(Clone, Copy, Debug)]
struct Slot {
    event: LegacyMaintainedEvent,
    source: u16,
    sound: Option<u16>,
    handle: Option<SoundHandle>,
    fallback_origin: [f32; 3],
    tracker_target: Option<u16>,
}

impl Default for Slot {
    fn default() -> Self {
        Self {
            event: LegacyMaintainedEvent::StartLoopingSound,
            source: 0,
            sound: None,
            handle: None,
            fallback_origin: [0.0; 3],
            tracker_target: None,
        }
    }
}

/// Fixed-capacity cgame lifetime table for event-maintained loops.
pub(crate) struct MaintainedSounds {
    slots: Box<[Slot]>,
    counts: Box<[u8]>,
    actions: Vec<LegacyMaintainedAction>,
    frame: Vec<LegacyMaintainedLoopDecision>,
}

impl MaintainedSounds {
    pub(crate) fn new() -> Self {
        Self {
            slots: vec![Slot::default(); MAX_ENTITIES * MAX_LOOPS_PER_ENTITY].into_boxed_slice(),
            counts: vec![0; MAX_ENTITIES].into_boxed_slice(),
            actions: Vec::with_capacity(MAX_ENTITIES + 3),
            frame: Vec::with_capacity(MAX_FRAME_LOOPS),
        }
    }

    pub(crate) fn begin_snapshot(&mut self) {
        self.actions.clear();
    }

    pub(crate) fn start(
        &mut self,
        event: LegacyMaintainedEvent,
        source: u16,
        sound: Option<u16>,
        handle: Option<SoundHandle>,
        origin: [f32; 3],
        tracker_target: Option<u16>,
    ) {
        let entity = usize::from(source);
        if entity >= MAX_ENTITIES {
            return;
        }
        if sound.is_none() {
            self.push_action(LegacyMaintainedAction {
                event,
                kind: LegacyMaintainedActionKind::Start,
                source: SourceId(u32::from(source)),
                channel: None,
                sound,
                handle,
                affected_slots: 0,
            });
            return;
        }
        let base = entity * MAX_LOOPS_PER_ENTITY;
        let count = usize::from(self.counts[entity]);
        let replacement = Slot {
            event,
            source,
            sound,
            handle,
            fallback_origin: origin,
            tracker_target,
        };

        // This intentionally retains codemp's observable behavior: an already
        // present handle is updated at cg_ents.c:148-166, then another slot is
        // appended at :167-180 (the comment says return; the code does not).
        if let Some(existing) = self.slots[base..base + count]
            .iter_mut()
            .find(|slot| slot.sound == sound)
        {
            *existing = replacement;
        }
        let mut affected = 0;
        if count < MAX_LOOPS_PER_ENTITY {
            self.slots[base + count] = replacement;
            self.counts[entity] += 1;
            affected = 1;
        }
        self.push_action(LegacyMaintainedAction {
            event,
            kind: LegacyMaintainedActionKind::Start,
            source: SourceId(u32::from(source)),
            channel: None,
            sound,
            handle,
            affected_slots: affected,
        });
    }

    pub(crate) fn stop(&mut self, event: LegacyMaintainedEvent, source: u16) {
        let entity = usize::from(source);
        if entity >= MAX_ENTITIES {
            return;
        }
        let affected = self.counts[entity];
        let base = entity * MAX_LOOPS_PER_ENTITY;
        self.slots[base..base + usize::from(affected)].fill(Slot::default());
        self.counts[entity] = 0;
        self.push_action(LegacyMaintainedAction {
            event,
            kind: LegacyMaintainedActionKind::Stop,
            source: SourceId(u32::from(source)),
            channel: None,
            sound: None,
            handle: None,
            affected_slots: affected,
        });
    }

    pub(crate) fn mute(&mut self, source: u16, channel: u32) {
        let entity = usize::from(source);
        let affected = self.counts.get(entity).copied().unwrap_or(0);
        if entity < MAX_ENTITIES {
            let base = entity * MAX_LOOPS_PER_ENTITY;
            self.slots[base..base + usize::from(affected)].fill(Slot::default());
            self.counts[entity] = 0;
        }
        self.push_action(LegacyMaintainedAction {
            event: LegacyMaintainedEvent::MuteSound,
            kind: LegacyMaintainedActionKind::Mute,
            source: SourceId(u32::from(source)),
            channel: Some(ChannelId(channel)),
            sound: None,
            handle: None,
            affected_slots: affected,
        });
    }

    pub(crate) fn build_frame(
        &mut self,
        snapshot: &Snapshot,
        mut presented_origin: impl FnMut(&EntityState) -> [f32; 3],
    ) -> &[LegacyMaintainedLoopDecision] {
        self.frame.clear();
        'entities: for entity in 0..MAX_ENTITIES {
            let base = entity * MAX_LOOPS_PER_ENTITY;
            for slot in &self.slots[base..base + usize::from(self.counts[entity])] {
                if self.frame.len() >= MAX_FRAME_LOOPS {
                    break 'entities;
                }
                let origin = if let Some(target) = slot.tracker_target
                    && target != snapshot.player.client_num()
                {
                    let Some(state) = snapshot
                        .entities
                        .iter()
                        .find(|state| state.number() == target)
                    else {
                        continue;
                    };
                    presented_origin(state)
                } else if let Some(state) = snapshot
                    .entities
                    .iter()
                    .find(|state| state.number() == slot.source)
                {
                    presented_origin(state)
                } else {
                    slot.fallback_origin
                };
                self.frame.push(LegacyMaintainedLoopDecision {
                    event: slot.event,
                    sound: slot.sound,
                    handle: slot.handle,
                    request: PlayRequest {
                        origin: Some(origin),
                        source: SourceId(u32::from(slot.source)),
                        channel: ChannelId(0),
                        volume: 1.0,
                        attenuation: normal_attenuation(0),
                    },
                    velocity: [0.0; 3],
                });
            }
        }
        &self.frame
    }

    pub(crate) fn actions(&self) -> &[LegacyMaintainedAction] {
        &self.actions
    }

    pub(crate) fn frame(&self) -> &[LegacyMaintainedLoopDecision] {
        &self.frame
    }

    pub(crate) fn active_slots(&self) -> usize {
        self.counts.iter().map(|count| usize::from(*count)).sum()
    }

    fn push_action(&mut self, action: LegacyMaintainedAction) {
        if self.actions.len() < self.actions.capacity() {
            self.actions.push(action);
        }
    }
}
