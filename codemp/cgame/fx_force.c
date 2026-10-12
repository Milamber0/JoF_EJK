/*
===========================================================================
Copyright (C) 2000 - 2013, Raven Software, Inc.
Copyright (C) 2001 - 2013, Activision, Inc.
Copyright (C) 2013 - 2015, OpenJK contributors

This file is part of the OpenJK source code.

OpenJK is free software; you can redistribute it and/or modify it
under the terms of the GNU General Public License version 2 as
published by the Free Software Foundation.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program; if not, see <http://www.gnu.org/licenses/>.
===========================================================================
*/

// Any dedicated force oriented effects

#include "cg_local.h"
#include "fx_local.h"

static void FX_ForceLightningReference(centity_t *cent, vec3_t origin, matrix3_t axis, qboolean wide) {
	int i;
	int arcs = wide ? 5 : 2;
	float spread = wide ? 0.8f : 0.5f;
	float arcAngle = Com_Clamp(0.0f, 360.0f, cg_lightningEnvironmentAngle.value);
	float minForwardDot = cosf(DEG2RAD(arcAngle * 0.5f));
	float traceDistance, directionLength;
	fxHandle_t mainEffect = wide ? cgs.effects.forceLightningReferenceWide : cgs.effects.forceLightningReference;
	vec3_t direction, end, angles;
	trace_t tr;

	if (!mainEffect)
		mainEffect = wide ? cgs.effects.forceLightningWide : cgs.effects.forceLightning;
	trap->FX_PlayEntityEffectID(mainEffect, origin, axis, -1, -1, -1, -1);

	for (i = 0; i < arcs; i++) {
		if (cent->lightningReferenceTime[i] < cg.time) {
			VectorCopy(axis[0], direction);
			direction[0] += 2.0f * ((rand() & 0x7fff) / 32767.0f - 0.5f) * spread;
			direction[1] += 2.0f * ((rand() & 0x7fff) / 32767.0f - 0.5f) * spread;
			direction[2] += 2.0f * ((rand() & 0x7fff) / 32767.0f - 0.5f) * spread;
			traceDistance = 350.0f;
		} else {
			VectorSubtract(origin, cent->lightningReferenceEnd[i], angles);
			AngleVectors(angles, direction, NULL, NULL);
			angles[ROLL] = 0.0f;
			AngleVectors(angles, direction, NULL, NULL);
			traceDistance = 200.0f;
		}

		if (arcAngle < 360.0f) {
			directionLength = VectorLength(direction);
			// Redirect rearward/over-wide bolts toward the player's current aim.

			if (directionLength < 0.0001f)
				VectorCopy(axis[0], direction);
			else if (DotProduct(direction, axis[0]) < minForwardDot * directionLength)
				VectorScale(axis[0], directionLength, direction);
		}
		VectorMA(origin, traceDistance, direction, end);

		CG_Trace(&tr, origin, NULL, NULL, end, -1, MASK_SOLID);
		if (tr.fraction >= 1.0f)
			continue;

		if (cent->lightningReferenceTime[i] < cg.time) {
			// Cache the requested endpoint, not the trace's surface contact.
			VectorCopy(end, cent->lightningReferenceEnd[i]);
			cent->lightningReferenceTime[i] = cg.time + Q_irand(500, 1500);
		}
		if (cg.frametime > 0 && (cg.frametime >= 50 || cg.time % 50 <= cg.frametime)) {
			trap->FX_PlayEffectID(cgs.effects.forceLightningReferenceArc ?
				cgs.effects.forceLightningReferenceArc : cgs.effects.forceLightningBranch,
				origin, direction, -1, -1, qfalse);
			// Pack arcs own their impact effect; only the fallback needs this one.
			if (!cgs.effects.forceLightningReferenceArc && !tr.startsolid && !tr.allsolid &&
				!(tr.surfaceFlags & (SURF_SKY | SURF_NOIMPACT | SURF_NODRAW))) {
				trap->FX_PlayEffectID(cgs.effects.demp2WallImpactEffectSmall,
					tr.endpos, tr.plane.normal, -1, -1, qfalse);
			}
		}
		if (cent->lightningReferenceSoundTime[i] < cg.time) {
			cent->lightningReferenceSoundTime[i] = cg.time + Q_irand(500, 750);
			trap->S_StartSound(end, cent->currentState.number, CHAN_AUTO,
				cgs.media.forceLightningEnvironmentSounds[Q_irand(0, ARRAY_LEN(cgs.media.forceLightningEnvironmentSounds) - 1)]);
		}
	}
}

// Shared per-window budget so trace/effect work stays bounded at high FPS.
#define LIGHTNING_INTERVAL 40
#define LIGHTNING_EMIT_INTERVAL 10
#define LIGHTNING_TRACE_BUDGET 192
#define LIGHTNING_EFFECT_BUDGET 256
#define LIGHTNING_SOUND_INTERVAL 200
#define LIGHTNING_SOUND_BUDGET 6
static int lightningBudgetTime;
static int lightningTraces;
static int lightningEffects;
static int lightningSoundBudgetTime;
static int lightningSounds;

static qboolean FX_LightningTrace(trace_t *tr, vec3_t start, vec3_t end, int owner) {
	if (lightningTraces >= LIGHTNING_TRACE_BUDGET)
		return qfalse;
	lightningTraces++;
	CG_Trace(tr, start, NULL, NULL, end, owner, MASK_SHOT);
	return !tr->startsolid && !tr->allsolid;
}

static qboolean FX_LightningSurface(const trace_t *tr) {
	if (tr->fraction == 1.0f || tr->startsolid || tr->allsolid ||
		(tr->surfaceFlags & (SURF_SKY | SURF_NOIMPACT | SURF_NODRAW)))
		return qfalse;
	return tr->entityNum == ENTITYNUM_WORLD ||
		(tr->entityNum >= 0 && tr->entityNum < ENTITYNUM_WORLD &&
		 cg_entities[tr->entityNum].currentState.solid == SOLID_BMODEL);
}

// Draws a thin electricity arc between two points. Only used for the nest
// link now; the main beam and nest strikes use native engine effects.
static void FX_LightningArc(const vec3_t start, const vec3_t end, float width, float chaos, qboolean mainBolt) {
	addElectricityArgStruct_t arc;
	vec3_t delta;
	VectorSubtract(end, start, delta);
	if (lightningEffects >= LIGHTNING_EFFECT_BUDGET || VectorLengthSquared(delta) < 1.0f)
		return;
	lightningEffects++;
	memset(&arc, 0, sizeof(arc));
	VectorCopy(start, arc.start);
	VectorCopy(end, arc.end);
	VectorSet(arc.sRGB, 1.0f, 1.0f, 1.0f);
	VectorCopy(arc.sRGB, arc.eRGB);
	arc.size1 = mainBolt ? width : width * 0.5f;
	arc.size2 = mainBolt ? 1.0f : 0.5f;
	// Dim the start for non-main bolts so the branch point doesn't look like
	// its own little flash; the end stays bright since it's the visible hit.
	arc.alpha1 = mainBolt ? 1.0f : 0.25f;
	arc.alpha2 = 1.0f;
	arc.chaos = chaos;
	arc.killTime = mainBolt ? 65 : 55;
	arc.shader = cgs.media.forceLightningArcShader;
	arc.flags = FX_ALPHA_LINEAR | FX_SIZE_LINEAR;
	if (mainBolt)
		arc.flags |= 0x01000000 | 0x02000000 | 0x04000000; // TAPER/BRANCH/GROW
	trap->FX_AddElectricity(&arc);
}

static void FX_LightningFlash(vec3_t origin, float size) {
	addspriteArgStruct_t flash;
	if (lightningEffects >= LIGHTNING_EFFECT_BUDGET)
		return;
	lightningEffects++;
	memset(&flash, 0, sizeof(flash));
	VectorCopy(origin, flash.origin);
	flash.scale = size;
	flash.sAlpha = 0.7f;
	flash.life = LIGHTNING_INTERVAL;
	flash.shader = cgs.media.forceLightningFlashShader;
	flash.flags = FX_ALPHA_LINEAR;
	trap->FX_AddSprite(&flash);
}

// --- Nests ---------------------------------------------------------------
// A nest is a short-lived point on a wall/floor/ceiling that the main beam
// periodically strikes: a thin link arc keeps it visibly connected, and a
// stronger "strike" (native lightning effect + DEMP2 impact) fires on its
// own timer. Nests live until expireTime, refilling their strike queue as
// needed instead of dying when strikes run out.
#define LIGHTNING_MAX_NESTS 8
#define LIGHTNING_NESTS_PER_OWNER 3
#define LIGHTNING_NEST_MIN_LIFE 120
#define LIGHTNING_NEST_MAX_LIFE 280
#define LIGHTNING_NEST_SCATTER_RADIUS 280.0f
#define LIGHTNING_NEST_SCATTER_ATTEMPTS 4
#define LIGHTNING_NEST_SCATTER_MIN_DIST 90.0f
// Minimum dot(direction, forward) on the HORIZONTAL plane only for a
// scatter candidate to be accepted - keeps side/rear wall strikes to a
// forward-biased cone (0.4 = within ~66 degrees of straight ahead) while
// never restricting floor/ceiling strikes, which have little/no horizontal
// component and are always allowed (see FX_LightningDirectionAllowed).
#define LIGHTNING_NEST_FORWARD_MIN_DOT 0.4f
// Ceiling candidates almost never fail the vertical-direction check above
// and indoor ceilings are usually close/easy to hit, so without this they
// land far more often than walls or floor. This extra coin-flip thins them
// out - lower = rarer ceiling strikes.
#define LIGHTNING_NEST_CEILING_CHANCE 0.1f
// Caps how steeply a wall-strike direction may tilt upward (dir.z). 0.3 =
// roughly 17 degrees above level. Without this, wall candidates could
// still aim steeply up toward the ceiling transition and land high on the
// wall even though they're technically "walls", not ceiling.
#define LIGHTNING_NEST_WALL_MAX_UP 0.55f
#define LIGHTNING_NEST_SPAWN_CHANCE 0.7f
#define LIGHTNING_NEST_MIN_STRIKES 10
#define LIGHTNING_NEST_MAX_STRIKES 18
#define LIGHTNING_NEST_STRIKE_MIN_GAP 15
#define LIGHTNING_NEST_STRIKE_MAX_GAP 45
#define LIGHTNING_NEST_LINK_WIDTH 4.0f
#define LIGHTNING_NEST_LINK_CHAOS_MIN 1.8f
#define LIGHTNING_NEST_LINK_CHAOS_MAX 3.2f
// Kept close to the arc's own killTime (55ms) so redraws don't overlap.
#define LIGHTNING_NEST_LINK_INTERVAL 55
#define LIGHTNING_NEST_LINK_ORIGIN_JITTER 6.0f
// Branch point is a random fraction along the main beam's own segment
// (hand -> tip), not always the tip - keeps it close to the hand here.
#define LIGHTNING_NEST_BRANCH_FRAC_MIN 0.03f
#define LIGHTNING_NEST_BRANCH_FRAC_MAX 0.2f
// Slow surface crawl so a nest isn't perfectly static for its whole life.
#define LIGHTNING_NEST_CRAWL_SPEED 6.0f
#define LIGHTNING_NEST_CRAWL_INTERVAL 90
#define LIGHTNING_NEST_CRAWL_TURN 0.5f
#define LIGHTNING_NEST_CRAWL_PROBE 24.0f
// Throttle for a nest's own impact sound - independent of strike frequency,
// so a short-lived, fast-striking nest plays roughly one sound, not one
// per strike.
#define LIGHTNING_NEST_SOUND_INTERVAL 200

typedef struct {
	qboolean active;
	int owner;
	vec3_t pos;
	vec3_t normal;
	vec3_t crawlDir;
	int expireTime;
	int strikesLeft;
	int nextStrikeTime;
	int nextLinkTime;
	int nextCrawlTime;
	int nextSoundTime;
} lightningNest_t;

static lightningNest_t lightningNests[LIGHTNING_MAX_NESTS];

static int FX_LightningCountOwnerNests(int owner) {
	int i, n = 0;
	for (i = 0; i < LIGHTNING_MAX_NESTS; i++)
		if (lightningNests[i].active && lightningNests[i].owner == owner)
			n++;
	return n;
}

static void FX_LightningExpireNests(int owner) {
	int i;
	for (i = 0; i < LIGHTNING_MAX_NESTS; i++)
		if (lightningNests[i].active && lightningNests[i].owner == owner &&
			cg.time >= lightningNests[i].expireTime)
			lightningNests[i].active = qfalse;
}

// True if dir is either near-vertical (floor always allowed; ceiling
// allowed but thinned out via LIGHTNING_NEST_CEILING_CHANCE) or within a
// forward-biased horizontal cone of forward AND not tilted too steeply
// upward (walls - restricted, so strikes don't land behind, at sharp side
// angles, or high up near the ceiling transition).
static qboolean FX_LightningDirectionAllowed(vec3_t dir, vec3_t forward) {
	vec3_t dirFlat, fwdFlat;
	VectorCopy(dir, dirFlat); dirFlat[2] = 0;

	if (VectorLengthSquared(dirFlat) < 0.05f) {
		if (dir[2] > 0.0f && Q_flrand(0.0f, 1.0f) > LIGHTNING_NEST_CEILING_CHANCE)
			return qfalse;
		return qtrue;
	}

	if (dir[2] > LIGHTNING_NEST_WALL_MAX_UP)
		return qfalse;

	VectorCopy(forward, fwdFlat); fwdFlat[2] = 0;
	VectorNormalize(dirFlat);
	if (VectorNormalize(fwdFlat) < 0.05f)
		return qtrue;
	return DotProduct(dirFlat, fwdFlat) >= LIGHTNING_NEST_FORWARD_MIN_DOT;
}

// Probes random 3D directions from the hand for a valid surface point.
static qboolean FX_LightningScatterNest(vec3_t origin, vec3_t forward, int owner, vec3_t outPos, vec3_t outNormal) {
	int attempt;
	vec3_t dir, end;
	trace_t tr;

	for (attempt = 0; attempt < LIGHTNING_NEST_SCATTER_ATTEMPTS; attempt++) {
		dir[0] = Q_flrand(-1.0f, 1.0f);
		dir[1] = Q_flrand(-1.0f, 1.0f);
		dir[2] = Q_flrand(-1.0f, 1.0f);
		if (VectorNormalize(dir) < 0.1f)
			continue;
		if (!FX_LightningDirectionAllowed(dir, forward))
			continue;

		VectorMA(origin, LIGHTNING_NEST_SCATTER_RADIUS, dir, end);
		if (!FX_LightningTrace(&tr, origin, end, owner) || !FX_LightningSurface(&tr))
			continue;
		if (tr.fraction * LIGHTNING_NEST_SCATTER_RADIUS < LIGHTNING_NEST_SCATTER_MIN_DIST)
			continue;

		VectorMA(tr.endpos, 2.0f, tr.plane.normal, outPos);
		VectorCopy(tr.plane.normal, outNormal);
		return qtrue;
	}
	return qfalse;
}

static void FX_LightningSpawnNest(vec3_t origin, vec3_t forward, int owner) {
	int slot, s;
	vec3_t pos, nrm;

	if (FX_LightningCountOwnerNests(owner) >= LIGHTNING_NESTS_PER_OWNER)
		return;
	if (Q_flrand(0.0f, 1.0f) > LIGHTNING_NEST_SPAWN_CHANCE)
		return;
	if (!FX_LightningScatterNest(origin, forward, owner, pos, nrm))
		return;

	slot = -1;
	for (s = 0; s < LIGHTNING_MAX_NESTS; s++)
		if (!lightningNests[s].active) { slot = s; break; }
	if (slot < 0)
		return;

	lightningNests[slot].active = qtrue;
	lightningNests[slot].owner = owner;
	VectorCopy(pos, lightningNests[slot].pos);
	VectorCopy(nrm, lightningNests[slot].normal);
	{
		vec3_t tangent, side;
		PerpendicularVector(tangent, nrm);
		CrossProduct(nrm, tangent, side);
		VectorScale(tangent, Q_flrand(-1.0f, 1.0f), lightningNests[slot].crawlDir);
		VectorMA(lightningNests[slot].crawlDir, Q_flrand(-1.0f, 1.0f), side, lightningNests[slot].crawlDir);
		if (VectorNormalize(lightningNests[slot].crawlDir) < 0.01f)
			VectorCopy(tangent, lightningNests[slot].crawlDir);
	}
	lightningNests[slot].expireTime = cg.time + (int)Q_flrand(LIGHTNING_NEST_MIN_LIFE, LIGHTNING_NEST_MAX_LIFE);
	lightningNests[slot].strikesLeft = LIGHTNING_NEST_MIN_STRIKES +
		(rand() % (LIGHTNING_NEST_MAX_STRIKES - LIGHTNING_NEST_MIN_STRIKES + 1));
	lightningNests[slot].nextStrikeTime = cg.time;
	lightningNests[slot].nextLinkTime = cg.time;
	lightningNests[slot].nextCrawlTime = cg.time + LIGHTNING_NEST_CRAWL_INTERVAL;
	lightningNests[slot].nextSoundTime = cg.time;
}

// Slides a nest along its surface, re-probing to stay stuck to it (or
// reversing direction if it walks off the edge).
static void FX_LightningCrawlNest(lightningNest_t *nest, int owner) {
	vec3_t tangent, side, candidate, probeStart, probeEnd;
	trace_t tr;
	float angle, c, s, dt, ds;

	if (lightningTraces >= LIGHTNING_TRACE_BUDGET)
		return;

	PerpendicularVector(tangent, nest->normal);
	CrossProduct(nest->normal, tangent, side);
	angle = Q_flrand(-LIGHTNING_NEST_CRAWL_TURN, LIGHTNING_NEST_CRAWL_TURN);
	c = cosf(angle);
	s = sinf(angle);
	dt = DotProduct(nest->crawlDir, tangent);
	ds = DotProduct(nest->crawlDir, side);
	VectorScale(tangent, dt * c - ds * s, nest->crawlDir);
	VectorMA(nest->crawlDir, dt * s + ds * c, side, nest->crawlDir);
	VectorNormalize(nest->crawlDir);

	VectorMA(nest->pos, LIGHTNING_NEST_CRAWL_SPEED * (LIGHTNING_NEST_CRAWL_INTERVAL * 0.001f),
		nest->crawlDir, candidate);

	VectorMA(candidate, LIGHTNING_NEST_CRAWL_PROBE, nest->normal, probeStart);
	VectorMA(candidate, -LIGHTNING_NEST_CRAWL_PROBE, nest->normal, probeEnd);
	if (!FX_LightningTrace(&tr, probeStart, probeEnd, owner) || !FX_LightningSurface(&tr)) {
		VectorScale(nest->crawlDir, -1.0f, nest->crawlDir);
		return;
	}
	// A ceiling surface's normal points downward (normal.z negative). The
	// spawn-time ceiling gate (LIGHTNING_NEST_CEILING_CHANCE) only stops a
	// nest from being BORN on the ceiling - without this check it could
	// still crawl onto one from an adjoining wall near a corner. Bounce
	// back instead of continuing onto it.
	if (tr.plane.normal[2] < -0.5f) {
		VectorScale(nest->crawlDir, -1.0f, nest->crawlDir);
		return;
	}

	VectorMA(tr.endpos, 2.0f, tr.plane.normal, nest->pos);
	VectorCopy(tr.plane.normal, nest->normal);
}

// Small DEMP2 wall-impact spark for a struck nest.
static void FX_LightningNestVisual(vec3_t origin, vec3_t normal) {
	trap->FX_PlayEffectID(cgs.effects.demp2WallImpactEffectSmall, origin, normal, -1, -1, qfalse);
}

// Nest impact sound - one random variant, shares the global sound budget
// with the main beam impact sound. Called only when nest->nextSoundTime
// allows it (see FX_LightningStrikeNest), not on every strike.
static void FX_LightningNestImpactSound(vec3_t pos) {
	if (cg.time < lightningSoundBudgetTime ||
		cg.time - lightningSoundBudgetTime >= LIGHTNING_SOUND_INTERVAL) {
		lightningSoundBudgetTime = cg.time;
		lightningSounds = 0;
	}
	if (lightningSounds >= LIGHTNING_SOUND_BUDGET)
		return;
	lightningSounds++;
	trap->S_StartSound(pos, ENTITYNUM_WORLD, CHAN_AUTO,
		cgs.media.forceLightningEnvironmentSounds[rand() % ARRAY_LEN(cgs.media.forceLightningEnvironmentSounds)]);
}

// Main beam impact sound - single variant, chosen by time and entity.
static void FX_LightningImpactSound(centity_t *cent, const trace_t *hit) {
	vec3_t contact;
	int sound;

	if (cent->lightningImpactSoundTime > cg.time &&
		cent->lightningImpactSoundTime <= cg.time + LIGHTNING_SOUND_INTERVAL + 60)
		return;
	if (cg.time < lightningSoundBudgetTime ||
		cg.time - lightningSoundBudgetTime >= LIGHTNING_SOUND_INTERVAL) {
		lightningSoundBudgetTime = cg.time;
		lightningSounds = 0;
	}
	if (lightningSounds >= LIGHTNING_SOUND_BUDGET)
		return;

	sound = (cg.time / LIGHTNING_INTERVAL + cent->currentState.number) % ARRAY_LEN(cgs.media.forceLightningEnvironmentSounds);
	cent->lightningImpactSoundTime = cg.time + LIGHTNING_SOUND_INTERVAL + (sound % 3) * 30;
	if (!cgs.media.forceLightningEnvironmentSounds[sound])
		return;

	lightningSounds++;
	VectorMA(hit->endpos, 2.0f, hit->plane.normal, contact);
	trap->S_StartSound(contact, ENTITYNUM_WORLD, CHAN_AUTO, cgs.media.forceLightningEnvironmentSounds[sound]);
}

// Random point along the main beam's hand->tip segment to branch off from.
static void FX_LightningBranchBase(vec3_t beamStart, vec3_t beamEnd, vec3_t outBase) {
	vec3_t seg;
	VectorSubtract(beamEnd, beamStart, seg);
	VectorMA(beamStart, Q_flrand(LIGHTNING_NEST_BRANCH_FRAC_MIN, LIGHTNING_NEST_BRANCH_FRAC_MAX), seg, outBase);
}

// Fires one strike into a nest using effects/force/lightning_branch.efx
// (forceLightning without its origin flash particle). Its Electricity
// block uses spawnflags org2fromTrace, so the engine traces from origin
// along dir itself - we only supply a direction.
static void FX_LightningStrikeNest(vec3_t beamStart, vec3_t beamEnd, lightningNest_t *nest) {
	vec3_t branchBase, dir, tangent, side;

	FX_LightningBranchBase(beamStart, beamEnd, branchBase);

	VectorSubtract(nest->pos, branchBase, dir);
	VectorNormalize(dir);

	PerpendicularVector(tangent, dir);
	CrossProduct(dir, tangent, side);
	VectorMA(dir, Q_flrand(-0.03f, 0.03f), tangent, dir);
	VectorMA(dir, Q_flrand(-0.03f, 0.03f), side, dir);
	VectorNormalize(dir);

	trap->FX_PlayEffectID(cgs.effects.forceLightningBranch, branchBase, dir, -1, -1, qfalse);
	FX_LightningNestVisual(nest->pos, nest->normal);
	if (cg.time >= nest->nextSoundTime) {
		nest->nextSoundTime = cg.time + LIGHTNING_NEST_SOUND_INTERVAL;
		FX_LightningNestImpactSound(nest->pos);
	}
}

// Updates every active nest owned by owner: redraws its link arc on its
// own cadence, fires due strikes, and paces the next strike. Runs every
// call regardless of whether the main beam currently hits a surface.
static void FX_LightningUpdateNests(vec3_t beamStart, vec3_t beamEnd, int owner) {
	int slot;
	for (slot = 0; slot < LIGHTNING_MAX_NESTS; slot++) {
		lightningNest_t *nest = &lightningNests[slot];
		if (!nest->active || nest->owner != owner)
			continue;

		if (cg.time >= nest->nextCrawlTime) {
			nest->nextCrawlTime = cg.time + LIGHTNING_NEST_CRAWL_INTERVAL;
			FX_LightningCrawlNest(nest, owner);
		}

		if (cg.time >= nest->nextLinkTime) {
			vec3_t linkDir, linkTangent, linkSide, linkOrigin, branchBase;
			nest->nextLinkTime = cg.time + LIGHTNING_NEST_LINK_INTERVAL;

			FX_LightningBranchBase(beamStart, beamEnd, branchBase);

			VectorSubtract(nest->pos, branchBase, linkDir);
			VectorNormalize(linkDir);
			PerpendicularVector(linkTangent, linkDir);
			CrossProduct(linkDir, linkTangent, linkSide);
			VectorMA(branchBase, Q_flrand(-LIGHTNING_NEST_LINK_ORIGIN_JITTER, LIGHTNING_NEST_LINK_ORIGIN_JITTER), linkTangent, linkOrigin);
			VectorMA(linkOrigin, Q_flrand(-LIGHTNING_NEST_LINK_ORIGIN_JITTER, LIGHTNING_NEST_LINK_ORIGIN_JITTER), linkSide, linkOrigin);
			FX_LightningArc(linkOrigin, nest->pos, LIGHTNING_NEST_LINK_WIDTH,
				Q_flrand(LIGHTNING_NEST_LINK_CHAOS_MIN, LIGHTNING_NEST_LINK_CHAOS_MAX), qfalse);
		}

		if (nest->strikesLeft <= 0 || cg.time < nest->nextStrikeTime)
			continue;

		FX_LightningStrikeNest(beamStart, beamEnd, nest);
		nest->strikesLeft--;
		if (nest->strikesLeft <= 0) {
			nest->strikesLeft = LIGHTNING_NEST_MIN_STRIKES +
				(rand() % (LIGHTNING_NEST_MAX_STRIKES - LIGHTNING_NEST_MIN_STRIKES + 1));
		}
		nest->nextStrikeTime = cg.time +
			LIGHTNING_NEST_STRIKE_MIN_GAP +
			(rand() % (LIGHTNING_NEST_STRIKE_MAX_GAP - LIGHTNING_NEST_STRIKE_MIN_GAP + 1));
	}
}
// ---------------------------------------------------------------------------

// CG_DoSaberShockEffects in the supplied MBII cgamei386.so, 0x872a0.
// Keep its unnormalized spread, requested-endpoint cache, angle conversion,
// surface test and frame gate. The impact EFX owns the spark sound.
static void FX_LightningSaberShock(lightningSaberShock_t *shock, int index,
	const vec3_t origin, const vec3_t bladeDir) {
	vec3_t direction, end, angles;
	trace_t tr;
	int i;

	if (shock->endpointTime[index] < cg.time) {
		VectorCopy(bladeDir, direction);
		for (i = 0; i < 3; i++)
			direction[i] += 2.0f * ((rand() & 0x7fff) / 32767.0f - 0.5f) * 4.0f;
		VectorMA(origin, 350.0f, direction, end);
	} else {
		VectorSubtract(origin, shock->endpoint[index], angles);
		AngleVectors(angles, direction, NULL, NULL);
		angles[ROLL] = 0.0f;
		AngleVectors(angles, direction, NULL, NULL);
		VectorMA(origin, 200.0f, direction, end);
	}

	CG_Trace(&tr, origin, NULL, NULL, end, -1, MASK_SOLID);
	if (tr.fraction >= 1.0f)
		return;
	if (shock->endpointTime[index] < cg.time) {
		VectorCopy(end, shock->endpoint[index]);
		shock->endpointTime[index] = cg.time + Q_irand(250, 750);
	}
	if (cg.frametime > 0 && (cg.frametime >= 50 || cg.time % 50 <= cg.frametime)) {
		trap->FX_PlayEffectID(cgs.effects.forceLightningDeflectFlare,
			(vec_t *)origin, direction, -1, -1, qfalse);
		trap->FX_PlayEffectID(cgs.effects.forceLightningDeflectArc,
			(vec_t *)origin, direction, -1, -1, qfalse);
	}
}

// CG_AddSaberBlade calls the shock routine in this order: base (1), quarter
// (3), middle (4), 90% (0), three-quarter (2). Each blade has five caches.
void FX_ForceLightningSaberContact(centity_t *guard, int saberNum, int bladeNum,
	const vec3_t bladeBase, const vec3_t bladeEnd, const vec3_t bladeDir) {
	lightningSaberShock_t *shock;
	vec3_t origin, delta;

	if (saberNum < 0 || saberNum >= MAX_SABERS || bladeNum < 0 || bladeNum >= MAX_BLADES ||
		guard->currentState.weapon != WP_SABER || guard->currentState.saberHolstered == 2 ||
		guard->currentState.saberInFlight)
		return;
	shock = &guard->lightningSaberShock[saberNum][bladeNum];
	VectorSubtract(bladeEnd, bladeBase, delta);
	FX_LightningSaberShock(shock, 1, bladeBase, bladeDir);
	VectorMA(bladeBase, 0.25f, delta, origin);
	FX_LightningSaberShock(shock, 3, origin, bladeDir);
	VectorMA(bladeBase, 0.5f, delta, origin);
	FX_LightningSaberShock(shock, 4, origin, bladeDir);
	// MBII evaluates this interpolation with a double-precision 0.1 constant.
	origin[0] = (float)(bladeEnd[0] + (bladeBase[0] - bladeEnd[0]) * 0.1);
	origin[1] = (float)(bladeEnd[1] + (bladeBase[1] - bladeEnd[1]) * 0.1);
	origin[2] = (float)(bladeEnd[2] + (bladeBase[2] - bladeEnd[2]) * 0.1);
	FX_LightningSaberShock(shock, 0, origin, bladeDir);
	VectorScale(delta, -1.0f, delta);
	VectorMA(bladeEnd, 0.25f, delta, origin);
	FX_LightningSaberShock(shock, 2, origin, bladeDir);
}

// Traces the main beam for hit detection, draws it via the native
// forceLightning/forceLightningWide effect, and drives the nest system.
// If the shared trace/effect budget is exhausted, the caller falls back
// to vanilla lightning entirely.
qboolean FX_ForceLightningEnvironment(centity_t *cent, vec3_t origin, matrix3_t axis, qboolean wide) {
	int ray, rays, surfaceRay = -1;
	float phase;
	vec3_t end, beamEnd;
	trace_t hits[4];
	vec3_t directions[4];
	qboolean valid[4];

	if (!cg_lightningEnvironment.integer)
		return qfalse;
	if (cg_lightningEnvironment.integer == 2) {
		FX_ForceLightningReference(cent, origin, axis, wide);
		return qtrue;
	}
	if (cent->lightningEnvironmentTime > cg.time &&
		cent->lightningEnvironmentTime <= cg.time + LIGHTNING_EMIT_INTERVAL)
		return qtrue;
	if (cg.time < lightningBudgetTime || cg.time - lightningBudgetTime >= LIGHTNING_INTERVAL) {
		lightningBudgetTime = cg.time;
		lightningTraces = lightningEffects = 0;
	}
	rays = wide ? 2 + ((cg.time / 10 + cent->currentState.number) % 3) : 1;
	if (lightningTraces + rays > LIGHTNING_TRACE_BUDGET ||
		lightningEffects + rays + 1 > LIGHTNING_EFFECT_BUDGET)
		return qfalse;
	cent->lightningEnvironmentTime = cg.time + LIGHTNING_EMIT_INTERVAL;
	phase = cg.time * 0.017f + cent->currentState.number * 2.39996f;
	for (ray = 0; ray < rays; ray++) {
		VectorCopy(axis[0], directions[ray]);
		if (wide) {
			float spread = (ray / (float)(rays - 1) - 0.5f) * 1.4f;
			VectorMA(directions[ray], spread + sinf(phase + ray * 2.4f) * 0.16f, axis[1], directions[ray]);
			VectorMA(directions[ray], sinf(phase * 0.7f + ray) * 0.08f, axis[2], directions[ray]);
		}
		VectorNormalize(directions[ray]);
		VectorMA(origin, wide ? 512.0f : 2048.0f, directions[ray], end);
		valid[ray] = FX_LightningTrace(&hits[ray], origin, end, cent->currentState.number);
	}
	for (ray = 0; ray < rays; ray++) {
		if (!valid[ray])
			continue;
		VectorCopy(hits[ray].endpos, end);
		if (FX_LightningSurface(&hits[ray])) {
			VectorMA(end, 2.0f, hits[ray].plane.normal, end);
			if (surfaceRay < 0 || hits[ray].fraction < hits[surfaceRay].fraction)
				surfaceRay = ray;
		}
	}

	// Main beam visual: fired once (not per ray - forceLightningWide already
	// fans out internally), via FX_PlayEntityEffectID with the full axis
	// matrix so the effect's own up/side orientation stays fixed to the
	// player instead of spinning with the camera.
	trap->FX_PlayEntityEffectID(wide ? cgs.effects.forceLightningWide : cgs.effects.forceLightning,
		origin, axis, -1, -1, -1, -1);
	FX_LightningFlash(origin, 18.0f);

	if (surfaceRay >= 0) {
		VectorMA(hits[surfaceRay].endpos, 2.0f, hits[surfaceRay].plane.normal, beamEnd);
		FX_LightningImpactSound(cent, &hits[surfaceRay]);
	} else if (valid[0])
		VectorCopy(hits[0].endpos, beamEnd);
	else
		VectorCopy(origin, beamEnd);

	{
		int owner = cent->currentState.number;
		FX_LightningExpireNests(owner);
		FX_LightningUpdateNests(origin, beamEnd, owner);

		if (cent->lightningSurfaceTime <= cg.time) {
			cent->lightningSurfaceTime = cg.time + LIGHTNING_INTERVAL;
			FX_LightningSpawnNest(origin, axis[0], owner);
		}
	}

	return qtrue;
}

/*
-------------------------
FX_ForceDrained
-------------------------
*/
// This effect is not generic because of possible enhancements
void FX_ForceDrained(vec3_t origin, vec3_t dir)
{
	VectorScale(dir, -1.0, dir);
	trap->FX_PlayEffectID(cgs.effects.forceDrained, origin, dir, -1, -1, qfalse);
}
