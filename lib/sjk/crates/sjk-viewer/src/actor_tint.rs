//! Per-instance entity colour without legacy render-effect overrides.
//!
//! BaseJKA copies player `shaderRGBA` onto the player refent, while attached
//! weapon refents are initialized independently by `CG_AddPlayerWeapon`
//! (`codemp/cgame/cg_weapons.c:421-498`). Consequently this helper changes
//! only the actor instance and deliberately leaves `entity_control` clear:
//! ordinary `shaderRGBA` is consumed only by entity-aware shader generators.

use super::ActorInstance;

impl ActorInstance {
    /// Supply shader entity colour without forcing `CGEN_ENTITY`.
    pub(super) fn with_entity_color(mut self, color: [u8; 4]) -> Self {
        self.entity_color = color.map(|channel| f32::from(channel) / 255.0);
        self
    }

    /// Apply rd-vanilla's `RF_RGB_TINT` override for callers that request it.
    pub(super) fn with_rgb_tint(mut self, color: [f32; 4]) -> Self {
        self.entity_color = color;
        self.entity_control[0] = 1.0;
        self
    }

    /// Apply rd-vanilla's RGB and alpha render-effect overrides.
    pub(super) fn with_rgba_tint(mut self, color: [f32; 4]) -> Self {
        self.entity_color = color;
        self.entity_control = [1.0; 2];
        self
    }

    /// Apply rd-vanilla's `RF_FORCE_ENT_ALPHA` vertex alpha (`ForceAlpha`,
    /// `tr_shade.cpp:1546-1556`), keeping the entity's RGB and RGB generators.
    /// The draw must also select the forced-alpha pipeline.
    pub(super) fn with_forced_alpha(mut self, alpha: u8) -> Self {
        self.entity_color[3] = f32::from(alpha) / 255.0;
        self.entity_control[1] = 1.0;
        self
    }
}
