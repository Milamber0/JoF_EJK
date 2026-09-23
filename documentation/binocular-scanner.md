# Binocular scanner

Binoculars (`zoomMode == 2`) display health and shield labels beside visible living
players and NPCs. Amber-yellow corner and target brackets, translucent dark panels, red health
readouts and segments, and green shield readouts and segments complement the existing binocular optics.
Players use their names; NPCs are labeled `NPC`, or `Vehicle` for vehicle-class
entities. Internal NPC types and scripted names may not describe the visible
model, so they are not displayed.
Player names retain the full `MAX_NETNAME` buffer. Short names grow to fill the
title height, and long names shrink to fit the rendered width. Names remain
vertically centered in the title area without overlapping the health row.
Shield means `STAT_ARMOR`.
Bars use the target's maximum health as their normal capacity, matching JA's HUD;
vehicle shield bars use the vehicle definition's shield capacity when available.
Numeric values retain any health or armor above that capacity.

Build and install both this repository's `cgame` and `jampgame` modules. No new art
assets or engine protocol changes are required. Other server mods must implement
the command below before this client can display their scan data.

## Display style and size

`cg_binocularScanStyle 0` selects the original detailed label (default).
`cg_binocularScanStyle 1` selects a compact health / shield readout using the
same digit artwork as `cg_hudFiles 0`: red health, an amber slash, and green
shield. Small amber brackets and a dark backing keep it readable through the
binocular optics. It has no name, captions, or meters. Both styles use the same
server data and visibility checks; changing style applies immediately and saves.

`cg_binocularScanScale` controls the entire scan label in percent, including text,
bars, brackets, panel, and spacing. Default: `100`. For example, use
`cg_binocularScanScale 75` for 75% size or `cg_binocularScanScale 150` for 150%.
Changes apply immediately and are saved. Values are clamped to 25-200% when drawn.
The binocular mask and world target positions are unaffected.

## Optional server command

The client advertises `binoScan=1` in userinfo. Only send telemetry to clients
advertising that capability, while they are alive, playing, and using binoculars.
Unmodified servers supply no telemetry; unmodified clients receive no commands.

```text
binoStats <serverTime> <count> [<entityNum> <health> <maxHealth> <armor>]...
```

All fields are nonnegative decimal integers; health and maximum health must be
positive. Each command replaces the previous set, with at most 32 targets.
The server includes only complete records that fit the engine's 1024-byte command
limit, prioritizing the contacts closest to the reticle if necessary.
Example: `binoStats 12000 2 3 85 100 25 64 250 300 0`.

The server sends at most five updates per second, selects contacts closest to the
view direction within 8192 game units, and checks PVS, line of sight, entity
visibility flags, cloaking, and mind tricks. It sends a zero-count clear when
binocular use ends. No positions or guessed health values are transmitted.

### Optional NPC names

The protocol extension below remains supported, but the current binocular HUD
deliberately displays `NPC` or `Vehicle` instead of these names.

Clients supporting NPC names also advertise `binoNames=1`. After `binoStats`,
send one or more commands with the same server timestamp:

```text
binoNames <serverTime> <entityNum> "<name>" [<entityNum> "<name>"]...
```

Names are limited to 63 bytes. Remove color codes and replace quotes, backslashes,
semicolons, and control characters with spaces before quoting them. Split name
records across commands to stay below 1024 bytes including the terminator.
The client only accepts names for contacts in the matching stats update; names
neither create contacts nor refresh their expiry. Each stats update clears names.
Servers implementing only `binoStats` remain supported. Do not send `binoNames`
to clients that have not advertised its capability.

The client anchors labels to interpolated entities, checks line of sight again
each frame, clips to the central binocular viewing area, suppresses overlapping
panels, and expires telemetry after 600 ms. Labels are disabled for spectator
views, death, intermission, normal view, and the disruptor scope.

## Verification

Build both modules:

```text
cmake --build build --config Release --target cgamex86 jampgamex86 --parallel 6
```

In-game checks still required:

- Scan a player and several NPC types; damage them and change their armor. Check
  numeric values, empty shields, low health, and values above normal capacity.
- Move targets behind solid cover, outside the viewing area, or beyond range;
  cloak or mind-trick the observer. Labels must disappear.
- Lower binoculars, switch to the disruptor, die, spectate, and restart the map;
  old contacts must not remain visible.
- Scan a crowd at 4:3 and widescreen resolutions; check mask boundaries,
  readability, and panel overlap.
- Connect an older client to the updated server and the updated client to an
  older server; neither should receive unknown-command spam or fabricated stats.
