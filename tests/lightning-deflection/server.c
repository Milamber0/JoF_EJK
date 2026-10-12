#include "game/g_local.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(condition) do { if (!(condition)) { fprintf(stderr, "%s:%d: %s\n", __FILE__, __LINE__, #condition); exit(1); } } while (0)
gentity_t g_entities[MAX_ENTITIESTOTAL];
level_locals_t level;
vmCvar_t g_fixLightning;
static gclient_t clients[3];
static gentity_t eventEntity;
static pmove_t movement;
static int damages, events, absorbCalls, absorbLevel = -1, forceAllowed = 1;

int ForcePowerUsableOn(gentity_t *attacker, gentity_t *target, forcePowers_t power) { return forceAllowed; }
int WP_AbsorbConversion(gentity_t *target, int absorb, gentity_t *attacker, int power, int rank, int spent) {
  ++absorbCalls; return absorbLevel;
}
void G_Damage(gentity_t *target, gentity_t *inflictor, gentity_t *attacker, vec3_t dir, vec3_t point, int damage, int flags, int mod) {
  CHECK(mod == MOD_FORCE_DARK); ++damages; target->health -= damage;
}
void G_SetAnim(gentity_t *ent, usercmd_t *cmd, int parts, int anim, int flags, int blend) {
  CHECK(parts == SETANIM_TORSO); CHECK(flags & SETANIM_FLAG_HOLD);
  ent->client->ps.torsoAnim = anim; ent->client->ps.torsoTimer = 100;
}
gentity_t *G_TempEntity(vec3_t origin, int event) {
  ++events; memset(&eventEntity, 0, sizeof(eventEntity)); eventEntity.s.event = event; return &eventEntity;
}
int G_SoundIndex(const char *name) { return 1; }
void G_Sound(gentity_t *ent, int channel, int sound) {}
void Jedi_Decloak(gentity_t *ent) { ent->client->ps.powerups[PW_CLOAKED] = 0; }
char *QDECL va(const char *format, ...) {
  static char buffer[256]; va_list args; va_start(args, format); vsnprintf(buffer, sizeof(buffer), format, args); va_end(args); return buffer;
}
#include "shared.h"
#include "server.h"

static void Reset(void) {
  int i;
  memset(clients, 0, sizeof(clients)); memset(g_entities, 0, sizeof(g_entities));
  memset(&movement, 0, sizeof(movement)); memset(&level, 0, sizeof(level));
  level.time = 1000; g_fixLightning.integer = 3; damages = events = absorbCalls = 0;
  absorbLevel = -1; forceAllowed = 1;
  for (i = 0; i < 3; ++i) {
    playerState_t *ps = &clients[i].ps;
    g_entities[i].client = &clients[i]; g_entities[i].inuse = qtrue;
    g_entities[i].takedamage = qtrue; g_entities[i].health = 100; g_entities[i].s.number = i;
    ps->clientNum = i; ps->pm_type = PM_NORMAL; ps->stats[STAT_HEALTH] = 100;
    ps->weapon = WP_SABER; ps->saberEntityNum = 10; ps->saberMove = LS_READY;
    ps->groundEntityNum = ENTITYNUM_WORLD; ps->speed = ps->basespeed = 250; ps->viewheight = DEFAULT_VIEWHEIGHT;
    ps->fd.forcePowerLevel[FP_SABER_DEFENSE] = FORCE_LEVEL_3;
    ps->fd.forcePowerMax = 100;
    clients[i].pers.cmd.weapon = WP_SABER; clients[i].pers.cmd.serverTime = level.time;
  }
  clients[0].ps.origin[0] = 100;
	clients[0].ps.viewangles[YAW] = 180;
  clients[0].ps.fd.forcePowerLevel[FP_LIGHTNING] = FORCE_LEVEL_3;
  clients[0].ps.fd.forcePowersActive = 1 << FP_LIGHTNING;
  clients[2].ps.origin[0] = 100; clients[2].ps.origin[1] = 20;
	clients[2].ps.viewangles[YAW] = 180;
  clients[2].ps.fd.forcePowersActive = 1 << FP_LIGHTNING;
  movement.ps = &clients[1].ps; movement.cmd = clients[1].pers.cmd;
}
static void Hit(int caster) {
  vec3_t direction = {-1, 0, 0}, impact = {0, 0, DEFAULT_VIEWHEIGHT};
  ForceLightningDamage(&g_entities[caster], &g_entities[1], direction, impact);
}
static void Requirements(void) {
  playerState_t *ps = &clients[1].ps; usercmd_t *cmd = &clients[1].pers.cmd;
  CHECK(WP_CanDeflectLightning(ps, cmd, level.time)); // No learned Lightning or Force points.
  ps->fd.forcePowerLevel[FP_SABER_DEFENSE] = FORCE_LEVEL_2;
  CHECK(!WP_CanDeflectLightning(ps, cmd, level.time)); ps->fd.forcePowerLevel[FP_SABER_DEFENSE] = FORCE_LEVEL_3;
  cmd->forwardmove = 64; cmd->buttons = BUTTON_WALKING; ps->velocity[0] = 125;
  CHECK(WP_CanDeflectLightning(ps, cmd, level.time));
  cmd->rightmove = -64; CHECK(WP_CanDeflectLightning(ps, cmd, level.time));
  cmd->buttons = 0; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  cmd->buttons = BUTTON_WALKING; ps->velocity[0] = 250;
  CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  ps->velocity[0] = 0; cmd->forwardmove = 127;
  CHECK(!WP_CanDeflectLightning(ps, cmd, level.time)); cmd->forwardmove = 64;
  ps->velocity[0] = 0; ps->saberHolstered = 2; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  ps->saberHolstered = 0; ps->saberInFlight = qtrue; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  ps->saberInFlight = qfalse; ps->groundEntityNum = ENTITYNUM_NONE; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  ps->groundEntityNum = ENTITYNUM_WORLD; ps->saberMove = LS_A_T2B; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  ps->saberMove = LS_READY; ps->m_iVehicleNum = 1; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
  ps->m_iVehicleNum = 0; ps->forceHandExtend = HANDEXTEND_KNOCKDOWN; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time));
	ps->forceHandExtend = HANDEXTEND_NONE; ps->weaponTime = 100; ps->saberBlocked = BLOCKED_UPPER_RIGHT;
	cmd->weapon = WP_MELEE; CHECK(!WP_CanDeflectLightning(ps, cmd, level.time)); // Wait for combat recovery to finish.
	ps->weaponTime = 0; CHECK(WP_CanDeflectLightning(ps, cmd, level.time)); // Stale input/block state alone is harmless.
}
static void Aiming(void) {
  playerState_t *ps = &clients[1].ps; vec3_t source = {100, 0, DEFAULT_VIEWHEIGHT}; int anim;
  CHECK(WP_LightningDeflectDirection(ps, source, &anim)); CHECK(anim == BOTH_BF1LOCK);
  source[1] = 50; CHECK(WP_LightningDeflectDirection(ps, source, &anim)); CHECK(anim == BOTH_BF1LOCK);
  source[1] = -50; CHECK(WP_LightningDeflectDirection(ps, source, &anim)); CHECK(anim == BOTH_BF1LOCK);
  source[1] = 0; ps->viewangles[YAW] = 49; CHECK(WP_LightningDeflectDirection(ps, source, NULL));
  ps->viewangles[YAW] = 51; CHECK(!WP_LightningDeflectDirection(ps, source, NULL));
  ps->viewangles[YAW] = 90; CHECK(!WP_LightningDeflectDirection(ps, source, NULL));
  ps->viewangles[YAW] = 180; CHECK(!WP_LightningDeflectDirection(ps, source, NULL));
  ps->viewangles[YAW] = 0; ps->viewangles[PITCH] = 60; CHECK(!WP_LightningDeflectDirection(ps, source, NULL));
  ps->viewangles[PITCH] = -45; source[2] += 100; CHECK(WP_LightningDeflectDirection(ps, source, NULL));
}
static void Interruption(void) {
  int i; const int buttons[] = {BUTTON_ATTACK, BUTTON_ALT_ATTACK, BUTTON_USE_HOLDABLE, BUTTON_GESTURE};
  for (i = 0; i < 4; ++i) {
    Reset(); Hit(0); movement.cmd.buttons = buttons[i]; WP_UpdateLightningDeflect(&g_entities[1], &movement.cmd);
    CHECK(clients[1].lightningDeflectTime == 0);
    CHECK(clients[1].ps.torsoTimer == 0 && clients[1].ps.weaponTime == 0);
	CHECK(movement.cmd.buttons == buttons[i]);
  }
  Reset(); Hit(0); movement.cmd.forwardmove = 127; WP_UpdateLightningDeflect(&g_entities[1], &movement.cmd);
  CHECK(clients[1].lightningDeflectTime == 0);
  Reset(); Hit(0); movement.cmd.upmove = 127; WP_UpdateLightningDeflect(&g_entities[1], &movement.cmd);
  CHECK(clients[1].lightningDeflectTime == 0);
}
static void Combat(void) {
  int i;
  // No combat move may acquire/reacquire the guard, even with attack released.
  for (i = LS_NONE; i < LS_MOVE_MAX; ++i) {
    if (i == LS_NONE || i == LS_READY) continue;
    Reset(); clients[1].ps.saberMove = i;
    clients[1].ps.torsoAnim = BOTH_A1_T__B_; clients[1].ps.torsoTimer = 250;
    clients[1].ps.weaponTime = 0;
    Hit(0);
    CHECK(damages == 1 && clients[1].lightningDeflectTime == 0);
    CHECK(clients[1].ps.torsoAnim == BOTH_A1_T__B_ && clients[1].ps.torsoTimer == 250);
    CHECK(clients[1].ps.weaponTime == 0 && clients[1].ps.saberMove == i);
    Reset(); Hit(0);
    clients[1].ps.saberMove = i; clients[1].ps.torsoAnim = BOTH_A1_T__B_;
    clients[1].ps.torsoTimer = 250; clients[1].ps.weaponTime = 250;
    WP_UpdateLightningDeflect(&g_entities[1], &movement.cmd);
    CHECK(clients[1].lightningDeflectTime == 0);
    CHECK(clients[1].ps.torsoTimer == 250 && clients[1].ps.weaponTime == 250);
  }
  Reset(); clients[1].ps.weaponTime = 100; Hit(0);
  CHECK(damages == 1 && clients[1].lightningDeflectTime == 0);
  Reset(); clients[1].ps.saberLockTime = level.time + 100; Hit(0);
  CHECK(damages == 1 && clients[1].lightningDeflectTime == 0);
}
static void Damage(void) {
  clients[1].ps.electrifyTime = level.time + 800; // A new guard clears a prior shock shell.
  Hit(0); CHECK(damages == 0 && absorbCalls == 0); CHECK(g_entities[1].health == 100);
  CHECK(clients[1].ps.electrifyTime == 0 && clients[1].ps.fd.forcePower == 0);
  CHECK(clients[1].ps.torsoAnim == BOTH_BF1LOCK); CHECK(clients[1].ps.legsAnim == 0);
  CHECK(clients[1].ps.forceHandExtend == HANDEXTEND_NONE);
  CHECK(clients[1].ps.forceHandExtendTime == 0 && clients[1].ps.forceDodgeAnim == 0);
  CHECK(clients[1].ps.weaponTime == 0 && clients[1].ps.torsoTimer == LIGHTNING_DEFLECT_ANIM_TIME);
  CHECK(clients[1].lightningDeflectTime > level.time);
  CHECK(events == 0);
  clients[1].pers.cmd.buttons = BUTTON_ATTACK; Hit(0);
  CHECK(damages == 1 && g_entities[1].health == 99); CHECK(clients[1].ps.electrifyTime > level.time);
  CHECK(clients[1].lightningDeflectTime == 0);
  clients[1].pers.cmd.buttons = 0; Hit(0); CHECK(damages == 1); // Settling down can reacquire against a continuous beam.
  Reset(); clients[1].ps.viewangles[YAW] = 90; Hit(0); CHECK(damages == 1);
  Reset(); clients[1].pers.cmd.forwardmove = 127; Hit(0); CHECK(damages == 1);
  CHECK(g_entities[0].health == 100); // Scattered lightning never reflects damage.
}
static void Lifecycle(void) {
  Hit(0); CHECK(events == 0); level.time += 50; Hit(0); CHECK(events == 0);
  level.time += 50; Hit(0); CHECK(events == 0);
  level.time += LIGHTNING_DEFLECT_HOLD_TIME; WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd);
  CHECK(clients[1].lightningDeflectTime == 0 && clients[1].ps.weaponTime == 0);
  Reset(); Hit(0); clients[0].ps.fd.forcePowersActive = 0;
  WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd); CHECK(clients[1].lightningDeflectTime == 0);
  Reset(); Hit(0); clients[1].ps.viewangles[YAW] = 90;
  WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd); CHECK(clients[1].lightningDeflectTime == 0);
  Reset(); Hit(0); clients[1].ps.forceHandExtend = HANDEXTEND_KNOCKDOWN; clients[1].ps.forceDodgeAnim = 2;
  WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd);
  CHECK(clients[1].ps.forceHandExtend == HANDEXTEND_KNOCKDOWN && clients[1].ps.forceDodgeAnim == 2);
  CHECK(clients[1].lightningDeflectTime == 0);
  // Combat owns its timer; ending deflection must preserve their timer.
  Reset(); Hit(0); clients[1].ps.saberMove = LS_PARRY_UR;
  clients[1].ps.torsoTimer = clients[1].ps.weaponTime = 350;
  WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd);
  CHECK(clients[1].lightningDeflectTime == 0);
  CHECK(clients[1].ps.torsoTimer == 350 && clients[1].ps.weaponTime == 350);
}
static void Sources(void) {
  clients[1].ps.fd.saberAnimLevel = SS_FAST;
  Hit(0);
  CHECK(BG_LightningDeflectBreakPose(&clients[1].ps, level.time));
  CHECK(!BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.weaponTime = 350;
  CHECK(BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.weaponTime = 0; clients[1].ps.saberLockTime = level.time + 500;
  CHECK(BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.saberLockTime = 0; clients[1].ps.saberLockFrame = 10;
  CHECK(BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.saberLockFrame = 0; clients[1].ps.forceHandExtend = HANDEXTEND_KNOCKDOWN;
  CHECK(BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.forceHandExtend = HANDEXTEND_NONE; clients[1].ps.saberMove = LS_A_T2B;
  CHECK(BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.saberMove = LS_READY; clients[1].ps.legsAnim = BOTH_LK_S_DL_T_SB_1_L;
  CHECK(BG_SaberLockMovement(&clients[1].ps, level.time));
  Reset();
  clients[1].ps.fd.saberAnimLevel = SS_STRONG;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_BF1LOCK);
  clients[1].ps.fd.saberAnimLevel = SS_FAST;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_S_DL_T_SB_1_L);
  clients[1].ps.fd.saberAnimLevel = SS_MEDIUM;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_S_DL_T_SB_1_L);
  CHECK(!BG_SaberLockMovement(&clients[1].ps, level.time));
  clients[1].ps.fd.saberAnimLevel = SS_DUAL;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_DL_S_T_L_1);
  clients[1].ps.saberHolstered = 1;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_S_DL_T_SB_1_L);
  clients[1].ps.fd.saberAnimLevel = SS_FAST;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_S_DL_T_SB_1_L);
  Reset(); clients[1].ps.fd.saberAnimLevel = SS_STAFF;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_ST_ST_T_L_1);
  clients[1].ps.saberHolstered = 1;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_S_DL_T_SB_1_L);
  clients[1].ps.fd.saberAnimLevel = SS_MEDIUM;
  Hit(0); CHECK(damages == 0 && clients[1].ps.torsoAnim == BOTH_LK_S_DL_T_SB_1_L);
  CHECK(!BG_SaberLockMovement(&clients[1].ps, level.time));
  Reset();
  clients[2].ps.origin[1] = 50; // Clearly across the active pose's center buffer.
  Hit(0); Hit(2); CHECK(damages == 0 && events == 0);
	CHECK(clients[1].ps.torsoAnim == BOTH_BF1LOCK);
	level.time += 50; Hit(0); Hit(2); CHECK(events == 0 && damages == 0);
	Reset(); clients[1].ps.origin[1] = 20; Hit(0);
	CHECK(clients[1].ps.torsoAnim == BOTH_BF1LOCK);
	Reset(); clients[1].ps.origin[1] = -20; Hit(0);
	CHECK(clients[1].ps.torsoAnim == BOTH_BF1LOCK);
	Reset(); Hit(0);
  clients[2].ps.origin[0] = 0; clients[2].ps.origin[1] = 100; Hit(2);
  CHECK(damages == 1 && clients[1].lightningDeflectTime == 0);
}
static void PositionPose(void) {
  int yaw, pitch, lateral;
  playerState_t *defender = &clients[1].ps;
  playerState_t *caster = &clients[0].ps;
  caster->origin[1] = 50;
  // The damage callback already establishes a hit. Changing caster aim must
  // not flip the guard while both players remain in the same places.
  for (yaw = 0; yaw < 360; yaw += 5) {
    for (pitch = -80; pitch <= 80; pitch += 20) {
      caster->viewangles[YAW] = yaw;
      caster->viewangles[PITCH] = pitch;
      Hit(0);
      CHECK(damages == 0 && clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
      CHECK(defender->torsoAnim == BOTH_BF1LOCK);
    }
  }
  // Crossing the defender's facing direction retains the block pose.
  caster->origin[1] = -50; Hit(0);
  CHECK(clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
  caster->origin[1] = 50; Hit(0);
  CHECK(clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
  // Aim changes inside the blocking cone cannot flip a stationary guard.
  for (yaw = -15; yaw <= 70; yaw += 5) {
    for (pitch = -15; pitch <= 15; pitch += 5) {
      defender->viewangles[YAW] = yaw;
      defender->viewangles[PITCH] = pitch;
      WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd);
      Hit(0);
      CHECK(clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
      CHECK(defender->torsoAnim == BOTH_BF1LOCK);
    }
  }
  defender->viewangles[PITCH] = 0;
  defender->viewangles[YAW] = 0;
  // Moving the defender across the caster retains the pose too.
  defender->origin[1] = 100; Hit(0);
  CHECK(clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
  defender->origin[1] = 0; Hit(0);
  CHECK(clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
  // A new guard uses the same pose after the old guard expires.
  level.time += LIGHTNING_DEFLECT_HOLD_TIME;
  WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd);
  CHECK(clients[1].lightningDeflectTime == 0);
  defender->viewangles[YAW] = 40;
  Hit(0);
  CHECK(clients[1].lightningDeflectAnim == BOTH_BF1LOCK);
  CHECK(damages == 0);
  // The same geometry works when the encounter is rotated in world space.
  Reset();
  defender->viewangles[YAW] = 90;
  caster->origin[0] = -50; caster->origin[1] = 100;
  Hit(0); CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  defender->viewangles[YAW] = 130;
  Hit(0); CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  defender->viewangles[YAW] = 90;
  caster->origin[0] = 50;
  Hit(0); CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  CHECK(damages == 0);
  // Small movements around the center line must not alternate the poses.
  Reset();
  caster->origin[1] = -50; Hit(0);
  for (lateral = -20; lateral <= 20; ++lateral) {
    level.time += 50;
    caster->origin[1] = lateral;
    Hit(0); CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  }
  caster->origin[1] = 50; Hit(0);
  CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  for (lateral = 20; lateral >= -20; --lateral) {
    level.time += 50;
    caster->origin[1] = lateral;
    Hit(0); CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  }
  caster->origin[1] = -50; Hit(0);
  CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  // Translate the encounter: map coordinates do not change the pose.
  defender->origin[0] += 12000; defender->origin[1] -= 9000;
  defender->origin[2] += 3000;
  caster->origin[0] += 12000; caster->origin[1] -= 9000;
  caster->origin[2] += 3000;
  Hit(0); CHECK(defender->torsoAnim == BOTH_BF1LOCK);
  CHECK(damages == 0);
}
static void Absorption(void) {
  forceAllowed = 0; Hit(0); CHECK(damages == 0 && events == 0);
  forceAllowed = 1; clients[1].ps.fd.forcePowerLevel[FP_SABER_DEFENSE] = FORCE_LEVEL_2;
  absorbLevel = 0; Hit(0); CHECK(damages == 0 && absorbCalls == 1);
  level.time += 50; Hit(0); CHECK(clients[1].ps.fd.forcePower == 1 && absorbCalls == 1);
}
static void Knockdown(void) {
  int anim, part, tick;
  // The hand state may clear before the recovery animation is replaced.
  // Every down/get-up pose must keep taking unabsorbed lightning damage.
  for (anim = 0; anim < MAX_TOTALANIMATIONS; ++anim) {
    if (!BG_InKnockDown(anim)) continue;
    for (part = 0; part < 2; ++part) {
      playerState_t *ps = &clients[1].ps;
      Reset(); Hit(0); // A prior guard must not survive a knockdown.
      if (part) { ps->torsoAnim = anim; ps->torsoTimer = 500; }
      else { ps->legsAnim = anim; ps->legsTimer = 500; }
      CHECK(ps->forceHandExtend == HANDEXTEND_NONE && ps->weaponTime == 0);
      WP_UpdateLightningDeflect(&g_entities[1], &clients[1].pers.cmd);
      CHECK(clients[1].lightningDeflectTime == 0);
      for (tick = 0; tick < 40; ++tick) {
        level.time += 50;
        Hit(0);
        CHECK(damages == tick + 1 && clients[1].lightningDeflectTime == 0);
        CHECK(part ? ps->torsoAnim == anim && ps->torsoTimer == 500 :
                     ps->legsAnim == anim && ps->legsTimer == 500);
      }
      CHECK(g_entities[1].health == 60 && clients[1].noLightningTime == 0);
      ps->torsoAnim = BOTH_STAND2; ps->legsAnim = BOTH_STAND2;
      ps->torsoTimer = ps->legsTimer = 0;
      Hit(0); CHECK(damages == 40 && clients[1].lightningDeflectTime > level.time);
    }
  }
  Reset(); clients[1].ps.forceHandExtend = HANDEXTEND_KNOCKDOWN;
  Hit(0); CHECK(damages == 1 && clients[1].lightningDeflectTime == 0);
}
int main(int argc, char **argv) {
  CHECK(argc == 2); Reset();
  if (!strcmp(argv[1], "requirements")) Requirements();
  else if (!strcmp(argv[1], "aiming")) Aiming();
  else if (!strcmp(argv[1], "interruption")) Interruption();
  else if (!strcmp(argv[1], "combat")) Combat();
  else if (!strcmp(argv[1], "damage")) Damage();
  else if (!strcmp(argv[1], "lifecycle")) Lifecycle();
  else if (!strcmp(argv[1], "sources")) Sources();
  else if (!strcmp(argv[1], "position_pose")) PositionPose();
  else if (!strcmp(argv[1], "absorption")) Absorption();
  else if (!strcmp(argv[1], "knockdown")) Knockdown();
  else CHECK(0);
  puts("Passed"); return 0;
}
