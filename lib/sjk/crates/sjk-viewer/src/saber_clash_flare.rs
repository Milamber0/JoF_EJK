//! View projection and BSP visibility for codemp's screen-space clash flare.

use glam::{Mat4, Vec3};
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_client::{LegacySaberClashFlare, LegacySaberClashVisibility};

const CONTENTS_SOLID: u32 = 1; // codemp/qcommon/surfaceflags.h:18

/// Screen-space quad consumed by the existing additive particle pipeline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProjectedFlare {
    pub(crate) center_ndc: [f32; 2],
    pub(crate) half_extent_ndc: [f32; 2],
    pub(crate) picture_scale: f32,
}

/// Evaluate the legacy flare and project its virtual 640x480 picture rect.
///
/// The point trace reuses `Bsp::trace_box`, the same collision implementation
/// used by pmove and the third-person camera; no parallel trace exists here.
#[allow(clippy::too_many_arguments)]
pub(crate) fn project(
    flare: &LegacySaberClashFlare,
    cg_time: i32,
    view_origin: Vec3,
    view_forward: Vec3,
    view_projection: Mat4,
    aspect: f32,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
) -> (LegacySaberClashVisibility, Option<ProjectedFlare>) {
    let bounds = Aabb::new([0.0; 3], [0.0; 3]).expect("zero point bounds are valid");
    let sample = flare.sample(
        cg_time,
        view_origin.to_array(),
        view_forward.to_array(),
        |start, end| {
            bsp.trace_box_with(scratch, start, end, bounds, CONTENTS_SOLID)
                .fraction
        },
    );
    if sample.visibility != LegacySaberClashVisibility::Visible {
        return (sample.visibility, None);
    }
    let clip = view_projection * Vec3::from_array(sample.position).extend(1.0);
    if clip.w <= 0.01 {
        return (LegacySaberClashVisibility::BehindView, None);
    }
    let center = clip.truncate() / clip.w;
    // CG_DrawPic uses virtual 640x480 coordinates. x/y are each scaled to the
    // current framebuffer by the UI transform, so these NDC extents are
    // resolution independent (cg_draw.c:5397-5399).
    // EternalJK scales the picture's width by `cgs.widthRatioCoef` (640x480 over the
    // window's shape), so the flare stays round on a wide screen instead of
    // stretching with the virtual 640-wide canvas.
    let width_ratio = if aspect > 0.0 {
        (4.0 / 3.0) / aspect
    } else {
        1.0
    };
    let half_extent_ndc = [
        sample.picture_scale * 300.0 / 320.0 * width_ratio,
        sample.picture_scale * 300.0 / 240.0,
    ];
    (
        LegacySaberClashVisibility::Visible,
        Some(ProjectedFlare {
            center_ndc: [center.x, center.y],
            half_extent_ndc,
            picture_scale: sample.picture_scale,
        }),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn append(
    flare: &LegacySaberClashFlare,
    cg_time: i32,
    view_origin: Vec3,
    view_forward: Vec3,
    view_projection: Mat4,
    aspect: f32,
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    atlas: &crate::ParticleAtlas,
    output: &mut Vec<crate::EntityInstance>,
) {
    let (_, projected) = project(
        flare,
        cg_time,
        view_origin,
        view_forward,
        view_projection,
        aspect,
        bsp,
        scratch,
    );
    let Some(projected) = projected else {
        return;
    };
    let layer = atlas.first_layer("gfx/effects/saberFlare", 0.0);
    output.push(crate::EntityInstance {
        position: [projected.center_ndc[0], projected.center_ndc[1], 0.0],
        kind: 6,
        size: 1.0,
        alpha: layer.alpha,
        uv_rect: layer.uv_rect,
        color: [0.8 * layer.rgb, 0.8 * layer.rgb, 0.8 * layer.rgb, 1.0],
        direction: [
            projected.half_extent_ndc[0],
            projected.half_extent_ndc[1],
            0.0,
        ],
        rotation: 0.0,
        uv_transform: layer.uv_transform,
    });
}
