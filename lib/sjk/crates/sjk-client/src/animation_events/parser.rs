//! Load-time parsing of the audio subset of BG_ParseAnimationEvtFile.
use super::*;

pub(super) fn load(
    out: &mut Events,
    vfs: &VirtualFileSystem,
    path: &str,
    config: &AnimationConfig,
    stack: &mut Vec<String>,
) {
    if stack.len() >= 16 || stack.iter().any(|p| p.eq_ignore_ascii_case(path)) {
        return;
    }
    let Ok(Some(asset)) = vfs.read(path) else {
        return;
    };
    if asset.bytes.len() >= 80_000 {
        return;
    }
    stack.push(path.to_owned());
    let tokens = crate::catalog_tokens::tokenize(&String::from_utf8_lossy(&asset.bytes));
    let mut i = 0;
    let mut track = None;
    while let Some(token) = tokens.get(i) {
        i += 1;
        match token.to_ascii_lowercase().as_str() {
            "include" => {
                if let Some(model) = tokens.get(i) {
                    load(
                        out,
                        vfs,
                        &format!("models/players/{model}/animevents.cfg"),
                        config,
                        stack,
                    );
                    i += 1;
                }
            }
            "lowerevents" => track = Some(0),
            "upperevents" => track = Some(1),
            "}" => track = None,
            _ => {
                let (Some(track), Some(sequence)) = (track, config.get(token)) else {
                    continue;
                };
                if sequence.frame_count == 0 {
                    continue;
                }
                if let Some(mut event) = event(&tokens, &mut i, sequence) {
                    let table = &mut out.tracks[track];
                    if let Some(old) = table
                        .iter_mut()
                        .find(|old| old.frame == event.frame && old.kind == event.kind)
                    {
                        event.order = old.order;
                        *old = event;
                    } else if table.len() < 600 {
                        event.order = table.len();
                        table.push(event);
                    }
                }
            }
        }
    }
    stack.pop();
}

fn next<'a>(tokens: &'a [String], i: &mut usize) -> Option<&'a str> {
    let token = tokens.get(*i)?;
    *i += 1;
    Some(token)
}
fn number(tokens: &[String], i: &mut usize) -> Option<i32> {
    next(tokens, i)?.parse().ok()
}

fn event(tokens: &[String], i: &mut usize, sequence: &AnimationSequence) -> Option<Event> {
    let kind = next(tokens, i)?.to_ascii_uppercase();
    let frame = (sequence.first_frame as i32).checked_add(number(tokens, i)?)?;
    let (cue, probability) = match kind.as_str() {
        "AEV_FOOTSTEP" => {
            let foot = next(tokens, i)?.to_ascii_uppercase();
            let cue = Cue::Footstep {
                right: foot.ends_with("_R"),
                heavy: foot.contains("HEAVY"),
                extra: false,
            };
            (cue, number(tokens, i)?)
        }
        "AEV_SOUND" | "AEV_SOUNDCHAN" => {
            let mut channel = if kind == "AEV_SOUNDCHAN" {
                channel(next(tokens, i)?)
            } else {
                0
            };
            let path = next(tokens, i)?.replace('\\', "/").to_ascii_lowercase();
            let low = number(tokens, i)?;
            let high = number(tokens, i)?;
            let chance = number(tokens, i)?;
            // codemp leaves custom '*' animation sounds unresolved.
            if path.starts_with('*') {
                return None;
            }
            let paths = if path.starts_with("sound/weapons/saber/saberhup") {
                channel = 0;
                let low = if low < 4 {
                    1
                } else if low < 7 {
                    4
                } else {
                    7
                };
                (low..low + 3)
                    .map(|n| format!("sound/weapons/saber/saberhup{n}.wav"))
                    .collect()
            } else if path.starts_with("sound/weapons/saber/saberspin") {
                channel = 0;
                if path.contains('%') {
                    (1..=3)
                        .map(|n| format!("sound/weapons/saber/saberspin{n}.wav"))
                        .collect()
                } else {
                    vec![path]
                }
            } else if low != 0 && high != 0 {
                (low..=high.min(low.saturating_add(3)))
                    .map(|n| {
                        path.replace("%d", &n.to_string())
                            .replace("%i", &n.to_string())
                    })
                    .collect()
            } else {
                vec![path]
            };
            if paths.is_empty() {
                return None;
            }
            (Cue::Sound { paths, channel }, chance)
        }
        _ => return None,
    };
    Some(Event {
        frame,
        probability: probability.clamp(0, 100) as u8,
        cue,
        kind,
        order: 0,
    })
}
fn channel(name: &str) -> u8 {
    match name.to_ascii_uppercase().as_str() {
        "CHAN_WEAPON" => 2,
        "CHAN_VOICE" => 3,
        "CHAN_VOICE_ATTEN" => 4,
        "CHAN_VOICE_GLOBAL" => 12,
        "CHAN_BODY" => 6,
        "CHAN_ANNOUNCER" => 9,
        _ => 0,
    }
}
