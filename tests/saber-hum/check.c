#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#ifdef _WIN32
#define Q_stricmp _stricmp
#else
#include <strings.h>
#define Q_stricmp strcasecmp
#endif

typedef int sfxHandle_t;
typedef enum { qfalse, qtrue } qboolean;
enum { MAX_CLIENTS = 32 };
typedef struct { int soundLoop; qboolean soundLoopCustom; } saberInfo_t;
#define ARRAY_LEN(a) (sizeof(a) / sizeof((a)[0]))
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); exit(1); } } while (0)
static struct { int integer; } cg_saberHum;
static struct { struct { sfxHandle_t saberHumSounds[5]; } media; } cgs;
static int registrations;

/* The parser supplies a token; the engine supplies a sound handle. */
static qboolean COM_ParseString(const char **p, const char **value) {
    *value = *p;
    return qfalse;
}
static int BG_SoundIndex(const char *value) {
    CHECK(value && value[0]);
    return ++registrations;
}
#include "actual.h"

static void checkHum(const char *path, qboolean custom) {
    saberInfo_t saber = { 0, qtrue };
    int selected, client;
    Saber_ParseSoundLoop(&saber, &path);
    CHECK(saber.soundLoopCustom == custom);
    CHECK(saber.soundLoop == registrations);

    /* Change the setting on the same loaded saber without parsing it again. */
    for (selected = 0; selected <= 6; ++selected) {
        cg_saberHum.integer = selected;
        for (client = 0; client < MAX_CLIENTS; ++client) {
            int expected = saber.soundLoop;
            if (!custom && selected < 6)
                expected = cgs.media.saberHumSounds[selected ? selected - 1 : client % 5];
            CHECK(CG_SaberHumSound(&saber, client) == expected);
        }
        CHECK(CG_SaberHumSound(&saber, -1) == saber.soundLoop);
        CHECK(CG_SaberHumSound(&saber, MAX_CLIENTS) == saber.soundLoop);
    }
    cg_saberHum.integer = -1;
    CHECK(CG_SaberHumSound(&saber, 0) == saber.soundLoop);
    cg_saberHum.integer = 7;
    CHECK(CG_SaberHumSound(&saber, 0) == saber.soundLoop);
}

int main(void) {
    int i;
    for (i = 0; i < 5; ++i)
        cgs.media.saberHumSounds[i] = 100 + i;

    checkHum("sound/weapons/saber/saberhum1.wav", qfalse);
    checkHum("sound/weapons/saber/saberhum2.wav", qfalse);
    checkHum("sound/weapons/saber/saberhum3.wav", qfalse);
    /* dual_1 and single_8 explicitly declare this stock loop. */
    checkHum("sound/weapons/saber/saberhum4.wav", qfalse);
    checkHum("sound/weapons/saber/saberhum5.wav", qfalse);
    checkHum("SOUND/WEAPONS/SABER/SABERHUM4.WAV", qfalse);
    checkHum("sound/weapons/Lumaya/saberloop.wav", qtrue);
    checkHum("sound/null.wav", qtrue);
    checkHum("sound/custom/saberhum4.wav", qtrue);
    checkHum("sound/weapons/saber/saberhum4.wav.extra", qtrue);
    puts("Saber hum: stock selections update live; custom and silent loops are preserved.");
    return 0;
}
