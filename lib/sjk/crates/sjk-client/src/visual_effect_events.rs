//! BaseJKA event-to-EFX decisions shared by live presentation and audits.

/// Return the registered EFX graph played for a stock player event.
///
/// `EV_BECOME_JEDIMASTER` uses `mp/jedispawn`; both teleport directions use
/// `mp/spawn` (`codemp/cgame/cg_event.c:2400-2430,2699-2750`).
pub fn legacy_visual_effect(event: u16) -> Option<&'static str> {
    match event & 0xff {
        34 => Some("mp/jedispawn"),
        64 | 65 => Some("mp/spawn"),
        _ => None,
    }
}
