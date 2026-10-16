//! `debug_collectibles`: the SJK team see every collectible as held, on this PC only, to
//! try them out without granting and taking them back at the hub (which would leave
//! those grants in its records and logs).
//!
//! It dresses a copy of the player's own profile (`Snapshot::me`) with every saber
//! shader, medal, achievement and card the client knows, for the screens that show what
//! the player holds: the Collection, the Profile, the Saber tab, the `saberskin` command
//! and the own blade drawn in the world. Nothing else reads it: the hub keeps the real
//! profile, the look sent to it stays what the player really holds (it refuses a skin
//! they lack), so other players see that; the new medal, achievement and holocron
//! pop-ups and the Staff page read the real profile too, so the seen lists and staff
//! actions are untouched. It only works while the hub says the player's key is staff.

use sjk_identity::{Snapshot, Status};

/// The switch: 1 shows every collectible as held (staff only), 0 the real profile.
pub(crate) const CVAR: &str = "debug_collectibles";

/// The note on every shader and medal the mode adds, which their cards show.
pub(crate) const NOTE: &str = "Debug collectibles: shown as yours on this PC only.";

/// Whether `grant` is one the mode added.
pub(crate) fn added(grant: &sjk_identity::Unlock) -> bool {
    grant.note == NOTE
}

/// Whether `debug_collectibles` is set on `console`.
pub(crate) fn wanted(console: &crate::console::ViewerConsole) -> bool {
    console.bool_cvar(CVAR) == Some(true)
}

/// Whether the mode applies to `snapshot`: `wanted`, the hub answered and says the
/// player's key is staff.
pub(crate) fn active(wanted: bool, snapshot: &Snapshot) -> bool {
    wanted
        && !matches!(snapshot.status, Status::Disabled | Status::NoHub)
        && snapshot.me.as_ref().is_some_and(|me| me.staff)
}

/// `snapshot` with every collectible held when the mode applies to it ([`active`]);
/// what the profile really holds keeps its dates, notes and counts.
pub(crate) fn dressed(wanted: bool, mut snapshot: Snapshot) -> Snapshot {
    if !active(wanted, &snapshot) {
        return snapshot;
    }
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(0));
    let faces = crate::cards::faces::current();
    if let Some(me) = snapshot.me.as_mut() {
        dress(me, &faces.list, now);
    }
    snapshot
}

/// Give `me` every unlockable, medal and achievement the client knows and one copy of
/// each card of `faces` in every finish, as held at `now`.
fn dress(me: &mut sjk_identity::Profile, faces: &[crate::cards::faces::Face], now: i64) {
    for unlockable in &crate::unlockables::ALL {
        if !me.unlocks.iter().any(|unlock| unlock.id == unlockable.id) {
            me.unlocks.push(sjk_identity::Unlock {
                id: unlockable.id.to_owned(),
                granted: now,
                note: NOTE.to_owned(),
                count: 1,
                medal: None,
            });
        }
    }
    for medal in crate::medals::Medal::ALL {
        if !me.medals.iter().any(|held| held.id == medal.id()) {
            me.medals.push(sjk_identity::Medal {
                id: medal.id().to_owned(),
                count: 1,
                awarded: now,
                note: NOTE.to_owned(),
            });
        }
    }
    for kind in &crate::achievements::ALL {
        match me.achievements.iter_mut().find(|held| held.id == kind.id) {
            Some(held) if held.unlocked > 0 => {}
            Some(held) => {
                held.progress = kind.goal;
                held.goal = kind.goal;
                held.unlocked = now;
            }
            None => me.achievements.push(sjk_identity::Achievement {
                id: kind.id.to_owned(),
                progress: kind.goal,
                goal: kind.goal,
                unlocked: now,
            }),
        }
    }
    for face in faces {
        for finish in &crate::cards::FINISHES {
            let held = me.cards.iter().any(|stack| {
                stack.card.as_deref() == Some(face.id.as_str()) && stack.finish == finish.id
            });
            if !held {
                me.cards.push(sjk_identity::CardStack {
                    card: Some(face.id.clone()),
                    rarity: face.rarity.id.to_owned(),
                    finish: finish.id.to_owned(),
                    edition: crate::editions::ALPHA.to_owned(),
                    count: 1,
                    first: now,
                    last: now,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(staff: bool) -> Snapshot {
        let me = sjk_identity::Profile {
            staff,
            unlocks: vec![sjk_identity::Unlock {
                id: "saber_sun".to_owned(),
                granted: 7,
                note: "kept".to_owned(),
                count: 3,
                medal: None,
            }],
            ..crate::console::profile_panel::tests::me("")
        };
        crate::console::profile_panel::tests::snapshot(Some(me), None)
    }

    #[test]
    fn staff_who_want_it_hold_everything_and_keep_what_they_had() {
        let dressed = dressed(true, snapshot(true));
        let me = dressed.me.as_ref().expect("a profile");
        assert_eq!(me.unlocks.len(), crate::unlockables::ALL.len());
        let sun = me.unlocks.iter().find(|unlock| unlock.id == "saber_sun");
        assert_eq!(sun.map(|sun| (sun.granted, sun.count)), Some((7, 3)));
        assert_eq!(me.medals.len(), crate::medals::Medal::COUNT);
        assert_eq!(me.achievements.len(), crate::achievements::ALL.len());
        assert!(me.achievements.iter().all(|held| held.unlocked > 0));
        let holdings = crate::unlockables::Holdings::of(true, Some(&dressed));
        assert!(crate::unlockables::blade_skins().all(|skin| holdings.can_wear(skin)));
    }

    #[test]
    fn cards_come_in_every_finish() {
        let mut me = crate::console::profile_panel::tests::me("");
        let face = crate::cards::faces::Face {
            id: "sol_001".to_owned(),
            name: "Sol".to_owned(),
            set: "sol_lore".to_owned(),
            set_name: "Sol lore".to_owned(),
            number: 1,
            kind: crate::cards::faces::Kind::Game,
            rarity: &crate::cards::RARITIES[2],
            lore: String::new(),
            artist: "Sol".to_owned(),
            art: String::new(),
        };
        dress(&mut me, std::slice::from_ref(&face), 5);
        let finishes: Vec<_> = me.cards.iter().map(|stack| stack.finish.as_str()).collect();
        assert_eq!(finishes, ["normal", "holo", "solar"]);
        assert!(me.cards.iter().all(|stack| stack.rarity == "rare"));
    }

    #[test]
    fn only_staff_who_want_it_and_a_hub_that_answered() {
        for (wanted, staff) in [(false, true), (true, false)] {
            let dressed = dressed(wanted, snapshot(staff));
            assert_eq!(dressed.me.as_ref().map(|me| me.unlocks.len()), Some(1));
        }
        let mut no_hub = snapshot(true);
        no_hub.status = Status::NoHub;
        assert_eq!(dressed(true, no_hub).me.map(|me| me.unlocks.len()), Some(1));
    }
}
