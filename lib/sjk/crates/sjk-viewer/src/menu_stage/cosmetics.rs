//! JoF EJK's hat and cape on the stage model: what `color1` and `color2`
//! wear (read from the cvars, so the stage shows what the userinfo sends),
//! each a detached mesh like the hilts, placed on the `*head_top` and
//! `*back` bolts of the pose the stage was skinned with, as
//! `UI_DrawCosmeticsOnCharacter` bolts them on JoF's preview model. Off with
//! `cg_cosmetics 0`.

use super::*;
use crate::cosmetics::{VISIBILITY_CVAR, Visibility, fitting_offset, model_path, placement_at};
use sjk_client::CosmeticSlot;

/// Bolt each slot is worn on.
const BOLTS: [&str; 2] = ["*head_top", "*back"];

/// What the stage wears, for comparing with what is wanted: the names and
/// the model they were fitted to.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Request {
    names: [Option<String>; 2],
    model: String,
}

/// One worn piece.
pub(super) struct StageCosmetic {
    slot: CosmeticSlot,
    offset: [f32; 3],
    /// The bolt of the current pose exists, so the piece is drawn.
    placed: bool,
    draws: Vec<DetachedDraw>,
    materials: DetachedMaterials,
    vertex_buffer: wgpu::Buffer,
    geometry_binding: wgpu::BindGroup,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
}

impl StageCosmetic {
    pub(super) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        blended: bool,
    ) {
        if !self.placed {
            return;
        }
        world_materials.draw_detached(
            pass,
            camera,
            &self.geometry_binding,
            &self.vertex_buffer,
            &self.index_buffer,
            &self.instance_buffer,
            &self.materials,
            &self.draws,
            blended,
        );
    }
}

impl GpuState {
    /// Load the pieces `color1`/`color2` name when they or the model changed
    /// (an unchanged frame only compares borrowed names).
    pub(super) fn sync_stage_cosmetics(&mut self) {
        let Some(actor) = self.menu_stage.actor.as_ref() else {
            return;
        };
        let console = self.console.as_ref();
        let shown = console.is_none_or(|console| {
            Visibility::from_cvar(console.integer_cvar(VISIBILITY_CVAR).unwrap_or(1)).shows(true)
        });
        let wanted = CosmeticSlot::ALL.map(|slot| {
            console
                .filter(|_| shown)
                .and_then(|console| console.text_value(slot.cvar()))
                .and_then(|value| sjk_client::split_color_value(value).1)
        });
        let current = &self.menu_stage.cosmetic_request;
        if current.model == actor.model
            && current
                .names
                .iter()
                .zip(wanted)
                .all(|(current, wanted)| current.as_deref() == wanted)
        {
            return;
        }
        let request = Request {
            names: wanted.map(|name| name.map(str::to_owned)),
            model: actor.model.clone(),
        };
        let (directory, skin) = split_model_cvar(&request.model);
        let skin = skin.to_owned();
        for slot in CosmeticSlot::ALL {
            let index = slot.index();
            self.menu_stage.cosmetics[index] = request.names[index].as_deref().and_then(|name| {
                self.build_stage_cosmetic(slot, name, &directory, &skin)
                    .inspect_err(|error| {
                        eprintln!("player stage could not wear {name}: {error}");
                    })
                    .ok()
            });
        }
        self.menu_stage.cosmetic_request = request;
        self.menu_stage.cosmetics_dirty = true;
    }

    fn build_stage_cosmetic(
        &mut self,
        slot: CosmeticSlot,
        name: &str,
        model: &str,
        skin: &str,
    ) -> Result<StageCosmetic, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("player stage has no VFS")?;
        let path = model_path(&vfs, slot, name).ok_or("not installed")?;
        let asset = vfs.read(&path)?.ok_or("model missing")?;
        let md3 = Md3::parse(&asset.bytes)?;
        let mut flattened = FlattenedScene::default();
        let draws = append_md3_mesh(&mut flattened, &md3)?;
        let materials = self.world_materials.compile_detached(
            &self.device,
            &self.queue,
            &vfs,
            &self.shaders,
            &flattened.materials,
        )?;
        let draws = draws
            .into_iter()
            .map(|draw| DetachedDraw {
                indices: draw.indices,
                material: draw.material,
            })
            .collect();
        let vertex_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK player stage cosmetic vertices"),
                contents: bytemuck::cast_slice(&flattened.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK player stage cosmetic indices"),
                contents: bytemuck::cast_slice(&flattened.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let quads = crate::shared_geometry::quads::upload(
            &self.device,
            flattened.vertices.len(),
            &flattened.indices,
            0,
        );
        let geometry_binding =
            crate::shared_geometry::quads::bind(&self.device, &vertex_buffer, &quads);
        let instance = ActorInstance::new([0.0; 3], Quat::IDENTITY.to_array(), [1.0; 3]);
        let instance_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK player stage cosmetic instance"),
                contents: bytemuck::bytes_of(&instance),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        Ok(StageCosmetic {
            slot,
            offset: fitting_offset(&vfs, slot, name, model, skin),
            placed: false,
            draws,
            materials,
            vertex_buffer,
            geometry_binding,
            index_buffer,
            instance_buffer,
        })
    }

    /// Put the pieces on the bolts of the stage's current pose, when the
    /// pose or the pieces changed.
    pub(super) fn place_stage_cosmetics(&mut self) {
        let stage = &mut self.menu_stage;
        if !std::mem::take(&mut stage.cosmetics_dirty) {
            return;
        }
        let Some(actor) = &stage.actor else {
            return;
        };
        if stage.cosmetics.iter().all(Option::is_none) {
            return;
        }
        // The pose the stage was skinned with, blended between frames.
        let matrices = Some(&actor.pose);
        for cosmetic in stage.cosmetics.iter_mut().flatten() {
            let bolt = matrices.as_ref().and_then(|matrices| {
                actor
                    .preview
                    .mesh
                    .surface_bolt_matrix(BOLTS[cosmetic.slot.index()], 0, matrices)
                    .ok()
                    .flatten()
            });
            let instance = bolt.and_then(|bolt| {
                placement_at(
                    bolt,
                    actor.origin,
                    actor.rotation,
                    Vec3::ONE,
                    cosmetic.offset,
                )
            });
            cosmetic.placed = instance.is_some();
            if let Some(mut instance) = instance {
                instance.set_light(actor.light);
                self.queue.write_buffer(
                    &cosmetic.instance_buffer,
                    0,
                    bytemuck::bytes_of(&instance),
                );
            }
        }
    }
}
