// Exercise production sound and weapon-change code with a controllable animation clock.
// Ghoul2 and audio output are mocked; this does not test rendering or live JA+ networking.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)
#define assert CHECK
#define DEBUGNAME(x) ((void)0)
#define MAX_CLIENTS 4
#define MAX_SABERS 2
#define MAX_WEAPONS 8
#define CHAN_AUTO 0
#define EF_DEAD 1
#define PMF_FOLLOW 1
#define PERS_TEAM 0
#define JAPRO_PLUGIN_HOLSTEREDSABER (1 << 9)
#define JAPRO_PLUGIN_HOLSTEREDSABERS (1 << 29)
enum { qfalse, qtrue };
enum { WP_NONE, WP_MELEE, WP_SABER };
enum { ET_PLAYER, ET_NPC };
enum { TEAM_FREE, TEAM_SPECTATOR };
enum { SVMOD_BASE, SVMOD_JAPLUS };
enum { SABER_SINGLE, SABER_STAFF };
enum { IDLE, BOTH_S1_S7, BOTH_STAND1TO2, BOTH_S1_S7_NEW, BOTH_STAND1TO2_NEW,
       BOTH_S7_S1_NEW, BOTH_STAND2TO1_NEW };
enum { EV_CHANGE_WEAPON, EV_ENTITY_SOUND, EV_SABER_UNHOLSTER };
enum { IGNITION = 10, SHUTDOWN, SELECT };
typedef int qboolean;
typedef int sfxHandle_t;
typedef float vec3_t[3];
typedef struct { float length, desiredLength; } blade_t;
typedef struct { char model[16]; int type, numBlades, soundOn, soundOff; blade_t blade[2]; } saberInfo_t;
typedef struct { int infoValid, team; saberInfo_t saber[2]; } clientInfo_t;
typedef struct {
    int number, clientNum, eType, weapon, saberHolstered, saberInFlight, eFlags;
    int torsoAnim, torsoFlip, eventParm, trickedentindex;
    struct { vec3_t trBase; } pos;
} entityState_t;
typedef struct {
    int clientNum, weapon, torsoAnim, saberHolstered, saberInFlight, eFlags, pm_flags, persistant[1];
    int duelInProgress, duelIndex;
    vec3_t origin;
} playerState_t;
typedef struct { playerState_t ps; int numEntities; entityState_t entities[MAX_CLIENTS]; } snapshot_t;
typedef struct {
    entityState_t currentState;
    int weapon, currentValid, torsoBolt, saberWasInFlight, saberHiltChanged;
    void *ghoul2, *ghoul2weapon;
    clientInfo_t *npcClient;
    vec3_t lerpOrigin;
    struct { struct { int animationNumber, animationTime, lastFlip; } torso; } pe;
} centity_t;
typedef struct { sfxHandle_t selectSound; } weaponInfo_t;
static weaponInfo_t cg_weapons[MAX_WEAPONS];
static centity_t cg_entities[MAX_CLIENTS];
static struct { int time, clientNum; playerState_t predictedPlayerState; snapshot_t *snap; } cg;
static snapshot_t snapshot;
static struct {
    int serverMod, gameModels[1], gameSounds[32];
    clientInfo_t clientinfo[MAX_CLIENTS];
    struct { int selectSound; } media;
} cgs;
static struct { int integer; } cp_pluginDisable, cg_holsteredStaffSound;
static struct { float value; } cg_holsteredStaffSwap;
static int sounds[64], soundTimes[64], soundCount, boneQueries;
static float boneFrame;
static int weaponModels[MAX_WEAPONS];

static void StartSound(const float *origin, int client, int channel, int sound) {
    CHECK(soundCount < 64);
    sounds[soundCount] = sound;
    soundTimes[soundCount++] = cg.time;
}
static int BoneAnim(void *model, const char *bone, int time, float *frame, int *first,
                    int *last, int *flags, float *speed, int *models, int index) {
    ++boneQueries;
    *frame = boneFrame;
    *first = 0;
    *last = 100;
    *flags = 0;
    *speed = 1;
    return qtrue;
}
static struct {
    void (*S_StartSound)(const float *, int, int, int);
    int (*G2API_GetBoneAnim)(void *, const char *, int, float *, int *, int *, int *, float *, int *, int);
} imports = { StartSound, BoneAnim }, *trap = &imports;
static float Com_Clamp(float low, float high, float value) {
    return value < low ? low : (value > high ? high : value);
}
static float DistanceSquared(const float *a, const float *b) {
    float x = a[0] - b[0], y = a[1] - b[1], z = a[2] - b[2];
    return x*x + y*y + z*z;
}
static void *CG_G2WeaponInstance(centity_t *cent, int weapon) { return &weaponModels[weapon]; }
static void CG_CopyG2WeaponInstance(centity_t *cent, int weapon, void *model) {}
static void BG_SI_SetDesiredLength(saberInfo_t *saber, float length, int blade) {
    int i;
    for (i = 0; i < saber->numBlades; ++i) saber->blade[i].desiredLength = length;
}
static int CG_ForceOwnSaberSound(const entityState_t *es, int source, int sound) { return sound; }
static const char *CG_ConfigString(int index) { return "*voice"; }
static int CG_ClassifyVoiceLine(const char *name, int *line) { return qfalse; }
static int CG_VoiceLineThrottled(int source, int line) { return qfalse; }
static int CG_CustomSound(int source, const char *name) { return SELECT; }
#define CS_SOUNDS 0

#include "actual.h"

static void Reset(int staff) {
    centity_t *cent;
    clientInfo_t *ci;
    memset(&cg, 0, sizeof(cg));
    memset(&cgs, 0, sizeof(cgs));
    memset(&snapshot, 0, sizeof(snapshot));
    memset(cg_entities, 0, sizeof(cg_entities));
    memset(staffSwapSound, 0, sizeof(staffSwapSound));
    memset(staffSwapLatch, 0, sizeof(staffSwapLatch));
    soundCount = boneQueries = 0;
    cg.time = 1000;
    cgs.serverMod = SVMOD_JAPLUS;
    cgs.media.selectSound = SELECT;
    cgs.gameSounds[IGNITION] = IGNITION;
    cgs.gameSounds[SHUTDOWN] = SHUTDOWN;
    cgs.gameSounds[SELECT] = SELECT;
    cp_pluginDisable.integer = 0;
    cg_holsteredStaffSound.integer = 1;
    cg_holsteredStaffSwap.value = 0.45f;
    cent = &cg_entities[0];
    ci = &cgs.clientinfo[0];
    cent->currentValid = qtrue;
    cent->ghoul2 = cent;
    cent->currentState.weapon = cent->weapon = cg.predictedPlayerState.weapon = WP_SABER;
    cent->ghoul2weapon = CG_G2WeaponInstance(cent, WP_SABER);
    ci->infoValid = qtrue;
    ci->saber[0].type = staff ? SABER_STAFF : SABER_SINGLE;
    ci->saber[0].numBlades = staff ? 2 : 1;
    ci->saber[0].soundOn = IGNITION;
    ci->saber[0].soundOff = SHUTDOWN;
    // WP_RemoveSaber calls WP_SaberSetDefaults, then clears only the model. A missing
    // second hilt still has sound handles; zero-filled mocks hid its early ignition.
    ci->saber[1].soundOn = IGNITION;
    ci->saber[1].soundOff = SHUTDOWN;
    ci->saber[0].blade[0].length = 40;
    ci->saber[0].blade[0].desiredLength = -1;
}
static void Anim(int anim, float frame) {
    cg.predictedPlayerState.torsoAnim = cg_entities[0].currentState.torsoAnim = anim;
    cg_entities[0].pe.torso.animationNumber = anim;
    cg_entities[0].pe.torso.animationTime = cg.time;
    boneFrame = frame;
}
static void Update(float target) {
    cgs.clientinfo[0].saber[0].blade[0].desiredLength = target;
    CG_StaffSwapUpdateSounds(&cg_entities[0], &cgs.clientinfo[0]);
}
static void Event(int weapon) {
    cg_entities[0].currentState.eventParm = weapon;
    WeaponChangeEvent(&cg_entities[0]);
}
static void Commit(int weapon) {
    cg.predictedPlayerState.weapon = weapon;
    CG_CheckPlayerG2Weapons(&cg.predictedPlayerState, &cg_entities[0]);
    cg_entities[0].currentState.weapon = weapon;
}
static int Count(int sound) {
    int i, n = 0;
    for (i = 0; i < soundCount; ++i) if (sounds[i] == sound) ++n;
    return n;
}

static void TestCanceledWeaponSwitch(void) {
    Reset(0);
    Commit(WP_MELEE);
    Event(WP_SABER);
    Commit(WP_SABER);
    soundCount = 0;
    Event(WP_MELEE); //lowering starts, but the player reselects saber before it finishes
    CHECK(Count(SHUTDOWN) == 0);
    Commit(WP_SABER); //the twirl keeps the same weapon and blade
    CHECK(Count(SHUTDOWN) == 0 && Count(IGNITION) == 0);
    Event(WP_MELEE);
    CHECK(Count(SHUTDOWN) == 0);
    Commit(WP_MELEE); //a real switch still shuts down exactly once
    Commit(WP_MELEE);
    CHECK(Count(SHUTDOWN) == 1);
}
static void TestManualShutdownBeforeMelee(void) {
    int anim, clearHolstered;
    for (anim = 0; anim < 2; anim++) for (clearHolstered = 0; clearHolstered < 2; clearHolstered++) {
        Reset(1);
        Anim(IDLE, 0);
        Update(-1);
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 2;
        trap->S_StartSound(cg_entities[0].lerpOrigin, 0, CHAN_AUTO, SHUTDOWN);
        Update(0); //the manual toggle sounded, but the blade is still retracting
        Anim(anim ? BOTH_STAND2TO1_NEW : BOTH_S7_S1_NEW, 90);
        Update(0);
        Commit(WP_MELEE);
        if (clearHolstered)
            cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
        Update(0);
        CHECK(Count(SHUTDOWN) == 1);
    }
    Reset(1);
    cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 2;
    trap->S_StartSound(cg_entities[0].lerpOrigin, 0, CHAN_AUTO, SHUTDOWN);
    Update(0); //manual shutdown in idle, before any put-away animation arrives
    cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
    Commit(WP_MELEE); //PM_Weapon clears saberHolstered once the weapon is no longer saber
    Update(0);
    CHECK(Count(SHUTDOWN) == 1);
}
static void ServerShutdown(int general) {
    centity_t event = {0};
    if (general) {
        if (!CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, SHUTDOWN))
            trap->S_StartSound(cg_entities[0].lerpOrigin, 0, CHAN_AUTO, SHUTDOWN);
    } else {
        event.currentState.eventParm = SHUTDOWN;
        EntitySoundEvent(&event);
    }
}
static void TestServerShutdownBeforeMelee(void) {
    int general, anim;
    for (general = 0; general < 2; general++) for (anim = 0; anim < 2; anim++) {
        Reset(1);
        cg.snap = &snapshot;
        snapshot.ps.weapon = WP_SABER;
        snapshot.ps.saberHolstered = 2;
        //The server sound precedes prediction and the blade update.
        ServerShutdown(general);
        CHECK(Count(SHUTDOWN) == 1);
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 2;
        Update(0);
        snapshot.ps.weapon = WP_MELEE;
        snapshot.ps.saberHolstered = 0;
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
        Anim(anim ? BOTH_STAND2TO1_NEW : BOTH_S7_S1_NEW, 90);
        Commit(WP_MELEE);
        Update(0);
        ServerShutdown(general); //a later server weapon-switch copy
        CHECK(Count(SHUTDOWN) == 1);
    }
}
static void TestShutdownCopiesAndToggles(void) {
    int general;
    for (general = 0; general < 2; general++) {
        Reset(1);
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 2;
        Update(0); //the off state can be rendered before its sound arrives
        ServerShutdown(general);
        CHECK(Count(SHUTDOWN) == 1);
        ServerShutdown(general);
        CHECK(Count(SHUTDOWN) == 1);

        {
            centity_t event = {0};
            cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
            event.currentState.eventParm = IGNITION;
            EntitySoundEvent(&event); //in-hand on/off commands between rendered frames
            cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 2;
            ServerShutdown(general);
            CHECK(Count(SHUTDOWN) == 2 && Count(IGNITION) == 1);
        }

        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
        Update(-1); //a real reactivation allows the next manual shutdown
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 2;
        ServerShutdown(general);
        Update(0);
        CHECK(Count(SHUTDOWN) == 3);

        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
        Update(-1);
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 1;
        ServerShutdown(general); //only the second blade closes; first blade remains lit
        Update(-1);
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 0;
        Update(-1);
        cg.predictedPlayerState.saberHolstered = cg_entities[0].currentState.saberHolstered = 1;
        ServerShutdown(general);
        Update(-1);
        CHECK(Count(SHUTDOWN) == 5);

        Anim(BOTH_STAND2TO1_NEW, 90);
        Update(0); //the remaining blade shuts down at the start of the back reach
        CHECK(Count(SHUTDOWN) == 6);
        ServerShutdown(general); //consume the server copy after the blade already sounded
        CHECK(Count(SHUTDOWN) == 6);
    }
}
static void TestRemoteShutdownSnapshotOrdering(void) {
    vec3_t origin = {200, 0, 0};
    Reset(1);
    cg.predictedPlayerState.clientNum = 1;
    cg.snap = &snapshot;
    snapshot.ps.clientNum = 1;
    snapshot.numEntities = 1;
    snapshot.entities[0] = cg_entities[0].currentState;
    snapshot.entities[0].saberHolstered = 2;
    snapshot.entities[0].pos.trBase[0] = 200;
    cg_entities[0].currentValid = qfalse;
    //The temp sound transitions before the carrier, who has moved since the last render.
    CHECK(!CG_StaffSwapHoldGeneralSound(origin, SHUTDOWN));
    trap->S_StartSound(origin, 0, CHAN_AUTO, SHUTDOWN);
    snapshot.entities[0].weapon = WP_MELEE;
    snapshot.entities[0].saberHolstered = 0;
    CHECK(CG_StaffSwapHoldGeneralSound(origin, SHUTDOWN));
    CHECK(Count(SHUTDOWN) == 1);

    Reset(1);
    cg_entities[0].lerpOrigin[0] = 20;
    cg_entities[1] = cg_entities[0];
    cg_entities[1].currentState.number = cg_entities[1].currentState.clientNum = 1;
    cg_entities[1].lerpOrigin[0] = 0;
    cgs.clientinfo[1] = cgs.clientinfo[0];
    cgs.clientinfo[1].saber[0].type = SABER_SINGLE;
    cgs.clientinfo[1].saber[0].numBlades = 1;
    staffSwapSound[0].shutdownPlayed = qtrue;
    CHECK(!CG_StaffSwapHoldGeneralSound(cg_entities[1].lerpOrigin, SHUTDOWN));
}
static void TestStaffTiming(void) {
    Reset(1);
    Anim(BOTH_S7_S1_NEW, 90);
    Update(0); //the blade starts retracting while the weapon still says saber
    CHECK(Count(SHUTDOWN) == 1 && soundTimes[0] == cg.time);
    cg.time += 50;
    Update(0);
    Commit(WP_MELEE);
    Update(0);
    CHECK(Count(SHUTDOWN) == 1);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    cg.time += 50;
    Anim(BOTH_S1_S7_NEW, 10);
    Commit(WP_SABER); //prediction sees saber before the entity does
    CHECK(Count(IGNITION) == 0);
    Update(0);
    CHECK(Count(IGNITION) == 0);
    cg.time += 20;
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1 && soundTimes[soundCount - 1] == cg.time);
    CHECK(CG_StaffSwapHoldIgnitionSound(0, IGNITION));
    CHECK(CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    cgs.clientinfo[0].saber[0].blade[0].length = 40;
    cg.time += 20;
    Anim(BOTH_S7_S1_NEW, 90);
    Update(0); //another real holster within 2 seconds must not be suppressed
    CHECK(Count(SHUTDOWN) == 2);
}
static void TestMissedAnimationFrames(void) {
    Reset(1);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_STAND1TO2_NEW, 60); //first rendered draw frame already has the hilt in hand
    CHECK(CG_StaffSwapHoldIgnitionSound(0, IGNITION));
    Update(-1);
    CHECK(Count(IGNITION) == 1 && soundTimes[0] == cg.time); //no extra 150 ms wait
    Reset(1);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_S1_S7_NEW, 60);
    Update(-1); //server ignition has not arrived; the visible blade still gets its sound now
    CHECK(Count(IGNITION) == 1);
    cg.time += 100;
    CHECK(CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    Reset(1);
    Commit(WP_MELEE); //the completed weapon change precedes the JA+ reach animation
    CHECK(Count(SHUTDOWN) == 0);
    Update(0);
    CHECK(Count(SHUTDOWN) == 1);
}
static void TestCanceledDraw(void) {
    Reset(1);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_S1_S7_NEW, 10);
    CHECK(CG_StaffSwapHoldIgnitionSound(0, IGNITION));
    Update(0);
    Commit(WP_MELEE);
    Anim(IDLE, 0);
    Update(0);
    cg.time += 3000;
    Update(0);
    CHECK(Count(IGNITION) == 0 && Count(SHUTDOWN) == 0);
}
static void TestWeaponChangeBeforeDrawAnimation(void) {
    Reset(1);
    Commit(WP_MELEE);
    Update(0);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    soundCount = 0;
    Anim(IDLE, 90); //weapon prediction commits before the server sends the reach
    Commit(WP_SABER);
    CHECK(Count(IGNITION) == 0);
    Update(-1);
    CHECK(Count(IGNITION) == 0);
    cg.time += 20;
    Anim(BOTH_S1_S7, 90); //plain draw still leaves the hilt on the back
    Update(0);
    CHECK(Count(IGNITION) == 0 && staffSwapSound[0].sound == IGNITION);
    cg.time += 20;
    Anim(BOTH_S1_S7_NEW, 10);
    Update(0);
    CHECK(Count(IGNITION) == 0);
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1 && soundTimes[0] == cg.time);
}
static void TestStaleTorsoFrames(void) {
    const char *why = "";
    float fraction = 0;
    centity_t *cent;
    Reset(1);
    cent = &cg_entities[0];
    cent->currentState.torsoAnim = BOTH_S1_S7_NEW;
    boneFrame = 90; //outgoing idle frame would incorrectly latch the hilt in hand
    CHECK(CG_StaffSwapPhaseReal(cent, &cgs.clientinfo[0], &why, &fraction) == STAFFSWAP_ONBACK);
    CHECK(boneQueries == 0);
    Anim(BOTH_S1_S7_NEW, 10);
    CHECK(CG_StaffSwapPhaseReal(cent, &cgs.clientinfo[0], &why, &fraction) == STAFFSWAP_ONBACK);
    boneFrame = 46;
    CHECK(CG_StaffSwapPhaseReal(cent, &cgs.clientinfo[0], &why, &fraction) == STAFFSWAP_INHAND);
    boneFrame = 43; //animation blending must not move the hilt back or repeat the sound
    CHECK(CG_StaffSwapPhaseReal(cent, &cgs.clientinfo[0], &why, &fraction) == STAFFSWAP_INHAND);
    CHECK(soundCount == 0); //phase queries have no audio side effects
    cent->currentState.torsoFlip = 1; //same animation restarted but not yet applied
    CHECK(CG_StaffSwapPhaseReal(cent, &cgs.clientinfo[0], &why, &fraction) == STAFFSWAP_ONBACK);
}
static void TestResidualBladeBeforeReach(void) {
    Reset(1);
    Commit(WP_MELEE);
    Update(0);
    soundCount = 0;
    Anim(IDLE, 90);
    Commit(WP_SABER); //rapid redraw while the blade is still retracting
    Update(-1);
    CHECK(Count(IGNITION) == 0 && staffSwapSound[0].sound == IGNITION);
    CHECK(CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    cg.time += 20;
    Anim(BOTH_STAND1TO2_NEW, 10);
    Update(0);
    CHECK(Count(IGNITION) == 0);
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1);
}
static void TestSingleBladeServerIgnition(void) {
    Reset(1);
    cg.predictedPlayerState.clientNum = 1; //observe a remote staff carrier
    cg_entities[0].currentState.saberHolstered = 1;
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_STAND1TO2_NEW, 10);
    //Sound events arrive before the render update records the activation.
    CHECK(CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    Update(0);
    CHECK(Count(IGNITION) == 0);
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    CHECK(CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    Update(-1);
    CHECK(Count(IGNITION) == 1);

    Reset(1);
    cg_entities[0].currentState.saberHolstered = 2; //prediction is ahead of the snapshot
    cg.predictedPlayerState.saberHolstered = 1;
    cg.predictedPlayerState.torsoAnim = BOTH_STAND1TO2_NEW;
    CHECK(CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
}
static void TestGatingAndReset(void) {
    Reset(1);
    cg_holsteredStaffSound.integer = 0;
    Commit(WP_MELEE);
    CHECK(Count(SHUTDOWN) == 1);
    Reset(1);
    cp_pluginDisable.integer = JAPRO_PLUGIN_HOLSTEREDSABER;
    CHECK(!CG_StaffSwapHoldShutdownSound(0));
    Reset(0);
    CHECK(!CG_StaffSwapHoldShutdownSound(0));
    Reset(1);
    Anim(BOTH_S1_S7_NEW, 10);
    CHECK(CG_StaffSwapHoldIgnitionSound(0, IGNITION));
    cg_entities[0].currentState.eFlags = EF_DEAD;
    Update(0);
    CHECK(!staffSwapSound[0].sound && soundCount == 0);
    CG_StaffSwapForgetClient(0);
    CHECK(!staffSwapLatch[0].swapped);
}
static void TestEntitySoundBeforePrediction(void) {
    centity_t event = {0};
    Reset(1);
    Commit(WP_MELEE);
    Update(0);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    soundCount = 0;
    cg.snap = &snapshot;
    snapshot.ps.weapon = WP_SABER;
    snapshot.ps.torsoAnim = BOTH_S1_S7_NEW;
    // Snapshot events run before prediction selects the saber or installs the reach.
    event.currentState.eventParm = IGNITION;
    EntitySoundEvent(&event);
    CHECK(Count(IGNITION) == 0 && staffSwapSound[0].sound == IGNITION);
    Anim(BOTH_S1_S7_NEW, 10);
    Commit(WP_SABER);
    Update(0);
    CHECK(Count(IGNITION) == 0);
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    EntitySoundEvent(&event);
    CHECK(Count(IGNITION) == 1); //late server copy is consumed
    event.currentState.eventParm = SHUTDOWN;
    EntitySoundEvent(&event);
    CHECK(Count(SHUTDOWN) == 1); //ordinary entity sounds still play
    event.currentState.eventParm = SELECT;
    EntitySoundEvent(&event);
    CHECK(Count(SELECT) == 1);
}
static void TestGeneralSoundSnapshotOrdering(void) {
    vec3_t origin = {200, 0, 0};
    Reset(1);
    Commit(WP_MELEE);
    Update(0);
    soundCount = 0;
    cg.snap = &snapshot;
    snapshot.ps.weapon = WP_SABER;
    snapshot.ps.torsoAnim = BOTH_S1_S7_NEW;
    snapshot.ps.origin[0] = 200;
    cg_entities[0].currentValid = qfalse;
    // Previous prediction and lerpOrigin still describe melee at a different position.
    CHECK(CG_StaffSwapHoldGeneralSound(origin, IGNITION));
    CHECK(soundCount == 0 && staffSwapSound[0].sound == IGNITION);

    Reset(1);
    cg.predictedPlayerState.clientNum = 1;
    cg.snap = &snapshot;
    snapshot.ps.clientNum = 1;
    snapshot.numEntities = 1;
    snapshot.entities[0] = cg_entities[0].currentState;
    snapshot.entities[0].torsoAnim = BOTH_S1_S7_NEW;
    snapshot.entities[0].pos.trBase[0] = 200;
    cg_entities[0].currentValid = qfalse;
    cg_entities[0].currentState.weapon = cg_entities[0].weapon = WP_MELEE;
    // The temp sound can be transitioned before the remote player's entity.
    CHECK(CG_StaffSwapHoldGeneralSound(origin, IGNITION));
    CHECK(staffSwapSound[0].sound == IGNITION);
}
static void TestInHandToggleAndNearbyPlayer(void) {
    Reset(1);
    CHECK(!CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    CHECK(!CG_StaffSwapHoldIgnitionSound(0, IGNITION)); //an ordinary in-hand toggle
    Anim(BOTH_S1_S7_NEW, 10);
    cg_entities[0].lerpOrigin[0] = 20;
    cg_entities[1] = cg_entities[0];
    cg_entities[1].currentState.number = cg_entities[1].currentState.clientNum = 1;
    cg_entities[1].lerpOrigin[0] = 0;
    cgs.clientinfo[1] = cgs.clientinfo[0];
    cgs.clientinfo[1].saber[0].type = SABER_SINGLE;
    cgs.clientinfo[1].saber[0].numBlades = 1;
    CHECK(!CG_StaffSwapHoldGeneralSound(cg_entities[1].lerpOrigin, IGNITION));
}
static void TestUnholsterWithoutSecondHilt(void) {
    Reset(1);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_S1_S7_NEW, 10);
    SaberUnholsterEvent(&cg_entities[0]);
    CHECK(Count(IGNITION) == 0 && staffSwapSound[0].sound == IGNITION);
    Update(0);
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    SaberUnholsterEvent(&cg_entities[0]);
    CHECK(Count(IGNITION) == 1);
}
static void TestRealSecondHilt(void) {
    Reset(0);
    strcpy(cgs.clientinfo[0].saber[1].model, "second-hilt");
    Commit(WP_MELEE);
    CHECK(Count(SHUTDOWN) == 2);
    soundCount = 0;
    Commit(WP_SABER);
    CHECK(Count(IGNITION) == 2);
    SaberUnholsterEvent(&cg_entities[0]);
    CHECK(Count(IGNITION) == 4);
}
static void TestSecondBladeAfterDraw(void) {
    centity_t event = {0};
    Reset(1);
    cg_entities[0].currentState.saberHolstered = cg.predictedPlayerState.saberHolstered = 1;
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_STAND1TO2_NEW, 60);
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    cgs.clientinfo[0].saber[0].blade[0].length = 40;
    Anim(IDLE, 0);
    Update(-1);
    // Cycling staff stance opens the second blade without another back draw.
    cg_entities[0].currentState.saberHolstered = cg.predictedPlayerState.saberHolstered = 0;
    event.currentState.eventParm = IGNITION;
    EntitySoundEvent(&event);
    CHECK(Count(IGNITION) == 2);
    CHECK(!CG_StaffSwapHoldGeneralSound(cg_entities[0].lerpOrigin, IGNITION));
    cg_entities[0].currentState.saberHolstered = cg.predictedPlayerState.saberHolstered = 1;
    Update(-1); // First blade stays on throughout repeated stance changes.
    cg_entities[0].currentState.saberHolstered = cg.predictedPlayerState.saberHolstered = 0;
    EntitySoundEvent(&event);
    CHECK(Count(IGNITION) == 3);

    Reset(1);
    cg_entities[0].currentState.saberHolstered = cg.predictedPlayerState.saberHolstered = 1;
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_STAND1TO2_NEW, 60);
    Update(-1);
    CHECK(Count(IGNITION) == 1);
    cg.snap = &snapshot;
    snapshot.ps.weapon = WP_SABER;
    snapshot.ps.torsoAnim = BOTH_STAND1TO2_NEW;
    snapshot.ps.saberHolstered = 0;
    // The second-blade event runs before prediction, while the reach lingers.
    EntitySoundEvent(&event);
    CHECK(Count(IGNITION) == 2);
}
static void TestDrawEndsBetweenRenders(void) {
    Reset(1);
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    Anim(BOTH_S1_S7_NEW, 10);
    CHECK(CG_StaffSwapHoldIgnitionSound(0, IGNITION));
    Update(0);
    cg.time += 500;
    Anim(IDLE, 0); // The next rendered frame is after the entire reach.
    Update(-1);
    CHECK(Count(IGNITION) == 1 && !staffSwapSound[0].sound);
    Update(-1);
    CHECK(Count(IGNITION) == 1);

    Reset(1);
    Commit(WP_MELEE);
    Update(0);
    soundCount = 0;
    Anim(IDLE, 0);
    Commit(WP_SABER); // No reach animation arrives at all.
    Update(-1);
    CHECK(Count(IGNITION) == 0);
    cg.time += 2100;
    Update(-1);
    CHECK(Count(IGNITION) == 1 && !staffSwapSound[0].sound);
    Update(-1);
    CHECK(Count(IGNITION) == 1);
}
static void TestConfirmedHiltChanges(void) {
    const qboolean changed[MAX_SABERS] = { qtrue, qfalse };
    const qboolean unchanged[MAX_SABERS] = { qfalse, qfalse };
    centity_t *cent;
    int hilt, blocked;
    Reset(1);
    cent = &cg_entities[0];
    Anim(IDLE, 0);
    for (hilt = 1; hilt <= 4; hilt++) {
        cgs.clientinfo[0].saber[0].soundOn = 20 + hilt;
        CG_SaberClientInfoChanged(0, qtrue, changed);
        CG_SaberClientInfoChanged(0, qtrue, unchanged);
        // Even if the model has already been reattached, each confirmed hilt
        // must sound once. An idle staff is already in hand and needs no reach.
        Commit(WP_SABER);
        CHECK(soundCount == hilt && sounds[hilt - 1] == 20 + hilt);
        CHECK(!cent->saberHiltChanged && !staffSwapSound[0].sound);
        Update(-1);
        Commit(WP_SABER);
        CHECK(soundCount == hilt);
    }

    Reset(1);
    Anim(IDLE, 0);
    cent->weapon = WP_NONE;
    cent->ghoul2weapon = NULL;
    cent->saberWasInFlight = qtrue; //old return marker belongs to the previous hilt
    CG_SaberClientInfoChanged(0, qtrue, changed);
    Commit(WP_SABER);
    CHECK(Count(IGNITION) == 1 && !cent->saberHiltChanged);

    // Replacing a hilt during a real draw still honors its handoff and consumes
    // late copies. The new hilt must not inherit the old draw's consumed sound.
    Reset(1);
    Anim(BOTH_S1_S7_NEW, 10);
    staffSwapSound[0].ignitionPlayed = qtrue;
    cgs.clientinfo[0].saber[0].blade[0].length = 0;
    CG_SaberClientInfoChanged(0, qtrue, changed);
    Commit(WP_SABER);
    CHECK(Count(IGNITION) == 0 && staffSwapSound[0].sound == IGNITION);
    Update(0);
    CG_SaberClientInfoChanged(0, qtrue, unchanged); //name/color refresh while held
    CHECK(staffSwapSound[0].sound == IGNITION);
    boneFrame = 46;
    Update(-1);
    CHECK(Count(IGNITION) == 1 && !staffSwapSound[0].sound);
    SaberUnholsterEvent(cent);
    Update(-1);
    CHECK(Count(IGNITION) == 1);

    // No actual changed hilt means no feedback (rejected or redundant commands).
    Reset(1);
    Anim(IDLE, 0);
    CG_SaberClientInfoChanged(0, qtrue, unchanged);
    Commit(WP_SABER);
    CHECK(soundCount == 0);
    CG_SaberClientInfoChanged(0, qfalse, changed); //initial loading
    CHECK(!cent->saberHiltChanged);
    cgs.clientinfo[0].infoValid = qfalse;
    CG_SaberClientInfoChanged(0, qtrue, changed); //first clientinfo
    CHECK(!cent->saberHiltChanged);
    cgs.clientinfo[1].infoValid = qtrue;
    CG_SaberClientInfoChanged(1, qtrue, changed);
    CHECK(!cg_entities[1].saberHiltChanged);

    // A missing player model defers feedback until it can be attached.
    Reset(1);
    Anim(IDLE, 0);
    cent->ghoul2 = NULL;
    CG_SaberClientInfoChanged(0, qtrue, changed);
    Commit(WP_SABER);
    CHECK(soundCount == 0 && cent->saberHiltChanged);
    cent->ghoul2 = cent;
    Commit(WP_SABER);
    CHECK(Count(IGNITION) == 1 && !cent->saberHiltChanged);

    for (blocked = 0; blocked < 5; blocked++) {
        Reset(1);
        CG_SaberClientInfoChanged(0, qtrue, changed);
        switch (blocked) {
        case 0: cg.predictedPlayerState.pm_flags = PMF_FOLLOW; break;
        case 1: cent->currentState.eFlags = EF_DEAD; break;
        case 2: cgs.clientinfo[0].team = TEAM_SPECTATOR; break;
        case 3: cent->torsoBolt = 1; break;
        case 4: cent->currentState.saberInFlight = qtrue; break;
        }
        Commit(WP_SABER);
        CHECK(soundCount == 0 && !cent->saberHiltChanged);
    }
}

int main(void) {
    TestManualShutdownBeforeMelee();
    TestServerShutdownBeforeMelee();
    TestShutdownCopiesAndToggles();
    TestRemoteShutdownSnapshotOrdering();
    TestConfirmedHiltChanges();
    TestSecondBladeAfterDraw();
    TestDrawEndsBetweenRenders();
    TestCanceledWeaponSwitch();
    TestStaffTiming();
    TestMissedAnimationFrames();
    TestCanceledDraw();
    TestWeaponChangeBeforeDrawAnimation();
    TestStaleTorsoFrames();
    TestResidualBladeBeforeReach();
    TestSingleBladeServerIgnition();
    TestGatingAndReset();
    TestEntitySoundBeforePrediction();
    TestGeneralSoundSnapshotOrdering();
    TestInHandToggleAndNearbyPlayer();
    TestUnholsterWithoutSecondHilt();
    TestRealSecondHilt();
    puts("Saber holster sound regression checks passed.");
    return 0;
}
