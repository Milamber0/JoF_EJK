<p align="center"><img src="assets/branding/sjk-logo-512.png" alt="Sol JK emblem" width="240"></p>

# Sol JK (SJK)

**Sol JK**, or **SJK** for short, is Sol's client for **Star Wars Jedi
Knight: Jedi Academy** multiplayer: the game as Sol sees it, developed by Sol in
Rust.

SJK aims to be a true JKA+: everything players expect from today's community
clients, from the retail look and feel to mod servers such as JA+ and modern
screens, in a client that stays clean and fast, without the bloat some clients
have accumulated. Once the client is solid, a JoF edition of SJK will follow.

SJK includes a graphical client and a dedicated server, with support for the
legacy protocol, PK3 content, maps, models, movement and combat. The renderer
uses wgpu. The client includes a server browser, console, configurable controls,
HUD, audio, screenshots and demo playback.

## Highlights

- the SJK UI, SJK's own menus over the live map, the default;
- a classic menu style after the retail menus: main, profile, setup, controls
  and server browser pages, retail connect and loading screens, the animated
  logo and glows with the SJK emblem in the ring, and no map behind the menu;
- a classic scoreboard with client IDs, and HUDs drawn from the game's own menu
  files, so the retail HUD and custom HUD packs work;
- JA+ support: the client identifies as the JA+ plugin, predicts JA+ movement,
  saber rules, `g_debugMelee`, the grapple and duel pass-through;
- client mods loaded on the servers they are for: JA+ tools (`japlus.*` admin teleports,
  auto login and plugin options) and JoF tools (`jof.*`);
- vector versions of the retail game fonts on every retail text surface, tighter
  console and chat rows, and text that scales at high resolutions;
- a renderer settings page, sharp levelshots, and MOUSE1, MOUSE2 and ESC locked
  in the controls editor;
- more reliable joining of public servers (lost handshake packets and lost
  gamestates are recovered) and support for older 72-bone player models;
- an FPS cap that holds its exact rate and can follow the monitor's refresh
  rate;
- optional rend2-style material maps for world surfaces, with a local generator;
- gameplay and presentation fixes (third-person camera, Force Speed afterimages,
  saber trails, death animations, key names for non-US keyboard layouts and
  more);
- a personal `debug_panel` console command: an in-game checklist of the changes
  in this build and how to test them.

Sol (Sol-Vulpes) develops SJK and decides where it goes. Contributors send pull
requests, Creyon first among them; see [CREDITS.md](CREDITS.md).

The programs are `sjk` (the game) and `sjk-server` (the dedicated server).
Settings have neutral engine names (`r_*`, `cg_*`), the source crates are
`sjk-*`, and the client's own folder is `GameData/SJK/`.

## Download

Ready-to-run Windows x64 and Linux x64 builds are on the
[releases page](https://github.com/Sol-Vulpes/SJK/releases) and the
[SJK website](https://sol-vulpes.github.io/SJK/). Extract the ZIP for your
platform into Jedi Academy's `GameData` folder, beside `base`, and launch
`sjk` (`sjk.exe` on Windows).

Releases are built by the [SJK release workflow](.github/workflows/release.yml)
when a tag named `sjk-v<version>` (for example `sjk-v0.2.0`) is pushed. It builds
the same drop-in ZIPs as the [GameData packages workflow](docs/packages.md),
smoke check included, names them `SJK-<version>-<platform>.zip`, and publishes
them with their checksums and the matching source snapshot.

## Build

Install Rust 1.88 or newer, Cargo, a C/C++ compiler, CMake and pkg-config.
On Linux, development packages for ALSA, Wayland and XKB are required, along
with working Vulkan or OpenGL graphics drivers.

```sh
cargo build --release -p sjk-viewer -p sjk-dedicated
```

This builds `target/release/sjk` and `target/release/sjk-server` (`.exe` on
Windows).

SJK is developed and tested mainly on Windows 11. It keeps its Linux support,
but changes are not routinely tested there.

## Game data

A legally obtained Jedi Academy installation is required. Put `sjk`
(`sjk.exe` on Windows) in its `GameData` directory, beside the `base`
folder containing `assets0.pk3` through `assets3.pk3`.
Game data is not included in this repository.

## Client

Launch the client from that folder or a shortcut to open the main menu. No
game-data path, environment variable or particular working directory is needed.
Put `sjk-server` (`sjk-server.exe` on Windows) beside it too for Create game
and local `devmap` support.

Connect directly to a server:

```sh
./sjk --connect 127.0.0.1:29070
```

If keeping the binary elsewhere, use `JKA_GAME_DATA=/path/to/GameData` or the
saved game-data setting; known installation locations are also checked. The
explicit positional form `sjk /path/to/GameData --connect HOST:PORT`
remains supported. See [client launch](docs/client.md#launch) for discovery order.
Use the in-game menus for controls, graphics, audio and player settings;
Settings > Interface > Menu style, or First setup's first row, switches
between the SJK UI and the classic menus.

Settings and player-created files live in `GameData/SJK/`: `config.cfg`,
`marks.txt`, favorites, friends, screenshots, demos and optional chat logs.
If that directory cannot be written, the client uses its per-user folder
instead. The console's `path` command shows the
active location. See [configuration and content](docs/client.md#configuration-and-content).

## Dedicated server

```sh
./target/release/sjk-server \
  --game-data /path/to/GameData \
  --map mp/ffa3 \
  --bind 0.0.0.0:29070 \
  --hostname "SJK server"
```

Server configuration uses familiar cvars and commands, including `+set` launch
arguments. For example, append `+set g_gametype 0 +set fraglimit 20` for a
free-for-all match. UDP port 29070 must be reachable for remote players to join.

## Source layout

All source is under `crates/`:

- `sjk-viewer`: graphical client and platform integration (builds `sjk`).
- `sjk-dedicated`: dedicated server and game integration (builds `sjk-server`).
- `sjk-game-jka`: shared Jedi Academy game rules and movement.
- `sjk-client`, `sjk-network`, `sjk-protocol`: client state and legacy networking.
- `sjk-bsp`, `sjk-scene`, `sjk-runtime`: maps, scene data and world state.
- `sjk-materialgen`: offline generator of local material maps from installed
  textures (see [rendering](docs/rendering.md#generating-material-maps)).
- Remaining crates provide formats, content loading, collision, navigation,
  scripting, audio, UI and the console.

Legacy formats and game-specific behavior stay in compatibility modules;
engine services use their own data structures.

## Documentation

The [project wiki](docs/README.md) covers architecture, development, the client, the dedicated server, rendering
and networking. Start with [current status and priorities](docs/status.md) for
verified behavior and open work. [AGENTS.md](AGENTS.md) contains shared
contributor and AI guidance, including updating relevant documentation alongside
code changes; [SJK conventions](docs/sjk.md) adds the rules specific to SJK.

## License and credits

GPL-2.0-only; see [LICENSE](LICENSE). SJK is by Sol (Sol-Vulpes) and
contributors; [CREDITS.md](CREDITS.md) lists who made what. Any SJK binaries are
built from the source published here, which is their corresponding source.

[NOTICE](NOTICE) holds the copyright notice. The license requires every copy and
modified version to keep it, and, under section 2(c), to keep the copyright and
no-warranty announcement the client and server print when they start. The
"Sol JK" and "SJK" names and the SJK emblem are not licensed with the code: a
modified version uses its own name and emblem and credits SJK as its origin.

OpenJK and TaystJK are compatibility references for Jedi Academy behavior. The
bundled Inter fonts retain their
[license](crates/sjk-viewer/assets/fonts/LICENSE.txt). Other dependencies retain
their respective licenses. The code license does not grant rights to retail
game assets or third-party PK3 content.

SJK began in October 2026 as a modified version of
[JKR](https://github.com/Bishop-R/JKR), an engine by Bishop (Bishop-R) and its
contributors. As section 2(a) of the license asks, the changes to those files,
with their authors and dates, are recorded in this repository's git history.

Star Wars, Jedi Knight and Jedi Academy are trademarks of their respective
owners. SJK is a fan project and is not affiliated with or endorsed by Lucasfilm,
Disney, Raven Software or Activision.
