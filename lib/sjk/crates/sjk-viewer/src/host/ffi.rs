//! The C interface `rd-sjk` calls (`codemp/rd-sjk/sjk_host.h` in JoF EJK).
//!
//! Every call is synchronous: its arguments are copied, the work runs on the host
//! thread ([`call`]), and the caller waits for the result. A panic on that thread
//! is reported through the log and returned as a failure, never unwound into the
//! engine.

use super::entities::{Decal, Entity, Light, Poly, PolyVert, Scene};
use super::layer::Picture;
use super::session::{Host, Start};
use super::View;
use glam::Vec3;
use std::ffi::{CStr, c_char, c_int};
use std::path::PathBuf;
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Mutex, PoisonError};

type Job = Box<dyn FnOnce(&mut Option<Host>) + Send>;

/// The host thread's job queue, started by the first call.
static JOBS: Mutex<Option<SyncSender<Job>>> = Mutex::new(None);
/// The host thread, joined at shutdown: the engine unloads this DLL on
/// `vid_restart`, and no thread may be left running its code.
static THREAD: Mutex<Option<std::thread::JoinHandle<()>>> = Mutex::new(None);
/// A whole client's stack (`world_shot::on_big_stack`).
const STACK_BYTES: usize = 256 << 20;

fn jobs() -> Option<SyncSender<Job>> {
    let mut jobs = JOBS.lock().unwrap_or_else(PoisonError::into_inner);
    if jobs.is_none() {
        let (sender, receiver) = sync_channel::<Job>(0);
        let thread = std::thread::Builder::new()
            .name("rd-freedom".to_owned())
            .stack_size(STACK_BYTES)
            .spawn(move || {
                let mut host: Option<Host> = None;
                for job in receiver {
                    job(&mut host);
                }
                drop(host);
            })
            .ok()?;
        *THREAD.lock().unwrap_or_else(PoisonError::into_inner) = Some(thread);
        *jobs = Some(sender);
    }
    jobs.clone()
}

/// Run `work` on the host thread and wait for it; `None` if it panicked.
fn call<R: Send + 'static>(work: impl FnOnce(&mut Option<Host>) -> R + Send + 'static) -> Option<R> {
    let (reply, result) = sync_channel(1);
    let job: Job = Box::new(move |host| {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(host)));
        if let Err(panic) = &outcome {
            let message = panic
                .downcast_ref::<&str>()
                .map(|text| (*text).to_owned())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".to_owned());
            crate::log::progress(format_args!("^1rd-freedom: internal error: {message}"));
        }
        let _ = reply.send(outcome.ok());
    });
    jobs()?.send(job).ok()?;
    result.recv().ok().flatten()
}

/// `text` as an owned string; empty for null.
///
/// # Safety
/// `text` is null or a NUL-terminated string valid for this call.
unsafe fn text(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    // SAFETY: the caller passes a NUL-terminated string valid for the call.
    unsafe { CStr::from_ptr(text) }
        .to_string_lossy()
        .into_owned()
}

/// `sjkHostStart_t`.
#[repr(C)]
pub struct HostStart {
    pub hwnd: isize,
    pub hinstance: isize,
    pub width: c_int,
    pub height: c_int,
    pub base_path: *const c_char,
    pub home_path: *const c_char,
    pub game: *const c_char,
    pub config: *const c_char,
    pub sjk_config: *const c_char,
}

/// `sjkHostView_t`: the parts of `refdef_t` the world needs.
#[repr(C)]
pub struct HostView {
    pub origin: [f32; 3],
    /// Forward, left, up (Quake's `viewaxis`).
    pub axis: [[f32; 3]; 3],
    pub fov_x: f32,
    pub fov_y: f32,
    pub x: c_int,
    pub y: c_int,
    pub width: c_int,
    pub height: c_int,
    pub time: c_int,
    pub rdflags: c_int,
}

/// Start the renderer on the engine's window. Returns 1 on success.
///
/// # Safety
/// `start` points to a valid `sjkHostStart_t` whose strings are NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostInit(start: *const HostStart) -> c_int {
    if start.is_null() {
        return 0;
    }
    // SAFETY: the caller passes a valid start block for this call.
    let start = unsafe { &*start };
    super::FORWARDING.store(true, std::sync::atomic::Ordering::Relaxed);
    // SAFETY: the strings are valid for this call (see above).
    let (base, home, game, config, sjk_config) = unsafe {
        (
            text(start.base_path),
            text(start.home_path),
            text(start.game),
            text(start.config),
            text(start.sjk_config),
        )
    };
    if let Some(folder) = std::path::Path::new(&config).parent() {
        super::open_log_file(&folder.join("rd-freedom.log"));
    }
    let start = Start {
        hwnd: start.hwnd,
        hinstance: start.hinstance,
        size: [start.width.max(1) as u32, start.height.max(1) as u32],
        home_path: if home.is_empty() {
            PathBuf::from(&base)
        } else {
            PathBuf::from(home)
        },
        base_path: PathBuf::from(base),
        game,
        config: PathBuf::from(config),
        sjk_config: (!sjk_config.is_empty()).then(|| PathBuf::from(sjk_config)),
    };
    call(move |host| {
        *host = None;
        match Host::start(start) {
            Ok(started) => {
                *host = Some(started);
                1
            }
            Err(error) => {
                crate::log::progress(format_args!("^1rd-freedom: could not start: {error}"));
                0
            }
        }
    })
    .unwrap_or(0)
}

/// Stop the renderer and release the window's surface.
#[unsafe(no_mangle)]
pub extern "C" fn SJK_HostShutdown() {
    crate::log::progress(format_args!("rd-freedom: shutting down"));
    let _ = call(|host| *host = None);
    crate::log::progress(format_args!("rd-freedom: renderer released"));
    // Close the queue and wait for the thread to finish.
    drop(JOBS.lock().unwrap_or_else(PoisonError::into_inner).take());
    if let Some(thread) = THREAD.lock().unwrap_or_else(PoisonError::into_inner).take() {
        let _ = thread.join();
    }
    crate::log::progress(format_args!("rd-freedom: stopped"));
}

/// Load the world of `name` (`maps/mp/ffa3.bsp`). Returns 1 on success.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostLoadWorld(name: *const c_char) -> c_int {
    // SAFETY: as documented.
    let name = unsafe { text(name) };
    call(move |host| {
        let Some(host) = host else { return 0 };
        match host.load_world(&name) {
            Ok(()) => 1,
            Err(error) => {
                crate::log::progress(format_args!("^1rd-freedom: could not load {name}: {error}"));
                0
            }
        }
    })
    .unwrap_or(0)
}

/// Drop the loaded world.
#[unsafe(no_mangle)]
pub extern "C" fn SJK_HostUnloadWorld() {
    let _ = call(|host| {
        if let Some(host) = host {
            host.unload_world();
        }
    });
}

/// The engine registered shader `name` as `handle`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostShaderName(handle: c_int, name: *const c_char) {
    // SAFETY: as documented.
    let name = unsafe { text(name) };
    let _ = call(move |_| {
        super::assets::set_shader(handle, name.clone());
        super::with_layer(|layer| layer.name(handle, name));
    });
}

/// The window is now `width` by `height` pixels.
#[unsafe(no_mangle)]
pub extern "C" fn SJK_HostResize(width: c_int, height: c_int) {
    let size = [width.max(1) as u32, height.max(1) as u32];
    let _ = call(move |host| {
        if let Some(host) = host {
            host.resize(size);
        }
    });
}

/// The engine registered model `name` (an MD3, a GLM, an inline `*N`) as `handle`.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostModelName(handle: c_int, name: *const c_char) {
    // SAFETY: as documented.
    let name = unsafe { text(name) };
    let _ = call(move |_| super::assets::set_model(handle, name));
}

/// The engine registered skin `handle`: its `surface,shader` lines.
///
/// # Safety
/// `surfaces` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostSkin(handle: c_int, surfaces: *const c_char) {
    // SAFETY: as documented.
    let lines = unsafe { text(surfaces) };
    let _ = call(move |_| super::assets::set_skin(handle, lines));
}

/// `sjkHostScene_t`.
#[repr(C)]
pub struct HostScene {
    pub entities: *const Entity,
    pub num_entities: c_int,
    pub bones: *const [[f32; 4]; 3],
    pub num_bones: c_int,
    pub ints: *const c_int,
    pub num_ints: c_int,
    pub lights: *const Light,
    pub num_lights: c_int,
    pub polys: *const Poly,
    pub num_polys: c_int,
    pub poly_verts: *const PolyVert,
    pub num_poly_verts: c_int,
    pub decals: *const Decal,
    pub num_decals: c_int,
    pub first_main_entity: c_int,
    pub num_main_entities: c_int,
    pub previews: *const super::preview::PreviewView,
    pub num_previews: c_int,
}

/// `pointer[..count]` copied, empty for null or a non-positive count.
///
/// # Safety
/// `pointer` holds `count` valid values when non-null.
unsafe fn copied<T: Copy>(pointer: *const T, count: c_int) -> Vec<T> {
    if pointer.is_null() || count <= 0 {
        return Vec::new();
    }
    // SAFETY: as documented.
    unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec()
}

/// Present one frame: the world from `view` with `scene`'s entities (null for none),
/// then `count` pictures.
///
/// # Safety
/// `view` and `scene` are null or valid, with every array they point to holding the
/// counts they give; `pictures` holds `count` valid entries (or `count` is 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostFrame(
    view: *const HostView,
    scene: *const HostScene,
    pictures: *const Picture,
    count: c_int,
) {
    // SAFETY: as documented.
    let view = unsafe { view.as_ref() }.map(|view| {
        let [forward, _left, up] = view.axis;
        View {
            origin: Vec3::from_array(view.origin),
            forward: Vec3::from_array(forward),
            up: Vec3::from_array(up),
            fov_y: view.fov_y,
            time_ms: view.time,
        }
    });
    // SAFETY: as documented.
    let scene = unsafe { scene.as_ref() }.map_or_else(Scene::default, |scene| unsafe {
        Scene {
            entities: copied(scene.entities, scene.num_entities),
            bones: copied(scene.bones, scene.num_bones),
            ints: copied(scene.ints, scene.num_ints),
            lights: copied(scene.lights, scene.num_lights),
            polys: copied(scene.polys, scene.num_polys),
            poly_verts: copied(scene.poly_verts, scene.num_poly_verts),
            decals: copied(scene.decals, scene.num_decals),
            main_entities: {
                let first = usize::try_from(scene.first_main_entity).unwrap_or(0);
                first..first + usize::try_from(scene.num_main_entities).unwrap_or(0)
            },
            previews: copied(scene.previews, scene.num_previews),
        }
    });
    // SAFETY: as documented.
    let pictures = unsafe { copied(pictures, count) };
    let _ = call(move |host| {
        if let Some(host) = host {
            host.frame(view, scene, &pictures);
        }
    });
}

/// Copy the next log line into `buffer` (NUL-terminated, truncated to `size`).
/// Returns 1 when a line was copied, 0 when none waits.
///
/// # Safety
/// `buffer` holds `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostPollLog(buffer: *mut c_char, size: c_int) -> c_int {
    if buffer.is_null() || size <= 0 {
        return 0;
    }
    let Some(line) = super::next_log_line() else {
        return 0;
    };
    // SAFETY: as documented.
    unsafe { copy_out(&line, buffer, size) };
    1
}

/// Copy the adapter's description into `buffer`.
///
/// # Safety
/// `buffer` holds `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostAdapter(buffer: *mut c_char, size: c_int) {
    if buffer.is_null() || size <= 0 {
        return;
    }
    let text = call(|host| host.as_ref().map(Host::adapter).unwrap_or_default()).unwrap_or_default();
    // SAFETY: as documented.
    unsafe { copy_out(&text, buffer, size) };
}

/// Set renderer setting `name` to `value`. Returns 1 when SJK took it.
///
/// # Safety
/// Both are NUL-terminated strings.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostSetCvar(name: *const c_char, value: *const c_char) -> c_int {
    // SAFETY: as documented.
    let (name, value) = unsafe { (text(name), text(value)) };
    call(move |host| host.as_mut().is_some_and(|host| host.set_cvar(&name, &value)))
        .map_or(0, c_int::from)
}

/// Whether `name` is a shader script or an image SJK finds. Returns 1 or 0.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostShaderExists(name: *const c_char) -> c_int {
    // SAFETY: as documented.
    let name = unsafe { text(name) };
    call(move |host| host.as_ref().is_some_and(|host| host.shader_exists(&name)))
        .map_or(0, c_int::from)
}

/// `R_MarkFragments`: clip `num_points` points (`[f32; 3]` each) swept along
/// `projection` against the world; fills at most `max_points` points and
/// `max_fragments` `(firstPoint, numPoints)` pairs. Returns the fragment count.
///
/// # Safety
/// `points` holds `num_points` vectors, `projection` one; `point_buffer` has room
/// for `max_points` vectors and `fragment_buffer` for `max_fragments` int pairs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostMarkFragments(
    num_points: c_int,
    points: *const [f32; 3],
    projection: *const [f32; 3],
    max_points: c_int,
    point_buffer: *mut [f32; 3],
    max_fragments: c_int,
    fragment_buffer: *mut [c_int; 2],
) -> c_int {
    if num_points < 3 || points.is_null() || projection.is_null() || max_points <= 0 || max_fragments <= 0 {
        return 0;
    }
    // SAFETY: as documented.
    let (corners, sweep) = unsafe {
        (
            std::slice::from_raw_parts(points, num_points as usize).to_vec(),
            *projection,
        )
    };
    let (max_points, max_fragments) = (max_points as usize, max_fragments as usize);
    let Some(Some(fragments)) = call(move |host| {
        let gpu = host.as_ref()?.gpu()?;
        Some(super::queries::mark_fragments(gpu, &corners, sweep, max_points, max_fragments))
    }) else {
        return 0;
    };
    let mut written = 0usize;
    for fragment in fragments.fragments().iter().take(max_fragments) {
        let end = fragment.first_point + fragment.point_count;
        if end > max_points {
            break;
        }
        let points = fragments.fragment_points(*fragment);
        // SAFETY: `end <= max_points` and `written < max_fragments`, the buffers' sizes.
        unsafe {
            std::ptr::copy_nonoverlapping(
                points.as_ptr(),
                point_buffer.add(fragment.first_point),
                points.len(),
            );
            *fragment_buffer.add(written) = [fragment.first_point as c_int, fragment.point_count as c_int];
        }
        written += 1;
    }
    written as c_int
}

/// `R_LightForPoint`: ambient, directed (0..1) and the direction toward the light.
/// Returns 0 without a world or light grid.
///
/// # Safety
/// `point` is one vector; `out` has room for three.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostLightForPoint(point: *const [f32; 3], out: *mut [[f32; 3]; 3]) -> c_int {
    if point.is_null() || out.is_null() {
        return 0;
    }
    // SAFETY: as documented.
    let point = unsafe { *point };
    match call(move |host| super::queries::light_for_point(host.as_ref()?.gpu()?, point)).flatten() {
        Some(light) => {
            // SAFETY: as documented.
            unsafe { *out = light };
            1
        }
        None => 0,
    }
}

/// `R_inPVS`. Returns 1 when the points' clusters see each other (or no world is
/// loaded, so nothing is culled).
///
/// # Safety
/// `a` and `b` are one vector each.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostInPVS(a: *const [f32; 3], b: *const [f32; 3]) -> c_int {
    if a.is_null() || b.is_null() {
        return 1;
    }
    // SAFETY: as documented.
    let (a, b) = unsafe { (*a, *b) };
    call(move |host| {
        host.as_ref()
            .and_then(|host| host.gpu())
            .is_none_or(|gpu| super::queries::in_pvs(gpu, a, b))
    })
    .map_or(1, c_int::from)
}

/// The renderer settings ([`Host::settings`]) into `buffer`, NUL-terminated and
/// truncated to `size`. Returns the length the whole text needs.
///
/// # Safety
/// `buffer` holds `size` (> 0) writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostSettings(buffer: *mut c_char, size: c_int) -> c_int {
    let text = call(|host| host.as_ref().map(Host::settings).unwrap_or_default()).unwrap_or_default();
    if !buffer.is_null() && size > 0 {
        // SAFETY: as documented.
        unsafe { copy_out(&text, buffer, size) };
    }
    c_int::try_from(text.len() + 1).unwrap_or(c_int::MAX)
}

/// `graphicsquality [level]`: show or set the graphics quality level. Its reply goes
/// into `buffer` (NUL-terminated, truncated to `size`).
///
/// # Safety
/// `args` is a NUL-terminated string; `buffer` holds `size` (> 0) writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn SJK_HostQuality(args: *const c_char, buffer: *mut c_char, size: c_int) {
    // SAFETY: as documented.
    let args: Vec<String> = unsafe { text(args) }
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let reply = call(move |host| host.as_mut().map(|host| host.quality_command(&args)).unwrap_or_default())
        .unwrap_or_default();
    if !buffer.is_null() && size > 0 {
        // SAFETY: as documented.
        unsafe { copy_out(&reply, buffer, size) };
    }
}

/// # Safety
/// `buffer` holds `size` (> 0) writable bytes.
unsafe fn copy_out(text: &str, buffer: *mut c_char, size: c_int) {
    let bytes = text.as_bytes();
    let length = bytes.len().min(size as usize - 1);
    // SAFETY: `length + 1 <= size` bytes are written.
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), buffer, length);
        *buffer.add(length) = 0;
    }
}
