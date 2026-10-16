//! The `mimic` console command and the viewer's hooks: commands in `live_session.rs`,
//! snapshots in `snapshot_presentation.rs`, leaving in `connection.rs`.

use super::*;

/// `mimic <player> [delay] [mirror|copy] [walk]` or `mimic off`, parsed.
#[derive(Debug, PartialEq)]
pub(crate) enum Request {
    Off,
    Status,
    Follow {
        player: String,
        delay: i32,
        mirror: bool,
        walk: bool,
    },
}

/// Parse the command's arguments: the player (a slot or part of a name), then in any
/// order a delay in seconds, `mirror` (the default) or `copy`, and `walk` to walk always.
pub(crate) fn parse(args: &[String]) -> Result<Request, String> {
    let Some((player, rest)) = args.split_first() else {
        return Ok(Request::Status);
    };
    if rest.is_empty() && player.eq_ignore_ascii_case("off") {
        return Ok(Request::Off);
    }
    let mut delay = 0;
    let mut mirror = true;
    let mut walk = false;
    for word in rest {
        if word.eq_ignore_ascii_case("walk") {
            walk = true;
        } else if word.eq_ignore_ascii_case("mirror") {
            mirror = true;
        } else if word.eq_ignore_ascii_case("copy") {
            mirror = false;
        } else if let Ok(seconds) = word.trim_end_matches('s').parse::<f32>() {
            if !(0.0..=MAX_DELAY as f32 / 1000.0).contains(&seconds) {
                return Err(format!(
                    "mimic: the delay goes from 0 to {} seconds",
                    MAX_DELAY / 1000
                ));
            }
            delay = (seconds * 1000.0).round() as i32;
        } else {
            return Err(format!("usage: {HELP}"));
        }
    }
    Ok(Request::Follow {
        player: player.clone(),
        delay,
        mirror,
        walk,
    })
}

impl crate::GpuState {
    /// The `mimic` console command.
    pub(crate) fn mimic_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        match parse(args)? {
            Request::Off => {
                if !self.mimic.active() {
                    return Err("mimic: not copying anyone".into());
                }
                self.mimic.stop_with("^3mimic: stopped.");
                Ok(self.mimic.take_messages())
            }
            Request::Status => Ok(vec![
                HELP.to_owned(),
                match &self.mimic.following {
                    Some(following) => format!(
                        "mimic: copying {}^7 ({}{}, {:.1} s delay)",
                        following.name,
                        if following.mirror { "mirror" } else { "copy" },
                        if following.always_walk {
                            ", walking"
                        } else {
                            ""
                        },
                        following.delay as f32 / 1000.0
                    ),
                    None => "mimic: off".into(),
                },
            ]),
            Request::Follow {
                player,
                delay,
                mirror,
                walk,
            } => {
                let session = self.live_session.as_ref().ok_or("mimic: not in a game")?;
                let snapshot = session.latest_snapshot();
                let own = snapshot.player.client_num();
                let mut roster = sjk_client::ChatRoster::default();
                roster.update(session.game_state());
                let slot = if player.bytes().all(|byte| byte.is_ascii_digit()) {
                    player.parse::<u16>().map_err(|_| "mimic: bad slot")?
                } else {
                    match roster.lookup(&player) {
                        sjk_client::PlayerLookup::Found(slot) => slot,
                        sjk_client::PlayerLookup::NotFound => {
                            return Err(format!("mimic: no player matches \"{player}^7\""));
                        }
                        sjk_client::PlayerLookup::Ambiguous(candidates) => {
                            let mut lines = vec![format!(
                                "mimic: \"{player}^7\" matches several players; use a number:"
                            )];
                            lines.extend(
                                candidates
                                    .into_iter()
                                    .map(|(slot, name)| format!("  {slot:2}: {name}^7")),
                            );
                            return Err(lines.join("\n"));
                        }
                    }
                };
                if slot == own {
                    return Err("mimic: pick another player, not yourself".into());
                }
                let name = roster
                    .target(Some(slot))
                    .and_then(|target| roster.display_name(target))
                    .map(str::to_owned)
                    .ok_or_else(|| format!("mimic: nobody is in slot {slot}"))?;
                if self.ghost.driving() {
                    return Err("mimic: a ghost replay is running; ghost stop first".into());
                }
                if snapshot.player.duel_in_progress() {
                    return Err("mimic: not in a duel".into());
                }
                if snapshot.player.health() <= 0 {
                    return Err("mimic: you are dead".into());
                }
                if self.local_prediction.fake_noclip()
                    || self.local_prediction.committed_state().is_none()
                {
                    return Err("mimic: needs your own predicted view (not flying)".into());
                }
                self.mimic.start(slot, name.clone(), delay, mirror, walk);
                Ok(vec![format!(
                    "^2mimic: copying {name}^2 ({}{}, {:.1} s delay)^7. Face them; hands off, \
                     any key stops it. ^5mimic off^7 ends it.",
                    if mirror { "mirror" } else { "copy" },
                    if walk { ", walking" } else { "" },
                    delay as f32 / 1000.0
                )])
            }
        }
    }

    /// What a live snapshot shows of the leader.
    pub(crate) fn mimic_observe(&mut self, snapshot: &sjk_protocol::Snapshot) {
        let Some(slot) = self.mimic.leader() else {
            return;
        };
        let entity = snapshot
            .entities
            .iter()
            .find(|entity| entity.number() == slot);
        self.mimic
            .observe(snapshot.server_time, entity, &snapshot.player);
    }
}
