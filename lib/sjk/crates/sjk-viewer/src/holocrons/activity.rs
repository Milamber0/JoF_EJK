//! What counts as actively playing, which every claim to the hub says (`docs/holocrons.md`,
//! "Earning"): the player is in a live, non-local game, in their own view (not spectating
//! or following someone), not at the intermission or the scoreboard freeze, not in a
//! menu, and had input (a key, a mouse button or wheel, or the mouse moving) in the last
//! two minutes. Only then does the hub count the time toward the next holocron.
//!
//! The viewer calls [`input`] from its window events and [`Play::active`] twice a second,
//! with the identity's turn (`identity_frame.rs`). Both cost an atomic store or a few
//! comparisons and allocate nothing.

use sjk_protocol::PlayerState;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How long after the last input the player still counts as playing.
pub(crate) const IDLE_AFTER: Duration = Duration::from_secs(120);
/// `PMF_FOLLOW`: the player state is that of the player being followed.
const PMF_FOLLOW: u16 = 0x1000;
/// `PM_INTERMISSION` and `PM_SPINTERMISSION`.
const PM_INTERMISSION: [u8; 2] = [7, 8];

/// When the clock below began.
static EPOCH: OnceLock<Instant> = OnceLock::new();
/// Milliseconds after [`EPOCH`] of the last input, plus one; 0 before any.
static LAST_INPUT: AtomicU64 = AtomicU64::new(0);

/// Note an input now (a key or mouse button pressed, the wheel, the mouse moved).
pub(crate) fn input() {
    input_at(Instant::now());
}

fn input_at(now: Instant) {
    let epoch = *EPOCH.get_or_init(|| now);
    let millis = now.saturating_duration_since(epoch).as_millis() as u64;
    LAST_INPUT.store(millis + 1, Ordering::Relaxed);
}

/// How long before `now` the last input was; `None` when there has been none.
pub(crate) fn since_input(now: Instant) -> Option<Duration> {
    let last = LAST_INPUT.load(Ordering::Relaxed).checked_sub(1)?;
    let epoch = *EPOCH.get()?;
    let at = epoch + Duration::from_millis(last);
    Some(now.saturating_duration_since(at))
}

/// Where the player stands, as far as earning goes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Play {
    /// Connected to a game server of someone else's: not a local game, a demo or the
    /// menus alone.
    pub(crate) remote: bool,
    /// The snapshot shows the player's own view and not a spectator's or a follower's.
    pub(crate) own_view: bool,
    /// At the intermission (the scoreboard freeze).
    pub(crate) intermission: bool,
    /// A menu, the console or a dialog has the keyboard.
    pub(crate) in_menu: bool,
}

impl Play {
    /// What `player` (the live snapshot's player state) shows, for the game's `client`
    /// number: the own view is the one whose client number is the player's and which
    /// does not follow, and a spectator is not playing.
    pub(crate) fn from_state(
        remote: bool,
        player: &PlayerState,
        client: i32,
        in_menu: bool,
    ) -> Self {
        let own_view = i32::from(player.client_num()) == client
            && player.movement_flags() & PMF_FOLLOW == 0
            && !player.is_spectator();
        Self {
            remote,
            own_view,
            intermission: PM_INTERMISSION.contains(&player.movement_type()),
            in_menu,
        }
    }

    /// Whether the player is actively playing: in a game of someone else's, in their own
    /// view, mid-match, out of the menus, and `since_input` (how long ago their last input
    /// was; `None` for none yet) within [`IDLE_AFTER`].
    pub(crate) fn active(&self, since_input: Option<Duration>) -> bool {
        self.remote
            && self.own_view
            && !self.intermission
            && !self.in_menu
            && since_input.is_some_and(|since| since <= IDLE_AFTER)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn playing() -> Play {
        Play {
            remote: true,
            own_view: true,
            intermission: false,
            in_menu: false,
        }
    }

    #[test]
    fn only_a_player_with_recent_input_in_a_live_match_is_active() {
        let recent = Some(Duration::from_secs(5));
        assert!(playing().active(recent));
        assert!(playing().active(Some(IDLE_AFTER)), "two minutes exactly");
        assert!(!playing().active(Some(IDLE_AFTER + Duration::from_millis(1))));
        assert!(!playing().active(None), "no input yet");
        for (name, play) in [
            (
                "a local game",
                Play {
                    remote: false,
                    ..playing()
                },
            ),
            (
                "spectating or following",
                Play {
                    own_view: false,
                    ..playing()
                },
            ),
            (
                "the intermission",
                Play {
                    intermission: true,
                    ..playing()
                },
            ),
            (
                "a menu",
                Play {
                    in_menu: true,
                    ..playing()
                },
            ),
            ("nothing", Play::default()),
        ] {
            assert!(!play.active(recent), "{name}");
        }
    }

    fn state(configure: impl FnOnce(&mut PlayerState)) -> PlayerState {
        let mut player = PlayerState::default();
        configure(&mut player);
        player
    }

    /// The player's own view, mid-match, is playing; following, spectating, the
    /// intermission and a view of someone else's client number are not.
    #[test]
    fn play_is_read_from_the_snapshots_player_state() {
        let me = 3;
        let playing = state(|player| {
            player.set_raw_field(43, me as u32);
        });
        let read = |player: &PlayerState| Play::from_state(true, player, me, false);
        assert_eq!(
            read(&playing),
            Play {
                remote: true,
                own_view: true,
                intermission: false,
                in_menu: false
            }
        );
        // Following (PMF_FOLLOW), or a view of another client.
        let following = state(|player| {
            player.set_raw_field(43, me as u32);
            player.set_raw_field(38, 0x1000);
        });
        assert!(!read(&following).own_view);
        let other = state(|player| {
            player.set_raw_field(43, 5);
        });
        assert!(!read(&other).own_view);
        // A spectator: PM_SPECTATOR.
        let spectator = state(|player| {
            player.set_raw_field(43, me as u32);
            player.set_raw_field(63, 4);
        });
        assert!(!read(&spectator).own_view);
        // The intermission, either kind.
        for movement in [7, 8] {
            let frozen = state(|player| {
                player.set_raw_field(43, me as u32);
                player.set_raw_field(63, movement);
            });
            assert!(read(&frozen).intermission, "{movement}");
            assert!(!read(&frozen).active(Some(Duration::ZERO)));
        }
        // A menu, a local game.
        assert!(!Play::from_state(true, &playing, me, true).active(Some(Duration::ZERO)));
        assert!(!Play::from_state(false, &playing, me, false).active(Some(Duration::ZERO)));
        assert!(Play::from_state(true, &playing, me, false).active(Some(Duration::ZERO)));
        // No client number yet (negative): nobody's view is the player's own.
        assert!(!Play::from_state(true, &playing, -1, false).own_view);
    }

    /// The input clock reads how long ago the last input was, and nothing before one.
    /// (The clock is the process's: this test makes the only input that counts here.)
    #[test]
    fn the_input_clock_counts_from_the_last_input() {
        let base = Instant::now();
        input_at(base);
        let later = base + Duration::from_secs(90);
        let since = since_input(later).expect("an input");
        assert!(since >= Duration::from_secs(89) && since <= Duration::from_secs(91));
        input_at(base + Duration::from_secs(80));
        let since = since_input(later).expect("an input");
        assert!(since <= Duration::from_secs(11), "{since:?}");
        // An input in the future of `now` (clocks read in a different order) is no gap.
        assert_eq!(since_input(base), Some(Duration::ZERO));
    }
}
