//! Slot updates preserve saber state and the ambient world's runtime timers.

use super::*;

impl LegacyLoopAdapter {
    pub(crate) fn remap_sound_handles(
        &mut self,
        mut remap: impl FnMut(SoundHandle) -> SoundHandle,
    ) {
        for sound in &mut self.sounds {
            sound.handle = sound.handle.map(&mut remap);
        }
    }

    pub(crate) fn refresh_sound_table(
        &mut self,
        index: usize,
        game_state: &GameState,
        vfs: &VirtualFileSystem,
        register: &mut impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        let path = game_state
            .config_string(index)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default();
        let mut intern = |path: &str| intern_sound(&mut self.sounds, vfs, path, register);
        match index {
            CS_SOUNDS..=1066 => {
                self.cs_sounds[index - CS_SOUNDS] = if path.is_empty() || path.starts_with('*') {
                    None
                } else {
                    Some(intern(path))
                };
            }
            CS_AMBIENT_SET..=292 => {
                let stages = &mut self.soundsets[index - CS_AMBIENT_SET];
                *stages = SoundsetStages::default();
                if let Some(set) = self
                    .ambient_catalog
                    .get(path)
                    .filter(|set| set.kind == AmbientSetKind::Bmodel)
                {
                    stages.present = true;
                    for (slot, path) in stages.stages.iter_mut().zip(&set.sub_waves) {
                        *slot = Some(intern(path));
                    }
                }
                self.ambient.refresh_config_string(
                    index,
                    game_state,
                    &self.ambient_catalog,
                    &mut intern,
                );
            }
            CS_GLOBAL_AMBIENT_SET => self.ambient.refresh_config_string(
                index,
                game_state,
                &self.ambient_catalog,
                &mut intern,
            ),
            _ => {}
        }
    }
}
