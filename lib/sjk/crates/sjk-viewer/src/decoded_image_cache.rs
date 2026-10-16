//! Process-lifetime decoded image cache shared by map and shell textures.

use image::RgbaImage;
use md4::{Digest, Md4};
use sjk_vfs::{AssetCacheIdentity, VirtualFileSystem, VirtualPath};
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex, OnceLock};

type ImageKey = (String, [u8; 16]);

static DECODED_IMAGES: OnceLock<Mutex<HashMap<ImageKey, Arc<RgbaImage>>>> = OnceLock::new();
type ResolvedKey = (AssetCacheIdentity, String);
static RESOLVED_IMAGES: OnceLock<Mutex<HashMap<ResolvedKey, Arc<RgbaImage>>>> = OnceLock::new();

fn resolved_images() -> &'static Mutex<HashMap<ResolvedKey, Arc<RgbaImage>>> {
    RESOLVED_IMAGES.get_or_init(|| Mutex::new(HashMap::with_capacity(1_024)))
}

fn shared_images() -> &'static Mutex<HashMap<ImageKey, Arc<RgbaImage>>> {
    DECODED_IMAGES.get_or_init(|| Mutex::new(HashMap::with_capacity(1_024)))
}

/// Decode one image through the cache used by all viewer material paths.
pub(crate) fn cached_decoded_image(
    vfs: &VirtualFileSystem,
    path: &str,
) -> Result<Option<Arc<RgbaImage>>, Box<dyn Error>> {
    let normalized = VirtualPath::new(path)?.as_str().to_owned();
    let resolved = vfs
        .asset_cache_identity(&normalized)?
        .map(|id| (id, normalized.clone()));

    if let Some(key) = &resolved {
        if let Some(image) = resolved_images()
            .lock()
            .expect("resolved image cache lock")
            .get(key)
            .cloned()
        {
            return Ok(Some(image));
        }
    }
    let image = read_decoded_image(vfs, &normalized)?;
    if let (Some(key), Some(image)) = (resolved, &image) {
        resolved_images()
            .lock()
            .expect("resolved image cache lock")
            .insert(key, Arc::clone(image));
    }
    Ok(image)
}

fn read_decoded_image(
    vfs: &VirtualFileSystem,
    path: &str,
) -> Result<Option<Arc<RgbaImage>>, Box<dyn Error>> {
    let Some(asset) = vfs.read(path)? else {
        return Ok(None);
    };

    // A virtual path alone aliases different server packs. Hash only while loading assets;
    // decoded images and GPU resources are still retained normally during rendering.
    let key = (path.to_ascii_lowercase(), Md4::digest(&asset.bytes).into());
    if let Some(image) = shared_images()
        .lock()
        .expect("decoded image cache lock")
        .get(&key)
        .cloned()
    {
        return Ok(Some(image));
    }
    let Ok(decoded) = crate::decode_image(&asset.bytes, path) else {
        return Ok(None);
    };
    let decoded = Arc::new(decoded.into_rgba8());
    let mut cache = shared_images().lock().expect("decoded image cache lock");
    Ok(Some(
        cache
            .entry(key)
            .or_insert_with(|| Arc::clone(&decoded))
            .clone(),
    ))
}
