/*
===========================================================================
Copyright (C) 2026 JoF contributors

Windows video decoding through Media Foundation.

Media Foundation ships with Windows and decodes H.264/AAC (and HEVC, VP9, WMV where
the codecs are installed), so nothing extra needs to be distributed. Its DLLs are
loaded at runtime rather than linked: "N" editions of Windows lack them unless the
Media Feature Pack is installed, and a hard link would stop the game from starting
there at all. Only mfuuid (GUID constants, a static library) is linked.
===========================================================================
*/

#ifdef _WIN32

#define WIN32_LEAN_AND_MEAN
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <objbase.h>
#include <mfapi.h>
#include <mfidl.h>
#include <mfreadwrite.h>
#include <mferror.h>

#include <vector>

#include "client.h"
#include "cl_video.h"

#ifdef _MSC_VER
#pragma comment( lib, "mfuuid.lib" )
#endif

typedef HRESULT ( WINAPI *pMFStartup_t )( ULONG, DWORD );
typedef HRESULT ( WINAPI *pMFShutdown_t )( void );
typedef HRESULT ( WINAPI *pMFCreateAttributes_t )( IMFAttributes **, UINT32 );
typedef HRESULT ( WINAPI *pMFCreateMediaType_t )( IMFMediaType ** );
typedef HRESULT ( WINAPI *pMFCreateMFByteStreamOnStream_t )( IStream *, IMFByteStream ** );
typedef HRESULT ( WINAPI *pMFCreateSourceReaderFromByteStream_t )( IMFByteStream *, IMFAttributes *, IMFSourceReader ** );

static struct {
	HMODULE									mfplat;
	HMODULE									mfreadwrite;
	bool									comInitialized;
	pMFStartup_t							Startup;
	pMFShutdown_t							Shutdown;
	pMFCreateAttributes_t					CreateAttributes;
	pMFCreateMediaType_t					CreateMediaType;
	pMFCreateMFByteStreamOnStream_t			CreateMFByteStreamOnStream;
	pMFCreateSourceReaderFromByteStream_t	CreateSourceReaderFromByteStream;
} mf;

template <class T> static void SafeRelease( T **p ) {
	if ( *p ) {
		( *p )->Release();
		*p = NULL;
	}
}

static const wchar_t *VideoMF_ContentType( const char *name ) {
	const char *ext = COM_GetExtension( name );

	if ( !Q_stricmp( ext, "mov" ) )							return L"video/quicktime";
	if ( !Q_stricmp( ext, "wmv" ) || !Q_stricmp( ext, "asf" ) )	return L"video/x-ms-asf";
	if ( !Q_stricmp( ext, "avi" ) )							return L"video/avi";
	if ( !Q_stricmp( ext, "mkv" ) )							return L"video/x-matroska";
	if ( !Q_stricmp( ext, "webm" ) )						return L"video/webm";
	return L"video/mp4";
}

class VideoDecoderMF : public VideoDecoder {
public:
	VideoDecoderMF() : hData( NULL ), reader( NULL ), audio( false ), audioRate( 0 ), audioChannels( 0 ),
		sourceChannels( 0 ), width( 0 ), height( 0 ), cropX( 0 ), cropY( 0 ), stride( 0 ), frameHeight( 0 ) {}

	~VideoDecoderMF() {
		SafeRelease( &reader );
		if ( hData ) {
			GlobalFree( hData );	// only still ours if Open never handed it to a stream
		}
	}

	byte *AllocData( int size ) {
		hData = GlobalAlloc( GMEM_MOVEABLE, size );
		return hData ? (byte *)GlobalLock( hData ) : NULL;
	}

	bool Open( const char *name ) {
		IStream			*stream = NULL;
		IMFByteStream	*byteStream = NULL;
		IMFAttributes	*streamAttributes = NULL;
		wchar_t			wideName[MAX_QPATH];
		bool			ok = false;

		GlobalUnlock( hData );
		if ( FAILED( CreateStreamOnHGlobal( hData, TRUE, &stream ) ) ) {
			return false;
		}
		hData = NULL;	// the stream frees it now

		if ( FAILED( mf.CreateMFByteStreamOnStream( stream, &byteStream ) ) ) {
			SafeRelease( &stream );
			return false;
		}
		SafeRelease( &stream );

		// Without a URL, the resolver needs to be told what the bytes are.
		if ( SUCCEEDED( byteStream->QueryInterface( IID_PPV_ARGS( &streamAttributes ) ) ) ) {
			streamAttributes->SetString( MF_BYTESTREAM_CONTENT_TYPE, VideoMF_ContentType( name ) );
			if ( MultiByteToWideChar( CP_UTF8, 0, name, -1, wideName, MAX_QPATH ) ) {
				streamAttributes->SetString( MF_BYTESTREAM_ORIGIN_NAME, wideName );
			}
			SafeRelease( &streamAttributes );
		}

		// Advanced processing (Windows 8+) is the fast converter; plain processing is the Windows 7 fallback.
		if ( CreateReader( byteStream, MF_SOURCE_READER_ENABLE_ADVANCED_VIDEO_PROCESSING ) ||
			 CreateReader( byteStream, MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING ) ) {
			SetupAudio();
			ok = true;
		}
		SafeRelease( &byteStream );
		return ok;
	}

	void DisableAudio( void ) {
		if ( audio ) {
			reader->SetStreamSelection( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, FALSE );
			audio = false;
		}
	}

	bool HasAudio( void ) const		{ return audio; }
	int AudioRate( void ) const		{ return audioRate; }
	int AudioChannels( void ) const	{ return audioChannels; }

	bool NextVideoFrame( double *pts ) {
		for ( ;; ) {
			DWORD		streamIndex, flags = 0;
			LONGLONG	timestamp = 0;
			IMFSample	*sample = NULL;
			bool		ok;

			if ( FAILED( reader->ReadSample( (DWORD)MF_SOURCE_READER_FIRST_VIDEO_STREAM, 0, &streamIndex, &flags, &timestamp, &sample ) ) ) {
				return false;
			}
			if ( flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED ) {
				if ( !ReadVideoFormat() ) {
					SafeRelease( &sample );
					return false;
				}
			}
			if ( flags & ( MF_SOURCE_READERF_ENDOFSTREAM | MF_SOURCE_READERF_ERROR ) ) {
				SafeRelease( &sample );
				return false;
			}
			if ( !sample ) {
				continue;	// a stream tick or gap carries no picture
			}
			ok = CopyPicture( sample );
			SafeRelease( &sample );
			if ( !ok ) {
				return false;
			}
			*pts = timestamp / 10000000.0;
			return true;
		}
	}

	const byte *Frame( int *w, int *h ) const {
		*w = width;
		*h = height;
		return rgba.empty() ? NULL : &rgba[0];
	}

	bool NextAudio( const short **samples, int *frames, double *pts ) {
		if ( !audio ) {
			return false;
		}
		for ( ;; ) {
			DWORD			streamIndex, flags = 0;
			LONGLONG		timestamp = 0;
			IMFSample		*sample = NULL;
			IMFMediaBuffer	*buffer = NULL;
			BYTE			*data;
			DWORD			length;
			int				count;

			if ( FAILED( reader->ReadSample( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, 0, &streamIndex, &flags, &timestamp, &sample ) ) ) {
				return false;
			}
			if ( flags & ( MF_SOURCE_READERF_ENDOFSTREAM | MF_SOURCE_READERF_ERROR ) ) {
				SafeRelease( &sample );
				return false;
			}
			if ( !sample ) {
				continue;
			}
			if ( FAILED( sample->ConvertToContiguousBuffer( &buffer ) ) ) {
				SafeRelease( &sample );
				return false;
			}
			if ( FAILED( buffer->Lock( &data, NULL, &length ) ) ) {
				SafeRelease( &buffer );
				SafeRelease( &sample );
				return false;
			}

			count = length / ( sourceChannels * 2 );
			pcm.resize( count * audioChannels );
			if ( count > 0 ) {
				Downmix( (const short *)data, count );
			}

			buffer->Unlock();
			SafeRelease( &buffer );
			SafeRelease( &sample );

			if ( count <= 0 ) {
				continue;
			}
			*samples = &pcm[0];
			*frames = count;
			*pts = timestamp / 10000000.0;
			return true;
		}
	}

private:
	bool CreateReader( IMFByteStream *byteStream, REFGUID processing ) {
		IMFAttributes	*attributes = NULL;
		IMFMediaType	*type = NULL;
		HRESULT			hr;

		SafeRelease( &reader );
		byteStream->SetCurrentPosition( 0 );	// a failed first attempt may have read into it
		if ( FAILED( mf.CreateAttributes( &attributes, 1 ) ) ) {
			return false;
		}
		attributes->SetUINT32( processing, TRUE );
		hr = mf.CreateSourceReaderFromByteStream( byteStream, attributes, &reader );
		SafeRelease( &attributes );
		if ( FAILED( hr ) ) {
			return false;
		}

		reader->SetStreamSelection( (DWORD)MF_SOURCE_READER_ALL_STREAMS, FALSE );
		reader->SetStreamSelection( (DWORD)MF_SOURCE_READER_FIRST_VIDEO_STREAM, TRUE );

		if ( FAILED( mf.CreateMediaType( &type ) ) ) {
			return false;
		}
		type->SetGUID( MF_MT_MAJOR_TYPE, MFMediaType_Video );
		type->SetGUID( MF_MT_SUBTYPE, MFVideoFormat_RGB32 );
		hr = reader->SetCurrentMediaType( (DWORD)MF_SOURCE_READER_FIRST_VIDEO_STREAM, NULL, type );
		SafeRelease( &type );

		if ( FAILED( hr ) || !ReadVideoFormat() ) {
			SafeRelease( &reader );
			return false;
		}
		return true;
	}

	bool ReadVideoFormat( void ) {
		IMFMediaType	*type = NULL;
		UINT32			w = 0, h = 0, rawStride = 0;
		MFVideoArea		area;

		if ( FAILED( reader->GetCurrentMediaType( (DWORD)MF_SOURCE_READER_FIRST_VIDEO_STREAM, &type ) ) ) {
			return false;
		}
		if ( FAILED( MFGetAttributeSize( type, MF_MT_FRAME_SIZE, &w, &h ) ) || !w || !h ) {
			SafeRelease( &type );
			return false;
		}

		// Decoders pad to whole macroblocks (1080 -> 1088); the aperture is the real picture.
		cropX = cropY = 0;
		width = (int)w;
		height = (int)h;
		if ( SUCCEEDED( type->GetBlob( MF_MT_MINIMUM_DISPLAY_APERTURE, (UINT8 *)&area, sizeof( area ), NULL ) ) &&
			 area.Area.cx > 0 && area.Area.cy > 0 &&
			 area.OffsetX.value + area.Area.cx <= (LONG)w && area.OffsetY.value + area.Area.cy <= (LONG)h ) {
			cropX = area.OffsetX.value;
			cropY = area.OffsetY.value;
			width = area.Area.cx;
			height = area.Area.cy;
		}

		stride = (int)w * 4;
		if ( SUCCEEDED( type->GetUINT32( MF_MT_DEFAULT_STRIDE, &rawStride ) ) ) {
			stride = (INT32)rawStride;
		}
		frameHeight = (int)h;
		SafeRelease( &type );

		rgba.resize( width * height * 4 );
		return true;
	}

	// RGB32 comes out as BGRX; the renderer wants RGBA, top-down.
	bool CopyPicture( IMFSample *sample ) {
		IMFMediaBuffer	*buffer = NULL;
		IMF2DBuffer		*buffer2D = NULL;
		BYTE			*scan0 = NULL;
		LONG			pitch = 0;
		bool			locked2D = false;
		int				x, y;

		if ( FAILED( sample->ConvertToContiguousBuffer( &buffer ) ) ) {
			return false;
		}

		if ( SUCCEEDED( buffer->QueryInterface( IID_PPV_ARGS( &buffer2D ) ) ) &&
			 SUCCEEDED( buffer2D->Lock2D( &scan0, &pitch ) ) ) {
			locked2D = true;
		} else {
			BYTE	*data;
			DWORD	length;

			SafeRelease( &buffer2D );
			if ( FAILED( buffer->Lock( &data, NULL, &length ) ) ) {
				SafeRelease( &buffer );
				return false;
			}
			if ( length < (DWORD)( abs( stride ) * frameHeight ) ) {
				buffer->Unlock();
				SafeRelease( &buffer );
				return false;
			}
			// A negative stride means the image is stored bottom-up.
			pitch = stride;
			scan0 = stride < 0 ? data + ( frameHeight - 1 ) * -stride : data;
		}

		for ( y = 0; y < height; y++ ) {
			const byte	*src = scan0 + ( cropY + y ) * pitch + cropX * 4;
			byte		*dst = &rgba[y * width * 4];

			for ( x = 0; x < width; x++, src += 4, dst += 4 ) {
				dst[0] = src[2];
				dst[1] = src[1];
				dst[2] = src[0];
				dst[3] = 255;
			}
		}

		if ( locked2D ) {
			buffer2D->Unlock2D();
			SafeRelease( &buffer2D );
		} else {
			buffer->Unlock();
		}
		SafeRelease( &buffer );
		return true;
	}

	void SetupAudio( void ) {
		IMFMediaType	*native = NULL;
		IMFMediaType	*type = NULL;
		IMFMediaType	*current = NULL;
		UINT32			nativeChannels = 2, bits = 0;
		HRESULT			hr;

		audio = false;
		if ( FAILED( reader->GetNativeMediaType( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, 0, &native ) ) ) {
			return;	// silent video
		}
		native->GetUINT32( MF_MT_AUDIO_NUM_CHANNELS, &nativeChannels );
		SafeRelease( &native );

		if ( FAILED( mf.CreateMediaType( &type ) ) ) {
			return;
		}
		type->SetGUID( MF_MT_MAJOR_TYPE, MFMediaType_Audio );
		type->SetGUID( MF_MT_SUBTYPE, MFAudioFormat_PCM );
		type->SetUINT32( MF_MT_AUDIO_BITS_PER_SAMPLE, 16 );

		// Ask the decoder for stereo first (the AAC decoder downmixes well); failing that take
		// what it gives and downmix ourselves.
		type->SetUINT32( MF_MT_AUDIO_NUM_CHANNELS, nativeChannels > 2 ? 2 : nativeChannels );
		hr = reader->SetCurrentMediaType( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, NULL, type );
		if ( FAILED( hr ) ) {
			type->DeleteItem( MF_MT_AUDIO_NUM_CHANNELS );
			hr = reader->SetCurrentMediaType( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, NULL, type );
		}
		SafeRelease( &type );
		if ( FAILED( hr ) ) {
			return;
		}

		if ( FAILED( reader->GetCurrentMediaType( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, &current ) ) ) {
			return;
		}
		audioRate = (int)MFGetAttributeUINT32( current, MF_MT_AUDIO_SAMPLES_PER_SECOND, 0 );
		sourceChannels = (int)MFGetAttributeUINT32( current, MF_MT_AUDIO_NUM_CHANNELS, 0 );
		bits = MFGetAttributeUINT32( current, MF_MT_AUDIO_BITS_PER_SAMPLE, 0 );
		SafeRelease( &current );

		if ( audioRate <= 0 || sourceChannels <= 0 || bits != 16 ) {
			return;
		}
		audioChannels = sourceChannels > 2 ? 2 : sourceChannels;
		reader->SetStreamSelection( (DWORD)MF_SOURCE_READER_FIRST_AUDIO_STREAM, TRUE );
		audio = true;
	}

	// Surround in WAVEFORMATEX order (FL FR C LFE BL BR ...): fold centre and rears into
	// the fronts so dialogue, usually on the centre channel, is not lost.
	void Downmix( const short *in, int count ) {
		int i;

		if ( sourceChannels <= 2 ) {
			memcpy( &pcm[0], in, count * sourceChannels * sizeof( short ) );
			return;
		}
		for ( i = 0; i < count; i++, in += sourceChannels ) {
			float left = in[0], right = in[1];

			if ( sourceChannels >= 3 ) {
				left += in[2] * 0.707f;
				right += in[2] * 0.707f;
			}
			if ( sourceChannels >= 6 ) {
				left += in[4] * 0.707f;
				right += in[5] * 0.707f;
			}
			pcm[i * 2 + 0] = (short)Com_Clamp( -32768.0f, 32767.0f, left * 0.6f );
			pcm[i * 2 + 1] = (short)Com_Clamp( -32768.0f, 32767.0f, right * 0.6f );
		}
	}

	HGLOBAL				hData;
	IMFSourceReader		*reader;
	bool				audio;
	int					audioRate, audioChannels, sourceChannels;
	int					width, height, cropX, cropY, stride, frameHeight;
	std::vector<byte>	rgba;
	std::vector<short>	pcm;
};

VideoDecoder *VideoMF_Create( void ) {
	return new VideoDecoderMF();
}

bool VideoMF_Init( void ) {
	HRESULT hr;

	memset( &mf, 0, sizeof( mf ) );
	mf.mfplat = LoadLibraryA( "mfplat.dll" );
	mf.mfreadwrite = LoadLibraryA( "mfreadwrite.dll" );
	if ( !mf.mfplat || !mf.mfreadwrite ) {
		Com_Printf( "Media Foundation not found: cutscene videos disabled (install the Media Feature Pack on Windows N)\n" );
		VideoMF_Shutdown();
		return false;
	}

	mf.Startup = (pMFStartup_t)GetProcAddress( mf.mfplat, "MFStartup" );
	mf.Shutdown = (pMFShutdown_t)GetProcAddress( mf.mfplat, "MFShutdown" );
	mf.CreateAttributes = (pMFCreateAttributes_t)GetProcAddress( mf.mfplat, "MFCreateAttributes" );
	mf.CreateMediaType = (pMFCreateMediaType_t)GetProcAddress( mf.mfplat, "MFCreateMediaType" );
	mf.CreateMFByteStreamOnStream = (pMFCreateMFByteStreamOnStream_t)GetProcAddress( mf.mfplat, "MFCreateMFByteStreamOnStream" );
	mf.CreateSourceReaderFromByteStream = (pMFCreateSourceReaderFromByteStream_t)GetProcAddress( mf.mfreadwrite, "MFCreateSourceReaderFromByteStream" );
	if ( !mf.Startup || !mf.Shutdown || !mf.CreateAttributes || !mf.CreateMediaType ||
		 !mf.CreateMFByteStreamOnStream || !mf.CreateSourceReaderFromByteStream ) {
		Com_Printf( "Media Foundation is incomplete: cutscene videos disabled\n" );
		VideoMF_Shutdown();
		return false;
	}

	// SDL may already have put this thread in an apartment; either model works for the reader.
	hr = CoInitializeEx( NULL, COINIT_APARTMENTTHREADED );
	mf.comInitialized = SUCCEEDED( hr );

	if ( FAILED( mf.Startup( MF_VERSION, MFSTARTUP_LITE ) ) ) {
		Com_Printf( "MFStartup failed: cutscene videos disabled\n" );
		mf.Shutdown = NULL;
		VideoMF_Shutdown();
		return false;
	}
	return true;
}

void VideoMF_Shutdown( void ) {
	if ( mf.Shutdown ) {
		mf.Shutdown();
	}
	if ( mf.comInitialized ) {
		CoUninitialize();
	}
	if ( mf.mfreadwrite ) {
		FreeLibrary( mf.mfreadwrite );
	}
	if ( mf.mfplat ) {
		FreeLibrary( mf.mfplat );
	}
	memset( &mf, 0, sizeof( mf ) );
}

#endif // _WIN32
