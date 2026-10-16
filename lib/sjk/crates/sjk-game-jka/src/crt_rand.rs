//! The C library's `rand()` as glibc computes it (`random_r.c`, the default `TYPE_3`
//! additive feedback generator, `x^31 + x^3 + 1`), which the reference's game module
//! uses beside its own `holdrand` generator: `RandFloat` (the saber's deflection of a
//! missile, `w_saber.c:56`), `random()`/`crandom()` (a blaster's spread, `q_shared.h`).
//! A server's sequence depends on every draw since its start; the oracle's, seeded by
//! its driver (`srand`), on the draws it makes — which is why this is ported exactly.

/// glibc's `random()` state: 31 words, the front pointer three ahead of the rear, 310
/// outputs discarded after seeding.
#[derive(Clone, Debug)]
pub struct CrtRand {
    state: [i32; 31],
    front: usize,
    rear: usize,
}

impl CrtRand {
    /// `srand(seed)`; a seed of 0 is taken as 1, as glibc takes it.
    pub fn new(seed: u32) -> Self {
        let mut state = [0_i32; 31];
        state[0] = if seed == 0 { 1 } else { seed as i32 };
        for i in 1..31 {
            // `(16807 * word) % 2147483647` without overflow (Schrage), negatives lifted.
            let hi = state[i - 1] / 127_773;
            let lo = state[i - 1] % 127_773;
            let mut word = 16_807 * lo - 2_836 * hi;
            if word < 0 {
                word += 2_147_483_647;
            }
            state[i] = word;
        }
        let mut generator = Self {
            state,
            front: 3,
            rear: 0,
        };
        for _ in 0..310 {
            generator.next();
        }
        generator
    }

    /// `rand()`: the next value, 0..=2147483647.
    pub fn next(&mut self) -> i32 {
        let value = self.state[self.front].wrapping_add(self.state[self.rear]);
        self.state[self.front] = value;
        self.front = (self.front + 1) % 31;
        self.rear = (self.rear + 1) % 31;
        (value as u32 >> 1) as i32
    }
}
