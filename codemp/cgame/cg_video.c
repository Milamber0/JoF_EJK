/*
Fullscreen cutscene videos.

The engine decodes and times the video (client/cl_video.cpp); this file decides when
one plays and draws it over the whole screen. A video is started either by the server:

	jof_video play <serial> <path> <flags>		flags: 1 = the player may skip it
	jof_video stop <serial>

which is answered with "videodone <serial> <end|skip|stopped|error|unsupported>" once
it is over, so the server can hand control back - or locally with the "cutscene"
console command, which the server never hears about.
*/

#include "cg_local.h"
#include "cg_video.h"
#include "ui/keycodes.h"
#include "ui/menudef.h"

#define CG_VIDEO_FLAG_SKIPPABLE	1
#define CG_VIDEO_HINT_TIME		4000

typedef struct {
	qboolean		active;
	qboolean		fromServer;
	unsigned int	serial;
	qboolean		skippable;
	int				startTime;
} cgVideo_t;

static cgVideo_t s_video;

// Engines older than the video extension end the import table before trap->ext.Video_*,
// so the cvar they never registered is the only safe way to tell.
static qboolean CG_VideoSupported( void ) {
	char value[8];

	trap->Cvar_VariableStringBuffer( "cl_video", value, sizeof( value ) );
	return ( atoi( value ) > 0 && trap->ext.Video_Play ) ? qtrue : qfalse;
}

static void CG_VideoReply( unsigned int serial, const char *reason ) {
	trap->SendClientCommand( va( "videodone %u %s", serial, reason ) );
}

static void CG_VideoFinish( const char *reason ) {
	cgVideo_t finished = s_video;

	if ( !s_video.active ) {
		return;
	}
	memset( &s_video, 0, sizeof( s_video ) );
	trap->ext.Video_Stop();

	// Set directly rather than through CG_EventHandling: this may be running inside it.
	if ( cgs.eventHandling == CGAME_EVENT_VIDEO ) {
		cgs.eventHandling = CGAME_EVENT_NONE;
		trap->Key_SetCatcher( trap->Key_GetCatcher() & ~KEYCATCH_CGAME );
	}
	if ( finished.fromServer ) {
		CG_VideoReply( finished.serial, reason );
	}

	// The engine stopped the level music for the video.
	CG_StartMusic( qtrue );
}

static void CG_VideoStart( const char *path, qboolean fromServer, unsigned int serial, qboolean skippable ) {
	if ( s_video.active ) {
		CG_VideoFinish( "stopped" );
	}

	if ( !CG_VideoSupported() ) {
		if ( fromServer ) {
			CG_VideoReply( serial, "unsupported" );
		} else {
			trap->Print( "This client cannot play videos (cl_video is 0)\n" );
		}
		return;
	}
	if ( !trap->ext.Video_Play( path ) ) {
		if ( fromServer ) {
			CG_VideoReply( serial, "error" );
		}
		return;
	}

	s_video.active = qtrue;
	s_video.fromServer = fromServer;
	s_video.serial = serial;
	s_video.skippable = skippable;
	s_video.startTime = trap->Milliseconds();

	if ( skippable ) {
		CG_EventHandling( CGAME_EVENT_VIDEO );
		trap->Key_SetCatcher( trap->Key_GetCatcher() | KEYCATCH_CGAME );
	}
}

void CG_VideoReset( void ) {
	memset( &s_video, 0, sizeof( s_video ) );
}

qboolean CG_VideoIsActive( void ) {
	return s_video.active;
}

void CG_VideoServerCommand( void ) {
	char			action[16];
	char			path[MAX_QPATH];
	unsigned int	serial;
	int				flags;

	// CG_Argv shares one buffer between calls: copy each argument out before the next.
	Q_strncpyz( action, CG_Argv( 1 ), sizeof( action ) );
	serial = (unsigned int)strtoul( CG_Argv( 2 ), NULL, 10 );

	if ( !Q_stricmp( action, "play" ) ) {
		Q_strncpyz( path, CG_Argv( 3 ), sizeof( path ) );
		flags = atoi( CG_Argv( 4 ) );
		CG_VideoStart( path, qtrue, serial, ( flags & CG_VIDEO_FLAG_SKIPPABLE ) ? qtrue : qfalse );
	} else if ( !Q_stricmp( action, "stop" ) ) {
		if ( s_video.active && s_video.fromServer && s_video.serial == serial ) {
			CG_VideoFinish( "stopped" );
		}
	}
}

// "cutscene <path>" plays a video locally, "cutscene stop" ends it. Handy for checking an encode.
void CG_Cutscene_f( void ) {
	char path[MAX_QPATH];

	if ( trap->Cmd_Argc() < 2 ) {
		trap->Print( "usage: cutscene <video path, e.g. video/intro.mp4> | stop\n" );
		return;
	}
	Q_strncpyz( path, CG_Argv( 1 ), sizeof( path ) );
	if ( !Q_stricmp( path, "stop" ) ) {
		CG_VideoFinish( "stopped" );
		return;
	}
	CG_VideoStart( path, qfalse, 0, qtrue );
}

void CG_VideoSkip( void ) {
	if ( s_video.active && s_video.skippable ) {
		CG_VideoFinish( "skip" );
	}
}

// Only a skippable video takes the keyboard; otherwise keys go wherever they would have.
qboolean CG_VideoKeyEvent( int key ) {
	if ( !s_video.active || !s_video.skippable ) {
		return qfalse;
	}
	switch ( key ) {
	case A_SPACE:
	case A_ENTER:
	case A_KP_ENTER:
	case A_ESCAPE:
	case A_MOUSE1:
		CG_VideoSkip();
		break;
	default:
		break;
	}
	return qtrue;	// the player is watching: nothing else gets the key
}

void CG_VideoDraw( void ) {
	static vec4_t	black = { 0.0f, 0.0f, 0.0f, 1.0f };
	videoStatus_t	status;
	int				videoWidth, videoHeight;
	float			screenAspect, videoAspect, w, h;

	if ( !s_video.active ) {
		return;
	}

	status = trap->ext.Video_Status();
	if ( status == VIDEOSTATUS_FINISHED ) {
		CG_VideoFinish( "end" );
		return;
	}
	if ( status != VIDEOSTATUS_PLAYING ) {
		CG_VideoFinish( "error" );
		return;
	}

	CG_FillRect( 0, 0, SCREEN_WIDTH, SCREEN_HEIGHT, black );

	// Letterbox to the real window shape; the 640x480 virtual screen stretches with it.
	trap->ext.Video_GetSize( &videoWidth, &videoHeight );
	if ( videoWidth > 0 && videoHeight > 0 && cgs.glconfig.vidWidth > 0 && cgs.glconfig.vidHeight > 0 ) {
		screenAspect = (float)cgs.glconfig.vidWidth / cgs.glconfig.vidHeight;
		videoAspect = (float)videoWidth / videoHeight;
		if ( videoAspect > screenAspect ) {
			w = SCREEN_WIDTH;
			h = SCREEN_HEIGHT * screenAspect / videoAspect;
		} else {
			w = SCREEN_WIDTH * videoAspect / screenAspect;
			h = SCREEN_HEIGHT;
		}
		trap->ext.Video_Draw( ( SCREEN_WIDTH - w ) * 0.5f, ( SCREEN_HEIGHT - h ) * 0.5f, w, h );
	}

	if ( s_video.skippable ) {
		const int elapsed = trap->Milliseconds() - s_video.startTime;

		if ( elapsed < CG_VIDEO_HINT_TIME ) {
			const char	*hint = "Press SPACE to skip";
			const float	scale = 0.5f;
			vec4_t		color = { 1.0f, 1.0f, 1.0f, 0.7f };

			color[3] *= Com_Clamp( 0.0f, 1.0f, ( CG_VIDEO_HINT_TIME - elapsed ) / 1000.0f );
			CG_Text_Paint( SCREEN_WIDTH - 12 - CG_Text_Width( hint, scale, FONT_SMALL ),
				SCREEN_HEIGHT - 12 - CG_Text_Height( hint, scale, FONT_SMALL ),
				scale, color, hint, 0, 0, ITEM_TEXTSTYLE_SHADOWED, FONT_SMALL );
		}
	}
}
