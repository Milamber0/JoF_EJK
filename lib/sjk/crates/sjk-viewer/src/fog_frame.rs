//! The frame's fog setting.
use crate::GpuState;

impl GpuState {
    /// Read the cached console toggle without allocating or locking.
    pub(crate) fn update_fog_setting(&mut self) {
        self.world_materials.fog_mode = self
            .console
            .as_ref()
            .map_or_else(Default::default, |console| console.draw_fog_mode());
    }
}
