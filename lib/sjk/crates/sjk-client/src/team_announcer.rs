//! EV_GLOBAL_TEAM_SOUND: codemp/cgame/cg_event.c:3205-3294.
//!
//! This MP revision uses absolute red/blue wording, regardless of listener team.
//! GTS_RED/BLUE_CAPTURE are deliberately silent; SCORED carries the announcement.

/// Register the CTF, CTY and team-score media in stable table order.
pub(super) fn register(intern: &mut impl FnMut(&str) -> u16) -> [Option<u16>; 13] {
    // cg_main.c:746-763, returned/taken CTF, returned/taken CTY, score/lead/tied.
    [
        "042", "041", "040", "039", "050", "049", "048", "047", "044", "043", "046", "045", "032",
    ]
    .map(|suffix| Some(intern(&format!("sound/chars/protocol/misc/40MOM{suffix}"))))
}

/// Select the stock media slot, including intentionally silent capture events.
pub(super) fn index(parameter: u8, gametype: i32) -> Option<usize> {
    let flag_offset = if gametype == 9 { 4 } else { 0 }; // GT_CTY
    match parameter {
        2 => Some(flag_offset + 1), // red returned the BLUE flag
        3 => Some(flag_offset),
        4 => Some(flag_offset + 2),
        5 => Some(flag_offset + 3),
        6..=10 => Some(usize::from(parameter) + 2),
        _ => None,
    }
}
