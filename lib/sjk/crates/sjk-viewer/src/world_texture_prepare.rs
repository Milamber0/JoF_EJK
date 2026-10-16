//! Bounded, load-only mip preparation. Uploads keep the material forge's texture keys.
use super::{PendingMaterial, forge::Forge};
use image::RgbaImage;
use std::{collections::HashSet, error::Error, sync::Arc, time::Instant};

use super::filtering::mips;

// Bound prepared chains per batch, rather than retaining every map texture twice.
// A single larger animated texture is handled alone, as in the serial path.
const BATCH_BYTES: u64 = 64 << 20;
const MAX_WORKERS: usize = 4;

struct Job<'a> {
    key: &'a str,
    images: &'a [Arc<RgbaImage>],
    width: u32,
    height: u32,
    bytes: u64,
}

impl<'a> Job<'a> {
    fn new(key: &'a str, images: &'a [Arc<RgbaImage>]) -> Self {
        let (width, height) = mips::extent(images);
        let bytes = (0..mips::levels(width, height))
            .map(|level| {
                u64::from((width >> level).max(1)) * u64::from((height >> level).max(1)) * 4
            })
            .sum::<u64>()
            .saturating_mul(images.len() as u64);
        Self {
            key,
            images,
            width,
            height,
            bytes,
        }
    }
}

/// Prepare each unique texture once before stage assembly, with unchanged mip pixels.
/// Workers only resize pixels; GPU creation and forge mutation stay on the installer.
pub(super) fn upload(
    device: &wgpu::Device,
    queue: &crate::frame_queue::FrameQueue,
    forge: &mut Forge,
    materials: &[PendingMaterial],
) -> Result<(), Box<dyn Error>> {
    if !forge.filtering.mipmapped() {
        return Ok(());
    }
    let started = Instant::now();
    let mut seen = HashSet::new();
    let mut jobs = Vec::new();
    for stage in materials.iter().flat_map(|material| &material.stages) {
        for (key, images) in
            std::iter::once((stage.primary_key.as_str(), stage.primary_pixels.as_slice())).chain(
                stage
                    .secondary_key
                    .as_deref()
                    .zip(stage.secondary_pixels.as_deref()),
            )
        {
            if !forge.texture_cache.contains_key(key)
                && !key.starts_with(super::videos::KEY_PREFIX)
                && seen.insert(key)
            {
                jobs.push(Job::new(key, images));
            }
        }
    }
    let workers = std::thread::available_parallelism()
        .map_or(1, usize::from)
        .saturating_sub(1)
        .clamp(1, MAX_WORKERS);
    let mut remaining = jobs.as_slice();
    while !remaining.is_empty() {
        let mut bytes = 0_u64;
        let mut count = 0;
        for job in remaining {
            if count > 0 && bytes.saturating_add(job.bytes) > BATCH_BYTES {
                break;
            }
            bytes = bytes.saturating_add(job.bytes);
            count += 1;
        }
        let (batch, tail) = remaining.split_at(count);
        let next = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| -> Result<(), Box<dyn Error>> {
            let (sender, receiver) = std::sync::mpsc::sync_channel(workers);
            for _ in 0..workers.min(batch.len()) {
                let sender = sender.clone();
                let next = &next;
                scope.spawn(move || {
                    while let Some(job) =
                        batch.get(next.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
                    {
                        let chains = mips::cache::prepare(job.images, job.width, job.height);
                        if sender.send((job, chains)).is_err() {
                            break;
                        }
                    }
                });
            }
            drop(sender);
            for (job, chains) in receiver {
                let texture = mips::upload_chains(device, queue, job.width, job.height, &chains)?;
                forge.texture_cache.insert(job.key.to_owned(), texture);
            }
            Ok(())
        })?;
        remaining = tail;
    }
    crate::log::progress(format_args!(
        "texture preparation: {} arrays, {workers} workers, {:.1} ms",
        jobs.len(),
        started.elapsed().as_secs_f64() * 1_000.0
    ));
    Ok(())
}
