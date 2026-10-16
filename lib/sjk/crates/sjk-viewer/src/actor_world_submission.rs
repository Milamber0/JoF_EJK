//! Actor, held-weapon, saber, and force-overlay frame submission.
#[path = "first_person_saber.rs"]
pub(crate) mod first_person_saber;

use super::*;

#[path = "flag_carrier.rs"]
pub(crate) mod flags;

#[path = "force_power_submission.rs"]
pub(crate) mod force_powers;

#[path = "thrown_saber.rs"]
mod thrown_saber;

#[path = "actor_model_scale.rs"]
pub(crate) mod model_scale;

#[path = "monster_hold.rs"]
pub(crate) mod monster_hold;

#[path = "player_sprites.rs"]
mod player_sprites;
#[path = "speed_trail.rs"]
pub(crate) mod speed_trail;

#[path = "grapple_rope.rs"]
mod grapple_rope;

#[path = "jetpack_submission.rs"]
mod jetpack_submission;

#[path = "player_sphere_submission.rs"]
mod player_spheres;

struct Sinks<'a> {
    flag_meshes: [Option<usize>; 2],
    shield_mesh: Option<usize>,
    shield_sphere: bool,
    shield_passes: u32,
    /// `g_gametype` from the serverinfo.
    gametype: i32,
    /// The rendered view's origin, which player spheres face.
    view_origin: Vec3,
    jetpack: crate::jetpack::Catalog,
    jetpack_sounds: &'a mut crate::jetpack::Sounds,
    world: &'a sjk_runtime::World,
    actor_meshes: &'a [ActorMesh],
    object_meshes: &'a [StaticModelMesh],
    actor_groups: &'a mut [Vec<ActorInstance>],
    object_groups: &'a mut [Vec<ActorInstance>],
    overrides: &'a mut Vec<entity_materials::OverrideInstance>,
    entity_instances: &'a mut Vec<EntityInstance>,
    saber_hilts: Option<&'a saber::HiltCatalog>,
    /// Who wears which blade skin.
    saber_skins: &'a crate::saber_skins::SaberSkins,
    saber_states: &'a mut saber_trail::StateSlab,
    saber_segments: &'a mut saber_trail::SegmentPool,
    /// Trail edges for held and flying blades; `None` with `cg_saberTrail 0`.
    trail_edges: Option<saber_trail::Edges<'a>>,
    saber_instances: &'a mut Vec<saber::Instance>,
    lights: &'a mut dynamic_lights::PointLightList,
    presentation_time: i64,
    muzzle_effects: &'a sjk_client::LegacyMuzzleEffects,
    particles: &'a mut Vec<Particle>,
    effect_aux: &'a mut effect_aux::Runtime,
    effects: &'a mut EffectLibrary,
    vfs: &'a VirtualFileSystem,
    game_audio: &'a mut Option<GameAudio>,
    force_tracker: &'a sjk_client::LegacyForceOverlayTracker,
    speed_trails: &'a mut speed_trail::Trails,
    /// `centity_t::trickAlpha` per entity.
    trick_fades: &'a mut sjk_client::LegacyTrickFades,
    /// The viewer's `forcePowersActive`, for `CG_IsMindTricked`'s Force Sight check.
    viewer_force_powers_active: u32,
    /// `cg_speedTrail`.
    speed_trail: bool,
    /// `cg_cosmetics`: whose hats and capes are drawn.
    cosmetics: crate::cosmetics::Visibility,
    material_overrides: model_materials::Overrides,
    camera_position: Vec3,
    camera_yaw: f32,
    view_height: f32,
    predicted_force_powers_active: Option<u32>,
    predicted_local_state: Option<&'a sjk_client::pmove::MovementState>,
    predicted_vehicle: Option<crate::vehicle_pose::Predicted>,
    local_equipment: Option<sjk_runtime::HeldEquipment>,
    third_person: bool,
    portal_view: bool,
    entity_view_flags: u32,
    detached_camera: bool,
    detached_flight: bool,
    first_person_saber: bool,
    /// This frame's JA+ grapple hooks, drawn as ropes from their players' hands.
    hooks: grapple_rope::Hooks,
    /// Every player's weapon charge, for the glow on its muzzle.
    charges: crate::charge_flash::Charges,
    /// Client slots muted on this PC (bits): drawn with the default saber colour
    /// and without hat or cape (`muted_players.rs`).
    muted: u32,
    /// The player whose card is up with its outline on (`card_outline.rs`).
    card_outline: Option<u16>,
    /// The players outlined as teammates and enemies (`hud::team_outline`).
    team_outline: crate::hud::team_outline::Groups,
}

/// Submit all presented actor-like entities without allocating frame storage.
pub(crate) fn submit(
    gpu: &mut GpuState,
    local_entity_id: Option<u64>,
    fallback_mesh: Option<usize>,
    presentation_time: i64,
    visual_now: Instant,
    game_audio: &mut Option<GameAudio>,
) -> usize {
    let snapshot = first_person_view::presented_snapshot(
        gpu.live_session.as_ref(),
        gpu.demo_session.as_ref(),
        presentation_time as i32,
    );
    let active_world = gpu
        .demo_session
        .as_ref()
        .map_or(&gpu.live_world, demo_playback::Session::world);
    let game_state = gpu
        .live_session
        .as_ref()
        .map(ClientSession::game_state)
        .or_else(|| {
            gpu.demo_session
                .as_ref()
                .map(demo_playback::Session::game_state)
        });
    let local_mesh = gpu.actor_meshes.iter().position(|mesh| {
        mesh.entity_id.map(|id| id.get()) == local_entity_id && mesh.entity_id.is_some()
    });
    let local_equipment = local_entity_id
        .and_then(|id| active_world.entity(sjk_runtime::EntityId::new(id)))
        .and_then(|entity| {
            local_actor_state::equipment(
                entity.equipment(),
                gpu.local_prediction
                    .predicted_state()
                    .filter(|_| !gpu.detached_camera),
                snapshot.map(|s| s.player.saber_move()),
            )
        });
    let saber_body = first_person_saber::visible(
        true,
        gpu.third_person,
        gpu.detached_camera,
        local_equipment.map(|held| held.weapon),
        snapshot.map(|s| &s.player),
    ) && gpu
        .local_prediction
        .predicted_state()
        .is_none_or(|p| p.health > 0 && p.entity_flags & 2 == 0)
        && local_mesh.is_some_and(|index| {
            !gpu.actor_meshes[index].surfaces.weapon_lost
                && gpu.actor_meshes[index].body_identity.is_none()
                // A rig model is one glTF surface: no head to hide, so no body view.
                && gpu.actor_meshes[index].preview.rig.is_none()
        });
    for (index, mesh) in gpu.actor_meshes.iter_mut().enumerate() {
        mesh.surfaces.first_person_saber(
            saber_body && local_mesh == Some(index),
            &mesh.preview.mesh.hierarchy,
        );
    }
    let aura_shell = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_auraShell"))
        .unwrap_or(1)
        != 0;
    let combined_protect_absorb = gpu
        .console
        .as_ref()
        .and_then(|console| console.bool_cvar("cg_spprotabscolor"))
        .unwrap_or(true);
    // 1 draws multiplayer's sphere around a shield hit; 0 hugs the body like single player.
    let shield_sphere = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_shieldSphere"))
        .unwrap_or(0)
        != 0;
    let shield_passes = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_shieldBrightness"))
        .unwrap_or(2)
        .clamp(1, 12) as u32;
    let trails = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_saberTrail"))
        .unwrap_or(1)
        != 0;
    let speed_trail = gpu
        .console
        .as_ref()
        .and_then(|console| console.integer_cvar("cg_speedTrail"))
        .unwrap_or(1)
        != 0;
    let cosmetics = crate::cosmetics::Visibility::from_cvar(
        gpu.console
            .as_ref()
            .and_then(|console| console.integer_cvar(crate::cosmetics::VISIBILITY_CVAR))
            .unwrap_or(1),
    );
    let saber_contact = gpu.effect_aux.saber_contacts.enabled;
    let detached_flight = gpu.free_camera_active();
    let gametype = game_state
        .and_then(|game| game.config_string(0))
        .and_then(|info| sjk_client::LegacyClientInfo::new(info).integer("g_gametype"))
        .unwrap_or(0);
    let mut sinks = Sinks {
        flag_meshes: gpu.pickup_catalog.carrier_meshes[flags::model_set(gametype)],
        shield_mesh: gpu
            .object_meshes
            .iter()
            .position(|mesh| mesh.appearance.model == "models/weaphits/testboom.md3"),
        shield_sphere,
        shield_passes,
        gametype,
        view_origin: gpu.view_origin,
        jetpack: gpu.jetpack,
        jetpack_sounds: &mut gpu.jetpack_sounds,
        world: active_world,
        actor_meshes: &gpu.actor_meshes,
        object_meshes: &gpu.object_meshes,
        actor_groups: &mut gpu.actor_groups,
        object_groups: &mut gpu.object_groups,
        overrides: &mut gpu.pickup_override_instances,
        entity_instances: &mut gpu.entity_instances,
        saber_hilts: gpu.saber_hilts.as_ref(),
        saber_skins: &gpu.saber_skins,
        saber_states: &mut gpu.saber_states,
        saber_segments: &mut gpu.saber_trail_segments,
        trail_edges: trails.then(|| {
            saber_trail::Edges::new(saber_contact.then_some((&gpu.bsp, &mut gpu.trace_scratch)))
        }),
        saber_instances: &mut gpu.saber_instances,
        lights: &mut gpu.dynamic_lights,
        presentation_time,
        muzzle_effects: &gpu.muzzle_effects,
        particles: &mut gpu.particles,
        effect_aux: &mut gpu.effect_aux,
        effects: &mut gpu.effects,
        vfs: gpu
            .vfs
            .as_deref()
            .expect("sessions retain their mounted VFS"),
        game_audio,
        force_tracker: &gpu.force_overlays,
        speed_trails: &mut gpu.speed_trails,
        trick_fades: &mut gpu.trick_fades,
        viewer_force_powers_active: gpu
            .local_prediction
            .predicted_state()
            .map(|state| state.force_powers_active)
            .or_else(|| snapshot.map(|snapshot| snapshot.player.force_powers_active()))
            .unwrap_or(0),
        speed_trail,
        cosmetics,
        material_overrides: gpu.model_material_overrides,
        camera_position: gpu.camera_position,
        camera_yaw: gpu.camera_yaw,
        view_height: gpu.local_prediction.view_height(),
        predicted_force_powers_active: gpu
            .local_prediction
            .predicted_state()
            .map(|state| state.force_powers_active),
        predicted_local_state: gpu
            .local_prediction
            .predicted_state()
            .filter(|_| !detached_flight),
        predicted_vehicle: gpu
            .live_session
            .as_ref()
            .and_then(|_| gpu.local_prediction.vehicle_pose()),
        local_equipment,
        third_person: gpu.third_person,
        portal_view: gpu.scene_views.has_portal_view(),
        entity_view_flags: 0,
        detached_camera: gpu.detached_camera,
        detached_flight,
        first_person_saber: saber_body,
        hooks: grapple_rope::Hooks::collect(snapshot, game_state, presentation_time as i32),
        charges: crate::charge_flash::Charges::collect(
            snapshot,
            gpu.live_session
                .as_ref()
                .and_then(|_| gpu.local_prediction.predicted_state()),
        ),
        muted: gpu.muted_players.slots().muted,
        card_outline: gpu.hud.card.outline().map(|(client, _)| client),
        team_outline: gpu.hud.team_outline.groups(),
    };
    let thrown = snapshot
        .zip(game_state)
        .map(|(snapshot, game)| sjk_client::LegacyThrownSabers::new(snapshot, game));
    let mut overlay_count = 0;
    for entity in active_world
        .entities()
        .filter(|entity| {
            !matches!(
                entity.kind,
                EntityKind::Projectile | EntityKind::Mover | EntityKind::Item | EntityKind::Effect
            )
        })
        .take(1_024)
    {
        sinks.entity_view_flags = u16::try_from(entity.id.get().saturating_sub(1))
            .ok()
            .map_or(0, |number| {
                crate::actor_instance::scene_flags(game_state, snapshot, number)
            });
        let mut transform = entity.sample(presentation_time);
        if matches!(entity.kind, EntityKind::Actor | EntityKind::Corpse) {
            overlay_count += submit_actor(
                &mut sinks,
                entity,
                &mut transform,
                snapshot,
                game_state,
                local_entity_id,
                fallback_mesh,
                presentation_time,
                visual_now,
                aura_shell,
                combined_protect_absorb,
            );
        } else if submit_limb(&mut sinks, entity, &transform, visual_now) {
            // A cut-off limb draws its own copy of the owner (`dismember`).
        } else if thrown_saber::submit(&mut sinks, thrown.as_ref(), entity, transform) {
            // The flying hilt is owned by this branch, including its blades.
        } else if let Some(mesh) = entity.appearance().and_then(|appearance| {
            sinks
                .object_meshes
                .iter()
                .position(|mesh| &mesh.appearance == appearance)
        }) {
            let mut instance =
                ActorInstance::new(transform.translation, transform.rotation, transform.scale);
            instance.view_flags = sinks.entity_view_flags;
            sinks.object_groups[mesh].push(instance);
        }
    }
    overlay_count
}

#[allow(clippy::too_many_arguments)]
fn submit_actor(
    sinks: &mut Sinks<'_>,
    entity: &sjk_runtime::SceneEntity,
    transform: &mut sjk_runtime::Transform,
    snapshot: Option<&Snapshot>,
    game_state: Option<&GameState>,
    local_entity_id: Option<u64>,
    fallback_mesh: Option<usize>,
    presentation_time: i64,
    visual_now: Instant,
    aura_shell: bool,
    combined_protect_absorb: bool,
) -> usize {
    let draw_actor =
        sinks.third_person || sinks.detached_camera || Some(entity.id.get()) != local_entity_id;
    if Some(entity.id.get()) == local_entity_id && !sinks.detached_camera {
        let (translation, rotation) =
            camera::local_actor_root(sinks.camera_position, sinks.view_height, sinks.camera_yaw);
        transform.translation = translation;
        transform.rotation = rotation;
    }
    let mesh = sinks
        .actor_meshes
        .iter()
        .position(|mesh| mesh.entity_id == Some(entity.id))
        .or_else(|| {
            (entity.kind != EntityKind::Corpse)
                .then_some(fallback_mesh)
                .flatten()
        });
    transform.rotation = actor_pose::world_rotation(
        mesh.and_then(|index| sinks.actor_meshes.get(index)),
        transform.rotation,
    );
    let state = snapshot.and_then(|snapshot| {
        snapshot
            .entities
            .iter()
            .find(|state| u64::from(state.number()) + 1 == entity.id.get())
    });
    crate::vehicle_pose::place(
        sinks.world,
        sinks.actor_meshes,
        mesh,
        entity,
        transform,
        snapshot,
        state,
        Some(entity.id.get()) == local_entity_id,
        presentation_time,
        sinks.predicted_vehicle,
    );
    if let Some(state) = state {
        model_scale::apply(transform, state.model_scale_percent(), state.npc_class());
    }
    // `EF_DISINTEGRATION`: `CG_Disintegration` draws the body and nothing else
    // (`cg_players.c`, `cg_ents.c`).
    let meshes = sinks.actor_meshes;
    if let Some(index) = mesh
        && let Some(burning) = meshes[index].disintegration.as_ref()
    {
        submit_disintegration(
            sinks, index, burning, entity, transform, draw_actor, visual_now,
        );
        return 0;
    }
    // A monster's victim is drawn in its hand or jaw (`cg_players.c:9220-9244`).
    let local = Some(entity.id.get()) == local_entity_id;
    let trick = trick_fade(sinks, snapshot, state, local, presentation_time);
    if let Some(snapshot) = snapshot.filter(|_| local || state.is_some()) {
        monster_hold::place(
            sinks.world,
            sinks.actor_meshes,
            snapshot,
            state.filter(|_| !local),
            transform,
            presentation_time,
        );
    }
    if let Some(snapshot) = snapshot {
        player_sprites::submit(
            sinks,
            entity.kind,
            transform.translation,
            snapshot,
            state,
            local,
            draw_actor,
            visual_now,
        );
    }
    // A JA+ hook's rope runs from the right hand (`cg_ents.c:2785-2792`).
    if entity.kind == EntityKind::Actor
        && let Some(attachment) =
            mesh.and_then(|mesh| sinks.actor_meshes[mesh].weapon_attachments[0])
        && let Ok(client) = u16::try_from(entity.id.get().saturating_sub(1))
    {
        let (hand, _) = saber::world_attachment(
            Vec3::from_array(transform.translation),
            weapon_view::actor_world_rotation(transform.rotation),
            attachment,
        );
        grapple_rope::submit(sinks, client, hand, visual_now);
    }
    if let (Some(mesh), Some(snapshot)) = (mesh, snapshot) {
        flags::submit(
            sinks,
            mesh,
            entity,
            *transform,
            snapshot,
            presentation_time,
            draw_actor,
        );
        force_powers::submit(
            sinks,
            mesh,
            entity,
            *transform,
            snapshot,
            presentation_time as i32,
            visual_now,
        );
    }
    let mut equipment = if Some(entity.id.get()) == local_entity_id {
        sinks.local_equipment
    } else {
        entity.equipment()
    };
    if let Some(body) = mesh.and_then(|index| sinks.actor_meshes[index].body_identity.as_ref()) {
        // CG_BodyQueueCopy removes dropped weapons above WP_BRYAR_PISTOL.
        equipment = equipment
            .filter(|_| (1..=4).contains(&body.weapon))
            .map(|mut held| {
                held.weapon = body.weapon as u8;
                held.kind = if body.weapon == 3 {
                    sjk_runtime::HeldItemKind::EnergyBlade
                } else {
                    sjk_runtime::HeldItemKind::Ranged
                };
                held.active = false;
                held.secondary_active = false;
                held.primary_in_flight = false;
                held
            });
    }
    // The weapon went with a cut-off arm, hand or waist (`CG_General`).
    if mesh.is_some_and(|index| sinks.actor_meshes[index].surfaces.weapon_lost) {
        equipment = None;
    }
    // A player muted on this PC, or their body, has the default blade colour.
    let client = match mesh.and_then(|index| sinks.actor_meshes[index].body_identity.as_ref()) {
        Some(body) => u64::from(body.client_num) + 1,
        None if entity.kind == EntityKind::Actor => entity.id.get(),
        None => 0,
    };
    let muted = crate::muted_players::entity_muted(sinks.muted, client);
    if muted {
        equipment = equipment.map(crate::muted_players::equipment);
    }
    // A hidden trickster keeps only a saber in flight (`cg_players.c:11732-11746`).
    if let Some((equipment, held_mesh)) = equipment
        .filter(|held| !trick.hidden || held.primary_in_flight)
        .zip(mesh)
    {
        submit_equipment(
            sinks,
            entity,
            equipment,
            held_mesh,
            *transform,
            draw_actor,
            local,
            trick.fading.then_some(trick.alpha),
            presentation_time,
            visual_now,
        );
    }
    let ghosts = match (mesh, snapshot) {
        (Some(_), Some(snapshot)) => {
            speed_ghosts(sinks, transform, snapshot, state, local, trick.fading)
        }
        _ => [None; 2],
    };
    let number = u16::try_from(entity.id.get().saturating_sub(1)).unwrap_or(u16::MAX);
    let e_flags = if local {
        snapshot.map_or(0, |snapshot| snapshot.player.entity_flags())
    } else {
        state.map_or(0, sjk_protocol::EntityState::e_flags)
    };
    if entity.kind == EntityKind::Actor
        && let (Some(mesh), Some(snapshot)) = (mesh, snapshot)
    {
        let tricked = !local
            && state.is_some_and(|state| {
                sjk_client::legacy_mind_tricked(
                    sjk_client::legacy_entity_trick_targets(state),
                    snapshot.player.client_num(),
                    sinks.viewer_force_powers_active,
                )
            });
        jetpack_submission::submit(
            sinks,
            jetpack_submission::Body {
                mesh,
                number,
                transform: *transform,
                e_flags,
                tricked,
                hidden: trick.hidden,
                fade: trick.fading.then_some(trick.alpha),
                draw_actor,
                muted,
            },
            presentation_time as i32,
            visual_now,
        );
    }
    // Everything after stock's mind-trick cut-off is skipped for a hidden trickster:
    // the body, its afterimages and its shells (`cg_players.c:11351-11356`).
    if trick.hidden {
        return 0;
    }
    // An electrified player crackles (`electrify.rs`), the local one in first person too.
    if entity.kind == EntityKind::Actor
        && !muted
        && let Some(snapshot) = snapshot
    {
        let until = if local {
            snapshot.player.electrify_time()
        } else {
            state.map_or(0, sjk_protocol::EntityState::emplaced_owner)
        };
        let now = presentation_time as i32;
        if until > now
            && sinks.effect_aux.continuous.due(visual_now)
            && crate::electrify::crackles(until, now, crate::electrify::draws(number, now))
            && let Some(audio) = sinks.game_audio.as_mut()
        {
            audio.play_on_channel(
                crate::electrify::CRACKLE,
                1.0,
                transform.translation,
                sjk_audio::SourceId(u32::from(number)),
                sjk_audio::ChannelId(0),
            );
        }
    }
    // `CG_DrawPlayerSphere`, the local player's included in first person.
    if entity.kind == EntityKind::Actor
        && let Some(snapshot) = snapshot
    {
        let powerups = if local {
            (0..16).fold(0, |bits, powerup| {
                bits | u32::from(snapshot.player.powerups[powerup] != 0) << powerup
            })
        } else {
            state.map_or(0, sjk_protocol::EntityState::powerups)
        };
        player_spheres::submit(
            sinks,
            Vec3::from_array(transform.translation),
            e_flags,
            powerups,
        );
    }
    if let (Some(mesh), Some(snapshot)) = (mesh, snapshot) {
        force_powers::submit_confusion(
            sinks,
            mesh,
            entity,
            *transform,
            snapshot,
            presentation_time as i32,
            visual_now,
        );
    }
    let saber_body = local && sinks.first_person_saber;
    if (draw_actor || saber_body || sinks.portal_view)
        && let Some(mesh) = mesh
    {
        let mut instance = ActorInstance::new(
            transform.translation,
            weapon_view::actor_world_rotation(transform.rotation).to_array(),
            transform.scale,
        )
        .with_entity_color(entity.color());
        let inside_body = local
            && sinks.detached_flight
            && crate::free_camera::inside_body(
                sinks.camera_position,
                Vec3::from_array(transform.translation),
                sinks.view_height,
            );
        // The first-person saber body is never on with a detached camera, so it never
        // meets the free camera's hidden body.
        instance.view_flags =
            sinks.entity_view_flags | u32::from((!draw_actor && !saber_body) || inside_body);
        if trick.fading {
            // `RF_FORCE_ENT_ALPHA` at `trickAlpha` (`cg_players.c:11358-11367`).
            if sinks.overrides.len() < sinks.overrides.capacity() {
                sinks.overrides.push(entity_materials::OverrideInstance {
                    mesh: entity_materials::OverrideMesh::Actor(mesh),
                    material: None,
                    instance: instance.with_forced_alpha(trick.alpha),
                    no_depth: false,
                    forced_alpha: true,
                });
            }
        } else {
            sinks.actor_groups[mesh].push(instance);
            // A player outline's mask: the body again, drawn only into the mask; the
            // card's outline before a team's.
            let client = entity.id.get().wrapping_sub(1);
            let outline = if sinks
                .card_outline
                .is_some_and(|card| u64::from(card) == client)
            {
                Some(crate::card_outline::Group::Card)
            } else {
                sinks.team_outline.group(client)
            };
            if draw_actor
                && let Some(group) = outline
                && sinks.overrides.len() < sinks.overrides.capacity()
            {
                sinks.overrides.push(entity_materials::OverrideInstance {
                    mesh: entity_materials::OverrideMesh::Actor(mesh),
                    material: Some(sinks.material_overrides.outline(group)),
                    instance,
                    no_depth: false,
                    forced_alpha: false,
                });
            }
        }
        // JoF EJK's hat and cape, on the body's bolts (`CG_Player`); none on a
        // player muted on this PC.
        if entity.kind == EntityKind::Actor && !muted {
            // `EF_DEAD`; the local player's flags are its player state's.
            const EF_DEAD: u32 = 1 << 1;
            let flags = if local {
                snapshot.map(|snapshot| snapshot.player.entity_flags())
            } else {
                state.map(sjk_protocol::EntityState::e_flags)
            };
            crate::cosmetics::actors::submit(
                &sinks.actor_meshes[mesh].cosmetics,
                &crate::cosmetics::actors::Frame {
                    transform: *transform,
                    visibility: sinks.cosmetics,
                    local,
                    draw_actor,
                    view_flags: sinks.entity_view_flags,
                    tricked: trick.fading,
                    dead: flags.is_some_and(|flags| flags & EF_DEAD != 0),
                },
                sinks.object_groups,
            );
        }
        // The copies share the actor's pose, scale, colour and view flags.
        for ghost in ghosts.into_iter().flatten() {
            if sinks.overrides.len() == sinks.overrides.capacity() {
                break;
            }
            let mut copy = instance.with_forced_alpha(ghost.alpha);
            copy.position = ghost.origin.to_array();
            sinks.overrides.push(entity_materials::OverrideInstance {
                mesh: entity_materials::OverrideMesh::Actor(mesh),
                material: None,
                instance: copy,
                no_depth: false,
                forced_alpha: true,
            });
        }
        sinks.actor_meshes[mesh]
            .retained_pose
            .mark_drawn(entity.id, presentation_time);
        if let (Some(snapshot), Some(game_state)) = (snapshot, game_state) {
            return force_overlay_submission::submit(
                sinks.overrides,
                mesh,
                instance,
                entity.id,
                snapshot,
                game_state,
                sinks.force_tracker,
                sinks.material_overrides,
                presentation_time as i32,
                sinks.third_person,
                aura_shell,
                combined_protect_absorb,
                sinks.predicted_force_powers_active,
                sinks.shield_mesh,
                sinks.shield_sphere,
                sinks.shield_passes,
            );
        }
    } else if draw_actor {
        sinks.entity_instances.push(EntityInstance {
            position: transform.translation,
            kind: 0,
            size: 1.0,
            alpha: 1.0,
            uv_rect: [0.0, 0.0, 1.0, 1.0],
            color: [1.0; 4],
            direction: [0.0; 3],
            rotation: 0.0,
            uv_transform: [1.0, 1.0, 0.0, 0.0],
        });
    }
    0
}

/// Run `CG_Player`'s mind-trick fade for one actor. The local player never tricks
/// itself; others fade while they trick the viewer (`cg_players.c:10191-10345`).
fn trick_fade(
    sinks: &mut Sinks<'_>,
    snapshot: Option<&Snapshot>,
    state: Option<&sjk_protocol::EntityState>,
    local: bool,
    presentation_time: i64,
) -> sjk_client::LegacyTrickFade {
    let (Some(snapshot), Some(state)) = (snapshot, state) else {
        return sjk_client::LegacyTrickFade::OPAQUE;
    };
    if local {
        return sjk_client::LegacyTrickFade::OPAQUE;
    }
    let tricked = sjk_client::legacy_mind_tricked(
        sjk_client::legacy_entity_trick_targets(state),
        snapshot.player.client_num(),
        sinks.viewer_force_powers_active,
    );
    sinks
        .trick_fades
        .advance(state.number(), tricked, presentation_time as i32)
}

/// Advance the Force Speed trail of one actor that stock passes through
/// `CG_Player`. The local player is `cg.predictedPlayerEntity`, whose state
/// comes from the predicted player state; others use their snapshot state.
fn speed_ghosts(
    sinks: &mut Sinks<'_>,
    transform: &sjk_runtime::Transform,
    snapshot: &Snapshot,
    state: Option<&sjk_protocol::EntityState>,
    local: bool,
    trick_fading: bool,
) -> [Option<speed_trail::Ghost>; 2] {
    // `PW_SPEED`, as `BG_PlayerStateToEntityState` maps powerups to bits.
    const PW_SPEED: usize = 10;
    let local_client = snapshot.player.client_num();
    let (number, velocity, trailing) = if local {
        let velocity = sinks
            .predicted_local_state
            .map_or_else(|| snapshot.player.velocity(), |state| state.velocity);
        let trailing = snapshot.player.powerups[PW_SPEED] != 0;
        (local_client, velocity, trailing)
    } else if let Some(state) = state {
        // Stock's `doAlpha`: the trick fade, out and back in (`cg_players.c:10842`).
        let trailing = state.powerups() & (1 << PW_SPEED) != 0 && !trick_fading;
        (state.number(), state.trajectory_delta(), trailing)
    } else {
        return [None; 2];
    };
    sinks.speed_trails.advance(
        number,
        Vec3::from_array(transform.translation),
        Vec3::from_array(velocity),
        trailing && sinks.speed_trail,
    )
}

#[allow(clippy::too_many_arguments)]
fn submit_equipment(
    sinks: &mut Sinks<'_>,
    entity: &sjk_runtime::SceneEntity,
    equipment: sjk_runtime::HeldEquipment,
    actor_mesh: usize,
    transform: sjk_runtime::Transform,
    draw_actor: bool,
    local: bool,
    forced_alpha: Option<u8>,
    presentation_time: i64,
    visual_now: Instant,
) {
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let origin = Vec3::from_array(transform.translation);
    if saber_submission::submit(
        entity.id.get(),
        sinks.saber_skins.get(entity.id.get()),
        std::array::from_fn(|hand| {
            let mesh = &sinks.actor_meshes[actor_mesh];
            mesh.saber_names[hand]
                .as_deref()
                .zip(mesh.weapon_attachments[hand])
        }),
        equipment,
        origin,
        rotation,
        sinks.saber_hilts,
        sinks.saber_states,
        sinks.saber_segments,
        sinks.object_groups,
        sinks.saber_instances,
        presentation_time,
        sinks.trail_edges.as_mut(),
        sinks.lights,
    ) {
        return;
    }
    // The local first-person player's model carries `RF_THIRD_PERSON`
    // (`cg_players.c:8901-8911`, sabers excepted), so its bolted world gun is
    // not drawn and the world flash is skipped (`cg_weapons.c:701-703`); the
    // view model and its `tag_flash` stand in (`first_person_weapon.rs`).
    if !draw_actor && !sinks.portal_view {
        return;
    }
    let Some(attachment) = sinks.actor_meshes[actor_mesh].weapon_attachments[0] else {
        return;
    };
    let Some(model) = weapon_view::held_model(equipment.weapon) else {
        return;
    };
    let (grip, weapon_rotation) = saber::world_attachment(origin, rotation, attachment);
    let Some(weapon_mesh) = sinks
        .object_meshes
        .iter()
        .position(|mesh| mesh.appearance.variant.is_empty() && mesh.appearance.model == model)
    else {
        return;
    };
    let mut instance = ActorInstance::new(grip.to_array(), weapon_rotation.to_array(), [1.0; 3]);
    let inside_body = local
        && sinks.detached_flight
        && crate::free_camera::inside_body(
            sinks.camera_position,
            Vec3::from_array(transform.translation),
            sinks.view_height,
        );
    instance.view_flags = sinks.entity_view_flags | u32::from(!draw_actor || inside_body);
    match forced_alpha {
        // The gun is a bolt-on of the body's Ghoul2 instance in stock, so it fades with it.
        Some(alpha) if sinks.overrides.len() < sinks.overrides.capacity() => {
            sinks.overrides.push(entity_materials::OverrideInstance {
                mesh: entity_materials::OverrideMesh::Object(weapon_mesh),
                material: None,
                instance: instance.with_forced_alpha(alpha),
                no_depth: false,
                forced_alpha: true,
            });
        }
        Some(_) => {}
        None => sinks.object_groups[weapon_mesh].push(instance),
    }
    // The main first-person flash already owns event/audio spawning. A
    // second view must never advance effects or play a second sound.
    if !draw_actor {
        return;
    }
    let client = u16::try_from(entity.id.get().saturating_sub(1)).ok();
    let request = client.and_then(|client| sinks.muzzle_effects.request(client));
    let socket = sinks.object_meshes[weapon_mesh]
        .flash_bolt
        .and_then(|flash| muzzle_flash::world_socket(flash, grip, weapon_rotation));
    if let (Some(charge), Some(socket)) = (client.and_then(|c| sinks.charges.of(c)), socket) {
        crate::charge_flash::push(
            sinks.particles,
            sinks.effects,
            charge,
            Vec3::from_array(socket.origin),
            presentation_time as i32,
            visual_now,
        );
    }
    if let (Some(request), Some(socket)) = (request, socket) {
        muzzle_effects::spawn(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            sinks.game_audio,
            request,
            socket,
            local,
            visual_now,
            presentation_time as i32,
        );
    }
}

/// `CG_Disintegration` (`cg_ents.c`): the burning pass with `gfx/effects/burn`, the
/// body eaten away from the hit point, and puffs of `disruptor/death_smoke` from the
/// lower back for the first second. A player is gone after 1.5 s; a body draws until
/// the server frees it.
fn submit_disintegration(
    sinks: &mut Sinks<'_>,
    mesh: usize,
    state: &crate::disintegration::State,
    entity: &sjk_runtime::SceneEntity,
    transform: &sjk_runtime::Transform,
    draw_actor: bool,
    visual_now: Instant,
) {
    use crate::disintegration::{PLAYER_MILLIS, RF_DISINTEGRATE1, RF_DISINTEGRATE2, SMOKE_EFFECT};
    let now = sinks.presentation_time;
    if entity.kind == EntityKind::Actor && now - state.started > PLAYER_MILLIS {
        return;
    }
    if !(draw_actor || sinks.portal_view) {
        return;
    }
    let rotation = weapon_view::actor_world_rotation(transform.rotation);
    let mut body = ActorInstance::new(transform.translation, rotation.to_array(), transform.scale)
        .with_entity_color(entity.color());
    body.view_flags = sinks.entity_view_flags | u32::from(!draw_actor);
    let mut burn = body;
    state.mark(&mut burn, RF_DISINTEGRATE2, now);
    if sinks.overrides.len() < sinks.overrides.capacity() {
        sinks.overrides.push(entity_materials::OverrideInstance {
            mesh: entity_materials::OverrideMesh::Actor(mesh),
            material: Some(sinks.material_overrides.disruptor_burn),
            instance: burn,
            no_depth: false,
            forced_alpha: false,
        });
    }
    state.mark(&mut body, RF_DISINTEGRATE1, now);
    sinks.actor_groups[mesh].push(body);
    sinks.actor_meshes[mesh]
        .retained_pose
        .mark_drawn(entity.id, now);
    let Some(lumbar) = sinks.actor_meshes[mesh].force_bones.lumbar else {
        return;
    };
    if !state.smoke_due(now) {
        return;
    }
    // `fxOrg`: the lower_lumbar bolt, 18 units toward the viewer, up or down by up
    // to 20; one puff, and a second half the time.
    let local =
        Vec3::from_array(crate::bolt::column(&lumbar, 3)) * Vec3::from_array(transform.scale);
    let yaw = sinks.camera_yaw;
    let toward_viewer = Vec3::new(yaw.cos(), yaw.sin(), 0.0);
    let seed = (entity.id.get() as u32).wrapping_mul(0x9e37_79b9) ^ (now as u32);
    let unit = |salt: u32| {
        let mixed = seed
            .wrapping_add(salt)
            .wrapping_mul(0x85eb_ca6b)
            .rotate_left(13);
        (mixed >> 8) as f32 / (1 << 24) as f32
    };
    let origin = Vec3::from_array(transform.translation) + rotation * local - toward_viewer * 18.0
        + Vec3::Z * ((unit(1) * 2.0 - 1.0) * 20.0);
    let puffs = if unit(2) > 0.5 { 2 } else { 1 };
    for puff in 0..puffs {
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            SMOKE_EFFECT,
            origin,
            visual_now,
            seed.wrapping_add(puff),
            0,
            sinks.game_audio,
            combat_effects::rotation_from_direction([0.0, 1.0, 0.0]),
        );
    }
}

/// A cut-off limb (`CG_General`'s client-limb case): its own mesh, rooted at the limb
/// surface and pivoting about its bone at the entity's origin, smoking from the cut
/// while it flies. Returns whether `entity` was a limb this viewer draws.
fn submit_limb(
    sinks: &mut Sinks<'_>,
    entity: &sjk_runtime::SceneEntity,
    transform: &sjk_runtime::Transform,
    visual_now: Instant,
) -> bool {
    let meshes = sinks.actor_meshes;
    let Some((index, limb)) = meshes.iter().enumerate().find_map(|(index, mesh)| {
        mesh.limb
            .as_ref()
            .filter(|_| mesh.entity_id == Some(entity.id))
            .map(|limb| (index, limb))
    }) else {
        return false;
    };
    let (position, rotation) = crate::dismember::limb_instance(limb, transform);
    let mut instance =
        ActorInstance::new(position.to_array(), rotation.to_array(), transform.scale)
            .with_entity_color(entity.color());
    instance.view_flags = sinks.entity_view_flags;
    sinks.actor_groups[index].push(instance);
    meshes[index]
        .retained_pose
        .mark_drawn(entity.id, sinks.presentation_time);
    if crate::dismember::trail_due(limb, transform.translation, sinks.presentation_time) {
        effect_runtime::spawn_effect(
            sinks.particles,
            sinks.effect_aux,
            sinks.effects,
            sinks.vfs,
            crate::dismember::SMOKE,
            Vec3::from_array(transform.translation),
            visual_now,
            (entity.id.get() as u32) ^ (sinks.presentation_time as u32).rotate_left(9),
            0,
            sinks.game_audio,
            combat_effects::rotation_from_direction([0.0, 1.0, 0.0]),
        );
    }
    true
}
