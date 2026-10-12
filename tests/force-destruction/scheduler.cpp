#include <cstdio>
#include <cstdlib>
#include "qcommon/q_shared.h"
#include "ghoul2/G2.h"

#define CHECK(x) do { if (!(x)) { std::fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); std::exit(1); } } while (0)
#define FX_MAX_EFFECTS 4
struct CGhoul2Info_v {};
struct CPrimitiveTemplate {};
struct SEffectTemplate { bool mInUse; };
static cvar_t freeze;
static cvar_t *fx_freeze = &freeze;
class CFxScheduler {
public:
    SEffectTemplate mEffectTemplates[FX_MAX_EFFECTS] = {};
    bool scheduled = false;
    int decodedEntity = -1, decodedModel = -1, decodedBolt = -1;
    int loopCalls = 0;
    void ReportPlayEffectError(int) { CHECK(false); }
    void ScheduleLoopedEffect(int, int, CGhoul2Info_v *, bool, int, bool) { ++loopCalls; }
    void PlayEffect(int, vec3_t, matrix3_t, int, CGhoul2Info_v *, int, int, int, bool, int, bool);
};
// Compile the actual scheduler validation and attachment decoding, including
// its decision to defer playback before any access to the caller's null axis.
#include "scheduler.h"

int main() {
    CFxScheduler scheduler;
    CGhoul2Info_v model;
    vec3_t origin = {0, 0, 0};
    scheduler.mEffectTemplates[1].mInUse = true;
    scheduler.PlayEffect(1, origin, nullptr, 0, &model, -1, -1, -1, false, 0, true);
    CHECK(scheduler.scheduled);
    CHECK(scheduler.decodedEntity == 0 && scheduler.decodedModel == 0 && scheduler.decodedBolt == 0);
    CHECK(scheduler.loopCalls == 0);
    int packed = (5 << ENTITY_SHIFT) | (2 << MODEL_SHIFT) | 1;
    scheduler.PlayEffect(1, origin, nullptr, packed, &model, -1, -1, -1, false, 1, true);
    CHECK(scheduler.scheduled && scheduler.loopCalls == 1);
    CHECK(scheduler.decodedEntity == 5 && scheduler.decodedModel == 2 && scheduler.decodedBolt == 1);
    matrix3_t axis = {{1,0,0}, {0,1,0}, {0,0,1}};
    scheduler.PlayEffect(1, origin, axis, -1, nullptr, -1, -1, -1, false, 0, false);
    CHECK(!scheduler.scheduled && scheduler.decodedBolt == -1 && scheduler.decodedEntity == -1);
    std::puts("Zero attachment ID schedules the local right-hand effect without using a null axis; other bolts and unattached effects checked.");
}
