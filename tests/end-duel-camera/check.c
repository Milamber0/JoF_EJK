#include "cgame/cg_local.h"

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); exit(1); } } while (0)
#define DEBUGNAME(x) ((void)0)

cg_t cg;
cgs_t cgs;
vmCvar_t cp_pluginDisable, cg_duelSounds, cg_duelMusic;
int cg_dueltypes[MAX_CLIENTS];
static snapshot_t snapshot;
static cgameImport_t imports;
cgameImport_t *trap = &imports;
static int musicRestarts;
static const char *pluginArg;
static char consoleOutput[8192];
static int pendingPlugins;

static const struct { const char *name, *value; } cvarDefaults[] = {
#define XCVAR_DEF(name, defVal, update, flags) { #name, defVal },
#include "cgame/cg_xcvar.h"
};

static int DefaultPlugins(void) {
    size_t i;
    for (i = 0; i < ARRAY_LEN(cvarDefaults); ++i) {
        if (!strcmp(cvarDefaults[i].name, "cp_pluginDisable")) {
            return atoi(cvarDefaults[i].value);
        }
    }
    CHECK(0);
    return 0;
}

static void Print(const char *format, ...) {
    size_t length = strlen(consoleOutput);
    va_list args;
    va_start(args, format);
    vsnprintf(consoleOutput + length, sizeof(consoleOutput) - length, format, args);
    va_end(args);
}

char *QDECL va(const char *format, ...) {
    static char buffer[128];
    va_list args;
    va_start(args, format);
    vsnprintf(buffer, sizeof(buffer), format, args);
    va_end(args);
    return buffer;
}

static int CmdArgc(void) { return pluginArg ? 2 : 1; }
static void CmdArgv(int arg, char *buffer, int length) {
    CHECK(arg == 1 && pluginArg);
    snprintf(buffer, length, "%s", pluginArg);
}
static void CvarSet(const char *name, const char *value) {
    CHECK(!strcmp(name, "cp_pluginDisable"));
    pendingPlugins = atoi(value);
}
static void CvarUpdate(vmCvar_t *cvar) {
    CHECK(cvar == &cp_pluginDisable);
    cvar->integer = pendingPlugins;
}

static void ClearLoopingSounds(void) {}
void CG_StartMusic(qboolean force) { CHECK(force); ++musicRestarts; }
void CG_CenterPrint(const char *text, int y, int width) { (void)text; (void)y; (void)width; }
const char *CG_GetStringEdString(char *section, char *name) { (void)section; return name; }

#include "actual.h"

static entityState_t Setup(void) {
    entityState_t event = {0};
    memset(&cg, 0, sizeof(cg));
    memset(&cgs, 0, sizeof(cgs));
    memset(&snapshot, 0, sizeof(snapshot));
    cp_pluginDisable.integer = DefaultPlugins() | JAPRO_PLUGIN_ENDDUELROTATION;
    Com_Printf = Print;
    imports.Cmd_Argc = CmdArgc;
    imports.Cmd_Argv = CmdArgv;
    imports.Cvar_Set = CvarSet;
    imports.Cvar_Update = CvarUpdate;
    imports.S_ClearLoopingSounds = ClearLoopingSounds;
    musicRestarts = 0;
    cgs.serverMod = SVMOD_JAPLUS;
    cg.snap = &snapshot;
    cg.clientNum = snapshot.ps.clientNum = event.number = 3;
    cg.time = 10000;
    snapshot.ps.stats[STAT_HEALTH] = 100;
    snapshot.ps.persistant[PERS_TEAM] = TEAM_FREE;
    snapshot.ps.persistant[PERS_SPAWN_COUNT] = 7;
    cg.predictedPlayerState = snapshot.ps;
    return event;
}

static void Plugin(const char *arg) {
    pluginArg = arg;
    consoleOutput[0] = '\0';
    CG_PluginDisable_f();
}

static void CheckPluginToggle(void) {
    entityState_t event = Setup();
    int defaults = DefaultPlugins();

    // Use the registered default, then exercise the actual /plugin command.
    CHECK(!(defaults & JAPRO_PLUGIN_ENDDUELROTATION));
    cp_pluginDisable.integer = defaults;
    Plugin(NULL);
    CHECK(strstr(consoleOutput, " 2 [ ] End duel rotation\n"));
    DispatchDuelEvent(&event);
    CHECK(cg.endDuelCameraTime == 0);
    CHECK(!CG_EndDuelCameraActive());

    Plugin("2");
    CHECK(cp_pluginDisable.integer == (defaults | JAPRO_PLUGIN_ENDDUELROTATION));
    CHECK(!strcmp(consoleOutput, "End duel rotation ^2Enabled^7\n"));
    Plugin(NULL);
    CHECK(strstr(consoleOutput, " 2 [X] End duel rotation\n"));
    DispatchDuelEvent(&event);
    CHECK(CG_EndDuelCameraActive());

    Plugin("2");
    CHECK(cp_pluginDisable.integer == defaults);
    CHECK(!strcmp(consoleOutput, "End duel rotation ^1Disabled^7\n"));
    CHECK(!CG_EndDuelCameraActive());
    Plugin(NULL);
    CHECK(strstr(consoleOutput, " 2 [ ] End duel rotation\n"));
    DispatchDuelEvent(&event);
    CHECK(cg.endDuelCameraTime == 0);

    // Other JA+ options retain their legacy disable-bit behavior.
    CHECK(CG_PluginOptionEnabled(3));
    CHECK(CG_PluginOptionEnabled(9));
    Plugin("3");
    CHECK(!CG_PluginOptionEnabled(3));
    CHECK(CG_PluginOptionEnabled(9));
    Plugin("9");
    CHECK(!CG_PluginOptionEnabled(9));
}

int main(void) {
    entityState_t event;

    CheckPluginToggle();
    event = Setup();

    // JA+ ends the duel through EV_PRIVATE_DUEL 0, without a required obituary.
    DispatchDuelEvent(&event);
    CHECK(musicRestarts == 1);
    CHECK(cg.endDuelCameraTime == cg.time);
    CHECK(CG_EndDuelCameraActive());
    CHECK(CG_EndDuelCameraAngle() == 0.0f);
    CHECK(CG_EndDuelCameraEnvelope() == 0.0f);
    // Match SP's linear spin and independent pitch/range ramps, not smoothstep.
    CHECK(END_DUEL_CAMERA_DURATION == 1000);
    cg.time += 165;
    CHECK(fabsf(CG_EndDuelCameraAngle() - 59.4f) < 0.01f);
    CHECK(fabsf(CG_EndDuelCameraEnvelope() - 0.5f) < 0.001f);
    cg.time += 335;
    CHECK(fabsf(CG_EndDuelCameraAngle() - 180.0f) < 0.01f);
    CHECK(CG_EndDuelCameraEnvelope() == 1.0f);
    cg.time += 335;
    CHECK(fabsf(CG_EndDuelCameraAngle() - 300.6f) < 0.01f);
    CHECK(fabsf(CG_EndDuelCameraEnvelope() - 0.5f) < 0.001f);
    cg.time += 165;
    CHECK(!CG_EndDuelCameraActive());
    CHECK(CG_EndDuelCameraAngle() == 0.0f);
    CHECK(CG_EndDuelCameraEnvelope() == 0.0f);

    // The end event can precede prediction catching up with the duel state.
    event = Setup();
    snapshot.ps.duelInProgress = cg.predictedPlayerState.duelInProgress = qtrue;
    DispatchDuelEvent(&event);
    CHECK(CG_EndDuelCameraActive());

    event = Setup();
    cp_pluginDisable.integer &= ~JAPRO_PLUGIN_ENDDUELROTATION;
    DispatchDuelEvent(&event);
    CHECK(cg.endDuelCameraTime == 0);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    snapshot.ps.stats[STAT_HEALTH] = 0;
    DispatchDuelEvent(&event);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    event.number = 4;
    DispatchDuelEvent(&event);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    snapshot.ps.clientNum = event.number = 4;
    snapshot.ps.pm_flags = PMF_FOLLOW;
    DispatchDuelEvent(&event);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    snapshot.ps.persistant[PERS_TEAM] = TEAM_SPECTATOR;
    DispatchDuelEvent(&event);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    cgs.serverMod = SVMOD_JAPRO;
    DispatchDuelEvent(&event);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    event.eventParm = 2;
    DispatchDuelEvent(&event);
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    ++cg.predictedPlayerState.persistant[PERS_SPAWN_COUNT];
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    cg.predictedPlayerState.pm_flags |= PMF_FOLLOW;
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    cg.predictedPlayerState.persistant[PERS_TEAM] = TEAM_SPECTATOR;
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    cg.predictedPlayerState.clientNum = 4;
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    cg.predictedPlayerState.stats[STAT_HEALTH] = 0;
    CHECK(!CG_EndDuelCameraActive());

    event = Setup();
    DispatchDuelEvent(&event);
    cp_pluginDisable.integer &= ~JAPRO_PLUGIN_ENDDUELROTATION;
    CHECK(!CG_EndDuelCameraActive());

    puts("PASS: Plugin 2 defaults, command/checkmark/status, JA+ duel-end event, delayed prediction, camera duration, immediate disable, death, follow/spectator, respawn, new duel, other players/mods.");
    return 0;
}
