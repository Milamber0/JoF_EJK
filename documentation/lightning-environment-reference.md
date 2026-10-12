# Environmental lightning and optional effect packs

`cg_lightningEnvironment` defaults to `2`, enabling environmental arcs.
`0` selects vanilla lightning alone and `1` selects environmental nests.
An existing saved cvar value remains the user's choice until changed or reset.

## Appearance

Mode 2 uses these optional effect files when an external asset pack supplies them:

- `effects/mp/lightning_reference/lightning.efx`: narrow main spray.
- `effects/mp/lightning_reference/lightningwide.efx`: wide main spray.
- `effects/mp/lightning_reference/lightning_arc.efx`: environmental bolt,
  including any impact effect declared by the EFX.

Each missing or unregistered handle falls back independently. Main sprays use
the existing vanilla `forceLightning` / `forceLightningWide` effects. Environmental
bolts use JoF's `effects/mp/lightning_branch`, with `effects/mp/lightning_environment_impact`
at valid traced surfaces. Fallback impacts skip sky, no-impact, nodraw and
solid-start contacts. Pack arcs own their impacts and do not receive an additional
fallback impact. The main spray plays once per invocation with the hand axes.

Mode 2 uses three dedicated environmental arc samples:
`sound/weapons/force/lightningenv1.mp3` through `lightningenv3.mp3`.
These files ship in `assets/jofclient` and leave player-hit audio unchanged.

Impact effects independently select stock `sound/ambience/spark1.wav` through
`spark6.wav`. The fallback impact supplies this spark layer; pack arcs own their
impact effect and receive no extra fallback impact. No stock spark files are
bundled. Mode 1 retains its existing six-spark audio and timing.

## Forward angle

`cg_lightningEnvironmentAngle` is archived and defaults to `360`, preserving
unrestricted arc directions. Set it to `160` to allow environmental bolt
directions to deviate up to 80 degrees from the player's current aim.
Directions outside that cone are redirected straight forward before tracing,
emitting and positioning sounds, preserving the randomized vector length.
This also applies to cached directions after the player turns.

The cvar takes the total cone width in degrees, clamped to 0-360. `0` points all
environmental bolts forward; `360` restores unrestricted arc directions.
Changes apply immediately in mode 2. This controls environmental bolt directions;
main-spray width and the jagged shape of individual bolts remain EFX settings.

## Direction and timing

Mode 2 emits two narrow environment arcs with component spread 0.5 or five wide
environment arcs with spread 0.8, in addition to the main hand spray.

- Fresh directions add three independent `rand()` offsets without normalizing,
  then apply the forward-angle limit and trace 350 times that vector, skipping
  entity `-1` with `MASK_SOLID`.
- Hits cache the requested endpoint, not the surface contact, for a random
  500-1500 ms. Misses leave the cache and timers alone.
- While cached, `origin - cachedEndpoint` is passed to `AngleVectors` twice,
  the second time with roll zero. The forward-angle limit is then applied and
  the direction is traced 200 units. This conversion produces
  changing directions between endpoint updates.
- Arcs emit when frame time is positive and either at least 50 ms or
  `cg.time % 50 <= cg.frametime`. Main lightning still plays on each invocation.
- Each arc has an independent 500-750 ms sound timer. Sound plays at the
  requested trace endpoint on the owner's `CHAN_AUTO`, with one of three dedicated
  environmental samples.
- Timer comparisons are strict `<`, including the original time-reversal and
  zero-time behavior. Mode 1's budgets, nests and extra hand flash remain specific
  to mode 1.

## Validation

`tests/lightning-reference` compares the production direction and timing logic
at angle `360` against a recorded event fixture, with deterministic random,
trace and math callbacks. It covers hits/misses, both widths, movement,
cached directions, timer boundaries, frame pacing, pause and time reversal.
It checks all eight combinations of optional effect handles, unchanged hand axes,
fallback surface-impact placement/filtering, and absence of duplicate impacts.
Another 3,853 cases verify the angle limit, backward-bolt redirection, cvar bounds,
fresh/cached directions, player turns, and trace/effect/sound consistency.
The fixture requires no external binary or Python packages to run. Sound
selection, timing, placement, channel and random-call order are checked directly.
Registration checks cover three arc handles and six stock spark handles.

Rendering uses the installed engine and asset pack; the automated checks cover
the code's callback behavior rather than the visual appearance of those assets.
