//! The rate at which continuously played legacy effects are re-played.
//!
//! Several cgame paths call `FX_PlayEffectID` on every rendered frame: the projectile
//! think (`CG_Missile`, `cg_ents.c:2943-2952`; e.g. `FX_RepeaterAltProjectileThink`,
//! `fx_heavyrepeater.c:141-155`), the muzzle-flash window (`cg_weapons.c:733-762`), the
//! item cone (`cg_ents.c:1950-1953`) and the force lightning/drain beam
//! (`cg_players.c:9408-9510`). Each call adds fresh primitives that live their authored life
//! (50 ms by default, `FxTemplate.cpp:51`), so the number of overlapping additive copies, and
//! with it how white and how wide the effect looks, grows with the frame rate.
//!
//! The reference look is TaystJK at `com_maxfps 125`, the engine default
//! (`shared/sys/sys_main.cpp:172-174`): a repeater alt orb then carries about six copies of
//! each sprite. SJK renders at 500 FPS and more, which stacked four times as many and turned
//! the orb into a flat white disc. These effects
//! are therefore re-played on a fixed 8 ms cgame cadence, independent of the render rate.
//! Everything else about them (lights, positions, lifetimes) is unchanged.

use std::time::{Duration, Instant};

/// One cgame frame at the reference `com_maxfps 125`.
pub(crate) const REFERENCE_FRAME: Duration = Duration::from_millis(8);

/// Decides, once per rendered frame, whether a reference cgame frame falls in it.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Cadence {
    next: Option<Instant>,
    /// The answer for the rendered frame asked last, keyed by its clock value.
    frame: Option<(Instant, bool)>,
}

impl Cadence {
    /// Whether the rendered frame at `now` plays continuous effects. All callers of one
    /// rendered frame pass the same `now` and get the same answer. Below 125 FPS every
    /// frame plays, as stock does; above it, frames are skipped to keep 8 ms spacing. A clock
    /// that jumps back (a demo seek, a restarted session) plays at once and re-phases.
    pub(crate) fn due(&mut self, now: Instant) -> bool {
        if let Some((at, due)) = self.frame
            && at == now
        {
            return due;
        }
        let due = self
            .next
            .is_none_or(|next| now >= next || next > now + REFERENCE_FRAME);
        if due {
            // Keep the phase while on time; after a stall or a jump, restart from `now`.
            self.next = Some(match self.next {
                Some(next) if now >= next && now < next + REFERENCE_FRAME => next + REFERENCE_FRAME,
                _ => now + REFERENCE_FRAME,
            });
        }
        self.frame = Some((now, due));
        due
    }
}
