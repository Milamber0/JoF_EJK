//! The system clipboard, through the desktop's own tools: text out for `viewpos` and `mark`,
//! text in for the console's Ctrl+V. winit has no clipboard and this is a console
//! convenience, not a frame path: a missing tool only means nothing is copied or pasted.
//!
//! On Windows the clipboard is read and set directly (`clipboard-win`): Ctrl+V waited for
//! a PowerShell to start, about a quarter of a second in the middle of play (the
//! `hitch: 248 ms ... between-frames` of 10/10/2026). PowerShell stays the fallback of a
//! copy the direct call could not make.
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How a copy tool receives the text.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Feed {
    /// The text's UTF-8 bytes on standard input.
    Stdin,
    /// The text in the [`COPY_ENV`] environment variable. PowerShell in Constrained
    /// Language Mode cannot read raw standard input, but it can read its environment, and
    /// the environment carries Unicode without a code page.
    Env,
}

/// A command that puts text on the clipboard.
struct CopyTool {
    argv: &'static [&'static str],
    feed: Feed,
    /// Whether the tool exits once the clipboard is set (PowerShell, `clip`, `pbcopy`),
    /// so its exit status says whether the copy worked. `wl-copy` and `xclip` keep serving
    /// the selection and are not waited for.
    exits: bool,
}

/// Environment variable that carries the text to a [`Feed::Env`] tool.
const COPY_ENV: &str = "SJK_CLIPBOARD_TEXT";
/// Longest text sent through [`COPY_ENV`]: a Windows environment variable holds at most
/// 32767 UTF-16 units, and a UTF-8 text of this many bytes never needs more.
const ENV_TEXT_MAX: usize = 30_000;

/// Copy tools, tried in order until one takes the text.
///
/// Windows tools talk in the console's OEM code page unless told otherwise, so `clip`
/// alone turns `€` or `’` into `?`. PowerShell therefore comes first, with UTF-8 stated
/// explicitly: through the environment (works in Constrained Language Mode too), then as
/// raw UTF-8 bytes (any length, Full Language Mode only). `clip` is the last resort, for
/// when PowerShell fails altogether; it may lose characters outside the console's code
/// page, but still copies plain text. `$ErrorActionPreference = 'Stop'` makes a failing
/// script exit non-zero.
const COPY: [CopyTool; 6] = [
    CopyTool {
        argv: &["wl-copy"],
        feed: Feed::Stdin,
        exits: false,
    },
    CopyTool {
        argv: &["xclip", "-selection", "clipboard"],
        feed: Feed::Stdin,
        exits: false,
    },
    CopyTool {
        argv: &["pbcopy"],
        feed: Feed::Stdin,
        exits: true,
    },
    CopyTool {
        argv: &[
            "powershell",
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference = 'Stop'; Set-Clipboard -Value $env:SJK_CLIPBOARD_TEXT",
        ],
        feed: Feed::Env,
        exits: true,
    },
    CopyTool {
        argv: &[
            "powershell",
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference = 'Stop'; $m = New-Object IO.MemoryStream; \
             [Console]::OpenStandardInput().CopyTo($m); \
             Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString($m.ToArray()))",
        ],
        feed: Feed::Stdin,
        exits: true,
    },
    CopyTool {
        argv: &["clip"],
        feed: Feed::Stdin,
        exits: true,
    },
];

/// How a paste tool's standard output is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Decode {
    /// UTF-8 text, possibly with a byte-order mark.
    Utf8,
    /// The text's UTF-16 code units as decimal numbers separated by spaces, which survive
    /// any console code page.
    CodeUnits,
}

/// A command that prints the clipboard's text.
struct PasteTool {
    argv: &'static [&'static str],
    decode: Decode,
}

/// Paste tools, tried in order until one succeeds.
///
/// PowerShell first writes the text's UTF-8 bytes straight to standard output; setting
/// `[Console]::OutputEncoding` would also change the code page of a console SJK was
/// started from. Constrained Language Mode refuses that (no .NET stream or encoding
/// calls), so the next entry prints code units with plain cmdlets and casts, which it
/// allows.
const PASTE: [PasteTool; 5] = [
    PasteTool {
        argv: &["wl-paste", "--no-newline"],
        decode: Decode::Utf8,
    },
    PasteTool {
        argv: &["xclip", "-selection", "clipboard", "-o"],
        decode: Decode::Utf8,
    },
    PasteTool {
        argv: &["pbpaste"],
        decode: Decode::Utf8,
    },
    PasteTool {
        argv: &[
            "powershell",
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference = 'Stop'; $t = Get-Clipboard -Raw; \
             if ($t) { $b = [Text.Encoding]::UTF8.GetBytes($t); \
             $o = [Console]::OpenStandardOutput(); $o.Write($b, 0, $b.Length); $o.Flush() }",
        ],
        decode: Decode::Utf8,
    },
    PasteTool {
        argv: &[
            "powershell",
            "-NoProfile",
            "-Command",
            "$ErrorActionPreference = 'Stop'; $t = Get-Clipboard -Raw; \
             if ($t) { ($t.ToCharArray() | ForEach-Object { [int]$_ }) -join ' ' }",
        ],
        decode: Decode::CodeUnits,
    },
];

/// The most a copy waits, in total, for the tools that exit once the clipboard is set. A
/// PowerShell start takes about 0.25 s, so this only matters on a slow machine; past it
/// the copy is reported as done and the tool is left pending (see [`PENDING_COPY`]). This
/// is a wait on the calling (UI) thread, taken only for an explicit copy.
const COPY_WAIT: Duration = Duration::from_millis(1500);
/// The most a paste waits for a copy that outlived [`COPY_WAIT`].
const PASTE_WAIT: Duration = Duration::from_millis(1000);
/// How often a wait looks at the child.
const POLL: Duration = Duration::from_millis(5);

/// The last copy that was still setting the clipboard when its wait ran out.
static PENDING_COPY: Mutex<Option<Child>> = Mutex::new(None);

impl CopyTool {
    /// Whether this tool can carry `text`: the environment holds neither NUL nor more than
    /// [`ENV_TEXT_MAX`] bytes, and an empty variable is the same as none.
    fn accepts(&self, text: &str) -> bool {
        match self.feed {
            Feed::Stdin => true,
            Feed::Env => !text.is_empty() && text.len() <= ENV_TEXT_MAX && !text.contains('\0'),
        }
    }
}

/// Put `text` on the clipboard; `false` if no tool took it.
///
/// Copies run one at a time: a copy still pending from before is stopped first, so an
/// older, slower tool cannot overwrite this text. A tool that exits once the clipboard is
/// set is waited for, up to [`COPY_WAIT`] in all, and a non-zero exit moves on to the next
/// tool in [`COPY`].
pub(crate) fn copy(text: &str) -> bool {
    stop_pending_copy();
    #[cfg(windows)]
    if clipboard_win::set_clipboard_string(text).is_ok() {
        return true;
    }
    let deadline = Instant::now() + COPY_WAIT;
    COPY.iter()
        .filter(|tool| tool.accepts(text))
        .any(|tool| run_copy(tool, text, deadline))
}

/// Run one copy tool; whether it took the text (or, past `deadline`, may still).
fn run_copy(tool: &CopyTool, text: &str, deadline: Instant) -> bool {
    let mut command = Command::new(tool.argv[0]);
    command
        .args(&tool.argv[1..])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match tool.feed {
        Feed::Stdin => command.stdin(Stdio::piped()),
        Feed::Env => command.stdin(Stdio::null()).env(COPY_ENV, text),
    };
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    if tool.feed == Feed::Stdin {
        let written = child
            .stdin
            .take()
            .is_some_and(|mut input| input.write_all(text.as_bytes()).is_ok());
        if !written {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
    }
    if !tool.exits {
        return true;
    }
    match wait_until(&mut child, deadline) {
        Some(success) => success,
        None => {
            if let Ok(mut pending) = PENDING_COPY.lock() {
                *pending = Some(child);
            }
            true
        }
    }
}

/// Wait for `child` to exit until `deadline`: `Some(success)` once it has, `None` if it is
/// still running.
fn wait_until(child: &mut Child, deadline: Instant) -> Option<bool> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) => {}
            Err(_) => return Some(false),
        }
        if Instant::now() >= deadline {
            return None;
        }
        std::thread::sleep(POLL);
    }
}

/// Stop a copy left pending, since a newer copy replaces its text. Killing and reaping a
/// process does not wait on the tool, so this never delays the caller noticeably.
fn stop_pending_copy() {
    let pending = PENDING_COPY.lock().ok().and_then(|mut slot| slot.take());
    if let Some(mut child) = pending {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Wait up to [`PASTE_WAIT`] for a copy left pending, so a paste right after a slow copy
/// reads the new text. A copy still running after that stays pending.
fn finish_pending_copy() {
    let pending = PENDING_COPY.lock().ok().and_then(|mut slot| slot.take());
    let Some(mut child) = pending else {
        return;
    };
    if wait_until(&mut child, Instant::now() + PASTE_WAIT).is_none()
        && let Ok(mut slot) = PENDING_COPY.lock()
    {
        *slot = Some(child);
    }
}

/// The clipboard's text, if a tool gave any. On Windows, straight from the clipboard:
/// nothing there as text (or the clipboard held by another program) pastes nothing.
pub(crate) fn paste() -> Option<String> {
    finish_pending_copy();
    if cfg!(windows) {
        return direct_paste();
    }
    PASTE.iter().find_map(|tool| {
        let output = Command::new(tool.argv[0])
            .args(&tool.argv[1..])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        match tool.decode {
            Decode::Utf8 => Some(pasted_text(&output.stdout)),
            Decode::CodeUnits => decode_code_units(&output.stdout),
        }
    })
}

#[cfg(windows)]
fn direct_paste() -> Option<String> {
    clipboard_win::get_clipboard_string().ok()
}

#[cfg(not(windows))]
fn direct_paste() -> Option<String> {
    None
}

/// A paste tool's output as text, without a leading byte-order mark.
fn pasted_text(output: &[u8]) -> String {
    let text = String::from_utf8_lossy(output);
    text.strip_prefix('\u{feff}').unwrap_or(&text).to_owned()
}

/// Text from UTF-16 code units printed as decimal numbers separated by white space (the
/// [`Decode::CodeUnits`] format); `None` if anything is not a code unit. A lone surrogate
/// becomes U+FFFD.
fn decode_code_units(output: &[u8]) -> Option<String> {
    let units = std::str::from_utf8(output)
        .ok()?
        .split_whitespace()
        .map(|unit| unit.parse::<u16>().ok())
        .collect::<Option<Vec<u16>>>()?;
    Some(String::from_utf16_lossy(&units))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SYMBOLS: &str = "name a×¥’¡²³‘€½¼©ñæ…";

    #[test]
    fn pasted_utf8_keeps_its_symbols() {
        assert_eq!(pasted_text(SYMBOLS.as_bytes()), SYMBOLS);
        let with_mark = [b"\xef\xbb\xbf".as_slice(), SYMBOLS.as_bytes()].concat();
        assert_eq!(pasted_text(&with_mark), SYMBOLS);
    }

    #[test]
    fn code_units_decode_to_the_text() {
        let units = SYMBOLS
            .encode_utf16()
            .map(|unit| unit.to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            decode_code_units(units.join(" ").as_bytes()).as_deref(),
            Some(SYMBOLS)
        );
        // Characters outside the BMP travel as surrogate pairs; PowerShell ends the line.
        assert_eq!(
            decode_code_units(b"55357 56832 13 10\r\n").as_deref(),
            Some("\u{1f600}\r\n")
        );
        assert_eq!(decode_code_units(b"").as_deref(), Some(""));
    }

    #[test]
    fn code_units_reject_anything_else() {
        assert_eq!(decode_code_units(b"97 x"), None);
        assert_eq!(decode_code_units(b"65536"), None);
        assert_eq!(decode_code_units(b"-1"), None);
        assert_eq!(decode_code_units(b"\xff"), None);
        // A lone surrogate is replaced, not an error.
        assert_eq!(decode_code_units(b"55357").as_deref(), Some("\u{fffd}"));
    }

    #[test]
    fn the_environment_feed_takes_only_what_it_can_carry() {
        let env = COPY
            .iter()
            .find(|tool| tool.feed == Feed::Env)
            .expect("an environment-fed tool");
        assert!(env.accepts(SYMBOLS));
        assert!(env.accepts("two\nlines\r\n"));
        assert!(!env.accepts(""));
        assert!(!env.accepts("nul\0inside"));
        assert!(env.accepts(&"a".repeat(ENV_TEXT_MAX)));
        assert!(!env.accepts(&"a".repeat(ENV_TEXT_MAX + 1)));
        assert!(
            COPY.iter()
                .filter(|tool| tool.feed == Feed::Stdin)
                .all(|tool| tool.accepts("") && tool.accepts("nul\0inside"))
        );
    }

    #[test]
    fn every_windows_fallback_is_tried_after_the_unicode_tools() {
        let names = COPY.iter().map(|tool| tool.argv[0]).collect::<Vec<_>>();
        let last_powershell = names.iter().rposition(|name| *name == "powershell");
        let clip = names.iter().position(|name| *name == "clip");
        assert!(last_powershell < clip);
        // Only the tools that exit are waited for.
        for tool in &COPY {
            assert_eq!(
                tool.exits,
                !matches!(tool.argv[0], "wl-copy" | "xclip"),
                "{}",
                tool.argv[0]
            );
        }
        assert_eq!(
            PASTE.last().map(|tool| tool.decode),
            Some(Decode::CodeUnits)
        );
    }

    /// Round trip through the real clipboard, so it replaces whatever text is there:
    /// `cargo test -p sjk-viewer clipboard -- --ignored` on a desktop session.
    #[test]
    #[ignore = "overwrites the real clipboard"]
    fn real_clipboard_round_trip() {
        let text = "a×¥’€…\r\nsecond line\n\u{1f600}";
        assert!(copy(text));
        assert_eq!(paste().as_deref(), Some(text));
        // A quick second copy replaces the first.
        assert!(copy("one"));
        assert!(copy(SYMBOLS));
        assert_eq!(paste().as_deref(), Some(SYMBOLS));
    }
}
