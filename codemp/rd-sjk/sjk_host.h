/*
===========================================================================
rd-sjk: the C interface of SJK's hosted renderer (lib/sjk, crates/sjk-viewer/src/host/ffi.rs).

SJK is (C) its authors, GPL-2.0-only (github.com/Sol-Vulpes/SJK, see its NOTICE).
Every call is synchronous and runs SJK's work on its own thread.
===========================================================================
*/

#pragma once

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct sjkHostStart_s {
	intptr_t hwnd;
	intptr_t hinstance;
	int width;
	int height;
	const char *basePath;	// fs_basepath (GameData)
	const char *homePath;	// fs_homepath
	const char *game;		// fs_game, "" for base
	const char *config;		// the hosted renderer's settings file
	const char *sjkConfig;	// SJK's own profile, read once to seed a new one ("" for none)
} sjkHostStart_t;

typedef struct sjkHostView_s {
	float origin[3];
	float axis[3][3];		// forward, left, up
	float fovX, fovY;
	int x, y, width, height;
	int time;				// refdef.time, msec
	int rdflags;
} sjkHostView_t;

#define SJK_PIC_STRETCH	0
#define SJK_PIC_ROTATE	1	// RE_RotatePic
#define SJK_PIC_ROTATE2	2	// RE_RotatePic2: x, y is the centre

typedef struct sjkHostPic_s {
	int shader;
	float x, y, w, h;		// 640x480 virtual screen
	float s1, t1, s2, t2;
	float color[4];
	float angle;
	int kind;
} sjkHostPic_t;

// Entities, one per drawn model (each model of a Ghoul2 entity is its own entry).
#define SJK_ENT_MODEL		0	// MD3 or rigid GLM: model handle
#define SJK_ENT_GHOUL2		1	// one Ghoul2 model posed by bones[] (EvalRender matrices)
#define SJK_ENT_BRUSH		2	// inline BSP model *N: model = N
#define SJK_ENT_SPRITE		3	// RT_SPRITE: origin, radius, rotation
#define SJK_ENT_QUAD		4	// RT_ORIENTED_QUAD: origin, axis[0] normal, radius, rotation
#define SJK_ENT_LINE		5	// RT_LINE / RT_ORIENTEDLINE / RT_BEAM: origin -> end, radius = width
#define SJK_ENT_SABER		6	// RT_SABER_GLOW: origin, axis[0], length, radius, shader
#define SJK_ENT_CYLINDER	7	// RT_CYLINDER: origin, axis[0], length, radius, width2
#define SJK_ENT_ELECTRICITY	8	// RT_ELECTRICITY: origin -> end, radius = width

typedef struct sjkHostEntity_s {
	int kind;
	int model;			// model handle, or the inline model number for SJK_ENT_BRUSH
	int shader;			// custom shader handle, 0 for none
	int skin;			// skin handle, 0 for the model's own shaders
	int renderfx;
	int key;			// SJK_ENT_GHOUL2: the same for one model instance frame to frame
	float origin[3];
	float axis[3][3];
	float scale[3];
	float end[3];
	float radius, rotation, length, width2;
	unsigned char rgba[4];
	int frame, oldframe;
	float backlerp;
	float lightingOrigin[3];
	int firstBone, numBones;			// 12 floats per bone in sjkHostScene_t.bones
	int firstSurface, numSurfaces;		// (surface index, G2 flags) pairs in sjkHostScene_t.ints
	int rootSurface;					// G2 mSurfaceRoot
} sjkHostEntity_t;

typedef struct sjkHostLight_s {
	float origin[3];
	float radius;
	float color[3];
	int additive;
} sjkHostLight_t;

typedef struct sjkHostPolyVert_s {
	float xyz[3];
	float st[2];
	unsigned char modulate[4];
} sjkHostPolyVert_t;

typedef struct sjkHostPoly_s {
	int shader;
	int firstVertex;
	int numVertices;
} sjkHostPoly_t;

typedef struct sjkHostDecal_s {		// RE_AddDecalToScene
	int shader;
	float origin[3];
	float dir[3];
	float orientation;
	float rgba[4];
	int alphaFade;
	float radius;
	int temporary;
} sjkHostDecal_t;

typedef struct sjkHostScene_s {
	const sjkHostEntity_t *entities;
	int numEntities;
	const float *bones;
	int numBones;
	const int *ints;
	int numInts;
	const sjkHostLight_t *lights;
	int numLights;
	const sjkHostPoly_t *polys;
	int numPolys;
	const sjkHostPolyVert_t *polyVerts;
	int numPolyVerts;
	const sjkHostDecal_t *decals;
	int numDecals;
} sjkHostScene_t;

int		SJK_HostInit( const sjkHostStart_t *start );
void	SJK_HostShutdown( void );
int		SJK_HostLoadWorld( const char *name );
void	SJK_HostUnloadWorld( void );
void	SJK_HostShaderName( int handle, const char *name );
void	SJK_HostResize( int width, int height );
void	SJK_HostFrame( const sjkHostView_t *view, const sjkHostScene_t *scene, const sjkHostPic_t *pics, int count );
void	SJK_HostModelName( int handle, const char *name );
void	SJK_HostSkin( int handle, const char *surfaces );	// "surface,shader" lines
int		SJK_HostPollLog( char *buffer, int size );
void	SJK_HostAdapter( char *buffer, int size );
int		SJK_HostSetCvar( const char *name, const char *value );
int		SJK_HostShaderExists( const char *name );	// a script or an image by that name

// World queries, answered from the loaded map (0 / nothing without one).
int		SJK_HostMarkFragments( int numPoints, const float *points, const float *projection, int maxPoints, float *pointBuffer, int maxFragments, int *fragmentBuffer );
int		SJK_HostLightForPoint( const float *point, float *ambientDirectedDirection );	// 9 floats, 0..1
int		SJK_HostInPVS( const float *a, const float *b );

#ifdef __cplusplus
}
#endif
