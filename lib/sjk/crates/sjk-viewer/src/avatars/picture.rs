//! The pictures themselves: a player's own image made into what the hub takes (cropped
//! to a square from its middle, scaled to [`SIZE`] and written as a PNG), a picture the
//! hub serves read back into pixels, and the round cut the UI draws them with. Pure
//! functions on bytes, run on worker threads, never on the frame thread.
//!
//! A player's file is checked before it is sent, whatever its name says: it must hold
//! bytes (not be empty), be a PNG, JPEG or TGA by its content (a TGA, which has no
//! signature, by its extension), decode whole within the size and memory limits, be
//! [`EDGE_MIN`] to 8192 pixels a side and at most [`ASPECT_MAX`] times as long as it is
//! wide, and show something once cropped (not every pixel clear). Each refusal has its
//! reason in words ([`PictureError`]), which the picture panel shows.

use image::{ImageEncoder, RgbaImage};
use sjk_identity::avatar::SIZE;

/// Largest file read as a picture, in bytes.
pub(crate) const FILE_MAX: u64 = 16 * 1024 * 1024;
/// Largest width or height a picture may have.
const EDGE_MAX: u32 = 8_192;
/// The most a picture the hub serves may be a side: the hub re-encodes every upload at
/// [`SIZE`], so a larger one is not its own, and a small file could otherwise decode
/// to hundreds of megabytes (an 8,192 pixel square PNG is under 256 KB).
const SERVED_EDGE_MAX: u32 = 256;
/// The most a served picture's decoding may allocate.
const SERVED_DECODE_LIMIT: u64 = 4 * 1024 * 1024;
/// Smallest width or height a picture may have: the hub's own least, as a smaller one
/// scaled up to [`SIZE`] is a blur.
pub(crate) const EDGE_MIN: u32 = 64;
/// How many times longer than wide (or wide than long) a picture may be: past this its
/// middle square loses most of it.
pub(crate) const ASPECT_MAX: u32 = 4;
/// The most opaque a pixel may be and still count as clear, for a picture with nothing
/// to see.
const CLEAR_ALPHA: u8 = 8;
/// Most memory a decoder may take for one picture.
const DECODE_LIMIT: u64 = 256 * 1024 * 1024;

/// Why a file cannot be the player's picture, in words the player reads.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum PictureError {
    /// The file could not be read.
    Unreadable(String),
    /// The file holds nothing.
    Empty,
    /// Over [`FILE_MAX`] bytes or [`EDGE_MAX`] pixels a side.
    TooLarge,
    /// Under [`EDGE_MIN`] pixels a side.
    TooSmall,
    /// More than [`ASPECT_MAX`] times as long as it is wide.
    TooNarrow,
    /// Every pixel of its middle square is clear.
    Invisible,
    /// A PNG, JPEG or TGA that stops short or is damaged.
    Damaged,
    /// Not a picture SJK reads.
    NotAPicture,
}

impl std::fmt::Display for PictureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unreadable(why) => write!(f, "Cannot read that file: {why}"),
            Self::Empty => write!(f, "That file is empty: it holds no picture"),
            Self::TooLarge => write!(
                f,
                "That picture is too big: at most 16 MB and 8192 pixels a side"
            ),
            Self::TooSmall => write!(
                f,
                "That picture is too small: at least {EDGE_MIN} pixels a side"
            ),
            Self::TooNarrow => write!(
                f,
                "That picture is too narrow or too wide: its long side may be at most {ASPECT_MAX} times its short side"
            ),
            Self::Invisible => write!(
                f,
                "That picture shows nothing: its middle is fully transparent"
            ),
            Self::Damaged => write!(
                f,
                "That picture file is damaged or cut short: save it again and retry"
            ),
            Self::NotAPicture => write!(
                f,
                "That file is not a picture SJK can read: use a PNG, JPEG or TGA"
            ),
        }
    }
}

/// A player's picture made ready to send: the PNG the hub takes and its pixels for the
/// preview.
#[derive(Clone, Debug)]
pub(crate) struct Prepared {
    /// [`SIZE`] square PNG, RGB when every pixel is opaque, else RGBA.
    pub(crate) png: Vec<u8>,
    /// Its [`SIZE`] square RGBA pixels, not yet cut round.
    pub(crate) rgba: Vec<u8>,
}

/// Whether `path` names a picture file by its extension (PNG, JPEG or TGA): a file
/// dropped on the window anywhere is taken as the player's new picture then.
pub(crate) fn looks_like_picture(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["png", "jpg", "jpeg", "tga"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

/// Read the file at `path` and make it a picture ([`prepare`]).
pub(crate) fn prepare_file(path: &std::path::Path) -> Result<Prepared, PictureError> {
    let metadata = std::fs::metadata(path)
        .map_err(|error| PictureError::Unreadable(error.kind().to_string()))?;
    if !metadata.is_file() {
        return Err(PictureError::NotAPicture);
    }
    let size = metadata.len();
    if size == 0 {
        return Err(PictureError::Empty);
    }
    if size > FILE_MAX {
        return Err(PictureError::TooLarge);
    }
    let bytes =
        std::fs::read(path).map_err(|error| PictureError::Unreadable(error.kind().to_string()))?;
    // A TGA has no signature to know it by: its extension says it.
    let tga = image::ImageFormat::from_path(path).ok() == Some(image::ImageFormat::Tga);
    prepare_as(&bytes, tga.then_some(image::ImageFormat::Tga))
}

/// Make `bytes` (a PNG, JPEG or TGA) a picture the hub takes: cropped to a square from
/// its middle, scaled to [`SIZE`] by area averaging and written as a PNG holding the
/// pixels only. (The game reads files, [`prepare_file`]; this is for tests.)
#[cfg(test)]
pub(crate) fn prepare(bytes: &[u8]) -> Result<Prepared, PictureError> {
    prepare_as(bytes, None)
}

/// [`prepare`], taking `bytes` as `fallback` when their format cannot be told from them.
fn prepare_as(
    bytes: &[u8],
    fallback: Option<image::ImageFormat>,
) -> Result<Prepared, PictureError> {
    if bytes.is_empty() {
        return Err(PictureError::Empty);
    }
    if bytes.len() as u64 > FILE_MAX {
        return Err(PictureError::TooLarge);
    }
    let image = decode(bytes, None, fallback, EDGE_MAX, DECODE_LIMIT)?;
    // The JPEG decoder fills in a file cut short: one without its end is refused.
    if image::guess_format(bytes).ok() == Some(image::ImageFormat::Jpeg) && !jpeg_ends(bytes) {
        return Err(PictureError::Damaged);
    }
    let (width, height) = image.dimensions();
    if width.min(height) < EDGE_MIN {
        return Err(PictureError::TooSmall);
    }
    if width.max(height) > width.min(height).saturating_mul(ASPECT_MAX) {
        return Err(PictureError::TooNarrow);
    }
    let edge = width.min(height);
    let square =
        image::imageops::crop_imm(&image, (width - edge) / 2, (height - edge) / 2, edge, edge)
            .to_image();
    let scaled = if edge == SIZE {
        square
    } else {
        image::imageops::thumbnail(&square, SIZE, SIZE)
    };
    if scaled.pixels().all(|pixel| pixel.0[3] <= CLEAR_ALPHA) {
        return Err(PictureError::Invisible);
    }
    let png = encode(&scaled).map_err(|error| PictureError::Unreadable(error.to_string()))?;
    Ok(Prepared {
        png,
        rgba: scaled.into_raw(),
    })
}

/// A picture the hub served, as [`SIZE`] square RGBA pixels; `None` when it is not a
/// square PNG.
pub(crate) fn decode_served(png: &[u8]) -> Option<Vec<u8>> {
    let image = decode(
        png,
        Some(image::ImageFormat::Png),
        None,
        SERVED_EDGE_MAX,
        SERVED_DECODE_LIMIT,
    )
    .ok()?;
    let (width, height) = image.dimensions();
    if width != height || width == 0 {
        return None;
    }
    if width == SIZE {
        return Some(image.into_raw());
    }
    Some(image::imageops::thumbnail(&image, SIZE, SIZE).into_raw())
}

/// Decode `bytes` (of `format`, or guessed from the bytes, else `fallback`) under the
/// size limits.
fn decode(
    bytes: &[u8],
    format: Option<image::ImageFormat>,
    fallback: Option<image::ImageFormat>,
    edge_max: u32,
    alloc_max: u64,
) -> Result<RgbaImage, PictureError> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes));
    match format {
        Some(format) => reader.set_format(format),
        None => {
            reader = reader
                .with_guessed_format()
                .map_err(|error| PictureError::Unreadable(error.to_string()))?;
            if reader.format().is_none()
                && let Some(fallback) = fallback
            {
                reader.set_format(fallback);
            }
        }
    }
    let format = reader.format().ok_or(PictureError::NotAPicture)?;
    if !matches!(
        format,
        image::ImageFormat::Png | image::ImageFormat::Jpeg | image::ImageFormat::Tga
    ) {
        return Err(PictureError::NotAPicture);
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(edge_max);
    limits.max_image_height = Some(edge_max);
    limits.max_alloc = Some(alloc_max);
    reader.limits(limits);
    reader
        .decode()
        .map(image::DynamicImage::into_rgba8)
        .map_err(|error| match error {
            image::ImageError::Limits(_) => PictureError::TooLarge,
            image::ImageError::Unsupported(_) => PictureError::NotAPicture,
            // A TGA has no signature: bytes that are none of these fail here.
            _ if format == image::ImageFormat::Tga => PictureError::NotAPicture,
            // A PNG or JPEG that stops short, or whose data is damaged.
            image::ImageError::Decoding(_) | image::ImageError::IoError(_) => PictureError::Damaged,
            other => PictureError::Unreadable(other.to_string()),
        })
}

/// Whether the JPEG `bytes` reach their end: past the segments before the image's
/// first scan (where an EXIF thumbnail, with an end of its own, may hide) an end of
/// image marker follows. A file cut short has none.
fn jpeg_ends(bytes: &[u8]) -> bool {
    const SOS: u8 = 0xDA;
    const EOI: [u8; 2] = [0xFF, 0xD9];
    let mut at = 2;
    loop {
        // Markers may be padded with fill bytes.
        while bytes.get(at) == Some(&0xFF) && bytes.get(at + 1) == Some(&0xFF) {
            at += 1;
        }
        let (Some(&0xFF), Some(&marker)) = (bytes.get(at), bytes.get(at + 1)) else {
            return false;
        };
        if marker == SOS {
            break;
        }
        let Some(length) = bytes.get(at + 2..at + 4) else {
            return false;
        };
        at += 2 + usize::from(u16::from_be_bytes([length[0], length[1]]));
    }
    bytes[at..].windows(2).any(|pair| pair == EOI)
}

/// `image` as a PNG: RGB when every pixel is opaque, else RGBA.
fn encode(image: &RgbaImage) -> image::ImageResult<Vec<u8>> {
    let mut out = Vec::new();
    let encoder = image::codecs::png::PngEncoder::new_with_quality(
        &mut out,
        image::codecs::png::CompressionType::Best,
        image::codecs::png::FilterType::Adaptive,
    );
    let (width, height) = image.dimensions();
    if image.pixels().all(|pixel| pixel.0[3] == 255) {
        let rgb: Vec<u8> = image
            .pixels()
            .flat_map(|pixel| [pixel.0[0], pixel.0[1], pixel.0[2]])
            .collect();
        encoder.write_image(&rgb, width, height, image::ExtendedColorType::Rgb8)?;
    } else {
        encoder.write_image(
            image.as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgba8,
        )?;
    }
    Ok(out)
}

/// Cut `rgba` (`edge` square) round: pixels outside the inscribed circle become clear,
/// those on its edge as clear as they are outside it, so the disc's rim is smooth.
pub(crate) fn round(rgba: &mut [u8], edge: u32) {
    let centre = edge as f32 * 0.5;
    let radius = centre;
    for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
        let x = (index as u32 % edge) as f32 + 0.5 - centre;
        let y = (index as u32 / edge) as f32 + 0.5 - centre;
        let distance = (x * x + y * y).sqrt();
        let cover = (radius - distance + 0.5).clamp(0.0, 1.0);
        pixel[3] = (f32::from(pixel[3]) * cover).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `width` x `height` pixels from `pixel`, encoded as `format`.
    fn picture(
        width: u32,
        height: u32,
        format: image::ImageFormat,
        pixel: impl Fn(u32, u32) -> [u8; 4],
    ) -> Vec<u8> {
        let image = RgbaImage::from_fn(width, height, |x, y| image::Rgba(pixel(x, y)));
        let mut out = std::io::Cursor::new(Vec::new());
        match format {
            image::ImageFormat::Jpeg => image::DynamicImage::ImageRgba8(image)
                .into_rgb8()
                .write_to(&mut out, format)
                .unwrap(),
            _ => image.write_to(&mut out, format).unwrap(),
        }
        out.into_inner()
    }

    fn pixels_of_png(png: &[u8]) -> RgbaImage {
        image::load_from_memory_with_format(png, image::ImageFormat::Png)
            .unwrap()
            .into_rgba8()
    }

    /// A picture the hub serves is SIZE square, or a little over; a large one is
    /// refused before it is decoded.
    #[test]
    fn a_served_picture_over_the_size_limit_is_not_decoded() {
        let flat = |side| picture(side, side, image::ImageFormat::Png, |_, _| [9, 9, 9, 255]);
        let size = sjk_identity::avatar::SIZE;
        assert_eq!(
            decode_served(&flat(size)).map(|rgba| rgba.len()),
            Some((size * size * 4) as usize)
        );
        assert!(decode_served(&flat(2 * size)).is_some());
        assert!(decode_served(&flat(SERVED_EDGE_MAX + 1)).is_none());
        assert!(decode_served(&flat(2_048)).is_none());
    }

    #[test]
    fn a_wide_picture_is_cropped_from_its_middle_and_scaled() {
        // Red at the left and right quarters, green in the middle half.
        let wide = picture(800, 400, image::ImageFormat::Png, |x, _| {
            if (200..600).contains(&x) {
                [0, 255, 0, 255]
            } else {
                [255, 0, 0, 255]
            }
        });
        let prepared = prepare(&wide).unwrap();
        let png = pixels_of_png(&prepared.png);
        assert_eq!(png.dimensions(), (SIZE, SIZE));
        assert!(png.pixels().all(|pixel| pixel.0 == [0, 255, 0, 255]));
        assert_eq!(prepared.rgba.len(), (SIZE * SIZE * 4) as usize);
        // Opaque pictures are sent as RGB: colour type 2 in the header.
        assert_eq!(prepared.png[25], 2);
        assert!(prepared.png.len() <= sjk_identity::avatar::UPLOAD_MAX);
    }

    #[test]
    fn a_tall_jpeg_and_a_small_tga_become_squares_of_the_hubs_size() {
        let tall = picture(300, 900, image::ImageFormat::Jpeg, |_, y| {
            if (300..600).contains(&y) {
                [20, 40, 220, 255]
            } else {
                [250, 250, 250, 255]
            }
        });
        let png = pixels_of_png(&prepare(&tall).unwrap().png);
        let centre = png.get_pixel(SIZE / 2, SIZE / 2).0;
        assert!(centre[2] > 180 && centre[0] < 80, "{centre:?}");
        // A TGA is known by its file's extension.
        let small = picture(80, 80, image::ImageFormat::Tga, |_, _| [9, 99, 199, 255]);
        assert_eq!(prepare(&small).err(), Some(PictureError::NotAPicture));
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("Me.TGA");
        std::fs::write(&file, &small).unwrap();
        let png = pixels_of_png(&prepare_file(&file).unwrap().png);
        assert_eq!(png.dimensions(), (SIZE, SIZE));
        assert_eq!(png.get_pixel(5, 5).0, [9, 99, 199, 255]);
    }

    #[test]
    fn transparency_is_kept_and_sent_as_rgba() {
        let clear = picture(256, 256, image::ImageFormat::Png, |x, _| {
            if x < 128 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let prepared = prepare(&clear).unwrap();
        assert_eq!(prepared.png[25], 6, "RGBA");
        let png = pixels_of_png(&prepared.png);
        assert_eq!(png.get_pixel(SIZE - 1, 0).0[3], 0);
        assert_eq!(png.get_pixel(0, 0).0, [255, 0, 0, 255]);
    }

    #[test]
    fn other_files_and_sizes_are_refused_with_a_reason() {
        assert_eq!(
            prepare(b"just some text, not a picture").err(),
            Some(PictureError::NotAPicture)
        );
        let gif = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
        assert_eq!(prepare(gif).err(), Some(PictureError::NotAPicture));
        let tiny = picture(16, 64, image::ImageFormat::Png, |_, _| [1, 2, 3, 255]);
        assert_eq!(prepare(&tiny).err(), Some(PictureError::TooSmall));
        let mut cut = picture(64, 64, image::ImageFormat::Png, |x, y| {
            [x as u8, y as u8, 0, 255]
        });
        cut.truncate(cut.len() / 2);
        assert_eq!(prepare(&cut).err(), Some(PictureError::Damaged));
        // A header claiming a huge picture is refused before its pixels are allocated.
        let huge = picture(64, 64, image::ImageFormat::Png, |_, _| [0; 4]);
        let mut header = huge.clone();
        header[16..20].copy_from_slice(&20_000_u32.to_be_bytes());
        header[20..24].copy_from_slice(&20_000_u32.to_be_bytes());
        assert!(prepare(&header).is_err());
        let file = std::env::temp_dir().join(format!("sjk-avatar-big-{}.png", std::process::id()));
        std::fs::File::create(&file)
            .unwrap()
            .set_len(FILE_MAX + 1)
            .unwrap();
        assert_eq!(prepare_file(&file).err(), Some(PictureError::TooLarge));
        std::fs::remove_file(&file).ok();
        assert!(matches!(
            prepare_file(std::path::Path::new("/nonexistent/sjk/avatar.png")),
            Err(PictureError::Unreadable(_))
        ));
    }

    /// The hub's least: 64 pixels a side; 63 is refused, by its shorter side too.
    #[test]
    fn a_picture_under_64_pixels_a_side_is_too_small() {
        let flat = |width, height| {
            picture(width, height, image::ImageFormat::Png, |_, _| {
                [40, 80, 120, 255]
            })
        };
        assert_eq!(EDGE_MIN, 64);
        assert!(prepare(&flat(64, 64)).is_ok());
        assert_eq!(prepare(&flat(63, 63)).err(), Some(PictureError::TooSmall));
        assert_eq!(prepare(&flat(200, 63)).err(), Some(PictureError::TooSmall));
        assert!(
            PictureError::TooSmall.to_string().contains("64 pixels"),
            "the reason names the size"
        );
    }

    /// An empty file, or no bytes at all, is refused before anything is decoded.
    #[test]
    fn an_empty_file_is_refused() {
        assert_eq!(prepare(&[]).err(), Some(PictureError::Empty));
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("me.png");
        std::fs::write(&file, []).unwrap();
        assert_eq!(prepare_file(&file).err(), Some(PictureError::Empty));
        // A folder named like a picture is no picture either.
        let folder = dir.path().join("folder.png");
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(prepare_file(&folder).err(), Some(PictureError::NotAPicture));
    }

    /// A picture more than four times as long as it is wide, either way, is refused;
    /// four times exactly is taken.
    #[test]
    fn a_picture_too_narrow_or_too_wide_is_refused() {
        let flat = |width, height| {
            picture(width, height, image::ImageFormat::Png, |_, _| {
                [200, 100, 50, 255]
            })
        };
        assert!(prepare(&flat(256, 64)).is_ok());
        assert!(prepare(&flat(64, 256)).is_ok());
        assert_eq!(prepare(&flat(257, 64)).err(), Some(PictureError::TooNarrow));
        assert_eq!(
            prepare(&flat(64, 1_000)).err(),
            Some(PictureError::TooNarrow)
        );
        assert!(PictureError::TooNarrow.to_string().contains("4 times"));
    }

    /// A fully transparent picture, or one clear wherever its middle square is, shows
    /// nothing and is refused; a single visible pixel is enough.
    #[test]
    fn a_picture_with_nothing_to_see_is_refused() {
        let clear = picture(128, 128, image::ImageFormat::Png, |_, _| [255, 0, 0, 0]);
        assert_eq!(prepare(&clear).err(), Some(PictureError::Invisible));
        let faint = picture(128, 128, image::ImageFormat::Png, |_, _| [255, 255, 255, 3]);
        assert_eq!(prepare(&faint).err(), Some(PictureError::Invisible));
        // Visible only at the sides, which the middle square leaves out.
        let sides = picture(256, 64, image::ImageFormat::Png, |x, _| {
            if (96..160).contains(&x) {
                [0, 0, 0, 0]
            } else {
                [0, 200, 0, 255]
            }
        });
        assert_eq!(prepare(&sides).err(), Some(PictureError::Invisible));
        let dot = picture(128, 128, image::ImageFormat::Png, |x, y| {
            if (60..68).contains(&x) && (60..68).contains(&y) {
                [0, 0, 255, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        assert!(prepare(&dot).is_ok());
    }

    /// Whatever its name says, a file whose bytes are not a PNG, JPEG or TGA is refused
    /// as not a picture: a program renamed .png or .jpg, a GIF, a WebP, a BMP. A file
    /// named .tga is read as a TGA (it has no signature) and refused the same way when
    /// it is not one.
    #[test]
    fn a_file_that_is_no_png_jpeg_or_tga_is_refused_whatever_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut program = b"MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff".to_vec();
        program.extend_from_slice(&[0x40; 256]);
        program.extend_from_slice(b"This program cannot be run in DOS mode.");
        let gif = b"GIF89a\x40\x00\x40\x00\x80\x00\x00\xff\xff\xff\x00\x00\x00!\xf9\x04\x00\x00\x00\x00\x00,\x00\x00\x00\x00\x40\x00\x40\x00\x00\x02\x02D\x01\x00;".to_vec();
        let mut webp = b"RIFF\x24\x00\x00\x00WEBPVP8 ".to_vec();
        webp.extend_from_slice(&[0; 24]);
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0; 64]);
        for (bytes, kind) in [
            (&program, "program"),
            (&gif, "gif"),
            (&webp, "webp"),
            (&bmp, "bmp"),
        ] {
            for name in ["me.png", "me.jpg", "me.jpeg", "me.tga", "me.exe", "me"] {
                let file = dir.path().join(name);
                std::fs::write(&file, bytes).unwrap();
                assert_eq!(
                    prepare_file(&file).err(),
                    Some(PictureError::NotAPicture),
                    "{kind} named {name}"
                );
            }
        }
    }

    /// A PNG or JPEG cut short at any point, or with damaged data, is refused with a
    /// reason and never panics.
    #[test]
    fn a_damaged_or_cut_png_or_jpeg_is_refused_cleanly() {
        let png = picture(128, 96, image::ImageFormat::Png, |x, y| {
            [x as u8, y as u8, (x ^ y) as u8, 255]
        });
        let jpeg = picture(128, 96, image::ImageFormat::Jpeg, |x, y| {
            [x as u8, y as u8, (x ^ y) as u8, 255]
        });
        for (whole, kind) in [(&png, "png"), (&jpeg, "jpeg")] {
            assert!(prepare(whole).is_ok(), "{kind} whole");
            for keep in [
                8,
                16,
                33,
                64,
                whole.len() / 3,
                whole.len() / 2,
                whole.len() - 12,
            ] {
                let cut = &whole[..keep];
                let refused = std::panic::catch_unwind(|| prepare(cut))
                    .unwrap_or_else(|_| panic!("{kind} cut at {keep} panicked"));
                assert!(refused.is_err(), "{kind} cut at {keep} was taken");
            }
            // Damaged in the middle: flipped bytes in its data.
            let mut damaged = whole.clone();
            let middle = damaged.len() / 2;
            for byte in &mut damaged[middle..middle + 32] {
                *byte ^= 0x5a;
            }
            let refused = std::panic::catch_unwind(|| prepare(&damaged))
                .unwrap_or_else(|_| panic!("damaged {kind} panicked"));
            // A JPEG's damaged scan may still decode to a picture; a PNG's checksums
            // catch it.
            if kind == "png" {
                assert_eq!(refused.err(), Some(PictureError::Damaged));
            }
        }
        // A PNG cut inside its pixels says it is damaged.
        let cut = &png[..png.len() * 2 / 3];
        assert_eq!(prepare(cut).err(), Some(PictureError::Damaged));
        let cut = &jpeg[..jpeg.len() * 2 / 3];
        assert!(prepare(cut).is_err());
    }

    #[test]
    fn pictures_are_known_by_their_extension() {
        assert!(looks_like_picture(std::path::Path::new("me.PNG")));
        assert!(looks_like_picture(std::path::Path::new("dir/me.jpeg")));
        assert!(looks_like_picture(std::path::Path::new("me.tga")));
        assert!(!looks_like_picture(std::path::Path::new("jampconfig.cfg")));
        assert!(!looks_like_picture(std::path::Path::new("png")));
    }

    #[test]
    fn served_pictures_are_read_back_at_the_hubs_size() {
        let served = picture(SIZE, SIZE, image::ImageFormat::Png, |_, _| [5, 6, 7, 255]);
        let rgba = decode_served(&served).unwrap();
        assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(&rgba[..4], &[5, 6, 7, 255]);
        let larger = picture(256, 256, image::ImageFormat::Png, |_, _| [5, 6, 7, 255]);
        assert_eq!(
            decode_served(&larger).unwrap().len(),
            (SIZE * SIZE * 4) as usize
        );
        let wide = picture(256, 128, image::ImageFormat::Png, |_, _| [0; 4]);
        assert_eq!(decode_served(&wide), None);
        let jpeg = picture(SIZE, SIZE, image::ImageFormat::Jpeg, |_, _| [0; 4]);
        assert_eq!(decode_served(&jpeg), None, "the hub serves PNG only");
    }

    #[test]
    fn the_round_cut_clears_the_corners_and_keeps_the_middle() {
        let mut rgba = vec![255; (SIZE * SIZE * 4) as usize];
        round(&mut rgba, SIZE);
        let alpha = |x: u32, y: u32| rgba[((y * SIZE + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0);
        assert_eq!(alpha(SIZE - 1, SIZE - 1), 0);
        assert_eq!(alpha(SIZE / 2, SIZE / 2), 255);
        assert!(alpha(SIZE / 2, 0) >= 250, "the top of the disc");
        assert!(alpha(1, SIZE / 2) >= 250);
        // Somewhere on the rim a pixel is part covered.
        assert!((0..SIZE).any(|x| (1..255).contains(&alpha(x, 10))));
    }
}
