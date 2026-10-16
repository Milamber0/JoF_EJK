//! What an NPC sees and hears (`codemp/game/NPC_senses.c`, `NPC_utils.c`'s
//! `CalcEntitySpot`): the spots it looks at on a body, whether a body is in its field of
//! view and in sight (`InFOV`, `CanSee`, `NPC_CheckVisibility`), and the level's alerts —
//! sounds and sights the game raises for NPCs to notice (`AddSoundEvent`, `AddSightEvent`,
//! `ClearPlayerAlertEvents`, `NPC_CheckAlertEvents`).
//!
//! A body is anything with a client, a player or an NPC, as the senses read it
//! ([`Body`]): its box, its eyes, its view, its health and its teams.
//!
//! The alerts are the game's rule, with the reference's capacity (32, `MAX_ALERT_EVENTS`):
//! a full list loses its oldest. No alert is ever found, though: in the reference as built
//! (GCC, the System V ABI) `alertEventLevel_e` has no negative value and is an unsigned
//! int, so `G_CheckSoundEvents`' and `G_CheckSightEvents`' first comparison,
//! `level >= bestAlert` with `bestAlert` at -1, compares with `UINT_MAX` and is never true
//! (`NPC_senses.c:387-389`, `464-466`). This port compares as that build does.
//!
//! Held to `tools/game-oracle/npcthink.c` (`game-npcthink.txt`).

use crate::player_angle_math::{normalized_angle, vector_angles};
use crate::pmove::MovementTrace;

/// `MASK_OPAQUE`: what sight is stopped by (`CONTENTS_SOLID|CONTENTS_SLIME|CONTENTS_LAVA|
/// CONTENTS_TERRAIN`).
pub const MASK_OPAQUE: u32 = 0x1 | 0x2 | 0x2_0000 | 0x1000;
/// `CONTENTS_OPAQUE`: what `G_ClearLOS` traces against.
const CONTENTS_OPAQUE: u32 = 0x8000;
/// `EF_NODRAW`.
pub const EF_NODRAW: u32 = 1 << 8;
/// `ALERT_CLEAR_TIME`: how long an alert lasts.
const ALERT_CLEAR_TIME: i32 = 200;
/// `MAX_ALERT_EVENTS`: the reference's capacity, a rule of the game here.
pub const MAX_ALERT_EVENTS: usize = 32;
/// `Q3_INFINITE`.
const Q3_INFINITE: i32 = 16_777_216;
/// `CLASS_ATST`, whose eyes sit higher.
const CLASS_ATST: i32 = 1;

/// Anything with a client — a player or an NPC — as an NPC's senses and judgement read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Body {
    /// Its entity number.
    pub number: u16,
    /// Whether it is an NPC (`ent->NPC`).
    pub npc: bool,
    /// `r.currentOrigin`, `r.mins`, `r.maxs`.
    pub origin: [f32; 3],
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `ps.viewheight`, `ps.viewangles`.
    pub view_height: i32,
    pub view_angles: [f32; 3],
    /// `renderInfo.eyePoint`, `renderInfo.eyeAngles`: where it looked from and along as its
    /// frame began (`UpdateClientRenderinfo`); zero where nothing set them.
    pub eye_point: [f32; 3],
    pub eye_angles: [f32; 3],
    /// `ent->health`, `ent->flags`, `s.eFlags`.
    pub health: i32,
    pub flags: u32,
    pub entity_flags: u32,
    /// `client->playerTeam`, `client->enemyTeam` (`npcteam_t`), `sess.sessionTeam`.
    pub player_team: i32,
    pub enemy_team: i32,
    pub session_team: i32,
    /// `client->NPC_class`, `s.weapon`.
    pub class: i32,
    pub weapon: i32,
    /// `ent->enemy`.
    pub enemy: Option<u16>,
    /// `client->tempSpectate >= level.time`.
    pub spectating: bool,
    /// Whether an NPC's own `surrenderTime` is running or it is marched (`SCF_FORCED_MARCH`).
    pub surrendering: bool,
    /// `ps.velocity`, and `ps.pm_flags & PMF_DUCKED`.
    pub velocity: [f32; 3],
    pub ducked: bool,
    /// `ps.saberHolstered != 0`, `ps.saberInFlight`.
    pub saber_holstered: bool,
    pub saber_in_flight: bool,
}

/// `spot_t`: where on a body to look.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spot {
    Origin,
    Chest,
    Head,
    HeadLean,
    Legs,
}

/// `CalcEntitySpot` (`NPC_utils.c:42-170`) for a body.
pub fn spot(body: &Body, spot: Spot) -> [f32; 3] {
    match spot {
        Spot::Origin => {
            if body.origin == [0.0; 3] {
                // A brush's centre: its linked box's (one unit of slack each way).
                let low: [f32; 3] =
                    std::array::from_fn(|axis| body.origin[axis] + body.mins[axis] - 1.0);
                let high: [f32; 3] =
                    std::array::from_fn(|axis| body.origin[axis] + body.maxs[axis] + 1.0);
                std::array::from_fn(|axis| low[axis] + 0.5 * (high[axis] - low[axis]))
            } else {
                body.origin
            }
        }
        Spot::Chest | Spot::Head | Spot::HeadLean => {
            let mut point = eyes(body);
            if spot == Spot::Chest && body.class != CLASS_ATST {
                point[2] -= body.maxs[2] * 0.2;
            }
            point
        }
        Spot::Legs => {
            let mut point = body.origin;
            point[2] += body.mins[2] * 0.5;
            point
        }
    }
}

/// `SPOT_HEAD` and `SPOT_HEAD_LEAN`: the eye point where it has one — over the centre of
/// an NPC's box — else the view height above the origin.
fn eyes(body: &Body) -> [f32; 3] {
    let [x, y, z] = body.eye_point;
    if x * x + y * y + z * z != 0.0 {
        let mut point = body.eye_point;
        if body.class == CLASS_ATST {
            point[2] += 28.0;
        }
        if body.npc {
            point[0] = body.origin[0];
            point[1] = body.origin[1];
        }
        point
    } else {
        let mut point = body.origin;
        point[2] += body.view_height as f32;
        point
    }
}

/// `AngleDelta`.
pub fn angle_delta(from: f32, to: f32) -> f32 {
    normalized_angle(from - to)
}

/// `VectorSubtract`.
pub fn subtract(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| a[axis] - b[axis])
}

/// `DistanceSquared` (`VectorLengthSquared` of the difference).
pub fn distance_squared(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = subtract(a, b);
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

/// `InFOV3` (`NPC_senses.c:130-148`): whether `spot` is within `hfov` and `vfov` degrees
/// of the view `angles` from `from`.
pub fn in_fov3(spot: [f32; 3], from: [f32; 3], angles: [f32; 3], hfov: i32, vfov: i32) -> bool {
    let toward = vector_angles(subtract(spot, from));
    angle_delta(angles[0], toward[0]).abs() <= vfov as f32
        && angle_delta(angles[1], toward[1]).abs() <= hfov as f32
}

/// `InFOV2` (`NPC_senses.c:150-168`): a point from a body's head, along its view.
pub fn in_fov2(point: [f32; 3], from: &Body, hfov: i32, vfov: i32) -> bool {
    in_fov3(point, spot(from, Spot::Head), from.view_angles, hfov, vfov)
}

/// `InFOV` (`NPC_senses.c:170-222`): a body's origin, head or legs within `from`'s field of
/// view from its leaning head, along its eyes' angles (`renderInfo.eyeAngles`) where it has
/// them, else its view.
pub fn in_fov(target: &Body, from: &Body, hfov: i32, vfov: i32) -> bool {
    let eyes = spot(from, Spot::HeadLean);
    let angles = if from.eye_angles != [0.0; 3] {
        from.eye_angles
    } else {
        from.view_angles
    };
    [Spot::Origin, Spot::Head, Spot::Legs]
        .into_iter()
        .any(|at| in_fov3(spot(target, at), eyes, angles, hfov, vfov))
}

/// `InVisrange` (`NPC_senses.c:224-270`): a body's origin within `visrange` of `from`'s
/// eyes.
pub fn in_visrange(target: &Body, from: &Body, visrange: f32) -> bool {
    let delta = subtract(spot(target, Spot::Origin), spot(from, Spot::HeadLean));
    delta[0] * delta[0] + delta[1] * delta[1] + delta[2] * delta[2] <= visrange * visrange
}

/// What the senses need of the world: straight traces and the potentially visible set.
pub trait SenseWorld {
    /// `trap->Trace` of a point from `start` to `end`, skipping `pass`.
    fn line(&mut self, start: [f32; 3], end: [f32; 3], pass: u16, mask: u32) -> MovementTrace;
    /// `trap->InPVS`.
    fn in_pvs(&self, from: [f32; 3], to: [f32; 3]) -> bool;
    /// `EntIsGlass`: a breakable pane the eye goes through.
    fn glass(&self, number: u16) -> bool;
}

/// `ShotThroughGlass` for sight (`NPC_combat.c:1095-1109`): through a pane that is not the
/// target, the trace goes on from where it met it.
fn through_glass(
    world: &mut impl SenseWorld,
    trace: MovementTrace,
    target: u16,
    spot: [f32; 3],
    mask: u32,
) -> MovementTrace {
    if trace.entity_number != target && world.glass(trace.entity_number) {
        return world.line(trace.end_position, spot, trace.entity_number, mask);
    }
    trace
}

/// `CanSee` (`NPC_senses.c:61-99`): a clear line from `from`'s leaning head to the target's
/// origin, head or legs, through panes of glass.
pub fn can_see(world: &mut impl SenseWorld, target: &Body, from: &Body) -> bool {
    let eyes = spot(from, Spot::HeadLean);
    [Spot::Origin, Spot::Head, Spot::Legs]
        .into_iter()
        .any(|at| {
            let point = spot(target, at);
            let trace = world.line(eyes, point, from.number, MASK_OPAQUE);
            through_glass(world, trace, target.number, point, MASK_OPAQUE).fraction == 1.0
        })
}

/// `visibility_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Visibility {
    Not,
    Pvs,
    Full360,
    Fov,
    Shoot,
}

/// `NPC_CheckVisibility`'s checks (`CHECK_PVS`, `CHECK_360`, `CHECK_FOV`, `CHECK_VISRANGE`).
pub const CHECK_PVS: u32 = 1;
pub const CHECK_360: u32 = 2;
pub const CHECK_FOV: u32 = 4;
pub const CHECK_SHOOT: u32 = 8;
pub const CHECK_VISRANGE: u32 = 16;

/// An NPC's eyes: its field of view and how far it sees (`stats.hfov`, `vfov`, `visrange`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sight {
    pub hfov: i32,
    pub vfov: i32,
    pub visrange: f32,
}

/// `NPC_CheckVisibility` (`NPC_senses.c:278-350`) of `target` by `me`, without the shot
/// check (`CanShoot`, the NPC plan's combat step): asked for, it answers `Fov` at best.
pub fn check_visibility(
    world: &mut impl SenseWorld,
    target: &Body,
    me: &Body,
    sight: Sight,
    flags: u32,
) -> Visibility {
    if flags == 0 {
        return Visibility::Not;
    }
    if flags & CHECK_PVS != 0 && !world.in_pvs(target.origin, me.origin) {
        return Visibility::Not;
    }
    if flags & (CHECK_360 | CHECK_FOV | CHECK_SHOOT) == 0 {
        return Visibility::Pvs;
    }
    if flags & CHECK_VISRANGE != 0 && !in_visrange(target, me, sight.visrange) {
        return Visibility::Pvs;
    }
    if flags & CHECK_360 != 0 && !can_see(world, target, me) {
        return Visibility::Pvs;
    }
    if flags & (CHECK_FOV | CHECK_SHOOT) == 0 {
        return Visibility::Full360;
    }
    if flags & CHECK_FOV != 0 && !in_fov(target, me, sight.hfov, sight.vfov) {
        return Visibility::Full360;
    }
    Visibility::Fov
}

/// `G_ClearLOS` (`NPC_senses.c:748-775`): a clear line through up to three panes of glass.
pub fn clear_los(world: &mut impl SenseWorld, start: [f32; 3], end: [f32; 3]) -> bool {
    let mut trace = world.line(
        start,
        end,
        crate::npc_spawn::ENTITYNUM_NONE,
        CONTENTS_OPAQUE,
    );
    let mut count = 0;
    while trace.fraction < 1.0 && count < 3 {
        if trace.entity_number < crate::pmove::ENTITY_NUMBER_WORLD
            && world.glass(trace.entity_number)
        {
            trace = world.line(trace.end_position, end, trace.entity_number, MASK_OPAQUE);
            count += 1;
            continue;
        }
        return false;
    }
    trace.fraction == 1.0
}

/// `alertEventType_e`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AlertKind {
    #[default]
    Sight,
    Sound,
}

/// `alertEventLevel_e`: `AEL_MINOR` through `AEL_DANGER_GREAT`.
pub const AEL_MINOR: u32 = 0;
pub const AEL_SUSPICIOUS: u32 = 1;
pub const AEL_DISCOVERED: u32 = 2;
pub const AEL_DANGER: u32 = 3;

/// `alertEvent_t`: something an NPC might notice.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AlertEvent {
    pub position: [f32; 3],
    pub radius: f32,
    /// `alertEventLevel_e`, unsigned as the reference's build has it.
    pub level: u32,
    pub kind: AlertKind,
    /// Who made it, by entity number.
    pub owner: Option<u16>,
    pub light: f32,
    pub add_light: f32,
    pub id: i32,
    pub timestamp: i32,
}

/// The level's alerts (`level.alertEvents`, `numAlertEvents`, `curAlertID`) and the
/// debounce `ClearPlayerAlertEvents` keeps (`eventClearTime`). The slots are the
/// reference's: a removal shifts the ones after it down (`memmove`) and leaves the last as
/// it was, and the clearing pass reads slots past the count — both part of the rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlertEvents {
    slots: [AlertEvent; MAX_ALERT_EVENTS],
    count: usize,
    next_id: i32,
    clear_time: i32,
}

impl AlertEvents {
    /// The alerts standing, oldest first.
    pub fn events(&self) -> &[AlertEvent] {
        &self.slots[..self.count]
    }

    /// `level.curAlertID`: the ID the next alert gets.
    pub fn next_id(&self) -> i32 {
        self.next_id
    }

    /// `eventClearTime`: when the alerts are next cleared (`ClearPlayerAlertEvents` sets it
    /// `ALERT_CLEAR_TIME` on as it clears them).
    pub fn clear_time(&self) -> i32 {
        self.clear_time
    }

    /// `AddSoundEvent` (`NPC_senses.c:601-640`): a sound at `position`, heard within
    /// `radius`; a quiet one (`needs_sight`) only by who can see where it was.
    pub fn add_sound(
        &mut self,
        owner: Option<u16>,
        position: [f32; 3],
        radius: f32,
        level: u32,
        needs_sight: bool,
        level_time: i32,
    ) {
        self.add(
            owner,
            position,
            radius,
            level,
            AlertKind::Sound,
            if needs_sight { 1.0 } else { 0.0 },
            level_time,
        );
    }

    /// `AddSightEvent` (`NPC_senses.c:645-677`).
    pub fn add_sight(
        &mut self,
        owner: Option<u16>,
        position: [f32; 3],
        radius: f32,
        level: u32,
        add_light: f32,
        level_time: i32,
    ) {
        self.add(
            owner,
            position,
            radius,
            level,
            AlertKind::Sight,
            add_light,
            level_time,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        owner: Option<u16>,
        position: [f32; 3],
        radius: f32,
        level: u32,
        kind: AlertKind,
        add_light: f32,
        level_time: i32,
    ) {
        if self.count >= MAX_ALERT_EVENTS && !self.remove_oldest() {
            return;
        }
        // Only a danger may have no owner.
        if owner.is_none() && level < AEL_DANGER {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        self.slots[self.count] = AlertEvent {
            position,
            radius,
            level,
            kind,
            owner,
            light: self.slots[self.count].light,
            add_light,
            id,
            timestamp: level_time,
        };
        self.count += 1;
    }

    /// One alert gone (`NPC_senses.c:697-710`): the rest shifted down over it, the last
    /// slot left as it was; the only one cleared.
    fn remove(&mut self, at: usize) {
        self.count -= 1;
        if self.count > 0 {
            self.slots.copy_within(at + 1.., at);
        } else {
            self.slots[at] = AlertEvent::default();
        }
    }

    /// `RemoveOldestAlert` (`NPC_senses.c:718-745`): the first of the oldest goes. Whether
    /// there is room now.
    fn remove_oldest(&mut self) -> bool {
        let mut oldest = None;
        let mut oldest_time = Q3_INFINITE;
        for (at, event) in self.slots[..self.count].iter().enumerate() {
            if event.timestamp < oldest_time {
                oldest = Some(at);
                oldest_time = event.timestamp;
            }
        }
        if let Some(at) = oldest {
            self.remove(at);
        }
        self.count < MAX_ALERT_EVENTS
    }

    /// `ClearPlayerAlertEvents` (`NPC_senses.c:682-716`), at the start of every frame: the
    /// alerts older than 200 ms go. As in the reference, the one after a removed alert
    /// slides into its place and is not looked at this time, and the pass runs to the
    /// count it began with.
    pub fn clear_old(&mut self, level_time: i32) {
        for at in 0..self.count {
            let event = self.slots[at];
            if event.timestamp != 0 && event.timestamp + ALERT_CLEAR_TIME < level_time {
                self.remove(at);
            }
        }
        if self.clear_time < level_time {
            self.clear_time = level_time + ALERT_CLEAR_TIME;
        }
    }

    /// `G_CheckSoundEvents` and `G_CheckSightEvents`' shared rule: whether an alert takes
    /// precedence over the best so far — compared as the reference's build compares an
    /// unsigned level with an int (-1, then a level) (`NPC_senses.c:387-389`).
    fn better(event: &AlertEvent, best_alert: i32, best_time: i32) -> bool {
        event.level >= best_alert as u32
            || (event.level == best_alert as u32 && event.timestamp >= best_time)
    }

    /// `G_CheckAlertEvents` (`NPC_senses.c:499-552`) for `me`, whose sight and hearing
    /// reach `sight` and `hear`: the index of the alert it notices. `client_zero_alive` is
    /// `g_entities[0].health > 0` — with client 0 dead, nothing is noticed.
    #[allow(clippy::too_many_arguments)]
    pub fn check(
        &mut self,
        world: &mut impl SenseWorld,
        me: &Body,
        sight: Sight,
        hear: f32,
        ignore: Option<usize>,
        needs_owner: bool,
        min_level: u32,
        client_zero_alive: bool,
    ) -> Option<usize> {
        if !client_zero_alive {
            return None;
        }
        let heard = self.check_sounds(world, me, hear, ignore, needs_owner, min_level);
        let seen = self.check_sights(world, me, sight, ignore, needs_owner, min_level);
        let heard_level = heard.map_or(-1, |at| self.slots[at].level as i32);
        match seen {
            Some(at) if self.slots[at].level as i32 > heard_level => {
                // The light it is seen in: its own plus the level's, which multiplayer
                // takes as full (`G_GetLightLevel`).
                self.slots[at].light = self.slots[at].add_light + 255.0;
                Some(at)
            }
            _ => heard,
        }
    }

    /// `G_CheckSoundEvents` (`NPC_senses.c:370-415`).
    fn check_sounds(
        &self,
        world: &mut impl SenseWorld,
        me: &Body,
        hear: f32,
        ignore: Option<usize>,
        needs_owner: bool,
        min_level: u32,
    ) -> Option<usize> {
        let (mut best, mut best_alert, mut best_time) = (None, -1, -1);
        let hear = hear * hear;
        for (at, event) in self.slots[..self.count].iter().enumerate() {
            if Some(at) == ignore
                || event.kind != AlertKind::Sound
                || event.level < min_level
                || (needs_owner && event.owner.is_none())
            {
                continue;
            }
            let distance = distance_squared(event.position, me.origin);
            if distance > hear || distance > event.radius * event.radius {
                continue;
            }
            if event.add_light != 0.0 && !clear_los(world, spot(me, Spot::HeadLean), event.position)
            {
                continue;
            }
            if Self::better(event, best_alert, best_time) {
                (best, best_alert, best_time) = (Some(at), event.level as i32, event.timestamp);
            }
        }
        best
    }

    /// `G_CheckSightEvents` (`NPC_senses.c:434-480`).
    fn check_sights(
        &self,
        world: &mut impl SenseWorld,
        me: &Body,
        sight: Sight,
        ignore: Option<usize>,
        needs_owner: bool,
        min_level: u32,
    ) -> Option<usize> {
        let (mut best, mut best_alert, mut best_time) = (None, -1, -1);
        let see = sight.visrange * sight.visrange;
        for (at, event) in self.slots[..self.count].iter().enumerate() {
            if Some(at) == ignore
                || event.kind != AlertKind::Sight
                || event.level < min_level
                || (needs_owner && event.owner.is_none())
            {
                continue;
            }
            let distance = distance_squared(event.position, me.origin);
            if distance > see || distance > event.radius * event.radius {
                continue;
            }
            if !in_fov2(event.position, me, sight.hfov, sight.vfov)
                || !clear_los(world, spot(me, Spot::HeadLean), event.position)
            {
                continue;
            }
            if Self::better(event, best_alert, best_time) {
                (best, best_alert, best_time) = (Some(at), event.level as i32, event.timestamp);
            }
        }
        best
    }
}
