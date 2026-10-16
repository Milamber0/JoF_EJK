/*
===========================================================================
rd-freedom: the engine-facing half of SJK's renderer for JoF EJK.

The model, skin, shader-name and Ghoul2 code beside this file is rd-dedicated's
(GL-free), because the game, cgame and server need it whatever draws. Drawing is
SJK's renderer (sjk_host.h): this file opens the engine's window, starts SJK on
it, tells it the shader names the game registers, and each frame hands it the
main view (refdef_t) and the 2D pictures, which SJK draws over its own world.

SJK: github.com/Sol-Vulpes/SJK, GPL-2.0-only; see its NOTICE.

This file is part of JoF EJK, GPL-2.0.
===========================================================================
*/

#include "tr_local.h"
#include "../rd-common/tr_common.h"
#include "../rd-common/tr_font.h"
#include "qcommon/qfiles.h"
#include "sjk_host.h"

#ifdef _WIN32
// The module that owns the engine's window class; Vulkan's Win32 surface needs it.
extern "C" __declspec(dllimport) void *__stdcall GetModuleHandleW( const wchar_t *moduleName );
#endif

glconfig_t	glConfig;
static window_t	window;

static qboolean	hostStarted;
static int		shadersSent;
static int		modelsSent, skinsSent;
static void		SJK_AllocScene( void );
void			SJK_InstallCrashLog( const char *path );	// sjk_crash.cpp
void			SJK_RemoveCrashLog( void );
void			SJK_Trace( const char *line );
static void		SJK_FreeScene( void );
static void		SJK_SendAssetNames( void );
qboolean		G2API_HaveWeGhoul2Models( CGhoul2Info_v &ghoul2 );

// The world the engine loaded: cgame parses its entities through GetEntityToken.
static char		*entityString;
static const char	*entityParsePoint;
static const byte	*worldVis;
static char		worldName[MAX_QPATH];

// This frame's main view and 2D pictures.
static sjkHostView_t	frameView;
static qboolean			frameHasView;
#define MAX_SJK_PICS	32768
static sjkHostPic_t		*framePics;
static int				numFramePics;
static float			drawColor[4] = { 1, 1, 1, 1 };

// Light styles cgame sets (CS_LIGHT_STYLES); SJK reads its own from the map for now.
static color4ub_t		lightStyles[MAX_LIGHT_STYLES];

static cvar_t	*r_sjkDebug;
cvar_t	*r_aspectCorrectFonts;
cvar_t	*r_ratioFix;
cvar_t	*r_fontOverride;

static void SJK_FlushLog( void ) {
	char line[1024];
	while ( SJK_HostPollLog( line, sizeof( line ) ) ) {
		ri.Printf( PRINT_ALL, "%s\n", line );
	}
}

// Tell SJK the names of shaders registered since the last call.
static void SJK_SendShaderNames( void ) {
	if ( !hostStarted ) {
		return;
	}
	for ( ; shadersSent < tr.numShaders; shadersSent++ ) {
		const shader_t *sh = tr.shaders[shadersSent];
		SJK_HostShaderName( shadersSent, sh ? sh->name : "" );
	}
}

static void SJK_StartHost( void ) {
	if ( hostStarted ) {
		return;
	}
	const char *basePath = ri.Cvar_VariableString( "fs_basepath" );
	char base[MAX_OSPATH], home[MAX_OSPATH], game[MAX_QPATH], config[MAX_OSPATH], sjkConfig[MAX_OSPATH];
	Q_strncpyz( base, basePath, sizeof( base ) );
	Q_strncpyz( home, ri.Cvar_VariableString( "fs_homepath" ), sizeof( home ) );
	Q_strncpyz( game, ri.Cvar_VariableString( "fs_game" ), sizeof( game ) );
	// The hosted renderer's own settings, beside EternalJK's; SJK's own profile is
	// read once to start them from what the player chose there.
	Com_sprintf( config, sizeof( config ), "%s/" ETERNALJKGAME "/rd-freedom/config.cfg", home[0] ? home : base );
	Com_sprintf( sjkConfig, sizeof( sjkConfig ), "%s/SJK/config.cfg", base );
	char crashLog[MAX_OSPATH];
	Com_sprintf( crashLog, sizeof( crashLog ), "%s/" ETERNALJKGAME "/rd-freedom/crash.log", home[0] ? home : base );
	SJK_InstallCrashLog( crashLog );

	sjkHostStart_t start;
	memset( &start, 0, sizeof( start ) );
	start.hwnd = (intptr_t)window.handle;
#ifdef _WIN32
	start.hinstance = (intptr_t)GetModuleHandleW( NULL );
#endif
	start.width = glConfig.vidWidth;
	start.height = glConfig.vidHeight;
	start.basePath = base;
	start.homePath = home;
	start.game = game;
	start.config = config;
	start.sjkConfig = sjkConfig;

	// GPL-2 section 2(c), as the renderer's NOTICE asks.
	ri.Printf( PRINT_ALL, "Freedom renderer for JoF EJK, built on SJK (Sol JK): Copyright (C) 2026 Sol-Vulpes,\n"
		"Bishop-R and the JKR contributors; modified for JoF EJK. Free software under the GNU GPL\n"
		"version 2 with ABSOLUTELY NO WARRANTY; see lib/sjk/NOTICE and lib/sjk/LICENSE.\n" );
	hostStarted = (qboolean)( SJK_HostInit( &start ) != 0 );
	SJK_FlushLog();
	if ( !hostStarted ) {
		ri.Error( ERR_FATAL, "rd-freedom: the renderer could not start on this window (see the console)" );
	}
	static char adapter[256];
	SJK_HostAdapter( adapter, sizeof( adapter ) );
	glConfig.renderer_string = adapter;
	shadersSent = 0;
	modelsSent = 0;
	skinsSent = 0;
}

/*
==================
Window and registration
==================
*/

static void R_Set2DRatio( void ) {
	if ( r_ratioFix->integer && glConfig.vidWidth && glConfig.vidHeight )
		tr.widthRatioCoef = ( (float)( SCREEN_WIDTH * glConfig.vidHeight ) / (float)( SCREEN_HEIGHT * glConfig.vidWidth ) );
	else
		tr.widthRatioCoef = 1.0f;
	if ( tr.widthRatioCoef > 1 )
		tr.widthRatioCoef = 1.0f;
}

extern void R_Init( void );
extern qhandle_t RE_RegisterShaderNoMip( const char *name );

static void SJK_InitWindow( void ) {
	if ( glConfig.vidWidth != 0 ) {
		return;
	}
	windowDesc_t windowDesc = { GRAPHICS_API_GENERIC };
	memset( &glConfig, 0, sizeof( glConfig ) );
	window = ri.WIN_Init( &windowDesc, &glConfig );
	if ( !window.handle ) {
		ri.Error( ERR_FATAL, "rd-freedom: the engine gave no native window" );
	}
	glConfig.vendor_string = "SJK";
	glConfig.renderer_string = "SJK renderer";
	glConfig.version_string = "rd-freedom";
	glConfig.extensions_string = "";
	glConfig.maxTextureSize = 16384;
	glConfig.maxActiveTextures = 8;
	glConfig.maxTextureFilterAnisotropy = 16.0f;
	glConfig.colorBits = 32;
	glConfig.depthBits = 24;
	glConfig.stencilBits = 8;
	glConfig.deviceSupportsGamma = qfalse;
	glConfig.textureCompression = TC_S3TC_ARB;
	glConfig.textureEnvAddAvailable = qtrue;
	glConfig.clampToEdgeAvailable = qtrue;
}

extern void R_SJK_ResetShaderExistence( void );	// tr_shader.cpp

/*
==================
Graphics settings

The hosted renderer keeps its settings in its own profile. Each one is mirrored as
an engine cvar of the same name, so the menus and the console set it as any other
cvar; a change is handed over at the next frame, and the mirrors follow the
renderer after a quality level sets many at once.
==================
*/

#define SJK_MAX_SETTINGS	128
typedef struct {
	cvar_t	*cvar;
	int		modificationCount;
} sjkSetting_t;
static sjkSetting_t	settings[SJK_MAX_SETTINGS];
static int			numSettings;

static char *SJK_SettingsText( void ) {
	const int size = SJK_HostSettings( NULL, 0 );
	if ( size <= 1 ) {
		return NULL;
	}
	char *text = (char *)Z_Malloc( size, TAG_GENERAL, qfalse );
	SJK_HostSettings( text, size );
	return text;
}

// Register or refresh every mirror from the renderer's values.
static void SJK_SyncSettings( void ) {
	char *text = SJK_SettingsText();
	if ( !text ) {
		return;
	}
	for ( char *line = text; line && *line; ) {
		char *next = strchr( line, '\n' );
		if ( next ) {
			*next++ = 0;
		}
		char *fields[4] = { line, NULL, NULL, NULL };
		for ( int i = 1; i < 4; i++ ) {
			char *tab = fields[i - 1] ? strchr( fields[i - 1], '\t' ) : NULL;
			if ( tab ) {
				*tab = 0;
				fields[i] = tab + 1;
			}
		}
		line = next;
		if ( !fields[1] || !fields[0][0] ) {
			continue;
		}
		int index = 0;
		while ( index < numSettings && Q_stricmp( settings[index].cvar->name, fields[0] ) ) {
			index++;
		}
		if ( index == numSettings ) {
			if ( numSettings == SJK_MAX_SETTINGS ) {
				continue;
			}
			settings[numSettings].cvar = ri.Cvar_Get( fields[0], fields[2] ? fields[2] : fields[1], CVAR_NONE, fields[3] ? fields[3] : "" );
			numSettings++;
		}
		sjkSetting_t *setting = &settings[index];
		if ( strcmp( setting->cvar->string, fields[1] ) ) {
			ri.Cvar_Set( fields[0], fields[1] );
		}
		setting->modificationCount = setting->cvar->modificationCount;
	}
	Z_Free( text );
}

// The engine's own video settings the renderer follows (the setup menu's Video
// Sync, brightness and texture filter): the engine's value is the one kept.
static const struct {
	const char *name, *defaultValue;
} forwardedSettings[] = {
	{ "r_swapInterval", "0" },
	{ "r_gamma", "1" },
	{ "r_textureMode", "GL_LINEAR_MIPMAP_LINEAR" },
	{ "r_ext_texture_filter_anisotropic", "16" },
};
static sjkSetting_t	forwarded[ARRAY_LEN( forwardedSettings )];

static void SJK_ForwardSettings( qboolean all ) {
	for ( size_t i = 0; i < ARRAY_LEN( forwardedSettings ); i++ ) {
		sjkSetting_t *setting = &forwarded[i];
		if ( !setting->cvar ) {
			setting->cvar = ri.Cvar_Get( forwardedSettings[i].name, forwardedSettings[i].defaultValue, CVAR_ARCHIVE_ND, "" );
			all = qtrue;
		}
		if ( all || setting->cvar->modificationCount != setting->modificationCount ) {
			setting->modificationCount = setting->cvar->modificationCount;
			SJK_HostSetCvar( setting->cvar->name, setting->cvar->string );
		}
	}
}

// Hand the renderer the settings changed since the last frame.
static void SJK_PushSettings( void ) {
	SJK_ForwardSettings( qfalse );
	qboolean changed = qfalse;
	for ( int i = 0; i < numSettings; i++ ) {
		sjkSetting_t *setting = &settings[i];
		if ( setting->cvar->modificationCount == setting->modificationCount ) {
			continue;
		}
		setting->modificationCount = setting->cvar->modificationCount;
		// The menus write numbers as "%f"; an on/off or whole-number setting takes
		// them rounded.
		const char *value = setting->cvar->string;
		char *end = NULL;
		const double number = strtod( value, &end );
		if ( !SJK_HostSetCvar( setting->cvar->name, value )
			&& !( end && end != value && !*end && SJK_HostSetCvar( setting->cvar->name, va( "%d", (int)floor( number + 0.5 ) ) ) ) ) {
			ri.Printf( PRINT_WARNING, "%s: \"%s\" is not a value it takes\n", setting->cvar->name, setting->cvar->string );
		}
		changed = qtrue;
	}
	if ( changed ) {
		SJK_SyncSettings();
	}
}

// graphicsquality [vanilla|performance|balanced|high|ultra]
static void SJK_Quality_f( void ) {
	char args[256];
	ri.Cmd_ArgsBuffer( args, sizeof( args ) );
	SJK_PushSettings();
	static char reply[1024];
	SJK_HostQuality( args, reply, sizeof( reply ) );
	if ( reply[0] ) {
		ri.Printf( PRINT_ALL, "%s\n", reply );
	}
	SJK_SyncSettings();
}

void RE_BeginRegistration( glconfig_t *glconfigOut ) {
	R_Init();
	R_InitShaders( qfalse );
	R_InitSkins();
	R_SJK_ResetShaderExistence();
	r_sjkDebug = ri.Cvar_Get( "r_freedomDebug", "0", CVAR_NONE, "Log the renderer's frame details (2: list the entities once)" );
	r_aspectCorrectFonts = ri.Cvar_Get( "r_aspectCorrectFonts", "0", CVAR_ARCHIVE, "" );
	r_ratioFix = ri.Cvar_Get( "cl_ratioFix", "1", CVAR_ARCHIVE, "" );
	r_fontOverride = ri.Cvar_Get( "r_fontOverride", "0", CVAR_NONE, "" );
	extern cvar_t *se_language;
	se_language = ri.Cvar_Get( "se_language", "english", CVAR_ARCHIVE | CVAR_NORESTART, "" );
	SJK_InitWindow();
	R_Set2DRatio();
	SJK_StartHost();
	SJK_SyncSettings();
	SJK_ForwardSettings( qtrue );
	ri.Cmd_AddCommand( "graphicsquality", SJK_Quality_f, "Show or set graphics quality: vanilla, performance, balanced, high or ultra" );

	// The shaders 2D code uses before any are registered.
	int lightmaps2d[MAXLIGHTMAPS] = { LIGHTMAP_2D, LIGHTMAP_2D, LIGHTMAP_2D, LIGHTMAP_2D };
	byte styles[MAXLIGHTMAPS] = { LS_LSNONE, LS_LSNONE, LS_LSNONE, LS_LSNONE };
	tr.defaultShader = R_FindShader( "<default>", lightmaps2d, styles, qtrue );
	tr.whiteShader = R_FindShader( "white", lightmaps2d, styles, qtrue );

	R_InitFonts();
	SJK_AllocScene();
	modelsSent = 0;
	skinsSent = 0;

	*glconfigOut = glConfig;
	tr.registered = qtrue;
	tr.viewCluster = -1;
	numFramePics = 0;
	frameHasView = qfalse;
	SJK_SendAssetNames();
}

void RE_EndRegistration( void ) {
	SJK_SendAssetNames();
	SJK_FlushLog();
}

// Register the world's inline models (*1..*N) as rd-vanilla's R_LoadSubmodels does, so
// cgame's movers get real handles and bounds; SJK draws them from its own BSP copy.
static void SJK_RegisterSubmodels( const byte *buffer, int length ) {
	const dheader_t *header = (const dheader_t *)buffer;
	const int offset = LittleLong( header->lumps[LUMP_MODELS].fileofs );
	const int size = LittleLong( header->lumps[LUMP_MODELS].filelen );
	if ( offset < 0 || size <= 0 || offset + size > length || size % sizeof( dmodel_t ) ) {
		return;
	}
	const dmodel_t *in = (const dmodel_t *)( buffer + offset );
	const int count = size / sizeof( dmodel_t );
	for ( int i = 1; i < count; i++ ) {
		model_t *mod = R_AllocModel();
		if ( !mod ) {
			break;
		}
		mod->type = MOD_BRUSH;
		Com_sprintf( mod->name, sizeof( mod->name ), "*%d", i );
		mod->bmodel = (bmodel_t *)ri.Hunk_Alloc( sizeof( bmodel_t ), h_low );
		memset( mod->bmodel, 0, sizeof( bmodel_t ) );
		for ( int k = 0; k < 3; k++ ) {
			mod->bmodel->bounds[0][k] = LittleFloat( in[i].mins[k] );
			mod->bmodel->bounds[1][k] = LittleFloat( in[i].maxs[k] );
		}
		RE_InsertModelIntoHash( mod->name, mod );
	}
}

// The world's entity lump, read straight from the BSP for cgame's spawn parsing.
static void SJK_ReadWorld( const char *name ) {
	if ( entityString ) {
		Z_Free( entityString );
		entityString = NULL;
	}
	entityParsePoint = NULL;
	void *buffer = NULL;
	const int length = ri.FS_ReadFile( name, &buffer );
	if ( length <= 0 || !buffer ) {
		ri.Error( ERR_DROP, "rd-freedom: %s not found", name );
		return;
	}
	const dheader_t *header = (const dheader_t *)buffer;
	const int offset = LittleLong( header->lumps[LUMP_ENTITIES].fileofs );
	const int size = LittleLong( header->lumps[LUMP_ENTITIES].filelen );
	if ( offset >= 0 && size > 0 && offset + size <= length ) {
		entityString = (char *)Z_Malloc( size + 1, TAG_GENERAL, qfalse );
		memcpy( entityString, (const byte *)buffer + offset, size );
		entityString[size] = 0;
		entityParsePoint = entityString;
	}
	SJK_RegisterSubmodels( (const byte *)buffer, length );
	ri.FS_FreeFile( buffer );
}

static void SJK_SendAssetNames( void );

void RE_LoadWorldMap( const char *name ) {
	if ( tr.worldMapLoaded ) {
		ri.Error( ERR_DROP, "ERROR: attempted to redundantly load world map" );
	}
	SJK_ReadWorld( name );
	Q_strncpyz( worldName, name, sizeof( worldName ) );
	tr.worldMapLoaded = qtrue;
	SJK_SendAssetNames();
	const int loaded = SJK_HostLoadWorld( name );
	SJK_FlushLog();
	if ( !loaded ) {
		ri.Printf( PRINT_WARNING, "rd-freedom: could not build %s; it shows no world\n", name );
	}
}

void RE_SetWorldVisData( const byte *vis ) {
	worldVis = vis;
}

qboolean RE_GetEntityToken( char *buffer, int size ) {
	if ( size == -1 ) {
		entityParsePoint = entityString;
		return qtrue;
	}
	if ( !entityParsePoint ) {
		if ( size > 0 ) buffer[0] = 0;
		return qfalse;
	}
	const char *s = COM_Parse( &entityParsePoint );
	Q_strncpyz( buffer, s, size );
	return ( entityParsePoint && s[0] ) ? qtrue : qfalse;
}

/*
==================
Model and skin names

SJK resolves every handle the game passes by name: shaders, models and skins are
told to it once each, as they are registered.
==================
*/


static void SJK_SendAssetNames( void ) {
	if ( !hostStarted ) {
		return;
	}
	SJK_SendShaderNames();
	for ( ; modelsSent < tr.numModels; modelsSent++ ) {
		const model_t *mod = tr.models[modelsSent];
		SJK_HostModelName( modelsSent, mod ? mod->name : "" );
	}
	for ( ; skinsSent < tr.numSkins; skinsSent++ ) {
		const skin_t *skin = tr.skins[skinsSent];
		static char text[128 * ( MAX_QPATH * 2 + 2 ) + 1];
		text[0] = 0;
		size_t used = 0;
		if ( skin ) {
			for ( int i = 0; i < skin->numSurfaces && i < 128; i++ ) {
				const skinSurface_t *surf = skin->surfaces[i];
				if ( !surf ) {
					continue;
				}
				const int wrote = Com_sprintf( text + used, sizeof( text ) - used, "%s,%s\n",
					surf->name, surf->shader ? surf->shader->name : "*off" );
				if ( wrote <= 0 ) {
					break;
				}
				used += wrote;
			}
		}
		SJK_HostSkin( skinsSent, text );
	}
}

/*
==================
Scene

cgame builds several scenes a frame (the main view, then menu model previews). The
calls between ClearScene and the main view's RenderScene are kept as given and turned
into SJK's entities there, at the view's time, where rd-vanilla poses Ghoul2.
==================
*/

#define SJK_MAX_REFENTITIES	4096
#define SJK_MAX_LIGHTS		256
#define SJK_MAX_ENTITIES	8192
#define SJK_MAX_BONE_FLOATS	( 1024 * 72 * 12 )
#define SJK_MAX_INTS		( 1024 * 256 )

static refEntity_t		*pendingEntities;
static int				numPendingEntities;
static sjkHostLight_t	pendingLights[SJK_MAX_LIGHTS];
static int				numPendingLights;
static int				numScenePolys;

static sjkHostEntity_t	*sceneEntities;
static int				numSceneEntities;
static float			*sceneBones;
static int				numSceneBoneFloats;
static int				*sceneInts;
static int				numSceneInts;
static sjkHostLight_t	sceneLights[SJK_MAX_LIGHTS];
static int				numSceneLights;

// Polys (marks, effect polys, trails): queued as given, handed over with the main view.
#define SJK_MAX_POLYS		8192
#define SJK_MAX_POLYVERTS	32768
static sjkHostPoly_t		*pendingPolys, *scenePolys;
static sjkHostPolyVert_t	*pendingPolyVerts, *scenePolyVerts;
static int					numPendingPolys, numPendingPolyVerts, numScenePolyList, numScenePolyVerts;

static void SJK_AllocScene( void ) {
	if ( pendingEntities ) {
		return;
	}
	pendingEntities = (refEntity_t *)Z_Malloc( sizeof( refEntity_t ) * SJK_MAX_REFENTITIES, TAG_GENERAL, qtrue );
	sceneEntities = (sjkHostEntity_t *)Z_Malloc( sizeof( sjkHostEntity_t ) * SJK_MAX_ENTITIES, TAG_GENERAL, qtrue );
	sceneBones = (float *)Z_Malloc( sizeof( float ) * SJK_MAX_BONE_FLOATS, TAG_GENERAL, qfalse );
	sceneInts = (int *)Z_Malloc( sizeof( int ) * SJK_MAX_INTS, TAG_GENERAL, qfalse );
	pendingPolys = (sjkHostPoly_t *)Z_Malloc( sizeof( sjkHostPoly_t ) * SJK_MAX_POLYS, TAG_GENERAL, qfalse );
	scenePolys = (sjkHostPoly_t *)Z_Malloc( sizeof( sjkHostPoly_t ) * SJK_MAX_POLYS, TAG_GENERAL, qfalse );
	pendingPolyVerts = (sjkHostPolyVert_t *)Z_Malloc( sizeof( sjkHostPolyVert_t ) * SJK_MAX_POLYVERTS, TAG_GENERAL, qfalse );
	scenePolyVerts = (sjkHostPolyVert_t *)Z_Malloc( sizeof( sjkHostPolyVert_t ) * SJK_MAX_POLYVERTS, TAG_GENERAL, qfalse );
}

static void SJK_FreeScene( void ) {
	if ( pendingEntities ) Z_Free( pendingEntities );
	if ( sceneEntities ) Z_Free( sceneEntities );
	if ( sceneBones ) Z_Free( sceneBones );
	if ( sceneInts ) Z_Free( sceneInts );
	if ( pendingPolys ) Z_Free( pendingPolys );
	if ( scenePolys ) Z_Free( scenePolys );
	if ( pendingPolyVerts ) Z_Free( pendingPolyVerts );
	if ( scenePolyVerts ) Z_Free( scenePolyVerts );
	pendingPolys = scenePolys = NULL;
	pendingPolyVerts = scenePolyVerts = NULL;
	pendingEntities = NULL;
	sceneEntities = NULL;
	sceneBones = NULL;
	sceneInts = NULL;
}

sjkHostEntity_t *SJK_NewEntity( const refEntity_t *ent ) {
	if ( !sceneEntities || numSceneEntities >= SJK_MAX_ENTITIES ) {
		return NULL;
	}
	sjkHostEntity_t *out = &sceneEntities[numSceneEntities++];
	memset( out, 0, sizeof( *out ) );
	out->renderfx = ent->renderfx;
	VectorCopy( ent->origin, out->origin );
	for ( int i = 0; i < 3; i++ ) {
		VectorCopy( ent->axis[i], out->axis[i] );
	}
	if ( ent->modelScale[0] || ent->modelScale[1] || ent->modelScale[2] ) {
		VectorCopy( ent->modelScale, out->scale );
	} else {
		VectorSet( out->scale, 1, 1, 1 );
	}
	VectorCopy( ent->oldorigin, out->end );
	out->radius = ent->radius;
	out->rotation = ent->rotation;
	out->shader = ent->customShader;
	out->skin = ent->customSkin;
	out->rgba[0] = ent->shaderRGBA[0];
	out->rgba[1] = ent->shaderRGBA[1];
	out->rgba[2] = ent->shaderRGBA[2];
	out->rgba[3] = ent->shaderRGBA[3];
	out->frame = ent->frame;
	out->oldframe = ent->oldframe;
	out->backlerp = ent->backlerp;
	VectorCopy( ( ent->renderfx & RF_LIGHTING_ORIGIN ) ? ent->lightingOrigin : ent->origin, out->lightingOrigin );
	out->rootSurface = -1;
	return out;
}

float *SJK_NewBones( int count, int *first ) {
	if ( !sceneBones || count <= 0 || numSceneBoneFloats + count * 12 > SJK_MAX_BONE_FLOATS ) {
		return NULL;
	}
	*first = numSceneBoneFloats / 12;
	float *bones = sceneBones + numSceneBoneFloats;
	numSceneBoneFloats += count * 12;
	return bones;
}

int *SJK_NewInts( int count, int *first ) {
	if ( !sceneInts || count <= 0 || numSceneInts + count > SJK_MAX_INTS ) {
		return NULL;
	}
	*first = numSceneInts;
	int *ints = sceneInts + numSceneInts;
	numSceneInts += count;
	return ints;
}

extern void R_SJK_AddGhoul2( const refEntity_t *ent, int refdefTime );	// tr_ghoul2.cpp

// One refEntity_t as rd-vanilla's R_AddEntitySurfaces would draw it.
static void SJK_AddEntity( const refEntity_t *ent, int time ) {
	switch ( ent->reType ) {
	case RT_MODEL: {
		if ( ent->ghoul2 && G2API_HaveWeGhoul2Models( *( (CGhoul2Info_v *)ent->ghoul2 ) ) ) {
			R_SJK_AddGhoul2( ent, time );
			return;
		}
		const model_t *mod = R_GetModelByHandle( ent->hModel );
		if ( !mod || !ent->hModel ) {
			return;
		}
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( !out ) return;
		if ( mod->type == MOD_BRUSH ) {
			out->kind = SJK_ENT_BRUSH;
			out->model = atoi( mod->name + 1 );
		} else {
			out->kind = SJK_ENT_MODEL;
			out->model = ent->hModel;
		}
		return;
	}
	case RT_SPRITE: {
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( out ) out->kind = SJK_ENT_SPRITE;
		return;
	}
	case RT_ORIENTED_QUAD: {
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( out ) out->kind = SJK_ENT_QUAD;
		return;
	}
	case RT_LINE:
	case RT_ORIENTEDLINE:
	case RT_BEAM: {
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( !out ) return;
		out->kind = SJK_ENT_LINE;
		out->radius = ent->reType == RT_BEAM ? (float)ent->frame : ent->data.line.width;
		out->width2 = ent->data.line.width2;
		return;
	}
	case RT_SABER_GLOW: {
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( !out ) return;
		out->kind = SJK_ENT_SABER;
		out->length = ent->saberLength;
		return;
	}
	case RT_CYLINDER: {
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( !out ) return;
		out->kind = SJK_ENT_CYLINDER;
		out->radius = ent->data.cylinder.width;
		out->width2 = ent->data.cylinder.width2;
		out->length = ent->data.cylinder.height;
		return;
	}
	case RT_ELECTRICITY: {
		sjkHostEntity_t *out = SJK_NewEntity( ent );
		if ( !out ) return;
		out->kind = SJK_ENT_ELECTRICITY;
		out->radius = ent->data.electricity.width;
		out->width2 = ent->data.electricity.deviation;
		return;
	}
	default:
		return;
	}
}

void RE_ClearScene( void ) {
	numPendingEntities = 0;
	numPendingLights = 0;
	numPendingPolys = 0;
	numPendingPolyVerts = 0;
	numScenePolys = 0;
}

// Effect decals are projected by SJK; queued until the next main view.
#define SJK_MAX_DECALS	256
static sjkHostDecal_t	pendingDecals[SJK_MAX_DECALS], sceneDecals[SJK_MAX_DECALS];
static int				numPendingDecals, numSceneDecals;

void RE_ClearDecals( void ) {
	numPendingDecals = 0;
}

void RE_AddRefEntityToScene( const refEntity_t *ent ) {
	if ( !ent || !tr.registered || !pendingEntities || numPendingEntities >= SJK_MAX_REFENTITIES ) {
		return;
	}
	pendingEntities[numPendingEntities++] = *ent;
}

void RE_AddMiniRefEntityToScene( const miniRefEntity_t *ent ) {
	// NULL ends an effect's chain of mini entities (rd-vanilla).
	if ( !ent || !tr.registered || !pendingEntities || numPendingEntities >= SJK_MAX_REFENTITIES ) {
		return;
	}
	// A miniRefEntity_t is the start of a refEntity_t.
	refEntity_t *out = &pendingEntities[numPendingEntities++];
	memset( out, 0, sizeof( *out ) );
	memcpy( out, ent, sizeof( *ent ) );
}

void RE_AddPolyToScene( qhandle_t hShader, int numVerts, const polyVert_t *verts, int num ) {
	numScenePolys += num;
	if ( !tr.registered || !pendingPolys || !verts || numVerts < 3 ) {
		return;
	}
	for ( int j = 0; j < num; j++ ) {
		if ( numPendingPolys >= SJK_MAX_POLYS || numPendingPolyVerts + numVerts > SJK_MAX_POLYVERTS ) {
			return;
		}
		sjkHostPoly_t *poly = &pendingPolys[numPendingPolys++];
		poly->shader = hShader;
		poly->firstVertex = numPendingPolyVerts;
		poly->numVertices = numVerts;
		for ( int k = 0; k < numVerts; k++ ) {
			const polyVert_t *in = &verts[j * numVerts + k];
			sjkHostPolyVert_t *out = &pendingPolyVerts[numPendingPolyVerts++];
			VectorCopy( in->xyz, out->xyz );
			out->st[0] = in->st[0];
			out->st[1] = in->st[1];
			memcpy( out->modulate, in->modulate, 4 );
		}
	}
}
void RE_AddDecalToScene( qhandle_t shader, const vec3_t origin, const vec3_t dir, float orientation, float r, float g, float b, float a, qboolean alphaFade, float radius, qboolean temporary ) {
	if ( !tr.registered || !tr.worldMapLoaded || numPendingDecals >= SJK_MAX_DECALS ) {
		return;
	}
	sjkHostDecal_t *decal = &pendingDecals[numPendingDecals++];
	decal->shader = shader;
	VectorCopy( origin, decal->origin );
	VectorCopy( dir, decal->dir );
	decal->orientation = orientation;
	decal->rgba[0] = r;
	decal->rgba[1] = g;
	decal->rgba[2] = b;
	decal->rgba[3] = a;
	decal->alphaFade = alphaFade;
	decal->radius = radius;
	decal->temporary = temporary;
}

static void SJK_AddLight( const vec3_t org, float intensity, float r, float g, float b, int additive ) {
	if ( !tr.registered || numPendingLights >= SJK_MAX_LIGHTS || intensity <= 0 ) {
		return;
	}
	sjkHostLight_t *light = &pendingLights[numPendingLights++];
	VectorCopy( org, light->origin );
	light->radius = intensity;
	light->color[0] = r;
	light->color[1] = g;
	light->color[2] = b;
	light->additive = additive;
}

void RE_AddLightToScene( const vec3_t org, float intensity, float r, float g, float b ) {
	SJK_AddLight( org, intensity, r, g, b, 0 );
}

void RE_AddAdditiveLightToScene( const vec3_t org, float intensity, float r, float g, float b ) {
	SJK_AddLight( org, intensity, r, g, b, 1 );
}

// rd-vanilla's R_LightForPoint, from SJK's light grid: ambient and directed in 0..255.
int R_LightForPoint( vec3_t point, vec3_t ambientLight, vec3_t directedLight, vec3_t lightDir ) {
	float light[3][3];
	if ( !tr.worldMapLoaded || !SJK_HostLightForPoint( point, &light[0][0] ) ) {
		return qfalse;
	}
	VectorScale( light[0], 255.0f, ambientLight );
	VectorScale( light[1], 255.0f, directedLight );
	VectorCopy( light[2], lightDir );
	return qtrue;
}

// Model views without a world (the menus' player model, sabers, cosmetics).
#define SJK_MAX_PREVIEWS	16
static sjkHostPreview_t	previews[SJK_MAX_PREVIEWS];
static int				numPreviews;
static int				firstMainEntity, numMainEntities;

static void SJK_AddPic( float x, float y, float w, float h, float s1, float t1, float s2, float t2, float a, int kind, qhandle_t hShader );

static void SJK_AddPreview( const refdef_t *fd ) {
	if ( numPreviews >= SJK_MAX_PREVIEWS || fd->width <= 0 || fd->height <= 0 || !glConfig.vidWidth || !glConfig.vidHeight ) {
		return;
	}
	sjkHostPreview_t *preview = &previews[numPreviews];
	VectorCopy( fd->vieworg, preview->origin );
	for ( int i = 0; i < 3; i++ ) {
		VectorCopy( fd->viewaxis[i], preview->axis[i] );
	}
	preview->fovX = fd->fov_x;
	preview->fovY = fd->fov_y;
	preview->x = fd->x;
	preview->y = fd->y;
	preview->width = fd->width;
	preview->height = fd->height;
	preview->firstEntity = numSceneEntities;
	for ( int i = 0; i < numPendingEntities; i++ ) {
		SJK_AddEntity( &pendingEntities[i], fd->time );
	}
	preview->numEntities = numSceneEntities - preview->firstEntity;
	if ( !preview->numEntities ) {
		return;
	}
	// Drawn where the 2D is now, in the 640x480 screen the pictures use.
	const float sx = 640.0f / glConfig.vidWidth, sy = 480.0f / glConfig.vidHeight;
	const float color[4] = { drawColor[0], drawColor[1], drawColor[2], drawColor[3] };
	drawColor[0] = drawColor[1] = drawColor[2] = drawColor[3] = 1.0f;
	SJK_AddPic( fd->x * sx, fd->y * sy, fd->width * sx, fd->height * sy, 0, 0, 1, 1, 0, SJK_PIC_VIEW, numPreviews );
	memcpy( drawColor, color, sizeof( color ) );
	numPreviews++;
}

void RE_RenderScene( const refdef_t *fd ) {
	if ( !tr.registered ) {
		return;
	}
	if ( fd->rdflags & RDF_NOWORLDMODEL ) {
		SJK_AddPreview( fd );
		return;
	}
	// The first full view of the world is the frame's main view; model previews in
	// menus (RDF_NOWORLDMODEL) come later.
	// The sky portal's own view (RDF_SKYBOXPORTAL; the main view carries
	// RDF_DRAWSKYBOX) is drawn by the renderer from the map's portal entity; the
	// automap is not drawn.
	if ( frameHasView || ( fd->rdflags & ( RDF_SKYBOXPORTAL | RDF_AUTOMAP ) ) || !tr.worldMapLoaded ) {
		return;
	}
	VectorCopy( fd->vieworg, frameView.origin );
	for ( int i = 0; i < 3; i++ ) {
		VectorCopy( fd->viewaxis[i], frameView.axis[i] );
	}
	frameView.fovX = fd->fov_x;
	frameView.fovY = fd->fov_y;
	frameView.x = fd->x;
	frameView.y = fd->y;
	frameView.width = fd->width;
	frameView.height = fd->height;
	frameView.time = fd->time;
	frameView.rdflags = fd->rdflags;
	frameHasView = qtrue;

	tr.refdef.time = fd->time;
	firstMainEntity = numSceneEntities;
	for ( int i = 0; i < numPendingEntities; i++ ) {
		SJK_AddEntity( &pendingEntities[i], fd->time );
	}
	numMainEntities = numSceneEntities - firstMainEntity;
	memcpy( sceneLights, pendingLights, sizeof( sjkHostLight_t ) * numPendingLights );
	numSceneLights = numPendingLights;
	if ( scenePolys ) {
		memcpy( scenePolys, pendingPolys, sizeof( sjkHostPoly_t ) * numPendingPolys );
		memcpy( scenePolyVerts, pendingPolyVerts, sizeof( sjkHostPolyVert_t ) * numPendingPolyVerts );
		numScenePolyList = numPendingPolys;
		numScenePolyVerts = numPendingPolyVerts;
	}
	memcpy( sceneDecals, pendingDecals, sizeof( sjkHostDecal_t ) * numPendingDecals );
	numSceneDecals = numPendingDecals;
	numPendingDecals = 0;
}

/*
==================
2D
==================
*/

void RE_SetColor( const float *rgba ) {
	if ( rgba ) {
		drawColor[0] = rgba[0];
		drawColor[1] = rgba[1];
		drawColor[2] = rgba[2];
		drawColor[3] = rgba[3];
	} else {
		drawColor[0] = drawColor[1] = drawColor[2] = drawColor[3] = 1.0f;
	}
}

static void SJK_AddPic( float x, float y, float w, float h, float s1, float t1, float s2, float t2, float a, int kind, qhandle_t hShader ) {
	if ( !framePics || numFramePics >= MAX_SJK_PICS ) {
		return;
	}
	if ( kind != SJK_PIC_VIEW && ( hShader < 0 || hShader >= tr.numShaders ) ) {
		hShader = 0;
	}
	sjkHostPic_t *pic = &framePics[numFramePics++];
	pic->shader = hShader;
	pic->x = x; pic->y = y; pic->w = w; pic->h = h;
	pic->s1 = s1; pic->t1 = t1; pic->s2 = s2; pic->t2 = t2;
	pic->color[0] = drawColor[0]; pic->color[1] = drawColor[1];
	pic->color[2] = drawColor[2]; pic->color[3] = drawColor[3];
	pic->angle = a;
	pic->kind = kind;
}

void RE_StretchPic( float x, float y, float w, float h, float s1, float t1, float s2, float t2, qhandle_t hShader ) {
	SJK_AddPic( x, y, w, h, s1, t1, s2, t2, 0.0f, SJK_PIC_STRETCH, hShader );
}

void RE_RotatePic( float x, float y, float w, float h, float s1, float t1, float s2, float t2, float a, qhandle_t hShader ) {
	SJK_AddPic( x, y, w, h, s1, t1, s2, t2, a, SJK_PIC_ROTATE, hShader );
}

void RE_RotatePic2( float x, float y, float w, float h, float s1, float t1, float s2, float t2, float a, qhandle_t hShader ) {
	SJK_AddPic( x, y, w, h, s1, t1, s2, t2, a, SJK_PIC_ROTATE2, hShader );
}

// Cinematics: not drawn yet (a black frame shows instead).
void RE_StretchRaw( int x, int y, int w, int h, int cols, int rows, const byte *data, int client, qboolean dirty ) {}
void RE_UploadCinematic( int cols, int rows, const byte *data, int client, qboolean dirty ) {}

/*
==================
Frames
==================
*/

void RE_BeginFrame( stereoFrame_t stereoFrame ) {
	if ( !tr.registered ) {
		return;
	}
	tr.frameCount++;
	tr.frameSceneNum = 0;
	numFramePics = 0;
	frameHasView = qfalse;
	SJK_PushSettings();
	if ( !framePics ) {
		framePics = (sjkHostPic_t *)Z_Malloc( sizeof( sjkHostPic_t ) * MAX_SJK_PICS, TAG_GENERAL, qfalse );
	}
	SJK_AllocScene();
	numSceneEntities = 0;
	numSceneBoneFloats = 0;
	numSceneInts = 0;
	numSceneLights = 0;
	numScenePolyList = 0;
	numScenePolyVerts = 0;
	numSceneDecals = 0;
	numPreviews = 0;
	firstMainEntity = numMainEntities = 0;
}

void RE_EndFrame( int *frontEndMsec, int *backEndMsec ) {
	if ( !tr.registered ) {
		return;
	}
	SJK_SendAssetNames();
	if ( r_sjkDebug && r_sjkDebug->integer ) {
		ri.Printf( PRINT_ALL, "rd-freedom frame: view %d, %d pictures, %d entities, %d bones, %d lights, %d polys\n",
			frameHasView, numFramePics, numSceneEntities, numSceneBoneFloats / 12, numSceneLights, numScenePolys );
	}
	if ( r_sjkDebug && r_sjkDebug->integer >= 2 && frameHasView ) {
		for ( int i = 0; i < numSceneEntities; i++ ) {
			const sjkHostEntity_t *e = &sceneEntities[i];
			const model_t *mod = ( e->kind == SJK_ENT_MODEL || e->kind == SJK_ENT_GHOUL2 ) ? R_GetModelByHandle( e->model ) : NULL;
			ri.Printf( PRINT_ALL, "  %d: kind %d model %d %s shader %d skin %d bones %d rfx %#x at %.0f %.0f %.0f\n", i, e->kind, e->model,
				mod ? mod->name : "", e->shader, e->skin, e->numBones, e->renderfx, e->origin[0], e->origin[1], e->origin[2] );
		}
		ri.Cvar_Set( "r_freedomDebug", "1" );
	}
	sjkHostScene_t scene;
	scene.entities = sceneEntities;
	scene.numEntities = numSceneEntities;
	scene.firstMainEntity = firstMainEntity;
	scene.numMainEntities = frameHasView ? numMainEntities : 0;
	scene.previews = previews;
	scene.numPreviews = numPreviews;
	scene.bones = sceneBones;
	scene.numBones = numSceneBoneFloats / 12;
	scene.ints = sceneInts;
	scene.numInts = numSceneInts;
	scene.lights = sceneLights;
	scene.numLights = frameHasView ? numSceneLights : 0;
	scene.polys = scenePolys;
	scene.numPolys = frameHasView ? numScenePolyList : 0;
	scene.polyVerts = scenePolyVerts;
	scene.numPolyVerts = frameHasView ? numScenePolyVerts : 0;
	scene.decals = sceneDecals;
	scene.numDecals = frameHasView ? numSceneDecals : 0;
	SJK_HostFrame( frameHasView ? &frameView : NULL, &scene, framePics, numFramePics );
	ri.WIN_Present( &window );
	SJK_FlushLog();
	if ( frontEndMsec ) *frontEndMsec = 0;
	if ( backEndMsec ) *backEndMsec = 0;
	numFramePics = 0;
	frameHasView = qfalse;
}

/*
==================
Queries the game makes of the world
==================
*/

// cgame's marks (scorches, saber burns) are clipped into SJK's copy of the world.
int R_MarkFragments( int numPoints, const vec3_t *points, const vec3_t projection, int maxPoints, vec3_t pointBuffer, int maxFragments, markFragment_t *fragmentBuffer ) {
	if ( !tr.worldMapLoaded ) {
		return 0;
	}
	return SJK_HostMarkFragments( numPoints, (const float *)points, projection, maxPoints, (float *)pointBuffer, maxFragments, (int *)fragmentBuffer );
}

qboolean R_inPVS( const vec3_t p1, const vec3_t p2, byte *mask ) {
	if ( !tr.worldMapLoaded ) {
		return qtrue;
	}
	return SJK_HostInPVS( p1, p2 ) ? qtrue : qfalse;
}

void RE_GetLightStyle( int style, color4ub_t color ) {
	if ( style < 0 || style >= MAX_LIGHT_STYLES ) {
		color[0] = color[1] = color[2] = color[3] = 255;
		return;
	}
	byteAlias_t *ba = (byteAlias_t *)&lightStyles[style];
	byteAlias_t *out = (byteAlias_t *)color;
	out->i = ba->i;
}

void RE_SetLightStyle( int style, int color ) {
	if ( style >= 0 && style < MAX_LIGHT_STYLES ) {
		byteAlias_t *ba = (byteAlias_t *)&lightStyles[style];
		if ( ba->i != color ) {
			ba->i = color;
		}
	}
}

void RE_GetBModelVerts( int bmodelIndex, vec3_t *verts, vec3_t normal ) {
	for ( int i = 0; i < 4; i++ ) VectorClear( verts[i] );
	VectorSet( normal, 0, 0, 1 );
}

void RE_RemapShader( const char *oldShader, const char *newShader, const char *timeOffset ) {
	SJK_HostSetCvar( "__remap", va( "%s %s %s", oldShader, newShader, timeOffset ? timeOffset : "" ) );
}

void RE_SetRangedFog( float range ) {}
void RE_SetRefractionProperties( float distortionAlpha, float distortionStretch, qboolean distortionPrePost, qboolean distortionNegate ) {}
float RE_GetDistanceCull( void ) { return 6000.0f; }
void RE_GetRealRes( int *w, int *h ) { *w = glConfig.vidWidth; *h = glConfig.vidHeight; }
void R_AutomapElevationAdjustment( float newHeight ) {}
qboolean R_InitializeWireframeAutomap( void ) { return qfalse; }
void RE_AddWeatherZone( vec3_t mins, vec3_t maxs ) {}
void RE_WorldEffectCommand( const char *command ) {}
qboolean RE_RegisterImages_LevelLoadEnd( void ) { return qfalse; }
void RE_TakeVideoFrame( int width, int height, byte *captureBuffer, byte *encodeBuffer, qboolean motionJpeg ) {}

/*
==================
Shutdown
==================
*/

void SJK_Shutdown( qboolean destroyWindow, qboolean restarting ) {
	// A restart (vid_restart) unloads this DLL even when it keeps the window, and
	// nothing of the renderer may outlive its code: stop it all and let the next
	// instance make the window again.
	if ( restarting ) {
		destroyWindow = qtrue;
	}
	if ( entityString ) {
		Z_Free( entityString );
		entityString = NULL;
	}
	entityParsePoint = NULL;
	tr.worldMapLoaded = qfalse;
	worldName[0] = 0;
	R_ShutdownFonts();
	if ( hostStarted ) {
		if ( destroyWindow ) {
			SJK_Trace( "shutdown: renderer" );
			SJK_HostShutdown();
			SJK_Trace( "shutdown: renderer stopped" );
			SJK_RemoveCrashLog();
			ri.Cmd_RemoveCommand( "graphicsquality" );
			numSettings = 0;
			memset( forwarded, 0, sizeof( forwarded ) );
			hostStarted = qfalse;
		} else {
			SJK_HostUnloadWorld();
		}
		SJK_FlushLog();
	}
	if ( framePics ) {
		Z_Free( framePics );
		framePics = NULL;
	}
	SJK_FreeScene();
	numPendingEntities = 0;
	numPendingLights = 0;
	if ( destroyWindow ) {
		SJK_Trace( "shutdown: window" );
		ri.WIN_Shutdown();
		memset( &glConfig, 0, sizeof( glConfig ) );
		memset( &window, 0, sizeof( window ) );
		SJK_Trace( "shutdown: done" );
	}
	shadersSent = 0;
	modelsSent = 0;
	skinsSent = 0;
}
