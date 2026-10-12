#include <cstdio>
#include <cstdlib>
#include <cstring>
#include "qcommon/q_shared.h"
#include "game/bg_public.h"
#define CHECK(x) do { if (!(x)) { std::fprintf(stderr, "FAIL %d: %s\n", __LINE__, #x); std::exit(1); } } while (0)

struct { struct Snapshot { playerState_t ps; } *snap; } cg;
static struct { qboolean forceDestruction; } cgs;
static decltype(cg)::Snapshot snapshot = {};
qboolean CG_HasStasis(void);
qboolean CG_HasRepulse(void);
qboolean CG_HasDash(void);
qboolean CG_HasDestruction(void);
#include "wheel.h"

struct { struct { qboolean valid; playerState_t ps; } snap; } cl;
static cvar_t stasis, repulse, dash, destruction;
static int commands, catcher;
static cvar_t *Cvar_Get(const char *name, const char *, int)
{
	if (!std::strcmp(name, "cl_stasisSelected")) return &stasis;
	if (!std::strcmp(name, "cl_repulseSelected")) return &repulse;
	if (!std::strcmp(name, "cl_dashSelected")) return &dash;
	CHECK(!std::strcmp(name, "cl_destructionSelected"));
	return &destruction;
}
static int Key_GetCatcher(void) { return catcher; }
static void Cbuf_AddText(const char *text)
{
	CHECK(!std::strcmp(text, "force_destruction\n"));
	++commands;
}
#include "input.h"

static void Press(bool down)
{
	usercmd_t cmd = {};
	cmd.buttons = down ? BUTTON_FORCEPOWER : 0;
	RouteForceButton(&cmd);
	if (destruction.integer) CHECK(!(cmd.buttons & BUTTON_FORCEPOWER));
}

static void CheckPartialWheelOrder()
{
	// Every pseudo-slot and its real anchor may be unavailable independently.
	for (int grants = 0; grants < 128; ++grants)
	{
		struct { int slots[FORCE_WHEEL_CAPACITY]; int guard; } wheel = {{0}, 12345};
		int expected[FORCE_WHEEL_CAPACITY], count = 0;
		snapshot.ps.fd.forcePowersKnown = (1 << FP_HEAL) | (1 << FP_PUSH) | (1 << FP_DRAIN);
		expected[count++] = FP_HEAL;
		if (grants & 16)
		{
			snapshot.ps.fd.forcePowersKnown |= 1 << DASH_KNOWN_BIT;
			expected[count++] = DASH_WHEEL_SLOT;
		}
		if (grants & 32)
		{
			snapshot.ps.fd.forcePowersKnown |= 1 << FP_SPEED;
			expected[count++] = FP_SPEED;
		}
		expected[count++] = FP_PUSH;
		if (grants & 1)
		{
			snapshot.ps.fd.forcePowersKnown |= 1 << FP_SEE;
			expected[count++] = FP_SEE;
		}
		if (grants & 2)
		{
			snapshot.ps.fd.forcePowersKnown |= 1 << STASIS_KNOWN_BIT;
			expected[count++] = STASIS_WHEEL_SLOT;
		}
		if (grants & 4)
		{
			snapshot.ps.fd.forcePowersKnown |= 1 << REPULSE_KNOWN_BIT;
			expected[count++] = REPULSE_WHEEL_SLOT;
		}
		if (grants & 64)
		{
			snapshot.ps.fd.forcePowersKnown |= DESTRUCTION_KNOWN_FLAG;
			expected[count++] = DESTRUCTION_WHEEL_SLOT;
		}
		if (grants & 8)
		{
			snapshot.ps.fd.forcePowersKnown |= 1 << FP_LIGHTNING;
			expected[count++] = FP_LIGHTNING;
		}
		expected[count++] = FP_DRAIN;
		CHECK(CG_BuildForceWheel(wheel.slots) == count && wheel.guard == 12345);
		for (int i = 0; i < count; ++i) CHECK(wheel.slots[i] == expected[i]);
	}
}

static void CheckMovementEligibility()
{
	const int modes[] = { PM_NORMAL, PM_JETPACK, PM_FLOAT, PM_NOCLIP, PM_SPECTATOR, PM_DEAD };
	cl.snap.valid = qtrue;
	cl.snap.ps.stats[STAT_HEALTH] = 100;
	cl.snap.ps.fd.forcePowersKnown = DESTRUCTION_KNOWN_FLAG;
	destruction.integer = 1;
	for (int mode : modes)
	{
		const int before = commands;
		const bool allowed = mode != PM_SPECTATOR && mode != PM_DEAD;
		snapshot.ps.pm_type = cl.snap.ps.pm_type = mode;
		CHECK(CG_HasDestruction() == allowed);
		Press(false); Press(true); Press(true);
		CHECK(commands == before + (allowed ? 1 : 0));
	}
	Press(false);
	snapshot.ps.pm_type = PM_NORMAL;
	std::memset(&cl, 0, sizeof(cl));
	destruction.integer = 0;
	commands = 0;
}

int main()
{
	struct { int slots[FORCE_WHEEL_CAPACITY]; int guard; } wheel = {{0}, 12345};
	int count, i, found;
	cg.snap = &snapshot;
	cgs.forceDestruction = qtrue;
	snapshot.ps.stats[STAT_HEALTH] = 100;
	snapshot.ps.fd.forcePowersKnown = DESTRUCTION_KNOWN_FLAG;
	CHECK(CG_BuildForceWheel(wheel.slots) == 1 && wheel.slots[0] == DESTRUCTION_WHEEL_SLOT);
	cgs.forceDestruction = qfalse;
	CHECK(CG_BuildForceWheel(wheel.slots) == 0); // unrelated servers may use the same spare bit
	cgs.forceDestruction = qtrue;
	CheckMovementEligibility();
	CheckPartialWheelOrder();
	snapshot.ps.fd.forcePowersKnown = (1 << FORCE_WHEEL_CAPACITY) - 1;
	count = CG_BuildForceWheel(wheel.slots);
	CHECK(count == FORCE_WHEEL_CAPACITY - 4 && wheel.guard == 12345);
	found = 0;
	for (i = 0; i < count; ++i)
	{
		if (wheel.slots[i] == DESTRUCTION_WHEEL_SLOT)
		{
			++found;
			CHECK(i > 0 && wheel.slots[i-1] == REPULSE_WHEEL_SLOT);
			CHECK(i + 1 < count && wheel.slots[i+1] == FP_LIGHTNING);
		}
		CHECK(wheel.slots[i] >= 0 && wheel.slots[i] < FORCE_WHEEL_CAPACITY);
	}
	CHECK(found == 1);
	snapshot.ps.fd.forcePowersKnown &= ~DESTRUCTION_KNOWN_FLAG;
	CHECK(!CG_HasDestruction() && CG_BuildForceWheel(wheel.slots) == count - 1);
	snapshot.ps.fd.forcePowersKnown |= DESTRUCTION_KNOWN_FLAG;
	snapshot.ps.pm_flags = PMF_FOLLOW;
	CHECK(!CG_HasDestruction());
	snapshot.ps.pm_flags = 0;
	snapshot.ps.stats[STAT_HEALTH] = 0;
	CHECK(!CG_HasDestruction());
	CHECK(!ForcePower_Valid(-1) && !ForcePower_Valid(32));
	cg.snap = NULL;
	CHECK(CG_BuildForceWheel(wheel.slots) == 0);

	cl.snap.valid = qtrue;
	cl.snap.ps.stats[STAT_HEALTH] = 100;
	cl.snap.ps.fd.forcePowersKnown = DESTRUCTION_KNOWN_FLAG;
	destruction.integer = 1;
	Press(false); Press(true); Press(true);
	CHECK(commands == 1);
	Press(false); Press(true);
	CHECK(commands == 2);
	cl.snap.ps.fd.forcePowersKnown = 0;
	Press(false); Press(true);
	CHECK(commands == 2);
	cl.snap.ps.fd.forcePowersKnown = DESTRUCTION_KNOWN_FLAG;
	catcher = 1;
	Press(false); Press(true);
	CHECK(commands == 2);
	catcher = 0;
	destruction.integer = 0;
	Press(false); Press(true);
	destruction.integer = 1;
	Press(true); CHECK(commands == 2); // no shot merely from selecting while held
	Press(false); Press(true); CHECK(commands == 3);
	cl.snap.valid = qfalse;
	Press(false); Press(true); CHECK(commands == 3);
	cl.snap.valid = qtrue;
	cl.snap.ps.pm_flags = PMF_FOLLOW;
	Press(false); Press(true); CHECK(commands == 3);
	cl.snap.ps.pm_flags = 0;
	cl.snap.ps.stats[STAT_HEALTH] = 0;
	Press(false); Press(true); CHECK(commands == 3);
	puts("Destruction wheel capacity/order/grant and one-press input checks passed.");
}
