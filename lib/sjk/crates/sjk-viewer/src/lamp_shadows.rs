//! Budgeted actor shadows. Static visibility always uses the map-lifetime atlas.
use glam::Vec3;
#[path = "lamp_shadow_candidates.rs"]
mod candidates;
#[path = "lamp_shadow_selection.rs"]
mod selection;

pub(super) const SLOTS: usize = 8;
const FACES: usize = 6;
const SIZE: u32 = 256;
const NEAR: f32 = 4.;

/// Per-frame shadow slots and their maps.
pub(in crate::world_materials) struct Runtime {
    static_depth: wgpu::TextureView,
    dynamic_depth: wgpu::TextureView,
    dynamic_layers: Vec<wgpu::TextureView>,
    cameras: Vec<(wgpu::Buffer, wgpu::BindGroup)>,
    table: wgpu::Buffer,
    camera_dirty: std::cell::Cell<u64>,
    cache_cameras: bool,
    candidates: Option<candidates::Candidates>,
    candidate_reference: std::cell::Cell<bool>,
    assigned: std::cell::RefCell<Vec<Option<usize>>>,
    matrices: std::cell::RefCell<Vec<glam::Mat4>>,
    /// One bit per face layer whose depth is known to hold only the clear value.
    clear: std::cell::Cell<u64>,
    /// This frame's slot table, uploaded by `publish` once the faces are encoded.
    pending: std::cell::RefCell<Table>,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Table {
    lamps: [[u32; 4]; 2],
    count: [u32; 4],
    weights: [[f32; 4]; 2],
    vp: [[[f32; 4]; 4]; SLOTS * FACES],
}

/// Layout entries at `base`: static depth array, dynamic depth array, slot table.
pub(in crate::world_materials) fn layout_entries(base: u32) -> [wgpu::BindGroupLayoutEntry; 3] {
    let stages = wgpu::ShaderStages::FRAGMENT;
    let depth = |binding| wgpu::BindGroupLayoutEntry {
        binding: base + binding,
        visibility: stages,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Depth,
            view_dimension: wgpu::TextureViewDimension::D2Array,
            multisampled: false,
        },
        count: None,
    };
    [
        depth(0),
        depth(1),
        wgpu::BindGroupLayoutEntry {
            binding: base + 2,
            visibility: stages,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        },
    ]
}

fn depth_array(
    device: &wgpu::Device,
    label: &str,
    size: u32,
) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: (SLOTS * FACES) as u32,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: crate::DepthTarget::FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let layers = (0..(SLOTS * FACES) as u32)
        .map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (array, layers)
}

/// One-texel stand-ins and an empty table for groups without lamp shadows.
pub(in crate::world_materials) fn neutral(
    device: &wgpu::Device,
) -> (wgpu::TextureView, wgpu::TextureView, wgpu::Buffer) {
    let (a, _) = depth_array(device, "SJK lamp shadows neutral", 1);
    let (b, _) = depth_array(device, "SJK lamp shadows neutral", 1);
    let table = wgpu::util::DeviceExt::create_buffer_init(
        device,
        &wgpu::util::BufferInitDescriptor {
            label: Some("SJK lamp shadow table"),
            contents: bytemuck::bytes_of(&Table {
                lamps: [[u32::MAX; 4]; 2],
                count: [0; 4],
                weights: [[0.; 4]; 2],
                vp: [[[0.; 4]; 4]; SLOTS * FACES],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        },
    );
    (a, b, table)
}

/// The six face axes (+X, -X, +Y, -Y, +Z, -Z) and their up vectors.
const FACE_AXES: [(Vec3, Vec3); FACES] = [
    (Vec3::X, Vec3::Z),
    (Vec3::NEG_X, Vec3::Z),
    (Vec3::Y, Vec3::Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y),
];

/// Whether a sphere of `radius` at `offset` from the lamp touches `face`'s frustum.
pub(super) fn face_contains(face: usize, offset: Vec3, radius: f32) -> bool {
    let axis = FACE_AXES[face].0;
    let along = offset.dot(axis);
    let across = (offset - axis * along).abs().max_element();
    along + radius > 0. && across <= along + 2. * radius
}

/// Each face sees a little past its 90 degree cell (tan 1.08) so the faces overlap: a
/// receiver picks its face by major axis and, at the seam, still samples inside that
/// face with room for the filter taps, instead of an unshadowed line along the seam.
const FACE_OVERLAP: f32 = 1.08;

fn face_matrix(position: Vec3, radius: f32, face: usize) -> glam::Mat4 {
    let (axis, up) = FACE_AXES[face];
    glam::camera::rh::proj::directx::perspective(
        2. * FACE_OVERLAP.atan(),
        1.,
        NEAR,
        radius.max(NEAR + 1.),
    ) * glam::camera::rh::view::look_at_mat4(position, position + axis, up)
}

impl Runtime {
    pub(super) fn new(
        device: &wgpu::Device,
        camera_layout: &wgpu::BindGroupLayout,
        lamps: &crate::lamp_lights::LampSet,
    ) -> Self {
        // Small source arrays are cheaper to scan; the index pays on dense maps.
        let indexed = match std::env::var("SJK_LAMP_SHADOW_BVH").as_deref() {
            Ok("0") => false,
            Ok("1") => true,
            _ => lamps.lamps.len() >= 8192,
        };
        // Keep the existing binding layout; static depth is no longer camera selected.
        let (static_depth, _) = depth_array(device, "SJK lamp static binding placeholder", 1);
        let (dynamic_depth, dynamic_layers) = depth_array(device, "SJK lamp shadows actors", SIZE);
        let cameras = (0..SLOTS * FACES)
            .map(|_| {
                let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("SJK lamp face camera"),
                    size: std::mem::size_of::<crate::CameraUniform>() as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: camera_layout,
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: buffer.as_entire_binding(),
                    }],
                });
                (buffer, group)
            })
            .collect();
        let table = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("SJK lamp shadow table"),
            size: std::mem::size_of::<Table>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            static_depth,
            dynamic_depth,
            dynamic_layers,
            cameras,
            table,
            candidates: (indexed)
                .then(|| candidates::Candidates::new(&lamps.lamps, selection::EXTRA_RADIUS)),
            candidate_reference: std::cell::Cell::new(!indexed),
            camera_dirty: std::cell::Cell::new(u64::MAX),
            cache_cameras: std::env::var("SJK_LAMP_CAMERA_CACHE").as_deref() != Ok("0"),
            assigned: std::cell::RefCell::new(vec![None; SLOTS]),
            matrices: std::cell::RefCell::new(vec![glam::Mat4::IDENTITY; SLOTS * FACES]),
            clear: std::cell::Cell::new(0),
            pending: std::cell::RefCell::new(bytemuck::Zeroable::zeroed()),
        }
    }

    /// Bind group entries at `base`.
    pub(in crate::world_materials) fn entries(&self, base: u32) -> [wgpu::BindGroupEntry<'_>; 3] {
        [
            wgpu::BindGroupEntry {
                binding: base,
                resource: wgpu::BindingResource::TextureView(&self.static_depth),
            },
            wgpu::BindGroupEntry {
                binding: base + 1,
                resource: wgpu::BindingResource::TextureView(&self.dynamic_depth),
            },
            wgpu::BindGroupEntry {
                binding: base + 2,
                resource: self.table.as_entire_binding(),
            },
        ]
    }

    /// Select actor-shadow lamps with a continuous weight at every ranking boundary.
    /// Return the assigned slot and source; static visibility does not use this table.
    /// `publish` uploads the table once the frame's faces are known.
    pub(super) fn select(
        &self,
        lamps: &crate::lamp_lights::LampSet,
        eye: Vec3,
    ) -> impl Iterator<Item = (usize, usize)> {
        let wanted = selection::select_with_candidates(
            lamps,
            eye,
            self.candidates
                .as_ref()
                .filter(|_| !self.candidate_reference.get()),
        );
        let mut assigned = self.assigned.borrow_mut();
        let mut matrices = self.matrices.borrow_mut();
        for holder in assigned.iter_mut() {
            if holder.is_some_and(|lamp| !wanted.iter().any(|w| w.0 == lamp)) {
                *holder = None;
            }
        }
        let mut plan = [(0, 0); SLOTS];
        let mut count = 0;
        let mut table = Table {
            lamps: [[u32::MAX; 4]; 2],
            count: [0; 4],
            weights: [[0.; 4]; 2],
            vp: [[[0.; 4]; 4]; SLOTS * FACES],
        };
        for (lamp, weight) in wanted.into_iter().filter(|w| w.0 != usize::MAX && w.1 > 0.) {
            let slot = assigned
                .iter()
                .position(|a| *a == Some(lamp))
                .or_else(|| assigned.iter().position(Option::is_none))
                .unwrap();
            if assigned[slot] != Some(lamp) {
                assigned[slot] = Some(lamp);
                self.camera_dirty
                    .set(self.camera_dirty.get() | (((1u64 << FACES) - 1) << (slot * FACES)));
                let light = &lamps.lamps[lamp];
                for face in 0..FACES {
                    matrices[slot * FACES + face] = face_matrix(light.position, light.radius, face);
                }
            }
            table.lamps[slot / 4][slot % 4] = lamp as u32;
            table.weights[slot / 4][slot % 4] = weight;
            plan[count] = (slot, lamp);
            count += 1;
        }
        for (i, m) in matrices.iter().enumerate() {
            table.vp[i] = m.to_cols_array_2d();
        }
        *self.pending.borrow_mut() = table;
        plan.into_iter().take(count)
    }

    /// Camera of `slot`'s `face`. Static lamps only change when a slot is reassigned.
    pub(super) fn face_camera(
        &self,
        queue: &crate::frame_queue::FrameQueue,
        lamps: &crate::lamp_lights::LampSet,
        slot: usize,
        lamp: usize,
        face: usize,
    ) -> (&wgpu::BindGroup, glam::Mat4) {
        let matrix = self.matrices.borrow()[slot * FACES + face];
        let position = lamps.lamps[lamp].position;
        let (buffer, group) = &self.cameras[slot * FACES + face];
        let bit = 1u64 << (slot * FACES + face);
        if !self.cache_cameras || self.camera_dirty.get() & bit != 0 {
            queue.write_buffer(
                buffer,
                0,
                bytemuck::bytes_of(&crate::CameraUniform {
                    view_projection: matrix.to_cols_array_2d(),
                    camera_position: position.to_array(),
                    shader_time: 0.,
                    view_forward: FACE_AXES[face].0.to_array(),
                    _padding: 0.,
                }),
            );
            self.camera_dirty.set(self.camera_dirty.get() & !bit);
        }
        (group, matrix)
    }

    /// Upload the slot table with the faces that hold a caster this frame (`count.yz`, one
    /// bit per layer): receivers skip the filtered depth compares of every empty face.
    pub(super) fn publish(&self, queue: &crate::frame_queue::FrameQueue) {
        let mut table = self.pending.borrow_mut();
        let occupied = !self.clear.get();
        table.count[1] = occupied as u32;
        table.count[2] = (occupied >> 32) as u32;
        queue.write_buffer(&self.table, 0, bytemuck::bytes_of(&*table));
    }

    /// Whether the face's last pass drew no caster, so its depth is still all clear.
    pub(super) fn is_clear(&self, slot: usize, face: usize) -> bool {
        self.clear.get() >> (slot * FACES + face) & 1 == 1
    }
    /// Record what the pass being encoded leaves in the face layer.
    pub(super) fn set_clear(&self, slot: usize, face: usize, clear: bool) {
        let bit = 1u64 << (slot * FACES + face);
        self.clear.set(if clear {
            self.clear.get() | bit
        } else {
            self.clear.get() & !bit
        });
    }

    pub(super) fn dynamic_layer(&self, slot: usize, face: usize) -> &wgpu::TextureView {
        &self.dynamic_layers[slot * FACES + face]
    }
    pub(super) const FACES: usize = FACES;
}
