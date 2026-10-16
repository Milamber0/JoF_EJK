//! Thin visible-saber adapter over the shared compatibility definition parser.

use sjk_vfs::VirtualFileSystem;
use std::collections::BTreeMap;
use std::error::Error;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Definition {
    pub(crate) sound_spin: Option<String>,
    pub(crate) sound_swing: [Option<String>; 3],
    pub(crate) name: String,
    pub(crate) model: String,
    pub(crate) num_blades: u8,
    pub(crate) blade_lengths: [f32; 8],
    pub(crate) blade_radii: [f32; 8],
    pub(crate) blade_style2_start: u8,
    pub(crate) trail_style: u8,
    pub(crate) trail_style2: u8,
    /// Authored no-light flag, preserved for hilt light submission.
    pub(crate) no_dlight: bool,
    pub(crate) no_wall_marks: bool,
}

pub(crate) fn load(
    vfs: &VirtualFileSystem,
) -> Result<BTreeMap<String, Definition>, Box<dyn Error>> {
    Ok(sjk_client::legacy_saber_definitions(vfs)?
        .into_iter()
        .map(|(key, definition)| {
            (
                key,
                Definition {
                    name: definition.name,
                    sound_spin: definition.sound_spin,
                    sound_swing: definition.sound_swing,
                    model: definition.model,
                    num_blades: definition.num_blades,
                    blade_lengths: definition.blade_lengths,
                    blade_radii: definition.blade_radii,
                    blade_style2_start: definition.blade_style2_start,
                    trail_style: definition.trail_style,
                    trail_style2: definition.trail_style2,
                    no_dlight: definition.no_dlight,
                    no_wall_marks: definition.no_wall_marks,
                },
            )
        })
        .collect())
}
