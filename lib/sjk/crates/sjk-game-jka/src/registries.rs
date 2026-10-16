//! What the game registers by index and tells its clients as configstrings: sounds
//! (`G_SoundIndex` → `G_FindConfigstringIndex`, `g_utils.c`), the items present
//! (`SaveRegisteredItems`, `g_items.c`), and the strings `G_InitGame` sets for the
//! scoreboard and the duel (`g_main.c:340-354`). Held against the configstring lines of
//! the whole-game transcripts, in the reference's order.

/// `CS_SOUNDS`: the first sound index is one, at this string.
pub const CS_SOUNDS: usize = 811;
/// `MAX_SOUNDS`.
const SOUNDS: usize = 256;
const CS_ITEMS: usize = 27;
const CS_CLIENT_DUELWINNER: usize = 29;
const CS_CLIENT_DUELISTS: usize = 30;
const CS_CLIENT_DUELHEALTHS: usize = 31;
/// `CS_LOCATIONS` (`bg_public.h`): "unknown", then the map's locations.
pub const CS_LOCATIONS: usize = 1_227;
const GT_POWER_DUEL: i32 = 4;
/// `bg_numItems`: how many items the item list has, for the mask of the ones present.
const ITEMS: usize = 51;

/// The sound table: names in registration order, each at `CS_SOUNDS + index`.
#[derive(Clone, Debug, Default)]
pub struct SoundTable {
    names: Vec<Vec<u8>>,
}

impl SoundTable {
    /// `G_SoundIndex`: the index of `name`, registering it — and publishing it through
    /// `set` — the first time. Zero for an empty name or a full table (the reference
    /// drops the server on a full table; this one has no more to give).
    pub fn index(&mut self, name: &[u8], set: &mut impl FnMut(usize, &[u8])) -> u16 {
        // `G_FindConfigstringIndex` compares with `strcmp`: a name in another case is
        // another sound.
        find_or_register(&mut self.names, CS_SOUNDS, SOUNDS, name, set)
    }

    /// The names registered, the first at index one.
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }

    /// How many sounds are registered.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether none is.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// `CS_MODELS`: the models' configstrings, at `CS_MODELS + index`.
pub const CS_MODELS: usize = 298;
/// `MAX_MODELS`.
const MODELS: usize = 512;

/// The model table (`G_ModelIndex`): names in registration order, each at
/// `CS_MODELS + index`.
#[derive(Clone, Debug, Default)]
pub struct ModelTable {
    names: Vec<Vec<u8>>,
}

impl ModelTable {
    /// `G_ModelIndex`: the index of `name`, registering and publishing it through `set`
    /// the first time. Zero for an empty name or a full table.
    pub fn index(&mut self, name: &[u8], set: &mut impl FnMut(usize, &[u8])) -> u16 {
        find_or_register(&mut self.names, CS_MODELS, MODELS, name, set)
    }

    /// The names registered, the first at index one.
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }
}

/// `CS_EFFECTS` (`bg_public.h:148`): the effects' configstrings, at `CS_EFFECTS + index`.
pub const CS_EFFECTS: usize = 1_355;
/// `MAX_FX`.
const EFFECTS: usize = 64;

/// The effect table (`G_EffectIndex`): names in registration order, each at
/// `CS_EFFECTS + index`.
#[derive(Clone, Debug, Default)]
pub struct EffectTable {
    names: Vec<Vec<u8>>,
}

impl EffectTable {
    /// `G_EffectIndex`: the index of `name`, registering and publishing it through `set`
    /// the first time. Zero for an empty name or a full table.
    pub fn index(&mut self, name: &[u8], set: &mut impl FnMut(usize, &[u8])) -> u16 {
        find_or_register(&mut self.names, CS_EFFECTS, EFFECTS, name, set)
    }

    /// The names registered, the first at index one.
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }
}

/// `CS_ICONS` (`bg_public.h:136`): the radar icons' configstrings, at `CS_ICONS + index`.
pub const CS_ICONS: usize = 1_067;
/// `MAX_ICONS`.
const ICONS: usize = 64;

/// The icon table (`G_IconIndex`, `g_utils.c:118-120`): the radar icons siege entities
/// name, in registration order, each at `CS_ICONS + index`.
#[derive(Clone, Debug, Default)]
pub struct IconTable {
    names: Vec<Vec<u8>>,
}

impl IconTable {
    /// `G_IconIndex`: the index of `name`, registering and publishing it through `set` the
    /// first time. Zero for an empty name or a full table.
    pub fn index(&mut self, name: &[u8], set: &mut impl FnMut(usize, &[u8])) -> u16 {
        find_or_register(&mut self.names, CS_ICONS, ICONS, name, set)
    }
}

/// `CS_G2BONES` (`bg_public.h:141`): the bones named for clients to turn, at
/// `CS_G2BONES + index`.
pub const CS_G2BONES: usize = 1_163;
/// `MAX_G2BONES`.
const G2BONES: usize = 64;

/// The bone table (`G_BoneIndex`, `g_utils.c:122-124`): the bones an entity's
/// `boneIndex1`..`4` name, in registration order, each at `CS_G2BONES + index`.
#[derive(Clone, Debug, Default)]
pub struct BoneTable {
    names: Vec<Vec<u8>>,
}

impl BoneTable {
    /// `G_BoneIndex`: the index of bone `name`, registering and publishing it through `set`
    /// the first time. Zero for an empty name or a full table.
    pub fn index(&mut self, name: &[u8], set: &mut impl FnMut(usize, &[u8])) -> u16 {
        find_or_register(&mut self.names, CS_G2BONES, G2BONES, name, set)
    }

    /// The names registered, the first at index one.
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }
}

/// `CS_AMBIENT_SET` (`bg_public.h:125`): the ambient soundsets' configstrings, at
/// `CS_AMBIENT_SET + index`.
pub const CS_AMBIENT_SET: usize = 37;
/// `MAX_AMBIENT_SETS`.
const AMBIENT_SETS: usize = 256;

/// The soundset table (`G_SoundSetIndex`): names in registration order, each at
/// `CS_AMBIENT_SET + index`. A client reads a soundset's sounds from its name.
#[derive(Clone, Debug, Default)]
pub struct SoundSetTable {
    names: Vec<Vec<u8>>,
}

impl SoundSetTable {
    /// `G_SoundSetIndex`: the index of `name`, registering and publishing it through
    /// `set` the first time. Zero for an empty name or a full table.
    pub fn index(&mut self, name: &[u8], set: &mut impl FnMut(usize, &[u8])) -> u16 {
        find_or_register(&mut self.names, CS_AMBIENT_SET, AMBIENT_SETS, name, set)
    }

    /// The index `name` was registered at, without registering it.
    pub fn find(&self, name: &[u8]) -> u16 {
        self.names
            .iter()
            .position(|known| known == name)
            .map_or(0, |found| found as u16 + 1)
    }

    /// The names registered, the first at index one.
    pub fn names(&self) -> &[Vec<u8>] {
        &self.names
    }
}

/// `G_FindConfigstringIndex` with `create` over one table of `capacity` strings from
/// `base`: the index of `name` (compared with `strcmp`), registered and published through
/// `set` the first time; zero for an empty name or a full table (where the reference
/// drops the server, this one has no more to give).
fn find_or_register(
    names: &mut Vec<Vec<u8>>,
    base: usize,
    capacity: usize,
    name: &[u8],
    set: &mut impl FnMut(usize, &[u8]),
) -> u16 {
    if name.is_empty() {
        return 0;
    }
    if let Some(found) = names.iter().position(|known| known == name) {
        return (found + 1) as u16;
    }
    if names.len() + 1 >= capacity {
        return 0;
    }
    names.push(name.to_vec());
    set(base + names.len(), name);
    names.len() as u16
}

/// What the game registers and publishes as it starts, in order: the four sounds of
/// `G_InitGame` (`g_main.c:223-227`), then — after the map's entities — the unknown
/// location, the duel strings and the items present. Between the sounds and the rest
/// come the worldspawn's strings (`worldspawn::spawn_world`), which the caller publishes
/// there. Returns the sound table to go on registering with.
pub fn init_game(set: &mut impl FnMut(usize, &[u8])) -> SoundTable {
    let mut sounds = SoundTable::default();
    for name in [
        &b"sound/player/fry.wav"[..],
        b"sound/player/hacking.wav",
        b"sound/player/supp_healed.wav",
        b"sound/player/supp_supplied.wav",
    ] {
        sounds.index(name, set);
    }
    sounds
}

/// The strings `G_InitGame` sets after the map's entities: `G_SpawnEntitiesFromString`'s
/// unknown location, the duel strings, and the items registered — in every game type
/// the weapons a player spawns with, at the item list's 19..22 (stun baton, melee, saber,
/// pistol); team items and the Jedi Master's saber are the modes' to add.
pub fn init_game_strings(gametype: i32, set: &mut impl FnMut(usize, &[u8])) {
    set(CS_LOCATIONS, b"unknown");
    set(
        CS_CLIENT_DUELISTS,
        if gametype == GT_POWER_DUEL {
            b"-1|-1|-1"
        } else {
            b"-1|-1"
        },
    );
    set(CS_CLIENT_DUELHEALTHS, b"-1|-1|!");
    set(CS_CLIENT_DUELWINNER, b"-1");
    let mut present = [b'0'; ITEMS];
    present[19..23].fill(b'1');
    set(CS_ITEMS, &present);
}

/// The sounds the first begin registers, once for the game: the Force powers' loops
/// (`WP_InitForcePowers`, `w_force.c:180-190`) and the saber's spin
/// (`WP_SaberInitBladeData`, `w_saber.c:650`).
pub fn first_begin_sounds(sounds: &mut SoundTable, set: &mut impl FnMut(usize, &[u8])) {
    for name in [
        &b"sound/weapons/force/speedloop.wav"[..],
        b"sound/weapons/force/rageloop.wav",
        b"sound/weapons/force/absorbloop.wav",
        b"sound/weapons/force/protectloop.wav",
        b"sound/weapons/force/seeloop.wav",
        b"sound/player/nullifyloop.wav",
        b"sound/weapons/saber/saberspin.wav",
    ] {
        sounds.index(name, set);
    }
}
