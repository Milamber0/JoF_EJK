//! Saber-move presentation data shared by the legacy adapter.

/// Return the authored saber-trail duration column for one `saberMove`.
///
/// The 162 entries are the final `trailLen` column of OpenJK
/// `codemp/game/bg_saber.c:148-376`, in `saberMoveName_t` order from
/// `codemp/game/bg_public.h:1280-1471`. The renderer receives only this
/// presentation duration and never needs the legacy `LS_*` ordinals.
pub fn legacy_saber_trail_length(saber_move: u32) -> u16 {
    u16::try_from(saber_move)
        .ok()
        .and_then(crate::saber_move_data::saber_move)
        .map_or(0, |movement| movement.trail_len)
}
