//! A gunner's view on an emplaced gun (`CG_EmplacedView`, OpenJK
//! `codemp/cgame/cg_view.c:2071-2105`, with `SetClientForceAngle` in
//! `client/cl_input.cpp:1266-1272`): while the player sits at a gun and looks more than a
//! degree past the gun's arc (`BG_EmplacedView` answering 2), its view is turned back to
//! the arc's edge, so the commands it sends aim where the gun can. The server holds the
//! shots to the arc whatever the view (`FireWeapon`); this keeps the gunner's own view
//! and crosshair honest.

use sjk_protocol::Snapshot;

/// `s.angles` (pitch, yaw, roll) and `s.origin2[0]` (the gun's arc either side) on the
/// wire.
const ES_ANGLES: [usize; 3] = [25, 9, 24];
const ES_ORIGIN2_X: usize = 56;

/// The yaw, in degrees as `yaw` is, the view is turned back to while its player works the gun `snapshot` names in
/// `emplacedIndex`, looking more than a degree past its arc; `None` otherwise.
pub(crate) fn forced_yaw(snapshot: &Snapshot, yaw: f32) -> Option<f32> {
    let index = snapshot.player.emplaced_index();
    if index == 0 {
        return None;
    }
    let gun = snapshot
        .entities
        .iter()
        .find(|entity| entity.number() == index)?;
    let angles = ES_ANGLES.map(|field| f32::from_bits(gun.raw_field(field).unwrap_or(0)));
    let constraint = f32::from_bits(gun.raw_field(ES_ORIGIN2_X).unwrap_or(0));
    let (held, turned) = sjk_game_jka::emplaced::emplaced_view([0.0, yaw, 0.0], angles, constraint);
    (held == 2).then_some(turned)
}
