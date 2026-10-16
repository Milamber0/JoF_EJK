//! What an invisible brush face is: the tool textures Radiant shows (clip,
//! trigger, ladder…) sorted by the contents and surface flags q3map2 compiled
//! into the BSP, and the names `r_showBrushesTypes` picks them by.

/// Contents and surface flags (`codemp/game/surfaceflags.h`).
mod flags {
    pub(super) const CONTENTS_SOLID: u32 = 0x0000_0001;
    pub(super) const CONTENTS_LAVA: u32 = 0x0000_0002;
    pub(super) const CONTENTS_WATER: u32 = 0x0000_0004;
    pub(super) const CONTENTS_FOG: u32 = 0x0000_0008;
    pub(super) const CONTENTS_PLAYERCLIP: u32 = 0x0000_0010;
    pub(super) const CONTENTS_MONSTERCLIP: u32 = 0x0000_0020;
    pub(super) const CONTENTS_BOTCLIP: u32 = 0x0000_0040;
    pub(super) const CONTENTS_SHOTCLIP: u32 = 0x0000_0080;
    pub(super) const CONTENTS_TRIGGER: u32 = 0x0000_0400;
    pub(super) const CONTENTS_NODROP: u32 = 0x0000_0800;
    pub(super) const CONTENTS_TERRAIN: u32 = 0x0000_1000;
    pub(super) const CONTENTS_LADDER: u32 = 0x0000_2000;
    pub(super) const CONTENTS_OUTSIDE: u32 = 0x0001_0000;
    pub(super) const CONTENTS_SLIME: u32 = 0x0002_0000;
    pub(super) const CONTENTS_INSIDE: u32 = 0x1000_0000;
    pub(super) const SURF_SLICK: u32 = 0x0000_4000;
    pub(super) const SURF_NODAMAGE: u32 = 0x0004_0000;
    pub(super) const SURF_NODRAW: u32 = 0x0020_0000;
}
use flags::*;

/// One kind of invisible brush face, drawn with its own colour and label.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// `textures/system/clip`: blocks players and NPCs.
    Clip,
    /// `blockplayer`: blocks players only.
    PlayerClip,
    /// `blocknpc`: blocks NPCs only.
    NpcClip,
    /// `shotonlyclip`: stops shots, not players.
    ShotClip,
    /// A solid brush with no drawn face (`physics_clip`, `nodraw_solid`): an
    /// invisible wall. Caulk is [`Kind::Caulk`].
    Solid,
    /// A `trigger_*` entity other than those below, or a trigger-textured brush.
    Trigger,
    Teleport,
    Push,
    Hurt,
    Ladder,
    /// Death pits and other volumes that leave no bodies or items.
    NoDrop,
    Slick,
    /// `cushion`: no falling damage.
    Cushion,
    /// `outside` / `inside` weather volumes.
    Weather,
    /// `do_not_enter`: a navigation hint for bots.
    BotClip,
    Caulk,
    /// Any other face that draws nothing (hint, skip, areaportal, origin…).
    Nodraw,
    /// A `func_door`: shown at the place the map file puts it.
    Door,
    /// A `func_plat` lift.
    Lift,
    /// A `func_button`.
    Button,
    /// A `func_train`.
    Train,
    /// A `func_rotating`, `func_pendulum` or `func_bobbing`: moves for good.
    Endless,
    /// A `func_breakable` or `func_glass`.
    Breakable,
    /// The box a door, lift or button moves through.
    Travel,
}

impl Kind {
    pub(crate) const ALL: [Self; 24] = [
        Self::Clip,
        Self::PlayerClip,
        Self::NpcClip,
        Self::ShotClip,
        Self::Solid,
        Self::Trigger,
        Self::Teleport,
        Self::Push,
        Self::Hurt,
        Self::Ladder,
        Self::NoDrop,
        Self::Slick,
        Self::Cushion,
        Self::Weather,
        Self::BotClip,
        Self::Caulk,
        Self::Nodraw,
        Self::Door,
        Self::Lift,
        Self::Button,
        Self::Train,
        Self::Endless,
        Self::Breakable,
        Self::Travel,
    ];

    /// What `r_showBrushes 1` shows: every kind that changes how a player
    /// moves, fights or dies.
    pub(crate) const GAMEPLAY: KindSet = KindSet::of(&[
        Self::Clip,
        Self::PlayerClip,
        Self::NpcClip,
        Self::ShotClip,
        Self::Solid,
        Self::Trigger,
        Self::Teleport,
        Self::Push,
        Self::Hurt,
        Self::Ladder,
        Self::NoDrop,
        Self::Slick,
        Self::Cushion,
        Self::Door,
        Self::Lift,
        Self::Button,
        Self::Train,
        Self::Endless,
    ]);

    pub(crate) fn index(self) -> usize {
        self as usize
    }

    /// The kind's name in `r_showBrushesTypes`.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Clip => "clip",
            Self::PlayerClip => "playerclip",
            Self::NpcClip => "npcclip",
            Self::ShotClip => "shotclip",
            Self::Solid => "solid",
            Self::Trigger => "trigger",
            Self::Teleport => "teleport",
            Self::Push => "push",
            Self::Hurt => "hurt",
            Self::Ladder => "ladder",
            Self::NoDrop => "nodrop",
            Self::Slick => "slick",
            Self::Cushion => "cushion",
            Self::Weather => "weather",
            Self::BotClip => "botclip",
            Self::Caulk => "caulk",
            Self::Nodraw => "nodraw",
            Self::Door => "door",
            Self::Lift => "lift",
            Self::Button => "button",
            Self::Train => "train",
            Self::Endless => "endless",
            Self::Breakable => "breakable",
            Self::Travel => "travel",
        }
    }

    /// The label written on the face's tiles, one or two lines.
    pub(crate) fn label(self) -> &'static [&'static str] {
        match self {
            Self::Clip => &["CLIP"],
            Self::PlayerClip => &["PLAYER", "CLIP"],
            Self::NpcClip => &["NPC", "CLIP"],
            Self::ShotClip => &["SHOT", "CLIP"],
            Self::Solid => &["SOLID"],
            Self::Trigger => &["TRIGGER"],
            Self::Teleport => &["TELEPORT"],
            Self::Push => &["PUSH"],
            Self::Hurt => &["HURT"],
            Self::Ladder => &["LADDER"],
            Self::NoDrop => &["NODROP"],
            Self::Slick => &["SLICK"],
            Self::Cushion => &["CUSHION"],
            Self::Weather => &["WEATHER"],
            Self::BotClip => &["BOT", "CLIP"],
            Self::Caulk => &["CAULK"],
            Self::Nodraw => &["NODRAW"],
            Self::Door => &["DOOR"],
            Self::Lift => &["LIFT"],
            Self::Button => &["BUTTON"],
            Self::Train => &["TRAIN"],
            Self::Endless => &["ALWAYS", "MOVING"],
            Self::Breakable => &["BREAKABLE"],
            Self::Travel => &["TRAVEL"],
        }
    }

    /// Display (sRGB) colour of the face.
    pub(crate) fn colour(self) -> [f32; 3] {
        match self {
            Self::Clip => [0.86, 0.26, 0.86],
            Self::PlayerClip => [0.96, 0.50, 0.96],
            Self::NpcClip => [0.56, 0.42, 1.0],
            Self::ShotClip => [0.26, 0.60, 1.0],
            Self::Solid => [0.80, 0.82, 0.86],
            Self::Trigger => [1.0, 0.60, 0.15],
            Self::Teleport => [0.25, 0.95, 0.66],
            Self::Push => [1.0, 0.88, 0.22],
            Self::Hurt => [1.0, 0.22, 0.18],
            Self::Ladder => [0.36, 0.86, 0.30],
            Self::NoDrop => [0.72, 0.10, 0.26],
            Self::Slick => [0.66, 0.92, 1.0],
            Self::Cushion => [0.96, 0.72, 0.56],
            Self::Weather => [0.46, 0.60, 0.76],
            Self::BotClip => [0.30, 0.76, 0.76],
            Self::Caulk => [1.0, 0.56, 0.72],
            Self::Nodraw => [0.86, 0.80, 0.40],
            Self::Door => [0.20, 0.86, 0.96],
            Self::Lift => [0.70, 0.86, 0.20],
            Self::Button => [1.0, 0.40, 0.62],
            Self::Train => [0.82, 0.66, 0.30],
            Self::Endless => [0.40, 0.50, 0.95],
            Self::Breakable => [0.70, 0.78, 0.90],
            Self::Travel => [0.95, 0.95, 0.95],
        }
    }

    /// The kind of every face of a `trigger_*` entity's brushes, by classname.
    pub(crate) fn of_trigger(classname: &str) -> Self {
        match classname.to_ascii_lowercase().as_str() {
            "trigger_teleport" => Self::Teleport,
            "trigger_push" => Self::Push,
            "trigger_hurt" => Self::Hurt,
            _ => Self::Trigger,
        }
    }

    /// The kind of every face of a brush entity that moves or breaks, by
    /// classname; `None` for an entity whose brushes the map draws and keeps still.
    pub(crate) fn of_mover(classname: &str) -> Option<Self> {
        Some(match classname.to_ascii_lowercase().as_str() {
            "func_door" | "func_door_rotating" => Self::Door,
            "func_plat" => Self::Lift,
            "func_button" => Self::Button,
            "func_train" => Self::Train,
            "func_rotating" | "func_pendulum" | "func_bobbing" => Self::Endless,
            "func_breakable" | "func_glass" => Self::Breakable,
            _ => return None,
        })
    }

    /// The kind of a world brush's face: `contents` are its brush's, `surface`
    /// and `shader` its own side's. `None` for a face that draws, or whose
    /// volume shows anyway (fog, water, lava, slime, terrain).
    pub(crate) fn of_face(contents: u32, surface: u32, shader: &[u8]) -> Option<Self> {
        if surface & SURF_NODRAW == 0
            || contents
                & (CONTENTS_FOG
                    | CONTENTS_WATER
                    | CONTENTS_LAVA
                    | CONTENTS_SLIME
                    | CONTENTS_TERRAIN)
                != 0
        {
            return None;
        }
        let clips = contents & (CONTENTS_PLAYERCLIP | CONTENTS_MONSTERCLIP);
        Some(if contents & CONTENTS_TRIGGER != 0 {
            Self::Trigger
        } else if contents & CONTENTS_NODROP != 0 {
            Self::NoDrop
        } else if contents & CONTENTS_LADDER != 0 {
            Self::Ladder
        } else if surface & SURF_SLICK != 0 {
            // `slick` is player clip with the slick flag.
            Self::Slick
        } else if surface & SURF_NODAMAGE != 0 {
            Self::Cushion
        } else if clips == CONTENTS_PLAYERCLIP | CONTENTS_MONSTERCLIP {
            Self::Clip
        } else if clips == CONTENTS_PLAYERCLIP {
            Self::PlayerClip
        } else if clips == CONTENTS_MONSTERCLIP {
            Self::NpcClip
        } else if contents & CONTENTS_SOLID == 0 && contents & CONTENTS_SHOTCLIP != 0 {
            Self::ShotClip
        } else if contents & (CONTENTS_OUTSIDE | CONTENTS_INSIDE) != 0 {
            Self::Weather
        } else if contents & CONTENTS_BOTCLIP != 0 {
            Self::BotClip
        } else if contents & CONTENTS_SOLID != 0 {
            // Caulk and an invisible wall carry the same flags; only the name tells.
            if contains_ignore_case(shader, b"caulk") {
                Self::Caulk
            } else {
                Self::Solid
            }
        } else {
            Self::Nodraw
        })
    }
}

fn contains_ignore_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// A set of [`Kind`]s, one bit each.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct KindSet(u32);

impl KindSet {
    pub(crate) const EMPTY: Self = Self(0);
    pub(crate) const ALL: Self = Self((1 << Kind::ALL.len()) - 1);

    pub(crate) const fn of(kinds: &[Kind]) -> Self {
        let mut bits = 0;
        let mut i = 0;
        while i < kinds.len() {
            bits |= 1 << kinds[i] as u32;
            i += 1;
        }
        Self(bits)
    }

    pub(crate) fn with(self, kind: Kind) -> Self {
        Self(self.0 | 1 << kind.index())
    }

    pub(crate) fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub(crate) fn contains(self, kind: Kind) -> bool {
        self.0 & 1 << kind.index() != 0
    }

    pub(crate) fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The kinds `r_showBrushes` `mode` and `r_showBrushesTypes` `types` show:
    /// `types`' names when it lists any, else the gameplay kinds for mode 1 and
    /// every kind for 2 and above. Unknown names are ignored.
    pub(crate) fn chosen(mode: i64, types: &str) -> Self {
        if mode <= 0 {
            return Self::EMPTY;
        }
        let listed = types
            .split(|c: char| c.is_ascii_whitespace() || c == ',' || c == ';')
            .filter_map(|name| {
                Kind::ALL
                    .into_iter()
                    .find(|kind| kind.name().eq_ignore_ascii_case(name))
            })
            .fold(Self::EMPTY, Self::with);
        if !listed.is_empty() {
            listed
        } else if mode == 1 {
            Kind::GAMEPLAY
        } else {
            Self::ALL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `surfaceparm` sets of retail `shaders/system.shader`, compiled.
    const NONSOLID_NODRAW: u32 = SURF_NODRAW;

    #[test]
    fn system_textures_get_their_kinds() {
        let face = |contents, surface| Kind::of_face(contents, surface, b"textures/system/x");
        assert_eq!(
            face(CONTENTS_PLAYERCLIP | CONTENTS_MONSTERCLIP, NONSOLID_NODRAW),
            Some(Kind::Clip)
        );
        assert_eq!(
            face(CONTENTS_PLAYERCLIP, NONSOLID_NODRAW),
            Some(Kind::PlayerClip)
        );
        assert_eq!(
            face(CONTENTS_MONSTERCLIP, NONSOLID_NODRAW),
            Some(Kind::NpcClip)
        );
        assert_eq!(
            face(CONTENTS_SHOTCLIP, NONSOLID_NODRAW),
            Some(Kind::ShotClip)
        );
        assert_eq!(face(CONTENTS_TRIGGER, SURF_NODRAW), Some(Kind::Trigger));
        assert_eq!(face(CONTENTS_NODROP, SURF_NODRAW), Some(Kind::NoDrop));
        assert_eq!(face(CONTENTS_LADDER, SURF_NODRAW), Some(Kind::Ladder));
        assert_eq!(
            face(CONTENTS_PLAYERCLIP, SURF_NODRAW | SURF_SLICK),
            Some(Kind::Slick)
        );
        assert_eq!(
            face(CONTENTS_SOLID, SURF_NODRAW | SURF_NODAMAGE),
            Some(Kind::Cushion)
        );
        assert_eq!(face(CONTENTS_OUTSIDE, SURF_NODRAW), Some(Kind::Weather));
        assert_eq!(face(CONTENTS_BOTCLIP, SURF_NODRAW), Some(Kind::BotClip));
        assert_eq!(face(CONTENTS_SOLID, SURF_NODRAW), Some(Kind::Solid));
        assert_eq!(
            Kind::of_face(CONTENTS_SOLID, SURF_NODRAW, b"textures/system/CAULK"),
            Some(Kind::Caulk)
        );
        assert_eq!(face(0, SURF_NODRAW), Some(Kind::Nodraw));
    }

    #[test]
    fn drawn_faces_and_visible_volumes_are_left_out() {
        assert_eq!(Kind::of_face(CONTENTS_PLAYERCLIP, 0, b"glass"), None);
        assert_eq!(
            Kind::of_face(CONTENTS_WATER, SURF_NODRAW, b"caulk_water"),
            None
        );
        assert_eq!(Kind::of_face(CONTENTS_FOG, SURF_NODRAW, b"fog"), None);
        assert_eq!(
            Kind::of_face(CONTENTS_TERRAIN, SURF_NODRAW, b"terrain"),
            None
        );
    }

    #[test]
    fn triggers_take_their_entity_kind() {
        assert_eq!(Kind::of_trigger("trigger_teleport"), Kind::Teleport);
        assert_eq!(Kind::of_trigger("Trigger_Push"), Kind::Push);
        assert_eq!(Kind::of_trigger("trigger_hurt"), Kind::Hurt);
        assert_eq!(Kind::of_trigger("trigger_multiple"), Kind::Trigger);
    }

    #[test]
    fn movers_take_their_entity_kind() {
        assert_eq!(Kind::of_mover("func_door"), Some(Kind::Door));
        assert_eq!(Kind::of_mover("FUNC_PLAT"), Some(Kind::Lift));
        assert_eq!(Kind::of_mover("func_button"), Some(Kind::Button));
        assert_eq!(Kind::of_mover("func_train"), Some(Kind::Train));
        assert_eq!(Kind::of_mover("func_bobbing"), Some(Kind::Endless));
        assert_eq!(Kind::of_mover("func_glass"), Some(Kind::Breakable));
        assert_eq!(Kind::of_mover("func_static"), None);
        assert_eq!(Kind::of_mover("trigger_hurt"), None);
    }

    #[test]
    fn modes_and_type_lists_choose_the_kinds() {
        assert_eq!(KindSet::chosen(0, "clip"), KindSet::EMPTY);
        assert_eq!(KindSet::chosen(1, ""), Kind::GAMEPLAY);
        assert_eq!(KindSet::chosen(2, " "), KindSet::ALL);
        let listed = KindSet::chosen(1, "Clip, teleport;bogus");
        assert!(listed.contains(Kind::Clip) && listed.contains(Kind::Teleport));
        assert!(!listed.contains(Kind::Trigger));
        assert!(!Kind::GAMEPLAY.contains(Kind::Caulk));
        // Movers are gameplay; their travel boxes and breakables need mode 2.
        assert!(Kind::GAMEPLAY.contains(Kind::Door));
        assert!(!Kind::GAMEPLAY.contains(Kind::Travel));
        assert!(!Kind::GAMEPLAY.contains(Kind::Breakable));
    }

    #[test]
    fn kinds_are_listed_in_order_with_unique_names() {
        for (index, kind) in Kind::ALL.into_iter().enumerate() {
            assert_eq!(kind.index(), index);
            assert_eq!(
                Kind::ALL.iter().filter(|k| k.name() == kind.name()).count(),
                1
            );
        }
    }
}
