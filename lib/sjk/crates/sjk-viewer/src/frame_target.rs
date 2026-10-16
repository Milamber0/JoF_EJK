//! Acquire the window swapchain image for a frame.

use crate::GpuState;
use crate::gpu_context::FrameStatus;

#[path = "post_aa.rs"]
/// Optional scene-only post-process, independent of depth and simulation.
pub(crate) mod aa;

#[path = "render_scale.rs"]
/// Single-sample scene supersampling, resolved before postprocessing and HUD.
pub(crate) mod scale;

/// This frame's swapchain image, its default (scene) view and its 2D view.
pub(crate) struct Output {
    pub(crate) frame: Option<wgpu::SurfaceTexture>,
    /// Default view: linear results are encoded by the sRGB format.
    pub(crate) view: wgpu::TextureView,
    /// UNORM view the 2D layer draws display values through
    /// ([`crate::ui_target`]); the default view when the image is not sRGB,
    /// or when the surface cannot alias it and the 2D layer uses the display pass.
    pub(crate) ui: wgpu::TextureView,
}

/// Scene target and any swapchain image acquired before recording the world.
pub(crate) struct Prepared {
    pub(crate) scene: wgpu::TextureView,
    early: Option<Output>,
}

/// Defer swapchain acquisition when the scene already has an offscreen target.
pub(crate) fn prepare(gpu: &mut GpuState) -> Result<Prepared, FrameStatus> {
    if let Some(scene) = gpu.offscreen_scene_target().cloned() {
        Ok(Prepared { scene, early: None })
    } else {
        let early = gpu.acquire_output()?;
        Ok(Prepared {
            scene: early.view.clone(),
            early: Some(early),
        })
    }
}

impl Prepared {
    /// Acquire the presentation target after world recording. On surface failure,
    /// submit the recorded work without presenting so encoder workers can finish.
    pub(crate) fn finish(
        self,
        gpu: &mut GpuState,
        encoder: wgpu::CommandEncoder,
        timing: &mut crate::frame_pacing::budget::Timer,
    ) -> Result<(Output, wgpu::CommandEncoder), FrameStatus> {
        use crate::frame_pacing::budget::Phase;
        let output = match self.early {
            Some(output) => output,
            None => {
                timing.mark(Phase::Acquire);
                match gpu.acquire_output() {
                    Ok(output) => output,
                    Err(status) => {
                        timing.mark(Phase::Submit);
                        gpu.frame_pacer
                            .split
                            .submit(&gpu.queue, encoder, None, timing);
                        return Err(status);
                    }
                }
            }
        };
        Ok((output, encoder))
    }
}

/// Acquire this frame's colour target. `Err` is the status to report
/// instead of rendering (the surface needs reconfiguring, or the frame is
/// skipped).
pub(crate) fn acquire(gpu: &GpuState) -> Result<Output, FrameStatus> {
    let frame = if let Some(surface) = &gpu.context.surface {
        match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                return Err(FrameStatus::Reconfigure);
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Validation => return Err(FrameStatus::Skip),
        }
    } else if let Some(texture) = &gpu.headless_frame {
        // Off-screen world shots: the image stands in for the swapchain's.
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let ui = crate::ui_target::surface_view(texture, gpu.context.format, gpu.context.ui_direct)
            .unwrap_or_else(|| view.clone());
        return Ok(Output {
            frame: None,
            view,
            ui,
        });
    } else {
        return Err(FrameStatus::Skip);
    };
    let texture = &frame.as_ref().expect("surface acquired").texture;
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let ui = crate::ui_target::surface_view(texture, gpu.context.format, gpu.context.ui_direct)
        .unwrap_or_else(|| view.clone());
    Ok(Output { frame, view, ui })
}

/// How the world pass starts on its colour target: cleared to the night tint.
pub(crate) fn world_load() -> wgpu::LoadOp<wgpu::Color> {
    wgpu::LoadOp::Clear(wgpu::Color {
        r: 0.015,
        g: 0.02,
        b: 0.035,
        a: 1.0,
    })
}

impl GpuState {
    /// Wait until the previous frame is submitted and presented, map its readbacks, then
    /// acquire this frame's swapchain image.
    fn acquire_output(&mut self) -> Result<Output, FrameStatus> {
        self.frame_pacer.split.wait_previous();
        if let Some(phases) = &self.gpu_phases {
            phases.after_submit();
            phases.report();
        }
        self.screenshots.after_submit();
        acquire(self)
    }
}
