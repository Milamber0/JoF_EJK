#pragma once

typedef struct gentity_s gentity_t;

void G_DialogueInit( void );
void G_DialogueShutdown( void );
void G_DialogueClientDisconnect( int clientNum );
qboolean G_DialogueStart( gentity_t *player, const char *name, gentity_t *source );
int G_DialogueGetQuestStage( gentity_t *player, const char *id );
qboolean G_DialogueSetQuestStage( gentity_t *player, const char *id, int stage );
void Cmd_DialogueResponse_f( gentity_t *ent );
void Cmd_DialogueTest_f( gentity_t *ent );
