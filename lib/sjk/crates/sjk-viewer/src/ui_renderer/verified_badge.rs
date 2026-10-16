//! The verified badge: a gold seal with a scalloped edge and a white tick, the mark
//! players know from social sites, drawn once at start into an icon cell
//! ([`super::VERIFIED_TEXTURE`]). The nameplates put it after the name of a player
//! the SJK hub's operator vouches for.
//!
//! It is computed rather than loaded: a signed distance to the seal's wavy outline
//! and to the tick's two strokes, each blended over one pixel, so the edges are
//! smooth at the cell's size and the sampler's scaling keeps them so.

/// Lobes around the seal's edge.
const LOBES: f32 = 8.0;
/// How far the lobes stand out, as a share of the radius.
const LOBE_DEPTH: f32 = 0.075;
/// The seal's mean radius, as a share of the cell, leaving room for the lobes.
const RADIUS: f32 = 0.43;
/// Gold at the top and bottom of the seal, and the darker rim.
const GOLD_TOP: [f32; 3] = [1.0, 0.86, 0.36];
const GOLD_BOTTOM: [f32; 3] = [0.95, 0.6, 0.1];
const RIM: [f32; 3] = [0.72, 0.45, 0.06];
/// The tick's corners, in radii from the centre (y down), and its half thickness.
const TICK: [[f32; 2]; 3] = [[-0.42, 0.03], [-0.12, 0.33], [0.44, -0.27]];
const TICK_HALF: f32 = 0.085;

/// Distance from `p` to the segment `a`..`b`.
fn segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [p[0] - a[0], p[1] - a[1]];
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / (ab[0] * ab[0] + ab[1] * ab[1])).clamp(0.0, 1.0);
    let d = [ap[0] - ab[0] * t, ap[1] - ab[1] * t];
    (d[0] * d[0] + d[1] * d[1]).sqrt()
}

/// Coverage of an edge at signed distance `d` pixels (negative inside).
fn coverage(d: f32) -> f32 {
    (0.5 - d).clamp(0.0, 1.0)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}

/// The badge as `size` x `size` straight (not premultiplied) RGBA bytes.
pub(crate) fn pixels(size: u32) -> Vec<u8> {
    let side = size as f32;
    let radius = RADIUS * side;
    let centre = side * 0.5;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let p = [x as f32 + 0.5 - centre, y as f32 + 0.5 - centre];
            let length = (p[0] * p[0] + p[1] * p[1]).sqrt();
            let angle = p[1].atan2(p[0]);
            let edge = radius * (1.0 + LOBE_DEPTH * (LOBES * angle).cos());
            let outside = length - edge;
            let seal = coverage(outside);
            let unit = [p[0] / radius, p[1] / radius];
            let tick = TICK
                .windows(2)
                .map(|pair| segment(unit, pair[0], pair[1]))
                .fold(f32::INFINITY, f32::min);
            let tick = coverage((tick - TICK_HALF) * radius);
            let shade = mix(
                GOLD_TOP,
                GOLD_BOTTOM,
                ((p[1] / radius + 1.0) * 0.5).clamp(0.0, 1.0),
            );
            // A rim two pixels wide, darker, along the scalloped edge.
            let rim = (1.0 - (-outside / (side * 0.03)).clamp(0.0, 1.0)) * 0.8;
            let gold = mix(shade, RIM, rim);
            let colour = mix(gold, [1.0; 3], tick);
            out.extend(
                colour
                    .iter()
                    .map(|channel| (channel * 255.0).round() as u8)
                    .chain([(seal * 255.0).round() as u8]),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(pixels: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let index = ((y * size + x) * 4) as usize;
        [
            pixels[index],
            pixels[index + 1],
            pixels[index + 2],
            pixels[index + 3],
        ]
    }

    #[test]
    fn the_seal_is_gold_with_a_white_tick_on_a_clear_cell() {
        let size = 128;
        let badge = pixels(size);
        assert_eq!(badge.len(), (size * size * 4) as usize);
        // The corners are clear.
        assert_eq!(at(&badge, size, 0, 0)[3], 0);
        assert_eq!(at(&badge, size, size - 1, size - 1)[3], 0);
        // Inside the seal, away from the tick: opaque gold (red over blue).
        let gold = at(&badge, size, 64, 30);
        assert_eq!(gold[3], 255);
        assert!(gold[0] > 200 && gold[2] < 120, "{gold:?}");
        // On the tick's corner: white.
        let corner = [
            (64.0 + TICK[1][0] * RADIUS * 128.0) as u32,
            (64.0 + TICK[1][1] * RADIUS * 128.0) as u32,
        ];
        let tick = at(&badge, size, corner[0], corner[1]);
        assert!(tick.iter().all(|channel| *channel > 230), "{tick:?}");
    }

    /// Writes the badge to `SJK_BADGE_PREVIEW` (a PNG path) to look at it.
    #[test]
    fn a_preview_can_be_written() {
        let Ok(path) = std::env::var("SJK_BADGE_PREVIEW") else {
            return;
        };
        let image = image::RgbaImage::from_raw(128, 128, pixels(128)).unwrap();
        image.save(path).unwrap();
    }
}
