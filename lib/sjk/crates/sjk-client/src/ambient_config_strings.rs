//! Add newly named ambient sets while preserving runtime set identities.

use super::*;

impl LegacyAmbientWorld {
    pub(crate) fn refresh_config_string(
        &mut self,
        index: usize,
        game_state: &GameState,
        catalog: &AmbientSets,
        intern: &mut impl FnMut(&str) -> u16,
    ) {
        let name = game_state.config_string(index).unwrap_or_default();
        let set = std::str::from_utf8(name).ok().and_then(|name| {
            if name.is_empty() || name.eq_ignore_ascii_case("default") {
                return None;
            }
            if let Some(index) = self
                .names
                .iter()
                .position(|known| known.eq_ignore_ascii_case(name))
            {
                return Some(index);
            }
            let set = catalog.get(name)?;
            self.sets.push(RuntimeSet::new(set, intern));
            self.names.push(name.to_ascii_lowercase());
            Some(self.sets.len() - 1)
        });
        if index == CS_GLOBAL_AMBIENT_SET {
            self.global_name.clear();
            self.global_name.extend_from_slice(name);
            self.global_set = set;
        } else if let Some(slot) = index
            .checked_sub(CS_AMBIENT_SET)
            .and_then(|slot| self.cs_sets.get_mut(slot))
        {
            *slot = set;
        }
    }
}
