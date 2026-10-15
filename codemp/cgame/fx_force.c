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
				cgs.media.forceLightningEnvironmentArcSounds[Q_irand(0, ARRAY_LEN(cgs.media.forceLightningEnvironmentArcSounds) - 1)]);
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

// Small DEMP2 wall-impact spark where the contained beam touches a surface.
static void FX_LightningContactVisual(vec3_t origin, vec3_t normal) {
	trap->FX_PlayEffectID(cgs.effects.demp2WallImpactEffectSmall, origin, normal, -1, -1, qfalse);
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

	sound = (cg.time / LIGHTNING_INTERVAL + cent->currentState.number) % ARRAY_LEN(cgs.media.forceLightningEnvironmentArcSounds);
	cent->lightningImpactSoundTime = cg.time + LIGHTNING_SOUND_INTERVAL + (sound % 3) * 30;
	if (!cgs.media.forceLightningEnvironmentArcSounds[sound])
		return;

	lightningSounds++;
	VectorMA(hit->endpos, 2.0f, hit->plane.normal, contact);
	trap->S_StartSound(contact, ENTITYNUM_WORLD, CHAN_AUTO, cgs.media.forceLightningEnvironmentArcSounds[sound]);
}

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

// cg_lightningEnvironment 1 (contained): traces the main beam for hit
// detection, draws it via the native forceLightning/forceLightningWide
// effect and sparks where it touches a surface. Mode 2 (erratic) hands off
// to FX_ForceLightningReference.
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

	// Contained lightning (mode 1) responds only at the main beam's contact point.
	if (surfaceRay >= 0 && cent->lightningSurfaceTime <= cg.time) {
		cent->lightningSurfaceTime = cg.time + LIGHTNING_INTERVAL;
		FX_LightningContactVisual(beamEnd, hits[surfaceRay].plane.normal);
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
