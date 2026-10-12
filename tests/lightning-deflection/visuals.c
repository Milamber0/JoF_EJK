#include "cgame/cg_local.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(c) do { if (!(c)) { fprintf(stderr, "line %d: %s\n", line, #c); exit(1); } } while (0)
cg_t cg;
cgs_t cgs;
static cgameImport_t imports;
cgameImport_t *trap = &imports;
static centity_t guard;
static FILE *reference;
static int line, randomCount, irandCount, traces, hitMask;

// Compare production callbacks and cache state with recorded x86 execution.
static void Expect(const char *kind, const double *values, int count) {
  char buffer[512], name[32], *cursor;
  int i;
  CHECK(fgets(buffer, sizeof(buffer), reference));
  ++line;
  CHECK(sscanf(buffer, "%31s", name) == 1 && !strcmp(kind, name));
  cursor = buffer + strlen(name);
  for (i = 0; i < count; ++i) {
    char *end;
    double expected = strtod(cursor, &end);
    CHECK(end != cursor);
    // Large cached vectors are treated as angles by MBII. Float trig on
    // Windows and the x87 reference may differ by sub-millimetre trace ends.
    if (fabs(values[i] - expected) > 0.001 + fabs(expected) * 0.000002) {
      fprintf(stderr, "reference line %d %s value %d: %.9g != %.9g\n", line, kind, i, values[i], expected);
      exit(1);
    }
    cursor = end;
  }
}

static int FixtureRand(void) {
  static const int values[] = {0,8192,16384,24576,32767,1234,30000,10000,22222};
  return values[randomCount++ % ARRAY_LEN(values)];
}
static int FixtureIRand(int low, int high) {
  int result = low + (irandCount++ * 137 + 73) % (high - low + 1);
  double values[] = {low, high, result};
  Expect("IRAND", values, ARRAY_LEN(values));
  return result;
}
static void FixtureAngles(const vec3_t angles, vec3_t forward, vec3_t right, vec3_t up) {
  double values[] = {angles[0], angles[1], angles[2]};
  Expect("ANGLES", values, ARRAY_LEN(values));
  AngleVectors(angles, forward, right, up);
}
void CG_Trace(trace_t *result, const vec3_t start, const vec3_t mins, const vec3_t maxs,
  const vec3_t end, int skipNumber, int mask) {
  double values[] = {start[0],start[1],start[2],end[0],end[1],end[2]};
  CHECK(!mins && !maxs && skipNumber == -1 && mask == MASK_SOLID);
  Expect("TRACE", values, ARRAY_LEN(values));
  memset(result, 0, sizeof(*result));
  result->fraction = (hitMask & (1 << traces++)) ? 0.25f : 1.0f;
}
static void Effect(int id, vec3_t origin, vec3_t direction, int volume, int radius, qboolean portal) {
  double values[] = {id,origin[0],origin[1],origin[2],direction[0],direction[1],direction[2]};
  CHECK(volume == -1 && radius == -1 && !portal);
  Expect("FX", values, ARRAY_LEN(values));
}

#define rand FixtureRand
#define Q_irand FixtureIRand
#define AngleVectors FixtureAngles
#include "visuals.h"

int main(int argc, char **argv) {
  char buffer[512];
  CHECK(argc == 2);
  reference = fopen(argv[1], "r");
  CHECK(reference);
  imports.FX_PlayEffectID = Effect;
  cgs.effects.forceLightningDeflectArc = 42;
  cgs.effects.forceLightningDeflectFlare = 43;
  while (fgets(buffer, sizeof(buffer), reference)) {
    int reset, saber, blade, i;
    vec3_t base, end, direction;
    lightningSaberShock_t *shock;
    ++line;
    CHECK(sscanf(buffer, "STEP %d %d %d %d %d %d %f %f %f %f %f %f %f %f %f",
      &reset,&saber,&blade,&cg.time,&cg.frametime,&hitMask,
      &base[0],&base[1],&base[2],&end[0],&end[1],&end[2],
      &direction[0],&direction[1],&direction[2]) == 15);
    if (reset) {
      memset(&guard, 0, sizeof(guard));
      guard.currentState.weapon = WP_SABER;
      randomCount = irandCount = 0;
    }
    traces = 0;
    FX_ForceLightningSaberContact(&guard, saber, blade, base, end, direction);
    shock = &guard.lightningSaberShock[saber][blade];
    for (i = 0; i < 5; i++) {
      double values[] = {i,shock->endpointTime[i],shock->endpoint[i][0],shock->endpoint[i][1],shock->endpoint[i][2]};
      Expect("STATE", values, ARRAY_LEN(values));
    }
    {
      double values[] = {randomCount,irandCount};
      Expect("RANDOM", values, ARRAY_LEN(values));
    }
  }
  fclose(reference);
  printf("Matched %d reference lines from the MBII x86 binary.\n", line);
  return 0;
}
