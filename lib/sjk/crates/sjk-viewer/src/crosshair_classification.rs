//! TaystJK cg_draw.c:7948-8180, independent of name retention and HUD pixel layout.
use sjk_protocol::{EntityState, GameState, Snapshot};

const WHITE: [f32; 4] = [1.0; 4];
const FRIEND: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const ENEMY: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const NEUTRAL: [f32; 4] = [1.0, 1.0, 0.0, 1.0];

fn info(game: &GameState, client: u16, key: &str) -> i32 {
    game.config_string(1131 + usize::from(client))
        .and_then(|bytes| sjk_client::LegacyClientInfo::new(bytes).integer(key))
        .unwrap_or(0)
}

/// Classify the entity hit by the existing trace, without looking for a second target.
pub(crate) fn classify(e: &EntityState, snap: &Snapshot, game: &GameState) -> Option<[f32; 4]> {
    let gametype = game
        .config_string(0)
        .and_then(|bytes| sjk_client::LegacyClientInfo::new(bytes).integer("g_gametype"))
        .unwrap_or(0);
    let local = snap.player.client_num();
    let team = i32::from(snap.player.team());
    // teamowner is netfield 21 (codemp/qcommon/msg.cpp entityStateFields).
    let owner_team = e.raw_field(21).unwrap_or(0) as i32;
    let mover = e.entity_type() == 6;
    let npc = e.entity_type() == 13;
    if !(e.number() < 32
        || npc
        || e.should_target()
        || e.health() != 0
        || (mover && (owner_team != 0 || (e.bolt1() && snap.player.weapon() == 3))))
    {
        return None;
    }
    if e.powerups() & (1 << 11) != 0 {
        return Some(WHITE);
    }
    let allied = |their| if their { FRIEND } else { ENEMY };
    let color = if e.number() < 32 {
        if (snap.player.duel_in_progress() && e.number() != snap.player.duel_index())
            || (!snap.player.duel_in_progress() && e.bolt1())
        {
            [0.4, 0.4, 0.4, 1.0]
        } else {
            allied(
                (gametype >= 6 && info(game, e.number(), "t") == team)
                    || (gametype == 4 && info(game, e.number(), "ds") == info(game, local, "ds")),
            )
        }
    } else if npc {
        if owner_team == 0 {
            if e.owner() < 32 {
                allied(gametype >= 6 && info(game, e.owner(), "t") == team)
            } else {
                NEUTRAL
            }
        } else {
            allied(owner_team == if gametype == 7 { team } else { 2 })
        }
    } else if e.should_target() {
        if matches!(owner_team, 1 | 2) {
            if gametype < 6 {
                NEUTRAL
            } else {
                allied(owner_team == team)
            }
        } else if e.owner() == local || (gametype >= 6 && owner_team == team) {
            FRIEND
        } else if owner_team == 16 || (gametype >= 6 && owner_team != 0 && owner_team != team) {
            ENEMY
        } else {
            [1.0, 0.8, 0.3, 1.0]
        }
    } else if mover && e.bolt1() && snap.player.weapon() == 3 {
        [0.2, 0.5, 1.0, 1.0]
    } else if owner_team == 0 || gametype < 6 {
        NEUTRAL
    } else {
        allied(owner_team == team)
    };
    Some(color)
}
