//! Network actor scale is presentation policy, not movement collision policy.

/// Apply CG_Player's iModelScale and non-vehicle origin correction to a fresh frame root.
///
/// OpenJK codemp/cgame/cg_players.c:8479-8493; CLASS_VEHICLE is teams.h:93.
/// Zero is the network's unit-scale sentinel, not a singular draw transform.
pub(crate) fn apply(root: &mut sjk_runtime::Transform, percent: i32, npc_class: u8) {
    if percent == 0 {
        return;
    }
    let scale = percent as f32 / 100.0;
    root.scale = [scale; 3];
    if npc_class != 53 && scale != 0.0 && scale != 1.0 {
        root.translation[2] += 24.0 * (scale - 1.0);
    }
}
