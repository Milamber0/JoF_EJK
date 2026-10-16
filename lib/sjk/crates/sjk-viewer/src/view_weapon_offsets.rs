//! TaystJK cg_weapons.c:879-881: offsets along the unbobbed camera axes.
use crate::console::ViewerConsole;
use glam::Vec3;

/// Move the hand rig and its muzzle together in camera space; `drop` adds to
/// `cg_gunZ` ([`ViewModelFov::drop`]).
pub(super) fn apply(
    origin: [f32; 3],
    yaw: f32,
    pitch: f32,
    console: Option<&ViewerConsole>,
    drop: f32,
) -> [f32; 3] {
    let mut values = ["cg_gunx", "cg_guny", "cg_gunz"]
        .map(|name| crate::cgame_options::scalar(console, name, 0.0));
    values[2] += drop;
    let forward = Vec3::new(
        yaw.cos() * pitch.cos(),
        yaw.sin() * pitch.cos(),
        pitch.sin(),
    );
    let left = Vec3::new(-yaw.sin(), yaw.cos(), 0.0);
    let up = forward.cross(left);
    (Vec3::from_array(origin) + forward * values[0] + left * values[1] + up * values[2]).to_array()
}

/// EternalJK's view-model field of view (`CG_AddViewWeapon`, `cg_weapons.c`): the
/// weapon is drawn with the world's projection, but its forward axis is scaled so it
/// looks as it would at `cg_fovViewmodel`, and lowered at view-model FOVs over 90.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ViewModelFov {
    /// `fracWeapFOV`: the factor on the hand's forward axis.
    pub(super) forward_scale: f32,
    /// `fovOffset`: added to `cg_gunZ`.
    pub(super) drop: f32,
}

impl ViewModelFov {
    pub(super) const NONE: Self = Self {
        forward_scale: 1.0,
        drop: 0.0,
    };
}

/// `horizontal_fov` is the rendered horizontal FOV in degrees (`cg.refdef.fov_x`).
pub(super) fn view_model_fov(
    console: Option<&ViewerConsole>,
    horizontal_fov: f32,
    aspect: f32,
) -> ViewModelFov {
    let viewmodel = crate::cgame_options::scalar(console, "cg_fovviewmodel", 80.0);
    let integer = |name, fallback| {
        console
            .and_then(|c| c.integer_cvar(name))
            .unwrap_or(fallback)
            != 0
    };
    let aspect_adjust = console
        .and_then(|c| c.bool_cvar("cg_fovaspectadjust"))
        .unwrap_or(true);
    let world = crate::cgame_options::scalar(console, "cg_fov", 90.0);
    fov_terms(
        viewmodel,
        world,
        integer("cg_fovviewmodeladjust", 1),
        aspect_adjust,
        horizontal_fov,
        aspect,
    )
}

/// The arithmetic of [`view_model_fov`]; `viewmodel` below 1 (`.integer` 0) uses
/// `world` and leaves the axis alone.
fn fov_terms(
    viewmodel: f32,
    world: f32,
    drop_adjust: bool,
    aspect_adjust: bool,
    horizontal_fov: f32,
    aspect: f32,
) -> ViewModelFov {
    let enabled = viewmodel.trunc() != 0.0;
    let desired = if enabled { viewmodel } else { world };
    let clamped = desired.clamp(1.0, 140.0);
    let drop = if drop_adjust && clamped > 90.0 {
        -0.2 * (clamped - 90.0)
    } else {
        0.0
    };
    if !(enabled && horizontal_fov > 0.0 && horizontal_fov < 180.0) {
        return ViewModelFov {
            forward_scale: 1.0,
            drop,
        };
    }
    let fov = if aspect_adjust {
        // Retail's 4:3 FOV widened to this screen, as `cg_fovAspectAdjust` widens cg_fov.
        ((desired.to_radians() * 0.5).tan() * 0.75 * aspect).atan() * 2.0
    } else {
        clamped.to_radians()
    };
    let forward_scale = (fov * 0.5).tan() / (horizontal_fov.to_radians() * 0.5).tan();
    ViewModelFov {
        forward_scale: if forward_scale.is_finite() && forward_scale > 0.0 {
            forward_scale
        } else {
            1.0
        },
        drop,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eternaljk_defaults_on_a_wide_screen() {
        // cg_fov 90 widened to 21:9 is about 121.6 degrees; cg_fovViewmodel 80 widened
        // the same way gives tan 1.50 against tan 1.79.
        let aspect = 3440.0 / 1440.0;
        let world = ((45f32.to_radians().tan() * 0.75 * aspect).atan() * 2.0).to_degrees();
        let terms = fov_terms(80.0, 90.0, true, true, world, aspect);
        assert!((terms.forward_scale - 0.84).abs() < 0.01, "{terms:?}");
        assert_eq!(terms.drop, 0.0);
        // At 4:3 with equal FOVs the weapon is untouched.
        let same = fov_terms(90.0, 90.0, true, true, 90.0, 4.0 / 3.0);
        assert!((same.forward_scale - 1.0).abs() < 1e-4);
    }

    #[test]
    fn zero_uses_cg_fov_and_wide_view_models_drop() {
        let off = fov_terms(0.0, 110.0, true, true, 110.0, 4.0 / 3.0);
        assert_eq!(off.forward_scale, 1.0);
        assert!((off.drop + 4.0).abs() < 1e-4);
        let wide = fov_terms(100.0, 90.0, false, false, 90.0, 4.0 / 3.0);
        assert_eq!(wide.drop, 0.0);
    }
}
