//! Automatic server demos (`sv_autoDemo`, `sv_ccmds.cpp:1561-1826`): every client in
//! the game is recorded from the moment it enters, each into
//! `demos/autorecord/<date>/<map> <map start>/<client> <name> <map> <now>.dm_26`. A map
//! change or restart ends every recording and starts new ones; with
//! `sv_autoDemoMaxMaps` only the newest maps' folders are kept.
use super::LegacyGameHost;
use super::hosts::View;
use crate::LegacyClientPhase;

/// `MAX_OSPATH`: every name here is cut to this, less one.
const OSPATH_BYTES: usize = 256;
/// `MAX_NAME_LENGTH`.
const NAME_BYTES: usize = 32;
/// Where the folders are, in the home directory's `base`.
const ROOT: &str = "demos/autorecord";
/// `SV_FindLeafFolders`' limit: the folders beyond it are neither kept nor pruned.
const MAX_FOLDERS: usize = 500;

/// The automatic demos' settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LegacyAutoDemoSettings {
    /// `sv_autoDemo`.
    pub enabled: bool,
    /// `sv_autoDemoMaxMaps`: how many maps' folders are kept; zero or less keeps all.
    pub max_maps: i32,
}

/// What the endpoint keeps of the current level for its demos.
#[derive(Clone, Debug, Default)]
pub(super) struct AutoDemoLevel {
    /// `sv.realMapTimeStarted`, as `%Y-%m-%d_%H-%M-%S`.
    pub started: String,
    /// `sv.demosPruned`.
    pub pruned: bool,
    /// `SV_SpawnServer`'s own `SV_BeginAutoRecordDemos` ran for this level; the first
    /// level's runs on the endpoint's first frame.
    pub spawned: bool,
}

impl AutoDemoLevel {
    /// The first level, before its first frame.
    pub fn first(now: String) -> Self {
        Self {
            started: now,
            pruned: false,
            spawned: false,
        }
    }
    /// A level that starts now (`SV_SpawnServer`, `SV_MapRestart_f`), whose caller
    /// begins the demos at once.
    pub fn starting(now: String) -> Self {
        Self {
            started: now,
            pruned: false,
            spawned: true,
        }
    }
}

/// `Q_CleanStr`: colour codes and every byte outside printable ASCII removed.
fn clean(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut at = 0;
    while at < text.len() {
        let byte = text[at];
        let colour = byte == b'^' && text.get(at + 1).is_some_and(|next| next.is_ascii_digit());
        if colour {
            at += 1;
        } else if (0x20..=0x7e).contains(&byte) {
            out.push(byte);
        }
        at += 1;
    }
    out
}

/// `Q_strstrip` of what a file name may not hold.
fn strip(text: &mut Vec<u8>) {
    text.retain(|byte| !b"\n\r;:.?*<>|\\/\"".contains(byte));
}

/// `Com_sprintf`'s cut to `MAX_OSPATH`.
fn cut(mut text: Vec<u8>) -> Vec<u8> {
    text.truncate(OSPATH_BYTES - 1);
    text
}

/// `SV_AutoRecordDemo`'s name for `client`'s demo: `player` is its name, `mapname` the
/// map's, `now` and `map_started` stamps as `%Y-%m-%d_%H-%M-%S`. Colour codes and bytes
/// outside printable ASCII are removed from the player's name, and the characters a file
/// name cannot hold from the file's and the folder's names.
pub fn legacy_auto_demo_name(
    client: usize,
    player: &[u8],
    mapname: &[u8],
    now: &str,
    map_started: &str,
) -> Vec<u8> {
    let player = clean(&player[..player.len().min(NAME_BYTES - 1)]);
    let mut file = cut([
        format!("{client} ").as_bytes(),
        &player,
        b" ",
        mapname,
        b" ",
        now.as_bytes(),
    ]
    .concat());
    let mut folder = cut([mapname, b" ", map_started.as_bytes()].concat());
    strip(&mut file);
    strip(&mut folder);
    // `%Y/%m/%d` of the map's start.
    let tree = map_started.get(..10).unwrap_or_default().replace('-', "/");
    cut([
        format!("autorecord/{tree}/").as_bytes(),
        &folder,
        b"/",
        &file,
    ]
    .concat())
}

/// The folders automatic demos are kept in, in the home directory's `base`.
pub trait LegacyDemoFolders {
    /// The subfolders of `path` (without `.` and `..`); none where it does not exist.
    fn folders(&self, path: &str) -> Vec<String>;
    /// Whether `path` holds any file.
    fn has_files(&self, path: &str) -> bool;
    /// `FS_HomeRmdir`: remove `path`, with everything under it when `recursive`, or only
    /// if it is empty otherwise.
    fn remove(&mut self, path: &str, recursive: bool);
}

/// `SV_FindLeafFolders`: every folder under `base` without subfolders, depth first, at
/// most `room` of them.
fn leaf_folders(
    folders: &impl LegacyDemoFolders,
    base: &str,
    room: usize,
    out: &mut Vec<String>,
) -> usize {
    let mut found = 0;
    for name in folders.folders(base) {
        let path = format!("{base}/{name}");
        let below = leaf_folders(folders, &path, room - found, out);
        found += below;
        if found >= room {
            break;
        }
        if below == 0 {
            out.push(path);
            found += 1;
            if found >= room {
                break;
            }
        }
    }
    found
}

/// `sscanf`'s `%<width>d`: blanks skipped, a sign, then digits, `width` characters in
/// all.
fn scan_number(text: &[u8], at: &mut usize, width: usize) -> Option<i64> {
    while text.get(*at).is_some_and(u8::is_ascii_whitespace) {
        *at += 1;
    }
    let start = *at;
    let negative = text.get(*at) == Some(&b'-');
    if matches!(text.get(*at), Some(b'+' | b'-')) {
        *at += 1;
    }
    let digits_start = *at;
    while *at - start < width && text.get(*at).is_some_and(u8::is_ascii_digit) {
        *at += 1;
    }
    if *at == digits_start {
        return None;
    }
    let value = text[digits_start..*at]
        .iter()
        .fold(0_i64, |value, digit| value * 10 + i64::from(digit - b'0'));
    Some(if negative { -value } else { value })
}

/// Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's algorithm).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// `SV_ExtractTimeFromDemoFolder`: the moment the last 19 characters of the folder's own
/// name give (`%4d-%2d-%2d_%2d-%2d-%2d`, fields out of range carried over as `mktime`
/// carries them), in UTC; 0 where they are not one.
fn folder_time(path: &str) -> i64 {
    let name = path.rsplit('/').next().unwrap_or(path).as_bytes();
    const LENGTH: usize = "0000-00-00_00-00-00".len();
    if name.len() < LENGTH {
        return 0;
    }
    let text = &name[name.len() - LENGTH..];
    let mut at = 0;
    let mut fields = [0_i64; 6];
    for (index, (width, separator)) in [
        (4, Some(b'-')),
        (2, Some(b'-')),
        (2, Some(b'_')),
        (2, Some(b'-')),
        (2, Some(b'-')),
        (2, None),
    ]
    .into_iter()
    .enumerate()
    {
        let Some(value) = scan_number(text, &mut at, width) else {
            return 0;
        };
        fields[index] = value;
        if let Some(separator) = separator {
            if text.get(at) != Some(&separator) {
                return 0;
            }
            at += 1;
        }
    }
    let [year, month, day, hour, minute, second] = fields;
    let month = month - 1;
    let days = days_from_civil(year + month.div_euclid(12), month.rem_euclid(12) + 1, 1) + day - 1;
    days * 86_400 + hour * 3_600 + minute * 60 + second
}

/// The pruning in `SV_BeginAutoRecordDemos`: the maps' folders (every folder under
/// `demos/autorecord` without subfolders) newest first by the time their names end with,
/// those without one last in reverse order of name; every one past the `keep` newest
/// removed with all it holds, and each parent it leaves empty up to `demos/autorecord`.
pub fn legacy_prune_auto_demos(folders: &mut impl LegacyDemoFolders, keep: usize) {
    let mut leaves = Vec::new();
    leaf_folders(folders, ROOT, MAX_FOLDERS, &mut leaves);
    // `SV_DemoFolderTimeComparator`. The reference compares the times' difference as an
    // `int`, which folders more than 68 years apart overflow; they are ordered by time.
    let mut keyed: Vec<(i64, String)> = leaves
        .into_iter()
        .map(|path| (folder_time(&path), path))
        .collect();
    keyed.sort_by(|(left_time, left), (right_time, right)| {
        match (*left_time == 0, *right_time == 0) {
            (true, true) => right.cmp(left),
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => right_time.cmp(left_time),
        }
    });
    for (_, folder) in keyed.into_iter().skip(keep) {
        folders.remove(&folder, true);
        let mut parent = folder.as_str();
        while let Some(slash) = parent.rfind('/') {
            parent = &parent[..slash];
            if parent == ROOT {
                break;
            }
            if !folders.has_files(parent) && folders.folders(parent).is_empty() {
                folders.remove(parent, false);
            } else {
                break;
            }
        }
    }
}

/// `SV_BeginAutoRecordDemos`: every client in the game not recorded yet starts a demo;
/// then, once a level, the old maps' folders are pruned.
pub(super) fn begin<G: LegacyGameHost>(view: &mut View<'_, G>) {
    let settings = view.settings.auto_demo;
    if !settings.enabled {
        return;
    }
    let now = view.game.timestamp();
    for client in 0..view.slots.len() {
        let slot = &view.slots[client];
        if slot.phase != LegacyClientPhase::Active || slot.demo.recording.is_some() {
            continue;
        }
        let name = legacy_auto_demo_name(
            client,
            &slot.name,
            view.game.server_info().mapname,
            &now,
            &view.auto_demo.started,
        );
        // `SV_RecordDemo`.
        let path = format!("demos/{}.dm_26", String::from_utf8_lossy(&name));
        view.game
            .console_log(format!("recording to {path}.\n").as_bytes());
        super::demo::flush_demo(&mut view.slots[client], client, &mut *view.game);
        if view.game.demo_open(client, &path) {
            view.slots[client].start_demo(client, &name, &*view.game);
        } else {
            view.game.console_log(b"ERROR: couldn't open.\n");
        }
    }
    if settings.max_maps > 0 && !view.auto_demo.pruned {
        view.game.prune_auto_demos(settings.max_maps as usize);
        view.auto_demo.pruned = true;
    }
}

/// `SV_StopAutoRecordDemos`: with automatic demos on, every recording ends.
pub(super) fn stop<G: LegacyGameHost>(view: &mut View<'_, G>) {
    if !view.settings.auto_demo.enabled {
        return;
    }
    for client in 0..view.slots.len() {
        if view.slots[client].demo.recording.is_some() {
            view.slots[client].stop_demo();
            view.game
                .console_log(format!("Stopped demo for client {client}.\n").as_bytes());
        }
    }
}
