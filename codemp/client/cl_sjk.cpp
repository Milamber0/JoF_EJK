/*
===========================================================================
Copyright (C) 2026, JoF contributors

This file is part of the OpenJK source code.

OpenJK is free software; you can redistribute it and/or modify it
under the terms of the GNU General Public License version 2 as
published by the Free Software Foundation.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program; if not, see <http://www.gnu.org/licenses/>.
===========================================================================
*/

// cl_sjk.cpp -- SJK chat: the one conversation of every SJK player (docs/sjk-chat.md)
//
// SJK's hub (version 1 of its protocol) knows players by an Ed25519 key, the
// identity.key file SJK makes, and every request here is signed with it: there is
// no account and no password. This client signs with SJK's key when SJK is
// installed, so the player is the same person in both, else makes its own key in
// the same format, and names itself EJK in its User-Agent either way.
//
// A worker thread registers the key and long-polls the feed, another sends what
// the player says on the SJK channel; the main thread prints, and in a match says
// the player's own lines in the game server's chat too, once the hub took them.

#include "client.h"

#if defined(_WIN32)

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#ifndef NOMINMAX
#define NOMINMAX
#endif
#include <windows.h>
#include <winhttp.h>
#include <bcrypt.h>
#include <shlobj.h>

#include <atomic>
#include <condition_variable>
#include <cctype>
#include <cstdarg>
#include <ctime>
#include <deque>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include "cJSON.h"
#include "monocypher-ed25519.h"

#define SJK_DEFAULT_HUB		"https://sjk.dfox.app"
// First line of SJK's key file; a file without it is not one.
#define SJK_KEY_MAGIC		"SJK-IDENTITY-1"
#define SJK_TEXT_MAX		300		// characters a message may hold
#define SJK_NAME_MAX		64		// characters a worn name may hold

#ifndef WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
#define WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY 4
#endif
#ifndef WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3
#define WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3 0x00002000
#endif

static const DWORD		SJK_CALL_TIMEOUT_MS	= 10000;	// any call but the feed
static const DWORD		SJK_FEED_TIMEOUT_MS	= 40000;	// the feed may hold a call 25 s
static const int		SJK_FEED_WAIT		= 25;
static const DWORD		SJK_SEND_GAP_MS		= 1100;		// the hub takes one message a second at most
static const int		SJK_SERVER_SAY_GAP_MS	= 1100;	// a game server one command a second (sv_floodProtect)
static const size_t		SJK_SERVER_SAY_MAX	= MAX_SAY_TEXT - 1;	// what a game server keeps of a say
static const size_t		SJK_ANSWER_MAX		= 256 * 1024;
static const int		SJK_RANDOM_TRIES	= 3;		// for a request's nonce, then it fails

/*
===============================================================================

TEXT

===============================================================================
*/

static std::string SJK_Format( const char *fmt, ... ) {
	char text[1024];
	va_list args;
	va_start( args, fmt );
	vsnprintf( text, sizeof( text ), fmt, args );
	va_end( args );
	return text;
}

static std::wstring SJK_Widen( const std::string &s ) {
	if ( s.empty() )
		return std::wstring();
	int n = MultiByteToWideChar( CP_UTF8, 0, s.data(), (int)s.size(), NULL, 0 );
	std::wstring w( n, L'\0' );
	MultiByteToWideChar( CP_UTF8, 0, s.data(), (int)s.size(), &w[0], n );
	return w;
}

static void SJK_AppendUTF8( std::string *out, uint32_t cp ) {
	if ( cp < 0x80 ) {
		*out += (char)cp;
	} else if ( cp < 0x800 ) {
		*out += (char)( 0xC0 | ( cp >> 6 ) );
		*out += (char)( 0x80 | ( cp & 0x3F ) );
	} else if ( cp < 0x10000 ) {
		*out += (char)( 0xE0 | ( cp >> 12 ) );
		*out += (char)( 0x80 | ( ( cp >> 6 ) & 0x3F ) );
		*out += (char)( 0x80 | ( cp & 0x3F ) );
	} else {
		*out += (char)( 0xF0 | ( cp >> 18 ) );
		*out += (char)( 0x80 | ( ( cp >> 12 ) & 0x3F ) );
		*out += (char)( 0x80 | ( ( cp >> 6 ) & 0x3F ) );
		*out += (char)( 0x80 | ( cp & 0x3F ) );
	}
}

// The next code point of UTF-8 text at *at, moving past it; U+FFFD for a bad byte.
static uint32_t SJK_NextUTF8( const std::string &s, size_t *at ) {
	const unsigned char c = (unsigned char)s[*at];
	int extra = c < 0x80 ? 0 : ( c & 0xE0 ) == 0xC0 ? 1 : ( c & 0xF0 ) == 0xE0 ? 2 : ( c & 0xF8 ) == 0xF0 ? 3 : -1;
	(*at)++;
	if ( extra < 0 )
		return 0xFFFD;
	uint32_t cp = extra ? c & ( 0x3F >> extra ) : c;
	for ( int i = 0; i < extra; i++ ) {
		if ( *at >= s.size() || ( (unsigned char)s[*at] & 0xC0 ) != 0x80 )
			return 0xFFFD;
		cp = ( cp << 6 ) | ( (unsigned char)s[(*at)++] & 0x3F );
	}
	return cp;
}

// The game keeps text in its own 8-bit set (ConvertUTF32ToExpectedCharset: Cyrillic as
// Windows-1251, central European letters as 1250, the rest as Latin-1). The hub
// speaks UTF-8. A byte can stand for letters of several sets, so the game's language
// picks which one a byte the player typed meant.
static uint32_t SJK_CharsetToUTF32( unsigned char b, const char *language ) {
	if ( b < 0x80 )
		return b;
	static uint32_t latin[256], central[256], cyrillic[256];
	static bool built = false;
	if ( !built ) {
		for ( uint32_t cp = 0xA0; cp < 0x500; cp++ ) {
			uint8_t to = ConvertUTF32ToExpectedCharset( cp );
			if ( cp <= 0xFF && !latin[to] )
				latin[to] = cp;
			else if ( cp >= 0x100 && cp < 0x180 && to != (uint8_t)cp && !central[to] )
				central[to] = cp;
			else if ( cp >= 0x400 && to != (uint8_t)cp && !cyrillic[to] )
				cyrillic[to] = cp;
		}
		built = true;
	}
	if ( language && !Q_stricmp( language, "russian" ) && cyrillic[b] )
		return cyrillic[b];
	if ( language && ( !Q_stricmp( language, "polish" ) || !Q_stricmp( language, "czech" ) ) && central[b] )
		return central[b];
	// Latin-1 has no letters below 0xA0: those bytes only ever stood for central ones.
	return latin[b] ? latin[b] : central[b] ? central[b] : cyrillic[b] ? cyrillic[b] : '?';
}

// Game text (the chat field, a name) as UTF-8 for the hub.
static std::string SJK_ToUTF8( const char *text, const char *language ) {
	std::string out;
	for ( const unsigned char *p = (const unsigned char *)text; *p; p++ )
		SJK_AppendUTF8( &out, SJK_CharsetToUTF32( *p, language ) );
	return out;
}

// The hub's UTF-8 as game text; what the game's sets cannot show becomes '?', and
// control characters are dropped.
static std::string SJK_FromUTF8( const std::string &text ) {
	std::string out;
	for ( size_t at = 0; at < text.size(); ) {
		uint32_t cp = SJK_NextUTF8( text, &at );
		if ( cp < 0x20 || cp == 0x7F )
			continue;
		if ( cp < 0x80 ) {
			out += (char)cp;
			continue;
		}
		uint8_t b = ConvertUTF32ToExpectedCharset( cp );
		out += ( cp <= 0xFF || b != (uint8_t)cp ) ? (char)b : '?';
	}
	return out;
}

// What the hub would make of text before its own checks: runs of spaces and tabs
// are one space and the ends trimmed. False (and why) for what it would refuse
// whatever else it holds; the hub checks the rest.
static bool SJK_PrepareText( const std::string &utf8, std::string *out, std::string *why ) {
	out->clear();
	bool space = false;
	size_t chars = 0;
	for ( size_t at = 0; at < utf8.size(); ) {
		size_t start = at;
		uint32_t cp = SJK_NextUTF8( utf8, &at );
		if ( cp == '\n' || cp == '\r' ) {
			*why = "SJK chat takes one line at a time";
			return false;
		}
		if ( cp == ' ' || cp == '\t' ) {
			space = !out->empty();
			continue;
		}
		if ( space ) {
			*out += ' ';
			chars++;
			space = false;
		}
		out->append( utf8, start, at - start );
		chars++;
	}
	if ( !chars ) {
		*why = "nothing to say";
		return false;
	}
	if ( chars > SJK_TEXT_MAX ) {
		*why = SJK_Format( "an SJK chat message holds at most %d characters", SJK_TEXT_MAX );
		return false;
	}
	return true;
}

/*
===============================================================================

KEYS AND SIGNATURES

===============================================================================
*/

static const char SJK_B64[] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

// Unpadded base64url, as the hub writes keys, nonces and signatures.
static std::string SJK_Base64( const uint8_t *bytes, size_t size ) {
	std::string out;
	for ( size_t i = 0; i < size; i += 3 ) {
		uint32_t n = (uint32_t)bytes[i] << 16;
		if ( i + 1 < size )
			n |= (uint32_t)bytes[i + 1] << 8;
		if ( i + 2 < size )
			n |= bytes[i + 2];
		out += SJK_B64[( n >> 18 ) & 63];
		out += SJK_B64[( n >> 12 ) & 63];
		if ( i + 1 < size )
			out += SJK_B64[( n >> 6 ) & 63];
		if ( i + 2 < size )
			out += SJK_B64[n & 63];
	}
	return out;
}

static bool SJK_Unbase64( const std::string &text, std::vector<uint8_t> *bytes ) {
	bytes->clear();
	uint32_t n = 0;
	int bits = 0;
	for ( char c : text ) {
		if ( c == '=' )
			break;
		const char *at = strchr( SJK_B64, c );
		if ( !c || !at )
			return false;
		n = ( n << 6 ) | (uint32_t)( at - SJK_B64 );
		bits += 6;
		if ( bits >= 8 ) {
			bits -= 8;
			bytes->push_back( (uint8_t)( n >> bits ) );
		}
	}
	return true;
}

static std::string SJK_Hex( const uint8_t *bytes, size_t size ) {
	static const char digits[] = "0123456789abcdef";
	std::string out;
	for ( size_t i = 0; i < size; i++ ) {
		out += digits[bytes[i] >> 4];
		out += digits[bytes[i] & 15];
	}
	return out;
}

static bool SJK_Sha256( const void *data, size_t size, uint8_t digest[32] ) {
	BCRYPT_ALG_HANDLE algorithm = NULL;
	BCRYPT_HASH_HANDLE hash = NULL;
	bool ok = BCryptOpenAlgorithmProvider( &algorithm, BCRYPT_SHA256_ALGORITHM, NULL, 0 ) == 0
		&& BCryptCreateHash( algorithm, &hash, NULL, 0, NULL, 0, 0 ) == 0
		&& BCryptHashData( hash, (PUCHAR)data, (ULONG)size, 0 ) == 0
		&& BCryptFinishHash( hash, digest, 32, 0 ) == 0;
	if ( hash )
		BCryptDestroyHash( hash );
	if ( algorithm )
		BCryptCloseAlgorithmProvider( algorithm, 0 );
	return ok;
}

static bool SJK_Random( uint8_t *bytes, size_t size ) {
	return BCryptGenRandom( NULL, bytes, (ULONG)size, BCRYPT_USE_SYSTEM_PREFERRED_RNG ) == 0;
}

struct sjkIdentity_t {
	uint8_t			secret[64];		// Monocypher's: the seed, then the public key
	uint8_t			pub[32];
	std::string		key;			// the public key, unpadded base64url
	std::string		keyId;			// the first 16 hex digits of its SHA-256

	~sjkIdentity_t() { crypto_wipe( secret, sizeof( secret ) ); }
};

static bool SJK_IdentityFromSeed( const uint8_t seed[32], sjkIdentity_t *id ) {
	uint8_t copy[32];
	memcpy( copy, seed, sizeof( copy ) );
	crypto_ed25519_key_pair( id->secret, id->pub, copy );	// wipes copy
	uint8_t digest[32];
	if ( !SJK_Sha256( id->pub, sizeof( id->pub ), digest ) )
		return false;
	id->key = SJK_Base64( id->pub, sizeof( id->pub ) );
	id->keyId = SJK_Hex( digest, 8 );
	return true;
}

// The text of SJK's key file, read as SJK reads it: the magic line, then the seed,
// unpadded base64url, and nothing else but blank lines and spaces.
static bool SJK_ParseKeyFile( const std::string &text, uint8_t seed[32] ) {
	std::vector<std::string> lines;
	size_t start = 0;
	while ( start <= text.size() ) {
		size_t end = text.find( '\n', start );
		if ( end == std::string::npos )
			end = text.size();
		std::string line = text.substr( start, end - start );
		size_t first = line.find_first_not_of( " \t\r" ), last = line.find_last_not_of( " \t\r" );
		if ( first != std::string::npos )
			lines.push_back( line.substr( first, last - first + 1 ) );
		start = end + 1;
	}
	std::vector<uint8_t> bytes;
	if ( lines.size() != 2 || lines[0] != SJK_KEY_MAGIC || !SJK_Unbase64( lines[1], &bytes ) || bytes.size() != 32 )
		return false;
	memcpy( seed, bytes.data(), 32 );
	return true;
}

// The Authorization header of one request: SJK-Sig over the method, the path with
// its query, the time, a nonce and the body's SHA-256, as the hub checks it.
static std::string SJK_Authorization( const sjkIdentity_t &id, const char *method, const std::string &pathAndQuery,
		const std::string &body, int64_t ts, const uint8_t nonce[12] ) {
	uint8_t digest[32];
	SJK_Sha256( body.data(), body.size(), digest );
	const std::string nonceText = SJK_Base64( nonce, 12 );
	const std::string message = std::string( "SJK-HUB-v1\n" ) + method + "\n" + pathAndQuery + "\n" + std::to_string( ts ) + "\n"
		+ nonceText + "\n" + SJK_Hex( digest, sizeof( digest ) );
	uint8_t signature[64];
	crypto_ed25519_sign( signature, id.secret, (const uint8_t *)message.data(), message.size() );
	return "SJK-Sig key=" + id.key + ", ts=" + std::to_string( ts ) + ", nonce=" + nonceText + ", sig=" + SJK_Base64( signature, sizeof( signature ) );
}

/*
===============================================================================

WHERE THE KEY IS

Never in a folder the game searches: a module a server sent could read it there.
FS_FOpenFileByMode refuses any identity.key besides.

===============================================================================
*/

static bool SJK_ReadSmallFile( const std::wstring &path, std::string *text ) {
	HANDLE file = CreateFileW( path.c_str(), GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;
	char buffer[1024];
	DWORD got = 0;
	bool ok = ReadFile( file, buffer, sizeof( buffer ), &got, NULL ) != 0;
	CloseHandle( file );
	if ( ok )
		text->assign( buffer, got );
	return ok;
}

static std::wstring SJK_ProgramDir( void ) {
	wchar_t path[MAX_PATH];
	DWORD n = GetModuleFileNameW( NULL, path, MAX_PATH );
	if ( !n || n >= MAX_PATH )
		return std::wstring();
	std::wstring program( path, n );
	size_t slash = program.find_last_of( L"\\/" );
	return slash == std::wstring::npos ? std::wstring() : program.substr( 0, slash );
}

static std::wstring SJK_AppData( void ) {
	wchar_t path[MAX_PATH];
	return SUCCEEDED( SHGetFolderPathW( NULL, CSIDL_APPDATA, NULL, SHGFP_TYPE_CURRENT, path ) ) ? std::wstring( path ) : std::wstring();
}

// SJK's key where SJK keeps it (its folder beside base, else the user's application
// data), else EJK's own, made the first time. *where says which, for sjkstatus.
static bool SJK_LoadIdentity( sjkIdentity_t *id, std::string *where, std::string *why ) {
	const std::wstring game = SJK_ProgramDir(), appData = SJK_AppData();
	struct { std::wstring path; const char *name; } places[] = {
		{ game.empty() ? L"" : game + L"\\SJK\\identity.key", "SJK's (in GameData\\SJK)" },
		{ appData.empty() ? L"" : appData + L"\\SJK\\identity.key", "SJK's (in %APPDATA%\\SJK)" },
		{ appData.empty() ? L"" : appData + L"\\EternalJK\\identity.key", "EJK's own (in %APPDATA%\\EternalJK)" },
	};
	uint8_t seed[32];
	for ( const auto &place : places ) {
		std::string text;
		if ( place.path.empty() || !SJK_ReadSmallFile( place.path, &text ) )
			continue;
		if ( !SJK_ParseKeyFile( text, seed ) ) {
			// A damaged key is never replaced: that would end the identity.
			*why = std::string( "the identity file " ) + place.name + " is damaged; it was left as it is";
			return false;
		}
		*where = place.name;
		bool ok = SJK_IdentityFromSeed( seed, id );
		crypto_wipe( seed, sizeof( seed ) );
		return ok;
	}

	// None yet: EJK's own, in SJK's format, created once and never overwritten.
	const std::wstring own = places[2].path;
	if ( own.empty() || !SJK_Random( seed, sizeof( seed ) ) ) {
		*why = "no place or no randomness for a new identity";
		return false;
	}
	CreateDirectoryW( ( appData + L"\\EternalJK" ).c_str(), NULL );
	HANDLE file = CreateFileW( own.c_str(), GENERIC_WRITE, 0, NULL, CREATE_NEW, FILE_ATTRIBUTE_NORMAL, NULL );
	const std::string text = std::string( SJK_KEY_MAGIC ) + "\n" + SJK_Base64( seed, sizeof( seed ) ) + "\n";
	DWORD wrote = 0;
	bool ok = file != INVALID_HANDLE_VALUE && WriteFile( file, text.data(), (DWORD)text.size(), &wrote, NULL ) && wrote == text.size()
		&& FlushFileBuffers( file );
	if ( file != INVALID_HANDLE_VALUE )
		CloseHandle( file );
	if ( !ok ) {
		crypto_wipe( seed, sizeof( seed ) );
		*why = "cannot write a new identity to %APPDATA%\\EternalJK";
		return false;
	}
	*where = "EJK's own (new, in %APPDATA%\\EternalJK)";
	ok = SJK_IdentityFromSeed( seed, id );
	crypto_wipe( seed, sizeof( seed ) );
	return ok;
}

/*
===============================================================================

HUB CALLS (WinHTTP; signed, JSON)

===============================================================================
*/

struct sjkAnswer_t {
	int				status = 0;		// HTTP status; 0 when no answer came
	std::string		body;
	std::string		error;			// why no answer came, or the hub's error code
	std::string		message;		// the hub's error message
	int64_t			serverTime = 0;	// the hub's clock, with a refusal for this one's
};

// A line for the chat box or the console, in game text: a message of the feed, or SJK
// chat's own notice (who empty). A line of the backlog, what was said before this client
// came, goes only to the SJK chat menu's history.
struct sjkLine_t {
	std::string		text;		// a notice, or the message without colour codes, GIFs as <GIF>
	std::string		client;		// the client it came from, "sjk" or "ejk" (SJK_MessageClient)
	std::string		id;			// its id in the feed, empty for a notice
	std::string		who;		// the sender's name with its colours and verified mark
	std::string		nameWords;	// the sender's name's letters and digits (SJK_Words) and
	std::string		words;		// the message's, to tell the server's copy of it
	bool			backlog = false;
	bool			inAll = false;	// said in the game server's All chat too: ALL/SJK
};

// A message to send: UTF-8 text for the hub and, said in a match (SJK_CurrentMatch),
// the game text for that server's chat once the hub took it.
struct sjkOutgoing_t {
	std::string		text;
	std::string		gameText;
	std::string		match;
};

// The server's copy of a message the hub answered: its game text, the match it was said
// in, and the id the hub gave it, empty when it refused it.
struct sjkServerCopy_t {
	std::string		gameText;
	std::string		match;
	std::string		id;
};

// One worker's view of the hub: what it calls, as whom. Each start of SJK chat makes
// a new one and lets go of the last, so a worker still waiting on the network never
// holds up the client; whoever lets go last frees it (SJK_Release).
struct sjkSession_t {
	std::mutex						lock;
	std::condition_variable			wake;
	std::atomic<bool>				stop{ false };
	std::atomic<int>				holders{ 0 };	// its two workers, and the client while it is current

	std::string						hub;		// https://host[:port], no path
	std::wstring					agent;		// SJK_UserAgent
	sjkIdentity_t					id;
	std::atomic<int64_t>			clockOffset{ 0 };

	// guarded by lock
	std::string						name;		// the name the player wears, UTF-8
	std::deque<sjkOutgoing_t>		outgoing;
	std::deque<sjkLine_t>			lines;		// for the main thread to print
	std::deque<sjkServerCopy_t>		serverCopies;	// for the main thread to say
	bool							registered = false;
	bool							reachable = true;
	int								online = 0;
};

static std::string SJK_NetError( DWORD code ) {
	switch ( code ) {
	case ERROR_WINHTTP_NAME_NOT_RESOLVED:
	case ERROR_WINHTTP_CANNOT_CONNECT:
		return "cannot reach the hub";
	case ERROR_WINHTTP_TIMEOUT:
		return "the hub took too long to answer";
	case ERROR_WINHTTP_SECURE_FAILURE:
		return "the secure connection to the hub failed";
	default:
		return SJK_Format( "the connection to the hub failed (error %lu)", code );
	}
}

// A request's nonce, from the system's random numbers: a few tries, else the request
// fails rather than going out with an unset one.
static bool SJK_Nonce( uint8_t nonce[12] ) {
	for ( int tries = 0; tries < SJK_RANDOM_TRIES; tries++ ) {
		if ( SJK_Random( nonce, 12 ) )
			return true;
	}
	return false;
}

static sjkAnswer_t SJK_Http( sjkSession_t *s, const char *method, const std::string &pathAndQuery,
		const std::string &body, DWORD timeoutMs ) {
	sjkAnswer_t answer;
	const std::wstring url = SJK_Widen( s->hub + pathAndQuery );
	wchar_t host[256], path[2048];
	URL_COMPONENTS parts;
	memset( &parts, 0, sizeof( parts ) );
	parts.dwStructSize = sizeof( parts );
	parts.lpszHostName = host;
	parts.dwHostNameLength = ARRAY_LEN( host );
	parts.lpszUrlPath = path;
	parts.dwUrlPathLength = ARRAY_LEN( path );
	if ( !WinHttpCrackUrl( url.c_str(), 0, 0, &parts ) ) {
		answer.error = "bad hub address";
		return answer;
	}
	// Asked for exactly as signed: the path with its query.
	const std::wstring target = SJK_Widen( pathAndQuery );

	HINTERNET session = WinHttpOpen( s->agent.c_str(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0 );
	if ( !session )
		session = WinHttpOpen( s->agent.c_str(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0 );
	if ( !session ) {
		answer.error = SJK_NetError( GetLastError() );
		return answer;
	}
	DWORD protocols = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
	if ( !WinHttpSetOption( session, WINHTTP_OPTION_SECURE_PROTOCOLS, &protocols, sizeof( protocols ) ) ) {
		protocols = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2;
		WinHttpSetOption( session, WINHTTP_OPTION_SECURE_PROTOCOLS, &protocols, sizeof( protocols ) );
	}
	WinHttpSetTimeouts( session, 10000, 10000, 10000, (int)timeoutMs );

	HINTERNET connection = WinHttpConnect( session, host, parts.nPort, 0 );
	HINTERNET request = connection ? WinHttpOpenRequest( connection, SJK_Widen( method ).c_str(), target.c_str(), NULL,
		WINHTTP_NO_REFERER, WINHTTP_DEFAULT_ACCEPT_TYPES, parts.nScheme == INTERNET_SCHEME_HTTPS ? WINHTTP_FLAG_SECURE : 0 ) : NULL;
	uint8_t nonce[12];
	if ( !request ) {
		answer.error = SJK_NetError( GetLastError() );
	} else if ( !SJK_Nonce( nonce ) ) {
		answer.error = "could not make a random number to sign the request";
	} else {
		const int64_t ts = (int64_t)time( NULL ) + s->clockOffset;
		std::string headers = "Authorization: " + SJK_Authorization( s->id, method, pathAndQuery, body, ts, nonce ) + "\r\n";
		if ( !body.empty() )
			headers += "Content-Type: application/json\r\n";
		const std::wstring wideHeaders = SJK_Widen( headers );
		if ( !WinHttpSendRequest( request, wideHeaders.c_str(), (DWORD)-1L, body.empty() ? WINHTTP_NO_REQUEST_DATA : (LPVOID)body.data(),
				(DWORD)body.size(), (DWORD)body.size(), 0 ) || !WinHttpReceiveResponse( request, NULL ) ) {
			answer.error = SJK_NetError( GetLastError() );
		} else {
			DWORD code = 0, size = sizeof( code );
			WinHttpQueryHeaders( request, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_HEADER_NAME_BY_INDEX,
				&code, &size, WINHTTP_NO_HEADER_INDEX );
			answer.status = (int)code;
			char buffer[16 * 1024];
			for ( ;; ) {
				DWORD got = 0;
				if ( !WinHttpReadData( request, buffer, sizeof( buffer ), &got ) ) {
					answer.status = 0;
					answer.error = "the hub's answer broke off";
					break;
				}
				if ( !got )
					break;
				if ( answer.body.size() + got > SJK_ANSWER_MAX ) {
					answer.status = 0;
					answer.error = "the hub's answer is too large";
					break;
				}
				answer.body.append( buffer, got );
			}
		}
	}
	if ( request )
		WinHttpCloseHandle( request );
	if ( connection )
		WinHttpCloseHandle( connection );
	WinHttpCloseHandle( session );

	// The hub's errors are {"error":"<code>","message":".."}.
	if ( answer.status >= 400 ) {
		cJSON *root = cJSON_ParseWithLength( answer.body.data(), answer.body.size() );
		const cJSON *error = cJSON_GetObjectItemCaseSensitive( root, "error" );
		const cJSON *message = cJSON_GetObjectItemCaseSensitive( root, "message" );
		const cJSON *serverTime = cJSON_GetObjectItemCaseSensitive( root, "server_time" );
		answer.error = cJSON_IsString( error ) ? error->valuestring : SJK_Format( "HTTP %d", answer.status );
		answer.message = cJSON_IsString( message ) ? message->valuestring : "";
		if ( cJSON_IsNumber( serverTime ) )
			answer.serverTime = (int64_t)serverTime->valuedouble;
		cJSON_Delete( root );
	}
	return answer;
}

// A signed call; a refusal for the clock (this PC's is off) corrects the offset and
// tries once more, as the hub asks.
static sjkAnswer_t SJK_Call( sjkSession_t *s, const char *method, const std::string &pathAndQuery,
		const std::string &body, DWORD timeoutMs = SJK_CALL_TIMEOUT_MS ) {
	sjkAnswer_t answer = SJK_Http( s, method, pathAndQuery, body, timeoutMs );
	if ( answer.status == 401 && answer.error == "clock" && answer.serverTime ) {
		s->clockOffset = answer.serverTime - (int64_t)time( NULL );
		answer = SJK_Http( s, method, pathAndQuery, body, timeoutMs );
	}
	return answer;
}

// What a player reads for a refusal.
static std::string SJK_Refusal( const sjkAnswer_t &a ) {
	if ( !a.status )
		return a.error;
	if ( a.error == "chat_quota" )
		return "you are sending too fast; wait a little";
	if ( a.error == "chat_duplicate" )
		return "you just said that";
	if ( a.error == "chat_links" )
		return "only verified SJK players may send links";
	if ( a.error == "chat_muted" )
		return "you are muted on SJK chat";
	if ( a.error == "chat_banned" )
		return "you are banned from SJK chat for now";
	if ( a.error == "not_registered" )
		return "this identity is not registered with the hub yet";
	return a.message.empty() ? a.error : a.message;
}

static std::string SJK_JSONBody( const std::string &text, const std::string &name ) {
	cJSON *root = cJSON_CreateObject();
	if ( !text.empty() )
		cJSON_AddStringToObject( root, "text", text.c_str() );
	if ( !name.empty() )
		cJSON_AddStringToObject( root, "name", name.c_str() );
	char *printed = cJSON_PrintUnformatted( root );
	std::string body = printed ? printed : "{}";
	cJSON_free( printed );
	cJSON_Delete( root );
	return body;
}

/*
===============================================================================

WORKERS

===============================================================================
*/

// One of the session's holders lets go; the last frees it, and its key with it.
static void SJK_Release( sjkSession_t *s ) {
	if ( --s->holders == 0 )
		delete s;
}

// The hub cannot be reached: what waits to be sent goes no later, since it would go out
// long after it was said, and nowhere else, not being SJK chat's. Each line is told.
// Called with s->lock held.
static void SJK_DropOutgoing( sjkSession_t *s ) {
	for ( const sjkOutgoing_t &item : s->outgoing )
		s->lines.push_back( { S_COLOR_YELLOW "SJK chat: not sent, the hub could not be reached: " S_COLOR_WHITE + item.gameText, std::string() } );
	s->outgoing.clear();
}

static void SJK_Say( sjkSession_t *s, const std::string &line ) {
	std::lock_guard<std::mutex> hold( s->lock );
	s->lines.push_back( { line, std::string() } );
}

// Sleeps up to ms, waking early when the session stops.
static void SJK_Nap( sjkSession_t *s, int ms ) {
	for ( int slept = 0; slept < ms && !s->stop; slept += 100 )
		Sleep( 100 );
}

// A GIPHY media file's name (`giphy.gif`, `200w.webp`, `giphy-downsized.gif`) and the
// `v1.<token>` segment of GIPHY's share links, as SJK reads them (chat_gifs/link.rs).
static bool SJK_GifFileName( const std::string &file ) {
	if ( file.empty() || file.size() > 48 || file.find( '.' ) == std::string::npos )
		return false;
	for ( unsigned char c : file ) {
		if ( !isalnum( c ) && c != '.' && c != '_' && c != '-' )
			return false;
	}
	return true;
}

static bool SJK_GifShareToken( const std::string &token ) {
	if ( token.compare( 0, 3, "v1." ) || token.size() < 4 || token.size() > 259 )
		return false;
	for ( size_t i = 3; i < token.size(); i++ ) {
		const unsigned char c = token[i];
		if ( !isalnum( c ) && c != '_' && c != '-' && c != '=' )
			return false;
	}
	return true;
}

// A GIPHY id: 6 to 40 ASCII letters and digits, at least one a capital or a digit (a
// slug's last word such as `birthday` is none).
static bool SJK_GifId( const std::string &id ) {
	if ( id.size() < 6 || id.size() > 40 )
		return false;
	bool mark = false;
	for ( unsigned char c : id ) {
		if ( c > 127 || !isalnum( c ) )
			return false;
		mark = mark || isupper( c ) || isdigit( c );
	}
	return mark;
}

// Whether `word` (its sentence's end punctuation left out) links to a GIPHY GIF, as SJK
// tells one (chat_gifs/link.rs, `parse`): https:// or http://, the host whole with no
// user or port, and one of `giphy.com/gifs|stickers/<slug>-<id>`, `giphy.com/embed/<id>`
// (also www.), `media[0-4].giphy.com/media/[v1.<token>/]<id>/<file>`, `i.giphy.com/<id>.gif`
// (.webp) or `i.giphy.com/media/[v1.<token>/]<id>/<file>`; query and fragment ignored.
static bool SJK_GiphyLink( const std::string &word ) {
	std::string rest;
	if ( !Q_stricmpn( word.c_str(), "https://", 8 ) )
		rest = word.substr( 8 );
	else if ( !Q_stricmpn( word.c_str(), "http://", 7 ) )
		rest = word.substr( 7 );
	else
		return false;
	rest = rest.substr( 0, rest.find_first_of( "?#" ) );
	const size_t slash = rest.find( '/' );
	std::string host = rest.substr( 0, slash );
	const std::string path = slash == std::string::npos ? std::string() : rest.substr( slash + 1 );
	if ( host.find_first_of( "@:" ) != std::string::npos )
		return false;
	for ( char &c : host )
		c = (char)tolower( (unsigned char)c );
	std::vector<std::string> segments;
	size_t at = 0;
	for ( ;; ) {
		const size_t end = path.find( '/', at );
		segments.push_back( path.substr( at, end == std::string::npos ? std::string::npos : end - at ) );
		if ( end == std::string::npos )
			break;
		at = end + 1;
	}
	if ( segments.size() > 1 && segments.back().empty() )
		segments.pop_back();	// one trailing slash
	for ( const std::string &segment : segments ) {
		if ( segment.empty() )
			return false;
	}
	auto media = [&segments]( void ) {
		if ( segments.size() == 3 && segments[0] == "media" )
			return SJK_GifFileName( segments[2] ) && SJK_GifId( segments[1] );
		if ( segments.size() == 4 && segments[0] == "media" )
			return SJK_GifShareToken( segments[1] ) && SJK_GifFileName( segments[3] ) && SJK_GifId( segments[2] );
		return false;
	};
	if ( host == "giphy.com" || host == "www.giphy.com" ) {
		if ( segments.size() == 2 && ( segments[0] == "gifs" || segments[0] == "stickers" ) ) {
			const size_t dash = segments[1].rfind( '-' );
			return SJK_GifId( dash == std::string::npos ? segments[1] : segments[1].substr( dash + 1 ) );
		}
		return segments.size() == 2 && segments[0] == "embed" && SJK_GifId( segments[1] );
	}
	if ( host == "media.giphy.com" || ( host.size() == 16 && !host.compare( 0, 5, "media" ) && host[5] >= '0' && host[5] <= '4' && !host.compare( 6, 10, ".giphy.com" ) ) )
		return media();
	if ( host == "i.giphy.com" ) {
		if ( segments.size() == 1 ) {
			const std::string &file = segments[0];
			for ( const char *extension : { ".gif", ".webp" } ) {
				const size_t length = strlen( extension );
				if ( file.size() > length && !file.compare( file.size() - length, length, extension ) )
					return SJK_GifId( file.substr( 0, file.size() - length ) );
			}
			return false;
		}
		return media();
	}
	return false;
}

// text (UTF-8) with each GIPHY link as <GIF>: JoF EJK shows no GIFs. Words split on
// spaces, as SJK's chat tidies them; the punctuation ending a sentence after a link stays.
static std::string SJK_GifText( const std::string &text ) {
	std::string out;
	size_t at = 0;
	for ( ;; ) {
		const size_t end = text.find( ' ', at );
		const std::string word = text.substr( at, end == std::string::npos ? std::string::npos : end - at );
		size_t length = word.find_last_not_of( ".,!?;:)]>'\"" );
		length = length == std::string::npos ? 0 : length + 1;
		if ( length && SJK_GiphyLink( word.substr( 0, length ) ) )
			out += "<GIF>" + word.substr( length );
		else
			out += word;
		if ( end == std::string::npos )
			break;
		out += ' ';
		at = end + 1;
	}
	return out;
}

// A text's letters and digits, lower case, without colour codes: what of an SJK chat
// message survives the game server's old code pages, to know its copy there (as SJK's
// server_copy.rs `words`).
static std::string SJK_Words( const std::string &text ) {
	std::string words;
	for ( size_t i = 0; i < text.size(); i++ ) {
		const unsigned char c = text[i];
		if ( c == Q_COLOR_ESCAPE && i + 1 < text.size() && text[i + 1] >= '0' && text[i + 1] <= '9' ) {
			i++;
			continue;
		}
		if ( c < 128 && isalnum( c ) )
			words += (char)tolower( c );
	}
	return words;
}

// One chat message of the feed in its parts (sjkLine_t; text empty when it has none):
// the name with its colours and a green + for a verified sender, the text without the
// colours its sender put in and with GIFs as <GIF>, and what tells its server's copy.
static sjkLine_t SJK_MessageParts( const cJSON *message ) {
	sjkLine_t line;
	const cJSON *name = cJSON_GetObjectItemCaseSensitive( message, "name" );
	const cJSON *text = cJSON_GetObjectItemCaseSensitive( message, "text" );
	if ( !cJSON_IsString( text ) )
		return line;
	std::string shown = SJK_FromUTF8( SJK_GifText( text->valuestring ) );
	Q_StripColor( &shown[0] );
	line.text = shown.c_str();
	line.who = cJSON_IsString( name ) ? SJK_FromUTF8( name->valuestring ) : "?";
	line.nameWords = cJSON_IsString( name ) ? SJK_Words( name->valuestring ) : std::string();
	if ( cJSON_IsTrue( cJSON_GetObjectItemCaseSensitive( message, "verified" ) ) )
		line.who += S_COLOR_GREEN "+";
	line.words = SJK_Words( text->valuestring );
	return line;
}

// A line as the console and the log show it, in the game's colours: a notice as it is; a
// message as the name, its label (SJK, or ALL/SJK when said in All chat too) and the text
// in the codes nearest SJK's golds, which the chat box and the SJK chat menu draw exactly.
static std::string SJK_ConsoleLine( const sjkLine_t &line ) {
	if ( line.who.empty() )
		return line.text;
	return line.who + S_COLOR_WHITE " " S_COLOR_ORANGE + ( line.inAll ? "ALL/SJK" : "SJK" ) + S_COLOR_WHITE ": " S_COLOR_YELLOW + line.text;
}

// Whether a feed read's answer is one (the hub's protocol, GET /v1/feed): an object with
// the id to read on from and the chat it brings, which the hub always sends.
static bool SJK_FeedAnswer( const cJSON *root ) {
	return cJSON_IsObject( root ) && cJSON_IsNumber( cJSON_GetObjectItemCaseSensitive( root, "next" ) )
		&& cJSON_IsArray( cJSON_GetObjectItemCaseSensitive( root, "chat" ) );
}

// The client a message of the feed came from, as the hub names it from the sender's
// User-Agent (its protocol, "Chat"): "sjk" or "ejk", and empty for any other or none.
static std::string SJK_MessageClient( const cJSON *message ) {
	const cJSON *client = cJSON_GetObjectItemCaseSensitive( message, "client" );
	if ( cJSON_IsString( client ) && ( !strcmp( client->valuestring, "sjk" ) || !strcmp( client->valuestring, "ejk" ) ) )
		return client->valuestring;
	return std::string();
}

// The id the hub gave a message (in the feed, or in its answer to the sender), in
// decimal, empty for none.
static std::string SJK_MessageId( const cJSON *message ) {
	const cJSON *id = cJSON_GetObjectItemCaseSensitive( message, "id" );
	if ( !cJSON_IsNumber( id ) || id->valuedouble < 1 )
		return std::string();
	return SJK_Format( "%lld", (long long)id->valuedouble );
}

// The end of the server's copy of an SJK chat message (docs/sjk-chat.md, "Said on a
// server too", as SJK does): the message's id in colour codes, which no client draws:
// ^9, then ^ and the number of digits, then ^ and each digit. Empty for an id of more
// than 9 digits, whose copy goes as a plain line.
static std::string SJK_ServerMarker( const std::string &id ) {
	if ( id.empty() || id.size() > 9 || id.find_first_not_of( "0123456789" ) != std::string::npos )
		return std::string();
	std::string marker = "^9^" + std::string( 1, (char)( '0' + id.size() ) );
	for ( char digit : id )
		marker += std::string( "^" ) + digit;
	return marker;
}

// What starts each piece of the server's copy of a message SJK chat took, so players on
// any client read it as an SJK line: SJK in SJK's gold, then All chat's green again.
static const char		SJK_SERVER_TAG[]	= S_COLOR_YELLOW "SJK" S_COLOR_GREEN " ";

// The pieces a line goes to a game server's chat in. A server keeps SJK_SERVER_SAY_MAX
// bytes of a say, counted as CL_SayToServer quotes them (% takes 3 and " 2), so a
// longer line is cut, at a space where one is near and never inside a colour code.
static std::vector<std::string> SJK_ServerPieces( const std::string &text, size_t room = SJK_SERVER_SAY_MAX ) {
	std::vector<std::string> pieces;
	size_t at = 0;
	while ( at < text.size() ) {
		while ( at < text.size() && text[at] == ' ' )
			at++;
		size_t end = at, cost = 0, space = std::string::npos;
		while ( end < text.size() ) {
			const size_t take = text[end] == '%' ? 3 : text[end] == '"' ? 2 : 1;
			if ( cost + take > room )
				break;
			if ( text[end] == ' ' )
				space = end;
			cost += take;
			end++;
		}
		if ( end < text.size() ) {
			if ( space != std::string::npos && space > at + ( end - at ) / 2 )
				end = space;
			else if ( end > at + 1 && text[end - 1] == '^' )
				end--;
		}
		std::string piece = text.substr( at, end - at );
		while ( !piece.empty() && piece.back() == ' ' )
			piece.pop_back();
		if ( !piece.empty() )
			pieces.push_back( piece );
		at = end;
	}
	return pieces;
}

static void SJK_FeedWorker( sjkSession_t *s ) {
	int64_t after = 0;
	bool first = true;
	int backoff = 2000;
	while ( !s->stop ) {
		std::string name;
		bool registered;
		{
			std::lock_guard<std::mutex> hold( s->lock );
			name = s->name;
			registered = s->registered;
		}

		if ( !registered ) {
			sjkAnswer_t a = SJK_Call( s, "POST", "/v1/register", SJK_JSONBody( std::string(), name ) );
			if ( a.status == 200 ) {
				{
					std::lock_guard<std::mutex> hold( s->lock );
					s->registered = true;
					if ( !s->reachable )
						s->lines.push_back( { S_COLOR_YELLOW "SJK chat: connected again", std::string() } );
					s->reachable = true;
				}
				s->wake.notify_all();	// what waited to be sent can go now
			} else {
				{
					std::lock_guard<std::mutex> hold( s->lock );
					if ( s->reachable )
						s->lines.push_back( { S_COLOR_YELLOW "SJK chat: " + SJK_Refusal( a ), std::string() } );
					s->reachable = false;
					SJK_DropOutgoing( s );
				}
				SJK_Nap( s, backoff );
				backoff = backoff * 2 > 60000 ? 60000 : backoff * 2;
				continue;
			}
		}

		// The first read only learns where the conversation is, so it does not wait: a
		// message that ended its wait would be taken for what was said before.
		const DWORD start = GetTickCount();
		sjkAnswer_t a = SJK_Call( s, "GET", SJK_Format( "/v1/feed?after=%lld&wait=%d", (long long)after, first ? 0 : SJK_FEED_WAIT ),
			std::string(), SJK_FEED_TIMEOUT_MS );
		if ( s->stop )
			break;
		// Only a feed answer moves the reading on. Anything else, even with a 200, goes as
		// a failed read: taken for one, a garbled first answer would end the first read
		// without learning where the conversation is, and the backlog would come next as new.
		cJSON *root = a.status == 200 ? cJSON_ParseWithLength( a.body.data(), a.body.size() ) : NULL;
		if ( a.status == 200 && !SJK_FeedAnswer( root ) ) {
			a.status = 0;
			a.error = "the hub's answer could not be read";
		}
		if ( a.status == 200 ) {
			const cJSON *next = cJSON_GetObjectItemCaseSensitive( root, "next" );
			const cJSON *chat = cJSON_GetObjectItemCaseSensitive( root, "chat" );
			const cJSON *online = cJSON_GetObjectItemCaseSensitive( root, "online" );
			std::vector<sjkLine_t> shown;
			const cJSON *message;
			cJSON_ArrayForEach( message, chat ) {
				const cJSON *id = cJSON_GetObjectItemCaseSensitive( message, "id" );
				// What was said before this client was here is not replayed in the game,
				// only kept for the SJK chat menu.
				if ( first || ( cJSON_IsNumber( id ) && (int64_t)id->valuedouble > after ) ) {
					sjkLine_t line = SJK_MessageParts( message );
					if ( !line.text.empty() ) {
						line.client = SJK_MessageClient( message );
						line.id = SJK_MessageId( message );
						line.backlog = first;
						shown.push_back( line );
					}
				}
			}
			{
				std::lock_guard<std::mutex> hold( s->lock );
				for ( const sjkLine_t &line : shown )
					s->lines.push_back( line );
				if ( cJSON_IsNumber( online ) )
					s->online = (int)online->valuedouble;
				if ( !s->reachable )
					s->lines.push_back( { S_COLOR_YELLOW "SJK chat: connected again", std::string() } );
				s->reachable = true;
			}
			after = (int64_t)next->valuedouble;
			cJSON_Delete( root );
			first = false;
			backoff = 2000;
		} else {
			cJSON_Delete( root );
			{
				std::lock_guard<std::mutex> hold( s->lock );
				if ( s->reachable )
					s->lines.push_back( { S_COLOR_YELLOW "SJK chat: " + SJK_Refusal( a ), std::string() } );
				s->reachable = false;
				if ( a.error == "not_registered" )
					s->registered = false;
				SJK_DropOutgoing( s );
			}
			SJK_Nap( s, backoff );
			backoff = backoff * 2 > 60000 ? 60000 : backoff * 2;
			continue;
		}
		// The hub asks for 2 s between two reads at least.
		const DWORD took = GetTickCount() - start;
		if ( took < 2000 )
			SJK_Nap( s, (int)( 2000 - took ) );
	}
	SJK_Release( s );
}

static void SJK_SendWorker( sjkSession_t *s ) {
	DWORD lastSent = 0;
	bool sent = false;
	while ( !s->stop ) {
		sjkOutgoing_t item;
		std::string name;
		{
			// Only a registered key may write: what is said sooner waits for that.
			std::unique_lock<std::mutex> hold( s->lock );
			s->wake.wait( hold, [s]() { return s->stop || ( s->registered && !s->outgoing.empty() ); } );
			if ( s->stop )
				break;
			item = s->outgoing.front();
			s->outgoing.pop_front();
			name = s->name;
		}
		// Lines said in a burst go one by one, as the hub takes them.
		const DWORD since = GetTickCount() - lastSent;
		if ( sent && since < SJK_SEND_GAP_MS ) {
			SJK_Nap( s, (int)( SJK_SEND_GAP_MS - since ) );
			if ( s->stop )
				break;
		}
		sjkAnswer_t a = SJK_Call( s, "POST", "/v1/chat", SJK_JSONBody( item.text, name ) );
		lastSent = GetTickCount();
		sent = true;
		if ( a.status != 200 )
			SJK_Say( s, S_COLOR_YELLOW "SJK chat: not sent: " + SJK_Refusal( a ) );
		if ( !item.match.empty() && a.status == 200 ) {
			// Said in a match and taken: now the server's copy, with the id the hub gave the
			// message, for the main thread to say. A refused one goes nowhere.
			cJSON *root = cJSON_ParseWithLength( a.body.data(), a.body.size() );
			const std::string id = SJK_MessageId( root );
			cJSON_Delete( root );
			if ( !id.empty() ) {
				std::lock_guard<std::mutex> hold( s->lock );
				s->serverCopies.push_back( { item.gameText, item.match, id } );
			}
		}
	}
	SJK_Release( s );
}

/*
===============================================================================

CLIENT (main thread)

===============================================================================
*/

static cvar_t			*cl_sjkChat;
static cvar_t			*cl_hubUrl;
static sjkSession_t		*s_session;
static std::string		s_identityWhere, s_identityError, s_identityKeyId, s_agent;
// The SJK chat menu's history (ui's FEEDER_SJK_CHAT): the last lines, the backlog the
// hub gave when the reading began first, and how often it changed.
static const size_t		SJK_HISTORY_MAX = 100;
static std::deque<sjkLine_t>	s_history;
static int				s_historyRevision;

static void SJK_Remember( const sjkLine_t &line ) {
	if ( s_history.size() == SJK_HISTORY_MAX )
		s_history.pop_front();
	s_history.push_back( line );
	s_historyRevision++;
}

// Whether the history holds this message already, as a new session's backlog brings
// again what the hub still holds (SJK chat turned off and on, a new cl_hubUrl). Its id,
// sender and text all match, so another hub's message of the same id is not taken for it.
static bool SJK_Remembered( const sjkLine_t &line ) {
	if ( line.id.empty() )
		return false;
	for ( const sjkLine_t &old : s_history ) {
		if ( old.id == line.id && old.who == line.who && old.text == line.text )
			return true;
	}
	return false;
}

// A line of SJK chat's own (game text, without the newline): to the console, and to the
// menu's history, where what is typed there gets its answer.
static void SJK_Notice( const std::string &line ) {
	Com_Printf( "%s\n", line.c_str() );
	SJK_Remember( { line, std::string(), std::string() } );
}

static std::deque<std::string>	s_serverSays;	// pieces still to say in the server's chat
static std::string		s_serverSaysMatch;		// the match they are for
static int				s_serverSayAt;			// Sys_Milliseconds when the next may go

// The match this client is in, as the server's sv_serverid and map, which a new map
// changes; empty when it is in none (the menus, a demo, loading).
static std::string SJK_CurrentMatch( void ) {
	char serverId[64], mapname[128];
	if ( !CL_SJK_ServerSession( serverId, sizeof( serverId ), mapname, sizeof( mapname ) ) )
		return std::string();
	return std::string( serverId ) + "\n" + mapname;
}

// The server's copy of a message the hub took, in the says it goes as: each piece after
// SJK_SERVER_TAG and before the marker of the id the hub gave it. None without a marker
// (no id, or one of more than 9 digits): a copy that could not be told from a plain line
// is not said, as a line SJK chat did not take is said nowhere.
static std::vector<std::string> SJK_ServerCopyLines( const std::string &gameText, const std::string &id ) {
	const std::string marker = SJK_ServerMarker( id );
	std::vector<std::string> says;
	if ( marker.empty() )
		return says;
	for ( const std::string &piece : SJK_ServerPieces( gameText, SJK_SERVER_SAY_MAX - strlen( SJK_SERVER_TAG ) - marker.size() ) )
		says.push_back( SJK_SERVER_TAG + piece + marker );
	return says;
}

// Queues the server's copy of a message said in match, with the id the hub gave it,
// unless that match is over.
static void SJK_QueueServerCopy( const std::string &gameText, const std::string &match, const std::string &id ) {
	if ( match.empty() || match != SJK_CurrentMatch() )
		return;
	if ( s_serverSaysMatch != match )
		s_serverSays.clear();
	s_serverSaysMatch = match;
	for ( const std::string &say : SJK_ServerCopyLines( gameText, id ) )
		s_serverSays.push_back( say );
}

static std::string SJK_WornName( void ) {
	std::string name = SJK_ToUTF8( Cvar_VariableString( "name" ), Cvar_VariableString( "se_language" ) );
	return name;
}

// https://host[:port] with no path (the path is signed), or http on this machine
// only, for a test hub. False (and why) for anything else.
static bool SJK_HubAddress( const char *url, std::string *hub, std::string *why ) {
	std::string address = url;
	while ( !address.empty() && address.back() == '/' )
		address.pop_back();
	const bool secure = !address.compare( 0, 8, "https://" );
	const bool local = !address.compare( 0, 17, "http://127.0.0.1:" ) || !address.compare( 0, 17, "http://localhost:" );
	const size_t hostAt = secure ? 8 : 7;
	if ( ( !secure && !local ) || address.find( '/', hostAt ) != std::string::npos || address.size() <= hostAt ) {
		*why = "cl_hubUrl must be https://host[:port] with no path (or http://127.0.0.1:<port> for a local test hub)";
		return false;
	}
	*hub = address;
	return true;
}

// The User-Agent, in the shape SJK's own takes (the hub's protocol, "Client"):
// EJK/<release tag> (<commit>) for a release, EJK/<version>-dev for a build from
// source, so the hub's staff can tell a JoF release from a changed or local build.
// The hub keeps it as a hint, never as proof, and refuses nothing on it.
static std::string SJK_UserAgent( const char *tag, const char *version, const char *commit ) {
	std::string agent = "EJK/";
	if ( tag[0] )
		agent += tag;
	else if ( version[0] && Q_stricmp( version, "dev" ) )
		agent += std::string( version ) + "-dev";
	else
		agent += "dev";
	const size_t length = strlen( commit );
	bool hex = length >= 7 && length <= 40;
	for ( size_t i = 0; hex && i < length; i++ )
		hex = isxdigit( (unsigned char)commit[i] ) != 0;
	if ( hex )
		agent += std::string( " (" ) + commit + ")";
	return agent;
}

static void SJK_Stop( void ) {
	sjkSession_t *s = s_session;
	if ( !s )
		return;
	s_session = NULL;
	s->stop = true;
	s->wake.notify_all();
	SJK_Release( s );		// its workers let go as they end; a long poll may hold one 40 s
}

static void SJK_Start( void ) {
	SJK_Stop();
	if ( !cl_sjkChat->integer )
		return;

	sjkSession_t *s = new sjkSession_t;
	std::string why;
	if ( !SJK_HubAddress( cl_hubUrl->string, &s->hub, &why ) ) {
		Com_Printf( S_COLOR_YELLOW "SJK chat: %s\n", why.c_str() );
		delete s;
		return;
	}
	s_identityError.clear();
	if ( !SJK_LoadIdentity( &s->id, &s_identityWhere, &s_identityError ) ) {
		Com_Printf( S_COLOR_YELLOW "SJK chat: %s\n", s_identityError.c_str() );
		delete s;
		return;
	}
	s_identityKeyId = s->id.keyId;
	s_agent = SJK_UserAgent( JOF_RELEASE_TAG, JOF_VERSION_STRING, JOF_COMMIT_SHA_SHORT );
	s->agent = SJK_Widen( s_agent );
	s->name = SJK_WornName();
	s->holders = 3;
	std::thread( SJK_FeedWorker, s ).detach();
	std::thread( SJK_SendWorker, s ).detach();
	s_session = s;
}

// Says text (game text, as the chat field holds it) on SJK chat and, in a match, in the
// game server's chat too, so its players on other clients read it: once the hub took
// it, with the message's id at the end (SJK_ServerMarker), so SJK readers on that
// server show the message once (cgame's CG_SJKServerLine). A line the hub refuses or
// cannot get is said nowhere. False, with the reason printed, when it went nowhere.
qboolean CL_SJK_SayText( const char *text ) {
	if ( !cl_sjkChat || !cl_sjkChat->integer ) {
		SJK_Notice( S_COLOR_YELLOW "SJK chat is off (Setup, SJK, or cl_sjkChat 1 turns it on)" );
		return qfalse;
	}
	if ( !s_session ) {
		SJK_Notice( S_COLOR_YELLOW "SJK chat is not running" + ( s_identityError.empty() ? std::string() : ": " + s_identityError ) );
		return qfalse;
	}
	std::string prepared, why;
	if ( !SJK_PrepareText( SJK_ToUTF8( text, Cvar_VariableString( "se_language" ) ), &prepared, &why ) ) {
		SJK_Notice( S_COLOR_YELLOW "SJK chat: " + why );
		return qfalse;
	}
	const std::string match = SJK_CurrentMatch();
	bool reachable;
	{
		// Not kept for later while the hub cannot be reached: it would go out long
		// after it was said.
		std::lock_guard<std::mutex> hold( s_session->lock );
		reachable = s_session->reachable;
		if ( reachable ) {
			s_session->outgoing.push_back( { prepared, text, match } );
			s_session->name = SJK_WornName();
		}
	}
	if ( !reachable ) {
		// Not SJK chat's then, so nowhere else either.
		SJK_Notice( S_COLOR_YELLOW "SJK chat: not sent: the hub cannot be reached right now" );
		return qfalse;
	}
	s_session->wake.notify_all();
	return qtrue;
}

static void CL_SJK_Say_f( void ) {
	if ( Cmd_Argc() < 2 ) {
		Com_Printf( "usage: sjksay <text>\n" );
		return;
	}
	CL_SJK_SayText( Cmd_ArgsFrom( 1 ) );
}

static void CL_SJK_Status_f( void ) {
	if ( !cl_sjkChat->integer ) {
		Com_Printf( "SJK chat is off (Setup, SJK, or cl_sjkChat 1 turns it on).\n" );
		return;
	}
	if ( !s_session ) {
		Com_Printf( "SJK chat is not running%s%s\n", s_identityError.empty() ? "." : ": ", s_identityError.c_str() );
		return;
	}
	bool registered, reachable;
	int online;
	{
		std::lock_guard<std::mutex> hold( s_session->lock );
		registered = s_session->registered;
		reachable = s_session->reachable;
		online = s_session->online;
	}
	Com_Printf( "SJK chat: hub %s, identity %s, key id %s, as %s\n", s_session->hub.c_str(), s_identityWhere.c_str(), s_identityKeyId.c_str(), s_agent.c_str() );
	Com_Printf( "  %s, %s, %d reading now\n", registered ? "registered" : "not registered yet", reachable ? "connected" : "not connected", online );
}

// cl_sjkChatStatus, which the SJK chat menu shows: whether SJK chat is on and connected,
// and how many read it.
static void SJK_UpdateStatus( void ) {
	std::string status;
	if ( !cl_sjkChat->integer ) {
		status = S_COLOR_YELLOW "SJK chat is off";
	} else if ( !s_session ) {
		status = S_COLOR_YELLOW "SJK chat is not running" + ( s_identityError.empty() ? std::string() : ": " + s_identityError );
	} else {
		std::lock_guard<std::mutex> hold( s_session->lock );
		if ( !s_session->reachable )
			status = S_COLOR_YELLOW "Not connected to the SJK hub";
		else if ( !s_session->registered )
			status = S_COLOR_WHITE "Connecting to the SJK hub...";
		else
			status = SJK_Format( S_COLOR_GREEN "Connected" S_COLOR_WHITE ", %d reading now", s_session->online );
	}
	if ( strcmp( Cvar_VariableString( "cl_sjkChatStatus" ), status.c_str() ) )
		Cvar_Set2( "cl_sjkChatStatus", status.c_str(), CVAR_ROM, qtrue );
}

void CL_SJK_Init( void ) {
	Cvar_Get( "cl_sjkChatStatus", "", CVAR_ROM, "How SJK chat is doing, as its menu shows it" );
	cl_sjkChat = Cvar_Get( "cl_sjkChat", "1", CVAR_ARCHIVE_ND, "SJK chat: talk with SJK players on any server (messagemode6, sjksay)" );
	cl_hubUrl = Cvar_Get( "cl_hubUrl", SJK_DEFAULT_HUB, CVAR_ARCHIVE_ND | CVAR_PROTECTED, "The SJK hub SJK chat goes through" );
	Cmd_AddCommand( "sjksay", CL_SJK_Say_f, "Say something on SJK chat" );
	Cmd_AddCommand( "sjkstatus", CL_SJK_Status_f, "Show how SJK chat is doing" );
	cl_sjkChat->modified = qfalse;
	cl_hubUrl->modified = qfalse;
	SJK_Start();
}

void CL_SJK_Frame( void ) {
	if ( cl_sjkChat->modified || cl_hubUrl->modified ) {
		cl_sjkChat->modified = qfalse;
		cl_hubUrl->modified = qfalse;
		SJK_Start();
	}

	std::deque<sjkLine_t> lines;
	std::deque<sjkServerCopy_t> copies;
	if ( s_session ) {
		std::lock_guard<std::mutex> hold( s_session->lock );
		lines.swap( s_session->lines );
		copies.swap( s_session->serverCopies );
	}
	for ( const sjkServerCopy_t &copy : copies )
		SJK_QueueServerCopy( copy.gameText, copy.match, copy.id );

	// The server's copy of what was said, a piece a second, while still in that match
	// (leaving it, or a new map, drops the rest).
	if ( !s_serverSays.empty() ) {
		if ( SJK_CurrentMatch() != s_serverSaysMatch ) {
			s_serverSays.clear();
		} else if ( Sys_Milliseconds() - s_serverSayAt >= 0 ) {
			CL_SayToServer( s_serverSays.front().c_str() );
			s_serverSays.pop_front();
			s_serverSayAt = Sys_Milliseconds() + SJK_SERVER_SAY_GAP_MS;
		}
	}

	// Into the chat box (which copies a line to the console) when the cgame takes them,
	// with what tells the server's copy of a message and the icon of the client it came
	// from, else to the console and the notify lines. Every line joins the SJK chat menu's
	// history; the backlog only that, and only what it does not hold yet.
	for ( const sjkLine_t &line : lines ) {
		if ( line.backlog ) {
			if ( !SJK_Remembered( line ) )
				SJK_Remember( line );
			continue;
		}
		SJK_Remember( line );
		// The console copy, the chat box's or this one, starts with the icon too.
		Con_SetPrintIcon( line.client == "sjk" ? CON_ICON_SJK : line.client == "ejk" ? CON_ICON_EJK : CON_ICON_NONE );
		const sjkChatLine_t parts = { line.who.c_str(), line.text.c_str(), line.nameWords.c_str(), line.words.c_str(), line.client.c_str(), line.id.c_str() };
		if ( !CL_SJK_ToChatBox( &parts ) )
			Com_Printf( "%s\n", SJK_ConsoleLine( line ).c_str() );
		Con_SetPrintIcon( CON_ICON_NONE );
	}

	SJK_UpdateStatus();
}

// How many lines the SJK chat menu's history holds; *revision changes with it.
int CL_SJK_ChatLineCount( int *revision ) {
	if ( revision )
		*revision = s_historyRevision;
	return (int)s_history.size();
}

// The history's line `index` (oldest first), in game text: who said it (with colours;
// "" for SJK chat's own notice), its text, the client it came from ("sjk", "ejk" or "")
// and whether it was said in All chat too. qfalse past the end.
qboolean CL_SJK_ChatLine( int index, char *who, int whoSize, char *text, int textSize, char *client, int clientSize, qboolean *inAll ) {
	if ( index < 0 || index >= (int)s_history.size() )
		return qfalse;
	const sjkLine_t &line = s_history[index];
	Q_strncpyz( who, line.who.c_str(), whoSize );
	Q_strncpyz( text, line.text.c_str(), textSize );
	Q_strncpyz( client, line.client.c_str(), clientSize );
	*inAll = (qboolean)line.inAll;
	return qtrue;
}

// The cgame saw message `id` said in the game server's All chat too: it reads ALL/SJK in
// the SJK chat menu's history as well.
void CL_SJK_SaidInAll( const char *id ) {
	if ( !id || !id[0] )
		return;
	for ( sjkLine_t &line : s_history ) {
		if ( line.id == id && !line.inAll ) {
			line.inAll = true;
			s_historyRevision++;
		}
	}
}

void CL_SJK_Shutdown( void ) {
	Cmd_RemoveCommand( "sjksay" );
	Cmd_RemoveCommand( "sjkstatus" );
	SJK_Stop();
}

#else // !_WIN32

// Only Windows has SJK chat for now.
qboolean CL_SJK_SayText( const char *text ) {
	Com_Printf( "SJK chat is only on Windows for now.\n" );
	return qfalse;
}

void CL_SJK_Init( void ) {
}

void CL_SJK_Frame( void ) {
}

void CL_SJK_Shutdown( void ) {
}

int CL_SJK_ChatLineCount( int *revision ) {
	if ( revision )
		*revision = 0;
	return 0;
}

qboolean CL_SJK_ChatLine( int index, char *who, int whoSize, char *text, int textSize, char *client, int clientSize, qboolean *inAll ) {
	return qfalse;
}

void CL_SJK_SaidInAll( const char *id ) {
}

#endif // _WIN32
