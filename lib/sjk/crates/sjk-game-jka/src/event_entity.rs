//! The game's freestanding events: an entity that exists for one event — `G_TempEntity`
//! (`g_utils.c:1058-1081`) as `ClientConnect`, `WP_InitForcePowers` and `ClientSpawn` use it
//! to tell every client that somebody joined, what the server's Force rules are, and where
//! a player just appeared. Held against the `temp` lines of the whole-game transcript.

use sjk_protocol::{EntityState, LEGACY_ENTITY_FIELDS};

/// `ET_EVENTS`: an entity whose type is this plus its event.
const ET_EVENTS: u32 = 18;
const EV_CLIENTJOIN: u32 = 1;
const EV_PLAYER_TELEPORT_IN: u32 = 64;
const EV_PLAYER_TELEPORT_OUT: u32 = 65;
const EV_SET_FREE_SABER: u32 = 106;
const EV_SET_FORCE_DISABLE: u32 = 107;
const ES_POS_BASE: [usize; 3] = [2, 1, 4];
const ES_TYPE: usize = 8;
const ES_CLIENT_NUM: usize = 32;
const ES_EVENT_PARM: usize = 42;

/// One event entity as the game makes it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EventEntity {
    /// The event.
    pub event: u32,
    /// Its parameter.
    pub parameter: u32,
    /// Where it happens; `G_TempEntity` snaps it to whole units.
    pub origin: [f32; 3],
    /// Whose it is, for the ones a client draws on a player.
    pub client: Option<u16>,
    /// `SVF_BROADCAST`: sent to everyone, wherever they are.
    pub broadcast: bool,
    /// Up to twelve more wire fields some callers set (`index`, value); index zero means none.
    pub extra: [(usize, u32); 12],
}

impl EventEntity {
    /// `ClientConnect` (`g_client.c:2577-2579`): everyone learns that `client` joined.
    pub fn client_join(client: u16) -> Self {
        Self {
            event: EV_CLIENTJOIN,
            parameter: u32::from(client),
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        }
    }

    /// `WP_InitForcePowers` (`w_force.c:334-355`): the server's Force rules, told to
    /// everyone at every begin — whether the sabers are free, whether the Force is off.
    pub fn force_rules(saber_only: bool, force_disabled: bool) -> [Self; 2] {
        let rule = |event, set: bool| Self {
            event,
            parameter: u32::from(set),
            origin: [0.0; 3],
            client: None,
            broadcast: true,
            extra: [(0, 0); 12],
        };
        [
            rule(EV_SET_FREE_SABER, saber_only),
            rule(EV_SET_FORCE_DISABLE, force_disabled),
        ]
    }

    /// `ClientSpawn` (`g_client.c:3784-3785`): the flash where a player appears.
    pub fn teleport_in(origin: [f32; 3], client: u16) -> Self {
        Self {
            event: EV_PLAYER_TELEPORT_IN,
            parameter: 0,
            origin,
            client: Some(client),
            broadcast: false,
            extra: [(0, 0); 12],
        }
    }

    /// `SetTeam` (`g_cmds.c:909-913`): the flash where a player leaves its team's world.
    pub fn teleport_out(origin: [f32; 3], client: u16) -> Self {
        Self {
            event: EV_PLAYER_TELEPORT_OUT,
            parameter: 0,
            origin,
            client: Some(client),
            broadcast: false,
            extra: [(0, 0); 12],
        }
    }

    /// The wire state, unnumbered: `G_TempEntity` sets the type and the snapped origin
    /// (`G_SetOrigin`: stationary), the callers the parameter and the client.
    pub fn state(&self) -> EntityState {
        let mut state = EntityState::zero(0, &LEGACY_ENTITY_FIELDS);
        state.set_raw_field(ES_TYPE, ET_EVENTS + self.event);
        for (index, value) in ES_POS_BASE.into_iter().zip(self.origin) {
            // The game module's own `SnapVector`: an `(int)` cast.
            state.set_raw_field(index, (value as i32 as f32).to_bits());
        }
        state.set_raw_field(ES_EVENT_PARM, self.parameter);
        if let Some(client) = self.client {
            state.set_raw_field(ES_CLIENT_NUM, u32::from(client));
        }
        for (index, value) in self.extra {
            if index != 0 {
                state.set_raw_field(index, value);
            }
        }
        state
    }
}
