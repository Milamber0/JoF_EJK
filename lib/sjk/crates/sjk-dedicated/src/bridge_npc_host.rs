//! The game as the NPC roster sees it ([`NpcHost`]): this server's entity pool, tables,
//! map and players — the traces and `Pmove` through the BSP and whoever stands in it, the
//! potentially visible set, the players as an NPC's senses read them, and the skeletons
//! NPC models animate with.

use super::super::*;
use sjk_game_jka::AnimationLengths;
use sjk_game_jka::npc_senses::Body;
use sjk_game_jka::npc_spawn::NpcHost;
use sjk_game_jka::pmove::{MoveContext, Predictor};
use sjk_game_jka::registries::EffectTable;
use std::sync::Arc;

/// `s.solid`.
const ES_SOLID: usize = 26;
/// `EF_SHADER_ANIM`: a usable that only animates its shader.
const EF_SHADER_ANIM: u32 = 1 << 4;
/// The skeleton every player model shares (`localAnimIndex` 0).
const HUMANOID: &str = "models/players/_humanoid/_humanoid";

/// An NPC model's skeleton: its animation lengths, and whether it is the humanoid one.
pub(super) type Skeleton = (Option<Arc<dyn AnimationLengths>>, bool);

/// What an NPC's think, pain or death did beyond the roster, applied by the game as the
/// roster call that did it returns — for a frame's thinks, as each NPC's turn ends.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum NpcOutcome {
    /// `AddScore` on a player.
    PlayerScore(u16, i32),
    /// A player's kill of an NPC: the stun baton's count and the excellent award.
    CreditKill(u16, u32),
    /// `AddScore` on an NPC: its team's score in a team game, and the ranks.
    NpcScored(i32, i32),
    /// `NPC_CheckAttacker`: whom a player is taken to fight.
    PlayerEnemy(u16, Option<u16>),
    /// `npc kill team nonally` on a player.
    KillPlayer(u16),
    /// A line of the game log (`G_LogPrintf`).
    Log(String),
    /// An NPC's blade's blow on a player or a breakable (`WP_SaberApplyDamage`).
    SaberBlow(u16, DamageRequest),
    /// An NPC's Force power's `G_Damage` on a player (lightning, drain, a grip's squeeze).
    ForceBlow(u16, DamageRequest),
    /// A reference function the NPC's think, pain or death reached that this server does
    /// not run ([`sjk_game_jka::npc_spawn::NpcHost::stub`]): told once, and kept for tests.
    Unported(u16, String),
    /// `saberKnockOutOfHand` on a player's saber by an NPC's blade, at a velocity.
    KnockSaber(u16, [f32; 3]),
    /// A player's knocked saber touched by an NPC (`SaberBounceSound`).
    TouchSaber(u16),
    /// `pmove.checkDuelLoss` for a player who lost a lock to an NPC: the NPC as attacker,
    /// where it stands, its blades' readings and its disarm chance.
    LostLock(
        u16,
        sjk_game_jka::damage::Attacker,
        [f32; 3],
        sjk_game_jka::saber_clash::SaberStorage,
        i32,
    ),
    /// `WP_Explode`'s `G_RadiusDamage` by an NPC (a cultist destroyer): where, how much, how
    /// far, by whom.
    Explode([f32; 3], i32, f32, u16),
    /// `G_TouchTriggers` for an NPC: the map's triggers its box meets
    /// ([`super::triggers`]).
    TouchTriggers(u16),
    /// A saber's wall-bounce splash on breakable `number` by an NPC: `G_Damage` of
    /// `damage`, `MOD_MELEE`.
    HurtBrush(u16, i32, u16),
    /// `saberCheckKnockdown_Smashed` on a player's thrown saber struck by an NPC's blade:
    /// the player, the NPC, whether it was defending, the damage.
    SmashSaber(u16, u16, bool, i32),
}

/// The game as an NPC spawn and think see it.
pub(crate) struct ServerHost<'a> {
    pub(super) pool: &'a mut EntityPool,
    pub(super) sounds: &'a mut SoundTable,
    pub(super) models: &'a mut ModelTable,
    pub(super) effects: &'a mut EffectTable,
    pub(super) bones: &'a mut sjk_game_jka::registries::BoneTable,
    pub(super) told: &'a mut Vec<Told>,
    /// The game's generator and `player_die`'s death counter, shared with the players.
    pub(super) deaths: &'a mut Deaths,
    /// The missiles in flight: an NPC's shots join them.
    pub(super) missiles: &'a mut Vec<(EntityId, Missile)>,
    /// The players, by their places, read for their names and scores.
    pub(super) server: &'a mut Server<(), crate::peer::Peer>,
    pub(super) world: WorldId,
    pub(super) roster: &'a crate::players::PlayerRoster,
    /// What the NPCs did to the players and the game beyond the roster, for the game to
    /// apply once the NPC's turn is over ([`NpcOutcome`]).
    pub(super) outcomes: Vec<NpcOutcome>,
    /// In a Jedi Master game, the master; the warm-up and the intermission.
    pub(super) jedi_master: Option<u16>,
    pub(super) warmup: bool,
    pub(super) intermission: bool,
    pub(super) map: Option<&'a LoadedMap>,
    /// Everything solid but the NPCs: the players first, then missiles and brushes.
    pub(super) solids: &'a [BoxObstacle],
    /// The players as the NPCs' senses read them.
    pub(super) players: &'a [Body],
    /// Which clients are in use (`g_entities[n].inuse`), a bit each.
    pub(super) clients: u64,
    /// The skeletons NPC models have been found to animate with, by model.
    pub(super) skeletons: &'a mut Vec<(Vec<u8>, Skeleton)>,
    /// Each NPC's server-side model, and the game's cache of the models they are built
    /// from; the Ghoul2 clock their bolts are read at (the previous frame's time).
    pub(super) bodies: &'a mut super::bodies::NpcBodies,
    pub(super) models_cache: &'a mut std::collections::HashMap<
        String,
        Arc<sjk_game_jka::server_skeleton::SkeletonModels>,
    >,
    pub(super) ghoul2_time: i32,
    pub(super) level_time: i32,
    pub(super) gravity: f32,
    pub(super) skill: i32,
    pub(super) gametype: i32,
    pub(super) client_zero: ([f32; 3], i32),
    /// The items registered while the level spawns; afterwards they are not recorded.
    pub(super) items: Option<&'a mut Vec<usize>>,
    /// The players `G_KillBox` found in an NPC's box.
    pub(super) telefrags: Vec<usize>,
    /// The movers, the `trigger_multiple`s, the breakable brushes and the usable brushes,
    /// which the navigator asks about (`G_EntIsDoor` and its kin).
    pub(super) doors: &'a [(EntityId, sjk_game_jka::movers::Door)],
    pub(super) multiples: &'a [sjk_game_jka::triggers::Multiple],
    pub(super) breakables: &'a [(EntityId, sjk_game_jka::breakables::Breakable)],
    pub(super) usables: &'a [(EntityId, sjk_game_jka::use_key::Usable)],
    /// The game's `rand()`, which a missile an NPC's push turns back reads; the dropped
    /// items a pull's torn-away weapons join.
    pub(super) crt: &'a mut CrtRand,
    pub(super) pickups: &'a mut Vec<(EntityId, Pickup)>,
    /// `g_TimeSinceLastFrame`, which the NPCs' Force regenerates by.
    pub(super) since_last_frame: i32,
    /// What the NPCs' Force powers leave for their use's end, kept between frames
    /// (`force`).
    pub(super) force_touch: &'a mut force::ForceTouch,
    /// `bg_fighterAltControl`.
    pub(super) fighter_alt_control: bool,
    /// `g_debugSaberLocks`.
    pub(super) debug_saber_locks: bool,
    /// `g_dismember` and the clients with heavy melee ([`dismember`]).
    pub(super) limbs: dismember::LimbRules,
}

impl ServerHost<'_> {
    /// Player `client`, when it is one in the world.
    pub(super) fn peer(&self, client: u16) -> Option<&crate::peer::Peer> {
        let handle = self.roster.at(usize::from(client))?;
        self.server.world(self.world)?.entity(handle)
    }

    /// A configstring registered: told to the clients.
    fn register(told: &mut Vec<Told>) -> impl FnMut(usize, &[u8]) + '_ {
        |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        }
    }

    /// The players (by place) linked where their box, grown by a unit, meets
    /// `low`..`high` (`EntitiesInBox`).
    fn players_in(&self, low: [f32; 3], high: [f32; 3]) -> impl Iterator<Item = &BoxObstacle> {
        self.solids.iter().filter(move |body| {
            usize::from(body.entity) < 32
                && body.model.is_none()
                && (0..3).all(|axis| {
                    body.origin[axis] + body.bounds.0[axis] - 1.0 <= high[axis]
                        && body.origin[axis] + body.bounds.1[axis] + 1.0 >= low[axis]
                })
        })
    }

    /// `SetupGameGhoul2Model`'s skeleton for `models/players/<model>/model.glm`
    /// (`G2API_GetGLAName`, `BG_ParseAnimationFile`): the animation.cfg beside its GLA,
    /// read once; the humanoid's where the GLA is the humanoid or the model is missing.
    fn skeleton(map: &LoadedMap, model: &[u8]) -> Skeleton {
        if model.ends_with(b".md3") {
            let path = String::from_utf8_lossy(model);
            let directory = path.rsplit_once('/').map_or("", |(directory, _)| directory);
            let config = map
                .files
                .read(&format!("{directory}/animation.cfg"))
                .ok()
                .flatten()
                .and_then(|asset| sjk_model::AnimationConfig::parse(&asset.bytes).ok());
            // A rigid body with no cfg has a known empty animation set. `None`
            // means unavailable timing data and makes NPC death skip cleanup.
            let lengths = config
                .map(|config| sjk_game_jka::AnimationLengthTable::from_animation_config(&config))
                .unwrap_or_else(|| sjk_game_jka::AnimationLengthTable::new([]));
            return (Some(Arc::new(lengths)), false);
        }
        let humanoid = (map.animations.clone(), true);
        let path = format!(
            "models/players/{}/model.glm",
            String::from_utf8_lossy(model)
        );
        let Ok(Some(glm)) = map.files.read(&path) else {
            return humanoid;
        };
        // The header's `animName` (64 bytes at 72): the GLA, without its extension.
        let Some(name) = glm.bytes.get(72..136) else {
            return humanoid;
        };
        let name =
            String::from_utf8_lossy(&name[..name.iter().position(|&byte| byte == 0).unwrap_or(64)])
                .into_owned();
        if name.is_empty() || name.eq_ignore_ascii_case(HUMANOID) {
            return humanoid;
        }
        let directory = name
            .rsplit_once('/')
            .map_or(name.as_str(), |(directory, _)| directory);
        let config = map
            .files
            .read(&format!("{directory}/animation.cfg"))
            .ok()
            .flatten()
            .and_then(|asset| sjk_model::AnimationConfig::parse(&asset.bytes).ok());
        let table = config.map(|config| {
            Arc::new(sjk_game_jka::AnimationLengthTable::from_animation_config(
                &config,
            )) as Arc<dyn AnimationLengths>
        });
        (table, false)
    }
}

impl sjk_game_jka::saber_definition::SaberParseHost for ServerHost<'_> {
    fn sound_index(&mut self, name: &[u8]) -> u16 {
        self.sounds.index(name, &mut Self::register(self.told))
    }

    fn irand(&mut self, low: i32, high: i32) -> i32 {
        self.deaths.rng.irand(low, high)
    }
}

impl NpcHost for ServerHost<'_> {
    fn crt_rand(&mut self) -> i32 {
        self.crt.next()
    }

    fn nav_obstacle(&self, number: u16) -> sjk_game_jka::npc_nav_setup::NavObstacle {
        use sjk_game_jka::npc_nav_setup::NavObstacle;
        if let Some(at) = self
            .doors
            .iter()
            .position(|(known, _)| known.legacy_number() == number)
        {
            return sjk_game_jka::npc_nav_setup::door_obstacle(self.doors, at, self.multiples);
        }
        if self
            .breakables
            .iter()
            .any(|(known, _)| known.legacy_number() == number)
        {
            return NavObstacle::Breakable;
        }
        // `G_EntIsRemovableUsable`: a named usable that is no shader animation and not
        // always on.
        let removable = self.usables.iter().any(|(known, usable)| {
            known.legacy_number() == number
                && usable.eflags & EF_SHADER_ANIM == 0
                && usable.spawnflags & sjk_game_jka::use_key::USABLE_ALWAYS_ON == 0
                && !usable.targetname.is_empty()
        });
        if removable {
            NavObstacle::RemovableUsable
        } else {
            NavObstacle::Other
        }
    }

    fn door_center(&self, number: u16) -> Option<[f32; 3]> {
        let at = self
            .doors
            .iter()
            .position(|(known, _)| known.legacy_number() == number)?;
        sjk_game_jka::npc_nav_setup::door_center(self.doors, at)
    }

    fn entity_box(&self, number: u16) -> Option<([f32; 3], [f32; 3], [f32; 3])> {
        self.solids
            .iter()
            .find(|body| body.entity == number)
            .map(|body| (body.origin, body.bounds.0, body.bounds.1))
    }

    // The NPC code still names entities by their legacy numbers (`NpcHost`): each
    // crossing into the pool goes through the adapter's numbering.
    fn spawn_entity(&mut self) -> Option<u16> {
        self.pool
            .spawn_entity(
                EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
                self.level_time,
            )
            .map(EntityId::legacy_number)
    }

    fn spawn_hidden(&mut self) -> Option<u16> {
        self.pool
            .spawn_hidden(self.level_time)
            .map(EntityId::legacy_number)
    }

    fn brush_bounds(&mut self, name: &str) -> Option<([f32; 3], [f32; 3])> {
        let index = name.strip_prefix('*')?.parse::<usize>().ok()?;
        let model = self.map?.bsp.render().models().get(index)?;
        Some((model.minimums, model.maximums))
    }

    fn show(&mut self, number: u16) {
        if let Some(id) = self.pool.legacy_id(number) {
            self.pool.show(id, true);
        }
    }

    fn free(&mut self, number: u16) {
        self.bodies.forget(number);
        if let Some(id) = self.pool.legacy_id(number) {
            self.pool.free(id, self.level_time);
        }
    }

    fn publish(
        &mut self,
        number: u16,
        state: &EntityState,
        bounds: ([f32; 3], [f32; 3]),
        contents: u32,
    ) {
        let Some(id) = self.pool.legacy_id(number) else {
            return;
        };
        let Some(slot) = self.pool.state_mut(id) else {
            return;
        };
        // Field by field into the slot's own state: an NPC is published every frame.
        for index in 0..sjk_protocol::LEGACY_ENTITY_FIELDS.len() {
            slot.set_raw_field(index, state.raw_field(index).unwrap_or(0));
        }
        let _ = slot.set_number(number);
        // `SV_LinkEntity`: a body's box packed into `solid` for a client's prediction.
        let solid = if contents & (0x1 | CONTENTS_BODY) != 0 {
            let byte = |value: f32| (value as i32).clamp(1, 255) as u32;
            byte(bounds.1[2] + 32.0) << 16 | byte(-bounds.0[2]) << 8 | byte(bounds.1[0])
        } else {
            0
        };
        slot.set_raw_field(ES_SOLID, solid);
        let model = usize::try_from(state.model_index())
            .ok()
            .and_then(|index| index.checked_sub(1))
            .and_then(|index| self.models.names().get(index));
        if model.is_some_and(|path| path.ends_with(b".md3"))
            && matches!(state.entity_type(), 13 | 15)
        {
            sjk_game_jka::npc_rigid::project(slot);
        }
        self.pool.set_bounds(id, bounds);
    }

    fn model_index(&mut self, name: &[u8]) -> u16 {
        self.models.index(name, &mut Self::register(self.told))
    }

    fn effect_index(&mut self, name: &[u8]) -> u16 {
        self.effects.index(name, &mut Self::register(self.told))
    }

    fn bone_index(&mut self, name: &[u8]) -> u16 {
        self.bones.index(name, &mut Self::register(self.told))
    }

    fn register_item(&mut self, item: usize) {
        if let Some(items) = self.items.as_deref_mut() {
            items.push(item);
        }
    }

    fn model_loads(&mut self, model: &[u8]) -> bool {
        if model.ends_with(b".md3") {
            return self.map.is_some_and(|map| {
                map.files
                    .contains(&String::from_utf8_lossy(model))
                    .unwrap_or(false)
            });
        }
        let path = format!(
            "models/players/{}/model.glm",
            String::from_utf8_lossy(model)
        );
        self.map
            .is_some_and(|map| map.files.contains(&path).unwrap_or(false))
    }

    fn trace(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        mask: u32,
        bodies: &[BoxObstacle],
    ) -> sjk_game_jka::pmove::MovementTrace {
        Everyone {
            solids: self.solids,
            bodies,
            pass,
            riders: &[],
        }
        .trace(self.map, start, mins, maxs, end, mask)
    }

    fn player_in_box(&mut self, low: [f32; 3], high: [f32; 3]) -> bool {
        self.players_in(low, high)
            .any(|body| body.contents & sjk_game_jka::npc_begin::MASK_NPCSOLID != 0)
    }

    fn telefrag_players(&mut self, low: [f32; 3], high: [f32; 3], _killer: u16) {
        let victims: Vec<usize> = self
            .players_in(low, high)
            .map(|body| usize::from(body.entity))
            .collect();
        self.telefrags.extend(victims);
    }

    fn player_in_box_but(&mut self, low: [f32; 3], high: [f32; 3], owner: u16) -> bool {
        self.players_in(low, high).any(|body| {
            body.entity != owner && body.contents & sjk_game_jka::npc_begin::MASK_NPCSOLID != 0
        })
    }

    fn telefrag_players_but(&mut self, low: [f32; 3], high: [f32; 3], _killer: u16, owner: u16) {
        let victims: Vec<usize> = self
            .players_in(low, high)
            .filter(|body| body.entity != owner)
            .map(|body| usize::from(body.entity))
            .collect();
        self.telefrags.extend(victims);
    }

    fn npc_bolt_matrix(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        _index: i32,
        bolt: Option<&str>,
        angles: [f32; 3],
        origin: [f32; 3],
        _level_time: i32,
    ) -> [[f32; 4]; 3] {
        bolt.and_then(|bolt| self.body_bolt_matrix_turned(npc.number, bolt, angles, origin))
            .unwrap_or_else(|| sjk_game_jka::npc_machine_parts::unbolted_matrix(angles, origin))
    }

    fn npc_struck_part(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        level_time: i32,
    ) -> Option<i32> {
        self.body_struck_part(npc, level_time)
    }

    fn launch_missile(&mut self, _shooter: u16, missile: sjk_game_jka::weapon_fire::Missile) {
        if let Some(number) = self
            .pool
            .spawn_entity(missile.state.clone(), self.level_time)
        {
            self.pool.set_bounds(number, missile.bounds);
            self.missiles.push((number, missile));
        }
    }

    fn raise(&mut self, event: EventEntity) {
        let _ = self
            .pool
            .spawn_temporary(event.state(), self.level_time, None);
    }

    fn npc_item(&self, index: usize) -> Option<sjk_game_jka::npc_weapon_pickup::NpcItem> {
        self.npc_item_at(index)
    }

    fn npc_item_refuses(&mut self, number: u16, toucher: u16, level_time: i32) -> bool {
        self.npc_item_refused(number, toucher, level_time)
    }

    fn npc_took_item(&mut self, number: u16, _toucher: u16, respawn: i32, level_time: i32) {
        self.npc_item_taken(number, respawn, level_time);
    }

    fn client_buttons(&self, number: u16) -> u16 {
        self.player_buttons(number)
    }

    fn drop_item(&mut self, _dropper: u16, pickup: Pickup) -> Option<u16> {
        let number = self
            .pool
            .spawn_entity(pickup.state.clone(), self.level_time)?;
        self.pool.set_bounds(number, pickup.bounds);
        self.pickups.push((number, pickup));
        Some(number.legacy_number())
    }

    fn client_zero(&self) -> ([f32; 3], i32) {
        self.client_zero
    }

    fn print(&mut self, text: &str) {
        print!("{text}");
    }

    fn gravity(&self) -> f32 {
        self.gravity
    }

    fn skill(&self) -> i32 {
        self.skill
    }

    fn gametype(&self) -> i32 {
        self.gametype
    }

    fn rng(&mut self) -> &mut Rng {
        &mut self.deaths.rng
    }

    fn death_counter(&mut self) -> &mut u8 {
        &mut self.deaths.next_death_event
    }

    fn fire_weapon(
        &mut self,
        _: u16,
        player: &mut PlayerState,
        entity: &EntityState,
        alternate: bool,
    ) {
        let mut fired = Vec::new();
        let Self {
            deaths,
            sounds,
            told,
            level_time,
            ..
        } = self;
        let mut sound = |name: &[u8]| {
            sounds.index(name, &mut |index, value| {
                told.push(Told::ConfigString {
                    index,
                    previous: Vec::new(),
                    value: value.to_vec(),
                })
            })
        };
        sjk_game_jka::weapon_fire::fire_weapon(
            player,
            entity,
            *level_time,
            alternate,
            &mut deaths.rng,
            0.0,
            &|_| false,
            &mut sound,
            &mut fired,
        );
        for missile in fired {
            self.launch_missile(0, missile);
        }
    }

    fn free_model(&mut self, number: u16) {
        self.bodies.forget(number);
        if let Some(id) = self.pool.legacy_id(number) {
            self.pool.free(id, self.level_time);
        }
        self.told
            .push(Told::Everyone(format!("kg2 {number}").into_bytes()));
    }

    fn client_name(&self, number: u16) -> Vec<u8> {
        self.peer(number)
            .map(|peer| peer.name.clone())
            .unwrap_or_default()
    }

    fn log(&mut self, text: &str) {
        self.outcomes.push(NpcOutcome::Log(text.to_owned()));
    }

    fn stub(&mut self, number: u16, name: &str) {
        self.outcomes
            .push(NpcOutcome::Unported(number, name.to_owned()));
    }

    fn add_player_score(&mut self, player: u16, points: i32) {
        self.outcomes.push(NpcOutcome::PlayerScore(player, points));
    }

    fn credit_kill(&mut self, player: u16, means: u32) {
        self.outcomes.push(NpcOutcome::CreditKill(player, means));
    }

    fn npc_scored(&mut self, team: i32, points: i32) {
        self.outcomes.push(NpcOutcome::NpcScored(team, points));
    }

    fn jedi_master(&self) -> Option<u16> {
        self.jedi_master
    }

    fn warmup(&self) -> bool {
        self.warmup
    }

    fn intermission(&self) -> bool {
        self.intermission
    }

    fn set_player_enemy(&mut self, player: u16, enemy: Option<u16>) {
        self.outcomes.push(NpcOutcome::PlayerEnemy(player, enemy));
    }

    fn humanoid_animations(&mut self) -> Option<Arc<dyn AnimationLengths>> {
        self.map.and_then(|map| map.animations.clone())
    }

    fn kill_player(&mut self, player: u16) {
        self.outcomes.push(NpcOutcome::KillPlayer(player));
    }

    fn client_slots(&self) -> u16 {
        self.roster.places() as u16
    }

    fn player_score(&self, client: u16) -> i32 {
        self.peer(client)
            .map_or(0, |peer| peer.state.persistent[0] as i32)
    }

    fn players(&self) -> &[Body] {
        self.players
    }

    fn in_use(&self, number: u16) -> bool {
        if number < 32 {
            return self.clients & (1 << number) != 0;
        }
        self.pool.legacy_id(number).is_some()
    }

    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool {
        self.map.is_none_or(|map| {
            crate::visibility::Eye::new(&map.bsp, &map.areas, from).sees_point(&map.bsp, to)
        })
    }

    fn move_npc(
        &mut self,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
        pass: u16,
        bodies: &[BoxObstacle],
    ) {
        // `PM_FootSlopeTrace` on the NPC's own model, at the Ghoul2 clock.
        let (npc_bodies, clock) = (std::cell::RefCell::new(&mut *self.bodies), self.ghoul2_time);
        let feet = |origin: [f32; 3], yaw: f32| {
            npc_bodies
                .borrow_mut()
                .foot_points(pass, yaw, origin, clock)
        };
        Everyone {
            solids: self.solids,
            bodies,
            pass,
            riders: &[],
        }
        .predict(self.map, movement, command, &context.with_foot_bolts(&feet));
    }

    fn move_npc_buried(
        &mut self,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
        pass: u16,
        bodies: &[BoxObstacle],
    ) {
        let collision = BuriedCollision {
            everyone: Everyone {
                solids: self.solids,
                bodies,
                pass,
                riders: &[],
            },
            map: self.map,
        };
        movement.predict_command_in(command, &collision, context);
    }

    fn move_vehicle(
        &mut self,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
        pass: u16,
        bodies: &[BoxObstacle],
        riders: &[u16],
        game: &mut dyn sjk_game_jka::pmove::vehicle::VehicleGame,
    ) {
        Everyone {
            solids: self.solids,
            bodies,
            pass,
            riders,
        }
        .predict_vehicle(self.map, movement, command, context, game);
    }

    fn npc_animations(&mut self, model: &[u8]) -> Skeleton {
        let Some(map) = self.map else {
            return (None, true);
        };
        if let Some((_, skeleton)) = self
            .skeletons
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(model))
        {
            return skeleton.clone();
        }
        let skeleton = Self::skeleton(map, model);
        self.skeletons.push((model.to_vec(), skeleton.clone()));
        skeleton
    }

    fn pose_npc(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        look_target: Option<[f32; 3]>,
        level_time: i32,
    ) -> sjk_game_jka::npc_skeleton::NpcBlades {
        self.pose_body(npc, look_target, level_time)
    }

    fn collide_npc(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
    ) -> sjk_game_jka::saber_damage::Ghoul2Answer {
        self.collide_body(npc, start, end, radius, level_time)
    }

    fn model_has_bolt(&mut self, model: &[u8], name: &str) -> bool {
        self.body_model_has_bolt(model, name)
    }

    fn npc_bolt(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        _index: i32,
        bolt: Option<&str>,
        _level_time: i32,
    ) -> [f32; 3] {
        self.body_bolt(npc, bolt)
    }

    fn rancor_attach(
        &mut self,
        rancor: &sjk_game_jka::npc_spawn::NpcActor,
        _victim: u16,
        in_mouth: bool,
        _level_time: i32,
    ) -> Option<([f32; 3], [f32; 3])> {
        let bolt = sjk_game_jka::npc_creature::rancor_attach_bolt(in_mouth);
        let matrix = self.body_bolt_matrix(
            rancor.number,
            bolt,
            rancor.mind.current_angles[1],
            rancor.current_origin,
        )?;
        Some(sjk_game_jka::npc_creature::attach_to_rancor(
            matrix, in_mouth,
        ))
    }

    fn set_npc_bone_angles(&mut self, number: u16, bone: &str, angles: [f32; 3], level_time: i32) {
        if self.vehicle_set_bone_angles(number, bone, angles, level_time) {
            return;
        }
        self.body_set_bone_angles(number, bone, angles, level_time);
    }

    fn freed_ground_residue(&mut self, number: u16, ground: u16) {
        self.pool.leave_freed_ground_residue(number, ground);
    }

    fn world_point_contents(&mut self, point: [f32; 3]) -> u32 {
        self.map.map_or(0, |map| {
            sjk_game_jka::pmove::MovementCollision::point_contents(
                &crate::collision::WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                point,
            )
        })
    }

    fn surface_status(&mut self, number: u16, name: &str) -> i32 {
        self.body_surface_status(number, name)
    }

    fn set_npc_surface(&mut self, number: u16, name: &str, flags: u32) {
        self.body_set_surface(number, name, flags);
    }

    fn dismember_setting(&self) -> i32 {
        self.limbs.dismember
    }

    fn heavy_melee(&self, attacker: u16) -> bool {
        self.heavy_melee_of(attacker)
    }

    fn update_npc_anims(&mut self, npc: &sjk_game_jka::npc_spawn::NpcActor, level_time: i32) {
        self.update_npc_skeleton(npc, level_time);
    }

    fn client_bolt_matrix(
        &mut self,
        number: u16,
        bone: &str,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> Option<[[f32; 4]; 3]> {
        self.client_bone_matrix(number, bone, angles, origin)
    }

    fn client_hilt_direction(
        &mut self,
        number: u16,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> Option<[f32; 3]> {
        self.client_hilt(number, angles, origin)
    }

    fn player_surface_location(
        &mut self,
        player: u16,
        spot: [f32; 3],
        level_time: i32,
    ) -> Option<sjk_game_jka::damage::HitLocation> {
        self.player_struck_location(player, spot, level_time)
    }

    fn player_saber_storage(&self, player: u16) -> Option<sjk_game_jka::saber_clash::SaberStorage> {
        self.player_storage(player)
    }

    fn entity_state(&self, number: u16) -> Option<&sjk_protocol::EntityState> {
        self.pool.state(self.pool.legacy_id(number)?)
    }

    fn client_ground(&self, number: u16) -> Option<u16> {
        self.peer(number).map(|peer| peer.state.ground_entity_num())
    }

    fn client_legs(&self, number: u16) -> Option<u16> {
        self.peer(number).map(|peer| peer.state.leg_animation())
    }

    fn projectile_ghoul2_collision(&self) -> bool {
        // This server sweeps every missile through the clients' models
        // (`d_projectileGhoul2Collision` 1, [`super::super::bridge_missile_models`]).
        true
    }

    fn npc_surface_location(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        flags: u32,
        spot: [f32; 3],
        level_time: i32,
    ) -> Option<sjk_game_jka::damage::HitLocation> {
        self.body_surface_location(npc, flags, spot, level_time)
    }

    fn collide_player(
        &mut self,
        player: u16,
        start: [f32; 3],
        end: [f32; 3],
        radius: f32,
        level_time: i32,
    ) -> sjk_game_jka::saber_damage::Ghoul2Answer {
        self.collide_player_body(player, start, end, radius, level_time)
    }

    fn player_saber_victim(
        &self,
        player: u16,
        swinger: &Body,
    ) -> Option<sjk_game_jka::saber_damage::SaberVictim> {
        self.player_victim(player, swinger)
    }

    fn player_saber(&self, saber_entity: u16) -> Option<sjk_game_jka::saber_clash::Fighter> {
        self.player_fighter(saber_entity)
    }

    fn set_player_saber(&mut self, owner: &sjk_game_jka::saber_clash::Fighter) {
        self.set_player_fighter(owner);
    }

    fn player_saber_boxes(&self, out: &mut Vec<BoxObstacle>) {
        self.saber_boxes(out);
    }

    fn saber_blow_on_player(&mut self, player: u16, request: DamageRequest) -> (i32, Option<u32>) {
        // Dealt once the NPC's turn is over, with the player's struck surface; the NPC's
        // hit counter then.
        self.outcomes.push(NpcOutcome::SaberBlow(player, request));
        (0, None)
    }

    fn take_entity_state(&mut self, number: u16) -> Option<EntityState> {
        self.take_state(number)
    }

    fn put_entity_state(&mut self, number: u16, state: EntityState, shown: bool) {
        self.put_state(number, state, shown);
    }

    fn entity_event(&mut self, number: u16, event_time: i32) {
        self.state_event(number, event_time);
    }

    fn player_blocks_thrown(&mut self, player: u16, point: [f32; 3]) -> bool {
        self.blocks_thrown(player, point)
    }

    fn player_saber_defense(&self, player: u16) -> Option<u8> {
        self.peer(player).map(|peer| peer.force.levels[16])
    }

    fn knock_player_saber(&mut self, player: u16, velocity: [f32; 3]) -> bool {
        // Done with the player's saber flight once the NPC's turn is over.
        self.outcomes.push(NpcOutcome::KnockSaber(player, velocity));
        false
    }

    fn touch_player_sabers(&mut self, origin: [f32; 3], bounds: ([f32; 3], [f32; 3])) {
        for client in 0..self.roster.places() as u16 {
            if self.peer(client).is_some_and(|peer| {
                peer.saber_entity.is_some()
                    && sjk_game_jka::saber_drop::touched_by(&peer.flight, origin, bounds)
            }) {
                self.outcomes.push(NpcOutcome::TouchSaber(client));
            }
        }
    }

    fn player_lock(
        &mut self,
        npc: &mut sjk_game_jka::npc_spawn::NpcActor,
        player: u16,
        npc_first: bool,
        bodies: &mut Vec<BoxObstacle>,
        level_time: i32,
    ) -> bool {
        self.lock_with_player(npc, player, npc_first, bodies, level_time)
    }

    fn move_npc_locked(
        &mut self,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
        pass: u16,
        bodies: &[BoxObstacle],
        partner: sjk_game_jka::npc_saber_lock::LockPartner<'_>,
        hits: i32,
    ) -> sjk_game_jka::pmove_saber_lock::LockOutcome {
        self.locked_move(movement, command, context, pass, bodies, partner, hits)
    }

    fn player_splash_state(&self, number: u16) -> Option<(bool, bool)> {
        // `ps.eFlags2 & EF2_HELD_BY_MONSTER`; standing on something.
        let state = &self.peer(number)?.state;
        Some((
            state.raw_field(103).unwrap_or(0) & 1 != 0,
            state.ground_entity_num() != sjk_protocol::ENTITY_NUMBER_NONE,
        ))
    }

    fn breakables_in_box(&self, mins: [f32; 3], maxs: [f32; 3], out: &mut Vec<u16>) {
        let overlaps = |(low, high): ([f32; 3], [f32; 3])| {
            (0..3).all(|axis| low[axis] - 1.0 <= maxs[axis] && high[axis] + 1.0 >= mins[axis])
        };
        out.extend(
            self.breakables
                .iter()
                .filter(|(_, brush)| overlaps(brush.bounds))
                .map(|(number, _)| number.legacy_number()),
        );
    }

    fn breakable_takes_damage(&self, number: u16) -> Option<bool> {
        self.breakables
            .iter()
            .find(|(ours, _)| ours.legacy_number() == number)
            .map(|(_, brush)| brush.takes_damage)
    }

    fn smash_player_saber(
        &mut self,
        player: u16,
        striker: u16,
        defending: bool,
        damage: i32,
    ) -> bool {
        // Done with the player's saber flight once the roster's run is over.
        self.outcomes
            .push(NpcOutcome::SmashSaber(player, striker, defending, damage));
        false
    }

    fn hurt_breakable(&mut self, number: u16, damage: i32, attacker: u16) {
        // Dealt once the roster's run is over, as an NPC's blows on players are.
        self.outcomes
            .push(NpcOutcome::HurtBrush(number, damage, attacker));
    }

    fn touch_map_triggers(&mut self, npc: &sjk_game_jka::npc_spawn::NpcActor) {
        // Touched once the roster's run is over, in the NPCs' order.
        self.outcomes.push(NpcOutcome::TouchTriggers(npc.number));
    }

    fn explode(
        &mut self,
        _npc: u16,
        at: [f32; 3],
        damage: i32,
        radius: f32,
        attacker: sjk_game_jka::damage::Attacker,
    ) -> (i32, Option<u32>) {
        // Dealt once the NPC's turn is over, as an NPC's blows on players are.
        self.outcomes
            .push(NpcOutcome::Explode(at, damage, radius, attacker.client));
        (0, None)
    }

    fn player_locked_down(
        &mut self,
        player: u16,
        until: Option<i32>,
        other_killer: Option<(u16, i32, i32)>,
    ) {
        self.locked_down(player, until, other_killer);
    }

    fn player_lost_lock(
        &mut self,
        player: u16,
        attacker: sjk_game_jka::damage::Attacker,
        origin: [f32; 3],
        storage: &sjk_game_jka::saber_clash::SaberStorage,
        chance: i32,
        _level_time: i32,
    ) {
        self.outcomes.push(NpcOutcome::LostLock(
            player, attacker, origin, *storage, chance,
        ));
    }

    fn saber_blow_on_entity(&mut self, entity: u16, request: DamageRequest) {
        self.outcomes.push(NpcOutcome::SaberBlow(entity, request));
    }

    fn raise_entity(&mut self, state: &EntityState, not_for: u16) {
        let _ = self
            .pool
            .spawn_temporary(state.clone(), self.level_time, Some(not_for));
    }

    fn jedi_player(&self, number: u16) -> Option<sjk_game_jka::npc_jedi_glue::JediClient> {
        self.jedi_client_of(number)
    }

    fn entity_motion(&self, number: u16) -> Option<sjk_game_jka::npc_jedi_glue::EntityMotion> {
        self.entity_motion_of(number)
    }

    fn incoming(
        &self,
        mins: [f32; 3],
        maxs: [f32; 3],
        out: &mut Vec<sjk_game_jka::npc_missile_block::IncomingEntity>,
    ) {
        self.incoming_of(mins, maxs, out);
    }

    fn player_force(&mut self) -> Option<&mut dyn sjk_game_jka::npc_force_update::PlayerForce> {
        Some(self)
    }

    fn bolt(&mut self, model: &[u8], name: &str) -> i32 {
        self.vehicle_model_bolt(model, name)
    }

    fn vehicle_bolt(
        &mut self,
        number: u16,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
        _: i32,
    ) -> sjk_game_jka::vehicle_weapons::MuzzleBolt {
        self.vehicle_bolt_at(number, tag, angles, origin)
    }

    fn vehicle_driver_offset(&mut self, number: u16) -> [f32; 3] {
        ServerHost::vehicle_driver_offset(self, number)
    }

    fn vehicle_tag(
        &mut self,
        number: u16,
        tag: i32,
        angles: [f32; 3],
        origin: [f32; 3],
        _: i32,
    ) -> sjk_game_jka::vehicle_weapons::MuzzleBolt {
        self.vehicle_tag_at(number, tag, angles, origin)
    }

    fn fighter_alt_control(&self) -> bool {
        self.fighter_alt_control
    }

    fn debug_saber_locks(&self) -> bool {
        self.debug_saber_locks
    }

    fn gunner(&mut self, number: u16) -> Option<sjk_game_jka::vehicle_turrets::Gunner> {
        let handle = self.roster.at(usize::from(number))?;
        let peer = self.server.world(self.world)?.entity(handle)?;
        (peer.health > 0).then(|| sjk_game_jka::vehicle_turrets::Gunner {
            view_angles: peer.state.view_angles(),
            buttons: peer.last_command.buttons,
        })
    }

    fn turret_targets(&mut self, targets: &mut Vec<sjk_game_jka::vehicle_turrets::TurretTarget>) {
        // The breakable brushes (`FL_BBRUSH`): a brush's `r.currentOrigin` is its own
        // origin, which is where a turret aims.
        for (id, breakable) in self.breakables {
            let (low, high) = breakable.bounds;
            targets.push(sjk_game_jka::vehicle_turrets::TurretTarget {
                number: id.legacy_number(),
                client: false,
                takes_damage: breakable.takes_damage,
                health: breakable.health,
                no_target: false,
                shootable_thing: breakable.takes_damage,
                session_team: 0,
                temp_spectate_until: 0,
                team_no_damage: 0,
                owner: 1_023,
                origin: [0.0; 3],
                bounds: (low.map(|value| value - 1.0), high.map(|value| value + 1.0)),
                velocity: [0.0; 3],
            });
        }
    }

    fn impact_bodies(&mut self, bodies: &mut Vec<sjk_game_jka::pmove::vehicle_impact::ImpactBody>) {
        use sjk_game_jka::pmove::vehicle_impact::{ImpactBody, ImpactClass};
        for (id, missile) in self.missiles.iter() {
            let origin =
                [2, 1, 4].map(|index| f32::from_bits(missile.state.raw_field(index).unwrap_or(0)));
            bodies.push(ImpactBody {
                number: id.legacy_number(),
                class: ImpactClass::Missile,
                origin,
                speed: 0.0,
                owner: missile.owner,
                takes_damage: false,
            });
        }
        for solid in self.solids.iter().filter(|solid| solid.model.is_some()) {
            // A brush entity (`SOLID_BMODEL`); a turning `func_rotating` with `IMPACT` is not
            // among this server's movers yet.
            let class = ImpactClass::Brush {
                rotating_impact: false,
            };
            bodies.push(ImpactBody {
                number: solid.entity,
                class,
                origin: solid.origin,
                speed: 0.0,
                owner: 1_023,
                takes_damage: false,
            });
        }
    }

    fn trace_past_riders(
        &mut self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        pass: u16,
        riders: &[u16],
        mask: u32,
        bodies: &[BoxObstacle],
    ) -> sjk_game_jka::pmove::MovementTrace {
        Everyone {
            solids: self.solids,
            bodies,
            pass,
            riders,
        }
        .trace(self.map, start, mins, maxs, end, mask)
    }

    fn cull_distance(&self) -> f32 {
        self.map.map_or(6_000.0, |map| map.world.distance_cull)
    }

    fn launch(&mut self, missile: Missile) -> Option<u16> {
        let number = self
            .pool
            .spawn_entity(missile.state.clone(), self.level_time)?;
        self.pool.set_bounds(number, missile.bounds);
        let legacy = number.legacy_number();
        self.missiles.push((number, missile));
        Some(legacy)
    }
}

/// The world with everyone in it an NPC's move or trace meets: the other solids (players,
/// missiles, brushes) and the NPCs' bodies, the NPC itself (`pass`) left out.
struct Everyone<'a> {
    solids: &'a [BoxObstacle],
    bodies: &'a [BoxObstacle],
    pass: u16,
    /// The players the moving vehicle owns (its pilot), which it goes through.
    riders: &'a [u16],
}

impl Everyone<'_> {
    /// The obstacles as a list the collision takes: gathered into a small buffer on the
    /// stack where they fit (the common case), else on the heap.
    fn with_obstacles<R>(&self, run: impl FnOnce(&[BoxObstacle]) -> R) -> R {
        const ON_STACK: usize = 64;
        let count = self.solids.len() + self.bodies.len();
        let all = self
            .solids
            .iter()
            .chain(self.bodies)
            .filter(|body| body.entity != self.pass && !self.riders.contains(&body.entity))
            .copied();
        if count <= ON_STACK {
            let mut buffer = [BoxObstacle {
                entity: 0,
                origin: [0.0; 3],
                bounds: ([0.0; 3], [0.0; 3]),
                contents: 0,
                model: None,
            }; ON_STACK];
            let mut used = 0;
            for body in all {
                buffer[used] = body;
                used += 1;
            }
            run(&buffer[..used])
        } else {
            run(&all.collect::<Vec<_>>())
        }
    }

    fn trace(
        &self,
        map: Option<&LoadedMap>,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        mask: u32,
    ) -> sjk_game_jka::pmove::MovementTrace {
        self.with_obstacles(|players| match map {
            Some(map) => WithPlayers {
                world: WorldCollision {
                    bsp: &map.bsp,
                    scratch: &map.scratch,
                },
                players,
            }
            .trace(start, mins, maxs, end, mask),
            None => WithPlayers {
                world: Void,
                players,
            }
            .trace(start, mins, maxs, end, mask),
        })
    }

    fn predict(
        &self,
        map: Option<&LoadedMap>,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
    ) {
        self.with_obstacles(|players| match map {
            Some(map) => movement.predict_command_in(
                command,
                &WithPlayers {
                    world: WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    players,
                },
                context,
            ),
            None => movement.predict_command_in(
                command,
                &WithPlayers {
                    world: Void,
                    players,
                },
                context,
            ),
        });
    }

    /// [`Self::predict`] for a vehicle, with the game's vehicle functions.
    fn predict_vehicle(
        &self,
        map: Option<&LoadedMap>,
        movement: &mut Predictor,
        command: UserCommand,
        context: &MoveContext,
        game: &mut dyn sjk_game_jka::pmove::vehicle::VehicleGame,
    ) {
        self.with_obstacles(|players| match map {
            Some(map) => movement.predict_vehicle_command(
                command,
                &WithPlayers {
                    world: WorldCollision {
                        bsp: &map.bsp,
                        scratch: &map.scratch,
                    },
                    players,
                },
                context,
                game,
            ),
            None => movement.predict_vehicle_command(
                command,
                &WithPlayers {
                    world: Void,
                    players,
                },
                context,
                game,
            ),
        });
    }
}

#[path = "bridge_npc_dismember.rs"]
pub(super) mod dismember;
#[path = "bridge_npc_force.rs"]
pub(super) mod force;
#[path = "bridge_npc_sabers.rs"]
mod sabers;

/// Sand moves through bodies, never through map solids or monster-clip brushes.
struct BuriedCollision<'a> {
    everyone: Everyone<'a>,
    map: Option<&'a LoadedMap>,
}
impl sjk_game_jka::pmove::MovementCollision for BuriedCollision<'_> {
    fn point_contents(&self, point: [f32; 3]) -> u32 {
        self.map.map_or(0, |map| {
            WorldCollision {
                bsp: &map.bsp,
                scratch: &map.scratch,
            }
            .point_contents(point)
        })
    }
    fn trace(
        &self,
        start: [f32; 3],
        mins: [f32; 3],
        maxs: [f32; 3],
        end: [f32; 3],
        _mask: u32,
    ) -> sjk_game_jka::pmove::MovementTrace {
        self.everyone.trace(self.map, start, mins, maxs, end, 0x21)
    }
}
