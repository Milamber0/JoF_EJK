/*
===========================================================================
Copyright (C) 2026 JoF contributors

macOS/Linux video decoding through the system FFmpeg libraries.

FFmpeg is not bundled: CMake links against whatever pkg-config finds and
compiles this file out (USE_FFMPEG undefined) when it finds nothing, in which
case the client simply reports cl_video 0. Supports FFmpeg 4.x through 7.x.
===========================================================================
*/

#ifdef USE_FFMPEG

extern "C" {
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libswscale/swscale.h>
#include <libswresample/swresample.h>
#include <libavutil/avutil.h>
}

#include <deque>
#include <vector>

#include "client.h"
#include "cl_video.h"

// FFmpeg 5.1 replaced the channel_layout bitmask with AVChannelLayout (and 7.0 removed the old one).
#if LIBAVUTIL_VERSION_INT >= AV_VERSION_INT( 57, 28, 100 )
#define VIDEO_FF_CHLAYOUT 1
#endif

#define VIDEO_FF_IO_BUFFER	32768

class VideoDecoderFFmpeg : public VideoDecoder {
public:
	VideoDecoderFFmpeg() : data( NULL ), size( 0 ), pos( 0 ), io( NULL ), fmt( NULL ),
		videoIndex( -1 ), audioIndex( -1 ), vctx( NULL ), actx( NULL ), sws( NULL ), swr( NULL ),
		frame( NULL ), packet( NULL ), demuxDone( false ), videoFlushed( false ), audioFlushed( false ),
		width( 0 ), height( 0 ), audioRate( 0 ), audioChannels( 0 ), startTime( 0.0 ), lastVideoPts( 0.0 ) {}

	~VideoDecoderFFmpeg() {
		ClearQueue( videoQueue );
		ClearQueue( audioQueue );
		av_frame_free( &frame );
		av_packet_free( &packet );
		sws_freeContext( sws );
		swr_free( &swr );
		avcodec_free_context( &vctx );
		avcodec_free_context( &actx );
		avformat_close_input( &fmt );
		if ( io ) {
			av_freep( &io->buffer );
			avio_context_free( &io );
		}
		free( data );
	}

	byte *AllocData( int bytes ) {
		data = (byte *)malloc( bytes );
		size = data ? bytes : 0;
		return data;
	}

	bool Open( const char *name ) {
		unsigned char *ioBuffer = (unsigned char *)av_malloc( VIDEO_FF_IO_BUFFER );

		if ( !ioBuffer ) {
			return false;
		}
		io = avio_alloc_context( ioBuffer, VIDEO_FF_IO_BUFFER, 0, this, ReadCallback, NULL, SeekCallback );
		if ( !io ) {
			av_free( ioBuffer );
			return false;
		}

		fmt = avformat_alloc_context();
		if ( !fmt ) {
			return false;
		}
		fmt->pb = io;
		fmt->flags |= AVFMT_FLAG_CUSTOM_IO;
		if ( avformat_open_input( &fmt, name, NULL, NULL ) < 0 ) {
			return false;	// avformat_open_input freed fmt
		}
		if ( avformat_find_stream_info( fmt, NULL ) < 0 ) {
			return false;
		}

		videoIndex = av_find_best_stream( fmt, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0 );
		if ( videoIndex < 0 || !OpenCodec( videoIndex, &vctx ) ) {
			return false;
		}
		audioIndex = av_find_best_stream( fmt, AVMEDIA_TYPE_AUDIO, -1, videoIndex, NULL, 0 );
		if ( audioIndex >= 0 && ( !OpenCodec( audioIndex, &actx ) || !SetupResampler() ) ) {
			avcodec_free_context( &actx );
			audioIndex = -1;
		}

		// Timestamps are reported relative to the container's start, so both streams share one zero.
		if ( fmt->start_time != AV_NOPTS_VALUE ) {
			startTime = fmt->start_time / (double)AV_TIME_BASE;
		}

		frame = av_frame_alloc();
		packet = av_packet_alloc();
		return frame && packet;
	}

	void DisableAudio( void ) {
		avcodec_free_context( &actx );
		ClearQueue( audioQueue );
		audioIndex = -1;
	}

	bool HasAudio( void ) const		{ return audioIndex >= 0; }
	int AudioRate( void ) const		{ return audioRate; }
	int AudioChannels( void ) const	{ return audioChannels; }

	bool NextVideoFrame( double *pts ) {
		if ( !Decode( vctx, videoIndex, videoQueue, videoFlushed ) ) {
			return false;
		}

		const int w = frame->width, h = frame->height;
		sws = sws_getCachedContext( sws, w, h, (AVPixelFormat)frame->format, w, h, AV_PIX_FMT_RGBA,
			SWS_BILINEAR, NULL, NULL, NULL );
		if ( !sws ) {
			av_frame_unref( frame );
			return false;
		}
		rgba.resize( w * h * 4 );
		uint8_t *dst[4] = { &rgba[0], NULL, NULL, NULL };
		int dstStride[4] = { w * 4, 0, 0, 0 };
		sws_scale( sws, frame->data, frame->linesize, 0, h, dst, dstStride );
		width = w;
		height = h;

		if ( frame->best_effort_timestamp != AV_NOPTS_VALUE ) {
			lastVideoPts = frame->best_effort_timestamp * av_q2d( fmt->streams[videoIndex]->time_base ) - startTime;
		} else {
			lastVideoPts += 1.0 / 30.0;
		}
		*pts = lastVideoPts;
		av_frame_unref( frame );
		return true;
	}

	const byte *Frame( int *w, int *h ) const {
		*w = width;
		*h = height;
		return rgba.empty() ? NULL : &rgba[0];
	}

	bool NextAudio( const short **samples, int *frames, double *pts ) {
		if ( audioIndex < 0 ) {
			return false;
		}
		for ( ;; ) {
			if ( !Decode( actx, audioIndex, audioQueue, audioFlushed ) ) {
				return false;
			}

			int capacity = swr_get_out_samples( swr, frame->nb_samples );
			if ( capacity <= 0 ) {
				av_frame_unref( frame );
				continue;
			}
			pcm.resize( capacity * audioChannels );
			uint8_t *out = (uint8_t *)&pcm[0];
			int count = swr_convert( swr, &out, capacity, (const uint8_t **)frame->extended_data, frame->nb_samples );

			double framePts = 0.0;
			const bool hasPts = frame->best_effort_timestamp != AV_NOPTS_VALUE;
			if ( hasPts ) {
				framePts = frame->best_effort_timestamp * av_q2d( fmt->streams[audioIndex]->time_base ) - startTime;
			}
			av_frame_unref( frame );
			if ( count <= 0 ) {
				continue;
			}
			*samples = &pcm[0];
			*frames = count;
			*pts = hasPts ? framePts : nextAudioPts;
			nextAudioPts = *pts + (double)count / audioRate;
			return true;
		}
	}

private:
	static int ReadCallback( void *opaque, uint8_t *buf, int bufSize ) {
		VideoDecoderFFmpeg *self = (VideoDecoderFFmpeg *)opaque;
		int64_t left = self->size - self->pos;

		if ( left <= 0 ) {
			return AVERROR_EOF;
		}
		if ( bufSize > left ) {
			bufSize = (int)left;
		}
		memcpy( buf, self->data + self->pos, bufSize );
		self->pos += bufSize;
		return bufSize;
	}

	static int64_t SeekCallback( void *opaque, int64_t offset, int whence ) {
		VideoDecoderFFmpeg *self = (VideoDecoderFFmpeg *)opaque;
		int64_t target;

		switch ( whence & ~AVSEEK_FORCE ) {
		case AVSEEK_SIZE:	return self->size;
		case SEEK_SET:		target = offset; break;
		case SEEK_CUR:		target = self->pos + offset; break;
		case SEEK_END:		target = self->size + offset; break;
		default:			return -1;
		}
		if ( target < 0 || target > self->size ) {
			return -1;
		}
		self->pos = target;
		return target;
	}

	bool OpenCodec( int index, AVCodecContext **ctx ) {
		const AVCodecParameters	*par = fmt->streams[index]->codecpar;
		const AVCodec			*codec = avcodec_find_decoder( par->codec_id );

		if ( !codec ) {
			return false;
		}
		*ctx = avcodec_alloc_context3( codec );
		if ( !*ctx || avcodec_parameters_to_context( *ctx, par ) < 0 ) {
			return false;
		}
		( *ctx )->thread_count = 0;	// let FFmpeg pick
		( *ctx )->pkt_timebase = fmt->streams[index]->time_base;
		return avcodec_open2( *ctx, codec, NULL ) >= 0;
	}

	bool SetupResampler( void ) {
		audioRate = actx->sample_rate;
		if ( audioRate <= 0 ) {
			return false;
		}
#ifdef VIDEO_FF_CHLAYOUT
		AVChannelLayout inLayout, outLayout;
		int inChannels = actx->ch_layout.nb_channels;

		if ( inChannels <= 0 ) {
			return false;
		}
		audioChannels = inChannels > 2 ? 2 : inChannels;
		if ( actx->ch_layout.order == AV_CHANNEL_ORDER_UNSPEC ) {
			av_channel_layout_default( &inLayout, inChannels );
		} else if ( av_channel_layout_copy( &inLayout, &actx->ch_layout ) < 0 ) {
			return false;
		}
		av_channel_layout_default( &outLayout, audioChannels );
		const int err = swr_alloc_set_opts2( &swr, &outLayout, AV_SAMPLE_FMT_S16, audioRate,
			&inLayout, actx->sample_fmt, actx->sample_rate, 0, NULL );
		av_channel_layout_uninit( &inLayout );
		av_channel_layout_uninit( &outLayout );
		if ( err < 0 ) {
			return false;
		}
#else
		int inChannels = actx->channels;
		int64_t inLayout = actx->channel_layout;

		if ( inChannels <= 0 ) {
			return false;
		}
		audioChannels = inChannels > 2 ? 2 : inChannels;
		if ( !inLayout ) {
			inLayout = av_get_default_channel_layout( inChannels );
		}
		swr = swr_alloc_set_opts( NULL, av_get_default_channel_layout( audioChannels ), AV_SAMPLE_FMT_S16,
			audioRate, inLayout, actx->sample_fmt, actx->sample_rate, 0, NULL );
		if ( !swr ) {
			return false;
		}
#endif
		return swr_init( swr ) >= 0;
	}

	static void ClearQueue( std::deque<AVPacket *> &queue ) {
		while ( !queue.empty() ) {
			AVPacket *p = queue.front();
			queue.pop_front();
			av_packet_free( &p );
		}
	}

	// Reads one packet from the file and files it under its stream.
	bool Demux( void ) {
		if ( demuxDone ) {
			return false;
		}
		if ( av_read_frame( fmt, packet ) < 0 ) {
			demuxDone = true;
			return false;
		}
		if ( packet->stream_index == videoIndex || packet->stream_index == audioIndex ) {
			AVPacket *copy = av_packet_alloc();
			if ( copy ) {
				av_packet_move_ref( copy, packet );
				( copy->stream_index == videoIndex ? videoQueue : audioQueue ).push_back( copy );
			}
		}
		av_packet_unref( packet );
		return true;
	}

	// Leaves the next decoded frame of the given stream in `frame`.
	bool Decode( AVCodecContext *ctx, int index, std::deque<AVPacket *> &queue, bool &flushed ) {
		if ( !ctx || index < 0 ) {
			return false;
		}
		for ( ;; ) {
			const int err = avcodec_receive_frame( ctx, frame );
			if ( err == 0 ) {
				return true;
			}
			if ( err != AVERROR( EAGAIN ) ) {
				return false;	// AVERROR_EOF once drained, or a real error
			}

			while ( queue.empty() && Demux() ) {
			}
			if ( queue.empty() ) {
				if ( flushed ) {
					return false;
				}
				avcodec_send_packet( ctx, NULL );	// end of file: drain the frames the codec still holds
				flushed = true;
				continue;
			}

			AVPacket *p = queue.front();
			queue.pop_front();
			avcodec_send_packet( ctx, p );	// a corrupt packet is skipped, not fatal
			av_packet_free( &p );
		}
	}

	byte					*data;
	int64_t					size, pos;
	AVIOContext				*io;
	AVFormatContext			*fmt;
	int						videoIndex, audioIndex;
	AVCodecContext			*vctx, *actx;
	SwsContext				*sws;
	SwrContext				*swr;
	AVFrame					*frame;
	AVPacket				*packet;
	std::deque<AVPacket *>	videoQueue, audioQueue;
	bool					demuxDone, videoFlushed, audioFlushed;
	std::vector<byte>		rgba;
	int						width, height;
	std::vector<short>		pcm;
	int						audioRate, audioChannels;
	double					startTime, lastVideoPts;
	double					nextAudioPts = 0.0;
};

VideoDecoder *VideoFFmpeg_Create( void ) {
	return new VideoDecoderFFmpeg();
}

bool VideoFFmpeg_Init( void ) {
	av_log_set_level( AV_LOG_ERROR );
	return true;
}

void VideoFFmpeg_Shutdown( void ) {
}

#endif // USE_FFMPEG
