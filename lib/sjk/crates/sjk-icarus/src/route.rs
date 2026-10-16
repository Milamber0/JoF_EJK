//! Routing a script's blocks into sequences (`CSequencer::Run`, `Route` and the
//! `Parse*` pre-processors of `Sequencer.cpp`).
//!
//! When a script runs, every block of its file is read at once and placed: plain
//! commands into the current sequence, the bodies of `loop`, `if`, `else` and `task` into
//! child sequences, an `affect` body into a sequence of the *affected* entity, a `run`
//! script's blocks (read from its own file) into a sequence of their own. Blocks go to
//! the front of their sequence, so a sequence runs from its back. When everything is
//! placed the first command is handed to the task manager ("Prime").

use std::sync::Arc;

use crate::block::Block;
use crate::host::{DebugLevel, IcarusHost, Owner};
use crate::ids::*;
use crate::print;
use crate::sequence::{SQ_AFFECT, SQ_CONDITIONAL, SQ_LOOP, SQ_PENDING, SQ_RETAIN, SQ_RUN, SQ_TASK};
use crate::sequencer::{ParseStream, StreamId, StreamSource};
use crate::stream::BlockStream;
use crate::{Icarus, strip_extension};

impl<O: Owner> Icarus<O> {
    /// `CSequencer::Run`: the script's blocks routed into a new sequence that returns to
    /// the one that was current. False if the file is no block file.
    pub(crate) fn run_buffer<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        buffer: Arc<[u8]>,
        host: &mut H,
    ) -> bool {
        self.recall(owner);
        let Some(stream) = self.add_stream(owner, BlockStream::open(buffer)) else {
            print::debug(host, DebugLevel::Error, "invalid stream");
            return false;
        };
        let current = self.current(owner);
        let Some(sequence) = self.add_sequence_under(owner, None, current, 0) else {
            return false;
        };
        self.route(owner, sequence, stream, host)
    }

    /// `AddStream`: a parse stream over `blocks`, after the current one. `None` (with the
    /// stream kept, as the reference keeps it) if the file could not be opened.
    fn add_stream(&mut self, owner: O, blocks: Option<BlockStream>) -> Option<StreamId> {
        let sequencer = self.sequencers.get_mut(&owner)?;
        let opened = blocks.is_some();
        let source = StreamSource::Own(blocks.unwrap_or_else(BlockStream::empty));
        let id = self.streams.add(ParseStream {
            source,
            last: sequencer.current_stream,
        });
        sequencer.streams_created.push(id);
        opened.then_some(id)
    }

    /// `DeleteStream`.
    fn delete_stream(&mut self, owner: O, stream: StreamId) {
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            if let Some(index) = sequencer
                .streams_created
                .iter()
                .position(|&created| created == stream)
            {
                sequencer.streams_created.remove(index);
            }
        }
        self.streams.remove(stream);
    }

    /// `AddSequence()`: a new sequence of this entity, pending.
    fn add_pending_sequence(&mut self, owner: O) -> Option<i32> {
        let sequencer = self.sequencers.get_mut(&owner)?;
        let id = self.sequences.create();
        sequencer.sequences.push(id);
        if let Some(sequence) = self.sequences.get_mut(id) {
            sequence.flags |= SQ_PENDING;
        }
        Some(id)
    }

    /// `AddSequence(parent, returnSeq, flags)`.
    pub(crate) fn add_sequence_under(
        &mut self,
        owner: O,
        parent: Option<i32>,
        return_to: Option<i32>,
        flags: u32,
    ) -> Option<i32> {
        let sequencer = self.sequencers.get_mut(&owner)?;
        let id = self.sequences.create();
        sequencer.sequences.push(id);
        if let Some(sequence) = self.sequences.get_mut(id) {
            sequence.flags = flags;
        }
        self.sequences.set_parent(id, parent);
        self.sequences.set_return(id, return_to);
        Some(id)
    }

    /// `CSequencer::GetSequence(id)`: one of this entity's sequences.
    pub(crate) fn own_sequence(&self, owner: O, id: i32) -> Option<i32> {
        self.sequencers
            .get(&owner)?
            .sequences
            .contains(&id)
            .then_some(id)
    }

    /// `Route`: blocks from the stream into `sequence` until the end of its body (a
    /// block end) or of the stream. At the end of the outermost stream the first command
    /// goes to the task manager.
    pub(crate) fn route<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        sequence: i32,
        stream: StreamId,
        host: &mut H,
    ) -> bool {
        {
            let Some(sequencer) = self.sequencers.get_mut(&owner) else {
                return false;
            };
            sequencer.current_stream = Some(stream);
            sequencer.current = Some(sequence);
        }
        while self
            .streams
            .blocks(stream)
            .is_some_and(|blocks| blocks.block_available())
        {
            let Some(block) = self.streams.blocks(stream).map(BlockStream::read_block) else {
                break;
            };
            let Some(sequencer) = self.sequencers.get_mut(&owner) else {
                return false;
            };
            if sequencer.else_valid != 0 {
                sequencer.else_valid -= 1;
            }
            match block.id {
                ID_BLOCK_END => {
                    self.push_command(owner, block, true);
                    let last = self.streams.last(stream);
                    if self.current_has(owner, SQ_RUN) || self.current_has(owner, SQ_AFFECT) {
                        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
                            sequencer.current_stream = last;
                        }
                    }
                    if self.current_has(owner, SQ_TASK) {
                        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
                            sequencer.current_stream = last;
                            sequencer.current_group = sequencer
                                .current_group
                                .and_then(|group| sequencer.tasks.groups.get(group))
                                .and_then(|group| group.parent);
                        }
                    }
                    let back = self
                        .current(owner)
                        .and_then(|current| self.sequences.return_of(current));
                    self.set_current(owner, back);
                    return true;
                }
                ID_AFFECT => {
                    if !self.parse_affect(owner, block, stream, host) {
                        return false;
                    }
                }
                ID_RUN => {
                    if !self.parse_run(owner, block, host) {
                        return false;
                    }
                }
                ID_LOOP => self.parse_loop(owner, block, stream, host),
                ID_IF => self.parse_if(owner, block, stream, host),
                ID_ELSE => {
                    if self
                        .sequencers
                        .get(&owner)
                        .is_some_and(|sequencer| sequencer.else_valid == 0)
                    {
                        print::debug(host, DebugLevel::Error, "Invalid 'else' found!\n");
                        return false;
                    }
                    if !self.parse_else(owner, stream, host) {
                        return false;
                    }
                }
                ID_TASK => self.parse_task(owner, block, stream, host),
                ID_WAIT | ID_PRINT | ID_SOUND | ID_MOVE | ID_ROTATE | ID_SET | ID_USE
                | ID_REMOVE | ID_KILL | ID_FLUSH | ID_CAMERA | ID_DO | ID_DECLARE | ID_FREE
                | ID_SIGNAL | ID_WAITSIGNAL | ID_PLAY => self.push_command(owner, block, true),
                id => {
                    print::debug(
                        host,
                        DebugLevel::Error,
                        &format!("'{id}' : invalid block ID"),
                    );
                    return false;
                }
            }
        }
        // "Check for a run sequence, it must be marked"
        if self.current_has(owner, SQ_RUN) {
            self.push_command(owner, Block::new(ID_BLOCK_END), true);
            return true;
        }
        let last = self.streams.last(stream);
        let pending = self
            .sequencers
            .get(&owner)
            .is_some_and(|sequencer| sequencer.command_count > 0);
        if last.is_none() && pending {
            // "Everything is routed, so get it all rolling"
            let command = self.pop_command(owner, true);
            self.prime(owner, command, host);
        }
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.current_stream = last;
        }
        self.delete_stream(owner, stream);
        true
    }

    /// `ParseRun`: the named script's blocks routed into a sequence of their own, and
    /// the `run` block left in their place naming it.
    fn parse_run<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        mut block: Block,
        host: &mut H,
    ) -> bool {
        let named = block.str_at(0).into_owned();
        let name = strip_extension(&named, 256);
        let Some(buffer) = self.get_script(&format!("scripts/{name}"), host) else {
            print::debug(
                host,
                DebugLevel::Error,
                &format!("'{named}' : could not open file\n"),
            );
            return false;
        };
        let Some(stream) = self.add_stream(owner, BlockStream::open(buffer)) else {
            print::debug(host, DebugLevel::Error, "invalid stream");
            return false;
        };
        let current = self.current(owner);
        let Some(sequence) = self.add_sequence_under(owner, current, current, SQ_RUN | SQ_PENDING)
        else {
            return false;
        };
        if let Some(current) = current {
            self.sequences.add_child(current, sequence);
        }
        if !self.route(owner, sequence, stream, host) {
            return false;
        }
        let back = self
            .current(owner)
            .and_then(|current| self.sequences.return_of(current));
        self.set_current(owner, back);
        block.write_f32(TK_FLOAT, sequence as f32);
        self.push_command(owner, block, true);
        true
    }

    /// `ParseIf`: the body into a conditional sequence; the `if` block, naming it, waits
    /// at the front of the current sequence for an `else` to follow.
    fn parse_if<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        mut block: Block,
        stream: StreamId,
        host: &mut H,
    ) {
        let current = self.current(owner);
        let Some(sequence) = self.add_sequence_under(owner, current, current, SQ_CONDITIONAL)
        else {
            return;
        };
        if let Some(current) = current {
            self.sequences.add_child(current, sequence);
        }
        block.write_f32(TK_FLOAT, sequence as f32);
        self.push_command(owner, block, true);
        self.route(owner, sequence, stream, host);
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.else_valid = 2;
            sequencer.else_owner = current;
        }
    }

    /// `ParseElse`: the body into a conditional sequence named by the `if` it follows.
    fn parse_else<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        stream: StreamId,
        host: &mut H,
    ) -> bool {
        let current = self.current(owner);
        let Some(sequence) = self.add_sequence_under(owner, current, current, SQ_CONDITIONAL)
        else {
            return false;
        };
        if let Some(current) = current {
            self.sequences.add_child(current, sequence);
        }
        let else_owner = self
            .sequencers
            .get(&owner)
            .and_then(|sequencer| sequencer.else_owner);
        let Some(if_block) = else_owner
            .and_then(|holder| self.sequences.get_mut(holder))
            .and_then(|holder| holder.commands.front_mut())
        else {
            print::debug(host, DebugLevel::Error, "Invalid 'else' found!\n");
            return false;
        };
        if_block.write_f32(TK_FLOAT, sequence as f32);
        if_block.flags |= crate::block::BF_ELSE;
        self.route(owner, sequence, stream, host);
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.else_valid = 0;
            sequencer.else_owner = None;
        }
        true
    }

    /// `ParseLoop`: the body into a looping sequence, its count drawn now if random.
    fn parse_loop<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        mut block: Block,
        stream: StreamId,
        host: &mut H,
    ) {
        let current = self.current(owner);
        let Some(sequence) = self.add_sequence_under(owner, current, current, SQ_LOOP | SQ_RETAIN)
        else {
            return;
        };
        if let Some(current) = current {
            self.sequences.add_child(current, sequence);
        }
        let iterations = if block.member_id(0) == Some(ID_RANDOM) {
            host.random(block.f32_at(1), block.f32_at(2)) as i32
        } else {
            block.f32_at(0) as i32
        };
        if let Some(loop_sequence) = self.sequences.get_mut(sequence) {
            loop_sequence.iterations = iterations;
        }
        block.write_f32(TK_FLOAT, sequence as f32);
        self.push_command(owner, block, true);
        self.route(owner, sequence, stream, host);
    }

    /// `AddAffect`, on the affected entity: the body into a pending sequence of its own,
    /// read from the affecting script's stream. Returns the sequence's id.
    fn add_affect<H: IcarusHost<O> + ?Sized>(
        &mut self,
        target: O,
        stream: StreamId,
        retain: bool,
        host: &mut H,
    ) -> Option<i32> {
        let sequence = self.add_pending_sequence(target)?;
        if let Some(affect) = self.sequences.get_mut(sequence) {
            affect.flags |= SQ_AFFECT | SQ_PENDING | if retain { SQ_RETAIN } else { 0 };
        }
        let current = self.current(target);
        self.sequences.set_return(sequence, current);
        let last = self
            .sequencers
            .get(&target)
            .and_then(|sequencer| sequencer.current_stream);
        let temporary = self.streams.add(ParseStream {
            source: StreamSource::Shares(stream),
            last,
        });
        let routed = self.route(target, sequence, temporary, host);
        self.streams.remove(temporary);
        if !routed {
            return None;
        }
        self.sequences.set_return(sequence, None);
        Some(sequence)
    }

    /// `ParseAffect`: the body routed to the named entity; the `affect` block, naming the
    /// sequence it went to, stays here. A name that finds no scripted entity sends the
    /// body to a sequence that is thrown away.
    ///
    /// A failure to read a `get` target returns the reference's `false` — which is its
    /// `SEQ_OK`: the body is then routed here as ordinary commands.
    fn parse_affect<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        mut block: Block,
        stream: StreamId,
        host: &mut H,
    ) -> bool {
        let entity_name = block.str_at(0).into_owned();
        let mut entity = self.entity_by_name(&entity_name);
        if entity.is_none() {
            let name = match block.member_id(0) {
                Some(TK_STRING | TK_IDENTIFIER | TK_CHAR) => Some(entity_name.clone()),
                Some(ID_GET) => {
                    let kind = block.f32_at(1) as i32;
                    let get_name = block.str_at(2).into_owned();
                    if kind != TK_STRING {
                        print::debug(
                            host,
                            DebugLevel::Error,
                            "Invalid parameter type on affect _1",
                        );
                        return true;
                    }
                    if !self.ask_string(owner, kind, &get_name, host) {
                        return true;
                    }
                    Some(self.shared.text().into_owned())
                }
                _ => {
                    print::debug(
                        host,
                        DebugLevel::Error,
                        "Invalid parameter type on affect _2",
                    );
                    return true;
                }
            };
            entity = name.as_deref().and_then(|name| self.entity_by_name(name));
            if entity.is_none() {
                // The reference prints this with its argument missing.
                print::debug(
                    host,
                    DebugLevel::Warning,
                    &format!("'{}' : invalid affect() target\n", name.unwrap_or_default()),
                );
            }
        }
        let Some(target) = entity.filter(|&target| self.sequencers.contains_key(&target)) else {
            print::debug(
                host,
                DebugLevel::Warning,
                &format!("'{entity_name}' : invalid affect() target\n"),
            );
            // "Fast-forward out of this affect block onto the next valid code"
            let back = self.current(owner);
            let trash = self.sequences.create();
            self.route(owner, trash, stream, host);
            self.recall(owner);
            self.destroy_sequence(owner, trash);
            self.set_current(owner, back);
            return true;
        };
        let retain = self.current_has(owner, SQ_RETAIN);
        let Some(sequence) = self.add_affect(target, stream, retain, host) else {
            return false;
        };
        block.write_f32(TK_FLOAT, sequence as f32);
        self.push_command(owner, block, true);
        true
    }

    /// `ParseTask`: the body into a retained task sequence, filed under a task group of
    /// that name.
    fn parse_task<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: Block,
        stream: StreamId,
        host: &mut H,
    ) {
        let current = self.current(owner);
        let Some(sequence) = self.add_sequence_under(owner, current, current, SQ_TASK | SQ_RETAIN)
        else {
            return;
        };
        if let Some(current) = current {
            self.sequences.add_child(current, sequence);
        }
        let name = block.str_at(0).into_owned();
        let Some(sequencer) = self.sequencers.get_mut(&owner) else {
            return;
        };
        let group = sequencer.tasks.add_group(&name);
        sequencer.tasks.groups[group].parent = sequencer.current_group;
        sequencer.current_group = Some(group);
        sequencer.task_sequences.insert(group, sequence);
        self.route(owner, sequence, stream, host);
    }

    /// `DestroySequence`: a sequence and everything below it, gone.
    pub(crate) fn destroy_sequence(&mut self, owner: O, sequence: i32) {
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.sequences.retain(|&id| id != sequence);
            sequencer.task_sequences.retain(|_, &mut id| id != sequence);
        }
        if let Some(parent) = self.sequences.get(sequence).and_then(|found| found.parent) {
            if let Some(parent) = self.sequences.get_mut(parent) {
                parent.children.retain(|&child| child != sequence);
            }
        }
        while let Some(child) = self
            .sequences
            .get(sequence)
            .and_then(|found| found.children.last().copied())
        {
            self.destroy_sequence(owner, child);
        }
        self.sequences.delete(sequence);
    }

    /// `Flush`: every sequence of the entity but `keep`, those below it, and those pending
    /// or holding a task, freed; `keep` becomes a root.
    pub(crate) fn flush(&mut self, owner: O, keep: Option<i32>) -> bool {
        let Some(keep) = keep else { return false };
        self.recall(owner);
        let Some(list) = self
            .sequencers
            .get(&owner)
            .map(|sequencer| sequencer.sequences.clone())
        else {
            return false;
        };
        let mut kept = Vec::with_capacity(list.len());
        for id in list {
            let stays = id == keep
                || self.sequences.has_child(keep, id)
                || self.sequences.has_flag(id, SQ_PENDING)
                || self.sequences.has_flag(id, SQ_TASK);
            if stays {
                kept.push(id);
                continue;
            }
            // `RemoveSequence`: the children lose their parent and return.
            let children = self
                .sequences
                .get(id)
                .map(|found| found.children.clone())
                .unwrap_or_default();
            for child in children {
                self.sequences.set_parent(child, None);
                self.sequences.set_return(child, None);
            }
            self.sequences.delete(id);
        }
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.sequences = kept;
        }
        self.sequences.set_parent(keep, None);
        self.sequences.set_return(keep, None);
        true
    }

    /// `ReturnSequence`: the first sequence along the returns that still has commands.
    pub(crate) fn return_sequence(&self, sequence: i32) -> Option<i32> {
        let mut sequence = sequence;
        while let Some(next) = self.sequences.return_of(sequence) {
            if next == sequence {
                return None;
            }
            sequence = next;
            if self.sequences.command_count(sequence) > 0 {
                return Some(sequence);
            }
        }
        None
    }
}
