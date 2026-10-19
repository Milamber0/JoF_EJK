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
- the start-up swap: files go in with the old ones kept as `.old`, a resumed swap
  skips what already went in, and a file held open by another program puts
  everything back. That case waits out the 10-second retry window.

```text
cmake -S tests/updater -B build/updater-check
cmake --build build/updater-check --config Release
ctest --test-dir build/updater-check -C Release --output-on-failure
```

It needs Windows and Python 3. No network is used. The download path and the
restart into the new program are covered by the manual test in
[docs/updater.md](../../docs/updater.md#testing-locally).
