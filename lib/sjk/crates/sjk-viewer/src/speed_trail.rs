//! Force Speed afterimages ("speed trail") behind players and NPCs.
//!
//! Stock cgame draws two copies of the player's legs refEntity, in the same
//! pose, behind an entity with `PW_SPEED`, which the server keeps set while
//! `FP_SPEED` is active (`codemp/game/w_force.c:5641-5644`). `CG_Player`
//! (`codemp/cgame/cg_players.c:10841-10906`) places the first copy a fixed
//! distance from this frame's origin towards the origin of the previous
//! frame, at `shaderRGBA[3]` 100 with `RF_FORCE_ENT_ALPHA`, and the second the
//! same distance beyond the first, towards where the first was drawn last
//! frame, at alpha 50. The distance is
//! `(int)(SPEED_TRAIL_DISTANCE * |trDelta| * 0.004)`, so the copies spread
//! with speed and collapse onto the player at rest. The end of `CG_Player`
//! (`cg_players.c:12595-12610`) then shifts the history. `cg_speedTrail 0`,
//! the absence of `PW_SPEED` and the mind-trick fade (`doAlpha`) clear it.
//!
//! [`Trails`] keeps that per-entity history in storage allocated once.

use glam::Vec3;

/// `SPEED_TRAIL_DISTANCE` (`cg_players.c:7810`).
const SPEED_TRAIL_DISTANCE: f64 = 6.0;

/// `shaderRGBA[3]` of the nearer and farther copies.
const ALPHAS: [u8; 2] = [100, 50];

/// One afterimage to draw this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ghost {
    pub(crate) origin: Vec3,
    /// rd-vanilla `RF_FORCE_ENT_ALPHA` alpha byte.
    pub(crate) alpha: u8,
}

/// `centity_t::frame_minus1`/`frame_minus2` and their `_refreshed` flags.
#[derive(Clone, Copy, Debug, Default)]
struct History {
    positions: [Vec3; 2],
    refreshed: [bool; 2],
}

/// Per-entity trail history, indexed by entity number.
pub(crate) struct Trails {
    entities: Box<[History]>,
}

impl Default for Trails {
    fn default() -> Self {
        Self {
            entities: vec![History::default(); sjk_protocol::MAX_LEGACY_ENTITIES]
                .into_boxed_slice(),
        }
    }
}

impl Trails {
    /// Run one `CG_Player` pass for entity `number` drawn at `origin` and
    /// return the copies to draw. Call once per rendered frame for every
    /// actor stock would pass through `CG_Player`, whether or not it trails:
    /// the history is primed while the power is off, as in stock.
    pub(crate) fn advance(
        &mut self,
        number: u16,
        origin: Vec3,
        velocity: Vec3,
        trailing: bool,
    ) -> [Option<Ghost>; 2] {
        let Some(history) = self.entities.get_mut(usize::from(number)) else {
            return [None; 2];
        };
        if !trailing {
            history.refreshed = [false; 2];
        }
        let mut ghosts = [None; 2];
        if history.refreshed.contains(&true) {
            let distance = distance(velocity);
            if history.refreshed[0] {
                let direction = normalized(history.positions[0] - origin);
                history.positions[0] = origin + direction * distance;
                ghosts[0] = Some(Ghost {
                    origin: history.positions[0],
                    alpha: ALPHAS[0],
                });
            }
            if history.refreshed[1] {
                let direction = normalized(history.positions[1] - history.positions[0]);
                history.positions[1] = history.positions[0] + direction * distance;
                ghosts[1] = Some(Ghost {
                    origin: history.positions[1],
                    alpha: ALPHAS[1],
                });
            }
        }
        history.positions[1] = history.positions[0];
        if history.refreshed[0] {
            history.refreshed[1] = true;
        }
        history.positions[0] = origin;
        history.refreshed[0] = true;
        ghosts
    }
}

/// `distVelBase`: an `int`, so the spacing moves in whole units.
fn distance(velocity: Vec3) -> f32 {
    (SPEED_TRAIL_DISTANCE * (f64::from(velocity.length()) * 0.004)) as i32 as f32
}

/// `VectorNormalize` leaves a zero vector unchanged.
fn normalized(vector: Vec3) -> Vec3 {
    let length = vector.length();
    if length == 0.0 {
        vector
    } else {
        vector / length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: Vec3 = Vec3::new(500.0, 0.0, 0.0);

    fn at(x: f32) -> Vec3 {
        Vec3::new(x, 0.0, 0.0)
    }

    #[test]
    fn copies_trail_at_whole_unit_spacing_and_fixed_alphas() {
        let mut trails = Trails::default();
        // First frame has no history yet.
        assert_eq!(trails.advance(3, at(0.0), FAST, true), [None; 2]);
        // 6 * 500 * 0.004 = 12 units behind, towards the previous origin.
        let [first, second] = trails.advance(3, at(10.0), FAST, true);
        assert_eq!(
            first,
            Some(Ghost {
                origin: at(-2.0),
                alpha: 100
            })
        );
        assert_eq!(second, None);
        let [first, second] = trails.advance(3, at(20.0), FAST, true);
        assert_eq!(
            first,
            Some(Ghost {
                origin: at(8.0),
                alpha: 100
            })
        );
        // Beyond the first, towards where the first was drawn last frame.
        assert_eq!(
            second,
            Some(Ghost {
                origin: at(-4.0),
                alpha: 50
            })
        );
    }

    #[test]
    fn spacing_truncates_and_collapses_at_rest() {
        assert_eq!(distance(Vec3::new(249.0, 0.0, 0.0)), 5.0);
        assert_eq!(distance(Vec3::new(0.0, 0.0, 41.0)), 0.0);
        let mut trails = Trails::default();
        trails.advance(0, at(5.0), Vec3::ZERO, true);
        let [first, _] = trails.advance(0, at(5.0), Vec3::ZERO, true);
        assert_eq!(first.map(|ghost| ghost.origin), Some(at(5.0)));
    }

    #[test]
    fn history_primes_while_off_and_clears_when_power_ends() {
        let mut trails = Trails::default();
        assert_eq!(trails.advance(7, at(0.0), FAST, false), [None; 2]);
        // Power on: the previous frame's origin is already known.
        let [first, second] = trails.advance(7, at(30.0), FAST, true);
        assert_eq!(first.map(|ghost| ghost.origin), Some(at(18.0)));
        assert_eq!(second, None);
        // Power off: nothing, even with history.
        assert_eq!(trails.advance(7, at(60.0), FAST, false), [None; 2]);
        let [first, second] = trails.advance(7, at(90.0), FAST, true);
        assert!(first.is_some());
        assert_eq!(second, None);
    }

    #[test]
    fn entities_keep_separate_history_and_bad_numbers_draw_nothing() {
        let mut trails = Trails::default();
        trails.advance(1, at(0.0), FAST, true);
        assert_eq!(trails.advance(2, at(100.0), FAST, true), [None; 2]);
        assert!(trails.advance(1, at(10.0), FAST, true)[0].is_some());
        assert_eq!(trails.advance(u16::MAX, at(0.0), FAST, true), [None; 2]);
    }
}
