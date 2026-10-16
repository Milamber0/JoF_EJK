//! Retail-compatible forwarding decision for unknown console commands.

#[derive(Debug, Eq, PartialEq)]
pub(super) enum ForwardAction<'a> {
    Ignore,
    Unknown,
    Reliable(&'a str),
}

pub(super) fn forward_payload<'a>(
    command: &'a str,
    tokens: &'a [String],
    connected: bool,
) -> ForwardAction<'a> {
    let Some(name) = tokens.first() else {
        return ForwardAction::Ignore;
    };
    if name.starts_with('-') {
        return ForwardAction::Ignore;
    }
    if !connected || name.starts_with('+') {
        return ForwardAction::Unknown;
    }
    ForwardAction::Reliable(if tokens.len() > 1 { command } else { name })
}
