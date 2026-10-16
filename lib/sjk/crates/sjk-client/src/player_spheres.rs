//! The spheres `CG_Player` draws around a player, and the jetpack flags it reads.
//!
//! Spheres follow `CG_Player` in OpenJK `codemp/cgame/cg_players.c` (JoF EJK
//! 13675-13718): the Ysalamiri shell (or, in Capture the Ysalamiri, the carrier's
//! team-coloured shell), Force Boon, Enlightenment light or dark, then the green
//! spawn-protection shell of `EF_INVULNERABLE`. Each one is a `CG_DrawPlayerSphere`
//! of `models/weaphits/testboom.md3` with a custom shader; the dead get none
//! (`cg_players.c:8849-8853`). The jetpack bits are the ones `CG_Player` tests before
//! bolting `models/weapons2/jetpack/model.glm` on (`cg_players.c:11843-11935`).

const EF_DEAD: u32 = 1 << 1;
const EF_JETPACK_ACTIVE: u32 = 1 << 11; // codemp/game/bg_public.h
const EF_INVULNERABLE: u32 = 1 << 27;
const EF_JETPACK: u32 = 1 << 29;
const EF_JETPACK_FLAMING: u32 = 1 << 30;
// `powerup_t` with `BASE_COMPAT` (`bg_public.h`).
const PW_REDFLAG: u32 = 4;
const PW_BLUEFLAG: u32 = 5;
const PW_FORCE_ENLIGHTENED_LIGHT: u32 = 12;
const PW_FORCE_ENLIGHTENED_DARK: u32 = 13;
const PW_FORCE_BOON: u32 = 14;
const PW_YSALAMIRI: u32 = 15;
const GT_CTY: i32 = 9;

/// The rigid model every player sphere is drawn with (`cgs.media.halfShieldModel`).
pub const LEGACY_PLAYER_SPHERE_MODEL: &str = "models/weaphits/testboom.md3";

/// Every shader a player sphere can use, for registration.
pub const LEGACY_PLAYER_SPHERE_SHADERS: [&str; 7] = [
    "powerups/ysalimarishell",
    "powerups/ysaliredshell",
    "powerups/ysaliblueshell",
    "powerups/boonshell",
    "powerups/endarkenmentshell",
    "powerups/enlightenmentshell",
    "powerups/invulnerabilityshell",
];

/// One `CG_DrawPlayerSphere` call.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyPlayerSphere {
    /// Custom shader of the sphere.
    pub shader: &'static str,
    /// Uniform scale of the sphere's axes.
    pub scale: f32,
}

/// The spheres around one player, in `CG_Player`'s order. `powerups` is the
/// entity-state bit set (a player state's timers mapped to bits).
pub fn legacy_player_spheres(
    e_flags: u32,
    powerups: u32,
    gametype: i32,
) -> [Option<LegacyPlayerSphere>; 4] {
    let mut spheres = [None; 4];
    if e_flags & EF_DEAD != 0 {
        return spheres;
    }
    let has = |powerup: u32| powerups & (1 << powerup) != 0;
    let sphere = |shader, scale| Some(LegacyPlayerSphere { shader, scale });
    let cty = gametype == GT_CTY;
    spheres[0] = if cty && has(PW_REDFLAG) {
        sphere("powerups/ysaliredshell", 1.4)
    } else if cty && has(PW_BLUEFLAG) {
        sphere("powerups/ysaliblueshell", 1.4)
    } else if has(PW_YSALAMIRI) {
        sphere("powerups/ysalimarishell", 1.4)
    } else {
        None
    };
    if has(PW_FORCE_BOON) {
        spheres[1] = sphere("powerups/boonshell", 2.0);
    }
    if has(PW_FORCE_ENLIGHTENED_DARK) {
        spheres[2] = sphere("powerups/endarkenmentshell", 2.0);
    } else if has(PW_FORCE_ENLIGHTENED_LIGHT) {
        spheres[2] = sphere("powerups/enlightenmentshell", 2.0);
    }
    if e_flags & EF_INVULNERABLE != 0 {
        spheres[3] = sphere("powerups/invulnerabilityshell", 1.0);
    }
    spheres
}

/// What `CG_Player` draws of a player's jetpack.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyJetpack {
    /// `EF_JETPACK` on a living player: the pack is bolted to `*chestg`.
    pub worn: bool,
    /// `EF_JETPACK_ACTIVE`: the jets burn and the hover loop plays.
    pub active: bool,
    /// `EF_JETPACK_FLAMING`: each jet plays twice and the fire loop plays.
    pub flaming: bool,
}

impl LegacyJetpack {
    /// Read the jetpack bits of an entity's (or a player state's) `eFlags`.
    pub const fn from_flags(e_flags: u32) -> Self {
        let worn = e_flags & EF_JETPACK != 0 && e_flags & EF_DEAD == 0;
        let active = worn && e_flags & EF_JETPACK_ACTIVE != 0;
        Self {
            worn,
            active,
            flaming: active && e_flags & EF_JETPACK_FLAMING != 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shaders(spheres: [Option<LegacyPlayerSphere>; 4]) -> Vec<&'static str> {
        spheres
            .iter()
            .flatten()
            .map(|sphere| sphere.shader)
            .collect()
    }

    #[test]
    fn spawn_protection_draws_the_green_shell() {
        let spheres = legacy_player_spheres(EF_INVULNERABLE, 0, 0);
        assert_eq!(
            spheres[3],
            Some(LegacyPlayerSphere {
                shader: "powerups/invulnerabilityshell",
                scale: 1.0
            })
        );
        assert_eq!(shaders(spheres).len(), 1);
    }

    #[test]
    fn the_dead_wear_no_sphere() {
        let flags = EF_INVULNERABLE | EF_DEAD;
        assert!(shaders(legacy_player_spheres(flags, 1 << PW_YSALAMIRI, 0)).is_empty());
    }

    #[test]
    fn spheres_keep_cg_player_order() {
        let powerups = 1 << PW_YSALAMIRI | 1 << PW_FORCE_BOON | 1 << PW_FORCE_ENLIGHTENED_LIGHT;
        assert_eq!(
            shaders(legacy_player_spheres(EF_INVULNERABLE, powerups, 0)),
            [
                "powerups/ysalimarishell",
                "powerups/boonshell",
                "powerups/enlightenmentshell",
                "powerups/invulnerabilityshell",
            ]
        );
    }

    #[test]
    fn darkness_wins_over_light() {
        let powerups = 1 << PW_FORCE_ENLIGHTENED_LIGHT | 1 << PW_FORCE_ENLIGHTENED_DARK;
        assert_eq!(
            shaders(legacy_player_spheres(0, powerups, 0)),
            ["powerups/endarkenmentshell"]
        );
    }

    #[test]
    fn capture_the_ysalamiri_colours_the_carrier() {
        assert_eq!(
            shaders(legacy_player_spheres(0, 1 << PW_BLUEFLAG, GT_CTY)),
            ["powerups/ysaliblueshell"]
        );
        // Outside CTY a flag carrier wears no sphere.
        assert!(shaders(legacy_player_spheres(0, 1 << PW_BLUEFLAG, 8)).is_empty());
    }

    #[test]
    fn jetpack_bits() {
        assert_eq!(LegacyJetpack::from_flags(0), LegacyJetpack::default());
        let flying = LegacyJetpack::from_flags(EF_JETPACK | EF_JETPACK_ACTIVE);
        assert!(flying.worn && flying.active && !flying.flaming);
        let burning =
            LegacyJetpack::from_flags(EF_JETPACK | EF_JETPACK_ACTIVE | EF_JETPACK_FLAMING);
        assert!(burning.flaming);
        // Active without the pack, or dead, draws nothing.
        assert!(!LegacyJetpack::from_flags(EF_JETPACK_ACTIVE).active);
        assert!(!LegacyJetpack::from_flags(EF_JETPACK | EF_DEAD).worn);
    }
}
