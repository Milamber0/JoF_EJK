//! Where weather can be, column by column: the rain cover of a map.
//!
//! For a vertical column of the world SJK finds the open air under the sky and the
//! surface the weather lands on. Rain, snow and mist are drawn only between the two,
//! so they stop on roofs, ledges and the ground, never fall through a ceiling, and
//! never show indoors. This replaces the reference's outside test, which only knew
//! what mappers marked by hand (a map without `system/inside` or `system/outside`
//! brushes rains everywhere there); those marks are honoured as well, as the
//! reference reads them.
//!
//! A column is surveyed with the map's collision data:
//!
//! 1. From the top of the world down, the first point in open air (a leaf of a
//!    visibility cluster, inside no solid brush) whose upward trace hits something.
//!    Leaves outside the map have no cluster and are skipped whole.
//! 2. That trace must end on a sky surface (`SURF_SKY`); any other ceiling covers the
//!    column and nothing is drawn in it.
//! 3. A downward trace from there, against solids and liquids, finds the floor: the
//!    first roof, ledge, ground or water surface under the sky.
//! 4. Inside and outside brushes (`CONTENTS_INSIDE`, `CONTENTS_OUTSIDE`) in the
//!    weather zones trim that span as `COutside` would (`tr_WorldEffects.cpp:379-702`).
//!
//! Brush entities (doors, lifts, `func_static`) are not surveyed: they move, and the
//! survey is made once per map.

use sjk_bsp::{Aabb, Bsp, TraceScratch};

/// `CONTENTS_SOLID`, `CONTENTS_LAVA`, `CONTENTS_WATER`, `CONTENTS_SLIME`.
const CONTENTS_SOLID: u32 = 0x1;
const CONTENTS_LIQUID: u32 = 0x2 | 0x4 | 0x2_0000;
/// `CONTENTS_OUTSIDE`, `CONTENTS_INSIDE`.
const CONTENTS_OUTSIDE: u32 = 0x1_0000;
const CONTENTS_INSIDE: u32 = 0x1000_0000;
/// `SURF_SKY`.
pub(crate) const SURF_SKY: u32 = 0x2000;

/// Steps through solid detail brushes inside a cluster.
const SOLID_STEP: f32 = 16.0;
/// Most points one column visits on its way down.
const MAX_STEPS: usize = 1024;

/// The span of one column where weather can be.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Column {
    /// Where falling weather stops.
    pub(crate) bottom: f32,
    /// The sky, or where the marked outside ends.
    pub(crate) top: f32,
    /// [`Column::SPLASH`], [`Column::LIQUID`].
    pub(crate) flags: u32,
}

impl Column {
    /// Weather ends on a surface there (not at the edge of an inside brush).
    pub(crate) const SPLASH: u32 = 1;
    /// That surface is water, slime or lava.
    pub(crate) const LIQUID: u32 = 2;
    /// The column holds none of the map's air: it lies beyond its walls, or is not
    /// surveyed yet. The fog reads the far cover there instead.
    pub(crate) const VOID: u32 = 4;

    /// No weather anywhere in the column. The empty span (bottom above top) is what the
    /// shader reads for it.
    pub(crate) const COVERED: Self = Self {
        bottom: f32::MAX,
        top: f32::MIN,
        flags: 0,
    };

    /// No weather, and none of the map's air ([`Column::VOID`]).
    pub(crate) const OUTSIDE_MAP: Self = Self {
        flags: Self::VOID,
        ..Self::COVERED
    };

    /// Weather can be somewhere in the column.
    pub(crate) fn is_open(self) -> bool {
        self.bottom <= self.top
    }

    /// The texel the GPU reads: bottom, top, flags.
    pub(crate) fn texel(self) -> [f32; 4] {
        [self.bottom, self.top, self.flags as f32, 0.0]
    }
}

/// Which kind of mark the map uses (`COutside::SWeatherZone::mMarkedOutside`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MarkKind {
    /// Weather only inside the marked outside brushes of the zones.
    Outside,
    /// Weather everywhere but the marked inside brushes of the zones.
    Inside,
}

/// A convex marked brush: its planes and its box.
#[derive(Clone, Debug)]
struct MarkedBrush {
    planes: Vec<([f32; 3], f32)>,
    bounds: [[f32; 3]; 2],
}

impl MarkedBrush {
    /// The part of the vertical line through `(x, y)` inside the brush.
    fn span(&self, x: f32, y: f32) -> Option<[f32; 2]> {
        let [low, high] = self.bounds;
        if x < low[0] || x > high[0] || y < low[1] || y > high[1] {
            return None;
        }
        let (mut bottom, mut top) = (f32::NEG_INFINITY, f32::INFINITY);
        for &(normal, distance) in &self.planes {
            // Inside: normal · p <= distance, with p = (x, y, z).
            let rest = distance - normal[0] * x - normal[1] * y;
            if normal[2].abs() < 1e-6 {
                if rest < 0.0 {
                    return None;
                }
            } else if normal[2] > 0.0 {
                top = top.min(rest / normal[2]);
            } else {
                bottom = bottom.max(rest / normal[2]);
            }
        }
        (bottom < top).then_some([bottom, top])
    }
}

/// The map's inside or outside marks, limited to its weather zones.
#[derive(Clone, Debug, Default)]
pub(crate) struct Marks {
    kind: Option<MarkKind>,
    zones: Vec<[[f32; 3]; 2]>,
    brushes: Vec<MarkedBrush>,
}

impl Marks {
    /// Read the marked brushes of the world model. `zones` are the map's
    /// `misc_weather_zone` boxes and the `zone` commands; none means the whole world,
    /// as the reference's `COutside::Cache` assumes. The reference refuses a map with
    /// both kinds of mark; here the more common kind wins.
    pub(crate) fn read(bsp: &Bsp, zones: &[[[f32; 3]; 2]]) -> Self {
        let world = bsp
            .render()
            .models()
            .first()
            .map(|model| model.brushes.clone());
        let mut brushes = [Vec::new(), Vec::new()];
        for brush in bsp.brushes().get(world.unwrap_or(0..0)).unwrap_or_default() {
            let slot = if brush.content_flags & CONTENTS_OUTSIDE != 0 {
                0
            } else if brush.content_flags & CONTENTS_INSIDE != 0 {
                1
            } else {
                continue;
            };
            let planes: Vec<_> = bsp.brush_sides()[brush.sides.clone()]
                .iter()
                .map(|side| {
                    let plane = bsp.planes()[side.plane];
                    (plane.normal, plane.distance)
                })
                .collect();
            let bounds = axial_bounds(&planes);
            brushes[slot].push(MarkedBrush { planes, bounds });
        }
        let [outside, inside] = brushes;
        let (kind, brushes) = match (outside.len(), inside.len()) {
            (0, 0) => return Self::default(),
            (outside_count, inside_count) if outside_count >= inside_count => {
                (MarkKind::Outside, outside)
            }
            _ => (MarkKind::Inside, inside),
        };
        let zones = if zones.is_empty() {
            bsp.render()
                .models()
                .first()
                .map(|model| vec![[model.minimums, model.maximums]])
                .unwrap_or_default()
        } else {
            zones.to_vec()
        };
        Self {
            kind: Some(kind),
            zones,
            brushes,
        }
    }

    /// The marked spans of the column through `(x, y)`, within the zones, appended to
    /// `spans` in no order.
    fn spans(&self, x: f32, y: f32, spans: &mut Vec<[f32; 2]>) {
        for brush in &self.brushes {
            let Some([bottom, top]) = brush.span(x, y) else {
                continue;
            };
            for [low, high] in &self.zones {
                if x < low[0] || x > high[0] || y < low[1] || y > high[1] {
                    continue;
                }
                let span = [bottom.max(low[2]), top.min(high[2])];
                if span[0] < span[1] {
                    spans.push(span);
                }
            }
        }
    }

    /// Trim the open span `[floor, sky]` of a column by the marks: the highest part of it
    /// weather may be in. `floor_flags` are the floor's [`Column`] flags.
    fn trim(&self, x: f32, y: f32, floor: f32, sky: f32, floor_flags: u32) -> Column {
        let Some(kind) = self.kind else {
            return Column {
                bottom: floor,
                top: sky,
                flags: floor_flags,
            };
        };
        let mut spans = Vec::new();
        self.spans(x, y, &mut spans);
        trim_spans(kind, &mut spans, floor, sky, floor_flags)
    }
}

/// [`Marks::trim`] on the marked spans of one column.
fn trim_spans(
    kind: MarkKind,
    spans: &mut [[f32; 2]],
    floor: f32,
    sky: f32,
    floor_flags: u32,
) -> Column {
    // Highest first; overlapping spans merge as they are walked.
    spans.sort_by(|a, b| b[1].total_cmp(&a[1]));
    match kind {
        // Weather falls from the sky until it meets an inside brush or the floor.
        MarkKind::Inside => {
            let mut top = sky;
            let mut bottom = floor;
            for &[low, high] in spans.iter() {
                if high >= top {
                    // The span covers the current top: weather starts below it.
                    top = top.min(low);
                } else {
                    bottom = bottom.max(high);
                    break;
                }
            }
            if bottom >= top {
                return Column::COVERED;
            }
            let flags = if bottom == floor { floor_flags } else { 0 };
            Column { bottom, top, flags }
        }
        // Weather only within the highest marked outside run below the sky.
        MarkKind::Outside => {
            let mut run: Option<[f32; 2]> = None;
            for &[low, high] in spans.iter() {
                let [low, high] = [low.max(floor), high.min(sky)];
                if low >= high {
                    continue;
                }
                match &mut run {
                    None => run = Some([low, high]),
                    Some(run) if high >= run[0] => run[0] = run[0].min(low),
                    Some(_) => break,
                }
            }
            let Some([bottom, top]) = run else {
                return Column::COVERED;
            };
            let flags = if bottom == floor { floor_flags } else { 0 };
            Column { bottom, top, flags }
        }
    }
}

/// The box of a brush from its axial planes (every compiled brush has the six); an
/// axis without one is unbounded.
fn axial_bounds(planes: &[([f32; 3], f32)]) -> [[f32; 3]; 2] {
    let mut bounds = [[f32::NEG_INFINITY; 3], [f32::INFINITY; 3]];
    for &(normal, distance) in planes {
        for axis in 0..3 {
            if normal[axis] == 1.0 {
                bounds[1][axis] = bounds[1][axis].min(distance);
            } else if normal[axis] == -1.0 {
                bounds[0][axis] = bounds[0][axis].max(-distance);
            }
        }
    }
    bounds
}

/// Surveys columns of one map; owned by the cover worker.
pub(crate) struct Surveyor {
    bsp: std::sync::Arc<Bsp>,
    marks: Marks,
    scratch: TraceScratch,
    /// The world's vertical extent, a little beyond its model's box.
    heights: [f32; 2],
    /// The map has a sky at all. Without one every open point is outside, as in the
    /// reference without marks.
    has_sky: bool,
}

impl Surveyor {
    pub(crate) fn new(bsp: std::sync::Arc<Bsp>, marks: Marks) -> Self {
        let heights = bsp
            .render()
            .models()
            .first()
            .map_or([-65536.0, 65536.0], |model| {
                [model.minimums[2] - 8.0, model.maximums[2] + 8.0]
            });
        let has_sky = bsp
            .shaders()
            .iter()
            .any(|shader| shader.surface_flags & SURF_SKY != 0);
        Self {
            scratch: bsp.trace_scratch(),
            bsp,
            marks,
            heights,
            has_sky,
        }
    }

    /// The map has a sky: without one the cover is not used at all.
    pub(crate) fn has_sky(&self) -> bool {
        self.has_sky
    }

    /// Survey the vertical column through `(x, y)`.
    pub(crate) fn column(&mut self, x: f32, y: f32) -> Column {
        let [bottom_limit, top_limit] = self.heights;
        let mut z = top_limit;
        for _ in 0..MAX_STEPS {
            if z < bottom_limit {
                break;
            }
            let point = [x, y, z];
            let leaf = &self.bsp.leaves()[self.bsp.leaf_at(point)];
            if leaf.cluster < 0 {
                // Outside the map or in structural solid: skip the whole leaf.
                z = (z - 1.0).min(leaf.minimums[2] as f32 - 1.0);
                continue;
            }
            if self.bsp.point_contents(point, CONTENTS_SOLID) != 0 {
                z -= SOLID_STEP;
                continue;
            }
            let up = self.trace(point, [x, y, top_limit + 64.0], CONTENTS_SOLID);
            if up.fraction >= 1.0 {
                // Open to the top of the world without a sky: past the map's edge.
                z -= SOLID_STEP;
                continue;
            }
            if up.surface_flags & SURF_SKY == 0 {
                return Column::COVERED;
            }
            let sky = up.end_position[2];
            let down = self.trace(
                point,
                [x, y, bottom_limit],
                CONTENTS_SOLID | CONTENTS_LIQUID,
            );
            let floor = down.end_position[2];
            let mut flags = 0;
            if down.fraction < 1.0 {
                flags |= Column::SPLASH;
                if down.content_flags & CONTENTS_LIQUID != 0 {
                    flags |= Column::LIQUID;
                }
            }
            return self.marks.trim(x, y, floor, sky, flags);
        }
        Column::OUTSIDE_MAP
    }

    fn trace(&mut self, start: [f32; 3], end: [f32; 3], mask: u32) -> sjk_bsp::CollisionTrace {
        self.bsp
            .trace_box_with(&mut self.scratch, start, end, Aabb::POINT, mask)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_bsp::{CollisionShader, box_brush, write_collision_map};

    fn shader(name: &str, surface_flags: u32, content_flags: u32) -> CollisionShader {
        CollisionShader {
            name: name.into(),
            surface_flags,
            content_flags,
        }
    }

    /// A courtyard: ground, four walls, a sky lid, a roofed hut in one corner, a pool
    /// and an inside-marked porch.
    fn courtyard(marks: bool) -> std::sync::Arc<Bsp> {
        let shaders = [
            shader("textures/stone", 0, CONTENTS_SOLID),
            shader("textures/skies/night", SURF_SKY, CONTENTS_SOLID),
            shader("textures/water", 0, 0x4),
            shader("textures/system/inside", 0, CONTENTS_INSIDE),
        ];
        let mut brushes = vec![
            box_brush([-1024.0, -1024.0, -64.0], [1024.0, 1024.0, 0.0], 0),
            box_brush([-1024.0, -1024.0, 0.0], [-1008.0, 1024.0, 1024.0], 0),
            box_brush([1008.0, -1024.0, 0.0], [1024.0, 1024.0, 1024.0], 0),
            box_brush([-1024.0, -1024.0, 0.0], [1024.0, -1008.0, 1024.0], 0),
            box_brush([-1024.0, 1008.0, 0.0], [1024.0, 1024.0, 1024.0], 0),
            box_brush([-1024.0, -1024.0, 1024.0], [1024.0, 1024.0, 1040.0], 1),
            // The hut's roof, 16 thick, 256 up.
            box_brush([512.0, 512.0, 256.0], [1008.0, 1008.0, 272.0], 0),
            // The pool.
            box_brush([-512.0, -512.0, 0.0], [-256.0, -256.0, 48.0], 2),
        ];
        if marks {
            brushes.push(box_brush([0.0, -512.0, 0.0], [256.0, -256.0, 128.0], 3));
        }
        let data = write_collision_map("{\n\"classname\" \"worldspawn\"\n}\n", &shaders, &brushes);
        std::sync::Arc::new(Bsp::parse(&data).expect("synthetic map parses"))
    }

    fn survey(bsp: &std::sync::Arc<Bsp>, x: f32, y: f32) -> Column {
        let marks = Marks::read(bsp, &[]);
        Surveyor::new(bsp.clone(), marks).column(x, y)
    }

    #[test]
    fn open_ground_takes_weather_from_the_sky_to_the_floor() {
        let bsp = courtyard(false);
        let column = survey(&bsp, 0.0, 0.0);
        assert_eq!(column.flags, Column::SPLASH);
        assert!((column.bottom - 0.0).abs() < 0.5, "{column:?}");
        assert!((column.top - 1024.0).abs() < 0.5, "{column:?}");
    }

    #[test]
    fn a_roof_stops_weather_and_keeps_it_out_from_under_it() {
        let bsp = courtyard(false);
        let column = survey(&bsp, 768.0, 768.0);
        assert!((column.bottom - 272.0).abs() < 0.5, "{column:?}");
        assert!((column.top - 1024.0).abs() < 0.5, "{column:?}");
        // Under the roof is below the column's floor: nothing falls there.
        assert!(column.bottom > 128.0);
    }

    #[test]
    fn rain_lands_on_water_as_water() {
        let bsp = courtyard(false);
        let column = survey(&bsp, -384.0, -384.0);
        assert_eq!(column.flags, Column::SPLASH | Column::LIQUID);
        assert!((column.bottom - 48.0).abs() < 0.5, "{column:?}");
    }

    #[test]
    fn inside_brushes_cut_the_weather_short() {
        let bsp = courtyard(true);
        let column = survey(&bsp, 128.0, -384.0);
        assert!((column.bottom - 128.0).abs() < 0.5, "{column:?}");
        assert_eq!(column.flags, 0, "no splash on an inside brush's edge");
        // Elsewhere the inside mark changes nothing.
        let open = survey(&bsp, 0.0, 256.0);
        assert!((open.bottom - 0.0).abs() < 0.5, "{open:?}");
    }

    #[test]
    fn a_ceiling_that_is_not_sky_covers_the_column() {
        let shaders = [shader("textures/stone", 0, CONTENTS_SOLID)];
        let brushes = [
            box_brush([-256.0, -256.0, -16.0], [256.0, 256.0, 0.0], 0),
            box_brush([-256.0, -256.0, 256.0], [256.0, 256.0, 272.0], 0),
        ];
        let data = write_collision_map("", &shaders, &brushes);
        let bsp = std::sync::Arc::new(Bsp::parse(&data).unwrap());
        let mut surveyor = Surveyor::new(bsp.clone(), Marks::read(&bsp, &[]));
        assert!(!surveyor.has_sky());
        assert_eq!(surveyor.column(0.0, 0.0), Column::COVERED);
    }

    #[test]
    fn a_column_beyond_the_walls_holds_none_of_the_maps_air() {
        let bsp = courtyard(false);
        // Past the east wall: nothing above it, all the way down.
        let column = survey(&bsp, 1100.0, 0.0);
        assert_eq!(column, Column::OUTSIDE_MAP);
        assert!(!column.is_open());
        assert!(survey(&bsp, 0.0, 0.0).is_open());
    }

    #[test]
    fn marks_trim_spans_as_the_reference_classifies_points() {
        let flags = Column::SPLASH;
        // Inside marks: the topmost open run, from the sky down to the first mark.
        let mut spans = [[100.0, 200.0], [300.0, 400.0]];
        let column = trim_spans(MarkKind::Inside, &mut spans, 0.0, 1000.0, flags);
        assert_eq!(
            (column.bottom, column.top, column.flags),
            (400.0, 1000.0, 0)
        );
        // A mark reaching the sky pushes the top down to its bottom.
        let mut spans = [[500.0, 1200.0], [100.0, 200.0]];
        let column = trim_spans(MarkKind::Inside, &mut spans, 0.0, 1000.0, flags);
        assert_eq!((column.bottom, column.top), (200.0, 500.0));
        // Outside marks: the highest marked run, clipped to the open span.
        let mut spans = [[-50.0, 120.0], [100.0, 300.0], [600.0, 700.0]];
        let column = trim_spans(MarkKind::Outside, &mut spans, 0.0, 650.0, flags);
        assert_eq!((column.bottom, column.top, column.flags), (600.0, 650.0, 0));
        let mut spans = [[-50.0, 120.0], [100.0, 300.0]];
        let column = trim_spans(MarkKind::Outside, &mut spans, 0.0, 650.0, flags);
        assert_eq!(
            (column.bottom, column.top, column.flags),
            (0.0, 300.0, flags)
        );
        assert_eq!(
            trim_spans(MarkKind::Outside, &mut [], 0.0, 650.0, flags),
            Column::COVERED
        );
    }
}
