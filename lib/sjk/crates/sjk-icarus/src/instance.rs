//! The interpreter instance (`ICARUS_Instance`) and its per-entity sequencers, with the
//! engine functions a game calls (`GameInterface.cpp`, `Q3_Interface.cpp`,
//! `sv_gameapi.cpp`'s `ICARUS_*` wrappers).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::block::Block;
use crate::host::{DebugLevel, IcarusHost, Owner};
use crate::print;
use crate::sequence::Sequences;
use crate::sequencer::{Sequencer, Streams};
use crate::shared::SharedText;
use crate::variables::{MAX_VARIABLES, Variables};

/// Sizes an instance is made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IcarusConfig {
    /// How many task slots each entity has (`NUM_TIDS` in Jedi Academy: the voice, the
    /// animations, the move, ...). The game names the slots; the interpreter only keeps
    /// the task id waiting in each.
    pub task_slots: usize,
    /// How many variables may be declared ([`MAX_VARIABLES`] in the reference).
    pub max_variables: i32,
}

impl Default for IcarusConfig {
    fn default() -> Self {
        Self {
            task_slots: 10,
            max_variables: MAX_VARIABLES,
        }
    }
}

/// The script interpreter: every entity's sequencer and task manager, the sequences
/// they run, the signals and variables scripts share, and the scripts read so far.
///
/// One instance lives as long as a server: the reference keeps its variables in the
/// engine across levels (`Q3_InitVariables` is never called in multiplayer), while
/// [`Icarus::shutdown`] and [`Icarus::init`] start the rest afresh for each level as
/// `ICARUS_Shutdown` and `ICARUS_Init` do.
#[derive(Debug)]
pub struct Icarus<O: Owner> {
    pub(crate) config: IcarusConfig,
    pub(crate) sequences: Sequences,
    pub(crate) sequencers: HashMap<O, Sequencer>,
    pub(crate) next_serial: u64,
    pub(crate) streams: Streams,
    signals: HashSet<String>,
    /// `ICARUS_EntList`: upper-cased `script_targetname` to entity.
    names: HashMap<String, O>,
    /// `ICARUS_BufferList`: script name (without extension) to its bytes.
    pub(crate) scripts: HashMap<String, Arc<[u8]>>,
    pub(crate) variables: Variables,
    /// The part of the engine's shared buffer with the game that `get(STRING, ...)`
    /// answers in (see [`crate::shared`]).
    pub(crate) shared: SharedText,
    /// `CTaskManager::Get`'s static buffer: the last number a command turned into text.
    pub(crate) temp: String,
}

impl<O: Owner> Icarus<O> {
    /// A new interpreter with no entities (`ICARUS_Init`).
    pub fn new(config: IcarusConfig) -> Self {
        Self {
            config,
            sequences: Sequences::default(),
            sequencers: HashMap::new(),
            next_serial: 0,
            streams: Streams::default(),
            signals: HashSet::new(),
            names: HashMap::new(),
            scripts: HashMap::new(),
            variables: Variables::new(config.max_variables),
            shared: SharedText::default(),
            temp: String::new(),
        }
    }

    /// The sizes this instance was made with.
    pub fn config(&self) -> IcarusConfig {
        self.config
    }

    /// Starts a level (`ICARUS_Init`): a fresh instance — sequence ids from zero, no
    /// signals — keeping the variables.
    pub fn init(&mut self) {
        let variables = std::mem::replace(
            &mut self.variables,
            Variables::new(self.config.max_variables),
        );
        *self = Self {
            variables,
            ..Self::new(self.config)
        };
    }

    /// Ends a level (`ICARUS_Shutdown`): every entity's sequencer freed, the scripts
    /// read, the names and signals forgotten. The variables stay.
    pub fn shutdown<H: IcarusHost<O> + ?Sized>(&mut self, host: &mut H) {
        let owners: Vec<O> = self.sequencers.keys().copied().collect();
        for owner in owners {
            let name = host.entity_names(owner).script_targetname;
            self.free_entity(owner, name.as_deref());
        }
        self.scripts.clear();
        self.names.clear();
        self.sequences.clear();
        self.sequencers.clear();
        self.signals.clear();
    }

    /// Gives an entity a sequencer and task manager (`ICARUS_InitEnt`) and associates
    /// its `script_targetname`. Precaching the scripts its behaviour sets name is the
    /// game's (`ICARUS_PrecacheEnt` reads game data): see [`Icarus::interrogate`]. An
    /// entity that has one already is left alone.
    pub fn init_entity(&mut self, owner: O, script_targetname: Option<&str>) {
        if self.sequencers.contains_key(&owner) {
            return;
        }
        let serial = self.next_serial;
        self.next_serial += 1;
        self.sequencers
            .insert(owner, Sequencer::new(serial, self.config.task_slots));
        self.associate(owner, script_targetname);
    }

    /// Frees an entity's sequencer (`ICARUS_FreeEnt`): its name is forgotten "so that
    /// when their g_entity index is reused, ICARUS doesn't try to affect the new
    /// (incorrect) ent", the commands it holds are recalled and dropped with its
    /// sequences.
    pub fn free_entity(&mut self, owner: O, script_targetname: Option<&str>) {
        if !self.sequencers.contains_key(&owner) {
            return;
        }
        if let Some(name) = script_targetname.filter(|name| !name.is_empty()) {
            self.names.remove(&upper(name));
        }
        // `DeleteSequencer`: recall, then free the task manager, the sequencer and its
        // sequences.
        self.recall(owner);
        if let Some(sequencer) = self.sequencers.remove(&owner) {
            for id in sequencer.sequences {
                self.sequences.delete(id);
            }
            for stream in sequencer.streams_created {
                self.streams.remove(stream);
            }
        }
    }

    /// Associates a `script_targetname` with an entity (`ICARUS_AssociateEnt`), so that
    /// `affect` finds it; an empty or missing name is ignored.
    pub fn associate(&mut self, owner: O, script_targetname: Option<&str>) {
        if let Some(name) = script_targetname.filter(|name| !name.is_empty()) {
            self.names.insert(upper(name), owner);
        }
    }

    /// The entity a `script_targetname` names (`Q3_GetEntityByName`), any case.
    pub fn entity_by_name(&self, name: &str) -> Option<O> {
        if name.is_empty() {
            return None;
        }
        self.names.get(&upper(name)).copied()
    }

    /// Whether the entity has a sequencer (`ICARUS_IsInitialized`).
    pub fn is_initialized(&self, owner: O) -> bool {
        self.sequencers.contains_key(&owner)
    }

    /// Whether the entity's task manager holds a task (`ICARUS_IsRunning`).
    pub fn is_running(&self, owner: O) -> bool {
        self.sequencers
            .get(&owner)
            .is_some_and(|sequencer| !sequencer.tasks.tasks.is_empty())
    }

    /// Runs the entity's commands for this frame (`ICARUS_MaintainTaskManager`): false
    /// if it has no task manager.
    pub fn maintain<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, host: &mut H) -> bool {
        if !self.sequencers.contains_key(&owner) {
            return false;
        }
        self.update(owner, host);
        true
    }

    /// `Q3_TaskIDPending`: whether a task waits in the entity's slot.
    pub fn task_id_pending(&self, owner: O, slot: usize) -> bool {
        self.sequencers
            .get(&owner)
            .and_then(|sequencer| sequencer.slots.get(slot))
            .is_some_and(|&task| task >= 0)
    }

    /// The task id waiting in the entity's slot, if any.
    pub fn task_id(&self, owner: O, slot: usize) -> Option<i32> {
        self.sequencers
            .get(&owner)
            .and_then(|sequencer| sequencer.slots.get(slot))
            .copied()
            .filter(|&task| task >= 0)
    }

    /// `Q3_TaskIDComplete`: the task in the slot is completed, and cleared from every
    /// slot that holds it "so we don't complete more than once".
    pub fn task_id_complete(&mut self, owner: O, slot: usize) {
        let Some(sequencer) = self.sequencers.get_mut(&owner) else {
            return;
        };
        let Some(&task) = sequencer.slots.get(slot).filter(|&&task| task >= 0) else {
            return;
        };
        sequencer.tasks.completed(task);
        for held in &mut sequencer.slots {
            if *held == task {
                *held = -1;
            }
        }
    }

    /// `Q3_TaskIDSet`: the slot waits for `task`; a task already there is completed first.
    pub fn task_id_set(&mut self, owner: O, slot: usize, task: i32) {
        if slot >= self.config.task_slots {
            return;
        }
        self.task_id_complete(owner, slot);
        // The reference writes the entity's slot whether or not it has a sequencer; one
        // without has nothing to wait in.
        if let Some(held) = self
            .sequencers
            .get_mut(&owner)
            .and_then(|sequencer| sequencer.slots.get_mut(slot))
        {
            *held = task;
        }
    }

    /// `Q3_TaskIDClear`: the slot waits for nothing; its task is not completed.
    pub fn task_id_clear(&mut self, owner: O, slot: usize) {
        if let Some(held) = self
            .sequencers
            .get_mut(&owner)
            .and_then(|sequencer| sequencer.slots.get_mut(slot))
        {
            *held = -1;
        }
    }

    /// `Signal`: marks a signal raised.
    pub fn signal(&mut self, name: &str) {
        self.signals.insert(name.to_owned());
    }

    /// `CheckSignal`.
    pub fn check_signal(&self, name: &str) -> bool {
        self.signals.contains(name)
    }

    /// `ClearSignal`.
    pub fn clear_signal(&mut self, name: &str) {
        self.signals.remove(name);
    }

    /// The declared variables (`Q3_Registers.cpp`).
    pub fn variables(&self) -> &Variables {
        &self.variables
    }

    /// `Q3_SetVar`: a `set` of a declared variable. A float variable takes a leading `+`
    /// or `-` as an increment; a name that is no variable is reported.
    pub fn set_variable<H: IcarusHost<O> + ?Sized>(
        &mut self,
        host: &mut H,
        name: &str,
        data: &str,
    ) {
        use crate::variables::VariableType;
        match self.variables.declared(name) {
            VariableType::Float => {
                let increment = counter_increment(data);
                let value = if increment != 0.0 {
                    self.variables.float(name).unwrap_or(0.0) + increment
                } else {
                    crate::cnum::atof(data) as f32
                };
                self.variables.set_float(name, value);
            }
            VariableType::String => self.variables.set_string(name, data),
            VariableType::Vector => self.variables.set_vector(name, data),
            VariableType::None => print::debug(
                host,
                DebugLevel::Error,
                &format!("{name} variable or field not found!\n"),
            ),
        }
    }

    /// Recalls every task the entity's task manager holds back into its current
    /// sequence, or drops them if it has none (`CSequencer::Recall`).
    pub(crate) fn recall(&mut self, owner: O) {
        loop {
            let Some(sequencer) = self.sequencers.get_mut(&owner) else {
                return;
            };
            // `RecallTask`: from the back.
            let Some(task) = sequencer.tasks.tasks.pop_back() else {
                return;
            };
            if sequencer.current.is_some() {
                self.push_command(owner, task.block, false);
            }
        }
    }

    /// `CSequencer::PushCommand`: onto the entity's current sequence (dropped if it has
    /// none, as the reference leaks it).
    pub(crate) fn push_command(&mut self, owner: O, block: Block, front: bool) {
        let Some(sequencer) = self.sequencers.get_mut(&owner) else {
            return;
        };
        let Some(sequence) = sequencer
            .current
            .and_then(|current| self.sequences.get_mut(current))
        else {
            return;
        };
        if front {
            sequence.commands.push_front(block);
        } else {
            sequence.commands.push_back(block);
        }
        sequencer.command_count += 1;
    }

    /// `CSequencer::PopCommand`: from the entity's current sequence.
    pub(crate) fn pop_command(&mut self, owner: O, back: bool) -> Option<Block> {
        let sequencer = self.sequencers.get_mut(&owner)?;
        let sequence = sequencer
            .current
            .and_then(|current| self.sequences.get_mut(current))?;
        let block = if back {
            sequence.commands.pop_back()
        } else {
            sequence.commands.pop_front()
        }?;
        sequencer.command_count -= 1;
        Some(block)
    }

    /// The entity's current sequence.
    pub(crate) fn current(&self, owner: O) -> Option<i32> {
        self.sequencers
            .get(&owner)
            .and_then(|sequencer| sequencer.current)
    }

    pub(crate) fn set_current(&mut self, owner: O, sequence: Option<i32>) {
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.current = sequence;
        }
    }

    /// Whether the entity's current sequence has `flag`.
    pub(crate) fn current_has(&self, owner: O, flag: u32) -> bool {
        self.current(owner)
            .is_some_and(|current| self.sequences.has_flag(current, flag))
    }

    /// The serial of the entity's sequencer: a sequencer freed and made again under the
    /// same entity has another.
    pub(crate) fn serial(&self, owner: O) -> Option<u64> {
        self.sequencers
            .get(&owner)
            .map(|sequencer| sequencer.serial)
    }
}

/// `Q3_CheckStringCounterIncrement`: `+n` is n, `-n` is minus n, anything else zero.
fn counter_increment(text: &str) -> f32 {
    let bytes = text.as_bytes();
    match (bytes.first(), bytes.len() > 1) {
        (Some(b'+'), true) => crate::cnum::atof(&text[1..]) as f32,
        (Some(b'-'), true) => (crate::cnum::atof(&text[1..]) * -1.0) as f32,
        _ => 0.0,
    }
}

/// `Q_strupr`: ASCII letters upper-cased.
pub(crate) fn upper(text: &str) -> String {
    text.to_ascii_uppercase()
}
