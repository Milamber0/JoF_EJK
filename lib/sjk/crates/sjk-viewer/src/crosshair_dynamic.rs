//! On-foot CG_CalcMuzzlePoint and CG_ScanForCrosshairEntity (TaystJK codemp).
use crate::console::ViewerConsole;
use crate::hud::identification::Camera;
use glam::Vec3;
use sjk_client::pmove::MovementState;
use sjk_protocol::{GameState, PlayerState};

/// One display-only trace, shared with target identification rather than repeated.
#[derive(Clone, Copy)]
pub(crate) struct Ray {
    /// World-space start, from the ordinary weapon muzzle when enabled.
    pub(crate) start: Vec3,
    /// Normalized firing direction.
    pub(crate) forward: Vec3,
    /// Trace distance in world units.
    pub(crate) distance: f32,
    /// Whether its endpoint moves the HUD crosshair.
    pub(crate) projected: bool,
}

/// Read scalar settings once and select the same ordinary muzzle offsets as cgame.
pub(crate) fn ray(
    console: Option<&ViewerConsole>,
    player: &PlayerState,
    predicted: Option<&MovementState>,
    game: &GameState,
    camera: Camera,
    third_person: bool,
    weapon: u8,
) -> Ray {
    let mode = console
        .and_then(|c| c.integer_cvar("cg_dynamiccrosshair"))
        .unwrap_or(1);
    let helper = console
        .and_then(|c| c.integer_cvar("cg_strafehelper"))
        .unwrap_or(0) as u32;
    let race = game.config_string(0).is_some_and(|bytes| {
        sjk_client::LegacyClientInfo::new(bytes)
            .bytes("gamename")
            .is_some_and(|name| {
                name.windows(5)
                    .any(|part| part.eq_ignore_ascii_case(b"japro"))
            })
    }) && player.stats[11] != 0;
    // cg_draw.c:9184-9203: mode 1 forces dynamic even for melee; mode 2 is selective.
    let selective_static = mode == 2
        && (weapon <= 3 || race || helper & ((1 << 1) | (1 << 2) | (1 << 3) | (1 << 13)) != 0);
    // Vehicle and emplaced muzzle/precision paths are explicitly not implemented here.
    let projected =
        mode != 0 && !selective_static && player.vehicle_entity_num() == 0 && weapon < 17;
    let forward = (camera.target - camera.eye).normalize_or_zero();
    if !projected {
        return Ray {
            start: camera.eye,
            forward,
            distance: 131_072.0,
            projected: false,
        };
    }
    let (origin, direction) = if third_person {
        let angles = predicted.map_or(player.view_angles(), |ps| ps.view_angles);
        let pitch = angles[0].to_radians();
        let yaw = angles[1].to_radians();
        let direction = Vec3::new(
            pitch.cos() * yaw.cos(),
            pitch.cos() * yaw.sin(),
            -pitch.sin(),
        );
        let origin = predicted.map_or(Vec3::from_array(player.origin()), |ps| ps.origin.into());
        (origin + Vec3::Z * player.view_height() as f32, direction)
    } else {
        (camera.eye, forward)
    };
    Ray {
        start: muzzle(origin, direction, weapon),
        forward: direction,
        distance: 6_000.0,
        projected: true,
    }
}

/// Cgame's ordinary muzzle is an offset table, not the rendered viewmodel's flash bolt.
pub(crate) fn muzzle(eye: Vec3, forward: Vec3, weapon: u8) -> Vec3 {
    // cg_weapons.c:3058-3063 zeroes melee/disruptor offsets; bg_weapons.c:30-49.
    let offset = match weapon {
        4 | 5 | 9 | 10 | 15 | 16 => [12.0, 6.0, -6.0],
        7 => [12.0, 2.0, -6.0],
        8 => [12.0, 4.5, -6.0],
        11 => [12.0, 8.0, -4.0],
        12 | 14 => [12.0, 0.0, -4.0],
        13 => [12.0, 0.0, -10.0],
        _ => [0.0; 3],
    };
    let yaw = forward.y.atan2(forward.x);
    let right = Vec3::new(yaw.sin(), -yaw.cos(), 0.0);
    eye + forward * offset[0] + right * offset[1] + Vec3::Z * offset[2]
}

/// Convert the hit point into the existing shader's upward-positive UV offset.
pub(crate) fn offset(camera: Camera, endpoint: Vec3) -> Option<[f32; 2]> {
    camera.project(endpoint).map(|point| {
        [
            point[0] / camera.viewport[0] - 0.5,
            0.5 - point[1] / camera.viewport[1],
        ]
    })
}
