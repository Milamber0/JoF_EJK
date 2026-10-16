//! The interpreter's console output (`Q3_DebugPrint`, `Q3_CenterPrint`).

use crate::host::{DebugLevel, IcarusHost, Owner};

/// `Q_vsnprintf` into the reference's 1024-byte buffers: at most 1023 bytes.
fn truncate(text: &str) -> &str {
    truncate_to(text, 1023)
}

fn truncate_to(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `Q3_DebugPrint` for every level but `WL_DEBUG`: nothing unless the developer
/// switch is on, then the text in the level's colour.
pub(crate) fn debug<O: Owner, H: IcarusHost<O> + ?Sized>(
    host: &mut H,
    level: DebugLevel,
    text: &str,
) {
    if !host.developer() {
        return;
    }
    let text = truncate(text);
    let line = match level {
        DebugLevel::Error => format!("^1ERROR: {text}"),
        DebugLevel::Warning => format!("^3WARNING: {text}"),
        DebugLevel::Verbose | DebugLevel::Debug => format!("^2INFO: {text}"),
    };
    host.print(&line);
}

/// `Q3_DebugPrint(WL_DEBUG, "%4d ...", owner, ...)`: a command as it runs, printed with
/// the entity's `script_targetname` (`(null)` without one, as glibc prints a null
/// string) and number. `text` is what follows the reference's `"%4d "` prefix, which
/// shared the 1023 bytes of the reference's buffer.
pub(crate) fn command<O: Owner, H: IcarusHost<O> + ?Sized>(host: &mut H, owner: O, text: &str) {
    if !host.developer() {
        return;
    }
    let name = host.entity_names(owner).script_targetname;
    let line = format!(
        "^4DEBUG: {}({}): {}\n",
        name.as_deref().unwrap_or("(null)"),
        owner,
        truncate_to(text, 1018)
    );
    host.print(&line);
}

/// `Q3_CenterPrint`: the text is the reference's format string. A leading `!` sends the
/// rest as a centre print and nothing else; a leading `@` sends it whole and prints it
/// too; anything else is only printed. `%%` is read as `%`; any other conversion is left
/// as written (the reference would read an argument that is not there).
pub(crate) fn center<O: Owner, H: IcarusHost<O> + ?Sized>(host: &mut H, format: &str) {
    let text = format.replace("%%", "%");
    let text = truncate(&text);
    if let Some(rest) = text.strip_prefix('!') {
        host.broadcast_command(&format!("cp \"{rest}\""));
        return;
    }
    if text.starts_with('@') {
        host.broadcast_command(&format!("cp \"{text}\""));
    }
    debug(host, DebugLevel::Verbose, &format!("{text}\n"));
}
