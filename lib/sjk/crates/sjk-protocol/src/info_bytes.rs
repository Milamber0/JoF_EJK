//! Info strings as the reference reads them: raw bytes, not text. A player's name may
//! hold any byte, so nothing here assumes UTF-8.

/// `Info_ValueForKey` (`q_shared.c`): the value of the first pair whose key matches
/// without regard to case. A key without a value ends the search.
pub fn info_value<'a>(info: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut rest = info.strip_prefix(b"\\").unwrap_or(info);
    while !rest.is_empty() {
        let split = rest.iter().position(|&byte| byte == b'\\')?;
        let candidate = &rest[..split];
        rest = &rest[split + 1..];
        let end = rest
            .iter()
            .position(|&byte| byte == b'\\')
            .unwrap_or(rest.len());
        if candidate.eq_ignore_ascii_case(key) {
            return Some(&rest[..end]);
        }
        rest = rest.get(end + 1..).unwrap_or_default();
    }
    None
}

/// `Info_NextPair` (`q_shared.c`) until it yields an empty key: every `(key, value)`
/// in order. A last key with nothing after it comes with an empty value.
pub fn info_pairs(info: &[u8]) -> impl Iterator<Item = (&[u8], &[u8])> {
    let mut rest = info;
    std::iter::from_fn(move || {
        let body = rest.strip_prefix(b"\\").unwrap_or(rest);
        let key_end = body
            .iter()
            .position(|&byte| byte == b'\\')
            .unwrap_or(body.len());
        let key = &body[..key_end];
        if key.is_empty() {
            return None;
        }
        let after = body.get(key_end + 1..).unwrap_or_default();
        let value_end = after
            .iter()
            .position(|&byte| byte == b'\\')
            .unwrap_or(after.len());
        // The next call starts at the backslash that ended the value, as the reference does.
        rest = &after[value_end..];
        Some((key, &after[..value_end]))
    })
}

/// `MAX_INFO_STRING`.
const MAX_INFO_STRING: usize = 1024;

/// `Info_RemoveKey` (`q_shared.c`): the first pair whose key is `key`, case and all,
/// taken out. A walk that meets a key with no backslash after it stops there.
pub fn info_remove_key(info: &mut Vec<u8>, key: &[u8]) {
    if key.contains(&b'\\') {
        return;
    }
    let mut at = 0;
    loop {
        let start = at;
        if info.get(at) == Some(&b'\\') {
            at += 1;
        }
        let Some(key_end) = info[at..]
            .iter()
            .position(|&byte| byte == b'\\')
            .map(|end| at + end)
        else {
            return;
        };
        let found = &info[at..key_end] == key;
        let value_end = info[key_end + 1..]
            .iter()
            .position(|&byte| byte == b'\\')
            .map_or(info.len(), |end| key_end + 1 + end);
        if found {
            info.drain(start..value_end);
            return;
        }
        if value_end == info.len() {
            return;
        }
        at = value_end;
    }
}

/// `Info_SetValueForKey` (`q_shared.c`): the key taken out, then — for a value — put
/// first. A key or value holding `\`, `;` or `"` changes nothing; a pair that would not
/// fit is left out.
pub fn info_set_value(info: &[u8], key: &[u8], value: &[u8]) -> Vec<u8> {
    let mut info = info.to_vec();
    if key
        .iter()
        .chain(value)
        .any(|byte| matches!(byte, b'\\' | b';' | b'"'))
    {
        return info;
    }
    info_remove_key(&mut info, key);
    if value.is_empty() || key.len() + value.len() + 2 + info.len() >= MAX_INFO_STRING {
        return info;
    }
    [b"\\", key, b"\\", value, &info].concat()
}
