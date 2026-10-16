//! Initial brush scenery before a server supplies live mover transforms.
//! These are frozen placements, not a second simulation of doors or game rules.
use crate::GpuState;

pub(super) fn prepare(gpu: &mut GpuState) {
    if !gpu.movers.is_empty() {
        return;
    }
    let Ok(entities) = sjk_entity::parse_entity_lump(gpu.bsp.entities()) else {
        return;
    };
    for entity in entities {
        if !matches!(
            entity.classname(),
            Some(
                "func_door"
                    | "func_plat"
                    | "func_static"
                    | "func_rotating"
                    | "func_bobbing"
                    | "func_button"
                    | "func_train"
                    | "func_breakable"
            )
        ) {
            continue;
        }
        let Some(model) = entity
            .get("model")
            .and_then(|v| v.strip_prefix('*'))
            .and_then(|v| v.parse::<usize>().ok())
        else {
            continue;
        };
        if model == 0 || model >= gpu.bsp.render().models().len() {
            continue;
        }
        let origin = entity.vector("origin").ok().flatten().unwrap_or([0.0; 3]);
        // A door's `angle` is its movement direction, not a rotation of its BSP mesh.
        gpu.movers.push(sjk_client::LegacyMoverPresentation {
            entity_number: sjk_client::pmove::ENTITY_NUMBER_WORLD,
            model_index: model,
            origin,
            angles: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            visible: true,
        });
    }
}
