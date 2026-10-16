//! Invisible brushes (`r_showBrushes`): clip, trigger, ladder, nodrop and the
//! other tool brushes a map is built with, drawn see-through with a labelled
//! tile per kind, as Radiant shows them. A map-learning and map-debugging
//! view; the client has these brushes anyway for its own collision.
//!
//! Nothing is built until the view is first switched on for a map; the faces
//! are then rebuilt from the BSP's brush planes ([`mesh`]) once per map.

mod gpu;
pub(crate) mod kinds;
mod mesh;
mod tiles;

use crate::GpuState;
use kinds::KindSet;

/// 0 off, 1 the gameplay kinds, 2 every kind.
pub(crate) const CVAR: &str = "r_showBrushes";
/// Kind names that replace the set `r_showBrushes` picks, when any is listed.
pub(crate) const TYPES_CVAR: &str = "r_showBrushesTypes";

/// The view's GPU resources, once built, and this frame's choice.
#[derive(Default)]
pub(crate) struct BrushView {
    renderer: Option<gpu::Renderer>,
    shown: KindSet,
}

impl BrushView {
    /// Draw in the overlay pass, which holds the scene depth read-only.
    pub(crate) fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, camera: &'a wgpu::BindGroup) {
        if let Some(renderer) = &self.renderer
            && !self.shown.is_empty()
        {
            renderer.draw(pass, camera, self.shown);
        }
    }

    /// Whether this frame draws anything.
    pub(crate) fn active(&self) -> bool {
        !self.shown.is_empty()
    }
}

/// Read the cvars and, the first time they ask for it on this map, build the view.
pub(crate) fn frame(gpu: &mut GpuState) {
    let console = gpu.console.as_ref();
    let mode = console.and_then(|c| c.integer_cvar(CVAR)).unwrap_or(0);
    let wanted = if gpu.is_menu_world || gpu.world_hidden {
        KindSet::EMPTY
    } else {
        KindSet::chosen(
            mode,
            console.and_then(|c| c.text_value(TYPES_CVAR)).unwrap_or(""),
        )
    };
    let view = &mut gpu.brush_view;
    if wanted.is_empty() {
        view.shown = KindSet::EMPTY;
        return;
    }
    let format = gpu.context.scene_format();
    if view.renderer.as_ref().is_none_or(|r| r.format != format) {
        view.renderer = Some(gpu::Renderer::new(
            &gpu.device,
            &gpu.queue,
            format,
            &gpu.camera_layout,
            gpu.context.anisotropy_limit,
            mesh::Mesh::build(&gpu.bsp),
        ));
    }
    view.shown = view
        .renderer
        .as_ref()
        .map_or(KindSet::EMPTY, |r| wanted.intersection(r.present));
}

/// For world shots: each kind the map has, its face count and the middle of
/// its faces within 512 units of its first one.
#[cfg(test)]
pub(crate) fn kind_sites(bsp: &sjk_bsp::Bsp) -> Vec<(kinds::Kind, usize, glam::Vec3)> {
    let mesh = mesh::Mesh::build(bsp);
    kinds::Kind::ALL
        .into_iter()
        .filter_map(|kind| {
            let range = mesh.face_ranges[kind.index()].clone();
            let faces = &mesh.faces[range.start as usize..range.end as usize];
            let first = faces.first()?;
            // The bounds of the faces near the first one: one brush, roughly.
            let anchor = glam::Vec3::from_array(first.position);
            let (min, max) = faces
                .iter()
                .map(|v| glam::Vec3::from_array(v.position))
                .filter(|p| p.distance(anchor) < 512.0)
                .fold((anchor, anchor), |(min, max), p| (min.min(p), max.max(p)));
            Some((kind, faces.len() / 3, (min + max) * 0.5))
        })
        .collect()
}
