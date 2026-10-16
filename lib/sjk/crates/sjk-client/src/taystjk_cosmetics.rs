//! TaystJK's reliable `cosmetics` unlock-table command.
//!
//! At TaystJK commit 5802c999, `CG_ParseCosmetics` parses one quoted payload
//! as colon/newline/tab-delimited groups of bit value, map, style, and duration
//! (`codemp/cgame/cg_servercmds.c:129-160`). Cgame's receiving table is capped
//! at 32 rows (`codemp/cgame/cg_local.h:2614-2623`). This adapter preserves
//! that fixed bound and is enabled only by [`crate::CompatProfile::TaystJk`].

use crate::{CompatProfile, team_info::legacy_atoi};

/// Maximum number of unlock rows consumed by TaystJK cgame.
pub const MAX_COSMETIC_UNLOCKS: usize = 32;
const MAP_BYTES: usize = 40;

/// One source-defined TaystJK cosmetic unlock requirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CosmeticUnlock {
    /// Cosmetic bit unlocked by this row.
    pub bitvalue: u16,
    /// Required style selector.
    pub style: i16,
    /// Required duration in the source table's units.
    pub duration: u32,
    mapname: [u8; MAP_BYTES],
    mapname_len: u8,
    active: bool,
}

impl CosmeticUnlock {
    const EMPTY: Self = Self {
        bitvalue: 0,
        style: 0,
        duration: 0,
        mapname: [0; MAP_BYTES],
        mapname_len: 0,
        active: false,
    };

    /// Map restriction, truncated to TaystJK's 39-byte `Q_strncpyz` payload.
    pub fn map_name(&self) -> &str {
        std::str::from_utf8(&self.mapname[..usize::from(self.mapname_len)]).unwrap_or_default()
    }

    /// Whether the source parser encountered the row's bit-value token.
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

/// Fixed-capacity state replaced whenever a `cosmetics` command arrives.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CosmeticUnlockTable {
    rows: [CosmeticUnlock; MAX_COSMETIC_UNLOCKS],
}

impl Default for CosmeticUnlockTable {
    fn default() -> Self {
        Self {
            rows: [CosmeticUnlock::EMPTY; MAX_COSMETIC_UNLOCKS],
        }
    }
}

impl CosmeticUnlockTable {
    /// Replace this table using TaystJK's `CG_ParseCosmetics` token contract.
    ///
    /// Parsing does not allocate. Like `strtok`, adjacent delimiters are
    /// skipped, and incomplete final rows retain the fields parsed so far.
    pub fn apply_payload(&mut self, payload: &[u8]) {
        self.rows.fill(CosmeticUnlock::EMPTY);
        let mut token_index = 0usize;
        for token in payload
            .split(|byte| matches!(byte, b':' | b'\n' | b'\t'))
            .filter(|token| !token.is_empty())
        {
            let row = token_index / 4;
            if row >= MAX_COSMETIC_UNLOCKS {
                break;
            }
            match token_index % 4 {
                0 => {
                    self.rows[row].bitvalue = legacy_atoi(token) as u16;
                    self.rows[row].active = true;
                }
                1 => {
                    let length = token.len().min(MAP_BYTES - 1);
                    self.rows[row].mapname[..length].copy_from_slice(&token[..length]);
                    self.rows[row].mapname_len = length as u8;
                }
                2 => self.rows[row].style = legacy_atoi(token) as i16,
                3 => self.rows[row].duration = legacy_atoi(token) as u32,
                _ => unreachable!(),
            }
            token_index += 1;
        }
    }

    /// Active rows in source order, including a partially received last row.
    pub fn active(&self) -> impl Iterator<Item = &CosmeticUnlock> {
        self.rows.iter().filter(|row| row.active)
    }

    /// Number of active rows.
    pub fn len(&self) -> usize {
        self.active().count()
    }

    /// Whether the table has no active rows.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Apply one `cosmetics` payload only for the source-backed TaystJK profile.
///
/// Returns whether the command belongs to the active profile. Other profiles
/// leave the table untouched, preventing jaPRO behavior from leaking into
/// BaseJKA or the separately source-gated JA+ adapter.
pub fn apply_taystjk_cosmetics(
    profile: &CompatProfile,
    payload: &[u8],
    table: &mut CosmeticUnlockTable,
) -> bool {
    if !matches!(profile, CompatProfile::TaystJk) {
        return false;
    }
    table.apply_payload(payload);
    true
}
