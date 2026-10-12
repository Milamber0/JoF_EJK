/*
 * Force Destruction: a server-authoritative, Legends-inspired dark-side blast.
 * Distributed under the GNU General Public License, version 2 or later.
 */
#include "g_local.h"

extern qboolean gSiegeRoundBegun;

#define DESTRUCTION_CHARGE 250
#define DESTRUCTION_RECOVERY 650
#define DESTRUCTION_LIFETIME 15000

static int DestructionClamp(int value, int low, int high)
{
	return value < low ? low : (value > high ? high : value);
}

static qboolean DestructionGranted(gentity_t *self)
{
	const unsigned int realPowers = (1u << NUM_FORCE_POWERS) - 1;
	unsigned int disabled = g_forcePowerDisable.integer;
	playerState_t *ps;

	if (!self || !self->inuse || !self->client || self->s.number >= MAX_CLIENTS ||
		self->client->pers.connected != CON_CONNECTED || !g_forceDestruction.integer)
		return qfalse;

	ps = &self->client->ps;
	if (!ps->duelInProgress)
		disabled |= g_forcePowerDisableFFA.integer;
	return self->health > 0 && ps->stats[STAT_HEALTH] > 0 &&
		ps->fd.forceSide == FORCE_DARKSIDE && ps->pm_type == PM_NORMAL &&
		!(ps->eFlags & EF_DEAD) && !(ps->pm_flags & PMF_FOLLOW) &&
		self->client->sess.sessionTeam != TEAM_SPECTATOR &&
		self->client->tempSpectate < level.time && !self->client->sess.raceMode &&
		!self->client->noclip && !ps->forceRestricted && !ps->trueNonJedi &&
		g_maxForceRank.integer > 0 && !(disabled & DESTRUCTION_KNOWN_FLAG) &&
		(disabled & realPowers) != realPowers;
}

static qboolean DestructionCanCast(gentity_t *self, qboolean charging)
{
	playerState_t *ps;
	int cost = DestructionClamp(g_forceDestructionCost.integer, 1, 100);
	// Recheck entitlement here: neither a stale snapshot nor a forged command is authority.
	if (!DestructionGranted(self) || level.intermissiontime || level.intermissionQueued ||
		level.pause.state != PAUSE_NONE || (level.gametype == GT_SIEGE && !gSiegeRoundBegun))
		return qfalse;

	ps = &self->client->ps;
	// Borrow only the shared offensive-power restrictions, not Lightning's rank/cost.
	if (!BG_CanUseFPNow(level.gametype, ps, level.time, FP_LIGHTNING) ||
		ps->fd.forceDeactivateAll || ps->fd.forceGripCripple ||
		ps->forceHandExtend != (charging ? HANDEXTEND_FORCEPUSH : HANDEXTEND_NONE) ||
		(!charging && ps->weaponTime > 0) ||
		(ps->pm_flags & PMF_STUCK_TO_WALL) || ps->heldByClient ||
		BG_InGrappleMove(ps->legsAnim) || BG_InGrappleMove(ps->torsoAnim) ||
		(ps->fd.forcePowersActive & ((1 << FP_GRIP) | (1 << FP_LIGHTNING) | (1 << FP_DRAIN))) ||
		(!charging && (self->client->forceDestructionCooldown > level.time || ps->fd.forcePower < cost)))
		return qfalse;

	if (!ps->saberHolstered &&
		((g_saberRestrictForce.integer &&
			((self->client->saber[0].saberFlags & SFL_TWO_HANDED) || self->client->saber[1].model[0])) ||
		((self->client->saber[0].forceRestrictions | self->client->saber[1].forceRestrictions) & (1 << FP_LIGHTNING))))
		return qfalse;

	return qtrue;
}

static qboolean DestructionLaunchOrigin(gentity_t *self, vec3_t start, vec3_t forward)
{
	trace_t trace;
	vec3_t mins = {-5, -5, -5}, maxs = {5, 5, 5};
	playerState_t *ps = &self->client->ps;
	AngleVectors(ps->viewangles, forward, NULL, NULL);
	VectorCopy(ps->origin, start);
	start[2] += ps->viewheight;
	// Trace again at release: the caster may have moved during the charge.
	JP_Trace(&trace, ps->origin, mins, maxs, start, self->s.number,
		MASK_SOLID | CONTENTS_SHOTCLIP, qfalse, 0, 0);
	if (trace.startsolid || trace.allsolid)
		return qfalse;
	VectorCopy(trace.endpos, start);
	return qtrue;
}

static void DestructionClearCharge(gentity_t *self)
{
	// Do not erase a newer Push/Grip effect that interrupted this charge.
	if (self->client->ps.powerups[PW_DISINT_4] == self->client->forceDestructionChargeTime + 1)
		self->client->ps.powerups[PW_DISINT_4] = 0;
	self->client->forceDestructionChargeTime = 0;
	self->client->ps.fd.forcePowersActive &= ~DESTRUCTION_HAND_FLAG;
}

static void DestructionLaunch(gentity_t *self)
{
	gentity_t *missile;
	vec3_t start, forward;
	vec3_t mins = {-5, -5, -5}, maxs = {5, 5, 5};
	if (!DestructionLaunchOrigin(self, start, forward))
		return;
	missile = CreateMissileNew(start, forward,
		self->client->forceDestructionSpeed,
		DESTRUCTION_LIFETIME, self, qfalse, qfalse, qfalse);
	missile->classname = "force_destruction";
	missile->s.weapon = WP_CONCUSSION; // compatible fallback for older clients
	missile->mass = 10;
	missile->s.generic1 = DESTRUCTION_MISSILE_TAG;
	missile->s.bolt1 = self->s.bolt1;
	missile->s.pos.trTime = level.time; // no projectile prestep across the spawn trace
	// Only advertise stock assets. Updated clients replace these locally, while
	// vanilla clients can render the attack even on maps without concussion items.
	missile->s.otherEntityNum2 = G_EffectIndex("concussion/shot");
	missile->s.emplacedOwner = G_EffectIndex("concussion/explosion");
	missile->damage = self->client->forceDestructionDamage;
	missile->splashDamage = missile->damage;
	missile->splashRadius = self->client->forceDestructionRadius;
	missile->methodOfDeath = missile->splashMethodOfDeath = MOD_FORCE_DARK;
	missile->clipmask = MASK_SHOT; // an energy blast is not a saber-deflectable blaster bolt
	missile->count = self->client->forceDestructionCost; // snapshot the spent energy for Absorb, even if cvars change in flight
	VectorCopy(mins, missile->r.mins);
	VectorCopy(maxs, missile->r.maxs);
	trap->LinkEntity((sharedEntity_t *)missile);
}

void G_UpdateForceDestruction(gentity_t *self)
{
	if (!self || !self->client)
		return;
	if (DestructionGranted(self))
		self->client->ps.fd.forcePowersKnown |= DESTRUCTION_KNOWN_FLAG;
	else
		self->client->ps.fd.forcePowersKnown &= ~DESTRUCTION_KNOWN_FLAG;
	if (!self->client->forceDestructionChargeTime)
		return;
	if (!DestructionCanCast(self, qtrue) ||
		self->s.bolt1 != self->client->forceDestructionDimension ||
		!(self->client->ps.fd.forcePowersActive & DESTRUCTION_HAND_FLAG))
	{
		DestructionClearCharge(self);
		return;
	}
	if (level.time >= self->client->forceDestructionChargeTime)
	{
		DestructionClearCharge(self);
		DestructionLaunch(self);
	}
}

void ForceDestruction(gentity_t *self)
{
	playerState_t *ps;
	gentity_t *sound;
	vec3_t start, forward;
	int cost = DestructionClamp(g_forceDestructionCost.integer, 1, 100);
	int cooldown = DestructionClamp(g_forceDestructionCooldown.integer, 500, 30000);
	if (!DestructionCanCast(self, qfalse) || self->client->forceDestructionChargeTime ||
		!DestructionLaunchOrigin(self, start, forward))
		return;
	ps = &self->client->ps;

	ps->fd.forcePower -= cost;
	ps->fd.forcePowerRegenDebounceTime = level.time + 1000;
	self->client->forceDestructionCooldown = level.time + cooldown;
	self->client->forceDestructionChargeTime = level.time + DESTRUCTION_CHARGE;
	self->client->forceDestructionCost = cost;
	self->client->forceDestructionDamage = DestructionClamp(g_forceDestructionDamage.integer, 1, 500);
	self->client->forceDestructionRadius = DestructionClamp(g_forceDestructionRadius.integer, 16, 512);
	self->client->forceDestructionSpeed = DestructionClamp(g_forceDestructionSpeed.integer, 100, 3000);
	self->client->forceDestructionDimension = self->s.bolt1;
	ps->fd.forcePowersActive |= DESTRUCTION_HAND_FLAG;
	ps->powerups[PW_DISINT_4] = self->client->forceDestructionChargeTime + 1;
	ps->forceHandExtend = HANDEXTEND_FORCEPUSH;
	ps->forceHandExtendTime = level.time + DESTRUCTION_RECOVERY;
	ps->weaponTime = DESTRUCTION_RECOVERY;
	self->client->dangerTime = level.time;
	ps->eFlags &= ~EF_INVULNERABLE;
	self->client->invulnerableTimer = 0;
	// Vanilla clients hear Force Push; updated clients can substitute the pack's
	// casting sound without advertising an unavailable custom sound to everyone.
	sound = G_TempEntity(self->r.currentOrigin, EV_GENERAL_SOUND);
	sound->s.eventParm = G_SoundIndex("sound/weapons/force/push.wav");
	sound->s.saberEntityNum = CHAN_BODY;
	sound->s.weapon = WP_CONCUSSION;
	sound->s.generic1 = DESTRUCTION_MISSILE_TAG;
	sound->s.owner = self->s.number;
	sound->s.bolt1 = self->s.bolt1;
}

static void DestructionDamage(gentity_t *missile, gentity_t *target,
	vec3_t origin, vec3_t direction, int damage, qboolean splash)
{
	gentity_t *attacker = missile->parent;
	int remaining;

	if (!attacker || !attacker->inuse || !target->inuse || !target->takedamage || damage <= 0)
		return;
	if (attacker->s.bolt1 != missile->s.bolt1 || (attacker->client &&
		(attacker->client->pers.connected != CON_CONNECTED || attacker->client->noclip || attacker->client->sess.raceMode)))
		return;
	if (target->client)
	{
		// Do these before Absorb or knockback: protected bystanders must be untouched.
		if (target->s.bolt1 != missile->s.bolt1 || target->client->noclip ||
			target->client->sess.sessionTeam == TEAM_SPECTATOR || target->health <= 0 ||
			target->client->tempSpectate >= level.time ||
			(target->client->sess.raceMode && !target->client->ps.duelInProgress) ||
			(target->flags & FL_GODMODE) || (target->client->ps.eFlags & EF_INVULNERABLE) ||
			BG_HasYsalamiri(level.gametype, &target->client->ps))
			return;
		if (attacker->client &&
			((target->client->ps.duelInProgress && target->client->ps.duelIndex != attacker->s.number) ||
			(attacker->client->ps.duelInProgress && attacker->client->ps.duelIndex != target->s.number)))
			return;
		if (target != attacker && OnSameTeam(target, attacker) && g_friendlyFire.value <= 0)
			return;

		if (target != attacker && (target->client->ps.fd.forcePowersActive & (1 << FP_ABSORB)))
		{
			// Treat the blast as level-three Force energy. Use Push's conversion path
			// so Lightning-specific server tweaks cannot alter Destruction's defenses.
			remaining = WP_AbsorbConversion(target, target->client->ps.fd.forcePowerLevel[FP_ABSORB],
				attacker, FP_PUSH, FORCE_LEVEL_3, missile->count * damage / missile->damage);
			if (remaining >= 0)
				damage = damage * remaining / FORCE_LEVEL_3;
			if (damage <= 0)
				return;
		}
	}
	// Keep normal armor, Protect, friendly-fire scaling, damage attribution and knockback.
	G_Damage(target, missile, attacker, direction, origin, damage,
		splash ? DAMAGE_RADIUS : 0, MOD_FORCE_DARK);
}

void G_ForceDestructionImpact(gentity_t *missile, trace_t *trace)
{
	gentity_t *direct = &g_entities[trace->entityNum];
	int entities[MAX_GENTITIES], count, i, axis;
	vec3_t mins, maxs, delta, direction;
	float distance;

	VectorCopy(missile->s.pos.trDelta, direction);
	DestructionDamage(missile, direct, trace->endpos, direction, missile->damage, qfalse);
	for (axis = 0; axis < 3; ++axis)
	{
		mins[axis] = trace->endpos[axis] - missile->splashRadius;
		maxs[axis] = trace->endpos[axis] + missile->splashRadius;
	}
	count = trap->EntitiesInBox(mins, maxs, entities, MAX_GENTITIES);
	for (i = 0; i < count; ++i)
	{
		gentity_t *target = &g_entities[entities[i]];
		if (target == direct || !target->takedamage)
			continue; // never stack direct damage with splash on the same target
		for (axis = 0; axis < 3; ++axis)
		{
			delta[axis] = trace->endpos[axis] < target->r.absmin[axis] ?
				target->r.absmin[axis] - trace->endpos[axis] :
				(trace->endpos[axis] > target->r.absmax[axis] ? trace->endpos[axis] - target->r.absmax[axis] : 0);
		}
		distance = VectorLength(delta);
		if (distance >= missile->splashRadius || !CanDamage(target, trace->endpos))
			continue;
		VectorSubtract(target->r.currentOrigin, trace->endpos, direction);
		direction[2] += 24;
		DestructionDamage(missile, target, trace->endpos, direction,
			(int)(missile->splashDamage * (1.0f - distance / missile->splashRadius)), qtrue);
	}

	G_AddEvent(missile, EV_MISSILE_MISS, DirToByte(trace->plane.normal));
	missile->freeAfterEvent = qtrue;
	missile->s.eType = ET_GENERAL;
	missile->takedamage = qfalse;
	G_SetOrigin(missile, trace->endpos);
	trap->LinkEntity((sharedEntity_t *)missile);
}
