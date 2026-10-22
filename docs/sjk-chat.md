# SJK chat

SJK chat is the one conversation of every SJK player, carried by the SJK hub
instead of the game server. The Windows client takes part in it: a JoF player reads
and writes it on any server, or in the menus with no server at all, and talks with
players on SJK's own client. Players on other clients never see it.

## What players see

- The chat field shows its channels, `All Team SJK:`, with the one in use lit.
  **Tab** goes round them, All, Team, SJK, and keeps what is typed. On a whisper,
  Tab goes back to All.
- `messagemode6` opens the field on SJK chat, for a key of its own:
  `/bind i messagemode6`. `messagemode` and `messagemode2` open it on All and Team as
  before, and `messagemode5` still replies to the last whisper (in SJK's client
  `messagemode5` is the SJK channel).
- Enter on SJK sends the text to the hub. `/sjksay <text>` does the same from the
  console, in a match or in the menus.
- **In a match, an SJK line is said in the server's chat too**, once SJK chat took it,
  as All chat without the chat style, so the server's players on other clients read
  it. It starts with SJK in yellow, `Name: SJK text`, so they can tell it is an SJK
  line. A server keeps 149 characters of a line, so a longer one goes as several, cut
  at a space, each starting with SJK, one a second (the server's flood protection
  takes one command a second).
  This happens on every server, whatever its mod. SJK chat players on that same
  server see it once, as the SJK line, whole and marked `ALL/SJK`: the server's copy
  of it is neither shown nor logged for them (Said on a server too, below). SJK's
  client says its players' lines on JoF servers as well. In the menus an SJK line goes
  to SJK chat only. A line SJK chat refused, or said while the hub cannot be reached,
  goes nowhere: the console says it was not sent. Normal chat (All, Team, whispers)
  is never touched.
- A line reads `Name+ SJK: text`, as in SJK's client: the name with its colours, a
  green `+` for a player the SJK team verified, the label, then the text without the
  sender's colour codes. The label is `ALL/SJK` for a line said in the server's chat
  too, which SJK chat players there read once. The chat box and the SJK chat menu draw
  the label and the text in SJK's golds (`#E8B84A` and `#F5C756`); the console, the
  notify lines and the log, which only have the game's colours, in the nearest, `^8`
  and `^3`. A GIPHY link, which SJK's client plays as a GIF, shows as `<GIF>`. In a
  match the line goes into the chat box, and into the console as chat does. In the
  menus it goes to the console. A long message takes several chat-box rows.
  What was said before the player came is not replayed, except in the SJK chat menu.
- **CHAT** in the main menu, on the bar under the ring to the left, opens the SJK chat
  menu. Its list holds the last 100
  lines of SJK chat, from a match or the menus, and SJK chat's own notices. It starts
  with what the hub still held when the game connected. Each line comes after its
  client's icon and is wrapped to the list. Above the list are SJK chat's On/Off
  switch and whether it is connected and how many players read it now. Below it is a
  field to say a line in,
  with Enter or SEND. It takes typing as soon as the menu opens. The first Escape
  leaves the field and keeps what was typed; the second goes back to the main menu.
  The button shows only with an engine that has the menu.
- A message starts with the icon of the client it was sent from, in the chat box, the
  console, the notify lines and the SJK chat menu: SJK's emblem for SJK, JoF's for
  this client, the player's own messages included. A message from any other client,
  or from a hub that does not say which client sent it, has none, nor has any on a
  pure server where the icons do not load. In the chat box the rest of a long message
  lines up after the icon.
- A message the hub refuses comes back as an `SJK chat: not sent: ...` line: a
  message of more than one line or more than 300 characters, sending too often (the
  hub is stricter with new players), a link from a player it has not verified, or a
  muted player. Lines said in a burst go out a second apart.
- `/sjkstatus` shows the hub, which identity is in use and its key id, whether it is
  registered and connected, and how many players read SJK chat now.
- `cl_sjkChat 0` turns SJK chat off: the client then makes no request to the hub at
  all. It is on by default, and switching it takes effect at once. Setup has it too, as
  **SJK Chat** in its **SJK** section, in the main menu's Setup and in the game's, with
  SJK chat's state under it. The SJK chat menu has the same switch.
- `cl_hubUrl` is the hub, `https://sjk.dfox.app` by default. It must be `https://`,
  or `http://127.0.0.1:<port>` for a test hub. A server cannot set it.
- Linux and macOS have no SJK chat yet: the SJK channel there says so, and the main
  menu has no CHAT.

## Identity

There is no account and no password. The hub knows a player by an Ed25519 key, the
file SJK calls `identity.key`, and the client signs every request with it.

- When SJK is installed, the client uses **SJK's key**, so the player is the same
  person in both: `GameData\SJK\identity.key` (beside the client's program), else
  `%APPDATA%\SJK\identity.key`.
- Otherwise it makes **its own** the first time, in the same format and under the
  same name, in `%APPDATA%\EternalJK\identity.key`. To be the same person in SJK
  later, copy that file into `GameData\SJK\` before SJK first starts.
- A damaged key file is never replaced, since that would end the identity: SJK chat
  stops and says which file it is.
- Whoever holds the file can speak as the player. Keep it private and back it up.
  The game's file system refuses to open any file named `identity.key`, so no cgame
  or ui module, including one a server sends, can read a copy that sits in a game
  folder.

The hub registers the key with the name the player wears, and each message carries
that name, so a name change shows on the next message.

### Being a JoF client

Every request carries a User-Agent in the shape SJK's client uses,
`SJK/<version> (<commit>)` (the hub's protocol, "Client"):

| build | User-Agent |
|-------|------------|
| a release, built by CI | `EJK/<release tag> (<short commit>)`, such as `EJK/1.6.2-beta.4 (e3560d5)` |
| built from source | `EJK/<latest release>-dev`, such as `EJK/1.6.1-dev`, or `EJK/dev` |

The hub keeps the agent each key signs with and the SJK team can see it, so a JoF
release can be told from SJK's client and from a changed build: its tag and commit
match JediofFreedom/JoF_EJK's release tags. It is a hint, never proof (any program can
send any agent), and the hub refuses nothing on it. `/sjkstatus` shows the agent.

From the agent of a message's `POST /v1/chat`, the hub writes the client into the
message as the feed carries it: `"client":"sjk"` for `SJK/...`, `"ejk"` for `EJK/...`,
and nothing for any other agent or from a hub before the field (its protocol, "Chat").
That is what picks the icon, here and in SJK's client.

## How it works

The code is [cl_sjk.cpp](../codemp/client/cl_sjk.cpp). It follows version 1 of the
hub's protocol, as SJK's client implements it (Sol-Vulpes/SJK,
`docs/hub-chat.md` and `docs/identity.md`):

- **Signing.** Each request carries
  `Authorization: SJK-Sig key=<public key>, ts=<unix s>, nonce=<12 random bytes>, sig=<signature>`,
  the Ed25519 signature of `SJK-HUB-v1`, the method, the path with its query, the
  time, the nonce and the SHA-256 of the body, one per line, all base64url without
  padding. When the hub answers that the clock is off, the client takes the hub's
  time and tries once more. Ed25519 is Monocypher's (`lib/monocypher`), SHA-256 and
  randomness are Windows' (BCrypt), and HTTP is WinHTTP, with the system's proxy. A
  nonce Windows cannot give after three tries fails the request, so a line says it
  was not sent.
- **Sessions.** Each start of SJK chat (turning it on, a new `cl_hubUrl`) makes a new
  session for its two workers and lets go of the last. The workers and the client
  each hold a share; the last to let go frees it and wipes its copy of the key
  (`SJK_Release`), so a worker still in a long poll never holds up the client.
- **Reading.** A worker thread registers the key (`POST /v1/register`), then
  long-polls `GET /v1/feed`: its first read does not wait and learns where the
  conversation is, and what it holds goes to the SJK chat menu only. Later reads let
  the hub hold them up to 25 seconds. It waits 2
  seconds at least between reads and backs off from 2 to 60 seconds when the hub
  fails. An answer without the feed's `next` and `chat` counts as a failure too, even
  with a 200 (`SJK_FeedAnswer`), so a garbled first answer cannot let the backlog
  through as new lines. A hub that restarted answers with what it holds, and that is
  not replayed either.
- **Sending.** Another worker sends what the player says (`POST /v1/chat`, with the
  text and the worn name), once the key is registered. While the hub cannot be
  reached, a new line is refused at once instead of going out minutes later. Lines
  still waiting when the hub turns out to be unreachable are dropped the same way,
  each with a notice that it was not sent.
- **Said on a server too.** A convention between JoF EternalJK and SJK, which SJK's
  `docs/hub-chat.md` describes too ("Said on the game server too"); the hub takes no
  part in it. In a match, `CL_SJK_SayText`
  sends the line to the hub with the match it was said in (the server's `sv_serverid`
  and map, `CL_SJK_ServerSession` in [cl_parse.cpp](../codemp/client/cl_parse.cpp)).
  When the hub took it, the server's copy goes, while still in that match: in pieces
  (`SJK_ServerPieces`), one a second (`CL_SJK_Frame`, through `CL_SayToServer` in
  [cl_keys.cpp](../codemp/client/cl_keys.cpp)) (`SJK_ServerCopyLines`). Each starts
  with `^3SJK^2 ` (`SJK_SERVER_TAG`: SJK in yellow, then All chat's green again) and
  ends with the id the hub gave the message in colour codes, which no client draws
  (`SJK_ServerMarker`: `^9`, then `^` and the number of digits, then `^` and each
  digit; id 12 is `^9^2^1^2`). A message the hub refused, or that never reached it,
  has no id and no copy. On the reading side, the cgame (`CG_SJKServerLine` in
  [cg_draw.c](../codemp/cgame/cg_draw.c), from `CG_Chat_f`) holds a server chat line
  with the ending, other than a whisper, for up to 2.5 seconds while SJK chat is on.
  When the message of that id comes through the feed, it shows instead, marked
  `ALL/SJK`, and the server's lines of it are dropped (`CG_SJKSawMessage`); one that
  comes up to 15 seconds after it is dropped as well, and turns the message's line
  into `ALL/SJK` in the chat box and the SJK chat menu (`CG_SJKSaidInAll`, telling the
  engine through `SJK_SaidInAll`). As SJK's client does (`server_copy.rs`), a line is
  taken for a message's copy only when its words are among the message's: the
  letters and digits of what follows the tag, lower case, which the server's old code
  pages keep, and a piece of a long message has some of them. A line with none is
  the copy only of a message with none, since an id alone would let anyone hide a
  line. Where the server says who sent the line (the client number after it), the
  sender's name must have the same letters and digits as the message's too. Any other line shows as it came, so a copy may show twice but
  never hides another player's line. A line without the ending is never held.
- **Text.** The game's 8-bit text becomes UTF-8 for the hub, and back for the game,
  with the engine's own conversion: Latin-1, or Cyrillic or central European letters
  as the language picks. What the game cannot draw shows as `?`.
- **GIFs.** `SJK_GifText` turns a GIPHY link into `<GIF>` by SJK's rules
  (`chat_gifs/link.rs`): `https://` or `http://`, the host with no user or port, and
  `giphy.com/gifs/<slug>-<id>` or `/stickers/...`, `giphy.com/embed/<id>` (also
  `www.`), `media[0-4].giphy.com/media/[v1.<token>/]<id>/<file>`,
  `i.giphy.com/<id>.gif` or `.webp`, or `i.giphy.com/media/...`, with any query and
  the punctuation that ends a sentence after it. Other sites' links stay as they are:
  SJK's client shows those only to players who ask for them.
- **Showing.** The main thread hands a message to the cgame in parts
  (`sjkChatLine_t` and `SJK_SetChatSink` in [cg_public.h](../codemp/cgame/cg_public.h),
  `CG_SJKChatLine` in [cg_draw.c](../codemp/cgame/cg_draw.c)): who said it, its text,
  the words of the name and of the text, its client and its id. The cgame logs it and
  prints it to the console as one line in the game's colours (as `SJK_ConsoleLine` in
  the engine), and puts it in the chat box as one item: the text in rows of the box's
  width, the first after the name and room for `ALL/SJK`. `CG_SJKChatBoxPaint` draws
  the name in its colours, then the label and the text in SJK's golds. SJK chat's own
  notices go as a server's line does, in pieces, since `CG_ChatBox_AddString` holds
  only `MAX_SAY_TEXT + MAX_NETNAME`. A cgame without the sink (an older one, or a server's own) leaves
  the lines to the console.
- **Icons.** The engine passes on a message's `client` only when it is `sjk` or
  `ejk`.
  - The cgame draws `gfx/sjk/sjk.png` or `gfx/sjk/ejk.png` (in
    [assets/jofclient](../assets/jofclient/gfx/sjk), so `jofclient-assets.pk3`)
    before the message's item in the chat box, at an emoji's size, and its rows after
    it (`chatBoxItem_t` `icon` and `indented`).
  - For the console, the engine sets the icon for the next print only
    (`Con_SetPrintIcon` in [cl_console.cpp](../codemp/client/cl_console.cpp)). That
    print, the chat box's copy or the engine's own line, then starts with two cells
    of room, the first marked with the icon in bits of the cell the colour leaves
    free. The console and the notify lines draw it there. Only the engine marks a
    cell, so no text a server prints can bring an icon.
  - An icon that did not load is left out: on a pure server that does not list
    `jofclient-assets.pk3`, nothing of that pk3 loads. The server's copy of a line is
    still hidden there, as that goes by the cgame having taken SJK chat from the
    engine (`cgs.sjkChat`), not by its icons.
  - Several chat-box items added in one frame, such as a long notice's pieces, share
    a time. `CG_ChatBox_DrawStrings` sorts by time and takes the items oldest first,
    so they keep the order they came in.
- **Channels.** The SJK channel lives in the engine's chat field
  ([cl_keys.cpp](../codemp/client/cl_keys.cpp), `Message_Key`;
  [cl_console.cpp](../codemp/client/cl_console.cpp), the prompt and `messagemode6`).
- **Menu.** The engine keeps the last 100 lines in their parts, and whether each was
  said in All chat too (`CL_SJK_ChatLineCount`, `CL_SJK_ChatLine` and
  `CL_SJK_SaidInAll`). A new session's backlog (SJK chat turned off and on, a new
  `cl_hubUrl`) adds only the messages it does not hold yet, told by id, sender and
  text together (`SJK_Remembered`). It keeps SJK chat's state in the read-only cvar
  `cl_sjkChatStatus`. The ui reaches the lines and `CL_SJK_SayText` through its import
  table's `ext` ([ui_public.h](../codemp/ui/ui_public.h)) and lists them with
  `FEEDER_SJK_CHAT` (`UI_SJKChatRefresh` in [ui_main.c](../codemp/ui/ui_main.c)), in
  [sjkchat.menu](../assets/japro/ui/jamp/sjkchat.menu). It draws a message's rows
  itself, in SJK's golds, through the list's `feederItemPaint`
  ([ui_shared.h](../codemp/ui/ui_shared.h)), a hook that leaves every other list as
  it was. An older engine's import table
  ends before these, so only an engine that has them sets `cl_sjkChatUI 1`. The ui
  checks that cvar before each call, and the main menu shows the button only with it.
  The two bars under the ring are one picture, `gfx/menus/main_leftbottomwindow`, the
  right one drawn mirrored (a negative width in its rect). They are sized so their
  lines are as wide as the side panels', 4 units under the panels and their curve
  against the ring, the gap the panels keep from it. CHAT is on the left one; the right one is
  kept for the next button.
- **Setup.** The SJK section is the `sjk` group of
  [setup.menu](../assets/japro/ui/jamp/setup.menu) and
  [ingame_setup.menu](../assets/japro/ui/jamp/ingame_setup.menu), at the end of each,
  for every SJK setting to come. Its button shows only with `cl_sjkChatUI`, while SJK
  chat is all it holds. Every other section's button hides the group, as each hides
  the others.

## Testing locally

[tests/sjk-chat](../tests/sjk-chat/README.md) checks the signing against the
protocol's test vector, the key file, the text rules, the charset, hub addresses,
the User-Agent, how a message reads, GIPHY links and the server's copy, without the
engine or a hub.

To talk across clients, run a local SJK hub as SJK's hub repository describes, on
`http://127.0.0.1:8787`, and start each client with
`+set cl_hubUrl "http://127.0.0.1:8787"`. The quotes must reach the game, because
`//` starts a comment: in Git Bash write `'"http://127.0.0.1:8787"'`, in PowerShell
``\`"http://127.0.0.1:8787\`"``. Give each test client its own key, a throwaway
`GameData\SJK\identity.key` in its folder, so neither uses the player's real one,
and point `fs_cdpath` at a real GameData for the retail assets. SJK's client takes
the same `cl_hubUrl` and keeps its key in its own `GameData\SJK`.

The hub allows few registrations an hour from one address, and every start of a
client registers, so many restarts in a row get `too many requests` until the hub
is restarted.

To try a line said on a server too, host a match in one client (`+devmap mp/ffa3`),
join it from a second (`+connect 127.0.0.1:29070`) and from a third with
`+set cl_sjkChat 0`, and keep a fourth in the menus. An SJK line from either player
in the match shows once, as an `ALL/SJK` line, in the first two and the fourth, and as
the server's chat in the third. A line said in All chat within 15 seconds that ends
with that message's id (`/say text^9^1^5` for id 5) shows as it came when its words
are not the message's, when it has none, or when another player says it. A line the
hub refuses, or said with `cl_hubUrl` set to a port nothing listens on, says it was
not sent and reaches no one on the server.
