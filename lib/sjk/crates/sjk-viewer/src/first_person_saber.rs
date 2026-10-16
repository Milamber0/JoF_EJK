//! First-person saber body visibility, from codemp CG_Player and JoF EJK's
//! CG_ForceFPLSPlayerModel: retain the posed body/arms, hide the head and hoses.

/// The local saber body is visible from its own first-person camera.
pub(crate) fn visible(
    local: bool,
    third_person: bool,
    detached: bool,
    weapon: Option<u8>,
    player: Option<&sjk_protocol::PlayerState>,
) -> bool {
    // codemp bg_public.h: EF_DEAD=1<<1, PMF_FOLLOW=4096, PM_SPECTATOR=4.
    local
        && !third_person
        && !detached
        && weapon == Some(3)
        && player.is_some_and(|player| {
            player.health() > 0
                && player.entity_flags() & 2 == 0
                && player.movement_flags() & 4096 == 0
                && !matches!(player.movement_type(), 4 | 5 | 7 | 8)
        })
}

/// JoF EJK's first-person head variants and TIE pilot hoses.
pub(crate) fn hide_surface(name: &str) -> bool {
    name.get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("head"))
        || name.eq_ignore_ascii_case("torso_l_hose")
        || name.eq_ignore_ascii_case("torso_r_hose")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn player() -> sjk_protocol::PlayerState {
        let mut player = sjk_protocol::PlayerState::default();
        player.stats[0] = 100;
        player
    }
    #[test]
    fn only_a_living_players_own_saber_view_draws_the_body() {
        let mut player = player();
        assert!(visible(true, false, false, Some(3), Some(&player)));
        assert!(!visible(true, true, false, Some(3), Some(&player)));
        assert!(!visible(true, false, true, Some(3), Some(&player)));
        assert!(!visible(true, false, false, Some(4), Some(&player)));
        assert!(player.set_raw_field(17, 2));
        assert!(!visible(true, false, false, Some(3), Some(&player)));
        assert!(player.set_raw_field(17, 0));
        player.stats[0] = 0;
        assert!(!visible(true, false, false, Some(3), Some(&player)));
        player.stats[0] = 100;
        player.set_movement_flags(4096);
        assert!(!visible(true, false, false, Some(3), Some(&player)));
        player.set_movement_flags(0);
        for movement in [4, 5, 7, 8] {
            player.set_movement_type(movement);
            assert!(!visible(true, false, false, Some(3), Some(&player)));
        }
    }
    #[test]
    fn head_variants_and_hoses_are_masked_without_hiding_arms() {
        for name in ["head", "heada_face", "headb_eyes_mouth", "torso_l_hose"] {
            assert!(hide_surface(name));
        }
        for name in ["r_arm", "l_hand", "torso", "hips", "h"] {
            assert!(!hide_surface(name));
        }
    }
}
