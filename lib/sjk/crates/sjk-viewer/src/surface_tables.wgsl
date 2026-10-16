// The stage vertex path's lookup tables (`surface_tables.rs`), one storage buffer of the
// geometry group: noise values and permutation for deforms and noise waveforms, and the
// surface-sprite random table.
struct SurfaceTables {
    noise_values: array<f32, 256>,
    noise_perm: array<i32, 256>,
    sprite_random: array<f32, 256>,
};
@group(2) @binding(6) var<storage, read> surface_tables: SurfaceTables;
