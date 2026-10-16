# JoF EternalJK Client
[![Website](https://img.shields.io/badge/website-jofacademy-brightgreen.svg)](https://jofacademy.eu/)

This is the JoF EternalJK Client which focuses on providing a safe client option for JoF Members, friends and others to use.

It was based on an [![Fork](https://img.shields.io/badge/repository-EternalJK-brightgreen.svg)](https://github.com/eternalcodes/EternalJK) version given to us by [Bucky](https://github.com/Bucky21659) with the promise of not sharing some code that was protected by the TECH definition. This is the version of the repo with all of that TECH code removed, and will be where continued development will happen.

## License

[![License](https://img.shields.io/github/license/eternalcodes/EternalJK.svg)](https://github.com/eternalcodes/EternalJK/blob/master/LICENSE.txt)

OpenJK is licensed under GPLv2 as free software. You are free to use, modify and redistribute OpenJK following the terms in LICENSE.txt.

## Contributing

Windows release maintainers: see [signed release packaging](documentation/developer/windows-signing.md)
for trusted publisher setup, PK3 DLL signing, and Smart App Control validation.

Please target the **beta** branch.
PRs opened against **master** may be closed and re-opened against beta.

## For players

Installing and running EternalJK:

1. [Download the latest release](https://github.com/Milamber0/JoF_EJK/releases).
2. Extract the file into the Jedi Academy `GameData` folder. For Steam users, this will be in `<Steam Folder>/steamapps/common/Jedi Academy/GameData/`.
3. Run eternaljk.x86.exe (Rename to jamp.exe for better steam support)


## The Freedom renderer

`rd-freedom_x86_64.dll` (Setup > Video > Renderer: "Freedom Renderer", Windows
x86_64) runs a modified copy of SJK's renderer inside the client: its lighting,
shadows, reflections, weather and post-processing on JoF EJK's game. Its settings
are under Setup > Video > Freedom Renderer Settings, or in the console as the
cvars of the same names, and `graphicsquality vanilla|performance|balanced|high|ultra`
sets them together. Settings marked `*` apply after a `vid_restart`.

It is built on Linux with MinGW-w64, CMake, Ninja and Rust:

    rustup target add x86_64-pc-windows-gnu
    scripts/build-rd-freedom.sh lib/sjk build-rd-freedom

CI builds it in the `freedom-renderer` job and bundles it with the Windows
x86_64 packages. `scripts/gen-freedom-graphics-menu.py` regenerates the settings
menu (`assets/japro/ui/jamp/freedom_graphics.menu`) from the renderer's own
settings catalog.

## Credits
### JoF Maintainers/Contributors
* [Milamber](https://github.com/Milamber0)
* Daniel
* [Jediman](https://github.com/Jediman9973)
* [lumayaa](https://github.com/lumayaa)
* [Sol-Vulpes](https://github.com/Sol-Vulpes)
* [Clix (looZ149)](https://github.com/looZ149)

### External Contributors
* [bucky](https://github.com/Bucky21659) - special thanks for giving us the starting point of this client.
* [eternal](https://github.com/eternalcodes)
* [loda](https://github.com/videoP)
* [SomaZ](https://github.com/SomaZ) - for the Rend2 renderer version we package with our releases: [![Fork](https://img.shields.io/badge/repository-OpenJK-brightgreen.svg)](https://github.com/SomaZ/OpenJK)
* [Sol](https://github.com/Sol-Vulpes), [Bishop](https://github.com/Bishop-R) and the JKR contributors - for SJK, whose renderer the Freedom renderer (`rd-freedom`) is built on: [![Fork](https://img.shields.io/badge/repository-SJK-brightgreen.svg)](https://github.com/Sol-Vulpes/SJK) (GPL-2.0; our modified copy and what we changed are in [lib/sjk](lib/sjk/JOF-CHANGES.md))
* [Sunny](https://github.com/JKSunny) - for the Vulkan renderer version we package with our releases: [![Fork](https://img.shields.io/badge/repository-EternalJK-brightgreen.svg)](https://github.com/JKSunny/EternalJK)
* [Tayst](https://github.com/taysta) - for changes contributed to Sunny's EJK which we sourced from.
* [Circa](https://jkhub.org/files/file/4564-elegance-hud-hd/) - for the HD Elegance HUD textures, based on the original HUD by [DarthNormaN](https://jkhub.org/files/file/296-elegance-hud/).
* [Alereon](https://github.com/Alereon) - for the Cosmetics feature (hats and capes)
* [Jedi Knight Galaxies Developers](https://github.com/JKGDevs) - for their Dialogue feature 
