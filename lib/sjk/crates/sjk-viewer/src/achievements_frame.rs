//! The per-frame side of achievements (`achievements.rs`): each live snapshot goes to
//! the tracker with the obituaries it brought, and twice a second (with the identity's
//! turn, `identity_frame.rs`) the counts are loaded, synced with the hub, saved, and
//! new unlocks announced with a console line and the pop-up (`unlock_toast.rs`).

use super::*;

/// Count what the live `snapshot` of `session` shows for the achievements;
/// `obituaries_before` is the obituary tracker's count before the snapshot was observed.
pub(crate) fn observe(
    tracker: &mut achievements::tracker::Tracker,
    obituaries: &sjk_client::ObituaryTracker,
    session: &ClientSession,
    snapshot: &Snapshot,
    obituaries_before: u64,
    now: Instant,
) {
    // The feed keeps the newest eight; more in one snapshot than that is unheard of.
    let new = (obituaries.decoded() - obituaries_before).min(8) as usize;
    let mut events = [None; 8];
    for (slot, offset) in events.iter_mut().zip((0..new).rev()) {
        *slot = obituaries.feed().newest(offset);
    }
    let events: Vec<sjk_client::ObituaryEvent> = events.into_iter().flatten().collect();
    let seen = achievements::tracker::Seen {
        snapshot,
        game_state: session.game_state(),
        server: session.server(),
        local: session.is_local(),
        obituaries: &events,
        now,
    };
    achievements::update(|record| tracker.observe(&seen, record));
}

impl GpuState {
    /// Twice a second: load the counts once, take in what the hub holds, hand the
    /// service the counts, save, and announce what was unlocked.
    pub(crate) fn update_achievements(&mut self) {
        let Some(console) = self.console.as_mut() else {
            return;
        };
        achievements::load(console.config_directory());
        let held = player_identity::own_achievements();
        achievements::sync(held.as_deref(), false);
        for kind in achievements::take_announcements() {
            console.push_log(format!(
                "^3Achievement unlocked: ^7{} ^5({})",
                kind.name, kind.description
            ));
            self.unlock_toast
                .push(crate::unlock_toast::Unlock::Achievement(kind));
        }
    }
}
