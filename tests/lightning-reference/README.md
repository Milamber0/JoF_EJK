# Environmental lightning regression

The tests compile `FX_ForceLightningReference` directly from `fx_force.c` and
compare its direction and timing events against a recorded 14-frame fixture.
They verify trace endpoints, cached directions, arc emission, sound calls,
random-call order and timer state at `cg_lightningEnvironmentAngle 360`.

Eight runs cover every combination of optional narrow, wide and arc effects.
They check hand axes, fallback surface placement and filtering, and the absence
of duplicate fallback impacts when a pack arc owns its impact effect.
Float comparisons allow 0.001 units for platform rounding differences.

Environmental arcs select three dedicated sound samples with sound IDs, timing,
positions, channels and random-call order checked directly against the fixture.
Impact effects supply an independent six-way stock spark layer. Registration
checks cover all three arc paths and six stock spark paths.

`lightning_forward_arc_check` exercises 3,853 cases of the same production helper:
fresh and cached directions, left/right angle boundaries, backwards directions,
player yaw/pitch, narrow/wide lightning, hits/misses and cvar bounds. It checks
that traces and emitted bolts stay within the configured cone, redirects preserve
trace reach, and the sound positions and timers follow the accepted traces.

```powershell
cmake -S tests/lightning-reference -B build/lightning-reference-tests -A x64
cmake --build build/lightning-reference-tests --config Release
ctest --test-dir build/lightning-reference-tests -C Release --output-on-failure
```
