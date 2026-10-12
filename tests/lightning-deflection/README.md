# Lightning deflection checks

These checks compile the production server guard and damage code, unchanged
saber animation/prediction code, and client effect recognition against real
game headers and math functions. Engine imports are mocked,
so the checks do not need a running game or assets.

```powershell
cmake -S tests/lightning-deflection -B build/lightning-deflection-check
cmake --build build/lightning-deflection-check --config Release
ctest --test-dir build/lightning-deflection-check -C Release --output-on-failure
```

Coverage includes Saber Defense 3 without learned Lightning or Force points,
ordinary prediction retaining the server-selected pose during idle/walk,
including a delayed snapshot with no transmitted Saber Defense level,
standing and walking, the 100-degree aiming cone, a stable BOTH_BF1LOCK pose across aim and position changes, immediate
attack/run/jump cancellation, damage and electrification, guard expiry, multiple
casters, every non-idle saber move and combat recovery preserving their animation timers,
existing Absorb behavior, continued damage throughout knockdown/get-up animations
after the hand state clears, preservation of a saber parry using the same pose,
and effect recognition from the caster's and defender's viewpoints.

For an in-game check, run a server with the new game module. Test both an older
client and the new JoF cgame module. With an ignited saber and Defense 3, stand or walk while aiming
at a Lightning user. The normal lightning beam must remain visually unchanged,
while the defender holds the `BOTH_BF1LOCK` saber-block animation (red uses `BOTH_BF1LOCK`; blue and yellow use `BOTH_LK_S_DL_T_SB_1_L`; dual sabers with both active use `BOTH_LK_DL_S_T_L_1`; staff uses `BOTH_LK_ST_ST_T_L_1`).
The updated cgame freezes the torso on the last playback frame for red, yellow, blue and duals, and the first playback frame for staff. Legs retain normal walking animation; attacks and guard cancellation restore normal torso playback. Older clients still play these animations normally.
Switching duals to one saber uses the blue pose; switching staff to one blade uses the yellow pose.
Blue-white flares and traced electrical arcs should originate along the blade;
they must not redirect or replace the beam. Running, attacking, throwing the saber,
jumping, holstering, or turning beyond 50 degrees should release the guard. Try
the pose from both client viewpoints while turning within the blocking cone and moving either player. Aim still controls the blocking cone throughout. Only the updated cgame and asset pack supply the additional blade effects/sounds.
Repeat with latency: swing input should immediately replace the pose through
normal prediction; run/jump/turn cancellation is confirmed by the server.

The player must slow down to walking speed to enter the guard. A player who is
hit while running or swinging can still acquire the guard after settling down,
even while the beam continues. Deflection has no Force cost and does not damage
the caster.
Knockdown and get-up animations cannot acquire or retain the guard, even after
the hand state clears. With Absorb off and no other damage protection, keep
Lightning hitting during a knockdown and recovery: damage must continue and no
block pose should interrupt recovery. Blocking can resume after recovery ends.

The server owns all guard eligibility, damage prevention, pose selection and
cancellation. Its caster, pose and expiry tracking stay in `gclient_t`, outside
the network protocol. It sends the normal torso animation with a one-second
animation timer so unacknowledged client commands can retain the pose. The
server releases that timer after 150 ms without a guarded hit, or immediately
when a command becomes ineligible, before and after movement processing.
Blue/yellow deflection is exempt from lock-break movement and weapon restrictions only while the saber is idle, with no active lock, lock frame, hand extension, weapon cooldown, or leg recovery animation. Real saber-lock breaks retain their restrictions.
No custom hand extension is involved;
normal saber attacks override the held torso animation, and walking keeps its
own leg animation. Client animation rendering uses its standard path.

Cgame recognizes that animation while a nearby player is actively casting
Lightning, then plays namespaced MBII lightning arc and flare assets at the
defender's blade. No custom entity event or borrowed effect flag is involved;
the normal Lightning beam remains on its original rendering path. Gameplay and
the stock poses use the server module alone; the updated cgame and assets add
the existing MBII presentation. The automatic checks exercise the normal
animation rules, but a live older-client check is still needed to establish
compatibility and responsiveness under its renderer and network prediction.

Install the separately distributed `jof-lightning-deflection-assets.pk3` in
`GameData/EternalJK`. It supplies the `effects/mp/lightning_deflect_mb2` and
`effects/mp/lightning_reference` effects, their namespaced textures and shaders,
and original asset credits. These assets are maintained outside this repository;
repository builds do not include them in `jofclient-assets.pk3`. The separate pack
contains no DLLs and includes isolated copies of the original materials and sounds.
Avoid packaging the same paths in another PK3. Use `cg_lightningEnvironment 2`
to enable the MBII lightning rendering mode.

The saber presentation follows `CG_DoSaberShockEffects` at `0x872a0` and its
five calls in `CG_AddSaberBlade` in the supplied MBII `cgamei386.so` (SHA-256
`1521db9b7bae7d358019c0b72ae240305c801386a0473e0d9506129a32a91fde`).
It uses five per-blade endpoint caches, the original trace and frame gates,
and spark sounds supplied by the impact effect.

`lightning_mb2_visuals` compares production callbacks and cache state against
353 events recorded from the original x86 caller block and shock routine.
The recording uses deterministic engine/RNG/angle callbacks. It covers moving
blades, misses, frame timing, pause, timer equality/expiry, time reversal and
independent saber/blade caches, allowing small host floating-point differences.
The asset pack preserves original effect/material tokens except resource paths
and includes byte-identical textures and six spark samples. A side-by-side
in-game check remains necessary to confirm the result under JoF's renderer.
