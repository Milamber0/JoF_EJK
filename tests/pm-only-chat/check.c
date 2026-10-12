#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>
#ifdef _WIN32
#define Q_stricmp _stricmp
#define Q_stricmpn _strnicmp
#else
#include <strings.h>
#define Q_stricmp strcasecmp
#define Q_stricmpn strncasecmp
#endif
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); exit(1); } } while (0)
#define MAX_STRING_CHARS 1024
#define MAX_NETNAME 36
#define MAX_SAY_TEXT 150
#define MAX_CHATBOX_ITEMS 24
#define MAX_CHATBOX_ITEM_EMOJIS 32
#define JAPRO_CHATLOG_ENABLE 1
#define CHAN_LOCAL_SOUND 1
#define KEYCATCH_CONSOLE 1
#define CHATBOX_FONT_HEIGHT 20
#define QINLINE
#define S_COLOR_WHITE "^7"
#define COLOR_GREEN '2'
#define COLOR_CYAN '5'
#define COLOR_MAGENTA '6'
#define Q_COLOR_ESCAPE '^'
#define EC "\x19"
#define Q_strncmp strncmp
#define Com_Memset memset
#define Com_sprintf snprintf
enum { qfalse, qtrue, ERR_DROP, ITEM_TEXTSTYLE_OUTLINED, FONT_SMALL, FONT_MEDIUM, FONT_SMALL2 };
typedef int qboolean;
typedef struct { int integer; float value; } cvar_t;
typedef struct { int emoji, xOffset, yOffset; } chatBoxEmoji_t;
typedef struct {
    char string[MAX_STRING_CHARS];
    int time, lines;
    qboolean isPrivate;
    chatBoxEmoji_t emoji[MAX_CHATBOX_ITEM_EMOJIS];
} chatBoxItem_t;
static struct {
    int time, chatItemActive, scoreBoardShowing, pressingScoreBoard;
    qboolean pmOnlyChat;
    char lastChatMsg[MAX_NETNAME + MAX_SAY_TEXT + 32];
    chatBoxItem_t chatItems[MAX_CHATBOX_ITEMS];
    struct { int file; } log;
} cg;
static struct {
    int numClients;
    float widthRatioCoef;
    struct { int talkSound, privateChatSound, teamChatSound; } media;
} cgs = {0, 1, {1, 2, 3}};
static cvar_t cg_teamChatsOnly, cg_cleanChatbox, cg_chatSounds = {2},
    cg_logChat = {1}, cg_chatBox = {10000}, cg_chatBoxLines = {MAX_CHATBOX_ITEMS},
    cg_chatBoxX, cg_chatBoxHeight, cg_chatBoxFontSize = {1, 1},
    cg_chatBoxShowHistory, cg_chatBoxEmojis, cg_newFont, cg_noFakeTells = {1};
static float colorWhite[4];
static const char *args[6];
static int argc, printed, logged, sounds, lastSound, catcher, drawn;
static char lastPrinted[MAX_STRING_CHARS], drawnText[MAX_CHATBOX_ITEMS][MAX_STRING_CHARS];
static void Q_strncpyz(char *out, const char *in, size_t size) { snprintf(out, size, "%s", in); }
static char *Q_stristr(const char *text, const char *find) {
    for (; *text; ++text) if (!Q_stricmpn(text, find, strlen(find))) return (char *)text;
    return NULL;
}
static void Q_CleanString(char *text) {
    char *out = text;
    for (; *text; ++text) {
        if (*text == '^' && text[1]) ++text;
        else *out++ = *text;
    }
    *out = 0;
}
static void Argv(int n, char *out, int size) { Q_strncpyz(out, n < argc ? args[n] : "", size); }
static int Argc(void) { return argc; }
static const char *CG_Argv(int n) { return n < argc ? args[n] : ""; }
static void Print(const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(lastPrinted, sizeof(lastPrinted), fmt, ap);
    va_end(ap);
    ++printed;
}
static void CG_LogPrintf(int file, const char *fmt, ...) { ++logged; }
static void Sound(int sound, int channel) { ++sounds; lastSound = sound; }
static void Localize(const char *key, char *out, int size) { Q_strncpyz(out, "Hangar", size); }
static void Error(int level, const char *text) { fprintf(stderr, "%s\n", text); exit(1); }
static int KeyCatcher(void) { return catcher; }
static void SetColor(const float *color) {}
static struct {
    void (*Cmd_Argv)(int, char *, int);
    int (*Cmd_Argc)(void);
    void (*Print)(const char *, ...);
    void (*S_StartLocalSound)(int, int);
    void (*SE_GetStringTextString)(const char *, char *, int);
    void (*Error)(int, const char *);
    int (*Key_GetCatcher)(void);
    void (*R_SetColor)(const float *);
} imports = {Argv, Argc, Print, Sound, Localize, Error, KeyCatcher, SetColor}, *trap = &imports;
static void CG_DrawPic(float x, float y, float w, float h, int shader) {}
static void CG_Text_Paint(float x, float y, float scale, const float *color,
    const char *text, float adjust, int limit, int style, int font) {
    CHECK(drawn < MAX_CHATBOX_ITEMS);
    Q_strncpyz(drawnText[drawn++], text, MAX_STRING_CHARS);
}
#include "actual.h"

static void Chat(const char *cmd, const char *name, const char *loc,
    const char *color, const char *body, int expected, int isPrivate) {
    int before = printed, beforeLog = logged, beforeSound = sounds, slot = cg.chatItemActive;
    args[0] = cmd; args[1] = name; args[2] = loc ? loc : "5";
    args[3] = color; args[4] = body; args[5] = "5";
    argc = loc ? 6 : 3;
    CG_Chat_f();
    CHECK(printed - before == expected);
    CHECK(logged - beforeLog == expected);
    if (expected) {
        CHECK(cg.chatItems[slot].isPrivate == isPrivate);
        CHECK(!strchr(lastPrinted, '\x19'));
    } else CHECK(sounds == beforeSound);
}
static void Mode(const char *value) {
    args[0] = "pmonly"; args[1] = value;
    argc = value ? 2 : 1;
    CG_PMOnly_f();
}
int main(void) {
    int before, beforeLog, slot;
    cg.time = 100;
    /* Retain the PM from existing history, hiding public text immediately. */
    Chat("chat", "Alice^7" EC ": ^2public", NULL, NULL, NULL, 1, qfalse);
    Chat("chat", EC "[Bob^7" EC "]" EC ": ^6incoming", NULL, NULL, NULL, 1, qtrue);
    Mode("on"); CHECK(cg.pmOnlyChat);
    CG_ChatBox_DrawStrings(); CHECK(drawn == 1 && strstr(drawnText[0], "incoming"));
    cg_chatBoxShowHistory.integer = 1; catcher = KEYCATCH_CONSOLE;
    cg.time = 20000; drawn = 0;
    CG_ChatBox_DrawStrings(); CHECK(drawn == 1 && strstr(drawnText[0], "incoming"));
    cg_chatBoxShowHistory.integer = 0; catcher = 0; cg.time = 100;

    /* Real server tells and sender echoes have the same escaped prefix. */
    Chat("chat", EC "[Bob^7" EC "]" EC ": ^6received", NULL, NULL, NULL, 1, qtrue);
    CHECK(lastSound == cgs.media.privateChatSound);
    Chat("chat", EC "[Me^7" EC "]" EC ": ^6sent", NULL, NULL, NULL, 1, qtrue);
    cg_teamChatsOnly.integer = 1;
    Chat("chat", EC "[^1Bob^7" EC "]" EC ": ^6colored name", NULL, NULL, NULL, 1, qtrue);
    Chat("lchat", EC "[Bob^7" EC "]" EC ": ", "@LOC_HANGAR", "6", "received nearby", 1, qtrue);
    CHECK(strstr(lastPrinted, "Hangar"));
    Chat("lchat", EC "[Me^7" EC "]" EC ": ", "Hangar", "6", "sent nearby", 1, qtrue);

    Chat("chat", "Me^7" EC ": ^2outgoing public", NULL, NULL, NULL, 0, qfalse);
    Chat("chat", "Alice^7" EC ": ^2public ^7]: ^6fake PM", NULL, NULL, NULL, 0, qfalse);
    Chat("chat", "[Bob^7]: ^6unmarked imitation", NULL, NULL, NULL, 0, qfalse);
    Chat("tchat", EC "(Alice^7" EC ")" EC ": ^5team", NULL, NULL, NULL, 0, qfalse);
    Chat("chat", EC "^1<Clan>^7(Alice^7" EC ")" EC ": ^1clan", NULL, NULL, NULL, 0, qfalse);
    Chat("chat", EC "^3<Admin>^7(Alice^7" EC ")" EC ": ^3admin", NULL, NULL, NULL, 0, qfalse);
    Chat("lchat", "Alice", "Hangar", "6", "fake location PM", 0, qfalse);
    Chat("ltchat", EC "(Alice^7" EC ")" EC ": ", "Hangar", "5", "location team", 0, qfalse);
    before = printed; beforeLog = logged; slot = cg.chatItemActive;
    CG_ChatBox_AddString("<Alice^7: voice command>", qfalse);
    CHECK(printed == before && logged == beforeLog && slot == cg.chatItemActive);

    /* Changing the mode does not corrupt the old history or command state. */
    Mode("invalid"); CHECK(cg.pmOnlyChat);
    Mode("off"); CHECK(!cg.pmOnlyChat);
    drawn = 0; CG_ChatBox_DrawStrings();
    CHECK(drawn == 7 && strstr(drawnText[0], "public"));
    cg_teamChatsOnly.integer = 0;
    Chat("chat", "Alice^7" EC ": ^2public again", NULL, NULL, NULL, 1, qfalse);
    Chat("tchat", EC "(Alice^7" EC ")" EC ": ^5team again", NULL, NULL, NULL, 1, qfalse);
    Mode(NULL); CHECK(cg.pmOnlyChat);
    Mode("0"); CHECK(!cg.pmOnlyChat);
    Mode("1"); CHECK(cg.pmOnlyChat);
    puts("PM-only: incoming/outgoing, location, channel filtering, spoofed text, history, voice text and mode commands passed");
    return 0;
}
