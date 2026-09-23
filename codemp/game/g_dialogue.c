/*
===========================================================================
Native, server-authoritative dialogue graphs.

Dialogue files live in dialogues/<name>.dlg.  The server parses and owns the
graph, conditions and actions; clients receive only the currently visible
text and choices.
===========================================================================
*/

#include "g_local.h"
#include "g_dialogue.h"

#define DLG_MAX_FILES           32
#define DLG_MAX_NODES           128   /* was 64 */
#define DLG_MAX_CHOICES         12
#define DLG_MAX_ACTIONS         8
#define DLG_MAX_QUESTS          256   /* was 32 */
#define DLG_MAX_FILE_SIZE       (128 * 1024)
#define DLG_ID_SIZE             32
#define DLG_SPEAKER_SIZE        64
#define DLG_TEXT_SIZE           512
#define DLG_CHOICE_TEXT_SIZE    160

typedef enum {
	DLG_CMP_NONE,
	DLG_CMP_EQ,
	DLG_CMP_NE,
	DLG_CMP_LT,
	DLG_CMP_LE,
	DLG_CMP_GT,
	DLG_CMP_GE
} dlgCompare_t;

typedef enum {
	DLG_ACTION_SETQUEST,
	DLG_ACTION_FIRE
} dlgActionType_t;

typedef struct {
	qboolean active;
	char quest[DLG_ID_SIZE];
	dlgCompare_t compare;
	int value;
} dlgCondition_t;

typedef struct {
	char text[DLG_CHOICE_TEXT_SIZE];
	char next[DLG_ID_SIZE];
	dlgCondition_t condition;
} dlgChoice_t;

typedef struct {
	dlgActionType_t type;
	char name[MAX_QPATH];
	int value;
} dlgAction_t;

typedef struct {
	char id[DLG_ID_SIZE];
	char speaker[DLG_SPEAKER_SIZE];
	char text[DLG_TEXT_SIZE];
	char next[DLG_ID_SIZE];
	qboolean end;
	int numChoices;
	dlgChoice_t choices[DLG_MAX_CHOICES];
	int numActions;
	dlgAction_t actions[DLG_MAX_ACTIONS];
} dlgNode_t;

typedef struct {
	char name[MAX_QPATH];
	char start[DLG_ID_SIZE];
	int numNodes;
	dlgNode_t nodes[DLG_MAX_NODES];
} dialogue_t;

typedef struct {
	char id[DLG_ID_SIZE];
	int stage;
} dlgQuestState_t;

typedef struct {
	qboolean active;
	unsigned int serial;
	dialogue_t *dialogue;
	int node;
	int sourceNum;
	int visibleCount;
	int visibleChoices[DLG_MAX_CHOICES];
	qboolean automaticNext;
} dlgSession_t;

static dialogue_t *s_dialogues[DLG_MAX_FILES];
static int s_numDialogues;
static dlgSession_t s_sessions[MAX_CLIENTS];
static dlgQuestState_t s_quests[MAX_CLIENTS][DLG_MAX_QUESTS];
static int s_numQuests[MAX_CLIENTS];
static unsigned int s_nextSerial;

static qboolean DLG_ReadToken( const char **cursor, char *out, size_t outSize ) {
	const char *token = COM_ParseExt( cursor, qtrue );
	if ( !token[0] ) {
		return qfalse;
	}
	Q_strncpyz( out, token, (int)outSize );
	return qtrue;
}

static qboolean DLG_Expect( const char **cursor, const char *expected, const char *fileName ) {
	char token[MAX_TOKEN_CHARS] = { 0 };
	if ( !DLG_ReadToken( cursor, token, sizeof( token ) ) || Q_stricmp( token, expected ) ) {
		trap->Print( "Dialogue %s: expected '%s', found '%s' on line %d\n",
			fileName, expected, token, COM_GetCurrentParseLine() );
		return qfalse;
	}
	return qtrue;
}

static dlgCompare_t DLG_ParseCompare( const char *token ) {
	if ( !Q_stricmp( token, "eq" ) || !strcmp( token, "==" ) ) return DLG_CMP_EQ;
	if ( !Q_stricmp( token, "ne" ) || !strcmp( token, "!=" ) ) return DLG_CMP_NE;
	if ( !Q_stricmp( token, "lt" ) || !strcmp( token, "<" ) ) return DLG_CMP_LT;
	if ( !Q_stricmp( token, "le" ) || !strcmp( token, "<=" ) ) return DLG_CMP_LE;
	if ( !Q_stricmp( token, "gt" ) || !strcmp( token, ">" ) ) return DLG_CMP_GT;
	if ( !Q_stricmp( token, "ge" ) || !strcmp( token, ">=" ) ) return DLG_CMP_GE;
	return DLG_CMP_NONE;
}

static qboolean DLG_ParseChoice( const char **cursor, dlgChoice_t *choice, const char *fileName ) {
	char token[MAX_TOKEN_CHARS];
	char value[MAX_TOKEN_CHARS];

	if ( !DLG_Expect( cursor, "{", fileName ) ) return qfalse;
	while ( DLG_ReadToken( cursor, token, sizeof( token ) ) ) {
		if ( !strcmp( token, "}" ) ) return choice->text[0] ? qtrue : qfalse;
		if ( !Q_stricmp( token, "text" ) ) {
			if ( !DLG_ReadToken( cursor, choice->text, sizeof( choice->text ) ) ) return qfalse;
		} else if ( !Q_stricmp( token, "next" ) ) {
			if ( !DLG_ReadToken( cursor, choice->next, sizeof( choice->next ) ) ) return qfalse;
		} else if ( !Q_stricmp( token, "require" ) ) {
			choice->condition.active = qtrue;
			if ( !DLG_ReadToken( cursor, choice->condition.quest, sizeof( choice->condition.quest ) ) ||
				 !DLG_ReadToken( cursor, value, sizeof( value ) ) ) return qfalse;
			choice->condition.compare = DLG_ParseCompare( value );
			if ( choice->condition.compare == DLG_CMP_NONE ||
				 !DLG_ReadToken( cursor, value, sizeof( value ) ) ) return qfalse;
			choice->condition.value = atoi( value );
		} else {
			trap->Print( "Dialogue %s: unknown choice field '%s' on line %d\n",
				fileName, token, COM_GetCurrentParseLine() );
			return qfalse;
		}
	}
	return qfalse;
}

static qboolean DLG_ParseNode( const char **cursor, dialogue_t *dialogue, const char *id, const char *fileName ) {
	dlgNode_t *node;
	char token[MAX_TOKEN_CHARS];
	char value[MAX_TOKEN_CHARS];

	if ( dialogue->numNodes >= DLG_MAX_NODES ) {
		trap->Print( "Dialogue %s: more than %d nodes\n", fileName, DLG_MAX_NODES );
		return qfalse;
	}
	node = &dialogue->nodes[dialogue->numNodes++];
	Q_strncpyz( node->id, id, sizeof( node->id ) );
	if ( !DLG_Expect( cursor, "{", fileName ) ) return qfalse;

	while ( DLG_ReadToken( cursor, token, sizeof( token ) ) ) {
		if ( !strcmp( token, "}" ) ) {
			return node->text[0] ? qtrue : qfalse;
		}
		if ( !Q_stricmp( token, "speaker" ) ) {
			if ( !DLG_ReadToken( cursor, node->speaker, sizeof( node->speaker ) ) ) return qfalse;
		} else if ( !Q_stricmp( token, "text" ) ) {
			if ( !DLG_ReadToken( cursor, node->text, sizeof( node->text ) ) ) return qfalse;
		} else if ( !Q_stricmp( token, "next" ) ) {
			if ( !DLG_ReadToken( cursor, node->next, sizeof( node->next ) ) ) return qfalse;
		} else if ( !Q_stricmp( token, "end" ) ) {
			node->end = qtrue;
		} else if ( !Q_stricmp( token, "choice" ) ) {
			if ( node->numChoices >= DLG_MAX_CHOICES ) {
				trap->Print( "Dialogue %s: node '%s' has more than %d choices\n",
					fileName, node->id, DLG_MAX_CHOICES );
				return qfalse;
			}
			if ( !DLG_ParseChoice( cursor, &node->choices[node->numChoices++], fileName ) ) return qfalse;
		} else if ( !Q_stricmp( token, "setquest" ) || !Q_stricmp( token, "fire" ) ) {
			dlgAction_t *action;
			if ( node->numActions >= DLG_MAX_ACTIONS ) return qfalse;
			action = &node->actions[node->numActions++];
			action->type = !Q_stricmp( token, "setquest" ) ? DLG_ACTION_SETQUEST : DLG_ACTION_FIRE;
			if ( !DLG_ReadToken( cursor, action->name, sizeof( action->name ) ) ) return qfalse;
			if ( action->type == DLG_ACTION_SETQUEST ) {
				if ( !DLG_ReadToken( cursor, value, sizeof( value ) ) ) return qfalse;
				action->value = atoi( value );
			}
		} else {
			trap->Print( "Dialogue %s: unknown node field '%s' on line %d\n",
				fileName, token, COM_GetCurrentParseLine() );
			return qfalse;
		}
	}
	return qfalse;
}

static int DLG_FindNode( const dialogue_t *dialogue, const char *id ) {
	int i;
	for ( i = 0; i < dialogue->numNodes; ++i ) {
		if ( !Q_stricmp( dialogue->nodes[i].id, id ) ) return i;
	}
	return -1;
}

static qboolean DLG_ValidName( const char *name ) {
	const unsigned char *p = (const unsigned char *)name;
	if ( !name[0] || strstr( name, ".." ) || name[0] == '/' || name[0] == '\\' ) return qfalse;
	for ( ; *p; ++p ) {
		if ( !( isalnum( *p ) || *p == '_' || *p == '-' || *p == '/' ) ) return qfalse;
	}
	return qtrue;
}

static qboolean DLG_ValidateLinks( const dialogue_t *dialogue, const char *fileName ) {
	int i, j;
	if ( DLG_FindNode( dialogue, dialogue->start ) < 0 ) {
		trap->Print( "Dialogue %s: start node '%s' does not exist\n", fileName, dialogue->start );
		return qfalse;
	}
	for ( i = 0; i < dialogue->numNodes; ++i ) {
		const dlgNode_t *node = &dialogue->nodes[i];
		for ( j = i + 1; j < dialogue->numNodes; ++j ) {
			if ( !Q_stricmp( node->id, dialogue->nodes[j].id ) ) {
				trap->Print( "Dialogue %s: duplicate node '%s'\n", fileName, node->id );
				return qfalse;
			}
		}
		if ( node->next[0] && DLG_FindNode( dialogue, node->next ) < 0 ) return qfalse;
		for ( j = 0; j < node->numChoices; ++j ) {
			if ( node->choices[j].next[0] && DLG_FindNode( dialogue, node->choices[j].next ) < 0 ) return qfalse;
		}
	}
	return qtrue;
}

static dialogue_t *DLG_Load( const char *name ) {
	char path[MAX_QPATH * 2];
	char token[MAX_TOKEN_CHARS];
	char id[MAX_TOKEN_CHARS];
	char *buffer;
	const char *cursor;
	fileHandle_t file = 0;
	dialogue_t *dialogue;
	int length, i;
	qboolean closed = qfalse;

	if ( !DLG_ValidName( name ) ) {
		trap->Print( "Dialogue: rejected unsafe name '%s'\n", name );
		return NULL;
	}
	for ( i = 0; i < s_numDialogues; ++i ) {
		if ( !Q_stricmp( s_dialogues[i]->name, name ) ) return s_dialogues[i];
	}
	if ( s_numDialogues >= DLG_MAX_FILES ) return NULL;

	Com_sprintf( path, sizeof( path ), "dialogues/%s.dlg", name );
	length = trap->FS_Open( path, &file, FS_READ );
	if ( length <= 0 || length >= DLG_MAX_FILE_SIZE ) {
		if ( file ) trap->FS_Close( file );
		trap->Print( "Dialogue: could not read %s\n", path );
		return NULL;
	}
	buffer = (char *)malloc( length + 1 );
	if ( !buffer ) {
		trap->FS_Close( file );
		return NULL;
	}
	trap->FS_Read( buffer, length, file );
	trap->FS_Close( file );
	buffer[length] = '\0';

	dialogue = (dialogue_t *)calloc( 1, sizeof( *dialogue ) );
	if ( !dialogue ) {
		free( buffer );
		return NULL;
	}
	Q_strncpyz( dialogue->name, name, sizeof( dialogue->name ) );
	cursor = buffer;
	COM_BeginParseSession( path );
	if ( !DLG_ReadToken( &cursor, token, sizeof( token ) ) || Q_stricmp( token, "dialogue" ) ||
		 !DLG_Expect( &cursor, "{", path ) ) goto parse_failed;

	while ( DLG_ReadToken( &cursor, token, sizeof( token ) ) ) {
		if ( !strcmp( token, "}" ) ) {
			closed = qtrue;
			break;
		}
		if ( !Q_stricmp( token, "start" ) ) {
			if ( !DLG_ReadToken( &cursor, dialogue->start, sizeof( dialogue->start ) ) ) goto parse_failed;
		} else if ( !Q_stricmp( token, "node" ) ) {
			if ( !DLG_ReadToken( &cursor, id, sizeof( id ) ) ||
				 !DLG_ParseNode( &cursor, dialogue, id, path ) ) goto parse_failed;
		} else {
			trap->Print( "Dialogue %s: unknown field '%s' on line %d\n", path, token, COM_GetCurrentParseLine() );
			goto parse_failed;
		}
	}
	free( buffer );
	if ( !dialogue->start[0] && dialogue->numNodes )
		Q_strncpyz( dialogue->start, dialogue->nodes[0].id, sizeof( dialogue->start ) );
	if ( !closed || !dialogue->numNodes || !DLG_ValidateLinks( dialogue, path ) ) goto validation_failed;
	s_dialogues[s_numDialogues++] = dialogue;
	trap->Print( "Loaded dialogue %s (%d nodes)\n", path, dialogue->numNodes );
	return dialogue;

parse_failed:
	free( buffer );
validation_failed:
	trap->Print( "Dialogue: failed to parse %s on line %d\n", path, COM_GetCurrentParseLine() );
	free( dialogue );
	return NULL;
}

static int DLG_GetQuest( int clientNum, const char *id ) {
	char normalizedId[DLG_ID_SIZE];
	int i;
	Q_strncpyz( normalizedId, id, sizeof( normalizedId ) );
	for ( i = 0; i < s_numQuests[clientNum]; ++i ) {
		if ( !Q_stricmp( s_quests[clientNum][i].id, normalizedId ) ) return s_quests[clientNum][i].stage;
	}
	return 0;
}

static qboolean DLG_SetQuest( int clientNum, const char *id, int stage ) {
	char normalizedId[DLG_ID_SIZE];
	int i;
	Q_strncpyz( normalizedId, id, sizeof( normalizedId ) );
	for ( i = 0; i < s_numQuests[clientNum]; ++i ) {
		if ( !Q_stricmp( s_quests[clientNum][i].id, normalizedId ) ) {
			s_quests[clientNum][i].stage = stage;
			return qtrue;
		}
	}
	if ( s_numQuests[clientNum] < DLG_MAX_QUESTS ) {
		dlgQuestState_t *quest = &s_quests[clientNum][s_numQuests[clientNum]++];
		Q_strncpyz( quest->id, normalizedId, sizeof( quest->id ) );
		quest->stage = stage;
		return qtrue;
	}
	return qfalse;
}

static qboolean DLG_CheckCondition( int clientNum, const dlgCondition_t *condition ) {
	int value;
	if ( !condition->active ) return qtrue;
	value = DLG_GetQuest( clientNum, condition->quest );
	switch ( condition->compare ) {
	case DLG_CMP_EQ: return value == condition->value;
	case DLG_CMP_NE: return value != condition->value;
	case DLG_CMP_LT: return value < condition->value;
	case DLG_CMP_LE: return value <= condition->value;
	case DLG_CMP_GT: return value > condition->value;
	case DLG_CMP_GE: return value >= condition->value;
	default: return qfalse;
	}
}

static void DLG_EscapeCommandText( const char *in, char *out, size_t outSize ) {
	size_t used = 0;
	while ( *in && used + 1 < outSize ) {
		unsigned char c = (unsigned char)*in++;
		if ( c == '\r' ) continue;
		if ( c == '\n' ) {
			if ( used + 2 >= outSize ) break;
			out[used++] = '\\'; out[used++] = 'n';
		} else if ( c == '"' ) {
			out[used++] = '\'';
		} else if ( c < 32 ) {
			out[used++] = ' ';
		} else {
			out[used++] = (char)c;
		}
	}
	out[used] = '\0';
}

static void DLG_Stop( gentity_t *player, qboolean completed ) {
	int clientNum = player - g_entities;
	dlgSession_t old = s_sessions[clientNum];
	if ( !old.active ) return;
	memset( &s_sessions[clientNum], 0, sizeof( s_sessions[clientNum] ) );
	trap->SendServerCommand( clientNum, va( "jof_dialogue stop %u", old.serial ) );
	if ( completed && old.sourceNum >= 0 && old.sourceNum < MAX_ENTITIESTOTAL ) {
		gentity_t *source = &g_entities[old.sourceNum];
		if ( source->inuse && source->target ) G_UseTargets( source, player );
	}
}

static void DLG_ShowNode( gentity_t *player, int nodeIndex ) {
	int clientNum = player - g_entities;
	dlgSession_t *session = &s_sessions[clientNum];
	dlgNode_t *node;
	unsigned int nodeSerial;
	char speaker[DLG_SPEAKER_SIZE];
	char text[DLG_TEXT_SIZE];
	int i;

	if ( !session->active || nodeIndex < 0 || nodeIndex >= session->dialogue->numNodes ) {
		DLG_Stop( player, qfalse );
		return;
	}
	session->node = nodeIndex;
	session->visibleCount = 0;
	session->automaticNext = qfalse;
	session->serial = ++s_nextSerial;
	if ( !session->serial ) session->serial = ++s_nextSerial;
	nodeSerial = session->serial;
	node = &session->dialogue->nodes[nodeIndex];

	for ( i = 0; i < node->numActions; ++i ) {
		dlgAction_t *action = &node->actions[i];
		if ( action->type == DLG_ACTION_SETQUEST ) {
			DLG_SetQuest( clientNum, action->name, action->value );
		} else if ( action->type == DLG_ACTION_FIRE ) {
			gentity_t *source = player;
			if ( session->sourceNum >= 0 && session->sourceNum < MAX_ENTITIESTOTAL &&
				 g_entities[session->sourceNum].inuse ) source = &g_entities[session->sourceNum];
			G_UseTargets2( source, player, action->name );
		}
		if ( !session->active || session->serial != nodeSerial ) return;
	}

	DLG_EscapeCommandText( node->speaker[0] ? node->speaker : "Dialogue", speaker, sizeof( speaker ) );
	DLG_EscapeCommandText( node->text, text, sizeof( text ) );
	trap->SendServerCommand( clientNum, va( "jof_dialogue begin %u \"%s\" \"%s\"", session->serial, speaker, text ) );

	for ( i = 0; i < node->numChoices; ++i ) {
		char choiceText[DLG_CHOICE_TEXT_SIZE];
		if ( !DLG_CheckCondition( clientNum, &node->choices[i].condition ) ) continue;
		session->visibleChoices[session->visibleCount] = i;
		DLG_EscapeCommandText( node->choices[i].text, choiceText, sizeof( choiceText ) );
		trap->SendServerCommand( clientNum, va( "jof_dialogue choice %u %d \"%s\"",
			session->serial, session->visibleCount, choiceText ) );
		++session->visibleCount;
	}
	if ( session->visibleCount == 0 ) {
		const char *label = node->next[0] && !node->end ? "Continue" : "Close";
		session->automaticNext = node->next[0] && !node->end;
		session->visibleCount = 1;
		session->visibleChoices[0] = -1;
		trap->SendServerCommand( clientNum, va( "jof_dialogue choice %u 0 \"%s\"", session->serial, label ) );
	}
	trap->SendServerCommand( clientNum, va( "jof_dialogue show %u", session->serial ) );
}

void G_DialogueInit( void ) {
	int i;
	for ( i = 0; i < s_numDialogues; ++i ) free( s_dialogues[i] );
	memset( s_dialogues, 0, sizeof( s_dialogues ) );
	memset( s_sessions, 0, sizeof( s_sessions ) );
	memset( s_quests, 0, sizeof( s_quests ) );
	memset( s_numQuests, 0, sizeof( s_numQuests ) );
	s_numDialogues = 0;
	s_nextSerial = 0;
}

void G_DialogueShutdown( void ) {
	int i;
	for ( i = 0; i < s_numDialogues; ++i ) free( s_dialogues[i] );
	s_numDialogues = 0;
	memset( s_dialogues, 0, sizeof( s_dialogues ) );
}

void G_DialogueClientDisconnect( int clientNum ) {
	if ( clientNum < 0 || clientNum >= MAX_CLIENTS ) return;
	memset( &s_sessions[clientNum], 0, sizeof( s_sessions[clientNum] ) );
	memset( s_quests[clientNum], 0, sizeof( s_quests[clientNum] ) );
	s_numQuests[clientNum] = 0;
}

qboolean G_DialogueStart( gentity_t *player, const char *name, gentity_t *source ) {
	dialogue_t *dialogue;
	dlgSession_t *session;
	int clientNum, start;
	if ( !player || !player->client || player->client->pers.connected != CON_CONNECTED ) return qfalse;
	dialogue = DLG_Load( name );
	if ( !dialogue ) return qfalse;
	clientNum = player - g_entities;
	session = &s_sessions[clientNum];
	memset( session, 0, sizeof( *session ) );
	session->active = qtrue;
	session->dialogue = dialogue;
	session->sourceNum = source ? (int)( source - g_entities ) : -1;
	start = DLG_FindNode( dialogue, dialogue->start );
	DLG_ShowNode( player, start );
	return qtrue;
}

int G_DialogueGetQuestStage( gentity_t *player, const char *id ) {
	if ( !player || !player->client || !id || !id[0] ) return 0;
	return DLG_GetQuest( player - g_entities, id );
}

qboolean G_DialogueSetQuestStage( gentity_t *player, const char *id, int stage ) {
	if ( !player || !player->client || !id || !id[0] ) return qfalse;
	return DLG_SetQuest( player - g_entities, id, stage );
}

void Cmd_DialogueResponse_f( gentity_t *ent ) {
	char arg[MAX_TOKEN_CHARS];
	unsigned int serial;
	int selection, clientNum = ent - g_entities;
	dlgSession_t *session = &s_sessions[clientNum];
	dlgNode_t *node;
	dlgChoice_t *choice;

	if ( trap->Argc() != 3 || !session->active ) return;
	trap->Argv( 1, arg, sizeof( arg ) ); serial = (unsigned int)strtoul( arg, NULL, 10 );
	trap->Argv( 2, arg, sizeof( arg ) ); selection = atoi( arg );
	if ( serial != session->serial ) return;
	if ( selection == -1 ) {
		DLG_Stop( ent, qfalse );
		return;
	}
	if ( selection < 0 || selection >= session->visibleCount ) return;
	node = &session->dialogue->nodes[session->node];
	if ( session->visibleChoices[selection] < 0 ) {
		if ( session->automaticNext ) DLG_ShowNode( ent, DLG_FindNode( session->dialogue, node->next ) );
		else DLG_Stop( ent, qtrue );
		return;
	}
	choice = &node->choices[session->visibleChoices[selection]];
	if ( !DLG_CheckCondition( clientNum, &choice->condition ) ) return;
	if ( choice->next[0] ) DLG_ShowNode( ent, DLG_FindNode( session->dialogue, choice->next ) );
	else DLG_Stop( ent, qtrue );
}

void Cmd_DialogueTest_f( gentity_t *ent ) {
	char name[MAX_QPATH];
	if ( trap->Argc() != 2 ) {
		trap->SendServerCommand( ent - g_entities, "print \"usage: dialoguetest <name>\\n\"" );
		return;
	}
	trap->Argv( 1, name, sizeof( name ) );
	if ( !G_DialogueStart( ent, name, NULL ) )
		trap->SendServerCommand( ent - g_entities, va( "print \"Could not start dialogue '%s'\\n\"", name ) );
}
