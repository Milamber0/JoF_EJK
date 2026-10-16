//! Who wears a blade, for the saber shaders drawn from it (`docs/unlockables.md`,
//! "Blade-skin files"): the wearer's team, for a skin taking its team's colour (`team`),
//! and the name its glyphs spell (`glyphs`); and, for a skin taking the colour of the light
//! where it is (`ambient`), that light, sampled from the map's light grid once a frame.
//!
//! A [`Persona`] rides on the skin's [`crate::saber_skins::SkinColor`] and reaches the
//! shader in the blade instance's `persona` lanes ([`crate::saber::Instance`]): lane 0 the
//! light's colour (three bytes) and the afterimage's brightness (the top byte, 255 for the
//! blade itself), lane 1 the team in its top two bits, and lanes 1 to 3 the name, six
//! 5-bit letters a lane (18 at most; 0 ends it). Names are read twice a second from the
//! game's player info (`CS_PLAYERS`), without allocating.

use crate::saber::Instance;
use crate::saber_skins::LoadedSkins;

/// `CS_PLAYERS` (`bg_public.h`).
const CS_PLAYERS: usize = 1_131;
/// Letters a name keeps (three lanes of six).
pub(crate) const NAME_LETTERS: usize = 18;
/// What a blade with no name known spells.
const NAMELESS: &[u8] = b"SJK";

/// A blade's wearer, as the skins see them.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Persona {
    /// 0 outside a team, 1 red, 2 blue.
    pub(crate) team: u8,
    /// The name's letters, six of 5 bits a lane.
    pub(crate) glyphs: [u32; 3],
}

impl Persona {
    /// The persona of a player named `name` (game text: colour codes are left out) in
    /// `team` (the player info's `t`: 1 red, 2 blue, anything else no team).
    pub(crate) fn of(name: &[u8], team: u8) -> Self {
        Self {
            team: if matches!(team, 1 | 2) { team } else { 0 },
            glyphs: spell(name),
        }
    }

    /// The same name in `team`.
    pub(crate) fn in_team(self, team: u8) -> Self {
        Self { team, ..self }
    }

    /// The instance lanes 1 to 3: the name, with the team in lane 1's top two bits.
    pub(crate) fn lanes(self) -> [u32; 3] {
        let [a, b, c] = self.glyphs;
        [a | (u32::from(self.team & 3) << 30), b, c]
    }

    /// The persona of the player whose info string (`CS_PLAYERS + slot`) is `info`.
    pub(crate) fn from_info(info: &[u8]) -> Self {
        let body = info.strip_prefix(b"\\").unwrap_or(info);
        let mut name: &[u8] = &[];
        let mut team = 0;
        let mut fields = body.split(|byte| *byte == b'\\');
        while let (Some(key), Some(value)) = (fields.next(), fields.next()) {
            match key {
                b"n" | b"name" if name.is_empty() => name = value,
                b"t" => {
                    team = std::str::from_utf8(value)
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0);
                }
                _ => {}
            }
        }
        Self::of(name, team)
    }

    /// Every client slot's persona in a session with `game_state`; empty slots the
    /// default.
    pub(crate) fn of_slots(
        game_state: &sjk_protocol::GameState,
    ) -> [Self; crate::saber_skins::MAX_CLIENTS] {
        std::array::from_fn(|slot| {
            game_state
                .config_string(CS_PLAYERS + slot)
                .map_or_else(Self::default, Self::from_info)
        })
    }
}

/// A letter's 5-bit code: `A`-`Z` 1 to 26, the digits folded onto 27 to 31; anything
/// else (spaces, signs, decorated characters) is left out.
fn letter(byte: u8) -> Option<u32> {
    match byte.to_ascii_uppercase() {
        upper @ b'A'..=b'Z' => Some(u32::from(upper - b'A') + 1),
        digit @ b'0'..=b'9' => Some(27 + u32::from(digit - b'0') % 5),
        _ => None,
    }
}

/// The letters of `name` (colour codes `^x` left out), packed six to a lane; a name with
/// none spells [`NAMELESS`].
pub(crate) fn spell(name: &[u8]) -> [u32; 3] {
    let mut lanes = [0u32; 3];
    let mut count = 0;
    let mut bytes = name.iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'^' {
            bytes.next();
            continue;
        }
        let Some(code) = letter(byte) else {
            continue;
        };
        if count == NAME_LETTERS {
            break;
        }
        lanes[count / 6] |= code << (5 * (count % 6));
        count += 1;
    }
    if count == 0 && name != NAMELESS {
        return spell(NAMELESS);
    }
    lanes
}

/// Give every instance of a skin taking the colour of the light where it is
/// ([`LoadedSkins::takes_ambient`]) that light, `sample`d at its hilt end (the grid's
/// ambient and directed light, 0 to 1 a channel). Once a frame after the entities are
/// lit; a few grid samples, no allocation.
pub(crate) fn light_instances(
    instances: &mut [Instance],
    skins: &LoadedSkins,
    sample: impl Fn([f32; 3]) -> [f32; 3],
) {
    for instance in instances {
        if instance
            .skin_index()
            .is_some_and(|index| skins.takes_ambient(index))
        {
            instance.set_surroundings(sample(instance.base()));
        }
    }
}

/// A light's colour as the instance's lane 0 holds it: its hue at full brightness (the
/// largest channel 255), so a dim room still gives its colour; black is grey.
pub(crate) fn pack_light(rgb: [f32; 3]) -> u32 {
    let top = rgb.into_iter().fold(0.0f32, f32::max);
    let unit = if top > 1e-4 {
        rgb.map(|c| (c / top).clamp(0.0, 1.0))
    } else {
        [1.0; 3]
    };
    let [r, g, b] = unit.map(|c| (c * 255.0).round() as u32);
    r | (g << 8) | (b << 16)
}

/// The strokes of SJK's glyphs on their 3 × 3 grid (`GLYPH_STROKES` in `saber.wgsl`):
/// six across, six up and four diagonals from the middle, as grid points.
pub(crate) const GLYPH_STROKES: [[f32; 4]; 16] = [
    [0.0, 0.0, 1.0, 0.0],
    [1.0, 0.0, 2.0, 0.0],
    [0.0, 1.0, 1.0, 1.0],
    [1.0, 1.0, 2.0, 1.0],
    [0.0, 2.0, 1.0, 2.0],
    [1.0, 2.0, 2.0, 2.0],
    [0.0, 0.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 2.0],
    [1.0, 0.0, 1.0, 1.0],
    [1.0, 1.0, 1.0, 2.0],
    [2.0, 0.0, 2.0, 1.0],
    [2.0, 1.0, 2.0, 2.0],
    [1.0, 1.0, 0.0, 0.0],
    [1.0, 1.0, 2.0, 0.0],
    [1.0, 1.0, 0.0, 2.0],
    [1.0, 1.0, 2.0, 2.0],
];

/// The strokes letter `code` draws, a bit each: three to seven of [`GLYPH_STROKES`], the
/// same every time; the CPU's copy of `saber.wgsl`'s `glyph_bits`.
pub(crate) fn glyph_bits(code: u32) -> u32 {
    let mut v = code.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    v = ((v >> ((v >> 28) + 4)) ^ v).wrapping_mul(277_803_737);
    let h = (v >> 22) ^ v;
    let mut w = code.wrapping_add(101).wrapping_mul(2_654_435_761);
    w ^= w >> 15;
    w = w.wrapping_mul(2_246_822_519);
    w ^= w >> 13;
    let mut bits = (h & w) & 0xffff;
    bits |= (1 << (w % 16)) | (1 << ((w >> 8) % 16)) | (1 << ((h >> 4) % 16));
    let mut dropped = 0;
    while dropped < 16 && bits.count_ones() > 7 {
        bits &= bits - 1;
        dropped += 1;
    }
    bits
}

/// Letter `code`'s strokes, each from and to a point of its glyph (0 to 1 across, and
/// along from the hilt side), as `saber.wgsl`'s `glyph` places them.
pub(crate) fn glyph_strokes(code: u32) -> impl Iterator<Item = ([f32; 2], [f32; 2])> {
    let bits = glyph_bits(code);
    let place = |x: f32, y: f32| [0.12 + 0.38 * x, 0.08 + 0.42 * y];
    GLYPH_STROKES
        .into_iter()
        .enumerate()
        .filter(move |(index, _)| bits & (1 << index) != 0)
        .map(move |(_, [ax, ay, bx, by])| (place(ax, ay), place(bx, by)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(lanes: [u32; 3]) -> Vec<u32> {
        (0..NAME_LETTERS)
            .map(|i| (lanes[i / 6] >> (5 * (i % 6))) & 31)
            .take_while(|code| *code != 0)
            .collect()
    }

    #[test]
    fn a_name_is_spelt_without_its_colours_and_signs() {
        assert_eq!(
            letters(spell(b"^1Sol^7Vulpes")),
            [19, 15, 12, 22, 21, 12, 16, 5, 19]
        );
        assert_eq!(letters(spell(b"a-b c")), [1, 2, 3]);
        // Digits fold onto the last five codes.
        assert_eq!(letters(spell(b"R2D2")), [18, 29, 4, 29]);
        // At most 18 letters.
        assert_eq!(letters(spell(&[b'Z'; 30])).len(), NAME_LETTERS);
    }

    #[test]
    fn a_name_without_letters_spells_sjk() {
        assert_eq!(spell(b"^1***"), spell(b"SJK"));
        assert_eq!(spell(b""), spell(b"SJK"));
    }

    #[test]
    fn the_team_rides_in_lane_one_above_the_letters() {
        let persona = Persona::of(b"Padawan", 2);
        let lanes = persona.lanes();
        assert_eq!(lanes[0] >> 30, 2);
        assert_eq!(lanes[0] & 0x3fff_ffff, persona.glyphs[0]);
        assert_eq!(Persona::of(b"x", 3).team, 0, "spectators are no team");
        assert_eq!(Persona::of(b"x", 1).team, 1);
    }

    #[test]
    fn the_player_info_gives_the_name_and_team() {
        let persona = Persona::from_info(b"n\\^4Kyle\\t\\1\\model\\kyle/default");
        assert_eq!(persona, Persona::of(b"Kyle", 1));
        assert_eq!(
            Persona::from_info(b"\\t\\2\\n\\Jan"),
            Persona::of(b"Jan", 2)
        );
        assert_eq!(Persona::from_info(b""), Persona::of(b"", 0));
    }

    #[test]
    fn every_letter_has_its_own_glyph_of_three_to_seven_strokes() {
        let glyphs: Vec<u32> = (1..32).map(glyph_bits).collect();
        for (index, bits) in glyphs.iter().enumerate() {
            assert!((3..=7).contains(&bits.count_ones()), "{index}: {bits:b}");
            assert!(
                !glyphs[..index].contains(bits),
                "letter {} repeats",
                index + 1
            );
        }
        assert_eq!(glyph_strokes(1).count(), glyphs[0].count_ones() as usize);
        // The shader draws the same: its strokes and its hash.
        let shader = include_str!("saber.wgsl");
        for [ax, ay, bx, by] in GLYPH_STROKES {
            let stroke = format!("vec4({ax:.1}, {ay:.1}, {bx:.1}, {by:.1})");
            assert!(shader.contains(&stroke), "{stroke}");
        }
        for line in [
            "var v = code * 747796405u + 2891336453u;",
            "v = ((v >> ((v >> 28u) + 4u)) ^ v) * 277803737u;",
            "var w = (code + 101u) * 2654435761u;",
            "w = w * 2246822519u;",
            "bits = bits | (1u << (w % 16u)) | (1u << ((w >> 8u) % 16u)) | (1u << ((h >> 4u) % 16u));",
            "let a = vec2(0.12, 0.08) + stroke.xy * vec2(0.38, 0.42);",
        ] {
            assert!(shader.contains(line), "saber.wgsl lost {line:?}");
        }
    }

    #[test]
    fn a_light_is_kept_at_full_brightness() {
        assert_eq!(pack_light([0.2, 0.1, 0.0]), 255 | (128 << 8));
        assert_eq!(pack_light([0.0; 3]), 0x00ff_ffff, "black is grey");
    }
}
