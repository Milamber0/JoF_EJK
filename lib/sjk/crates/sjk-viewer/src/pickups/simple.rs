//! Stock CG_Item sprite path, cg_ents.c:1965-2000; icons from bg_misc.c:710-1686.
use super::*;
use crate::particle_types::PrimitiveShape;

/// Disabled enlightenment sprites selected by CG_Item (cg_ents.c:2005-2014).
pub(crate) const DISABLED_ICONS: [&str; 2] = [
    "gfx/misc/mp_light_enlight_disable",
    "gfx/misc/mp_dark_enlight_disable",
];

/// Stock placeholder pulse and disabled-power tint (cg_ents.c:1980-2016).
pub(super) fn color(flags: u32, grey: bool, at_time: i32) -> [f32; 4] {
    if grey {
        [100.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0, 200.0 / 255.0]
    } else if flags & EF_ITEMPLACEHOLDER != 0 {
        let alpha = (150.0 + (at_time as f32 * 0.01).sin() * 30.0) / 255.0;
        [200.0 / 255.0, 200.0 / 255.0, 200.0 / 255.0, alpha]
    } else {
        [1.0; 4]
    }
}

/// bg_itemlist icon names indexed by the protocol item number.
pub(crate) const ICONS: [&str; 51] = [
    "",
    "gfx/mp/small_shield",
    "gfx/mp/large_shield",
    "gfx/hud/i_icon_medkit",
    "gfx/hud/i_icon_seeker",
    "gfx/hud/i_icon_shieldwall",
    "gfx/hud/i_icon_bacta",
    "gfx/hud/i_icon_big_bacta",
    "gfx/hud/i_icon_zoom",
    "gfx/hud/i_icon_sentrygun",
    "gfx/hud/i_icon_jetpack",
    "gfx/hud/i_icon_healthdisp",
    "gfx/hud/i_icon_ammodisp",
    "gfx/hud/i_icon_eweb",
    "gfx/hud/i_icon_cloak",
    "gfx/hud/mpi_jlight",
    "gfx/hud/mpi_dklight",
    "gfx/hud/mpi_fboon",
    "gfx/hud/mpi_ysamari",
    "gfx/hud/w_icon_stunbaton",
    "gfx/hud/w_icon_melee",
    "gfx/hud/w_icon_lightsaber",
    "gfx/hud/w_icon_blaster_pistol",
    "gfx/hud/w_icon_c_rifle",
    "gfx/hud/w_icon_briar",
    "gfx/hud/w_icon_blaster",
    "gfx/hud/w_icon_disruptor",
    "gfx/hud/w_icon_bowcaster",
    "gfx/hud/w_icon_repeater",
    "gfx/hud/w_icon_demp2",
    "gfx/hud/w_icon_flechette",
    "gfx/hud/w_icon_merrsonn",
    "gfx/hud/w_icon_thermal",
    "gfx/hud/w_icon_tripmine",
    "gfx/hud/w_icon_detpack",
    "gfx/hud/w_icon_thermal",
    "gfx/hud/w_icon_tripmine",
    "gfx/hud/w_icon_detpack",
    "gfx/hud/w_icon_blaster",
    "gfx/hud/w_icon_blaster",
    "gfx/hud/w_icon_blaster",
    "gfx/hud/i_icon_battery",
    "gfx/mp/ammo_power_cell",
    "gfx/mp/ammo_metallic_bolts",
    "gfx/mp/ammo_rockets",
    "gfx/mp/ammo_rockets",
    "gfx/hud/mpi_rflag",
    "gfx/hud/mpi_bflag",
    "icons/iconf_neutral1",
    "icons/iconh_rorb",
    "icons/iconh_borb",
];

/// Team objectives keep their models, even with simple items enabled.
pub(crate) fn uses_icon(index: usize, enabled: bool) -> bool {
    enabled && (1..46).contains(&index)
}

/// Select the icon path before item-cone submission.
pub(crate) fn prepare(items: &mut Vec<Presented>, console: Option<&crate::console::ViewerConsole>) {
    let enabled = console
        .and_then(|c| c.bool_cvar("cg_simpleitems"))
        .unwrap_or(false);
    for item in items {
        item.simple = uses_icon(item.item_index, enabled);
        if item.simple {
            item.holo = None;
        }
    }
}

/// Replace last frame's icons and append this frame's bounded model/sprite instances.
/// Clearing drops every frame billboard, including the player sprites that actor
/// submission appends after this (`player_sprites.rs`).
pub(crate) fn append_frame(gpu: &mut crate::GpuState, now: Instant) {
    gpu.particles
        .retain(|p| !matches!(p.shape, PrimitiveShape::FrameBillboard));
    append_instances(
        &gpu.pickups,
        &gpu.pickup_catalog,
        &gpu.object_meshes,
        &mut gpu.object_groups,
        gpu.model_material_overrides,
        &mut gpu.pickup_override_instances,
    );
    for item in gpu.pickups.iter().filter(|item| item.simple) {
        if !crate::particle_types::frame_billboard_fits(gpu.particles.len()) {
            break;
        }
        // Force-boon placeholders are omitted by the stock sprite branch.
        if item.placeholder && item.item_index == 17 {
            continue;
        }
        let constant = |value| {
            crate::effect_envelope::Envelope::from_values(
                value,
                value,
                0.0,
                sjk_effect::CurveFlags::default(),
            )
        };
        let shader = match item.material_override {
            Some(MaterialOverride::LightDisabled) => DISABLED_ICONS[0],
            Some(MaterialOverride::DarkDisabled) => DISABLED_ICONS[1],
            _ => ICONS[item.item_index],
        };
        gpu.particles.push(Particle {
            motion: crate::particle_motion::Motion::new(
                Vec3::from_array(item.simple_origin),
                Vec3::ZERO,
                Vec3::ZERO,
                0.0,
                0.0,
                now,
            ),

            spawned_at: now,
            delay: Duration::ZERO,
            lifetime: Duration::from_secs(60),
            size: constant(14.0),
            start_length: 1.0,
            end_length: 1.0,
            streak: None,
            trace_streak: false,
            normal: None,
            alpha: constant(item.simple_color[3]),
            use_alpha: true,
            set_shader_time: false,
            rgb: [constant(item.simple_color[0]); 3],
            seed: 0,
            shader: gpu.effects.shader(shader),
            physics: crate::particle_physics::State::new(
                gpu.effects.code_primitive_definition(),
                0,
                0,
                0.0,
                sjk_effect::PrimitiveFlags::default(),
            ),
            shape: PrimitiveShape::FrameBillboard,
        });
    }
}
