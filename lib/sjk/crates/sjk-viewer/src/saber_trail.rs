//! Fixed-capacity stock saber motion trails.

use crate::saber::Extension;
use crate::saber_rgb::BladeColor;
use bytemuck::{Pod, Zeroable};

const MAX_ENTITIES: usize = 1_024;
const SABERS_PER_ENTITY: usize = 2;
const BLADES_PER_SABER: usize = 8;
pub(crate) const MAX_SEGMENTS: usize = 4_096;

#[path = "thrown_saber_tilt.rs"]
pub(crate) mod tilt;

#[path = "saber_trail_edge.rs"]
mod edge;
pub(crate) use edge::{Edge, Edges};

/// One stock trail vertex in the order passed to `FX_AddPrimitive`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub(crate) position: [f32; 3],
    pub(crate) uv: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) style: u32,
    pub(crate) _padding: u32,
}

impl Vertex {
    pub(crate) fn layout() -> wgpu::VertexBufferLayout<'static> {
        const ATTRIBUTES: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x3,
            1 => Float32x2,
            2 => Float32x4,
            3 => Uint32
        ];
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &ATTRIBUTES,
        }
    }
}

/// Four source vertices for one motion slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Quad {
    pub(crate) vertices: [Vertex; 4],
    pub(crate) lifetime_millis: u16,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Trail {
    base: [f32; 3],
    tip: [f32; 3],
    last_time: i64,
    initialized: bool,
}

impl Trail {
    /// Add a slice and then retain the current endpoints exactly like
    /// `CG_AddSaberBlade` (`codemp/cgame/cg_players.c:6297-6319,6400-6470,
    /// 6504-6507`). `edge` is this frame's muzzle and wall-clipped tip
    /// ([`Edges::edge`]). `cg_saberTrail 2` deliberately follows mode 1
    /// because the disabled stencil experiment is outside this renderer.
    pub(crate) fn update(
        &mut self,
        edge: Edge,
        now: i64,
        authored_duration: u16,
        style: u8,
        color: BladeColor,
        enabled: bool,
    ) -> Option<Quad> {
        if style > 1 || now <= self.last_time + 2 {
            return None;
        }
        let Edge { base, tip } = edge;
        let diff = now.saturating_sub(self.last_time);
        let quad = (enabled && self.initialized && now < self.last_time + 2_000)
            .then(|| {
                build_quad(
                    base,
                    tip,
                    self.base,
                    self.tip,
                    diff,
                    authored_duration,
                    style,
                    color,
                )
            })
            .flatten();
        self.base = base;
        self.tip = tip;
        self.last_time = now;
        self.initialized = true;
        quad
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct BladeState {
    pub(crate) extension: Extension,
    pub(crate) trail: Trail,
    /// Its afterimages' poses, for a skin leaving them (`saber_ghosts.rs`).
    pub(crate) ghosts: crate::saber_ghosts::GhostTrail,
}

/// O(1) per-entity/per-saber/per-blade state with one load-time allocation,
/// plus each owner's thrown-saber tilt.
pub(crate) struct StateSlab {
    states: Box<[BladeState]>,
    tilts: Box<[tilt::ThrowTilt]>,
}

impl Default for StateSlab {
    fn default() -> Self {
        Self {
            states: vec![
                BladeState::default();
                MAX_ENTITIES * SABERS_PER_ENTITY * BLADES_PER_SABER
            ]
            .into_boxed_slice(),
            tilts: vec![tilt::ThrowTilt::default(); MAX_ENTITIES].into_boxed_slice(),
        }
    }
}

impl StateSlab {
    pub(crate) fn blade_mut(
        &mut self,
        entity: u64,
        saber: usize,
        blade: usize,
    ) -> Option<&mut BladeState> {
        let entity = usize::try_from(entity.checked_sub(1)?).ok()?;
        let index = entity
            .checked_mul(SABERS_PER_ENTITY * BLADES_PER_SABER)?
            .checked_add(saber.checked_mul(BLADES_PER_SABER)?)?
            .checked_add(blade)?;
        self.states.get_mut(index)
    }

    /// The thrown-saber tilt kept on the owner's entity (`centity_t::bolt3`).
    pub(crate) fn tilt_mut(&mut self, owner_entity: u64) -> Option<&mut tilt::ThrowTilt> {
        self.tilts
            .get_mut(usize::try_from(owner_entity.checked_sub(1)?).ok()?)
    }
}

#[derive(Clone, Copy, Debug)]
struct Segment {
    quad: Quad,
    spawned_at: i64,
    active: bool,
}

impl Default for Segment {
    fn default() -> Self {
        Self {
            quad: Quad {
                vertices: [Vertex::zeroed(); 4],
                lifetime_millis: 0,
            },
            spawned_at: 0,
            active: false,
        }
    }
}

/// Bounded persistent trail slices; exhausted pools drop new slices.
pub(crate) struct SegmentPool {
    segments: Box<[Segment]>,
    cursor: usize,
    dropped: u64,
    inserted: u64,
}

impl Default for SegmentPool {
    fn default() -> Self {
        Self {
            segments: vec![Segment::default(); MAX_SEGMENTS].into_boxed_slice(),
            cursor: 0,
            dropped: 0,
            inserted: 0,
        }
    }
}

impl SegmentPool {
    pub(crate) fn insert(&mut self, quad: Quad, now: i64) -> bool {
        for offset in 0..self.segments.len() {
            let index = (self.cursor + offset) % self.segments.len();
            let segment = &mut self.segments[index];
            let expired = now >= segment.spawned_at + i64::from(segment.quad.lifetime_millis);
            if !segment.active || expired {
                *segment = Segment {
                    quad,
                    spawned_at: now,
                    active: true,
                };
                self.cursor = (index + 1) % self.segments.len();
                self.inserted = self.inserted.saturating_add(1);
                return true;
            }
        }
        self.dropped = self.dropped.saturating_add(1);
        false
    }

    pub(crate) fn append_vertices(&mut self, now: i64, output: &mut Vec<Vertex>) -> usize {
        let start = output.len();
        for segment in &mut self.segments {
            if !segment.active {
                continue;
            }
            let age = now.saturating_sub(segment.spawned_at);
            if age >= i64::from(segment.quad.lifetime_millis) {
                segment.active = false;
                continue;
            }
            // The stock trail never fades its colour: `CTrail::Update`
            // (`codemp/cgame/FxPrimitives.cpp:1771-1789`) scrolls U from `ST`
            // toward `destST = ST + 1` (clamped at 1) over the lifetime, so the
            // clamp-mapped glow texture's dark edge sweeps across the slice.
            // The shader derives that scroll from `1 - color.a`.
            let fade = 1.0 - age as f32 / f32::from(segment.quad.lifetime_millis);
            let vertices = segment.quad.vertices.map(|mut vertex| {
                vertex.color[3] = fade;
                vertex
            });
            output.extend(triangles(vertices));
        }
        (output.len() - start) / 6
    }
}

/// `CTrail::Draw` (`codemp/client/FxPrimitives.cpp`) splits the slice along
/// new tip to old muzzle: (new muzzle, new tip, old muzzle) and (old muzzle,
/// old tip, new tip). The texture coordinates are interpolated per triangle,
/// so the other diagonal smears the fade differently across a slice whose
/// tip moves much further than its muzzle, which is every swing.
fn triangles(vertices: [Vertex; 4]) -> [Vertex; 6] {
    let [new_muzzle, new_tip, old_tip, old_muzzle] = vertices;
    [
        new_muzzle, new_tip, old_muzzle, old_muzzle, old_tip, new_tip,
    ]
}

pub(crate) fn trail_duration(authored_duration: u16) -> u16 {
    let duration = authored_duration / 5;
    if duration == 0 { 40 } else { duration }
}

fn build_quad(
    new_base: [f32; 3],
    new_tip: [f32; 3],
    old_base: [f32; 3],
    old_tip: [f32; 3],
    diff: i64,
    authored_duration: u16,
    style: u8,
    color: BladeColor,
) -> Option<Quad> {
    if diff > 10_000 {
        return None;
    }
    let lifetime_millis = trail_duration(authored_duration);
    let old_alpha = 1.0 - diff as f32 / f32::from(lifetime_millis);
    let rgb = if style == 1 {
        [32.0 / 255.0; 3]
    } else {
        color.trail_rgb()
    };
    let vertex = |position, uv| Vertex {
        position,
        uv,
        color: [rgb[0], rgb[1], rgb[2], 1.0],
        style: u32::from(style),
        _padding: 0,
    };
    Some(Quad {
        vertices: [
            vertex(new_base, [0.0, 1.0]),
            vertex(new_tip, [0.0, 0.0]),
            vertex(old_tip, [1.0 - old_alpha, 0.0]),
            vertex(old_base, [1.0 - old_alpha, 1.0]),
        ],
        lifetime_millis: if style == 1 {
            lifetime_millis.saturating_mul(2)
        } else {
            lifetime_millis
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saber::Color;

    fn edge(x: f32) -> Edge {
        Edge {
            base: [x, 0.0, 0.0],
            tip: [x, 0.0, 44.0],
        }
    }

    fn color() -> BladeColor {
        BladeColor::Retail(Color::Yellow)
    }

    #[test]
    fn slice_spans_the_frame_in_texture_space() {
        let mut trail = Trail::default();
        assert!(
            trail
                .update(edge(0.0), 1_000, 200, 0, color(), true)
                .is_none()
        );
        // Within 2 ms nothing moves, as in stock.
        assert!(
            trail
                .update(edge(1.0), 1_002, 200, 0, color(), true)
                .is_none()
        );
        let quad = trail
            .update(edge(2.0), 1_008, 200, 0, color(), true)
            .expect("a fresh slice");
        assert_eq!(quad.lifetime_millis, 40);
        let [new_muzzle, new_tip, old_tip, old_muzzle] = quad.vertices;
        assert_eq!(new_muzzle.position, [2.0, 0.0, 0.0]);
        assert_eq!(new_tip.position, [2.0, 0.0, 44.0]);
        assert_eq!(old_tip.position, [0.0, 0.0, 44.0]);
        assert_eq!(old_muzzle.position, [0.0, 0.0, 0.0]);
        // ST[0] of the old edge is diff / trailDur = 8 / 40.
        assert!((old_tip.uv[0] - 0.2).abs() < 1e-6);
        assert_eq!(new_muzzle.uv, [0.0, 1.0]);
        assert_eq!(new_tip.uv, [0.0, 0.0]);
    }

    #[test]
    fn idle_moves_keep_the_edge_without_drawing() {
        let mut trail = Trail::default();
        assert!(
            trail
                .update(edge(0.0), 1_000, 0, 0, color(), false)
                .is_none()
        );
        assert!(
            trail
                .update(edge(1.0), 1_010, 0, 0, color(), false)
                .is_none()
        );
        let quad = trail
            .update(edge(2.0), 1_020, 150, 0, color(), true)
            .expect("the first attack frame bridges from the remembered edge");
        assert_eq!(quad.vertices[3].position, [1.0, 0.0, 0.0]);
        assert_eq!(quad.lifetime_millis, 30);
    }

    #[test]
    fn slices_split_along_new_tip_to_old_muzzle() {
        let mut trail = Trail::default();
        trail.update(edge(0.0), 1_000, 200, 0, color(), true);
        let quad = trail
            .update(edge(2.0), 1_008, 200, 0, color(), true)
            .expect("a fresh slice");
        let mut pool = SegmentPool::default();
        assert!(pool.insert(quad, 1_008));
        let mut output = Vec::new();
        assert_eq!(pool.append_vertices(1_008, &mut output), 1);
        let positions: Vec<_> = output.iter().map(|vertex| vertex.position).collect();
        let [new_muzzle, new_tip, old_tip, old_muzzle] = quad.vertices.map(|v| v.position);
        assert_eq!(
            positions,
            [
                new_muzzle, new_tip, old_muzzle, old_muzzle, old_tip, new_tip
            ]
        );
    }

    #[test]
    fn slices_expire_after_their_lifetime() {
        let mut trail = Trail::default();
        trail.update(edge(0.0), 1_000, 200, 0, color(), true);
        let quad = trail
            .update(edge(2.0), 1_008, 200, 0, color(), true)
            .expect("a fresh slice");
        let mut pool = SegmentPool::default();
        pool.insert(quad, 1_008);
        let mut output = Vec::new();
        assert_eq!(pool.append_vertices(1_047, &mut output), 1);
        output.clear();
        assert_eq!(pool.append_vertices(1_048, &mut output), 0);
    }
}
