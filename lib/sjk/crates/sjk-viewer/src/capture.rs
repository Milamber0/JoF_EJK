//! PNG and JPEG screenshot encoding.
use image::ImageEncoder;
use std::error::Error;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub(crate) fn write_png(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let encoder = image::codecs::png::PngEncoder::new(BufWriter::new(file));
    encoder.write_image(rgba, size[0], size[1], image::ExtendedColorType::Rgba8)?;
    Ok(())
}

/// JPEG has no alpha channel: the encoder refuses RGBA, so the alpha is dropped first.
pub(crate) fn write_jpeg(path: &Path, size: [u32; 2], rgba: &[u8]) -> Result<(), Box<dyn Error>> {
    let rgb: Vec<u8> = rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let file = File::create(path)?;
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(BufWriter::new(file), 95);
    encoder.write_image(&rgb, size[0], size[1], image::ExtendedColorType::Rgb8)?;
    Ok(())
}

/// A smaller JPEG of `rgba` for a world note sent to the hub: fitted within
/// [`PREVIEW_SIZE`], and under [`PREVIEW_BYTES`] (the hub takes 1920 x 1080 and 400 KiB).
pub(crate) fn preview_jpeg(size: [u32; 2], rgba: &[u8]) -> Result<Vec<u8>, Box<dyn Error>> {
    let image = image::RgbaImage::from_raw(size[0], size[1], rgba.to_vec())
        .ok_or("the screenshot's pixels do not fill its size")?;
    let scale = (PREVIEW_SIZE[0] as f32 / size[0] as f32)
        .min(PREVIEW_SIZE[1] as f32 / size[1] as f32)
        .min(1.0);
    let width = ((size[0] as f32 * scale).round() as u32).max(1);
    let height = ((size[1] as f32 * scale).round() as u32).max(1);
    let image = if scale < 1.0 {
        image::imageops::resize(&image, width, height, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let rgb: Vec<u8> = image
        .pixels()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    for quality in [82, 65, 45] {
        let mut bytes = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, quality).write_image(
            &rgb,
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )?;
        if bytes.len() <= PREVIEW_BYTES {
            return Ok(bytes);
        }
    }
    Err("the picture stays too large".into())
}

/// The largest picture a world note sends, and its largest size in bytes.
const PREVIEW_SIZE: [u32; 2] = [1280, 720];
const PREVIEW_BYTES: usize = 380 * 1024;

#[cfg(test)]
mod tests {
    #[test]
    fn note_pictures_fit_the_hubs_bounds() {
        let size = [2560, 1600];
        let rgba: Vec<u8> = (0..size[0] * size[1])
            .flat_map(|i| {
                [
                    (i % 251) as u8,
                    (i % 13) as u8 * 19,
                    (i / 7 % 256) as u8,
                    255,
                ]
            })
            .collect();
        let bytes = super::preview_jpeg(size, &rgba).expect("encodes");
        assert!(bytes.len() <= super::PREVIEW_BYTES);
        let image = image::load_from_memory(&bytes).expect("decodes");
        assert_eq!((image.width(), image.height()), (1152, 720));
        let small = super::preview_jpeg([4, 4], &[200; 64]).expect("encodes");
        let image = image::load_from_memory(&small).expect("decodes");
        assert_eq!((image.width(), image.height()), (4, 4));
    }

    #[test]
    fn jpeg_screenshots_drop_the_alpha() {
        let path = std::env::temp_dir().join(format!("sjk-capture-{}.jpg", std::process::id()));
        let rgba: Vec<u8> = (0..4 * 4)
            .flat_map(|i| [i as u8 * 16, 128, 255, 255])
            .collect();
        super::write_jpeg(&path, [4, 4], &rgba).expect("writes a jpeg");
        let image = image::open(&path).expect("decodes");
        assert_eq!((image.width(), image.height()), (4, 4));
        let _ = std::fs::remove_file(&path);
    }
}
