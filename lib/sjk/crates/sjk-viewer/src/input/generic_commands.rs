//! Bind-command names that travel as `usercmd_t::generic_cmd`.
//!
//! The stock client registers one `IN_GenCMD<n>` handler per name
//! (`codemp/client/cl_input.cpp:1719-1750`); each stores a `genCmds_t` value
//! (`codemp/qcommon/q_shared.h:1389-1421`) that the next user command carries.
//! Only one value fits per command, so the latest press in a frame wins, as
//! in `CL_CreateCmd`.

/// Usercmd `generic_cmd` value used by codemp's `sv_saberswitch` command.
pub(crate) const GENCMD_SABER_SWITCH: u8 = 1;
/// Usercmd `generic_cmd` value used by codemp's `saberAttackCycle` command.
pub(crate) const GENCMD_SABER_ATTACK_CYCLE: u8 = 26;

/// `(bind command, genCmds_t value)`, in `genCmds_t` order.
pub(crate) const GENERIC_COMMANDS: [(&str, u8); 31] = [
    ("sv_saberswitch", GENCMD_SABER_SWITCH),
    ("engage_duel", 2),
    ("force_heal", 3),
    ("force_speed", 4),
    ("force_throw", 5),
    ("force_pull", 6),
    ("force_distract", 7),
    ("force_rage", 8),
    ("force_protect", 9),
    ("force_absorb", 10),
    ("force_healother", 11),
    ("force_forcepowerother", 12),
    ("force_seeing", 13),
    ("use_seeker", 14),
    ("use_field", 15),
    ("use_bacta", 16),
    ("use_electrobinoculars", 17),
    ("zoom", 18),
    ("use_sentry", 19),
    ("use_jetpack", 20),
    ("use_bactabig", 21),
    ("use_healthdisp", 22),
    ("use_ammodisp", 23),
    ("use_eweb", 24),
    ("use_cloak", 25),
    ("saberattackcycle", GENCMD_SABER_ATTACK_CYCLE),
    ("taunt", 27),
    ("bow", 28),
    ("meditate", 29),
    ("flourish", 30),
    ("gloat", 31),
];

/// The `generic_cmd` value for a lower-cased bind command, if it is one.
pub(crate) fn generic_command(name: &str) -> Option<u8> {
    GENERIC_COMMANDS
        .iter()
        .find(|(command, _)| *command == name)
        .map(|(_, value)| *value)
}
