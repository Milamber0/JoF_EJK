# Changelog

Every Sol JK release, newest first. The client shows this page in game
(main menu > Changelog, or the `changelog` console command).

Credits close each line: who made the change, and "after <client>" when it
follows that client's behaviour; [CREDITS.md](CREDITS.md) has the full record.
Many of Sol's changes were written with Claude (Anthropic) as a coding
assistant.

<!--
Format, read by the client (crates/sjk-viewer/src/changelog_data.rs, whose
tests check this file):
- "## <version> | <dd/mm/yyyy>" starts a release ("## Unreleased" for main
  after the last release, without a date);
- other lines up to the first item are the release's introduction;
- "> <Heading>: <sentence>" lines before the first item are the release's short
  recap (3 to 6 lines, shown by the what's-new card after an update); every release
  dated 12/10/2026 or later needs one;
- a command or cvar in backticks (`whatsnew`), in a recap or a change, is drawn
  in a clickable chip by the what's-new card and the changelog page;
- "- <change> _(<credit>)_" is one change; every change ends with its credit;
- ASCII only, as the menu font draws bytes.
Add each release's notes here when it is tagged (docs/sjk.md "Releases").
-->
## Unreleased

- The Solfox player model reaches every SJK player through the hub: anyone may wear it, and SJK players see it on each other _(Reyced, Sol)_
- Synced emotes sound like one song: every follower's music plays at the very same sample as the leader's and the group is mixed once, so no more echo or extra loudness _(Sol)_

## 2026.1015.4 (Alpha) | 15/10/2026

A hotfix: synced emotes play their music again.

> Synced music: Joining an emote with Sync emote starts your music where the leader's is, in step.
> Both heard: The leader and every follower play the song together.
> Leader stops: Your music carries on without a jump.

- Synced emotes play their music too: joining someone's emote starts your music where theirs is, in step, instead of leaving you silent _(Sol)_

## 2026.1015.3 (Alpha) | 15/10/2026

Dance in step with another player, and emote music that survives busy fights.

> Sync emote: `sjkemote sync` or Sync emote on the quick wheel joins the emote of the player you aim at, or the nearest one.
> In step everywhere: Every SJK player sees the two of you in step, whatever each one's delay.
> One song: The pair shares the leader's music, which carries on if the leader stops first.
> Busy fights: Emote music no longer cuts out when many sounds play at once.

- Dance together: `sjkemote sync` (or Sync emote on the quick wheel) joins the emote of the player you aim at, or the nearest one, in step on every SJK player's screen, sharing one song. Name a player to pick one (`sjkemote sync Sol`) _(Sol)_
- Emote music no longer stops in busy fights: SJK now mixes up to 128 sounds at once (was 32), and when even those are full an emote's music is the last sound given up _(Sol)_

## 2026.1015.2 (Alpha) | 15/10/2026

Seeing bars means sharing yours, a card that says who sees your model, and emote music loops.

> Fair bars: Nameplates with bars share your own (`cg_nameplateShare`), and other players' bars need the SJK hub to have yours.
> Gold contour: Every player who shares has it, including those who see everyone's bars.
> Your model: The player card says whether the player sees the model you wear (`cl_shareModels`).
> Emotes: Looping emotes loop their music, and emote pictures show on the quick wheel.

- SJK staff can try every collectible without touching the hub: `debug_collectibles 1` shows every shader, medal, achievement and card as yours on your PC only, while other players still see what you really hold _(Sol)_
- Seeing other players' bars means sharing yours: nameplates with bars keep `cg_nameplateShare` on, shared and estimated bars show only while the SJK hub has your own, and the hub gives the values only to players who share _(Sol)_
- The gold nameplate contour now also shows on players who share because they see everyone's bars (`cg_nameplateEstimate`), and on your own plate (`cg_nameplateSelf`) in that case _(Sol)_
- The player card says whether the inspected SJK player sees the model you wear ("Sees your model" or "Can't see your model"): SJK tells the hub which player models you have, which `cl_shareModels` (Settings > Network > Share my model list) turns off _(Sol)_
- Looping emotes loop their music, music made of an intro and a loop plays both, and emote pictures stand on a blue disc so they show on the quick wheel; the Blender add-on takes a Music loop per emote _(Sol)_

## 2026.1015.1 (Alpha) | 15/10/2026

Nameplate bars you choose to share, emotes on every JKA model, and the grey settings icons back.

> Shared bars: Health, shield and Force bars show only on players who share theirs (`cg_nameplateShare`); sharers get a gold contour.
> Bars on everyone: `cg_nameplateEstimate` estimates everyone's bars, shares yours and tells the others on your player card.
> Emotes: Emotes made for high quality models now play on the normal JKA models too.
> Fixes: No more purple arrows on community models, and the settings icons are the grey discs again.

- Other players' health, shield and Force bars on the nameplates now show only for SJK players who share theirs (`cg_nameplateShare`, off by default, also in First setup); estimated bars on everyone (`cg_nameplateEstimate`, off) share your own bars and your player card tells the others you see theirs; a player who shares has a gold contour on their nameplate _(Sol)_
- Settings icons are the grey metal discs again, as before 13/10: the gold and the holocron sets are gone, in Settings, the classic settings lists, the in-game menu and the what's-new card; Chat, Mods and the Quick wheel wear their grey icons again _(Sol)_
- The Blender add-on has a Tail gravity setting for a model's tail, cape or other following part, kept across exports _(Sol)_
- Emotes made for high quality models play on the normal JKA models too (Kyle, the retail characters, community models), with the hands, fingers and saber in place and the arms kept out of wider bodies _(Creyon, #103)_
- Community player models no longer show small purple arrows on the chest, belt and face: the model's tag surfaces are never drawn, as in the original game _(Creyon, #104, after Nib's report)_
- The profile's Force preview: Saber Offense waits in the strong (red) stance between swings, and the Force jump stands up as soon as its landing ends _(Creyon, #101)_

## 2026.1014.2 (Alpha) | 14/10/2026

High quality models with pictures in the character grid, emotes every model on the same skeleton plays, tails that sway and hang, and emote music that stays with you.

> High quality models: They get their own heading and pictures in the character grid, and emotes show theirs on the quick wheel.
> Shared emotes: Every high quality model on the same skeleton plays every installed emote, on its own build.
> Tails: A model's tail springs after the body, wags gently and hangs under gravity.
> Emote sound: Your emote's music stays with you as you move; Settings > Sound has an Emote volume.
> Fixes: The character grid no longer opens on a blue team skin.

- Rig models with a picture are listed in the character grid, and an emote's picture shows on the quick wheel; the Blender add-on takes a character icon, an emote's name and icon, a pack name several models can share, and parts rigged to their own skeleton that follow the model's, such as a tail _(Sol)_
- Emotes are no longer tied to the model they were made on: every rig model with the same skeleton (the same joint names, as Unreal characters share) plays them, on its own proportions _(Sol)_
- The Blender add-on names an emote after itself (`sjkemote comrade` rather than the model and action names) _(Sol)_
- Your own emote's sound stays with you when you move (it played where you started it), and Settings > Sound has an Emote volume (`s_emoteVolume`, twice as loud by default) _(Sol)_
- The character grid opens on your model with its default skin, no longer on a blue team skin when your model is named without one, and lists the high quality models under their own heading before the community's _(Sol)_
- Tails move: a rig model's tail (or any chain its sjk_rig.cfg names with `dynamic`) springs after the body and wags gently while you stand; the Blender add-on keeps a tail's own bones and writes the line _(Sol)_
- Tails hang: gravity pulls a moving tail down (one that stuck up 29 degrees now hangs about 24 below level); `dynamic <joint> <gravity>` sets it per chain _(Sol)_

## 2026.1014.1 (Alpha) | 14/10/2026

Player models with their own skeleton and emotes, made with a Blender add-on, 21 new saber shaders after Creyon's ideas, smoother rockets, footprints in sand and snow, and a model viewer.

> Rig models: Player models can bring their own skeleton and dances with sound, made with the SJK Blender add-on.
> Saber shaders: 21 new shaders after Creyon's ideas, each naming its saber smith.
> Rockets: Firing while moving no longer stutters, and the lock-on ring builds up with its sounds.
> Footprints: Sand, snow and mud take footprints and dust.
> Tools: `modelviewer` lists every model and `r_showEntities` marks a map's entities.
> Fixes: Decals fade into fog, and the sky no longer flickers through low ceilings.

- Rig models: a player model can be a glTF with its own skeleton (fingers, face, any bones), animated by the game's own moves through a bone map; a model folder with an sjk_rig.cfg file turns it on; the saber, hats and Force effects sit on the model's own hands and head, and its faces show the right side _(Sol)_
- SJK Rig Model Export, a Blender add-on (scripts/blender/sjk_rig_export): pick a rigged character, name it, one click writes its rig model folder into GameData, the blend file left as it was _(Sol)_
- Rig model emotes: a rig model's own animations play on its skeleton as emotes (`sjkemote <id>`, or the quick wheel, whose editor now lists installed emotes); the Blender add-on exports ticked actions as emotes and packs the model and its emotes into one `rig_<name>.pk3` _(Sol)_
- Emote sounds and moving emotes: an emote's sound plays on the player and stops if it is cut short, an emote can keep playing while you run (`moving`), `sjkemote stop` (and the wheel's Stop emote) ends one; the add-on takes a sound per emote and the animation frame rate, and its emotes reach the other SJK players by default _(Sol)_
- Rig models keep their feet on the ground: each frame their lower ankle is planted where Kyle's is, so a model with longer legs no longer sinks; `lift` in sjk_rig.cfg (the add-on's Height adjustment) fine-tunes it _(Sol)_
- Nine plain saber shaders, after Creyon's Saber ideas page: Tidal, Circuit, Aurora and Echo (the new white Common tier) and Nebula, Cinder, Venom, Sakura and Firefly (Uncommon); the skin list takes up to 256 shaders now _(Sol, ideas by Creyon)_
- Twelve more plain saber shaders: Slate, Amber, Lilac, Mint, Coral, Ivory (Common) and Glacier, Dusk, Ruby, Amethyst, Jade, Moonlight (Uncommon) _(Sol, ideas by Creyon)_
- The last three saber shaders from Creyon's Blade Lab, solid blades: Comic (Legendary; black outline and halftone dots, in your saber colour), Quicksilver (Legendary; rippling liquid chrome with beads rolling up it) and Obsidian Edge (Mythical; a black glass blade with lightning crackling off its edges) _(Sol, ideas by Creyon)_
- Seven more saber shaders from Creyon's Blade Lab: Sketch (Rare), Hourglass and Fiber Optic (Legendary), Serpent, Monarch, Phoenix and Quantum (Mythical), each with its own sounds; a pencil sketch, pouring sand, glass fibres, a coiled serpent, butterflies, wings of fire and ghost copies of the blade _(Sol, ideas by Creyon)_
- Six new saber shaders from Creyon's Blade Lab: Neon and Peppermint (Uncommon), Soap Bubble and Lava Lamp (Rare), Reactor and Bleeding Kyber (Mythical), each with its own sounds and parts you can switch off; holocrons can drop them, so a Mythical holocron can now bring a Mythical shader _(Sol, ideas by Creyon)_
- Every saber shader names its saber smith, who gave its idea: Creyon for the 21 plain ones from Creyon's Saber ideas page, Sol for the others; shown on the shader's card and hover card, and the shader search finds it _(Sol)_
- The holocron odds come from the SJK hub: the loot table, card odds, holocron lines and the crate spin always show what the hub rolls with; a shader newer than your version of SJK still shows when a holocron brings it; the Saber tab's blade row scrolls with the wheel and its arrows without wearing anything _(Sol)_
- Fixes after a review of the shader work: a shader's hover card under a search or filter showed the wrong shader; the search kept the typing after a click elsewhere; the loot table and card odds forgot that a holocron may bring a shader instead of a card and its credits; the Supporter medal hangs like the others (rod, long ribbon, same size) _(Sol)_
- Everything collectible now carries an edition stamp, "Alpha Edition" for the whole alpha (the start of a season system): shown on a shader's card, a medal's card and the hover cards of shaders, medals, achievements and the toy; the shader search finds "alpha" _(Sol)_
- Firing rockets while moving no longer makes you stutter back and forth _(Creyon, after JoF EternalJK)_
- Rocket launcher lock-on: the red ring builds up around the target with its tick sounds, and the lock pulses and sounds when complete _(Creyon, after JoF EternalJK)_
- The in-game profile's Force page shows each power on the model under the pointer: the game's gesture, effect and shell, the saber skills, the jump and the throw _(Creyon)_
- Footprints and dust: steps on sand, snow, mud and dirt kick up dust and leave footprints for ten seconds, untagged sand such as siege_desert's included; `cg_footprints 0` turns them off _(Sol, after the original game)_
- Jumps and forward landings step with both feet, with the ground's sound, dust and prints; `cg_jumpFootsteps 0` keeps the original game's steps alone, and `cg_footstepGuess 0` makes untagged ground stone again _(Sol)_
- Tables and other props a map hangs on a brush entity's model2 are drawn, such as the bar tables of t2_rancor _(Sol, after the original game)_
- sjk-materialgen finds its overrides file in the client's generated folder (and copies one left in the old JKR folder there), says which file it read or warns when it found none, and records it in the pack: the 12/10 pack had been built without Sol's rules _(Sol)_
- Generated material maps (generation 7, regenerate the pack): crates are painted metal rather than wood, a vent in a concrete wall stays concrete, look-alike words (background, rockets, rustic) no longer pick ground, rock or rust, and the overrides take strength= to deepen or soften one texture's relief _(Sol)_
- Decals on walls fade into fog with their wall instead of showing through it at full strength, like the Imperial symbol on mp/ctf4 _(Sol)_
- The sky and its clouds no longer flicker through a ceiling built just under the sky, as on JoF Memorial Grounds _(Sol)_
- Map entities view: `r_showEntities` marks the lights, spawns, origins, items and targets a map places (2 adds sounds, effects, props and the rest) through walls with their names, and a world note picks the one you aim at; `r_showEntitiesTypes` picks kinds, Settings > Game options > Map entities _(Sol)_
- Model viewer: `modelviewer` opens a searchable list of every installed model, players and NPCs with each skin, then every md3 and glm (items, weapons, map objects), shown on the stage _(Sol)_

## 2026.1013.2 (Alpha) | 13/10/2026

World pings for the SJK players on your server, red and green outlines with team logos in team games, world notes that take a page with the remaps in play and the map's models, and cards that tilt towards the pointer.

> Pings: Middle mouse marks a spot for the SJK players on your server, your team only in team games.
> Team games: Red outlines on enemies, green on teammates, your team's logo over their heads, and no more team overlay.
> World notes: Write up to a page, and note a model too; the note keeps the shader remaps in play.
> Cards: Cards tilt towards the pointer, their light following it.

- World pings: `+ping` (middle mouse in a new profile) marks the point under the crosshair for the other SJK players on the server, your team only in team games, with a gold ring seen through walls, your name, the distance and a short sound for 7 seconds; Settings > HUD > World pings hides others' pings (`cg_worldPings`) or the sound (`cg_worldPingSound`), and muted players' pings never show _(Sol)_
- Team games: enemies get a thin red outline and teammates a green one, only where you can see them; Settings > HUD+ > Team outlines (`cg_teamOutline`, on by default) _(Sol)_
- Team logos over your teammates' heads, as JoF EJK floats them: the Imperial or Rebel logo, readable at any range and hidden behind walls, under the nameplate; Settings > HUD+ > Team logos (`cg_drawFriend`) _(Sol, after JoF EJK)_
- The player card's outline is half as thick _(Sol)_
- The team status overlay at the top right is gone, with `cg_drawTeamOverlay` _(Sol)_
- World notes take up to 4000 characters (500 before) in a taller note card of ten lines; a note keeps every shader remap active when it was taken (the map's, the server's and the console's, with the remapped shader beside the map's own); aiming at a model drawn from the map (a `misc_model_static` prop, a breakable) selects that model, outlined in green, and the note names its path, entity, placement and every shader with its remap; Clear model in the card notes what is behind it, and a click on the scene round the card notes what is under the pointer; all of it goes to the SJK team with the note _(Sol)_
- Cards tilt towards the pointer: on the Collection's Cards tab a card under the pointer turns towards it a little in perspective, its Holo or Solar light following the pointer under a soft glare; Holo's light stays inside the card's edge and corners and its sweep crosses the card, then waits, instead of jumping back _(Sol)_

## 2026.1013.1 (Alpha) | 13/10/2026

Settings in two modes, Simple and Advanced, with new holocron icons and the choice of Vanilla or the SJK renderer, trading cards with faces, a character creator of tiles, and chat in Russian, Japanese, Chinese, Korean and emoji.

> Force: Hold push or pull to use it again the moment it is ready, as in JoF EJK.
> Collection: Rest the pointer on anything you collect to see how you get it.
> Settings: Simple or Advanced, Ctrl+F finds any; new players start on Vanilla.
> Cards: Cards have faces and SJK's own rarity symbols, starting with Toxiee and Sol on JoF Memorial.
> Character: Pick models and hats from tiles; Apply wears them.
> Chat: Russian, Japanese, Chinese, Korean and emoji now show instead of question marks.

- SJK Symbols, SJK's own symbol font: marks for the rarities (Common to Mythical, one holocron octagon whose marks grow with rank) and the cards' finishes, drawn in the colour of the text around them on cards, the loot table and the Shaders tab, beside the rarity's name _(Sol)_
- Russian and Japanese text: chat, names, the console and every other text draw Cyrillic, kana and kanji instead of question marks (Noto Sans JP is bundled for Japanese); Russian is sent and read in Windows-1251 as EternalJK sends it, so SJK and EternalJK players with `se_language russian` read each other, and Settings > Chat > Other clients' text (`cl_charset`: Auto, Western, Cyrillic) fixes a wrong guess; Japanese goes as UTF-8, read by SJK players, and the SJK channel and bios take it too; the chat field now stops where the server would cut the line, so a Russian line holds 149 letters _(Sol, after EternalJK)_
- Chinese, Korean and colour emoji: Simplified Chinese and Hangul draw from the bundled Noto Sans SC and KR, emoji are Twemoji's colour pictures, skin tones, families and flags drawn as one and taken whole by the caret and Backspace; they go as UTF-8, read by SJK players, and the SJK channel and bios take Korean, Chinese punctuation and emoji too _(Sol)_
- Holding a push or pull key pushes or pulls again as soon as the power is ready, as in JoF EJK, keyboard or mouse; Settings > Controls > Hold push/pull repeats (`cl_forceRepeat`, on by default) turns it off _(Sol, after JoF EJK)_
- Settings in two modes: Simple, the default, shows the settings that matter most in eight plain categories (Display, Graphics, Sound, Controls, Key bindings, Gameplay, HUD, General), and Advanced every setting, regrouped (Display and Network under sub-headings, Graphics led by the renderer, the FPS readout with the HUD's readouts, the first-person weapon with the camera); the top bar or F2 switches, and `ui_settingsMode` keeps the choice _(Sol)_
- Advanced settings regrouped by subject: every sound in Sound (the SJK chat's and the quick wheel's under Notifications), a Chat category with the chat box and the SJK chat, a Mods category with the asset browser, JA+ and JoF tools, invisible brushes and the experimental showcase tools, and the scoreboard's settings under their own heading in HUD _(Sol)_
- New settings icons, holocron faces: a bronze frame round a glowing blue crystal panel with a bold white-blue symbol, readable at the rail's size, for every category (Chat a speech bubble, Mods a puzzle piece, the Quick wheel its ring of discs, Graphics a prism); they show in Settings, the classic settings lists, the in-game menu and the what's-new card _(Sol, icon art by Sol)_
- Settings' search: Ctrl+F and F3 open it as `/` does, with what was typed selected so typing replaces it, in Settings and Key bindings; it finds every setting in both modes, under the new category names _(Sol)_
- Renderer: Vanilla (the EJK level, renamed) or SJK (alpha), one row in Graphics and First setup in place of the EJK graphics switch; SJK quality picks the SJK renderer's level (Performance to Ultra), and a warning in the detail column says the SJK renderer is in alpha. A new profile starts on Vanilla; existing profiles keep their settings. `renderer vanilla|sjk` in the console; `graphicsquality vanilla` (ejk still works) _(Sol)_
- Choices are named: Dynamic glow, Glow style, Light shafts, Weather quality, Force weather, Ground fog, Shader remaps, Invisible brushes, Saber rain steam and Last key wins show Off, Sabers, Never, Map's fog and the like instead of 0, 1 and 2; Display gamma is now Brightness _(Sol)_
- Settings usage: what players change in Settings and what they search for goes to the SJK team anonymously (no key, name or server) when Settings closes, to make it clearer; Settings > Network > Share settings usage (`cl_shareUsage`) turns it off _(Sol)_
- Cards have faces: a card the SJK hub picks shows as a trading card, its name, its picture, its kind and set, its lore, its number in its set and its artist, under its finish's light, on the Collection's Cards tab (whose card up close is bigger, to be read) and when a holocron opens. The faces download from the SJK hub like the blade skins. Reveal (or R) on the Cards tab turns sealed cards into cards of their rarity once there are some. The first card is Toxiee and Sol on JoF Memorial (Sol lore 1, Legendary, art by Sol, AI-assisted); opening holocrons is still for the SJK team's tests. A Holo or Solar card's light keeps inside its rounded corners _(Sol)_
- Character creator: Profile > Character is one grid of tiles under a strip of tabs (Model, Head, Torso, Legs, Skin, Hat, Cape), with the search, the Default/Red/Blue segments and a turn area for the model; a model or part is worn when Apply is pressed (name, hats and capes at once); the head picker works again after the Model row's removal _(Sol, Apply from a friend's idea)_
- The Jedi Academy's own models, species and saber hilts come first, under a "Jedi Academy" heading with the community's after it under "Community", told by the retail archives (assets0 to assets3) _(Sol)_
- Hover cards in the Collection: rest the pointer on a medal, an achievement, a saber shader, the toy, a card or a holocron tier and a card beside it says how you get it, and where you stand (when it was given, how far along you are, how many you hold, how rare it is) _(Sol)_

## 2026.1012.2 (Alpha) | 12/10/2026

GIFs in the SJK chat, a loot table for holocrons, and Creyon's pull requests: fixes for ledges, dropped sabers, doors and muzzle flashes, a server browser that finds more servers and keeps your favourites and passwords, clan emblems, and the frame rate and clock where JoF EJK puts them.

> Browser: A second master server, favourites by address, saved passwords, a scrolling player list and names read in the right charset.
> HUD: The frame rate on the scoreboard and in the upper right, with an optional match clock under it.
> SJK chat: Favourite GIFs, a GIPHY search, `sjksay` and `sjkgif`.
> Fixes: Ledges, dropped sabers, doors, the muzzle flash and grapple rope.
> Updates: This card pages through skipped releases, says who made them and shows commands such as `whatsnew` in chips you can click.
> Holocrons: The Loot table shows every chance a holocron brings; opening them is being tested by the SJK team and comes soon.

- Force powers apply when you leave the Force page, as the original game's Force menu did on close: Escape, Back, another page or closing the screen no longer drops your changes, and leaving tells the server to read your powers even when nothing changed, so your next respawn or Full Force duel uses them _(Sol)_
- Loot table: the Holocrons tab's Loot table (or L) shows every chance a holocron brings: how often each tier drops, the card's rarity and finish and the credits inside each, the same for any holocron found, and every kind of card as a share and as "1 in N" _(Sol)_
- Opening holocrons is being tested: for now only the SJK team can open theirs, and the Holocrons tab says opening comes soon in place of Open and Open all for everyone else; your holocrons wait for you _(Sol)_
- Holocrons open: Open or Open all on the Collection's Holocrons tab plays a ceremony and gives each one a sealed card (Normal, Holo or Solar, Alpha Edition; its face comes with the first set of cards) and credits, kept on the SJK hub; the new Cards tab shows them, and debug_open rehearses it _(Sol)_
- Standing on a ledge no longer makes the legs stutter: prediction keeps the server's slope pose while you stand still _(Creyon)_
- A dropped or knocked-away saber stays visible on the ground, hilt only _(Creyon, after OpenJK)_
- Doors, plats and buttons play their start and end sounds again (EV_PLAYDOORSOUND) _(Creyon, after OpenJK)_
- Your own muzzle flash starts with the predicted shot, and the snapshot that confirms it no longer plays the flash and its sound a second time _(Creyon, after OpenJK)_
- The SJK scoreboard shows your frame rate left of the clock; Settings > HUD > Scoreboard frame rate (`cg_scoreboardFps`) _(Creyon)_
- Clan emblems come from a table: the JoF Academy (JoFA) has its own emblem, JoF's is teal, look-alike letters such as an o with a stroke or a zero count, and Settings > HUD > Clan emblems (`cg_clanEmblems`) hides them _(Creyon)_
- The in-game menu's SJK chat docks under the match card, a sender's card opens on its left and waits a moment for the pointer to reach it, and the chat page leaves out the text under that card _(Creyon)_
- The Character page has no Model row: the grid chooses the model _(Creyon)_
- The server browser asks a second master (Settings > Network > Second master server, `cl_master2`) and merges the lists, reads server and player names in the game's charset (no more missing servers or ? in names), adds a favourite by its address (Add a favourite, or A), remembers a locked server's password on this PC (Remember on this PC), and scrolls a long player list with the wheel _(Creyon)_
- The frame rate stacks in the upper right as in JoF EJK, with its frame times smaller under it; Settings > HUD > Upper right clock (`cg_drawTimerUpperRight`, off by default) adds the match clock under it in place of the layout's clock _(Creyon, after JoF EJK; Sol)_
- Server browser: the same servers on every refresh, favourites included _(Creyon)_
- Profile: in game, drag the model to turn it; hats and capes picked from chips _(Creyon)_
- Every text field takes Ctrl+A to select all its text, Ctrl+V (or Shift+Insert) to paste, Ctrl+C to copy (not passwords) and a held Backspace to delete on: the server address and password prompts, the browser, settings, character, create game and key binding searches and names, the report dialog and bios _(Sol)_
- Console: `showplayerid` lists each player's client number, team and name; `amcolor` sets and saves your player tint, so it survives a reconnect _(Creyon, after JoF EJK)_
- SJK chat GIF picker: GIF beside Send on the SJK chat page opens your favourite GIFs, a GIPHY search (trending when nothing is typed) and the GIFs seen in the chat as animated tiles; picking one puts its link in your message. The star on a tile, F, or Add GIF to favourites on a chosen message keeps it in your favourites, on this PC. Search uses your own free GIPHY key: the picker shows the link and three steps to get one, and takes it pasted (Settings > Network > GIPHY key, `cl_giphyKey`). The chat's fields copy with Ctrl+C _(Sol)_
- SJK chat from binds: `sjksay <text>` says something in the SJK chat and `sjkgif <favourite number or GIPHY id> [text]` sends a GIF; `+gifwheel` (hold) opens the quick wheel on your favourite GIFs and sends the one you let go on; Settings > Keys > Other has SJK chat page (`sjkchat`, the big chat in one press, in a game too) and GIF wheel _(Sol)_
- JA+ grapple hook: its rope shows again and the hook holds after you release `+button12`. SJK names itself 1.4 SJK to JA+ servers (`amstatus`), as a bare SJK made them treat it as a client without the JA+ plugin _(Sol)_
- The What's new card puts each recap heading, with an icon, above its sentence, turns through the releases you skipped a page each (Left and Right), says who made each release, and shows commands and settings (such as `whatsnew`) in chips: click one to open its setting, or to type it in the console _(Sol)_
- The changelog page shows commands and settings in chips too, clickable the same way _(Sol)_
- An electrified player crackles (a DEMP2 hit, a JoF admin login), at the same rate at any frame rate _(Creyon, after JoF EJK)_

## 2026.1012.1 (Alpha) | 12/10/2026

Downloads that work on JoF servers, your physics following your FPS cap, showcase tools for filming sabers (mimic, ghost replay, strafe lines, invisible brushes), logs the SJK team can ask for, and a card that recaps each update, like this one.

> Servers: Downloads work with spaced pak names, JoF servers get 125 commands a second, known servers sit in one table and JA+ admins see SJK in `amstatus`.
> Gameplay FPS: Your physics follow your FPS cap on Auto, and First setup starts with an import from other clients.
> Showcase tools: Mimic and ghost replay (experimental, off by default), strafe helper lines and the invisible brushes view.
> Player card: The line meets the hips and the player you inspect gets an outline ring.
> SJK team: Staff can ask for your game's logs; nothing is sent unless you choose Send logs.
> Updates: After an update a card recaps what changed; the `whatsnew` command shows it again.

- After an update, a What's new card recaps what changed in a few short lines the first time you reach the menu, with a button to the full changelog; every release now carries that recap in CHANGELOG.md, and `whatsnew` shows the newest again _(Sol)_
- JA+ server admins see SJK in `amstatus`; `cjp_client` sets another name (sent to JA+ and jaPRO servers; jaPRO servers keep 1.4JAPRO by default, the value their client support needs) _(Sol)_
- Downloads (`cl_allowDownload` 1): server paks with spaces, braces or apostrophes in their names now download (JoF's did not), one pak the server refuses no longer stops you joining, and a map you downloaded once still loads on a server that stops listing its paks. A download only adds content now: its configs, scripts, menus and programs are never loaded, it cannot replace your game's or your packs' files, and its shaders cannot change existing ones _(Sol)_
- Log requests: when you have a problem, the SJK team can ask for your game's logs; a card on the menu asks you first and nothing is sent unless you choose Send logs. SJK now keeps the console lines of this session and the last one (no chat) in logs/ beside your settings _(Sol)_
- Typing rate 0 or snaps 0 in the console no longer stops you joining servers or changing your name in a game: they are sent as the lowest values servers accept _(Sol)_
- Gameplay FPS: in the original game your FPS was also your physics (jumps and strafes differ at 125, 142, 250 or 333). On Auto (`gameplay_fps` 0, the default) SJK moves you at your FPS cap as the other clients do, on a steady clock, and servers that fix the movement step (pmove_fixed) keep theirs and JoF's servers get 125 a second at most (Settings > Video > Ignore server caps, or `gameplay_fps_nocap` 1, lifts that); if a server does not take your inputs at a faster rate, the gameplay FPS card opens by itself and suggests 125; set `gameplay_fps` 125, 142, 250, 333 or anything from 20 to 1000 to keep that movement whatever FPS you draw. Settings > Video > Gameplay FPS, First setup, or `help_gameplay_fps` to read why and choose _(Sol, after JoF EJK)_
- First setup starts with Import from a client: it finds the configs of Jedi Academy, JA+, OpenJK, EternalJK / JoF EJK and TaystJK on your computer and brings your name, model, field of view, FPS cap and keys, then asks for your gameplay FPS (Auto, or 125 for a JoF EJK config) _(Sol)_
- rate now defaults to 90000 (was 25000, which made busy servers delay your snapshots so other players looked choppy); a saved 25000 moves to 90000 once, and servers still cap it _(Sol)_
- Player card: its line now meets the player at the hips instead of below them, and the player whose card is up gets a ring around the outside of their body in the card's colour (red, blue or SJK blue), the same width near and far and never through walls; Settings > HUD+ > Card outline (`cg_playerCardOutline`) _(Sol)_
- Strafe lines like JoF EJK's: two lines from the bottom of the screen show the best heading for strafe-jumping with WA and WD, the one for the keys you hold in green; in first and third person, in the air and on the ground. Settings > HUD+ > Strafe lines, or `cg_strafeHelper` 2242 as in JoF EJK (strafehelper lists the keys) _(Sol, after JoF EJK)_
- Mimic for saber showcases (experimental, off by default: Settings > Gameplay > Experimental: showcase tools): mimic <player> [delay] [mirror|copy] [walk] copies another player's view, movement, jumps, crouches, saber attacks, stance and holster live while you face them, as a mirror by default, after the delay you set; walk keeps you walking the whole time; any key stops it _(Sol)_
- Ghost replay for filming showcases (experimental, off by default: Settings > Gameplay > Experimental: showcase tools): ghost record <name> records your run, ghost play <name> walks you to its start and replays it on the same server while someone films, correcting small drift on the ground; any key stops it _(Sol)_
- Invisible brushes: clip, trigger, teleport, push, hurt, ladder, nodrop, slick and other brushes a map hides are drawn see-through with their names, as the Radiant map editor shows them, and so are doors, lifts, buttons and trains (with the box a door, lift or button moves through at 2); Settings > Game options > Invisible brushes (`r_showBrushes`: 0 off, 1 the ones that change play, 2 all; `r_showBrushesTypes` picks kinds), on any server _(Sol)_

## 2026.1011.3 (Alpha) | 11/10/2026

The Void and the Sun get new orbiting effects, and Lumaya's contributions land: jetpack flight prediction, a JoF EJK-style classic scoreboard, saber steam in rain, a lagometer scale, console height and wall-move view fixes.

> Saber shaders: The Void gets orbiting black holes and the Sun's glints circle the blade below the tip.
> Jetpacks: Flight is predicted as the original game does, which should end corrections on JA+ servers.
> Classic scoreboard: A JoF EJK-style scoreboard with chat, a compact list on busy servers and duel winners.
> Small fixes: Wall moves hold the view, the console resizes at once, saber steam in rain, a lagometer scale.

- The Void gets more effects: two small black holes orbit its blade below the tip, bending the world behind them, plus warped space, a gravity pulse beating with its drone and afterimages; each is a part you can switch off _(Sol)_
- Wall runs, run-ups, ledge flips and rebounds hold the view every frame, so fresh mouse movement no longer turns the camera or your character while the wall move holds the view; JA+ free look is unchanged _(Lumaya, after OpenJK)_
- Changing `con_height` while the console is open resizes it at once, and a `toggleconsole` bind follows the setting; Ctrl (full) and Shift (quarter) still force their height _(Lumaya)_
- Saber steam in rain: exposed blades give off the stock steam and sizzle under open sky, not under roofs or in water; Settings > Game > Saber rain steam (`cg_saberRainSteam`: 0 off, 1 everyone, 2 only your blades) _(Lumaya, after JoF EJK)_
- Lagometer scale: Settings > HUD > Lagometer scale (`cg_lagometerScale`, 0.25 to 4) resizes the lagometer around its bottom-right corner in every HUD _(Lumaya)_
- Classic scoreboard like JoF EJK's: chat at the bottom left (hidden while you hold the scores on a crowded server), a compact list on crowded servers with a hold-Tab hint, instant open with a fade on release, duel winners at the intermission, and joins and leaves between score updates tracked; it still does not open on death, and the SJK scoreboard is unchanged _(Lumaya, after JoF EJK)_
- Jetpack flight is predicted as the original game does (gravity, thrust, hover, leg animations), which should end the corrections while flying on JA+ servers, and the pack shows on your back as soon as you equip it; jump pads now recognise the jetpack _(Lumaya, after OpenJK)_
- The Sun's glint leaves the tip: two glints, one smaller, circle the blade a little below it, dimming as they pass behind; the part is now Orbiting glints _(Sol)_

## 2026.1011.2 (Alpha) | 11/10/2026

The Mythical saber shaders grow: the Sun and the Spectral get new effects, and you choose which parts your blade shows, for every SJK player. Also reply and remembered chat channels, jetpacks and the spawn bubble back, weather that no longer covers sabers, sounds on the right side again, and fuller credits.

- The credits page lists much more of what SJK is: Sol's card gains the work of the last days, and the SJK UI's fonts (Rajdhani, Exo 2), GIPHY (the chat's GIFs) and TheRisqe (the Radial HUD SJK's radial HUD follows) are credited _(Sol)_
- Sounds play on the side they come from again: left and right were swapped _(Sol)_
- Your own SJK chat messages play the SJK chat sound when they go out, as other players' do _(Sol)_
- Resizing or maximising the window while a map loads no longer crashes the game when the map starts _(Sol)_
- Rain, snow and fog no longer cover sabers and weapon effects in front of them, the repeater's alt-fire burst no longer shows dark squares, and effects a server hides no longer show as white squares _(Sol)_
- reply <message> whispers to the last player who whispered to you; bare, or on its key (Settings > Key bindings > Other), it opens the chat on that whisper, which Tab also reaches after SJK _(Sol, after JoF EJK)_
- The chat key opens on the channel of your last message (All, Team, SJK or a whisper); Settings > HUD > Remember chat channel turns it off (`cg_chatRememberChannel`) _(Sol)_
- SJK chat lines in the game start with SJK's logo instead of the letters SJK _(Sol)_
- Mythical saber shaders get more effects and parts you can switch off. The Sun: prominences arching off the blade, rising sparks, a glint at the tip and heat haze bending what is behind it. The Spectral: wisps of smoke rising off it, souls drifting round it, a smoky edge, a cold violet rim, an echo swaying beside it and a dimmer light. Collection, Shaders: tick the parts you want under an owned shader (or keys 1 to 6, or saberskin <id> <part> on|off); every SJK player sees your blade that way _(Sol)_
- The green spawn-protection bubble is visible again, as are the Ysalamiri, Force Boon and Enlightenment spheres; jetpacks show on their wearers' backs with their jet flames, start and stop sounds and hover loop _(Sol, after JoF EJK)_
- Saber shaders no longer draw a thin bar across the blade's tip _(Sol)_
- The body shield-hit flash is a little brighter by default: two passes (`cg_shieldBrightness` 2); a profile still on the old default 1 (or 4) moves to 2 once, other values stay _(Sol)_

## 2026.1011.1 (Alpha) | 11/10/2026

Hotfix: when SJK crashes, it now sends the SJK team a crash report at the next start, so crashes can be found and fixed. Also client mods for JA+ and JoF servers and lighter Balanced graphics.

- The Balanced graphics level no longer draws light shafts, the costliest effect it kept _(Sol)_
- When SJK crashes, the next start sends the SJK team a crash report (build, system, graphics card, map, the error and the last console lines, no chat); Settings > Network > Send crash reports turns it off, and crashes/last-crash.txt keeps a copy _(Sol)_
- Client mods that load by themselves on the servers they are for (switches in Settings > Game options): JA+ tools on JA+ servers (japlus.guntele, japlus.goto, japlus.bring and other admin teleports, japlus.autologin, completion of JA+ server commands) and JoF tools on JoF servers (completion of their own commands). serverconfig and pluginDisable are now japlus.serverconfig and japlus.plugin _(Sol, after JoF EJK)_
- Pasting with Ctrl+V no longer freezes the game for a quarter of a second on Windows _(Sol)_
- Fixed a server map change that could leave SJK unable to join again or make it close: it kept reloading the menu map in a loop while reconnecting _(Sol)_
- No more hitch when some players' models appear: their shaders and textures are prepared in the background first _(Sol)_

## 2026.1010.3 (Alpha) | 10/10/2026

Hotfix: no more hitch when players join a server. SJK also checks for updates while it runs.

- No more stutter when a player joins or changes sabers: the saber files are read once a map instead of at every change _(Sol)_
- SJK looks for a new version every 30 minutes while it runs, not only when it starts, so a release made while you play shows its Update card within half an hour _(Sol)_

## 2026.1010.2 (Alpha) | 10/10/2026

Smoother play: no more stutter when players join or change model or when you press keys, far
less memory on servers, a low-FPS helper (help_fps) that offers EJK graphics (Ultra low renamed)
and graphics settings that apply without a restart. Also a Collection of everything you gather,
holocrons earned by playing (collected and shown, not opened yet), saber shader tiers, nine new
blades and GIFs in the SJK chat.

- Fewer stutters when players join or change model: their model now loads in the background and the player keeps their old model (or a stand-in) for that moment; the game also stops copying all of the map's geometry for each new model and prepares glow effects ahead of time _(Sol)_
- The body shield-hit flash is quieter by default: one pass instead of four (`cg_shieldBrightness` 1, the stock look); a profile still on the old default 4 moves to 1 once, other values stay _(Lumaya)_
- The red damage indicator near the crosshair is off by default; Settings > HUD > Damage hitmarker (`cg_hitMarker` 1) brings it back at once. Camera kick and hit sounds are unchanged _(Lumaya)_
- Asset browser: Settings > Game > Asset browser (or the assetbrowser command) lists the installed PK3 packs and turns each on or off for the next start without moving files; the retail base packs stay on, and built-in content, hub unlocks, loose files and packs a server sends are not affected _(Lumaya)_
- Models the server lists but nothing uses are no longer loaded when a map starts: one is read on another thread the first time something shows it (map props, stock weapons and items, effect models and players still load at the start), which should shorten joins and save memory; a rare model can show a moment late _(Lumaya)_
- Your own pistol shot no longer plays its sound twice when the server's snapshot arrives before the shot is predicted: your own snapshot entity adds no event sounds, as the original cgame builds it from the player state _(Lumaya, after OpenJK)_
- Players holding the multiplayer pistol show the blaster pistol model in third person instead of the old Bryar's, so a DL-44 replacement pack applies there too; the old Bryar keeps its own model _(Lumaya, after OpenJK)_
- Low FPS help: a few seconds after a map loads, SJK times its lighting, and when it keeps you under your FPS target a pop-up offers EJK graphics (with a Don't suggest this again tick); type `help_fps` for the same pop-up any time. Ultra low graphics are now called EJK graphics (graphicsquality ejk; ultralow still works) _(Sol)_
- Pressing bound keys no longer rewrites config.cfg: settings and binds are saved only when one of them changed, 2 seconds after the last change and off the main thread, and at once when SJK closes, so key presses cannot stutter a frame on a slow disk or with antivirus scanning _(Sol)_
- Stutters during play are written to the log: a frame over 33 ms and three times the recent median adds a "hitch:" line with the work that took the time (at most one a second), and a "hitches:" count each minute that had any. They also show in the console, and in qconsole.log with logfile 1, to send with a stutter report _(Sol)_
- The cards for achievements, medals and holocrons wait while SJK is alt-tabbed out or minimised and show when you come back, going on from where they were; the big medal and holocron pop-ups also wait to start. A new SJK version found at start now shows a card too: Update available, with the version and Main menu > Update _(Sol)_
- A new medal or holocron that comes during a match now shows as a card at the top of the screen, the same as an achievement (its picture, its name, what it is for or how rare it is, and the achievement chime) instead of a line in the middle of the screen; Esc still opens the big pop-up, which takes the card away. Settings > Sound > Achievement sound is now Unlock sound and covers these cards too _(Sol)_
- SJK uses far less memory on servers: the menu map is no longer kept loaded during a match (it loads again in a few seconds when you leave), and graphics memory is packed tighter after map changes; at 4K about 7 GB instead of 11 to 12 GB _(Sol)_
- Graphics settings that needed a restart (EJK graphics, the graphics quality levels, HDR, FXAA, sun and sky, shadows, light shafts, material maps, reflection probes, texture filtering) now apply with a graphics reload: a pop-up offers it once you leave Settings, and `vid_restart` does it; on a server the map loads again in a few seconds and you stay connected _(Sol)_
- Chat letter spacing: Settings > HUD > Chat letter spacing (`cg_chatBoxLetterSpacing`, -2 to 8, default -0.5: a little tighter than before) spreads or packs the letters of chat messages; long messages wrap to match _(Sol)_
- The resolution list opens as a card in the SJK UI's look, in both menu styles, and the last screens that still used the old settings form (the tabbed settings, the renderer page and the key bindings on their own) now open the SJK UI's Settings or Key bindings _(Sol)_
- SJK chat names keep the marks players put around clan tags: {JoF}, \o/ and Latin-1 symbols such as guillemets now show in the feed, the dock, the chat page and its online window instead of being dropped; messages keep their own characters _(Sol)_
- Saber shaders have tiers, the same as holocrons: Mythical (Sun, Void, Molten, Spectral), Legendary (Storm, Unstable, Chameleon, Prism), Rare (Frost, Hologram, Runic, Heartbeat, Glitch, Banner), and Uncommon for plainer ones to come; the Collection and the Saber tab list the rarest first and show each tier in its colour, and the Collection's Shaders can show a grid of cards (V, or the two marks by Saber; `ui_shaderView` keeps it) _(Sol)_
- Rarity shows itself: Rare frames breathe, Legendary ones catch a sweeping sheen and Mythical ones a spark running round the border with motes rising, on the Collection's shader cards and swatches, the Holocrons tab, the Saber tab and the player card (Uncommon stays plain; locked ones only breathe faintly); the chosen card of the grid lifts with a white outline; Settings > Interface > Rarity effects (`ui_rarityEffects`) turns them off _(Sol)_
- A new SJK chat message from another player plays a sound, in the menus and in games: a variant of JKA's chat sound, made from the game's own file when SJK starts. Not for your own messages, muted players or the history; at most once a second. Settings > Network > SJK chat sound (`cl_sjkChatSound`) turns it off, as `cg_chatSounds` 0 does with every chat sound _(Sol)_
- GIPHY links in the SJK chat show their GIF, animated, under the message on the SJK chat page, the chat docks of the main page and the game menu, and the chat box while you play (a small one, three lines high, that fades with its line; the link reads GIF). Your PC loads each GIF from GIPHY itself, so GIPHY sees your address; muted players' GIFs are never loaded. Settings > Network > SJK chat GIFs (`cl_sjkChatGifs`) turns them off and keeps links as text _(Sol)_
- Create game and its map list are drawn in the SJK UI's look, with the classic menus too: the match's rows under Match and Server, a gold Start button, the chosen map's picture with its game type, bots, limits and who can join, and a map list with a search pill and each map's game types _(Sol)_
- The console feed, the console lines that showed for a few seconds at the top left while playing, is off by default; turn it back on with Settings > Interface > Console feed (`con_drawNotify` 1). Chat, centre prints and the kill feed still show, and the console keeps every line _(Sol)_
- A new Collection, under Profile on the main page and in the game menu, holds what you collect: Medals hanging from a rail, Achievements as a wall of medallions by category with the ones nearest to unlock, Shaders (the blade skins, now called saber shaders), Toys (Illuminate) and Nameplates (ornaments to come); the Profile screen keeps Character, Saber, Force and SJK Profile, and the collection, achievements and unlockables commands open its tabs _(Sol)_
- On the Collection's Shaders, Toys and Nameplates your own character stands beside the page (on the menu map, or in a live preview in a match): it holds the saber shader you choose, a locked one too as a preview only you see, the Illuminate holocron floats by it, and your nameplate stands over its head _(Sol)_
- Holocron drops: after every 30 minutes of active play on a server (moving, looking or pressing keys, in the match, out of the menus, not spectating) the SJK hub can drop a holocron of one of four tiers, Uncommon (60%), Rare (28%), Legendary (10.5%) or Mythical (1.5%), at most 8 a day and 1 Mythical a day for you; a new one shows once in a pop-up on the main menu or the game menu (a centre print says so in a match) in its tier's colour, and several show one after another. Legendary and Mythical drops are told to every SJK player in the SJK chat as a line in the tier's colour with a small gem; yours also show as "You found ...". The SJK team can give one with a note (Staff page). debug_holocron <tier|all> [x<count>] tries the pop-up without the hub _(Sol)_
- The Collection has a Holocrons tab (and a `holocrons` command; the Profile page's See holocrons in the classic menus): a holocron turns in the world behind it in the look and light of the tier you choose (Uncommon, Rare, Legendary, Mythical), dimmed with a padlock when you hold none of that tier; beside it, the four tiers with how many you hold and the odds of a drop, how long to your next holocron and how many dropped today, and your ten newest holocrons. Holocrons cannot be opened yet _(Sol)_
- Nine new saber shaders, each with its own sounds: Unstable (a raw red blade, its edge ragged and its length faltering), Molten (lava cracks and embers falling off it), Spectral (fading afterimages when it swings), Glitch (colours splitting, blocks jumping), Hologram (a wireframe with scan lines), Runic (glyphs spelling your name), Chameleon (the colour of the light where you stand), Banner (your team's colour, silver outside a team) and Heartbeat (a double heartbeat, its light beating too). The SJK team gives them for now; the Collection's rack and the Saber tab's blade row scroll to hold them all _(Sol)_
- Medals bring saber shaders: the Bug Hunter medal the Glitch blade, Early Tester the Hologram, Early Contributor the Runic, for as long as you hold the medal (the SJK hub takes the shader back with the medal). The Collection says which medal a shader came with, and each medal says which shader it brings _(Sol)_
- The SJK chat page no longer freezes the game when you choose a message or rest the pointer on a name _(Sol)_
- Players with the JoF clan's tag in their name ({JoF}, jof., [JOF] and so on, in any case, with no letter touching it) get the clan's J-o-F emblem on the left of their name in the game chat, the SJK chat, the scoreboard, the player card and the Players page; the JoF Clan medal is gone, replaced by this tag _(Sol)_
- Inspect (X) is for players only now; world notes get their own key, World note (Key bindings > Interaction, or bind <key> worldnote), O by default: press it on a wall, floor or object to name it, again to write the note _(Sol)_
- The player card shows the saber shader a player wears (a live swatch and its name) and, for SJK players, their profile picture, a teal SJK TEAM mark for the SJK team and a gem of the rarest holocron they hold; pinned with Inspect it also shows their achievements (unlocked out of 21, the three rarest as medallions), the first line of their bio and since when they are in SJK _(Sol)_
- The quick wheel opens on its Force page by default: the pages are now Force, General, Toys and Weather, and Q opens on the page you used last, or on Force in a new run. A wheel you had set up keeps its pages and order; one still holding the pages as they came takes the new order _(Sol)_
- A Toys page on the quick wheel (`+wheel toys`) holds Illuminate, which no longer sits on the Force page, so that page lists only Force powers. Toys are choices like any other: Settings > Quick wheel has a Toys group to put Illuminate back, and a wheel you had set up gets the Toys page once, after its Force page _(Sol)_
- Illuminate is a toy now, not a Force power: it is gone from the Force bar and wheel, and its command is toy_illuminate (renamed from force_illuminate, which still works in a key bind or quick wheel choice you saved, but is no longer listed); every toy will be a toy_<name> command. It is always there: bind a key to Toy: Illuminate (Settings > Key bindings > Interaction), use the quick wheel's Toys page, or the Holocron lit switch on the Collection's Toys tab. The Settings > Game switch and `cg_illuminate` are gone, and the holocron is out at every start _(Sol)_
- A Melee action in Settings > Key bindings > Weapons (the `weapmelee` command, unbound until you pick a key) selects the fists directly, never the saber, and does nothing when they are already out; Saber / melee on key 1 is unchanged _(Sol)_
- `firstsetup` import takes a quoted Windows path as Explorer's Copy as path gives it ("C:\Users\...\my.cfg"); its backslashes were read as escapes, so the file was not found _(Sol)_
- Importing another client's config no longer needs dragging a file or typing a path: First setup has an Import a config file row (near the end) that opens a file window listing .cfg files, and the Import page has a Browse button (B) to pick another one _(Sol)_
- In the SJK UI the Import page is a pop-up card like Update's, with a switch per part and Browse, Import and Cancel buttons, instead of the old full-screen page _(Sol)_
- The Update and Import pages and Camera control have the SJK UI's look with the classic menus too, and the SJK UI's fonts load in every menu style, so SJK's own pages (Profile, Collection and the rest) no longer fall back to another font there _(Sol)_
- The last screens in the old full-screen form look are gone, so every SJK screen and page now has the SJK UI's look, and the Menu contrast setting, which only darkened those screens, is removed _(Sol)_
- The resolution list puts the sizes closest in shape to your monitor right after its own: on a 16:9 screen 16:10 comes next, then 4:3 and 5:4, instead of 16:10 last _(Sol)_

- Chroma saber shaders: the Storm, Unstable, Spectral, Glitch, Hologram, Runic and Heartbeat blades now take your saber's colour (each saber its own, color1 and color2; a white custom colour keeps the shader's own), and everyone sees them in your colour; a small colour wheel marks them in the Collection, on the Saber tab and on the player card. On the Collection's Shaders a double click wears an owned shader, and a long name no longer runs under its Worn / Yours / Locked label _(Sol)_
- Staff tools: the SJK team can verify a player (Verify, or Unverify to take the mark away) and, for a player who reset their key, merge the new key's player into their old one: the Staff page's Keys and merge tab takes the other key id (typed, pasted or picked in the list) and a card says who is kept, who is merged away and that it cannot be undone. Everything moves to the kept player, and one player can now have several keys: each linked key plays as that player, with their name, medals, unlocks, achievements and holocrons, and staff can unlink a key again (it starts afresh, nothing moves back) _(Sol)_
- The SJK chat page has a small window of who is online, as round faces: each SJK player reading the chat now shows their profile picture (or the first letter of their name), a gold dot for those playing a match, and below them smaller faces for the ones seen most recently; rest the pointer on a face for their card, which says when they were seen. Until the SJK hub lists them it shows how many are reading and the faces of the last players who spoke _(Sol)_
- Ctrl+V (or Shift+Insert) pastes into the SJK chat's field, on the SJK chat page and in the chat docked on the main page and the game menu: line breaks become spaces, characters the SJK chat cannot send are left out, colour codes are kept and the paste stops at the 150-character limit _(Sol)_
- The debug panel's test list (`debug_panel`) is a full SJK UI page in every menu style, like What's new: tabs under its title, the entries with their ticks on the left, the chosen one's changes, steps and notes on the right, keys at the bottom _(Sol)_

## 2026.1010.1 (Alpha) | 10/10/2026

The Profile screen becomes one row of tabs, with Medals and a Collection of everything to unlock;
the Saber tab can search hilts and wear your blade skins; the quick wheel gets a live Force page;
the SJK chat wraps long messages and joins the in-game menu; and your key stays off screen.

- The Profile screen has one row of tabs: Character, Saber, Force, SJK Profile, Achievements, Medals (each medal with its picture, what it is for and when it was given, and the ones still to get) and Collection (every unlockable, owned or locked, with how to get it); the in-game menu's Achievements entry is now a tab, and the main page's Character button reads Profile _(Sol)_
- The SJK identity key moved from the Profile screen to Settings, Network, and its key id and file stay hidden until Show key, each time the page opens, so it can be opened on a stream _(Sol)_
- Your SJK picture can be chosen with Browse... (the system's file dialog), and a picture is checked more strictly before it is sent: empty, damaged or cut-short files, files that are no PNG, JPEG or TGA whatever their name, pictures under 64 pixels, more than four times as long as wide or fully transparent are refused, each with its reason _(Sol)_
- Your own SJK key id is no longer printed on the Profile screen, your Players card, your sender card or the SJK chat's side panel; the SJK page of the main menu loses its Profile entry, now the main Profile button _(Sol)_
- The Profile screen's tabs make room for a larger menu text size (`ui_textScale`, `ui_letterSpacing`), so Achievements is no longer cut, and the main page's Profile line is shorter, clear of the servers _(Sol)_
- The Profile screen's Saber tab can search the hilts (type any part of a name; Escape clears it) and has a Blade row to wear the stock blade or any blade skin you own, shown as small Collection swatches _(Sol)_
- The quick wheel has a Force page, between General and Weather: the Force powers you can use right now, the selected one marked; an instant power is used at once and selected, Grip, Lightning, Drain and Stasis are selected for your Use Force key; past 12 powers the rest go on a Force 2 page. Existing wheels get it once; Settings > Quick wheel can rename, move, remove and add it back _(Sol)_
- The SJK chat docked on the main page shows a long message whole, wrapped onto as many lines as it needs instead of cut at the column's edge, and the SJK UI's in-game menu (Escape in a match) docks the same chat under the match: read it, type and send, rest the pointer on a name for the sender's card _(Sol)_
- The gold dot after a changed setting (and after a rebound key) in the SJK UI's Settings sits right after the name's last letter at every size, instead of over the letters or far after them _(Sol)_

## 2026.1009.1 (Alpha) | 09/10/2026

SJK's lighting got much faster on heavy maps such as JoFTemple, with an Ultra low switch for the
most frames; the in-game menu is rebuilt around seven entries; and the bugs players reported
through the hub are fixed: saber styles, fullscreen, Force profiles, saber changes on JA+
servers, the bug report box, vehicle pilots' names and a vehicle HUD.

- Blue (fast) style with a staff saber that has one blade off, or with dual sabers with one switched off, no longer restarts the swing animation and spams the saber-on sound: the client works out the saber stance base for each predicted command, as OpenJK's client does, instead of treating the sabers as fully off _(Sol, after JoF EternalJK)_
- Leaving exclusive fullscreen works: Alt-Tab or Win+D drops the game to a window and the desktop gets its own video mode back; returning to the game enters exclusive fullscreen again. Borderless and windowed are unchanged _(Sol)_
- A forcepowers typed in the console or set by a cfg is sent to the server like the Force page's Apply, so a Force level you lower there applies at your next respawn; when nothing is sent, the console says why _(Sol)_
- On JA+ and jaPRO servers a saber picked in the menu (or by setting saber1 and saber2) is sent with the server's saber command and applies at once instead of at your next respawn, when the server allows it (standing still, free for all, no duel) _(Sol, after JoF EternalJK)_
- Report a bug (and the other text boxes of its kind) has a real text cursor: Left, Right, Up, Down, Home, End, Delete and Backspace work where the cursor is, Ctrl moves by words, a click places the cursor, and the cursor sits between the letters whatever the text scale; why Send refuses shows in a red band above the buttons _(Sol)_
- The names of pilots sitting inside fighters and walkers show on their nameplates, above the vehicle _(Sol, after JoF EternalJK)_
- A vehicle's weapons no longer play an ordinary gun's flash sound: the original game plays none for them _(Sol, after JoF EternalJK)_
- Vehicle HUD for the pilot: hull, shield, speed, ammunition and turbo bars at the bottom centre, as in the original game; fighters replace the player's own status HUD and the crosshair is doubled or uses the vehicle's own _(Sol, after JoF EternalJK)_
- JoFTemple runs about twice as fast with SJK's lighting: large floors with a coarse lightmap now take their lamp light from the lighting cache instead of being lit lamp by lamp every frame (7.5 ms a frame becomes 3.6 ms at 4K) _(Sol)_
- Maps with large tiled light textures load much faster with SJK's lighting: a material that would make more than 4,096 lights is read at a coarser grain, so JoFJKMission's JKLevel1 loads in about 28 s instead of 91 s and looks the same _(Sol)_
- Platforms and parts that never stop moving (bobbing, rotating, swinging) no longer block the map's lamps, which made the lighting redo work every frame: JoFTemple runs about 40% faster with SJK's lighting while its platforms move _(Sol)_
- Maps load faster where fullbright models with a lit shine (such as JoFTemple's statues) were taken for light fixtures: they no longer become thousands of lights, so JoFTemple loads in 16 s instead of 23 s with SJK's lighting _(Sol)_
- Ultra low graphics: a switch under Graphics quality (Video and First setup) that brings back the original game's look, baked map lighting without SJK's sun, shadows, reflections, material maps or post-processing, for the most frames on heavy maps such as JoFTemple; turning it off restores your settings. Maps also load faster without SJK's lighting, as their lamps are no longer built when nothing uses them _(Sol)_
- The in-game menu has seven entries (Resume, Profile, Achievements, Players, Settings, Servers, Leave): your side, the vote on and Call a vote are on the match card, Camera control, What's new, Credits, Report a bug, SJK chat and Staff tools are small icons under the emblem, and Tab moves between the list, the icons and the card. Profile is one screen with Character, Profile and Identity as tabs (Ctrl+Tab), from the game menu and the main page; Camera control is also a quick wheel action and the cameracontrol command _(Sol)_
- A blade skin's sounds now play over the stock saber sounds instead of replacing them: the normal ignition, switching off, swings and hum are still heard below, with the skin's on top; a thrown saber hums both too _(Sol)_
- Fixes from reviewing the saber switch sounds, profile pictures and chat mute: no sound list growth from retail's .mp3-only saber sounds, a player never shows another's picture after a hub change and a hub's oversized picture is refused, a reconnect redraws muted players as Kyle, muting slot 0 no longer silences every saber block, and a mute list that is not UTF-8 still loads _(Sol)_
- Mute a player from their name in chat: hover a name in the in-game chat (composer open), the menu dock or the chat page for their card with Mute or Unmute. A muted player's chat is hidden, they show as Kyle with the default blue saber (no blade skin, hat or cape), and none of their sounds play; the list is kept in chat-mutes.txt and survives restarts _(Sol)_
- The SJK profile card, bottom left of the SJK UI's main page and in the in-game menu: your picture (or your initial), name and verified tick, medals and achievements, model and blade; a click opens your Profile. Upload a 128 px square picture from the Profile page (drop a PNG, JPEG or TGA on the window, or sjkavatar <file>); other players' pictures show on the player card and Staff can take one down _(Sol)_
- SJK chat is written in its own gold, outside the game's colour codes, a sender's tag, name and tick stay on the first row and the message goes on there, and a verified sender shows the tick alone _(Sol)_
- Lightsaber ignition and retraction sounds play when a player switches to or from the saber, for you and for other players, as in OpenJK; a client's saber unholster event plays that player's own saber sounds; a blade skin's sound plays over them _(Sol, after OpenJK)_
- Blade skins: the Sun blade's tip is rounded instead of square, and skins can have lightning arcs, drifting motes and turning hues; four new skins from the SJK hub (Storm, Void, Frost, Prism). The Unlockables page shows two cards at a time and scrolls _(Sol)_
- New medals get a pop-up in the menu style in use (floating over the scene in the SJK UI, a classic+ box in the classic menus) with an award animation, the Jedi Master fanfare from the game data and a Next or Close button; debug_medal <id> [x<count>] [note] rehearses it without touching the hub _(Sol)_
- The SJK UI's Force page keeps Apply, Discard and Start over clear of the This server text (Apply spans the left column, the other two sit under it), and a right click on a Force level removes it; a left click only buys up to a level _(Sol)_
- On servers with force-based teams, a change to the server's settings no longer sends you the wrong side's Force profile; your nameplate's own Force powers match what the server grants the profile sent; and the kill feed shows Force Grip's holocron for a grip kill instead of Lightning's _(Sol)_
- A server that parks you in spectator over your Force profile is now answered again as intended: the up to three rejoin retries are polled every frame (they never went out), and not in duel or power duel, where waiting as a spectator is the queue _(Sol)_
- Force profile on a server with other Force rules: the profile you apply is sent to the server (read at once as a spectator, else at your next respawn), the Force page takes the server's rank and free saber skills from it and marks the powers it turns off and the team powers outside team games, and the SJK UI's Force page has a This server panel _(Sol)_
- Dynamic lights (sabers, muzzle flashes, explosions, Force effects) no longer light through walls: each active light (up to 32) traces a small shadow tile against the map every frame; `r_dynamicLightShadows` 0 turns it off _(Sol)_
- Parallax mapping reaches farther: the depth fades only where it would move the texture by less than half a pixel (a floor keeps it to about 410 units at 1080p, not 90-160), and `r_parallaxNearDistance` (default 24 units, 0 is off) holds it back close to a surface so it stops swimming under the camera; `r_materialMapsDebug` 7 shows the reach _(Sol)_
- Kill feed: up to five kills at the top right, newest first, each as killer, weapon icon and victim (a skull for a suicide or a fall), kept 5 s and then faded; `cg_killfeed` (Settings > HUD > Kill feed) is on by default once and replaces the old one-line obituary _(Sol)_
- The compact SJK scoreboard is sized to its names and centred on the screen instead of always running the full width: the name column is 140-320 px, the numbers sit close behind it, a board that would cover the chat sits just right of it, and the duel cards share the centre _(Sol)_
- The green beam of a teleport or a spawn now stands up from the floor to the ceiling or the sky, as in OpenJK, instead of a small square at the player's feet: effect lines that follow the trace (org2fromTrace) get their length, the floor drop stops on terrain too, and the Jedi Master spawn effect points straight up _(Sol, after OpenJK)_
- Kill messages in the console print the names of players with accented letters or symbols instead of noname, end each name in white as OpenJK does, and keep the right gender in suicide messages _(Sol, after OpenJK)_
- Doors, buttons and other brush models play their sounds from the middle of the model, so the opening and closing sounds of doors that the map gives no origin are heard again _(Sol, after OpenJK)_
- Real-time lighting pipelines are now compiled while a map loads instead of in the first frames of play, to cut the stalls (around 10 FPS for a while) that followed a load; the load itself takes a little longer _(Sol)_
- `com_maxfpsMonitor` (Settings > Video > Detect refresh rate), off by default: the AUTO FPS cap (`com_maxfps` -1) is now 125 unless it is on, when it follows the monitor's refresh rate as before; a cap you set always wins _(Sol)_
- The game no longer freezes when you switch back to it after a long time away if the sound device went away meanwhile (a headset or monitor speakers switching off): a full sound queue waits a quarter of a second, then drops sounds, the log says audio output stopped taking sound, and `snd_restart` brings the sound back _(Sol)_
- On the SJK UI's Character screen, a click on a saber colour, a < > control or a Force power now lands where it was aimed: it picks the blade chip under the pointer and turns a < > control by the half you click, and a click on a Force power's name only chooses it _(Sol)_
- Unlockables: blade skins your SJK identity owns at the SJK hub, first the Sun blade, a white-gold core in an orange corona with flares running along it, an amber trail, a warm flickering light and its own ignition, hum and swing sounds; its look and sounds come from the SJK hub as a pack the client downloads (kept in assets beside identity.key), so a blade skin shows once its pack has arrived. Every SJK player on your server sees and hears the skin you wear, and now sees your Illuminate holocron too. The Unlockables page (Profile, or the unlockables command) shows what you own and equips it; saberskin does it from the console; for now the SJK team gives them _(Sol)_

## 2026.1008.2 (Alpha) | 08/10/2026

The modern menu style is gone, leaving the SJK UI and the classic menus. New: profiles
with achievements, SJK chat through the hub, graphics quality levels, doors and lifts
that block lamp light, a compact scoreboard, cl_idrive and +duck, and percent signs and
quotes that survive in chat; plus the fixes that came with them.

- A % typed in chat now arrives as a % and a " as a ", for SJK and JoF EternalJK players alike (they used to turn into . and a space); the chat box counts the longer escapes in its length limit _(Creyon, after JoF EternalJK)_
- `+duck` crouches without rolling, as in JoF EternalJK: bind it in place of `+movedown` (Controls > Movement > Crouch, no roll) to crouch while running without going into a roll _(Sol, after JoF EternalJK)_
- Remapped surfaces drawn with a vertex-lit texture (like the ones a server puts on mp/ffa4) take the map's live light instead of staying too bright _(Sol)_
- `cg_remapsBlockedMaps` lists maps where the server may not swap textures, as `cg_remaps 0` on those maps only; `blockRemaps` and `unblockRemaps` add or remove the loaded map (or a named one) _(Sol)_
- The SJK scoreboard is compact by default: thinner rows keep a full server (32 players in free for all, or on one team) in one column; Settings > Scoreboard > Compact SJK scoreboard turns it off. While the scoreboard is held, in any style, the HUD steps aside as for the quick wheel (chat stays) _(Sol)_
- Doors, lifts and other moving brushes now block the light of the map's lamps while closed and let it through when open, hidden or broken; far sun shadows follow them too _(Sol)_
- SJK chat: one conversation for every SJK player, through the SJK hub rather than the game server, so it reaches SJK players on any server and in the menus. In a game, I (messagemode5, bound in new profiles; bind it under Settings > Key bindings > Other otherwise) writes to it (Tab cycles All, Team and SJK) and its messages show in the chat tagged SJK; the main menu docks it under Recent servers; its page (sjkchat, Open chat, or the in-game SJK menu) shows it all and mutes a sender on this PC. The hub keeps the last 200 messages in memory only; `cl_sjkChat` 0 (Settings > Network > SJK chat) turns it off _(Sol)_
- Emotes, groundwork: sjkemote <id> plays an emote the other SJK players on your server see, through the SJK hub; emotes come as emotes/<id>.emote files, none yet, and until their animations exist the console says who emoted _(Sol)_
- `cl_idrive` (Settings > Controls): with both keys of a movement pair held, the one pressed last wins (1 all pairs, 2 jump/crouch only); `cl_idriveDelay` keeps you still for a few milliseconds when you reverse, for servers that dislike instant reversals _(Sol, after JoF EJK)_
- Graphics quality: four levels (Performance, Balanced, High, Ultra) at the top of First setup and of Settings > Video set the costly rendering options together; a fresh profile reads High, a hand-changed setting reads Custom, and nothing changes until you pick a level (`graphicsquality` in the console) _(Sol)_
- The character screen's model squares, the Force page, the part pictures, the key-binding pictures and the map and HUD previews no longer turn black or show another model after you join a server, change map or disconnect _(Sol)_
- The modern menus are gone: SJK has the SJK UI (the default) and the classic menus. The modern scoreboard, console and HUD layout go with them, as do the menu accent setting and the gate on mp/ffa3 that opened as you joined. A setting saved as modern starts on its default again (the SJK UI's menus, the game HUD) _(Sol)_
- The player card points at the player's hips instead of their head, Escape unpins a pinned card (without opening the game menu), the duel record line is gone and medal names are small print _(Sol)_
- Surfaces that a server or the map remaps to another shader keep the map's light and shadow; they used to look flat and uniformly pale _(Sol)_
- Staff tools (for SJK staff, from Profile or the staff command): find any player, give or take back their medals with a note, and clear their achievements; clearing your own also resets your counts, so you can unlock them again _(Sol)_
- U is now bound by default to a private message to the player under your crosshair (`messagemode3`); Y chat and T team chat stay, and an existing profile gets U only when it is free _(Lumaya, after OpenJK)_
- The HUD steps aside while the quick wheel is open (what HUD off hides; chat and nameplates stay) _(Sol)_
- Unlocking an achievement shows a pop-up at the top of the screen, as in World of Warcraft: its medallion lights up with a gold ring, a burst of light and sparks, with the name and what it took, for five seconds, while the single-player game's secret-area chime plays. It takes no input and never pauses play, shows on the menus too, and several unlocks follow one another; Settings > Sound > Achievement sound turns the chime off _(Sol)_
- Chat emojis: `cg_chatBoxEmojis 1` (Settings > HUD > Chat emojis) draws JoF EternalJK's emoji pictures in place of their names, like `:poop:`, in new chat messages; `listEmojis` lists them (the pictures come from japro-assets.pk3 in the EternalJK folder) _(Creyon, after JoF EternalJK)_
- Typing in the console shows colour codes in their colours: `^1` turns what follows red and the code itself is drawn in that colour, in the classic console and the SJK UI's _(Creyon, after JoF EternalJK)_
- Profile (SJK > Profile, or the profile command): your SJK profile as other players see it, with your medals, your name and the names you wore, your record and your bio, which you now write there on up to 6 lines. Bios keep to letters, digits, spaces and simple punctuation: no emoji, invisible or direction-changing characters, and nothing else shows even if a hub sent it _(Sol)_
- Achievements: 21 to unlock, from First Blood to a hundred duels won, ten flags captured or 25 maps played, counted in your matches on servers and kept on the SJK hub with your profile; Profile's Achievements tab (or the achievements command) shows the board _(Sol)_
- Quick wheel: every choice has its own icon (custom commands share one), and the wheel plays the game's menu sounds as you change page, move to another choice and run one; Settings > Quick wheel > Wheel sounds turns them off _(Sol)_
- The menus, the window title and the console call the client SJK instead of Sol JK _(Sol)_
- Running water on walls stays on the wall: it no longer flickers or slides as you move or look around, and runs down in thinner streaks _(Sol)_
- Your drain now empties your victim's nameplate Force bar by exactly what it took (your own heal says how much), and what it shows of the server's drain speed and strength is used for everyone else's drain and lightning _(Sol)_
- A chat message that wraps onto more rows keeps its colour: a green message is green on every row and a colour change carries into the next row, where the second row used to turn white _(Sol)_

## 2026.1008.1 (Alpha) | 08/10/2026

The SJK UI becomes the default menu style, with pop-up cards, its credits and the
character pages in game; rain wets the world; medals, player reports and Illuminate
arrive; nameplates follow the Force powers; the camera stays locked behind you by
default, and Sol JK stands on its own.

- Rain wets the world: what it falls on darkens and takes a sheen of the sky, while floors under roofs and players stay dry. With weather quality high, water runs down walls and slopes; with ultra, puddles gather on flat ground with rain rings in them (Settings > Graphics, Weather quality) _(Sol)_
- SJK UI Credits: the emblem is the page's sun on the left, its sunburst and god rays kept, with the sections under it (Tab jumps between them, E opens every fold) and the people in a column on the right. Cards show their medals in every menu style; Creyon and Lumaya wear Early Contributor _(Sol)_
- SJK UI Report a bug is a pop-up card, as are a player report's words and the world note: the question, your text with a gold caret, the rules and the count, Cancel and Send. A report's card then waits for the SJK hub and says it was sent (with its number) or why not, with Edit to change your text and send it again _(Sol)_
- Nameplates predict much more: lightning, grip and rage now cost health (and rage halves the hits a player takes), protect turns hits into Force spent, team heal and energize give their exact share, drainers heal by what they drain, and the Force bar charges force jumps by how fast they climb, katas, cartwheels, lunges and other saber specials, wall jumps, heals, team powers and the start of a grip. Absorb gives Force back, a knocked-away saber no longer counts as a throw, and a special refused for want of Force shows the bar under half _(Sol)_
- Shot controls is now Camera control (the game menu's entry, or F8), and with the SJK UI it has its look: a column down the right edge over a fade, the rest of the scene left clear with viewfinder corners and thirds marks, Camera and Sun as tabs, switches for live preview and the HUD, and the keys of what you are on. Up and Down stop once on each slider _(Sol)_
- Q opens the quick wheel on the page you used last (it always opened on General); R still opens Weather _(Sol)_
- Nameplates count Force drain: the Force bar of whoever you (or anyone) drain drops shot by shot and stops refilling for a moment, and drainers pay for their drain. The verified tick now sits level with the name _(Sol)_
- The quick wheel has pages: hold Q, then scroll or left and right click to go from General to Weather and any page you add. Settings > Quick wheel (Interface > Quick wheel pages with the classic and modern menus) adds, names, orders and removes pages and picks each page's choices from SJK's actions or your own console commands. The wheel now has the SJK UI's look _(Sol)_
- SJK UI First setup is a pop-up over the map: its settings scroll inside the card, and "Don't show at start" is a tick always in view at its foot, beside All settings and Done _(Sol)_
- SJK UI Update is a pop-up card too: the version, what the check found, the download's progress, and Release notes, Check again, Close and Install along its foot _(Sol)_
- Sol JK stands on its own: the old jkr_ names of settings and commands, the JKR_ environment variables and the one-time import of GameData/jkr are gone. Downloaded files now go to SJK's own folder (%LOCALAPPDATA%\SJK\downloads), the dedicated server saves sjk_server.cfg and the material-map generator writes zzz_sjk_materials.pk3 _(Sol)_
- SJK UI in game: the in-game menu's Character opens the SJK UI's character, saber and Force pages too, your model standing beside them in a live preview holding your sabers, over the dimmed match _(Sol)_
- Medals: the SJK team can give players medals (Early Tester, Early Contributor, Bug Hunter, JoF Clan). They grant nothing; they show as ribbon bars after the SJK emblem on the scoreboard, as medallions on the player card (X) and on the SJK UI's Players page, and in full with the date and the team's note on your Identity page. A medal new to you shows once in a pop-up on the main menu, or when you open the game menu in a match _(Sol)_
- Illuminate, a free power everyone has: the last entry of the Force wheel (or the force_illuminate bind) turns on a holocron that floats by your shoulder, slowly turning, and lights the way in dark maps. Only you see it; Settings > Game > Illuminate holocron takes it off the wheel _(Sol)_
- A new profile starts at mouse sensitivity 5 again; the move to stock sensitivity units turned it into 13.022 _(Sol)_
- SJK UI Character: the saber style is three buttons and the hilts a list you can scroll and click (two side by side for dual sabers); the Force page groups the powers as classic+ does (Neutral, your side, Lightsaber) with classic+'s round level marks numbered with their cost, each group in its colour (silver, blue, red, green), bought levels glowing, a hovered level charging up with a spark and its price previewed on the points bar, a ring sent out when you buy one, and a box at the bottom right shows the power under the mouse in big with what it does _(Sol)_
- Saber hilt lists come in the same order as in JoF EJK (the game's load order, not alphabetical), in every menu style _(Sol, after JoF EJK)_
- The SJK UI is now the default menu style. Profiles still on the old default (classic) switch to it once when you update; pick Classic or Modern again and it stays _(Sol)_
- First setup offers the menu style as its first row: SJK, Classic or Modern, and First setup stays open in the style you pick _(Sol)_
- The quicksetup console command is now only `firstsetup`, so q completes to quit alone _(Sol)_
- The camera stays locked behind you by default, as in JoF EJK (camera style EJK). Profiles still on the old default (SJK, the camera that trails you a little) switch to it once when you update; pick SJK again and it stays _(Sol, after JoF EJK)_
- First setup opens with Styles: the menu style, then the camera style _(Sol)_
- Grabbing a wall before a wall jump, your body now stays facing the wall while the camera looks around (JA+ servers keep the camera free) _(Sol)_
- Report a player: the game menu lists everyone on the server, a small scoreboard with scores, pings and who is a verified SJK player (SJK UI: Players; classic and modern: SJK > Report a player). Choose a player, a reason and a few words, and the report goes to the SJK team with who, where and when. Only verified SJK players can send reports; the hub limits how often anyone reports and how often one player can be reported _(Sol)_
- The console has the SJK look with the SJK UI (`con_style` auto, or `con_style` sjk with any menus): a full-width panel with a header, an input band and key hints; the rest of a command shows faintly as you type, and a scroll bar. Commands and cvars (F3) gets the SJK look too, its text selectable with the mouse; Ctrl+C copies the selection or the entry _(Sol)_
- Video screens play: shaders that show a video (videoMap), such as the screens some servers put on consoles, play it looping instead of a magenta checker _(Sol)_
- Ceiling lamps and light strips that only looked painted glow: declared lights without a glow layer take emission maps (regenerate the material pack to see them) _(Sol)_
- Footsteps on sand, snow and grass the map does not mark as such sound like sand, snow and grass, as on Siege Desert's dunes _(Sol)_
- World notes name the shader a server's remap shows on the surface, not only the map's own _(Sol)_

## 2026.1007.3 (Alpha) | 07/10/2026

Lumaya's free camera, peek and first-person saber body; the SJK UI gains its server
browser, scoreboard, loading screen and in-game menu; ground fog keeps its height,
parallax returns with a depth slider, and the chat font is evenly spaced.

- Ground fog keeps its height: distant fog no longer jumps up and down as you step between roofs and streets or jump. It lies on the map's own floors, and past the map's walls as one level bank _(Sol)_
- SJK UI loading screen: joining a server keeps the menu's map behind the server's name and a gold line that fills step by step, then fades to the destination's picture with the map's name, the server, its rules and message of the day and a gold load bar along the bottom; a failed join says why in the same place _(Sol)_
- SJK UI scoreboard: with the SJK UI's menus, Tab shows SJK's own scoreboard, its columns floating over the darkened game: the map, mode, time left and your place in gold on top, the teams side by side under their scores, duelists as facing cards with their records, pings as signal bars. Settings > Scoreboard > Scoreboard style offers Auto (the new default: SJK's with the SJK UI, else classic), SJK, Classic and Modern; profiles move to Auto once, and a style you pick after that stays _(Sol)_
- Parallax is back at a tenth of its old depth, with a Parallax depth slider (and a Parallax mapping switch) in Settings > Graphics > Image: 0 flat, 1 the full depth of the maps; the slider shows its change at once _(Sol)_
- First person with a saber shows your arms and body holding it, as the original game does, instead of a hilt floating alone; the head stays hidden, and dying or following someone goes back to the old view _(Lumaya, after JoF EJK)_
- Peek: `/peek <id or name> [seconds]` (or `/peek` on the player under your crosshair) watches another player for a few seconds from behind, as JoF EJK's peek does, with the camera kept out of walls; `/peek off` returns _(Lumaya, after JoF EJK)_
- Free camera: `/freecam` (bindable under Key bindings > Other) flies the camera away from your body as JoF EJK's fake noclip does, while you stand still on the server with the chat balloon; actions are held back while flying, and death, spectating, a vehicle, a disconnect or a map change end it _(Lumaya, after JoF EJK)_
- Even letter spacing in the chat font: most letters sat against the left of their space while C, G, O and a few others sat against the right, leaving holes inside words. The menu and classic HUD fonts had the same fault, less visibly _(Sol)_
- Bug reports and world notes carry the name you play under, so the SJK team knows who sent them; the hub adds it to your identity's name history like the names it sees on servers _(Sol)_
- SJK UI Servers: the server browser over the map. Favourites and the filters for empty, full and locked servers and the game type sit on the left; the list sorts by any column and shows each server's name in its colours, its mod and signal bars; the chosen server shows its map's picture, its numbers and who is playing. The keyboard reaches everything, and the password and address prompts match the rest. Searching finds servers by their name without its colour codes _(Sol)_
- SJK UI: coloured names read on its dark ground: black shows as grey, and dark blue, red, green and magenta are lighter with the same hue _(Sol)_
- SJK UI in-game menu: Escape in a match keeps the match on screen with the menu on an arc down its left side (Resume, Team, Vote, Character, Settings, Servers, Shot controls, Sol JK, Leave) and a card on the right with the server, map, mode and limits, your score and place, the time left and who is playing. Settings and Servers open in the SJK UI over the match, and Escape always goes back to the entry you came from _(Sol)_

## 2026.1007.2 (Alpha) | 07/10/2026

The SJK UI arrives (Settings > Interface > Menu style > SJK) with its main page, Settings,
Character and update pages over a camera tour of duel6; world notes reach the SJK team;
vector fonts everywhere, modern settings, a camera style choice and working screenshots.

- World notes reach the SJK team: aim at a wall, floor or door, press the inspect key (X) twice and write what is wrong. With the identity on, the note goes to the SJK hub with where you stood, what you aimed at and a small screenshot, and a centre print says it arrived. Your copy stays on your PC in SJK/notes.txt as before; `cl_identity` 0 sends nothing _(Sol)_
- The SJK UI begins (Settings > Interface > Menu style > SJK): SJK's own menus over the live map, in the site's Rajdhani and Exo 2 type. Its main page: the emblem in a turning holo ring with the menu on an arc round it, and the servers you joined last on the right, each joined with one click (the JoF server until you have joined any). The screens it has no look for yet open in their classic one _(Sol)_
- SJK UI Settings: every category down a lit rail on the left, switches, sliders and side-by-side choices under sub-headings, the setting you are on explained on the right (what it does, its default and range, when it applies), and the search always at the top. The key bindings are one of its categories, each action with its two keys as caps; the one waiting for your key turns gold _(Sol)_
- SJK UI Character: your character stands on the map itself, on the path below the tower in the map's own light, and every change shows on them at once. Your name is the title; the model grid, sabers (colour chips and sliders, the saber thrown out to float before you) and Force powers sit in a column beside them _(Sol)_
- SJK UI What's new, Update and Identity: the releases down a lit rail with the chosen one's notes beside them, and the update and identity pages in the same look _(Sol)_
- SJK UI text sits centred in its buttons, key caps, fields and rows (it was drawn a few pixels high, twice that at 4K) _(Sol)_
- Behind the SJK UI the camera tours mp/duel6: ten slow shots of the map (the tower from the gardens and from its foot, the wings' rooms, the temple stair) in a new order every start, with a soft fade between them _(Sol)_
- Screenshots work again: `screenshot`, the quick wheel's screenshot and world notes' pictures no longer fail with "Screenshot readback failed", and a second screenshot no longer freezes the game _(Sol)_
- Settings look modern (classic menus): on/off settings are switches you click, numbers are slim sliders with their value in a box, short choices (menu style, contrast, console style) sit side by side, longer lists open from a field, labels read as words instead of all capitals, long groups have sub-headings, and a changed setting shows a gold dot and a reset arrow _(Sol)_
- A camera style setting (Settings > Gameplay > Game options and First setup, `cg_cameraStyle`): SJK, the default, keeps the third-person camera easing after you as before; EJK locks it behind you with no lag when you move or turn, as JoF EJK plays with its strafe helper on _(Sol, after JoF EJK)_
- The game's own fonts are vector fonts: menus, chat and the classic HUD keep their look and layout but stay sharp at any size, with no bitmap left; the menu and HUD fonts are traced from the JoF HD fonts, the chat font is OCR-A set to the game's spacing with its accents, and the WSI logo in names is sharp too _(Sol, fonts traced from the JoF HD Fonts & Icons pack, OCR-A by John Sauter)_

## 2026.1007.1 (Alpha) | 07/10/2026

Report bugs from inside the game, bring your config over from another client, quick
wheels on Q and R, saber blades that stop at walls, and a credits page with everyone's
work.

- Report a bug: a button at the bottom of the screen while the game menu is open (and Escape > SJK > Report a bug) opens a panel where you describe the problem and send it to the SJK hub, signed with your SJK identity, with the map, build and server; with the classic menus the panel and the button take the classic look, a gold button on a red band and an in-game pop-up _(Sol)_
- The credits page lists everything: each person has a panel with their name in large letters, their role and counts, and folds that open their highlights and all their work, every feature and pull request by day, each unfolding into its commits, with pull requests and commits opening on GitHub; golden god rays turn down from the top and a sunburst turns behind the emblem, as on the website _(Sol)_
- Settings groups have icons (classic menus): every group down the left of Key bindings, Options, Graphics and Gameplay shows a grey metal disc in the quick wheels' style, lit while the group is open or pointed at _(Sol)_
- The console draws with a sharp vector font, JetBrains Mono, instead of the old blocky bitmap font, on the same grid; so do the FPS counter, vote text and kill feed _(Sol)_
- Saber blades stop at walls: the glow and core end where the blade meets a wall instead of shining through it, and reach full length again as it leaves, also on hilts that leave no wall marks _(Lumaya)_
- Configs from other clients load: exec runs a .cfg whose name or binds use accented letters (it said "not valid UTF-8"), and a jampconfig.cfg copied over SJK's config.cfg loads with its unbindall and unknown keys instead of losing all your settings _(Sol)_
- First setup opens at every start until you tick "Don't show at start", and you can drop a .cfg from another client (jampconfig.cfg, EternalJK, JA+) on the window to import your name, model, field of view and key bindings, ticking the ones you want _(Sol)_
- The key under Escape opens the console on every keyboard layout, including Hungarian where it types 0; where it types ^ (German), Shift+^ opens it _(Sol, after EternalJK)_
- Melee kicks play smoothly: a game joined from the menu now predicts kicks and saber attacks instead of starting them a round trip late, and on JA+ servers JA+'s own spin, back and backflip kicks hold you still instead of pulling you back with every snapshot _(Creyon, after JoF EternalJK)_
- The quick wheels show icons: grey metal discs in the style of the game's Force icons, the one you point at grows in an accent ring with its name in the middle _(Sol)_
- Quick wheels: hold Q (general: third person, nameplates, HUD, screenshot, cosmetics, AFK, game menu, first setup) or R (weather: the map's own, drizzle, rain, storm, snow, ground fog, clouds, weather on/off), point the mouse at a choice and let go; Escape or letting go in the middle cancels (`+wheel general`, `+wheel weather`, Settings > Key bindings > Other) _(Sol)_
- Credits: Sol JK's own section, JKR's with Bishop, and every one of Creyon's merged pull requests; GitHub handles and the cards' links open in your browser when clicked _(Sol)_

## 2026.1006.1 (Alpha) | 06/10/2026

Weather, nameplates and the radial HUD arrive, SJK can now update itself, and your
movement reaches servers 125 times a second.

- Fixed a crash right after joining some games (FFA2, T2_Rogue on JA+ servers): a see-through player or afterimage on a map with material maps stopped the client _(Sol)_
- Settings regrouped (classic menus): Options now lists First setup (the old Quick setup), Graphics, Sound and Gameplay; Graphics holds Video, the renderer's Image, Lighting and Shadows and a new Weather group (also a Weather tab in the modern renderer settings); Gameplay holds Mouse, Game options, Interface, HUD, Scoreboard and Network _(Sol)_
- V cycles the nameplates: off, names only, bars on the player you aim at and your duel opponent, bars on everyone; the mode's name shows for a moment (`nameplates` command, Controls > Other) _(Sol)_
- Rain splashes are now 3D: a ring and a wet spot on the ground, a crown of water that rises and collapses, and drops thrown out on arcs, all placed in the world so they hold their shape from any angle; on water, widening ripples _(Sol)_
- The nameplate health estimate counts a JA+ grapple hook hit as exactly one point _(Sol)_
- The nameplate health estimate knows JA+'s chat protection: a player with the chat balloon up takes no damage unless swinging, kicking, punching or grabbing _(Sol)_
- Your HUD shows overheal and overshield like the nameplates: health and armour over 100 (125 at spawn, up to 199 armour) fill a thinner, deeper band inside their meter, in the radial, modern and classic HUDs _(Sol)_
- Weather, round two: rain splashes stay where they land instead of following you, and rain is a faint blue-grey instead of white; rain and snow leave a haze outdoors and the maps' fog becomes real ground fog that drifts with the wind (never indoors); 3D clouds drift over every map with sky, lit by its sun and darker in a storm; Settings > Renderer adds Weather quality (`r_weatherQuality`), Force weather (`r_weatherForce`: drizzle, rain, storm or snow on any map with sky), Ground fog (`r_weatherFog`) and Volumetric clouds (`r_clouds`) _(Sol)_
- Nameplate bars: shield on top, always shown (broken grey when empty), in your HUD's green, and health in its red; health and shield over the maximum (125 at spawn, up to 199 shield) show as a deeper second band; a yellow "?" stands over a bar the estimate cannot place yet; the estimate follows JA+ private duels (100 and 100 at the start, 100 and 25 for the winner) and the pain sounds servers play when they hide health _(Sol)_
- SJK identity needs nothing from you: your key is registered under the name you play with, and the SJK hub keeps the names you have worn (the Identity page shows them; the name field is gone, the bio stays optional); verified players get a gold badge after their name on nameplates, and `cg_nameplateSelf` shows your own nameplate in third person _(Sol)_
- Weather: maps with rain, snow, dust or blowing mist (T1_Rail, Hoth...) now show it; it stays under open sky, so it stops on roofs and the ground, never falls indoors and is cut at eaves and windows; rain splashes on the ground and ripples on water, a fainter far layer carries the storm into the distance, and the wind blows it; Settings > Renderer > Weather (`r_weather`) and its density (`r_weatherDensity`, 1 as the original game, 2 by default), and `r_we heavyrain` (or snow, fog, clear...) tries it on any map _(Sol)_
- The game goes silent while its window is alt-tabbed out or minimised, music included, and plays again when you come back; Settings > Sound > Mute in background (`snd_mute_losefocus`, on by default) turns it off _(Sol, after EternalJK)_
- Nameplates estimate the health and shield the server does not send for other players, from their spawns, pains, shield and saber hits, falls, pickups and Force heals (`cg_nameplatePredict`); every estimated bar, Force included, shows how unsure it is as a grey haze around its edge; the Force bar takes your HUD's Force colour; and the weapon a player holds shows left of the plate, a saber ringed in its stance's colour (`cg_nameplateWeapon`) _(Sol)_
- Your movement reaches the server 125 times a second at any frame rate, as with JoF EJK's `cl_cmdratecap`: a user command every 8 ms on the server's 8 ms grid instead of one every 25 ms, so quick taps are never merged or dropped (a `pmove_fixed` server loses commands from a client above 125 FPS) and you move in the same steps as a 125 FPS player; `cl_maxpackets` now sets how many packets a second carry them (default 125, 15 to 1000), `cl_packetdup` repeats earlier packets like the original game, and the flip kick counts one command per EJK frame again _(Sol, after JoF EJK)_
- New defaults for a new profile, an existing config keeps its values: the classic scoreboard (`cg_scoreboardStyle`) and the retail fonts (`ui_gameFont`) to go with the classic menus; the match timer and team overlay on (`cg_drawTimer`, `cg_drawTeamOverlay`); cut-off limbs shown on servers that enable dismemberment (`cg_dismember 2`); 120 snapshots a second asked of servers (`snaps`, the slider now goes to 125); and 30 FPS while the window has no focus (`com_maxfpsUnfocused`) _(Sol)_
- A radial HUD look, "SJK radial" (Settings > HUD > HUD look, or `cg_hudStyle radial`): health and armor as segmented arcs left of the crosshair, Force and ammunition right of it, low on the screen and sized with the window height, each on a rounded shadow that joins the pill carrying the numbers, which hug the bars; a saber's style is one full line in its colour (Fast blue, Medium yellow, Strong red, Dual green, Staff magenta); drawn by the engine, no HUD pack needed _(Sol, after TheRisqe's Radial HUD)_
- A console socket for external apps: `cl_consoleSocket <port>` streams the console, chat included, to a program on this computer and runs the commands it sends, as JoF EJK does, so Sol's Archive and chat tools work again; it needs `cl_consoleSocketPassword`, and `consolesocket` explains it _(Sol, after JoF EJK)_
- Nameplates show the holocron icons of the Force powers a player has on (lightning, grip, protect, absorb, speed and more) over the name when close (`cg_nameplateIcons`) _(Sol)_
- Protect and Absorb used together show one cyan shell on the body, as in single player (`cg_spProtAbsColor`, Settings > Game), instead of a green and a blue one _(Sol, after JoF EJK)_
- Nameplates like an online RPG over players: a small name from afar, and up close a framed plate with health, shield (teammates) and estimated Force bars under the name, in the classic font with its colours; shrink and fade with distance, ease out behind walls, anchored to the head, optional NPC plates; Settings > HUD has a row for each (`cg_nameplate*`); the plain `cg_drawPlayerNames` stays _(Sol)_
- SJK updates itself: it checks for a newer release at start (`cl_autoUpdate`), and main menu > Update (or the `update` command) downloads it, verifies it and installs it _(Sol)_
- A Changelog page in the main menu (and the `changelog` command) lists every release with its credits, in the classic+ look with the classic menus _(Sol)_
- The classic profile's Force page shows every power's holocron and level numbers again (Dark Rage and Team Energize were missing, and a hover hid a Lightning number) _(Sol)_
- A Credits page (main menu, the in-game SJK menu, or the `credits` command) shows the people who make Sol JK, animated, in retail colours with the classic menus _(Sol)_
- Classic menus: Controls and Setup are one Settings screen with KEY BINDINGS and OPTIONS tabs, every key binding in one list, a search field that finds any option or binding, and dropdowns for choices that change only when applied (Escape keeps the value); the main menu's SJK button gathers Changelog, Credits and Update _(Sol)_
- An SJK button left of About on the in-game bar (an SJK row in the modern game menu) opens SJK's own screens, starting with the changelog _(Sol)_
- A duel challenge from a player whose name has a symbol such as the multiplication sign no longer crashes the client, and the name shows that symbol instead of `?` _(Creyon)_
- Centre prints break only at a space, so a name with hidden codes stays on one row _(Creyon, after EternalJK)_
- The third-person camera follows fast moves, flips and rolls closely, damped per frame (`cg_cameraFPS`, default 125) instead of per 50 ms step _(Creyon, after EternalJK)_
- The crosshair is one of the game's crosshair pictures (`cg_drawCrosshair` 1 to 10, a 0-10 picker in Settings) instead of SJK's own "+" _(Creyon, after EternalJK)_
- Dismemberment (`cg_dismember`): cut-off limbs fly off with their cap and smoke; and bodies burn _(Creyon, after EternalJK)_
- The mouse wheel can be bound in Controls, so `flipkick` can go on it _(Creyon)_
- Small and far impact marks no longer show as black holes on walls, and scorch marks sit under explosions instead of over them _(Creyon)_
- Player animation fixes: runs and walks without a saber, a thrown saber's torso, the old Bryar's stance, with the unfixed attack animations predicted _(Creyon, after EternalJK)_
- Saber clashes no longer flash the whole screen _(Creyon)_
- Full servers (jaPRO) show every scoreboard row instead of repeating one name _(Creyon)_
- Symbols from the old Windows character set and hidden codes in names show as in the original game _(Creyon, after EternalJK)_
- Copying and pasting symbols on Windows keeps them instead of turning them into `?` _(Creyon)_
- JoF's HD effects load (a bare key no longer eats an effect block's closing brace), and images are tried as jpg, png then tga like the engine _(Creyon)_
- The first-person weapon follows `cg_fovViewmodel` _(Creyon, after EternalJK)_
- Zooming goes into first person, the HD scope mask is used, and camera shakes are measured from the view _(Creyon, after EternalJK)_
- The weapon selection row shows in every HUD style _(Creyon, after EternalJK)_
- Trip mine beams, placed charges lying flat, charge glow and the concussion beam are drawn _(Creyon, after EternalJK)_
- `fx_debug` and `cg_debugMissiles` diagnostics for effects and missiles _(Creyon)_
- With EternalJK installed, its character set (jaPRO's `charsgrid_med`) is used _(Creyon, after EternalJK)_
- `flipkick`: one press starts a run of jump taps for JA+ flip kicks (`cg_fkDuration`, `cg_fkFirstJumpDuration`, `cg_fkSecondJumpDelay`; bindable in Controls > Movement) _(Sol, after JoF EJK)_
- Player identity: SJK keeps an identity key and tells the SJK hub (sjk.dfox.app, `cl_hubUrl`) which game server and slot you play in, with your public key and in-game name, so other SJK players can see you; main menu > SJK > IDENTITY (or the in-game SJK menu, or the `identity` command) has a switch, your name and bio, and a button to copy your key id, and `cl_identity 0` stops it all _(Sol)_
- Player card: look at a player with a steady view for 1.5 seconds (`cg_playerCard`, `cg_playerCardDelay`) and a card beside them shows their model, sabers and duel record, and for SJK hub players the SJK emblem, hub name and a gold VERIFIED; the scoreboard marks hub players with the emblem too _(Sol)_

## 2026.1005.1 (Alpha) | 05/10/2026

Releases are now numbered by date and a counter. The top of the screen shows
the version, date and commit of your build (`cg_drawVersion 0` hides it).

- Classic+ menus: setting panels with a description box, marks on changed values and Backspace for the default, grouped pages, pictures for key bindings and a classic renderer page _(Sol)_
- A HUD picker that previews every installed HUD, the game's own included; new configs start on the game's HUD _(Sol)_
- Player profile: every model's icon in the grid, model search and a live model preview _(Sol)_
- The classic Force page with saved templates and readable costs _(Sol, after JoF EJK)_
- Lightsaber creation shows the saber alone in 3D, with RGB colour sliders _(Sol, after JoF EJK)_
- The Force wheel, with Stasis, Repulse and Dash on JoF servers, and the weapon selection row _(Sol, after JoF EJK)_
- Hats and capes, open to everyone (`cosmetics` window; needs the JoF cosmetics pack) _(Sol, after JoF EJK)_
- The console key also closes the console _(Sol, after EternalJK)_
- The F3 command browser has the classic look with the classic console _(Sol)_
- Talk balloons no longer vanish in busy fights, and yours shows while you are alt-tabbed or minimised _(Sol, after EternalJK)_
- Rocket trails no longer disappear when many rockets fly at once _(Sol)_
- Alt codes: hold Alt and type a number on the numeric keypad in the console, chat and menus _(Sol)_
- Accented letters you type are readable by EternalJK and retail players (sent as Windows-1252) _(Sol)_
- A player whose model you don't have shows as Kyle instead of keeping their previous model _(Sol)_
- A negative score shows as negative, and melee no longer shows a stun baton in first person _(Sol)_
- On servers that add map pieces, the camera no longer goes through their walls, and server props use their scale _(Sol)_
- Shader remaps use JKR's implementation (JKR #127), with worldspawn remaps, effects that follow remaps and `clearRemaps` (JKR #128-#131) _(Bishop; Sol)_
- Multiplayer camera and vehicle behaviour, and burrowing sand creatures on SJK's own server (JKR #124-#126) _(Bishop)_
- Map items named with a leading slash, as on some JoF maps, load (JKR #133) _(Sol)_
- The modern main menu says SJK, and the client shows its copyright and licence notice at startup _(Sol)_

## 0.1.0-alpha.2 | 05/10/2026

The programs are now `sjk` and `sjk-server`, and settings live in
`GameData/SJK/` (alpha.1 settings are imported once; old `jkr_*` setting
names keep working).

- SJK's own name and emblem: program and window icons, and the emblem in the main menu _(Sol)_
- A classic console, now the default (`con_style modern` brings back the other one) _(Sol, after EternalJK)_
- Truer colours: text and menus are drawn the way retail draws them, with retail's `^` colour codes _(Sol)_
- Dynamic glow for glowing skins and sabers (`r_DynamicGlow`) _(Sol, after EternalJK)_
- Eye adaptation: brightness follows dark rooms and bright light (`r_autoExposure`) _(Sol)_
- Your own Force powers show (Drain, Lightning, Grip, Push), Drain's bolts have their retail shape, and Mind Trick hides the trickster _(Sol)_
- Many more custom player models load instead of falling back to Kyle _(Sol)_
- Shader remaps sent by servers and maps (`cg_remaps`) _(Sol)_
- With a generated material pack: reflections on metal, normal-mapped lamp light, mapped mirror floors and glowing emission maps _(Sol)_
- New defaults: sun at noon, bloom and dust in sunbeams _(Sol)_

## 0.1.0-alpha.1 | 04/10/2026

SJK's first public build: Bishop's JKR with Sol's changes, played and tested
by a small group.

- JKR itself: the Rust engine, the wgpu renderer and its lighting, the native client and dedicated server, protocol 26 networking, prediction, the Jedi Academy game rules, content loading, console, menus, HUD, server browser and demos _(Bishop)_
- Drop-in installs with portable settings, distributable ZIPs and actor animation fixes (JKR #103, #104, #112) _(Bishop)_
- The classic menu style as the default, the classic scoreboard and game-data HUDs, retail game fonts, text spacing and high-resolution scaling _(Sol)_
- JA+ servers: plugin identity, `serverconfig`, JA+ movement and saber rules, the grapple and duel pass-through _(Sol)_
- Reliable joining, old player models, material maps and the renderer settings page _(Sol)_
- Key handling: layout key names, dead keys, locked binds and capital key names _(Sol)_
- Sharp levelshots, saber trails, the third-person camera, Force Speed afterimages, death animations and dust motes _(Sol)_
- The `debug_panel` test list of each build's changes _(Sol)_
