//! SJK's renderer inside another engine.
//!
//! JoF EJK loads `rd-sjk`, a renderer DLL. Its C++ half keeps EJK's model, skin,
//! shader-name and Ghoul2 code (the game and the server need them); its drawing is
//! this module. A [`GpuState`](crate::GpuState) is built without a winit window
//! and presents into the engine's window through a raw-handle surface
//! ([`session`]). Each frame the engine's view (`refdef_t`) replaces SJK's camera
//! and the engine's 2D commands ([`layer`]) replace SJK's own HUD and menus; the
//! world, its materials, lighting, shadows, weather and post-processing are SJK's,
//! unchanged.
//!
//! Every call runs on one thread with the stack a whole client needs ([`ffi`]
//! marshals each engine call onto it), so the hooks the frame reads ([`view`],
//! [`draw_layer`]) are that thread's.

pub(crate) mod entities;
mod ffi;
pub(crate) mod layer;
mod queries;
mod session;

use glam::Vec3;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The folder EternalJK mounts between `base` and `fs_game`.
pub(crate) const ETERNALJK: &str = "EternalJK";

/// The engine's view of this frame.
#[derive(Clone, Copy, Debug)]
pub(crate) struct View {
    pub(crate) origin: Vec3,
    pub(crate) forward: Vec3,
    pub(crate) up: Vec3,
    /// Vertical field of view, degrees (`refdef_t::fov_y`).
    pub(crate) fov_y: f32,
    /// `refdef_t::time`, milliseconds.
    pub(crate) time_ms: i32,
}

thread_local! {
    static VIEW: Cell<Option<View>> = const { Cell::new(None) };
    static LAYER: RefCell<Option<layer::Layer>> = const { RefCell::new(None) };
}

/// Lines SJK logged since the engine last asked, for its console.
static LOG: Mutex<VecDeque<String>> = Mutex::new(VecDeque::new());
/// Logging goes to [`LOG`] rather than stderr.
static FORWARDING: AtomicBool = AtomicBool::new(false);
/// At most this many lines wait for the engine.
const LOG_LINES: usize = 4096;

/// Whether an engine hosts this renderer (set before anything else starts).
pub(crate) fn hosted() -> bool {
    FORWARDING.load(Ordering::Relaxed)
}

/// Whether this thread renders for a hosting engine.
pub(crate) fn active() -> bool {
    LAYER.with(|layer| layer.borrow().is_some())
}

/// The engine's view while this thread renders it.
pub(crate) fn view() -> Option<View> {
    VIEW.with(Cell::get)
}

fn set_view(view: Option<View>) {
    VIEW.with(|cell| cell.set(view));
}

fn install_layer(layer: layer::Layer) {
    LAYER.with(|cell| *cell.borrow_mut() = Some(layer));
}

fn remove_layer() {
    LAYER.with(|cell| *cell.borrow_mut() = None);
}

fn with_layer(work: impl FnOnce(&mut layer::Layer)) {
    LAYER.with(|cell| {
        if let Some(layer) = cell.borrow_mut().as_mut() {
            work(layer);
        }
    });
}

/// Draw the engine's 2D layer over `target`, the frame's 2D view.
pub(crate) fn draw_layer(
    encoder: &mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    _size: [u32; 2],
) {
    LAYER.with(|cell| {
        if let Some(layer) = cell.borrow().as_ref() {
            layer.draw(encoder, target);
        }
    });
}

/// Every log line, also written straight to a file: the engine's console only sees
/// lines after a frame returns, which a crash inside one never does.
static LOG_FILE: Mutex<Option<std::fs::File>> = Mutex::new(None);

/// Start `path` afresh as the log file, and log panics (with where they happened)
/// into it before anything else runs.
fn open_log_file(path: &std::path::Path) {
    let Ok(file) = std::fs::File::create(path) else {
        return;
    };
    *LOG_FILE.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(file);
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write_log_file(&format!(
            "panic on {:?}: {info}\n{}",
            std::thread::current().name(),
            std::backtrace::Backtrace::force_capture()
        ));
        previous(info);
    }));
}

fn write_log_file(line: &str) {
    use std::io::Write;
    if let Some(file) = LOG_FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_mut()
    {
        let _ = writeln!(file, "{line}");
        let _ = file.flush();
    }
}

/// Queue a log line for the engine; false when not hosted (log to stderr instead).
pub(crate) fn forward_log(arguments: &std::fmt::Arguments<'_>) -> bool {
    if !FORWARDING.load(Ordering::Relaxed) {
        return false;
    }
    write_log_file(&arguments.to_string());
    let mut lines = LOG.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if lines.len() >= LOG_LINES {
        lines.pop_front();
    }
    lines.push_back(arguments.to_string());
    true
}

fn next_log_line() -> Option<String> {
    LOG.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .pop_front()
}

/// The names of the engine's shader, model and skin handles.
pub(crate) mod assets {
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Names {
        shaders: HashMap<i32, String>,
        models: HashMap<i32, String>,
        skins: HashMap<i32, String>,
    }

    thread_local! {
        static NAMES: RefCell<Names> = RefCell::new(Names::default());
    }

    pub(crate) fn set_shader(handle: i32, name: String) {
        NAMES.with(|names| names.borrow_mut().shaders.insert(handle, name));
    }

    pub(crate) fn set_model(handle: i32, name: String) {
        NAMES.with(|names| names.borrow_mut().models.insert(handle, name));
    }

    pub(crate) fn set_skin(handle: i32, lines: String) {
        NAMES.with(|names| names.borrow_mut().skins.insert(handle, lines));
    }

    pub(crate) fn shader_name(handle: i32) -> Option<String> {
        NAMES.with(|names| names.borrow().shaders.get(&handle).cloned())
            .filter(|name| !name.is_empty())
    }

    pub(crate) fn model_name(handle: i32) -> Option<String> {
        NAMES.with(|names| names.borrow().models.get(&handle).cloned())
            .filter(|name| !name.is_empty())
    }

    /// The `surface,shader` lines of skin `handle`.
    pub(crate) fn skin_lines(handle: i32) -> Option<String> {
        NAMES.with(|names| names.borrow().skins.get(&handle).cloned())
    }
}
