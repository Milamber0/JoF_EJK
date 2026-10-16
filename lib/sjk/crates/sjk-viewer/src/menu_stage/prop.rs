//! A rigid model (`.md3` or `.glm`) on the stage in place of the player, for the
//! `modelviewer` page: scaled to the height of a player, centred on the stage model's
//! spot and turning slowly, lit as the player is. The player's own actor stays loaded
//! behind it (for the stage's spot, light and the preview's camera) and is not drawn
//! while a prop shows ([`MenuStage::draw_parts`]).

use super::*;

/// The longest side of a prop on the stage, in game units (a player stands 56 high).
const FITTED_SIZE: f32 = 64.0;
/// How far above the stage model's origin the prop's centre stands: the middle of the
/// body the camera frames.
const CENTRE_ABOVE_ORIGIN: f32 = 8.0;
/// Radians a second the prop turns.
const TURN: f32 = 0.7;

/// The prop on show.
pub(super) struct StageProp {
    /// The asset path it was built from.
    pub(super) path: String,
    /// The middle of its box, and the scale that fits it to [`FITTED_SIZE`].
    centre: Vec3,
    scale: f32,
    began: Instant,
    draws: Vec<DetachedDraw>,
    materials: DetachedMaterials,
    vertex_buffer: wgpu::Buffer,
    geometry_binding: wgpu::BindGroup,
    index_buffer: wgpu::Buffer,
    instance_buffer: wgpu::Buffer,
}

impl StageProp {
    pub(super) fn draw<'pass>(
        &'pass self,
        pass: &mut wgpu::RenderPass<'pass>,
        world_materials: &'pass world_materials::Runtime,
        camera: &'pass wgpu::BindGroup,
        blended: bool,
    ) {
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

/// The middle of the box round `vertices` and the scale that fits its longest side to
/// [`FITTED_SIZE`] (1 for a model with no extent).
fn fit(vertices: &[GpuVertex]) -> (Vec3, f32) {
    if vertices.is_empty() {
        return (Vec3::ZERO, 1.0);
    }
    let mut low = Vec3::splat(f32::MAX);
    let mut high = Vec3::splat(f32::MIN);
    for vertex in vertices {
        let at = Vec3::from_array(vertex.position);
        low = low.min(at);
        high = high.max(at);
    }
    let longest = (high - low).max_element();
    let scale = if longest > 1e-3 {
        FITTED_SIZE / longest
    } else {
        1.0
    };
    ((low + high) * 0.5, scale)
}

impl GpuState {
    /// Show the rigid model the model viewer has chosen, or none: build it when it
    /// changed, and turn it on the stage model's spot.
    pub(super) fn sync_stage_prop(&mut self, now: Instant) {
        let wanted = self
            .client_menu
            .as_ref()
            .and_then(menu::ClientMenu::viewer_prop);
        let Some(path) = wanted else {
            self.menu_stage.prop = None;
            self.menu_stage.prop_failed = None;
            return;
        };
        let build = match &self.menu_stage.prop {
            Some(prop) if prop.path == path => false,
            _ => self.menu_stage.prop_failed.as_deref() != Some(path),
        };
        if build {
            let path = path.to_owned();
            match self.build_stage_prop(&path, now) {
                Ok(prop) => {
                    self.menu_stage.prop = Some(prop);
                    self.menu_stage.prop_failed = None;
                }
                Err(error) => {
                    eprintln!("model viewer could not load {path}: {error}");
                    self.menu_stage.prop = None;
                    self.menu_stage.prop_failed = Some(path);
                }
            }
        }
        self.place_stage_prop(now);
    }

    fn build_stage_prop(&mut self, path: &str, now: Instant) -> Result<StageProp, Box<dyn Error>> {
        let vfs = self.vfs.clone().ok_or("player stage has no VFS")?;
        let asset = vfs.read(path)?.ok_or("model missing")?;
        let mut flattened = FlattenedScene::default();
        let draws = if path.ends_with(".glm") {
            append_static_glm_mesh(&mut flattened, &Glm::parse(&asset.bytes)?)?
        } else {
            append_md3_mesh(&mut flattened, &Md3::parse(&asset.bytes)?)?
        };
        if draws.is_empty() {
            return Err("nothing to draw".into());
        }
        let (centre, scale) = fit(&flattened.vertices);
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
                label: Some("SJK stage prop vertices"),
                contents: bytemuck::cast_slice(&flattened.vertices),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::STORAGE,
            });
        let index_buffer = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("SJK stage prop indices"),
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
                label: Some("SJK stage prop instance"),
                contents: bytemuck::bytes_of(&instance),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
        Ok(StageProp {
            path: path.to_owned(),
            centre,
            scale,
            began: now,
            draws,
            materials,
            vertex_buffer,
            geometry_binding,
            index_buffer,
            instance_buffer,
        })
    }

    /// Turn the prop about its middle on the stage model's spot, lit as that is.
    fn place_stage_prop(&mut self, now: Instant) {
        let (Some(prop), Some(actor)) = (&self.menu_stage.prop, &self.menu_stage.actor) else {
            return;
        };
        let turn = now.saturating_duration_since(prop.began).as_secs_f32() * TURN;
        let rotation = Quat::from_rotation_z(actor.yaw + turn);
        let middle = actor.origin + Vec3::Z * CENTRE_ABOVE_ORIGIN;
        let position = middle - rotation * (prop.centre * prop.scale);
        let mut instance =
            ActorInstance::new(position.to_array(), rotation.to_array(), [prop.scale; 3]);
        instance.set_light(actor.light);
        self.queue
            .write_buffer(&prop.instance_buffer, 0, bytemuck::bytes_of(&instance));
    }
}
