#include "cgame/cg_local.h"
#include "ghoul2/G2.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "line %d: %s\n", __LINE__, #x); exit(1); } } while (0)
cg_t cg;
centity_t cg_entities[MAX_GENTITIES];
bgLoadedAnim_t bgAllAnims[MAX_ANIM_FILES];
static cgameImport_t imports;
cgameImport_t *trap = &imports;
static int boneCalls, boneFrame, boneBlend;
static int torsoCalls, motionCalls;
static qboolean SetBoneAnim(void *ghoul2, const int model, const char *bone,
  const int first, const int last, const int flags, const float speed,
  const int time, const float setFrame, const int blend) {
  CHECK(!strcmp(bone, "lower_lumbar") || !strcmp(bone, "Motion"));
  if (!strcmp(bone, "lower_lumbar")) ++torsoCalls;
  else ++motionCalls;
  CHECK(last == first + 1 && setFrame == first && (flags & BONE_ANIM_OVERRIDE_FREEZE));
  ++boneCalls; boneFrame = first; boneBlend = blend;
  return qtrue;
}
#include "presentation.h"

int main(void) {
  centity_t *guard = &cg_entities[1];
  centity_t *caster = &cg_entities[0];
  cg.predictedPlayerState.clientNum = 0;
  cg.predictedPlayerState.origin[0] = 100;
  cg.predictedPlayerState.activeForcePass = FORCE_LEVEL_3;
  cg.predictedPlayerState.fd.forcePowersActive = 1 << FP_LIGHTNING;
  guard->currentState.number = 1;
  guard->currentState.weapon = WP_SABER;
  guard->currentState.saberMove = LS_READY;
  guard->currentState.torsoAnim = BOTH_BF1LOCK;
  // The viewing caster is absent from the snapshot list, as during prediction.
  CHECK(!caster->currentValid && CG_LightningDeflectionActive(guard));
  guard->currentState.torsoAnim = BOTH_LK_DL_S_T_L_1;
  CHECK(CG_LightningDeflectionActive(guard));
  guard->currentState.torsoAnim = BOTH_LK_ST_ST_T_L_1;
  CHECK(CG_LightningDeflectionActive(guard));
  guard->currentState.torsoAnim = BOTH_LK_S_S_T_L_2;
  CHECK(!CG_LightningDeflectionActive(guard));
  cg.predictedPlayerState.forceHandExtend = HANDEXTEND_NONE;
  {
    animation_t animations[MAX_TOTALANIMATIONS] = {0};
    lerpFrame_t lf = {0};
    const int poses[] = {BOTH_BF1LOCK, BOTH_LK_S_DL_T_SB_1_L,
      BOTH_LK_DL_S_T_L_1, BOTH_LK_ST_ST_T_L_1};
    int i;
    bgAllAnims[0].anims = animations;
    guard->ghoul2 = guard;
    imports.G2API_SetBoneAnim = SetBoneAnim;
    for (i = 0; i < ARRAY_LEN(poses); ++i) {
      int pose = poses[i], expected = pose == BOTH_LK_ST_ST_T_L_1 ? 100 : 109;
      animations[pose].firstFrame = 100;
      animations[pose].numFrames = 10;
      animations[pose].frameLerp = 50;
      guard->currentState.torsoAnim = pose;
      guard->lightningDeflectFrameAnim = 0;
      CHECK(CG_LightningDeflectionFrame(guard, &lf, pose));
      CHECK(boneFrame == expected && boneBlend == 100);
      CHECK(torsoCalls == motionCalls && torsoCalls > 0);
      CHECK(lf.frame == expected && lf.oldFrame == expected && lf.animationNumber == -1);
      CHECK(CG_LightningDeflectionFrame(guard, &lf, pose));
      CHECK(boneFrame == expected && boneBlend == 0);
      animations[pose].frameLerp = -50;
      CHECK(CG_LightningDeflectionFrame(guard, &lf, pose));
      CHECK(boneFrame == (pose == BOTH_LK_ST_ST_T_L_1 ? 109 : 100));
      guard->currentState.saberMove = LS_A_T2B;
      {
        int before = boneCalls;
        CHECK(!CG_LightningDeflectionFrame(guard, &lf, pose));
        CHECK(boneCalls == before && guard->lightningDeflectFrameAnim == 0);
      }
      guard->currentState.saberMove = LS_READY;
    }
  }
  guard->currentState.torsoAnim = BOTH_LK_S_DL_T_SB_1_L;
  CHECK(CG_LightningDeflectionActive(guard));
  guard->currentState.torsoAnim = BOTH_P1_S1_TR;
  CHECK(!CG_LightningDeflectionActive(guard));
  guard->currentState.torsoAnim = BOTH_BF1LOCK;
  guard->currentState.saberMove = LS_PARRY_UR;
  CHECK(!CG_LightningDeflectionActive(guard));
  guard->currentState.saberMove = LS_READY;
  guard->currentState.saberInFlight = qtrue;
  CHECK(!CG_LightningDeflectionActive(guard));
  guard->currentState.saberInFlight = qfalse;
  guard->currentState.saberHolstered = 2;
  CHECK(!CG_LightningDeflectionActive(guard));
  guard->currentState.saberHolstered = 0;
  cg.predictedPlayerState.fd.forcePowersActive = 0;
  CHECK(!CG_LightningDeflectionActive(guard));
  // Switch to the defender's view. Effects follow its held, predicted pose.
  cg.predictedPlayerState.clientNum = 1;
  cg.predictedPlayerState.torsoTimer = 150;
  caster->currentValid = qtrue;
  caster->currentState.activeForcePass = FORCE_LEVEL_3;
  caster->currentState.forcePowersActive = 1 << FP_LIGHTNING;
  caster->lerpOrigin[0] = 100;
  CHECK(CG_LightningDeflectionActive(guard));
  cg.predictedPlayerState.torsoTimer = 0;
  CHECK(!CG_LightningDeflectionActive(guard));
  cg.predictedPlayerState.torsoTimer = 150;
  cg.predictedPlayerState.forceHandExtend = HANDEXTEND_KNOCKDOWN;
  CHECK(!CG_LightningDeflectionActive(guard));
  puts("Passed effect recognition from caster and defender views");
  return 0;
}
