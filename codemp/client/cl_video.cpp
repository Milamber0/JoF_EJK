/*
===========================================================================
Copyright (C) 2026 JoF contributors

Fullscreen video playback (MP4 and friends) for in-game cutscenes.

The stock engine only plays RoQ, and its "cinematic" command flips the client into
CA_CINEMATIC, which stops it talking to the server. This plays a modern video file
while the client stays connected: cgame asks for it through the import table and
draws the picture itself, so the server keeps simulating and nobody times out.

Timing: the video runs on a wall clock started when the first picture is ready.
Each client frame presents the newest decoded picture that is due, and keeps the
raw sound buffer fed a little ahead of that clock. Audio is scheduled against the
same clock rather than against the mixer, so a muted or unfocused window (where
S_RawSamples drops everything) cannot make the decoder run away.

Drawing: every renderer's DrawStretchRaw (including the prebuilt Vulkan one) only
takes power-of-two images. Rather than resample, each picture is copied into the
top-left of a power-of-two texture padded with black, and the quad is stretched so
the padding falls to the right of and below the requested rectangle. cgame draws
over black anyway, so the padding never shows.
===========================================================================
*/

#include "client.h"
#include "snd_local.h"
#include "cl_video.h"

extern int	s_soundStarted;
extern int	s_soundtime;

// cl_cin.cpp hands out scratch images from 0 upwards for RoQ handles; take the last one.
#define VIDEO_SCRATCH_IMAGE		15

#define VIDEO_MAX_FILE_SIZE		(1024 * 1024 * 1024)
#define VIDEO_AUDIO_LEAD		0.2		// seconds of sound queued ahead of the picture
#define VIDEO_AUDIO_LATE		0.1		// sound older than this is dropped, not played late
#define VIDEO_MAX_CATCHUP		16		// pictures decoded per client frame when behind
#define VIDEO_MAX_DIMENSION		2048	// padded texture limit: 1080p pads to 2048x2048 already

static cvar_t	*cl_video;
static cvar_t	*cl_videoBackend;
static cvar_t	*cl_videoVolume;

typedef VideoDecoder *( *videoCreate_t )( void );

static struct {
	videoCreate_t	create;
	void			( *shutdown )( void );
	const char		*name;
} backend;

static struct {
	VideoDecoder	*dec;
	videoStatus_t	status;
	char			name[MAX_QPATH];
	int				startTime;			// Sys_Milliseconds() at clock zero

	// picture on screen, in the top-left corner of a power-of-two texture
	byte			*frame;
	int				width, height;		// the picture
	int				texWidth, texHeight;	// the texture around it
	qboolean		dirty;				// needs uploading
	double			lastPts;

	// picture decoded but not due yet
	qboolean		pending;
	double			pendingPts;
	qboolean		videoDone;

	// sound
	qboolean		audio;
	const short		*pcm;
	int				pcmFrames;
	int				pcmPos;
	double			pcmPts;
	double			audioEnd;			// clock time at which queued sound runs out
	qboolean		audioDone;
} vid;

static double CL_VideoClock( void ) {
	return ( Sys_Milliseconds() - vid.startTime ) / 1000.0;
}

static void CL_VideoRelease( void ) {
	delete vid.dec;
	free( vid.frame );
	memset( &vid, 0, sizeof( vid ) );
}

static int CL_VideoPowerOfTwo( int n ) {
	int pot = 1;

	while ( pot < n ) {
		pot <<= 1;
	}
	return pot;
}

// Copies the decoder's newest picture into our own texture, so the decoder is free to
// work on the next one while this one stays on screen.
static qboolean CL_VideoTakePending( void ) {
	int			w, h, y;
	const byte	*src = vid.dec->Frame( &w, &h );

	if ( !src || w <= 0 || h <= 0 ) {
		return qfalse;
	}
	if ( w > VIDEO_MAX_DIMENSION || h > VIDEO_MAX_DIMENSION ) {
		Com_Printf( S_COLOR_YELLOW "Video is %ix%i: cutscenes are limited to %ix%i (1920x1080 or less recommended)\n",
			w, h, VIDEO_MAX_DIMENSION, VIDEO_MAX_DIMENSION );
		return qfalse;
	}

	// A new size gets a fresh, black texture; the padding is never written again after that.
	if ( w != vid.width || h != vid.height || !vid.frame ) {
		const int texWidth = CL_VideoPowerOfTwo( w );
		const int texHeight = CL_VideoPowerOfTwo( h );

		free( vid.frame );
		vid.frame = (byte *)calloc( texWidth * texHeight, 4 );
		if ( !vid.frame ) {
			return qfalse;
		}
		vid.width = w;
		vid.height = h;
		vid.texWidth = texWidth;
		vid.texHeight = texHeight;
	}
	for ( y = 0; y < h; y++ ) {
		memcpy( vid.frame + y * vid.texWidth * 4, src + y * w * 4, w * 4 );
	}
	vid.lastPts = vid.pendingPts;
	vid.dirty = qtrue;
	vid.pending = qfalse;
	return qtrue;
}

static void CL_VideoDecodeNext( void ) {
	vid.pending = vid.dec->NextVideoFrame( &vid.pendingPts ) ? qtrue : qfalse;
	if ( !vid.pending ) {
		vid.videoDone = qtrue;
	}
}

static void CL_VideoFeedAudio( double now ) {
	const double target = now + VIDEO_AUDIO_LEAD;
	const int rate = vid.dec->AudioRate();
	const int channels = vid.dec->AudioChannels();
	const float gain = s_volume->value * Com_Clamp( 0.0f, 2.0f, cl_videoVolume->value );
	int guard = 0;

	while ( !vid.audioDone && guard++ < 256 ) {
		double start, end;
		int remaining, count;

		if ( vid.pcmPos >= vid.pcmFrames ) {
			if ( !vid.dec->NextAudio( &vid.pcm, &vid.pcmFrames, &vid.pcmPts ) ) {
				vid.audioDone = qtrue;
				break;
			}
			vid.pcmPos = 0;
			continue;
		}

		remaining = vid.pcmFrames - vid.pcmPos;
		start = vid.pcmPts + (double)vid.pcmPos / rate;
		end = start + (double)remaining / rate;
		if ( start >= target ) {
			break;
		}

		// We fell behind (a hitch longer than the lead): skip what should already have played.
		if ( end < now - VIDEO_AUDIO_LATE ) {
			vid.pcmPos = vid.pcmFrames;
			continue;
		}

		count = remaining;
		if ( end > target ) {
			count = (int)( ( target - start ) * rate ) + 1;
			if ( count > remaining ) {
				count = remaining;
			}
		}
		S_RawSamples( count, rate, 2, channels, (const byte *)( vid.pcm + vid.pcmPos * channels ), gain, 1 );
		vid.pcmPos += count;
		vid.audioEnd = start + (double)count / rate;
	}
}

/*
==================
CL_VideoPlay

Loads and starts a video. Returns qfalse if it cannot be played, having said why.
==================
*/
qboolean CL_VideoPlay( const char *name ) {
	char			path[MAX_QPATH];
	fileHandle_t	f;
	long			len;
	byte			*data;

	CL_VideoStop();

	if ( !backend.create ) {
		Com_Printf( S_COLOR_YELLOW "Video playback is not available in this build\n" );
		return qfalse;
	}
	if ( !name || !name[0] || strstr( name, ".." ) || name[0] == '/' || name[0] == '\\' || strchr( name, ':' ) ) {
		Com_Printf( S_COLOR_YELLOW "Refusing to play video \"%s\"\n", name ? name : "" );
		return qfalse;
	}

	Q_strncpyz( path, name, sizeof( path ) );
	COM_DefaultExtension( path, sizeof( path ), ".mp4" );

	len = FS_FOpenFileRead( path, &f, qtrue );
	if ( !f || len <= 0 ) {
		Com_Printf( S_COLOR_YELLOW "Video \"%s\" not found\n", path );
		if ( f ) {
			FS_FCloseFile( f );
		}
		return qfalse;
	}
	if ( len > VIDEO_MAX_FILE_SIZE ) {
		Com_Printf( S_COLOR_YELLOW "Video \"%s\" is too large (%ld bytes)\n", path, len );
		FS_FCloseFile( f );
		return qfalse;
	}

	vid.dec = backend.create();
	data = vid.dec ? vid.dec->AllocData( (int)len ) : NULL;
	if ( !data ) {
		Com_Printf( S_COLOR_YELLOW "Out of memory loading video \"%s\"\n", path );
		FS_FCloseFile( f );
		CL_VideoRelease();
		return qfalse;
	}
	if ( FS_Read( data, (int)len, f ) != len ) {
		Com_Printf( S_COLOR_YELLOW "Could not read video \"%s\"\n", path );
		FS_FCloseFile( f );
		CL_VideoRelease();
		return qfalse;
	}
	FS_FCloseFile( f );

	if ( !vid.dec->Open( path ) ) {
		Com_Printf( S_COLOR_YELLOW "Could not decode video \"%s\" (%s)\n", path, backend.name );
		CL_VideoRelease();
		return qfalse;
	}

	vid.audio = ( vid.dec->HasAudio() && s_soundStarted ) ? qtrue : qfalse;
	if ( !vid.audio ) {
		vid.dec->DisableAudio();
	}
	vid.audioDone = vid.audio ? qfalse : qtrue;

	// The first picture has to decode before we commit, or a broken file would show black forever.
	CL_VideoDecodeNext();
	if ( !vid.pending || !CL_VideoTakePending() ) {
		Com_Printf( S_COLOR_YELLOW "Video \"%s\" has no playable picture\n", path );
		CL_VideoRelease();
		return qfalse;
	}
	CL_VideoDecodeNext();

	Q_strncpyz( vid.name, path, sizeof( vid.name ) );
	vid.status = VIDEOSTATUS_PLAYING;

	// A cutscene silences the level music (cgame restarts it afterwards), and its own
	// soundtrack must not queue behind what is left of the music in the raw buffer.
	S_StopBackgroundTrack();
	if ( vid.audio ) {
		s_rawend = s_soundtime;
	}
	vid.startTime = Sys_Milliseconds() - (int)( vid.lastPts * 1000.0 );
	return qtrue;
}

void CL_VideoStop( void ) {
	CL_VideoRelease();
}

videoStatus_t CL_VideoStatus( void ) {
	return vid.status;
}

void CL_VideoGetSize( int *width, int *height ) {
	*width = vid.width;
	*height = vid.height;
}

/*
==================
CL_VideoDraw

Draws the current picture into a rectangle in 640x480 virtual coordinates.
The black padding of the texture spills past the right and bottom edges.
==================
*/
void CL_VideoDraw( float x, float y, float w, float h ) {
	float texW, texH;

	if ( vid.status != VIDEOSTATUS_PLAYING || !vid.frame || !re ) {
		return;
	}
	texW = w * vid.texWidth / vid.width;
	texH = h * vid.texHeight / vid.height;
	re->DrawStretchRaw( (int)( x + 0.5f ), (int)( y + 0.5f ), (int)( texW + 0.5f ), (int)( texH + 0.5f ),
		vid.texWidth, vid.texHeight, vid.frame, VIDEO_SCRATCH_IMAGE, vid.dirty );
	vid.dirty = qfalse;
}

/*
==================
CL_VideoFrame

Advances playback. Called once per client frame.
==================
*/
void CL_VideoFrame( void ) {
	double	now;
	int		guard = 0;

	if ( vid.status != VIDEOSTATUS_PLAYING ) {
		return;
	}

	now = CL_VideoClock();

	while ( vid.pending && vid.pendingPts <= now && guard++ < VIDEO_MAX_CATCHUP ) {
		if ( !CL_VideoTakePending() ) {
			vid.status = VIDEOSTATUS_ERROR;
			return;
		}
		CL_VideoDecodeNext();
	}

	if ( vid.audio ) {
		CL_VideoFeedAudio( now );
	}

	// Finished once the last picture has had its time on screen and the sound has drained.
	if ( vid.videoDone && !vid.pending && vid.audioDone && now >= vid.lastPts + 0.1 && now >= vid.audioEnd ) {
		vid.status = VIDEOSTATUS_FINISHED;
	}
}

void CL_VideoInit( void ) {
	cl_videoVolume = Cvar_Get( "cl_videoVolume", "1", CVAR_ARCHIVE_ND, "Volume of cutscene videos, relative to s_volume" );

	memset( &backend, 0, sizeof( backend ) );
#ifdef _WIN32
	if ( VideoMF_Init() ) {
		backend.create = VideoMF_Create;
		backend.shutdown = VideoMF_Shutdown;
		backend.name = "mediafoundation";
	}
#endif
#ifdef USE_FFMPEG
	if ( !backend.create && VideoFFmpeg_Init() ) {
		backend.create = VideoFFmpeg_Create;
		backend.shutdown = VideoFFmpeg_Shutdown;
		backend.name = "ffmpeg";
	}
#endif

	// Servers read cl_video from userinfo to know whether this client can show a cutscene.
	cl_video = Cvar_Get( "cl_video", "0", CVAR_USERINFO | CVAR_ROM, "Video playback protocol version supported by this client (0 = none)" );
	cl_videoBackend = Cvar_Get( "cl_videoBackend", "", CVAR_ROM, "Decoder used for video playback" );
	Cvar_Set( "cl_video", backend.create ? "1" : "0" );
	Cvar_Set( "cl_videoBackend", backend.name ? backend.name : "none" );
}

void CL_VideoShutdown( void ) {
	CL_VideoStop();
	if ( backend.shutdown ) {
		backend.shutdown();
	}
	memset( &backend, 0, sizeof( backend ) );
}
