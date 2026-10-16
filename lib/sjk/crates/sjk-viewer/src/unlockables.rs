//! The unlockables this client knows (`docs/unlockables.md`, "Catalogue"): the one
//! catalogue the looks, the blade skins, the `saberskin` command, the Unlockables page
//! and the Staff page read. The hub keeps its own copy (`src/unlocks.rs`); a new
//! unlockable needs both, and an id never changes meaning. An id this client does not
//! know is ignored wherever it comes from (a profile, a look, `cg_saberSkin`).
//!
//! The catalogue is public metadata only: what an unlockable looks and sounds like is
//! art in the SJK hub's packs (`sjk_packs.rs`), a blade skin's in its blade-skin file.

/// Archived: the blade-skin unlock id the player wears, empty for the stock blade. It
/// shows (and is sent to the hub) only while the player's own hub profile lists it.
pub(crate) const SABER_SKIN_CVAR: &str = "cg_saberSkin";

/// What an unlockable is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Kind {
    /// A look a player's blades wear instead of their colour, drawn from its blade-skin
    /// file (`skins/blades/<id>.bladeskin` in a pack).
    BladeSkin,
}

/// How rare an unlockable is, the commonest first: Common (14/10/2026, white: filler
/// that holocrons of any tier may drop; there is no Common holocron) and the holocrons'
/// four tiers ([`crate::holocrons::TIERS`]) with their names and colours.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub(crate) enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
    Mythical,
}

impl Rarity {
    /// The holocron tier of the same name; `None` for Common, which no holocron is.
    fn holocron_tier(self) -> Option<&'static crate::holocrons::Tier> {
        (self as usize)
            .checked_sub(1)
            .map(|index| &crate::holocrons::TIERS[index])
    }

    /// The tier's name ("Rare").
    pub(crate) fn label(self) -> &'static str {
        self.holocron_tier().map_or("Common", |tier| tier.label)
    }

    /// The tier's colour: Common's is white (a touch cool, to sit on the menus).
    pub(crate) fn colour(self) -> sjk_ui::Color {
        self.holocron_tier()
            .map_or(crate::holocrons::rgb(0xE6_EAF2), |tier| tier.colour)
    }

    /// The tier's mark in SJK Symbols, Common's the empty octagon.
    pub(crate) fn symbol(self) -> char {
        crate::symbols::RARITY[self as usize]
    }

    /// Its place among the effects' ranks ([`crate::rarity_fx`]): Common and Uncommon
    /// are 0 and draw none.
    pub(crate) fn effect_rank(self) -> usize {
        (self as usize).saturating_sub(1)
    }
}

/// The saber smiths: who gave a shader's idea ([`Unlockable::smith`]).
pub(crate) const SOL: &str = "Sol";
pub(crate) const CREYON: &str = "Creyon";

/// One unlockable of the catalogue.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct Unlockable {
    /// The id the hub, a look and `cg_saberSkin` name it by.
    pub(crate) id: &'static str,
    pub(crate) name: &'static str,
    pub(crate) kind: Kind,
    /// How rare it is: the holocrons' four tiers, their names and colours.
    pub(crate) tier: Rarity,
    /// A chroma: a blade skin that takes the wearer's saber colour (`color1` for the
    /// first saber, `color2` for the second), its colours turned round to that hue
    /// ([`crate::saber_skins::Chroma`]). The others keep the colours their file gives.
    pub(crate) chroma: bool,
    /// Whether a holocron may drop it (the hub's `droppable`, `docs/unlockables.md`,
    /// "Holocron shader drops"); the others are given by the team or come with a medal.
    pub(crate) droppable: bool,
    /// The edition it belongs to ([`crate::editions`]): the stamp of when it came.
    pub(crate) edition: &'static str,
    /// Its saber smith: who gave the idea, credited on its cards ("Saber smith: Sol"),
    /// even when it was made together.
    pub(crate) smith: &'static str,
    /// What it is, in a line.
    pub(crate) description: &'static str,
    /// How a player gets it, in a line.
    pub(crate) how_to_get: &'static str,
}

impl Unlockable {
    /// Whether it is a blade skin.
    pub(crate) const fn is_blade_skin(&self) -> bool {
        matches!(self.kind, Kind::BladeSkin)
    }
}

/// Every unlockable, in the hub's catalogue order (the order profiles list them in).
pub(crate) const ALL: [Unlockable; 51] = [
    Unlockable {
        id: "saber_sun",
        name: "Sun blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A white-gold core in an orange corona: flares, prominences arching off it, sparks, a glint at its tip and heat haze.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_storm",
        name: "Storm blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A white-blue core crackling with lightning that leaps off it, sparks at its tip.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_void",
        name: "Void blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A dark core in a violet rim, star specks drifting into it, and a deep hum.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_frost",
        name: "Frost blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A pale ice core with a pointed tip, frost shards trailing it, and a chiming hum.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_prism",
        name: "Prism blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A white core in a corona whose colours run through the rainbow, slowly turning.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_unstable",
        name: "Unstable blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A raw red core that crackles and sputters, its edge ragged and its length faltering.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_molten",
        name: "Molten blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A dark rock core split by glowing lava cracks, embers dripping off it and falling.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_spectral",
        name: "Spectral blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A pale, see-through blade: wisps rise off it, souls drift round it, an echo sways beside it and afterimages trail its swings.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_glitch",
        name: "Glitch blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A blade whose colours split apart, blocks of it jumping sideways now and then.",
        how_to_get: "Comes with the Bug Hunter medal, for as long as you hold it.",
    },
    Unlockable {
        id: "saber_hologram",
        name: "Hologram blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A wireframe projection with scan lines running along it, jittering like a training hologram.",
        how_to_get: "Comes with the Early Tester medal, for as long as you hold it.",
    },
    Unlockable {
        id: "saber_runic",
        name: "Runic blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A gold core with glyphs scrolling along it that spell its wearer's name.",
        how_to_get: "Comes with the Early Contributor medal, for as long as you hold it.",
    },
    Unlockable {
        id: "saber_chameleon",
        name: "Chameleon blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A blade that takes the colour of the light where it is, changing from map to map.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_banner",
        name: "Banner blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: false,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "Your team's colour in team games, rippling like a flag; silver outside them.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_heartbeat",
        name: "Heartbeat blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: true,
        droppable: false,
        edition: crate::editions::ALPHA,
        smith: SOL,
        description: "A warm, deep blade that pulses with a slow double heartbeat, its light beating with it.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_tidal",
        name: "Tidal blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A teal blade with a wave of brighter light surging from hilt to tip on a slow loop.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_circuit",
        name: "Circuit blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A thin cyan blade with sharp data pulses racing along it.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_aurora",
        name: "Aurora blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Slow ribbons of green, teal and violet drifting along the blade.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_echo",
        name: "Echo blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A sky-blue blade with a bright ring rippling along it and fading away.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_nebula",
        name: "Nebula blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Deep indigo with tiny stars twinkling inside, a slice of galaxy.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_cinder",
        name: "Cinder blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A dull coal-orange blade that flickers and sheds glowing embers.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_venom",
        name: "Venom blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Acid green with droplets that bead up and drip off the blade.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_sakura",
        name: "Sakura blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A soft pink blade shedding blossom petals that drift away.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_firefly",
        name: "Firefly blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A soft warm-green blade with fireflies blinking around it.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_slate",
        name: "Slate blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A cool steel-grey blade with a faint, slow shimmer, plain and sturdy.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_amber",
        name: "Amber blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A warm honey-gold blade with a soft glow that swells and settles.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_lilac",
        name: "Lilac blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A gentle lilac blade whose colour drifts a little as it moves.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_mint",
        name: "Mint blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A pale, fresh green blade with a cool wave of light running along it.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_coral",
        name: "Coral blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A salmon-pink blade with a warm pulse travelling from hilt to tip.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_ivory",
        name: "Ivory blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Common,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A creamy, near-white blade with a soft golden fringe.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_glacier",
        name: "Glacier blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Ice-blue with tiny crystals of frost glittering inside.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_dusk",
        name: "Dusk blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Orange sinking into violet, the colour of the sky after sunset, slowly turning.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_ruby",
        name: "Ruby blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A deep red blade shedding bright sparks as it burns.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_amethyst",
        name: "Amethyst blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Rich purple with tiny flecks of light twinkling in the crystal.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_jade",
        name: "Jade blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A calm sea-green blade whose hue slowly shifts, with specks of light inside.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_moonlight",
        name: "Moonlight blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Silver-blue with pale motes drifting along it like dust in moonlight.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_neon",
        name: "Neon blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: true,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A glass neon tube, gas glowing inside, one of its six sections stuttering out now and then like an old bar sign.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_peppermint",
        name: "Peppermint blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Uncommon,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A candy cane: red and white stripes winding up the blade, sugar glinting off it.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_bubble",
        name: "Soap Bubble blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Soap film: rainbow swirls sliding round a nearly clear blade, bubbles drifting off it.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_lavalamp",
        name: "Lava Lamp blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A violet glass column where glowing wax rises in blobs, merging and splitting on the way.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_reactor",
        name: "Reactor blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Plasma squeezed by containment rings marching up the blade; now and then it overheats orange and vents sparks.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_bleeding",
        name: "Bleeding Kyber blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A pure white blade fighting a red corruption that cracks its way up from the hilt and is pushed back.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_sketch",
        name: "Sketch blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Rare,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A blade drawn in pencil: wobbly outlines and hatching redrawn eight times a second, like a flipbook.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_hourglass",
        name: "Hourglass blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Two glass chambers pinched at the middle, glowing sand pouring through until it runs out and flips.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_fiber",
        name: "Fiber Optic blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A bundle of glass fibres fanning out from the hilt, each ending in its own slowly changing point of light.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_serpent",
        name: "Serpent blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A golden serpent coiled round an emerald blade, slithering in place, its head by the tip hissing now and then.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_monarch",
        name: "Monarch blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A warm amber blade where monarch butterflies rest, open their wings and now and then fly off and come back.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_phoenix",
        name: "Phoenix blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "Living flame rolling up the blade, burning feathers falling as ash, wings of fire unfolding from the hilt.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_quantum",
        name: "Quantum blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A blade in superposition: ghost copies at other angles drift apart and collapse back into it with a flash.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_comic",
        name: "Comic blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: true,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A cel-shaded blade: flat colour, a thick black outline and its glow printed in halftone dots. It takes your saber colour.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_quicksilver",
        name: "Quicksilver blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Legendary,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A blade of liquid mirror: ripples run up its chrome, beads roll to the tip and drip off; it barely glows.",
        how_to_get: "Given by the SJK team.",
    },
    Unlockable {
        id: "saber_obsidian",
        name: "Obsidian Edge blade",
        kind: Kind::BladeSkin,
        tier: Rarity::Mythical,
        chroma: false,
        droppable: true,
        edition: crate::editions::ALPHA,
        smith: CREYON,
        description: "A flat, pointed blade of black volcanic glass with a white-hot edge crackling along it.",
        how_to_get: "Given by the SJK team.",
    },
];

/// The unlockable `id` names (spaces round it and case ignored), if this client knows it.
pub(crate) fn find(id: &str) -> Option<&'static Unlockable> {
    let id = id.trim();
    ALL.iter()
        .find(|unlockable| unlockable.id.eq_ignore_ascii_case(id))
}

/// The blade skin `id` names, if this client knows one by it.
pub(crate) fn blade_skin(id: &str) -> Option<&'static Unlockable> {
    find(id).filter(|unlockable| unlockable.is_blade_skin())
}

/// Whether the blade skin `id` names is a chroma ([`Unlockable::chroma`]).
pub(crate) fn is_chroma(id: &str) -> bool {
    blade_skin(id).is_some_and(|unlockable| unlockable.chroma)
}

/// The blade skins, in catalogue order.
pub(crate) fn blade_skins() -> impl Iterator<Item = &'static Unlockable> {
    ALL.iter().filter(|unlockable| unlockable.is_blade_skin())
}

/// The blade skins as players see them listed: the rarest tier first, then in
/// catalogue order within a tier.
pub(crate) fn blade_skins_by_tier() -> &'static [&'static Unlockable] {
    static ORDER: std::sync::OnceLock<Vec<&'static Unlockable>> = std::sync::OnceLock::new();
    ORDER.get_or_init(|| {
        let mut skins: Vec<_> = blade_skins().collect();
        // A stable sort keeps the catalogue's order within a tier.
        skins.sort_by_key(|skin| std::cmp::Reverse(skin.tier));
        skins
    })
}

/// Wear blade skin `id`, `""` for the stock blade: what the Collection's Equip and
/// Unequip and the Saber tab's blade choice do.
pub(crate) fn wear(console: &mut crate::console::ViewerConsole, id: &str) {
    console.set_cvar(SABER_SKIN_CVAR, id);
}

/// What this client knows of the unlocks the player's own hub profile lists, by the
/// same rule as the gate on what they wear (`player_identity::owns_unlock`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Holdings<'a> {
    /// The identity is off: unlockables need it.
    IdentityOff,
    /// No hub is set (`cl_hubUrl`).
    NoHub,
    /// The hub has not answered yet, or cannot be reached.
    Waiting,
    /// The own profile's unlocks.
    Known(&'a [sjk_identity::Unlock]),
}

impl<'a> Holdings<'a> {
    /// From `cl_identity` (`enabled`) and the identity service's snapshot.
    pub(crate) fn of(enabled: bool, snapshot: Option<&'a sjk_identity::Snapshot>) -> Self {
        use sjk_identity::Status;
        let Some(snapshot) = snapshot.filter(|_| enabled) else {
            return Self::IdentityOff;
        };
        match (&snapshot.status, &snapshot.me) {
            (Status::Disabled, _) => Self::IdentityOff,
            (Status::NoHub, _) => Self::NoHub,
            (_, Some(me)) => Self::Known(&me.unlocks),
            (_, None) => Self::Waiting,
        }
    }

    /// The grant of `id` the profile lists, if it lists it.
    pub(crate) fn unlock(&self, id: &str) -> Option<&'a sjk_identity::Unlock> {
        match self {
            Self::Known(unlocks) => unlocks.iter().find(|unlock| unlock.id == id),
            _ => None,
        }
    }

    /// Whether the player may wear `unlockable`: a blade skin their profile lists. The
    /// Collection's Equip and the Saber tab's blade choice offer these.
    pub(crate) fn can_wear(&self, unlockable: &Unlockable) -> bool {
        unlockable.is_blade_skin() && self.unlock(unlockable.id).is_some()
    }

    /// The blade skin the player wears: the one `setting` (`cg_saberSkin`) names, while
    /// their profile lists it; `None` for the stock blade.
    pub(crate) fn worn_blade_skin(&self, setting: &str) -> Option<&'static Unlockable> {
        blade_skin(setting).filter(|skin| self.can_wear(skin))
    }

    /// Why what the player holds is not known, in a sentence; `None` once it is.
    pub(crate) fn reason(&self) -> Option<&'static str> {
        match self {
            Self::IdentityOff => {
                Some("Unlockables need the SJK identity: switch it on in Identity settings.")
            }
            Self::NoHub => Some("No hub is set: unlockables are kept on the SJK hub (cl_hubUrl)."),
            Self::Waiting => Some("Waiting for the SJK hub to answer."),
            Self::Known(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_chromas_are_the_skins_drawn_in_any_colour() {
        let chromas: Vec<_> = super::blade_skins()
            .filter(|unlockable| unlockable.chroma)
            .map(|unlockable| unlockable.id)
            .collect();
        assert_eq!(
            chromas,
            [
                "saber_storm",
                "saber_unstable",
                "saber_spectral",
                "saber_glitch",
                "saber_hologram",
                "saber_runic",
                "saber_heartbeat",
                "saber_neon",
                "saber_comic",
            ]
        );
        assert!(super::is_chroma("SABER_GLITCH"));
        assert!(!super::is_chroma("saber_sun"));
        assert!(!super::is_chroma("saber_nothing"));
    }

    use super::*;

    /// Creyon's "Saber ideas" page gave the 21 plain ones (14/10/2026) and his Blade Lab
    /// page the ones after them (15/10/2026); Sol the others.
    #[test]
    fn every_shader_names_its_saber_smith() {
        let creyon: Vec<_> = blade_skins()
            .filter(|skin| skin.smith == CREYON)
            .map(|skin| skin.id)
            .collect();
        assert_eq!(
            creyon,
            [
                "saber_tidal",
                "saber_circuit",
                "saber_aurora",
                "saber_echo",
                "saber_nebula",
                "saber_cinder",
                "saber_venom",
                "saber_sakura",
                "saber_firefly",
                "saber_slate",
                "saber_amber",
                "saber_lilac",
                "saber_mint",
                "saber_coral",
                "saber_ivory",
                "saber_glacier",
                "saber_dusk",
                "saber_ruby",
                "saber_amethyst",
                "saber_jade",
                "saber_moonlight",
                "saber_neon",
                "saber_peppermint",
                "saber_bubble",
                "saber_lavalamp",
                "saber_reactor",
                "saber_bleeding",
                "saber_sketch",
                "saber_hourglass",
                "saber_fiber",
                "saber_serpent",
                "saber_monarch",
                "saber_phoenix",
                "saber_quantum",
                "saber_comic",
                "saber_quicksilver",
                "saber_obsidian",
            ]
        );
        assert!(blade_skins().all(|skin| [SOL, CREYON].contains(&skin.smith)));
    }

    /// Sol's tiers (10/10/2026); Prism, Glitch and Banner are placed provisionally.
    #[test]
    fn every_shader_has_its_tier_and_the_rarest_list_first() {
        let of = |rarity: Rarity| -> Vec<&str> {
            blade_skins()
                .filter(|skin| skin.tier == rarity)
                .map(|skin| skin.id)
                .collect()
        };
        assert_eq!(
            of(Rarity::Mythical),
            [
                "saber_sun",
                "saber_void",
                "saber_molten",
                "saber_spectral",
                "saber_reactor",
                "saber_bleeding",
                "saber_serpent",
                "saber_monarch",
                "saber_phoenix",
                "saber_quantum",
                "saber_obsidian",
            ]
        );
        assert_eq!(
            of(Rarity::Legendary),
            [
                "saber_storm",
                "saber_prism",
                "saber_unstable",
                "saber_chameleon",
                "saber_hourglass",
                "saber_fiber",
                "saber_comic",
                "saber_quicksilver",
            ]
        );
        assert_eq!(
            of(Rarity::Rare),
            [
                "saber_frost",
                "saber_glitch",
                "saber_hologram",
                "saber_runic",
                "saber_banner",
                "saber_heartbeat",
                "saber_bubble",
                "saber_lavalamp",
                "saber_sketch",
            ]
        );
        assert_eq!(
            of(Rarity::Uncommon),
            [
                "saber_nebula",
                "saber_cinder",
                "saber_venom",
                "saber_sakura",
                "saber_firefly",
                "saber_glacier",
                "saber_dusk",
                "saber_ruby",
                "saber_amethyst",
                "saber_jade",
                "saber_moonlight",
                "saber_neon",
                "saber_peppermint",
            ]
        );
        assert_eq!(
            of(Rarity::Common),
            [
                "saber_tidal",
                "saber_circuit",
                "saber_aurora",
                "saber_echo",
                "saber_slate",
                "saber_amber",
                "saber_lilac",
                "saber_mint",
                "saber_coral",
                "saber_ivory",
            ]
        );
        let order = blade_skins_by_tier();
        assert_eq!(order.len(), blade_skins().count());
        assert!(order.windows(2).all(|pair| pair[0].tier >= pair[1].tier));
        assert_eq!(order[0].id, "saber_sun");
        assert_eq!(order[11].id, "saber_storm");
        // The holocrons' names and colours.
        assert_eq!(Rarity::Mythical.label(), "Mythical");
        assert_eq!(Rarity::Uncommon.label(), "Uncommon");
        assert_eq!(Rarity::Common.label(), "Common");
        assert_eq!(Rarity::Rare.colour(), crate::holocrons::TIERS[1].colour);
        assert_eq!(Rarity::Common.symbol(), crate::symbols::RARITY[0]);
        assert_eq!(Rarity::Mythical.symbol(), crate::symbols::RARITY[4]);
    }

    #[test]
    fn ids_are_unique_and_every_blade_skin_has_one_entry() {
        for (index, unlockable) in ALL.iter().enumerate() {
            assert!(
                ALL[index + 1..]
                    .iter()
                    .all(|other| !other.id.eq_ignore_ascii_case(unlockable.id)),
                "{} is listed twice",
                unlockable.id
            );
            assert!(
                unlockable
                    .id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{} is a hub id",
                unlockable.id
            );
            assert!(!unlockable.name.is_empty() && !unlockable.description.is_empty());
            assert!(unlockable.how_to_get.ends_with('.'));
        }
        assert_eq!(
            blade_skins().count(),
            ALL.iter().filter(|u| u.kind == Kind::BladeSkin).count()
        );
    }

    #[test]
    fn holdings_follow_the_identity_and_the_own_profile() {
        use sjk_identity::{Profile, Snapshot, Status, Unlock};
        let unlock = Unlock {
            id: "saber_sun".into(),
            granted: 1_791_336_225,
            note: "Thanks".into(),
            count: 1,
            medal: None,
        };
        let me = Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "0123456789abcdef".into(),
            key: String::new(),
            name: "Sol".into(),
            bio: String::new(),
            verified: false,
            staff: false,
            created: 0,
            names: Vec::new(),
            medals: Vec::new(),
            achievements: Vec::new(),
            unlocks: vec![unlock.clone()],
            avatar: String::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        };
        let snapshot = |status, me| Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            local_key_id: "0123456789abcdef".into(),
            status,
            key_id: "0123456789abcdef".into(),
            me,
            server: None,
            players: Vec::new(),
            profiles: std::collections::HashMap::new(),
            model_checks: std::collections::HashMap::new(),
            profiles_revision: 0,
            notice: None,
            revision: 0,
            report: None,
            note: None,
            player_report: None,
            avatar: None,
            look_outcome: None,
            packs_revision: 0,
            assets_note: None,
            crashes_sent: 0,
            crash_note: None,
            holocrons: None,
            log_requests: Vec::new(),
            log_answer: None,
        };
        let online = snapshot(Status::Online, Some(me));
        assert_eq!(Holdings::of(false, Some(&online)), Holdings::IdentityOff);
        assert_eq!(Holdings::of(true, None), Holdings::IdentityOff);
        let known = Holdings::of(true, Some(&online));
        assert_eq!(known.unlock("saber_sun"), Some(&unlock));
        assert_eq!(known.unlock("saber_moon"), None);
        assert_eq!(known.reason(), None);
        let waiting = snapshot(Status::Registering, None);
        assert_eq!(Holdings::of(true, Some(&waiting)), Holdings::Waiting);
        let no_hub = snapshot(Status::NoHub, None);
        assert_eq!(Holdings::of(true, Some(&no_hub)), Holdings::NoHub);
        assert!(
            Holdings::IdentityOff
                .reason()
                .unwrap()
                .contains("SJK identity")
        );
        assert_eq!(Holdings::Waiting.unlock("saber_sun"), None);
    }

    /// The hub's catalogue (`src/unlocks.rs` in Sol-Vulpes/SJK-hub) lists exactly these, in
    /// this order and with these names; its test pins the same list, so a change to one
    /// side fails until the other follows.
    #[test]
    fn the_catalogue_matches_the_hubs() {
        let listed: Vec<(&str, &str)> = ALL.iter().map(|u| (u.id, u.name)).collect();
        assert_eq!(
            listed,
            [
                ("saber_sun", "Sun blade"),
                ("saber_storm", "Storm blade"),
                ("saber_void", "Void blade"),
                ("saber_frost", "Frost blade"),
                ("saber_prism", "Prism blade"),
                ("saber_unstable", "Unstable blade"),
                ("saber_molten", "Molten blade"),
                ("saber_spectral", "Spectral blade"),
                ("saber_glitch", "Glitch blade"),
                ("saber_hologram", "Hologram blade"),
                ("saber_runic", "Runic blade"),
                ("saber_chameleon", "Chameleon blade"),
                ("saber_banner", "Banner blade"),
                ("saber_heartbeat", "Heartbeat blade"),
                ("saber_tidal", "Tidal blade"),
                ("saber_circuit", "Circuit blade"),
                ("saber_aurora", "Aurora blade"),
                ("saber_echo", "Echo blade"),
                ("saber_nebula", "Nebula blade"),
                ("saber_cinder", "Cinder blade"),
                ("saber_venom", "Venom blade"),
                ("saber_sakura", "Sakura blade"),
                ("saber_firefly", "Firefly blade"),
                ("saber_slate", "Slate blade"),
                ("saber_amber", "Amber blade"),
                ("saber_lilac", "Lilac blade"),
                ("saber_mint", "Mint blade"),
                ("saber_coral", "Coral blade"),
                ("saber_ivory", "Ivory blade"),
                ("saber_glacier", "Glacier blade"),
                ("saber_dusk", "Dusk blade"),
                ("saber_ruby", "Ruby blade"),
                ("saber_amethyst", "Amethyst blade"),
                ("saber_jade", "Jade blade"),
                ("saber_moonlight", "Moonlight blade"),
                ("saber_neon", "Neon blade"),
                ("saber_peppermint", "Peppermint blade"),
                ("saber_bubble", "Soap Bubble blade"),
                ("saber_lavalamp", "Lava Lamp blade"),
                ("saber_reactor", "Reactor blade"),
                ("saber_bleeding", "Bleeding Kyber blade"),
                ("saber_sketch", "Sketch blade"),
                ("saber_hourglass", "Hourglass blade"),
                ("saber_fiber", "Fiber Optic blade"),
                ("saber_serpent", "Serpent blade"),
                ("saber_monarch", "Monarch blade"),
                ("saber_phoenix", "Phoenix blade"),
                ("saber_quantum", "Quantum blade"),
                ("saber_comic", "Comic blade"),
                ("saber_quicksilver", "Quicksilver blade"),
                ("saber_obsidian", "Obsidian Edge blade"),
            ]
        );
        assert!(ALL.iter().all(Unlockable::is_blade_skin));
        // The blade skins fit the renderer's slots, every one loaded at once.
        assert!(blade_skins().count() <= crate::saber_skins::MAX_SKINS);
    }

    #[test]
    fn ids_are_found_whatever_the_case_and_spaces() {
        assert_eq!(find("saber_sun").map(|u| u.name), Some("Sun blade"));
        assert_eq!(blade_skin(" SABER_Sun ").map(|u| u.id), Some("saber_sun"));
        assert!(blade_skin("saber_sun").is_some_and(Unlockable::is_blade_skin));
        assert!(find("saber_moon").is_none());
        assert!(find("").is_none());
    }
}
