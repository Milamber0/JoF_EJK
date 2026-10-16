//! Native support for one-piece MD3 NPCs through stock `CG_General` presentation.
//! The actor retains its NPC class and simulation; only its published entity changes
//! from `ET_NPC` to `ET_GENERAL`. Protocol fields and retail assets are unchanged.
use crate::npc_parms::NpcParms;
use sjk_protocol::EntityState;

/// A verified rigid asset for one NPC definition. Only the native profile installs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RigidModel {
    /// NPC block name, matched without case.
    pub npc: Vec<u8>,
    /// Existing virtual asset path; never a generated or copied retail model.
    pub path: Vec<u8>,
}

/// Find single-part NPC definitions whose asset exists as a rigid MD3. Existing
/// Ghoul2 models win. `rigid` must validate that a candidate needs neither skeletal
/// animation nor a multipart assembly before accepting it.
pub fn discover(
    parms: &NpcParms,
    exists: impl Fn(&str) -> bool,
    rigid: impl Fn(&str) -> bool,
) -> Vec<RigidModel> {
    let mut models = Vec::new();
    for name in crate::npc_names::npc_names(parms.text()) {
        let Ok(mut parser) = parms.block(&name) else {
            continue;
        };
        let (mut player, mut legs) = (Vec::new(), Vec::new());
        let (mut multipart, mut vehicle) = (false, false);
        loop {
            let key = parser.parse_ext(true);
            if key.is_empty() || key == b"}" {
                break;
            }
            let key = key.to_ascii_lowercase();
            match key.as_slice() {
                b"playermodel" => player = parser.parse_string().to_vec(),
                b"legsmodel" => legs = parser.parse_string().to_vec(),
                b"headmodel" | b"torsomodel" => {
                    multipart |= !parser.parse_string().eq_ignore_ascii_case(b"none")
                }
                b"class" => vehicle |= parser.parse_string().eq_ignore_ascii_case(b"CLASS_VEHICLE"),
                _ => parser.skip_rest_of_line(),
            }
        }
        if multipart || vehicle {
            continue;
        }
        if !player.is_empty()
            && exists(&format!(
                "models/players/{}/model.glm",
                String::from_utf8_lossy(&player)
            ))
        {
            continue;
        }
        let source = if player.is_empty() { &legs } else { &player };
        if source.is_empty() {
            continue;
        }
        let source = String::from_utf8_lossy(source);
        // Retail's seeker names `remote`; its shipped one-piece mesh is the same
        // model the seeker holdable uses, under models/items rather than players.
        for path in [
            format!("models/players/{source}/lower.md3"),
            format!("models/items/{source}.md3"),
        ] {
            if rigid(&path) {
                models.push(RigidModel {
                    npc: name,
                    path: path.into_bytes(),
                });
                break;
            }
        }
    }
    models
}

/// Project a rigid NPC into the original client's MD3-capable general-model path.
/// The caller has already copied the actor's state; its origin, angles, collision,
/// scale, health and events are preserved. No extra entity or protocol extension.
pub fn project(state: &mut EntityState) {
    use crate::npc_spawn::es;
    state.set_raw_field(es::TYPE, 0);
    state.set_raw_field(es::MODEL_GHOUL2, 0);
    // CG_CalcEntityLerpPositions only interpolates TR_LINEAR_STOP implicitly
    // for client slots and ET_NPC. Our ET_GENERAL body must request it explicitly;
    // otherwise its command-time extrapolation diverges from the camera's snapshots.
    state.set_raw_field(es::POS_TYPE, 1); // TR_INTERPOLATE
    // NPC saber colours share boltToPlayer, which CG_General treats as attachment.
    state.set_raw_field(es::BOLT_TO_PLAYER, 0);
    state.set_raw_field(es::NPC_SABER1, 0);
    state.set_raw_field(es::NPC_SABER2, 0);
}
