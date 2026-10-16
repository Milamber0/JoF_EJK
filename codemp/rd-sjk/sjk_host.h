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

int		SJK_HostInit( const sjkHostStart_t *start );
void	SJK_HostShutdown( void );
int		SJK_HostLoadWorld( const char *name );
void	SJK_HostUnloadWorld( void );
void	SJK_HostShaderName( int handle, const char *name );
void	SJK_HostResize( int width, int height );
void	SJK_HostFrame( const sjkHostView_t *view, const sjkHostPic_t *pics, int count );
int		SJK_HostPollLog( char *buffer, int size );
void	SJK_HostAdapter( char *buffer, int size );
int		SJK_HostSetCvar( const char *name, const char *value );
int		SJK_HostShaderExists( const char *name );	// a script or an image by that name

#ifdef __cplusplus
}
#endif
