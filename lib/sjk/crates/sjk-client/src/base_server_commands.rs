//! Typed BaseJKA reliable-command decisions outside configstrings and scores.
//!
//! The dispatch entries are in `codemp/cgame/cg_servercmds.c:1608-1626`.
//! Parsers below mirror `CG_NewForceRank_f` (1325-1350),
//! `CG_RestoreClientGhoul_f` (1407-1472), and
//! `CG_SiegeProfileMenu_f` (1318-1323).

use crate::team_info::{argument, legacy_atoi};

pub(crate) fn killed_entity(value: &[u8]) -> Option<u16> {
    let number = legacy_atoi(value);
    (32..1024).contains(&number).then_some(number as u16)
}

/// Values stored by cgame when a server sends `nfr`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ForceRankUpdate {
    /// New force rank selected by the server.
    pub rank: i32,
    /// Whether cgame would open the siege force-profile menu.
    pub open_profile: bool,
    /// Team value copied to cgame's `ui_myteam` cvar.
    pub team: i32,
}

/// Reliable request to restore the mutable Ghoul2 state for one client.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ghoul2Restore {
    /// Client whose mutable actor state must be rebuilt.
    pub client_num: u8,
    /// `ircg` additionally copies this player's current pose into a body slot.
    pub immediate_body: Option<ImmediateBodyCopy>,
}

/// Additional body-copy operands carried only by `ircg`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImmediateBodyCopy {
    /// Corpse entity slot receiving the client's current appearance.
    pub body_entity: i32,
    /// Weapon index copied to the corpse in codemp.
    pub weapon: i32,
    /// Force-side selector copied to the corpse in codemp.
    pub light_side: bool,
}

/// Typed client-shell or presentation action emitted by a BaseJKA command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BaseServerCommandEvent {
    /// Update force-rank UI state from `nfr`.
    ForceRank(ForceRankUpdate),
    /// Rebuild a client actor, with an optional `ircg` body copy.
    RestoreGhoul2(Ghoul2Restore),
    /// Immutable client identity captured at the reliable `ircg` command.
    CopyBody(BodyIdentity),
    /// Release a non-client entity's copied Ghoul2 state (`kg2`).
    KillGhoul2(u16),
    /// First observation of an unknown reliable command name (bounded to 64).
    UnknownCommand(String),
    /// Ask the modern shell to present its siege-profile flow.
    SiegeProfileMenu,
    /// Open Siege class selection (`scl`), with no operands.
    SiegeClassSelect,
}

/// Operand-free UI commands (TaystJK codemp/cgame/cg_servercmds.c:1468-1484).
pub(crate) fn siege_menu_command(name: &[u8]) -> Option<BaseServerCommandEvent> {
    match name {
        b"scl" => Some(BaseServerCommandEvent::SiegeClassSelect),
        b"spc" => Some(BaseServerCommandEvent::SiegeProfileMenu),
        _ => None,
    }
}

/// Owned appearance and hilt identity, independent of later CS_PLAYERS updates.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyIdentity {
    /// Source client slot.
    pub client_num: u8,
    /// Validated body entity slot.
    pub entity_num: u16,
    /// Model and team-resolved skin at the time of the copy.
    pub appearance: sjk_runtime::Appearance,
    /// Both saber definition names at the time of the copy.
    pub sabers: [Option<String>; 2],
    /// Weapon supplied by CopyToBodyQue.
    pub weapon: i32,
}

impl BodyIdentity {
    /// Capture before subsequent reliable configstring changes are applied.
    pub fn capture(game: &sjk_protocol::GameState, restore: Ghoul2Restore) -> Option<Self> {
        let copy = restore.immediate_body?;
        let entity_num = u16::try_from(copy.body_entity)
            .ok()
            .filter(|n| (32..1024).contains(n))?;
        Some(Self {
            client_num: restore.client_num,
            entity_num,
            appearance: crate::legacy_client_appearance(game, u16::from(restore.client_num))?,
            sabers: crate::legacy_client_saber_names(game, u16::from(restore.client_num)),
            weapon: copy.weapon,
        })
    }
}

/// Parse `nfr`; malformed short commands are ignored like cgame's early return.
pub fn parse_new_force_rank(arguments: &[Vec<u8>]) -> Option<ForceRankUpdate> {
    // The original checks argc < 3, then reads argv(3); an omitted team thus
    // retains atoi("") == 0 in release builds.
    (arguments.len() >= 3).then(|| ForceRankUpdate {
        rank: legacy_atoi(argument(arguments, 1)),
        open_profile: legacy_atoi(argument(arguments, 2)) != 0,
        team: legacy_atoi(argument(arguments, 3)),
    })
}

/// Parse `rcg`/`ircg`, rejecting the same invalid client slot cgame ignores.
pub fn parse_restore_client_ghoul(name: &[u8], arguments: &[Vec<u8>]) -> Option<Ghoul2Restore> {
    let client = legacy_atoi(argument(arguments, 1));
    let client_num = u8::try_from(client)
        .ok()
        .filter(|client| usize::from(*client) < 32)?;
    Some(Ghoul2Restore {
        client_num,
        immediate_body: (name == b"ircg").then(|| ImmediateBodyCopy {
            body_entity: legacy_atoi(argument(arguments, 2)),
            weapon: legacy_atoi(argument(arguments, 3)),
            light_side: legacy_atoi(argument(arguments, 4)) != 0,
        }),
    })
}
