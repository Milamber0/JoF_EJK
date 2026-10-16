//! The rocket launcher's lock-on, as JoF EJK's `CG_DrawRocketLocking` (`cg_draw.c`)
//! draws and sounds it.
//!
//! Holding the rocket launcher's alternate attack on a target makes the game set
//! `ps.rocketLockIndex` (the target) and `ps.rocketLockTime` (when the lock began).
//! Every `lockTimeInterval` (1200 ms over 16 steps, 2400 in Siege) after that time one
//! more red `gfx/2d/wedge` slice is drawn around the target, each slice turned another
//! 45 degrees about the ring's centre and a little more opaque; at eight slices the
//! lock is complete and the additive `gfx/2d/lock` pulses over them. Each new slice
//! plays `sound/weapons/rocket/tick.wav` at the target, the eighth `lock.wav`.
//!
//! The ring is drawn square on wide screens (as [`super::crosshair`] keeps the
//! crosshair square); the classic HUD stretches it with the 640-wide virtual screen.
//! A teammate's lock is not drawn in team games, as in stock. A vehicle weapon's own
//! lock time and sounds are not covered.

use super::icons::assets::decode;
use crate::hud::identification::Camera;
use glam::Vec3;
use sjk_shader::ShaderCatalog;
use sjk_ui::{Color, DrawCommand, DrawList, Rect, TextureId};
use sjk_vfs::VirtualFileSystem;

/// `cgs.media` wedge: one slice of the ring.
const WEDGE: &str = "gfx/2d/wedge";
/// The picture over a complete lock.
const LOCK: &str = "gfx/2d/lock";
/// A slice added to the lock.
pub(crate) const TICK_SOUND: &str = "sound/weapons/rocket/tick.wav";
/// The lock complete.
pub(crate) const LOCK_SOUND: &str = "sound/weapons/rocket/lock.wav";
/// `ENTITYNUM_NONE`: no lock.
const NO_TARGET: u16 = 1023;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: u16 = 32;
/// Slices in a complete lock.
const SLICES: i32 = 8;
/// `GT_TEAM`: from here on a teammate's lock is not drawn.
const GT_TEAM: i32 = 6;
/// `GT_SIEGE`: locks take twice as long.
const GT_SIEGE: i32 = 7;

/// The lock-on's map-lifetime pictures.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Pictures {
    wedge: Option<TextureId>,
    lock: Option<TextureId>,
}

/// Upload the wedge and the lock once per installed world; absent ones stay `None`.
pub(crate) fn load(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    mut upload: impl FnMut(TextureId, &[u8]),
) -> Pictures {
    let first = crate::ui_renderer::ROCKET_LOCK_ICON_FIRST;
    let wedge = decode(vfs, shaders, WEDGE).map(|pixels| {
        let id = TextureId(first);
        upload(id, pixels.as_raw());
        id
    });
    let lock = decode(vfs, shaders, LOCK).map(|mut pixels| {
        additive_to_alpha(pixels.as_mut());
        let id = TextureId(first + 1);
        upload(id, pixels.as_raw());
        id
    });
    Pictures { wedge, lock }
}

/// Stock draws the lock additively ("this art is additive"); the HUD blends by alpha,
/// so each pixel's brightness becomes its coverage and its colour is brightened to
/// match: black stays clear, white stays white.
fn additive_to_alpha(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let peak = pixel[0].max(pixel[1]).max(pixel[2]);
        if peak == 0 {
            pixel[3] = 0;
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = (u16::from(*channel) * 255 / u16::from(peak)) as u8;
        }
        pixel[3] = (u16::from(pixel[3]) * u16::from(peak) / 255) as u8;
    }
}

/// What one frame's lock-on needs from the game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Input {
    /// `ps.rocketLockIndex`.
    pub(crate) target: u16,
    /// `ps.rocketLockTime`: when the lock began, `-1` while it holds its last step.
    pub(crate) since: f32,
    /// `cg.time`.
    pub(crate) now: i32,
    /// `g_gametype`.
    pub(crate) gametype: i32,
    /// The local player spectates.
    pub(crate) spectator: bool,
    /// The target is a player on the local player's team.
    pub(crate) teammate: bool,
    /// The target's presented origin (`cent->lerpOrigin`), if it is presented.
    pub(crate) origin: Option<Vec3>,
    /// Intermission, `cg_draw2D 0` or no snapshot: nothing is drawn or sounded.
    pub(crate) hidden: bool,
}

/// Read this frame's [`Input`] from the presented snapshot: `ps.rocketLockIndex`
/// (field 24) and `ps.rocketLockTime` (field 79, a float), the teams from the
/// players' config strings, and the target's presented origin from `world`.
pub(crate) fn input(
    snapshot: &sjk_protocol::Snapshot,
    game: &sjk_protocol::GameState,
    world: &sjk_runtime::World,
    now: i32,
    hidden: bool,
) -> Input {
    let player = &snapshot.player;
    let target = player.raw_field(24).map_or(NO_TARGET, |index| index as u16);
    let since = f32::from_bits(player.raw_field(79).unwrap_or(0));
    let team = |client: u16| {
        super::identification::info_number(game.config_string(1131 + usize::from(client)), "t")
    };
    Input {
        target,
        since,
        now,
        gametype: super::identification::info_number(game.config_string(0), "g_gametype"),
        spectator: player.team() == 3,
        teammate: target < MAX_CLIENTS && team(target) == team(player.client_num()),
        origin: (target < NO_TARGET)
            .then(|| world.entity(sjk_runtime::EntityId::new(u64::from(target) + 1)))
            .flatten()
            .map(|entity| Vec3::from_array(entity.sample(i64::from(now)).translation)),
        hidden,
    }
}

/// One frame's ring, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ring {
    /// The ring's centre, where every slice turns.
    center: [f32; 2],
    /// One slice's side.
    side: f32,
    /// Slices drawn, 0 to 8.
    slices: i32,
    /// The lock's grey, while complete.
    pulse: f32,
}

/// The lock-on between frames: stock's `oldDif` and `lastvalidlockdif`.
#[derive(Debug, Default)]
pub(crate) struct State {
    pictures: Pictures,
    ring: Option<Ring>,
    last_slices: i32,
    last_valid: i32,
    /// A sound to play at a world position this frame.
    sound: Option<(&'static str, [f32; 3])>,
}

impl State {
    pub(crate) fn set_pictures(&mut self, pictures: Pictures) {
        self.pictures = pictures;
    }

    /// The sound this frame's update asked for, once.
    pub(crate) fn take_sound(&mut self) -> Option<(&'static str, [f32; 3])> {
        self.sound.take()
    }

    /// `CG_DrawRocketLocking` for one frame through `camera`.
    pub(crate) fn update(&mut self, input: Input, camera: Camera) {
        self.ring = None;
        self.sound = None;
        if input.hidden
            || input.target == NO_TARGET
            || input.now as f32 - input.since <= 0.0
            || input.since == 0.0
            || input.spectator
            || (input.teammate && input.gametype >= GT_TEAM)
        {
            return;
        }
        let interval = if input.gametype == GT_SIEGE {
            2400.0
        } else {
            1200.0
        } / 16.0;
        let mut slices = ((input.now as f32 - input.since) / interval) as i32;
        if input.since != -1.0 {
            self.last_valid = slices;
        } else {
            slices = self.last_valid;
        }
        let Some(origin) = input.origin else {
            return;
        };
        // `CG_WorldCoordToScreenCoord` refuses only points behind the view.
        let Some(point) = camera.project_within(origin, f32::MAX) else {
            return;
        };
        let near = (origin.distance(camera.eye) / 1024.0).clamp(0.0, 1.0);
        let size = (1.0 - near) * (1.0 - near) * 32.0 + 6.0;
        if slices < 0 {
            self.last_slices = 0;
            return;
        }
        let slices = slices.min(SLICES);
        if self.last_slices != slices {
            let sound = if slices == SLICES {
                LOCK_SOUND
            } else {
                TICK_SOUND
            };
            self.sound = Some((sound, origin.to_array()));
        }
        self.last_slices = slices;
        // Virtual 640x480 units to pixels; the ring is centred half a slice above
        // the projected origin (`cy += sz * 0.5`, slices drawn from `cy - sz`).
        let scale = camera.viewport[1] / 480.0;
        let side = size * scale;
        self.ring = Some(Ring {
            center: [point[0], point[1] - side * 0.5],
            side,
            slices,
            pulse: (input.now as f32 * 0.05).sin() * 0.5 + 0.5,
        });
    }

    /// Append this frame's slices and lock.
    pub(crate) fn emit(&self, list: &mut DrawList) {
        let Some(ring) = self.ring else {
            return;
        };
        let [x, y] = ring.center;
        let side = ring.side;
        if let Some(wedge) = self.pictures.wedge {
            for slice in 0..ring.slices {
                let _ = list.push(DrawCommand::TexturedQuadTurned {
                    rect: Rect::new(x - side, y, side, side),
                    texture: wedge,
                    color: Color::new(1.0, 0.0, 0.0, 0.1 * slice as f32 + 0.2),
                    pivot: ring.center,
                    degrees: slice as f32 * 45.0,
                });
            }
        }
        if ring.slices == SLICES
            && let Some(lock) = self.pictures.lock
        {
            let grey = ring.pulse;
            let _ = list.push(DrawCommand::TexturedQuad {
                rect: Rect::new(x - side, y - side, side * 2.0, side * 2.0),
                texture: lock,
                color: Color::new(grey, grey, grey, 1.0),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn camera() -> Camera {
        Camera {
            eye: Vec3::ZERO,
            target: Vec3::X,
            up: Vec3::Z,
            fov: 75.0,
            viewport: [1920.0, 1080.0],
        }
    }

    fn input(now: i32) -> Input {
        Input {
            target: 5,
            since: 10_000.0,
            now,
            gametype: 0,
            spectator: false,
            teammate: false,
            origin: Some(Vec3::new(512.0, 0.0, 0.0)),
            hidden: false,
        }
    }

    fn with_pictures() -> State {
        State {
            pictures: Pictures {
                wedge: Some(TextureId(1)),
                lock: Some(TextureId(2)),
            },
            ..State::default()
        }
    }

    /// One slice every 75 ms, a tick for each new one and the lock sound at eight,
    /// then silence while the lock holds.
    #[test]
    fn slices_build_up_with_a_tick_each_and_lock_at_eight() {
        let mut state = with_pictures();
        let mut sounds = Vec::new();
        for now in (10_001..10_900).step_by(5) {
            state.update(input(now), camera());
            if let Some((sound, _)) = state.take_sound() {
                sounds.push((now, sound));
            }
        }
        // 0 slices at the start (a change from stock's initial 0 is silent), then 1..8.
        assert_eq!(sounds.len(), 8, "{sounds:?}");
        assert!(sounds[..7].iter().all(|(_, sound)| *sound == TICK_SOUND));
        assert_eq!(sounds[7].1, LOCK_SOUND);
        assert!((10_600..10_610).contains(&sounds[7].0), "{sounds:?}");
        assert_eq!(state.ring.map(|ring| ring.slices), Some(8));
        let mut list = DrawList::new(16);
        state.emit(&mut list);
        assert_eq!(list.len(), 9, "eight slices and the lock");
    }

    #[test]
    fn slices_turn_about_the_ring_and_grow_more_opaque() {
        let mut state = with_pictures();
        state.update(input(10_000 + 75 * 3 + 1), camera());
        let mut list = DrawList::new(16);
        state.emit(&mut list);
        let turns: Vec<_> = list
            .commands()
            .iter()
            .map(|command| match *command {
                DrawCommand::TexturedQuadTurned {
                    rect,
                    color,
                    pivot,
                    degrees,
                    ..
                } => {
                    assert_eq!([rect.right(), rect.y], pivot, "turned about its top right");
                    (degrees, color.a)
                }
                _ => panic!("{command:?}"),
            })
            .collect();
        assert_eq!(turns.len(), 3);
        for (slice, (degrees, alpha)) in turns.into_iter().enumerate() {
            assert_eq!(degrees, slice as f32 * 45.0);
            assert!((alpha - (0.1 * slice as f32 + 0.2)).abs() < 1e-6);
        }
    }

    /// Centred on the target, a little above its origin, larger the nearer it is.
    #[test]
    fn the_ring_sits_on_the_target_and_shrinks_with_distance() {
        let mut state = with_pictures();
        state.update(input(10_100), camera());
        let near = state.ring.unwrap();
        assert!((near.center[0] - 960.0).abs() < 0.5);
        assert!(near.center[1] < 540.0);
        // 512 units: (1 - 0.5)^2 * 32 + 6 = 14 virtual units, 31.5 pixels at 1080.
        assert!((near.side - 31.5).abs() < 0.01, "{near:?}");
        let mut far = input(10_100);
        far.origin = Some(Vec3::new(4_000.0, 0.0, 0.0));
        state.update(far, camera());
        assert!((state.ring.unwrap().side - 6.0 * 1080.0 / 480.0).abs() < 0.01);
    }

    #[test]
    fn nothing_without_a_lock_a_teammate_or_a_view_of_the_target() {
        let mut state = with_pictures();
        for input in [
            Input {
                target: NO_TARGET,
                ..input(10_100)
            },
            Input {
                since: 0.0,
                ..input(10_100)
            },
            Input {
                spectator: true,
                ..input(10_100)
            },
            Input {
                teammate: true,
                gametype: GT_TEAM,
                ..input(10_100)
            },
            Input {
                origin: Some(Vec3::new(-500.0, 0.0, 0.0)),
                ..input(10_100)
            },
            Input {
                hidden: true,
                ..input(10_100)
            },
            input(9_000),
        ] {
            state.update(input, camera());
            assert_eq!(state.ring, None, "{input:?}");
        }
        // In free-for-all a lock on someone of the same (free) team is drawn.
        state.update(
            Input {
                teammate: true,
                ..input(10_100)
            },
            camera(),
        );
        assert!(state.ring.is_some());
    }

    /// `rocketLockTime -1` holds the last step drawn.
    #[test]
    fn a_held_lock_keeps_its_last_step() {
        let mut state = with_pictures();
        state.update(input(10_000 + 75 * 5 + 1), camera());
        state.update(
            Input {
                since: -1.0,
                ..input(20_000)
            },
            camera(),
        );
        assert_eq!(state.ring.map(|ring| ring.slices), Some(5));
    }

    #[test]
    fn the_additive_lock_keeps_its_light_and_loses_its_black() {
        let mut pixels = [0, 0, 0, 255, 255, 255, 255, 255, 128, 64, 0, 255];
        additive_to_alpha(&mut pixels);
        assert_eq!(pixels[3], 0, "black is clear");
        assert_eq!(&pixels[4..8], &[255, 255, 255, 255]);
        assert_eq!(&pixels[8..12], &[255, 127, 0, 128]);
    }
}
