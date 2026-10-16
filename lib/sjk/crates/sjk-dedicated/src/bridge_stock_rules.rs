//! `g_stockRules`: this server's own option (not the reference's). SJK's server lifts
//! the limits and quirks a stock server has — `0`, the default — and keeps the stock
//! behaviour exactly with `1`, for operators who want a server indistinguishable from a
//! stock one. Each place that differs says so where it reads [`NativeGame::stock_rules`]:
//! a passed team vote makes its leader (`bridge_team_votes`); `npc spawn` of a vehicle's
//! name spawns the vehicle, friendly names find their NPC, MD3 NPCs spawn with a Ghoul2
//! stand-in, and the entity budget, NPC files and vehicle table are not capped
//! (`bridge_npc_names`); native sand-creature AI runs outside stock mode
//! (`bridge_npcs`, `sjk_game_jka::npc_sand_creature`).

use super::NativeGame;

/// The option's name.
pub(super) const STOCK_RULES: &[u8] = b"g_stockRules";

impl NativeGame {
    /// `g_stockRules` registered (archived: an operator's choice is remembered).
    pub(super) fn register_stock_rules(&mut self) {
        let about: &[u8] = b"SJK: 1 keeps every stock limit and quirk; 0 (default) lifts them";
        self.cvars
            .get(STOCK_RULES, b"0", crate::cvars::CVAR_ARCHIVE, Some(about));
    }

    /// Whether the stock rules are asked for.
    pub(super) fn stock_rules(&self) -> bool {
        self.cvars.integer(STOCK_RULES) != 0
    }
}
