Installation:
Unzip this file directly into your Gamedata folder, replacing any conflicting files.

If you had EternalJK previously installed, you'll want to go inside GameData/EternalJK and delete the japro-win-x86.pk3 file. It is being replaced by the jofclient-win-x86.pk3. There might be updates to japro-assets.pk3 as well so it's recommended to replace this one too.



Changelog:

1.5.5:
- Fixes npc lag introduced by 1.5.3
- Removes heal FX when moving
- Fixes prediction errors when spectating a player riding a npc vehicle
- Sunny Vulkan update to 25.06.25 release

1.5.4:
- Hotfix heal effect missing registration
- Hotfix potential loading client info issue.

1.5.3:
- increase number of animation files, fixes some issues with npcs.
- increase filelist in profile view buffer size by 4 times.
- increased some G2 buffers
- Fixed a crash where trying to copy paste a certain character would cause an infinite loop.
- Fixed a crash related to cg_defaultModelRandom where regardless of 0/1 it could cause clients to crash on team games.
- Cleaned up some clientside prediction code
- Fixed old crash with cg_sabertrail 3
- Added a clientside implementation for heal effects - by Jediman
- Added amTeleOffset x y z added, supports using only x, x and y, or all 3 - by Clix
- Added cg_hideDuelerNames for hiding name plates of duelers - by Clix
- updated cg_saberHum to be able set 1-5 specific hums, 0 chooses random for that player - by Clix
- Added an initial version of a soundBuffer command s_maxSounds 50 where you can set the number of simoultanous sounds that can play within 1 second - by Clix.
- Added menu option to toggle free selection of force powers - by Toxiee
- Enabled team forces to be selected in non-team gamemodes if the server allows them - by Toxiee


1.5.2:
- cg_drawRewards changed to default off.
- cg_dismember changed to default 3 max dismemberment.
- fixes crash bug when cg_defaultModelRandom is enabled.
- fix for black icons bug in character selection.
- character selection UI now has an option to disable skins without icons.
- when whispering people it will now show you the name of your target
- messagemode5 added to let you reply to the last person who whispered to you. /reply <message> also works.
- cg_adminPers added will let you keep say_team_mod admin set through map changes.
- Added chat shortcuts: Say %U% for saying your speed, %UKM% for kilometres, %UM% for miles.
- cg_spprotabscolor 1 added, makes absorb + protect behave like in SP.
- can now spectate in first person when cg_fpls is enabled
- r_flares is now 0 by default for performance reasons.
- August 4th 2024 official release of Sunny's vulkan renderer has been added.

1.5.1:
- fix bug with scoreboard not showing deaths

1.5.0:
- Removes the old spoof fix implementation and adds a new one that is a proper anti spoofing fix. Only works on servers that have the corresponding serverside implementation, currently only JoF server. The client will still work on other servers.
- cl_antiSpoof 1 added, setting this to 0 and restarting will turn off the anti spoof system completely.
- cl_discordRichPresenceSharePassword 1 default value has been changed to 0.
- September 5th official release of Sunny's vulkan renderer has been added.

1.4.7:
- Added Seasonal cosmetics
- Added cg_VoteBeep to specifically be able to disable the voting beep noise without turning off all chat noise with cg_chatSounds.

1.4.6:
- Missed one "CL_ParseServerMEssage: Illegible server message" now also fixed.

1.4.5 - Crash Fix:
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
- The JoF code team contributors - Milamber, Daniel and Jediman