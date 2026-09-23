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
#define CHECK(x) do { if (!(x)) { fprintf(stderr,"FAIL %d: %s\n",__LINE__,#x); exit(1); } } while(0)
#define ARRAY_LEN(x) (sizeof(x)/sizeof((x)[0]))
#define MAX_CLIENTS 32
#define MAX_INFO_STRING 1024
#define CS_SERVERINFO 0
#define CHAN_AUTO 0
typedef int qboolean;
enum { qfalse, qtrue };
enum { IT_HEALTH=1, IT_ARMOR, IT_AMMO, IT_HOLDABLE, IT_WEAPON };
#include "../../codemp/game/bg_pickup.h"
typedef struct { int giType; const char *pickup_sound; } item_t;
typedef struct { struct { int pickupConfirmed; } pers; } gclient_t;
typedef struct { struct { int number, modelindex; } s; item_t *item; gclient_t *client; } gentity_t;
static item_t bg_itemlist[] = {{0,0},{IT_HEALTH,"health"},{IT_ARMOR,"armor"},{IT_AMMO,"ammo"},{IT_HOLDABLE,"bacta"},{IT_WEAPON,"weapon"}};
static int bg_numItems = ARRAY_LEN(bg_itemlist);
static struct { int integer; } g_pickupConfirm = {3}, cg_pickupConfirm = {1};
typedef struct { struct { int clientNum; } ps; } snapshot_t;
static snapshot_t snapshot;
static struct {
 int time, itemPickup, itemPickupTime, pickupQueue[16], pickupQueueHead, pickupQueueCount, clientNum;
	 snapshot_t *snap;
	 int pickupHandshakeActive, demoPlayback, pickupConfirmed;
} cg;
static const char *serverVersion = "3", *clientVersion = "1", *argument = "1", *extra = "";
static const char *readyVersion = "3";
static const char *clientReadyVersion = "3";
static int readyArgc = 2, readyCount;
static int Argc(void) { return readyArgc; }
static void Argv(int n, char *out, int size) { snprintf(out,size,"%s",readyVersion); }
static void SendClientCommand(const char *cmd) { CHECK(!strcmp(cmd,"jof_pickupReady 3")); ++readyCount; }
static void CvarSet(const char *name, const char *value) {
 CHECK(!strcmp(name,"cg_pickupReady") && !strcmp(value,"0"));
 clientReadyVersion="0";
}
static const char *jaPlusVersion = "2.5B0";
static char messages[32][64];
static int messageCount, soundCount, feedbackCount, feedback[32];
static const char *CG_ConfigString(int n) { return serverVersion; }
static const char *Info_ValueForKey(const char *info, const char *key) {
 if (!strcmp(key,"V")) return jaPlusVersion;
 if (!strcmp(key,"cg_pickupReady")) return clientReadyVersion;
 return info;
}
static const char *CG_Argv(int n) { return n == 1 ? argument : extra; }
static void GetUserinfo(int n, char *out, int size) { snprintf(out,size,"%s",clientVersion); }
static void SendServerCommand(int n,const char *text) { CHECK(n==0); if (!strcmp(text,"jof_pickupReady 3")) return; CHECK(messageCount<32); strcpy(messages[messageCount++],text); }
static int RegisterSound(const char *s) { CHECK(s && *s); return 1; }
static void StartSound(int s,int channel) { ++soundCount; }
static char *va(const char *fmt,...) { static char buf[64]; va_list args; va_start(args,fmt); vsnprintf(buf,sizeof(buf),fmt,args); va_end(args); return buf; }
static struct {
 void (*GetUserinfo)(int,char*,int);
 void (*SendServerCommand)(int,const char*);
	int (*S_RegisterSound)(const char*);
	void (*S_StartLocalSound)(int,int);
	int (*Argc)(void);
	void (*Argv)(int,char*,int);
	void (*SendClientCommand)(const char*);
	void (*Cvar_Set)(const char*,const char*);
} imports = {GetUserinfo,SendServerCommand,RegisterSound,StartSound,Argc,Argv,SendClientCommand,CvarSet}, *trap=&imports;
static void CG_QueuePickupNotification(int);
static void CG_ItemPickup(int index) { feedback[feedbackCount++]=index; CG_QueuePickupNotification(index); }
/* Run the actual engine flood filter, not just a mock transport. */
enum { CS_ACTIVE = 4 };
typedef struct { int state, lastReliableTime[4]; } client_t;
static struct { int integer; } clRunning = {0}, floodProtect = {1}, newFloodProtect = {0};
#define com_cl_running (&clRunning)
#define sv_floodProtect (&floodProtect)
#define sv_newfloodProtect (&newFloodProtect)
static struct { int time; } svs;
#include "actual.h"
int main(void) {
 gclient_t client={0}, legacyClient={0};
 gentity_t collector={{0,0},NULL,&client}, item={{50,2},&bg_itemlist[2],NULL};
 int i;
 cg.snap=&snapshot; cg.time=1000;
 /* A stale userinfo flag alone must never cause a custom server command. */
 G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==0);
 clientReadyVersion="0"; Cmd_PickupReady_f(&collector); CHECK(!client.pers.pickupConfirmed);
 clientReadyVersion="3"; clientVersion=""; Cmd_PickupReady_f(&collector); CHECK(!client.pers.pickupConfirmed);
 clientVersion="1";
 readyVersion="1"; Cmd_PickupReady_f(&collector); G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==0);
 readyVersion="2"; Cmd_PickupReady_f(&collector); CHECK(!client.pers.pickupConfirmed);
 readyVersion="3x"; Cmd_PickupReady_f(&collector); CHECK(!client.pers.pickupConfirmed);
 readyVersion="3"; readyArgc=3; Cmd_PickupReady_f(&collector); CHECK(!client.pers.pickupConfirmed);
 readyArgc=2; Cmd_PickupReady_f(&collector); CHECK(client.pers.pickupConfirmed);
 readyVersion="0"; Cmd_PickupReady_f(&collector); G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==0);
 readyVersion="3"; Cmd_PickupReady_f(&collector);
 collector.client=&legacyClient; G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==0);
 collector.client=&client;
 ShutdownPickup();
 CHECK(!strcmp(clientReadyVersion,"0"));
 G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==0);
 RevokeFromUserinfo(&client, clientVersion);
 CHECK(!client.pers.pickupConfirmed);
 clientReadyVersion="3"; RevokeFromUserinfo(&client, clientVersion);
 G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==0);
 Cmd_PickupReady_f(&collector);
 CG_UpdatePickupHandshake(); CHECK(readyCount==1);
 cg.time=1500; CG_UpdatePickupHandshake(); CHECK(readyCount==1);
 /* A filtered/lost handshake must never trigger periodic retry traffic. */
 for (cg.time=2000;cg.time<600000;cg.time+=8) CG_UpdatePickupHandshake();
 CHECK(readyCount==1 && !CG_UsesPickupConfirmation());
 cg.time=500; CG_UpdatePickupHandshake(); CHECK(readyCount==1);
 serverVersion="1"; CG_UpdatePickupHandshake(); CHECK(readyCount==1 && !cg.pickupHandshakeActive);
 serverVersion="2"; CG_UpdatePickupHandshake(); CHECK(readyCount==1 && !cg.pickupHandshakeActive);
 serverVersion="3"; cg.demoPlayback=1; CG_UpdatePickupHandshake(); CHECK(readyCount==1);
 cg.demoPlayback=0; cg.time=1000;
 CHECK(!CG_UsesPickupConfirmation());
 argument="3"; CG_PickupReady_f(); CHECK(!CG_UsesPickupConfirmation());
 CG_UpdatePickupHandshake(); argument="1"; CG_PickupReady_f(); CHECK(!CG_UsesPickupConfirmation());
 argument="2"; CG_PickupReady_f(); CHECK(!CG_UsesPickupConfirmation());
 argument="3"; extra="bad"; CG_PickupReady_f(); CHECK(!CG_UsesPickupConfirmation()); extra="";
 CG_PickupReady_f();
 CHECK(CG_UsesPickupConfirmation()); argument="1";
 for (cg.time=2000;cg.time<600000;cg.time+=8) CG_UpdatePickupHandshake();
 CHECK(readyCount==2 && CG_UsesPickupConfirmation()); cg.time=1000;
 /* Two accepted items retain separate reliable messages, regardless of event
    ring contents or item entities disappearing before the client receives them. */
 G_ConfirmItemPickup(&item,&collector);
 item.s.modelindex=1; item.item=&bg_itemlist[1];
 G_ConfirmItemPickup(&item,&collector);
 CHECK(messageCount==2);
 for(i=0;i<messageCount;i++) { argument=strchr(messages[i],' ')+1; CG_ConfirmedPickup_f(); }
 CHECK(soundCount==2 && feedbackCount==2 && feedback[0]==2 && feedback[1]==1);
 CHECK(cg.itemPickup==2 && cg.pickupQueueCount==1);
 cg.time=1749; CG_AdvancePickupQueue(); CHECK(cg.itemPickup==2);
 cg.time=1750; CG_AdvancePickupQueue(); CHECK(cg.itemPickup==1 && cg.pickupQueueCount==0);
 /* Consecutive identical accepted items are distinct, not time-deduplicated. */
 argument="1"; CG_ConfirmedPickup_f(); CG_ConfirmedPickup_f();
 CHECK(feedbackCount==4 && soundCount==4);
 argument="-1"; CG_ConfirmedPickup_f(); argument="99999999999999999999"; CG_ConfirmedPickup_f();
 argument="1x"; CG_ConfirmedPickup_f(); argument="0"; CG_ConfirmedPickup_f();
 argument="5"; CG_ConfirmedPickup_f(); argument="1"; extra="bad"; CG_ConfirmedPickup_f(); extra="";
 CHECK(feedbackCount==4);
 snapshot.ps.clientNum=1; CG_ConfirmedPickup_f(); CHECK(feedbackCount==4); snapshot.ps.clientNum=0;
 serverVersion=""; CG_ConfirmedPickup_f(); CHECK(feedbackCount==4 && !CG_UsesPickupConfirmation());
 serverVersion="1"; CHECK(!CG_UsesPickupConfirmation());
 serverVersion="2"; CHECK(!CG_UsesPickupConfirmation()); serverVersion="3";
 /* Other servers retain ordinary feedback even when advertising capability. */
 jaPlusVersion=""; CHECK(!CG_UsesPickupConfirmation()); CG_ConfirmedPickup_f();
 jaPlusVersion="2.4B8"; CHECK(!CG_UsesPickupConfirmation()); CG_ConfirmedPickup_f();
 jaPlusVersion="2.5B0-other"; CHECK(!CG_UsesPickupConfirmation()); CG_ConfirmedPickup_f();
 CHECK(feedbackCount==4 && soundCount==4);
 cg.pickupQueueCount=0;
 CG_QueuePickupNotification(2); CHECK(cg.itemPickup==2 && cg.pickupQueueCount==0);
 jaPlusVersion="2.5B0"; CHECK(CG_UsesPickupConfirmation());
 cg_pickupConfirm.integer=0; CHECK(!CG_UsesPickupConfirmation()); cg_pickupConfirm.integer=1;
 clientVersion="0"; G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==2);
 clientVersion=""; G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==2);
 clientVersion="2"; G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==2);
 clientVersion="1"; item.item=&bg_itemlist[5]; G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==2);
 item.item=&bg_itemlist[1]; collector.s.number=MAX_CLIENTS; G_ConfirmItemPickup(&item,&collector); CHECK(messageCount==2);
 for(i=0;i<40;i++) CG_QueuePickupNotification(1);
 CHECK(cg.pickupQueueCount==16);
 for(i=0;i<16;i++) { cg.time+=750; CG_AdvancePickupQueue(); }
 CHECK(cg.pickupQueueCount==0);
 /* Reproduce v2 starving score/login under the standard shared flood limit,
    then run ten seconds of the new frame hook with gameplay commands. */
 {
  client_t transport = {CS_ACTIVE,{0}};
  svs.time=1000; CHECK(FilterCommand(&transport,"jof_pickupReady 2"));
  svs.time=1100; CHECK(!FilterCommand(&transport,"score"));
  svs.time=2000; CHECK(FilterCommand(&transport,"jof_pickupReady 2"));
  svs.time=2100; CHECK(!FilterCommand(&transport,"amlogin test"));
  cg.pickupHandshakeActive=cg.pickupConfirmed=0; readyCount=0;
  for (cg.time=3000;cg.time<13000;cg.time+=8) {
   int before=readyCount;
   svs.time=cg.time;
   CG_UpdatePickupHandshake();
   if (readyCount!=before) CHECK(FilterCommand(&transport,"jof_pickupReady 3"));
   if (cg.time>3000 && cg.time%1000==0)
    CHECK(FilterCommand(&transport,cg.time%2000==0 ? "score" : "amlogin test"));
  }
  CHECK(readyCount==1 && !CG_UsesPickupConfirmation());
 }
 puts("PASS: paired confirmation, two-item feedback and HUD queue, repeat items, malformed commands, legacy/version fallback, spectators, bounded backlog.");
 return 0;
}
