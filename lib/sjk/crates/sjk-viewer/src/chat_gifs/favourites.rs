//! The player's favourite GIFs (`docs/hub-chat.md`, "The GIF picker"): GIPHY ids kept
//! on this PC in `gif-favourites.txt` beside `config.cfg`, one a line, newest first,
//! read at start and written back at every change. Nothing is sent anywhere: the
//! picker's Favourites tab shows them, and a star on a tile or the chosen message's
//! button adds or removes one.

use super::GifId;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

/// The list's file, beside `config.cfg`.
pub(crate) const FILE: &str = "gif-favourites.txt";
/// The most favourites kept; adding one past it lets the oldest go.
pub(crate) const MAX: usize = 500;
/// The file's largest size read: every line a full id.
const MAX_FILE_BYTES: u64 = (MAX * 48) as u64;

/// Favourite GIFs, newest first, each once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Favourites {
    ids: Vec<GifId>,
}

impl Favourites {
    pub(crate) const fn new() -> Self {
        Self { ids: Vec::new() }
    }

    /// Newest first.
    pub(crate) fn ids(&self) -> &[GifId] {
        &self.ids
    }

    pub(crate) fn contains(&self, id: &GifId) -> bool {
        self.ids.contains(id)
    }

    /// Make `id` a favourite (the newest) or not; returns whether the list changed.
    pub(crate) fn set(&mut self, id: &GifId, favourite: bool) -> bool {
        let at = self.ids.iter().position(|known| known == id);
        match (at, favourite) {
            (Some(_), true) | (None, false) => false,
            (Some(at), false) => {
                self.ids.remove(at);
                true
            }
            (None, true) => {
                self.ids.insert(0, id.clone());
                self.ids.truncate(MAX);
                true
            }
        }
    }

    /// The list a file holds: one id a line, newest first; blank lines, lines that are
    /// not an id and repeats are left out.
    pub(crate) fn parse(text: &str) -> Self {
        let mut list = Self::new();
        for line in text.lines() {
            if list.ids.len() == MAX {
                break;
            }
            if let Some(id) = GifId::new(line.trim())
                && !list.ids.contains(&id)
            {
                list.ids.push(id);
            }
        }
        list
    }

    /// The file's text.
    pub(crate) fn to_text(&self) -> String {
        let mut text = String::with_capacity(self.ids.len() * 24);
        for id in &self.ids {
            text.push_str(id.as_str());
            text.push('\n');
        }
        text
    }
}

/// Read the list from `path`; a missing file is an empty list.
pub(crate) fn load_file(path: &Path) -> std::io::Result<Favourites> {
    use std::io::Read;
    match std::fs::File::open(path) {
        Ok(file) => {
            let mut bytes = Vec::new();
            file.take(MAX_FILE_BYTES).read_to_end(&mut bytes)?;
            Ok(Favourites::parse(&String::from_utf8_lossy(&bytes)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Favourites::new()),
        Err(error) => Err(error),
    }
}

/// Write the list to `path` through a temporary file, so a failed write leaves the
/// last list whole.
pub(crate) fn save_file(path: &Path, list: &Favourites) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pending = path.with_extension("txt.pending");
    std::fs::write(&pending, list.to_text())?;
    std::fs::rename(&pending, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&pending);
    })
}

struct State {
    list: Favourites,
    /// Where the list is kept, once read.
    path: Option<PathBuf>,
    /// Counts changes, so the picker knows when to read the list again.
    revision: u64,
}

static STATE: Mutex<State> = Mutex::new(State {
    list: Favourites::new(),
    path: None,
    revision: 0,
});

fn lock() -> MutexGuard<'static, State> {
    STATE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read the list from `directory` (the settings folder), once.
pub(crate) fn load(directory: &Path) {
    let mut state = lock();
    if state.path.is_some() {
        return;
    }
    let path = directory.join(FILE);
    match load_file(&path) {
        Ok(list) => {
            state.list = list;
            state.revision += 1;
        }
        Err(error) => crate::log::progress(format_args!("GIF favourites: {error}")),
    }
    state.path = Some(path);
}

/// The favourites, newest first.
pub(crate) fn list() -> Vec<GifId> {
    lock().list.ids().to_vec()
}

/// Whether `id` is a favourite.
pub(crate) fn contains(id: &GifId) -> bool {
    lock().list.contains(id)
}

/// Make `id` a favourite or not, and keep the list.
pub(crate) fn set(id: &GifId, favourite: bool) {
    let mut state = lock();
    if !state.list.set(id, favourite) {
        return;
    }
    state.revision += 1;
    if let Some(path) = &state.path
        && let Err(error) = save_file(path, &state.list)
    {
        crate::log::progress(format_args!("GIF favourites: could not save: {error}"));
    }
}

/// Counts changes to the list.
pub(crate) fn revision() -> u64 {
    lock().revision
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: usize) -> GifId {
        GifId::new(&format!("Fav{n:04}xyz")).unwrap()
    }

    #[test]
    fn favourites_are_newest_first_and_kept_once() {
        let mut list = Favourites::new();
        assert!(list.set(&id(1), true));
        assert!(list.set(&id(2), true));
        assert!(!list.set(&id(1), true), "already one");
        assert_eq!(list.ids(), [id(2), id(1)]);
        assert!(list.set(&id(2), false));
        assert!(!list.set(&id(2), false));
        assert_eq!(list.ids(), [id(1)]);
        // Past the most kept, the oldest goes.
        let mut list = Favourites::new();
        for n in 0..MAX + 3 {
            list.set(&id(n), true);
        }
        assert_eq!(list.ids().len(), MAX);
        assert_eq!(list.ids()[0], id(MAX + 2));
        assert!(!list.contains(&id(2)) && list.contains(&id(3)));
    }

    #[test]
    fn the_file_round_trips_and_bad_lines_are_left_out() {
        let mut list = Favourites::new();
        list.set(&id(1), true);
        list.set(&id(2), true);
        assert_eq!(Favourites::parse(&list.to_text()), list);
        let read =
            Favourites::parse("Fav0001xyz\r\n\nnot an id\nbirthday\nFav0001xyz\n Fav0002xyz \n");
        assert_eq!(read.ids(), [id(1), id(2)]);
    }

    #[test]
    fn the_list_is_saved_and_read_back() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join(FILE);
        assert_eq!(load_file(&path).unwrap(), Favourites::new(), "no file yet");
        let mut list = Favourites::new();
        list.set(&id(7), true);
        save_file(&path, &list).unwrap();
        assert_eq!(load_file(&path).unwrap(), list);
    }
}
