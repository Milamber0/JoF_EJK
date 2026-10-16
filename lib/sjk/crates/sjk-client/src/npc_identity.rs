//! BaseJKA NPC (`ET_NPC`) identity — JKA's `CG_G2AnimEntModelLoad`
//! (`codemp/cgame/cg_players.c:7046-7308`).
//!
//! NPCs are not clients: their entity number is at or above `MAX_CLIENTS`
//! (`NPC_spawn.c:908`), so nothing describes them in `CS_PLAYERS`. Stock
//! reads the model from `CS_MODELS + modelindex`, where the server wrote
//! `models/players/<dir>/model.glm` with the skin appended after a `*`
//! (`g_client.c:1748-1765`; `CG_HandleAppendedSkin`, `cg_players.c:6869-6919`,
//! splits at the last `*`, a `|` inside the skin meaning a three-part skin).
//! Saber definitions come from `npcSaber1`/`npcSaber2`, `CS_MODELS` indices
//! whose strings start with `@` (`cg_players.c:7184-7203`). Blade colours
//! are the packed `boltToPlayer` overrides (`CG_AddSaberBlade`,
//! `cg_players.c:6115-6127`), falling back to the `.sab` default.
//!
//! Vehicles (`NPC_class == CLASS_VEHICLE`) pass their vehicle name as the model string,
//! `$<vehicle>` (`cg_players.c:8738-8790`): model and skin come from the vehicle table in
//! `ext_data/vehicles/*.veh`. This adapter has no file system, so it hands the `$` name on
//! as the appearance's model and whoever loads assets resolves it
//! ([`legacy_vehicle_name`]).
//!
//! Out of scope here: the `.sab` `saberColor` default, for which this adapter uses
//! `SABER_RED` like `WP_SaberSetDefaults` (`bg_saberLoad.c:407`).

use crate::presentation_equipment::{
    equipment_active, legacy_blade_rgb, legacy_held_item_kind, secondary_equipment_active,
    trail_duration,
};
use sjk_protocol::{EntityState, GameState, Snapshot};
use sjk_runtime::{Appearance, HeldEquipment};

const CS_MODELS: usize = 298;
/// `ET_NPC` in `bg_public.h`'s `entityType_t`.
pub(crate) const ET_NPC: u8 = 13;

/// The `CS_MODELS` string behind `index`, when set and non-empty.
fn model_string(game_state: &GameState, index: u16) -> Option<&str> {
    if index == 0 {
        return None;
    }
    let bytes = game_state.config_string(CS_MODELS + usize::from(index))?;
    std::str::from_utf8(bytes)
        .ok()
        .filter(|name| !name.is_empty())
}

/// Resolve the model and skin an NPC entity's `modelindex` advertises.
///
/// A vehicle keeps its `$<vehicle>` reference as the model. `None` for anything else that is not a
/// `models/players/<dir>/model.glm[*skin]` string, so the caller keeps the
/// shared fallback actor. A missing skin resolves to `default`: stock passes
/// skin handle 0 to Ghoul2, which is the same surfaces `model_default.skin`
/// names for retail player models.
pub fn legacy_npc_appearance(game_state: &GameState, state: &EntityState) -> Option<Appearance> {
    let index = u16::try_from(state.model_index()).ok()?;
    npc_appearance_from_model(model_string(game_state, index)?)
}

/// `true` when a `CS_MODELS` string names an NPC body rather than a rigid
/// model, so object loaders leave it to the actor path.
pub fn legacy_npc_body_model(name: &str) -> bool {
    npc_appearance_from_model(name).is_some()
}

/// The vehicle name behind an appearance whose model is a `$<vehicle>` reference.
pub fn legacy_vehicle_name(model: &str) -> Option<&str> {
    model.strip_prefix('$').filter(|name| !name.is_empty())
}

/// `CG_HandleAppendedSkin` over one `CS_MODELS` string.
pub(crate) fn npc_appearance_from_model(name: &str) -> Option<Appearance> {
    if legacy_vehicle_name(name).is_some() {
        // Resolved against the vehicle table by the asset loader; names compare without case.
        return Some(Appearance {
            model: name.to_ascii_lowercase(),
            variant: String::new(),
        });
    }
    let (path, skin) = name.rsplit_once('*').unwrap_or((name, ""));
    let (directory, file) = path.rsplit_once('/')?;
    if !file.eq_ignore_ascii_case("model.glm") || directory.is_empty() {
        return None;
    }
    Some(Appearance {
        model: directory.to_owned(),
        variant: if skin.is_empty() { "default" } else { skin }.to_owned(),
    })
}

/// Both saber definition names of an NPC, without allocating: the `@` is
/// stripped like `cg_players.c:7189`; a slot whose index is zero or whose
/// string does not start with `@` carries no saber.
pub fn legacy_npc_saber_names_borrowed<'a>(
    game_state: &'a GameState,
    state: &EntityState,
) -> [Option<&'a str>; 2] {
    state
        .npc_saber_indices()
        .map(|index| model_string(game_state, index).and_then(|name| name.strip_prefix('@')))
}

/// Owned copy of [`legacy_npc_saber_names_borrowed`], lower-cased for the
/// hilt catalog.
pub fn legacy_npc_saber_names(game_state: &GameState, state: &EntityState) -> [Option<String>; 2] {
    legacy_npc_saber_names_borrowed(game_state, state).map(|name| name.map(str::to_ascii_lowercase))
}

/// The NPC entity state with `entity_number` in `snapshot`, if it is one.
pub fn legacy_npc_state(snapshot: &Snapshot, entity_number: u16) -> Option<&EntityState> {
    snapshot
        .entities
        .iter()
        .find(|state| state.number() == entity_number && state.entity_type() == ET_NPC)
}

/// `saber_colors_t` packed into `boltToPlayer` for saber `saber` (0 or 1),
/// `None` when the NPC definition set no colour override.
pub(crate) fn npc_packed_saber_color(bolt_to_player: u8, saber: usize) -> Option<i32> {
    let packed = (bolt_to_player >> (3 * saber)) & 0x07;
    (packed != 0).then(|| i32::from(packed) - 1)
}

/// Blade colour of NPC saber `saber`: the packed override, else `SABER_RED`.
fn npc_blade_rgb(bolt_to_player: u8, saber: usize) -> [u8; 3] {
    legacy_blade_rgb(npc_packed_saber_color(bolt_to_player, saber).unwrap_or(0))
}

/// [`HeldEquipment`] for an NPC entity; the clientinfo-free twin of
/// `legacy_equipment`, colours coming from the entity instead of `CS_PLAYERS`.
pub(crate) fn legacy_npc_equipment(state: &EntityState, alive: bool) -> Option<HeldEquipment> {
    let kind = legacy_held_item_kind(state.weapon())?;
    let holstered = state.saber_holstered();
    let bolt = state.bolt_to_player();
    Some(HeldEquipment {
        kind,
        weapon: state.weapon(),
        primary_in_flight: kind == sjk_runtime::HeldItemKind::EnergyBlade
            && state.saber_in_flight(),
        active: equipment_active(kind, holstered, alive),
        color: npc_blade_rgb(bolt, 0),
        secondary_active: secondary_equipment_active(kind, holstered, alive),
        secondary_color: npc_blade_rgb(bolt, 1),
        trail_duration_millis: trail_duration(kind, state.saber_move()),
    })
}
