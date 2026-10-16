//! The task manager's state (`CTaskManager`, `CTaskGroup`, `CTask` in `TaskManager.cpp`).
//!
//! A task is a command handed from the sequencer to be carried out, with an id from the
//! task manager's counter. A task group is a named `task(...) { }` body: the ids of the
//! commands that ran inside it, and how many of them the game has completed; `wait("name")`
//! waits for all of them. Tasks and groups draw their ids from the same counter, as in
//! the reference.

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::block::Block;

/// "Check for run away scripts": the commands one update may run (`RUNAWAY_LIMIT`).
pub const RUNAWAY_LIMIT: i32 = 256;

/// A command being carried out (`CTask`).
#[derive(Debug)]
pub(crate) struct Task {
    pub id: i32,
    pub time_stamp: u32,
    pub block: Block,
}

/// A named task body (`CTaskGroup`).
#[derive(Debug, Default)]
pub(crate) struct TaskGroup {
    pub id: i32,
    pub parent: Option<usize>,
    /// Task id to whether it is done (`m_completedTasks`).
    pub tasks: BTreeMap<i32, bool>,
    pub completed: i32,
}

impl TaskGroup {
    /// `Init`: forget the tasks and the parent.
    pub fn reset(&mut self) {
        self.tasks.clear();
        self.completed = 0;
        self.parent = None;
    }

    /// `Complete`: every task marked — counted, so a task marked twice counts twice.
    pub fn complete(&self) -> bool {
        usize::try_from(self.completed).is_ok_and(|completed| completed == self.tasks.len())
    }

    /// `MarkTaskComplete`.
    pub fn mark(&mut self, task: i32) -> bool {
        match self.tasks.get_mut(&task) {
            Some(done) => {
                *done = true;
                self.completed += 1;
                true
            }
            None => false,
        }
    }
}

/// One entity's task manager (`CTaskManager`).
#[derive(Debug, Default)]
pub(crate) struct TaskManager {
    pub tasks: VecDeque<Task>,
    pub groups: Vec<TaskGroup>,
    pub group_names: HashMap<String, usize>,
    pub current_group: Option<usize>,
    pub next_id: i32,
    pub count: i32,
}

impl TaskManager {
    /// `AddTaskGroup`: a group by this name is reset and reused.
    pub fn add_group(&mut self, name: &str) -> usize {
        if let Some(&index) = self.group_names.get(name) {
            self.groups[index].reset();
            return index;
        }
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.groups.push(TaskGroup {
            id,
            ..TaskGroup::default()
        });
        let index = self.groups.len() - 1;
        self.group_names.insert(name.to_owned(), index);
        index
    }

    /// `GetTaskGroup(int)`: a group by its id.
    pub fn group_by_id(&self, id: i32) -> Option<usize> {
        self.groups.iter().position(|group| group.id == id)
    }

    /// `SetCommand`: a new task for the block, in the current group if there is one.
    pub fn set_command(&mut self, block: Block, front: bool) {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if let Some(group) = self
            .current_group
            .and_then(|group| self.groups.get_mut(group))
        {
            group.tasks.insert(id, false);
        }
        let task = Task {
            id,
            time_stamp: 0,
            block,
        };
        if front {
            self.tasks.push_front(task);
        } else {
            self.tasks.push_back(task);
        }
    }

    /// `Completed`: the first group holding the task marks it.
    pub fn completed(&mut self, task: i32) {
        for group in &mut self.groups {
            if group.mark(task) {
                break;
            }
        }
    }

    /// `MarkTask(id, TASK_START)`: the group is reset and becomes current.
    pub fn start_group(&mut self, index: usize) {
        let parent = self.current_group;
        if let Some(group) = self.groups.get_mut(index) {
            group.reset();
            group.parent = parent;
            self.current_group = Some(index);
        }
    }

    /// `MarkTask(id, TASK_END)`: the current group's parent becomes current.
    pub fn end_group(&mut self) {
        if let Some(current) = self.current_group {
            self.current_group = self.groups.get(current).and_then(|group| group.parent);
        }
    }
}
