#include <cstdio>
#include <cstdlib>
#include <string>
#include <thread>

#define CHECK(x) do { if (!(x)) { std::fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); std::exit(1); } } while (0)
using qboolean = bool;
constexpr bool qfalse = false, qtrue = true;
enum { CA_LOADING, CA_ACTIVE };
struct { bool downloadMenuActive, downloadRestart, downloadFinished; int checksumFeed; } clc;
struct { int state; bool cgameStarted; } cls;
static const auto windowThread = std::this_thread::get_id();
static std::string calls;
static bool changeState;
static int failStage;
static void Record(char call) {
    CHECK(std::this_thread::get_id() == windowThread);
    calls += call;
}
void FS_Restart(int feed) { CHECK(feed == 42); Record('F'); }
void CL_AddReliableCommand(const char *cmd, bool disconnect) {
    CHECK(std::string(cmd) == "donedl" && !disconnect); Record('D');
}
void Com_EventLoop() { Record('E'); if (changeState) cls.state = CA_ACTIVE; }
void Cvar_Set(const char *name, const char *value) {
    CHECK(std::string(name) == "r_uiFullScreen" && std::string(value) == "0"); Record('U');
}
void CL_FlushMemory() { Record('R'); if (failStage == 1) throw 17; }
void CL_InitCGame() { CHECK(cls.cgameStarted); Record('G'); if (failStage == 2) throw 18; }
void CL_SendPureChecksums() { Record('C'); }
void CL_WritePacket() { Record('P'); }
#include "actual.h"
static void Reset() {
    clc = {}; cls = {}; calls.clear(); changeState = false; failStage = 0;
    clc.downloadMenuActive = true; clc.checksumFeed = 42;
}
int main() {
    // Repeated map changes keep renderer reset and cgame work on the window thread.
    for (int i = 0; i < 5; ++i) {
        Reset(); CL_DownloadsComplete();
        CHECK(calls == "EURGCPPP" && cls.cgameStarted && !clc.downloadMenuActive);
    }
    Reset(); clc.downloadRestart = true; CL_DownloadsComplete();
    CHECK(calls == "FD" && !clc.downloadRestart && clc.downloadFinished && !cls.cgameStarted);
    Reset(); changeState = true; CL_DownloadsComplete();
    CHECK(calls == "E" && !cls.cgameStarted);
    for (int stage = 1; stage <= 2; ++stage) {
        Reset(); failStage = stage;
        try { CL_DownloadsComplete(); CHECK(false); }
        catch (int code) { CHECK(code == 16 + stage); }
        CHECK(calls == (stage == 1 ? "EUR" : "EURG"));
    }
    std::puts("Map load thread, restart, event reentry and error propagation checks passed");
}
