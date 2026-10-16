//! A fetched GIF made ready to draw, on the GIFs' worker: decoded frame by frame
//! (each frame composed over the last as the file says), made to fit [`MAX_SIDE`]
//! pixels, its delays clamped as browsers clamp them ([`delay_ms`]). What could take
//! too much memory is refused or cut: a canvas over [`MAX_CANVAS`] pixels a side is
//! refused; an animation over [`MAX_FRAMES`] frames or [`MAX_DECODED`] bytes of pixels
//! keeps its first frame only, still.

use image::{AnimationDecoder, ImageDecoder, RgbaImage};

/// The largest side a decoded GIF keeps; a larger one is scaled down to it.
pub(crate) const MAX_SIDE: u32 = 480;
/// The largest canvas side decoded at all.
pub(crate) const MAX_CANVAS: u32 = 2_048;
/// The most frames kept.
pub(crate) const MAX_FRAMES: usize = 240;
/// The most bytes of RGBA pixels one GIF keeps, every frame together.
pub(crate) const MAX_DECODED: usize = 32 << 20;
/// What the decoder may allocate while it works.
const DECODER_ALLOC: u64 = 64 << 20;
/// Delays shorter than this are taken as [`SLOW_DELAY_MS`], as browsers do: GIFs made
/// with a delay of 0 or 10 ms mean "as fast as you like", which browsers show slowly.
pub(crate) const FAST_DELAY_MS: u32 = 20;
pub(crate) const SLOW_DELAY_MS: u32 = 100;

/// A decoded GIF.
#[derive(Debug)]
pub(crate) struct Gif {
    /// Width and height of every frame.
    pub(crate) size: [u32; 2],
    pub(crate) frames: Vec<Frame>,
    /// The frames' delays added up, in milliseconds.
    pub(crate) length_ms: u64,
}

/// One frame: RGBA pixels of [`Gif::size`] and how long it shows.
#[derive(Debug)]
pub(crate) struct Frame {
    pub(crate) rgba: Box<[u8]>,
    pub(crate) delay_ms: u32,
}

impl Gif {
    /// Bytes of pixels kept.
    pub(crate) fn bytes(&self) -> usize {
        self.frames.iter().map(|frame| frame.rgba.len()).sum()
    }

    /// The frame showing `elapsed_ms` after the animation began, looping.
    pub(crate) fn frame_at(&self, elapsed_ms: u64) -> usize {
        if self.frames.len() < 2 || self.length_ms == 0 {
            return 0;
        }
        let mut left = elapsed_ms % self.length_ms;
        for (index, frame) in self.frames.iter().enumerate() {
            let delay = u64::from(frame.delay_ms);
            if left < delay {
                return index;
            }
            left -= delay;
        }
        self.frames.len() - 1
    }
}

/// A frame's delay as it shows: under [`FAST_DELAY_MS`], [`SLOW_DELAY_MS`].
pub(crate) fn delay_ms(stated_ms: u32) -> u32 {
    if stated_ms < FAST_DELAY_MS {
        SLOW_DELAY_MS
    } else {
        stated_ms
    }
}

/// The size `size` is drawn at within [`MAX_SIDE`]: unchanged when it fits, else
/// scaled down keeping its shape (at least a pixel a side).
pub(crate) fn fitted(size: [u32; 2]) -> [u32; 2] {
    let [width, height] = size;
    let largest = width.max(height);
    if largest <= MAX_SIDE {
        return size;
    }
    let scale = |side: u32| ((u64::from(side) * u64::from(MAX_SIDE)) / u64::from(largest)).max(1);
    [scale(width) as u32, scale(height) as u32]
}

/// `bytes` (a GIF file) decoded, or `None` when it is not a GIF, is broken, has no
/// frame or a canvas over [`MAX_CANVAS`].
pub(crate) fn decode(bytes: &[u8]) -> Option<Gif> {
    decode_within(bytes, MAX_DECODED)
}

/// [`decode`] keeping at most `budget` bytes of pixels.
fn decode_within(bytes: &[u8], budget: usize) -> Option<Gif> {
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return None;
    }
    let mut decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes)).ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_CANVAS);
    limits.max_image_height = Some(MAX_CANVAS);
    limits.max_alloc = Some(DECODER_ALLOC);
    decoder.set_limits(limits).ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let size = fitted([width, height]);
    let frame_bytes = size[0] as usize * size[1] as usize * 4;
    let mut frames = Vec::new();
    let mut over = false;
    for frame in decoder.into_frames() {
        let Ok(frame) = frame else {
            // A broken frame ends the animation; what came before it stays.
            break;
        };
        if frames.len() >= MAX_FRAMES || (frames.len() + 1) * frame_bytes > budget {
            over = true;
            break;
        }
        let (numerator, denominator) = frame.delay().numer_denom_ms();
        let stated = numerator.checked_div(denominator).unwrap_or(0);
        let rgba = scaled(frame.into_buffer(), size);
        frames.push(Frame {
            rgba: rgba.into_raw().into_boxed_slice(),
            delay_ms: delay_ms(stated),
        });
    }
    if over {
        frames.truncate(1);
    }
    let length_ms = if frames.len() > 1 {
        frames.iter().map(|frame| u64::from(frame.delay_ms)).sum()
    } else {
        0
    };
    (!frames.is_empty()).then_some(Gif {
        size,
        frames,
        length_ms,
    })
}

/// `image` at `size`: as it is when it already is, else resized.
fn scaled(image: RgbaImage, size: [u32; 2]) -> RgbaImage {
    if image.dimensions() == (size[0], size[1]) {
        image
    } else {
        image::imageops::resize(
            &image,
            size[0],
            size[1],
            image::imageops::FilterType::Triangle,
        )
    }
}

/// A GIF file of `frames` (each `size`, RGBA, with its delay in milliseconds), made for
/// the tests and the world shots.
#[cfg(test)]
pub(crate) fn encode_for_test(size: [u32; 2], frames: &[(Vec<u8>, u32)]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut bytes, 30);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Infinite)
            .expect("repeat");
        for (rgba, delay) in frames {
            let buffer = RgbaImage::from_raw(size[0], size[1], rgba.clone()).expect("pixels");
            let frame = image::Frame::from_parts(
                buffer,
                0,
                0,
                image::Delay::from_numer_denom_ms(*delay, 1),
            );
            encoder.encode_frame(frame).expect("frame");
        }
    }
    bytes
}

/// A small test animation: `count` frames of `size`, a gold disc moving across a dark
/// blue field, `delay` milliseconds each.
#[cfg(test)]
pub(crate) fn test_animation(size: [u32; 2], count: usize, delay: u32) -> Vec<u8> {
    let [width, height] = size;
    let frames: Vec<(Vec<u8>, u32)> = (0..count)
        .map(|index| {
            let centre = [
                (index as f32 + 0.5) / count as f32 * width as f32,
                height as f32 * 0.5,
            ];
            let radius = height as f32 * 0.3;
            let mut rgba = Vec::with_capacity((width * height * 4) as usize);
            for y in 0..height {
                for x in 0..width {
                    let dx = x as f32 + 0.5 - centre[0];
                    let dy = y as f32 + 0.5 - centre[1];
                    let pixel = if dx * dx + dy * dy <= radius * radius {
                        [0xf5, 0xc7, 0x56, 0xff]
                    } else if (x / 8 + y / 8) % 2 == 0 {
                        [0x14, 0x22, 0x3a, 0xff]
                    } else {
                        [0x1c, 0x30, 0x50, 0xff]
                    };
                    rgba.extend_from_slice(&pixel);
                }
            }
            (rgba, delay)
        })
        .collect();
    encode_for_test(size, &frames)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_animation_keeps_its_frames_and_delays() {
        let gif = decode(&test_animation([40, 20], 4, 80)).expect("decoded");
        assert_eq!(gif.size, [40, 20]);
        assert_eq!(gif.frames.len(), 4);
        assert!(gif.frames.iter().all(|frame| frame.delay_ms == 80));
        assert!(
            gif.frames
                .iter()
                .all(|frame| frame.rgba.len() == 40 * 20 * 4)
        );
        assert_eq!(gif.length_ms, 320);
        // The disc moved: the frames differ.
        assert_ne!(gif.frames[0].rgba, gif.frames[3].rgba);
        assert_eq!(gif.frame_at(0), 0);
        assert_eq!(gif.frame_at(79), 0);
        assert_eq!(gif.frame_at(80), 1);
        assert_eq!(gif.frame_at(319), 3);
        assert_eq!(gif.frame_at(320), 0, "it loops");
    }

    #[test]
    fn fast_delays_are_slowed_as_browsers_do() {
        assert_eq!(delay_ms(0), 100);
        assert_eq!(delay_ms(10), 100);
        assert_eq!(delay_ms(19), 100);
        assert_eq!(delay_ms(20), 20);
        assert_eq!(delay_ms(70), 70);
        let gif = decode(&test_animation([16, 16], 3, 10)).expect("decoded");
        assert!(gif.frames.iter().all(|frame| frame.delay_ms == 100));
        let gif = decode(&test_animation([16, 16], 3, 0)).expect("decoded");
        assert_eq!(gif.length_ms, 300);
    }

    #[test]
    fn a_large_gif_is_scaled_to_fit() {
        assert_eq!(fitted([200, 200]), [200, 200]);
        assert_eq!(fitted([480, 270]), [480, 270]);
        assert_eq!(fitted([960, 540]), [480, 270]);
        assert_eq!(fitted([600, 200]), [480, 160]);
        assert_eq!(fitted([10, 2000]), [2, 480]);
        assert_eq!(fitted([2000, 1]), [480, 1]);
        let gif = decode(&test_animation([960, 240], 2, 50)).expect("decoded");
        assert_eq!(gif.size, [480, 120]);
        assert!(
            gif.frames
                .iter()
                .all(|frame| frame.rgba.len() == 480 * 120 * 4)
        );
    }

    #[test]
    fn too_many_frames_or_bytes_keep_the_first_frame_still() {
        let gif = decode(&test_animation([8, 8], MAX_FRAMES + 1, 40)).expect("decoded");
        assert_eq!(gif.frames.len(), 1);
        assert_eq!(gif.length_ms, 0);
        assert_eq!(gif.frame_at(12_345), 0);
        let gif = decode(&test_animation([8, 8], MAX_FRAMES, 40)).expect("decoded");
        assert_eq!(gif.frames.len(), MAX_FRAMES, "at the cap it animates");
        // 480 x 480 x 4 bytes a frame: 36 frames fit 32 MiB, 37 do not.
        assert_eq!(MAX_DECODED / (MAX_SIDE * MAX_SIDE * 4) as usize, 36);
        // The same rule on a smaller budget: 10 frames of 16 x 16 fit 10 KiB, 11 do not.
        let budget = 10 * 16 * 16 * 4;
        let gif = decode_within(&test_animation([16, 16], 10, 40), budget).expect("decoded");
        assert_eq!(gif.frames.len(), 10);
        assert_eq!(gif.bytes(), budget);
        let gif = decode_within(&test_animation([16, 16], 11, 40), budget).expect("decoded");
        assert_eq!(gif.frames.len(), 1);
        assert!(gif.bytes() <= budget);
    }

    #[test]
    fn what_is_not_a_gif_or_too_big_is_refused() {
        assert!(decode(b"").is_none());
        assert!(decode(b"\x89PNG\r\n\x1a\n").is_none());
        assert!(decode(b"GIF89a broken").is_none());
        // A canvas wider than the cap, stated in the header.
        let mut wide = test_animation([8, 8], 1, 40);
        wide[6..8].copy_from_slice(&(MAX_CANVAS as u16 + 1).to_le_bytes());
        assert!(decode(&wide).is_none());
    }
}
