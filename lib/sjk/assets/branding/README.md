# SJK branding

The SJK emblem, a gold starburst with the JK blade, a glowing orange core and
cyan blade lights, was provided by Sol. Every image here and in `site/assets` is
derived from Sol's original (2048x2048, on black) by two scripts, so none of
them is edited by hand.

| File | Size | Used by |
| --- | --- | --- |
| `sjk-logo.png` | 1024, transparent | Master; the client's menu emblem ([emblem.rs](../../crates/sjk-viewer/src/menu/emblem.rs)) |
| `sjk-logo-512.png` | 512, transparent | README and release notes |
| `emblem-core.png` | 512, additive | The menu emblem's pulsing orange core and ring |
| `emblem-lights.png` | 512, additive | The menu emblem's shimmering cyan blade lights |
| `icon-32.png`, `icon-64.png` | 32, 64 | The client's window icons ([window_icon.rs](../../crates/sjk-viewer/src/window_icon.rs)) |
| `jof-emblem.png` | 128, white on transparent | The JoF clan's emblem beside tagged names, drawn tinted teal ([jof_tag.rs](../../crates/sjk-viewer/src/jof_tag.rs)); rendered from the clan's SVG path by `python scripts/jof_emblem.py` (not SJK's emblem) |
| `jofa-emblem.png` | 128, full colour | JoF Academy's emblem beside JoFA-tagged names ([jof_tag.rs](../../crates/sjk-viewer/src/jof_tag.rs)); the academy's HD logo (1295x1214, supplied by the clan, not kept here) shrunk by `python scripts/clan_emblem.py <logo.png> <out.png>` (not SJK's emblem) |
| `sjk.ico` | 16 to 256 | Windows icon of `sjk.exe` and `sjk-server.exe` (the crates' `build.rs`) |
| `site/assets/sjk-logo-512.png` | 512 | The site's hero |
| `site/assets/favicon.ico`, `favicon-32.png` | 16 to 48 | The site's favicon and navigation mark |
| `site/assets/apple-touch-icon.png` | 180 | Home-screen icon |
| `site/assets/social-preview.jpg` | 1200x630 | Open Graph and Twitter card |

## Regenerating

With Python 3 and Pillow, numpy and scipy (`pip install pillow numpy scipy`),
from the repository root:

```sh
python scripts/logo_alpha.py SJK_Logo.jpg sjk_logo.png
python scripts/sjk_branding.py sjk_logo.png .
```

[logo_alpha.py](../../scripts/logo_alpha.py) cuts the emblem out of its black
background: only black connected to the border becomes transparent, so the dark
blade stays opaque, and edge colour is un-multiplied from black so no dark fringe
remains. [sjk_branding.py](../../scripts/sjk_branding.py) writes everything in
the table. Neither the 2048 original nor the cut-out is kept in the repository.

## Choices

- **Glow layers.** The core layer is the bright, saturated orange inside the dark
  ring; the lights layer is the cyan on the blade. Each is the masked colour plus
  a soft blur of it, scaled so its brightest channel is full, on black. The
  client adds them as light over the emblem with a strength that moves: the core
  breathes between 0.08 and 0.45 every 4.2 s, the lights shimmer at 0.30 ± 0.15
  from two quick sines (0.9 s and 0.37 s). See
  [motion.rs](../../crates/sjk-viewer/src/menu/art/motion.rs).
- **Small icons.** At 40 pixels and under (the title bar, taskbar, small Explorer
  views and the favicon) the icons show the medallion: the dark ring with its
  core and the blade, cut out of the emblem. The whole starburst blurs into a
  brown blot at those sizes; from 48 pixels up the icons show the whole emblem.
  Icons up to 32 pixels get a light unsharp mask after the Lanczos reduction.
- **Formats.** PNG keeps the transparency; the social card is a JPEG because it
  is opaque and photographic. Every `.ico` frame is PNG-compressed (Windows Vista
  and later).
