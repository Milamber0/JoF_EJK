//! The names a script `set`s and `get`s (`setType_e`, `icarus/Q3_Interface.h`) and the
//! table the game looks them up in (`setTable`, `g_ICARUScb.c:70-282`).
//!
//! Generated from the reference. The table is not the whole enum: four names the enum
//! has (`SET_LOSTENEMYSCRIPT`, `SET_MISSIONSTATUSACTIVE`, `SET_SECRET_AREA_FOUND`,
//! `SET_OBJECTIVEFOSTER`) are missing from it, so the game never finds them and a script
//! setting one sets a variable of that name instead (`Q3_Set`'s default case).

/// `SET_PARM1`: Set entity parm1.
pub const SET_PARM1: i32 = 0;
/// `SET_PARM2`: Set entity parm2.
pub const SET_PARM2: i32 = 1;
/// `SET_PARM3`: Set entity parm3.
pub const SET_PARM3: i32 = 2;
/// `SET_PARM4`: Set entity parm4.
pub const SET_PARM4: i32 = 3;
/// `SET_PARM5`: Set entity parm5.
pub const SET_PARM5: i32 = 4;
/// `SET_PARM6`: Set entity parm6.
pub const SET_PARM6: i32 = 5;
/// `SET_PARM7`: Set entity parm7.
pub const SET_PARM7: i32 = 6;
/// `SET_PARM8`: Set entity parm8.
pub const SET_PARM8: i32 = 7;
/// `SET_PARM9`: Set entity parm9.
pub const SET_PARM9: i32 = 8;
/// `SET_PARM10`: Set entity parm10.
pub const SET_PARM10: i32 = 9;
/// `SET_PARM11`: Set entity parm11.
pub const SET_PARM11: i32 = 10;
/// `SET_PARM12`: Set entity parm12.
pub const SET_PARM12: i32 = 11;
/// `SET_PARM13`: Set entity parm13.
pub const SET_PARM13: i32 = 12;
/// `SET_PARM14`: Set entity parm14.
pub const SET_PARM14: i32 = 13;
/// `SET_PARM15`: Set entity parm15.
pub const SET_PARM15: i32 = 14;
/// `SET_PARM16`: Set entity parm16.
pub const SET_PARM16: i32 = 15;
/// `SET_SPAWNSCRIPT`: Script to run when spawned //0 - do not change these, these are equal to BSET_SPAWN, etc.
pub const SET_SPAWNSCRIPT: i32 = 16;
/// `SET_USESCRIPT`: Script to run when used.
pub const SET_USESCRIPT: i32 = 17;
/// `SET_AWAKESCRIPT`: Script to run when startled.
pub const SET_AWAKESCRIPT: i32 = 18;
/// `SET_ANGERSCRIPT`: Script run when find an enemy for the first time.
pub const SET_ANGERSCRIPT: i32 = 19;
/// `SET_ATTACKSCRIPT`: Script to run when you shoot.
pub const SET_ATTACKSCRIPT: i32 = 20;
/// `SET_VICTORYSCRIPT`: Script to run when killed someone.
pub const SET_VICTORYSCRIPT: i32 = 21;
/// `SET_LOSTENEMYSCRIPT`: Script to run when you can't find your enemy.
pub const SET_LOSTENEMYSCRIPT: i32 = 22;
/// `SET_PAINSCRIPT`: Script to run when hit.
pub const SET_PAINSCRIPT: i32 = 23;
/// `SET_FLEESCRIPT`: Script to run when hit and low health.
pub const SET_FLEESCRIPT: i32 = 24;
/// `SET_DEATHSCRIPT`: Script to run when killed.
pub const SET_DEATHSCRIPT: i32 = 25;
/// `SET_DELAYEDSCRIPT`: Script to run after a delay.
pub const SET_DELAYEDSCRIPT: i32 = 26;
/// `SET_BLOCKEDSCRIPT`: Script to run when blocked by teammate.
pub const SET_BLOCKEDSCRIPT: i32 = 27;
/// `SET_FFIRESCRIPT`: Script to run when player has shot own team repeatedly.
pub const SET_FFIRESCRIPT: i32 = 28;
/// `SET_FFDEATHSCRIPT`: Script to run when player kills a teammate.
pub const SET_FFDEATHSCRIPT: i32 = 29;
/// `SET_MINDTRICKSCRIPT`: Script to run when player kills a teammate.
pub const SET_MINDTRICKSCRIPT: i32 = 30;
/// `SET_VIDEO_PLAY`: Play a video (inGame).
pub const SET_VIDEO_PLAY: i32 = 31;
/// `SET_CINEMATIC_SKIPSCRIPT`: Script to run when skipping the running cinematic.
pub const SET_CINEMATIC_SKIPSCRIPT: i32 = 32;
/// `SET_ENEMY`: Set enemy by targetname.
pub const SET_ENEMY: i32 = 33;
/// `SET_LEADER`: Set for BS_FOLLOW_LEADER.
pub const SET_LEADER: i32 = 34;
/// `SET_NAVGOAL`: *Move to this navgoal then continue script.
pub const SET_NAVGOAL: i32 = 35;
/// `SET_CAPTURE`: Set captureGoal by targetname.
pub const SET_CAPTURE: i32 = 36;
/// `SET_VIEWTARGET`: Set angles toward ent by targetname.
pub const SET_VIEWTARGET: i32 = 37;
/// `SET_WATCHTARGET`: Set angles toward ent by targetname, will *continue* to face them... only in BS_CINEMATIC.
pub const SET_WATCHTARGET: i32 = 38;
/// `SET_TARGETNAME`: Set/change your targetname.
pub const SET_TARGETNAME: i32 = 39;
/// `SET_PAINTARGET`: Set/change what to use when hit.
pub const SET_PAINTARGET: i32 = 40;
/// `SET_CAMERA_GROUP`: all ents with this cameraGroup will be focused on.
pub const SET_CAMERA_GROUP: i32 = 41;
/// `SET_CAMERA_GROUP_TAG`: What tag on all clients to try to track.
pub const SET_CAMERA_GROUP_TAG: i32 = 42;
/// `SET_LOOK_TARGET`: object for NPC to look at.
pub const SET_LOOK_TARGET: i32 = 43;
/// `SET_ADDRHANDBOLT_MODEL`: object to place on NPC right hand bolt.
pub const SET_ADDRHANDBOLT_MODEL: i32 = 44;
/// `SET_REMOVERHANDBOLT_MODEL`: object to remove from NPC right hand bolt.
pub const SET_REMOVERHANDBOLT_MODEL: i32 = 45;
/// `SET_ADDLHANDBOLT_MODEL`: object to place on NPC left hand bolt.
pub const SET_ADDLHANDBOLT_MODEL: i32 = 46;
/// `SET_REMOVELHANDBOLT_MODEL`: object to remove from NPC left hand bolt.
pub const SET_REMOVELHANDBOLT_MODEL: i32 = 47;
/// `SET_CAPTIONTEXTCOLOR`: Color of text RED,WHITE,BLUE, YELLOW.
pub const SET_CAPTIONTEXTCOLOR: i32 = 48;
/// `SET_CENTERTEXTCOLOR`: Color of text RED,WHITE,BLUE, YELLOW.
pub const SET_CENTERTEXTCOLOR: i32 = 49;
/// `SET_SCROLLTEXTCOLOR`: Color of text RED,WHITE,BLUE, YELLOW.
pub const SET_SCROLLTEXTCOLOR: i32 = 50;
/// `SET_COPY_ORIGIN`: Copy the origin of the ent with targetname to your origin.
pub const SET_COPY_ORIGIN: i32 = 51;
/// `SET_DEFEND_TARGET`: This NPC will attack the target NPC's enemies.
pub const SET_DEFEND_TARGET: i32 = 52;
/// `SET_TARGET`: Set/change your target.
pub const SET_TARGET: i32 = 53;
/// `SET_TARGET2`: Set/change your target2, on NPC's, this fires when they're knocked out by the red hypo.
pub const SET_TARGET2: i32 = 54;
/// `SET_LOCATION`: What trigger_location you're in - Can only be gotten, not set!.
pub const SET_LOCATION: i32 = 55;
/// `SET_REMOVE_TARGET`: Target that is fired when someone completes the BS_REMOVE behaviorState.
pub const SET_REMOVE_TARGET: i32 = 56;
/// `SET_LOADGAME`: Load the savegame that was auto-saved when you started the holodeck.
pub const SET_LOADGAME: i32 = 57;
/// `SET_LOCKYAW`: Lock legs to a certain yaw angle (or "off" or "auto" uses current).
pub const SET_LOCKYAW: i32 = 58;
/// `SET_FULLNAME`: This name will appear when ent is scanned by tricorder.
pub const SET_FULLNAME: i32 = 59;
/// `SET_VIEWENTITY`: Make the player look through this ent's eyes - also shunts player movement control to this ent.
pub const SET_VIEWENTITY: i32 = 60;
/// `SET_LOOPSOUND`: Looping sound to play on entity.
pub const SET_LOOPSOUND: i32 = 61;
/// `SET_ICARUS_FREEZE`: Specify name of entity to freeze - !!!NOTE!!! since the ent is frozen, it cannot unfreeze itself, you must have some other entity unfreeze a frozen ent!!!.
pub const SET_ICARUS_FREEZE: i32 = 62;
/// `SET_ICARUS_UNFREEZE`: Specify name of entity to unfreeze - !!!NOTE!!! since the ent is frozen, it cannot unfreeze itself, you must have some other entity unfreeze a frozen ent!!!.
pub const SET_ICARUS_UNFREEZE: i32 = 63;
/// `SET_SCROLLTEXT`: key of text string to print.
pub const SET_SCROLLTEXT: i32 = 64;
/// `SET_LCARSTEXT`: key of text string to print in LCARS frame.
pub const SET_LCARSTEXT: i32 = 65;
/// `SET_ORIGIN`: Set origin explicitly or with TAG.
pub const SET_ORIGIN: i32 = 66;
/// `SET_ANGLES`: Set angles explicitly or with TAG.
pub const SET_ANGLES: i32 = 67;
/// `SET_TELEPORT_DEST`: Set origin here as soon as the area is clear.
pub const SET_TELEPORT_DEST: i32 = 68;
/// `SET_XVELOCITY`: Velocity along X axis.
pub const SET_XVELOCITY: i32 = 69;
/// `SET_YVELOCITY`: Velocity along Y axis.
pub const SET_YVELOCITY: i32 = 70;
/// `SET_ZVELOCITY`: Velocity along Z axis.
pub const SET_ZVELOCITY: i32 = 71;
/// `SET_Z_OFFSET`: Vertical offset from original origin... offset/ent's speed * 1000ms is duration.
pub const SET_Z_OFFSET: i32 = 72;
/// `SET_DPITCH`: Pitch for NPC to turn to.
pub const SET_DPITCH: i32 = 73;
/// `SET_DYAW`: Yaw for NPC to turn to.
pub const SET_DYAW: i32 = 74;
/// `SET_TIMESCALE`: Speed-up slow down game (0 - 1.0).
pub const SET_TIMESCALE: i32 = 75;
/// `SET_CAMERA_GROUP_Z_OFS`: when following an ent with the camera, apply this z ofs.
pub const SET_CAMERA_GROUP_Z_OFS: i32 = 76;
/// `SET_VISRANGE`: How far away NPC can see.
pub const SET_VISRANGE: i32 = 77;
/// `SET_EARSHOT`: How far an NPC can hear.
pub const SET_EARSHOT: i32 = 78;
/// `SET_VIGILANCE`: How often to look for enemies (0 - 1.0).
pub const SET_VIGILANCE: i32 = 79;
/// `SET_GRAVITY`: Change this ent's gravity - 800 default.
pub const SET_GRAVITY: i32 = 80;
/// `SET_FACEAUX`: Set face to Aux expression for number of seconds.
pub const SET_FACEAUX: i32 = 81;
/// `SET_FACEBLINK`: Set face to Blink expression for number of seconds.
pub const SET_FACEBLINK: i32 = 82;
/// `SET_FACEBLINKFROWN`: Set face to Blinkfrown expression for number of seconds.
pub const SET_FACEBLINKFROWN: i32 = 83;
/// `SET_FACEFROWN`: Set face to Frown expression for number of seconds.
pub const SET_FACEFROWN: i32 = 84;
/// `SET_FACENORMAL`: Set face to Normal expression for number of seconds.
pub const SET_FACENORMAL: i32 = 85;
/// `SET_FACEEYESCLOSED`: Set face to Eyes closed.
pub const SET_FACEEYESCLOSED: i32 = 86;
/// `SET_FACEEYESOPENED`: Set face to Eyes open.
pub const SET_FACEEYESOPENED: i32 = 87;
/// `SET_WAIT`: Change an entity's wait field.
pub const SET_WAIT: i32 = 88;
/// `SET_FOLLOWDIST`: How far away to stay from leader in BS_FOLLOW_LEADER.
pub const SET_FOLLOWDIST: i32 = 89;
/// `SET_SCALE`: Scale the entity model.
pub const SET_SCALE: i32 = 90;
/// `SET_ANIM_HOLDTIME_LOWER`: Hold lower anim for number of milliseconds.
pub const SET_ANIM_HOLDTIME_LOWER: i32 = 91;
/// `SET_ANIM_HOLDTIME_UPPER`: Hold upper anim for number of milliseconds.
pub const SET_ANIM_HOLDTIME_UPPER: i32 = 92;
/// `SET_ANIM_HOLDTIME_BOTH`: Hold lower and upper anims for number of milliseconds.
pub const SET_ANIM_HOLDTIME_BOTH: i32 = 93;
/// `SET_HEALTH`: Change health.
pub const SET_HEALTH: i32 = 94;
/// `SET_ARMOR`: Change armor.
pub const SET_ARMOR: i32 = 95;
/// `SET_WALKSPEED`: Change walkSpeed.
pub const SET_WALKSPEED: i32 = 96;
/// `SET_RUNSPEED`: Change runSpeed.
pub const SET_RUNSPEED: i32 = 97;
/// `SET_YAWSPEED`: Change yawSpeed.
pub const SET_YAWSPEED: i32 = 98;
/// `SET_AGGRESSION`: Change aggression 1-5.
pub const SET_AGGRESSION: i32 = 99;
/// `SET_AIM`: Change aim 1-5.
pub const SET_AIM: i32 = 100;
/// `SET_FRICTION`: Change ent's friction - 6 default.
pub const SET_FRICTION: i32 = 101;
/// `SET_SHOOTDIST`: How far the ent can shoot - 0 uses weapon.
pub const SET_SHOOTDIST: i32 = 102;
/// `SET_HFOV`: Horizontal field of view.
pub const SET_HFOV: i32 = 103;
/// `SET_VFOV`: Vertical field of view.
pub const SET_VFOV: i32 = 104;
/// `SET_DELAYSCRIPTTIME`: How many milliseconds to wait before running delayscript.
pub const SET_DELAYSCRIPTTIME: i32 = 105;
/// `SET_FORWARDMOVE`: NPC move forward -127(back) to 127.
pub const SET_FORWARDMOVE: i32 = 106;
/// `SET_RIGHTMOVE`: NPC move right -127(left) to 127.
pub const SET_RIGHTMOVE: i32 = 107;
/// `SET_STARTFRAME`: frame to start animation sequence on.
pub const SET_STARTFRAME: i32 = 108;
/// `SET_ENDFRAME`: frame to end animation sequence on.
pub const SET_ENDFRAME: i32 = 109;
/// `SET_ANIMFRAME`: frame to set animation sequence to.
pub const SET_ANIMFRAME: i32 = 110;
/// `SET_COUNT`: Change an entity's count field.
pub const SET_COUNT: i32 = 111;
/// `SET_SHOT_SPACING`: Time between shots for an NPC - reset to defaults when changes weapon.
pub const SET_SHOT_SPACING: i32 = 112;
/// `SET_MISSIONSTATUSTIME`: Amount of time until Mission Status should be shown after death.
pub const SET_MISSIONSTATUSTIME: i32 = 113;
/// `SET_WIDTH`: Width of NPC bounding box.
pub const SET_WIDTH: i32 = 114;
/// `SET_IGNOREPAIN`: Do not react to pain.
pub const SET_IGNOREPAIN: i32 = 115;
/// `SET_IGNOREENEMIES`: Do not acquire enemies.
pub const SET_IGNOREENEMIES: i32 = 116;
/// `SET_IGNOREALERTS`: Do not get enemy set by allies in area(ambush).
pub const SET_IGNOREALERTS: i32 = 117;
/// `SET_DONTSHOOT`: Others won't shoot you.
pub const SET_DONTSHOOT: i32 = 118;
/// `SET_NOTARGET`: Others won't pick you as enemy.
pub const SET_NOTARGET: i32 = 119;
/// `SET_DONTFIRE`: Don't fire your weapon.
pub const SET_DONTFIRE: i32 = 120;
/// `SET_LOCKED_ENEMY`: Keep current enemy until dead.
pub const SET_LOCKED_ENEMY: i32 = 121;
/// `SET_CROUCHED`: Force NPC to crouch.
pub const SET_CROUCHED: i32 = 122;
/// `SET_WALKING`: Force NPC to move at walkSpeed.
pub const SET_WALKING: i32 = 123;
/// `SET_RUNNING`: Force NPC to move at runSpeed.
pub const SET_RUNNING: i32 = 124;
/// `SET_CHASE_ENEMIES`: NPC will chase after enemies.
pub const SET_CHASE_ENEMIES: i32 = 125;
/// `SET_LOOK_FOR_ENEMIES`: NPC will be on the lookout for enemies.
pub const SET_LOOK_FOR_ENEMIES: i32 = 126;
/// `SET_FACE_MOVE_DIR`: NPC will face in the direction it's moving.
pub const SET_FACE_MOVE_DIR: i32 = 127;
/// `SET_DONT_FLEE`: NPC will not run from danger.
pub const SET_DONT_FLEE: i32 = 128;
/// `SET_FORCED_MARCH`: NPC will not move unless you aim at him.
pub const SET_FORCED_MARCH: i32 = 129;
/// `SET_UNDYING`: Can take damage down to 1 but not die.
pub const SET_UNDYING: i32 = 130;
/// `SET_NOAVOID`: Will not avoid other NPCs or architecture.
pub const SET_NOAVOID: i32 = 131;
/// `SET_SOLID`: Make yourself notsolid or solid.
pub const SET_SOLID: i32 = 132;
/// `SET_PLAYER_USABLE`: Can be activateby the player's "use" button.
pub const SET_PLAYER_USABLE: i32 = 133;
/// `SET_LOOP_ANIM`: For non-NPCs, loop your animation sequence.
pub const SET_LOOP_ANIM: i32 = 134;
/// `SET_INTERFACE`: Player interface on/off.
pub const SET_INTERFACE: i32 = 135;
/// `SET_SHIELDS`: NPC has no shields (Borg do not adapt).
pub const SET_SHIELDS: i32 = 136;
/// `SET_INVISIBLE`: Makes an NPC not solid and not visible.
pub const SET_INVISIBLE: i32 = 137;
/// `SET_VAMPIRE`: Draws only in mirrors/portals.
pub const SET_VAMPIRE: i32 = 138;
/// `SET_FORCE_INVINCIBLE`: Force Invincibility effect, also godmode.
pub const SET_FORCE_INVINCIBLE: i32 = 139;
/// `SET_GREET_ALLIES`: Makes an NPC greet teammates.
pub const SET_GREET_ALLIES: i32 = 140;
/// `SET_VIDEO_FADE_IN`: Makes video playback fade in.
pub const SET_VIDEO_FADE_IN: i32 = 141;
/// `SET_VIDEO_FADE_OUT`: Makes video playback fade out.
pub const SET_VIDEO_FADE_OUT: i32 = 142;
/// `SET_PLAYER_LOCKED`: Makes it so player cannot move.
pub const SET_PLAYER_LOCKED: i32 = 143;
/// `SET_LOCK_PLAYER_WEAPONS`: Makes it so player cannot switch weapons.
pub const SET_LOCK_PLAYER_WEAPONS: i32 = 144;
/// `SET_NO_IMPACT_DAMAGE`: Stops this ent from taking impact damage.
pub const SET_NO_IMPACT_DAMAGE: i32 = 145;
/// `SET_NO_KNOCKBACK`: Stops this ent from taking knockback from weapons.
pub const SET_NO_KNOCKBACK: i32 = 146;
/// `SET_ALT_FIRE`: Force NPC to use altfire when shooting.
pub const SET_ALT_FIRE: i32 = 147;
/// `SET_NO_RESPONSE`: NPCs will do generic responses when this is on (usescripts override generic responses as well).
pub const SET_NO_RESPONSE: i32 = 148;
/// `SET_INVINCIBLE`: Completely unkillable.
pub const SET_INVINCIBLE: i32 = 149;
/// `SET_MISSIONSTATUSACTIVE`.
pub const SET_MISSIONSTATUSACTIVE: i32 = 150;
/// `SET_NO_COMBAT_TALK`: NPCs will not do their combat talking noises when this is on.
pub const SET_NO_COMBAT_TALK: i32 = 151;
/// `SET_NO_ALERT_TALK`: NPCs will not do their combat talking noises when this is on.
pub const SET_NO_ALERT_TALK: i32 = 152;
/// `SET_TREASONED`: Player has turned on his own- scripts will stop, NPCs will turn on him and level changes load the brig.
pub const SET_TREASONED: i32 = 153;
/// `SET_DISABLE_SHADER_ANIM`: Allows turning off an animating shader in a script.
pub const SET_DISABLE_SHADER_ANIM: i32 = 154;
/// `SET_SHADER_ANIM`: Sets a shader with an image map to be under frame control.
pub const SET_SHADER_ANIM: i32 = 155;
/// `SET_SABERACTIVE`: Turns saber on/off.
pub const SET_SABERACTIVE: i32 = 156;
/// `SET_ADJUST_AREA_PORTALS`: Only set this on things you move with script commands that you *want* to open/close area portals.  Default is off.
pub const SET_ADJUST_AREA_PORTALS: i32 = 157;
/// `SET_DMG_BY_HEAVY_WEAP_ONLY`: When true, only a heavy weapon class missile/laser can damage this ent.
pub const SET_DMG_BY_HEAVY_WEAP_ONLY: i32 = 158;
/// `SET_SHIELDED`: When true, ion_cannon is shielded from any kind of damage.
pub const SET_SHIELDED: i32 = 159;
/// `SET_NO_GROUPS`: This NPC cannot alert groups or be part of a group.
pub const SET_NO_GROUPS: i32 = 160;
/// `SET_FIRE_WEAPON`: Makes NPC will hold down the fire button, until this is set to false.
pub const SET_FIRE_WEAPON: i32 = 161;
/// `SET_NO_MINDTRICK`: Makes NPC immune to jedi mind-trick.
pub const SET_NO_MINDTRICK: i32 = 162;
/// `SET_INACTIVE`: in lieu of using a target_activate or target_deactivate.
pub const SET_INACTIVE: i32 = 163;
/// `SET_FUNC_USABLE_VISIBLE`: provides an alternate way of changing func_usable to be visible or not, DOES NOT AFFECT SOLID.
pub const SET_FUNC_USABLE_VISIBLE: i32 = 164;
/// `SET_SECRET_AREA_FOUND`: Increment secret areas found counter.
pub const SET_SECRET_AREA_FOUND: i32 = 165;
/// `SET_MISSION_STATUS_SCREEN`: Display Mission Status screen before advancing to next level.
pub const SET_MISSION_STATUS_SCREEN: i32 = 166;
/// `SET_END_SCREENDISSOLVE`: End of game dissolve into star background and credits.
pub const SET_END_SCREENDISSOLVE: i32 = 167;
/// `SET_USE_CP_NEAREST`: NPCs will use their closest combat points, not try and find ones next to the player, or flank player.
pub const SET_USE_CP_NEAREST: i32 = 168;
/// `SET_MORELIGHT`: NPC will have a minlight of 96.
pub const SET_MORELIGHT: i32 = 169;
/// `SET_NO_FORCE`: NPC will not be affected by force powers.
pub const SET_NO_FORCE: i32 = 170;
/// `SET_NO_FALLTODEATH`: NPC will not scream and tumble and fall to hit death over large drops.
pub const SET_NO_FALLTODEATH: i32 = 171;
/// `SET_DISMEMBERABLE`: NPC will not be dismemberable if you set this to false (default is true).
pub const SET_DISMEMBERABLE: i32 = 172;
/// `SET_NO_ACROBATICS`: Jedi won't jump, roll or cartwheel.
pub const SET_NO_ACROBATICS: i32 = 173;
/// `SET_USE_SUBTITLES`: When true NPC will always display subtitle regardless of subtitle setting.
pub const SET_USE_SUBTITLES: i32 = 174;
/// `SET_CLEAN_DAMAGING_ENTS`: Removes entities that could muck up cinematics, explosives, turrets, seekers.
pub const SET_CLEAN_DAMAGING_ENTS: i32 = 175;
/// `SET_HUD`: Turns on/off HUD.
pub const SET_HUD: i32 = 176;
/// `SET_SKILL`: Cannot set this, only get it - valid values are 0 through 3.
pub const SET_SKILL: i32 = 177;
/// `SET_ANIM_UPPER`: Torso and head anim.
pub const SET_ANIM_UPPER: i32 = 178;
/// `SET_ANIM_LOWER`: Legs anim.
pub const SET_ANIM_LOWER: i32 = 179;
/// `SET_ANIM_BOTH`: Set same anim on torso and legs.
pub const SET_ANIM_BOTH: i32 = 180;
/// `SET_PLAYER_TEAM`: Your team.
pub const SET_PLAYER_TEAM: i32 = 181;
/// `SET_ENEMY_TEAM`: Team in which to look for enemies.
pub const SET_ENEMY_TEAM: i32 = 182;
/// `SET_BEHAVIOR_STATE`: Change current bState.
pub const SET_BEHAVIOR_STATE: i32 = 183;
/// `SET_DEFAULT_BSTATE`: Change fallback bState.
pub const SET_DEFAULT_BSTATE: i32 = 184;
/// `SET_TEMP_BSTATE`: Set/Chang a temp bState.
pub const SET_TEMP_BSTATE: i32 = 185;
/// `SET_EVENT`: Events you can initiate.
pub const SET_EVENT: i32 = 186;
/// `SET_WEAPON`: Change/Stow/Drop weapon.
pub const SET_WEAPON: i32 = 187;
/// `SET_ITEM`: Give items.
pub const SET_ITEM: i32 = 188;
/// `SET_MUSIC_STATE`: Set the state of the dynamic music.
pub const SET_MUSIC_STATE: i32 = 189;
/// `SET_FORCE_HEAL_LEVEL`: Change force power level.
pub const SET_FORCE_HEAL_LEVEL: i32 = 190;
/// `SET_FORCE_JUMP_LEVEL`: Change force power level.
pub const SET_FORCE_JUMP_LEVEL: i32 = 191;
/// `SET_FORCE_SPEED_LEVEL`: Change force power level.
pub const SET_FORCE_SPEED_LEVEL: i32 = 192;
/// `SET_FORCE_PUSH_LEVEL`: Change force power level.
pub const SET_FORCE_PUSH_LEVEL: i32 = 193;
/// `SET_FORCE_PULL_LEVEL`: Change force power level.
pub const SET_FORCE_PULL_LEVEL: i32 = 194;
/// `SET_FORCE_MINDTRICK_LEVEL`: Change force power level.
pub const SET_FORCE_MINDTRICK_LEVEL: i32 = 195;
/// `SET_FORCE_GRIP_LEVEL`: Change force power level.
pub const SET_FORCE_GRIP_LEVEL: i32 = 196;
/// `SET_FORCE_LIGHTNING_LEVEL`: Change force power level.
pub const SET_FORCE_LIGHTNING_LEVEL: i32 = 197;
/// `SET_SABER_THROW`: Change force power level.
pub const SET_SABER_THROW: i32 = 198;
/// `SET_SABER_DEFENSE`: Change force power level.
pub const SET_SABER_DEFENSE: i32 = 199;
/// `SET_SABER_OFFENSE`: Change force power level.
pub const SET_SABER_OFFENSE: i32 = 200;
/// `SET_OBJECTIVE_SHOW`: Show objective on mission screen.
pub const SET_OBJECTIVE_SHOW: i32 = 201;
/// `SET_OBJECTIVE_HIDE`: Hide objective from mission screen.
pub const SET_OBJECTIVE_HIDE: i32 = 202;
/// `SET_OBJECTIVE_SUCCEEDED`: Mark objective as completed.
pub const SET_OBJECTIVE_SUCCEEDED: i32 = 203;
/// `SET_OBJECTIVE_FAILED`: Mark objective as failed.
pub const SET_OBJECTIVE_FAILED: i32 = 204;
/// `SET_MISSIONFAILED`: Mission failed screen activates.
pub const SET_MISSIONFAILED: i32 = 205;
/// `SET_TACTICAL_SHOW`: Show tactical info on mission objectives screen.
pub const SET_TACTICAL_SHOW: i32 = 206;
/// `SET_TACTICAL_HIDE`: Hide tactical info on mission objectives screen.
pub const SET_TACTICAL_HIDE: i32 = 207;
/// `SET_OBJECTIVE_CLEARALL`: Force all objectives to be hidden.
pub const SET_OBJECTIVE_CLEARALL: i32 = 208;
/// `SET_OBJECTIVEFOSTER`.
pub const SET_OBJECTIVEFOSTER: i32 = 209;
/// `SET_MISSIONSTATUSTEXT`: Text to appear in mission status screen.
pub const SET_MISSIONSTATUSTEXT: i32 = 210;
/// `SET_MENU_SCREEN`: Brings up specified menu screen.
pub const SET_MENU_SCREEN: i32 = 211;
/// `SET_CLOSINGCREDITS`: Show closing credits.
pub const SET_CLOSINGCREDITS: i32 = 212;
/// `SET_LEAN`: Lean left, right or stop leaning.
pub const SET_LEAN: i32 = 213;

/// `setTable`: every name the game knows, with its number, in the table's order.
pub const SET_TABLE: &[(&str, i32)] = &[
    ("SET_SPAWNSCRIPT", SET_SPAWNSCRIPT),
    ("SET_USESCRIPT", SET_USESCRIPT),
    ("SET_AWAKESCRIPT", SET_AWAKESCRIPT),
    ("SET_ANGERSCRIPT", SET_ANGERSCRIPT),
    ("SET_ATTACKSCRIPT", SET_ATTACKSCRIPT),
    ("SET_VICTORYSCRIPT", SET_VICTORYSCRIPT),
    ("SET_PAINSCRIPT", SET_PAINSCRIPT),
    ("SET_FLEESCRIPT", SET_FLEESCRIPT),
    ("SET_DEATHSCRIPT", SET_DEATHSCRIPT),
    ("SET_DELAYEDSCRIPT", SET_DELAYEDSCRIPT),
    ("SET_BLOCKEDSCRIPT", SET_BLOCKEDSCRIPT),
    ("SET_FFIRESCRIPT", SET_FFIRESCRIPT),
    ("SET_FFDEATHSCRIPT", SET_FFDEATHSCRIPT),
    ("SET_MINDTRICKSCRIPT", SET_MINDTRICKSCRIPT),
    ("SET_NO_MINDTRICK", SET_NO_MINDTRICK),
    ("SET_ORIGIN", SET_ORIGIN),
    ("SET_TELEPORT_DEST", SET_TELEPORT_DEST),
    ("SET_ANGLES", SET_ANGLES),
    ("SET_XVELOCITY", SET_XVELOCITY),
    ("SET_YVELOCITY", SET_YVELOCITY),
    ("SET_ZVELOCITY", SET_ZVELOCITY),
    ("SET_Z_OFFSET", SET_Z_OFFSET),
    ("SET_ENEMY", SET_ENEMY),
    ("SET_LEADER", SET_LEADER),
    ("SET_NAVGOAL", SET_NAVGOAL),
    ("SET_ANIM_UPPER", SET_ANIM_UPPER),
    ("SET_ANIM_LOWER", SET_ANIM_LOWER),
    ("SET_ANIM_BOTH", SET_ANIM_BOTH),
    ("SET_ANIM_HOLDTIME_LOWER", SET_ANIM_HOLDTIME_LOWER),
    ("SET_ANIM_HOLDTIME_UPPER", SET_ANIM_HOLDTIME_UPPER),
    ("SET_ANIM_HOLDTIME_BOTH", SET_ANIM_HOLDTIME_BOTH),
    ("SET_PLAYER_TEAM", SET_PLAYER_TEAM),
    ("SET_ENEMY_TEAM", SET_ENEMY_TEAM),
    ("SET_BEHAVIOR_STATE", SET_BEHAVIOR_STATE),
    ("SET_BEHAVIOR_STATE", SET_BEHAVIOR_STATE),
    ("SET_HEALTH", SET_HEALTH),
    ("SET_ARMOR", SET_ARMOR),
    ("SET_DEFAULT_BSTATE", SET_DEFAULT_BSTATE),
    ("SET_CAPTURE", SET_CAPTURE),
    ("SET_DPITCH", SET_DPITCH),
    ("SET_DYAW", SET_DYAW),
    ("SET_EVENT", SET_EVENT),
    ("SET_TEMP_BSTATE", SET_TEMP_BSTATE),
    ("SET_COPY_ORIGIN", SET_COPY_ORIGIN),
    ("SET_VIEWTARGET", SET_VIEWTARGET),
    ("SET_WEAPON", SET_WEAPON),
    ("SET_ITEM", SET_ITEM),
    ("SET_WALKSPEED", SET_WALKSPEED),
    ("SET_RUNSPEED", SET_RUNSPEED),
    ("SET_YAWSPEED", SET_YAWSPEED),
    ("SET_AGGRESSION", SET_AGGRESSION),
    ("SET_AIM", SET_AIM),
    ("SET_FRICTION", SET_FRICTION),
    ("SET_GRAVITY", SET_GRAVITY),
    ("SET_IGNOREPAIN", SET_IGNOREPAIN),
    ("SET_IGNOREENEMIES", SET_IGNOREENEMIES),
    ("SET_IGNOREALERTS", SET_IGNOREALERTS),
    ("SET_DONTSHOOT", SET_DONTSHOOT),
    ("SET_DONTFIRE", SET_DONTFIRE),
    ("SET_LOCKED_ENEMY", SET_LOCKED_ENEMY),
    ("SET_NOTARGET", SET_NOTARGET),
    ("SET_LEAN", SET_LEAN),
    ("SET_CROUCHED", SET_CROUCHED),
    ("SET_WALKING", SET_WALKING),
    ("SET_RUNNING", SET_RUNNING),
    ("SET_CHASE_ENEMIES", SET_CHASE_ENEMIES),
    ("SET_LOOK_FOR_ENEMIES", SET_LOOK_FOR_ENEMIES),
    ("SET_FACE_MOVE_DIR", SET_FACE_MOVE_DIR),
    ("SET_ALT_FIRE", SET_ALT_FIRE),
    ("SET_DONT_FLEE", SET_DONT_FLEE),
    ("SET_FORCED_MARCH", SET_FORCED_MARCH),
    ("SET_NO_RESPONSE", SET_NO_RESPONSE),
    ("SET_NO_COMBAT_TALK", SET_NO_COMBAT_TALK),
    ("SET_NO_ALERT_TALK", SET_NO_ALERT_TALK),
    ("SET_UNDYING", SET_UNDYING),
    ("SET_TREASONED", SET_TREASONED),
    ("SET_DISABLE_SHADER_ANIM", SET_DISABLE_SHADER_ANIM),
    ("SET_SHADER_ANIM", SET_SHADER_ANIM),
    ("SET_INVINCIBLE", SET_INVINCIBLE),
    ("SET_NOAVOID", SET_NOAVOID),
    ("SET_SHOOTDIST", SET_SHOOTDIST),
    ("SET_TARGETNAME", SET_TARGETNAME),
    ("SET_TARGET", SET_TARGET),
    ("SET_TARGET2", SET_TARGET2),
    ("SET_LOCATION", SET_LOCATION),
    ("SET_PAINTARGET", SET_PAINTARGET),
    ("SET_TIMESCALE", SET_TIMESCALE),
    ("SET_VISRANGE", SET_VISRANGE),
    ("SET_EARSHOT", SET_EARSHOT),
    ("SET_VIGILANCE", SET_VIGILANCE),
    ("SET_HFOV", SET_HFOV),
    ("SET_VFOV", SET_VFOV),
    ("SET_DELAYSCRIPTTIME", SET_DELAYSCRIPTTIME),
    ("SET_FORWARDMOVE", SET_FORWARDMOVE),
    ("SET_RIGHTMOVE", SET_RIGHTMOVE),
    ("SET_LOCKYAW", SET_LOCKYAW),
    ("SET_SOLID", SET_SOLID),
    ("SET_CAMERA_GROUP", SET_CAMERA_GROUP),
    ("SET_CAMERA_GROUP_Z_OFS", SET_CAMERA_GROUP_Z_OFS),
    ("SET_CAMERA_GROUP_TAG", SET_CAMERA_GROUP_TAG),
    ("SET_LOOK_TARGET", SET_LOOK_TARGET),
    ("SET_ADDRHANDBOLT_MODEL", SET_ADDRHANDBOLT_MODEL),
    ("SET_REMOVERHANDBOLT_MODEL", SET_REMOVERHANDBOLT_MODEL),
    ("SET_ADDLHANDBOLT_MODEL", SET_ADDLHANDBOLT_MODEL),
    ("SET_REMOVELHANDBOLT_MODEL", SET_REMOVELHANDBOLT_MODEL),
    ("SET_FACEAUX", SET_FACEAUX),
    ("SET_FACEBLINK", SET_FACEBLINK),
    ("SET_FACEBLINKFROWN", SET_FACEBLINKFROWN),
    ("SET_FACEFROWN", SET_FACEFROWN),
    ("SET_FACENORMAL", SET_FACENORMAL),
    ("SET_FACEEYESCLOSED", SET_FACEEYESCLOSED),
    ("SET_FACEEYESOPENED", SET_FACEEYESOPENED),
    ("SET_SCROLLTEXT", SET_SCROLLTEXT),
    ("SET_LCARSTEXT", SET_LCARSTEXT),
    ("SET_SCROLLTEXTCOLOR", SET_SCROLLTEXTCOLOR),
    ("SET_CAPTIONTEXTCOLOR", SET_CAPTIONTEXTCOLOR),
    ("SET_CENTERTEXTCOLOR", SET_CENTERTEXTCOLOR),
    ("SET_PLAYER_USABLE", SET_PLAYER_USABLE),
    ("SET_STARTFRAME", SET_STARTFRAME),
    ("SET_ENDFRAME", SET_ENDFRAME),
    ("SET_ANIMFRAME", SET_ANIMFRAME),
    ("SET_LOOP_ANIM", SET_LOOP_ANIM),
    ("SET_INTERFACE", SET_INTERFACE),
    ("SET_SHIELDS", SET_SHIELDS),
    ("SET_NO_KNOCKBACK", SET_NO_KNOCKBACK),
    ("SET_INVISIBLE", SET_INVISIBLE),
    ("SET_VAMPIRE", SET_VAMPIRE),
    ("SET_FORCE_INVINCIBLE", SET_FORCE_INVINCIBLE),
    ("SET_GREET_ALLIES", SET_GREET_ALLIES),
    ("SET_PLAYER_LOCKED", SET_PLAYER_LOCKED),
    ("SET_LOCK_PLAYER_WEAPONS", SET_LOCK_PLAYER_WEAPONS),
    ("SET_NO_IMPACT_DAMAGE", SET_NO_IMPACT_DAMAGE),
    ("SET_PARM1", SET_PARM1),
    ("SET_PARM2", SET_PARM2),
    ("SET_PARM3", SET_PARM3),
    ("SET_PARM4", SET_PARM4),
    ("SET_PARM5", SET_PARM5),
    ("SET_PARM6", SET_PARM6),
    ("SET_PARM7", SET_PARM7),
    ("SET_PARM8", SET_PARM8),
    ("SET_PARM9", SET_PARM9),
    ("SET_PARM10", SET_PARM10),
    ("SET_PARM11", SET_PARM11),
    ("SET_PARM12", SET_PARM12),
    ("SET_PARM13", SET_PARM13),
    ("SET_PARM14", SET_PARM14),
    ("SET_PARM15", SET_PARM15),
    ("SET_PARM16", SET_PARM16),
    ("SET_DEFEND_TARGET", SET_DEFEND_TARGET),
    ("SET_WAIT", SET_WAIT),
    ("SET_COUNT", SET_COUNT),
    ("SET_SHOT_SPACING", SET_SHOT_SPACING),
    ("SET_VIDEO_PLAY", SET_VIDEO_PLAY),
    ("SET_VIDEO_FADE_IN", SET_VIDEO_FADE_IN),
    ("SET_VIDEO_FADE_OUT", SET_VIDEO_FADE_OUT),
    ("SET_REMOVE_TARGET", SET_REMOVE_TARGET),
    ("SET_LOADGAME", SET_LOADGAME),
    ("SET_MENU_SCREEN", SET_MENU_SCREEN),
    ("SET_OBJECTIVE_SHOW", SET_OBJECTIVE_SHOW),
    ("SET_OBJECTIVE_HIDE", SET_OBJECTIVE_HIDE),
    ("SET_OBJECTIVE_SUCCEEDED", SET_OBJECTIVE_SUCCEEDED),
    ("SET_OBJECTIVE_FAILED", SET_OBJECTIVE_FAILED),
    ("SET_MISSIONFAILED", SET_MISSIONFAILED),
    ("SET_TACTICAL_SHOW", SET_TACTICAL_SHOW),
    ("SET_TACTICAL_HIDE", SET_TACTICAL_HIDE),
    ("SET_FOLLOWDIST", SET_FOLLOWDIST),
    ("SET_SCALE", SET_SCALE),
    ("SET_OBJECTIVE_CLEARALL", SET_OBJECTIVE_CLEARALL),
    ("SET_MISSIONSTATUSTEXT", SET_MISSIONSTATUSTEXT),
    ("SET_WIDTH", SET_WIDTH),
    ("SET_CLOSINGCREDITS", SET_CLOSINGCREDITS),
    ("SET_SKILL", SET_SKILL),
    ("SET_MISSIONSTATUSTIME", SET_MISSIONSTATUSTIME),
    ("SET_FULLNAME", SET_FULLNAME),
    ("SET_FORCE_HEAL_LEVEL", SET_FORCE_HEAL_LEVEL),
    ("SET_FORCE_JUMP_LEVEL", SET_FORCE_JUMP_LEVEL),
    ("SET_FORCE_SPEED_LEVEL", SET_FORCE_SPEED_LEVEL),
    ("SET_FORCE_PUSH_LEVEL", SET_FORCE_PUSH_LEVEL),
    ("SET_FORCE_PULL_LEVEL", SET_FORCE_PULL_LEVEL),
    ("SET_FORCE_MINDTRICK_LEVEL", SET_FORCE_MINDTRICK_LEVEL),
    ("SET_FORCE_GRIP_LEVEL", SET_FORCE_GRIP_LEVEL),
    ("SET_FORCE_LIGHTNING_LEVEL", SET_FORCE_LIGHTNING_LEVEL),
    ("SET_SABER_THROW", SET_SABER_THROW),
    ("SET_SABER_DEFENSE", SET_SABER_DEFENSE),
    ("SET_SABER_OFFENSE", SET_SABER_OFFENSE),
    ("SET_VIEWENTITY", SET_VIEWENTITY),
    ("SET_WATCHTARGET", SET_WATCHTARGET),
    ("SET_SABERACTIVE", SET_SABERACTIVE),
    ("SET_ADJUST_AREA_PORTALS", SET_ADJUST_AREA_PORTALS),
    ("SET_DMG_BY_HEAVY_WEAP_ONLY", SET_DMG_BY_HEAVY_WEAP_ONLY),
    ("SET_SHIELDED", SET_SHIELDED),
    ("SET_NO_GROUPS", SET_NO_GROUPS),
    ("SET_FIRE_WEAPON", SET_FIRE_WEAPON),
    ("SET_INACTIVE", SET_INACTIVE),
    ("SET_FUNC_USABLE_VISIBLE", SET_FUNC_USABLE_VISIBLE),
    ("SET_MISSION_STATUS_SCREEN", SET_MISSION_STATUS_SCREEN),
    ("SET_END_SCREENDISSOLVE", SET_END_SCREENDISSOLVE),
    ("SET_LOOPSOUND", SET_LOOPSOUND),
    ("SET_ICARUS_FREEZE", SET_ICARUS_FREEZE),
    ("SET_ICARUS_UNFREEZE", SET_ICARUS_UNFREEZE),
    ("SET_USE_CP_NEAREST", SET_USE_CP_NEAREST),
    ("SET_MORELIGHT", SET_MORELIGHT),
    ("SET_CINEMATIC_SKIPSCRIPT", SET_CINEMATIC_SKIPSCRIPT),
    ("SET_NO_FORCE", SET_NO_FORCE),
    ("SET_NO_FALLTODEATH", SET_NO_FALLTODEATH),
    ("SET_DISMEMBERABLE", SET_DISMEMBERABLE),
    ("SET_NO_ACROBATICS", SET_NO_ACROBATICS),
    ("SET_MUSIC_STATE", SET_MUSIC_STATE),
    ("SET_USE_SUBTITLES", SET_USE_SUBTITLES),
    ("SET_CLEAN_DAMAGING_ENTS", SET_CLEAN_DAMAGING_ENTS),
    ("SET_HUD", SET_HUD),
];

/// `GetIDForString(setTable, name)`: the number of a name (any case), or -1.
pub fn set_id(name: &str) -> i32 {
    SET_TABLE
        .iter()
        .find(|(entry, _)| entry.eq_ignore_ascii_case(name))
        .map_or(-1, |&(_, id)| id)
}
