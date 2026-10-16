//! Display names of Jedi Academy NPC classes for overhead tags. The wire carries
//! only the `class_t` number (`teams.h`), never a name.

/// Names indexed by `class_t`; empty for classes that never get a tag.
const NAMES: [&str; 56] = [
    "",
    "AT-ST",
    "Bartender",
    "Bespin Cop",
    "Claw Monster",
    "Commando",
    "Desann",
    "Fish",
    "Flier",
    "Galak",
    "Glider",
    "Gonk Droid",
    "Gran",
    "Howler",
    "Imperial",
    "Imperial Worker",
    "Interrogator Droid",
    "Jan",
    "Jedi",
    "Kyle",
    "Lando",
    "Lizard",
    "Luke",
    "Mark I Droid",
    "Mark II Droid",
    "Galak Mech",
    "Mine Monster",
    "Mon Mothma",
    "Morgan Katarn",
    "Mouse Droid",
    "Murjj",
    "Prisoner",
    "Probe Droid",
    "Protocol Droid",
    "R2-D2",
    "R5-D2",
    "Rebel",
    "Reborn",
    "Reelo",
    "Remote",
    "Rodian",
    "Seeker Droid",
    "Sentry Droid",
    "Shadowtrooper",
    "Stormtrooper",
    "Swamp Creature",
    "Swamp Trooper",
    "Tavion",
    "Trandoshan",
    "Ugnaught",
    "Jawa",
    "Weequay",
    "Boba Fett",
    "",
    "Rancor",
    "Wampa",
];

/// Name for `class`, or `None` for no class, vehicles and unknown values.
pub(super) fn name(class: u8) -> Option<&'static str> {
    NAMES
        .get(usize::from(class))
        .copied()
        .filter(|name| !name.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_class_numbers_the_server_uses() {
        // Same values as sjk-game-jka's `class` constants (teams.h:40-97).
        assert_eq!(name(1), Some("AT-ST"));
        assert_eq!(name(6), Some("Desann"));
        assert_eq!(name(18), Some("Jedi"));
        assert_eq!(name(44), Some("Stormtrooper"));
        assert_eq!(name(47), Some("Tavion"));
        assert_eq!(name(52), Some("Boba Fett"));
        assert_eq!(name(54), Some("Rancor"));
    }

    #[test]
    fn no_class_vehicles_and_unknown_numbers_have_no_tag() {
        assert_eq!(name(0), None);
        assert_eq!(name(53), None);
        assert_eq!(name(200), None);
    }
}
