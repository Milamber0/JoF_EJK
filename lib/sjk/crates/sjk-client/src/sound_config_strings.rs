//! Incremental `CG_ConfigStringModified` sound registration; latches stay intact.

use super::*;

impl LegacySoundAdapter {
    /// Translate worker-local handles into the process sound bank on installation.
    pub fn remap_sound_handles(&mut self, mut remap: impl FnMut(SoundHandle) -> SoundHandle) {
        for sound in &mut self.sounds {
            sound.handle = sound.handle.map(&mut remap);
        }
        self.loops.remap_sound_handles(remap);
    }

    /// Registered global sound for a protocol sound-table slot, for diagnostics.
    pub fn registered_config_sound(&self, slot: u8) -> Option<&RegisteredLegacySound> {
        self.cs_sounds[usize::from(slot)].and_then(|sound| self.sound(sound))
    }

    /// Re-intern one changed sound/ambient slot without resetting playing sounds,
    /// event latches, custom player sounds, or ambient crossfade state.
    pub fn refresh_sound_table(
        &mut self,
        index: usize,
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        mut register: impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        let path = game_state
            .config_string(index)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default();
        let mut intern = |path: &str| intern_sound(&mut self.sounds, vfs, path, &mut register);
        match index {
            CS_SOUNDS..=1066 => {
                let slot = index - CS_SOUNDS;
                self.cs_custom[slot] = custom_kind(path);
                self.cs_sounds[slot] = if path.is_empty() || path.starts_with('*') {
                    None
                } else {
                    Some(intern(path))
                };
            }
            CS_AMBIENT_SET..=292 => {
                let stages = &mut self.ambient_stages[index - CS_AMBIENT_SET];
                *stages = [None; 3];
                if let Some(set) = self
                    .ambient_catalog
                    .get(path)
                    .filter(|set| set.kind == AmbientSetKind::Bmodel)
                {
                    for (slot, path) in stages.iter_mut().zip(&set.sub_waves) {
                        *slot = Some(intern(path));
                    }
                }
            }
            crate::ambient_world::CS_GLOBAL_AMBIENT_SET => {}
            _ => return,
        }
        self.loops
            .refresh_sound_table(index, game_state, vfs, &mut register);
    }
}
