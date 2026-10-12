#include "cgame/cg_local.h"
#include "game/bg_local.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "line %d: %s\n", __LINE__, #x); exit(1); } } while (0)

cgs_t cgs;
centity_t cg_entities[MAX_GENTITIES];
pmove_t *pm;
bgLoadedAnim_t bgAllAnims[MAX_ANIM_FILES];

// Engine/movement services unrelated to the saber animation rules under test.
int PM_GetSaberStance(void) { return BOTH_STAND2; }
void PM_AddEvent(int event) {}
void BG_AddPredictableEventToPlayerstate(int event, int parm, playerState_t *ps) {}
saberInfo_t *BG_MySaber(int client, int saber) { return NULL; }
int PM_irand_timesync(int low, int high) { return low; }
qboolean PM_WalkingAnim(int anim);
qboolean PM_RunningAnim(int anim);
qboolean PM_SaberInBrokenParry(int move);
#include "prediction.h"

int main(void) {
  playerState_t ps;
  pmove_t movement;
  animation_t animations[MAX_TOTALANIMATIONS];
  int pose, stance, move;
  const int poses[] = {BOTH_BF1LOCK, BOTH_LK_DL_S_T_L_1, BOTH_LK_ST_ST_T_L_1, BOTH_LK_S_DL_T_SB_1_L, BOTH_LK_S_DL_T_SB_1_L};
  const int stances[] = {BOTH_STAND2, BOTH_WALK1, BOTH_WALK2, BOTH_WALKBACK1};
  memset(animations, 0, sizeof(animations));
  for (move = 0; move < MAX_TOTALANIMATIONS; ++move) {
    animations[move].firstFrame = 1;
    animations[move].numFrames = 10;
    animations[move].frameLerp = 50;
  }
  memset(&movement, 0, sizeof(movement));
  movement.ps = &ps;
  movement.animations = animations;
  pm = &movement;

  // Run the unchanged saber setter and animation implementation, with no
  // lightning-aware prediction helper or transmitted Saber Defense level.
  for (pose = 0; pose < ARRAY_LEN(poses); ++pose) {
    for (stance = 0; stance < ARRAY_LEN(stances); ++stance) {
      memset(&ps, 0, sizeof(ps));
      ps.pm_type = PM_NORMAL;
      ps.weapon = WP_SABER;
      ps.saberMove = LS_READY;
      ps.fd.saberAnimLevel = poses[pose] == BOTH_LK_DL_S_T_L_1 ? SS_DUAL :
        poses[pose] == BOTH_LK_ST_ST_T_L_1 ? SS_STAFF :
        poses[pose] == BOTH_BF1LOCK ? SS_STRONG :
        poses[pose] == BOTH_LK_S_DL_T_SB_1_L ? (pose == 3 ? SS_FAST : SS_MEDIUM) : SS_MEDIUM;
      ps.groundEntityNum = ENTITYNUM_WORLD;
      ps.legsAnim = stances[stance];
      ps.torsoAnim = poses[pose];
      ps.torsoTimer = 1000;
      movement.cmd.forwardmove = stance ? 64 : 0;
      movement.cmd.buttons = stance ? BUTTON_WALKING : 0;
      CHECK(ps.fd.forcePowerLevel[FP_SABER_DEFENSE] == 0);
      PM_SetSaberMove(LS_READY);
      CHECK(ps.torsoAnim == poses[pose] && ps.torsoTimer == 1000);
      CHECK(ps.legsAnim == stances[stance] && ps.legsTimer == 0);
      CHECK(ps.forceHandExtend == HANDEXTEND_NONE && ps.weaponTime == 0);
      // A delayed snapshot must still survive unacknowledged idle commands.
      ps.torsoTimer -= 600;
      PM_SetSaberMove(LS_READY);
      CHECK(ps.torsoAnim == poses[pose] && ps.torsoTimer == 400);

      // Every ordinary swing wind-up overrides the guard in this command.
      for (move = LS_S_TL2BR; move <= LS_S_T2B; ++move) {
        ps.saberMove = LS_READY;
        ps.torsoAnim = poses[pose];
        ps.torsoTimer = 1000;
        movement.cmd.buttons |= BUTTON_ATTACK;
        PM_SetSaberMove(move);
        CHECK(ps.saberMove == move && ps.torsoAnim != poses[pose]);
        CHECK(ps.torsoTimer > 150 && (movement.cmd.buttons & BUTTON_ATTACK));
      }
      ps.saberMove = LS_READY;
      ps.torsoAnim = poses[pose];
      ps.torsoTimer = 0;
      movement.cmd.buttons &= ~BUTTON_ATTACK;
      PM_SetSaberMove(LS_READY);
      CHECK(ps.torsoAnim != poses[pose]);
    }
  }
  puts("Passed stock saber animation retention, attack override and expiry");
  return 0;
}
