//! Commands on their way from the sequencer to the task manager (`CSequencer::Prep` and
//! its `Check*` steps, `Prime`, `Callback`, `Affect` in `Sequencer.cpp`).
//!
//! A command popped off the current sequence may be a structural one — an `affect`, a
//! `flush`, a `loop` or its end, a `run` or its end, an `if` or its end, a `do` or a task
//! body's end. Each is resolved here (entering or leaving a sequence, starting another
//! entity's affect) and replaced by the next command, until an ordinary command is left
//! for the task manager. Commands of a retained sequence (a loop body, a task) are put
//! back at the front of their sequence as they are used, so that they run again.
//!
//! Where the reference leaves a command both put back and handed on (it keeps a pointer
//! to a block it has just pushed) only in cases that end in a crash, this hands nothing
//! on.

use crate::Icarus;
use crate::block::Block;
use crate::host::{DebugLevel, IcarusHost, Owner};
use crate::ids::*;
use crate::print;
use crate::sequence::{SQ_AFFECT, SQ_CONDITIONAL, SQ_LOOP, SQ_PENDING, SQ_RETAIN, SQ_RUN, SQ_TASK};

impl<O: Owner> Icarus<O> {
    /// `Prep`: every check in the reference's order.
    pub(crate) fn prep<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        self.check_affect(owner, command, host);
        self.check_flush(owner, command, host);
        self.check_loop(owner, command, host);
        self.check_run(owner, command, host);
        self.check_if(owner, command, host);
        self.check_do(owner, command, host);
    }

    /// `Prime`: the command resolved and handed to the task manager, at the back.
    pub(crate) fn prime<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: Option<Block>,
        host: &mut H,
    ) {
        let mut command = command;
        self.prep(owner, &mut command, host);
        if let (Some(command), Some(sequencer)) = (command, self.sequencers.get_mut(&owner)) {
            sequencer.tasks.set_command(command, false);
        }
    }

    /// Puts a used command back at the front of its sequence if the sequence is
    /// retained, else drops it.
    fn retain_or_drop(&mut self, owner: O, block: Block) {
        if self.current_has(owner, SQ_RETAIN) {
            self.push_command(owner, block, true);
        }
    }

    /// The next command of the (new) current sequence, resolved.
    fn next_command<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        *command = self.pop_command(owner, true);
        self.prep(owner, command, host);
    }

    /// `CheckRun`: entering a `run` script's sequence, or leaving it at its end.
    fn check_run<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        let Some(id) = command.as_ref().map(|block| block.id) else {
            return;
        };
        if id == ID_RUN {
            let Some(block) = command.take() else { return };
            let sequence = block.f32_at(1) as i32;
            print::command(
                host,
                owner,
                &format!("run( \"{}\" ); [{}]", block.str_at(0), host.time()),
            );
            self.retain_or_drop(owner, block);
            let found = self.own_sequence(owner, sequence);
            self.set_current(owner, found);
            let Some(found) = found else {
                print::debug(host, DebugLevel::Error, "Unable to find 'run' sequence!\n");
                return;
            };
            if self.sequences.command_count(found) > 0 {
                self.next_command(owner, command, host);
            }
            return;
        }
        if id == ID_BLOCK_END && self.current_has(owner, SQ_RUN) {
            let Some(block) = command.take() else { return };
            self.retain_or_drop(owner, block);
            let back = self
                .current(owner)
                .and_then(|current| self.return_sequence(current));
            self.set_current(owner, back);
            if back.is_some_and(|back| self.sequences.command_count(back) > 0) {
                self.next_command(owner, command, host);
            }
        }
    }

    /// `CheckIf`: the condition evaluated and the `if` or `else` body entered, or the
    /// body's end left.
    fn check_if<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        let Some(id) = command.as_ref().map(|block| block.id) else {
            return;
        };
        if id == ID_IF {
            let Some(block) = command.take() else { return };
            let result = self.evaluate_conditional(owner, &block, host);
            let count = block.members.len();
            let taken = if result != 0 {
                let success = if block.has_else() {
                    block.f32_at(count.wrapping_sub(2))
                } else {
                    block.f32_at(count.wrapping_sub(1))
                };
                Some((
                    success as i32,
                    "Unable to find conditional success sequence!\n",
                ))
            } else if block.has_else() {
                Some((
                    block.f32_at(count.wrapping_sub(1)) as i32,
                    "Unable to find conditional failure sequence!\n",
                ))
            } else {
                None
            };
            if let Some((sequence, missing)) = taken {
                let Some(found) = self.own_sequence(owner, sequence) else {
                    print::debug(host, DebugLevel::Error, missing);
                    return;
                };
                self.retain_or_drop(owner, block);
                self.set_current(owner, Some(found));
                self.next_command(owner, command, host);
                return;
            }
            // "Conditional failed, just move on to the next command"
            self.retain_or_drop(owner, block);
            self.next_command(owner, command, host);
            return;
        }
        if id == ID_BLOCK_END && self.current_has(owner, SQ_CONDITIONAL) {
            let Some(current) = self.current(owner) else {
                return;
            };
            if self.sequences.return_of(current).is_none() {
                *command = None;
                return;
            }
            let Some(block) = command.take() else { return };
            let parent_retains = self
                .sequences
                .get(current)
                .and_then(|sequence| sequence.parent)
                .is_some_and(|parent| self.sequences.has_flag(parent, SQ_RETAIN));
            if parent_retains {
                self.push_command(owner, block, true);
            }
            let back = self.return_sequence(current);
            self.set_current(owner, back);
            if back.is_none() {
                return;
            }
            self.next_command(owner, command, host);
        }
    }

    /// `CheckLoop`: entering a loop's sequence (its count restored), or at its end,
    /// another iteration or leaving it.
    fn check_loop<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        let Some(id) = command.as_ref().map(|block| block.id) else {
            return;
        };
        if id == ID_LOOP {
            let Some(block) = command.take() else { return };
            let (iterations, id_member) = if block.member_id(0) == Some(ID_RANDOM) {
                (host.random(block.f32_at(1), block.f32_at(2)) as i32, 3)
            } else {
                (block.f32_at(0) as i32, 1)
            };
            let sequence = block.f32_at(id_member) as i32;
            let Some(found) = self.own_sequence(owner, sequence) else {
                print::debug(host, DebugLevel::Error, "Unable to find 'loop' sequence!\n");
                return;
            };
            if self
                .sequences
                .get(found)
                .and_then(|found| found.parent)
                .is_none()
            {
                return;
            }
            if let Some(found) = self.sequences.get_mut(found) {
                found.iterations = iterations;
            }
            self.retain_or_drop(owner, block);
            self.set_current(owner, Some(found));
            self.next_command(owner, command, host);
            return;
        }
        if id == ID_BLOCK_END && self.current_has(owner, SQ_LOOP) {
            let Some(current) = self.current(owner) else {
                return;
            };
            let iterations = {
                let Some(sequence) = self.sequences.get_mut(current) else {
                    return;
                };
                if sequence.iterations > 0 {
                    sequence.iterations -= 1;
                }
                sequence.iterations
            };
            let Some(block) = command.take() else { return };
            if iterations != 0 {
                // "Another iteration is going to happen, so this will need to be considered again"
                self.push_command(owner, block, true);
                self.next_command(owner, command, host);
                return;
            }
            if self.sequences.return_of(current).is_none() {
                return;
            }
            let parent_retains = self
                .sequences
                .get(current)
                .and_then(|sequence| sequence.parent)
                .is_some_and(|parent| self.sequences.has_flag(parent, SQ_RETAIN));
            if parent_retains {
                self.push_command(owner, block, true);
            }
            let back = self.return_sequence(current);
            self.set_current(owner, back);
            if back.is_none() {
                return;
            }
            self.next_command(owner, command, host);
        }
    }

    /// `CheckFlush`: every other sequence of the entity freed.
    fn check_flush<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        if command.as_ref().map(|block| block.id) != Some(ID_FLUSH) {
            return;
        }
        let Some(block) = command.take() else { return };
        let current = self.current(owner);
        self.flush(owner, current);
        self.retain_or_drop(owner, block);
        self.next_command(owner, command, host);
    }

    /// `CheckAffect`: the affected entity's sequence started (flushing or inserted), and
    /// its task manager updated at once; or an affect body's end left.
    fn check_affect<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        let Some(id) = command.as_ref().map(|block| block.id) else {
            return;
        };
        if id == ID_AFFECT {
            let Some(block) = command.as_ref() else {
                return;
            };
            let entity_name = block.str_at(0).into_owned();
            let mut member = 1;
            let mut entity = self.entity_by_name(&entity_name);
            if entity.is_none() {
                let name = match block.member_id(0) {
                    Some(TK_STRING | TK_IDENTIFIER | TK_CHAR) => Some(entity_name.clone()),
                    Some(ID_GET) => {
                        let kind = block.f32_at(1) as i32;
                        let get_name = block.str_at(2).into_owned();
                        member = 3;
                        if kind != TK_STRING {
                            print::debug(
                                host,
                                DebugLevel::Error,
                                "Invalid parameter type on affect _1",
                            );
                            return;
                        }
                        if !self.ask_string(owner, kind, &get_name, host) {
                            return;
                        }
                        Some(self.shared.text().into_owned())
                    }
                    _ => {
                        print::debug(
                            host,
                            DebugLevel::Error,
                            "Invalid parameter type on affect _2",
                        );
                        return;
                    }
                };
                entity = name.as_deref().and_then(|name| self.entity_by_name(name));
                if entity.is_none() {
                    print::debug(
                        host,
                        DebugLevel::Warning,
                        &format!("'{}' : invalid affect() target\n", name.unwrap_or_default()),
                    );
                }
            }
            let Some(block) = command.take() else { return };
            let kind = block.f32_at(member) as i32;
            let sequence = block.f32_at(member + 1) as i32;
            self.retain_or_drop(owner, block);
            let target = entity.filter(|&target| self.sequencers.contains_key(&target));
            let Some(target) = target else {
                self.next_command(owner, command, host);
                return;
            };
            self.affect(target, sequence, kind, host);
            self.next_command(owner, command, host);
            // "ents need to update upon being affected"
            if self.sequencers.contains_key(&target) {
                self.update(target, host);
            }
            return;
        }
        if id == ID_BLOCK_END && self.current_has(owner, SQ_AFFECT) {
            let Some(block) = command.take() else { return };
            self.retain_or_drop(owner, block);
            let back = self
                .current(owner)
                .and_then(|current| self.return_sequence(current));
            self.set_current(owner, back);
            if back.is_none() {
                return;
            }
            self.next_command(owner, command, host);
        }
    }

    /// `CheckDo`: a task's body entered as a task group, or left at its end.
    fn check_do<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        command: &mut Option<Block>,
        host: &mut H,
    ) {
        let Some(id) = command.as_ref().map(|block| block.id) else {
            return;
        };
        if id == ID_DO {
            let Some(block) = command.take() else { return };
            let name = block.str_at(0).into_owned();
            let group = self.task_group_named(owner, &name, host);
            let sequence = group.and_then(|group| {
                self.sequencers
                    .get(&owner)?
                    .task_sequences
                    .get(&group)
                    .copied()
            });
            let Some(group) = group else {
                print::debug(
                    host,
                    DebugLevel::Error,
                    &format!("ICARUS Unable to find task group \"{name}\"!\n"),
                );
                return;
            };
            let Some(sequence) = sequence else {
                print::debug(
                    host,
                    DebugLevel::Error,
                    "ICARUS Unable to find task 'group' sequence!\n",
                );
                return;
            };
            self.retain_or_drop(owner, block);
            let current = self.current(owner);
            self.sequences.set_return(sequence, current);
            self.set_current(owner, Some(sequence));
            let Some(sequencer) = self.sequencers.get_mut(&owner) else {
                return;
            };
            sequencer.tasks.groups[group].parent = sequencer.current_group;
            sequencer.current_group = Some(group);
            // `MarkTask(group, TASK_START)`: "Mark all the following commands as being in the task"
            sequencer.tasks.start_group(group);
            self.next_command(owner, command, host);
            return;
        }
        if id == ID_BLOCK_END && self.current_has(owner, SQ_TASK) {
            let Some(block) = command.take() else { return };
            self.retain_or_drop(owner, block);
            // `MarkTask(m_curGroup, TASK_END)`
            let group = self
                .sequencers
                .get(&owner)
                .and_then(|sequencer| sequencer.current_group);
            if let Some(group) = group {
                self.mark_task_end(owner, group, host);
            }
            if let Some(sequencer) = self.sequencers.get_mut(&owner) {
                sequencer.current_group = group
                    .and_then(|group| sequencer.tasks.groups.get(group))
                    .and_then(|group| group.parent);
            }
            let Some(current) = self.current(owner) else {
                return;
            };
            let back = self.return_sequence(current);
            self.sequences.set_return(current, None);
            self.set_current(owner, back);
            if back.is_none() {
                return;
            }
            self.next_command(owner, command, host);
        }
    }

    /// `CTaskManager::GetTaskGroup(name)`, with its warning.
    pub(crate) fn task_group_named<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        name: &str,
        host: &mut H,
    ) -> Option<usize> {
        let group = self
            .sequencers
            .get(&owner)?
            .tasks
            .group_names
            .get(name)
            .copied();
        if group.is_none() {
            print::debug(
                host,
                DebugLevel::Warning,
                &format!("Could not find task group \"{name}\"\n"),
            );
        }
        group
    }

    /// `MarkTask(id, TASK_END)`: the group is looked up by its id (with the reference's
    /// warning if it is gone), then the task manager's current group's parent becomes
    /// current.
    fn mark_task_end<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, group: usize, host: &mut H) {
        let Some(sequencer) = self.sequencers.get_mut(&owner) else {
            return;
        };
        let Some(id) = sequencer.tasks.groups.get(group).map(|group| group.id) else {
            return;
        };
        if sequencer.tasks.group_by_id(id).is_none() {
            print::debug(
                host,
                DebugLevel::Warning,
                &format!("Could not find task group \"{id}\"\n"),
            );
            return;
        }
        sequencer.tasks.end_group();
    }

    /// `CSequencer::Affect`, on the affected entity: its sequence entered, flushing what
    /// it ran before or returning to it afterwards.
    fn affect<H: IcarusHost<O> + ?Sized>(
        &mut self,
        target: O,
        sequence: i32,
        kind: i32,
        host: &mut H,
    ) {
        let Some(sequence) = self.own_sequence(target, sequence) else {
            return;
        };
        match kind {
            TYPE_FLUSH => {
                self.flush(target, Some(sequence));
                self.sequences.remove_flag(sequence, SQ_PENDING, true);
                self.set_current(target, Some(sequence));
                let command = self.pop_command(target, true);
                self.prime(target, command, host);
            }
            TYPE_INSERT => {
                self.recall(target);
                let current = self.current(target);
                self.sequences.set_return(sequence, current);
                self.sequences.remove_flag(sequence, SQ_PENDING, true);
                self.set_current(target, Some(sequence));
                let command = self.pop_command(target, true);
                self.prime(target, command, host);
            }
            _ => print::debug(host, DebugLevel::Error, "unknown affect type found"),
        }
    }

    /// `CSequencer::Callback`: a command finished; put back if retained, and the next one
    /// handed to the task manager, at the front.
    pub(crate) fn callback<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: Block,
        host: &mut H,
    ) {
        let Some(current) = self.current(owner) else {
            return;
        };
        if self.sequences.has_flag(current, SQ_RETAIN) {
            self.push_command(owner, block, true);
        }
        if self.sequences.command_count(current) == 0 {
            let Some(back) = self.sequences.return_of(current) else {
                return;
            };
            self.set_current(owner, Some(back));
        }
        let mut command = self.pop_command(owner, true);
        self.prep(owner, &mut command, host);
        if let (Some(command), Some(sequencer)) = (command, self.sequencers.get_mut(&owner)) {
            sequencer.tasks.set_command(command, true);
        }
    }
}
