//! The frame glue of world pings (`world_ping.rs`): the `+ping` press traces the
//! crosshair's ray to the world and sends the point through the SJK hub; each frame the
//! pings the hub relayed for this server are checked against what the game shows and
//! join the ones on show; and the HUD pass lays them out over the world.

use super::*;
use world_ping::{Held, Scope, WorldPings};

/// `CONTENTS_SOLID`: what a ping lands on (not clips, which nobody sees).
const PING_MASK: u32 = 1;
/// How long a refusal the chat said stays said, unless it changes.
const REFUSAL_QUIET: Duration = Duration::from_secs(30);

impl GpuState {
    /// `+ping` / `worldping`: mark the point under the crosshair for the players who may
    /// see it, and show it at once.
    pub(crate) fn world_ping_press(&mut self) {
        let now = Instant::now();
        let centre = |gpu: &mut Self, text: &str| {
            gpu.chat.receive(
                sjk_client::ServerEventKind::CenterPrint,
                text.to_owned(),
                None,
                now,
            );
        };
        if self.live_session.is_none() {
            centre(self, "Join a server to ping");
            return;
        }
        let Some(camera) = self.frame_view else {
            return;
        };
        let forward = (camera.target - camera.eye).normalize_or_zero();
        let trace = self.bsp.trace_box_with(
            &mut self.trace_scratch,
            camera.eye.to_array(),
            (camera.eye + forward * world_ping::REACH).to_array(),
            sjk_bsp::Aabb::new([0.0; 3], [0.0; 3]).expect("point bounds are valid"),
            PING_MASK,
        );
        if forward == Vec3::ZERO
            || trace.fraction >= 1.0
            || trace.start_solid
            || trace.surface_flags & weather::cover::SURF_SKY != 0
        {
            centre(self, "Nothing to ping there");
            return;
        }
        if let Err(held) = self.world_pings.may_send(now) {
            if held == Held::TooMany {
                centre(self, "Too many pings: wait a moment");
            }
            return;
        }
        let point = Vec3::from_array(trace.end_position);
        let Some(session) = self.live_session.as_ref() else {
            return;
        };
        let game = session.game_state();
        let own = usize::try_from(game.client_num)
            .ok()
            .filter(|slot| *slot < 32);
        let name = own
            .and_then(|slot| player_identity::shown_name(game, slot))
            .unwrap_or_default();
        let scope = own.and_then(|slot| world_ping::scope_in(game, slot));
        let local = session.is_local();
        let slot = own.and_then(|slot| u8::try_from(slot).ok()).unwrap_or(0);
        self.world_pings.place(point, slot, true, &name, now);
        self.play_world_ping_sound(now);
        // A local game has no hub claim; a player on no team has no one to tell.
        if let Some(scope) = scope.filter(|_| !local) {
            let pos = point.round().as_ivec3().to_array();
            player_identity::ping(pos, scope.name());
        }
    }

    /// Take the pings the hub relayed for this server: each shows when the game shows
    /// its claimed name in its slot, that slot's team is the ping's scope, the player
    /// sees that scope, the sender is not muted here and not over their allowance.
    pub(crate) fn update_world_pings(&mut self) {
        let now = Instant::now();
        let server = self
            .live_session
            .as_ref()
            .filter(|session| !session.is_local())
            .map(ClientSession::server);
        if server != self.world_pings.server {
            self.world_pings.server = server;
            self.world_pings.clear();
        }
        let received = player_identity::take_pings(server);
        self.say_ping_refusal(now);
        let Some(session) = self.live_session.as_ref() else {
            return;
        };
        if received.is_empty() {
            return;
        }
        let shown = self
            .console
            .as_ref()
            .is_none_or(|console| console.bool_cvar(world_ping::SHOW_CVAR) != Some(false));
        let game = session.game_state();
        let own = usize::try_from(game.client_num)
            .ok()
            .filter(|slot| *slot < 32);
        let Some(reader) = own.and_then(|slot| world_ping::scope_in(game, slot)) else {
            return;
        };
        let mut placed = false;
        for ping in received {
            let slot = usize::from(ping.slot);
            if !shown || Some(slot) == own || self.muted_players.contains(u16::from(ping.slot)) {
                continue;
            }
            let Some(name) = player_identity::shown_name(game, slot) else {
                continue;
            };
            if !sjk_identity::names_match(&ping.claimed_name, &name) {
                continue;
            }
            let Some(scope) = Scope::parse(&ping.scope) else {
                continue;
            };
            if world_ping::scope_in(game, slot) != Some(scope) || !reader.sees(scope) {
                continue;
            }
            if !self.world_pings.admit(ping.slot, now) {
                continue;
            }
            let point = Vec3::from_array(ping.pos.map(|axis| axis as f32));
            self.world_pings.place(point, ping.slot, false, &name, now);
            placed = true;
        }
        if placed {
            self.play_world_ping_sound(now);
        }
    }

    /// Say in the chat why the hub refused the player's last ping, once per outcome and
    /// once in [`REFUSAL_QUIET`] for the same words.
    fn say_ping_refusal(&mut self, now: Instant) {
        let Some(outcome) = player_identity::ping_outcome() else {
            return;
        };
        if outcome.serial == self.world_pings.outcome_seen {
            return;
        }
        self.world_pings.outcome_seen = outcome.serial;
        if outcome.sent {
            return;
        }
        let quiet = self
            .world_pings
            .refusal_said
            .as_ref()
            .is_some_and(|(said, at)| {
                *said == outcome.message && now.saturating_duration_since(*at) < REFUSAL_QUIET
            });
        if quiet {
            return;
        }
        self.chat.receive(
            sjk_client::ServerEventKind::Chat,
            format!("^3SJK ping: {}", outcome.message),
            None,
            now,
        );
        self.world_pings.refusal_said = Some((outcome.message, now));
    }

    fn play_world_ping_sound(&mut self, now: Instant) {
        let on = self
            .console
            .as_ref()
            .is_none_or(|console| console.bool_cvar(world_ping::SOUND_CVAR) != Some(false));
        if on && self.world_pings.sound_due(now) {
            audio::ui_cues::post(audio::ui_cues::Cue::WorldPing);
        }
    }

    /// Lay the pings out over this frame's view and add their words to the HUD's text;
    /// nothing while `shown` is false (menus, console, scoreboard, intermission).
    pub(crate) fn append_world_pings(&mut self, viewport: [f32; 2], shown: bool) {
        let shown = shown || forced(&self.world_pings);
        self.world_pings
            .build(self.frame_view, Instant::now(), shown);
        if self.world_pings.list.commands().is_empty() {
            return;
        }
        let pings = &self.world_pings;
        ui_renderer::append_text_commands(
            &pings.list,
            |id| pings.text(id),
            &mut self.text_vertices,
            &self.ui_font,
            viewport,
            text::TextStyle::NEUTRAL,
        );
    }
}

/// Whether a world shot asked for the pings without a game.
#[cfg(test)]
fn forced(pings: &WorldPings) -> bool {
    pings.forced
}

#[cfg(not(test))]
fn forced(_: &WorldPings) -> bool {
    false
}
