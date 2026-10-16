//! Light mirrors in their own light-buffer images, leaving the main view's intact.
//!
//! Each floor reflection is a whole scene lit in the light buffer before it is drawn.
//! With a second set of images the mirrors are lit there instead of over the main
//! view's light, so the main light is neither saved before them nor restored after.
use super::*;

impl LightBuffer {
    pub(in crate::world_materials) fn preservation_enabled(&self) -> bool {
        self.mirror.is_some()
    }

    pub(in crate::world_materials) fn configure_preservation(
        &mut self,
        device: &wgpu::Device,
        enabled: bool,
    ) {
        self.mirror = enabled.then(|| Images::new(device, self.size, self.directed));
        self.mirroring.set(false);
        self.receivers
            .configure_mirror(device, self.mirror.as_ref());
    }
}

impl super::super::super::Runtime {
    /// Allocate at map installation only when automatic floor reflections can use it.
    pub(crate) fn configure_light_preservation(&mut self, device: &wgpu::Device, enabled: bool) {
        let Some(shadow) = self.shadows.as_mut() else {
            return;
        };
        let Some(light) = shadow.light.as_mut() else {
            return;
        };
        light.configure_preservation(device, enabled);
        shadow.mirror_groups = shadow.build_mirror_groups(device);
        let mirror = shadow.mirror_groups.as_ref().map(|groups| &groups.receiver);
        if let Some(sun) = &mut self.forge.model_sun {
            sun.set_mirror(mirror);
        }
    }

    /// Light and draw mirrors in their own images until `end_mirror_light`. False when
    /// there are none: mirrors then overwrite the main view's light, which has to be
    /// lit again after them.
    pub(crate) fn begin_mirror_light(&self) -> bool {
        self.select_mirror_light(true)
    }

    /// Back to the main view's light, untouched by the mirrors.
    pub(crate) fn end_mirror_light(&self) {
        self.select_mirror_light(false);
    }

    fn select_mirror_light(&self, mirror: bool) -> bool {
        let Some(light) = self.shadows.as_ref().and_then(|s| s.light.as_ref()) else {
            return false;
        };
        if light.mirror.is_none() {
            return false;
        }
        light.mirroring.set(mirror);
        if let Some(sun) = &self.forge.model_sun {
            sun.mirroring.set(mirror);
        }
        true
    }
}
