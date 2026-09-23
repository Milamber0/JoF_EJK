# Confirmed capacity-limited pickups

This branch replaces the unconditional prediction suppression from #184 with
an opt-in client/server path. It requires **both** updated modules. This repo's
server is JAPro-derived; a separately maintained JA+ server must port the change.

## Protocol v3 (one-time readiness)

- JoF JA+ server advertises `V=2.5B0` and `g_pickupConfirm=3` in serverinfo. Other servers retain original client pickup behavior even if they advertise the capability, including this repository's JAPro-derived server.
- Client advertises `cg_pickupConfirm=1` in userinfo (ROM), but that flag alone
  does not authorize custom commands. The server advertises `g_pickupConfirm=3`.
- On a supported JoF JA+ server (`V=2.5B0`), the loaded client module sends
  `jof_pickupReady 3` once active, with no periodic renewal or retry. The server
  stores readiness for the connection; ClientConnect clears it. Respawns and
  team changes preserve readiness. On map restart the client resets negotiation
  and makes one new attempt. The loaded v3 module sets `cg_pickupReady=3`.
  Module shutdown sets `cg_pickupReady=0` through
  engine-handled userinfo, which revokes readiness even when game commands are
  flood-filtered. Legacy modules do not set this new lifecycle flag, even if
  they re-register the old `cg_pickupConfirm=1` capability. Module initialization
  restores the lifecycle flag, but flags alone never enable server delivery.
  Userinfo revocation clears server readiness.
- Send `jof_pickup` only while this connection's readiness is valid, the client
  capability is exactly 1, loaded-module flag is exactly 3, and the server
  protocol version is 3. Never broadcast it.
  Older clients cannot opt in through a leftover userinfo cvar. Older servers
  advertising version 1 or 2 receive no handshake from this client and retain normal
  pickup feedback. The server acknowledges a valid request with
  `jof_pickupReady 3`, only to that requesting client. Until acknowledged, the
  client keeps ordinary feedback. A flood-filtered request leaves the client on
  ordinary feedback for that module/map; it does not start a retry loop.
  No acknowledgement or probe is sent to clients that did not request it.
- After an accepted health, armor, ammo or holdable pickup, the server sends
  the collector `jof_pickup <item-modelindex>` on the reliable command channel.
  Rejected touches send nothing. Item indices use the existing shared item table.
- The client disables speculative pickups for those types only on a supporting
  server, and suppresses its duplicate ordinary pickup events. The reliable
  command invokes sound/console feedback once, without the entity-based 500ms
  suppression. Command sequence handling already executes each reliable command
  once; two separate commands for the same item type are two real pickups.
- Ordinary events are retained for other players and legacy clients. Legacy
  servers retain the original client prediction behavior. Weapons, flags and
  powerups keep their existing handling.
- HUD notifications have a 16-entry pending queue, advancing no faster than
  every 750ms. Overflow drops the oldest pending icon, not console/sound feedback.
  The last icon retains the normal three-second fade. Map restart clears the queue.

Do not deploy only the client and expect legacy JA+ pickup prediction to change.
The JA+ server port must update its protocol, readiness lifetime, connection
reset and userinfo revocation together; changing only the advertised version is
not sufficient. Version 2's one-second heartbeat could starve scoreboard/login
commands under `sv_floodProtect=1`; v3 must never renew readiness periodically.
Do not combine this with the separate `fix/confirmed-pickup-feedback` snapshot
workaround: remove that workaround when integrating into a branch containing it.

## Automated checks

```text
cmake -S tests/pickup-confirmation -B build/pickup-confirmation-check
cmake --build build/pickup-confirmation-check --config Release
ctest --test-dir build/pickup-confirmation-check -C Release --output-on-failure
```

The harness compiles extracted production server confirmation, capability,
command parsing and HUD queue functions. Engine transport and the final
sound/console callback are mocked. It checks shield + medpack, distinct repeated
pickups, malformed commands, version mismatch, spectator filtering, and queue
bounds. It also checks that stale userinfo flags cannot enable delivery, readiness
is per client, userinfo revocation requires a fresh handshake, and only a valid
acknowledgement enables the client's confirmation path. Ten minutes of frame
updates produce only one request, with and without acknowledgement. The actual
engine flood-filter block reproduces v2 dropping score/login and verifies that
v3 leaves subsequent command slots available. It does **not** simulate a live network
or prove audibility/rendering.

## Required live checks

1. Run the updated JoF JA+ server (`V=2.5B0`) and client; verify server capability
   `g_pickupConfirm=3`, client `cg_pickupConfirm=1`, and the single readiness exchange.
2. At low health/armor, collect a shield and medpack together: verify both stats,
   two console lines and feedback sounds, and the two icons displayed in sequence.
3. Near capacity, touch several shields or medpacks: feedback must match only
   accepted pickups. Test two real pickups of the same type within 500ms too.
4. Repeat under latency/loss, across death, reconnect and map restart.
5. Test updated client on old server and old client on updated server; neither
   should get unknown commands or lose its normal pickup feedback.
6. Verify weapon autoswitch, flags, dropped items and spectator behavior.
7. With `sv_floodProtect=1`, verify scoreboard and JA+ login after joining, then
   across death/team changes, map restart, reconnect and client-module restart.
   A switch to a legacy module must revoke readiness without recurring traffic.

Reliable confirmation avoids reliance on the two-entry snapshot event ring,
but introduces confirmation latency and shares the engine's finite reliable
command buffer. Pathological pickup spam can overflow that buffer, as with
other reliable server messages. This is not a wire-protocol expansion.
