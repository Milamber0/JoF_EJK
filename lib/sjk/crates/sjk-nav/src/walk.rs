//! Walkable floor found in a world's collision, for a level whose game supplies no nodes of
//! its own: an actor's box stepped along a line a stride at a time — up a step (or a jump),
//! across, down onto floor it may stand on — and a lattice of floor samples flooded out from
//! places known to be walkable, each joined to its neighbours where the box can walk there.
//!
//! No game is in it: the box, the step, the drop and the lattice are a [`Walker`] and
//! [`Lattice`] the game chooses, and the world answers through [`SweepWorld`]. Doors and
//! other movers are the game's to judge: a world that leaves them out walks through them,
//! and the game flags the links they stand in.
//!
//! The results are plain lists ([`Walkways`]) a game turns into a [`crate::Graph`].

use std::collections::{HashMap, VecDeque};

use crate::{EDGE_DROP, EDGE_JUMP};

/// What a swept box met.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sweep {
    /// How far along it went, 0 to 1.
    pub fraction: f32,
    /// Where it stopped.
    pub end: [f32; 3],
    /// The normal of what it met (zero where it met nothing).
    pub normal: [f32; 3],
    /// Whether it started inside something solid.
    pub start_solid: bool,
}

/// The world a graph is made in: a box swept through its collision.
pub trait SweepWorld {
    /// The box `mins`..`maxs` swept from `start` to `end` against what stops an actor.
    fn sweep(&mut self, start: [f32; 3], mins: [f32; 3], maxs: [f32; 3], end: [f32; 3]) -> Sweep;

    /// Whether the line from `start` to `end` is clear.
    fn clear_line(&mut self, start: [f32; 3], end: [f32; 3]) -> bool {
        let sweep = self.sweep(start, [0.0; 3], [0.0; 3], end);
        sweep.fraction >= 1.0 && !sweep.start_solid
    }
}

/// How an actor walks: its box, how high it steps and jumps, how far it lets itself drop,
/// the steepest floor it stands on, and the stride it is checked at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Walker {
    /// Its box about its origin.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// The highest ledge it steps up without jumping.
    pub step_height: f32,
    /// The furthest it drops in one stride.
    pub max_drop: f32,
    /// The highest ledge it jumps up; no higher than `step_height` for an actor that never
    /// jumps.
    pub jump_height: f32,
    /// The least upward normal of a floor it stands on.
    pub min_floor_normal: f32,
    /// How far apart the checks along a way are.
    pub stride: f32,
}

/// A way walked: where the box stands at its end, and [`EDGE_DROP`] or [`EDGE_JUMP`]
/// where the way took one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroll {
    pub end: [f32; 3],
    pub flags: u8,
}

impl Walker {
    /// Where the box comes to stand dropped from `point` by at most `depth`: on floor it
    /// may stand on, not starting inside anything.
    pub fn floor_below(
        &self,
        world: &mut impl SweepWorld,
        point: [f32; 3],
        depth: f32,
    ) -> Option<[f32; 3]> {
        let down = world.sweep(
            point,
            self.mins,
            self.maxs,
            [point[0], point[1], point[2] - depth],
        );
        (!down.start_solid && down.fraction < 1.0 && down.normal[2] >= self.min_floor_normal)
            .then_some(down.end)
    }

    /// The box walked from `from` (standing) to above `to`'s place, a stride at a time:
    /// each stride lifted a step, carried across, and set down on floor no more than
    /// `max_drop` below; where a step does not clear what is in the way, a jump may.
    /// `None` where the way is blocked, runs over a gap or onto too steep a floor.
    pub fn walk(
        &self,
        world: &mut impl SweepWorld,
        from: [f32; 3],
        to: [f32; 2],
    ) -> Option<Stroll> {
        let across = [to[0] - from[0], to[1] - from[1]];
        let length = across[0].hypot(across[1]);
        let strides = ((length / self.stride).ceil() as usize).max(1);
        let (mut at, mut flags) = (from, 0);
        for stride in 1..=strides {
            let share = stride as f32 / strides as f32;
            let target = [from[0] + across[0] * share, from[1] + across[1] * share];
            let (landed, jumped) = self.stride(world, at, target)?;
            if jumped {
                flags |= EDGE_JUMP;
            }
            if landed[2] < at[2] - self.step_height {
                flags |= EDGE_DROP;
            }
            at = landed;
        }
        Some(Stroll { end: at, flags })
    }

    /// One stride from `at` to above `target`: stepped, else jumped. Where it lands and
    /// whether it jumped.
    fn stride(
        &self,
        world: &mut impl SweepWorld,
        at: [f32; 3],
        target: [f32; 2],
    ) -> Option<([f32; 3], bool)> {
        let lifts = [(self.step_height, false), (self.jump_height, true)];
        for (lift, jump) in lifts {
            if jump && lift <= self.step_height {
                break;
            }
            let up = world.sweep(at, self.mins, self.maxs, [at[0], at[1], at[2] + lift]);
            if up.start_solid {
                return None;
            }
            let top = up.end;
            let over = world.sweep(top, self.mins, self.maxs, [target[0], target[1], top[2]]);
            if over.start_solid || over.fraction < 1.0 {
                continue;
            }
            let landed = self.floor_below(world, over.end, lift + self.max_drop)?;
            // A jump that lands no higher than a step would have is not one.
            if jump && landed[2] <= at[2] + self.step_height {
                return None;
            }
            return Some((landed, jump));
        }
        None
    }
}

/// The floor lattice: how far apart its samples are, how many it makes at most, and how
/// close in height two samples of one column are before they are one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Lattice {
    pub spacing: f32,
    pub max_nodes: usize,
    pub merge_height: f32,
}

/// A one-way link between two of [`Walkways::points`], with its `EDGE_*` flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Link {
    pub from: u32,
    pub to: u32,
    pub flags: u8,
}

/// Walkable places and the ways between them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Walkways {
    /// Where the walker's box stands at each.
    pub points: Vec<[f32; 3]>,
    /// One-way links: a way both ways is two.
    pub links: Vec<Link>,
}

impl Walkways {
    /// Whether `from` already links to `to`.
    pub fn linked(&self, from: u32, to: u32) -> bool {
        self.links
            .iter()
            .any(|link| link.from == from && link.to == to)
    }
}

/// The eight neighbouring columns, in the order they are tried.
const NEIGHBOURS: [(i32, i32); 8] = [
    (1, 0),
    (0, 1),
    (-1, 0),
    (0, -1),
    (1, 1),
    (-1, 1),
    (-1, -1),
    (1, -1),
];

/// The floor lattice flooded out from `seeds` (places the walker is known to stand near):
/// each seed set on the floor below it, then every sample tried against the eight
/// neighbouring columns — at the column's middle, else a quarter of the spacing off it
/// on each axis — and linked to the sample it walks to there (one already in that column
/// at about that height, or a new one while there is room). Deterministic: the same world
/// and seeds make the same walkways.
pub fn flood_lattice(
    world: &mut impl SweepWorld,
    walker: &Walker,
    lattice: &Lattice,
    seeds: &[[f32; 3]],
) -> Walkways {
    let mut ways = Walkways::default();
    let mut columns: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
    let mut queue = VecDeque::new();
    let column = |point: [f32; 3]| {
        (
            (point[0] / lattice.spacing).floor() as i32,
            (point[1] / lattice.spacing).floor() as i32,
        )
    };
    for &seed in seeds {
        // A seed may stand on its floor to the unit (a spawn point does): lifted a step first.
        let lifted = [seed[0], seed[1], seed[2] + walker.step_height];
        let Some(floor) = walker.floor_below(world, lifted, 256.0 + walker.step_height) else {
            continue;
        };
        if ways.points.len() >= lattice.max_nodes
            || sample_near(
                &ways,
                &columns,
                column(floor),
                floor[2],
                lattice.merge_height,
            )
            .is_some()
        {
            continue;
        }
        let id = ways.points.len() as u32;
        ways.points.push(floor);
        columns.entry(column(floor)).or_default().push(id);
        queue.push_back(id);
    }
    let quarter = lattice.spacing * 0.25;
    let nudges = [
        (0.0, 0.0),
        (quarter, 0.0),
        (-quarter, 0.0),
        (0.0, quarter),
        (0.0, -quarter),
    ];
    while let Some(id) = queue.pop_front() {
        let from = ways.points[id as usize];
        let (x, y) = column(from);
        for (dx, dy) in NEIGHBOURS {
            let next = (x + dx, y + dy);
            let middle = [
                (next.0 as f32 + 0.5) * lattice.spacing,
                (next.1 as f32 + 0.5) * lattice.spacing,
            ];
            for (nx, ny) in nudges {
                let Some(stroll) = walker.walk(world, from, [middle[0] + nx, middle[1] + ny])
                else {
                    continue;
                };
                if column(stroll.end) != next {
                    continue;
                }
                let to =
                    match sample_near(&ways, &columns, next, stroll.end[2], lattice.merge_height) {
                        Some(existing) => {
                            // The sample there may stand elsewhere in the column: walked to itself.
                            let there = ways.points[existing as usize];
                            match walker.walk(world, from, [there[0], there[1]]) {
                                Some(way)
                                    if (way.end[2] - there[2]).abs() < lattice.merge_height =>
                                {
                                    if !ways.linked(id, existing) {
                                        ways.links.push(Link {
                                            from: id,
                                            to: existing,
                                            flags: way.flags,
                                        });
                                    }
                                    Some(existing)
                                }
                                _ => None,
                            }
                        }
                        None if ways.points.len() < lattice.max_nodes => {
                            let new = ways.points.len() as u32;
                            ways.points.push(stroll.end);
                            columns.entry(next).or_default().push(new);
                            queue.push_back(new);
                            ways.links.push(Link {
                                from: id,
                                to: new,
                                flags: stroll.flags,
                            });
                            Some(new)
                        }
                        None => None,
                    };
                if to.is_some() {
                    break;
                }
            }
        }
    }
    ways
}

/// The sample of `column` within `merge_height` of `height`, if there is one.
fn sample_near(
    ways: &Walkways,
    columns: &HashMap<(i32, i32), Vec<u32>>,
    column: (i32, i32),
    height: f32,
    merge_height: f32,
) -> Option<u32> {
    columns
        .get(&column)?
        .iter()
        .copied()
        .find(|&id| (ways.points[id as usize][2] - height).abs() < merge_height)
}

/// `candidates` (pairs of `points`) kept where the walker walks from the first to the
/// second and comes to stand within `tolerance` of it in height; flagged as the walk went.
pub fn walked_links(
    world: &mut impl SweepWorld,
    walker: &Walker,
    points: &[[f32; 3]],
    candidates: &[(u32, u32)],
    tolerance: f32,
) -> Vec<Link> {
    let mut links = Vec::new();
    for &(from, to) in candidates {
        let (Some(&start), Some(&end)) = (points.get(from as usize), points.get(to as usize))
        else {
            continue;
        };
        if from == to
            || links
                .iter()
                .any(|link: &Link| link.from == from && link.to == to)
        {
            continue;
        }
        let Some(start) = walker.floor_below(world, start, walker.step_height + walker.max_drop)
        else {
            continue;
        };
        if let Some(way) = walker.walk(world, start, [end[0], end[1]])
            && (way.end[2] - end[2]).abs() <= tolerance
        {
            links.push(Link {
                from,
                to,
                flags: way.flags,
            });
        }
    }
    links
}
