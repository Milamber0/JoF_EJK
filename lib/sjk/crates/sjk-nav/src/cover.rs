//! Places worth taking cover at, found about a graph's nodes: each node, and the spot its
//! actor reaches sliding from it to the nearest wall. A place is cover beside a wall (some
//! of the directions round it are blocked within reach, not all of them), overlooking
//! ground others stand on while hidden, crouched, from some of it. Where the wall is lower
//! than a standing actor's eyes the cover is low: an actor ducks behind it.
//!
//! No game is in it: heights, distances and the actor's box are a [`CoverRules`] the game
//! chooses, and the world answers through [`SweepWorld`].

use crate::walk::SweepWorld;

/// What makes a place cover.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CoverRules {
    /// How near a wall is to count, round the place.
    pub wall_reach: f32,
    /// How far from a node its actor slides to the nearest wall, and the box it slides
    /// with (a little wider than the actor, so that it stops short of the wall).
    pub slide: f32,
    pub slide_mins: [f32; 3],
    pub slide_maxs: [f32; 3],
    /// The height of a crouching and of a standing actor's eyes above a node.
    pub crouch_eye: f32,
    pub stand_eye: f32,
    /// The nearest and furthest another node is to count as a place a threat stands.
    pub near_threat: f32,
    pub far_threat: f32,
    /// How many such places are tried from each node, spread over those in range.
    pub threat_samples: usize,
    /// How close two chosen places may be, and how many are chosen at most.
    pub spacing: f32,
    pub max_points: usize,
}

/// A place chosen: the node it was found from, where it is, and whether its cover is low.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cover {
    pub node: usize,
    pub position: [f32; 3],
    pub low: bool,
}

/// The eight directions round a place, as unit vectors on the floor.
fn compass() -> [[f32; 2]; 8] {
    std::array::from_fn(|at| {
        let angle = at as f32 * std::f32::consts::FRAC_PI_4;
        [angle.cos(), angle.sin()]
    })
}

/// The places about `points` that give cover, best first and no two within
/// `rules.spacing`: each beside a wall, seen from at least one sampled threat place
/// (standing eyes to standing eyes) and hidden from at least one (its crouching eyes);
/// better the more evenly it is both.
pub fn find_cover(
    world: &mut impl SweepWorld,
    points: &[[f32; 3]],
    rules: &CoverRules,
) -> Vec<Cover> {
    let mut scored = Vec::new();
    let mut in_range = Vec::new();
    for (node, &point) in points.iter().enumerate() {
        let slid = slide_to_wall(world, point, rules);
        for place in std::iter::once(point).chain(slid) {
            let Some(low) = beside_wall(world, place, rules) else {
                continue;
            };
            in_range.clear();
            in_range.extend(
                points
                    .iter()
                    .enumerate()
                    .filter(|&(other, &there)| {
                        let distance = crate::distance(place, there);
                        other != node
                            && distance >= rules.near_threat
                            && distance <= rules.far_threat
                    })
                    .map(|(other, _)| other),
            );
            let (seen, hidden) = threats(world, place, points, &in_range, rules);
            if seen > 0 && hidden > 0 {
                scored.push((seen.min(hidden), node, place, low));
            }
        }
    }
    // Best first; among equals, the lower node, then the node's own place.
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    let mut chosen: Vec<Cover> = Vec::new();
    for (_, node, position, low) in scored {
        if chosen.len() >= rules.max_points {
            break;
        }
        if chosen
            .iter()
            .all(|cover| crate::distance(cover.position, position) >= rules.spacing)
        {
            chosen.push(Cover {
                node,
                position,
                low,
            });
        }
    }
    chosen
}

/// Where the actor's box comes to rest slid from `point` toward the nearest wall within
/// `rules.slide` (found at crouching eyes), if it moves at all.
fn slide_to_wall(
    world: &mut impl SweepWorld,
    point: [f32; 3],
    rules: &CoverRules,
) -> Option<[f32; 3]> {
    let crouch = [point[0], point[1], point[2] + rules.crouch_eye];
    let mut nearest: Option<(f32, [f32; 2])> = None;
    for direction in compass() {
        let far = [
            crouch[0] + direction[0] * rules.slide,
            crouch[1] + direction[1] * rules.slide,
            crouch[2],
        ];
        let line = world.sweep(crouch, [0.0; 3], [0.0; 3], far);
        if !line.start_solid
            && line.fraction < 1.0
            && nearest.is_none_or(|(best, _)| line.fraction < best)
        {
            nearest = Some((line.fraction, direction));
        }
    }
    let (_, direction) = nearest?;
    let far = [
        point[0] + direction[0] * rules.slide,
        point[1] + direction[1] * rules.slide,
        point[2],
    ];
    let slide = world.sweep(point, rules.slide_mins, rules.slide_maxs, far);
    if slide.start_solid || crate::distance(point, slide.end) < 8.0 {
        return None;
    }
    // Still over the floor, no further below than the box is tall: not slid off a ledge.
    let end = slide.end;
    let depth = rules.slide_maxs[2] - rules.slide_mins[2];
    let floor = world.sweep(end, [0.0; 3], [0.0; 3], [end[0], end[1], end[2] - depth]);
    (floor.fraction < 1.0).then_some(end)
}

/// Whether `place` stands beside a wall (blocked at crouching eyes in one to five of the
/// eight directions), and if so whether the cover is low (a blocked direction clear at
/// standing eyes).
fn beside_wall(world: &mut impl SweepWorld, place: [f32; 3], rules: &CoverRules) -> Option<bool> {
    let crouch = [place[0], place[1], place[2] + rules.crouch_eye];
    let stand = [place[0], place[1], place[2] + rules.stand_eye];
    let (mut walls, mut low) = (0, false);
    for direction in compass() {
        let reach = |eye: [f32; 3]| {
            [
                eye[0] + direction[0] * rules.wall_reach,
                eye[1] + direction[1] * rules.wall_reach,
                eye[2],
            ]
        };
        if !world.clear_line(crouch, reach(crouch)) {
            walls += 1;
            low |= world.clear_line(stand, reach(stand));
        }
    }
    (1..=5).contains(&walls).then_some(low)
}

/// Of the threat places `in_range` (indices into `points`), sampled evenly: how many see
/// `place` standing, and how many cannot see it crouched.
fn threats(
    world: &mut impl SweepWorld,
    place: [f32; 3],
    points: &[[f32; 3]],
    in_range: &[usize],
    rules: &CoverRules,
) -> (usize, usize) {
    let crouch = [place[0], place[1], place[2] + rules.crouch_eye];
    let stand = [place[0], place[1], place[2] + rules.stand_eye];
    let every = in_range.len().div_ceil(rules.threat_samples.max(1)).max(1);
    let (mut seen, mut hidden) = (0, 0);
    for &other in in_range.iter().step_by(every) {
        let there = points[other];
        let eye = [there[0], there[1], there[2] + rules.stand_eye];
        if world.clear_line(eye, stand) {
            seen += 1;
        }
        if !world.clear_line(eye, crouch) {
            hidden += 1;
        }
    }
    (seen, hidden)
}
