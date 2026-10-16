//! Client preferences composed with the existing console, chat and connection paths.
use super::*;

/// Register only preferences or status values with an actual consumer.
pub(super) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    for (name, value, help) in [
        (
            "cl_allowEnterCompletion",
            1_i64,
            "Complete a unique command before Enter submits",
        ),
        (
            // EternalJK's default (`cl_main.cpp:4144`): the key under Escape opens
            // the console on every layout, including those where it types `0`.
            "cl_consoleUseScanCode",
            1,
            "Use the physical key under Escape instead of cl_consoleKeys",
        ),
        (
            "cl_consoleShiftRequirement",
            0,
            "Native console key: 0 always, 1 opening, 2 always Shift",
        ),
        (
            "cl_downloadOverlay",
            1,
            "Show download filename and byte progress while connecting",
        ),
        (
            "cl_enableGuid",
            1,
            "Send a persistent GUID on the next connection",
        ),
        (
            "cl_guidServerUniq",
            1,
            "Use a different GUID for each server",
        ),
    ] {
        cvars.register(CvarDefinition::new(name, value, CvarFlags::ARCHIVE, help))?;
    }
    cvars.register(CvarDefinition::new(
        "cl_conXOffset",
        0.0,
        CvarFlags::NONE,
        "Horizontal offset of console notify text",
    ))?;
    cvars.register(CvarDefinition::new(
        "cl_freezeDemo",
        0_i64,
        CvarFlags::NONE,
        "Hold demo presentation time for frame inspection",
    ))?;
    for definition in [
        CvarDefinition::new(
            "cl_chatStylePrefix",
            "",
            CvarFlags::ARCHIVE,
            "Prefix for global messages sent from the chat composer",
        ),
        CvarDefinition::new(
            "cl_chatStyleSuffix",
            "",
            CvarFlags::ARCHIVE,
            "Suffix for global messages sent from the chat composer",
        ),
        CvarDefinition::new(
            "cl_reconnectArgs",
            "",
            CvarFlags::ARCHIVE,
            "Last server address used by reconnect",
        ),
        CvarDefinition::new(
            "cl_downloadName",
            "",
            CvarFlags::READ_ONLY,
            "Current server download filename",
        ),
        CvarDefinition::new(
            "cl_downloadProtocol",
            "",
            CvarFlags::READ_ONLY,
            "Current content transfer protocol",
        ),
    ] {
        cvars.register(definition)?;
    }
    Ok(())
}

/// TaystJK sdl_input.cpp:269-278; only native scan-code detection is Shift-gated.
/// A layout that types `^` on the key needs Shift whatever the requirement, so `^`
/// stays free for colour codes (`IN_TranslateSDLToJKKey`, `sdl_input.cpp:392`).
pub(super) fn native_console(requirement: i64, caret: bool, shift: bool, open: bool) -> bool {
    if caret {
        return shift;
    }
    match requirement {
        0 => true,
        2 => shift,
        _ => shift || open,
    }
}

/// Apply affixes only to global composer messages, then reuse the safe quoting/byte budget.
pub(super) fn style_chat(command: &str, prefix: &str, suffix: &str) -> String {
    if prefix.is_empty() && suffix.is_empty() {
        return command.to_owned();
    }
    // chat_command has already sanitized and quoted the composer payload. Keep its UTF-8
    // intact rather than round-tripping it through the legacy byte-oriented tokenizer.
    let Some(body) = command
        .strip_prefix("say \"")
        .and_then(|s| s.strip_suffix('"'))
    else {
        return command.to_owned();
    };
    if body.contains('"') {
        return command.to_owned();
    }
    sjk_client::chat_command(
        sjk_client::ChatDestination::Global,
        &format!("{prefix}{body}{suffix}"),
    )
    .unwrap_or_else(|| command.to_owned())
}

impl ViewerConsole {
    /// OpenJK cl_cgame.cpp:755-758 holds demo serverTime, not the wall clock.
    pub(crate) fn demo_time(&self, requested: u64, current: u64) -> u64 {
        if self.integer_cvar("cl_freezedemo").unwrap_or(0) != 0 {
            current
        } else {
            requested
        }
    }
    /// Snapshot connection-time identity preferences for the worker; no key I/O on frames.
    pub(crate) fn guid_policy(&self) -> crate::client_guid::Policy {
        crate::client_guid::Policy {
            config: self.config_directory.join("config.cfg"),
            enabled: self.integer_cvar("cl_enableguid").unwrap_or(1) != 0,
            server_unique: self.integer_cvar("cl_guidserveruniq").unwrap_or(1) != 0,
        }
    }
    /// Mirror the worker's validated progress into read-only cvars and select its presentation.
    pub(crate) fn download_status(&mut self, progress: Option<&str>) -> String {
        let name = progress
            .and_then(|s| s.strip_prefix("Downloading "))
            .and_then(|s| s.split_once(':'))
            .map_or("", |(name, _)| name);
        let _ = self.shell.cvars.restore_text("cl_downloadName", name);
        let _ = self.shell.cvars.restore_text(
            "cl_downloadProtocol",
            if name.is_empty() { "" } else { "UDP" },
        );
        if self.integer_cvar("cl_downloadoverlay").unwrap_or(1) != 0 {
            progress.unwrap_or("").to_owned()
        } else {
            "Downloading server content".to_owned()
        }
    }
}
