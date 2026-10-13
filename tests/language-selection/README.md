# Language selection regression

The standalone harness compiles the production language feeder, menu parser,
selection lookup, and click handler. It reproduces the in-game directory list
with case variants (`french` and `French`, etc.) and verifies that clicking
forward from German reaches English and completes a full cycle. It also checks
backward cycling, empty and oversized language lists, case-insensitive English
selection, and both actual Voice menu definitions.

```text
cmake -S tests/language-selection -B build/language-selection
cmake --build build/language-selection --config Release
ctest --test-dir build/language-selection -C Release --output-on-failure
```

Engine file access, cvars, and language discovery are mocked. This test covers
selector behavior; it does not render fonts or operate a live game window.
