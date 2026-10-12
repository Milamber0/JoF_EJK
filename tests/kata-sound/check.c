#include "../../codemp/qcommon/q_shared.h"
// Include client-only JA+ animation IDs without changing the harness imports.
#define _CGAME
#include "../../codemp/game/bg_public.h"
#include "../../codemp/game/anims.h"

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)

static int randomChoice, customEntity, customCalls, registrations;
static char customName[MAX_QPATH], registeredName[MAX_QPATH];
static int transitions, damageFeedback, painFeedback;
static int animationEvents;
#define MAX_CUSTOM_SOUNDS 40
#define MAX_CUSTOM_COMBAT_SOUNDS 40
#define MAX_CUSTOM_EXTRA_SOUNDS 40
#define MAX_CUSTOM_JEDI_SOUNDS 40
typedef struct { sfxHandle_t sounds[40]; } clientInfo_t;
static clientInfo_t testNpc;
static int testNpcSounds;
static const char *npcNames[] = { "*combat1", "*combat2", "*combat3", "*gasp", NULL };
static const char *GetCustomSoundForType(int type, int index) {
    (void)type;
    return npcNames[index];
}
static void SetCustomSoundForType(clientInfo_t *ci, int type, int index, sfxHandle_t sound) {
    (void)type;
    ci->sounds[index] = sound;
}
typedef struct {
    entityState_t currentState, nextState;
    qboolean currentValid;
    qboolean interpolate;
    void *ghoul2;
    vec3_t turAngles, lerpOrigin, modelScale;
} centity_t;
typedef centity_t testEntity_t;
static testEntity_t cg_entities[MAX_GENTITIES];
bgLoadedAnim_t bgAllAnims[MAX_ANIM_FILES];
bgLoadedEvents_t bgAllEvents[MAX_ANIM_FILES];
int bgNumAnimEvents = 1;
static void CG_PlayerAnimEventDo(testEntity_t *ent, animevent_t *event) {
    (void)ent; (void)event;
    animationEvents++;
}
typedef struct { playerState_t ps; } testSnapshot_t;
static testSnapshot_t serverSnapshot;
static struct {
    playerState_t predictedPlayerState;
    testSnapshot_t *snap;
    vec3_t predictedError;
    int predictedErrorTime;
    float predictedTimeFrac;
    int time;
} cg;
static struct { qhandle_t gameModels[MAX_MODELS]; } cgs;
static struct { int integer; } cg_noPredict, g_synchronousClients, cg_smoothClients;
#define CAMERA_MIN_FPS 15
static struct { int integer; float value; } cg_cameraFPS;
static qboolean CG_UsingEWeb(void) { return qfalse; }
static void CG_AddCEntity(centity_t *cent) {
    CHECK(!cent->interpolate && !cg_smoothClients.integer);
    // The normal entity interpolation path would blend this already
    // interpolated position toward nextState for a second time.
    CHECK(cent == &cg_entities[cg.predictedPlayerState.clientNum]);
}

void QDECL Com_Printf(const char *format, ...) { (void)format; }
void QDECL Com_Error(int code, const char *format, ...) { (void)code; (void)format; exit(2); }
void *BG_Alloc(int size) { void *p = calloc(1, size); CHECK(p); return p; }
int Q_irand(int low, int high) { CHECK(high >= low); return randomChoice ? high : low; }
void VectorClear(vec3_t v) { v[0] = v[1] = v[2] = 0; }
void VectorCopy(const vec3_t a, vec3_t b) { int i; for (i = 0; i < 3; ++i) b[i] = a[i]; }
void VectorSubtract(const vec3_t a, const vec3_t b, vec3_t out) {
    int i; for (i = 0; i < 3; ++i) out[i] = a[i] - b[i];
}
void VectorScale(const vec3_t a, float scale, vec3_t out) {
    int i; for (i = 0; i < 3; ++i) out[i] = a[i] * scale;
}
vec_t DistanceSquared(const vec3_t a, const vec3_t b) {
    vec3_t delta;
    int i;
    for (i = 0; i < 3; ++i) delta[i] = a[i] - b[i];
    return delta[0]*delta[0] + delta[1]*delta[1] + delta[2]*delta[2];
}

static sfxHandle_t RegisterSound(const char *name) {
    CHECK(name[0] != '*');
    Q_strncpyz(registeredName, name, sizeof(registeredName));
    if (testNpcSounds) {
        registrations++;
        return !strcmp(name, "sound/chars/kothos/misc/combat1") ? 801 : 0;
    }
    return 100 + ++registrations;
}
static int RegisterEffect(const char *name) { (void)name; return 1; }
static int modelEventLength = -1, eventParses, fileCloses;
static char eventDirectory[MAX_QPATH];
static const char *glaName = "models/players/_humanoid/_humanoid.gla";
static int OpenFile(const char *name, fileHandle_t *file, fsMode_t mode) {
    CHECK(mode == FS_READ);
    CHECK(!strcmp(name, "models/players/custom/animevents.cfg"));
    *file = modelEventLength >= 0 ? 1 : 0;
    return modelEventLength;
}
static void CloseFile(fileHandle_t file) { CHECK(file == 1); fileCloses++; }
static void GetGLAName(void *g2, int model, char *name) {
    CHECK(g2 == &testNpc && model == 0);
    Q_strncpyz(name, glaName, MAX_QPATH);
}
int BG_ParseAnimationEvtFile(const char *directory, int animIndex, int eventIndex) {
    CHECK(eventIndex == bgNumAnimEvents);
    CHECK(animIndex == 0 || animIndex == 2);
    eventParses++;
    Q_strncpyz(eventDirectory, directory, sizeof(eventDirectory));
    return !strcmp(directory, "models/players/custom/") && modelEventLength == 0 ? -1 : 7;
}
static int handBolt, matrixCalls, matrixBolt;
static qboolean matrixAvailable = qtrue;
static void *handModel;
static int AddBolt(void *g2, int model, const char *name) {
    CHECK(model == 0 && !strcmp(name, "*l_hand"));
    handModel = g2;
    return handBolt;
}
static qboolean GetBoltMatrix(void *g2, int model, int bolt, mdxaBone_t *matrix,
    const vec3_t angles, const vec3_t position, int time, qhandle_t *models, vec3_t scale) {
    (void)angles; (void)position; (void)time; (void)scale;
    CHECK(g2 == handModel && model == 0 && models == cgs.gameModels);
    matrixCalls++;
    matrixBolt = bolt;
    matrix->matrix[0][3] = 11;
    matrix->matrix[1][3] = 22;
    matrix->matrix[2][3] = 33;
    return matrixAvailable;
}
void BG_GiveMeVectorFromMatrix(mdxaBone_t *matrix, int flags, vec3_t result) {
    CHECK(flags == ORIGIN);
    result[0] = matrix->matrix[0][3];
    result[1] = matrix->matrix[1][3];
    result[2] = matrix->matrix[2][3];
}
static struct {
    sfxHandle_t (*S_RegisterSound)(const char *);
    int (*FX_RegisterEffect)(const char *);
    int (*FS_Open)(const char *, fileHandle_t *, fsMode_t);
    void (*FS_Close)(fileHandle_t);
    void (*G2API_GetGLAName)(void *, int, char *);
    int (*G2API_AddBolt)(void *, int, const char *);
    qboolean (*G2API_GetBoltMatrix)(void *, int, int, mdxaBone_t *, const vec3_t,
        const vec3_t, int, qhandle_t *, vec3_t);
} imports = { RegisterSound, RegisterEffect, OpenFile, CloseFile, GetGLAName,
    AddBolt, GetBoltMatrix }, *trap = &imports;

static sfxHandle_t CG_CustomSound(int entityNum, const char *name) {
    if (testNpcSounds && entityNum == 70) {
        int i;
        char stripped[MAX_QPATH];
        COM_StripExtension(name, stripped, sizeof(stripped));
        for (i = 0; npcNames[i]; i++) {
            if (!strcmp(stripped, npcNames[i])) return testNpc.sounds[i];
        }
        return 0;
    }
    customEntity = entityNum;
    customCalls++;
    Q_strncpyz(customName, name, sizeof(customName));
    return entityNum + 1000;
}
static void CG_InterpolatePlayerState(qboolean grabAngles) {
    CHECK(!grabAngles);
    cg.predictedPlayerState = cg.snap->ps;
}
static void CG_TransitionPlayerState(playerState_t *ps, playerState_t *old) {
    transitions++;
    if (ps->damageEvent != old->damageEvent && ps->damageCount) damageFeedback++;
    if (ps->stats[STAT_HEALTH] < old->stats[STAT_HEALTH] - 3) painFeedback++;
}

static stringID_table_t animTable[] = {
    ENUM2STRING(BOTH_KYLE_PA_2), ENUM2STRING(BOTH_KYLE_PA_3),
    ENUM2STRING(BOTH_PLAYER_PA_2), ENUM2STRING(BOTH_PLAYER_PA_3),
    ENUM2STRING(BOTH_PULLED_INAIR_B), ENUM2STRING(BOTH_PULLED_INAIR_F),
    ENUM2STRING(BOTH_SABERKILLER1), ENUM2STRING(BOTH_SABERKILLEE1),
    ENUM2STRING(BOTH_ALORA_SPIN_THROW), ENUM2STRING(BOTH_FORCE_DRAIN_GRAB_START),
    ENUM2STRING(BOTH_FORCE_DRAIN_GRAB_HOLD), ENUM2STRING(BOTH_FORCE_DRAIN_GRABBED),
    ENUM2STRING(BOTH_COWER1_START), ENUM2STRING(BOTH_SONICPAIN_START),
    ENUM2STRING(BOTH_KISSEE), ENUM2STRING(BOTH_KISSER),
    ENUM2STRING(BOTH_JUMP_BACKFLIP_ATCKEE), ENUM2STRING(BOTH_NEW_STABER),
    ENUM2STRING(BOTH_NEW_STABEE), ENUM2STRING(BOTH_JUMP1),
    ENUM2STRING(BOTH_KYLE_PA_1), ENUM2STRING(BOTH_PLAYER_PA_1), {NULL, -1}
};
#include "actual.h"

static animevent_t events[MAX_ANIM_EVENTS];
static animation_t animations[MAX_TOTALANIMATIONS];

static void Parse(const char *text) {
    int i = 0;
    memset(events, 0, sizeof(events));
    ParseAnimationEvtBlock("test", events, animations, &i, &text);
}

static void TestMeleeCarryCamera(void) {
    int mode, frame;
    for (mode = 0; mode < 2; ++mode) {
        float previous = 0;
        dampPos_t position = {0};
        memset(&cam, 0, sizeof(cam));
        memset(&cg.predictedPlayerState, 0, sizeof(cg.predictedPlayerState));
        cg.predictedPlayerState.stats[STAT_HEALTH] = 100;
        cg.predictedPlayerState.forceHandExtend = HANDEXTEND_PRETHROWN;
        cg.predictedPlayerState.commandTime = 1000;
        cg.time = 1000;
        cg.predictedTimeFrac = 0;
        cg_cameraFPS.integer = mode ? 125 : 0;
        cg_cameraFPS.value = (float)cg_cameraFPS.integer;
        CG_ThirdPersonCameraDeltaTime();
        CHECK(cam.meleeCarry && cam.lastTime == 0); // One reset on entry.
        CG_SaveThirdPersonCameraTime();
        // NPC-controlled positions continue moving even when the victim's
        // command time is unchanged, or corrections make it step backward.
        for (frame = 1; frame <= 12; ++frame) {
            float elapsed, cameraPosition;
            cg.time += 8;
            if (frame == 6) cg.predictedPlayerState.commandTime -= 100;
            if (frame == 9) cg.predictedPlayerState.commandTime += 200;
            cg.predictedTimeFrac = frame & 1 ? 0 : 25;
            elapsed = CG_ThirdPersonCameraDeltaTime();
            CHECK(elapsed == 8 && cam.lastTime != 0);
            position.ideal[0] = (float)frame;
            CG_DampPosition(&position, 0.7f, elapsed);
            cameraPosition = position.ideal[0] + position.damp[0];
            CHECK(cameraPosition > previous && cameraPosition < position.ideal[0]);
            previous = cameraPosition;
            CG_SaveThirdPersonCameraTime();
        }
        // The NPC holder link can wrap to zero; PRETHROWN still owns the camera.
        CHECK(!cg.predictedPlayerState.heldByClient && cam.meleeCarry);
        cg.predictedPlayerState.forceHandExtend = HANDEXTEND_POSTTHROWN;
        CG_ThirdPersonCameraDeltaTime();
        CHECK(!cam.meleeCarry && cam.lastTime == 0); // Release changes clocks once.
        cg.predictedTimeFrac = 4;
        CG_SaveThirdPersonCameraTime();
        cg.predictedPlayerState.commandTime += 8;
        cg.predictedTimeFrac = 6;
        CHECK(CG_ThirdPersonCameraDeltaTime() == 10); // Normal command prediction resumes.
    }
    memset(&cg.predictedPlayerState, 0, sizeof(cg.predictedPlayerState));
    cg.predictedTimeFrac = 0;
}

int main(void) {
    int i, calls;
    TestMeleeCarryCamera();
    cg_entities[0].interpolate = qtrue;
    cg_smoothClients.integer = 1;
    CG_AddPredictedPlayerEntity();
    CHECK(cg_entities[0].interpolate && cg_smoothClients.integer == 1);
    cg_entities[0].interpolate = qfalse;
    cg_smoothClients.integer = 0;
    CG_AddPredictedPlayerEntity();
    CHECK(!cg_entities[0].interpolate && !cg_smoothClients.integer);
    // Missing custom NPC events use the skeleton on every load, even when
    // an earlier precache may have cached an empty model-specific event set.
    CHECK(CG_NPCEventIndexForModel(&testNpc, "models/players/custom/", 0) == 7);
    CHECK(!strcmp(eventDirectory, "models/players/_humanoid/"));
    CHECK(eventParses == 1 && fileCloses == 0);
    CHECK(CG_NPCEventIndexForModel(&testNpc, "models/players/custom/", 0) == 7);
    CHECK(!strcmp(eventDirectory, "models/players/_humanoid/"));
    CHECK(eventParses == 2 && fileCloses == 0);
    // Resolve non-humanoid skeletons too, retaining their own animation layout.
    glaName = "models/players/custom_droid/model.gla";
    CHECK(CG_NPCEventIndexForModel(&testNpc, "models/players/custom/", 2) == 7);
    CHECK(!strcmp(eventDirectory, "models/players/custom_droid/"));
    glaName = "models/players/_humanoid/_humanoid.gla";
    // A model's explicit events (including deliberate silence) take precedence.
    modelEventLength = 20;
    CHECK(CG_NPCEventIndexForModel(&testNpc, "models/players/custom/", 0) == 7);
    CHECK(!strcmp(eventDirectory, "models/players/custom/") && fileCloses == 1);
    modelEventLength = 0;
    CHECK(CG_NPCEventIndexForModel(&testNpc, "models/players/custom/", 0) == -1);
    CHECK(!strcmp(eventDirectory, "models/players/custom/") && fileCloses == 2);

    // NPC holders use entity numbers rather than player slots. Resolve their
    // own hand bolt, including bolt zero, and reject stale or invalid holders.
    {
        centity_t victim = {0};
        vec3_t position = {0};
        victim.currentState.heldByClient = 71;
        cg_entities[70].currentValid = qtrue;
        cg_entities[70].currentState.eType = ET_NPC;
        cg_entities[70].ghoul2 = &testNpc;
        handBolt = 0;
        CHECK(CG_GetHeldHandPosition(&victim, position));
        CHECK(handModel == &testNpc && matrixBolt == 0);
        CHECK(position[0] == 11 && position[1] == 22 && position[2] == 33);
        handBolt = 5;
        CHECK(CG_GetHeldHandPosition(&victim, position) && matrixBolt == 5);
        calls = matrixCalls;
        handBolt = -1;
        CHECK(!CG_GetHeldHandPosition(&victim, position) && matrixCalls == calls);
        handBolt = 0;
        matrixAvailable = qfalse;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        matrixAvailable = qtrue;
        cg_entities[70].currentValid = qfalse;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        cg_entities[70].currentValid = qtrue;
        cg_entities[70].ghoul2 = NULL;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        cg_entities[70].ghoul2 = &testNpc;
        cg_entities[70].currentState.eType = ET_GENERAL;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        victim.currentState.heldByClient = -1;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        victim.currentState.heldByClient = ENTITYNUM_WORLD + 1;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        victim.currentState.heldByClient = 1;
        victim.currentState.number = 3;
        cg_entities[0].currentValid = qtrue;
        cg_entities[0].currentState.eType = ET_PLAYER;
        cg_entities[0].currentState.legsAnim = cg_entities[0].currentState.torsoAnim = BOTH_A3_TL_BR;
        cg_entities[0].ghoul2 = &testNpc;
        CHECK(CG_GetHeldHandPosition(&victim, position));

        // Real snapshots truncate entity+1 to six bits. An unrelated player
        // at the aliased index must not become the hand attachment target.
        victim.currentState.heldByClient = 71 & 63;
        cg_entities[6] = cg_entities[0];
        cg_entities[6].currentState.legsAnim = cg_entities[6].currentState.torsoAnim = BOTH_STAND1;
        cg_entities[70].currentState.eType = ET_NPC;
        cg_entities[70].currentState.legsAnim = cg_entities[70].currentState.torsoAnim = BOTH_A3_TL_BR;
        CHECK(CG_GetHeldHandPosition(&victim, position));
        cg_entities[6].currentState.legsAnim = cg_entities[6].currentState.torsoAnim = BOTH_A3_TL_BR;
        CHECK(!CG_GetHeldHandPosition(&victim, position)); // Ambiguous.
        cg_entities[6].currentValid = qfalse;
        cg_entities[70].currentState.pos.trBase[0] = 200;
        CHECK(!CG_GetHeldHandPosition(&victim, position)); // Too far away.
        cg_entities[127] = cg_entities[0];
        cg_entities[127].currentState.eType = ET_NPC;
        victim.currentState.heldByClient = 128 & 63;
        CHECK(!CG_GetHeldHandPosition(&victim, position));
        victim.currentState.legsAnim = BOTH_KNEES1;
        CHECK(CG_GetHeldHandPosition(&victim, position)); // Link wrapped to zero.
    }

    // A render stall crosses the impact frame with both samples >3 frames away.
    bgAllAnims[0].anims = animations;
    animations[BOTH_KYLE_PA_2].firstFrame = 100;
    animations[BOTH_KYLE_PA_2].numFrames = 100;
    animations[BOTH_KYLE_PA_2].frameLerp = 50;
    cg_entities[0].currentState.torsoAnim = BOTH_KYLE_PA_2;
    cg_entities[0].nextState.torsoAnim = BOTH_KYLE_PA_2;
    bgAllEvents[0].torsoAnimEvents[0].eventType = AEV_SOUND;
    bgAllEvents[0].torsoAnimEvents[0].keyFrame = 150;
    CG_PlayerAnimEvents(0, 0, qtrue, 140, 160, 0);
    CHECK(animationEvents == 1);
    CG_PlayerAnimEvents(0, 0, qtrue, 160, 170, 0);
    CHECK(animationEvents == 1); // Do not repeat an already crossed event.
    CG_PlayerAnimEvents(0, 0, qtrue, 90, 160, 0);
    CHECK(animationEvents == 1); // Reject a stale frame from another animation.
    cg_entities[0].nextState.torsoAnim = BOTH_KYLE_PA_3;
    CG_PlayerAnimEvents(0, 0, qtrue, 140, 160, 0);
    CHECK(animationEvents == 2); // Future snapshot must not swallow the current impact.
    CG_PlayerAnimEvents(0, 0, qtrue, 90, 160, 0);
    CHECK(animationEvents == 2); // Still reject frames from a different animation.
    cg_entities[0].currentState.torsoAnim = BOTH_JUMP1;
    cg_entities[0].nextState.torsoAnim = BOTH_JUMP1;
    animations[BOTH_JUMP1] = animations[BOTH_KYLE_PA_2];
    CG_PlayerAnimEvents(0, 0, qtrue, 140, 160, 0);
    CHECK(animationEvents == 2); // Preserve unrelated animations' behavior.
    for (i = 0; i < MAX_TOTALANIMATIONS; i++) {
        animations[i].firstFrame = i;
        animations[i].numFrames = 200;
    }

    // Every additional JA+ grapple retains actor-specific voices and crossed
    // frame events, without becoming a movement/prediction grapple here.
    for (i = 4; animTable[i].id != BOTH_JUMP1; i++) {
        int anim = animTable[i].id;
        int before = animationEvents;
        CHECK(BG_InGrappleMove(anim) == 0);
        CHECK(BG_IsGrappleSoundAnim(anim));
        Parse(va("{ %s AEV_SOUNDCHAN 50 CHAN_VOICE *gasp.wav 0 0 0 }", animTable[i].name));
        CHECK(CG_AnimEventSound(3, &events[0]) == 1003);
        CHECK(customEntity == 3 && !strcmp(customName, "*gasp.wav"));
        CHECK(CG_AnimEventSound(70, &events[0]) == 1070);
        CHECK(customEntity == 70 && !strcmp(customName, "*gasp.wav"));
        bgAllEvents[0].torsoAnimEvents[0] = events[0];
        cg_entities[0].currentState.torsoAnim = anim;
        cg_entities[0].nextState.torsoAnim = anim;
        CG_PlayerAnimEvents(0, 0, qtrue, anim + 40, anim + 60, 0);
        CHECK(animationEvents == before + 1);
    }
    CHECK(registrations == 0);

    // NPC soundsets often provide only variant 1. The real loader must map
    // extensionless variants 2/3 to it, which animation playback then uses.
    testNpcSounds = 1;
    CG_RegisterCustomSounds(&testNpc, 4, "kothos");
    CHECK(testNpc.sounds[0] == 801 && testNpc.sounds[1] == 801 && testNpc.sounds[2] == 801);
    CHECK(testNpc.sounds[3] == 0);
    CHECK(registrations == 6); // No numbered fallback for unnumbered gasp.
    Parse("{ BOTH_KYLE_PA_2 AEV_SOUNDCHAN 44 CHAN_AUTO *combat%d.mp3 1 3 0 }");
    randomChoice = 1;
    CHECK(CG_AnimEventSound(70, &events[0]) == 801);
    testNpcSounds = 0;
    registrations = 0;

    // All three directional pairs resolve each actor's voices, including the
    // punch combo, and retain events on both torso and legs across frame skips.
    {
        const int pairs[] = { BOTH_KYLE_PA_1, BOTH_PLAYER_PA_1,
            BOTH_KYLE_PA_2, BOTH_PLAYER_PA_2, BOTH_KYLE_PA_3, BOTH_PLAYER_PA_3 };
        const char *names[] = { "BOTH_KYLE_PA_1", "BOTH_PLAYER_PA_1",
            "BOTH_KYLE_PA_2", "BOTH_PLAYER_PA_2", "BOTH_KYLE_PA_3", "BOTH_PLAYER_PA_3" };
        for (i = 0; i < 6; i++) {
            int anim = pairs[i];
            int before = animationEvents;
            Parse(va("{ %s AEV_SOUNDCHAN 50 CHAN_AUTO *jump1.mp3 0 0 0 }", names[i]));
            CHECK(CG_AnimEventSound(70, &events[0]) == 1070);
            CHECK(customEntity == 70);
            CHECK(CG_AnimEventSound(3, &events[0]) == 1003);
            CHECK(customEntity == 3);
            bgAllEvents[0].torsoAnimEvents[0] = events[0];
            bgAllEvents[0].legsAnimEvents[0] = events[0];
            cg_entities[0].currentState.torsoAnim = cg_entities[0].nextState.torsoAnim = anim;
            cg_entities[0].currentState.legsAnim = cg_entities[0].nextState.legsAnim = anim;
            CG_PlayerAnimEvents(0, 0, qtrue, anim + 40, anim + 60, 0);
            CG_PlayerAnimEvents(0, 0, qfalse, anim + 40, anim + 60, 0);
            CHECK(animationEvents == before + 2);
            CG_PlayerAnimEvents(0, 0, qtrue, anim + 60, anim + 70, 0);
            CHECK(animationEvents == before + 2);

            // The shipped victim clips put fallsplat on the legs near the
            // end. A future get-up snapshot must not silence that impact.
            bgAllEvents[0].legsAnimEvents[0].keyFrame = anim + 95;
            cg_entities[0].nextState.legsAnim = BOTH_GETUP1;
            CG_PlayerAnimEvents(0, 0, qfalse, anim + 90, anim + 100, 0);
            CHECK(animationEvents == before + 3);
            CG_PlayerAnimEvents(0, 0, qfalse, anim + 100, anim + 101, 0);
            CHECK(animationEvents == before + 3);
        }
    }

    // One shared animation must use the voice of each actor at playback.
    Parse("{ BOTH_PLAYER_PA_2 AEV_SOUNDCHAN 35 CHAN_VOICE *gasp.mp3 0 0 0 }");
    CHECK(registrations == 0);
    CHECK(events[0].eventData[AED_SOUNDCHANNEL] == CHAN_VOICE);
    CHECK(CG_AnimEventSound(3, &events[0]) == 1003);
    CHECK(customEntity == 3 && !strcmp(customName, "*gasp.mp3"));
    CHECK(CG_AnimEventSound(70, &events[0]) == 1070);
    CHECK(customEntity == 70 && !strcmp(customName, "*gasp.mp3"));

    // Random custom variants retain their lower bound, including a single variant.
    Parse("{ BOTH_PLAYER_PA_3 AEV_SOUND 35 *choke%d.mp3 2 3 75 }");
    CHECK(events[0].eventData[AED_SOUND_PROBABILITY] == 75);
    randomChoice = 0;
    CG_AnimEventSound(3, &events[0]);
    CHECK(!strcmp(customName, "*choke2.mp3"));
    randomChoice = 1;
    CG_AnimEventSound(3, &events[0]);
    CHECK(!strcmp(customName, "*choke3.mp3"));
    Parse("{ BOTH_KYLE_PA_3 AEV_SOUND 29 *taunt%d.mp3 2 2 0 }");
    CG_AnimEventSound(70, &events[0]);
    CHECK(!strcmp(customName, "*taunt2.mp3"));

    // Ordinary impacts still use registered sounds, not custom voice lookup.
    calls = customCalls;
    Parse("{ BOTH_KYLE_PA_2 AEV_SOUND 32 sound/weapons/melee/punch%d.mp3 1 4 0 }");
    CHECK(registrations == 4);
    CHECK(CG_AnimEventSound(70, &events[0]) == 104);
    CHECK(customCalls == calls);

    // An overriding impact must clear a previous custom name on the same frame.
    Parse("{ BOTH_PLAYER_PA_2 AEV_SOUND 47 *pain50.mp3 0 0 0\n"
          "BOTH_PLAYER_PA_2 AEV_SOUND 47 sound/player/fallsplat.wav 0 0 0 }");
    CHECK(CG_AnimEventSound(3, &events[0]) == 105);
    CHECK(!events[0].stringData && customCalls == calls);

    // Unrelated animations keep their existing gameplay-driven voice behavior.
    Parse("{ BOTH_JUMP1 AEV_SOUND 1 *jump1.mp3 0 0 0 }");
    CHECK(CG_AnimEventSound(3, &events[0]) == 0);
    Parse("{ BOTH_JUMP1 AEV_SOUND 1 *jump%d.mp3 1 3 0 }");
    CHECK(CG_AnimEventSound(3, &events[0]) == 0);
    CHECK(customCalls == calls);

    // A held local player must get each damage transition once, even while
    // movement prediction is bypassed and many render frames share a snapshot.
    cg.snap = &serverSnapshot;
    cg.predictedPlayerState.stats[STAT_HEALTH] = 100;
    serverSnapshot.ps = cg.predictedPlayerState;
    serverSnapshot.ps.legsAnim = BOTH_PLAYER_PA_2;
    serverSnapshot.ps.legsTimer = 3000;
    serverSnapshot.ps.stats[STAT_HEALTH] = 80;
    serverSnapshot.ps.damageEvent = 1;
    serverSnapshot.ps.damageCount = 20;
    PredictHold();
    CHECK(transitions == 1 && damageFeedback == 1 && painFeedback == 1);
    PredictHold();
    PredictHold();
    CHECK(damageFeedback == 1 && painFeedback == 1);
    serverSnapshot.ps.stats[STAT_HEALTH] = 50;
    serverSnapshot.ps.damageEvent++;
    PredictHold();
    CHECK(damageFeedback == 2 && painFeedback == 2);
    calls = transitions;
    serverSnapshot.ps.legsAnim = BOTH_PLAYER_PA_3_FLY;
    PredictHold();
    CHECK(transitions == calls); // Flight resumes normal prediction.

    // Beta's hold path clears prediction error and avoids duplicate damage
    // transitions when ordinary movement prediction is disabled.
    serverSnapshot.ps.legsAnim = BOTH_PLAYER_PA_2;
    cg_noPredict.integer = 1;
    cg.predictedError[0] = 10;
    cg.predictedErrorTime = 100;
    cg.predictedTimeFrac = 0.5f;
    PredictHold();
    CHECK(transitions == calls);
    CHECK(cg.predictedError[0] == 0 && cg.predictedErrorTime == 0 && cg.predictedTimeFrac == 0);
    cg_noPredict.integer = 0;

    // The binary's A + melee victim is KNEES1 with timers refreshed to 1.
    // Bypass command prediction before the hand link exists, then while held
    // by an NPC. The release must restore ballistic prediction immediately.
    memset(&serverSnapshot.ps, 0, sizeof(serverSnapshot.ps));
    serverSnapshot.ps.stats[STAT_HEALTH] = 50;
    serverSnapshot.ps.legsAnim = serverSnapshot.ps.torsoAnim = BOTH_KNEES1;
    serverSnapshot.ps.legsTimer = serverSnapshot.ps.torsoTimer = 1;
    serverSnapshot.ps.forceHandExtend = HANDEXTEND_PRETHROWN;
    serverSnapshot.ps.origin[0] = 40;
    serverSnapshot.ps.viewangles[YAW] = 90;
    cg.predictedError[0] = 20;
    cg.predictedErrorTime = 100;
    calls = transitions;
    PredictHold();
    CHECK(transitions == calls + 1);
    CHECK(cg.predictedPlayerState.origin[0] == 40 && cg.predictedPlayerState.viewangles[YAW] == 90);
    CHECK(cg.predictedError[0] == 0 && cg.predictedErrorTime == 0);
    serverSnapshot.ps.heldByClient = 128 & 63;
    CHECK(CG_InMeleeGrappleVictimState(&serverSnapshot.ps));
    serverSnapshot.ps.heldByClient = 71 & 63;
    serverSnapshot.ps.forceHandExtend = HANDEXTEND_NONE;
    CHECK(CG_InMeleeGrappleVictimState(&serverSnapshot.ps));
    serverSnapshot.ps.forceHandExtend = HANDEXTEND_POSTTHROWN;
    serverSnapshot.ps.heldByClient = 0;
    serverSnapshot.ps.velocity[0] = 400;
    CHECK(!CG_InMeleeGrappleVictimState(&serverSnapshot.ps));
    calls = transitions;
    PredictHold();
    CHECK(transitions == calls);
    serverSnapshot.ps.forceHandExtend = HANDEXTEND_NONE;
    serverSnapshot.ps.heldByClient = ENTITYNUM_WORLD + 1;
    CHECK(!CG_InMeleeGrappleVictimState(&serverSnapshot.ps));
    serverSnapshot.ps.heldByClient = 71;
    serverSnapshot.ps.stats[STAT_HEALTH] = 0;
    CHECK(!CG_InMeleeGrappleVictimState(&serverSnapshot.ps));
    serverSnapshot.ps.heldByClient = 0;
    serverSnapshot.ps.stats[STAT_HEALTH] = 50;

    // Restore only live flight channels. An expired channel and a dead
    // player's animation must not be overwritten with the paired throw.
    serverSnapshot.ps.legsAnim = BOTH_PLAYER_PA_3_FLY;
    serverSnapshot.ps.legsTimer = 500;
    serverSnapshot.ps.legsFlip = qtrue;
    serverSnapshot.ps.torsoAnim = BOTH_PLAYER_PA_2;
    cg.predictedPlayerState.legsAnim = BOTH_JUMP1;
    cg.predictedPlayerState.torsoAnim = BOTH_JUMP1;
    CG_PreserveMeleeKataFlightAnimation(&cg.predictedPlayerState, &serverSnapshot.ps);
    CHECK(cg.predictedPlayerState.legsAnim == BOTH_PLAYER_PA_3_FLY && cg.predictedPlayerState.legsFlip);
    CHECK(cg.predictedPlayerState.torsoAnim == BOTH_JUMP1);
    serverSnapshot.ps.legsTimer = 0;
    cg.predictedPlayerState.legsAnim = BOTH_JUMP1;
    CG_PreserveMeleeKataFlightAnimation(&cg.predictedPlayerState, &serverSnapshot.ps);
    CHECK(cg.predictedPlayerState.legsAnim == BOTH_JUMP1);
    serverSnapshot.ps.legsTimer = 500;
    serverSnapshot.ps.stats[STAT_HEALTH] = 0;
    CHECK(!CG_InMeleeGrappleVictimState(&serverSnapshot.ps));
    CG_PreserveMeleeKataFlightAnimation(&cg.predictedPlayerState, &serverSnapshot.ps);
    CHECK(cg.predictedPlayerState.legsAnim == BOTH_JUMP1);

    puts("kata sound checks passed");
    return 0;
}
