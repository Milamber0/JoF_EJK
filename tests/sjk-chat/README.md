# SJK chat check

The standalone harness compiles the production SJK chat code,
`codemp/client/cl_sjk.cpp`, against a stand-in for the engine headers, and checks:
- the signature against the hub protocol's test vector (the same one SJK's client
  checks): the public key, the key id and the whole `Authorization` header;
- SJK's key file: it reads with Windows line ends, spaces and blank lines, and a
  wrong first line, a short or malformed key, extra lines and an empty file are refused;
- base64url both ways, unpadded;
- the text the hub takes: runs of spaces made one, one line, 1 to 300 characters
  counted as characters, not bytes;
- the game's charset to UTF-8 and back, with the engine's own conversion (cut out of
  `shared/sdl/sdl_input.cpp`): Latin-1, Cyrillic in Russian, central European
  letters, `?` for what the game cannot draw, control characters dropped;
- hub addresses: `https://` only, or `http://` on this machine, with no path;
- the User-Agent: a release's tag and commit, `-dev` for a build from source, no
  commit that is not one;
- how a feed message reads in the game: its colours left out, the verified mark, and
  the client it came from, only `sjk` or `ejk`, for its icon;
- a line said on a game server too: the id the hub gave a message, from the feed and
  from its answer; the ending of the server's copy (the id in colour codes, 1 to 9
  digits, none otherwise); the chat body, only the text and the name; and the pieces
  the server's chat gets, each with its ending within what a server keeps of a say as
  quoted (`%` takes 3), cut at spaces and never inside a colour code. The cgame's side,
  holding and dropping the server's copy, is the manual test in
  [docs/sjk-chat.md](../../docs/sjk-chat.md#testing-locally).

```text
cmake -S tests/sjk-chat -B build/sjk-chat-check
cmake --build build/sjk-chat-check --config Release
ctest --test-dir build/sjk-chat-check -C Release --output-on-failure
```

It needs Windows. No network is used. Talking across clients through a local hub is
the manual test in [docs/sjk-chat.md](../../docs/sjk-chat.md#testing-locally).
