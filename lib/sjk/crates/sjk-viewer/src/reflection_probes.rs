//! Reflection probes for specular-mapped surfaces, after OpenJK rend2's cubemaps
//! (`R_LoadCubemapEntities`, `R_AssignCubemapsToWorldSurfaces` in `tr_bsp.cpp`,
//! `CalcIBLContribution` in `glsl/lightall.glsl`).
//!
//! Probes stand at the map's `misc_cubemap` entities, else at its player spawn points
//! (rend2 takes the first of those classes that has any; spawn points get eye height).
//! Points closer than [`MERGE_DISTANCE`] merge, and at most [`MAX_PROBES`] are kept,
//! spread by farthest-point selection. Each probe measures the room it stands in with
//! six axis traces against the world brushes: the box the shader projects reflections
//! onto (Lagarde's box-projected cubemap, the accurate form of rend2's cheap
//! sphere-radius parallax for Quake's axis-aligned rooms). Every material-mapped surface
//! takes the nearest probe it can see, as rend2 assigns the nearest cubemap per surface;
//! the index travels in the spare byte of the vertex frames.
//!
//! The faces are captured through the ordinary scene path after map load
//! (`reflection_capture.rs`) and prefiltered on the GPU ([`gpu`]); the split-sum BRDF
//! table comes from [`brdf`].

#[path = "reflection_probe_brdf.rs"]
pub(crate) mod brdf;
#[path = "reflection_probe_gpu.rs"]
pub(crate) mod gpu;

use glam::Vec3;
use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::ops::Range;
use std::sync::atomic::{AtomicU32, Ordering};

/// Probes per map; the cube array and the shader's table hold this many.
pub(crate) const MAX_PROBES: usize = 64;
/// Candidates nearer than this merge into one probe: spawn points come in clusters a
/// few player widths apart, which a single probe serves.
pub(crate) const MERGE_DISTANCE: f32 = 256.;
/// Spawn points hold the player's origin; probes see the room from eye height
/// (`DEFAULT_VIEWHEIGHT` above it).
const SPAWN_LIFT: f32 = 26.;
/// Reach of the room-measuring traces, and the smallest box half-extent kept.
const ROOM_REACH: f32 = 2048.;
const ROOM_MIN: f32 = 32.;
/// Surfaces try this many of their nearest probes for one in plain view.
const VISIBLE_TRIES: usize = 4;
/// Q3 `CONTENTS_SOLID`.
const CONTENTS_SOLID: u32 = 1;

pub(crate) const DEFAULT_SIZE: u32 = 128;
const MIN_SIZE: u32 = 32;
const MAX_SIZE: u32 = 512;

/// The face size the viewer sampled (0 off), with [`LATCHED`] once sampled.
static LATCH: AtomicU32 = AtomicU32::new(0);
const LATCHED: u32 = 1 << 31;

/// `r_cubeMapSize` as a usable face size: a power of two in 32..=512.
pub(crate) fn face_size(requested: i64) -> u32 {
    let clamped = requested.clamp(i64::from(MIN_SIZE), i64::from(MAX_SIZE)) as u32;
    1 << (31 - clamped.leading_zeros())
}

/// The startup value (face size, 0 off), remembered so a later change can ask for a
/// graphics reload like the other material-map controls.
pub(super) fn sample(console: Option<&crate::console::ViewerConsole>) -> u32 {
    let value = read(console);
    if console.is_some() {
        LATCH.store(LATCHED | value, Ordering::Relaxed);
    }
    value
}

/// What [`sample`] would read now, without remembering it.
pub(super) fn read(console: Option<&crate::console::ViewerConsole>) -> u32 {
    let on = console
        .and_then(|c| c.integer_cvar("r_cubeMapping"))
        .unwrap_or(1)
        != 0;
    let size = face_size(
        console
            .and_then(|c| c.integer_cvar("r_cubeMapSize"))
            .unwrap_or(i64::from(DEFAULT_SIZE)),
    );
    if on { size } else { 0 }
}

/// Whether the viewer runs with something other than `value` (face size, 0 off).
fn restart_needed(latch: u32, value: u32) -> bool {
    latch & LATCHED != 0 && latch & !LATCHED != value
}

/// `r_cubeMapping` (rend2's name and meaning, on by default here) and `r_cubeMapSize`.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), CvarError> {
    cvars.register(CvarDefinition::new(
        "r_cubeMapping",
        1_i64,
        CvarFlags::ARCHIVE,
        "Reflection probes on specular-mapped surfaces (needs r_specularMapping); \
         vid_restart applies it",
    ))?;
    cvars.register(CvarDefinition::new(
        "r_cubeMapSize",
        i64::from(DEFAULT_SIZE),
        CvarFlags::ARCHIVE,
        "Reflection probe face size, a power of two 32..512; vid_restart applies it",
    ))?;
    let report = |name: &'static str| {
        move |change: &sjk_shell::CvarChange| {
            let latch = LATCH.load(Ordering::Relaxed);
            let size = latch & !LATCHED;
            let value = match (name, &change.current) {
                ("r_cubeMapping", CvarValue::Integer(0)) => 0,
                ("r_cubeMapping", _) if size == 0 => DEFAULT_SIZE,
                ("r_cubeMapSize", CvarValue::Integer(requested)) if size != 0 => {
                    face_size(*requested)
                }
                _ => size,
            };
            if restart_needed(latch, value) {
                crate::log::progress(format_args!(
                    "{name} changed: {}",
                    crate::graphics_reload::APPLY
                ));
            }
            crate::graphics_reload::notice();
        }
    };
    cvars.on_change("r_cubeMapping", report("r_cubeMapping"))?;
    cvars.on_change("r_cubeMapSize", report("r_cubeMapSize"))?;
    Ok(())
}

/// One probe: where it captures from and the room box reflections are projected on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Probe {
    pub(crate) origin: Vec3,
    pub(crate) box_min: Vec3,
    pub(crate) box_max: Vec3,
}

/// Spawn classes, in rend2's order after `misc_cubemap`; team and duel spawns too, so
/// CTF and duel maps get probes where their players stand.
const SPAWNS: [&str; 9] = [
    "info_player_deathmatch",
    "info_player_start",
    "info_player_duel",
    "info_player_duel1",
    "info_player_duel2",
    "team_CTF_redplayer",
    "team_CTF_blueplayer",
    "team_CTF_redspawn",
    "team_CTF_bluespawn",
];

/// Candidate probe positions: the `misc_cubemap` origins when the map places any, else
/// every spawn point at eye height, else the intermission spots.
pub(crate) fn candidates(entities: &[sjk_entity::Entity]) -> Vec<Vec3> {
    let origins = |accept: &dyn Fn(&str) -> bool, lift: f32| -> Vec<Vec3> {
        entities
            .iter()
            .filter(|e| e.classname().is_some_and(accept))
            .filter_map(|e| e.vector("origin").ok().flatten())
            .map(|origin| Vec3::from_array(origin) + Vec3::Z * lift)
            .collect()
    };
    let authored = origins(&|class| class.eq_ignore_ascii_case("misc_cubemap"), 0.);
    if !authored.is_empty() {
        return authored;
    }
    let spawns = origins(
        &|class| SPAWNS.iter().any(|spawn| spawn.eq_ignore_ascii_case(class)),
        SPAWN_LIFT,
    );
    if !spawns.is_empty() {
        return spawns;
    }
    origins(
        &|class| class.eq_ignore_ascii_case("info_player_intermission"),
        0.,
    )
}

/// Merge points within `merge` of a probe (into their mean), then keep at most `cap`
/// by farthest-point selection from the first. Deterministic in input order.
pub(crate) fn cluster(points: &[Vec3], merge: f32, cap: usize) -> Vec<Vec3> {
    let mut sums: Vec<(Vec3, f32)> = Vec::new();
    for &point in points {
        match sums
            .iter_mut()
            .find(|(sum, count)| (*sum / *count).distance(point) <= merge)
        {
            Some((sum, count)) => {
                *sum += point;
                *count += 1.;
            }
            None => sums.push((point, 1.)),
        }
    }
    let centres: Vec<Vec3> = sums.into_iter().map(|(sum, count)| sum / count).collect();
    if cap == 0 {
        return Vec::new();
    }
    if centres.len() <= cap {
        return centres;
    }
    let mut kept = vec![centres[0]];
    let mut nearest: Vec<f32> = centres.iter().map(|c| c.distance(centres[0])).collect();
    while kept.len() < cap {
        let (index, _) =
            nearest.iter().enumerate().fold(
                (0, -1.),
                |best, (i, &d)| if d > best.1 { (i, d) } else { best },
            );
        kept.push(centres[index]);
        for (distance, centre) in nearest.iter_mut().zip(&centres) {
            *distance = distance.min(centre.distance(centres[index]));
        }
    }
    kept
}

/// The room box around `origin`: along each axis, the distance `free` reports (the
/// world's free space in that direction), kept within [`ROOM_MIN`]..[`ROOM_REACH`].
pub(crate) fn room(origin: Vec3, mut free: impl FnMut(Vec3, Vec3) -> f32) -> Probe {
    let mut reach = |axis: Vec3| free(origin, axis).clamp(ROOM_MIN, ROOM_REACH);
    let box_max = origin + Vec3::new(reach(Vec3::X), reach(Vec3::Y), reach(Vec3::Z));
    let box_min = origin - Vec3::new(reach(-Vec3::X), reach(-Vec3::Y), reach(-Vec3::Z));
    Probe {
        origin,
        box_min,
        box_max,
    }
}

/// The map's probes: candidates outside solid space, clustered, each with its room.
pub(crate) fn place(bsp: &sjk_bsp::Bsp) -> Vec<Probe> {
    let Ok(entities) = sjk_entity::parse_entity_lump(bsp.entities()) else {
        return Vec::new();
    };
    let open = |point: Vec3| {
        bsp.point_contents(point.to_array(), CONTENTS_SOLID) == 0
            && bsp.leaves()[bsp.leaf_at(point.to_array())].cluster >= 0
    };
    let points: Vec<Vec3> = candidates(&entities)
        .into_iter()
        .filter(|&p| open(p))
        .collect();
    let mut scratch = bsp.trace_scratch();
    cluster(&points, MERGE_DISTANCE, MAX_PROBES)
        .into_iter()
        .map(|origin| {
            room(origin, |from, axis| {
                let to = from + axis * ROOM_REACH;
                let trace = bsp.trace_box_with(
                    &mut scratch,
                    from.to_array(),
                    to.to_array(),
                    sjk_bsp::Aabb::POINT,
                    CONTENTS_SOLID,
                );
                trace.fraction * ROOM_REACH
            })
        })
        .collect()
}

/// The probe a surface centred at `centre` reflects: the nearest one `visible` from it
/// among the [`VISIBLE_TRIES`] nearest, else the nearest. `None` without probes.
pub(crate) fn nearest(
    probes: &[Probe],
    centre: Vec3,
    mut visible: impl FnMut(Vec3, Vec3) -> bool,
) -> Option<usize> {
    let mut order = [(f32::INFINITY, usize::MAX); VISIBLE_TRIES];
    for (index, probe) in probes.iter().enumerate() {
        let mut candidate = (probe.origin.distance_squared(centre), index);
        for slot in &mut order {
            if candidate.0 < slot.0 {
                std::mem::swap(slot, &mut candidate);
            }
        }
    }
    let first = order[0].1;
    if first == usize::MAX {
        return None;
    }
    order
        .iter()
        .filter(|(_, index)| *index != usize::MAX)
        .find(|(_, index)| visible(centre, probes[*index].origin))
        .map_or(Some(first), |(_, index)| Some(*index))
}

/// Write probe `index + 1` (0: none) into the spare fourth byte of the light-direction
/// word of every vertex `indices[range]` names (`material_map_frames.rs`).
pub(crate) fn mark_vertices(
    frames: &mut [[u32; 2]],
    indices: &[u32],
    range: Range<u32>,
    probe: Option<usize>,
) {
    let byte = probe.map_or(0, |index| (index + 1).min(255) as u32);
    for &vertex in &indices[range.start as usize..range.end as usize] {
        if let Some(frame) = frames.get_mut(vertex as usize) {
            frame[1] = frame[1] & 0x00ff_ffff | byte << 24;
        }
    }
}

/// Lighting a probe's capture was made under (sun direction and intensity, sky
/// colour, light scale, indirect gain); see `Runtime::lighting_signature`.
pub(crate) type Signature = [f32; 9];

/// Whether lighting moved far enough from `captured` that the probes look stale: the sun
/// turned by half a degree, or a colour or gain changed by 2%.
pub(crate) fn relit(captured: &Signature, current: &Signature) -> bool {
    let sun = |s: &Signature| Vec3::new(s[0], s[1], s[2]);
    let turned = sun(captured)
        .normalize_or_zero()
        .dot(sun(current).normalize_or_zero())
        < 0.5_f32.to_radians().cos();
    let changed = (3..9).any(|i| {
        (captured[i] - current[i]).abs() > 0.02 * captured[i].abs().max(current[i].abs()).max(1e-3)
    });
    (turned && (captured[3] > 0. || current[3] > 0.)) || changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(class: &str, origin: &str) -> sjk_entity::Entity {
        sjk_entity::Entity::from_fields(vec![
            ("classname".into(), class.into()),
            ("origin".into(), origin.into()),
        ])
    }

    #[test]
    fn authored_cubemaps_win_over_spawn_points() {
        let entities = [
            entity("info_player_deathmatch", "0 0 0"),
            entity("misc_cubemap", "10 20 30"),
        ];
        assert_eq!(candidates(&entities), vec![Vec3::new(10., 20., 30.)]);
        // Without them every spawn class counts, at eye height.
        let spawns = [
            entity("info_player_deathmatch", "0 0 0"),
            entity("team_CTF_redspawn", "100 0 0"),
            entity("info_player_intermission", "0 0 999"),
            entity("light", "5 5 5"),
        ];
        assert_eq!(
            candidates(&spawns),
            vec![
                Vec3::new(0., 0., SPAWN_LIFT),
                Vec3::new(100., 0., SPAWN_LIFT)
            ]
        );
        // The intermission spot is the last resort.
        let only = [entity("info_player_intermission", "1 2 3")];
        assert_eq!(candidates(&only), vec![Vec3::new(1., 2., 3.)]);
        assert!(candidates(&[]).is_empty());
    }

    #[test]
    fn nearby_points_merge_and_the_cap_keeps_them_spread() {
        let points = [
            Vec3::ZERO,
            Vec3::new(100., 0., 0.),
            Vec3::new(1000., 0., 0.),
            Vec3::new(1050., 0., 0.),
        ];
        let merged = cluster(&points, MERGE_DISTANCE, MAX_PROBES);
        assert_eq!(
            merged,
            vec![Vec3::new(50., 0., 0.), Vec3::new(1025., 0., 0.)]
        );
        // A line of 10 points 1000 apart capped at 3: the ends and the middle.
        let line: Vec<Vec3> = (0..10).map(|i| Vec3::X * (i as f32 * 1000.)).collect();
        let kept = cluster(&line, MERGE_DISTANCE, 3);
        assert_eq!(kept.len(), 3);
        assert!(kept.contains(&Vec3::ZERO));
        assert!(kept.contains(&(Vec3::X * 9000.)));
        assert!(kept.iter().any(|p| (4000.0..=5000.0).contains(&p.x)));
        assert_eq!(cluster(&line, MERGE_DISTANCE, MAX_PROBES).len(), 10);
        assert!(cluster(&[], MERGE_DISTANCE, MAX_PROBES).is_empty());
    }

    #[test]
    fn room_boxes_follow_free_space_within_limits() {
        // A 512 x 256 x 128 room around the origin, open sky above.
        let probe = room(Vec3::ZERO, |_, axis| {
            if axis == Vec3::Z {
                1e9
            } else if axis.x != 0. {
                256.
            } else if axis.y != 0. {
                128.
            } else {
                4.
            }
        });
        assert_eq!(probe.box_max, Vec3::new(256., 128., ROOM_REACH));
        assert_eq!(probe.box_min, Vec3::new(-256., -128., -ROOM_MIN));
    }

    #[test]
    fn surfaces_take_the_nearest_visible_probe() {
        let probe = |x: f32| Probe {
            origin: Vec3::X * x,
            box_min: Vec3::ZERO,
            box_max: Vec3::ZERO,
        };
        let probes = [probe(0.), probe(100.), probe(300.)];
        assert_eq!(nearest(&probes, Vec3::X * 90., |_, _| true), Some(1));
        // The nearest is behind a wall: the next one in view.
        assert_eq!(
            nearest(&probes, Vec3::X * 90., |_, to| to.x != 100.),
            Some(0)
        );
        // None in view: the nearest anyway.
        assert_eq!(nearest(&probes, Vec3::X * 290., |_, _| false), Some(2));
        assert_eq!(nearest(&[], Vec3::ZERO, |_, _| true), None);
    }

    #[test]
    fn probe_indices_use_the_spare_frame_byte() {
        let mut frames = [[1, 0x00_7f_00_00], [2, 0x00_00_7f_00], [3, 0x00_00_00_7f]];
        mark_vertices(&mut frames, &[0, 2, 2], 0..3, Some(4));
        assert_eq!(frames[0], [1, 0x05_7f_00_00]);
        assert_eq!(frames[1], [2, 0x00_00_7f_00]);
        assert_eq!(frames[2], [3, 0x05_00_00_7f]);
        mark_vertices(&mut frames, &[2], 0..1, None);
        assert_eq!(frames[2], [3, 0x00_00_00_7f]);
    }

    #[test]
    fn face_sizes_are_powers_of_two_in_range() {
        assert_eq!(face_size(128), 128);
        assert_eq!(face_size(200), 128);
        assert_eq!(face_size(1), MIN_SIZE);
        assert_eq!(face_size(100_000), MAX_SIZE);
        assert_eq!(face_size(-5), MIN_SIZE);
    }

    #[test]
    fn only_changes_after_the_sample_ask_for_a_restart() {
        assert!(!restart_needed(0, 0));
        assert!(!restart_needed(LATCHED | 128, 128));
        assert!(restart_needed(LATCHED | 128, 0));
        assert!(restart_needed(LATCHED, 128));
        assert!(restart_needed(LATCHED | 128, 256));
    }

    #[test]
    fn small_lighting_changes_keep_the_probes() {
        let base: Signature = [0., 0., 1., 1., 0.2, 0.3, 0.4, 1., 1.];
        assert!(!relit(&base, &base));
        let mut turned = base;
        turned[0] = 1f32.to_radians().sin();
        assert!(relit(&base, &turned));
        let mut nudged = base;
        nudged[0] = 0.1f32.to_radians().sin();
        assert!(!relit(&base, &nudged));
        let mut brighter = base;
        brighter[3] = 1.05;
        assert!(relit(&base, &brighter));
        let mut sky = base;
        sky[5] = 0.301;
        assert!(!relit(&base, &sky));
        // A sunless map turning its unused sun is no change.
        let mut dark = base;
        dark[3] = 0.;
        let mut dark_turned = dark;
        dark_turned[0] = 1.;
        assert!(!relit(&dark, &dark_turned));
    }
}
