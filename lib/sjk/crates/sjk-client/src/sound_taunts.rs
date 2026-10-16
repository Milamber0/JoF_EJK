//! Occurrence-based voice selection with registered-sample fallbacks.
use super::*;
impl LegacySoundAdapter {
    pub(super) fn taunt_sound(&mut self, client: usize, parameter: u8) -> Option<u16> {
        let set = self.custom[client];
        let valid =
            |index: Option<u16>| index.filter(|i| self.sounds[usize::from(*i)].handle.is_some());
        let mut pick = |bank: [Option<u16>; 3]| valid(bank[self.taunt_random.index(3)]);
        // Tayst's expanded taunt bank is available in FFA too. Resolve actual
        // registered samples before falling back, not merely occupied table slots.
        let selected = match parameter {
            3 => {
                let first = pick(set.deflect);
                let second = pick(set.gloat);
                if self.taunt_random.index(2) == 0 {
                    first.or(second)
                } else {
                    second.or(first)
                }
                .or_else(|| valid(set.anger[self.taunt_random.index(3)]))
            }
            4 => pick(set.victory),
            1 | 2 => None,
            _ => {
                let anger = pick(set.anger);
                let taunt = pick(set.taunt_numbered);
                if self.taunt_random.index(4) == 0 {
                    valid(set.taunt)
                } else if self.taunt_random.index(2) == 0 {
                    anger.or(taunt)
                } else {
                    taunt.or(anger)
                }
            }
        };
        selected.or_else(|| valid(set.taunt))
    }
}
