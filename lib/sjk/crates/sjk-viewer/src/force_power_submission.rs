//! Pose-attached MP force effects, sharing actor matrices and the EFX pool.
use super::*;

#[path = "force_sprites.rs"]
pub(crate) mod sprites;

/// Select the activeForcePass graph (cg_players.c:9408-9510).
pub(crate) fn beam(pass: u8) -> Option<&'static str> {
    match pass {
        0 => None,
        1..=2 => Some("force/lightning"),
        3 => Some("force/lightningwide"),
        4..=5 => Some("mp/drain"),
        _ => Some("mp/drainwide"),
    }
}

/// The beam's effect frame, `AnglesToAxis(fAng)` with `fAng` = torso pitch, torso yaw and
/// no roll (`cg_players.c:9408-9510`): forward along the aim, left level, up above it.
///
/// The wide arc spreads its bolts across this frame's left axis (`origin2` spans ±384
/// there, ±2 up), so the axis must stay level. A shortest-arc turn from +X rolls the
/// frame with the aim instead, by up to half a turn when facing back along -X, and the
/// fan tilted, stood upright and flipped as the player turned.
pub(crate) fn beam_rotation(pitch_degrees: f32, yaw_degrees: f32) -> Quat {
    Quat::from_euler(
        glam::EulerRot::ZYX,
        yaw_degrees.to_radians(),
        pitch_degrees.to_radians(),
        0.0,
    )
}

/// `PW_DISINT_4`: set while Grip is held and while Push or Pull shows (`w_force.c`).
const PW_DISINT_4: u32 = 1 << 9;
/// `FP_GRIP` in `forcePowersActive`.
const FP_GRIP: u32 = 1 << 6;
/// `EF_BODYPUSH`.
const EF_BODYPUSH: u32 = 1 << 19;
/// `CLASS_VEHICLE`.
const CLASS_VEHICLE: u8 = 53;

/// The `currentState` fields `CG_Player` reads for one caster's hand and body effects.
///
/// Stock keeps a `centity_t` for the local player too: `CG_AddPacketEntities` rebuilds
/// its state from `cg.predictedPlayerState` every frame (`cg_ents.c:3843-3846`,
/// `BG_PlayerStateToEntityState`), because the server never sends a client its own
/// entity (`sv_snapshot.cpp:642`). Looking every caster up in the snapshot's entity list
/// therefore lost the local player's own beams and puffs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Caster {
    /// `activeForcePass`: 1-3 the Lightning level, 4-6 the Drain level plus 3.
    pub(crate) active_force_pass: u8,
    pub(crate) force_powers_active: u32,
    /// `powerups` as entity bits: any nonzero player deadline sets its bit
    /// (`BG_PlayerStateToEntityState`).
    pub(crate) powerups: u32,
    pub(crate) e_flags: u32,
    pub(crate) npc_class: u8,
    /// This caster is the viewing client.
    pub(crate) local: bool,
    /// The caster has mind-tricked the viewing client (`trickedentindex*`).
    pub(crate) tricks_viewer: bool,
}

impl Caster {
    /// A remote player or NPC from its snapshot entity, seen by client `viewer` whose
    /// `forcePowersActive` is `viewer_powers` (Force Sight defeats a trick).
    pub(crate) fn from_entity(
        state: &sjk_protocol::EntityState,
        viewer: u16,
        viewer_powers: u32,
    ) -> Self {
        Self {
            active_force_pass: state.raw_field(68).unwrap_or(0) as u8,
            force_powers_active: state.force_powers_active(),
            powerups: state.powerups(),
            e_flags: state.e_flags(),
            npc_class: state.npc_class(),
            local: false,
            tricks_viewer: sjk_client::legacy_mind_tricked(
                sjk_client::legacy_entity_trick_targets(state),
                viewer,
                viewer_powers,
            ),
        }
    }

    /// The local player from its player state, taking the fields prediction carries
    /// from `cg.predictedPlayerState` as stock does.
    pub(crate) fn from_player(
        player: &sjk_protocol::PlayerState,
        predicted: Option<&sjk_client::pmove::MovementState>,
    ) -> Self {
        let powerups = player
            .powerups
            .iter()
            .enumerate()
            .filter(|(_, deadline)| **deadline != 0)
            .fold(0, |bits, (index, _)| bits | (1 << index));
        Self {
            active_force_pass: predicted.map_or(player.raw_field(72).unwrap_or(0) as u8, |state| {
                state.active_force_pass
            }),
            force_powers_active: predicted.map_or(player.force_powers_active(), |state| {
                state.force_powers_active
            }),
            powerups,
            e_flags: predicted.map_or(player.entity_flags(), |state| state.entity_flags),
            npc_class: 0,
            local: true,
            // A client's own `trickedentindex` bits are the players it tricked.
            tricks_viewer: false,
        }
    }

    /// The caster drawn as entity `number`: the local player from its player state,
    /// anyone else from the snapshot's entity list.
    pub(crate) fn find(
        snapshot: &Snapshot,
        number: u16,
        predicted: Option<&sjk_client::pmove::MovementState>,
    ) -> Option<Self> {
        let viewer = snapshot.player.client_num();
        if number == viewer {
            return Some(Self::from_player(&snapshot.player, predicted));
        }
        let index = snapshot
            .entities
            .binary_search_by_key(&number, |e| e.number())
            .ok()?;
        let viewer_powers = predicted.map_or(snapshot.player.force_powers_active(), |state| {
            state.force_powers_active
        });
        Some(Self::from_entity(
            &snapshot.entities[index],
            viewer,
            viewer_powers,
        ))
    }
}

/// What `CG_Player` plays at the left hand of a `PW_DISINT_4` caster.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HandEffect {
    None,
    /// `CG_ForcePushBlur` at the hand: Push or Pull.
    Push,
    /// Two `CG_ForceGripEffect` calls.
    Grip,
}

/// The hand and body effects one caster shows this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) beam: Option<&'static str>,
    pub(crate) body_push: bool,
    pub(crate) hand: HandEffect,
}

/// `CG_Player`'s hand and body effects (EternalJK `cg_players.c:11100-11335`, stock code).
/// They all come before the "effects that should be visible during mindtrick" line
/// (`:11351`), so a caster who tricked the viewer still shows them; only
/// `CG_ForcePushBodyBlur` returns for that caster (`:5433-5441`). Grip's puffs are skipped
/// for the local player in first person (`:11236-11237`).
pub(crate) fn select(caster: Caster, third_person: bool) -> Selection {
    let beam = if caster.npc_class == CLASS_VEHICLE {
        None
    } else {
        beam(caster.active_force_pass)
    };
    let hand = if caster.powerups & PW_DISINT_4 == 0 {
        HandEffect::None
    } else if caster.force_powers_active & FP_GRIP == 0 {
        HandEffect::Push
    } else if third_person || !caster.local {
        HandEffect::Grip
    } else {
        HandEffect::None
    };
    Selection {
        beam,
        body_push: caster.e_flags & EF_BODYPUSH != 0 && !caster.tricks_viewer,
        hand,
    }
}

/// Submit effects from the already evaluated hand bolts and body joints.
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    entity: &sjk_runtime::SceneEntity,
    transform: sjk_runtime::Transform,
    snapshot: &Snapshot,
    time: i32,
    now: Instant,
) {
    let number = entity.id.get().saturating_sub(1) as u16;
    if entity.kind != EntityKind::Actor {
        return;
    }
    let Some(caster) = Caster::find(snapshot, number, sinks.predicted_local_state) else {
        return;
    };
    let selection = select(caster, sinks.third_person);
    let actor = &sinks.actor_meshes[mesh];
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let origin = Vec3::from_array(transform.translation);
    // In first person the local body is still posed under the camera, only hidden
    // (`RF_THIRD_PERSON`, `cg_players.c:10351-10359`); stock starts the beam and the
    // puffs at its left hand all the same.
    let left =
        actor.weapon_attachments[1].map(|bolt| saber::world_attachment(origin, rotation, bolt).0);
    // The beam is re-played on the reference 8 ms cadence (`effect_cadence.rs`).
    if let (Some(name), Some(hand)) = (selection.beam, left)
        && sinks.effects.contains_definition(name)
        && sinks.effect_aux.continuous.due(now)
    {
        let pose = entity.sample_pose(i64::from(time));
        let yaw = if caster.local {
            crate::local_actor_state::pose(pose, sinks.predicted_local_state)
                .map_or(snapshot.player.view_angles(), |pose| {
                    pose.view_angles_degrees
                })[1]
        } else {
            pose.map(|pose| pose.view_angles_degrees[1])
                .or_else(|| {
                    snapshot
                        .entities
                        .binary_search_by_key(&number, |e| e.number())
                        .ok()
                        .map(|index| snapshot.entities[index].angular_trajectory_base()[1])
                })
                .unwrap_or(0.0)
        };
        let frame = beam_rotation(actor.angle_controller.torso_pitch_degrees(), yaw);
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            name,
            hand,
            now,
            u32::from(number) ^ time as u32,
            0,
            sinks.game_audio,
            frame,
        );
    }
    let view_left = Vec3::new(-sinks.camera_yaw.sin(), sinks.camera_yaw.cos(), 0.0);
    if selection.body_push {
        for local in actor.force_bones.origins.iter().flatten() {
            let point = origin + rotation * (*local * Vec3::from_array(transform.scale));
            sprites::pair(
                sinks.particles,
                sinks.effects,
                point,
                view_left,
                false,
                time,
                now,
            );
        }
    }
    let (grip, count) = match selection.hand {
        HandEffect::None => return,
        HandEffect::Push => (false, 1),
        HandEffect::Grip => (true, 2),
    };
    if let Some(hand) = left {
        for _ in 0..count {
            sprites::pair(
                sinks.particles,
                sinks.effects,
                hand,
                view_left,
                grip,
                time,
                now,
            );
        }
    }
}

/// `force/confusion_old` over the head of a player the viewer has mind-tricked
/// (`cg_players.c:11414-11457`): `CG_IsMindTricked` on the viewer's snapshot
/// `forceMindtrickTargetIndex*`, which the target's active Force Sight defeats. Stock
/// orients it by the head bolt; the effect spawns on a sphere with `axisFromSphere`,
/// so the body's rotation stands in for that axis.
pub(super) fn submit_confusion(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    entity: &sjk_runtime::SceneEntity,
    transform: sjk_runtime::Transform,
    snapshot: &Snapshot,
    time: i32,
    now: Instant,
) {
    const CONFUSION: &str = "force/confusion_old";
    let number = entity.id.get().saturating_sub(1) as u16;
    if entity.kind != EntityKind::Actor || number == snapshot.player.client_num() {
        return;
    }
    let Ok(index) = snapshot
        .entities
        .binary_search_by_key(&number, |e| e.number())
    else {
        return;
    };
    if !sjk_client::legacy_mind_tricked(
        sjk_client::legacy_player_trick_targets(&snapshot.player),
        number,
        snapshot.entities[index].force_powers_active(),
    ) {
        return;
    }
    let Some(head) = sinks.actor_meshes[mesh].force_bones.head else {
        return;
    };
    if !sinks.effects.contains_definition(CONFUSION) || !sinks.effect_aux.continuous.due(now) {
        return;
    }
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let origin = Vec3::from_array(transform.translation)
        + rotation * (head * Vec3::from_array(transform.scale));
    effect_runtime::spawn_effect(
        sinks.particles,
        sinks.effect_aux,
        sinks.effects,
        sinks.vfs,
        CONFUSION,
        origin,
        now,
        u32::from(number) ^ (time as u32).rotate_left(16),
        0,
        sinks.game_audio,
        rotation,
    );
}

#[cfg(test)]
mod tests {
    use super::{Caster, HandEffect, beam_rotation, select};
    use glam::Vec3;
    use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS, PlayerState, Snapshot};

    const LOCAL: u16 = 3;
    const REMOTE: u16 = 7;

    fn snapshot(player: PlayerState, entities: Vec<EntityState>) -> Snapshot {
        Snapshot {
            message_sequence: 1,
            reliable_acknowledge: 0,
            server_commands: Vec::new(),
            server_time: 1_000,
            delta_from: None,
            flags: 0,
            area_mask: Vec::new(),
            player,
            vehicle_player: None,
            entities,
            consumed_bits: 0,
        }
    }

    fn local_player(active_force_pass: u32) -> PlayerState {
        let mut player = PlayerState::zero();
        player.set_client_num(LOCAL);
        player.set_raw_field(72, active_force_pass);
        player
    }

    fn local_beam(active_force_pass: u32) -> Option<&'static str> {
        let snapshot = snapshot(local_player(active_force_pass), Vec::new());
        let caster = Caster::find(&snapshot, LOCAL, None).expect("the local player is a caster");
        select(caster, false).beam
    }

    /// The server omits the client's own entity, so its beam comes from `activeForcePass`
    /// in the player state, at every level (`cg_players.c:11100-11203`).
    #[test]
    fn own_beam_comes_from_the_player_state() {
        assert_eq!(local_beam(0), None);
        assert_eq!(local_beam(2), Some("force/lightning"));
        assert_eq!(local_beam(3), Some("force/lightningwide"));
        assert_eq!(local_beam(4), Some("mp/drain"));
        assert_eq!(local_beam(5), Some("mp/drain"));
        assert_eq!(local_beam(6), Some("mp/drainwide"));
    }

    fn gripping_player() -> PlayerState {
        let mut player = local_player(0);
        player.powerups[9] = 61_000;
        player.set_raw_field(82, 1 << 6);
        player
    }

    #[test]
    fn own_grip_puffs_only_in_third_person() {
        let snapshot = snapshot(gripping_player(), Vec::new());
        let caster = Caster::find(&snapshot, LOCAL, None).unwrap();
        assert_eq!(select(caster, false).hand, HandEffect::None);
        assert_eq!(select(caster, true).hand, HandEffect::Grip);
    }

    #[test]
    fn own_push_shows_in_first_person() {
        let mut player = local_player(0);
        player.powerups[9] = 1_500;
        player.set_raw_field(17, 1 << 19);
        let snapshot = snapshot(player, Vec::new());
        let selection = select(Caster::find(&snapshot, LOCAL, None).unwrap(), false);
        assert_eq!(selection.hand, HandEffect::Push);
        assert!(selection.body_push);
    }

    fn remote(active_force_pass: u32, tricks_local: bool) -> EntityState {
        let mut state = EntityState::zero(REMOTE, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(68, active_force_pass);
        state.set_raw_field(77, 1 << 9);
        state.set_raw_field(75, 1 << 6);
        state.set_raw_field(19, 1 << 19);
        state.set_raw_field(58, u32::from(tricks_local) << LOCAL);
        state
    }

    /// Others still come from their snapshot entity: the player state is ignored for them.
    #[test]
    fn remote_caster_still_comes_from_the_entity_list() {
        let snapshot = snapshot(local_player(6), vec![remote(2, false)]);
        let caster = Caster::find(&snapshot, REMOTE, None).unwrap();
        let selection = select(caster, false);
        assert_eq!(selection.beam, Some("force/lightning"));
        assert_eq!(
            selection.hand,
            HandEffect::Grip,
            "others' grip shows in first person"
        );
        assert!(selection.body_push);
        assert!(Caster::find(&snapshot, 9, None).is_none());
    }

    /// A trickster's beam and hand puffs stay visible to its victim; only the body push
    /// blur is hidden (`cg_players.c:5433-5441`, `:11351`).
    #[test]
    fn trickster_keeps_beam_but_hides_body_push() {
        let snapshot = snapshot(local_player(0), vec![remote(5, true)]);
        let selection = select(Caster::find(&snapshot, REMOTE, None).unwrap(), false);
        assert_eq!(selection.beam, Some("mp/drain"));
        assert_eq!(selection.hand, HandEffect::Grip);
        assert!(!selection.body_push);
    }

    /// The viewer's active Force Sight sees through the trick (`CG_IsMindTricked`).
    #[test]
    fn force_sight_shows_a_tricksters_body_push() {
        let mut player = local_player(0);
        player.set_raw_field(82, 1 << 14);
        let snapshot = snapshot(player, vec![remote(0, true)]);
        assert!(select(Caster::find(&snapshot, REMOTE, None).unwrap(), false).body_push);
    }

    /// `AngleVectors` forward for Quake pitch (positive down) and yaw, in degrees.
    fn forward(pitch: f32, yaw: f32) -> Vec3 {
        let (sp, cp) = pitch.to_radians().sin_cos();
        let (sy, cy) = yaw.to_radians().sin_cos();
        Vec3::new(cp * cy, cp * sy, -sp)
    }

    #[test]
    fn beam_frame_matches_angles_to_axis() {
        for pitch in [-60.0, -15.0, 0.0, 10.0, 45.0] {
            for yaw in [0.0, 37.0, 90.0, 179.0, 180.0, 181.0, 270.0, 359.0] {
                let frame = beam_rotation(pitch, yaw);
                let (sy, cy) = yaw.to_radians().sin_cos();
                let along = frame * Vec3::X;
                let left = frame * Vec3::Y;
                let up = frame * Vec3::Z;
                assert!(along.distance(forward(pitch, yaw)) < 1e-5);
                assert!(left.distance(Vec3::new(-sy, cy, 0.0)) < 1e-5);
                assert!(up.z > 0.0, "up {up} at pitch {pitch} yaw {yaw}");
            }
        }
    }

    /// `force/lightningwide` spreads bolts to `origin2` (500..524, -384..384, -2..2): the fan's
    /// ends must stay level with each other whichever way the player aims.
    #[test]
    fn wide_arc_stays_level_facing_any_way() {
        for pitch in [-20.0, 5.0, 30.0] {
            for yaw in (0..360).step_by(5) {
                let frame = beam_rotation(pitch, yaw as f32);
                let right_end = frame * Vec3::new(512.0, -384.0, 0.0);
                let left_end = frame * Vec3::new(512.0, 384.0, 0.0);
                assert!(
                    (right_end.z - left_end.z).abs() < 1e-2,
                    "fan tilted at pitch {pitch} yaw {yaw}: {right_end} {left_end}"
                );
            }
        }
    }
}
