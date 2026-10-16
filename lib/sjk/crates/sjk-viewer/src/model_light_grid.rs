//! Map-lifetime GPU light grid. Legacy decoding stays in this viewer adapter.
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

/// Storage header, followed by 12 words per sample and the u32 indirection array.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Header {
    origin: [f32; 4],
    inverse: [f32; 4],
    bounds: [u32; 4],
    offsets: [u32; 4],
}

/// Build once per map; invalid/missing layouts use the existing entity-light fallback.
pub(super) fn bytes(bsp: &sjk_bsp::Bsp) -> Vec<u8> {
    let Some(layout) = crate::entity_lighting::EntityLighting::from_world(bsp).layout() else {
        return vec![0; 80];
    };
    let render = bsp.render();
    let samples = render.light_grid();
    let indices = render.light_grid_array();
    let mut header = Header::zeroed();
    header.origin[..3].copy_from_slice(&layout.origin());
    header.inverse[..3].copy_from_slice(&layout.inverse_cell_size());
    header.bounds[..3].copy_from_slice(&layout.bounds().map(|v| v as u32));
    header.bounds[3] = indices.len() as u32;
    header.offsets = [samples.len() as u32, (samples.len() * 12) as u32, 0, 0];
    let mut output = Vec::with_capacity(64 + samples.len() * 48 + indices.len() * 4);
    output.extend_from_slice(bytemuck::bytes_of(&header));
    for sample in samples {
        let mut data = [[0.0_f32; 4]; 3];
        // tr_light.cpp:207-232: styles terminate at LS_LSNONE, not at style zero.
        data[0][3] = f32::from(u8::from(sample.styles[0] != 255));
        for slot in 0..4 {
            if sample.styles[slot] == 255 {
                break;
            }
            for channel in 0..3 {
                data[0][channel] += f32::from(sample.ambient[slot][channel]);
                data[1][channel] += f32::from(sample.directed[slot][channel]);
            }
        }
        // Same decode as sjk-bsp. Light styles remain white, as in the CPU adapter.
        let turn = std::f32::consts::TAU / 256.;
        let longitude = f32::from(sample.latitude_longitude[0]) * turn;
        let latitude = f32::from(sample.latitude_longitude[1]) * turn;
        data[2] = [
            latitude.cos() * longitude.sin(),
            latitude.sin() * longitude.sin(),
            longitude.cos(),
            0.,
        ];
        output.extend_from_slice(bytemuck::cast_slice(&data));
    }
    for index in indices {
        output.extend_from_slice(&u32::from(*index).to_ne_bytes());
    }
    output
}

/// Upload immutable grid storage. Reject excessive binding sizes without a half-built binding.
pub(super) fn upload(device: &wgpu::Device, bsp: &sjk_bsp::Bsp) -> wgpu::Buffer {
    let mut data = bytes(bsp);
    if data.len() == 80 {
        eprintln!("model light grid: missing or invalid layout; entity fallback");
    }
    if data.len() as u64 > u64::from(device.limits().max_storage_buffer_binding_size) {
        eprintln!(
            "model light grid: {} bytes exceeds adapter binding limit; entity fallback",
            data.len()
        );
        data = vec![0; 80];
    }
    eprintln!("model light grid: {} immutable GPU bytes", data.len());
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK spatial model light grid"),
        contents: &data,
        usage: wgpu::BufferUsages::STORAGE,
    })
}

/// A valid empty binding for material factories before their map grid is installed.
pub(super) fn empty(device: &wgpu::Device) -> wgpu::Buffer {
    device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("SJK absent model light grid"),
        contents: &[0; 80],
        usage: wgpu::BufferUsages::STORAGE,
    })
}
