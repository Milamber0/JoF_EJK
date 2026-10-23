#include "game/b_local.h"
#ifndef _WIN32
#include <strings.h>
#endif

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)
#define RANCOR MAX_CLIENTS

gentity_t g_entities[MAX_ENTITIESTOTAL];
level_locals_t level;
npcStatic_t NPCS;
vmCvar_t g_noSpecMove, g_allowNoFollow, g_blockDuelHealthSpec;
static gclient_t clients[MAX_CLIENTS + 1];
static gNPC_t npcInfo;
static gameImport_t imports;
gameImport_t *trap = &imports;
static int damageCalls, knockdowns, throws, attachments, links, events;
static int nearby[4], nearbyCount;

void Rancor_Swing(qboolean tryGrab);
void Rancor_Smash(void);
void Rancor_Bite(void);
void Rancor_Crush(void);
void Rancor_CheckDropVictim(void);
void Rancor_Attack(float distance, qboolean doCharge);

// Engine/world services. Damage deliberately has no spectator protection:
// these tests require the production AI to reject the target itself.
void G_Damage(gentity_t *ent, gentity_t *inflictor, gentity_t *attacker,
    vec3_t dir, vec3_t point, int damage, int flags, int mod) {
  ++damageCalls;
  ent->health -= damage;
  ent->client->ps.stats[STAT_HEALTH] = ent->health;
}
void G_Knockdown(gentity_t *ent) { ++knockdowns; }
void G_Throw(gentity_t *ent, vec3_t dir, float force) { ++throws; }
void G_Dismember(gentity_t *ent, gentity_t *enemy, vec3_t point, int limb,
    float roll, float pitch, int anim, qboolean postDeath) { ++events; }
void G_AddEvent(gentity_t *ent, int event, int parm) { ++events; }
void G_Sound(gentity_t *ent, int channel, int sound) { ++events; }
int G_SoundIndex(const char *name) { return 1; }
void G_SetEnemy(gentity_t *ent, gentity_t *enemy) { ent->enemy = enemy; }
void G_SetOrigin(gentity_t *ent, vec3_t origin) { VectorCopy(origin, ent->r.currentOrigin); }
void G_SetAngles(gentity_t *ent, vec3_t angles) { VectorCopy(angles, ent->r.currentAngles); }
void SetClientViewAngle(gentity_t *ent, vec3_t angles) { VectorCopy(angles, ent->client->ps.viewangles); }
void G_LeaveVehicle(gentity_t *ent, qboolean check) { ent->client->ps.m_iVehicleNum = 0; }
void TossClientItems(gentity_t *ent) {}
void NPC_SetAnim(gentity_t *ent, int part, int anim, int flags) {
  if (part & SETANIM_LEGS) ent->client->ps.legsAnim = anim;
  if (part & SETANIM_TORSO) ent->client->ps.torsoAnim = anim;
}
void G_GetBoltPosition(gentity_t *ent, int bolt, vec3_t pos, int model) { VectorClear(pos); }
gentity_t *G_ScreenShake(vec3_t pos, gentity_t *target, float intensity, int duration, qboolean global) { return NULL; }
void AddSightEvent(gentity_t *ent, vec3_t pos, float radius, alertEventLevel_e alert, float light) {}
void AddSoundEvent(gentity_t *ent, vec3_t pos, float radius, alertEventLevel_e alert, qboolean los) {}
int NPC_GetEntsNearBolt(int *entities, float radius, int bolt, vec3_t origin) {
  VectorClear(origin);
  memcpy(entities, nearby, nearbyCount * sizeof(*entities));
  return nearbyCount;
}
gentity_t *UpdateGoal(void) { return NULL; }
qboolean NPC_MoveToGoal(qboolean straight) { return qfalse; }
qboolean NPC_CheckEnemyExt(qboolean alerts) { return qfalse; }
gentity_t *NPC_CheckEnemy(qboolean findNew, qboolean tooFar, qboolean setEnemy) { return NULL; }
qboolean NPC_UpdateAngles(qboolean pitch, qboolean yaw) { return qtrue; }
qboolean NPC_FaceEnemy(qboolean pitch) { return qtrue; }
qboolean NPC_ClearLOS4(gentity_t *ent) { return qtrue; }
qboolean InFOV3(vec3_t spot, vec3_t from, vec3_t angles, int hFov, int vFov) { return qtrue; }
qboolean ValidEnemy(gentity_t *ent) { return qtrue; }
int Q_stricmp(const char *a, const char *b) {
#ifdef _WIN32
  return _stricmp(a, b);
#else
  return strcasecmp(a, b);
#endif
}
char *QDECL va(const char *format, ...) {
  static char buffer[256];
  va_list args;
  va_start(args, format);
  vsnprintf(buffer, sizeof(buffer), format, args);
  va_end(args);
  return buffer;
}
static void LinkEntity(sharedEntity_t *ent) { ++links; ent->r.linked = qtrue; }
static void UnlinkEntity(sharedEntity_t *ent) { ent->r.linked = qfalse; }
static void Trace(trace_t *trace, const vec3_t start, const vec3_t mins,
    const vec3_t maxs, const vec3_t end, int pass, int mask, int capsule, int flags, int lod) {
  memset(trace, 0, sizeof(*trace));
  trace->fraction = 1.0f; // The victim has room to escape.
}
void BG_AttachToRancor(void *ghoul2, float yaw, vec3_t origin, int time,
    qhandle_t *models, vec3_t scale, qboolean inMouth, vec3_t outOrigin,
    vec3_t outAngles, matrix3_t outAxis) {
  ++attachments;
  VectorSet(outOrigin, 25, 50, 75);
}
static void SV_PMTrace(trace_t *trace, const vec3_t start, const vec3_t mins,
    const vec3_t maxs, const vec3_t end, int pass, int mask) {}
void Pmove(pmove_t *pmove) {}
void G_TouchTriggers(gentity_t *ent) {}
void Cmd_FollowCycle_f(gentity_t *ent, int direction) {}
void G_TimeShiftAllClients(int time, gentity_t *ent, qboolean anims) {}
void G_UnTimeShiftAllClients(gentity_t *ent, qboolean anims) {}
static int SpectatorFind(gentity_t *ent) { return -1; }
int G_AdminAllowed(gentity_t *ent, unsigned int command, qboolean cheats, qboolean race, char *name) { return 0; }
void ClientBegin(int clientNum, qboolean reset) { CHECK(0); }

#include "actual.h"

static void Reset(void) {
  int i;
  memset(g_entities, 0, sizeof(g_entities));
  memset(clients, 0, sizeof(clients));
  memset(&level, 0, sizeof(level));
  memset(&npcInfo, 0, sizeof(npcInfo));
  memset(&NPCS, 0, sizeof(NPCS));
  level.time = 1000;
  level.clients = clients;
  level.maxclients = MAX_CLIENTS;
  for (i = 0; i <= MAX_CLIENTS; ++i) {
    gentity_t *ent = &g_entities[i];
    ent->s.number = i;
    ent->client = &clients[i];
    ent->inuse = qtrue;
    ent->health = clients[i].ps.stats[STAT_HEALTH] = 100;
    clients[i].pers.connected = CON_CONNECTED;
    clients[i].sess.sessionTeam = TEAM_FREE;
    clients[i].tempSpectate = -1;
    clients[i].ps.clientNum = i;
    clients[i].ps.groundEntityNum = ENTITYNUM_NONE;
  }
  NPCS.NPC = &g_entities[RANCOR];
  NPCS.NPCInfo = &npcInfo;
  NPCS.NPC->NPC = &npcInfo;
  NPCS.NPC->s.NPC_class = clients[RANCOR].NPC_class = CLASS_RANCOR;
  NPCS.NPC->s.eType = ET_NPC;
  NPCS.NPC->client->ps.legsTimer = 3000;
  imports.LinkEntity = LinkEntity;
  imports.UnlinkEntity = UnlinkEntity;
  imports.Trace = Trace;
  attachments = links = damageCalls = knockdowns = throws = events = 0;
  nearbyCount = 0;
  TIMER_Clear();
}

static void Grab(gentity_t *victim) {
  NPCS.NPC->activator = NPCS.NPC->enemy = victim;
  NPCS.NPC->count = 1;
  victim->client->ps.eFlags2 |= EF2_HELD_BY_MONSTER;
  victim->client->ps.hasLookTarget = qtrue;
  victim->client->ps.lookTarget = RANCOR;
}

static void Spectator(gentity_t *ent, int mode) {
  if (mode == 0 || mode == 1) {
    ent->client->sess.sessionTeam = TEAM_SPECTATOR;
    ent->client->sess.spectatorState = mode == 0 ? SPECTATOR_FREE : SPECTATOR_FOLLOW;
  }
  if (mode == 1 || mode == 3) ent->client->ps.pm_flags |= PMF_FOLLOW;
  // Check the inclusive boundary used by siege's temporary spectator state.
  if (mode == 2) ent->client->tempSpectate = level.time;
}

static void CheckFollow(qboolean unfollow) {
  gentity_t *victim, *watcher;
  usercmd_t cmd = {0};
  Reset();
  victim = &g_entities[0];
  watcher = &g_entities[1];
  Grab(victim);
  victim->client->ps.eFlags2 |= EF2_ALERTED;
  Spectator(watcher, 1);
  watcher->client->sess.spectatorClient = 0;
  SpectatorClientEndFrame(watcher);
  CHECK(watcher->client->ps.eFlags2 & EF2_HELD_BY_MONSTER);
  CHECK(watcher->client->ps.lookTarget == RANCOR);
  CHECK(watcher->client->ps.pm_flags & PMF_FOLLOW);

  cmd.forwardmove = 100;
  G_HeldByMonster(watcher, &cmd);
  CHECK(!attachments && !links && cmd.forwardmove == 100);
  watcher->r.linked = qtrue; // Recover even a spectator left linked earlier.
  SpectatorThink(watcher, &cmd);
  CHECK(!watcher->r.linked);

  // Escape between snapshots, while the follower still has the held flag.
  Rancor_CheckDropVictim();
  CHECK(!NPCS.NPC->activator && !NPCS.NPC->count);
  CHECK(!(victim->client->ps.eFlags2 & EF2_HELD_BY_MONSTER));
  CHECK(watcher->client->ps.eFlags2 & EF2_HELD_BY_MONSTER);
  if (unfollow) {
    StopFollowing(watcher);
    CHECK(!(watcher->client->ps.pm_flags & PMF_FOLLOW));
    CHECK(!watcher->client->ps.hasLookTarget);
    CHECK(watcher->client->ps.lookTarget == ENTITYNUM_NONE);
    CHECK(watcher->client->ps.clientNum == 1);
  } else {
    SpectatorClientEndFrame(watcher);
    CHECK(watcher->client->ps.pm_flags & PMF_FOLLOW);
  }
  CHECK(!(watcher->client->ps.eFlags2 & EF2_HELD_BY_MONSTER));
  CHECK(watcher->client->ps.eFlags2 & EF2_ALERTED);
  G_HeldByMonster(watcher, &cmd);
  CHECK(!attachments && !links && cmd.forwardmove == 100);

  // Radius queries used to accept this spectator after the copied flag cleared.
  nearby[0] = 1;
  nearbyCount = 1;
  Rancor_Swing(qtrue);
  Rancor_Swing(qfalse);
  Rancor_Smash();
  Rancor_Bite();
  CHECK(!NPCS.NPC->activator && !damageCalls && !knockdowns && !throws);
  CHECK(watcher->health == 100);
}

static void CheckAttachment(void) {
  int mode;
  for (mode = 0; mode < 5; ++mode) {
    usercmd_t cmd = {0};
    gentity_t *victim;
    Reset();
    victim = &g_entities[0];
    Grab(victim);
    if (mode < 4) Spectator(victim, mode);
    cmd.forwardmove = 100;
    cmd.rightmove = 50;
    cmd.upmove = 25;
    G_HeldByMonster(victim, &cmd);
    if (mode < 4) {
      CHECK(!attachments && !links);
      CHECK(cmd.forwardmove == 100 && cmd.rightmove == 50 && cmd.upmove == 25);
    } else {
      CHECK(attachments == 1 && links == 1);
      CHECK(!cmd.forwardmove && !cmd.rightmove && !cmd.upmove);
      CHECK(victim->r.currentOrigin[2] == 75);
    }
  }
}

static void CheckAttacks(void) {
  int mode, attack;
  for (mode = 0; mode < 5; ++mode) {
    for (attack = 0; attack < 5; ++attack) {
      gentity_t *victim;
      Reset();
      victim = &g_entities[0];
      if (mode < 4) Spectator(victim, mode);
      nearby[0] = 0;
      nearbyCount = 1;
      NPCS.NPC->client->ps.groundEntityNum = 0;
      switch (attack) {
      case 0: Rancor_Swing(qtrue); break;
      case 1: Rancor_Swing(qfalse); break;
      case 2: Rancor_Smash(); break;
      case 3: Rancor_Bite(); break;
      case 4: Rancor_Crush(); break;
      }
      if (mode < 4) {
        CHECK(!damageCalls && !knockdowns && !throws && !events);
        CHECK(!NPCS.NPC->activator && victim->health == 100);
      } else if (attack == 0) {
        CHECK(NPCS.NPC->activator == victim && NPCS.NPC->count == 1);
        CHECK(victim->client->ps.eFlags2 & EF2_HELD_BY_MONSTER);
      } else {
        CHECK(damageCalls == 1 && victim->health < 100);
      }
    }
  }
}

static void CheckDelayed(void) {
  int mode, phase;
  for (mode = 0; mode < 5; ++mode) {
    for (phase = 0; phase < 3; ++phase) {
      gentity_t *victim;
      Reset();
      victim = &g_entities[0];
      Grab(victim);
      if (mode < 4) Spectator(victim, mode);
      NPCS.NPC->client->ps.legsAnim = phase == 0 ? BOTH_ATTACK1 : BOTH_ATTACK3;
      TIMER_Set(NPCS.NPC, "attacking", 3000);
      TIMER_Set(NPCS.NPC, "clearGrabbed", 5000);
      TIMER_Set(NPCS.NPC, phase == 2 ? "attack_dmg2" : "attack_dmg", -1);
      // Exercise the full production AI entry point, including delayed damage.
      NPC_BSRancor_Default();
      if (mode < 4) {
        CHECK(!damageCalls && victim->health == 100);
        CHECK(!NPCS.NPC->activator && !NPCS.NPC->count);
        CHECK(!(victim->client->ps.eFlags2 & EF2_HELD_BY_MONSTER));
        CHECK(!victim->client->ps.hasLookTarget);
        CHECK(!TIMER_Exists(NPCS.NPC, "attack_dmg"));
        CHECK(!TIMER_Exists(NPCS.NPC, "attack_dmg2"));
        CHECK(!TIMER_Exists(NPCS.NPC, "clearGrabbed"));
        CHECK(!TIMER_Exists(NPCS.NPC, "attacking"));
      } else {
        CHECK(damageCalls == 1 && victim->health < 100);
        if (phase > 0) CHECK(victim->health <= 0);
      }
    }
  }
}

int main(int argc, char **argv) {
  CHECK(argc == 2);
  if (!strcmp(argv[1], "follow")) CheckFollow(qfalse);
  else if (!strcmp(argv[1], "escape")) CheckFollow(qtrue);
  else if (!strcmp(argv[1], "attachment")) CheckAttachment();
  else if (!strcmp(argv[1], "attacks")) CheckAttacks();
  else if (!strcmp(argv[1], "delayed")) CheckDelayed();
  else CHECK(0);
  printf("PASS %s\n", argv[1]);
  return 0;
}
