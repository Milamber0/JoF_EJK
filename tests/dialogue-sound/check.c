#include "qcommon/q_shared.h"

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)
#define CON_CONNECTED 2
#define CGAME_EVENT_NONE 0
#define CGAME_EVENT_DIALOGUE 1

typedef struct { struct { int connected; } pers; } gclient_t;
typedef struct gentity_s { gclient_t *client; qboolean inuse; const char *target; } gentity_t;
static gentity_t g_entities[MAX_ENTITIESTOTAL];
static struct { int time, clientNum; } cg;
static struct { int eventHandling; float cursorX, cursorY; } cgs;
static const char *fileText;
static char args[8][MAX_TOKEN_CHARS], scratch[MAX_TOKEN_CHARS];
static int argc, catcher, played, muted, registered, targetUses, sent;
static int soundEntity;
static int soundChannel, loadWarnings;
static qboolean missingSound;
static char soundPath[MAX_QPATH];
static char commands[64][MAX_STRING_CHARS];
static int recipients[64];

void QDECL Com_Printf(const char *fmt, ...) {
  if (strstr(fmt, "Dialogue: could not load sound")) ++loadWarnings;
}
void QDECL Com_Error(int level, const char *fmt, ...) {
  va_list ap;
  (void)level;
  va_start(ap, fmt);
  vfprintf(stderr, fmt, ap);
  va_end(ap);
  exit(1);
}
static int FS_Open(const char *path, fileHandle_t *file, fsMode_t mode) {
  (void)path;
  CHECK(mode == FS_READ);
  *file = 1;
  return (int)strlen(fileText);
}
static void FS_Read(void *buffer, int length, fileHandle_t file) {
  CHECK(file == 1);
  memcpy(buffer, fileText, length);
}
static void FS_Close(fileHandle_t file) { CHECK(file == 1); }
static int Argc(void) { return argc; }
static void Argv(int i, char *out, int size) { Q_strncpyz(out, i < argc ? args[i] : "", size); }
static const char *CG_Argv(int i) {
  Argv(i, scratch, sizeof(scratch));
  return scratch; // Match the production shared scratch buffer.
}
static void SetArgs(const char *command) {
  argc = 0;
  while (argc < ARRAY_LEN(args)) {
    const char *token = COM_ParseExt(&command, qtrue);
    if (!token[0]) break;
    Q_strncpyz(args[argc++], token, sizeof(args[0]));
  }
}
static void SendServerCommand(int client, const char *command) {
  CHECK(sent < ARRAY_LEN(commands));
  recipients[sent] = client;
  Q_strncpyz(commands[sent++], command, sizeof(commands[0]));
}
static void SendClientCommand(const char *command) { CHECK(strstr(command, "dialogueresponse ") == command); }
static void CG_EventHandling(int mode) { cgs.eventHandling = mode; }
static int Key_GetCatcher(void) { return catcher; }
static void Key_SetCatcher(int value) { catcher = value; }
static sfxHandle_t RegisterSound(const char *path) {
  ++registered;
  Q_strncpyz(soundPath, path, sizeof(soundPath));
  return missingSound ? 0 : 42;
}
static void StartSound(const vec3_t origin, int entity, int channel, sfxHandle_t sound) {
  CHECK(!origin && entity == ENTITYNUM_NONE && channel == CHAN_VOICE_GLOBAL && sound == 42);
  soundEntity = entity;
  soundChannel = channel;
  ++played;
}
static void MuteSound(int entity, int channel) {
  CHECK(entity == soundEntity && channel == soundChannel);
  ++muted;
}
static void G_UseTargets(gentity_t *source, gentity_t *player) { (void)source; (void)player; ++targetUses; }
static void G_UseTargets2(gentity_t *source, gentity_t *player, const char *name) {
  (void)source; (void)player; CHECK(!strcmp(name, "quest_started")); ++targetUses;
}
static struct {
  void (*Print)(const char *, ...);
  int (*FS_Open)(const char *, fileHandle_t *, fsMode_t);
  void (*FS_Read)(void *, int, fileHandle_t);
  void (*FS_Close)(fileHandle_t);
  int (*Argc)(void);
  void (*Argv)(int, char *, int);
  void (*SendServerCommand)(int, const char *);
  void (*SendClientCommand)(const char *);
  int (*Key_GetCatcher)(void);
  void (*Key_SetCatcher)(int);
  sfxHandle_t (*S_RegisterSound)(const char *);
  void (*S_StartSound)(const vec3_t, int, int, sfxHandle_t);
  void (*S_MuteSound)(int, int);
} imports = { Com_Printf, FS_Open, FS_Read, FS_Close, Argc, Argv, SendServerCommand,
  SendClientCommand, Key_GetCatcher, Key_SetCatcher, RegisterSound, StartSound, MuteSound }, *trap = &imports;

#include "../../codemp/game/g_dialogue.h"
#include "../../codemp/cgame/cg_dialogue.h"
#include "server.h"
#include "client.h"

// Exercise the engine's actual attenuation function, rather than assuming
// that a channel named LOCAL is independent of the player's position.
#define SOUND_FULLVOLUME 256
#define SOUND_ATTENUATE 0.0008f
#define VOICE_ATTENUATE 0.004f
static const float SOUND_FMAXVOL = 0.75f;
static vec3_t listener_origin, listener_axis[3];
static struct { int channels; } dma = {2};
static cvar_t separation = {0};
static cvar_t *s_separation = &separation;
#include "spatialize.h"

static void Receive(const char *command) { SetArgs(command); CG_DialogueServerCommand(); }
static void Deliver(int client) {
  int i;
  for (i = 0; i < sent; ++i) {
    CHECK(recipients[i] == client); // Dialogue audio must never be broadcast.
    Receive(commands[i]);
  }
  sent = 0;
}
static void Respond(gentity_t *player, int choice) {
  SetArgs(va("dialogueresponse %u %d", s_dialogue.serial, choice));
  Cmd_DialogueResponse_f(player);
  Deliver((int)(player - g_entities));
}
static qboolean LoadText(const char *text) {
  G_DialogueInit();
  fileText = text;
  return DLG_Load("test") != NULL;
}

int main(void) {
  gclient_t client = {{CON_CONNECTED}};
  gentity_t *player = &g_entities[3];
  char longPath[MAX_QPATH + 1], malformed[256];
  unsigned int firstSerial;
  int left, right;
  vec3_t distantSource = {10000, 0, 0};
  separation.value = 0.5f;
  S_SpatializeOrigin(distantSource, 255, &left, &right, CHAN_LOCAL);
  CHECK(left == 0 && right == 0); // Reproduce the old channel's attenuation.
  S_SpatializeOrigin(distantSource, 255, &left, &right, CG_DLG_SOUND_CHANNEL);
  CHECK(left > 0 && right > 0); // Voice-over survives a different view entity.
  player->client = &client;
  player->inuse = qtrue;
  cg.clientNum = 3;
  catcher = KEYCATCH_CONSOLE;

  CHECK(LoadText("dialogue { node a { text Hello Sound sound/voice/hello.mp3 next b "
    "setquest test 1 fire quest_started } node b { text Silent next c } "
    "node c { text Goodbye sound sound/voice/goodbye.wav end } }"));
  CHECK(G_DialogueStart(player, "test", NULL));
  CHECK(G_DialogueGetQuestStage(player, "test") == 1 && targetUses == 1);
  Deliver(3);
  CHECK(CG_DialogueIsActive() && played == 1 && registered == 1 && muted == 0);
  CHECK(!strcmp(soundPath, "sound/voice/hello.mp3"));
  firstSerial = s_dialogue.serial;
  Receive(va("jof_dialogue show %u sound/voice/duplicate.wav", firstSerial));
  Receive(va("jof_dialogue show %u sound/voice/stale.wav", firstSerial + 100));
  Receive(va("jof_dialogue stop %u", firstSerial + 100));
  CHECK(played == 1 && registered == 1 && muted == 0 && CG_DialogueIsActive());

  Respond(player, 0); // A silent node also stops the previous voice-over.
  CHECK(CG_DialogueIsActive() && muted == 1 && played == 1);
  Receive(va("jof_dialogue show %u sound/voice/stale.wav", firstSerial));
  CHECK(played == 1);
  Respond(player, 0);
  CHECK(played == 2 && !strcmp(soundPath, "sound/voice/goodbye.wav"));
  Respond(player, 0);
  CHECK(!CG_DialogueIsActive() && muted == 2 && catcher == KEYCATCH_CONSOLE);

  CHECK(G_DialogueStart(player, "test", NULL));
  Deliver(3);
  CHECK(G_DialogueStart(player, "test", NULL)); // Replacing a session stops its sound.
  Deliver(3);
  CHECK(played == 4 && muted == 3);
  CG_DialogueCancel();
  CHECK(!CG_DialogueIsActive() && muted == 4 && catcher == KEYCATCH_CONSOLE);
  CG_DialogueReset();
  CHECK(muted == 4); // Closing/resetting twice must not mute unrelated audio.

  missingSound = qtrue;
  CHECK(G_DialogueStart(player, "test", NULL));
  Deliver(3);
  CHECK(CG_DialogueIsActive() && played == 4);
  CHECK(loadWarnings == 1);
  CG_DialogueCancel();
  CHECK(muted == 4);
  missingSound = qfalse;

  CHECK(G_DialogueStart(player, "test", NULL));
  Deliver(3);
  CHECK(played == 5);
  cg.clientNum = 4;
  CG_DialogueReset(); // Mute the entity used at playback, even across a reset.
  CHECK(muted == 5);
  cg.clientNum = 3;

  CHECK(LoadText("dialogue { node a { text Legacy end } }"));
  CHECK(G_DialogueStart(player, "test", NULL));
  Deliver(3);
  CHECK(CG_DialogueIsActive() && played == 5);
  Respond(player, -1);
  CHECK(!CG_DialogueIsActive() && muted == 5);

  CHECK(!LoadText("dialogue { node a { text Hello sound } }"));
  CHECK(!LoadText("dialogue { node a { text Hello sound \"\" end } }"));
  memset(longPath, 'a', MAX_QPATH);
  longPath[MAX_QPATH] = '\0';
  Com_sprintf(malformed, sizeof(malformed), "dialogue { node a { text Hello sound %s end } }", longPath);
  CHECK(!LoadText(malformed));
  longPath[MAX_QPATH - 1] = '\0';
  Com_sprintf(malformed, sizeof(malformed), "dialogue { node a { text Hello sound %s end } }", longPath);
  CHECK(LoadText(malformed));
  G_DialogueShutdown();
  puts("Dialogue sound checks passed.");
  return 0;
}
