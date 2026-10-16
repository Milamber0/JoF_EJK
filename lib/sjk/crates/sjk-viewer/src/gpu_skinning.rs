//! Viewer-owned static skin inputs and per-actor joint palette; no trace/readback consumer.
use crate::*;
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Shader skinning input; GLM-local bone references are resolved once at model load.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    position: [f32; 4],
    normal: [f32; 4],
    joints: [u32; 4],
    weights: [f32; 4],
}

/// Row-major affine transform plus an initialized-pose marker.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(crate) struct Joint {
    rows: [[f32; 4]; 3],
    valid: [f32; 4],
}

/// Actor-local range in the shared joint upload slab; allocated at model load.
pub(crate) struct Palette {
    offset: u64,
    count: usize,
}

impl Palette {
    /// Joints in this palette (the mesh's bone count).
    pub(crate) fn len(&self) -> usize {
        self.count
    }

    /// Stage the same evaluated rows into a shared map-lifetime upload slab.
    pub(crate) fn stage(
        &self,
        target: &mut Buffers,
        matrices: &[[[f32; 4]; 3]],
    ) -> Result<(), Box<dyn Error>> {
        if self.count != matrices.len() {
            return Err("skin palette changed size".into());
        }
        let start = self.offset as usize / std::mem::size_of::<Joint>();
        let end = start + self.count;
        for (joint, rows) in target.staged[start..end].iter_mut().zip(matrices) {
            *joint = Joint {
                rows: *rows,
                valid: [1.0, 0.0, 0.0, 0.0],
            };
        }
        if target.dirty.is_empty() {
            target.dirty = start..end;
        } else {
            target.dirty.start = target.dirty.start.min(start);
            target.dirty.end = target.dirty.end.max(end);
        }
        Ok(())
    }
}

/// Map-lifetime read-only vertex storage. Rigid/world vertices have a zero lookup entry.
pub(crate) struct Buffers {
    /// Shared-vertex-index to skin-record index plus one; zero is rigid.
    pub(crate) lookup: wgpu::Buffer,
    /// Bind-space positions, normals and weights, only for skeletal vertices.
    pub(crate) vertices: wgpu::Buffer,
    /// Distinct joint ranges per actor, including corpse-pool actors.
    pub(crate) joints: wgpu::Buffer,
    // Unchanged actors keep their last uploaded rows, including gaps in a dirty span.
    staged: Vec<Joint>,
    dirty: std::ops::Range<usize>,
    /// Whether this scene skins on the GPU at all; decided once at world load.
    enabled: bool,
}

fn buffer(device: &wgpu::Device, label: &str, bytes: &[u8]) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents: bytes,
        usage: USAGE,
    })
}

/// `COPY_SRC`: the buffers grow when a player model is loaded mid-match.
const USAGE: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);

/// Skin records per shared vertex range: the range and its records.
type SkinRecords = Vec<(std::ops::Range<usize>, Vec<Vertex>)>;

/// One mesh's skin records with joints offset by `base`, and for each of its shared
/// vertex ranges the records' indices. GLM-local bone references are resolved here.
fn skin_records(mesh: &ActorMesh, base: usize) -> Result<SkinRecords, Box<dyn Error>> {
    if let Some(rig) = &mesh.preview.rig {
        return rig_skin_records(mesh, rig, base);
    }
    let model = &mesh.preview.mesh;
    let visible: Vec<_> = model.lods[0]
        .surfaces
        .iter()
        .zip(&model.hierarchy)
        .filter(|(_, h)| h.flags & 2 == 0)
        .map(|(s, _)| s)
        .collect();
    mesh.vertex_ranges
        .iter()
        .map(|range| {
            // Match the CPU path's checked lookup: hidden surfaces shorten `visible`,
            // and custom content must not panic the world load.
            let surface = *visible
                .get(range.surface_index)
                .ok_or("skin source surface missing from the visible LOD")?;
            if surface.vertices.len() != range.vertices.len() {
                return Err("skin source topology differs from render topology".into());
            }
            let records = surface
                .vertices
                .iter()
                .map(|source| {
                    if source.weights.len() > 4 {
                        return Err("skin has more than four weights".into());
                    }
                    let mut vertex = Vertex::zeroed();
                    vertex.position[..3].copy_from_slice(&source.position);
                    vertex.position[3] = source.weights.len() as f32;
                    vertex.normal[..3].copy_from_slice(&source.normal);
                    for (slot, weight) in source.weights.iter().enumerate() {
                        vertex.joints[slot] =
                            (base + surface.bone_references[weight.bone_reference]).try_into()?;
                        vertex.weights[slot] = weight.weight;
                    }
                    Ok(vertex)
                })
                .collect::<Result<Vec<Vertex>, Box<dyn Error>>>()?;
            Ok((range.vertices.clone(), records))
        })
        .collect()
}

/// [`skin_records`] for a rig model: its own joints, its surfaces' vertices.
fn rig_skin_records(
    mesh: &ActorMesh,
    rig: &crate::rig_model::RigModel,
    base: usize,
) -> Result<SkinRecords, Box<dyn Error>> {
    mesh.vertex_ranges
        .iter()
        .map(|range| {
            let surface = rig
                .surfaces
                .get(range.surface_index)
                .ok_or("rig surface missing")?;
            if surface.vertices.len() != range.vertices.len() {
                return Err("rig surface topology differs from render topology".into());
            }
            let records = surface
                .vertices
                .iter()
                .map(|source| {
                    let mut vertex = Vertex::zeroed();
                    vertex.position[..3].copy_from_slice(&source.position);
                    vertex.position[3] = source.influences() as f32;
                    vertex.normal[..3].copy_from_slice(&source.normal);
                    for slot in 0..4 {
                        vertex.joints[slot] =
                            (base + usize::from(source.joints[slot])).try_into()?;
                        vertex.weights[slot] = source.weights[slot];
                    }
                    Ok(vertex)
                })
                .collect::<Result<Vec<Vertex>, Box<dyn Error>>>()?;
            Ok((range.vertices.clone(), records))
        })
        .collect()
}

/// Joints in `mesh`'s palette: its rig's joints, or its GLM's bones.
fn palette_len(mesh: &ActorMesh) -> usize {
    mesh.preview
        .rig
        .as_ref()
        .map_or(mesh.preview.mesh.bone_count, |rig| rig.joint_count())
}

impl Buffers {
    /// One queue transfer for all actors, preserving their exact palette offsets and bytes.
    pub(crate) fn flush(&mut self, queue: &crate::frame_queue::FrameQueue) {
        if self.dirty.is_empty() {
            return;
        }
        queue.write_buffer(
            &self.joints,
            (self.dirty.start * std::mem::size_of::<Joint>()) as u64,
            bytemuck::cast_slice(&self.staged[self.dirty.clone()]),
        );
        self.dirty = 0..0;
    }

    /// Rigid-only fallback for tests, menus and worlds without skeletal actors.
    pub(crate) fn empty(device: &wgpu::Device) -> Self {
        Self {
            lookup: buffer(device, "rigid skin lookup", &[0; 4]),
            vertices: buffer(device, "empty skin vertices", &[0; 64]),
            joints: buffer(device, "empty skin joints", &[0; 64]),
            staged: vec![Joint::zeroed()],
            dirty: 0..0,
            enabled: false,
        }
    }

    /// Resolve initial actor meshes onto their existing shared vertex ranges.
    pub(crate) fn actors(
        device: &wgpu::Device,
        count: usize,
        meshes: &mut [ActorMesh],
    ) -> Result<Self, Box<dyn Error>> {
        let mut lookup = vec![0u32; count.max(1)];
        let mut vertices = Vec::new();
        let mut joint_count = 0;
        for mesh in meshes {
            let base = joint_count;
            for (range, records) in skin_records(mesh, base)? {
                for (index, vertex) in range.zip(records) {
                    lookup[index] = u32::try_from(vertices.len() + 1)?;
                    vertices.push(vertex);
                }
            }
            let count = palette_len(mesh);
            joint_count += count;
            mesh.gpu_palette = Some(Palette {
                offset: (base * 64) as u64,
                count,
            });
        }
        Ok(Self {
            lookup: buffer(device, "static skin lookup", bytemuck::cast_slice(&lookup)),
            vertices: buffer(
                device,
                "static skin vertices",
                if vertices.is_empty() {
                    &[0; 64]
                } else {
                    bytemuck::cast_slice(&vertices)
                },
            ),
            joints: buffer(
                device,
                "actor joint palettes",
                &vec![0; joint_count.max(1) * 64],
            ),
            staged: vec![Joint::zeroed(); joint_count.max(1)],
            dirty: 0..0,
            enabled: false,
        })
    }

    /// Palette-skin a mesh appended to the shared buffers after world load (a player who
    /// joined, changed model, or whose client info arrived after the gamestate). Without
    /// this every such actor was skinned on the CPU and re-uploaded each frame. The
    /// lookup grows to `shared_vertices`; a mesh that cannot be palette skinned stays on
    /// the CPU path by itself. The caller rebinds the geometry group.
    pub(crate) fn append_actor(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        shared_vertices: u32,
        mesh: &mut ActorMesh,
    ) -> Result<bool, Box<dyn Error>> {
        if !self.enabled {
            return Ok(false);
        }
        let base = self.staged.len();
        let records = skin_records(mesh, base)?;
        let first = self.vertices.size() / std::mem::size_of::<Vertex>() as u64;
        let appended: Vec<Vertex> = records
            .iter()
            .flat_map(|(_, r)| r.iter().copied())
            .collect();
        let bones = palette_len(mesh);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("SJK skinning growth"),
        });
        let grow = |encoder: &mut wgpu::CommandEncoder, old: &wgpu::Buffer, label, size: u64| {
            let new = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(old.size()),
                usage: USAGE,
                mapped_at_creation: false,
            });
            encoder.copy_buffer_to_buffer(old, 0, &new, 0, old.size());
            new
        };
        // New buffer bytes are zero: rigid lookup entries and not-yet-posed joints.
        let lookup = grow(
            &mut encoder,
            &self.lookup,
            "skin lookup",
            u64::from(shared_vertices) * 4,
        );
        let vertices = grow(
            &mut encoder,
            &self.vertices,
            "skin vertices",
            self.vertices.size() + (appended.len() * std::mem::size_of::<Vertex>()) as u64,
        );
        let joints = grow(
            &mut encoder,
            &self.joints,
            "actor joint palettes",
            ((base + bones) * std::mem::size_of::<Joint>()) as u64,
        );
        // Copy first, write afterwards: queued writes land ahead of the next submission,
        // and a mesh's lookup range can lie inside the old buffer the copy overwrites.
        queue.submit([encoder.finish()]);
        queue.write_buffer(
            &vertices,
            self.vertices.size(),
            bytemuck::cast_slice(&appended),
        );
        let mut record = u32::try_from(first)? + 1;
        for (range, skinned) in &records {
            let keys: Vec<u32> = (record..record + skinned.len() as u32).collect();
            queue.write_buffer(&lookup, range.start as u64 * 4, bytemuck::cast_slice(&keys));
            record += skinned.len() as u32;
        }
        (self.lookup, self.vertices, self.joints) = (lookup, vertices, joints);
        self.staged.resize(base + bones, Joint::zeroed());
        mesh.gpu_palette = Some(Palette {
            offset: (base * std::mem::size_of::<Joint>()) as u64,
            count: bones,
        });
        Ok(true)
    }
}

/// Upload shared geometry, optionally enabling render skinning independently of CPU traces.
pub(crate) fn upload(
    device: &wgpu::Device,
    meshes: &mut [ActorMesh],
    scene: &FlattenedScene,
) -> Result<SharedGeometry, Box<dyn Error>> {
    let mut geometry = SharedGeometry::upload(device, &scene.vertices, &scene.indices);
    // Default on: measured pixel-identical output, 27x less pose time and 29.5x less
    // upload traffic. `SJK_GPU_SKINNING=0` restores CPU skinning as an escape hatch,
    // sampled once at load, never in a frame.
    if std::env::var("SJK_GPU_SKINNING").as_deref() == Ok("0") {
        return Ok(geometry);
    }
    // A model whose skin topology differs from its render topology cannot be palette
    // skinned. Fall back to CPU skinning for the whole scene rather than failing the
    // world load, because custom content is exactly where that mismatch shows up.
    match Buffers::actors(device, scene.vertices.len(), meshes) {
        Ok(mut skinning) => {
            skinning.enabled = true;
            geometry.skinning = skinning;
            geometry.rebind(device);
        }
        Err(error) => {
            // `actors` marks each mesh as it succeeds, so a later failure would leave
            // earlier meshes skinning against empty joint buffers. Clear them all so
            // every actor takes the CPU path together.
            for mesh in meshes {
                mesh.gpu_palette = None;
            }
            crate::log::progress(format_args!(
                "GPU skinning unavailable, using CPU skinning: {error}"
            ));
        }
    }
    Ok(geometry)
}
