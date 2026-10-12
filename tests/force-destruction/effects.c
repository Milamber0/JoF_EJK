#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "cgame/cg_local.h"

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)

enum { PROJECTILE_PRESENT = 1, IMPACT_PRESENT = 2, ENHANCED_PRESENT = 4,
	ICON_PRESENT = 8, CAST_PRESENT = 16, SOUND1_PRESENT = 32, SOUND2_PRESENT = 64, SUPER_PRESENT = 128 };
enum { STOCK_PROJECTILE = 1, STOCK_IMPACT, CUSTOM_PROJECTILE, CUSTOM_IMPACT, ENHANCED_IMPACT,
	STOCK_ICON, CUSTOM_ICON, STOCK_CAST, CUSTOM_CAST, STOCK_BOOM, CUSTOM_BOOM1, CUSTOM_BOOM2, CUSTOM_SUPER };
static int available, plays, playedEffect, sounds, playedSound;
static vec3_t playedOrigin, playedDirection;
cgs_t cgs;

static void TestPrint(const char *fmt, ...) { (void)fmt; }
static void NORETURN TestError(int code, const char *fmt, ...) { (void)code; (void)fmt; abort(); }
void (*Com_Printf)(const char *, ...) = TestPrint;
NORETURN_PTR void (*Com_Error)(int, const char *, ...) = TestError;

static qhandle_t RegisterShader(const char *name)
{
	CHECK(!strcmp(name, "gfx/forcedestruction/force_destruction.tga"));
	return (available & ICON_PRESENT) ? CUSTOM_ICON : 0;
}

static fxHandle_t RegisterEffect(const char *name)
{
	if (!strcmp(name, "concussion/shot")) return STOCK_PROJECTILE;
	if (!strcmp(name, "concussion/explosion")) return STOCK_IMPACT;
	if (!strcmp(name, "forcedestruction/destruction"))
		return (available & PROJECTILE_PRESENT) ? CUSTOM_PROJECTILE : 0;
	if (!strcmp(name, "force/drain_hand"))
		return 99;
	if (!strcmp(name, "forcedestruction/destruction_super"))
		return (available & SUPER_PRESENT) ? CUSTOM_SUPER : 0;
	if (!strcmp(name, "forcedestruction/destruction_explode_enhanced2"))
		return (available & ENHANCED_PRESENT) ? ENHANCED_IMPACT : 0;
	CHECK(!strcmp(name, "forcedestruction/destruction_explode"));
	return (available & IMPACT_PRESENT) ? CUSTOM_IMPACT : 0;
}

static sfxHandle_t RegisterSound(const char *name)
{
	if (!strcmp(name, "sound/weapons/force/push.wav")) return STOCK_CAST;
	if (!strcmp(name, "sound/vehicles/weapons/mine/impact.wav")) return STOCK_BOOM;
	if (!strcmp(name, "sound/forcedestruction/destruction.mp3"))
		return (available & CAST_PRESENT) ? CUSTOM_CAST : 0;
	if (!strcmp(name, "sound/forcedestruction/forcedestruct01.wav"))
		return (available & SOUND1_PRESENT) ? CUSTOM_BOOM1 : 0;
	CHECK(!strcmp(name, "sound/forcedestruction/forcedestruct02.wav"));
	return (available & SOUND2_PRESENT) ? CUSTOM_BOOM2 : 0;
}

static void PlaySound(const vec3_t origin, int entity, int channel, sfxHandle_t sound)
{
	CHECK(entity == 0 || entity == 1);
	CHECK(channel == CHAN_AUTO && VectorCompare(origin, playedOrigin));
	CHECK(sound >= STOCK_BOOM && sound <= CUSTOM_BOOM2);
	++sounds;
	playedSound = sound;
}

static void PlayEffect(int effect, vec3_t origin, vec3_t direction, int vol, int rad, qboolean portal)
{
	CHECK((effect >= STOCK_PROJECTILE && effect <= ENHANCED_IMPACT) || effect == CUSTOM_SUPER);
	CHECK(vol == -1 && rad == -1 && !portal);
	++plays;
	playedEffect = effect;
	VectorCopy(origin, playedOrigin);
	VectorCopy(direction, playedDirection);
}

static cgameImport_t imports;
cgameImport_t *trap = &imports;

#include "effects.h"

static void CheckEffects(int assets)
{
	entityState_t missile = {0};
	vec3_t origin = {3, 4, 5}, velocity = {0, 3, 4}, normal = {0, 0, 1}, stopped = {0};
	int projectile = (assets & PROJECTILE_PRESENT) ? CUSTOM_PROJECTILE : STOCK_PROJECTILE;
	int layers = (assets & PROJECTILE_PRESENT) ? 3 : 1;
	int impact = (assets & ENHANCED_PRESENT) ? ENHANCED_IMPACT :
		((assets & IMPACT_PRESENT) ? CUSTOM_IMPACT : STOCK_IMPACT);
	int sound1 = (assets & SOUND1_PRESENT) ? CUSTOM_BOOM1 :
		((assets & SOUND2_PRESENT) ? CUSTOM_BOOM2 : STOCK_BOOM);
	int sound2 = (assets & SOUND2_PRESENT) ? CUSTOM_BOOM2 : sound1;

	available = assets;
	plays = sounds = 0;
	cgs.media.forcePowerIcons[FP_LIGHTNING] = STOCK_ICON;
	CG_RegisterDestructionEffects();
	CHECK(cgs.effects.destructionProjectile == projectile);
	CHECK(cgs.effects.destructionImpact == impact);
	CHECK(cgs.effects.destructionDrainHand == 99);
	CHECK(cgs.effects.destructionSuper == ((assets & SUPER_PRESENT) ? CUSTOM_SUPER : 0));
	CHECK(cgs.effects.destructionCustomProjectile == (projectile != STOCK_PROJECTILE));
	CHECK(cgs.effects.destructionCustomImpact == (impact != STOCK_IMPACT));
	CHECK(cgs.media.destructionIcon == ((assets & ICON_PRESENT) ? CUSTOM_ICON : STOCK_ICON));
	CHECK(cgs.media.destructionImpactSounds[0] == sound1 && cgs.media.destructionImpactSounds[1] == sound2);

	missile.weapon = WP_CONCUSSION;
	missile.generic1 = DESTRUCTION_MISSILE_TAG;
	CHECK(CG_DestructionCastSound(&missile, 77) == ((assets & CAST_PRESENT) ? CUSTOM_CAST : STOCK_CAST));
	// Rendering is local, even with old servers advertising unavailable FX.
	missile.otherEntityNum2 = missile.emplacedOwner = MAX_FX;
	CHECK(CG_PlayDestructionEffect(&missile, origin, velocity, qfalse));
	CHECK(plays == layers && playedEffect == projectile && sounds == 0);
	CHECK(VectorCompare(origin, playedOrigin));
	CHECK(playedDirection[0] == 0 && fabs(playedDirection[1] - 0.6f) < 0.0001f &&
		fabs(playedDirection[2] - 0.8f) < 0.0001f);
	CHECK(velocity[1] == 3 && velocity[2] == 4);
	CHECK(CG_PlayDestructionEffect(&missile, origin, normal, qtrue));
	CHECK(plays == layers + 1 && playedEffect == impact && VectorCompare(normal, playedDirection));
	CHECK(sounds == (impact != STOCK_IMPACT));
	if (sounds) CHECK(playedSound == sound1);
	missile.number = 1;
	CHECK(CG_PlayDestructionEffect(&missile, origin, normal, qtrue));
	CHECK(plays == layers + 2 && playedEffect == impact);
	CHECK(sounds == 2 * (impact != STOCK_IMPACT));
	if (sounds) CHECK(playedSound == sound2);
	CHECK(CG_PlayDestructionEffect(&missile, origin, stopped, qfalse));
	CHECK(plays == 2 * layers + 2 && playedEffect == projectile && VectorCompare(normal, playedDirection));

	// Super Destruction (melee): the server marks the orb with iModelScale 115.
	missile.iModelScale = 115;
	CHECK(CG_PlayDestructionEffect(&missile, origin, velocity, qfalse));
	CHECK(plays == 3 * layers + 2 && playedEffect ==
		(((assets & (PROJECTILE_PRESENT | SUPER_PRESENT)) == (PROJECTILE_PRESENT | SUPER_PRESENT)) ? CUSTOM_SUPER : projectile));
	CHECK(CG_PlayDestructionEffect(&missile, origin, normal, qtrue));
	CHECK(plays == 3 * layers + 3 && playedEffect == impact);
	sounds -= (impact != STOCK_IMPACT);
	missile.iModelScale = 0;
	plays -= layers + 1;
	missile.generic1 = 0;
	CHECK(!CG_PlayDestructionEffect(&missile, origin, velocity, qfalse));
	CHECK(!CG_PlayDestructionEffect(&missile, origin, normal, qtrue));
	CHECK(CG_DestructionCastSound(&missile, 77) == 77);
	missile.generic1 = DESTRUCTION_MISSILE_TAG;
	missile.weapon = WP_BLASTER;
	CHECK(!CG_PlayDestructionEffect(&missile, origin, velocity, qfalse));
	CHECK(!CG_PlayDestructionEffect(&missile, origin, normal, qtrue));
	CHECK(CG_DestructionCastSound(&missile, 77) == 77);
	CHECK(plays == 2 * layers + 2 && sounds == 2 * (impact != STOCK_IMPACT));
}

int main(void)
{
	int assets;
	imports.FX_RegisterEffect = RegisterEffect;
	imports.R_RegisterShaderNoMip = RegisterShader;
	imports.FX_PlayEffectID = PlayEffect;
	imports.S_RegisterSound = RegisterSound;
	imports.S_StartSound = PlaySound;
	for (assets = 0; assets < 256; ++assets) CheckEffects(assets);
	CheckEffects(0); // reinitialization resets the previous complete asset set
	puts("Destruction projectile layering, single impacts, optional Super orb FX, FX/icon/audio fallback, 256 partial-pack combinations, reload and normal-weapon checks passed.");
	return 0;
}
