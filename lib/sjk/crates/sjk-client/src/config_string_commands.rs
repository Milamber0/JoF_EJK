//! Live reliable configstring application, separated for socket-free verification.

use crate::ClientError;
use sjk_protocol::{ConfigStringDirty, GameState, MAX_BIG_INFO_STRING_BYTES};

pub(super) fn apply(
    arguments: &[Vec<u8>],
    game: &mut GameState,
    dirty: &mut ConfigStringDirty,
    pending: &mut Option<(usize, Vec<u8>)>,
) -> Result<Option<usize>, ClientError> {
    let Some(name) = arguments.first().map(Vec::as_slice) else {
        return Ok(None);
    };
    if !matches!(name, b"cs" | b"bcs0" | b"bcs1" | b"bcs2") {
        return Ok(None);
    }
    let index = arguments
        .get(1)
        .and_then(|value| std::str::from_utf8(value).ok())
        .and_then(|value| value.parse::<usize>().ok())
        .ok_or(ClientError::MalformedConfigStringCommand)?;
    let fragment = arguments.get(2).cloned().unwrap_or_default();
    let value = match name {
        b"cs" => {
            *pending = None;
            fragment
        }
        b"bcs0" => {
            *pending = Some((index, fragment));
            return Ok(None);
        }
        b"bcs1" | b"bcs2" => {
            let (_, value) = pending
                .as_mut()
                .filter(|(slot, _)| *slot == index)
                .ok_or(ClientError::MalformedConfigStringCommand)?;
            if value.len() + fragment.len() > MAX_BIG_INFO_STRING_BYTES {
                return Err(ClientError::BigConfigStringTooLarge);
            }
            value.extend_from_slice(&fragment);
            if name == b"bcs1" {
                return Ok(None);
            }
            pending
                .take()
                .ok_or(ClientError::MalformedConfigStringCommand)?
                .1
        }
        _ => unreachable!(),
    };
    if game.replace_config_string(index, value)? {
        dirty.mark(index);
    }
    Ok(Some(index))
}
