//! Small shared-client diagnostics without server, renderer or wire-policy changes.
use super::*;
use console_cvars::IntegerSetting;

/// Callback caches used where allocating cvar queries would be inappropriate.
pub(super) struct Settings {
    /// Live alias gate read by the ordinary command dispatcher.
    pub(super) exit_command: IntegerSetting,
    busy_wait: IntegerSetting,
}

impl Settings {
    /// Register only connected settings, before archive restoration.
    pub(super) fn register(cvars: &mut CvarRegistry) -> Result<Self, sjk_shell::CvarError> {
        for (name, default, help) in [
            (
                "com_busyWait",
                0_i64,
                "Poll rather than sleep while waiting for the frame deadline",
            ),
            ("cl_exitCommand", 0, "Enable exit as a quit alias"),
            (
                "com_timestamps",
                1,
                "Timestamps in diagnostic stderr and the console file log",
            ),
            (
                "logfile",
                0,
                "Console file log: 0 off, 1 write, 2 synchronize each line",
            ),
        ] {
            cvars.register(CvarDefinition::new(name, default, CvarFlags::ARCHIVE, help))?;
        }
        cvars.on_change("com_timestamps", |change| {
            if let CvarValue::Integer(value) = change.current {
                crate::log::set_timestamps(value != 0);
            }
        })?;
        Ok(Self {
            exit_command: IntegerSetting::bind(cvars, "cl_exitCommand", 0)?,
            busy_wait: IntegerSetting::bind(cvars, "com_busyWait", 0)?,
        })
    }
}

/// Preserve the same deadline and command cadence; only the OS waiting strategy changes.
pub(crate) fn wait_control(
    console: Option<&ViewerConsole>,
    deadline: Instant,
) -> winit::event_loop::ControlFlow {
    if console.is_some_and(|console| console.qcommon.busy_wait.enabled()) {
        winit::event_loop::ControlFlow::Poll
    } else {
        winit::event_loop::ControlFlow::WaitUntil(deadline)
    }
}
