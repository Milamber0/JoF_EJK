#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <ctype.h>
#include <stdarg.h>
#ifdef _WIN32
#define Q_stricmp _stricmp
#else
#include <strings.h>
#define Q_stricmp strcasecmp
#endif

typedef enum { qfalse, qtrue } qboolean;
typedef float vec3_t[3];
enum { MAX_CLIENTS = 32, ENTITYNUM_WORLD = 1022, ENTITYNUM_NONE = 1023,
    ET_PLAYER = 1, ET_NPC = 13, CLASS_JEDI = 1, CLASS_VEHICLE = 2,
    SVMOD_JAPRO = 1, SVMOD_JAPLUS = 2, SVMOD_BASE = 3,
    JAPRO_CINFO_FLIPKICK = 1, JAPRO_CINFO_FIXSIDEKICK = 64,
    JAPLUS_CINFO_FLIPKICK = 1, MAX_PS_EVENTS = 2,
    EV_FALL = 1, EV_ROLL, EV_FIRE_WEAPON, EV_ALT_FIRE, EV_SABER_ATTACK,
    EV_USE_ITEM1, EV_USE_ITEM2, EV_USE_ITEM3, EV_USE_ITEM4, EV_USE_ITEM5,
    EV_USE_ITEM6, EV_USE_ITEM7, EV_USE_ITEM8, EV_USE_ITEM9, EV_USE_ITEM10, EV_USE_ITEM11,
    GT_SIEGE = 7, DF_NO_FALLING = 8, DAMAGE_NO_ARMOR = 2, MOD_FALLING = 19,
    CHAN_AUTO = 0, EF_INVULNERABLE = 1 << 27 };
#define S_COLOR_YELLOW ""
#define VectorSet(v,x,y,z) do { (v)[0]=(x); (v)[1]=(y); (v)[2]=(z); } while (0)
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); exit(1); } } while (0)

typedef struct { int eType, eFlags, NPC_class, number; } entityState_t;
typedef struct { int eFlags, duelInProgress, clientNum, groundEntityNum;
    int fallingToDeath, legsAnim, eventSequence, events[2], eventParms[2];
    vec3_t velocity; } playerState_t;
typedef struct { entityState_t s; } bgEntity_t;
typedef struct { playerState_t ps; struct { int raceMode; } sess;
    int dangerTime, invulnerableTimer; } gclient_t;
typedef struct { entityState_t s; gclient_t *client; void *NPC;
    int health, pain_debounce_time; } gentity_t;
typedef struct { int integer; float value; } cvar_t;
static cvar_t g_flipKick, g_maxFallDmg, dmflags, g_tweakJetpack;
static struct { int serverMod, jcinfo, cinfo; } cgs;
static int dueltypes[MAX_CLIENTS], cg_dueltypes[MAX_CLIENTS];
static bgEntity_t entities[ENTITYNUM_NONE + 1];
static struct { playerState_t *ps; struct { int upmove; } cmd; } movement, *pm = &movement;
static struct { int time, gametype; } level;
static int damageCalls, lastDamage, warnings;

/* Mock engine boundaries; the parser dispatch, eligibility, bounce and fall
 * event handlers below are extracted directly from the production sources. */
static bgEntity_t *PM_BGEntForNum(int num) { CHECK(num >= 0 && num < ENTITYNUM_WORLD); return &entities[num]; }
static const char *COM_ParseExt(const char **p, qboolean allowLines) {
    static char token[64];
    size_t i = 0;
    while (**p && isspace((unsigned char)**p)) {
        if (**p == '\n' && !allowLines) { token[0] = 0; return token; }
        ++*p;
    }
    while (**p && !isspace((unsigned char)**p) && i + 1 < sizeof(token))
        token[i++] = *(*p)++;
    token[i] = 0;
    return token;
}
static qboolean COM_ParseInt(const char **p, int *n) {
    const char *token = COM_ParseExt(p, qfalse);
    if (!token[0]) return qtrue;
    *n = atoi(token);
    return qfalse;
}
static void SkipRestOfLine(const char **p) { while (**p && **p != '\n') ++*p; }
static void Com_Printf(const char *format, ...) { (void)format; ++warnings; }
static qboolean BG_InKnockDownOnly(int anim) { return anim == 1 ? qtrue : qfalse; }
static void G_Damage(gentity_t *ent, void *a, void *b, void *c, void *d, int damage, int flags, int mod) {
    CHECK(flags == DAMAGE_NO_ARMOR && mod == MOD_FALLING);
    ++damageCalls; lastDamage = damage; ent->health -= damage;
}
static int G_SoundIndex(const char *name) { (void)name; return 1; }
static void G_Sound(gentity_t *ent, int channel, int sound) { (void)ent; (void)channel; (void)sound; }
#define FireWeapon(ent,alt) ((void)(ent))
#define ItemUse_Seeker(ent) ((void)(ent))
#define ItemUse_Shield(ent) ((void)(ent))
#define ItemUse_MedPack(ent) ((void)(ent))
#define ItemUse_MedPack_Big(ent) ((void)(ent))
#define ItemUse_Binoculars(ent) ((void)(ent))
#define ItemUse_Sentry(ent) ((void)(ent))
#define ItemUse_Jetpack(ent) ((void)(ent))
#define ItemUse_UseEWeb(ent) ((void)(ent))
#define ItemUse_UseCloak(ent) ((void)(ent))
#include "actual.h"

static void setFlipkick(int enabled) {
    g_flipKick.integer = enabled;
    cgs.jcinfo = enabled ? JAPRO_CINFO_FLIPKICK : 0;
    cgs.cinfo = enabled ? JAPLUS_CINFO_FLIPKICK : 0;
}

static void testParameter(void) {
    gclient_t client = {0};
    gentity_t npc = {0};
    npc.client = &client; npc.NPC = &npc;
    npc.s.eType = ET_NPC; npc.s.NPC_class = CLASS_JEDI;
    npc.s.eFlags = client.ps.eFlags = EF_INVULNERABLE;
    ParseNPC(&npc, "Flipkickable 1\n");
    CHECK(BG_IsFlipkickableNPC(&npc.s));
    CHECK(client.ps.eFlags == (EF_INVULNERABLE | EF_NPC_FLIPKICKABLE));
    CHECK(npc.s.eFlags == client.ps.eFlags);
    ParseNPC(&npc, "flipKICKable 1\nFlipkickable 0\n");
    CHECK(!BG_IsFlipkickableNPC(&npc.s));
    CHECK(npc.s.eFlags == EF_INVULNERABLE && client.ps.eFlags == EF_INVULNERABLE);
    ParseNPC(&npc, "FLIPKICKABLE 1\n");
    CHECK(BG_IsFlipkickableNPC(&npc.s));
    ParseNPC(&npc, "health 100\n");
    CHECK(!BG_IsFlipkickableNPC(&npc.s)); // Omission resets a previous opt-in.
    warnings = 0;
    ParseNPC(&npc, "Flipkickable -1\nFlipkickable 2\n");
    CHECK(warnings == 2 && !BG_IsFlipkickableNPC(&npc.s));
    ParseNPC(&npc, "Flipkickable\nhealth 100\n");
    CHECK(!BG_IsFlipkickableNPC(&npc.s));
    npc.NPC = NULL;
    ParseNPC(&npc, "Flipkickable 1\n");
    CHECK(!BG_IsFlipkickableNPC(&npc.s));
}

static void testMovement(void) {
    playerState_t player = {0};
    bgEntity_t *npc = &entities[MAX_CLIENTS];
    pm->ps = &player;
    npc->s.eType = ET_NPC; npc->s.NPC_class = CLASS_JEDI;
    npc->s.eFlags = EF_NPC_FLIPKICKABLE;
    CHECK(BG_IsFlipkickableNPC(&npc->s));
    CHECK(!BG_IsFlipkickableNPC(NULL));
    CHECK(PM_IsPlayerLikeGround(0));
    CHECK(PM_IsPlayerLikeGround(MAX_CLIENTS));
    CHECK(!PM_IsPlayerLikeGround(-1));
    CHECK(!PM_IsPlayerLikeGround(ENTITYNUM_WORLD));
    CHECK(!PM_IsPlayerLikeGround(ENTITYNUM_NONE));
    cgs.serverMod = SVMOD_JAPLUS;
    setFlipkick(1);
    CHECK(CanFlipkickNPC(&player, npc));
    CHECK(!CanFlipkickNPC(&player, NULL));
    setFlipkick(0);
    CHECK(!CanFlipkickNPC(&player, npc));
    BounceFromNPC(npc);
    CHECK(player.velocity[2] == 0 && pm->cmd.upmove == 0);
    npc->s.eFlags = 0;
    CHECK(!PM_IsPlayerLikeGround(MAX_CLIENTS));
    BounceFromNPC(npc);
    CHECK(player.velocity[2] == 270 && pm->cmd.upmove == 127);
    setFlipkick(1);
#ifdef _GAME
    CHECK(CanFlipkickNPC(&player, npc)); // Preserve existing server NPC kicks.
#else
    CHECK(!CanFlipkickNPC(&player, npc)); // Unflagged JA+ NPCs stay excluded.
#endif
    cgs.serverMod = SVMOD_JAPRO;
    CHECK(CanFlipkickNPC(&player, npc));
    npc->s.eFlags = EF_NPC_FLIPKICKABLE;
    npc->s.NPC_class = CLASS_VEHICLE;
    CHECK(!BG_IsFlipkickableNPC(&npc->s) && !PM_IsPlayerLikeGround(MAX_CLIENTS));
    npc->s.eType = ET_PLAYER;
    CHECK(!BG_IsFlipkickableNPC(&npc->s) && !CanFlipkickNPC(&player, npc));
    player.duelInProgress = 1; cgs.serverMod = SVMOD_JAPRO;
    dueltypes[0] = 0; cg_dueltypes[0] = 1;
    npc->s.eType = ET_NPC; npc->s.NPC_class = CLASS_JEDI;
    CHECK(!CanFlipkickNPC(&player, npc)); // NF duels still disable kicks.
}

static void testFall(void) {
    gclient_t client = {0};
    gentity_t npc = {0};
    npc.client = &client; npc.health = 1000;
    npc.s.eType = ET_NPC; npc.s.NPC_class = CLASS_JEDI;
    client.ps.eventSequence = 1; client.ps.events[0] = EV_FALL;
    client.ps.eventParms[0] = 50;
    damageCalls = 0;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 0);
    npc.s.eFlags = EF_NPC_FLIPKICKABLE;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 1 && lastDamage == 8);
    client.ps.legsAnim = 1; // Knocked-down landing uses splat damage.
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 2 && lastDamage == 50);
    g_maxFallDmg.integer = 30; g_maxFallDmg.value = 30;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 3 && lastDamage == 30);
    dmflags.integer = DF_NO_FALLING;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 3);
    dmflags.integer = 0; client.sess.raceMode = 1;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 3);
    client.sess.raceMode = 0; client.ps.fallingToDeath = 1;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 3);
    client.ps.fallingToDeath = 0; npc.s.NPC_class = CLASS_VEHICLE;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 3);
    npc.s.eType = ET_PLAYER; npc.s.eFlags = 0;
    ClientEvents(&npc, 0);
    CHECK(damageCalls == 4 && lastDamage == 30);
}

int main(void) {
    testParameter();
    testMovement();
    testFall();
    puts("NPC Flipkickable: parameter, prediction, head standing and fall/splat damage passed.");
    return 0;
}
