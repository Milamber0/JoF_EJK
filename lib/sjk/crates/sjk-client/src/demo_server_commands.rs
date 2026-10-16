//! Reliable configstring application for offline demo streams.

use super::*;

pub(super) struct OfflineServerCommands {
    reliable_sequence: i32,
    pending_big_config_string: Option<(usize, Vec<u8>)>,
}

impl OfflineServerCommands {
    pub(super) fn new(reliable_sequence: i32) -> Self {
        Self {
            reliable_sequence,
            pending_big_config_string: None,
        }
    }

    pub(super) fn apply(
        &mut self,
        sequence: i32,
        command: &[u8],
        game_state: &mut GameState,
        dirty: &mut sjk_protocol::ConfigStringDirty,
        remaps: &mut crate::ShaderRemaps,
    ) -> Result<(), DemoPlaybackError> {
        if sequence <= self.reliable_sequence {
            return Ok(());
        }
        self.reliable_sequence = sequence;
        let arguments = tokenize_command(command);
        let Some(name) = arguments.first().map(Vec::as_slice) else {
            return Ok(());
        };
        if remaps.command(&arguments) {
            return Ok(());
        }
        match name {
            b"cs" => {
                let index = parse_index(arguments.get(1))?;
                let value = arguments.get(2).map_or(&[][..], Vec::as_slice);
                self.pending_big_config_string = None;
                if game_state.replace_config_string(index, value.to_vec())? {
                    dirty.mark(index);
                }
                if index == crate::SHADER_STATE_CONFIG {
                    remaps.apply_config(game_state.config_string(index).unwrap_or_default());
                }
            }
            b"bcs0" => {
                let index = parse_index(arguments.get(1))?;
                let value = arguments.get(2).map_or(&[][..], Vec::as_slice);
                self.pending_big_config_string = Some((index, value.to_vec()));
            }
            b"bcs1" => {
                let index = parse_index(arguments.get(1))?;
                let value = arguments.get(2).map_or(&[][..], Vec::as_slice);
                self.append_big(index, value)?;
            }
            b"bcs2" => {
                let index = parse_index(arguments.get(1))?;
                let value = arguments.get(2).map_or(&[][..], Vec::as_slice);
                self.append_big(index, value)?;
                let (pending_index, bytes) = self
                    .pending_big_config_string
                    .take()
                    .ok_or(DemoPlaybackError::UnexpectedBigConfigPart)?;
                if game_state.replace_config_string(pending_index, bytes)? {
                    dirty.mark(pending_index);
                }
                if pending_index == crate::SHADER_STATE_CONFIG {
                    remaps
                        .apply_config(game_state.config_string(pending_index).unwrap_or_default());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn append_big(&mut self, index: usize, bytes: &[u8]) -> Result<(), DemoPlaybackError> {
        let (pending_index, pending) = self
            .pending_big_config_string
            .as_mut()
            .ok_or(DemoPlaybackError::UnexpectedBigConfigPart)?;
        if *pending_index != index {
            return Err(DemoPlaybackError::MismatchedBigConfigIndex {
                expected: *pending_index,
                actual: index,
            });
        }
        if pending.len().saturating_add(bytes.len()) > MAX_BIG_CONFIG_STRING_BYTES {
            return Err(DemoPlaybackError::BigConfigTooLarge);
        }
        pending.extend_from_slice(bytes);
        Ok(())
    }
}
