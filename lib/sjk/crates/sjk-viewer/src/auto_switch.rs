//! CG_ItemPickup and CG_OutOfAmmoChange; selection changes, never weapon simulation.
use sjk_client::{LegacyWeaponInventory, legacy_weapon_selectable};

/// Last observed player event sequence; fixed storage, including duplicate suppression.
pub(crate) struct Tracker {
    sequence: Option<i32>,
    time: i32,
    external: u16,
    items: [u16; 1024],
    predicted: [Option<u16>; 2],
}

impl Default for Tracker {
    fn default() -> Self {
        Self {
            sequence: None,
            time: 0,
            external: 0,
            items: [0; 1024],
            predicted: [None; 2],
        }
    }
}

impl Tracker {
    /// Predict selection with the event and remember its sequence for acknowledgement suppression.
    pub(crate) fn predicted(
        &mut self,
        event: sjk_client::predicted_events::PredictedEvent,
        mode: i64,
        snapshot: &sjk_protocol::Snapshot,
    ) -> Option<u8> {
        if !matches!(event.event, 22 | 25) {
            return None;
        }
        let selected = self.event(event.event, event.parameter, mode, &snapshot.player);
        if selected.is_some() {
            self.predicted[event.sequence as usize & 1] = Some(event.sequence);
        }
        selected
    }

    fn acknowledged_prediction(&mut self, sequence: u16) -> bool {
        let slot = sequence as usize & 1;
        if self.predicted[slot] == Some(sequence) {
            self.predicted[slot] = None;
            true
        } else {
            false
        }
    }

    fn event(
        &self,
        event: u16,
        parameter: u16,
        mode: i64,
        player: &sjk_protocol::PlayerState,
    ) -> Option<u8> {
        let inventory = LegacyWeaponInventory::from_player_state(player);
        if inventory.spectator || inventory.following || inventory.emplaced {
            return None;
        }
        match event & 255 {
            // EV_ITEM_PICKUP carries an entity number, not bg_itemlist's index.
            22 if !player.duel_in_progress() => pickup(
                mode,
                player.weapon(),
                self.items.get(parameter as usize).copied().unwrap_or(0),
            ),
            25 if player.weapon() > 3 && player.vehicle_entity_num() == 0 => {
                let old = parameter as u8;
                empty(
                    mode,
                    if (1..19).contains(&old) {
                        old
                    } else {
                        player.weapon()
                    },
                    &inventory,
                )
            }
            _ => None,
        }
    }
}

/// OpenJK/TaystJK cg_event.c CG_ItemPickup: safe/non-safe upgrades, excluding saber.
pub(crate) fn pickup(mode: i64, current: u8, item: u16) -> Option<u8> {
    // bg_misc.c bg_itemlist weapon entries 19..34, matching weapon_t 1..16.
    const WEAPONS: [u8; 16] = [1, 2, 3, 4, 15, 16, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14];
    let weapon = *WEAPONS.get(item.checked_sub(19)? as usize)?;
    (current != 3 && weapon > current && (mode == 2 || (mode == 1 && !matches!(weapon, 11..=14))))
        .then_some(weapon)
}

/// CG_OutOfAmmoChange scans descending selectable weapons, with mode-1 explosive exclusion.
pub(crate) fn empty(mode: i64, current: u8, inventory: &LegacyWeaponInventory) -> Option<u8> {
    (1..=16).rev().find(|weapon| {
        *weapon != current
            && !(mode == 1 && matches!(*weapon, 11..=14))
            && legacy_weapon_selectable(inventory, *weapon)
    })
}

impl crate::GpuState {
    /// Consume new authoritative pickup/no-ammo events once, independent of HUD visibility.
    pub(crate) fn update_auto_switch(&mut self) {
        let Some(session) = &self.live_session else {
            return;
        };
        let snapshot = session.latest_snapshot();
        if self.auto_switch.sequence.is_some() && self.auto_switch.time == snapshot.server_time {
            return;
        }
        let player = &snapshot.player;
        let next = player.event_sequence();
        let old = self.auto_switch.sequence.replace(next).unwrap_or(next);
        let reset = snapshot.server_time < self.auto_switch.time;
        self.auto_switch.time = snapshot.server_time;
        if reset || next < old {
            self.auto_switch.items.fill(0);
            self.auto_switch.external = 0;
            self.auto_switch.predicted.fill(None);
            return;
        }
        for entity in &snapshot.entities {
            if entity.entity_type() == 2
                && let Some(slot) = self.auto_switch.items.get_mut(entity.number() as usize)
            {
                *slot = entity.model_index().max(0) as u16;
            }
        }
        let mode = self
            .console
            .as_ref()
            .and_then(|c| c.integer_cvar("cg_autoswitch"))
            .unwrap_or(1);
        let inventory = LegacyWeaponInventory::from_player_state(player);
        if inventory.spectator || inventory.following || inventory.emplaced {
            return;
        }
        let mut selected = None;
        for sequence in old.max(next.saturating_sub(2))..next {
            let slot = sequence as usize & 1;
            if self.auto_switch.acknowledged_prediction(sequence as u16) {
                continue;
            }
            selected = self
                .auto_switch
                .event(
                    player.event(slot).unwrap_or(0),
                    player.event_parameter(slot).unwrap_or(0),
                    mode,
                    player,
                )
                .or(selected);
        }
        let external = player.external_event();
        if external != self.auto_switch.external {
            self.auto_switch.external = external;
            selected = self
                .auto_switch
                .event(
                    external,
                    u16::from(player.external_event_parameter()),
                    mode,
                    player,
                )
                .or(selected);
        }
        if let Some(weapon) = selected {
            self.choose_auto_weapon(weapon);
        }
    }

    /// Apply automatic selection through the same retained input/HUD fields as binds.
    pub(crate) fn choose_auto_weapon(&mut self, weapon: u8) {
        self.selected_weapon = Some(weapon);
        self.weapon_selected_at = Some(std::time::Instant::now());
    }
}
