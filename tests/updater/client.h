// Stand-in for codemp/client/client.h: only what cl_update.cpp uses of the engine.
// CMake copies cl_update.cpp beside this file, so its #include "client.h" lands here.
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

#define JOF_RELEASE_TAG		"1.6.1-beta.119"

#define CVAR_ARCHIVE_ND		0x1u
#define CVAR_INIT			0x10u
#define CVAR_TEMP			0x100u
#define CVAR_PROTECTED		0x8000u

struct cvar_t {
	char		*string;
	int			integer;
	qboolean	modified;
};

enum { EXEC_NOW, EXEC_INSERT, EXEC_APPEND };
enum { CA_DISCONNECTED, CA_ACTIVE = 8 };

struct clientStatic_t {
	int		state;
};
extern clientStatic_t cls;

typedef void ( *xcommand_t )( void );

cvar_t	*Cvar_Get( const char *name, const char *value, uint32_t flags, const char *description = NULL );
void	Cmd_AddCommand( const char *name, xcommand_t function, const char *description = NULL );
void	Cmd_RemoveCommand( const char *name );
void	Cbuf_ExecuteText( int when, const char *text );
void	Com_Printf( const char *fmt, ... );
void	Com_DPrintf( const char *fmt, ... );

inline int Q_stricmp( const char *a, const char *b ) {
	return _stricmp( a, b );
}
