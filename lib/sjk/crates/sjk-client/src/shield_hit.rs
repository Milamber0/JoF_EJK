//! CG_PlayerShieldHit / CG_DrawPlayerShield, codemp/cgame/cg_players.c:5114-5198.

/// Fixed per-entity damage shell timer and outward-facing direction.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LegacyShieldHit {
    /// Presentation-time expiry of the longest overlapping hit.
    pub until: i32,
    /// Negated ByteToDir normal, retained only when the timer is extended.
    pub direction: [f32; 3],
    /// Length of the hit that set `until`, for [`Self::body_brightness`].
    pub duration: i32,
}

impl LegacyShieldHit {
    /// Apply the stock amount rule; a weaker overlapping hit cannot turn the shell.
    pub fn hit(&mut self, now: i32, amount: i32, direction: [f32; 3]) {
        let duration = if amount > 100 {
            2000
        } else {
            500 + amount.saturating_mul(15)
        };
        let until = now.saturating_add(duration);
        if until > self.until {
            self.until = until;
            self.duration = duration;
            self.direction = direction.map(|v| -v);
        }
    }

    /// Brightness of the form-fitting body shell: it fades over its own hit's length, from
    /// full to nothing, where [`Self::sample`] fades over a fixed 2 s and so leaves a small
    /// hit (0.8 s for 20 damage) dim from the start.
    pub fn body_brightness(self, now: i32, random_unit: f32) -> Option<u8> {
        if self.until <= now {
            return None;
        }
        let left = (self.until - now) as f32 / self.duration.max(1) as f32;
        Some((255.0 * left + random_unit.clamp(0.0, 1.0) * 16.0).clamp(0.0, 255.0) as u8)
    }

    /// Stock RGB brightness and scale; alpha remains opaque in the shader input.
    pub fn sample(self, now: i32, random_unit: f32) -> Option<(u8, f32)> {
        if self.until <= now {
            return None;
        }
        let brightness = (255.0 * (self.until - now) as f32 / 2000.0
            + random_unit.clamp(0.0, 1.0) * 16.0)
            .clamp(0.0, 255.0) as u8;
        Some((brightness, 1.4 - f32::from(brightness) * (0.4 / 255.0)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_small_hit_starts_at_full_body_brightness_where_the_stock_fade_starts_dim() {
        let mut hit = LegacyShieldHit::default();
        hit.hit(1_000, 20, [0.0; 3]); // 800 ms
        let stock = hit.sample(1_000, 0.0).expect("lit").0;
        assert!(stock < 110, "stock starts at {stock}");
        assert_eq!(hit.body_brightness(1_000, 0.0), Some(255));
        let half = hit.body_brightness(1_400, 0.0).expect("lit");
        assert!((126..=129).contains(&half), "{half}");
        assert_eq!(hit.body_brightness(1_800, 0.0), None);
    }

    #[test]
    fn a_longer_overlapping_hit_sets_the_fade() {
        let mut hit = LegacyShieldHit::default();
        hit.hit(0, 20, [0.0; 3]);
        hit.hit(100, 200, [0.0; 3]); // 2000 ms, ends at 2100
        assert_eq!(hit.duration, 2_000);
        hit.hit(200, 20, [0.0; 3]); // would end at 1000: ignored
        assert_eq!(hit.until, 2_100);
        assert_eq!(hit.duration, 2_000);
    }
}
