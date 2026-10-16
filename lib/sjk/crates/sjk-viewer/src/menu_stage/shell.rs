//! The Force shell the preview's model wears on the SJK UI's Force page
//! ([`crate::player_menu::force_pose::shell`]): the actor's own surfaces drawn again
//! with the shell's shader, tinted as `CG_Player` tints a player using the power
//! (`force_overlay_submission.rs` does the same for the world's players). The shell
//! shares the actor's skinned vertices, so it follows every frame of the pose.

use super::*;
use crate::player_menu::force_pose::{Shell, Tint};
use crate::scene_flatten::ViewerMaterial;

/// The shells made for one actor: each look compiled once, so moving between powers
/// does not compile a shader again.
pub(super) struct StageShell {
    /// The actor build the shells were made for: a new one needs new shells.
    build: u64,
    /// The actor's draws, every one with the shell's material.
    draws: Vec<DetachedDraw>,
    /// Each look asked for so far; `None` where its shader did not compile.
    compiled: Vec<(Shell, Option<DetachedMaterials>)>,
    /// The look worn now (an index of `compiled`) and the actor's instance tinted
    /// for it.
    worn: Option<(usize, wgpu::Buffer)>,
}

impl StageShell {
    pub(super) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        actor: &'pass StageActor,
        blended: bool,
    ) {
        let Some((index, instance_buffer)) = &self.worn else {
            return;
        };
        let Some(Some(materials)) = self.compiled.get(*index).map(|(_, materials)| materials)
        else {
            return;
        };
        if actor.build != self.build {
            return;
        }
        world_materials.draw_detached(
            pass,
            camera,
            &actor.geometry_binding,
            &actor.vertex_buffer,
            &actor.index_buffer,
            instance_buffer,
            materials,
            &self.draws,
            blended,
        );
    }
}

/// The actor's instance tinted for `look`, as `force_overlay_submission.rs` tints
/// the world's shells.
pub(super) fn tinted(instance: ActorInstance, look: Shell) -> ActorInstance {
    let color = look.rgba.map(|channel| f32::from(channel) / 255.0);
    match look.tint {
        Tint::Shader => instance.with_entity_color(look.rgba),
        Tint::Rgb => instance.with_rgb_tint(color),
        Tint::Rgba => instance.with_rgba_tint(color),
    }
}

/// The actor's draws, every index range given the one shell material.
pub(super) fn shell_draws(actor: &[DetachedDraw]) -> Vec<DetachedDraw> {
    actor
        .iter()
        .map(|draw| DetachedDraw {
            indices: draw.indices.clone(),
            material: 0,
        })
        .collect()
}

impl GpuState {
    /// Put `wanted` on the preview's actor, or take its shell off. A shell whose
    /// shader does not compile stays off and is logged once.
    pub(super) fn sync_stage_shell(&mut self, wanted: Option<Shell>) {
        let Some(actor) = &self.menu_stage.actor else {
            self.menu_stage.shell = None;
            return;
        };
        if self
            .menu_stage
            .shell
            .as_ref()
            .is_none_or(|shell| shell.build != actor.build)
        {
            self.menu_stage.shell = Some(StageShell {
                build: actor.build,
                draws: shell_draws(&actor.draws),
                compiled: Vec::new(),
                worn: None,
            });
        }
        let instance = actor.instance;
        let Some(shell) = &mut self.menu_stage.shell else {
            return;
        };
        let Some(look) = wanted.filter(|_| self.menu_stage.preview_only) else {
            shell.worn = None;
            return;
        };
        let index = match shell.compiled.iter().position(|(seen, _)| *seen == look) {
            Some(index) => index,
            None => {
                let compiled = self.vfs.as_ref().and_then(|vfs| {
                    self.world_materials
                        .compile_detached(
                            &self.device,
                            &self.queue,
                            vfs,
                            &self.shaders,
                            &[ViewerMaterial {
                                shader: look.shader.to_owned(),
                                lightmap: -1,
                            }],
                        )
                        .map_err(|error| {
                            eprintln!("preview Force shell {} did not load: {error}", look.shader);
                        })
                        .ok()
                });
                shell.compiled.push((look, compiled));
                shell.compiled.len() - 1
            }
        };
        if shell.worn.as_ref().is_some_and(|(worn, _)| *worn == index) {
            return;
        }
        let instance = tinted(instance, look);
        let buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK preview Force shell instance"),
                contents: bytemuck::bytes_of(&instance),
                usage: wgpu::BufferUsages::VERTEX,
            });
        shell.worn = Some((index, buffer));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(tint: Tint) -> Shell {
        Shell {
            shader: "gfx/misc/forceprotect",
            rgba: [0, 128, 0, 254],
            tint,
        }
    }

    /// The shader's `rgbGen entity` takes the colour as it is; `RF_RGB_TINT` marks
    /// the override, and the team powers' `RF_FORCE_ENT_ALPHA` the alpha's too.
    #[test]
    fn the_shell_takes_the_power_s_tint() {
        let instance = ActorInstance::new([0.0; 3], [0.0, 0.0, 0.0, 1.0], [1.0; 3]);
        let shader = tinted(instance, look(Tint::Shader));
        assert_eq!(
            shader.entity_color,
            [0.0, 128.0 / 255.0, 0.0, 254.0 / 255.0]
        );
        assert_eq!(shader.entity_control, instance.entity_control);
        let rgb = tinted(instance, look(Tint::Rgb));
        assert_eq!(rgb.entity_control[0], 1.0);
        assert_eq!(rgb.entity_control[1], instance.entity_control[1]);
        let rgba = tinted(instance, look(Tint::Rgba));
        assert_eq!(rgba.entity_control, [1.0; 2]);
    }

    #[test]
    fn the_shell_covers_every_surface_of_the_body() {
        let actor = [
            DetachedDraw {
                indices: 0..30,
                material: 2,
            },
            DetachedDraw {
                indices: 30..90,
                material: 0,
            },
        ];
        let shell = shell_draws(&actor);
        assert_eq!(shell.len(), 2);
        assert!(shell.iter().all(|draw| draw.material == 0));
        assert_eq!(shell[1].indices, 30..90);
    }
}
