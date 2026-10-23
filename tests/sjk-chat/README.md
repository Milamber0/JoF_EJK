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
- how a feed message reads in the game: in its parts, its colours left out, the
  verified mark, the label (`SJK`, or `ALL/SJK`) as the console shows it, and the
  client it came from, only `sjk` or `ejk`, for its icon;
- GIPHY links as SJK reads them, each shown as `<GIF>`, and links that are not;
- the words that tell a message's copy on a server: letters and digits, lower case,
  without colour codes or other scripts;
- a feed read's answer: only an object with a numeric `next` and a `chat` list moves
  the reading on, anything else counts as a failed read;
- the SJK chat menu's history: a new session's backlog adds a message only when the
  history lacks it, told by its id, sender and text together;
- a line said on a game server too: the id the hub gave a message, from the feed and
  from its answer; the ending of the server's copy (the id in colour codes, 1 to 9
  digits, none otherwise); the chat body, only the text and the name; the pieces the
  server's chat gets, each with its ending within what a server keeps of a say as
  quoted (`%` takes 3), cut at spaces and never inside a colour code; and no copy at
  all without the hub's id. The cgame's side, holding and dropping the server's copy,
  is the manual test in [docs/sjk-chat.md](../../docs/sjk-chat.md#testing-locally).

```text
cmake -S tests/sjk-chat -B build/sjk-chat-check
cmake --build build/sjk-chat-check --config Release
ctest --test-dir build/sjk-chat-check -C Release --output-on-failure
```

It needs Windows. No network is used. Talking across clients through a local hub is
the manual test in [docs/sjk-chat.md](../../docs/sjk-chat.md#testing-locally).
