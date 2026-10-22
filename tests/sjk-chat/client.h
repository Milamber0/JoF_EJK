// Stand-in for codemp/client/client.h: only what cl_sjk.cpp uses of the engine.
// CMake copies cl_sjk.cpp beside this file, so its #include "client.h" lands here.
#pragma once

#include <cstdarg>
#include <cstdint>
#include <cstdio>
#include <cstring>

typedef enum { qfalse, qtrue } qboolean;

#define ARRAY_LEN( x )		( sizeof( x ) / sizeof( *( x ) ) )
#define S_COLOR_RED			"^1"
#define S_COLOR_GREEN		"^2"
#define S_COLOR_YELLOW		"^3"
#define S_COLOR_WHITE		"^7"
#define S_COLOR_ORANGE		"^8"
#define Q_COLOR_ESCAPE		'^'

// As cgame/cg_public.h.
typedef struct sjkChatLine_s {
	const char		*who;
	const char		*text;
	const char		*nameWords;
	const char		*words;
	const char		*client;
	const char		*id;
} sjkChatLine_t;

#define JOF_RELEASE_TAG		"1.0.0-beta.1"
#define JOF_VERSION_STRING	"1.0.0"
#define JOF_COMMIT_SHA_SHORT	"0123abc"

#define CVAR_ARCHIVE_ND		0x1u
#define CVAR_PROTECTED		0x8000u
#define CVAR_ROM			0x40u

struct cvar_t {
	char		*string;
	int			integer;
	qboolean	modified;
};

typedef void ( *xcommand_t )( void );

cvar_t		*Cvar_Get( const char *name, const char *value, uint32_t flags, const char *description = NULL );
cvar_t		*Cvar_Set2( const char *name, const char *value, uint32_t defaultFlags, qboolean force );
const char	*Cvar_VariableString( const char *name );
void		Cmd_AddCommand( const char *name, xcommand_t function, const char *description = NULL );
void		Cmd_RemoveCommand( const char *name );
int			Cmd_Argc( void );
char		*Cmd_ArgsFrom( int arg );
void		Com_Printf( const char *fmt, ... );
qboolean	CL_SJK_ToChatBox( const sjkChatLine_t *line );
qboolean	CL_SJK_ServerSession( char *serverId, int serverIdSize, char *mapname, int mapnameSize );
void		CL_SayToServer( const char *text );
int			Sys_Milliseconds( bool baseTime = false );

#define MAX_SAY_TEXT		150
typedef enum { CON_ICON_NONE, CON_ICON_SJK, CON_ICON_EJK } conIcon_t;
inline void	Con_SetPrintIcon( conIcon_t ) {}
void		Q_StripColor( char *text );
uint8_t		ConvertUTF32ToExpectedCharset( uint32_t utf32 );	// the engine's, extracted by CMake

inline void Q_strncpyz( char *dest, const char *src, int destsize ) {
	strncpy( dest, src, destsize - 1 );
	dest[destsize - 1] = 0;
}

inline int Q_stricmp( const char *a, const char *b ) {
	return _stricmp( a, b );
}

inline int Q_stricmpn( const char *a, const char *b, int n ) {
	return _strnicmp( a, b, n );
}
