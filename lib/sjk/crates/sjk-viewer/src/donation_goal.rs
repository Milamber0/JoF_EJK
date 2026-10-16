//! The donation goal on the SJK UI's main page (`docs/sjk-ui.md`, "Donation goal"):
//! how much of SJK's monthly goal came in, a bar, and Donate, which opens SJK's Ko-fi
//! page in the browser. The numbers come from the SJK hub (`GET /v1/donations/goal`,
//! [`sjk_identity::DonationGoal`]), asked without the identity key on a thread of its
//! own, at most every ten minutes while the main page shows, and kept in memory: the
//! menu never waits for it. Without an answer the card shows Donate alone.
//! `ui_donationGoal 0` hides the card and asks nothing.
//!
//! Shelved since 14/10/2026 ([`SHELVED`]): no card, no setting, no Supporter medal
//! in the Collection until held. To bring them back, set it to `false` and put the
//! "Donation goal" row back in `settings/catalog.rs` (GAME), `settings/groups.rs`
//! (Interface) and its help line in `settings/help.rs` (in git history).

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use sjk_identity::DonationGoal;
use sjk_identity::donations::DEFAULT_URL;

/// Donations are shelved (Sol, 14/10/2026): the card, its setting and the unheld
/// Supporter medal stay hidden whatever `ui_donationGoal` says.
pub(crate) const SHELVED: bool = true;

/// The setting that shows the card (on by default).
pub(crate) const CVAR: &str = "ui_donationGoal";
/// How long an answer is kept before the hub is asked again.
const REFRESH: Duration = Duration::from_secs(10 * 60);
/// How long after a failed ask the hub is asked again.
const RETRY: Duration = Duration::from_secs(5 * 60);

/// What the card shows: the numbers when the hub answered.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Card {
    /// Raised and goal in cents, and whether the goal is monthly; `None` until the hub
    /// answered.
    pub(crate) numbers: Option<Numbers>,
}

/// The goal's numbers.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Numbers {
    pub(crate) raised_cents: i64,
    pub(crate) goal_cents: i64,
    pub(crate) monthly: bool,
    /// The goal is in euros (else the hub's currency code is not drawn: the card says
    /// the amounts without a sign).
    pub(crate) euros: bool,
}

impl Numbers {
    /// How full the bar is, 0 to 1.
    pub(crate) fn fill(self) -> f32 {
        (self.raised_cents as f64 / self.goal_cents.max(1) as f64).clamp(0.0, 1.0) as f32
    }
}

struct State {
    goal: Option<DonationGoal>,
    /// When the hub was last asked, and whether that ask is still going.
    asked: Option<Instant>,
    failed: bool,
    busy: bool,
}

static STATE: Mutex<State> = Mutex::new(State {
    goal: None,
    asked: None,
    failed: false,
    busy: false,
});

fn lock() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the player shows the card.
pub(crate) fn shown(console: Option<&crate::console::ViewerConsole>) -> bool {
    !SHELVED && console.is_none_or(|console| console.bool_cvar(CVAR) != Some(false))
}

/// The card this frame, `None` when it is hidden. Asks the hub again when the last
/// answer is old (on its own thread; never in tests).
pub(crate) fn card(console: Option<&crate::console::ViewerConsole>) -> Option<Card> {
    if !shown(console) {
        return None;
    }
    let url = console
        .and_then(|console| console.text_value("cl_hubUrl"))
        .unwrap_or_default()
        .trim();
    let mut state = lock();
    let wait = if state.failed { RETRY } else { REFRESH };
    let due = !state.busy && state.asked.is_none_or(|asked| asked.elapsed() >= wait);
    if due && !url.is_empty() && !cfg!(test) {
        state.busy = true;
        state.asked = Some(Instant::now());
        let url = url.to_owned();
        let spawned = std::thread::Builder::new()
            .name("donation-goal".to_owned())
            .spawn(move || fetch(&url));
        if spawned.is_err() {
            state.busy = false;
        }
    }
    Some(Card {
        numbers: state.goal.as_ref().map(numbers),
    })
}

fn numbers(goal: &DonationGoal) -> Numbers {
    Numbers {
        raised_cents: goal.raised_cents,
        goal_cents: goal.goal_cents,
        monthly: goal.monthly,
        euros: goal.currency == "EUR",
    }
}

fn fetch(url: &str) {
    use sjk_identity::Hub as _;
    let agent = format!("SJK/{}", crate::build_info::VERSION);
    let answer = sjk_identity::HttpHub::new(url, &agent).and_then(|mut hub| hub.donation_goal());
    let mut state = lock();
    state.busy = false;
    match answer {
        Ok(goal) => {
            state.goal = Some(goal);
            state.failed = false;
        }
        Err(error) => {
            state.failed = true;
            drop(state);
            crate::log::progress(format_args!("donation goal: not read ({error})"));
        }
    }
}

/// Show `goal` as if the hub had answered it (world shots and tests).
#[cfg(test)]
pub(crate) fn set_for_shot(goal: Option<DonationGoal>) {
    let mut state = lock();
    state.goal = goal;
    state.asked = Some(Instant::now());
}

/// Whether `url` is safe to hand to the system's opener: https, and only the characters
/// of a plain address (no `&`, `^`, quotes or spaces a shell would read).
fn safe_url(url: &str) -> bool {
    url.starts_with("https://")
        && url.len() <= 200
        && url
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~:/?#=%+".contains(&byte))
}

/// The page Donate opens: the hub's, when it is a plain https address, else SJK's Ko-fi.
pub(crate) fn url() -> String {
    lock()
        .goal
        .as_ref()
        .map(|goal| goal.url.as_str())
        .filter(|url| safe_url(url))
        .unwrap_or(DEFAULT_URL)
        .to_owned()
}

/// Open the donation page in the player's browser.
pub(crate) fn open() {
    let url = url();
    crate::log::progress(format_args!("donation goal: opening {url}"));
    if !cfg!(test) {
        crate::update::open_page(&url);
    }
}

/// `amount` cents as the card writes it: whole euros when round ("80 €"), else with
/// cents ("12.50 €").
pub(crate) struct Amount {
    pub(crate) cents: i64,
    pub(crate) euros: bool,
}

impl std::fmt::Display for Amount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let cents = self.cents.max(0);
        if cents % 100 == 0 {
            write!(f, "{}", cents / 100)?;
        } else {
            write!(f, "{}.{:02}", cents / 100, cents % 100)?;
        }
        if self.euros {
            f.write_str(" \u{20ac}")
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_and_the_bar() {
        let show = |cents| Amount { cents, euros: true }.to_string();
        assert_eq!(show(20_000), "200 \u{20ac}");
        assert_eq!(show(1_250), "12.50 \u{20ac}");
        assert_eq!(show(-5), "0 \u{20ac}");
        let numbers = Numbers {
            raised_cents: 5_000,
            goal_cents: 20_000,
            monthly: true,
            euros: true,
        };
        assert!((numbers.fill() - 0.25).abs() < 1e-6);
        let over = Numbers {
            raised_cents: 90_000,
            ..numbers
        };
        assert_eq!(over.fill(), 1.0);
    }

    #[test]
    fn only_a_plain_https_address_is_opened() {
        assert!(safe_url("https://ko-fi.com/soljk"));
        assert!(!safe_url("http://ko-fi.com/soljk"));
        assert!(!safe_url("https://x.com/a&calc"));
        assert!(!safe_url("https://x.com/\"a"));
        assert!(!safe_url("https://x.com/a b"));
        assert_eq!(url(), DEFAULT_URL);
    }
}
