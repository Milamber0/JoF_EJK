//! Interface sound cues: hover, click and back posted by every menu canvas as
//! it routes input, the stage model's saber throw and catch posted by the
//! menu stage, the quick wheel's page, move and run cues, the unlock
//! pop-up's chime, the new medal's fanfare and the SJK chat's sound for a new
//! message, all played once per frame by the audio owner.
//!
//! The posters live inside screens that have no audio access, so cues go
//! through one process-wide atomic mailbox. A cue is a bit, not a queue:
//! ten hovers in one frame play one tick, which is what a menu wants.

use super::GameAudio;
use sjk_audio::{ChannelId, SourceId};
use std::sync::atomic::{AtomicU16, Ordering};

/// One interface sound; each maps to a sound file below.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cue {
    /// The pointer or keyboard focus arrived on a new control.
    Hover = 1,
    /// A control was activated.
    Click = 2,
    /// A screen was cancelled or closed.
    Back = 4,
    /// The stage model threw its saber (the Saber tab opened).
    Throw = 8,
    /// The thrown saber landed back in the stage model's hand.
    Catch = 16,
    /// The quick wheel turned to another page.
    WheelPage = 32,
    /// The quick wheel's highlight moved to another choice.
    WheelMove = 64,
    /// The quick wheel ran the chosen choice.
    WheelRun = 128,
    /// An unlock pop-up appeared (`unlock_toast.rs`): an achievement, or a medal or
    /// holocron in a match.
    Achievement = 256,
    /// A new medal began its entrance on the medal pop-up (`medal_popup.rs`).
    Medal = 512,
    /// A new SJK chat message from another player (`sjk_chat_frame.rs`); its sound is
    /// made in memory, not a file of [`CUE_SOUNDS`] (`sjk_chat_sound.rs`).
    SjkChat = 1024,
    /// A world ping appeared (`world_ping_frame.rs`), the player's own or another's.
    WorldPing = 2048,
}

/// `(cue, sound path, volume)` — the game's own sounds, looked up through
/// the VFS like every other sound. Throw and catch are what a thrown saber
/// plays in game: the flight loop `saberspin.wav` (`codemp/game/w_saber.c`
/// `saberent->s.loopSound = saberSpinSound`) and `saber_catch.wav` on
/// return (`saberCheckRadiusDamage` → `G_Sound(saberent, CHAN_AUTO, …)`).
///
/// The quick wheel's are the retail menus' own, quieter than theirs since the
/// wheel is used during matches: a page is `sub_select`, which the controls
/// menu plays as its sub-tabs change (`ui/controls.menu`); a move is
/// `menuroam`, every menu's focus sound (`itemFocusSound`), which the Force
/// power screen also plays as powers are picked (`ui/ingameforceselect.menu`);
/// a run is `button1`, the menus' button press.
///
/// An unlock card (an achievement's, or a medal's or holocron's in a match) plays
/// `secret_area`, the single-player game's sound for a secret area found: its game
/// module (`jagamex86.dll`) plays it with the `@SP_INGAME_SECRET_AREA` centre print.
/// Its file is in the shared `assets0.pk3`, so a multiplayer install has it.
///
/// A medal, rarer and given by hand, plays the multiplayer game's own fanfare:
/// `music/goodsmall.mp3`, which its cgame registers as `cgs.media.happyMusic`
/// and plays to the player who becomes the Jedi Master (`EV_BECOME_JEDIMASTER`,
/// as `sjk-client`'s `sound_events.rs` does). Like every cue it is looked up in
/// the player's game data and is silently left out when the file is missing.
///
/// A world ping plays `update`, a short interface blip (a third of a second, in the
/// shared `assets0.pk3`), so a ping is heard without covering the game.
pub(crate) const CUE_SOUNDS: [(Cue, &str, f32); 11] = [
    (Cue::Hover, "sound/interface/menuroam.mp3", 0.6),
    (Cue::Click, "sound/interface/button1.mp3", 0.9),
    (Cue::Back, "sound/interface/esc.mp3", 0.9),
    (Cue::Throw, "sound/weapons/saber/saberspin.wav", 0.8),
    (Cue::Catch, "sound/weapons/saber/saber_catch.wav", 0.8),
    (Cue::WheelPage, "sound/interface/sub_select.mp3", 0.4),
    (Cue::WheelMove, "sound/interface/menuroam.mp3", 0.5),
    (Cue::WheelRun, "sound/interface/button1.mp3", 0.5),
    (Cue::Achievement, "sound/interface/secret_area.mp3", 0.8),
    (Cue::Medal, "music/goodsmall.mp3", 0.75),
    (Cue::WorldPing, "sound/interface/update.mp3", 0.8),
];

/// Interface cues never share a channel with world sounds.
const UI_SOURCE: SourceId = SourceId(u32::MAX);

static PENDING: AtomicU16 = AtomicU16::new(0);

#[cfg(test)]
thread_local! {
    /// The cues this thread posted, in order (tests: each test runs on its own
    /// thread, while the mailbox is shared by all of them).
    static POSTED: std::cell::RefCell<Vec<Cue>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Post `cue` for the next frame's playback.
pub(crate) fn post(cue: Cue) {
    PENDING.fetch_or(cue as u16, Ordering::Relaxed);
    #[cfg(test)]
    POSTED.with(|posted| posted.borrow_mut().push(cue));
}

/// The cues this thread posted since the last call, in order (tests).
#[cfg(test)]
pub(crate) fn take_posted() -> Vec<Cue> {
    POSTED.with(|posted| std::mem::take(&mut *posted.borrow_mut()))
}

/// Play every cue posted since the last call.
pub(crate) fn play_pending(audio: &mut GameAudio) {
    let pending = PENDING.swap(0, Ordering::Relaxed);
    if pending == 0 {
        return;
    }
    for (index, (cue, path, volume)) in CUE_SOUNDS.iter().enumerate() {
        if pending & *cue as u16 != 0 {
            audio.play_local(path, *volume, UI_SOURCE, ChannelId(index as u32));
        }
    }
    if pending & Cue::SjkChat as u16 != 0 {
        audio.play_sjk_chat(UI_SOURCE, ChannelId(CUE_SOUNDS.len() as u32));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cue_has_its_own_bit_and_one_sound() {
        let mut bits = 0_u16;
        for (cue, path, volume) in CUE_SOUNDS {
            let bit = cue as u16;
            assert_eq!(bit.count_ones(), 1, "{cue:?}");
            assert_eq!(bits & bit, 0, "{cue:?} twice");
            assert_ne!(cue, Cue::SjkChat, "made in memory, never a file");
            bits |= bit;
            // The game's sounds, or its short music stingers (the medal's fanfare).
            assert!(
                (path.starts_with("sound/") || path.starts_with("music/"))
                    && (0.0..=1.0).contains(&volume)
            );
        }
        // The wheel's are quieter than the menus' and than gameplay (1.0).
        let volume = |wanted| {
            CUE_SOUNDS
                .iter()
                .find(|(cue, ..)| *cue == wanted)
                .unwrap()
                .2
        };
        assert!(volume(Cue::WheelMove) < volume(Cue::Hover));
        assert!(volume(Cue::WheelRun) < volume(Cue::Click));
        assert!(volume(Cue::WheelPage) < 1.0);
        assert_eq!(bits & Cue::SjkChat as u16, 0);
        assert_eq!((Cue::SjkChat as u16).count_ones(), 1);
    }
}
