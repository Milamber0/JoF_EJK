//! Runtime projection feeding match information into the retained HUD.

use super::*;
use bytemuck::{Pod, Zeroable};

/// Append HUD text and overhead labels without duplicating the classic-font fallback.
pub(crate) fn append(
    gpu: &mut GpuState,
    classic: bool,
    _visibility: hud::HudVisibility,
    _layout: hud::HudLayout,
    _scale: f32,
    viewport: [f32; 2],
) {
    // Nameplates use the classic HUD font whatever `cg_classicHudFont` says,
    // and fall back to Inter when that font is not loaded.
    let power_icons = gpu.hud.power_icons();
    match &gpu.classic_hud_font {
        Some(font) => gpu.hud.nameplate.append(
            &gpu.chat,
            &power_icons,
            &gpu.hud.icons,
            &mut gpu.classic_text_vertices,
            font,
            viewport,
        ),
        None => gpu.hud.nameplate.append(
            &gpu.chat,
            &power_icons,
            &gpu.hud.icons,
            &mut gpu.text_vertices,
            &gpu.ui_font,
            viewport,
        ),
    }
    gpu.hud
        .identification
        .append(&gpu.chat, &mut gpu.text_vertices, &gpu.ui_font, viewport);
    gpu.hud
        .card
        .append(&mut gpu.text_vertices, &gpu.ui_font, viewport);
    if classic && let Some(font) = &gpu.classic_hud_font {
        gpu.hud.append(
            &mut gpu.game_fonts,
            &mut gpu.classic_text_vertices,
            font,
            viewport,
        );
    } else {
        gpu.hud.append(
            &mut gpu.game_fonts,
            &mut gpu.text_vertices,
            &gpu.ui_font,
            viewport,
        );
    }
}

/// GPU layout shared with `hud.wgsl`; appended parameters begin at byte 144.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct HudUniform {
    /// Resolved crosshair RGBA.
    pub(crate) crosshair_color: [f32; 4],
    /// Displayed health fraction.
    pub(crate) health_ratio: f32,
    /// Displayed armor fraction.
    pub(crate) armor_ratio: f32,
    /// Displayed Force fraction.
    pub(crate) force_ratio: f32,
    /// Menu backdrop selector.
    pub(crate) menu_open: f32,
    /// Reciprocal physical viewport width.
    pub(crate) inverse_width: f32,
    /// Reciprocal physical viewport height.
    pub(crate) inverse_height: f32,
    /// Selected menu row.
    pub(crate) menu_row: f32,
    /// Number of menu rows.
    pub(crate) menu_row_count: f32,
    /// Crosshair size; zero hides it.
    pub(crate) crosshair: f32,
    /// Global HUD visibility.
    pub(crate) hud_visible: f32,
    /// Legacy status-bar switch.
    pub(crate) status_visible: f32,
    /// Menu presentation variant.
    pub(crate) menu_kind: f32,
    /// Seconds since the UI epoch.
    pub(crate) menu_phase: f32,
    /// Damage direction, horizontal component.
    pub(crate) damage_x: f32,
    /// Damage direction, vertical component.
    pub(crate) damage_y: f32,
    /// Damage indicator opacity.
    pub(crate) damage_alpha: f32,
    /// Damage indicator intensity.
    pub(crate) damage_strength: f32,
    /// Retained health-bar rectangle.
    pub(crate) health_bar: [f32; 4],
    /// Retained armor-bar rectangle.
    pub(crate) armor_bar: [f32; 4],
    /// Retained Force-bar rectangle.
    pub(crate) force_bar: [f32; 4],
    /// Existing uniform tail padding.
    pub(crate) _padding: [f32; 3],
    /// Offset UV and dimensions used to size the crosshair.
    pub(crate) crosshair_parameters: [f32; 4],
}

/// Refresh retained HUD data and project movement guides through this frame's camera.
pub(crate) fn update(
    gpu: &mut GpuState,
    view_position: Vec3,
    view: (Vec3, Vec3, f32),
    presentation_time: i32,
    intermission: bool,
) {
    let (view_target, view_up, vertical_fov) = view;
    hud::tints::update(gpu, view_position, presentation_time, intermission);
    hud::vehicle_feed::sample(gpu, presentation_time);
    let scoreboard = gpu.gameplay_input.held(input::GameButton::Scores);
    let labels_hidden = scoreboard
        || intermission
        || gpu.console.as_ref().is_some_and(|c| {
            !c.bool_cvar("cg_drawhud").unwrap_or(true) || !c.bool_cvar("cg_draw2d").unwrap_or(true)
        });
    gpu.hud.identification.sample(gpu.console.as_ref());
    if let Some(console) = gpu.console.as_mut() {
        hud::nameplate::hold_sharing(console);
    }
    gpu.hud.nameplate.sample(gpu.console.as_ref());
    gpu.hud.card.sample(gpu.console.as_ref());
    gpu.hud.team_logo.sample(gpu.console.as_ref());
    gpu.hud.team_outline.sample(gpu.console.as_ref());
    // Nameplate text is in the classic stream, which draws over the menus' text.
    let plates_hidden = labels_hidden
        || gpu.game_menu
        || gpu
            .client_menu
            .as_ref()
            .is_some_and(|menu| menu.is_visible());
    // A nameplate replaces the plain overhead names.
    let labels_hidden = labels_hidden || gpu.hud.nameplate.enabled();
    // `CG_DrawRocketLocking` belongs to `CG_Draw2D`: none at intermission or with
    // `cg_draw2D 0`.
    let lock_hidden = intermission
        || gpu
            .console
            .as_ref()
            .is_some_and(|c| !c.bool_cvar("cg_draw2d").unwrap_or(true));
    let camera = hud::identification::Camera {
        eye: view_position,
        target: view_target,
        up: view_up,
        fov: vertical_fov,
        viewport: [
            gpu.configuration.width as f32,
            gpu.configuration.height as f32,
        ],
    };
    if let Some(session) = &gpu.live_session {
        let snapshot = session.latest_snapshot();
        gpu.scoreboard.observe_deaths(
            &gpu.obituaries,
            snapshot,
            session.game_state(),
            gpu.console.as_ref(),
        );
        // Team logos first: the overhead names and plates stand above them.
        gpu.hud.team_logo.update(
            snapshot,
            session.game_state(),
            &gpu.live_world,
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            plates_hidden,
            |logo| gpu.hud.icons.team_logo(logo),
        );
        gpu.hud.team_outline.update(
            snapshot,
            session.game_state(),
            &gpu.live_world,
            i64::from(presentation_time),
            camera,
            plates_hidden,
        );
        let logo_tops = gpu.hud.team_logo.tops();
        gpu.hud.identification.set_logo_tops(logo_tops);
        gpu.hud.nameplate.set_logo_tops(logo_tops);
        gpu.hud.identification.update(
            snapshot,
            session.game_state(),
            &gpu.live_world,
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            labels_hidden,
        );
        gpu.hud
            .nameplate
            .refresh_verified(i64::from(presentation_time), || {
                player_identity::verified_slots(session.game_state())
            });
        gpu.hud
            .nameplate
            .refresh_shared(i64::from(presentation_time), || {
                let game_state = session.game_state();
                (
                    player_identity::shared_vitals(game_state),
                    player_identity::sharing_live(game_state),
                )
            });
        player_identity::share_vitals(gpu.hud.nameplate.sharing(Some(snapshot)));
        let detached_flight = gpu.free_camera_active();
        gpu.hud.nameplate.set_own_origin(gpu.third_person.then(|| {
            gpu.local_prediction
                .predicted_state()
                .filter(|_| !detached_flight)
                .map_or(snapshot.player.origin(), |state| state.origin)
        }));
        gpu.hud.nameplate.update(
            snapshot,
            session.game_state(),
            &gpu.live_world,
            session.team_info(),
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            plates_hidden,
        );
        gpu.hud.rocket_lock.update(
            hud::rocket_lock::input(
                snapshot,
                session.game_state(),
                &gpu.live_world,
                presentation_time,
                lock_hidden,
            ),
            camera,
        );
        gpu.hud
            .update_family(snapshot, session.game_state(), gpu.console.as_ref());
        let ja_plus = matches!(
            session.compat_profile(),
            sjk_client::CompatProfile::JaPlus { .. }
        );
        gpu.hud.update_selection(
            &gpu.gameplay_input.selection,
            &snapshot.player,
            presentation_time,
            ja_plus,
        );
        gpu.lagometer
            .add_frame(presentation_time - snapshot.server_time);
        let mut ray = crosshair_scan::dynamic::ray(
            gpu.console.as_ref(),
            &snapshot.player,
            gpu.local_prediction.predicted_state(),
            session.game_state(),
            camera,
            gpu.third_person,
            snapshot.player.weapon(),
        );
        if ray.projected {
            ray.distance = gpu.crosshair_scan.distance_cull.unwrap_or(6000.0);
        }
        let crosshair = gpu.crosshair_scan.scan(
            snapshot,
            session.game_state(),
            ray,
            presentation_time,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            crosshair_scan::Suppression {
                scoreboard,
                intermission,
            },
        );
        // The nameplates' target mode keeps the player aimed at for a few seconds.
        gpu.hud.nameplate.set_focus(
            gpu.crosshair_scan.hit_now(presentation_time),
            i64::from(presentation_time),
        );
        gpu.hud.targeting.dynamic_offset = if ray.projected {
            Some(
                crosshair_scan::dynamic::offset(camera, gpu.crosshair_scan.endpoint)
                    .unwrap_or([2.0; 2]),
            )
        } else {
            None
        };
        let head_icons = &gpu.scoreboard;
        let (looks, skins) = (&gpu.looks, &*gpu.blade_skins);
        let own_slot = crate::looks::ViewSlots::of(session.game_state(), &snapshot.player).own;
        gpu.hud.card.update(hud::player_card::Input {
            seen: gpu.crosshair_scan.hit_now(presentation_time),
            game: session.game_state(),
            world: &gpu.live_world,
            now: presentation_time,
            camera,
            hidden: plates_hidden,
            hub: &player_identity::hub_info,
            // What players share changes their card too.
            hub_revision: player_identity::revision()
                ^ player_identity::vitals_flags(session.game_state()).rotate_left(17),
            entities: &snapshot.entities,
            icon: &|slot| head_icons.head_icon(slot),
            shader: &|slot| card_shader(looks, skins, own_slot, slot),
            skins,
            looks_revision: looks.revision() ^ skins.generation().rotate_left(32),
            profiles_revision: player_identity::profiles_revision(),
        });
        if let (Some(client), Some(vfs)) = (
            gpu.hud
                .card
                .shown_client()
                .and_then(|client| u8::try_from(client).ok()),
            &gpu.vfs,
        ) {
            gpu.scoreboard.ensure_head_icon(
                session.game_state(),
                client,
                vfs,
                &gpu.shaders,
                &gpu.ui_shapes,
                &gpu.queue,
            );
        }
        gpu.hud.update(
            session,
            presentation_time.max(0) as u64,
            gpu.local_prediction.predicted_state(),
            gpu.console.as_ref(),
        );
        gpu.hud.update_match_information(
            session.game_state(),
            presentation_time,
            &gpu.obituaries,
            snapshot.player.client_num(),
            &gpu.lagometer,
            crosshair,
            !session.is_local()
                && sjk_client::connection_interrupted(presentation_time, snapshot.server_time),
        );
    } else if let Some(session) = &gpu.demo_session {
        let snapshot = session.snapshot_at_or_before(presentation_time);
        gpu.scoreboard.observe_deaths(
            &gpu.obituaries,
            snapshot,
            session.game_state(),
            gpu.console.as_ref(),
        );
        gpu.hud.team_logo.update(
            snapshot,
            session.game_state(),
            session.world(),
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            plates_hidden,
            |logo| gpu.hud.icons.team_logo(logo),
        );
        gpu.hud.team_outline.update(
            snapshot,
            session.game_state(),
            session.world(),
            i64::from(presentation_time),
            camera,
            plates_hidden,
        );
        let logo_tops = gpu.hud.team_logo.tops();
        gpu.hud.identification.set_logo_tops(logo_tops);
        gpu.hud.nameplate.set_logo_tops(logo_tops);
        gpu.hud.identification.update(
            snapshot,
            session.game_state(),
            session.world(),
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            labels_hidden,
        );
        gpu.hud
            .nameplate
            .set_own_origin(gpu.third_person.then(|| snapshot.player.origin()));
        gpu.hud.nameplate.update(
            snapshot,
            session.game_state(),
            session.world(),
            &sjk_client::TeamInfoTable::default(),
            i64::from(presentation_time),
            camera,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            plates_hidden,
        );
        gpu.hud.rocket_lock.update(
            hud::rocket_lock::input(
                snapshot,
                session.game_state(),
                session.world(),
                presentation_time,
                lock_hidden,
            ),
            camera,
        );
        gpu.hud
            .update_family(snapshot, session.game_state(), gpu.console.as_ref());
        // Only merc mode needs the server's mod; read it from the demo's gamestate then.
        let ja_plus = snapshot.player.entity_flags() & 0x1000 != 0
            && matches!(
                sjk_client::CompatProfile::from_game_state(session.game_state()),
                sjk_client::CompatProfile::JaPlus { .. }
            );
        gpu.hud.update_selection(
            &gpu.gameplay_input.selection,
            &snapshot.player,
            presentation_time,
            ja_plus,
        );
        gpu.lagometer
            .add_frame(presentation_time - snapshot.server_time);
        let mut ray = crosshair_scan::dynamic::ray(
            gpu.console.as_ref(),
            &snapshot.player,
            None,
            session.game_state(),
            camera,
            gpu.third_person,
            snapshot.player.weapon(),
        );
        if ray.projected {
            ray.distance = gpu.crosshair_scan.distance_cull.unwrap_or(6000.0);
        }
        let crosshair = gpu.crosshair_scan.scan(
            snapshot,
            session.game_state(),
            ray,
            presentation_time,
            &gpu.bsp,
            &mut gpu.trace_scratch,
            crosshair_scan::Suppression {
                scoreboard,
                intermission,
            },
        );
        // The nameplates' target mode keeps the player aimed at for a few seconds.
        gpu.hud.nameplate.set_focus(
            gpu.crosshair_scan.hit_now(presentation_time),
            i64::from(presentation_time),
        );
        gpu.hud.targeting.dynamic_offset = if ray.projected {
            Some(
                crosshair_scan::dynamic::offset(camera, gpu.crosshair_scan.endpoint)
                    .unwrap_or([2.0; 2]),
            )
        } else {
            None
        };
        let head_icons = &gpu.scoreboard;
        let (looks, skins) = (&gpu.looks, &*gpu.blade_skins);
        let own_slot = crate::looks::ViewSlots::of(session.game_state(), &snapshot.player).own;
        gpu.hud.card.update(hud::player_card::Input {
            seen: gpu.crosshair_scan.hit_now(presentation_time),
            game: session.game_state(),
            world: session.world(),
            now: presentation_time,
            camera,
            hidden: plates_hidden,
            hub: &player_identity::hub_info,
            // What players share changes their card too.
            hub_revision: player_identity::revision()
                ^ player_identity::vitals_flags(session.game_state()).rotate_left(17),
            entities: &snapshot.entities,
            icon: &|slot| head_icons.head_icon(slot),
            shader: &|slot| card_shader(looks, skins, own_slot, slot),
            skins,
            looks_revision: looks.revision() ^ skins.generation().rotate_left(32),
            profiles_revision: player_identity::profiles_revision(),
        });
        if let (Some(client), Some(vfs)) = (
            gpu.hud
                .card
                .shown_client()
                .and_then(|client| u8::try_from(client).ok()),
            &gpu.vfs,
        ) {
            gpu.scoreboard.ensure_head_icon(
                session.game_state(),
                client,
                vfs,
                &gpu.shaders,
                &gpu.ui_shapes,
                &gpu.queue,
            );
        }
        gpu.hud
            .update_player(&snapshot.player, presentation_time.max(0) as u64);
        gpu.hud.update_demo_votes(
            session.game_state(),
            &snapshot.player,
            presentation_time.max(0) as u64,
            gpu.console.as_ref(),
        );
        gpu.hud.update_match_information(
            session.game_state(),
            presentation_time,
            &gpu.obituaries,
            snapshot.player.client_num(),
            &gpu.lagometer,
            crosshair,
            false,
        );
    }
    let game = gpu
        .live_session
        .as_ref()
        .map(|s| s.game_state())
        .or_else(|| gpu.demo_session.as_ref().map(|s| s.game_state()));
    {
        if let Some(game) = gpu
            .resident
            .session
            .as_ref()
            .map(|s| s.game_state())
            .or(game)
        {
            gpu.chat.update_roster(game);
        } else {
            gpu.hud.identification.list.clear();
            gpu.hud.nameplate.clear();
            gpu.hud.card.clear();
            gpu.hud.team_logo.clear();
            gpu.hud.team_outline.clear();
        }
    }
    if let Some(game) = game {
        let snapshot = gpu
            .live_session
            .as_ref()
            .map(|s| s.latest_snapshot())
            .or_else(|| {
                gpu.demo_session
                    .as_ref()
                    .map(|s| s.snapshot_at_or_before(presentation_time))
            });
        if let Some(snapshot) = snapshot {
            gpu.hud
                .targeting
                .classify(gpu.crosshair_scan.color, snapshot.player.weapon());
            gpu.hud.enemy_info.update(
                game,
                snapshot,
                gpu.console.as_ref(),
                &gpu.chat,
                labels_hidden,
                gpu.live_session.as_ref().map_or(&[], |s| s.scores()),
            );
            if let Some(vfs) = &gpu.vfs {
                gpu.hud.enemy_info.update_portrait(
                    game,
                    vfs,
                    &gpu.shaders,
                    &gpu.ui_shapes,
                    &gpu.queue,
                );
            }
            gpu.chat.observe_combat(
                snapshot,
                game,
                &gpu.obituaries,
                gpu.console.as_ref(),
                camera,
                presentation_time,
            );
        }
    }
    let japro = game
        .and_then(|g| g.config_string(0))
        .is_some_and(|info| info.windows(5).any(|w| w.eq_ignore_ascii_case(b"japro")));
    let keys = gpu
        .live_session
        .as_ref()
        .and_then(|_| gpu.local_prediction.guide_input());
    gpu.hud.restrict_guides(!japro, keys);
    gpu.hud.guide_camera(
        view_target - view_position,
        view_up,
        vertical_fov,
        gpu.configuration.width as f32 / gpu.configuration.height as f32,
    );
}

/// Pixel rectangles (x, y, width, height) the in-game HUD program (`hud.wgsl`) can draw
/// into. Outside a menu it draws only the crosshair and the damage ring and discards
/// every other pixel, so the full-screen pass is scissored to these instead of shading
/// the whole screen; `None` keeps the full screen.
pub(crate) type HudScissors = Option<[Option<[u32; 4]>; 2]>;

impl HudUniform {
    /// The rectangles for this uniform on a `width` x `height` target.
    pub(crate) fn scissors(&self, width: u32, height: u32) -> HudScissors {
        if self.menu_open > 0.5 || self.status_visible > 0.5 || width == 0 || height == 0 {
            return None;
        }
        let [w, h] = [width as f32, height as f32];
        // Shader UVs run right and up from the bottom-left corner; rows run down.
        let rectangle = |center: [f32; 2], half: [f32; 2]| -> Option<[u32; 4]> {
            let left = ((center[0] - half[0]) * w - 2.0).floor().clamp(0.0, w);
            let right = ((center[0] + half[0]) * w + 2.0).ceil().clamp(0.0, w);
            let top = ((1.0 - center[1] - half[1]) * h - 2.0)
                .floor()
                .clamp(0.0, h);
            let bottom = ((1.0 - center[1] + half[1]) * h + 2.0).ceil().clamp(0.0, h);
            (right > left && bottom > top).then_some([
                left as u32,
                top as u32,
                (right - left) as u32,
                (bottom - top) as u32,
            ])
        };
        let visible = self.hud_visible > 0.5;
        let mut crosshair = None;
        if visible && self.crosshair > 0.0 {
            let [x, y, scale_x, scale_y] = self.crosshair_parameters;
            // Degenerate scales would put the crosshair test everywhere.
            if !(scale_x > 0.0 && scale_y > 0.0 && x.is_finite() && y.is_finite()) {
                return None;
            }
            // The outline reaches 14/26 of the crosshair size on each axis.
            let reach = 14.0 / 26.0 * self.crosshair.max(0.001);
            crosshair = rectangle([0.5 + x, 0.5 + y], [reach / scale_x, reach / scale_y]);
        }
        let mut damage = None;
        if visible && self.damage_alpha > 0.0 {
            let outer = 0.092 + 0.004 * self.damage_strength;
            let aspect = self.inverse_height / self.inverse_width;
            if !(aspect > 0.0 && outer.is_finite()) {
                return None;
            }
            damage = rectangle([0.5, 0.5], [outer / aspect, outer]);
        }
        // A HUD draw runs the entire shader, so overlapping scissors would blend
        // the crosshair twice while taking damage. Draw their union once instead.
        if let (Some(a), Some(b)) = (crosshair, damage)
            && a[0] < b[0] + b[2]
            && b[0] < a[0] + a[2]
            && a[1] < b[1] + b[3]
            && b[1] < a[1] + a[3]
        {
            let left = a[0].min(b[0]);
            let top = a[1].min(b[1]);
            crosshair = Some([
                left,
                top,
                (a[0] + a[2]).max(b[0] + b[2]) - left,
                (a[1] + a[3]).max(b[1] + b[3]) - top,
            ]);
            damage = None;
        }
        Some([crosshair, damage])
    }
}

/// The saber shader the player in `slot` wears, as their blade draws it here: the local
/// player's own (`own_slot`, gated by their unlocks) or the looks' for another, only
/// when its pack is loaded. For the player card.
fn card_shader(
    looks: &crate::looks::Looks,
    skins: &crate::saber_skins::LoadedSkins,
    own_slot: Option<usize>,
    slot: u8,
) -> Option<&'static str> {
    let slot = usize::from(slot);
    let id = if own_slot == Some(slot) {
        looks.own_saber_skin()
    } else {
        looks.saber_skin_id(slot)
    }?;
    skins.get(id).map(|skin| skin.id)
}

#[cfg(test)]
mod tests {
    use super::card_shader;
    use crate::looks::{Looks, Worn};
    use crate::saber_skins::{LoadedSkins, tests::loaded_sample};

    #[test]
    fn the_card_names_a_shader_only_when_it_draws_on_the_blade() {
        let loaded = loaded_sample(1);
        let sun = sjk_identity::Look {
            saber: "saber_sun".to_owned(),
            illuminate: false,
            saber_off: Vec::new(),
        };
        let mut looks = Looks::default();
        looks.replace_roster([(3, "Fox", Some(&sun))]);
        looks.rebuild(|slot| (slot == 3).then(|| "Fox".to_owned()));
        // Another player, worn by the looks (which the hub relays only when owned).
        assert_eq!(card_shader(&looks, &loaded, None, 3), Some("saber_sun"));
        // Nobody's look, or one whose pack is not loaded here.
        assert_eq!(card_shader(&looks, &loaded, None, 4), None);
        assert_eq!(
            card_shader(&looks, &LoadedSkins::of(Vec::new(), 1), None, 3),
            None
        );
        // The local player: their own look, gated by their unlocks.
        looks.set_own(Some(1), Worn::own("saber_sun", "", |_| true, false));
        assert_eq!(card_shader(&looks, &loaded, Some(1), 1), Some("saber_sun"));
        looks.set_own(Some(1), Worn::own("saber_sun", "", |_| false, false));
        assert_eq!(card_shader(&looks, &loaded, Some(1), 1), None);
        looks.set_own(Some(1), Worn::own("", "", |_| true, false));
        assert_eq!(card_shader(&looks, &loaded, Some(1), 1), None);
    }
}
