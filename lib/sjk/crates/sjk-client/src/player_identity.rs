//! BaseJKA player model/skin identity compatibility.
//!
//! This mirrors OpenJK `codemp/cgame/cg_players.c`: `CG_NewClientInfo` reads
//! `CS_PLAYERS + clientNum`, splits the `model` info key at its first slash,
//! and defaults a missing skin to `default`. `CG_Player` deliberately uses
//! `entityState.clientNum` rather than the entity number because one client can
//! own both a live entity and body-queue corpses. `CG_BodyQueueCopy` duplicates
//! the live Ghoul2 instance, so a corpse retains the appearance captured when
//! it entered the body queue even if that client slot is later changed.

use sjk_protocol::GameState;
use sjk_runtime::{Appearance, EntityId};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) const CS_PLAYERS: usize = 1_131;

/// Resolve the model and skin requested by one BaseJKA player configstring.
///
/// The returned model is an engine asset path while `variant` is the skin name
/// understood by the model loader. Asset validation and team/siege overrides
/// are separate loading-policy concerns, just as they are after
/// `CG_NewClientInfo` in codemp.
pub fn legacy_client_appearance(game_state: &GameState, client_num: u16) -> Option<Appearance> {
    legacy_client_appearance_forced(game_state, client_num, None)
}

/// Apply cg_forceModel before the existing team-skin policy (cg_players.c:1656-1696).
pub fn legacy_client_appearance_forced(
    game_state: &GameState,
    client_num: u16,
    forced: Option<&str>,
) -> Option<Appearance> {
    let config = game_state.config_string(CS_PLAYERS + usize::from(client_num))?;
    let mut appearance = appearance_from_config(config)?;
    if let Some(model) = forced {
        let (name, skin) = model.split_once('/').unwrap_or((model, "default"));
        appearance.model = format!("models/players/{name}");
        let remote_has_skin = crate::LegacyClientInfo::new(config)
            .text("model")
            .is_some_and(|value| value.contains('/'));
        if !crate::TeamColorPolicy::from_game_state(game_state).team_override_active
            || !remote_has_skin
        {
            appearance.variant = skin.to_owned();
        }
    }
    if crate::TeamColorPolicy::from_game_state(game_state).team_override_active {
        let team = crate::LegacyClientInfo::new(config)
            .integer("t")
            .unwrap_or(0);
        let model = appearance
            .model
            .strip_prefix("models/players/")
            .unwrap_or("");
        if !model
            .get(..5)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("jedi_"))
        {
            if let Some(color) = match team {
                1 => Some("red"),
                2 => Some("blue"),
                _ => None,
            } {
                let skin = &appearance.variant;
                appearance.variant = if ["default", "red", "blue"]
                    .iter()
                    .any(|name| skin.eq_ignore_ascii_case(name))
                    || skin.contains('|')
                {
                    color.to_owned()
                } else if skin.ends_with(color) {
                    skin.clone()
                } else {
                    format!("{skin}_{color}")
                };
            }
        }
    }
    Some(appearance)
}

/// Resolve the primary saber definition name advertised in clientinfo.
pub fn legacy_client_saber_name(game_state: &GameState, client_num: u16) -> Option<String> {
    legacy_client_saber_names(game_state, client_num)[0].clone()
}

/// Resolve both saber definition names from legacy clientinfo.
///
/// Raven cgame reads the compact `st`/`st2` keys (`cg_players.c:1750-1773`);
/// older BaseJKA-compatible servers also emit the long `saber1`/`saber2`
/// spellings, which retain precedence for existing fixtures. A second
/// saber named `none` or `remove` is no saber: `WP_SetSaber`
/// (`bg_saberLoad.c:2219-2225`) removes that slot instead of loading it, so
/// it resolves to `None` rather than to a hilt fallback.
pub fn legacy_client_saber_names(game_state: &GameState, client_num: u16) -> [Option<String>; 2] {
    let Some(config) = game_state.config_string(CS_PLAYERS + usize::from(client_num)) else {
        return [None, None];
    };
    let borrowed = saber_names_from_config(config);
    borrowed.map(|name| name.map(str::to_owned))
}

/// `WP_SetSaber` removes a non-primary saber named `none` or `remove`.
fn removes_second_saber(name: &str) -> bool {
    name.eq_ignore_ascii_case("none") || name.eq_ignore_ascii_case("remove")
}

/// Borrow both definition names without allocating during presentation.
pub(crate) fn saber_names_from_config(config: &[u8]) -> [Option<&str>; 2] {
    let Ok(text) = std::str::from_utf8(config) else {
        return saber_names_from_bytes(config);
    };
    saber_names_from_text(text)
}

fn saber_names_from_bytes(config: &[u8]) -> [Option<&str>; 2] {
    let mut compact = [None, None];
    let mut long = [None, None];
    let mut fields = config
        .strip_prefix(b"\\")
        .unwrap_or(config)
        .split(|byte| *byte == b'\\');
    while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
        if value.is_empty() {
            continue;
        }
        if key.eq_ignore_ascii_case(b"saber1") {
            long[0] = std::str::from_utf8(value).ok();
        } else if key.eq_ignore_ascii_case(b"saber2") {
            long[1] = std::str::from_utf8(value).ok();
        } else if key.eq_ignore_ascii_case(b"st") {
            compact[0] = std::str::from_utf8(value).ok();
        } else if key.eq_ignore_ascii_case(b"st2") {
            compact[1] = std::str::from_utf8(value).ok();
        }
    }
    let second = long[1]
        .or(compact[1])
        .filter(|name| !removes_second_saber(name));
    [long[0].or(compact[0]), second]
}

fn saber_names_from_text(text: &str) -> [Option<&str>; 2] {
    let body = text.strip_prefix('\\').unwrap_or(text);
    let mut fields = body.split('\\');
    let mut compact = [None, None];
    let mut long = [None, None];
    while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
        if value.is_empty() {
            continue;
        }
        if key.eq_ignore_ascii_case("saber1") {
            long[0] = Some(value);
        } else if key.eq_ignore_ascii_case("saber2") {
            long[1] = Some(value);
        } else if key.eq_ignore_ascii_case("st") {
            compact[0] = Some(value);
        } else if key.eq_ignore_ascii_case("st2") {
            compact[1] = Some(value);
        }
    }
    let second = long[1]
        .or(compact[1])
        .filter(|name| !removes_second_saber(name));
    [long[0].or(compact[0]), second]
}

fn appearance_from_config(config: &[u8]) -> Option<Appearance> {
    let info = crate::LegacyClientInfo::new(config);
    let model = info.text("model")?;
    let (model, variant) = model.split_once('/').unwrap_or((model, "default"));
    if model.is_empty() || variant.is_empty() {
        return None;
    }
    Some(Appearance {
        model: format!("models/players/{model}"),
        variant: variant.to_owned(),
    })
}

/// Lifecycle cache for player and body-queue appearances.
///
/// Live players re-resolve configstrings every snapshot, matching
/// `CG_NewClientInfo` updates. Bodies capture once, matching the independent
/// Ghoul2 instance created by codemp's `CG_BodyQueueCopy`.
#[derive(Default)]
pub(crate) struct LegacyPlayerIdentities {
    corpse_appearances: BTreeMap<EntityId, Appearance>,
    copied: BTreeSet<EntityId>,
}

impl LegacyPlayerIdentities {
    pub(crate) fn live(&self, game_state: &GameState, client_num: u16) -> Option<Appearance> {
        legacy_client_appearance(game_state, client_num)
    }

    pub(crate) fn corpse(
        &mut self,
        game_state: &GameState,
        entity_id: EntityId,
        client_num: u16,
    ) -> Option<Appearance> {
        if let Some(appearance) = self.corpse_appearances.get(&entity_id) {
            return Some(appearance.clone());
        }
        let appearance = legacy_client_appearance(game_state, client_num)?;
        Some(self.capture_corpse(entity_id, appearance))
    }

    fn capture_corpse(&mut self, entity_id: EntityId, appearance: Appearance) -> Appearance {
        self.corpse_appearances
            .insert(entity_id, appearance.clone());
        appearance
    }

    pub(crate) fn copy_client_to_body(
        &mut self,
        game_state: &GameState,
        client_num: u16,
        body_id: EntityId,
    ) {
        if let Some(appearance) = legacy_client_appearance(game_state, client_num) {
            self.corpse_appearances.insert(body_id, appearance);
        }
    }

    pub(crate) fn retain(&mut self, active_entities: &BTreeSet<EntityId>) {
        self.corpse_appearances
            .retain(|id, _| active_entities.contains(id) || self.copied.contains(id));
    }

    pub(crate) fn copy_body(&mut self, body: &crate::BodyIdentity) {
        let id = EntityId::new(u64::from(body.entity_num) + 1);
        self.copied.insert(id);
        self.corpse_appearances.insert(id, body.appearance.clone());
    }

    pub(crate) fn kill_body(&mut self, number: u16) {
        let id = EntityId::new(u64::from(number) + 1);
        self.copied.remove(&id);
        self.corpse_appearances.remove(&id);
    }
}
