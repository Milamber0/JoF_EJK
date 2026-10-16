//! Planar glossy floors share one scratch mirror target, rendered/composited per plane.
use super::*;
#[path = "floor_reflection_bounds.rs"]
mod bounds;
#[path = "floor_reflection_finish.rs"]
mod finish;
#[path = "floor_reflection_planes.rs"]
mod planes;
#[path = "floor_reflection_render.rs"]
mod render;
#[path = "floor_reflection_gpu.rs"]
mod resources;
#[path = "floor_reflection_visibility.rs"]
mod visibility;

/// A mirrored plane re-renders the whole scene, so a frame mirrors only the planes that
/// matter on screen. `alzoc3_enclave` has 247 polished planes (every stair step and trim
/// piece) with 49 in view from a spawn: 19.5 ms of GPU time and 83 ms per frame. A plane
/// enters the budget at `ENTER` of the screen (the bounding rectangle of its visible
/// faces) and leaves below `LEAVE`, so it does not flicker at the threshold; the largest
/// `MAX_MIRRORS` are kept. Planes outside the budget keep their ordinary material.
const MAX_MIRRORS: usize = 6;
const ENTER: f32 = 0.010;
const LEAVE: f32 = 0.007;

/// Live `r_floorReflections`; `SJK_FLOOR_REFLECTIONS=0` still forces mirrors off.
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Register `r_floorReflections` (default 1, live): polished floors mirror the scene.
pub(crate) fn register(cvars: &mut sjk_shell::CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    use std::sync::atomic::Ordering;
    cvars.register(sjk_shell::CvarDefinition::new(
        "r_floorReflections",
        1_i64,
        sjk_shell::CvarFlags::ARCHIVE,
        "Polished floors (environment-mapped shaders) mirror the scene; 0 keeps their \
         ordinary material; live",
    ))?;
    let on = |value: &sjk_shell::CvarValue| !matches!(value, sjk_shell::CvarValue::Integer(0));
    ENABLED.store(
        on(&cvars.get("r_floorReflections").expect("registered").value),
        Ordering::Relaxed,
    );
    cvars.on_change("r_floorReflections", move |change| {
        ENABLED.store(on(&change.current), Ordering::Relaxed)
    })
}

/// Mirrors are drawn: the cvar is on and the environment override does not forbid them.
fn enabled(environment: bool) -> bool {
    environment && ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

struct Floor {
    plane: planes::Plane,
    camera: resources::Camera,
    /// Mirrored last frame: the hysteresis state of the budget.
    mirrored: bool,
    /// Share of the screen covered by the bounding rectangle of its visible faces.
    coverage: f32,
    visible: bool,
    region: [f32; 4],
    clip: Mat4,
}
/// Map-owned floor reflection resources; no camera-ranked slots or frame allocations.
pub(super) struct Floors {
    floors: Vec<Floor>,
    pub(super) target: Option<gpu::Target>,
    finish: Option<finish::Finish>,
    finish_layout: wgpu::BindGroupLayout,
    /// Floor material maps groups; 0 is the neutral one (`Face::maps`).
    map_groups: Vec<wgpu::BindGroup>,
    scale: f32,
    roughness: f32,
    pipeline: wgpu::RenderPipeline,
    cluster: Option<usize>,
    enabled: bool,
    visibility: Option<visibility::Visibility>,
    commands_disabled: std::cell::Cell<bool>,
}
impl Floors {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        sample: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        size: [u32; 2],
        scene: &FlattenedScene,
        shaders: &ShaderCatalog,
        maps: &dyn Fn(usize) -> Option<crate::world_materials::material_maps::FloorMaps>,
    ) -> Self {
        let maps_layout = resources::maps_layout(device);
        let mut map_groups = vec![resources::maps_group(device, &maps_layout, None)];
        let mut by_material = std::collections::HashMap::new();
        let mut planes = planes::collect(scene, shaders);
        for plane in &mut planes {
            for face in &mut plane.faces {
                face.maps = *by_material.entry(face.material).or_insert_with(|| {
                    maps(face.material).map_or(0, |found| {
                        map_groups.push(resources::maps_group(device, &maps_layout, Some(&found)));
                        map_groups.len() - 1
                    })
                });
                plane.mapped |= face.maps != 0;
            }
        }
        let floors = planes
            .into_iter()
            .map(|plane| Floor {
                plane,
                camera: resources::Camera::new(device, camera),
                mirrored: false,
                coverage: 0.,
                visible: false,
                region: [0.; 4],
                clip: Mat4::IDENTITY,
            })
            .collect::<Vec<_>>();
        crate::log::progress(format_args!(
            "Floor mirrors: {} polished planes, {} with material maps",
            floors.len(),
            floors.iter().filter(|floor| floor.plane.mapped).count()
        ));
        let target =
            (!floors.is_empty()).then(|| gpu::Target::new(device, camera, sample, format, size));
        let finish_layout = finish::layout(device);
        let finish = target
            .as_ref()
            .map(|t| finish::Finish::new(device, &finish_layout, &t.color));
        let pipeline = resources::pipeline(device, camera, &finish_layout, &maps_layout, format);
        let visibility = visibility::Visibility::new(device, camera, &floors);
        Self {
            commands_disabled: std::cell::Cell::new(
                std::env::var("SJK_FLOOR_COMMANDS").as_deref() == Ok("0"),
            ),
            visibility,
            target,
            finish,
            finish_layout,
            map_groups,
            scale: finish::SCALE,
            roughness: finish::ROUGHNESS,
            floors,
            pipeline,
            cluster: None,
            // `SJK_FLOOR_REFLECTIONS=0` predates the cvar and still forces mirrors off.
            enabled: std::env::var_os("SJK_FLOOR_REFLECTIONS").is_none_or(|v| v != "0"),
        }
    }
    pub(super) fn configure_commands(
        &mut self,
        device: &wgpu::Device,
        arguments: Option<&wgpu::Buffer>,
    ) {
        if !self.commands_disabled.get() {
            if let (Some(visibility), Some(arguments)) = (&mut self.visibility, arguments) {
                visibility.configure_commands(device, arguments);
            }
        }
    }
    pub(super) fn resize(
        &mut self,
        device: &wgpu::Device,
        camera: &wgpu::BindGroupLayout,
        sample: &wgpu::BindGroupLayout,
        format: wgpu::TextureFormat,
        size: [u32; 2],
    ) {
        if self.target.is_some() {
            self.set_target(
                Some(gpu::Target::new(device, camera, sample, format, size)),
                device,
            );
        }
    }
    pub(super) fn set_target(&mut self, target: Option<gpu::Target>, device: &wgpu::Device) {
        self.finish = target
            .as_ref()
            .map(|t| finish::Finish::new(device, &self.finish_layout, &t.color));
        self.target = target;
    }
    pub(super) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        bsp: &Bsp,
        areas: &crate::world_materials::areas::Areas,
        view: Mat4,
        projection: Mat4,
        time: i32,
    ) {
        let eye = view.inverse().w_axis.truncate();
        if let (Some(target), Some(finish)) = (&self.target, &self.finish) {
            finish.update(queue, target.size, self.scale, self.roughness);
        }
        self.cluster = usize::try_from(bsp.leaves()[bsp.leaf_at(eye.to_array())].cluster).ok();
        for floor in &mut self.floors {
            let p = &floor.plane;
            floor.visible = false;
            if !enabled(self.enabled) || p.normal.dot(eye) <= p.distance + 0.05 {
                continue;
            }
            let mut region = [1., 1., 0., 0.];
            for f in &p.faces {
                if !areas.visible(&f.clusters, self.cluster, bsp.render().visibility())
                    || !sphere_visible(projection * view, f.center, f.radius)
                {
                    continue;
                }
                if let Some(r) = bounds::project(projection * view, f.bounds) {
                    region = [
                        f32::min(region[0], r[0]),
                        f32::min(region[1], r[1]),
                        f32::max(region[2], r[2]),
                        f32::max(region[3], r[3]),
                    ];
                    floor.visible = true;
                }
            }
            if !floor.visible {
                continue;
            }
            floor.coverage = ((region[2] - region[0]) * (region[3] - region[1])).max(0.);
            floor.region = [1. - region[2], region[1], 1. - region[0], region[3]];

            floor.region = finish::region(
                floor.region,
                self.target.as_ref().unwrap().size,
                self.scale,
                if p.mapped {
                    finish::MAPPED_MARGIN
                } else {
                    self.roughness
                },
            );
            let reflected = math::reflect_plane(p.normal, p.distance, view, eye);
            let clip = crate::oblique_clip::oblique_projection(
                projection,
                reflected.matrix,
                reflected.clip_point + p.normal * 0.05,
                reflected.clip_normal,
            );
            floor.clip = clip * reflected.matrix;
            queue.write_buffer(
                &floor.camera.buffer,
                0,
                bytemuck::bytes_of(&CameraUniform {
                    view_projection: (finish::raster_projection(self.scale)
                        * clip
                        * reflected.matrix)
                        .to_cols_array_2d(),
                    camera_position: reflected.eye.to_array(),
                    view_forward: reflected.forward.to_array(),
                    shader_time: time as f32 * 0.001,
                    _padding: 9.,
                }),
            );
        }
        self.apply_budget();
    }

    /// Keep the `MAX_MIRRORS` largest candidates above the coverage threshold; no allocation.
    fn apply_budget(&mut self) {
        let mut kept = [(0f32, usize::MAX); MAX_MIRRORS];
        for (index, floor) in self.floors.iter().enumerate() {
            let threshold = if floor.mirrored { LEAVE } else { ENTER };
            if !floor.visible || floor.coverage < threshold {
                continue;
            }
            // Insert by coverage, descending; the smallest kept candidate falls off.
            let mut candidate = (floor.coverage, index);
            for slot in &mut kept {
                if candidate.0 > slot.0 {
                    std::mem::swap(slot, &mut candidate);
                }
            }
        }
        for (index, floor) in self.floors.iter_mut().enumerate() {
            floor.visible = floor.visible && kept.iter().any(|slot| slot.1 == index);
            floor.mirrored = floor.visible;
        }
    }
    pub(super) fn active(&self) -> bool {
        self.floors.iter().any(|f| f.visible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floor_program_validates() {
        crate::wgsl_source::validate(concat!(
            include_str!("vertex_transform.wgsl"),
            include_str!("floor_reflection.wgsl")
        ));
        // The shader's blur and lookup limits fit the margin the region keeps.
        assert!(
            include_str!("floor_reflection.wgsl").contains("const FLOOR_MAX_ROUGHNESS: f32 = 0.6;")
        );
        assert!((finish::MAPPED_MARGIN - 2. * 0.6).abs() < 1e-6);
    }

    #[test]
    fn the_cvar_switches_mirrors_and_the_environment_still_overrides() {
        let mut cvars = sjk_shell::CvarRegistry::new();
        register(&mut cvars).expect("registers");
        assert!(enabled(true));
        assert!(!enabled(false));
        cvars.set_text("r_floorReflections", "0").expect("set");
        assert!(!enabled(true));
        cvars.set_text("r_floorReflections", "1").expect("set");
        assert!(enabled(true));
    }
}
