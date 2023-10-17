Installation:
Unzip this file directly into your Gamedata folder, replacing any conflicting files.

If you had EternalJK previously installed, you'll want to go inside GameData/EternalJK and delete the japro-win-x86.pk3 file. It is being replaced by the jofclient-win-x86.pk3.

Good to know:
cg_drawRewards - controls the "Awards!" that are spammed when getting multiple kills.

Changelog:

1.4.5CF - Crash Fix:
- Stops the client from crashing from the recent spoofing attacks.

1.4.4:
- Fixes a bug where the game crashes when attempting to load too many sound files (hard limit of 512 pk3 files still exists)
- Fixed a bug where area portals didn't properly work in the eternaljk renderer.
- Fixed bug where cg_fpls 1 and exiting first person on a vehicle would cause the client to crash
- Fixed bugged scoreboard on basejka/non japlus/japro mods
- Fixed bug where people using female skins you didn't have would show as kyle instead of jan
- Added support for Sunny's Vulkan Renderer - https://github.com/jksunny/eternaljk
	- usage: "/cl_renderer rd-vulkan" then "/vid_restart". to switch back do the same but with "rd-eternaljk"

1.4.3:
- Fixes the laggy npc's back to original standard.
- Enables cg_antiamkiss on Japlus servers, this will allow you to instantly slap anyone who tries to kiss you.
- fixes a bug with cg_enableForceMenu where it couldn't be toggled on and off.
- enables /clientlistinfo console command, which lets you see a concentrated list of info about other clients, in the same way /configstrings does.
- Adds the fix for the model crash bug.
- Opening the console will now let you use your mouse freely.


Credits:
- The OpenJK contributors
- The EternalJK contributors, especially Bucky for the most up to date changes used by JoF
- The JoF code team contributors - Milamber and Daniel