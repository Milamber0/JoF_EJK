# Dialogue sound checks

```text
cmake -S tests/dialogue-sound -B build/dialogue-sound-check
cmake --build build/dialogue-sound-check --config Release
ctest --test-dir build/dialogue-sound-check -C Release --output-on-failure
```

The harness runs the production dialogue parser, server session handling and
client command handling with the real tokenizer and mocked filesystem, transport
and audio imports. It covers case-insensitive `Sound`, optional and malformed
paths, per-player delivery, stale/duplicate commands, silent nodes, transitions,
completion, cancellation, session replacement, reset and failed sound registration.
It also runs the engine's attenuation function with a distant listener, checking
that dialogue voice-over remains audible independently of the player's position.

For a live check, package a voiced `.dlg` and its audio in a loaded PK3, then run
`/dialoguetest <name>` with cheats enabled. Verify playback and interruption on
node changes/close, and that a second player does not hear the dialogue. The
automated harness cannot verify audibility or sound decoding.
