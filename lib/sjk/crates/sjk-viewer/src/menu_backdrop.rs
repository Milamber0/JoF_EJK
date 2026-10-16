//! Live-map backdrop behind the standalone client menus.
//!
//! The main menu sits over the real renderer instead of a flat gradient: the
//! camera parks at the map's `info_player_intermission` view (the vantage the
//! map author chose to show off the level) and drifts very slowly so the
//! screen never reads as a still image. Screens that ask for a different
//! shot (settings) fly there along a per-map authored route and back again.

mod flight;
mod routes;
mod tour;

use super::GpuState;
use flight::Path;
use glam::Vec3;
pub(crate) use routes::Stage;
#[cfg(test)]
pub(crate) use routes::tour_for;
use sjk_bsp::Bsp;
use sjk_entity::parse_entity_lump;
use sjk_ui::{Easing, Tween};
use std::time::Instant;

/// Which backdrop shot a menu screen wants behind it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Shot {
    /// The intermission vantage behind the main menu.
    Main,
    /// The map's server-browser vantage (the big gate at the end of the
    /// corridor beside the ship on mp/ffa3).
    Browser,
    /// The map's settings vantage (the balcony behind the main vantage on mp/ffa3).
    Settings,
    /// The map's player stage (the tower balcony on mp/ffa3).
    Player,
    /// Where the thrown saber floats (between the tower walls on mp/ffa3),
    /// reached from the player stage.
    Saber,
    /// The Profile screen's Holocrons tab: a view over open ground, left clear for
    /// the 3D holocron, which floats before the camera wherever it is
    /// (`holocrons::stage`). A map without a route for it keeps the main vantage.
    Holocrons,
}

/// Fraction of the flight over which the destination screen fades in. It
/// starts at departure so settings are usable while the camera is still
/// moving; the flight is scenery, not a wait.
const REVEAL_SPAN: f32 = 0.18;

/// Camera anchor for the menu backdrop.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Vantage {
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
    pitch: f32,
}

/// One drifted camera sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Sample {
    pub(crate) origin: Vec3,
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// One authored flight: from the main vantage, or chained onto the end of
/// another flight (`parent`).
struct Flight {
    shot: Shot,
    parent: Option<usize>,
    path: Path,
    millis: u32,
    stage: Option<Stage>,
    focus: Option<Vec3>,
}

/// Camera state behind the standalone menus: the main vantage, the routes
/// to the other shots, and where along the active one the camera is.
pub(crate) struct Backdrop {
    main: Vantage,
    flights: Vec<Flight>,
    /// Flight the camera is currently on (or parked at an end of).
    active: Option<usize>,
    /// 0 = parked where the active flight departs from (the main vantage or
    /// its parent's shot), 1 = parked on the active flight's shot.
    progress: Tween,
    last_progress: f32,
    /// The map's camera tour, when it has one: the main shot plays it, and
    /// the other shots are reached by a cut through dark instead of a flight
    /// (a tour's shots are all over the map, so no route starts from them).
    tour: Option<tour::Tour>,
    /// The cut under way on a toured map: the flight it leaves for (`None`
    /// back to the tour) and the backdrop time it began.
    cut: Option<(Option<usize>, u64)>,
    /// How dark the tour's fades and cuts make the world this frame.
    darkness: f32,
}

impl Backdrop {
    pub(crate) fn from_bsp(bsp: &Bsp) -> Option<Self> {
        let main = Vantage::from_bsp(bsp)?;
        let message = worldspawn_message(bsp);
        let routes = [
            Shot::Browser,
            Shot::Settings,
            Shot::Player,
            Shot::Saber,
            Shot::Holocrons,
        ]
        .into_iter()
        .filter_map(|shot| routes::route_for(message.as_deref()?, shot))
        .collect::<Vec<_>>();
        let mut backdrop = Self::new(main, &routes);
        if let Some(shots) = message.as_deref().and_then(routes::tour_for) {
            // Each start shuffles the tour differently.
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(1, |time| time.as_nanos() as u64);
            backdrop.tour = Some(tour::Tour::new(shots, seed));
        }
        Some(backdrop)
    }

    /// Routes chained onto another shot must follow that shot's route; a
    /// route whose parent is missing is dropped.
    fn new(main: Vantage, routes: &[&routes::Route]) -> Self {
        let mut flights: Vec<Flight> = Vec::with_capacity(routes.len());
        for route in routes {
            let parent = match route.from {
                Shot::Main => None,
                from => match flights.iter().position(|flight| flight.shot == from) {
                    Some(parent) => Some(parent),
                    None => continue,
                },
            };
            let start = parent.map_or(main, |parent| flights[parent].path.destination());
            flights.push(Flight {
                shot: route.shot,
                parent,
                path: Path::new(start, route),
                millis: route.millis,
                stage: route.stage,
                focus: route.focus.map(Vec3::from_array),
            });
        }
        Self {
            main,
            flights,
            active: None,
            progress: Tween::settled(0.0),
            last_progress: 0.0,
            tour: None,
            cut: None,
            darkness: 0.0,
        }
    }

    /// How dark the tour's fades and cuts make the world this frame (0 clear,
    /// 1 black); 0 on a map without a tour.
    pub(crate) fn darkness(&self) -> f32 {
        self.darkness
    }

    /// The camera on a toured map at backdrop time `millis`, `wanted` the
    /// flight whose shot the screen asks for (`None`: the tour). Moving
    /// between the tour and a shot fades to black, cuts, and fades back in.
    fn drive_tour(&mut self, wanted: Option<usize>, millis: u64) -> Sample {
        let half = tour::FADE_MILLIS / 2;
        if self.cut.is_none() && wanted != self.active {
            self.cut = Some((wanted, millis));
        }
        let mut darkness = 0.0_f32;
        if let Some((to, began)) = &mut self.cut {
            let elapsed = millis.saturating_sub(*began);
            if elapsed < half {
                // Still fading out: the screen may change its mind.
                *to = wanted;
                darkness = elapsed as f32 / half as f32;
            } else {
                let to = *to;
                if self.active != to {
                    self.active = to;
                    if to.is_none()
                        && let Some(tour) = &mut self.tour
                    {
                        tour.advance(millis);
                    }
                }
                darkness = 1.0 - (elapsed - half) as f32 / half as f32;
                if elapsed >= 2 * half {
                    self.cut = None;
                }
            }
        }
        let seconds = millis as f32 / 1_000.0;
        let sample = match (self.active, &mut self.tour) {
            (Some(index), _) => self.flights[index].path.destination().sample(seconds),
            (None, Some(tour)) => {
                let (sample, fade) = tour.sample(millis);
                darkness = darkness.max(fade);
                sample
            }
            (None, None) => self.main.sample(seconds),
        };
        self.darkness = darkness.clamp(0.0, 1.0);
        sample
    }

    fn flight_for(&self, shot: Shot) -> Option<usize> {
        self.flights.iter().position(|flight| flight.shot == shot)
    }

    /// Whether `ancestor` is `flight` itself or a flight it chains onto.
    fn descends_from(&self, flight: Option<usize>, ancestor: Option<usize>) -> bool {
        let mut current = flight;
        loop {
            if current == ancestor {
                return true;
            }
            match current {
                Some(index) => current = self.flights[index].parent,
                None => return false,
            }
        }
    }

    /// The flight chained directly onto `parent` on the way to `wanted`.
    fn child_toward(&self, parent: Option<usize>, wanted: Option<usize>) -> Option<usize> {
        let mut current = wanted?;
        while self.flights[current].parent != parent {
            current = self.flights[current].parent?;
        }
        Some(current)
    }

    /// Advance toward `shot` at backdrop time `millis` and return the camera.
    /// A destination off the current chain first flies back down the chain
    /// (to the main vantage at most), then departs along the new one; a
    /// chained shot is reached through its parent's shot.
    pub(crate) fn drive(&mut self, shot: Shot, millis: u64) -> Sample {
        let wanted = self.flight_for(shot);
        if self.tour.is_some() {
            return self.drive_tour(wanted, millis);
        }
        let progress = self.progress.sample(millis);
        let parked_start = self.progress.target() == 0.0 && progress <= 1e-4;
        let parked_end = self.progress.target() == 1.0 && progress >= 1.0 - 1e-4;
        // Parked at a flight's start: the camera is on the parent's shot.
        if parked_start && let Some(active) = self.active {
            self.active = self.flights[active].parent;
            self.progress = Tween::settled(1.0);
        }
        let target = if self.active.is_none() && wanted.is_none() {
            0.0
        } else if self.active == wanted {
            1.0
        } else if self.descends_from(wanted, self.active) {
            // On the way out along the chain: depart the next flight once
            // the camera has reached this one's end.
            let next = self.child_toward(self.active, wanted);
            if self.active.is_none() || parked_end {
                self.active = next;
                self.progress = Tween::settled(0.0);
                1.0
            } else {
                1.0
            }
        } else {
            0.0
        };
        let millis_full = self.active.map_or(0, |index| self.flights[index].millis);
        if self.progress.target() != target {
            let remaining = (target - self.progress.sample(millis)).abs();
            let duration = (millis_full as f32 * remaining) as u32;
            self.progress
                .retarget(target, millis, duration, Easing::SmoothStep);
        }
        let progress = self.progress.sample(millis);
        self.last_progress = progress;
        let seconds = millis as f32 / 1_000.0;
        let Some(flight) = self.active.map(|index| &self.flights[index]) else {
            return self.main.sample(seconds);
        };
        let pose = flight.path.sample(progress);
        let start = flight
            .parent
            .map_or(self.main, |parent| self.flights[parent].path.destination());
        let departure = start.drift(seconds);
        let arrival = flight.path.destination().drift(seconds);
        let drift = departure.blend(arrival, progress);
        Sample {
            origin: pose.origin + drift.origin,
            yaw: pose.yaw + drift.yaw,
            pitch: pose.pitch + drift.pitch,
        }
    }

    /// Opacity for the screen that belongs to `shot`: fully visible while the
    /// camera is parked there or heading there, fading in as soon as the
    /// flight departs and gone while the camera flies the other way. A
    /// screen stays visible while a flight chained onto its shot returns to
    /// it (the saber flying back to the player screen).
    pub(crate) fn reveal(&self, shot: Shot) -> f32 {
        // On a toured map every screen is up at once; the world cuts behind it.
        if self.tour.is_some() {
            return 1.0;
        }
        let wanted = self.flight_for(shot);
        let target = match wanted {
            Some(index) if self.active == Some(index) => 1.0,
            None if shot == Shot::Main => 0.0,
            // A shot without a route on this map sits over the main vantage.
            None if self.active.is_none() => return 1.0,
            _ => {
                let returning = self.progress.target() == 0.0
                    && self
                        .active
                        .is_some_and(|active| self.flights[active].parent == wanted);
                return if returning { 1.0 } else { 0.0 };
            }
        };
        if self.progress.target() != target {
            return 0.0;
        }
        if target == 0.0
            && self
                .active
                .is_some_and(|active| self.flights[active].parent.is_some())
        {
            // Still on a chained flight: the main menu waits for the last leg.
            return 0.0;
        }
        let toward = (1.0 - (target - self.last_progress).abs()).clamp(0.0, 1.0);
        if toward >= 1.0 - 1e-4 {
            return 1.0;
        }
        (toward / REVEAL_SPAN).clamp(0.0, 1.0)
    }

    /// Where the player's model stands on this map. The stage is part of the
    /// scenery from every shot (the ffa3 balcony is in view from the main
    /// vantage), not only while the camera is on the player route.
    pub(crate) fn stage(&self) -> Option<Stage> {
        self.flights.iter().find_map(|flight| flight.stage)
    }

    /// Where `shot` presents floating objects on this map, and the camera's
    /// right along which several of them line up.
    pub(crate) fn focus(&self, shot: Shot) -> Option<Focus> {
        let flight = self.flights.iter().find(|flight| flight.shot == shot)?;
        let yaw = flight.path.destination().yaw;
        Some(Focus {
            point: flight.focus?,
            right: Vec3::new(yaw.sin(), -yaw.cos(), 0.0),
        })
    }
}

/// A shot's presentation spot: the point it looks at and the horizontal
/// axis to the camera's right, so a row of objects lines up across the view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Focus {
    pub(crate) point: Vec3,
    pub(crate) right: Vec3,
}

/// The map's worldspawn `message` (its display name), the key the authored
/// routes and props are filed under.
pub(crate) fn worldspawn_message(bsp: &Bsp) -> Option<String> {
    let entities = parse_entity_lump(bsp.entities()).ok()?;
    entities
        .iter()
        .find(|entity| entity.classname() == Some("worldspawn"))
        .and_then(|entity| entity.get("message"))
        .map(str::to_owned)
}

impl Vantage {
    /// The intermission view (looking at its `target` when it has one), else
    /// the same spawn the free camera starts from.
    pub(crate) fn from_bsp(bsp: &Bsp) -> Option<Self> {
        let entities = parse_entity_lump(bsp.entities()).ok()?;
        let intermission = entities
            .iter()
            .find(|entity| entity.classname() == Some("info_player_intermission"));
        if let Some(entity) = intermission
            && let Some(origin) = entity.vector("origin").ok().flatten()
        {
            let origin = Vec3::from_array(origin);
            let target = entity.get("target").and_then(|name| {
                entities
                    .iter()
                    .find(|candidate| candidate.get("targetname") == Some(name))
                    .and_then(|candidate| candidate.vector("origin").ok().flatten())
            });
            let (yaw, pitch) = match target {
                Some(target) => {
                    let direction = (Vec3::from_array(target) - origin).normalize_or(Vec3::X);
                    (direction.y.atan2(direction.x), direction.z.asin())
                }
                None => {
                    let angles = entity.vector("angles").ok().flatten().unwrap_or_default();
                    (angles[1].to_radians(), -angles[0].to_radians())
                }
            };
            return Some(Self { origin, yaw, pitch });
        }
        let (origin, yaw) = crate::assets::initial_camera(bsp).ok()?;
        Some(Self {
            origin: Vec3::from_array(origin),
            yaw,
            pitch: 0.0,
        })
    }

    /// Slow cinematic drift: a gentle yaw sweep, a slower pitch breath, and a
    /// few units of lateral float. Periods are long enough that no motion is
    /// perceptible frame to frame; only the parallax over seconds is.
    pub(crate) fn sample(self, seconds: f32) -> Sample {
        let drift = self.drift(seconds);
        Sample {
            origin: self.origin + drift.origin,
            yaw: self.yaw + drift.yaw,
            pitch: self.pitch + drift.pitch,
        }
    }

    /// The drift offset alone, so it can be blended across a flight.
    fn drift(self, seconds: f32) -> Sample {
        let right = Vec3::new(-self.yaw.sin(), self.yaw.cos(), 0.0);
        Sample {
            origin: right * 8.0 * (seconds * 0.09).sin()
                + Vec3::Z * 3.0 * (seconds * 0.13 + 0.7).sin(),
            yaw: 0.09 * (seconds * 0.11).sin(),
            pitch: 0.02 * (seconds * 0.07 + 1.3).sin(),
        }
    }
}

impl Sample {
    fn blend(self, other: Self, t: f32) -> Self {
        Self {
            origin: self.origin.lerp(other.origin, t),
            yaw: self.yaw + (other.yaw - self.yaw) * t,
            pitch: self.pitch + (other.pitch - self.pitch) * t,
        }
    }
}

/// True while a client menu is up outside any live or demo session, or
/// while a join is under way in the menu world, i.e. the world on screen is
/// only the menu backdrop and no HUD belongs over it.
pub(crate) fn standalone_menu_visible(gpu: &GpuState) -> bool {
    let sessions = gpu.live_session.is_some() || gpu.demo_session.is_some();
    gpu.client_menu
        .as_ref()
        .is_some_and(|menu| menu_holds_view(menu, sessions, gpu.is_menu_world))
}

/// Whether the classic menu style covers this frame with an opaque screen,
/// so the world is not drawn: its main pages and the screens they open
/// outside a match, and the connect and loading screens (joins and server
/// map changes alike). Over a live match, the in-game menu and the screens
/// it opens leave the world visible, as retail's do. The SJK UI's main page
/// sits over the live map; the classic screens it opens cover it. Its
/// loading screen keeps the menu map under it until the destination's
/// levelshot covers it, and leaves out a server's world.
pub(crate) fn classic_hides_world(gpu: &GpuState) -> bool {
    // The Holocrons tab floats a 3D holocron in the world, which the page leaves clear
    // on the left, in every menu style.
    if holocrons_shown(gpu) {
        return false;
    }
    gpu.client_menu.as_ref().is_some_and(|menu| {
        if menu.sjk_loading_on_show() {
            return menu.sjk_loading_hides_world(gpu.is_menu_world);
        }
        !menu.sjk_screen()
            && menu.is_visible()
            && (standalone_menu_visible(gpu) || menu.is_loading_screen())
    })
}

/// No HUD belongs over the menu backdrop, nor under the loading card of a
/// map restart or map change on a server world.
pub(crate) fn hides_hud(gpu: &GpuState) -> bool {
    standalone_menu_visible(gpu)
        || gpu
            .client_menu
            .as_ref()
            .is_some_and(crate::menu::ClientMenu::is_connecting)
}

/// Whether `menu` owns the camera given that a live or demo session exists
/// (`sessions`) and whether the world on screen is the menu map
/// (`menu_world`). A join's session appears while the menu still reads
/// `Connecting` — the map load names the map a frame later — so the whole
/// connect phase counts, not only the map load: for that one frame the
/// live camera would otherwise render the joined player's position inside
/// the menu map. Only the menu world has the backdrop's camera routes: a
/// map restart or map change reloads behind the loading card on a server
/// world, whose game camera keeps the view — the backdrop's gate shot would
/// show that map from the menu's vantage until the reload lands.
pub(crate) fn menu_holds_view(
    menu: &crate::menu::ClientMenu,
    sessions: bool,
    menu_world: bool,
) -> bool {
    menu.is_visible() && (!sessions || (menu_world && menu.is_connecting()))
}

/// Whether the Profile screen's Holocrons page is open.
fn holocrons_shown(gpu: &GpuState) -> bool {
    gpu.console
        .as_ref()
        .is_some_and(|console| console.holocrons_stage().is_some())
}

/// Fly or park the free camera on the current screen's shot while a
/// standalone menu is up.
pub(crate) fn drive(gpu: &mut GpuState, now: Instant) {
    if !standalone_menu_visible(gpu) || gpu.resident.map_change_pending {
        return;
    }
    let millis = now.duration_since(gpu.ui_epoch).as_millis() as u64;
    let holocrons = holocrons_shown(gpu);
    if let Some(menu) = gpu.client_menu.as_mut() {
        menu.set_holocron_shot(holocrons);
    }
    let Some(sample) = gpu
        .client_menu
        .as_mut()
        .and_then(|menu| menu.drive_backdrop(millis))
    else {
        return;
    };
    gpu.camera_position = sample.origin;
    gpu.camera_yaw = sample.yaw;
    gpu.camera_pitch = sample.pitch;
}

/// Legacy `hud.wgsl` backdrop selector: 4 = console, 3 = a menu screen that
/// still relies on the shader-drawn backdrop, 0 = none.
pub(crate) fn shader_menu_state(gpu: &GpuState) -> f32 {
    if gpu
        .console
        .as_ref()
        .is_some_and(|console| console.is_open())
    {
        4.0
    } else if gpu
        .client_menu
        .as_ref()
        .is_some_and(|menu| menu.is_visible() && menu.draw_list().is_none())
    {
        3.0
    } else {
        0.0
    }
}
