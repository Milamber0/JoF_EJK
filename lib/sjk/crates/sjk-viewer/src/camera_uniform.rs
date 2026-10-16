//! Camera data shared by colour and volume passes.
use bytemuck::{Pod, Zeroable};

/// View projection and fog distance axis, shared by world and entity passes.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct CameraUniform {
    pub(crate) view_projection: [[f32; 4]; 4],
    pub(crate) camera_position: [f32; 3],
    pub(crate) shader_time: f32,
    pub(crate) view_forward: [f32; 3],
    /// Geometry view flags in the legacy padding lane: bit 0 secondary view,
    /// bit 1 no fog outside a map sky portal, bit 2 sky-portal scene.
    /// Kept float for the GPU ABI.
    pub(crate) _padding: f32,
}

impl CameraUniform {
    /// Final rendered position and degree angles accepted by `--free-camera`.
    /// JKA pitch is positive downward, opposite the renderer's forward Z.
    pub(crate) fn viewpos(&self) -> String {
        let [x, y, z] = self.camera_position;
        let forward = glam::Vec3::from_array(self.view_forward).normalize_or_zero();
        let yaw = forward.y.atan2(forward.x).to_degrees().rem_euclid(360.0);
        let pitch = -forward.z.clamp(-1.0, 1.0).asin().to_degrees();
        format!("({x:.3} {y:.3} {z:.3}) : {yaw:.3} {pitch:.3}")
    }
}

impl crate::GpuState {
    /// Upload the final main view and retain it for caster bounds. Answer `viewpos`
    /// here, after third-person offsets, camera collision and view effects resolve.
    pub(crate) fn upload_scene_camera(&mut self, camera: CameraUniform) {
        self.world_materials
            .shadow_view(glam::Mat4::from_cols_array_2d(&camera.view_projection));
        self.world_materials.shadow_time(camera.shader_time);
        let sun = self
            .console
            .as_mut()
            .filter(|c| c.director.sun_active())
            .and_then(|c| {
                self.world_materials
                    .natural_sun(camera.shader_time)
                    .and_then(|natural| c.director.sun(natural))
            });
        self.world_materials.set_director_sun(&self.queue, sun);
        self.refresh_shot_preview();
        self.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));
        self.view_origin = glam::Vec3::from_array(camera.camera_position);
        self.world_notes.set_camera(camera);
        self.prepare_dust_motes();
        self.prepare_weather(&camera);
        self.ground_hud.set_view(&camera);
        if let Some(layer) = self.post_aa.as_ref().and_then(|aa| aa.effect_layer()) {
            layer.set_view(camera.view_projection);
        }
        if let Some(console) = &mut self.console
            && console.wants_pose()
        {
            let map = if self.resident.exploring() {
                &self.resident.map
            } else {
                &self.world_load_map
            };
            console.answer_pose(&camera.viewpos(), map);
        }
    }
}
