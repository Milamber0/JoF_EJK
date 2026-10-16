//! Optional outdoor sources; local lighting does not require an outdoor environment.
#[derive(Clone, Copy, Default)]
pub(crate) struct Policy {
    /// Compiler request to omit baked shader sun; never disables the live day cycle.
    pub(crate) suppress_sun: bool,
}
impl Policy {
    pub(crate) fn from_bsp(bsp: &sjk_bsp::Bsp) -> Self {
        let suppress_sun = sjk_entity::parse_entity_lump(bsp.entities())
            .ok()
            .and_then(|entities| {
                entities
                    .into_iter()
                    .find(|e| e.classname() == Some("worldspawn"))
            })
            .and_then(|world| world.get("_noshadersun").map(str::to_owned))
            .is_some_and(|v| v.parse::<f32>().is_ok_and(|v| v != 0.));
        Self { suppress_sun }
    }
}

/// Shared presentation sun for sky maps that do not declare one. It remains separate
/// from authored metadata so toggling lighting cannot turn it into baked sunlight.
pub(crate) fn default_sun() -> sjk_shader::SunParms {
    sjk_shader::SunParms {
        direction: [0.55, 0.35, 0.76],
        color: [1., 0.96, 0.86],
        intensity: 150.,
    }
}
