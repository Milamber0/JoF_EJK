/*
===========================================================================
Copyright (C) 2026 JoF contributors

Fullscreen video playback (MP4 and friends) for in-game cutscenes.

cl_video.cpp owns playback: clock, audio scheduling and drawing. The actual
decoding sits behind VideoDecoder, with one implementation per platform:
Media Foundation on Windows (cl_video_mf.cpp) and FFmpeg elsewhere
(cl_video_ffmpeg.cpp, only when built with USE_FFMPEG).
===========================================================================
*/

#pragma once

#include "qcommon/q_shared.h"

class VideoDecoder {
public:
	virtual ~VideoDecoder() {}

	// The core reads the whole file into the buffer returned here, then calls Open.
	// The decoder owns the buffer; it must stay valid until the decoder is deleted.
	virtual byte *AllocData( int size ) = 0;
	virtual bool Open( const char *name ) = 0;

	// Called before the first decode when the sound system cannot play the soundtrack,
	// so the decoder stops buffering audio it will never be asked for.
	virtual void DisableAudio( void ) = 0;
	virtual bool HasAudio( void ) const = 0;
	virtual int AudioRate( void ) const = 0;
	virtual int AudioChannels( void ) const = 0;	// always 1 or 2: surround is downmixed

	// Decodes the next picture. Frame() then returns it as top-down RGBA, valid until the
	// next NextVideoFrame call. Returns false at the end of the stream or on error.
	virtual bool NextVideoFrame( double *pts ) = 0;
	virtual const byte *Frame( int *width, int *height ) const = 0;

	// Decodes the next block of interleaved 16-bit PCM, valid until the next NextAudio call.
	virtual bool NextAudio( const short **samples, int *frames, double *pts ) = 0;
};

// Platform backends. Init returns false when the backend cannot work on this machine
// (e.g. a Windows "N" edition without the Media Feature Pack).
#ifdef _WIN32
bool VideoMF_Init( void );
void VideoMF_Shutdown( void );
VideoDecoder *VideoMF_Create( void );
#endif

#ifdef USE_FFMPEG
bool VideoFFmpeg_Init( void );
void VideoFFmpeg_Shutdown( void );
VideoDecoder *VideoFFmpeg_Create( void );
#endif
