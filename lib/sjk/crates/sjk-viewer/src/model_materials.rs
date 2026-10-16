//! Thin compatibility adapter for cgame's entity-level material overrides.
//!
//! All shader parsing, stage compilation, texture upload, and pipeline state
//! live in `world_materials`/`world_stage`. This module only ensures that the
//! shaders selected directly by cgame are present in that shared material
//! table. Item overrides follow `cg_ents.c:2162-2197`; player force shells
//! follow `cg_players.c:9673-9723,10857-11119`, and the shaders of
//! `CG_DrawPlayerSphere` (`player_spheres.rs`) share the same table, as do SJK's
//! player outlines (`card_outline.rs`).

use super::ViewerMaterial;

/// Shared-table indices for the custom shaders selected by `CG_Item`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Overrides {
    pub(crate) placeholder: usize,
    pub(crate) light_disabled: usize,
    pub(crate) dark_disabled: usize,
    /// `cgs.media.disruptorShader`, `CG_Disintegration`'s burning pass.
    pub(crate) disruptor_burn: usize,
    force: [usize; 15],
    /// The player card outline's mask ([`crate::card_outline`]).
    pub(crate) card_outline: usize,
    /// The team outlines' masks, teammates' and enemies'.
    pub(crate) friend_outline: usize,
    pub(crate) enemy_outline: usize,
}

impl Overrides {
    /// The outline mask material of `group`.
    pub(crate) const fn outline(&self, group: crate::card_outline::Group) -> usize {
        match group {
            crate::card_outline::Group::Card => self.card_outline,
            crate::card_outline::Group::Friend => self.friend_outline,
            crate::card_outline::Group::Enemy => self.enemy_outline,
        }
    }

    /// Every outline mask material with the value its bodies write into the mask.
    pub(crate) fn outline_groups(&self) -> [(usize, f64); 3] {
        crate::card_outline::Group::ALL.map(|group| (self.outline(group), group.mask_value()))
    }
}

/// Append or reuse the cgame entity-override materials before the shared Q3
/// stage runtime is built.
pub(crate) fn append_overrides(materials: &mut Vec<ViewerMaterial>) -> Overrides {
    let mut result = Overrides {
        placeholder: ensure(materials, "powerups/placeholder"),
        light_disabled: ensure(materials, "gfx/misc/mp_light_enlight_disable"),
        dark_disabled: ensure(materials, "gfx/misc/mp_dark_enlight_disable"),
        disruptor_burn: ensure(materials, crate::disintegration::BURN_SHADER),
        force: [0; 15],
        card_outline: ensure(materials, crate::card_outline::Group::Card.shader()),
        friend_outline: ensure(materials, crate::card_outline::Group::Friend.shader()),
        enemy_outline: ensure(materials, crate::card_outline::Group::Enemy.shader()),
    };
    for (slot, shader) in result.force.iter_mut().zip(FORCE_SHADERS) {
        *slot = ensure(materials, shader);
    }
    result
}

const FORCE_SHADERS: [&str; 15] = [
    "halfShieldShell",
    "gfx/misc/electric",
    "gfx/misc/fullbodyelectric2",
    "gfx/misc/sightbubble",
    "gfx/misc/forceprotect",
    "gfx/misc/personalshield",
    "powerups/forceshell",
    "powerups/sightshell",
    "powerups/ysalimarishell",
    "powerups/ysaliredshell",
    "powerups/ysaliblueshell",
    "powerups/boonshell",
    "powerups/endarkenmentshell",
    "powerups/enlightenmentshell",
    "powerups/invulnerabilityshell",
];

impl Overrides {
    pub(crate) fn force(&self, shader: &str) -> Option<usize> {
        FORCE_SHADERS
            .iter()
            .position(|candidate| candidate.eq_ignore_ascii_case(shader))
            .map(|index| self.force[index])
    }
}

fn ensure(materials: &mut Vec<ViewerMaterial>, shader: &str) -> usize {
    if let Some(index) = materials
        .iter()
        .position(|material| material.lightmap < 0 && material.shader.eq_ignore_ascii_case(shader))
    {
        return index;
    }
    let index = materials.len();
    materials.push(ViewerMaterial {
        shader: shader.to_owned(),
        lightmap: -1,
    });
    index
}
