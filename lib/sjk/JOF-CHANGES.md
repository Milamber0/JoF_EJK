# JoF EJK's copy of SJK

This folder is SJK (Sol JK, https://github.com/Sol-Vulpes/SJK), GPL-2.0, taken
at upstream commit `d655587d` ("Merge personal/emote-sync-audio") and modified for
JoF EJK. JoF EJK builds its Freedom renderer (`rd-freedom_x86_64.dll`,
`codemp/rd-freedom`) from it: the engine keeps its game, server, model and Ghoul2
code, and SJK's renderer draws the world, the models and the 2D.

The Freedom renderer is a modified version of SJK and is not an official SJK
release. As SJK's [NOTICE](NOTICE) asks, it carries a name of its own and no SJK
emblem; the copyright notices, the [LICENSE](LICENSE) and [CREDITS.md](CREDITS.md)
are kept as they are. Only the files below differ from upstream, and only the
`site`, `docs` and `.github` folders, and the `scripts` but `build_version.rs`
(which the build scripts include), were left out of this copy.

## Changes (October 2026, JoF EJK)

- `crates/sjk-viewer/src/host/` (new): the hosted renderer.
  - `ffi.rs`: the C interface `codemp/rd-freedom/sjk_host.h` declares. Every call
    runs on one renderer thread, which is joined at shutdown so `vid_restart` can
    unload the DLL.
  - `session.rs`: SJK's GPU state on the engine's window (a raw-handle surface),
    with a settings profile of its own. A new profile takes its `r_*` settings from
    `GameData/SJK/config.cfg` when there is one. It also exports the settings and
    runs the `graphicsquality` command.
  - `entities.rs`: the engine's refEntities become SJK draws.
    - Ghoul2 models are skinned with the engine's bones, and rigid GLMs are
      placed by their bone. Brush models, MD3s and custom-shader shells are drawn.
    - Sabers use SJK's blades, lights and trails. Sprites, lines and quads become
      particles. Engine polys, decals and dynamic lights are drawn too.
  - `layer.rs`: the engine's 2D (menus, HUD, console, fonts) in a 640x480 virtual
    screen, with shader stages, `animMap`, `videoMap` (RoQ), `tcMod` and colour
    generators.
  - `queries.rs`: `R_MarkFragments`, `R_LightForPoint` and `R_inPVS` from the
    BSP SJK loaded.
- `main.rs`, `frame_overlays.rs`, `gpu_context.rs`, `platform.rs`, `log.rs`: build
  as a library, take the engine's view and 2D, start without a window of their own,
  and forward the log.
- `post_exposure.rs`, `scene_view_render.rs`: eye adaptation and the sky portal are
  on while hosted, as in a session.
- `decal_marks.rs`, `decal_store.rs`, `dismember.rs`, `gpu_skinning.rs`,
  `player_assets.rs`, `settings.rs`, `console.rs`: small hooks the host uses (mark
  clipping into caller buffers, engine polys, surface overrides, palette size, a
  model from a GLM and skin, the renderer settings rows, writing the profile).
- `crash_report.rs`, `session_log.rs`: off while hosted; the engine keeps its own
  crash reports. SJK's crash reporting is never sent from the Freedom renderer.
- `Cargo.toml`: the viewer also builds as the `sjk_host` library.
