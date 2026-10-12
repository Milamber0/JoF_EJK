#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef float vec3_t[3];
typedef vec3_t matrix3_t[3];
typedef int qboolean;
typedef int fxHandle_t;
enum { qfalse = 0, ROLL = 2, CHAN_AUTO = 0, MASK_SOLID = 0x1001,
	SURF_SKY = 1, SURF_NOIMPACT = 2, SURF_NODRAW = 4 };
typedef struct {
	struct { int number; } currentState;
	int lightningReferenceTime[5];
	vec3_t lightningReferenceEnd[5];
	int lightningReferenceSoundTime[5];
} centity_t;
typedef struct {
	float fraction;
	int startsolid, allsolid, surfaceFlags;
	vec3_t endpos;
	struct { vec3_t normal; } plane;
} trace_t;
static struct { int time, frametime; } cg = { 1000, 16 };
static struct { float value; } cg_lightningEnvironmentAngle;
static struct {
	struct { int forceLightningReference, forceLightningReferenceWide, forceLightningReferenceArc;
		int forceLightning, forceLightningWide, forceLightningBranch, forceLightningEnvironmentImpact; } effects;
	struct { int forceLightningEnvironmentArcSounds[3]; } media;
} cgs = { { 40, 41, 42 }, { { 11, 12, 13 } } };

static int cases, traces, arcs, sounds, mains, randoms, hits, forceZeroRandom;
static uint32_t randomState = 17;
static vec3_t aim, endpoints[5], directions[5], testOrigin = { 10, 20, 30 };
static int expectedMain;
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "Case %d, line %d: %s\n", cases, __LINE__, #x); exit(1); } } while (0)
#define DEG2RAD(a) ((a) * (3.14159265358979323846f / 180.0f))
#define DotProduct(a,b) ((a)[0]*(b)[0] + (a)[1]*(b)[1] + (a)[2]*(b)[2])
#define VectorCopy(a,b) memcpy(b, a, sizeof(vec3_t))
#define ARRAY_LEN(a) ((int)(sizeof(a) / sizeof((a)[0])))
static float Com_Clamp(float min, float max, float v) { return v < min ? min : (v > max ? max : v); }
static float VectorLength(const vec3_t v) { return sqrtf(DotProduct(v, v)); }
static void VectorScale(const vec3_t v, float s, vec3_t out) {
	int i; for (i = 0; i < 3; i++) out[i] = s * v[i];
}
static void VectorMA(const vec3_t a, float s, const vec3_t b, vec3_t out) {
	int i; for (i = 0; i < 3; i++) out[i] = a[i] + s * b[i];
}
static void VectorSubtract(const vec3_t a, const vec3_t b, vec3_t out) {
	int i; for (i = 0; i < 3; i++) out[i] = a[i] - b[i];
}
static void AngleVectors(const vec3_t angles, vec3_t forward, vec3_t right, vec3_t up) {
	float yaw = DEG2RAD(angles[1]), pitch = DEG2RAD(angles[0]);
	CHECK(!right && !up);
	forward[0] = cosf(pitch) * cosf(yaw);
	forward[1] = cosf(pitch) * sinf(yaw);
	forward[2] = -sinf(pitch);
}
static void Equal(const vec3_t a, const vec3_t b) {
	int i; for (i = 0; i < 3; i++) CHECK(fabsf(a[i] - b[i]) < 0.001f);
}
static void InsideArc(const vec3_t direction) {
	double length = 0, forwardLength = 0, dot = 0, cosine, degrees, maximum;
	int i;
	for (i = 0; i < 3; i++) {
		length += (double)direction[i] * direction[i];
		forwardLength += (double)aim[i] * aim[i];
		dot += (double)direction[i] * aim[i];
	}
	CHECK(length > 0.00000001);
	cosine = dot / sqrt(length * forwardLength);
	if (cosine < -1) cosine = -1;
	if (cosine > 1) cosine = 1;
	degrees = acos(cosine) * 180.0 / 3.14159265358979323846;
	maximum = cg_lightningEnvironmentAngle.value;
	if (maximum < 0) maximum = 0;
	if (maximum > 360) maximum = 360;
	CHECK(degrees <= maximum / 2.0 + 0.02);
}
static int TestRandom(void) {
	randoms++;
	randomState = randomState * 1664525u + 1013904223u;
	return forceZeroRandom ? 0 : (int)((randomState >> 8) & 0x7fff);
}
#define rand TestRandom
static int Q_irand(int min, int max) { return min + (max - min) / 2; }
static void CG_Trace(trace_t *tr, vec3_t start, void *mins, void *maxs, vec3_t end, int owner, int mask) {
	vec3_t delta;
	CHECK(!mins && !maxs && owner == -1 && mask == MASK_SOLID && traces < 5);
	Equal(start, testOrigin);
	VectorSubtract(end, start, delta);
	InsideArc(delta);
	VectorCopy(end, endpoints[traces++]);
	memset(tr, 0, sizeof(*tr));
	tr->fraction = hits ? 0.25f : 1.0f;
}
static void PlayMain(int effect, vec3_t origin, matrix3_t axis, int a, int b, int c, int d) {
	CHECK(effect == expectedMain && a == -1 && b == -1 && c == -1 && d == -1);
	Equal(origin, testOrigin); Equal(axis[0], aim); mains++;
}
static void PlayArc(int effect, vec3_t origin, vec3_t direction, int a, int b, qboolean portal) {
	vec3_t delta;
	CHECK(effect == 42 && a == -1 && b == -1 && !portal && arcs < traces);
	Equal(origin, testOrigin); InsideArc(direction);
	VectorCopy(direction, directions[arcs]);
	VectorSubtract(endpoints[arcs], origin, delta);
	CHECK(fabsf(DotProduct(delta, direction) / (VectorLength(delta) * VectorLength(direction)) - 1) < 0.0001f);
	arcs++;
}
static void Sound(vec3_t origin, int entity, int channel, int sound) {
	CHECK(entity == 7 && channel == CHAN_AUTO && sound == 12 && sounds < traces);
	Equal(origin, endpoints[sounds++]);
}
static struct {
	void (*FX_PlayEntityEffectID)(int, vec3_t, matrix3_t, int, int, int, int);
	void (*FX_PlayEffectID)(int, vec3_t, vec3_t, int, int, qboolean);
	void (*S_StartSound)(vec3_t, int, int, int);
} api = { PlayMain, PlayArc, Sound }, *trap = &api;

#include "actual.h"

static void Run(centity_t *cent, matrix3_t axis, int wide, int cached) {
	int i, count = wide ? 5 : 2;
	traces = arcs = sounds = mains = randoms = 0;
	expectedMain = wide ? 41 : 40;
	VectorCopy(axis[0], aim);
	cases++;
	FX_ForceLightningReference(cent, testOrigin, axis, wide);
	CHECK(mains == 1 && traces == count && randoms == (cached ? 0 : 3 * count));
	CHECK(arcs == (hits ? count : 0) && sounds == arcs);
	for (i = 0; i < count; i++) {
		if (!cached) {
			CHECK(cent->lightningReferenceTime[i] == (hits ? 2000 : 0));
			if (hits) Equal(cent->lightningReferenceEnd[i], endpoints[i]);
		} else CHECK(cent->lightningReferenceTime[i] == 2000);
		CHECK(cent->lightningReferenceSoundTime[i] == (hits ? 1625 : 0));
	}
}

int main(void) {
	static const float widths[] = { -10, 0, 60, 120, 160, 180, 360, 999 };
	static const float edgeYaws[] = { 0, 79, 80, 81, 90, 179, 180, -79, -80, -81, -90, -179 };
	centity_t cent;
	matrix3_t axis = { { 1, 0, 0 }, { 0, 1, 0 }, { 0, 0, 1 } };
	vec3_t angles, delta;
	int w, yaw, pitch, cached, wide, i;
	// Cached directions on either side of the boundary, including directly behind.
	hits = 1; cg_lightningEnvironmentAngle.value = 160;
	for (i = 0; i < (int)(sizeof(edgeYaws) / sizeof(edgeYaws[0])); i++) {
		int j;
		memset(&cent, 0, sizeof(cent)); cent.currentState.number = 7;
		for (j = 0; j < 5; j++) {
			cent.lightningReferenceTime[j] = 2000;
			VectorCopy(testOrigin, cent.lightningReferenceEnd[j]);
			cent.lightningReferenceEnd[j][1] -= edgeYaws[i];
		}
		Run(&cent, axis, 1, 1);
		if (fabsf(edgeYaws[i]) > 80) Equal(directions[0], axis[0]);
		else if (fabsf(edgeYaws[i]) < 80) {
			angles[0] = angles[2] = 0; angles[1] = edgeYaws[i];
			AngleVectors(angles, delta, NULL, NULL); Equal(directions[0], delta);
		}
	}
	// A fresh randomized world-space direction can also reverse a diagonal aim.
	memset(&cent, 0, sizeof(cent)); cent.currentState.number = 7;
	axis[0][0] = axis[0][1] = axis[0][2] = 1.0f / sqrtf(3.0f);
	forceZeroRandom = 1;
	Run(&cent, axis, 1, 0);
	VectorSubtract(endpoints[0], testOrigin, delta);
	CHECK(fabsf(VectorLength(delta) - 350.0f * (0.8f * sqrtf(3.0f) - 1.0f)) < 0.001f);
	forceZeroRandom = 0;
	// Rotate and pitch the player independently from cached world-space endpoints.
	for (w = 0; w < (int)(sizeof(widths) / sizeof(widths[0])); w++)
	for (yaw = -180; yaw < 180; yaw += 15)
	for (pitch = -80; pitch <= 80; pitch += 40)
	for (wide = 0; wide <= 1; wide++)
	for (cached = 0; cached <= 1; cached++) {
		cg_lightningEnvironmentAngle.value = widths[w];
		angles[0] = (float)pitch; angles[1] = (float)yaw; angles[2] = 0;
		AngleVectors(angles, axis[0], NULL, NULL);
		memset(&cent, 0, sizeof(cent)); cent.currentState.number = 7;
		hits = (yaw / 15 + pitch / 40) & 1;
		if (cached) for (i = 0; i < 5; i++) {
			cent.lightningReferenceTime[i] = 2000;
			cent.lightningReferenceEnd[i][0] = 100.0f * i - 137;
			cent.lightningReferenceEnd[i][1] = -54.0f * i + 44;
			cent.lightningReferenceEnd[i][2] = 51.0f * i - 2;
		}
		Run(&cent, axis, wide, cached);
	}
	printf("%d forward-arc cases passed: angle limits, fresh/cached directions, player turns, traces, FX, sound and timers.\n", cases);
	return 0;
}
