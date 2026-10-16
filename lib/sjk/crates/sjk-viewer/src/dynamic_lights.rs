//! Fixed-capacity generic point lights for the world material pass.
//!
//! The capacity matches rd-vanilla `MAX_DLIGHTS == 32`
//! (`codemp/rd-common/tr_types.h:29`), whose surface masks cannot represent
//! more lights. This module remains renderer-generic; legacy weapon selection
//! stays in `sjk-client`.

use bytemuck::{Pod, Zeroable};

#[path = "dynamic_light_settings.rs"]
mod settings;
pub(crate) use settings::Settings;

#[path = "point_light_grid.rs"]
mod grid;

#[path = "entity_lights.rs"]
/// Legacy entity-source selection feeding the existing generic light list.
pub(crate) mod entities;

/// Maximum point lights submitted in one frame.
pub(crate) const MAX_POINT_LIGHTS: usize = 32;

/// Texels along each edge of a light's octahedral shadow tile
/// (`dynamic_light_shadows.rs`, `POINT_TILE_EDGE` in `point_light_octa.wgsl`): the most
/// that fits a tile for every light in the uniform block.
pub(crate) const SHADOW_TILE_EDGE: u32 = 28;
/// Two bytes per texel: the static world's free distance from the light, in 255ths of
/// its reach (255: nothing within reach), and the normal of what stops it.
pub(crate) const SHADOW_TILE_BYTES: u64 = (SHADOW_TILE_EDGE * SHADOW_TILE_EDGE * 2) as u64;
/// Where the shadow header (`GpuPointLightBlock::shadows`) lies in the uniform block;
/// the tiles follow it.
pub(crate) const SHADOW_HEADER_OFFSET: u64 =
    (std::mem::size_of::<GpuPointLightBlock>() - std::mem::size_of::<[u32; 4]>()) as u64;
/// The whole uniform block: the CPU's [`GpuPointLightBlock`], then a tile per light that
/// only the GPU writes. 58,208 bytes, within the 64 KiB uniform binding limit.
pub(crate) const BLOCK_BYTES: u64 =
    std::mem::size_of::<GpuPointLightBlock>() as u64 + MAX_POINT_LIGHTS as u64 * SHADOW_TILE_BYTES;

/// One generic RGB point light with a finite radius.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct PointLight {
    pub(crate) origin: [f32; 3],
    pub(crate) radius: f32,
    pub(crate) color: [f32; 3],
}

/// Allocation-free per-frame light list.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointLightList {
    lights: [PointLight; MAX_POINT_LIGHTS],
    len: usize,
    dropped: usize,
    radiant: u32,
}

impl Default for PointLightList {
    fn default() -> Self {
        Self {
            lights: [PointLight::default(); MAX_POINT_LIGHTS],
            len: 0,
            dropped: 0,
            radiant: 0,
        }
    }
}

impl PointLightList {
    pub(crate) fn clear(&mut self) {
        self.len = 0;
        self.dropped = 0;
        self.radiant = 0;
    }

    pub(crate) fn push(&mut self, light: PointLight) -> bool {
        if light.radius <= 0.0 {
            return false;
        }
        let Some(slot) = self.lights.get_mut(self.len) else {
            self.dropped += 1;
            return false;
        };
        *slot = light;
        self.len += 1;
        true
    }

    /// Add emitted illumination independently of the existing surface light in day mode.
    pub(crate) fn push_radiant(&mut self, light: PointLight) -> bool {
        let index = self.len;
        if !self.push(light) {
            return false;
        }
        self.radiant |= 1 << index;
        true
    }

    pub(crate) fn as_slice(&self) -> &[PointLight] {
        &self.lights[..self.len]
    }

    pub(crate) fn gpu_block(&self) -> GpuPointLightBlock {
        let mut block = GpuPointLightBlock::zeroed();
        block.metadata[0] = self.len as u32;
        block.metadata[3] = self.radiant;
        block.grid = grid::build(self.as_slice());
        for (index, (destination, source)) in block
            .lights
            .iter_mut()
            .zip(&self.lights[..self.len])
            .enumerate()
        {
            destination.origin_radius = [
                source.origin[0],
                source.origin[1],
                source.origin[2],
                source.radius,
            ];
            destination.color = [
                source.color[0],
                source.color[1],
                source.color[2],
                f32::from(self.radiant & (1 << index) != 0),
            ];
        }
        block
    }
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuPointLight {
    origin_radius: [f32; 4],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(crate) struct GpuPointLightBlock {
    lights: [GpuPointLight; MAX_POINT_LIGHTS],
    /// Light count, fragment-diffuse enable, lighting-mode bits, then a mask of emitted-light sources.
    pub(crate) metadata: [u32; 4],
    grid: grid::Grid,
    /// How many lights have a shadow tile this frame. The CPU always writes zero; the
    /// tracer's copy (`dynamic_light_shadows.rs`) replaces it with its tiles, so a frame
    /// that traces none reads every light as unshadowed.
    shadows: [u32; 4],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shadow_tiles_follow_the_cpu_block_within_the_uniform_limit() {
        // The header is the CPU part's last vec4, so its zero resets every frame.
        assert_eq!(
            SHADOW_HEADER_OFFSET as usize,
            std::mem::offset_of!(GpuPointLightBlock, shadows)
        );
        assert_eq!(
            SHADOW_HEADER_OFFSET + 16,
            std::mem::size_of::<GpuPointLightBlock>() as u64
        );
        assert_eq!(BLOCK_BYTES, 58_208);
        // wgpu's default `max_uniform_buffer_binding_size`, which the device requests.
        assert!(BLOCK_BYTES <= wgpu::Limits::default().max_uniform_buffer_binding_size);
        // Buffer copies move whole words; uniform arrays step by vec4.
        assert_eq!(SHADOW_HEADER_OFFSET % 16, 0);
        assert_eq!(SHADOW_TILE_BYTES % 16, 0);
        let block = PointLightList::default().gpu_block();
        assert_eq!(block.shadows, [0; 4]);
    }

    #[test]
    fn the_programs_read_the_block_the_cpu_and_the_tracer_write() {
        use wgpu::naga;
        let source = crate::world_materials::world_sun_shader();
        let module = naga::front::wgsl::parse_str(source).expect("the stage program parses");
        let mut layouter = naga::proc::Layouter::default();
        layouter
            .update(module.to_ctx())
            .expect("the stage program's types lay out");
        let size = |name: &str| {
            let (handle, _) = module
                .types
                .iter()
                .find(|(_, ty)| ty.name.as_deref() == Some(name))
                .unwrap_or_else(|| panic!("{name} is declared"));
            u64::from(layouter[handle].size)
        };
        assert_eq!(size("PointLightBlock"), BLOCK_BYTES);
        assert_eq!(
            size("PointLightGrid"),
            std::mem::size_of::<grid::Grid>() as u64
        );
        // The shaders' tile shape is the CPU's.
        let mapping = include_str!("point_light_octa.wgsl");
        assert!(mapping.contains(&format!(
            "const POINT_TILE_EDGE: u32 = {SHADOW_TILE_EDGE}u;"
        )));
        let texels = SHADOW_TILE_BYTES / 2;
        assert!(mapping.contains(&format!("const POINT_TILE_TEXELS: u32 = {texels}u;")));
        assert!(source.contains("tiles: array<vec4<u32>, 3136>,"));
        assert_eq!(MAX_POINT_LIGHTS as u64 * SHADOW_TILE_BYTES / 16, 3136);
    }
}
