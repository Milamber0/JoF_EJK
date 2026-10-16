# Illuminate's holocron

The Illuminate toy (SJK's own, not a Force power, [client.md](../../../../docs/client.md#illuminate))
shows a holocron floating by the player's shoulder. Its pictures were generated
by Sol (07/10/2026): a holocron icon in the style of Jedi Academy's Force icons
(1024x1024, on a flat grey ground) and one face of the holocron (2048x2048). The
files here are made from those two by [holocron_assets.py](../../../../scripts/holocron_assets.py),
so none is edited by hand except `holocron.shader`.

| File | Game path | What |
| --- | --- | --- |
| `toy_illuminate.png` | `gfx/sjk/toy_illuminate.png` | The toy's icon (quick wheel, Collection, key bindings), 128x128, like `gfx/mp/f_icon_*` |
| `holocron.jpg` | `models/sjk/holocron.jpg` | The face, 512x512 |
| `holocron_glow.jpg` | `models/sjk/holocron_glow.jpg` | The face's emblem alone, for the additive stage |
| `holocron.md3` | `models/sjk/holocron.md3` | A 6-unit cube with the face on every side |
| `holocron.shader` | `shaders/sjk_holocron.shader` | `models/sjk/holocron`: lit face, glowing emblem |

The client mounts them in memory below all game data
([illuminate.rs](../../src/illuminate.rs)), so a PK3 that has the same paths
replaces them.

## Loot-box tiers

The holocron drops come in four tiers, each a variant of the one holocron: the
same cube (`holocron.md3`, no new geometry), the steel plate kept steel, and the
emblem, the ring's runes and bevels and a faint plate tint in the tier's colour.
Colour is not the only cue: the tiers climb in how much they glow (the share of the
ring that is lit, the emblem's light on the plate, the emblem's brightness and its
breathing), so they also tell apart by lightness and by how much is lit, for viewers
who cannot rely on hue.

| Id | Colour | Emblem and ring | Extra |
| --- | --- | --- | --- |
| `uncommon` | green | emblem a little dim, ring barely lit | |
| `rare` | blue | emblem bright, ring faintly lit | |
| `legendary` | purple | emblem full, ring lit | |
| `mythical` | gold, shiny | emblem full, ring fully lit, widest halo, double rim and a glint on the icon | the face breathes too, a sheen stripe slides across it |

Per tier `<tier>` (one of the ids above), all mounted in memory like the files above:

| File | Game path | What |
| --- | --- | --- |
| `holocron_<tier>.md3` | `models/sjk/holocron_<tier>.md3` | The cube with the surface shader `models/sjk/holocron_<tier>` |
| `holocron_<tier>.jpg` | `models/sjk/holocron_<tier>.jpg` | The face in the tier's colour, 512x512 |
| `holocron_<tier>_glow.jpg` | `models/sjk/holocron_<tier>_glow.jpg` | Its emblem and lit ring alone, for the additive and glow stage |
| `holocron_<tier>.png` | `gfx/sjk/holocron_<tier>.png` | UI icon, 256x256 with alpha: the face in a chamfered octagon with a rim and halo in the tier's colour, for a dark UI |
| `holocron_<tier>_small.png` | `gfx/sjk/holocron_<tier>_small.png` | The same, 64x64 |
| `holocron_tiers.shader` | `shaders/sjk_holocron_tiers.shader` | `models/sjk/holocron_<tier>` for the four tiers: the base shader's stages with the tier's pictures and breathing |
| `holocron_mythical_sheen.jpg` | `models/sjk/holocron_mythical_sheen.jpg` | A soft diagonal stripe (128x128, tiles along s + t) that the mythical shader scrolls over the face (`tcMod scroll`) |

The model's own shader is still `models/sjk/holocron` (the base, Illuminate's). A surface's
shader is named by its model file, so the shared cube cannot draw a tier: each tier has a
model of its own, `holocron_<tier>.md3` (the same cube, its surface shader
`models/sjk/holocron_<tier>`), which the Profile screen's Holocrons tab draws.
The shaders use only stock Quake 3 / Jedi Academy keywords (`rgbGen lightingDiffuse`,
`rgbGen wave`, `blendFunc GL_ONE GL_ONE`, `tcMod scale` and `scroll`, `glow`); the
test in [illuminate.rs](../../src/illuminate.rs) parses them with the client's shader
parser and checks every picture they name is mounted. The pictures come out of
`holocron.jpg`, so nothing of the originals is needed to remake the tiers.

### The locked look

A tier the player holds none of is drawn dimmed, not hidden, by the one shared locked look:

| File | Game path | What |
| --- | --- | --- |
| `holocron_locked.jpg` | `models/sjk/holocron_locked.jpg` | The face with its colour drained and darkened, 512x512 |
| `holocron_locked.md3` | `models/sjk/holocron_locked.md3` | The cube with the surface shader `models/sjk/holocron_locked` |
| `holocron_locked.shader` | `shaders/sjk_holocron_locked.shader` | A lit face and a faint light of its own, no glow |

### Light colours

The point light of each tier (the one the 3D menu uses), in the scale
of `COLOR` in `illuminate.rs` (`[1.5, 1.3, 1.0]`, Illuminate's warm white): linear RGB
floats, not clamped. They are in [holocron_assets.py](../../../../scripts/holocron_assets.py)
as `Tier.light` too; the tiers rise in strength (the sum of the three) as they rise in
tier.

| Id | R | G | B | Sum |
| --- | --- | --- | --- | --- |
| `uncommon` | 0.55 | 1.50 | 0.60 | 2.65 |
| `rare` | 0.50 | 1.00 | 1.75 | 3.25 |
| `legendary` | 1.25 | 0.45 | 1.75 | 3.45 |
| `mythical` | 1.85 | 1.30 | 0.40 | 3.55 |

```rust
const TIER_LIGHTS: [(&str, [f32; 3]); 4] = [
    ("uncommon", [0.55, 1.50, 0.60]),
    ("rare", [0.50, 1.00, 1.75]),
    ("legendary", [1.25, 0.45, 1.75]),
    ("mythical", [1.85, 1.30, 0.40]),
];
```

## Regenerating

With Python 3 and Pillow, numpy and scipy (`pip install pillow numpy scipy`),
from the repository root. Everything, from Sol's two originals (the tiers follow
from the new face):

```sh
python scripts/holocron_assets.py icon.jpg face.jpg crates/sjk-viewer/assets/holocron
```

Only the tiers (pictures, icons, sheen and shader), from the `holocron.jpg` in the
folder; the output is deterministic, and it is the `TIERS` table in the script that
sets the colours, the glow ladder, the shader's waves and the lights:

```sh
python scripts/holocron_assets.py tiers crates/sjk-viewer/assets/holocron
```
