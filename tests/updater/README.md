# In-client updater check

The standalone harness compiles the production updater, `codemp/client/cl_update.cpp`,
against a stand-in for the engine headers, and checks:
- that release tags order the way `/update` needs, with betas after the stable
  release they are built on and alpha builds never comparable;
- the paths an update may write, and the ones it may not, such as configs,
  folders other than `base/` and `EternalJK/`, and anything outside GameData;
- manifests made by `scripts/update/make_update_package.py`: signed with a listed
  key they pass, while changed bytes, an unlisted key, an alpha tag, a mismatched
  channel or platform, a bad or duplicate path and a newer format are refused;
- the client's zip reader against the script's archive, file by file, against the
  manifest hashes and the original bytes;
- remove lists: the script lists a file a later release drops, and a retired one,
  with the versions shipped, carries them into later releases and drops one that
  is shipped again, and refuses a retail file. The client refuses a list that
  removes a file it also installs, a retail file, a config, a path outside
  GameData, a file with no versions or listed twice;
- which files get removed: only one still as JoF shipped it, never one the player
  changed (even at a shipped size), nor the running program;
- whether a staged update still goes in: never an older release, and the running
  release only to finish a swap that was stopped;
- the start-up swap: files go in with the old ones kept as `.old`, files to remove
  go to `.old`, a resumed swap skips what already went in or out, a file changed
  since it was checked is left alone, and a file held open by another program puts
  everything back. That includes what a stopped swap had moved before a resumed one
  fails. The three held-open cases each wait out the 10-second retry window.

```text
cmake -S tests/updater -B build/updater-check
cmake --build build/updater-check --config Release
ctest --test-dir build/updater-check -C Release --output-on-failure
```

It needs Windows and Python 3. No network is used. The download path and the
restart into the new program are covered by the manual test in
[docs/updater.md](../../docs/updater.md#testing-locally).
