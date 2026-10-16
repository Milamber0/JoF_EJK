//! Players borrowed by the roster's vehicle code: a rider, or a vehicle's whole crew — its
//! pilot and its passengers (`m_pPilot`, `m_ppPassengers`). While the roster works on
//! them, each one's states are set aside in a scratch of its own ([`RiderScratch`]) — the
//! roster's host borrows the players — swapped in and out and put back after. The
//! scratches are kept for the next time, never allocated per command.
//!
//! Afterwards every player aboard keeps its seat in `ps.generic1`: a pilot's leaving moves
//! the first passenger to the controls and the rest up (`g_vehicles.c:715-745`).

use super::host::ServerHost;
use super::*;
use sjk_game_jka::vehicle_rider::{Rider, RiderBody};

/// `ps.generic1`: a passenger's seat, plus one.
const PS_GENERIC1: usize = 85;

/// A rider's states while the roster's vehicle code has it.
pub(crate) struct RiderScratch {
    client: usize,
    state: PlayerState,
    movement: Predictor,
    entity: EntityState,
    body: RiderBody,
    command: UserCommand,
    health: i32,
    connected: bool,
}

impl Default for RiderScratch {
    fn default() -> Self {
        let state = PlayerState::zero();
        Self {
            client: 0,
            movement: Predictor::from_player_state(&state, authoritative()),
            state,
            entity: EntityState::zero(0, &sjk_protocol::LEGACY_ENTITY_FIELDS),
            body: RiderBody::default(),
            command: UserCommand::default(),
            health: 0,
            connected: false,
        }
    }
}

impl RiderScratch {
    /// The rider these states make, borrowed from the scratch.
    fn rider(&mut self) -> Rider<'_> {
        let maxs = self.movement.box_bounds().1;
        let lengths = self.movement.shared_animation_lengths();
        Rider {
            number: self.client as u16,
            movement: self.movement.state_mut(),
            player: &mut self.state,
            entity: &mut self.entity,
            body: &mut self.body,
            command: self.command,
            health: self.health,
            connected: self.connected,
            lengths,
            maxs,
            clip_mask: sjk_game_jka::pmove::PLAYER_CONTENT_MASK,
        }
    }
}

impl NativeGame {
    /// The players aboard vehicle `vehicle`: its pilot first where it is one of them, else
    /// `lead` (the player whose think this is), then the rest in client order.
    pub(crate) fn crew_of(&self, vehicle: u16, lead: usize) -> Vec<usize> {
        let pilot = self
            .npcs
            .roster
            .actors
            .iter()
            .find(|npc| npc.number == vehicle)
            .and_then(|npc| npc.vehicle.as_deref()?.pilot)
            .map(usize::from);
        let aboard = |client: usize| {
            self.peer(client)
                .is_some_and(|peer| peer.state.vehicle_entity_num() == vehicle)
        };
        let first = pilot.filter(|&pilot| aboard(pilot)).unwrap_or(lead);
        let mut crew = vec![first];
        crew.extend(
            (0..self.players.holders().count()).filter(|&client| client != first && aboard(client)),
        );
        crew
    }

    /// Runs `run` on the roster, its host, and player `client` as a rider — with the rest
    /// of its vehicle's crew set aside with it, whose seats its leaving may move.
    pub(crate) fn with_rider<R>(
        &mut self,
        client: usize,
        run: impl FnOnce(&mut NpcRoster, &mut Rider<'_>, &mut ServerHost<'_>) -> R,
    ) -> Option<R> {
        let vehicle = self
            .peer(client)
            .map_or(0, |peer| peer.state.vehicle_entity_num());
        let mut crew = if vehicle == 0 {
            vec![client]
        } else {
            self.crew_of(vehicle, client)
        };
        if let Some(at) = crew.iter().position(|&rider| rider == client) {
            crew.swap(0, at);
        }
        self.with_riders(&crew, |roster, riders, host| {
            run(roster, &mut riders[0], host)
        })
    }

    /// Runs `run` on the roster, its host, and players `clients` as riders (the first
    /// always among them, or nothing runs): their states set aside for the while and put
    /// back after, their movements written to their wire states, every seat kept; then
    /// what the roster's run did beyond the roster ([`host::NpcOutcome`]).
    pub(crate) fn with_riders<R>(
        &mut self,
        clients: &[usize],
        run: impl FnOnce(&mut NpcRoster, &mut [Rider<'_>], &mut ServerHost<'_>) -> R,
    ) -> Option<R> {
        let &first = clients.first()?;
        // Everyone but the first rider and the vehicle it owns, which its traces pass.
        self.gather_obstacles(first);
        let mut taken = std::mem::take(&mut self.npcs.rider_scratch);
        let mut count = 0;
        for &client in clients {
            let Some(peer) = self.peer_mut(client) else {
                continue;
            };
            if taken.len() <= count {
                taken.push(RiderScratch::default());
            }
            let slot = &mut taken[count];
            std::mem::swap(&mut peer.state, &mut slot.state);
            std::mem::swap(&mut peer.movement, &mut slot.movement);
            std::mem::swap(peer.entity.state_mut(), &mut slot.entity);
            (
                slot.client,
                slot.body,
                slot.command,
                slot.health,
                slot.connected,
            ) = (
                client,
                peer.riding,
                peer.last_command,
                peer.health,
                peer.begun && peer.playing(),
            );
            count += 1;
        }
        let map = self.map.take();
        let result = match &map {
            Some(map) if count != 0 && taken[0].client == first => {
                let mut roster = std::mem::take(&mut self.npcs.roster);
                let mut host = self.npc_host(Some(map), None);
                let mut riders: Vec<Rider<'_>> =
                    taken[..count].iter_mut().map(RiderScratch::rider).collect();
                let result = run(&mut roster, &mut riders, &mut host);
                let commands: Vec<UserCommand> = riders.iter().map(|rider| rider.command).collect();
                drop(riders);
                let outcomes = std::mem::take(&mut host.outcomes);
                drop(host);
                self.npcs.roster = roster;
                for (slot, command) in taken.iter_mut().zip(commands) {
                    slot.command = command;
                }
                Some((result, outcomes))
            }
            _ => None,
        };
        self.map = map;
        for slot in &mut taken[..count] {
            slot.movement.state().write_player_state(&mut slot.state);
            self.put_back(slot);
        }
        self.npcs.rider_scratch = taken;
        let (result, outcomes) = result?;
        self.apply_npc_outcomes(outcomes);
        Some(result)
    }

    /// A rider's states back in its peer, its seat kept as its vehicle has it now.
    fn put_back(&mut self, slot: &mut RiderScratch) {
        let vehicle = slot.state.vehicle_entity_num();
        let seat = (vehicle != 0)
            .then(|| self.npcs.roster.seat_of(vehicle, slot.client as u16))
            .flatten();
        let Some(peer) = self.peer_mut(slot.client) else {
            return;
        };
        std::mem::swap(&mut peer.state, &mut slot.state);
        std::mem::swap(&mut peer.movement, &mut slot.movement);
        std::mem::swap(peer.entity.state_mut(), &mut slot.entity);
        peer.riding = slot.body;
        peer.last_command = slot.command;
        if let Some(seat) = seat {
            peer.state.set_raw_field(PS_GENERIC1, seat);
        }
    }
}
