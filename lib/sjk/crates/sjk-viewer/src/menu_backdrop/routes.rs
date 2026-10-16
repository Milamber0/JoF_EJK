//! Per-map camera routes for the menu backdrop: where each menu screen
//! flies to and the waypoints that get the camera there without clipping
//! walls. Keyed by the worldspawn `message` so map packs can be added as
//! plain data rows.

use super::Shot;

/// One authored camera pose along a route, angles in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Waypoint {
    pub(crate) origin: [f32; 3],
    pub(crate) yaw: f32,
    pub(crate) pitch: f32,
}

/// Where the player's model stands in a shot: the point it is dropped onto
/// the floor from and the direction it faces, in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stage {
    pub(crate) origin: [f32; 3],
    pub(crate) yaw: f32,
}

/// Flight to one screen's shot. The route starts where `from` parks — the
/// map's `info_player_intermission` for [`Shot::Main`] (not repeated here),
/// else the end of that shot's own route — and ends on the shot; everything
/// in between keeps the camera in open air.
#[derive(Debug, PartialEq)]
pub(crate) struct Route {
    /// Worldspawn `message` this route belongs to.
    pub(crate) map_message: &'static str,
    /// Screen shot the route arrives at.
    pub(crate) shot: Shot,
    /// Shot the route departs from; a route from anything but the main
    /// vantage chains onto that shot's route and must be listed after it.
    pub(crate) from: Shot,
    /// Wall-clock length of the full one-way flight.
    pub(crate) millis: u32,
    pub(crate) points: &'static [Waypoint],
    /// Where the player's model stands once the camera has arrived.
    pub(crate) stage: Option<Stage>,
    /// Where the shot presents a floating object (the thrown saber).
    pub(crate) focus: Option<[f32; 3]>,
}

const fn point(origin: [f32; 3], yaw: f32, pitch: f32) -> Waypoint {
    Waypoint { origin, yaw, pitch }
}

/// mp/ffa3 browser: from the courtyard over the parked transport's nose,
/// down into the walled corridor that leaves the landing pad to the south
/// and along it to the big gate (`door_lrg`, x 528..880 at y −1488, z
/// −144..144) at its end, parking at the mouth of its recessed portal so the
/// gate and the sunlit floor in front of it sit behind the server list.
const TATOOINE_FFA_BROWSER: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Browser,
    from: Shot::Main,
    millis: 2_600,
    points: &[
        point([1_400.0, 700.0, 140.0], 200.0, -8.0),
        point([1_050.0, 650.0, 230.0], 225.0, -15.0),
        point([820.0, 420.0, 180.0], 255.0, -20.0),
        point([740.0, 250.0, 60.0], 268.0, -6.0),
        point([708.0, -300.0, -20.0], 270.0, -3.0),
        point([704.0, -800.0, -50.0], 270.0, -1.0),
        point([704.0, -1_120.0, -60.0], 270.0, 3.0),
    ],
    stage: None,
    focus: None,
};

/// mp/ffa3 settings: the rock-walled balcony in the wall directly behind
/// the intermission vantage (its sill at z ≈ 250 above the crates, arch top
/// ≈ 420). The camera keeps facing the parked transport, climbs straight up
/// out of the courtyard and backs through the arch, so the settings form
/// sits in the alcove with the ship framed in the opening beside it.
const TATOOINE_FFA_SETTINGS: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Settings,
    from: Shot::Main,
    millis: 1_600,
    points: &[
        point([1_800.0, 810.0, 180.0], 192.0, 10.0),
        point([1_900.0, 835.0, 330.0], 192.0, 0.0),
        point([2_100.0, 900.0, 340.0], 190.0, -6.0),
        point([2_280.0, 985.0, 345.0], 186.0, -10.0),
    ],
    stage: None,
    focus: None,
};

/// mp/ffa3 player: rise out of the courtyard and glide up to the round
/// tower's balcony, where the model stands on the lip facing out over the
/// yard — close enough to the main vantage to be seen from the main menu.
const TATOOINE_FFA_PLAYER: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Player,
    from: Shot::Main,
    millis: 2_200,
    points: &[
        point([1_500.0, 780.0, 160.0], 190.0, 6.0),
        point([1_100.0, 700.0, 380.0], 200.0, 4.0),
        point([780.0, 600.0, 500.0], 210.0, 0.0),
        point([562.0, 508.0, 518.0], 219.0, -8.0),
    ],
    // The balcony is patch geometry (no brush collision), so the floor
    // height is authored: its drawn surface lies flat at z 472 (sampled from
    // the tessellated `concrete_wall` triangles under this point).
    stage: Some(Stage {
        origin: [452.0, 478.0, 472.0],
        yaw: 14.0,
    }),
    focus: None,
};

/// mp/ffa3 saber: from the balcony shot, swing round past the tower and
/// settle looking through the gap between the two curved walls at the far
/// building's gate, where the thrown saber floats.
const TATOOINE_FFA_SABER: Route = Route {
    map_message: "Tatooine FFA",
    shot: Shot::Saber,
    from: Shot::Player,
    millis: 1_400,
    points: &[
        point([600.0, 522.0, 512.0], 300.0, 2.0),
        point([665.0, 540.0, 506.0], 16.0, -1.0),
    ],
    stage: None,
    // 48 units ahead (was 70: the owner found the hilt too far away).
    focus: Some([709.0, 552.5, 492.0]),
};

/// mp/duel6 player: the model stands on the south-west path from the tower,
/// between its stone benches, facing the camera, the tower rising behind;
/// the camera stands off to the model's left so the SJK UI's form on the
/// left of the screen leaves it in view. A toured map cuts to it, so the one
/// waypoint is the shot itself.
const YAVIN_TRAINING_PLAYER: Route = Route {
    map_message: "Yavin Training Grounds",
    shot: Shot::Player,
    from: Shot::Main,
    millis: 1_000,
    points: &[point([-405.0, -424.0, 394.0], 70.0, -2.0)],
    // The path's floor is at z 352 (traced).
    stage: Some(Stage {
        origin: [-360.0, -360.0, 376.0],
        yaw: 225.0,
    }),
    focus: None,
};

/// mp/duel6 saber: a step closer than the player shot, the thrown saber
/// floating between the camera and the model, right of the screen's middle
/// where the SJK UI's form leaves room, the model and the tower behind it.
const YAVIN_TRAINING_SABER: Route = Route {
    map_message: "Yavin Training Grounds",
    shot: Shot::Saber,
    from: Shot::Player,
    millis: 1_000,
    points: &[point([-425.0, -458.0, 392.0], 72.0, -3.0)],
    stage: None,
    focus: Some([-399.6, -417.3, 392.0]),
};

/// mp/duel6 holocrons: the tower and the quarter garden from the south-west path,
/// high enough over it that the garden's greens, not a wall, fill the clear left of the
/// screen; the Holocrons tab floats its holocron before this camera, which the page's
/// dark panel on the right leaves free. A toured map cuts to it, so the one waypoint is
/// the shot itself.
const YAVIN_TRAINING_HOLOCRONS: Route = Route {
    map_message: "Yavin Training Grounds",
    shot: Shot::Holocrons,
    from: Shot::Main,
    millis: 1_000,
    points: &[point([-405.0, -424.0, 394.0], 70.0, -2.0)],
    stage: None,
    focus: None,
};

/// One shot of a map's camera tour behind the main menu: the camera glides
/// from `from` to `to` while looking at `at`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TourShot {
    pub(crate) from: [f32; 3],
    pub(crate) to: [f32; 3],
    pub(crate) at: [f32; 3],
}

/// A map's camera tour: shots the main menu cuts between, the first the map's
/// own intermission view.
#[derive(Debug, PartialEq)]
pub(crate) struct Tour {
    /// Worldspawn `message` this tour belongs to.
    pub(crate) map_message: &'static str,
    pub(crate) shots: &'static [TourShot],
}

const fn glide(from: [f32; 3], to: [f32; 3], at: [f32; 3]) -> TourShot {
    TourShot { from, to, at }
}

/// mp/duel6, the SJK UI's map: four-fold symmetric round the central
/// courtyard and its tower (at 0 0), quarter gardens between the courtyard's
/// octagonal wall and the four wings, whose upper rooms look down the
/// corridors at the courtyard (the intermission view is the west one). The
/// shots were placed and checked with the off-screen world shots
/// (`world_shot::tests::duel6_tour`); the north wing's (its trees fill the
/// view) and the west garden's (half a dark wall) were left out.
const YAVIN_TRAINING_TOUR: Tour = Tour {
    map_message: "Yavin Training Grounds",
    shots: &[
        // The intermission view, easing forward down the west corridor.
        glide(
            [-2_224.0, 0.0, 888.0],
            [-2_060.0, 0.0, 866.0],
            [-640.0, 0.0, 709.0],
        ),
        // The tower from the south-west garden, high, circling a little.
        glide(
            [-1_150.0, -1_050.0, 700.0],
            [-1_050.0, -1_150.0, 700.0],
            [0.0, 0.0, 300.0],
        ),
        // The east wing's room, sliding across its window.
        glide(
            [2_224.0, 70.0, 888.0],
            [2_224.0, -70.0, 880.0],
            [640.0, 0.0, 709.0],
        ),
        // Up the tower from its foot.
        glide(
            [-290.0, -230.0, 380.0],
            [-230.0, -290.0, 420.0],
            [0.0, 0.0, 760.0],
        ),
        // The tower from the north-east garden.
        glide(
            [1_050.0, 1_150.0, 720.0],
            [1_150.0, 1_050.0, 690.0],
            [0.0, 0.0, 300.0],
        ),
        // Over the courtyard's wall, looking down at the tower's base.
        glide(
            [-610.0, -130.0, 720.0],
            [-570.0, 130.0, 700.0],
            [0.0, 0.0, 300.0],
        ),
        // The south temple's stair, from the courtyard's gate.
        glide(
            [0.0, -1_150.0, 330.0],
            [0.0, -1_290.0, 350.0],
            [0.0, -2_000.0, 520.0],
        ),
        // Down a diagonal path from the tower.
        glide(
            [-380.0, -380.0, 470.0],
            [-480.0, -480.0, 470.0],
            [-1_000.0, -1_000.0, 380.0],
        ),
        // The tower from the south-east garden.
        glide(
            [1_050.0, -1_150.0, 700.0],
            [1_150.0, -1_050.0, 700.0],
            [0.0, 0.0, 300.0],
        ),
        // The south wing's room, sliding across.
        glide(
            [-70.0, -2_224.0, 888.0],
            [70.0, -2_224.0, 880.0],
            [0.0, -640.0, 709.0],
        ),
    ],
};

const TOURS: &[Tour] = &[YAVIN_TRAINING_TOUR];

/// The camera tour authored for the map whose worldspawn message is
/// `message`.
pub(crate) fn tour_for(message: &str) -> Option<&'static [TourShot]> {
    TOURS
        .iter()
        .find(|tour| tour.map_message == message)
        .map(|tour| tour.shots)
}

const ROUTES: &[Route] = &[
    TATOOINE_FFA_BROWSER,
    TATOOINE_FFA_SETTINGS,
    TATOOINE_FFA_PLAYER,
    TATOOINE_FFA_SABER,
    YAVIN_TRAINING_PLAYER,
    YAVIN_TRAINING_SABER,
    YAVIN_TRAINING_HOLOCRONS,
];

/// Route authored for `shot` on the map whose worldspawn message is
/// `message`.
pub(crate) fn route_for(message: &str, shot: Shot) -> Option<&'static Route> {
    ROUTES
        .iter()
        .find(|route| route.map_message == message && route.shot == shot)
}
