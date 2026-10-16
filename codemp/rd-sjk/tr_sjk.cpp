/*
===========================================================================
rd-sjk: the engine-facing half of SJK's renderer for JoF EJK.

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
	Com_sprintf( config, sizeof( config ), "%s/" ETERNALJKGAME "/rd-sjk/config.cfg", home[0] ? home : base );
	Com_sprintf( sjkConfig, sizeof( sjkConfig ), "%s/SJK/config.cfg", base );

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

	hostStarted = (qboolean)( SJK_HostInit( &start ) != 0 );
	SJK_FlushLog();
	if ( !hostStarted ) {
		ri.Error( ERR_FATAL, "rd-sjk: SJK's renderer could not start on this window (see the console)" );
	}
	static char adapter[256];
	SJK_HostAdapter( adapter, sizeof( adapter ) );
	glConfig.renderer_string = adapter;
	shadersSent = 0;
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
		ri.Error( ERR_FATAL, "rd-sjk: the engine gave no native window" );
	}
	glConfig.vendor_string = "SJK";
	glConfig.renderer_string = "SJK renderer";
	glConfig.version_string = "rd-sjk";
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

void RE_BeginRegistration( glconfig_t *glconfigOut ) {
	R_Init();
	R_InitShaders( qfalse );
	R_InitSkins();
	R_SJK_ResetShaderExistence();
	r_sjkDebug = ri.Cvar_Get( "r_sjkDebug", "0", CVAR_NONE, "Log rd-sjk frame details" );
	r_aspectCorrectFonts = ri.Cvar_Get( "r_aspectCorrectFonts", "0", CVAR_ARCHIVE, "" );
	r_ratioFix = ri.Cvar_Get( "cl_ratioFix", "1", CVAR_ARCHIVE, "" );
	r_fontOverride = ri.Cvar_Get( "r_fontOverride", "0", CVAR_NONE, "" );
	extern cvar_t *se_language;
	se_language = ri.Cvar_Get( "se_language", "english", CVAR_ARCHIVE | CVAR_NORESTART, "" );
	SJK_InitWindow();
	R_Set2DRatio();
	SJK_StartHost();

	// The shaders 2D code uses before any are registered.
	int lightmaps2d[MAXLIGHTMAPS] = { LIGHTMAP_2D, LIGHTMAP_2D, LIGHTMAP_2D, LIGHTMAP_2D };
	byte styles[MAXLIGHTMAPS] = { LS_LSNONE, LS_LSNONE, LS_LSNONE, LS_LSNONE };
	tr.defaultShader = R_FindShader( "<default>", lightmaps2d, styles, qtrue );
	tr.whiteShader = R_FindShader( "white", lightmaps2d, styles, qtrue );

	R_InitFonts();

	*glconfigOut = glConfig;
	tr.registered = qtrue;
	tr.viewCluster = -1;
	numFramePics = 0;
	frameHasView = qfalse;
	SJK_SendShaderNames();
}

void RE_EndRegistration( void ) {
	SJK_SendShaderNames();
	SJK_FlushLog();
}

// The world's entity lump, read straight from the BSP for cgame's spawn parsing.
static void SJK_ReadEntities( const char *name ) {
	if ( entityString ) {
		Z_Free( entityString );
		entityString = NULL;
	}
	entityParsePoint = NULL;
	void *buffer = NULL;
	const int length = ri.FS_ReadFile( name, &buffer );
	if ( length <= 0 || !buffer ) {
		ri.Error( ERR_DROP, "rd-sjk: %s not found", name );
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
	ri.FS_FreeFile( buffer );
}

void RE_LoadWorldMap( const char *name ) {
	if ( tr.worldMapLoaded ) {
		ri.Error( ERR_DROP, "ERROR: attempted to redundantly load world map" );
	}
	SJK_ReadEntities( name );
	Q_strncpyz( worldName, name, sizeof( worldName ) );
	tr.worldMapLoaded = qtrue;
	SJK_SendShaderNames();
	const int loaded = SJK_HostLoadWorld( name );
	SJK_FlushLog();
	if ( !loaded ) {
		ri.Printf( PRINT_WARNING, "rd-sjk: SJK could not build %s; it shows no world\n", name );
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
Scene
==================
*/

static int numSceneEntities, numSceneLights, numScenePolys;

void RE_ClearScene( void ) {
	numSceneEntities = 0;
	numSceneLights = 0;
	numScenePolys = 0;
}

void RE_ClearDecals( void ) {}
void RE_AddRefEntityToScene( const refEntity_t *ent ) { numSceneEntities++; }
void RE_AddMiniRefEntityToScene( const miniRefEntity_t *ent ) { numSceneEntities++; }
void RE_AddPolyToScene( qhandle_t hShader, int numVerts, const polyVert_t *verts, int num ) { numScenePolys += num; }
void RE_AddDecalToScene( qhandle_t shader, const vec3_t origin, const vec3_t dir, float orientation, float r, float g, float b, float a, qboolean alphaFade, float radius, qboolean temporary ) {}
void RE_AddLightToScene( const vec3_t org, float intensity, float r, float g, float b ) { numSceneLights++; }
void RE_AddAdditiveLightToScene( const vec3_t org, float intensity, float r, float g, float b ) { numSceneLights++; }

int R_LightForPoint( vec3_t point, vec3_t ambientLight, vec3_t directedLight, vec3_t lightDir ) {
	VectorSet( ambientLight, 96, 96, 96 );
	VectorSet( directedLight, 128, 128, 128 );
	VectorSet( lightDir, 0.57735f, 0.57735f, 0.57735f );
	return qtrue;
}

void RE_RenderScene( const refdef_t *fd ) {
	if ( !tr.registered ) {
		return;
	}
	// The first full view of the world is the frame's main view; model previews in
	// menus (RDF_NOWORLDMODEL) come later.
	if ( frameHasView || ( fd->rdflags & RDF_NOWORLDMODEL ) || !tr.worldMapLoaded ) {
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
	if ( hShader < 0 || hShader >= tr.numShaders ) {
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
	if ( !framePics ) {
		framePics = (sjkHostPic_t *)Z_Malloc( sizeof( sjkHostPic_t ) * MAX_SJK_PICS, TAG_GENERAL, qfalse );
	}
}

void RE_EndFrame( int *frontEndMsec, int *backEndMsec ) {
	if ( !tr.registered ) {
		return;
	}
	SJK_SendShaderNames();
	if ( r_sjkDebug && r_sjkDebug->integer ) {
		ri.Printf( PRINT_ALL, "rd-sjk frame: view %d, %d pictures, %d entities, %d lights, %d polys\n",
			frameHasView, numFramePics, numSceneEntities, numSceneLights, numScenePolys );
	}
	SJK_HostFrame( frameHasView ? &frameView : NULL, framePics, numFramePics );
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

int R_MarkFragments( int numPoints, const vec3_t *points, const vec3_t projection, int maxPoints, vec3_t pointBuffer, int maxFragments, markFragment_t *fragmentBuffer ) {
	return 0;
}

qboolean R_inPVS( const vec3_t p1, const vec3_t p2, byte *mask ) {
	return qtrue;
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
			SJK_HostShutdown();
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
	if ( destroyWindow ) {
		ri.WIN_Shutdown();
		memset( &glConfig, 0, sizeof( glConfig ) );
		memset( &window, 0, sizeof( window ) );
	}
	shadersSent = 0;
}
