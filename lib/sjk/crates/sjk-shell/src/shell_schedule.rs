//! TaystJK cmd.cpp:68-124,220-244,290-328. OpenJK has only blocking `wait`.
use super::*;
use std::time::Instant;

pub(super) const COMMANDS: &[(&str, &str)] = &[
    ("delay", "Defer remaining commands by milliseconds"),
    (
        "waitf",
        "Defer remaining commands by frames without blocking other input",
    ),
    ("delaycancel", "Cancel delayed text containing a substring"),
    (
        "waitfcancel",
        "Cancel frame-delayed text containing a substring",
    ),
];

pub(super) struct Pending {
    frames: bool,
    deadline: u64,
    commands: Vec<crate::command_buffer::QueuedCommand>,
}

pub(super) struct Schedule {
    epoch: Instant,
    frame: u64,
    external_frames: bool,
    override_millis: Option<u64>,
    pub(super) pending: Vec<Pending>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            frame: 0,
            external_frames: false,
            override_millis: None,
            pending: Vec::new(),
        }
    }
}

impl Schedule {
    fn millis(&self) -> u64 {
        self.override_millis
            .unwrap_or_else(|| self.epoch.elapsed().as_millis() as u64)
    }
}

impl Shell {
    /// Use application render frames rather than command-dispatch calls for waitf.
    pub fn set_command_frame(&mut self, frame: u64) {
        self.schedule.external_frames = true;
        self.schedule.frame = frame;
    }

    /// Advance one externally driven frame, without advancing it for typed commands.
    pub fn advance_command_frame(&mut self) {
        self.schedule.external_frames = true;
        self.schedule.frame += 1;
    }
    /// Current monotonic console clock in milliseconds.
    pub fn command_clock_millis(&self) -> u64 {
        self.schedule.millis()
    }
    /// Supply a deterministic millisecond clock for replay/tests; frame calls still advance frames.
    pub fn set_command_clock_millis(&mut self, millis: u64) {
        self.schedule.override_millis = Some(millis);
    }

    pub(super) fn resume_scheduled(&mut self) -> Result<(), ShellError> {
        if !self.schedule.external_frames {
            self.schedule.frame += 1;
        }
        let millis = self.schedule.millis();
        let mut index = 0;
        while index < self.schedule.pending.len() {
            let pending = &self.schedule.pending[index];
            let now = if pending.frames {
                self.schedule.frame
            } else {
                millis
            };
            if now < pending.deadline {
                index += 1;
                continue;
            }
            let pending = self.schedule.pending.remove(index);
            for command in pending.commands {
                // Append in scheduling order. Each queued command retains exec recursion depth.
                self.command_buffer.append_queued(command)?;
            }
        }
        Ok(())
    }

    pub(super) fn schedule_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<Vec<String>, ShellError> {
        let frames = name.starts_with("waitf");
        let [arg] = args else {
            return Err(ShellError::Usage("delay/waitf <count>; commands"));
        };
        if name.ends_with("cancel") {
            let query = arg.to_ascii_lowercase();
            self.schedule.pending.retain(|pending| {
                pending.frames != frames
                    || !pending
                        .commands
                        .iter()
                        .any(|cmd| cmd.text.to_ascii_lowercase().contains(&query))
            });
            return Ok(Vec::new());
        }
        let delay = arg
            .parse::<u64>()
            .ok()
            .filter(|v| *v > 0)
            .ok_or(ShellError::Usage("delay/waitf count must be positive"))?;
        let retained: usize = self
            .schedule
            .pending
            .iter()
            .flat_map(|p| &p.commands)
            .map(|cmd| cmd.text.len() + 1)
            .sum();
        if retained >= crate::MAX_COMMAND_BUFFER_BYTES {
            return Err(
                crate::CommandBufferError::Overflow(crate::MAX_COMMAND_BUFFER_BYTES).into(),
            );
        }
        let mut commands = Vec::new();
        while let Some(command) = self.command_buffer.pop_front() {
            commands.push(command);
        }
        let now = if frames {
            self.schedule.frame
        } else {
            self.schedule.millis()
        };
        self.schedule.pending.push(Pending {
            frames,
            deadline: now.saturating_add(delay),
            commands,
        });
        Ok(Vec::new())
    }
}
