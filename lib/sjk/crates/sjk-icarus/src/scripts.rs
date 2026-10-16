//! Script files: reading and caching them, running one on an entity, and precaching
//! what a script uses (`ICARUS_GetScript`, `ICARUS_RegisterScript`, `ICARUS_RunScript`,
//! `ICARUS_InterrogateScript` in `GameInterface.cpp`).
//!
//! Scripts are named without their `.IBI` extension and cached under the exact name
//! asked for; the file itself is read as `<name>.IBI` through the host, whose file
//! system decides case.

use std::sync::Arc;

use crate::Icarus;
use crate::host::{DebugLevel, IcarusHost, Owner, SetKind};
use crate::ids::*;
use crate::print;
use crate::stream::BlockStream;

/// The directory scripts are named from (`Q3_SCRIPT_DIR`).
pub const SCRIPT_DIR: &str = "scripts";

/// `COM_StripExtension` into a buffer of `size` bytes: the text after the last `.`
/// (if it comes after the last `/`) dropped, and the rest cut to `size - 1` bytes.
pub fn strip_extension(name: &str, size: usize) -> String {
    let dot = name.rfind('.');
    let slash = name.rfind('/');
    let mut end = name.len();
    if let Some(dot) = dot {
        if slash.is_none_or(|slash| slash < dot) {
            end = dot;
        }
    }
    let mut end = end.min(size.saturating_sub(1));
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_owned()
}

impl<O: Owner> Icarus<O> {
    /// `ICARUS_RegisterScript`: reads and caches `<name>.IBI`. True if it is cached —
    /// except while precaching, when a script already cached answers false, which stops
    /// a script that runs itself from being precached forever.
    pub fn register_script<H: IcarusHost<O> + ?Sized>(
        &mut self,
        name: &str,
        interrogating: bool,
        host: &mut H,
    ) -> bool {
        if self.scripts.contains_key(name) {
            return !interrogating;
        }
        let mut file = format!("{name}.IBI");
        file.truncate(floor_boundary(&file, 1023));
        let data = host.read_file(&file).filter(|data| !data.is_empty());
        let Some(data) = data else {
            if !interrogating {
                host.print(&format!("^1Could not open file '{file}'\n"));
            }
            return false;
        };
        self.scripts.insert(name.to_owned(), Arc::from(data));
        true
    }

    /// `ICARUS_GetScript`: a cached script, read first if need be.
    pub(crate) fn get_script<H: IcarusHost<O> + ?Sized>(
        &mut self,
        name: &str,
        host: &mut H,
    ) -> Option<Arc<[u8]>> {
        if !self.scripts.contains_key(name) && !self.register_script(name, false, host) {
            return None;
        }
        self.scripts.get(name).cloned()
    }

    /// `ICARUS_RunScript`: the script (`name` includes the `scripts/` directory) run on
    /// the entity, replacing what it ran — which it returns to afterwards. False if the
    /// entity has no sequencer or the script cannot be read.
    pub fn run_script<H: IcarusHost<O> + ?Sized>(
        &mut self,
        owner: O,
        name: &str,
        host: &mut H,
    ) -> bool {
        if !self.sequencers.contains_key(&owner) {
            return false;
        }
        let Some(buffer) = self.get_script(name, host) else {
            return false;
        };
        if !self.run_buffer(owner, buffer, host) {
            return false;
        }
        if host.developer() {
            let names = host.entity_names(owner);
            let text = format!(
                "{} Script {} executed by {} {}\n",
                host.time(),
                name,
                names.classname.as_deref().unwrap_or("(null)"),
                names.targetname.as_deref().unwrap_or("(null)")
            );
            print::debug(host, DebugLevel::Verbose, &text);
        }
        true
    }

    /// `ICARUS_InterrogateScript`: a script and every script it runs or names read into
    /// the cache, and the sounds and ROFFs they use precached. `filename` may include the
    /// `scripts/` directory or not; `NULL` and `default` are no scripts.
    pub fn interrogate<H: IcarusHost<O> + ?Sized>(&mut self, filename: &str, host: &mut H) {
        if filename.eq_ignore_ascii_case("NULL") || filename.eq_ignore_ascii_case("default") {
            return;
        }
        let has_dir = filename.len() >= SCRIPT_DIR.len()
            && filename.as_bytes()[..SCRIPT_DIR.len()].eq_ignore_ascii_case(SCRIPT_DIR.as_bytes());
        let mut name = if has_dir {
            filename.to_owned()
        } else {
            format!("{SCRIPT_DIR}/{filename}")
        };
        name.truncate(floor_boundary(&name, 1023));
        if !self.register_script(&name, true, host) {
            return;
        }
        let Some(buffer) = self.scripts.get(&name).cloned() else {
            return;
        };
        let Some(mut stream) = BlockStream::open(buffer) else {
            return;
        };
        while stream.block_available() {
            let block = stream.read_block();
            match block.id {
                ID_CAMERA => {
                    if block.f32_at(0) == TYPE_PATH as f32 {
                        host.cache_roff(&block.str_at(1));
                    }
                }
                ID_PLAY => {
                    if block.str_at(0).eq_ignore_ascii_case("PLAY_ROFF") {
                        host.cache_roff(&block.str_at(1));
                    }
                }
                ID_RUN => {
                    let run = strip_extension(&block.str_at(0), 1024);
                    self.interrogate(&run, host);
                }
                ID_SOUND => host.precache_sound(&block.str_at(1)),
                ID_SET => {
                    if block.member_id(0) == Some(TK_STRING) {
                        let (set, value) = (block.str_at(0), block.str_at(1));
                        match host.set_kind(&set) {
                            SetKind::Script => self.interrogate(&value, host),
                            SetKind::LoopSound => host.precache_sound(&value),
                            SetKind::Other => {}
                        }
                    }
                }
                0 => return,
                _ => {}
            }
        }
    }
}

/// The largest char boundary at or below `limit`.
fn floor_boundary(text: &str, limit: usize) -> usize {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    end
}
