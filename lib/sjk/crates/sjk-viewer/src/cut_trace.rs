//! Per-frame stderr trace of the view around the cut from the menu world
//! into a joined map.
//!
//! Enabled by `SJK_TRACE_CUT=1`. Prints every one of the first frames of the
//! installed world, and after those any frame in the first seconds whose view,
//! snapshot or size moved, so a wrong camera at the cut can be attributed to
//! the branch that chose it and a later snap to what changed.

use super::GpuState;
use glam::Vec3;
use std::time::{Duration, Instant};

/// How many frames of a freshly installed world are traced unconditionally.
const LIVE_FRAMES: u32 = 40;
/// For how long after the cut changed frames are still traced.
const LIVE_WATCH: Duration = Duration::from_secs(3);
/// Camera or snapshot movement below this (units) is not a change.
const MOVED: f32 = 0.5;

/// What the last printed live frame showed, to print only changes after it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shown {
    branch: &'static str,
    position: Vec3,
    target: Vec3,
    snapshot: Vec3,
    flags: u8,
    entity_flags: u32,
    size: (u32, u32),
    field_of_view: f32,
}

impl Shown {
    fn differs(&self, other: &Self) -> bool {
        self.branch != other.branch
            || self.position.distance(other.position) > MOVED
            || self.target.distance(other.target) > MOVED
            || self.snapshot.distance(other.snapshot) > MOVED
            || self.flags != other.flags
            || self.entity_flags != other.entity_flags
            || self.size != other.size
            || self.field_of_view != other.field_of_view
    }
}

#[derive(Debug)]
pub(crate) struct CutTrace {
    enabled: bool,
    frames: u32,
    first_live: Option<Instant>,
    shown: Option<Shown>,
}

impl CutTrace {
    pub(crate) fn new() -> Self {
        Self {
            enabled: std::env::var_os("SJK_TRACE_CUT").is_some(),
            frames: 0,
            first_live: None,
            shown: None,
        }
    }
}

/// Print this frame's view when it lies inside the traced window.
pub(crate) fn tick(gpu: &mut GpuState, branch: &'static str, view: (Vec3, Vec3), now: Instant) {
    if !gpu.cut_trace.enabled || gpu.is_menu_world {
        return;
    }
    let millis = now.duration_since(gpu.ui_epoch).as_millis() as u64;
    let (position, target) = view;
    let cam = gpu.camera_position;
    let Some(session) = gpu.live_session.as_ref() else {
        return;
    };
    let first_live = *gpu.cut_trace.first_live.get_or_insert(now);
    if now.duration_since(first_live) > LIVE_WATCH {
        return;
    }
    let snapshot = session.latest_snapshot();
    let shown = Shown {
        branch,
        position,
        target,
        snapshot: Vec3::from_array(snapshot.player.origin()),
        flags: snapshot.flags,
        entity_flags: snapshot.player.entity_flags(),
        size: (gpu.size.width, gpu.size.height),
        field_of_view: gpu.field_of_view,
    };
    let dense = gpu.cut_trace.frames < LIVE_FRAMES;
    if !dense
        && gpu
            .cut_trace
            .shown
            .is_some_and(|last| !last.differs(&shown))
    {
        gpu.cut_trace.frames += 1;
        return;
    }
    gpu.cut_trace.frames += 1;
    gpu.cut_trace.shown = Some(shown);
    let offset = gpu.local_prediction.view_offset();
    let since_cut = now.duration_since(first_live).as_secs_f64() * 1_000.0;
    eprintln!(
        "cut trace: live frame={} +{since_cut:.1}ms t={millis} branch={branch} \
         view=({:.1},{:.1},{:.1})->({:.1},{:.1},{:.1}) \
         cam=({:.1},{:.1},{:.1}) yaw={:.3} pitch={:.3} \
         snap=({:.1},{:.1},{:.1}) snap_time={} flags={} eflags={:#x} \
         error=({:.1},{:.1},{:.1}) third_person={} fov={} size={}x{}",
        gpu.cut_trace.frames,
        position.x,
        position.y,
        position.z,
        target.x,
        target.y,
        target.z,
        cam.x,
        cam.y,
        cam.z,
        gpu.camera_yaw,
        gpu.camera_pitch,
        shown.snapshot.x,
        shown.snapshot.y,
        shown.snapshot.z,
        snapshot.server_time,
        shown.flags,
        shown.entity_flags,
        offset.x,
        offset.y,
        offset.z,
        gpu.third_person,
        shown.field_of_view,
        shown.size.0,
        shown.size.1,
    );
}
