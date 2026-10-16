//! `camera(...)` (`CTaskManager::Camera`): read and reported, then refused — the
//! multiplayer engine answers every camera call with "NOT SUPPORTED IN MP"
//! (`Q3_Interface.cpp`'s `CGCam_*`, `Q3_CameraFade`, `Q3_CameraPath`), and so does this.

use crate::Icarus;
use crate::cnum::format_f;
use crate::host::{DebugLevel, IcarusHost, Owner};
use crate::ids::*;
use crate::print;
use crate::tasks::Task;

fn vector(value: [f32; 3]) -> String {
    format!(
        "<{} {} {}>",
        format_f(value[0]),
        format_f(value[1]),
        format_f(value[2])
    )
}

/// Runs a camera command: its members read as the reference reads them, the command
/// printed, the refusal printed, the task completed.
pub(crate) fn run<O: Owner, H: IcarusHost<O> + ?Sized>(
    icarus: &mut Icarus<O>,
    owner: O,
    task: &Task,
    host: &mut H,
) {
    let block = &task.block;
    let mut member = 0;
    let Some(kind) = icarus.camera_float(owner, block, &mut member, host) else {
        return;
    };
    let stamp = task.time_stamp;
    let float = |icarus: &mut Icarus<O>, host: &mut H, member: &mut usize| {
        icarus.camera_float(owner, block, member, host)
    };
    let refusal = "Camera functions NOT SUPPORTED IN MP\n";
    let (line, warning) = match kind as i32 {
        TYPE_PAN => {
            let Some(angles) = icarus.camera_vector(owner, block, &mut member, host) else {
                return;
            };
            let Some(direction) = icarus.camera_vector(owner, block, &mut member, host) else {
                return;
            };
            let Some(duration) = float(icarus, host, &mut member) else {
                return;
            };
            (
                format!(
                    "camera( PAN, {}, {}, {}); [{stamp}]",
                    vector(angles),
                    vector(direction),
                    format_f(duration)
                ),
                refusal,
            )
        }
        TYPE_ZOOM | TYPE_ROLL | TYPE_DISTANCE | TYPE_SHAKE => {
            let Some(first) = float(icarus, host, &mut member) else {
                return;
            };
            let Some(second) = float(icarus, host, &mut member) else {
                return;
            };
            let (first, second) = (format_f(first), format_f(second));
            let line = match kind as i32 {
                TYPE_ZOOM => format!("camera( ZOOM, {first}, {second}); [{stamp}]"),
                TYPE_ROLL => format!("camera( ROLL, {first}, {second}); [{stamp}]"),
                TYPE_DISTANCE => format!("camera( DISTANCE, {first}, {second}); [{stamp}]"),
                _ => format!("camera( SHAKE, {first}, {second} ); [{stamp}]"),
            };
            (line, refusal)
        }
        TYPE_MOVE => {
            let Some(origin) = icarus.camera_vector(owner, block, &mut member, host) else {
                return;
            };
            let Some(duration) = float(icarus, host, &mut member) else {
                return;
            };
            (
                format!(
                    "camera( MOVE, {}, {}); [{stamp}]",
                    vector(origin),
                    format_f(duration)
                ),
                refusal,
            )
        }
        TYPE_FOLLOW | TYPE_TRACK => {
            let Some(name) = icarus.camera_text(owner, block, &mut member, host) else {
                return;
            };
            let Some(speed) = float(icarus, host, &mut member) else {
                return;
            };
            let Some(lerp) = float(icarus, host, &mut member) else {
                return;
            };
            let which = if kind as i32 == TYPE_FOLLOW {
                "FOLLOW"
            } else {
                "TRACK"
            };
            (
                format!(
                    "camera( {which}, \"{name}\", {}, {}); [{stamp}]",
                    format_f(speed),
                    format_f(lerp)
                ),
                refusal,
            )
        }
        TYPE_FADE => {
            let Some(source) = icarus.camera_vector(owner, block, &mut member, host) else {
                return;
            };
            let Some(source_alpha) = float(icarus, host, &mut member) else {
                return;
            };
            let Some(destination) = icarus.camera_vector(owner, block, &mut member, host) else {
                return;
            };
            let Some(destination_alpha) = float(icarus, host, &mut member) else {
                return;
            };
            let Some(duration) = float(icarus, host, &mut member) else {
                return;
            };
            let line = format!(
                "camera( FADE, {}, {}, {}, {}, {}); [{stamp}]",
                vector(source),
                format_f(source_alpha),
                vector(destination),
                format_f(destination_alpha),
                format_f(duration)
            );
            (line, "Q3_CameraFade: NOT SUPPORTED IN MP\n")
        }
        TYPE_PATH => {
            let Some(name) = icarus.camera_text(owner, block, &mut member, host) else {
                return;
            };
            (
                format!("camera( PATH, \"{name}\"); [{stamp}]"),
                "Q3_CameraPath: NOT SUPPORTED IN MP\n",
            )
        }
        TYPE_ENABLE => (format!("camera( ENABLE ); [{stamp}]"), refusal),
        TYPE_DISABLE => (format!("camera( DISABLE ); [{stamp}]"), refusal),
        _ => {
            icarus.complete_task(owner, task.id);
            return;
        }
    };
    print::command(host, owner, &line);
    print::debug(host, DebugLevel::Warning, warning);
    icarus.complete_task(owner, task.id);
}
