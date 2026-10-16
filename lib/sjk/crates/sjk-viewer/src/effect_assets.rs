//! Discovery of shaders referenced by stock and server-configured EFX graphs.

use sjk_effect::load_effect;
use sjk_vfs::VirtualFileSystem;
use std::collections::{BTreeSet, HashSet};

pub(crate) const STOCK_EFFECTS: &[&str] = &[
    "mp/drain",
    "mp/drainwide",
    "emplaced/dead_smoke",
    "emplaced/explode",
    "turret/explode",
    "sparks/spark_explosion",
    "tripmine/explosion",
    "detpack/explosion",
    "flechette/alt_blow",
    "stunbaton/flesh_impact",
    "sparks/spark_exp_nosnd",
    "env/water_impact",
    "env/acid_splash",
    "env/lava_splash",
    "materials/mud_large",
    "materials/sand_large",
    "materials/dirt_large",
    "materials/snow_large",
    "materials/gravel_large",
    // `cgs.effects.footstep*` (`cg_main.c:1207-1210`): the dust a step kicks up.
    "materials/mud",
    "materials/sand",
    "materials/snow",
    "materials/gravel",
    "bryar/flesh_impact",
    "bryar/wall_impact",
    "bryar/wall_impact2",
    "bryar/wall_impact3",
    "blaster/flesh_impact",
    "blaster/wall_impact",
    "blaster/deflect",
    "disruptor/flesh_impact",
    "disruptor/wall_impact",
    "disruptor/alt_miss",
    "disruptor/alt_hit",
    // `cgs.effects.mDisruptorDeathSmoke`, puffed by `CG_Disintegration`.
    "disruptor/death_smoke",
    "bowcaster/explosion",
    "repeater/flesh_impact",
    "repeater/wall_impact",
    "repeater/concussion",
    "demp2/flesh_impact",
    "demp2/wall_impact",
    "demp2/altdetonate",
    "flechette/wall_impact",
    "flechette/flesh_impact",
    "rocket/explosion",
    "thermal/explosion",
    "thermal/shockwave",
    "concussion/explosion",
    "sparks/spark_nosnd",
    "bryar/shot",
    "blaster/shot",
    "bowcaster/shot",
    "repeater/projectile",
    "repeater/alt_projectile",
    "demp2/projectile",
    "concussion/muzzle_flash",
    "concussion/altmuzzle_flash",
    "concussion/alt_ring",
    "bryar/muzzle_flash",
    "blaster/muzzle_flash",
    "disruptor/muzzle_flash",
    "bowcaster/muzzle_flash",
    "repeater/muzzle_flash",
    "repeater/altmuzzle_flash",
    "demp2/muzzle_flash",
    "demp2/altmuzzle_flash",
    "flechette/muzzle_flash",
    "flechette/altmuzzle_flash",
    "rocket/muzzle_flash",
    "rocket/altmuzzle_flash",
    "saber/saber_cut",
    "saber/fizz",
    "saber/saber_block",
    "saber/blood_sparks_mp",
    "saber/blood_sparks_25_mp",
    "saber/blood_sparks_50_mp",
    "mp/itemcone",
    "force/lightning",
    "force/lightningwide",
    "force/confusion_old",
    // JoF EJK's `CG_ForceAnimFx` sprays (`cgs.effects.rageFX`, `heal2FX`), which the
    // in-game profile's Force page shows on its model (`menu_stage/force_fx.rs`).
    "force/rage2",
    "force/heal2",
    "mp/spawn",
    "mp/jedispawn",
    "chunks/grateexplode",
];

/// Shaders referenced from code rather than from EFX graphs: the saber clash
/// flare (`cg_draw.c`), the disruptor beam lines (`fx_disruptor.c`), the
/// sprites floated over players (`cg_players.c`) and the `white` lines of
/// `CG_TestLine` (JA+ grapple ropes, `grapple_rope.rs`).
pub(crate) const CODE_SHADERS: &[&str] = &[
    "gfx/damage/rivetmark",
    "gfx/effects/saberdamageglow",
    "gfx/misc/spark",
    "gfx/effects/forcePush",
    "gfx/effects/sabers/red_glow",
    "gfx/effects/saberFlare",
    "gfx/effects/redLine",
    "gfx/misc/whiteline2",
    "gfx/effects/blueLine",
    "white",
    sjk_client::LegacyPlayerSprite::ConnectionInterrupted.shader(),
    sjk_client::LegacyPlayerSprite::Talk.shader(),
    crate::charge_flash::SHADERS[0],
    crate::charge_flash::SHADERS[1],
    crate::charge_flash::SHADERS[2],
    // The footprints (`cgs.media.fs*MarkShader`, `footprints.rs`).
    "footstep_r",
    "footstep_l",
    "footstep_heavy_r",
    "footstep_heavy_l",
];

pub(crate) fn required_shaders<'a>(
    vfs: &VirtualFileSystem,
    configured_effects: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut pending = STOCK_EFFECTS
        .iter()
        .map(|effect| (*effect).to_owned())
        .collect::<Vec<_>>();
    pending.extend(configured_effects.into_iter().map(str::to_owned));
    let mut visited = HashSet::new();
    let mut shaders = BTreeSet::new();
    while let Some(effect) = pending.pop() {
        if !visited.insert(effect.to_ascii_lowercase()) {
            continue;
        }
        let Ok(definition) = load_effect(vfs, &effect) else {
            continue;
        };
        for component in definition.components {
            shaders.extend(component.shaders);
            pending.extend(component.effects);
            pending.extend(component.impact_effects);
            pending.extend(component.death_effects);
            pending.extend(component.emit_effects);
        }
    }
    shaders.extend(CODE_SHADERS.iter().map(|shader| (*shader).to_owned()));
    shaders.extend(
        crate::pickups::simple::ICONS
            .iter()
            .chain(crate::pickups::simple::DISABLED_ICONS.iter())
            .filter(|s| !s.is_empty())
            .map(|s| (*s).to_owned()),
    );
    shaders
}

pub(crate) fn required_models<'a>(
    vfs: &VirtualFileSystem,
    configured_effects: impl IntoIterator<Item = &'a str>,
) -> BTreeSet<String> {
    let mut pending = STOCK_EFFECTS
        .iter()
        .map(|effect| (*effect).to_owned())
        .collect::<Vec<_>>();
    pending.extend(configured_effects.into_iter().map(str::to_owned));
    let mut visited = HashSet::new();
    let mut models = BTreeSet::new();
    while let Some(effect) = pending.pop() {
        if !visited.insert(effect.to_ascii_lowercase()) {
            continue;
        }
        let Ok(definition) = load_effect(vfs, &effect) else {
            continue;
        };
        for component in definition.components {
            models.extend(component.models);
            pending.extend(component.effects);
            pending.extend(component.impact_effects);
            pending.extend(component.death_effects);
            pending.extend(component.emit_effects);
        }
    }
    models
}
