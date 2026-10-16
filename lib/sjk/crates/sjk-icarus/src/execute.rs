//! Carrying out commands (`CTaskManager::Update`, `Go` and the command functions of
//! `TaskManager.cpp`), and the engine's side of each game call (`Q3_Interface.cpp`).
//!
//! An update runs the entity's commands one after another until one has to wait — a
//! `wait` whose time has not come, a `waitsignal` not yet raised — or 256 have run
//! ("Runaway loop detected!"). A command that the game finishes later (a move, a
//! rotation, a `set` the game does not complete at once) does not hold the script up;
//! only a `wait` on its task group does.

use std::borrow::Cow;

use crate::Icarus;
use crate::block::Block;
use crate::cnum::{float_to_int, format_f};
use crate::host::{DebugLevel, IcarusHost, Owner};
use crate::ids::*;
use crate::print;
use crate::tasks::{RUNAWAY_LIMIT, Task};

/// What `CTaskManager::Get` hands back: a pointer into the block, into its static
/// buffer, or into the shared buffer, read when used.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Text {
    Member(usize),
    Temp,
    Shared,
}

impl<O: Owner> Icarus<O> {
    /// `CTaskManager::Update`: nothing while the entity is frozen.
    pub(crate) fn update<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, host: &mut H) {
        if host.frozen(owner) {
            return;
        }
        let Some(sequencer) = self.sequencers.get_mut(&owner) else {
            return;
        };
        sequencer.tasks.count = 0;
        self.go(owner, host);
    }

    /// `Go`, its recursion through `CallbackCommand` unrolled into a loop.
    fn go<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, host: &mut H) {
        let Some(serial) = self.serial(owner) else {
            return;
        };
        loop {
            let Some(sequencer) = self
                .sequencers
                .get_mut(&owner)
                .filter(|sequencer| sequencer.serial == serial)
            else {
                return;
            };
            let count = sequencer.tasks.count;
            sequencer.tasks.count += 1;
            if count > RUNAWAY_LIMIT {
                print::debug(host, DebugLevel::Error, "Runaway loop detected!\n");
                return;
            }
            let Some(mut task) = sequencer.tasks.tasks.pop_back() else {
                return;
            };
            if task.time_stamp == 0 {
                task.time_stamp = host.time();
            }
            match task.block.id {
                ID_WAIT | ID_WAITSIGNAL => {
                    let done = if task.block.id == ID_WAIT {
                        self.wait(owner, &mut task, host)
                    } else {
                        self.wait_signal(owner, &task, host)
                    };
                    let Some(sequencer) = self
                        .sequencers
                        .get_mut(&owner)
                        .filter(|sequencer| sequencer.serial == serial)
                    else {
                        return;
                    };
                    if !done {
                        sequencer.tasks.tasks.push_back(task);
                        return;
                    }
                    sequencer.tasks.completed(task.id);
                }
                ID_PRINT => self.print_command(owner, &task, host),
                ID_SOUND => self.sound(owner, &task, host),
                ID_MOVE => self.move_command(owner, &task, host),
                ID_ROTATE => self.rotate(owner, &task, host),
                ID_KILL => self.kill(owner, &task, host),
                ID_REMOVE => self.remove(owner, &task, host),
                ID_CAMERA => self.camera(owner, &task, host),
                ID_SET => self.set(owner, &task, host),
                ID_USE => self.use_command(owner, &task, host),
                ID_DECLARE => self.declare(owner, &task, host),
                ID_FREE => self.free_variable(owner, &task, host),
                ID_SIGNAL => self.signal_command(owner, &task, host),
                ID_PLAY => self.play(owner, &task, host),
                _ => {
                    print::debug(host, DebugLevel::Error, "Found unknown task type!\n");
                    return;
                }
            }
            // `CallbackCommand`: the sequencer is pumped for another task, then `Go` again.
            if self.serial(owner) != Some(serial) {
                return;
            }
            self.callback(owner, task.block, host);
        }
    }

    /// The text a [`Text`] points at now.
    pub(crate) fn text<'a>(&'a self, block: &'a Block, text: Text) -> Cow<'a, str> {
        match text {
            Text::Member(index) => block.str_at(index),
            Text::Temp => Cow::Borrowed(&self.temp),
            Text::Shared => self.shared.text(),
        }
    }

    fn check(block: &Block, member: usize, id: i32) -> bool {
        block.member_id(member) == Some(id)
    }

    /// `CTaskManager::GetFloat`.
    fn get_float<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        host: &mut H,
    ) -> Option<f32> {
        if Self::check(block, *member, ID_GET) {
            let kind = block.f32_at(*member + 1) as i32;
            let name = block.str_at(*member + 2);
            *member += 3;
            if kind != TK_FLOAT {
                print::debug(
                    host,
                    DebugLevel::Error,
                    "Get() call tried to return a non-FLOAT parameter!\n",
                );
                return None;
            }
            return self.ask_float(owner, kind, &name, host);
        }
        if Self::check(block, *member, ID_RANDOM) {
            let (min, max) = (block.f32_at(*member + 1), block.f32_at(*member + 2));
            *member += 3;
            return Some(host.random(min, max));
        }
        if Self::check(block, *member, ID_TAG) {
            print::debug(
                host,
                DebugLevel::Warning,
                "Invalid use of \"tag\" inline.  Not a valid replacement for type FLOAT\n",
            );
            return None;
        }
        match block.member(*member).map(|found| found.id) {
            Some(TK_INT) => {
                let value = block.i32_at(*member) as f32;
                *member += 1;
                Some(value)
            }
            Some(TK_FLOAT) => {
                let value = block.f32_at(*member);
                *member += 1;
                Some(value)
            }
            _ => {
                print::debug(
                    host,
                    DebugLevel::Warning,
                    "Unexpected value; expected type FLOAT\n",
                );
                None
            }
        }
    }

    /// `CTaskManager::GetVector`. A tag that is not found answers "yes" with the
    /// vector as it was: the reference returns `TASK_FAILED`, which is 1, there.
    fn get_vector<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        value: &mut [f32; 3],
        host: &mut H,
    ) -> bool {
        if Self::check(block, *member, ID_GET) {
            let kind = block.f32_at(*member + 1) as i32;
            let name = block.str_at(*member + 2);
            *member += 3;
            if kind != TK_VECTOR {
                print::debug(
                    host,
                    DebugLevel::Error,
                    "Get() call tried to return a non-VECTOR parameter!\n",
                );
            }
            return match self.ask_vector(owner, kind, &name, *value, host) {
                Some(answer) => {
                    *value = answer;
                    true
                }
                None => false,
            };
        }
        if Self::check(block, *member, ID_RANDOM) {
            let (min, max) = (block.f32_at(*member + 1), block.f32_at(*member + 2));
            *member += 3;
            for slot in value.iter_mut() {
                *slot = host.random(min, max);
            }
            return true;
        }
        if Self::check(block, *member, ID_TAG) {
            *member += 1;
            let Some(name) = self.get(owner, block, member, host) else {
                return true;
            };
            let name = self.text(block, name).into_owned();
            let Some(lookup) = self.get_float(owner, block, member, host) else {
                return true;
            };
            if !self.ask_tag(owner, &name, lookup as i32, value, host) {
                print::debug(
                    host,
                    DebugLevel::Error,
                    &format!("Unable to find tag \"{name}\" for ent {owner}!\n"),
                );
            }
            return true;
        }
        if block.f32_at(*member) as i32 != TK_VECTOR {
            return false;
        }
        *member += 1;
        for index in 0..3 {
            match self.get_float(owner, block, member, host) {
                Some(component) => value[index] = component,
                None => return false,
            }
        }
        true
    }

    /// `CTaskManager::Get`: a member as text.
    pub(crate) fn get<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        host: &mut H,
    ) -> Option<Text> {
        if Self::check(block, *member, ID_GET) {
            let kind = block.f32_at(*member + 1) as i32;
            let name = block.str_at(*member + 2);
            *member += 3;
            let not_found = |host: &mut H, name: &str| {
                print::debug(
                    host,
                    DebugLevel::Error,
                    &format!("Get() parameter \"{name}\" could not be found!\n"),
                );
                None
            };
            return match kind {
                TK_STRING => {
                    if self.ask_string(owner, kind, &name, host) {
                        Some(Text::Shared)
                    } else {
                        not_found(host, &name)
                    }
                }
                TK_FLOAT => match self.ask_float(owner, kind, &name, host) {
                    Some(value) => {
                        self.temp = format_f(value);
                        Some(Text::Temp)
                    }
                    None => not_found(host, &name),
                },
                TK_VECTOR => match self.ask_vector(owner, kind, &name, [0.0; 3], host) {
                    Some(value) => {
                        self.temp = format!(
                            "{} {} {}",
                            format_f(value[0]),
                            format_f(value[1]),
                            format_f(value[2])
                        );
                        Some(Text::Temp)
                    }
                    None => not_found(host, &name),
                },
                _ => {
                    print::debug(
                        host,
                        DebugLevel::Error,
                        "Get() call tried to return an unknown type!\n",
                    );
                    None
                }
            };
        }
        if Self::check(block, *member, ID_RANDOM) {
            let (min, max) = (block.f32_at(*member + 1), block.f32_at(*member + 2));
            *member += 3;
            self.temp = format_f(host.random(min, max));
            return Some(Text::Temp);
        }
        if Self::check(block, *member, ID_TAG) {
            *member += 1;
            let Some(name) = self.get(owner, block, member, host) else {
                return Some(Text::Temp);
            };
            let name = self.text(block, name).into_owned();
            let Some(lookup) = self.get_float(owner, block, member, host) else {
                return Some(Text::Temp);
            };
            let mut vector = [0.0; 3];
            if !self.ask_tag(owner, &name, lookup as i32, &mut vector, host) {
                print::debug(
                    host,
                    DebugLevel::Error,
                    &format!("Unable to find tag \"{name}\"!\n"),
                );
                return None;
            }
            self.temp = format!(
                "{} {} {}",
                format_f(vector[0]),
                format_f(vector[1]),
                format_f(vector[2])
            );
            return Some(Text::Temp);
        }
        match block.member(*member).map(|found| found.id) {
            Some(TK_INT) => {
                self.temp = format_f(block.i32_at(*member) as f32);
                *member += 1;
                Some(Text::Temp)
            }
            Some(TK_FLOAT) => {
                self.temp = format_f(block.f32_at(*member));
                *member += 1;
                Some(Text::Temp)
            }
            Some(TK_VECTOR) => {
                *member += 1;
                let mut vector = [0.0; 3];
                for slot in &mut vector {
                    *slot = self.get_float(owner, block, member, host)?;
                }
                self.temp = format!(
                    "{} {} {}",
                    format_f(vector[0]),
                    format_f(vector[1]),
                    format_f(vector[2])
                );
                Some(Text::Temp)
            }
            Some(TK_STRING | TK_IDENTIFIER) => {
                *member += 1;
                Some(Text::Member(*member - 1))
            }
            _ => {
                print::debug(
                    host,
                    DebugLevel::Warning,
                    "Unexpected value; expected type STRING\n",
                );
                None
            }
        }
    }

    /// Two members as text, read in order (`Get` twice), each resolved only after both
    /// were read, as the reference's pointers are.
    fn get_two<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        host: &mut H,
    ) -> Option<(String, String)> {
        let mut member = 0;
        let first = self.get(owner, block, &mut member, host)?;
        let second = self.get(owner, block, &mut member, host)?;
        Some((
            self.text(block, first).into_owned(),
            self.text(block, second).into_owned(),
        ))
    }

    fn get_one<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        host: &mut H,
    ) -> Option<String> {
        let mut member = 0;
        let text = self.get(owner, block, &mut member, host)?;
        Some(self.text(block, text).into_owned())
    }

    fn complete(&mut self, owner: O, task: i32) {
        if let Some(sequencer) = self.sequencers.get_mut(&owner) {
            sequencer.tasks.completed(task);
        }
    }

    /// `Wait`: for a task group to finish, or for time to pass. A random time is drawn
    /// once and kept in the block until the wait is over.
    fn wait<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &mut Task, host: &mut H) -> bool {
        let now = host.time();
        if task.block.member_id(0) == Some(TK_STRING) {
            let mut member = 0;
            let Some(name) = self.get(owner, &task.block, &mut member, host) else {
                return false;
            };
            let name = self.text(&task.block, name).into_owned();
            if task.time_stamp == now {
                print::command(
                    host,
                    owner,
                    &format!("wait(\"{name}\"); [{}]", task.time_stamp),
                );
            }
            let Some(group) = self.task_group_named(owner, &name, host) else {
                return false;
            };
            return self
                .sequencers
                .get(&owner)
                .is_some_and(|sequencer| sequencer.tasks.groups[group].complete());
        }
        let mut member = 0;
        let time = if Self::check(&task.block, member, ID_RANDOM) {
            let mut time = task.block.f32_at(member);
            member += 1;
            if time == INFINITE {
                let (min, max) = (task.block.f32_at(member), task.block.f32_at(member + 1));
                time = host.random(min, max);
                task.block.members[0].data = time.to_le_bytes().to_vec();
            }
            time
        } else {
            let block = std::mem::replace(&mut task.block, Block::new(0));
            let time = self.get_float(owner, &block, &mut member, host);
            task.block = block;
            let Some(time) = time else { return false };
            time
        };
        if task.time_stamp == now {
            print::command(
                host,
                owner,
                &format!("wait( {} ); [{}]", float_to_int(time), task.time_stamp),
            );
        }
        if (task.time_stamp as f32 + time) < host.time() as f32 {
            if Self::check(&task.block, 0, ID_RANDOM) {
                // "set the data back to 0 so it will be re-randomized next time"
                task.block.members[0].data = INFINITE.to_le_bytes().to_vec();
            }
            return true;
        }
        false
    }

    /// `WaitSignal`: done once the signal is raised, which it lowers.
    fn wait_signal<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        task: &Task,
        host: &mut H,
    ) -> bool {
        let Some(name) = self.get_one(owner, &task.block, host) else {
            return false;
        };
        if task.time_stamp == host.time() {
            print::command(
                host,
                owner,
                &format!("waitsignal(\"{name}\"); [{}]", task.time_stamp),
            );
        }
        if self.check_signal(&name) {
            self.clear_signal(&name);
            return true;
        }
        false
    }

    fn print_command<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some(text) = self.get_one(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("print(\"{text}\"); [{}]", task.time_stamp),
        );
        print::center(host, &text);
        self.complete(owner, task.id);
    }

    fn sound<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some((channel, name)) = self.get_two(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("sound(\"{channel}\", \"{name}\"); [{}]", task.time_stamp),
        );
        // `Q3_PlaySound`: "Only instantly complete if the user has requested it"
        self.shared.write_str(&channel);
        if host.play_sound(self, task.id, owner, &name, &channel) {
            self.complete(owner, task.id);
        }
    }

    fn rotate<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let block = &task.block;
        let mut member = 0;
        let mut angles = [0.0; 3];
        if Self::check(block, member, ID_TAG) {
            member += 1;
            let Some(name) = self.get(owner, block, &mut member, host) else {
                return;
            };
            let name = self.text(block, name).into_owned();
            let Some(lookup) = self.get_float(owner, block, &mut member, host) else {
                return;
            };
            if !self.ask_tag(owner, &name, lookup as i32, &mut angles, host) {
                print::debug(
                    host,
                    DebugLevel::Error,
                    &format!("Unable to find tag \"{name}\"!\n"),
                );
                return;
            }
        } else if !self.get_vector(owner, block, &mut member, &mut angles, host) {
            return;
        }
        let Some(duration) = self.get_float(owner, block, &mut member, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!(
                "rotate( <{},{},{}>, {}); [{}]",
                format_f(angles[0]),
                format_f(angles[1]),
                format_f(angles[2]),
                float_to_int(duration),
                task.time_stamp
            ),
        );
        host.lerp_to_angles(self, task.id, owner, &mut angles, duration);
    }

    fn remove<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some(name) = self.get_one(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("remove(\"{name}\"); [{}]", task.time_stamp),
        );
        host.remove(self, owner, &name);
        self.complete(owner, task.id);
    }

    fn camera<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        crate::camera::run(self, owner, task, host);
    }

    pub(crate) fn camera_float<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        host: &mut H,
    ) -> Option<f32> {
        self.get_float(owner, block, member, host)
    }

    pub(crate) fn camera_vector<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        host: &mut H,
    ) -> Option<[f32; 3]> {
        let mut value = [0.0; 3];
        self.get_vector(owner, block, member, &mut value, host)
            .then_some(value)
    }

    pub(crate) fn camera_text<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        block: &Block,
        member: &mut usize,
        host: &mut H,
    ) -> Option<String> {
        let text = self.get(owner, block, member, host)?;
        Some(self.text(block, text).into_owned())
    }

    pub(crate) fn complete_task(&mut self, owner: O, task: i32) {
        self.complete(owner, task);
    }

    fn move_command<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let block = &task.block;
        let mut member = 0;
        let mut origin = [0.0; 3];
        if !self.get_vector(owner, block, &mut member, &mut origin, host) {
            return;
        }
        let mut angles = [0.0; 3];
        if !self.get_vector(owner, block, &mut member, &mut angles, host) {
            let Some(duration) = self.get_float(owner, block, &mut member, host) else {
                return;
            };
            print::command(
                host,
                owner,
                &format!(
                    "move( <{} {} {}>, {} ); [{}]",
                    format_f(origin[0]),
                    format_f(origin[1]),
                    format_f(origin[2]),
                    format_f(duration),
                    task.time_stamp
                ),
            );
            host.lerp_to_position(self, task.id, owner, &mut origin, None, duration);
            return;
        }
        let Some(duration) = self.get_float(owner, block, &mut member, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!(
                "move( <{} {} {}>, <{} {} {}>, {} ); [{}]",
                format_f(origin[0]),
                format_f(origin[1]),
                format_f(origin[2]),
                format_f(angles[0]),
                format_f(angles[1]),
                format_f(angles[2]),
                format_f(duration),
                task.time_stamp
            ),
        );
        host.lerp_to_position(
            self,
            task.id,
            owner,
            &mut origin,
            Some(&mut angles),
            duration,
        );
    }

    fn kill<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some(name) = self.get_one(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("kill( \"{name}\" ); [{}]", task.time_stamp),
        );
        host.kill(self, owner, &name);
        self.complete(owner, task.id);
    }

    fn set<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some((name, value)) = self.get_two(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("set( \"{name}\", \"{value}\" ); [{}]", task.time_stamp),
        );
        // `Q3_Set`: complete at once if the game says so.
        self.shared.write_str(&value);
        if host.set(self, task.id, owner, &name, &value) {
            self.complete(owner, task.id);
        }
    }

    fn use_command<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some(name) = self.get_one(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("use( \"{name}\" ); [{}]", task.time_stamp),
        );
        host.use_target(self, owner, &name);
        self.complete(owner, task.id);
    }

    fn declare<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let block = &task.block;
        let mut member = 0;
        let Some(kind) = self.get_float(owner, block, &mut member, host) else {
            return;
        };
        let Some(name) = self.get(owner, block, &mut member, host) else {
            return;
        };
        let name = self.text(block, name).into_owned();
        let kind = float_to_int(kind);
        print::command(
            host,
            owner,
            &format!("declare( {kind}, \"{name}\" ); [{}]", task.time_stamp),
        );
        match self.variables.declare(kind, &name) {
            Ok(()) => {}
            Err(crate::variables::DeclareError::TooMany) => {
                let limit = self.variables.limit();
                print::debug(
                    host,
                    DebugLevel::Error,
                    &format!("too many variables already declared, maximum is {limit}\n"),
                );
            }
            Err(crate::variables::DeclareError::UnknownType) => {
                print::debug(
                    host,
                    DebugLevel::Error,
                    "unknown 'type' for declare() function!\n",
                );
            }
        }
        self.complete(owner, task.id);
    }

    fn free_variable<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some(name) = self.get_one(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("free( \"{name}\" ); [{}]", task.time_stamp),
        );
        self.variables.free(&name);
        self.complete(owner, task.id);
    }

    fn signal_command<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some(name) = self.get_one(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("signal( \"{name}\" ); [{}]", task.time_stamp),
        );
        self.signal(&name);
        self.complete(owner, task.id);
    }

    fn play<H: IcarusHost<O> + ?Sized>(&mut self, owner: O, task: &Task, host: &mut H) {
        let Some((kind, name)) = self.get_two(owner, &task.block, host) else {
            return;
        };
        print::command(
            host,
            owner,
            &format!("play( \"{kind}\", \"{name}\" ); [{}]", task.time_stamp),
        );
        self.shared.write_str(&name);
        host.play(self, task.id, owner, &kind, &name);
    }

    // ---- the engine's side of the game calls (`Q3_Interface.cpp`) ----

    /// `Q3_GetFloat`: the shared value zeroed, then the game asked.
    pub(crate) fn ask_float<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        kind: i32,
        name: &str,
        host: &mut H,
    ) -> Option<f32> {
        self.shared.write_floats(&[0.0]);
        let answer = host.get_float(self, owner, kind, name);
        if let Some(value) = answer {
            self.shared.write_floats(&[value]);
        }
        answer
    }

    /// `Q3_GetVector`: the caller's vector copied in, then the game asked.
    pub(crate) fn ask_vector<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        kind: i32,
        name: &str,
        input: [f32; 3],
        host: &mut H,
    ) -> Option<[f32; 3]> {
        self.shared.write_floats(&input);
        let answer = host.get_vector(self, owner, kind, name);
        if let Some(value) = answer {
            self.shared.write_floats(&value);
        }
        answer
    }

    /// `Q3_GetString`: the game's answer copied into the shared buffer (unless it gave
    /// none); true if the name was found.
    pub(crate) fn ask_string<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        kind: i32,
        name: &str,
        host: &mut H,
    ) -> bool {
        let answer = host.get_string(self, owner, kind, name);
        if let Some(value) = &answer.value {
            self.shared.write_str(value);
        }
        answer.found
    }

    /// `Q3_GetTag`: the vector passes through the shared buffer both ways.
    pub(crate) fn ask_tag<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        name: &str,
        lookup: i32,
        info: &mut [f32; 3],
        host: &mut H,
    ) -> bool {
        self.shared.write_floats(info);
        let found = host.tag(self, owner, name, lookup, info);
        self.shared.write_floats(info);
        found
    }
}
