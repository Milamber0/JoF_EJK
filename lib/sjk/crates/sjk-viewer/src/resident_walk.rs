//! Local exploration uses the same quantized JKA movement and BSP collision as play.
use crate::{GpuState, input::GameplayInput, local_prediction::movers::Collider};
use glam::Vec3;
use sjk_client::pmove::{MovementConfig, MovementState, Predictor};

pub(super) struct Walk {
    predictor: Predictor,
    solids: Vec<Collider>,
    respawn: MovementState,
    void_height: f32,
    previous_millis: u64,
}

impl Walk {
    pub(super) fn new(gpu: &GpuState) -> Self {
        let mut origin = gpu.camera_position - Vec3::Z * 36.0;
        let bounds = sjk_bsp::Aabb::new([-15.0, -15.0, -24.0], [15.0, 15.0, 40.0]).unwrap();
        if gpu
            .bsp
            .trace_box(
                origin.to_array(),
                origin.to_array(),
                bounds,
                sjk_client::pmove::PLAYER_CONTENT_MASK,
            )
            .start_solid
        {
            if let Ok((eye, _)) = crate::assets::initial_camera(&gpu.bsp) {
                origin = Vec3::from_array(eye) - Vec3::Z * 32.0;
            }
        }
        let state = MovementState {
            origin: origin.to_array(),
            view_angles: [
                -gpu.camera_pitch.to_degrees(),
                gpu.camera_yaw.to_degrees(),
                0.0,
            ],
            movement_type: 0,
            health: 100,
            max_health: 100,
            gravity: 800.0,
            speed: 250.0,
            base_speed: 250.0,
            ground_entity_number: sjk_protocol::ENTITY_NUMBER_NONE,
            view_height: 36,
            standing_height: 40.0,
            crouching_height: 16.0,
            ..Default::default()
        };
        let mut predictor = Predictor::from_state(state.clone(), MovementConfig::default());
        if let Some(mesh) = gpu.actor_meshes.first() {
            predictor.set_animation_lengths(std::sync::Arc::new(
                sjk_client::AnimationLengthTable::from_animation_config(&mesh.preview.config),
            ));
        }
        Self {
            predictor,
            previous_millis: gpu.gameplay_input.motion.now,
            solids: gpu.movers.iter().map(Collider::frozen).collect(),
            respawn: state,
            void_height: gpu.bsp.render().models()[0].minimums[2] - 1024.0,
        }
    }

    pub(super) fn advance(
        &mut self,
        input: &mut GameplayInput,
        pitch: f32,
        yaw: f32,
        bsp: &sjk_bsp::Bsp,
        scratch: &mut sjk_bsp::TraceScratch,
    ) -> Vec3 {
        // CL_CreateCmd uses integer msec, not a fixed 60 Hz integration step.
        // Keep the caller's 8/7/4/3 ms command caps and clamp pathological frame gaps.
        let elapsed = input.motion.now.saturating_sub(self.previous_millis);
        self.previous_millis = input.motion.now;
        if elapsed == 0 {
            let state = self.predictor.state();
            return Vec3::from_array(state.origin) + Vec3::Z * state.view_height as f32;
        }
        let msec = elapsed.min(50) as i32;
        let time = self.predictor.state().command_time.wrapping_add(msec);
        let mut command = input.user_command(time, pitch, yaw, [0; 3], 0, 0, 0);
        // Exploration has movement, not authority to fire or use server-owned items.
        command.buttons &= 16; // BUTTON_WALKING, codemp/qcommon/q_shared.h
        let collision = crate::movement_collision::BspMovementCollision::with_movers(
            bsp,
            scratch,
            &self.solids,
        );
        self.predictor.predict_command(command, &collision);
        input.finish_command();
        if self.predictor.state().origin[2] < self.void_height {
            let mut state = self.respawn.clone();
            state.command_time = time;
            self.predictor = Predictor::from_state(state, MovementConfig::default());
        }
        let state = self.predictor.state();
        Vec3::from_array(state.origin) + Vec3::Z * state.view_height as f32
    }
}
