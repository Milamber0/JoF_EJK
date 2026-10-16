//! Post-command cgame prediction hooks; never part of the network codec.
use super::*;

impl Predictor {
    /// CG_TouchTriggerPrediction (:685-740), called after each Pmove, not each slice.
    pub fn touch_prediction_triggers(
        &mut self,
        entities: &[crate::prediction_items::PredictionTrigger],
        time: i32,
        gametype: u8,
        predict_items: bool,
        mut touches_brush: impl FnMut(usize, [f32; 3], [f32; 3], [f32; 3]) -> bool,
    ) {
        self.state.hyperspace = false;
        self.state.jump_pad_entity = None;
        if self.state.health <= 0 || !matches!(self.state.movement_type, 0 | 1 | 2 | 4) {
            return;
        }
        let top = if self.state.movement_flags & (PMF_DUCKED | 4) != 0 {
            self.state.crouching_height
        } else {
            self.state.standing_height
        };
        for entity in entities {
            if entity.kind == 2 && self.state.movement_type != 4 {
                let number = usize::from(entity.number);
                if !predict_items
                    || self.state.predicted_item_hidden(entity.number)
                    || !crate::prediction_items::can_predict_item(&self.state, entity, gametype)
                {
                    continue;
                }
                let position = crate::legacy_evaluate_trajectory(
                    entity.base,
                    entity.delta,
                    entity.trajectory,
                    entity.start,
                    entity.duration,
                    time,
                );
                if !crate::prediction_items::touches_item(self.state.origin, position) {
                    continue;
                }
                if number >= 1024 {
                    continue;
                }
                self.state.predicted_items[number / 64] |= 1 << (number % 64);
                self.add_event(22, entity.number); // EV_ITEM_PICKUP takes entity number.
                crate::prediction_items::give_weapon_seed(&mut self.state, entity.model);
            } else if matches!(entity.kind, 4 | 5)
                && entity.solid == 0x00ff_ffff
                && entity.model > 0
                && touches_brush(
                    entity.model as usize,
                    self.state.origin,
                    [-15.0, -15.0, -24.0],
                    [15.0, 15.0, top],
                )
            {
                if entity.kind == 5 {
                    self.state.hyperspace = true;
                } else if self.state.movement_type != 4 {
                    // BG_TouchJumpPad, bg_misc.c:2543-2572. No event in JKA.
                    self.state.velocity = entity.launch;
                    self.state.force_powers_active &= !(1 << 1);
                    self.state.jump_pad_entity = Some(entity.number);
                }
            }
        }
    }
}

impl MovementState {
    /// Presentation-only hide mask; a fresh authoritative seed rolls it back.
    pub fn predicted_item_hidden(&self, entity: u16) -> bool {
        let number = usize::from(entity);
        number < 1024 && self.predicted_items[number / 64] & (1 << (number % 64)) != 0
    }
}
