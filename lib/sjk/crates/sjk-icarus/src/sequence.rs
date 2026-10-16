//! Sequences (`CSequence`, `Sequence.cpp`) and the instance's store of them.
//!
//! A sequence is a list of commands with a place in a tree: the body of a loop, an `if`
//! or an `else`, a task, an affect, a script `run` from another. Sequences have ids from
//! one counter shared by every entity (`ICARUS_Instance::m_GUID`), and blocks name
//! sequences by id — stored as floats, as the reference stores them. They refer to each
//! other by id here, so a sequence freed while something still names it is simply not
//! found (where the reference would follow a dangling pointer).

use std::collections::{HashMap, VecDeque};

use crate::block::Block;

/// A looping sequence (`SQ_LOOP`).
pub const SQ_LOOP: u32 = 0x01;
/// Keep commands after running them: inside a loop they run again (`SQ_RETAIN`).
pub const SQ_RETAIN: u32 = 0x02;
/// An affect's body (`SQ_AFFECT`).
pub const SQ_AFFECT: u32 = 0x04;
/// A `run` script's body (`SQ_RUN`).
pub const SQ_RUN: u32 = 0x08;
/// Waiting to be used; a flush keeps it (`SQ_PENDING`).
pub const SQ_PENDING: u32 = 0x10;
/// A conditional's body (`SQ_CONDITIONAL`).
pub const SQ_CONDITIONAL: u32 = 0x20;
/// A task's body (`SQ_TASK`).
pub const SQ_TASK: u32 = 0x40;

/// One sequence.
#[derive(Debug)]
pub(crate) struct Sequence {
    pub flags: u32,
    pub iterations: i32,
    pub parent: Option<i32>,
    pub return_to: Option<i32>,
    pub children: Vec<i32>,
    pub commands: VecDeque<Block>,
}

impl Sequence {
    pub fn has(&self, flag: u32) -> bool {
        self.flags & flag != 0
    }
}

/// Every sequence of the instance (`ICARUS_Instance::m_sequences`), by id.
#[derive(Debug, Default)]
pub(crate) struct Sequences {
    map: HashMap<i32, Sequence>,
    next_id: i32,
}

impl Sequences {
    /// A new sequence with the next id (`ICARUS_Instance::GetSequence()`,
    /// `CSequence::Create`): common, one iteration, no relations.
    pub fn create(&mut self) -> i32 {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.map.insert(
            id,
            Sequence {
                flags: 0,
                iterations: 1,
                parent: None,
                return_to: None,
                children: Vec::new(),
                commands: VecDeque::new(),
            },
        );
        id
    }

    pub fn get(&self, id: i32) -> Option<&Sequence> {
        self.map.get(&id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Sequence> {
        self.map.get_mut(&id)
    }

    pub fn has_flag(&self, id: i32, flag: u32) -> bool {
        self.get(id).is_some_and(|sequence| sequence.has(flag))
    }

    /// Frees a sequence (`ICARUS_Instance::DeleteSequence`, `CSequence::Delete`): its
    /// parent forgets it, its children lose their parent, its commands go.
    pub fn delete(&mut self, id: i32) {
        let Some(sequence) = self.map.remove(&id) else {
            return;
        };
        if let Some(parent) = sequence.parent.and_then(|parent| self.map.get_mut(&parent)) {
            parent.children.retain(|&child| child != id);
        }
        for child in sequence.children {
            if let Some(child) = self.map.get_mut(&child) {
                child.parent = None;
            }
        }
    }

    /// Frees every sequence (`ICARUS_Instance::Free`).
    pub fn clear(&mut self) {
        self.map.clear();
    }

    /// `AddChild`.
    pub fn add_child(&mut self, parent: i32, child: i32) {
        if let Some(parent) = self.map.get_mut(&parent) {
            parent.children.push(child);
        }
    }

    /// `SetParent`: the child inherits `SQ_RETAIN` and `SQ_PENDING`.
    pub fn set_parent(&mut self, child: i32, parent: Option<i32>) {
        let inherited = parent
            .and_then(|parent| self.map.get(&parent))
            .map_or(0, |parent| parent.flags & (SQ_RETAIN | SQ_PENDING));
        if let Some(child) = self.map.get_mut(&child) {
            child.parent = parent;
            child.flags |= inherited;
        }
    }

    /// `SetReturn`.
    pub fn set_return(&mut self, id: i32, return_to: Option<i32>) {
        if let Some(sequence) = self.map.get_mut(&id) {
            sequence.return_to = return_to;
        }
    }

    /// `HasChild`: `sequence` is somewhere below `parent`.
    pub fn has_child(&self, parent: i32, sequence: i32) -> bool {
        let Some(parent) = self.map.get(&parent) else {
            return false;
        };
        parent
            .children
            .iter()
            .any(|&child| child == sequence || self.has_child(child, sequence))
    }

    /// `RemoveFlag(flag, children)`.
    pub fn remove_flag(&mut self, id: i32, flag: u32, children: bool) {
        let Some(sequence) = self.map.get_mut(&id) else {
            return;
        };
        sequence.flags &= !flag;
        if children {
            let count = sequence.children.len();
            for index in 0..count {
                let Some(child) = self
                    .map
                    .get(&id)
                    .and_then(|sequence| sequence.children.get(index).copied())
                else {
                    break;
                };
                self.remove_flag(child, flag, true);
            }
        }
    }

    /// The number of commands (`GetNumCommands`); zero for a sequence that is gone.
    pub fn command_count(&self, id: i32) -> usize {
        self.get(id).map_or(0, |sequence| sequence.commands.len())
    }

    /// The sequence's return (`GetReturn`).
    pub fn return_of(&self, id: i32) -> Option<i32> {
        self.get(id).and_then(|sequence| sequence.return_to)
    }
}
