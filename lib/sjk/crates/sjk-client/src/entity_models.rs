//! Which model, if any, codemp draws from a snapshot entity's `modelindex`.
//!
//! `CG_AddCEntity` (`codemp/cgame/cg_ents.c`) dispatches on `eType`, and only
//! some of the per-type functions turn `modelindex` into a model:
//!
//! | `eType` | codemp function | model from `modelindex` |
//! | --- | --- | --- |
//! | `ET_GENERAL`, `ET_BODY` | `CG_General` | `CS_MODELS` entry, never an inline model |
//! | `ET_HOLOCRON` | `CG_General` | holocron table below -100, else `CS_MODELS` |
//! | `ET_MOVER` | `CG_Mover` | inline model for `SOLID_BMODEL`, else `CS_MODELS` |
//! | `ET_MISSILE` | `CG_Missile` | `CS_MODELS` saber only for `WP_SABER` |
//! | `ET_PLAYER`, `ET_NPC` | `CG_Player`, `CG_G2Animated` | actor path, not here |
//! | `ET_ITEM` | `CG_Item` | `bg_itemlist` index, not here |
//! | every other type | `CG_Special`, `CG_Beam`, `CG_Portal`, `CG_Speaker`, `CG_FX`, none | none |
//!
//! Any other use draws an unrelated map model wherever the entity is: a
//! portable shield (`ET_SPECIAL`) carries `HI_SHIELD` and an `fx_runner`
//! (`ET_FX`) an effect index in the same field.

use sjk_protocol::{EntityState, GameState};
use sjk_runtime::Appearance;

// `entityType_t`, codemp/game/bg_public.h.
const ET_GENERAL: u8 = 0;
const ET_MISSILE: u8 = 3;
const ET_HOLOCRON: u8 = 5;
const ET_MOVER: u8 = 6;
const ET_BODY: u8 = 15;
/// `WP_SABER` (`codemp/game/bg_weapons.h`).
const WP_SABER: u8 = 3;
/// `G2_MODEL_PART` and `G2_MODELPART_HEAD..=G2_MODELPART_RLEG`
/// (`codemp/game/bg_public.h`).
const G2_MODEL_PART: u8 = 50;
const G2_MODELPART_HEAD: u8 = 10;
const G2_MODELPART_RLEG: u8 = 16;
/// `CG_General` returns before drawing anything while `modelGhoul2 == 127`.
const MODEL_GHOUL2_NOT_READY: u8 = 127;
/// `CG_General` draws a force holocron model below this index.
const HOLOCRON_MODEL_LIMIT: i16 = -100;
/// `CG_Missile` falls back to `DEFAULT_SABER_MODEL` (`bg_public.h`).
const DEFAULT_SABER_MODEL: &str = "models/weapons2/saber_1/saber_1.glm";
/// `SOLID_BMODEL` (`codemp/qcommon/q_shared.h`).
const SOLID_BMODEL: u32 = 0x00ff_ffff;

/// The model codemp draws from `modelindex` for a non-actor, non-item entity.
///
/// Returns `None` for entity types whose cgame function never reads
/// `modelindex` as a model, and for the `CG_General` cases that draw nothing
/// or something other than a `CS_MODELS` entry.
pub(crate) fn legacy_entity_model_appearance(
    game_state: &GameState,
    state: &EntityState,
) -> Option<Appearance> {
    let model_index = state.model_index();
    match state.entity_type() {
        ET_GENERAL | ET_BODY | ET_HOLOCRON => {
            if state.model_ghoul2() == MODEL_GHOUL2_NOT_READY
                || is_limb(state.weapon(), state.model_ghoul2())
                || (state.entity_type() == ET_HOLOCRON && model_index < HOLOCRON_MODEL_LIMIT)
            {
                // A held or flying saber, a dismembered limb, or a force
                // holocron's own model table: none of them is the CS_MODELS
                // entry `modelindex` would name.
                return None;
            }
            crate::legacy_model_appearance(game_state, model_index)
        }
        ET_MOVER if state.solid() == SOLID_BMODEL => inline_model_appearance(model_index),
        ET_MOVER => crate::legacy_model_appearance(game_state, model_index),
        ET_MISSILE if state.weapon() == WP_SABER => {
            crate::legacy_model_appearance(game_state, model_index).or_else(|| {
                Some(Appearance {
                    model: DEFAULT_SABER_MODEL.to_owned(),
                    variant: String::new(),
                })
            })
        }
        _ => None,
    }
}

/// The second model `CG_Mover` draws with an `ET_MOVER` (`cg_ents.c:3469-3479`):
/// `modelindex2`, a `CS_MODELS` entry the game set from the entity's `model2` key
/// (`InitMover`, `SP_func_usable`, `g_mover.c`), at the mover's own origin and
/// angles, scaled by `iModelScale` percent when that is set.
///
/// A `func_usable` table in `t2_rancor` is a clip-only brush (no drawn surface) with
/// `model2 models/map_objects/nar_shaddar/table01.md3`: the md3 is all a player sees.
/// `EF_NODRAW` hides it, as it hides the mover's brush in SJK
/// ([`crate::legacy_present_mover`]).
pub fn legacy_mover_model2(
    game_state: &GameState,
    state: &EntityState,
) -> Option<LegacyMoverModel> {
    const EF_NODRAW: u32 = 1 << 8;
    if state.entity_type() != ET_MOVER || state.e_flags() & EF_NODRAW != 0 {
        return None;
    }
    let appearance = crate::legacy_model_appearance(game_state, i16::from(state.model_index2()))?;
    let percent = state.model_scale_percent();
    Some(LegacyMoverModel {
        appearance,
        scale: if percent == 0 {
            1.0
        } else {
            percent as f32 / 100.0
        },
    })
}

/// A mover's second model: what to draw and its uniform scale.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyMoverModel {
    pub appearance: Appearance,
    pub scale: f32,
}

/// A dismembered limb entity's part (`G2_MODELPART_HEAD` 10 to `G2_MODELPART_RLEG` 16)
/// and its owner: `modelindex`, or `otherEntityNum2` when that is negative
/// (`CG_General`, `cg_ents.c`).
pub fn legacy_limb(state: &sjk_protocol::EntityState) -> Option<(u8, u16)> {
    if !is_limb(state.weapon(), state.model_ghoul2()) {
        return None;
    }
    let owner = u16::try_from(state.model_index()).unwrap_or_else(|_| state.other_entity_num2());
    Some((state.model_ghoul2(), owner))
}

/// A dismembered limb (`CG_General`'s client-limb case): `modelindex` holds the
/// owner's entity number (`codemp/game/g_combat.c`), not a model.
fn is_limb(weapon: u8, model_ghoul2: u8) -> bool {
    weapon == G2_MODEL_PART && (G2_MODELPART_HEAD..=G2_MODELPART_RLEG).contains(&model_ghoul2)
}

fn inline_model_appearance(model_index: i16) -> Option<Appearance> {
    let index = usize::try_from(model_index).ok()?;
    (index != 0).then(|| Appearance {
        model: format!("*{index}"),
        variant: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_protocol::{
        LEGACY_ENTITY_FIELDS, MAX_LEGACY_MESSAGE_BYTES, MessageWriter, ServiceCommand,
        decode_initial_gamestate, write_gamestate_block,
    };

    const CS_MODELS: usize = 298;
    const MODEL: &str = "models/map_objects/imperial/crate.md3";
    // entityState_t netfields (`codemp/qcommon/msg.cpp`).
    const TYPE: usize = 8;
    const WEAPON: usize = 14;
    const SOLID: usize = 26;
    const MODEL_INDEX: usize = 46;
    const MODEL_GHOUL2: usize = 54;

    fn game_state() -> GameState {
        let models = [
            (CS_MODELS + 2, MODEL.as_bytes()),
            (
                CS_MODELS + 7,
                b"models/weapons2/saber_2/saber_2.glm".as_slice(),
            ),
        ];
        let mut message = MessageWriter::new(MAX_LEGACY_MESSAGE_BYTES);
        message.write_i32(0).unwrap();
        write_gamestate_block(&mut message, 0, models, [], 0, 0).unwrap();
        message.write_u8(ServiceCommand::End as u8).unwrap();
        decode_initial_gamestate(&message.finish().unwrap())
            .unwrap()
            .game_state
    }

    fn entity(entity_type: u8, model_index: i16) -> EntityState {
        let mut state = EntityState::zero(100, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(TYPE, u32::from(entity_type));
        state.set_raw_field(MODEL_INDEX, model_index as u16 as u32);
        state
    }

    fn model(state: &EntityState) -> Option<String> {
        legacy_entity_model_appearance(&game_state(), state).map(|appearance| appearance.model)
    }

    fn cs_model() -> Option<String> {
        Some(MODEL.to_owned())
    }

    #[test]
    fn general_draws_its_configstring_model() {
        assert_eq!(model(&entity(ET_GENERAL, 2)), cs_model());
        assert_eq!(model(&entity(ET_GENERAL, 0)), None);
    }

    #[test]
    fn general_never_draws_an_inline_model() {
        // CG_General reads cgs.gameModels whatever `solid` says.
        let mut state = entity(ET_GENERAL, 2);
        state.set_raw_field(SOLID, SOLID_BMODEL);
        assert_eq!(model(&state), cs_model());
    }

    #[test]
    fn general_not_ready_ghoul2_draws_nothing() {
        let mut state = entity(ET_GENERAL, 2);
        state.set_raw_field(MODEL_GHOUL2, u32::from(MODEL_GHOUL2_NOT_READY));
        assert_eq!(model(&state), None);
    }

    #[test]
    fn limbs_do_not_draw_their_owner_number_as_a_model() {
        for part in G2_MODELPART_HEAD..=G2_MODELPART_RLEG {
            let mut state = entity(ET_GENERAL, 2);
            state.set_raw_field(WEAPON, u32::from(G2_MODEL_PART));
            state.set_raw_field(MODEL_GHOUL2, u32::from(part));
            assert_eq!(model(&state), None, "limb part {part}");
        }
        // Outside the limb parts CG_General still sets the model.
        let mut state = entity(ET_GENERAL, 2);
        state.set_raw_field(WEAPON, u32::from(G2_MODEL_PART));
        assert_eq!(model(&state), cs_model());
    }

    #[test]
    fn body_draws_its_configstring_model() {
        assert_eq!(model(&entity(ET_BODY, 2)), cs_model());
    }

    #[test]
    fn holocron_uses_its_own_table_below_minus_100() {
        // SP_misc_holocron sends `count - 128`.
        assert_eq!(model(&entity(ET_HOLOCRON, -128)), None);
        assert_eq!(model(&entity(ET_HOLOCRON, -101)), None);
        assert_eq!(model(&entity(ET_HOLOCRON, 2)), cs_model());
    }

    #[test]
    fn mover_draws_its_brush_or_configstring_model() {
        let mut brush = entity(ET_MOVER, 2);
        brush.set_raw_field(SOLID, SOLID_BMODEL);
        assert_eq!(model(&brush), Some("*2".to_owned()));
        assert_eq!(model(&entity(ET_MOVER, 2)), cs_model());
        let mut empty = entity(ET_MOVER, 0);
        empty.set_raw_field(SOLID, SOLID_BMODEL);
        assert_eq!(model(&empty), None);
    }

    #[test]
    fn mover_draws_its_model2_at_its_scale_unless_hidden() {
        const MODEL2: usize = 41;
        const EFLAGS: usize = 19;
        const MODEL_SCALE: usize = 76;
        let mut table = entity(ET_MOVER, 27);
        table.set_raw_field(SOLID, SOLID_BMODEL);
        assert_eq!(legacy_mover_model2(&game_state(), &table), None);
        table.set_raw_field(MODEL2, 2);
        let drawn = legacy_mover_model2(&game_state(), &table).unwrap();
        assert_eq!(drawn.appearance.model, MODEL);
        assert_eq!(drawn.scale, 1.0);
        table.set_raw_field(MODEL_SCALE, 150);
        assert_eq!(
            legacy_mover_model2(&game_state(), &table).unwrap().scale,
            1.5
        );
        table.set_raw_field(EFLAGS, 1 << 8);
        assert_eq!(legacy_mover_model2(&game_state(), &table), None);
        // Only a mover's modelindex2 is a model; an item's or effect's is not.
        let mut general = entity(ET_GENERAL, 2);
        general.set_raw_field(MODEL2, 2);
        assert_eq!(legacy_mover_model2(&game_state(), &general), None);
    }

    #[test]
    fn missile_draws_only_a_saber_from_modelindex() {
        // Other missiles take their model from the weapon table.
        assert_eq!(model(&entity(ET_MISSILE, 2)), None);
        let mut saber = entity(ET_MISSILE, 7);
        saber.set_raw_field(WEAPON, u32::from(WP_SABER));
        assert_eq!(
            model(&saber),
            Some("models/weapons2/saber_2/saber_2.glm".to_owned())
        );
        let mut default = entity(ET_MISSILE, 0);
        default.set_raw_field(WEAPON, u32::from(WP_SABER));
        assert_eq!(model(&default), Some(DEFAULT_SABER_MODEL.to_owned()));
    }

    #[test]
    fn player_item_and_npc_are_not_drawn_from_modelindex_here() {
        for entity_type in [1, 2, 13] {
            assert_eq!(model(&entity(entity_type, 2)), None, "type {entity_type}");
        }
    }

    #[test]
    fn special_draws_no_model() {
        // A portable shield: modelindex is HI_SHIELD (2), drawn as an effect.
        assert_eq!(model(&entity(4, 2)), None);
    }

    #[test]
    fn beam_draws_no_model() {
        assert_eq!(model(&entity(7, 2)), None);
    }

    #[test]
    fn portal_draws_no_model() {
        assert_eq!(model(&entity(8, 2)), None);
    }

    #[test]
    fn speaker_draws_no_model() {
        assert_eq!(model(&entity(9, 2)), None);
    }

    #[test]
    fn triggers_draw_no_brush_model() {
        for entity_type in [10, 11] {
            let mut state = entity(entity_type, 2);
            state.set_raw_field(SOLID, SOLID_BMODEL);
            assert_eq!(model(&state), None, "type {entity_type}");
        }
    }

    #[test]
    fn invisible_draws_no_model() {
        // Spectating, intermission or gibbed players and removed NPCs.
        assert_eq!(model(&entity(12, 2)), None);
    }

    #[test]
    fn team_draws_no_model() {
        assert_eq!(model(&entity(14, 2)), None);
    }

    #[test]
    fn terrain_draws_no_model() {
        let mut state = entity(16, 2);
        state.set_raw_field(SOLID, SOLID_BMODEL);
        assert_eq!(model(&state), None);
    }

    #[test]
    fn fx_draws_no_model() {
        // fx_runner's modelindex is a CS_EFFECTS index.
        assert_eq!(model(&entity(17, 2)), None);
    }

    #[test]
    fn events_draw_no_model() {
        assert_eq!(model(&entity(18, 2)), None);
        assert_eq!(model(&entity(18 + 60, 2)), None);
    }
}
