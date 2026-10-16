//! config.cfg is written only when something it holds changed, after the save
//! delay and off the main thread, and at once when the console is dropped.

use super::ViewerConsole;
use std::path::Path;
use std::time::{Duration, Instant};

/// A console whose profile is settled on disk, with the file then removed so any
/// later write shows as the file coming back.
fn settled(path: &Path) -> ViewerConsole {
    let mut console = ViewerConsole::new(path.to_owned()).unwrap();
    console.flush_config();
    assert!(!console.shell.config_dirty());
    std::fs::remove_file(path).unwrap();
    console
}

fn frame(console: &mut ViewerConsole) {
    console.execute_buffered_frame(None, |_, _| false);
    console.persist();
}

#[test]
fn bound_key_presses_do_not_write_the_config() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.cfg");
    let mut console = settled(&path);
    console.execute_console_line("bind k \"+forward; +attack\"", None);
    console.flush_config();
    std::fs::remove_file(&path).unwrap();
    for _ in 0..20 {
        console.queue_bound_script("K", true);
        frame(&mut console);
        console.queue_bound_script("K", false);
        frame(&mut console);
    }
    // A `wait` script keeps the buffer busy for frames: still nothing to save.
    console.execute_console_line("wait 5; echo done", None);
    for _ in 0..8 {
        frame(&mut console);
    }
    assert!(!console.shell.config_dirty());
    assert!(!path.exists());
}

#[test]
fn an_unarchived_change_does_not_write_the_config() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.cfg");
    let mut console = settled(&path);
    console.execute_console_line("developer 1", None);
    console.execute_console_line("set sjk_scratch 3", None);
    let start = Instant::now();
    console.shell.autosave(start).unwrap();
    console
        .shell
        .autosave(start + Duration::from_secs(5))
        .unwrap();
    console.flush_config();
    assert!(!path.exists());
}

#[test]
fn an_archived_change_is_written_once_after_the_delay() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.cfg");
    let mut console = settled(&path);
    console.execute_console_line("cg_fov 100", None);
    assert!(console.shell.config_dirty());
    let start = Instant::now();
    console.shell.autosave(start).unwrap();
    console
        .shell
        .autosave(start + sjk_shell::CONFIG_SAVE_DELAY / 2)
        .unwrap();
    assert!(!path.exists());
    console
        .shell
        .autosave(start + sjk_shell::CONFIG_SAVE_DELAY)
        .unwrap();
    assert!(!console.shell.config_dirty());
    // Clean: this only waits for the background writer.
    console.shell.save_if_dirty().unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("seta cg_fov \"100.0\""), "{saved}");
    // Saved once: later frames leave the file alone.
    std::fs::remove_file(&path).unwrap();
    console
        .shell
        .autosave(start + sjk_shell::CONFIG_SAVE_DELAY * 3)
        .unwrap();
    console.shell.save_if_dirty().unwrap();
    assert!(!path.exists());
}

#[test]
fn a_change_made_during_the_delay_restarts_it() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.cfg");
    let mut console = settled(&path);
    let start = Instant::now();
    // Straight to the shell: no save check at the real time in between.
    console.shell.execute_line("cg_fov 100").unwrap();
    console.shell.autosave(start).unwrap();
    console.shell.execute_line("cg_fov 105").unwrap();
    console
        .shell
        .autosave(start + Duration::from_millis(1500))
        .unwrap();
    console
        .shell
        .autosave(start + Duration::from_millis(2500))
        .unwrap();
    assert!(console.shell.config_dirty());
    console
        .shell
        .autosave(start + Duration::from_millis(3600))
        .unwrap();
    console.shell.save_if_dirty().unwrap();
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("seta cg_fov \"105.0\""), "{saved}");
}

#[test]
fn dropping_the_console_writes_pending_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.cfg");
    let mut console = settled(&path);
    console.execute_console_line("cg_fov 110", None);
    console.execute_console_line("bind j \"echo hi\"", None);
    frame(&mut console);
    drop(console);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("seta cg_fov \"110.0\""), "{saved}");
    assert!(saved.contains("bind \"j\" \"echo hi\""), "{saved}");
    let console = ViewerConsole::new(path).unwrap();
    assert_eq!(console.shell.binds.get("j"), Some("echo hi"));
}

#[test]
fn exec_of_the_config_reads_pending_changes() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.cfg");
    let mut console = settled(&path);
    console.execute_console_line("cg_fov 95", None);
    console.execute_console_line("exec config.cfg", None);
    let saved = std::fs::read_to_string(&path).unwrap();
    assert!(saved.contains("seta cg_fov \"95.0\""), "{saved}");
}
