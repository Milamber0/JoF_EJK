//! Exact area reduction, with a single-cell path for power-of-two source blocks.
use glam::{Vec2, Vec4};

pub(super) struct Reduction {
    size: [usize; 2],
    scale: Vec2,
    aligned: Option<([u32; 2], f32)>,
}

impl Reduction {
    pub(super) fn new(size: [usize; 2], source: [usize; 2]) -> Self {
        let scale = Vec2::new(
            size[0] as f32 / source[0] as f32,
            size[1] as f32 / source[1] as f32,
        );
        let aligned = (0..2)
            .all(|i| {
                // Exact integer coordinates and binary fractions make the old overlap
                // weight identical to scale.x * scale.y, including edge pixels.
                source[i] <= (1 << 24)
                    && source[i] >= size[i]
                    && source[i] % size[i] == 0
                    && (source[i] / size[i]).is_power_of_two()
            })
            .then(|| {
                (
                    std::array::from_fn(|i| (source[i] / size[i]).trailing_zeros()),
                    scale.x * scale.y,
                )
            });
        Self {
            size,
            scale,
            aligned,
        }
    }

    pub(super) fn add(&self, output: &mut [Vec4], at: [usize; 2], color: Vec4) {
        if let Some((shift, weight)) = self.aligned {
            output[(at[0] >> shift[0]) + (at[1] >> shift[1]) * self.size[0]] += color * weight;
            return;
        }
        // Keep exact box overlap for NPOT images and mixed-size animation frames.
        let lo = Vec2::new(at[0] as f32, at[1] as f32) * self.scale;
        let hi = Vec2::new((at[0] + 1) as f32, (at[1] + 1) as f32) * self.scale;
        for y in lo.y.floor() as usize..(hi.y.ceil() as usize).min(self.size[1]) {
            for x in lo.x.floor() as usize..(hi.x.ceil() as usize).min(self.size[0]) {
                let weight = (hi.x.min(x as f32 + 1.) - lo.x.max(x as f32))
                    * (hi.y.min(y as f32 + 1.) - lo.y.max(y as f32));
                output[x + y * self.size[0]] += color * weight;
            }
        }
    }
}
