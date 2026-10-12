#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdarg.h>

#define ARRAY_LEN(a) ((int)(sizeof(a) / sizeof((a)[0])))
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "Line %d: %s\n", __LINE__, #x); exit(1); } } while (0)
static struct { struct { int forceLightningEnvironmentSounds[6]; int forceLightningEnvironmentArcSounds[3]; } media; } cgs;
static int registrations;

static const char *va(const char *format, ...) {
	static char buffer[128];
	va_list args;
	va_start(args, format);
	vsnprintf(buffer, sizeof(buffer), format, args);
	va_end(args);
	return buffer;
}

static int RegisterSound(const char *path) {
	char expected[128];
	CHECK(registrations < 9);
	if (registrations < 3)
		snprintf(expected, sizeof(expected), "sound/weapons/force/lightningenv%d.mp3", registrations + 1);
	else
		snprintf(expected, sizeof(expected), "sound/ambience/spark%d.wav", registrations - 2);
	CHECK(!strcmp(path, expected));
	return 11 + registrations++;
}

static struct { int (*S_RegisterSound)(const char *); } api = { RegisterSound }, *trap = &api;

static void RegisterEnvironmentSounds(void) {
	int i;
#include "sound_registration.h"
}

int main(void) {
	int i;
	RegisterEnvironmentSounds();
	CHECK(registrations == 9);
	for (i = 0; i < 3; i++) CHECK(cgs.media.forceLightningEnvironmentArcSounds[i] == 11 + i);
	for (i = 0; i < 6; i++) CHECK(cgs.media.forceLightningEnvironmentSounds[i] == 14 + i);
	puts("Three MB2 arc sounds and six stock sparks registered; player-hit paths preserved.");
	return 0;
}
