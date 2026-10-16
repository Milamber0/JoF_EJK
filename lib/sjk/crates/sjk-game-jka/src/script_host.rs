//! The interpreter's host over a [`ScriptWorld`]: the engine services and the game's
//! `GVM_ICARUS_*` exports in one ([`sjk_icarus::IcarusHost`]).

use sjk_icarus::{EntityNames, Icarus, IcarusHost, SetKind, StringAnswer};

use crate::icarus_set_table::{
    SET_CINEMATIC_SKIPSCRIPT, SET_LOOPSOUND, SET_MINDTRICKSCRIPT, SET_SPAWNSCRIPT, set_id,
};
use crate::script_entity::SVF_ICARUS_FREEZE;
use crate::script_world::ScriptWorld;
use crate::{script_calls, script_gets, script_set};

/// The host a world lends the interpreter for one call.
pub struct ScriptHost<'a, W: ScriptWorld> {
    world: &'a mut W,
}

impl<'a, W: ScriptWorld> ScriptHost<'a, W> {
    /// The host over `world`.
    pub fn new(world: &'a mut W) -> Self {
        Self { world }
    }
}

impl<W: ScriptWorld> IcarusHost<W::Id> for ScriptHost<'_, W> {
    fn time(&self) -> u32 {
        self.world.server_time()
    }

    fn read_file(&mut self, path: &str) -> Option<Vec<u8>> {
        self.world.read_file(path)
    }

    fn random(&mut self, min: f32, max: f32) -> f32 {
        self.world.engine_random(min, max)
    }

    fn frozen(&self, owner: W::Id) -> bool {
        self.world
            .entity(owner)
            .is_some_and(|ent| ent.svflags & SVF_ICARUS_FREEZE != 0)
    }

    fn developer(&self) -> bool {
        self.world.developer() != 0
    }

    fn print(&mut self, text: &str) {
        self.world.interpreter_print(text);
    }

    fn broadcast_command(&mut self, command: &str) {
        self.world.broadcast_command(command);
    }

    fn entity_names(&self, owner: W::Id) -> EntityNames {
        let Some(ent) = self.world.entity(owner) else {
            return EntityNames::default();
        };
        EntityNames {
            classname: Some(ent.classname.clone()),
            targetname: ent.targetname.clone(),
            script_targetname: ent.script_targetname.clone(),
        }
    }

    fn cache_roff(&mut self, file: &str) {
        self.world.cache_roff(file);
    }

    fn play_sound(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        task: i32,
        owner: W::Id,
        name: &str,
        channel: &str,
    ) -> bool {
        script_calls::play_sound(self.world, icarus, task, owner, name, channel)
    }

    fn set(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        task: i32,
        owner: W::Id,
        name: &str,
        value: &str,
    ) -> bool {
        script_set::set(self.world, icarus, task, owner, name, value)
    }

    fn lerp_to_position(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        task: i32,
        owner: W::Id,
        origin: &mut [f32; 3],
        angles: Option<&mut [f32; 3]>,
        duration: f32,
    ) {
        script_calls::lerp_to_position(
            self.world,
            icarus,
            task,
            owner,
            *origin,
            angles.map(|angles| *angles),
            duration,
        );
    }

    fn lerp_to_angles(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        task: i32,
        owner: W::Id,
        angles: &mut [f32; 3],
        duration: f32,
    ) {
        script_calls::lerp_to_angles(self.world, icarus, task, owner, *angles, duration);
    }

    fn tag(
        &mut self,
        _icarus: &mut Icarus<W::Id>,
        owner: W::Id,
        name: &str,
        lookup: i32,
        info: &mut [f32; 3],
    ) -> bool {
        script_calls::tag(self.world, owner, name, lookup, info)
    }

    fn use_target(&mut self, icarus: &mut Icarus<W::Id>, owner: W::Id, target: &str) {
        script_calls::use_target(self.world, icarus, owner, target);
    }

    fn kill(&mut self, icarus: &mut Icarus<W::Id>, owner: W::Id, name: &str) {
        script_calls::kill(self.world, icarus, owner, name);
    }

    fn remove(&mut self, _icarus: &mut Icarus<W::Id>, owner: W::Id, name: &str) {
        script_calls::remove(self.world, owner, name);
    }

    fn play(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        task: i32,
        owner: W::Id,
        kind: &str,
        name: &str,
    ) {
        script_calls::play(self.world, icarus, task, owner, kind, name);
    }

    fn get_float(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        owner: W::Id,
        _kind: i32,
        name: &str,
    ) -> Option<f32> {
        script_gets::get_float(self.world, icarus, owner, name)
    }

    fn get_vector(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        owner: W::Id,
        _kind: i32,
        name: &str,
    ) -> Option<[f32; 3]> {
        script_gets::get_vector(self.world, icarus, owner, name)
    }

    fn get_string(
        &mut self,
        icarus: &mut Icarus<W::Id>,
        owner: W::Id,
        _kind: i32,
        name: &str,
    ) -> StringAnswer {
        script_gets::get_string(self.world, icarus, owner, name)
    }

    fn precache_sound(&mut self, file: &str) {
        self.world.sound_index(file);
    }

    /// `ICARUS_InterrogateScript`'s cases (`GameInterface.cpp:558-590`): the behaviour
    /// scripts and the cinematic skip script are scripts, `SET_LOOPSOUND` a sound. (Its
    /// `SET_LOSTENEMYSCRIPT` case is never reached: the table cannot name it.)
    fn set_kind(&mut self, name: &str) -> SetKind {
        match set_id(name) {
            id if (SET_SPAWNSCRIPT..=SET_MINDTRICKSCRIPT).contains(&id)
                || id == SET_CINEMATIC_SKIPSCRIPT =>
            {
                SetKind::Script
            }
            SET_LOOPSOUND => SetKind::LoopSound,
            _ => SetKind::Other,
        }
    }
}
