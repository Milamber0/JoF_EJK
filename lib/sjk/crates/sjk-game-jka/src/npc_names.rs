//! What `npc spawn <name>` finds beyond the reference's exact lookup.
//! These are this server's own names, outside the stock rules
//! (`g_stockRules 0`); the reference takes a name exactly as `NPC_ParseParms` reads it
//! (`Q_stricmp`), refuses a vehicle's name without `vehicle` ("NPC_ParseParms: ... is a
//! vehicle"), and refuses an NPC with no `playerModel` ("MD3 MODEL NPC'S ARE NOT
//! SUPPORTED IN MP!", `NPC_stats.c:3540`).

use crate::text_parse::TextParser;

/// The name of every NPC block in the joined NPC text, in order.
pub fn npc_names(text: &[u8]) -> Vec<Vec<u8>> {
    let mut parser = TextParser::new(text);
    let mut names = Vec::new();
    while parser.is_live() {
        let token = parser.parse_ext(true);
        if token.is_empty() {
            break;
        }
        names.push(token.to_vec());
        parser.skip_braced_section(0);
    }
    names
}

/// What a requested name is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// An NPC, by its own name.
    Npc(Vec<u8>),
    /// A vehicle, by its own name (`npc spawn vehicle <name>`).
    Vehicle(Vec<u8>),
    /// Several names answer; the request is refused with them.
    Ambiguous(Vec<Vec<u8>>),
    /// Nothing answers: the reference's own refusal follows.
    Unknown,
}

/// A name with its case, `_`, `-` and spaces gone: `mine_monster` and `Minemonster`,
/// `xwing` and `X-Wing` are the same.
fn key(name: &[u8]) -> Vec<u8> {
    name.iter()
        .filter(|byte| !matches!(byte, b'_' | b'-' | b' '))
        .map(u8::to_ascii_lowercase)
        .collect()
}

/// `requested` against the NPCs and the vehicles: a vehicle's exact name is the vehicle
/// (`npc spawn tauntaun`, `npc spawn swoop`), an NPC's exact name the NPC; then the same
/// with case, `_`, `-` and spaces ignored — an NPC first, then a vehicle, then a vehicle
/// with `_mp` (`swoop` finds `swoop_mp` only when nothing else answers).
pub fn resolve(requested: &[u8], npcs: &[Vec<u8>], vehicles: &[Vec<u8>]) -> Resolved {
    if let Some(vehicle) = vehicles
        .iter()
        .find(|name| name.eq_ignore_ascii_case(requested))
    {
        return Resolved::Vehicle(vehicle.clone());
    }
    if let Some(npc) = npcs
        .iter()
        .find(|name| name.eq_ignore_ascii_case(requested))
    {
        return Resolved::Npc(npc.clone());
    }
    let wanted = key(requested);
    let unique = |names: &[Vec<u8>], wanted: &[u8]| -> Option<Result<Vec<u8>, Vec<Vec<u8>>>> {
        let found: Vec<Vec<u8>> = names
            .iter()
            .filter(|name| key(name) == wanted)
            .cloned()
            .collect();
        match found.len() {
            0 => None,
            1 => Some(Ok(found[0].clone())),
            _ => Some(Err(found)),
        }
    };
    let mp = [wanted.as_slice(), b"mp"].concat();
    match (
        unique(npcs, &wanted),
        unique(vehicles, &wanted),
        unique(vehicles, &mp),
    ) {
        (Some(Ok(npc)), None, _) => Resolved::Npc(npc),
        (None, Some(Ok(vehicle)), _) => Resolved::Vehicle(vehicle),
        (None, None, Some(Ok(vehicle))) => Resolved::Vehicle(vehicle),
        (None, None, None) => Resolved::Unknown,
        (npcs, vehicles, _) => {
            let flatten = |found: Option<Result<Vec<u8>, Vec<Vec<u8>>>>| match found {
                Some(Ok(name)) => vec![name],
                Some(Err(names)) => names,
                None => Vec::new(),
            };
            Resolved::Ambiguous([flatten(npcs), flatten(vehicles)].concat())
        }
    }
}
