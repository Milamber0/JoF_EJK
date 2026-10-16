//! Cgame post-Pmove trigger queries, separate from movement's solid collision list.
use super::*;

pub(super) fn touch_triggers(
    predictor: &mut Predictor,
    entities: &[sjk_client::prediction_items::PredictionTrigger],
    bsp: &Bsp,
    time: i32,
    gametype: u8,
    predict_items: bool,
) {
    predictor.touch_prediction_triggers(
        entities,
        time,
        gametype,
        predict_items,
        |model, origin, mins, maxs| {
            let Ok(bounds) = sjk_bsp::Aabb::new(mins, maxs) else {
                return false;
            };
            // cg_predict.c:728: CM_Trace, not TransformedBoxTrace. Inline trigger
            // geometry uses the untransformed player origin and all contents.
            bsp.trace_model_box(model, origin, origin, bounds, u32::MAX)
                .start_solid
        },
    );
}

impl LocalPrediction {
    /// rd-vanilla/tr_backend.cpp:409-420, exposed without renderer types.
    pub(crate) fn hyperspace_shade(&self) -> Option<f64> {
        self.predicted_state()
            .filter(|state| state.hyperspace)
            .map(|_| f64::from(self.presentation_time & 255) / 255.0)
    }

    pub(crate) fn set_predict_items(&mut self, enabled: bool) {
        self.predict_items = enabled;
    }

    /// Hide only the display copy; snapshots still restore rejected/respawned items.
    pub(crate) fn pickups(
        &self,
        snapshot: &Snapshot,
        time: i32,
        out: &mut Vec<crate::pickups::Presented>,
    ) {
        crate::pickups::collect(snapshot, time, out);
        if let Some(state) = self.predicted_state() {
            out.retain(|item| !state.predicted_item_hidden(item.entity_number));
        }
    }
}
