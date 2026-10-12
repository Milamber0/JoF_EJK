#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cgame/cg_local.h"

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)

cg_t cg;
cgs_t cgs;
vmCvar_t cg_drainFX, cp_pluginDisable;
static cgameImport_t imports;
cgameImport_t *trap = &imports;
static snapshot_t snapshot;
static int effectCount, lastEffect, leftHits, rightHits, grips, pushes, rightFetches, leftFetches;
static vec3_t leftOrigin = { 1, 2, 3 }, rightOrigin = { 4, 5, 6 };
static matrix3_t expectAxis;
static int boltedCalls, unboltedCalls;
static qboolean boltedOk = qtrue;
enum { DRAIN = 11, DRAIN_WIDE, DRAIN_WIDE_JAPRO, DRAIN_HAND };

static void TestPrint(const char *fmt, ...) { (void)fmt; }
static void NORETURN TestError(int code, const char *fmt, ...) { (void)code; (void)fmt; abort(); }
void (*Com_Printf)(const char *, ...) = TestPrint;
NORETURN_PTR void (*Com_Error)(int, const char *, ...) = TestError;

static void SetMatrix(mdxaBone_t *matrix, const vec3_t origin)
{
	memset(matrix, 0, sizeof(*matrix));
	matrix->matrix[0][3] = origin[0];
	matrix->matrix[1][3] = origin[1];
	matrix->matrix[2][3] = origin[2];
}
static qboolean GetBoltMatrix(void *g2, const int model, const int bolt, mdxaBone_t *matrix,
	const vec3_t angles, const vec3_t position, const int time, qhandle_t *models, vec3_t scale)
{
	(void)g2; (void)angles; (void)position; (void)models; (void)scale;
	CHECK(model == 0 && time == cg.time);
	CHECK(bolt == 1 || bolt == 0);
	if (bolt == 0) ++rightFetches; else ++leftFetches;
	SetMatrix(matrix, bolt == 0 ? rightOrigin : leftOrigin);
	return qtrue;
}
static void PlayEntityEffect(int id, vec3_t org, matrix3_t axis, const int boltInfo, const int entNum, int vol, int rad)
{
	CHECK(boltInfo == -1 && entNum == -1 && vol == -1 && rad == -1);
	CHECK(!memcmp(axis, expectAxis, sizeof(matrix3_t)));
	if (VectorCompare(org, leftOrigin)) ++leftHits;
	else { CHECK(VectorCompare(org, rightOrigin)); ++rightHits; }
	lastEffect = id;
	++effectCount;
	++unboltedCalls;
}
static qboolean PlayBoltedEffect(int id, vec3_t org, void *g2, const int bolt, const int entNum, const int modelNum, int loop, qboolean relative)
{
	CHECK(g2 != NULL && modelNum == 0 && loop == 0 && !relative);
	CHECK(bolt == 1 || bolt == 0);
	if (!boltedOk) return qfalse;
	CHECK(VectorCompare(org, bolt == 0 ? rightOrigin : leftOrigin));
	CHECK(entNum == 0 || entNum == 1);
	if (bolt == 0) ++rightHits; else ++leftHits;
	lastEffect = id;
	++effectCount;
	++boltedCalls;
	return qtrue;
}
void BG_GiveMeVectorFromMatrix(mdxaBone_t *matrix, int flags, vec3_t out)
{
	(void)matrix; (void)flags;
	VectorClear(out);
}
static void CG_ForceGripEffect(vec3_t org) { CHECK(VectorCompare(org, leftOrigin)); ++grips; }
static void CG_ForcePushBlur(vec3_t org, centity_t *cent) { (void)cent; CHECK(VectorCompare(org, leftOrigin)); ++pushes; }

#include "hand.h"

static void Draw(centity_t *cent, clientInfo_t *ci, qboolean visible)
{
	mdxaBone_t left, right;
	vec3_t fAng;
	SetMatrix(&left, leftOrigin);
	SetMatrix(&right, rightOrigin);
	VectorSet(fAng, cent->pe.torso.pitchAngle, cent->pe.torso.yawAngle, 0);
	AnglesToAxis(fAng, expectAxis);
	effectCount = grips = pushes = rightFetches = leftFetches = 0;
	DrawHandBlock(cent, ci, visible, qtrue, qfalse, left, right);
	CHECK(leftFetches == 0); // preserve the existing left-hand matrix cache
}
static void CheckDrainChoice(centity_t *cent, clientInfo_t *ci)
{
	// Always force/drain_hand.efx, whatever the Drain settings.
	int plugin, fx;
	for (plugin = 0; plugin < 2; ++plugin)
		for (fx = 0; fx < 2; ++fx)
		{
			cp_pluginDisable.integer = 1056 | (plugin ? JAPRO_PLUGIN_NEWDRAINEFX : 0);
			cg_drainFX.integer = fx;
			Draw(cent, ci, qtrue);
			CHECK(effectCount == 1 && lastEffect == DRAIN_HAND);
		}
	cp_pluginDisable.integer = 1056;
	cg_drainFX.integer = 1;
	// Not loaded: nothing is played (never handle 0).
	cgs.effects.destructionDrainHand = 0;
	Draw(cent, ci, qtrue);
	CHECK(effectCount == 0);
	cgs.effects.destructionDrainHand = DRAIN_HAND;
}
static void CheckHand(void)
{
	centity_t cent = {0};
	clientInfo_t ci = {0};
	int i;
	cgs.effects.forceDrain = DRAIN;
	cgs.effects.forceDrainWide = DRAIN_WIDE;
	cgs.effects.forceDrainWideJaPRO = DRAIN_WIDE_JAPRO;
	cgs.effects.destructionDrainHand = DRAIN_HAND;
	cg.renderingThirdPerson = qfalse;
	cg.time = 1000;
	cent.ghoul2 = &cent;
	ci.bolt_lhand = 1;
	ci.bolt_rhand = 0;
	cent.pe.torso.pitchAngle = 20;
	cent.pe.torso.yawAngle = 135;
	cent.currentState.number = 1;
	cent.currentState.powerups = 1 << PW_DISINT_4;
	cent.currentState.forcePowersActive = DESTRUCTION_HAND_FLAG | (1 << FP_GRIP);

	// Normal Destruction: the left hand plays the Drain effect, never Grip or Push.
	cent.currentState.weapon = WP_SABER;
	CheckDrainChoice(&cent, &ci);
	leftHits = rightHits = 0;
	for (i = 0; i < 200; ++i)
	{
		Draw(&cent, &ci, qtrue);
		CHECK(effectCount == 1 && grips == 0 && pushes == 0 && rightFetches == 0);
	}
	CHECK(leftHits == 200 && rightHits == 0);

	// Super (melee): both hands every frame.
	cent.currentState.weapon = WP_MELEE;
	leftHits = rightHits = 0;
	for (i = 0; i < 200; ++i)
	{
		Draw(&cent, &ci, qtrue);
		CHECK(effectCount == 2 && grips == 0 && pushes == 0 && rightFetches == 1);
	}
	CHECK(leftHits == 200 && rightHits == 200);
	// Super detected from the two-handed pose too, even if the weapon field isn't melee.
	cent.currentState.weapon = WP_SABER;
	cent.currentState.torsoAnim = BOTH_FORCE_2HANDEDLIGHTNING;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 2 && rightFetches == 1);
	// No right-hand bolt: left hand only.
	ci.bolt_rhand = -1;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 1 && rightFetches == 0);
	ci.bolt_rhand = 0;
	cent.currentState.torsoAnim = 0;
	cent.currentState.weapon = WP_MELEE;

	// Bolts resolve delayed spawn positions; particles stay in world space to preserve trails.
	boltedCalls = unboltedCalls = 0;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 2 && boltedCalls == 2 && unboltedCalls == 0);
	boltedOk = qfalse;
	boltedCalls = unboltedCalls = 0;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 2 && boltedCalls == 0 && unboltedCalls == 2);
	boltedOk = qtrue;

	// Hidden (mind trick) or own first person: nothing, and no Grip/Push fallthrough.
	Draw(&cent, &ci, qfalse);
	CHECK(effectCount == 0 && grips == 0 && pushes == 0);
	cent.currentState.number = snapshot.ps.clientNum;
	cent.currentState.forcePowersActive = DESTRUCTION_HAND_FLAG;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 0 && grips == 0 && pushes == 0);
	cg.renderingThirdPerson = qtrue;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 2);

	// Marker cleared: stops at once; Grip and Push are unchanged.
	cent.currentState.powerups = 0;
	cent.currentState.forcePowersActive = 0;
	cent.bodyFadeTime = 1;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 0 && grips == 0 && pushes == 0 && cent.bodyFadeTime == 0);
	cent.currentState.powerups = 1 << PW_DISINT_4;
	cent.currentState.forcePowersActive = 1 << FP_GRIP;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 0 && grips == 2 && pushes == 0);
	cent.currentState.forcePowersActive = 0;
	Draw(&cent, &ci, qtrue);
	CHECK(effectCount == 0 && grips == 0 && pushes == 1);
}
int main(void)
{
	imports.G2API_GetBoltMatrix = GetBoltMatrix;
	imports.FX_PlayEntityEffectID = PlayEntityEffect;
	imports.FX_PlayBoltedEffectID = PlayBoltedEffect;
	cg.snap = &snapshot;
	srand(1);
	CheckHand();
	puts("Destruction charge plays force/drain_hand.efx bolted to the hands, Super plays both hands every frame, visibility, marker priority and stock Grip/Push checks passed.");
	return 0;
}
