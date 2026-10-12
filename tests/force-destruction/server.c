#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "../../codemp/game/g_force_destruction.c"

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)
level_locals_t level;
gentity_t g_entities[MAX_ENTITIESTOTAL];
static gclient_t clients[4];
qboolean gSiegeRoundBegun;
int dueltypes[MAX_CLIENTS];
vmCvar_t g_forceDestruction, g_forceDestructionCost, g_forceDestructionDamage;
vmCvar_t g_forceDestructionRadius, g_forceDestructionSpeed, g_forceDestructionCooldown;
vmCvar_t g_forcePowerDisable, g_forcePowerDisableFFA, g_maxForceRank;
vmCvar_t g_saberRestrictForce, g_friendlyFire, g_tweakForce, g_fixLightning, g_teamAbsorbScale;
static gameImport_t imports;
gameImport_t *trap = &imports;
static int shots, damageTaken[4], damageCalls[4], covered, blocked;
static gentity_t absorbSound;
static gentity_t castSound;

static void TestPrint(const char *fmt, ...) { (void)fmt; }
static void NORETURN TestError(int code, const char *fmt, ...) { (void)code; (void)fmt; abort(); }
void (*Com_Printf)(const char *, ...) = TestPrint;
NORETURN_PTR void (*Com_Error)(int, const char *, ...) = TestError;
void G_Sound(gentity_t *ent, int channel, int index) { (void)ent; (void)channel; (void)index; }
int G_SoundIndex(const char *name) { CHECK(strstr(name, "force/")); return 1; }
int G_EffectIndex(const char *name)
{
	if (!strcmp(name, "concussion/shot")) return 1;
	CHECK(!strcmp(name, "concussion/explosion"));
	return 2;
}
gentity_t *G_TempEntity(vec3_t origin, int event)
{
	CHECK(event == EV_GENERAL_SOUND);
	memset(&castSound, 0, sizeof(castSound));
	castSound.s.eType = ET_EVENTS + event;
	VectorCopy(origin, castSound.s.pos.trBase);
	return &castSound;
}
gentity_t *G_PreDefSound(vec3_t org, int sound) { (void)org; CHECK(sound == PDSOUND_ABSORBHIT); return &absorbSound; }
void G_AddEvent(gentity_t *ent, int event, int parm) { ent->s.event = event; ent->s.eventParm = parm; }
void G_SetOrigin(gentity_t *ent, vec3_t origin) { VectorCopy(origin, ent->r.currentOrigin); }
static void Link(sharedEntity_t *ent) { (void)ent; }
qboolean CanDamage(gentity_t *target, vec3_t origin) { (void)origin; return !(covered & (1 << target->s.number)); }
qboolean OnSameTeam(gentity_t *a, gentity_t *b)
{
	return a->client && b->client && a->client->sess.sessionTeam != TEAM_FREE &&
		a->client->sess.sessionTeam == b->client->sess.sessionTeam;
}
void JP_Trace(trace_t *result, const vec3_t start, const vec3_t mins, const vec3_t maxs,
	const vec3_t end, int pass, int mask, int capsule, int flags, int lod)
{
	(void)start; (void)mins; (void)maxs; (void)pass; (void)mask; (void)capsule; (void)flags; (void)lod;
	memset(result, 0, sizeof(*result));
	result->fraction = 1;
	result->startsolid = blocked;
	VectorCopy(end, result->endpos);
}
static int Entities(const vec3_t mins, const vec3_t maxs, int *list, int capacity)
{
	int i;
	(void)mins; (void)maxs; CHECK(capacity >= 4);
	for (i = 0; i < 4; ++i) list[i] = i;
	return 4;
}
gentity_t *CreateMissileNew(vec3_t org, vec3_t dir, float speed, int life,
	gentity_t *owner, qboolean alt, qboolean inheritance, qboolean unlagged)
{
	gentity_t *missile = &g_entities[MAX_CLIENTS];
	CHECK(!alt && !inheritance && !unlagged);
	memset(missile, 0, sizeof(*missile));
	missile->inuse = qtrue;
	missile->parent = owner;
	missile->s.owner = owner->s.number;
	missile->nextthink = level.time + life;
	VectorCopy(org, missile->r.currentOrigin);
	VectorScale(dir, speed, missile->s.pos.trDelta);
	++shots;
	return missile;
}
void G_Damage(gentity_t *target, gentity_t *inflictor, gentity_t *attacker,
	vec3_t dir, vec3_t point, int damage, int flags, int mod)
{
	(void)dir; (void)point;
	CHECK(attacker == &g_entities[0] && inflictor == &g_entities[MAX_CLIENTS]);
	CHECK(mod == MOD_FORCE_DARK && !(flags & DAMAGE_NO_PROTECTION));
	damageTaken[target->s.number] += damage;
	++damageCalls[target->s.number];
}
#include "server_helpers.h"

static gentity_t *Reset(void)
{
	int i;
	memset(g_entities, 0, sizeof(g_entities));
	memset(clients, 0, sizeof(clients));
	memset(&level, 0, sizeof(level));
	memset(dueltypes, 0, sizeof(dueltypes));
	memset(damageTaken, 0, sizeof(damageTaken));
	memset(damageCalls, 0, sizeof(damageCalls));
	shots = covered = blocked = 0;
	level.time = 1000;
	level.gametype = GT_FFA;
	gSiegeRoundBegun = qtrue;
	g_forceDestruction.integer = 1;
	g_forceDestructionCost.integer = 50;
	g_forceDestructionDamage.integer = 90;
	g_forceDestructionRadius.integer = 160;
	g_forceDestructionSpeed.integer = 900;
	g_forceDestructionCooldown.integer = 4000;
	g_maxForceRank.integer = 7;
	g_forcePowerDisable.integer = g_forcePowerDisableFFA.integer = 0;
	g_saberRestrictForce.integer = g_tweakForce.integer = g_fixLightning.integer = 0;
	g_friendlyFire.value = 0;
	g_teamAbsorbScale.value = 1;
	imports.LinkEntity = Link;
	imports.EntitiesInBox = Entities;
	for (i = 0; i < 4; ++i)
	{
		gentity_t *ent = &g_entities[i];
		ent->inuse = ent->takedamage = qtrue;
		ent->s.number = i;
		ent->health = 100;
		ent->client = &clients[i];
		clients[i].pers.connected = CON_CONNECTED;
		clients[i].ps.clientNum = i;
		clients[i].ps.stats[STAT_HEALTH] = 100;
		clients[i].ps.fd.forceSide = FORCE_DARKSIDE;
		clients[i].ps.fd.forcePower = clients[i].ps.fd.forcePowerMax = 100;
		clients[i].ps.viewheight = 24;
		clients[i].ps.weapon = WP_SABER;
	}
	return &g_entities[0];
}

static void CastAndRelease(gentity_t *self)
{
	ForceDestruction(self);
	if (self->client->forceDestructionChargeTime)
	{
		level.time = self->client->forceDestructionChargeTime;
		G_UpdateForceDestruction(self);
	}
}

static void CheckCharge(void)
{
	gentity_t *self = Reset();
	int expiry;
	self->client->ps.fd.forcePowersActive = 1 << FP_SPEED;
	ForceDestruction(self);
	expiry = self->client->forceDestructionChargeTime;
	CHECK(expiry == 1250 && shots == 0);
	CHECK(self->client->ps.fd.forcePower == 50);
	CHECK(self->client->ps.powerups[PW_DISINT_4] > expiry);
	CHECK(self->client->ps.fd.forcePowersActive == ((1 << FP_SPEED) | DESTRUCTION_HAND_FLAG));
	ForceDestruction(self);
	CHECK(shots == 0 && self->client->ps.fd.forcePower == 50);
	level.time = expiry - 1;
	G_UpdateForceDestruction(self);
	CHECK(shots == 0 && (self->client->ps.fd.forcePowersActive & DESTRUCTION_HAND_FLAG));
	// Launch with the current origin/aim and the tuning captured when charging began.
	self->client->ps.origin[0] = 40;
	g_forceDestructionDamage.integer = 1;
	g_forceDestructionRadius.integer = 16;
	g_forceDestructionSpeed.integer = 100;
	level.time = expiry;
	G_UpdateForceDestruction(self);
	CHECK(shots == 1 && self->client->ps.fd.forcePowersActive == (1 << FP_SPEED));
	CHECK(self->client->ps.powerups[PW_DISINT_4] == 0 && self->client->forceDestructionChargeTime == 0);
	CHECK(g_entities[MAX_CLIENTS].r.currentOrigin[0] == 40);
	CHECK(g_entities[MAX_CLIENTS].damage == 90 && g_entities[MAX_CLIENTS].splashRadius == 160);
	CHECK(VectorLength(g_entities[MAX_CLIENTS].s.pos.trDelta) == 900);
	G_UpdateForceDestruction(self);
	CHECK(shots == 1);
}

#define CANCEL(change) do { gentity_t *self = Reset(); ForceDestruction(self); change; level.time = 1250; G_UpdateForceDestruction(self); CHECK(shots == 0 && self->client->forceDestructionChargeTime == 0 && !(self->client->ps.fd.forcePowersActive & DESTRUCTION_HAND_FLAG) && self->client->ps.powerups[PW_DISINT_4] == 0); } while (0)
static void CheckChargeCancellation(void)
{
	CANCEL(self->health = 0);
	CANCEL(g_forceDestruction.integer = 0);
	CANCEL(self->client->ps.fd.forceSide = FORCE_LIGHTSIDE);
	CANCEL(self->client->pers.connected = CON_DISCONNECTED);
	CANCEL(self->client->sess.sessionTeam = TEAM_SPECTATOR);
	CANCEL(self->client->noclip = qtrue);
	CANCEL(self->client->ps.forceHandExtend = HANDEXTEND_KNOCKDOWN);
	CANCEL(self->client->ps.fd.forceDeactivateAll = qtrue);
	CANCEL(self->client->ps.powerups[PW_YSALAMIRI] = 2000);
	CANCEL(self->s.bolt1 = 1);
	CANCEL(level.pause.state = PAUSE_PAUSED);
	CANCEL(level.intermissionQueued = 1);
	CANCEL(blocked = 1);
	{
		gentity_t *self = Reset();
		ForceDestruction(self);
		self->client->ps.forceHandExtend = HANDEXTEND_FORCE_HOLD;
		self->client->ps.powerups[PW_DISINT_4] = 4000;
		self->client->ps.fd.forcePowersActive |= 1 << FP_GRIP;
		G_UpdateForceDestruction(self);
		CHECK(shots == 0 && self->client->forceDestructionChargeTime == 0);
		CHECK(self->client->ps.powerups[PW_DISINT_4] == 4000);
		CHECK(self->client->ps.fd.forcePowersActive == (1 << FP_GRIP));
	}

}

static void CheckGrantAndCast(void)
{
	gentity_t *self = Reset();
	unsigned int preserved = GHOST_KNOWN_FLAG | (1 << FP_SPEED) | (1 << REPULSE_KNOWN_BIT);
	self->client->ps.fd.forcePowersKnown = preserved;
	G_UpdateForceDestruction(self);
	CHECK((unsigned int)self->client->ps.fd.forcePowersKnown == (preserved | DESTRUCTION_KNOWN_FLAG));
	g_forceDestruction.integer = 0;
	G_UpdateForceDestruction(self);
	CHECK((unsigned int)self->client->ps.fd.forcePowersKnown == preserved);
	CastAndRelease(self); CHECK(shots == 0);
	g_forceDestruction.integer = 1;
	self->client->ps.fd.forceSide = FORCE_LIGHTSIDE;
	CastAndRelease(self); CHECK(shots == 0);
	self->client->ps.fd.forceSide = FORCE_DARKSIDE;
	CastAndRelease(self);
	CHECK(shots == 1 && self->client->ps.fd.forcePower == 50);
	CHECK(self->client->forceDestructionCooldown == 5000);
	CHECK(self->client->ps.forceHandExtend == HANDEXTEND_FORCEPUSH);
	CHECK(g_entities[MAX_CLIENTS].s.generic1 == DESTRUCTION_MISSILE_TAG);
	CHECK(g_entities[MAX_CLIENTS].s.weapon == WP_CONCUSSION);
	CHECK(g_entities[MAX_CLIENTS].s.otherEntityNum2 == 1);
	CHECK(g_entities[MAX_CLIENTS].s.emplacedOwner == 2);
	CHECK(castSound.s.eType == ET_EVENTS + EV_GENERAL_SOUND);
	CHECK(castSound.s.eventParm == 1 && castSound.s.saberEntityNum == CHAN_BODY);
	CHECK(castSound.s.weapon == WP_CONCUSSION && castSound.s.generic1 == DESTRUCTION_MISSILE_TAG);
	CHECK(castSound.s.owner == self->s.number && castSound.s.bolt1 == self->s.bolt1);
	CHECK(g_entities[MAX_CLIENTS].nextthink == level.time + 15000);
	self->client->ps.weaponTime = 0;
	self->client->ps.forceHandExtend = HANDEXTEND_NONE;
	CastAndRelease(self); CHECK(shots == 1);
	level.time = 5000;
	CastAndRelease(self); CHECK(shots == 2 && self->client->ps.fd.forcePower == 0);
	CHECK(g_entities[MAX_CLIENTS].nextthink == level.time + 15000);
	level.time = 10000;
	self->client->ps.weaponTime = 0;
	self->client->ps.forceHandExtend = HANDEXTEND_NONE;
	CastAndRelease(self); CHECK(shots == 2);
}

#define REJECT(change) do { gentity_t *self = Reset(); change; CastAndRelease(self); CHECK(shots == 0 && self->client->ps.fd.forcePower == 100); } while (0)
static void CheckRestrictions(void)
{
	REJECT(self->health = 0);
	REJECT(self->client->sess.sessionTeam = TEAM_SPECTATOR);
	REJECT(self->client->ps.pm_flags = PMF_FOLLOW);
	REJECT(self->client->sess.raceMode = qtrue);
	REJECT(self->client->noclip = qtrue);
	REJECT(self->client->pers.connected = CON_DISCONNECTED);
	REJECT(self->client->ps.forceRestricted = qtrue);
	REJECT(self->client->ps.trueNonJedi = qtrue);
	REJECT(self->client->ps.m_iVehicleNum = 100);
	REJECT(self->client->ps.powerups[PW_YSALAMIRI] = 10000);
	REJECT(self->client->ps.heldByClient = 1);
	REJECT(self->client->ps.torsoAnim = BOTH_PLAYER_PA_3);
	REJECT(self->client->ps.saberLockTime = 2000);
	REJECT(self->client->ps.brokenLimbs = 1 << BROKENLIMB_RARM);
	REJECT(self->client->ps.duelInProgress = qtrue);
	REJECT(self->client->ps.fd.forcePowersActive = 1 << FP_LIGHTNING);
	REJECT(self->client->ps.weaponTime = 1);
	REJECT(level.pause.state = PAUSE_PAUSED);
	REJECT(level.intermissionQueued = 1);
	REJECT(level.gametype = GT_SIEGE; gSiegeRoundBegun = qfalse);
	REJECT(g_maxForceRank.integer = 0);
	REJECT(g_forcePowerDisable.integer = (1 << NUM_FORCE_POWERS) - 1);
	REJECT(g_forcePowerDisableFFA.integer = DESTRUCTION_KNOWN_FLAG);
	REJECT(g_saberRestrictForce.integer = 1; self->client->saber[0].saberFlags = SFL_TWO_HANDED);
	REJECT(blocked = 1);
}

static void CheckDamage(void)
{
	gentity_t *self = Reset(), *missile = &g_entities[MAX_CLIENTS], *target = &g_entities[1];
	trace_t impact = {0};
	vec3_t origin = {0}, direction = {1, 0, 0};
	CastAndRelease(self);
	impact.entityNum = 1;
	impact.plane.normal[2] = 1;
	VectorSet(g_entities[2].r.absmin, 80, 0, 0);
	VectorSet(g_entities[2].r.absmax, 80, 0, 0);
	covered = 1 << 3;
	G_ForceDestructionImpact(missile, &impact);
	CHECK(damageTaken[1] == 90 && damageCalls[1] == 1);
	CHECK(damageTaken[2] == 45 && damageTaken[3] == 0 && damageTaken[0] == 90);
	CHECK(missile->freeAfterEvent && missile->s.event == EV_MISSILE_MISS);
	CHECK(missile->s.weapon == WP_CONCUSSION && missile->s.emplacedOwner == 2);

	memset(damageTaken, 0, sizeof(damageTaken));
	target->client->ps.fd.forcePower = 0;
	target->client->ps.fd.forcePowersActive = 1 << FP_ABSORB;
	target->client->ps.fd.forcePowerLevel[FP_ABSORB] = 3;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 0 && target->client->ps.fd.forcePower == 48);
	target->client->ps.fd.forcePowerLevel[FP_ABSORB] = 1;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 60);
	target->client->ps.fd.forcePowerLevel[FP_ABSORB] = 2;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 90);
	target->client->ps.fd.forcePowersActive = 0;
	target->client->ps.powerups[PW_YSALAMIRI] = 1000;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 90);
	target->client->ps.powerups[PW_YSALAMIRI] = 0;
	target->s.bolt1 = 2;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 90);
	target->s.bolt1 = 0;
	target->client->ps.duelInProgress = qtrue;
	target->client->ps.duelIndex = 2;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 90);
	target->client->ps.duelInProgress = qfalse;
	target->client->sess.sessionTeam = self->client->sess.sessionTeam = TEAM_RED;
	DestructionDamage(missile, target, origin, direction, 90, qfalse);
	CHECK(damageTaken[1] == 90);
}

static void CheckTuningAndFullForceDuel(void)
{
	gentity_t *self = Reset(), *missile = &g_entities[MAX_CLIENTS];
	self->client->ps.duelInProgress = qtrue;
	dueltypes[0] = 1;
	g_forceDestructionCost.integer = -10;
	g_forceDestructionDamage.integer = 1000;
	g_forceDestructionRadius.integer = 1000;
	g_forceDestructionSpeed.integer = 10000;
	g_forceDestructionCooldown.integer = 0;
	CastAndRelease(self);
	CHECK(shots == 1 && self->client->ps.fd.forcePower == 99);
	CHECK(self->client->forceDestructionCooldown == 1500);
	CHECK(missile->damage == 500 && missile->splashRadius == 512);
	CHECK(VectorLength(missile->s.pos.trDelta) == 3000);
	g_forceDestructionDamage.integer = 1;
	g_forceDestructionRadius.integer = 16;
	CHECK(missile->damage == 500 && missile->splashRadius == 512);
}

int main(void)
{
	CheckCharge();
	CheckChargeCancellation();
	CheckGrantAndCast();
	CheckRestrictions();
	CheckDamage();
	CheckTuningAndFullForceDuel();
	puts("Destruction server grant, casting, restrictions, splash/cover and Absorb checks passed.");
	return 0;
}
