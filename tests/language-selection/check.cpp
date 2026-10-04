#include "ui/ui_local.h"
#include "botlib/botlib.h"
#include "botlib/l_script.h"
#include "botlib/l_precomp.h"
#include <vector>
#include <string>
#include <cstdio>
#include <cstdlib>
#include <cstdarg>

botlib_import_t botimport = {};
uiInfo_t uiInfo = {};
uiImport_t imports = {};
uiImport_t *trap = &imports;
displayContextDef_t context = {};
displayContextDef_t *DC = &context;
static std::vector<std::string> languages;
static std::string selected;
static std::string repository;
static FILE *files[64] = {};
static int checks;
void QDECL Log_Write(char *, ...) {}

static void require(bool condition, const char *message) {
    if (!condition) { fprintf(stderr, "FAIL: %s\n", message); exit(1); }
    ++checks;
}
void QDECL Com_Printf(const char *format, ...) {
    va_list args; va_start(args, format); vfprintf(stderr, format, args); va_end(args);
}
void NORETURN QDECL Com_Error(int, const char *format, ...) {
    va_list args; va_start(args, format); vfprintf(stderr, format, args); va_end(args); exit(1);
}
static void QDECL print(int, char *format, ...) {
    va_list args; va_start(args, format); vfprintf(stderr, format, args); va_end(args);
}
static void *allocate(int size) { return malloc(size); }
static int openFile(const char *name, fileHandle_t *handle, fsMode_t) {
    std::string path = !strncmp(name, "ui/", 3) ? repository + "/assets/japro/" + name : name;
    for (int i = 1; i < 64; ++i) if (!files[i]) {
        files[i] = fopen(path.c_str(), "rb");
        if (!files[i]) { *handle = 0; return -1; }
        fseek(files[i], 0, SEEK_END); int length = (int)ftell(files[i]); rewind(files[i]);
        *handle = i; return length;
    }
    exit(1);
}
static int readFile(void *buffer, int length, fileHandle_t handle) {
    return (int)fread(buffer, 1, length, files[handle]);
}
static void closeFile(fileHandle_t handle) { fclose(files[handle]); files[handle] = nullptr; }
static void languageName(int index, char *out) { Q_strncpyz(out, languages.at(index).c_str(), 128); }
static void getCvar(const char *, char *out, int size) { Q_strncpyz(out, selected.c_str(), size); }
static void setCvar(const char *, const char *value) { selected = value; }
static qboolean feederSelection(float, int, itemDef_t *) { return qtrue; }
const char *String_Alloc(const char *value) {
    char *copy = (char *)malloc(strlen(value) + 1);
    strcpy(copy, value);
    return copy;
}
static void Item_ValidateTypeData(itemDef_t *item) { require(item->typeData.multi != nullptr, "multi data exists"); }
static void PC_SourceError(int, char *message, ...) { fprintf(stderr, "%s\n", message); exit(1); }

#define UI_BUILD 1
#include "actual.h"

static void checkCycle(itemDef_t *item, const char *start, int key) {
    selected = start;
    item->window.rect = {0, 0, 100, 20};
    item->window.flags = WINDOW_HASFOCUS;
    context.cursorx = 10;
    context.cursory = 10;
    bool reachedEnglish = false;
    for (int i = 0; i < item->typeData.multi->count; ++i) {
        require(Item_Multi_HandleKey(item, key) == qtrue, "language click handled");
        if (!Q_stricmp(selected.c_str(), "english")) reachedEnglish = true;
    }
    require(reachedEnglish, "cycling languages reaches English");
    require(!Q_stricmp(selected.c_str(), start), "full language cycle returns to starting choice");
}

static void checkOptions(itemDef_t *item, const char *const *values, const char *const *labels, int count) {
    require(item->typeData.multi->count == count, "option count");
    for (int i = 0; i < count; ++i) {
        selected = values[i];
        require(Item_Multi_FindCvarByValue(item) == i, "option index matches cvar");
        require(!strcmp(Item_Multi_Setting(item), labels[i]), "option label matches language");
        checkCycle(item, values[i], A_MOUSE1);
        checkCycle(item, values[i], A_MOUSE2);
    }
    selected = "ENGLISH";
    require(!strcmp(Item_Multi_Setting(item), "English"), "English selection is case insensitive");
}
static void parseFeeder(itemDef_t *item, const std::string &fixture) {
    int handle = PC_LoadSourceHandle(fixture.c_str());
    require(handle != 0, "open feeder fixture");
    require(ItemParse_cvarStrList(item, handle) == qtrue, "parse text language feeder");
    PC_FreeSourceHandle(handle);
}
int main(int argc, char **argv) {
    require(argc == 2, "repository argument");
    repository = argv[1];
    botimport.Print = print; botimport.GetMemory = allocate; botimport.FreeMemory = free;
    botimport.HunkAlloc = allocate; botimport.FS_FOpenFile = openFile;
    botimport.FS_Read = readFile; botimport.FS_FCloseFile = closeFile;
    imports.PC_ReadToken = PC_ReadTokenHandle; imports.SE_GetLanguageName = languageName;
    context.getCVarString = getCvar;
    context.setCVar = setCvar;
    context.feederSelection = feederSelection;
    FILE *fixture = fopen("feeder.fixture", "wb"); require(fixture != nullptr, "create feeder fixture");
    fputs("feeder", fixture); fclose(fixture);
    const char *textValues[] = {"english", "French", "German", "Spanish", "Custom"};
    const char *textLabels[] = {"English", "Francais", "Deutsch", "Espanol", "Custom"};
    multiDef_t textMulti = {}; itemDef_t textItem = {};
    textItem.cvar = "se_language"; textItem.special = FEEDER_LANGUAGES; textItem.typeData.multi = &textMulti;
    // Match the case-variant language directories observed in the live in-game UI.
    languages = {"Russian", "french", "german", "spanish", "French", "German", "Spanish"};
    uiInfo.languageCount = (int)languages.size(); parseFeeder(&textItem, "feeder.fixture");
    checkCycle(&textItem, "german", A_MOUSE1);
    const char *duplicateValues[] = {"english", "Russian", "french", "german", "spanish"};
    const char *duplicateLabels[] = {"English", "Russian", "Francais", "Deutsch", "Espanol"};
    checkOptions(&textItem, duplicateValues, duplicateLabels, 5);
    languages = {"French", "ENGLISH", "German", "Spanish", "Custom"};
    uiInfo.languageCount = (int)languages.size(); parseFeeder(&textItem, "feeder.fixture");
    checkOptions(&textItem, textValues, textLabels, 5);
    languages = {"French", "German", "Spanish", "Custom"};
    uiInfo.languageCount = (int)languages.size(); parseFeeder(&textItem, "feeder.fixture");
    checkOptions(&textItem, textValues, textLabels, 5);
    languages.clear(); uiInfo.languageCount = 0; parseFeeder(&textItem, "feeder.fixture");
    checkOptions(&textItem, textValues, textLabels, 1);
    for (int i = 0; i < MAX_MULTI_CVARS + 40; ++i) languages.push_back("Language" + std::to_string(i));
    uiInfo.languageCount = (int)languages.size(); parseFeeder(&textItem, "feeder.fixture");
    require(textMulti.count == MAX_MULTI_CVARS, "large language list stays within capacity");
    const char *voiceValues[] = {"english", "francais", "deutsch", "espanol"};
    const char *voiceLabels[] = {"English", "Francais", "Deutsch", "Espanol"};
    for (const char *menu : {"setup.menu", "ingame_setup.menu"}) {
        std::string path = std::string("ui/jamp/") + menu;
        int handle = PC_LoadSourceHandle(path.c_str()); require(handle != 0, "open actual setup menu");
        pc_token_t token; bool found = false;
        while (PC_ReadTokenHandle(handle, &token)) {
            if (strcmp(token.string, "cvar")) continue;
            require(PC_ReadTokenHandle(handle, &token) != 0, "read cvar name");
            if (strcmp(token.string, "s_language")) continue;
            require(PC_ReadTokenHandle(handle, &token) && !strcmp(token.string, "cvarStrList"), "voice list keyword");
            multiDef_t voiceMulti = {}; itemDef_t voiceItem = {};
            voiceItem.cvar = "s_language"; voiceItem.typeData.multi = &voiceMulti;
            require(ItemParse_cvarStrList(&voiceItem, handle) == qtrue, "parse actual voice options");
            checkOptions(&voiceItem, voiceValues, voiceLabels, 4); found = true; break;
        }
        require(found, "voice selector exists"); PC_FreeSourceHandle(handle);
    }
    printf("PASS: %d checks, including clicks back to English with duplicate language directories and actual Voice menus\n", checks);
    return 0;
}
