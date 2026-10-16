//! The combined vertex/index buffers every world, actor and object draw reads
//! from. They are uploaded once at map load with room to spare ([`room`]) and
//! take geometry the load did not know about (a player's new model, a hilt
//! nobody carried yet) behind what they hold: a write into the spare room while
//! it lasts, else a reallocation with the old contents copied on the GPU and
//! room to spare again. Growing by exactly the new mesh made every model that
//! came in during a match reallocate and copy the whole map's geometry. That
//! happens per change, not per frame, so the hot path only ever sees a plain
//! buffer handle.

use super::{ActorDraw, GpuVertex, PreviewVertexRange};

#[path = "geometry_environment.rs"]
pub(crate) mod environment;
#[path = "quad_geometry.rs"]
pub(crate) mod quads;

pub(crate) struct SharedGeometry {
    pub(crate) vertex_buffer: wgpu::Buffer,
    pub(crate) index_buffer: wgpu::Buffer,
    pub(crate) deform_binding: wgpu::BindGroup,
    quad_buffer: wgpu::Buffer,
    vertex_count: u32,
    index_count: u32,
    /// What the buffers can hold; the quad lookup has one entry a vertex.
    vertex_capacity: u32,
    index_capacity: u32,
    environment_buffer: wgpu::Buffer,
    pub(crate) environment: environment::State,
    /// Static skin inputs and per-actor joint palettes, independent of CPU trace storage.
    pub(crate) skinning: crate::actor_pose::gpu_skinning::Buffers,
}

/// Where an appended mesh landed in the shared buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Placement {
    pub(crate) vertex_base: u32,
    pub(crate) index_base: u32,
}

const VERTEX_USAGE: wgpu::BufferUsages = wgpu::BufferUsages::VERTEX
    .union(wgpu::BufferUsages::STORAGE)
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);
const INDEX_USAGE: wgpu::BufferUsages = wgpu::BufferUsages::INDEX
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);
const QUAD_USAGE: wgpu::BufferUsages = wgpu::BufferUsages::STORAGE
    .union(wgpu::BufferUsages::COPY_DST)
    .union(wgpu::BufferUsages::COPY_SRC);

/// Spare room past `count` elements: a quarter more, and at least `least` (about
/// a dozen player models' worth for vertices and indices).
pub(crate) fn room(count: u32, least: u32) -> u32 {
    count.saturating_add((count / 4).max(least))
}
/// Vertices and indices always spare.
const LEAST_VERTICES: u32 = 1 << 17;
const LEAST_INDICES: u32 = 3 << 17;

/// A buffer of `capacity` bytes starting with `contents`.
fn buffer_with(
    device: &wgpu::Device,
    label: &str,
    usage: wgpu::BufferUsages,
    contents: &[u8],
    capacity: u64,
) -> wgpu::Buffer {
    let size = capacity
        .max(contents.len() as u64)
        .max(32)
        .next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage,
        mapped_at_creation: true,
    });
    if !contents.is_empty() {
        buffer
            .slice(..contents.len() as u64)
            .get_mapped_range_mut()
            .expect("a buffer mapped at creation")
            .copy_from_slice(contents);
    }
    buffer.unmap();
    buffer
}

impl SharedGeometry {
    /// Upload the load-time scene.
    pub(crate) fn upload(device: &wgpu::Device, vertices: &[GpuVertex], indices: &[u32]) -> Self {
        let vertex_capacity = room(vertices.len() as u32, LEAST_VERTICES);
        let index_capacity = room(indices.len() as u32, LEAST_INDICES);
        let vertex_buffer = buffer_with(
            device,
            "SJK world vertices",
            VERTEX_USAGE,
            bytemuck::cast_slice(vertices),
            u64::from(vertex_capacity) * std::mem::size_of::<GpuVertex>() as u64,
        );
        let index_buffer = buffer_with(
            device,
            "SJK world indices",
            INDEX_USAGE,
            bytemuck::cast_slice(indices),
            u64::from(index_capacity) * 4,
        );
        let quad_buffer = buffer_with(
            device,
            "SJK quad lookup",
            QUAD_USAGE,
            bytemuck::cast_slice(&quads::references(vertices.len(), indices, 0)),
            u64::from(vertex_capacity) * std::mem::size_of::<quads::QuadRef>() as u64,
        );
        let environment = environment::State::default();
        let environment_buffer = environment::buffer(device, &environment.data);
        let deform_binding =
            quads::bind_with_environment(device, &vertex_buffer, &quad_buffer, &environment_buffer);
        Self {
            vertex_buffer,
            index_buffer,
            quad_buffer,
            deform_binding,
            vertex_count: vertices.len() as u32,
            index_count: indices.len() as u32,
            vertex_capacity,
            index_capacity,
            environment_buffer,
            environment,
            skinning: crate::actor_pose::gpu_skinning::Buffers::empty(device),
        }
    }

    /// Append a mesh whose `indices` are relative to its own first vertex;
    /// they are rebased onto the shared buffer here.
    pub(crate) fn append(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vertices: &[GpuVertex],
        indices: &[u32],
    ) -> Result<Placement, Box<dyn std::error::Error>> {
        let placement = Placement {
            vertex_base: self.vertex_count,
            index_base: self.index_count,
        };
        let rebased = indices
            .iter()
            .map(|index| {
                placement
                    .vertex_base
                    .checked_add(*index)
                    .ok_or("shared geometry index overflow")
            })
            .collect::<Result<Vec<u32>, _>>()?;
        let refs = quads::references(vertices.len(), indices, placement.vertex_base);
        let vertex_size = std::mem::size_of::<GpuVertex>() as u64;
        let quad_size = std::mem::size_of::<quads::QuadRef>() as u64;
        let vertex_total = self
            .vertex_count
            .checked_add(vertices.len() as u32)
            .ok_or("shared geometry vertex overflow")?;
        let index_total = self
            .index_count
            .checked_add(rebased.len() as u32)
            .ok_or("shared geometry index overflow")?;
        if vertex_total > self.vertex_capacity || index_total > self.index_capacity {
            // Out of room: reallocate with room to spare again, the old contents
            // copied on the GPU.
            self.vertex_capacity = self.vertex_capacity.max(room(vertex_total, LEAST_VERTICES));
            self.index_capacity = self.index_capacity.max(room(index_total, LEAST_INDICES));
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("SJK shared geometry growth"),
            });
            self.vertex_buffer = grow(
                device,
                &mut encoder,
                &self.vertex_buffer,
                u64::from(self.vertex_count) * vertex_size,
                u64::from(self.vertex_capacity) * vertex_size,
                "SJK world vertices",
                VERTEX_USAGE,
            );
            self.index_buffer = grow(
                device,
                &mut encoder,
                &self.index_buffer,
                u64::from(self.index_count) * 4,
                u64::from(self.index_capacity) * 4,
                "SJK world indices",
                INDEX_USAGE,
            );
            self.quad_buffer = grow(
                device,
                &mut encoder,
                &self.quad_buffer,
                u64::from(self.vertex_count) * quad_size,
                u64::from(self.vertex_capacity) * quad_size,
                "SJK quad lookup",
                QUAD_USAGE,
            );
            self.rebind(device);
            queue.submit(std::iter::once(encoder.finish()));
        }
        // Into the spare room; the queue writes after the copies above.
        let write = |buffer: &wgpu::Buffer, offset: u64, bytes: &[u8]| {
            if !bytes.is_empty() {
                queue.write_buffer(buffer, offset, bytes);
            }
        };
        write(
            &self.vertex_buffer,
            u64::from(self.vertex_count) * vertex_size,
            bytemuck::cast_slice(vertices),
        );
        write(
            &self.index_buffer,
            u64::from(self.index_count) * 4,
            bytemuck::cast_slice(&rebased),
        );
        write(
            &self.quad_buffer,
            u64::from(self.vertex_count) * quad_size,
            bytemuck::cast_slice(&refs),
        );
        self.vertex_count = vertex_total;
        self.index_count = index_total;
        Ok(placement)
    }

    pub(crate) fn update_environment(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        time: i32,
        projection: glam::Mat4,
        controls: Option<&environment::Cvars>,
    ) {
        self.environment.update(time, projection, controls);
        queue.write_buffer(
            &self.environment_buffer,
            0,
            bytemuck::bytes_of(&self.environment.data),
        );
    }

    /// Rebind shared buffers after load-time skin installation or geometry growth.
    /// Give a mesh already appended to these buffers its skinning palette, then rebind.
    pub(crate) fn append_actor_skin(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        mesh: &mut crate::ActorMesh,
    ) -> Result<bool, Box<dyn std::error::Error>> {
        let skinned = self
            .skinning
            .append_actor(device, queue, self.vertex_count, mesh)?;
        if skinned {
            self.rebind(device);
        }
        Ok(skinned)
    }

    pub(crate) fn rebind(&mut self, device: &wgpu::Device) {
        self.deform_binding = quads::bind_with_skin(
            device,
            &self.vertex_buffer,
            &self.quad_buffer,
            &self.environment_buffer,
            &self.skinning,
        );
    }
}

/// A buffer of `capacity` bytes starting with the first `used` bytes of `old`,
/// copied by `encoder`.
fn grow(
    device: &wgpu::Device,
    encoder: &mut wgpu::CommandEncoder,
    old: &wgpu::Buffer,
    used: u64,
    capacity: u64,
    label: &str,
    usage: wgpu::BufferUsages,
) -> wgpu::Buffer {
    let new = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: capacity
            .max(used)
            .max(32)
            .next_multiple_of(wgpu::COPY_BUFFER_ALIGNMENT),
        usage,
        mapped_at_creation: false,
    });
    if used != 0 {
        encoder.copy_buffer_to_buffer(old, 0, &new, 0, used);
    }
    new
}

/// Move `draws` and `ranges` from a mesh's private scene onto the shared
/// buffers at `placement`, with its materials starting at `material_base`.
pub(crate) fn relocate(
    draws: &mut [ActorDraw],
    ranges: &mut [PreviewVertexRange],
    placement: Placement,
    material_base: usize,
) {
    for draw in draws {
        draw.indices =
            draw.indices.start + placement.index_base..draw.indices.end + placement.index_base;
        draw.material += material_base;
    }
    for range in ranges {
        let base = placement.vertex_base as usize;
        range.vertices = range.vertices.start + base..range.vertices.end + base;
    }
}
