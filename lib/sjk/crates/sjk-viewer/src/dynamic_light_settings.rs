//! Stock scene-light gate, shared with console-free world installation.

use sjk_shell::{CvarDefinition, CvarError, CvarFlags, CvarRegistry, CvarValue};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[path = "model_light_settings.rs"]
mod model;

/// The static world's shadows on dynamic lights (`dynamic_light_shadows.rs`); SJK's.
pub(crate) const SHADOWS_CVAR: &str = "r_dynamicLightShadows";

/// Retained `r_dynamiclight` policy; no frame-time registry lookup or locking.
#[derive(Clone)]
pub(crate) struct Settings(
    Arc<AtomicBool>,
    model::Settings,
    crate::world_materials::lighting_mode::Settings,
    crate::world_materials::shadows::day::live::Clock,
    /// [`SHADOWS_CVAR`].
    Arc<AtomicBool>,
);

impl Default for Settings {
    fn default() -> Self {
        Self(
            Arc::new(AtomicBool::new(true)),
            model::Settings::default(),
            Default::default(),
            Default::default(),
            Arc::new(AtomicBool::new(true)),
        )
    }
}

impl Settings {
    /// Register stock's archived default and seed before accepting change callbacks.
    pub(crate) fn bind(cvars: &mut CvarRegistry) -> Result<Self, CvarError> {
        // OpenJK codemp/rd-vanilla/tr_init.cpp:1612.
        cvars.register(CvarDefinition::new(
            "r_dynamiclight",
            1_i64,
            CvarFlags::ARCHIVE,
            "Dynamic world and model lights (0 off, nonzero on); applies immediately",
        ))?;
        let enabled = matches!(cvars.get("r_dynamiclight").map(|c| &c.value),
            Some(CvarValue::Integer(value)) if *value != 0);
        cvars.register(CvarDefinition::new(
            SHADOWS_CVAR,
            1_i64,
            CvarFlags::ARCHIVE,
            "Walls stop dynamic lights, in real-time lighting (0 lights through them, as \
             the original game); applies immediately",
        ))?;
        let shadows = matches!(cvars.get(SHADOWS_CVAR).map(|c| &c.value),
            Some(CvarValue::Integer(value)) if *value != 0);
        let settings = Self(
            Arc::new(AtomicBool::new(enabled)),
            model::Settings::bind(cvars)?,
            crate::world_materials::lighting_mode::Settings::bind(cvars)?,
            crate::world_materials::shadows::day::live::Clock::bind(cvars)?,
            Arc::new(AtomicBool::new(shadows)),
        );
        let changed = settings.clone();
        cvars.on_change("r_dynamiclight", move |change| {
            if let CvarValue::Integer(value) = change.current {
                changed.0.store(value != 0, Ordering::Relaxed);
            }
        })?;
        let changed = settings.4.clone();
        cvars.on_change(SHADOWS_CVAR, move |change| {
            if let CvarValue::Integer(value) = change.current {
                changed.store(value != 0, Ordering::Relaxed);
            }
        })?;
        Ok(settings)
    }

    /// Whether walls stop dynamic lights ([`SHADOWS_CVAR`]).
    pub(crate) fn shadows(&self) -> bool {
        self.4.load(Ordering::Relaxed)
    }

    /// Gate the completed scene list before either world upload or entity lighting.
    pub(crate) fn apply(&self, lights: &mut super::PointLightList) {
        // OpenJK codemp/rd-vanilla/tr_scene.cpp:521-524 zeroes num_dlights here,
        // not the light emitters: effects must keep aging while lighting is disabled.
        if !self.0.load(Ordering::Relaxed) {
            lights.clear();
        }
    }
    /// Publish the same gated scene lights to world shading and subsequent entity lighting.
    pub(crate) fn upload(
        &self,
        world: &crate::world_materials::Runtime,
        queue: &crate::frame_queue::FrameQueue,
        lights: &mut super::PointLightList,
    ) {
        self.apply(lights);
        world.update_day_clock(queue, self.3.values());
        world.set_day_debug(self.3.debug());
        world.set_gap_close(self.3.gap_close());
        world.set_ambient_fill(self.3.ambient_fill());
        world.set_indirect_readability(self.3.indirect_readability());
        world.update_scene_lighting_mode(
            queue,
            lights,
            self.1.enabled(),
            self.2.bits(),
            self.shadows(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walls_stop_dynamic_lights_by_default_and_the_cvar_turns_it_off_live() {
        let mut cvars = CvarRegistry::new();
        let settings = Settings::bind(&mut cvars).expect("the settings bind");
        assert!(settings.shadows());
        let shadows = cvars.get(SHADOWS_CVAR).expect("registered");
        assert_eq!(shadows.value, CvarValue::Integer(1));
        assert!(shadows.flags.contains(CvarFlags::ARCHIVE));
        cvars.set_text(SHADOWS_CVAR, "0").expect("set");
        assert!(!settings.shadows());
        cvars.set_text(SHADOWS_CVAR, "1").expect("set");
        assert!(settings.shadows());
    }
}
