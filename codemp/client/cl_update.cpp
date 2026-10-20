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

// cl_update.cpp -- the in-client updater (docs/updater.md)
//
// At start-up a worker thread looks for the newest stable release on GitHub, or
// the newest beta with betaupdates 1, and the client prints "An update is
// available" when it is newer than this build. /update downloads it, checks it
// against the release's signed manifest and stages it in jofupdate/. The files
// are swapped in at the next launch, before anything has a pk3 open (Windows
// cannot replace a file in use), and /update again restarts into it.
//
// Anyone with write access to the repository can replace a release's files, so
// nothing is trusted that the release signing key did not sign: the manifest's
// Ed25519 signature, its tag, channel and platform, and the SHA-512 of the
// archive and of every file are all checked. Alpha builds are never offered.
//
// A manifest can also list files earlier releases shipped that this one no
// longer has. Those are removed with the same swap, but only where the installed
// file is still byte for byte a version JoF shipped: a player's own copy stays.

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

#include <algorithm>
#include <atomic>
#include <cstdarg>
#include <functional>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

#include <zlib.h>
#include "cJSON.h"
#include "monocypher-ed25519.h"
#include "jofupdatekey.h"

#define UPDATE_REPOSITORY	"https://github.com/JediofFreedom/JoF_EJK"
#define UPDATE_API			"https://api.github.com/repos/JediofFreedom/JoF_EJK"
#ifdef _WIN64
#define UPDATE_PLATFORM		"windows-x86_64"
#else
#define UPDATE_PLATFORM		"windows-x86"
#endif
// The release file listing this platform's update, beside its .sig.
#define UPDATE_MANIFEST		"JoF-update-" UPDATE_PLATFORM ".json"
// The manifest layout this client reads; a newer one means a newer updater.
#define UPDATE_FORMAT		1
// Under the install folder: download/, pending/ and the notes between starts.
#define UPDATE_DIR			L"jofupdate"

#ifndef WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY
#define WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY 4
#endif
#ifndef WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3
#define WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3 0x00002000
#endif

static const size_t		MAX_MANIFEST_BYTES	= 1024 * 1024;
static const size_t		MAX_REFS_BYTES		= 4 * 1024 * 1024;
static const uint64_t	MAX_ARCHIVE_BYTES	= 1024ull * 1024 * 1024;
static const int		MAX_REMOVALS		= 256;		// files one manifest may remove
static const int		MAX_REMOVAL_VERSIONS	= 1024;	// versions of one such file it may name
static const int		MAX_BETA_TRIES		= 8;		// newest beta tags tried for one with update files
static const int		MAX_APPLY_ATTEMPTS	= 3;		// starts that may fail to swap before the update is dropped
static const DWORD		APPLY_WAIT_MS		= 10000;	// how long a swap waits for the exiting client to let go

/*
===============================================================================

RELEASE VERSIONS

===============================================================================
*/

enum releaseKind_t { RELEASE_STABLE, RELEASE_BETA, RELEASE_ALPHA };

struct releaseVersion_t {
	int				major, minor, patch;
	releaseKind_t	kind;
	int				number;		// N of -beta.N or -alpha.N
};

// Reads a release tag: 1.6.2, 1.6.2-beta.4 or 1.6.2-alpha.9.
static bool Update_ParseTag( const char *tag, releaseVersion_t *v ) {
	auto number = [&tag]( int *out ) {
		if ( *tag < '0' || *tag > '9' )
			return false;
		long long n = 0;
		while ( *tag >= '0' && *tag <= '9' ) {
			n = n * 10 + ( *tag++ - '0' );
			if ( n > 1000000000 )
				return false;
		}
		*out = (int)n;
		return true;
	};

	memset( v, 0, sizeof( *v ) );
	if ( !number( &v->major ) || *tag++ != '.' || !number( &v->minor ) || *tag++ != '.' || !number( &v->patch ) )
		return false;
	v->kind = RELEASE_STABLE;
	if ( !*tag )
		return true;
	if ( !strncmp( tag, "-beta.", 6 ) ) {
		v->kind = RELEASE_BETA;
		tag += 6;
	} else if ( !strncmp( tag, "-alpha.", 7 ) ) {
		v->kind = RELEASE_ALPHA;
		tag += 7;
	} else {
		return false;
	}
	return number( &v->number ) && !*tag;
}

// Whether release c comes after the installed release i. A beta is built on top
// of the stable release it names, so 1.6.2-beta.4 is newer than 1.6.2, and alpha
// numbers have nothing to do with beta ones.
static bool Update_IsNewer( const releaseVersion_t &c, const releaseVersion_t &i ) {
	if ( c.major != i.major )
		return c.major > i.major;
	if ( c.minor != i.minor )
		return c.minor > i.minor;
	if ( c.patch != i.patch )
		return c.patch > i.patch;
	if ( i.kind == RELEASE_STABLE )
		return c.kind == RELEASE_BETA;
	if ( i.kind == RELEASE_BETA )
		return c.kind == RELEASE_BETA && c.number > i.number;
	return false;
}

static std::string Update_Format( const char *fmt, ... ) {
	char text[1024];
	va_list args;
	va_start( args, fmt );
	vsnprintf( text, sizeof( text ), fmt, args );
	va_end( args );
	return text;
}

// A tag as players see it: 1.6.2-beta.4 is "1.6.2 beta 4".
static std::string Update_Display( const std::string &tag ) {
	releaseVersion_t v;
	if ( !Update_ParseTag( tag.c_str(), &v ) || v.kind == RELEASE_STABLE )
		return tag;
	return Update_Format( "%d.%d.%d %s %d", v.major, v.minor, v.patch, v.kind == RELEASE_BETA ? "beta" : "alpha", v.number );
}

/*
===============================================================================

FILES

All paths are wide, so a non-Latin install folder works. Manifest paths are
relative to the install folder with forward slashes.

===============================================================================
*/

static std::wstring Update_Widen( const std::string &s ) {
	if ( s.empty() )
		return std::wstring();
	int n = MultiByteToWideChar( CP_UTF8, 0, s.data(), (int)s.size(), NULL, 0 );
	std::wstring w( n, L'\0' );
	MultiByteToWideChar( CP_UTF8, 0, s.data(), (int)s.size(), &w[0], n );
	return w;
}

static std::string Update_Narrow( const std::wstring &w ) {
	if ( w.empty() )
		return std::string();
	int n = WideCharToMultiByte( CP_UTF8, 0, w.data(), (int)w.size(), NULL, 0, NULL, NULL );
	std::string s( n, '\0' );
	WideCharToMultiByte( CP_UTF8, 0, w.data(), (int)w.size(), &s[0], n, NULL, NULL );
	return s;
}

// The running program, which an update replaces.
static std::wstring Update_ProgramPath( void ) {
	std::vector<wchar_t> path( MAX_PATH );
	for ( ;; ) {
		DWORD n = GetModuleFileNameW( NULL, path.data(), (DWORD)path.size() );
		if ( n == 0 )
			return std::wstring();
		if ( n < path.size() )
			return std::wstring( path.data(), n );
		path.resize( path.size() * 2 );
	}
}

// The folder the update writes into: the program's own, GameData.
static std::wstring Update_InstallDir( void ) {
	std::wstring program = Update_ProgramPath();
	size_t slash = program.find_last_of( L"\\/" );
	return slash == std::wstring::npos ? std::wstring() : program.substr( 0, slash );
}

static std::wstring Update_Join( const std::wstring &dir, const std::string &relative ) {
	std::wstring path = Update_Widen( relative );
	std::replace( path.begin(), path.end(), L'/', L'\\' );
	return dir + L"\\" + path;
}

static bool Update_Exists( const std::wstring &path ) {
	return GetFileAttributesW( path.c_str() ) != INVALID_FILE_ATTRIBUTES;
}

static bool Update_MakeDirs( const std::wstring &dir ) {
	if ( CreateDirectoryW( dir.c_str(), NULL ) || GetLastError() == ERROR_ALREADY_EXISTS )
		return true;
	if ( GetLastError() != ERROR_PATH_NOT_FOUND )
		return false;
	size_t slash = dir.find_last_of( L"\\/" );
	if ( slash == std::wstring::npos || !Update_MakeDirs( dir.substr( 0, slash ) ) )
		return false;
	return CreateDirectoryW( dir.c_str(), NULL ) || GetLastError() == ERROR_ALREADY_EXISTS;
}

static void Update_MakeParentDirs( const std::wstring &path ) {
	size_t slash = path.find_last_of( L"\\/" );
	if ( slash != std::wstring::npos )
		Update_MakeDirs( path.substr( 0, slash ) );
}

static void Update_RemoveTree( const std::wstring &dir ) {
	WIN32_FIND_DATAW found;
	HANDLE search = FindFirstFileW( ( dir + L"\\*" ).c_str(), &found );
	if ( search != INVALID_HANDLE_VALUE ) {
		do {
			std::wstring name = found.cFileName;
			if ( name == L"." || name == L".." )
				continue;
			std::wstring path = dir + L"\\" + name;
			if ( found.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY ) {
				Update_RemoveTree( path );
			} else {
				SetFileAttributesW( path.c_str(), FILE_ATTRIBUTE_NORMAL );
				DeleteFileW( path.c_str() );
			}
		} while ( FindNextFileW( search, &found ) );
		FindClose( search );
	}
	RemoveDirectoryW( dir.c_str() );
}

static bool Update_ReadFile( const std::wstring &path, std::string *text ) {
	HANDLE file = CreateFileW( path.c_str(), GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;
	LARGE_INTEGER size;
	bool ok = GetFileSizeEx( file, &size ) && size.QuadPart < 1024 * 1024;
	if ( ok ) {
		text->assign( (size_t)size.QuadPart, '\0' );
		DWORD got = 0;
		ok = !size.QuadPart || ( ReadFile( file, &( *text )[0], (DWORD)size.QuadPart, &got, NULL ) && got == size.QuadPart );
	}
	CloseHandle( file );
	return ok;
}

// Writes path whole or not at all, through path.tmp.
static bool Update_WriteFile( const std::wstring &path, const std::string &text ) {
	std::wstring temp = path + L".tmp";
	HANDLE file = CreateFileW( temp.c_str(), GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;
	DWORD wrote = 0;
	bool ok = ( text.empty() || ( WriteFile( file, text.data(), (DWORD)text.size(), &wrote, NULL ) && wrote == text.size() ) ) && FlushFileBuffers( file );
	CloseHandle( file );
	if ( ok )
		ok = MoveFileExW( temp.c_str(), path.c_str(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH ) != 0;
	if ( !ok )
		DeleteFileW( temp.c_str() );
	return ok;
}

static std::vector<std::string> Update_Lines( const std::string &text ) {
	std::vector<std::string> lines;
	size_t start = 0;
	while ( start < text.size() ) {
		size_t end = text.find( '\n', start );
		if ( end == std::string::npos )
			end = text.size();
		std::string line = text.substr( start, end - start );
		if ( !line.empty() && line.back() == '\r' )
			line.pop_back();
		if ( !line.empty() )
			lines.push_back( line );
		start = end + 1;
	}
	return lines;
}

static bool Update_Writable( const std::wstring &dir ) {
	HANDLE probe = CreateFileW( ( dir + L"\\jofupdate-probe.tmp" ).c_str(), GENERIC_WRITE, 0, NULL, CREATE_ALWAYS,
		FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE, NULL );
	if ( probe == INVALID_HANDLE_VALUE )
		return false;
	CloseHandle( probe );
	return true;
}

static bool Update_HashFile( const std::wstring &path, uint64_t *size, uint8_t sha512[64] ) {
	HANDLE file = CreateFileW( path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, NULL,
		OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;
	crypto_sha512_ctx ctx;
	crypto_sha512_init( &ctx );
	std::vector<uint8_t> buffer( 256 * 1024 );
	*size = 0;
	bool ok = true;
	for ( ;; ) {
		DWORD got = 0;
		if ( !ReadFile( file, buffer.data(), (DWORD)buffer.size(), &got, NULL ) ) {
			ok = false;
			break;
		}
		if ( !got )
			break;
		crypto_sha512_update( &ctx, buffer.data(), got );
		*size += got;
	}
	crypto_sha512_final( &ctx, sha512 );
	CloseHandle( file );
	return ok;
}

// A file's size and last-write time, to tell at the swap whether it changed since
// it was checked. False for a folder or a file that is not there.
static bool Update_Stamp( const std::wstring &path, uint64_t *size, uint64_t *written ) {
	WIN32_FILE_ATTRIBUTE_DATA data;
	if ( !GetFileAttributesExW( path.c_str(), GetFileExInfoStandard, &data ) || ( data.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY ) )
		return false;
	*size = ( (uint64_t)data.nFileSizeHigh << 32 ) | data.nFileSizeLow;
	*written = ( (uint64_t)data.ftLastWriteTime.dwHighDateTime << 32 ) | data.ftLastWriteTime.dwLowDateTime;
	return true;
}

// The only files an update may write: programs and libraries beside the client
// and pk3s in base/ and EternalJK/, so not even a signed manifest can drop a
// config or reach outside the install folder.
static bool Update_ValidPath( const std::string &path ) {
	if ( path.empty() || path.size() > 128 )
		return false;
	for ( char c : path ) {
		if ( !( ( c >= 'a' && c <= 'z' ) || ( c >= 'A' && c <= 'Z' ) || ( c >= '0' && c <= '9' ) || c == '.' || c == '_' || c == '-' || c == '/' ) )
			return false;
	}
	size_t slash = path.find( '/' );
	if ( slash == 0 )
		return false;
	std::string dir = slash == std::string::npos ? "" : path.substr( 0, slash );
	std::string name = slash == std::string::npos ? path : path.substr( slash + 1 );
	if ( name.empty() || name[0] == '.' || name.find( '/' ) != std::string::npos )
		return false;
	auto endsWith = [&name]( const char *ext ) {
		size_t n = strlen( ext );
		return name.size() > n && !Q_stricmp( name.c_str() + name.size() - n, ext );
	};
	if ( dir.empty() )
		return endsWith( ".exe" ) || endsWith( ".dll" );
	if ( !Q_stricmp( dir.c_str(), "base" ) || !Q_stricmp( dir.c_str(), "EternalJK" ) )
		return endsWith( ".pk3" );
	return false;
}

// Retail game files that sit where an update may write. JoF never ships them, so
// no manifest may remove them, however it is signed.
static bool Update_RetailFile( const std::string &path ) {
	static const char *retail[] = { "base/assets0.pk3", "base/assets1.pk3", "base/assets2.pk3", "base/assets3.pk3", "jamp.exe", "jasp.exe" };
	for ( const char *name : retail ) {
		if ( !Q_stricmp( path.c_str(), name ) )
			return true;
	}
	return false;
}

/*
===============================================================================

MANIFESTS

A release that the updater can install carries, per platform:

  JoF-update-<platform>.zip       its files, paths relative to GameData
  JoF-update-<platform>.json      the manifest: tag, channel, platform, and the
                                  size and SHA-512 of the zip and of every file
  JoF-update-<platform>.json.sig  the Ed25519 signature of the manifest's bytes

The manifest's optional "remove" list names the files earlier releases shipped
that this one no longer has, each with the size and SHA-512 of every version of
it JoF shipped:

  "remove": [ { "path": "EternalJK/old.pk3", "versions": [ { "size": 123, "sha512": "..." } ] } ]

scripts/update/make_update_package.py writes them; build.yml signs them.

===============================================================================
*/

struct updateFile_t {
	std::string		path;
	uint64_t		size;
	uint8_t			sha512[64];
};

struct updateVersion_t {	// one version of a file, as JoF shipped it
	uint64_t		size;
	uint8_t			sha512[64];
};

struct updateRemove_t {		// a file this release no longer ships
	std::string						path;
	std::vector<updateVersion_t>	versions;	// every version of it JoF shipped
};

struct updateRelease_t {
	std::string					tag;
	releaseVersion_t			version;
	std::string					archive;		// the zip's file name in the release
	uint64_t					archiveSize;
	uint8_t						archiveSha512[64];
	std::vector<updateFile_t>	files;
	std::vector<updateRemove_t>	remove;
};

static bool Update_ParseHex( const char *hex, uint8_t *out, size_t size ) {
	if ( !hex || strlen( hex ) != size * 2 )
		return false;
	for ( size_t i = 0; i < size * 2; i++ ) {
		char c = hex[i];
		int digit = ( c >= '0' && c <= '9' ) ? c - '0' : ( c >= 'a' && c <= 'f' ) ? c - 'a' + 10 : ( c >= 'A' && c <= 'F' ) ? c - 'A' + 10 : -1;
		if ( digit < 0 )
			return false;
		if ( i & 1 )
			out[i / 2] |= digit;
		else
			out[i / 2] = (uint8_t)( digit << 4 );
	}
	return true;
}

struct updateKey_t {
	uint8_t		bytes[32];
};

// The keys this build accepts manifests from (all zeros is no key).
static std::vector<updateKey_t> Update_BuildKeys( void ) {
	std::vector<updateKey_t> keys;
	const char *hex = JOF_UPDATE_PUBKEYS;
	for ( size_t at = 0; at + 64 <= strlen( hex ); at += 64 ) {
		updateKey_t key;
		std::string one( hex + at, 64 );
		if ( !Update_ParseHex( one.c_str(), key.bytes, sizeof( key.bytes ) ) )
			continue;
		for ( uint8_t b : key.bytes ) {
			if ( b ) {
				keys.push_back( key );
				break;
			}
		}
	}
	return keys;
}

static bool Update_Signed( const std::string &signature, const std::string &message ) {
	for ( const updateKey_t &key : Update_BuildKeys() ) {
		if ( !crypto_ed25519_check( (const uint8_t *)signature.data(), key.bytes, (const uint8_t *)message.data(), message.size() ) )
			return true;
	}
	return false;
}

static bool Update_JSONSize( const cJSON *item, uint64_t limit, uint64_t *size ) {
	if ( !cJSON_IsNumber( item ) || item->valuedouble < 0 || item->valuedouble > (double)limit || item->valuedouble != (double)(uint64_t)item->valuedouble )
		return false;
	*size = (uint64_t)item->valuedouble;
	return true;
}

static bool Update_JSONHash( const cJSON *item, uint8_t sha512[64] ) {
	return cJSON_IsString( item ) && Update_ParseHex( item->valuestring, sha512, 64 );
}

static bool Update_ReadManifest( const cJSON *root, updateRelease_t *r, std::string *error ) {
	const cJSON *format = cJSON_GetObjectItemCaseSensitive( root, "format" );
	if ( !cJSON_IsNumber( format ) || format->valueint != UPDATE_FORMAT ) {
		*error = "this update needs a newer updater; download it from its release page";
		return false;
	}

	const cJSON *tag = cJSON_GetObjectItemCaseSensitive( root, "tag" );
	if ( !cJSON_IsString( tag ) || !Update_ParseTag( tag->valuestring, &r->version ) ) {
		*error = "the update manifest has no valid tag";
		return false;
	}
	r->tag = tag->valuestring;
	if ( r->version.kind == RELEASE_ALPHA ) {
		*error = "alpha builds are never installed by /update";
		return false;
	}

	const cJSON *channel = cJSON_GetObjectItemCaseSensitive( root, "channel" );
	const char *expected = r->version.kind == RELEASE_BETA ? "beta" : "stable";
	if ( !cJSON_IsString( channel ) || strcmp( channel->valuestring, expected ) ) {
		*error = "the update manifest's channel does not match its tag";
		return false;
	}

	const cJSON *platform = cJSON_GetObjectItemCaseSensitive( root, "platform" );
	if ( !cJSON_IsString( platform ) || strcmp( platform->valuestring, UPDATE_PLATFORM ) ) {
		*error = "the update manifest is for another platform";
		return false;
	}

	const cJSON *archive = cJSON_GetObjectItemCaseSensitive( root, "archive" );
	const cJSON *archiveName = cJSON_GetObjectItemCaseSensitive( archive, "name" );
	if ( !cJSON_IsString( archiveName ) || !Update_JSONSize( cJSON_GetObjectItemCaseSensitive( archive, "size" ), MAX_ARCHIVE_BYTES, &r->archiveSize )
		|| !r->archiveSize || !Update_JSONHash( cJSON_GetObjectItemCaseSensitive( archive, "sha512" ), r->archiveSha512 ) ) {
		*error = "the update manifest does not describe its archive";
		return false;
	}
	r->archive = archiveName->valuestring;
	size_t length = r->archive.size();
	bool plainName = length > 4 && length < 128 && !Q_stricmp( r->archive.c_str() + length - 4, ".zip" );
	for ( char c : r->archive ) {
		if ( !( ( c >= 'a' && c <= 'z' ) || ( c >= 'A' && c <= 'Z' ) || ( c >= '0' && c <= '9' ) || c == '.' || c == '_' || c == '-' ) )
			plainName = false;
	}
	if ( !plainName ) {
		*error = "the update manifest names an odd archive";
		return false;
	}

	const cJSON *files = cJSON_GetObjectItemCaseSensitive( root, "files" );
	const cJSON *item;
	r->files.clear();
	cJSON_ArrayForEach( item, files ) {
		updateFile_t file;
		const cJSON *path = cJSON_GetObjectItemCaseSensitive( item, "path" );
		if ( !cJSON_IsString( path ) || !Update_ValidPath( path->valuestring )
			|| !Update_JSONSize( cJSON_GetObjectItemCaseSensitive( item, "size" ), MAX_ARCHIVE_BYTES, &file.size )
			|| !Update_JSONHash( cJSON_GetObjectItemCaseSensitive( item, "sha512" ), file.sha512 ) ) {
			*error = Update_Format( "the update manifest lists a file the updater may not install (%s)",
				cJSON_IsString( path ) ? path->valuestring : "?" );
			return false;
		}
		file.path = path->valuestring;
		for ( const updateFile_t &other : r->files ) {
			if ( !Q_stricmp( other.path.c_str(), file.path.c_str() ) ) {
				*error = "the update manifest lists a file twice";
				return false;
			}
		}
		r->files.push_back( file );
	}
	if ( !cJSON_IsArray( files ) || r->files.empty() ) {
		*error = "the update manifest lists no files";
		return false;
	}

	const cJSON *remove = cJSON_GetObjectItemCaseSensitive( root, "remove" );
	r->remove.clear();
	if ( remove && ( !cJSON_IsArray( remove ) || cJSON_GetArraySize( remove ) > MAX_REMOVALS ) ) {
		*error = "the update manifest's remove list is malformed";
		return false;
	}
	cJSON_ArrayForEach( item, remove ) {
		updateRemove_t old;
		const cJSON *path = cJSON_GetObjectItemCaseSensitive( item, "path" );
		const cJSON *versions = cJSON_GetObjectItemCaseSensitive( item, "versions" );
		bool ok = cJSON_IsString( path ) && Update_ValidPath( path->valuestring ) && !Update_RetailFile( path->valuestring )
			&& cJSON_IsArray( versions ) && cJSON_GetArraySize( versions ) > 0 && cJSON_GetArraySize( versions ) <= MAX_REMOVAL_VERSIONS;
		const cJSON *version, *known = ok ? versions : NULL;
		cJSON_ArrayForEach( version, known ) {
			updateVersion_t v;
			if ( !Update_JSONSize( cJSON_GetObjectItemCaseSensitive( version, "size" ), MAX_ARCHIVE_BYTES, &v.size )
				|| !Update_JSONHash( cJSON_GetObjectItemCaseSensitive( version, "sha512" ), v.sha512 ) ) {
				ok = false;
				break;
			}
			old.versions.push_back( v );
		}
		if ( !ok ) {
			*error = Update_Format( "the update manifest asks to remove a file the updater may not touch (%s)",
				cJSON_IsString( path ) ? path->valuestring : "?" );
			return false;
		}
		old.path = path->valuestring;
		for ( const updateFile_t &file : r->files ) {
			if ( !Q_stricmp( file.path.c_str(), old.path.c_str() ) ) {
				*error = "the update manifest both installs and removes " + old.path;
				return false;
			}
		}
		for ( const updateRemove_t &other : r->remove ) {
			if ( !Q_stricmp( other.path.c_str(), old.path.c_str() ) ) {
				*error = "the update manifest lists a file to remove twice";
				return false;
			}
		}
		r->remove.push_back( old );
	}
	return true;
}

static bool Update_ParseManifest( const std::string &text, updateRelease_t *release, std::string *error ) {
	cJSON *root = cJSON_ParseWithLength( text.data(), text.size() );
	if ( !root ) {
		*error = "the update manifest is not valid JSON";
		return false;
	}
	bool ok = Update_ReadManifest( root, release, error );
	cJSON_Delete( root );
	return ok;
}

/*
===============================================================================

HTTP (WinHTTP: system TLS and proxy settings, nothing to ship)

===============================================================================
*/

typedef std::function<bool( const uint8_t *data, size_t size )> httpSink_t;

// Set at shutdown: the worker stops between reads. A request still waiting on
// the network is left to its timeouts, in a worker the client no longer waits for.
static std::atomic<bool>	s_cancel;

static std::string Update_NetError( DWORD code ) {
	switch ( code ) {
	case ERROR_WINHTTP_NAME_NOT_RESOLVED:
	case ERROR_WINHTTP_CANNOT_CONNECT:
		return "could not reach GitHub (are you online?)";
	case ERROR_WINHTTP_TIMEOUT:
		return "GitHub took too long to answer";
	case ERROR_WINHTTP_SECURE_FAILURE:
		return "the secure connection to GitHub failed (is Windows up to date?)";
	case ERROR_WINHTTP_OPERATION_CANCELLED:
		return "cancelled";
	default:
		return Update_Format( "the connection to GitHub failed (error %lu)", code );
	}
}

// GET url, handing the body to sink. Returns the HTTP status, or 0 with *error
// set when no answer came or sink refused the data (sink sets its own error).
static int Update_HttpGet( const std::string &url, const wchar_t *headers, const httpSink_t &sink, std::string *error ) {
	std::wstring wideUrl = Update_Widen( url );
	wchar_t host[256], path[2048];
	URL_COMPONENTS parts;
	memset( &parts, 0, sizeof( parts ) );
	parts.dwStructSize = sizeof( parts );
	parts.lpszHostName = host;
	parts.dwHostNameLength = ARRAY_LEN( host );
	parts.lpszUrlPath = path;
	parts.dwUrlPathLength = ARRAY_LEN( path );
	if ( !WinHttpCrackUrl( wideUrl.c_str(), 0, 0, &parts ) ) {
		*error = "bad address " + url;
		return 0;
	}
	const bool secure = parts.nScheme == INTERNET_SCHEME_HTTPS;

	std::wstring agent = Update_Widen( "JoF-EJK-Updater/" + std::string( JOF_RELEASE_TAG[0] ? JOF_RELEASE_TAG : "dev" ) );
	HINTERNET session = WinHttpOpen( agent.c_str(), WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0 );
	if ( !session ) // before Windows 8.1
		session = WinHttpOpen( agent.c_str(), WINHTTP_ACCESS_TYPE_DEFAULT_PROXY, WINHTTP_NO_PROXY_NAME, WINHTTP_NO_PROXY_BYPASS, 0 );
	if ( !session ) {
		*error = Update_NetError( GetLastError() );
		return 0;
	}
	// GitHub wants TLS 1.2 or later, which older Windows does not offer by default.
	DWORD protocols = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2 | WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_3;
	if ( !WinHttpSetOption( session, WINHTTP_OPTION_SECURE_PROTOCOLS, &protocols, sizeof( protocols ) ) ) {
		protocols = WINHTTP_FLAG_SECURE_PROTOCOL_TLS1_2;
		WinHttpSetOption( session, WINHTTP_OPTION_SECURE_PROTOCOLS, &protocols, sizeof( protocols ) );
	}
	WinHttpSetTimeouts( session, 15000, 15000, 30000, 30000 );

	int status = 0;
	HINTERNET connection = WinHttpConnect( session, host, parts.nPort, 0 );
	HINTERNET request = connection ? WinHttpOpenRequest( connection, L"GET", path, NULL, WINHTTP_NO_REFERER,
		WINHTTP_DEFAULT_ACCEPT_TYPES, secure ? WINHTTP_FLAG_SECURE : 0 ) : NULL;

	if ( !request ) {
		*error = Update_NetError( GetLastError() );
	} else if ( s_cancel ) {
		*error = "cancelled";
	} else if ( !WinHttpSendRequest( request, headers ? headers : WINHTTP_NO_ADDITIONAL_HEADERS, headers ? (DWORD)-1L : 0,
			WINHTTP_NO_REQUEST_DATA, 0, 0, 0 ) || !WinHttpReceiveResponse( request, NULL ) ) {
		*error = Update_NetError( GetLastError() );
	} else {
		DWORD code = 0, size = sizeof( code );
		WinHttpQueryHeaders( request, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_HEADER_NAME_BY_INDEX,
			&code, &size, WINHTTP_NO_HEADER_INDEX );
		status = (int)code;
		if ( status == 200 ) {
			std::vector<uint8_t> buffer( 64 * 1024 );
			for ( ;; ) {
				DWORD got = 0;
				if ( !WinHttpReadData( request, buffer.data(), (DWORD)buffer.size(), &got ) ) {
					*error = s_cancel ? "cancelled" : "the download broke off";
					status = 0;
					break;
				}
				if ( !got )
					break;
				if ( s_cancel ) {
					*error = "cancelled";
					status = 0;
					break;
				}
				if ( !sink( buffer.data(), got ) ) {
					status = 0;
					break;
				}
			}
		}
	}

	if ( request )
		WinHttpCloseHandle( request );
	if ( connection )
		WinHttpCloseHandle( connection );
	WinHttpCloseHandle( session );
	return status;
}

static int Update_HttpGetText( const std::string &url, size_t limit, const wchar_t *headers, std::string *body, std::string *error ) {
	body->clear();
	return Update_HttpGet( url, headers, [&]( const uint8_t *data, size_t size ) {
		if ( body->size() + size > limit ) {
			*error = "GitHub's answer is too large";
			return false;
		}
		body->append( (const char *)data, size );
		return true;
	}, error );
}

/*
===============================================================================

ZIP (stored or deflated entries, no zip64: what make_update_package.py writes)

===============================================================================
*/

struct zipEntry_t {
	std::string	name;
	uint16_t	method;
	uint32_t	crc;
	uint32_t	compressedSize;
	uint32_t	size;
	uint32_t	offset;			// of its local header
};

static uint16_t Update_LE16( const uint8_t *p ) { return (uint16_t)( p[0] | p[1] << 8 ); }
static uint32_t Update_LE32( const uint8_t *p ) { return (uint32_t)p[0] | (uint32_t)p[1] << 8 | (uint32_t)p[2] << 16 | (uint32_t)p[3] << 24; }

static bool Update_ReadAt( HANDLE file, uint64_t offset, void *buffer, DWORD size ) {
	LARGE_INTEGER at;
	at.QuadPart = (LONGLONG)offset;
	DWORD got = 0;
	return SetFilePointerEx( file, at, NULL, FILE_BEGIN ) && ReadFile( file, buffer, size, &got, NULL ) && got == size;
}

static bool Update_ZipDirectory( HANDLE zip, std::vector<zipEntry_t> *entries ) {
	LARGE_INTEGER fileSize;
	if ( !GetFileSizeEx( zip, &fileSize ) || fileSize.QuadPart < 22 )
		return false;
	const uint64_t size = (uint64_t)fileSize.QuadPart;

	// The end of central directory record, found from the back past any comment.
	DWORD tail = (DWORD)( size < 22 + 65535 ? size : 22 + 65535 );
	std::vector<uint8_t> end( tail );
	if ( !Update_ReadAt( zip, size - tail, end.data(), tail ) )
		return false;
	int at = -1;
	for ( int i = (int)tail - 22; i >= 0; i-- ) {
		if ( Update_LE32( &end[i] ) == 0x06054b50 ) {
			at = i;
			break;
		}
	}
	if ( at < 0 )
		return false;
	const uint8_t *record = &end[at];
	uint16_t count = Update_LE16( record + 10 );
	uint32_t dirSize = Update_LE32( record + 12 ), dirOffset = Update_LE32( record + 16 );
	if ( Update_LE16( record + 4 ) || Update_LE16( record + 6 ) || count == 0xffff || dirOffset == 0xffffffff
		|| dirSize > 16 * 1024 * 1024 || (uint64_t)dirOffset + dirSize > size )
		return false;

	std::vector<uint8_t> dir( dirSize );
	if ( dirSize && !Update_ReadAt( zip, dirOffset, dir.data(), dirSize ) )
		return false;
	size_t p = 0;
	for ( int i = 0; i < count; i++ ) {
		if ( p + 46 > dir.size() || Update_LE32( &dir[p] ) != 0x02014b50 )
			return false;
		const uint8_t *header = &dir[p];
		uint16_t flags = Update_LE16( header + 8 );
		uint16_t nameLength = Update_LE16( header + 28 ), extraLength = Update_LE16( header + 30 ), commentLength = Update_LE16( header + 32 );
		if ( p + 46 + nameLength + extraLength + commentLength > dir.size() )
			return false;
		zipEntry_t entry;
		entry.method = Update_LE16( header + 10 );
		entry.crc = Update_LE32( header + 16 );
		entry.compressedSize = Update_LE32( header + 20 );
		entry.size = Update_LE32( header + 24 );
		entry.offset = Update_LE32( header + 42 );
		entry.name.assign( (const char *)header + 46, nameLength );
		if ( ( flags & 1 ) || ( entry.method != 0 && entry.method != 8 ) || entry.size == 0xffffffff || entry.compressedSize == 0xffffffff )
			return false;
		entries->push_back( entry );
		p += 46 + nameLength + extraLength + commentLength;
	}
	return true;
}

// Unpacks entry into out, checking its CRC and returning the SHA-512 of what it wrote.
static bool Update_ZipExtract( HANDLE zip, const zipEntry_t &entry, const std::wstring &out, uint8_t sha512[64] ) {
	uint8_t local[30];
	if ( !Update_ReadAt( zip, entry.offset, local, sizeof( local ) ) || Update_LE32( local ) != 0x04034b50 )
		return false;
	LARGE_INTEGER data;
	data.QuadPart = (LONGLONG)entry.offset + 30 + Update_LE16( local + 26 ) + Update_LE16( local + 28 );
	if ( !SetFilePointerEx( zip, data, NULL, FILE_BEGIN ) )
		return false;

	HANDLE file = CreateFileW( out.c_str(), GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;

	crypto_sha512_ctx ctx;
	crypto_sha512_init( &ctx );
	uLong crc = crc32( 0L, Z_NULL, 0 );
	uint64_t written = 0;
	auto emit = [&]( const uint8_t *bytes, size_t n ) {
		DWORD wrote = 0;
		written += n;
		crypto_sha512_update( &ctx, bytes, n );
		crc = crc32( crc, bytes, (uInt)n );
		return written <= entry.size && WriteFile( file, bytes, (DWORD)n, &wrote, NULL ) && wrote == n;
	};

	z_stream stream;
	memset( &stream, 0, sizeof( stream ) );
	bool ok = entry.method == 0 || inflateInit2( &stream, -MAX_WBITS ) == Z_OK;
	const bool inflating = ok && entry.method == 8;
	bool finished = false;
	uint64_t remaining = entry.compressedSize;
	std::vector<uint8_t> input( 64 * 1024 ), output( 256 * 1024 );

	while ( ok && !finished ) {
		DWORD want = (DWORD)( remaining < input.size() ? remaining : input.size() );
		DWORD got = 0;
		if ( want && ( !ReadFile( zip, input.data(), want, &got, NULL ) || got != want ) ) {
			ok = false;
			break;
		}
		remaining -= got;

		if ( !inflating ) {
			ok = !got || emit( input.data(), got );
			finished = !remaining;
			continue;
		}

		stream.next_in = input.data();
		stream.avail_in = got;
		for ( ;; ) {
			stream.next_out = output.data();
			stream.avail_out = (uInt)output.size();
			int result = inflate( &stream, Z_NO_FLUSH );
			size_t produced = output.size() - stream.avail_out;
			if ( produced && !emit( output.data(), produced ) ) {
				ok = false;
				break;
			}
			if ( result == Z_STREAM_END ) {
				finished = true;
				break;
			}
			if ( result == Z_BUF_ERROR )	// needs more input
				break;
			if ( result != Z_OK ) {
				ok = false;
				break;
			}
			if ( stream.avail_out && !stream.avail_in )
				break;
		}
		if ( ok && !finished && !remaining )	// the data ended before the stream did
			ok = false;
	}

	if ( inflating )
		inflateEnd( &stream );
	crypto_sha512_final( &ctx, sha512 );
	ok = FlushFileBuffers( file ) && ok;
	CloseHandle( file );
	return ok && written == entry.size && crc == entry.crc;
}

/*
===============================================================================

STATE SHARED WITH THE WORKER

The worker never touches the engine: it gets its settings as a copy, and the
main thread reads what it did in CL_Update_Frame and prints it.

===============================================================================
*/

enum updateState_t {
	UPDATE_IDLE,			// nothing checked yet
	UPDATE_CHECKING,
	UPDATE_UP_TO_DATE,
	UPDATE_AVAILABLE,		// a newer release is out; /update downloads it
	UPDATE_DOWNLOADING,
	UPDATE_STAGED,			// downloaded and checked: installs at the next start
	UPDATE_FAILED,
	UPDATE_DISABLED			// this build cannot update itself (message says why)
};

struct updateShared_t {
	std::mutex			lock;
	updateState_t		state = UPDATE_IDLE;
	int					generation = 0;		// rises with every change of state
	std::string			message;			// why it failed or is disabled, or a note on what it staged
	bool				haveRelease = false;
	updateRelease_t		release;			// the newer release, once found
	uint64_t			done = 0, total = 0;	// download progress
};

struct updateSettings_t {
	std::string		installed;		// the tag this build is (cl_updateAs to pretend)
	bool			beta;			// betaupdates
	std::string		repository;		// release downloads
	std::string		api;			// GitHub's API for the repository
	std::wstring	dir;			// the install folder
};

// Both are left to the process to free: a worker still running at exit uses them.
static updateShared_t		*s_shared;
static std::thread			*s_worker;
static std::atomic<bool>	s_busy;

static void Update_Set( updateState_t state, const std::string &message = std::string(), const updateRelease_t *release = NULL ) {
	std::lock_guard<std::mutex> hold( s_shared->lock );
	s_shared->state = state;
	s_shared->message = message;
	if ( release ) {
		s_shared->release = *release;
		s_shared->haveRelease = true;
	}
	s_shared->generation++;
}

// Only called once the last job has set its result, so at worst the join below
// waits for that worker's last instructions.
static void Update_Start( std::function<void()> job ) {
	if ( s_worker ) {
		s_worker->join();
		delete s_worker;
	}
	s_busy = true;
	s_cancel = false;
	s_worker = new std::thread( [job]() {
		SetThreadPriority( GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL );
		job();
		s_busy = false;
	} );
}

/*
===============================================================================

CHECK (worker)

===============================================================================
*/

enum fetchResult_t { FETCH_OK, FETCH_MISSING, FETCH_FAILED };

// The tag a manifest names, read before its signature is checked: only to find
// the signature in that same release.
static std::string Update_ManifestTag( const std::string &manifest ) {
	std::string tag;
	cJSON *root = cJSON_ParseWithLength( manifest.data(), manifest.size() );
	const cJSON *item = cJSON_GetObjectItemCaseSensitive( root, "tag" );
	releaseVersion_t v;
	if ( cJSON_IsString( item ) && Update_ParseTag( item->valuestring, &v ) )
		tag = item->valuestring;
	cJSON_Delete( root );
	return tag;
}

// The release whose manifest is at url, once its signature checks out.
// FETCH_MISSING: that release has no update files, or not all of them yet
// (publish-update-files.sh uploads the manifest last).
static fetchResult_t Update_FetchRelease( const std::string &repository, const std::string &url, updateRelease_t *release, std::string *error ) {
	std::string manifest, signature;
	int status = Update_HttpGetText( url, MAX_MANIFEST_BYTES, NULL, &manifest, error );
	if ( status == 404 )
		return FETCH_MISSING;
	if ( status != 200 ) {
		if ( status )
			*error = Update_Format( "GitHub answered %d for the update manifest", status );
		return FETCH_FAILED;
	}
	// From the release the manifest names, not through releases/latest again,
	// which may have moved on to a newer release in between.
	const std::string tag = Update_ManifestTag( manifest );
	if ( tag.empty() ) {
		*error = "the update manifest has no valid tag";
		return FETCH_FAILED;
	}
	status = Update_HttpGetText( repository + "/releases/download/" + tag + "/" UPDATE_MANIFEST ".sig", 64, NULL, &signature, error );
	if ( status == 404 )
		return FETCH_MISSING;
	if ( status != 200 || signature.size() != 64 ) {
		if ( status )
			*error = "the update manifest has no valid signature; nothing was installed";
		return FETCH_FAILED;
	}
	if ( !Update_Signed( signature, manifest ) ) {
		*error = "the update is not signed with the JoF release key; nothing was installed";
		return FETCH_FAILED;
	}
	return Update_ParseManifest( manifest, release, error ) ? FETCH_OK : FETCH_FAILED;
}

// The beta numbers of the tags GitHub lists under prefix (1.6.2-beta.), newest first.
static std::vector<int> Update_BetaNumbers( const std::string &json, const std::string &prefix ) {
	std::vector<int> numbers;
	const std::string head = "refs/tags/" + prefix;
	cJSON *root = cJSON_ParseWithLength( json.data(), json.size() );
	const cJSON *item;
	cJSON_ArrayForEach( item, root ) {
		const cJSON *ref = cJSON_GetObjectItemCaseSensitive( item, "ref" );
		releaseVersion_t v;
		if ( cJSON_IsString( ref ) && !strncmp( ref->valuestring, head.c_str(), head.size() )
			&& Update_ParseTag( ref->valuestring + 10, &v ) && v.kind == RELEASE_BETA )
			numbers.push_back( v.number );
	}
	cJSON_Delete( root );
	std::sort( numbers.begin(), numbers.end(), []( int a, int b ) { return a > b; } );
	return numbers;
}

static void Update_Check( const updateSettings_t &s ) {
	releaseVersion_t installed;
	Update_ParseTag( s.installed.c_str(), &installed );
	std::string error;

	// The newest stable release: GitHub's latest one, no API (and no rate limit) needed.
	updateRelease_t stable;
	bool haveStable = Update_FetchRelease( s.repository, s.repository + "/releases/latest/download/" UPDATE_MANIFEST, &stable, &error ) == FETCH_OK
		&& stable.version.kind == RELEASE_STABLE;

	// The newest beta on top of it: the highest -beta.N tag whose release has good
	// update files. A tag is pushed before its release is published, and anyone who
	// can push may push a bogus one, so several are tried.
	updateRelease_t beta;
	bool haveBeta = false;
	if ( s.beta ) {
		const releaseVersion_t &base = haveStable ? stable.version : installed;
		std::string prefix = Update_Format( "%d.%d.%d-beta.", base.major, base.minor, base.patch );
		std::string refs, refsError;
		int status = Update_HttpGetText( s.api + "/git/matching-refs/tags/" + prefix, MAX_REFS_BYTES,
			L"Accept: application/vnd.github+json\r\n", &refs, &refsError );
		if ( status == 200 ) {
			std::vector<int> numbers = Update_BetaNumbers( refs, prefix );
			for ( size_t i = 0; i < numbers.size() && (int)i < MAX_BETA_TRIES; i++ ) {
				std::string tag = prefix + std::to_string( numbers[i] );
				fetchResult_t result = Update_FetchRelease( s.repository, s.repository + "/releases/download/" + tag + "/" UPDATE_MANIFEST, &beta, &error );
				if ( result == FETCH_OK && beta.tag == tag ) {
					haveBeta = true;
					break;
				}
				if ( result == FETCH_OK )
					error = "the " + tag + " release's manifest names another release";
				// Missing or bad: the next one down may be fine.
			}
		} else {
			error = status ? Update_Format( "GitHub refused the beta lookup (%d); try again later", status ) : refsError;
		}
	}

	const updateRelease_t *best = NULL;
	if ( haveStable && Update_IsNewer( stable.version, installed ) )
		best = &stable;
	if ( haveBeta && Update_IsNewer( beta.version, installed ) && ( !best || Update_IsNewer( beta.version, best->version ) ) )
		best = &beta;

	if ( best )
		Update_Set( UPDATE_AVAILABLE, std::string(), best );
	else if ( !error.empty() && !haveStable && !haveBeta )
		Update_Set( UPDATE_FAILED, error );
	else
		Update_Set( UPDATE_UP_TO_DATE );
}

/*
===============================================================================

DOWNLOAD AND STAGE (worker)

jofupdate/pending/files/ holds the files that differ from the installed ones,
and jofupdate/pending/ready.txt lists them, then the files to remove with the
size and last-write time they had when they were found to be JoF's. The swap
renames it applying.txt and adds an "aside" line before it moves each file aside
(Update_Swap):

  jofupdate 1
  tag 1.6.2
  attempts 0
  file eternaljk.x86_64.exe
  file EternalJK/jofclient-win-x86_64.pk3
  remove 123456 133420000000000000 EternalJK/old.pk3
  aside eternaljk.x86_64.exe

===============================================================================
*/

struct updateRemoval_t {
	std::string		path;
	uint64_t		size;
	uint64_t		written;	// last-write time (FILETIME) when it was checked
};

struct updatePending_t {
	std::string						tag;
	int								attempts = 0;
	std::vector<std::string>		files;
	std::vector<updateRemoval_t>	remove;
	std::vector<std::string>		aside;		// the swap's journal: files it moved aside
};

// "remove <size> <written> <path>"
static bool Update_ReadRemoval( const std::string &line, updateRemoval_t *removal ) {
	const char *at = line.c_str() + 7;
	char *end;
	if ( *at < '0' || *at > '9' )
		return false;
	removal->size = _strtoui64( at, &end, 10 );
	if ( *end != ' ' || end[1] < '0' || end[1] > '9' )
		return false;
	removal->written = _strtoui64( end + 1, &end, 10 );
	if ( *end != ' ' )
		return false;
	removal->path = end + 1;
	return Update_ValidPath( removal->path ) && !Update_RetailFile( removal->path );
}

static bool Update_ReadPending( const std::wstring &path, updatePending_t *p ) {
	std::string text;
	if ( !Update_ReadFile( path, &text ) )
		return false;
	std::vector<std::string> lines = Update_Lines( text );
	if ( lines.empty() || lines[0] != "jofupdate 1" )
		return false;
	for ( size_t i = 1; i < lines.size(); i++ ) {
		const std::string &line = lines[i];
		updateRemoval_t removal;
		if ( !line.compare( 0, 4, "tag " ) )
			p->tag = line.substr( 4 );
		else if ( !line.compare( 0, 9, "attempts " ) )
			p->attempts = atoi( line.c_str() + 9 );
		else if ( !line.compare( 0, 5, "file " ) && Update_ValidPath( line.substr( 5 ) ) )
			p->files.push_back( line.substr( 5 ) );
		else if ( !line.compare( 0, 7, "remove " ) && Update_ReadRemoval( line, &removal ) )
			p->remove.push_back( removal );
		else if ( !line.compare( 0, 6, "aside " ) && Update_ValidPath( line.substr( 6 ) ) )
			p->aside.push_back( line.substr( 6 ) );
		else
			return false;
	}
	return !p->tag.empty() && ( !p->files.empty() || !p->remove.empty() );
}

static bool Update_WritePending( const std::wstring &path, const updatePending_t &p ) {
	std::string text = "jofupdate 1\ntag " + p.tag + "\nattempts " + std::to_string( p.attempts ) + "\n";
	for ( const std::string &file : p.files )
		text += "file " + file + "\n";
	for ( const updateRemoval_t &removal : p.remove )
		text += "remove " + std::to_string( removal.size ) + " " + std::to_string( removal.written ) + " " + removal.path + "\n";
	for ( const std::string &file : p.aside )
		text += "aside " + file + "\n";
	return Update_WriteFile( path, text );
}

static bool Update_DownloadArchive( const updateSettings_t &s, const updateRelease_t &r, const std::wstring &archive, std::string *error ) {
	std::wstring part = archive + L".part";
	HANDLE file = CreateFileW( part.c_str(), GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE ) {
		*error = "cannot write the download in " + Update_Narrow( s.dir );
		return false;
	}

	crypto_sha512_ctx ctx;
	crypto_sha512_init( &ctx );
	uint64_t done = 0;
	std::string url = s.repository + "/releases/download/" + r.tag + "/" + r.archive;
	int status = Update_HttpGet( url, NULL, [&]( const uint8_t *data, size_t size ) {
		DWORD wrote = 0;
		if ( done + size > r.archiveSize ) {
			*error = "the download is larger than its manifest says; nothing was installed";
			return false;
		}
		if ( !WriteFile( file, data, (DWORD)size, &wrote, NULL ) || wrote != size ) {
			*error = "cannot write the download (is the disk full?)";
			return false;
		}
		crypto_sha512_update( &ctx, data, size );
		done += size;
		std::lock_guard<std::mutex> hold( s_shared->lock );
		s_shared->done = done;
		return true;
	}, error );
	CloseHandle( file );

	uint8_t sha512[64];
	crypto_sha512_final( &ctx, sha512 );
	bool ok = false;
	if ( status != 200 ) {
		if ( status )
			*error = Update_Format( "GitHub answered %d for the download", status );
	} else if ( done != r.archiveSize ) {
		*error = "the download is incomplete; nothing was installed";
	} else if ( memcmp( sha512, r.archiveSha512, sizeof( sha512 ) ) ) {
		*error = "the download does not match its manifest; nothing was installed";
	} else {
		ok = MoveFileExW( part.c_str(), archive.c_str(), MOVEFILE_REPLACE_EXISTING ) != 0;
		if ( !ok )
			*error = "cannot keep the download";
	}
	if ( !ok )
		DeleteFileW( part.c_str() );
	return ok;
}

// Whether the file at path, of that size, is one of these versions. It is only
// hashed when its size is one of theirs.
static bool Update_Shipped( const std::wstring &path, uint64_t size, const std::vector<updateVersion_t> &versions ) {
	bool hashed = false;
	uint64_t got;
	uint8_t sha512[64];
	for ( const updateVersion_t &v : versions ) {
		if ( v.size != size )
			continue;
		if ( !hashed ) {
			if ( !Update_HashFile( path, &got, sha512 ) || got != size )
				return false;
			hashed = true;
		}
		if ( !memcmp( v.sha512, sha512, sizeof( sha512 ) ) )
			return true;
	}
	return false;
}

// Of the files the release no longer ships, the ones to remove: those still byte
// for byte a version JoF shipped. A changed or replaced one is the player's and
// stays, as does the running program; *kept names those.
static void Update_PlanRemovals( const std::wstring &dir, const updateRelease_t &r, std::vector<updateRemoval_t> *remove,
		std::vector<std::string> *kept ) {
	const std::wstring program = Update_ProgramPath();
	for ( const updateRemove_t &old : r.remove ) {
		const std::wstring path = Update_Join( dir, old.path );
		updateRemoval_t removal;
		removal.path = old.path;
		if ( !Update_Stamp( path, &removal.size, &removal.written ) )
			continue;	// not there: nothing to remove
		if ( Update_Shipped( path, removal.size, old.versions ) && _wcsicmp( path.c_str(), program.c_str() ) )
			remove->push_back( removal );
		else
			kept->push_back( old.path );
	}
}

// Unpacks the files that differ from the installed ones into pending/files/ and
// lists them in pending/ready.txt, with the files to remove. *staged: how many
// files go in or out; *kept: files to remove that stay as they are not JoF's.
static bool Update_Unpack( const updateSettings_t &s, const updateRelease_t &r, const std::wstring &archive,
		const std::wstring &pending, int *staged, std::vector<std::string> *kept, std::string *error ) {
	HANDLE zip = CreateFileW( archive.c_str(), GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN, NULL );
	std::vector<zipEntry_t> entries;
	if ( zip == INVALID_HANDLE_VALUE || !Update_ZipDirectory( zip, &entries ) ) {
		if ( zip != INVALID_HANDLE_VALUE )
			CloseHandle( zip );
		*error = "the update archive is unreadable";
		return false;
	}

	updatePending_t list;
	list.tag = r.tag;
	bool ok = true;
	for ( const updateFile_t &file : r.files ) {
		if ( s_cancel ) {
			*error = "cancelled";
			ok = false;
			break;
		}
		const zipEntry_t *entry = NULL;
		for ( const zipEntry_t &e : entries ) {
			if ( e.name == file.path )
				entry = &e;
		}
		if ( !entry || entry->size != file.size ) {
			*error = "the update archive does not hold " + file.path;
			ok = false;
			break;
		}

		uint64_t size;
		uint8_t sha512[64];
		if ( Update_HashFile( Update_Join( s.dir, file.path ), &size, sha512 ) && size == file.size && !memcmp( sha512, file.sha512, sizeof( sha512 ) ) )
			continue;	// already this release's

		std::wstring out = Update_Join( pending + L"\\files", file.path );
		Update_MakeParentDirs( out );
		if ( !Update_ZipExtract( zip, *entry, out + L".tmp", sha512 ) || memcmp( sha512, file.sha512, sizeof( sha512 ) )
			|| !MoveFileExW( ( out + L".tmp" ).c_str(), out.c_str(), MOVEFILE_REPLACE_EXISTING ) ) {
			*error = file.path + " does not match the update's manifest; nothing was installed";
			ok = false;
			break;
		}
		list.files.push_back( file.path );
	}
	CloseHandle( zip );

	if ( ok )
		Update_PlanRemovals( s.dir, r, &list.remove, kept );
	*staged = (int)( list.files.size() + list.remove.size() );
	if ( ok && *staged && !Update_WritePending( pending + L"\\ready.txt", list ) ) {
		*error = "cannot write " + Update_Narrow( pending );
		ok = false;
	}
	return ok;
}

// jofupdate/lock, held by the one client that stages, swaps or cleans up files, so a
// second JoF window cannot take the staged files or the .old backups from under it.
// Let go of when it goes out of scope, by release(), or by Windows when the process ends.
struct updateLock_t {
	HANDLE	file;
	explicit updateLock_t( const std::wstring &root )
		: file( CreateFileW( ( root + L"\\lock" ).c_str(), GENERIC_WRITE, 0, NULL, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL ) ) {}
	~updateLock_t() { release(); }
	bool held( void ) const { return file != INVALID_HANDLE_VALUE; }
	void release( void ) {
		if ( file != INVALID_HANDLE_VALUE )
			CloseHandle( file );
		file = INVALID_HANDLE_VALUE;
	}
};

static void Update_Download( const updateSettings_t &s, const updateRelease_t &r ) {
	const std::wstring root = s.dir + L"\\" UPDATE_DIR;
	const std::wstring downloads = root + L"\\download", pending = root + L"\\pending";
	if ( !Update_Writable( s.dir ) || !Update_MakeDirs( root ) ) {
		Update_Set( UPDATE_FAILED, "JoF cannot write to " + Update_Narrow( s.dir ) + ", so it cannot update itself. Download "
			+ Update_Display( r.tag ) + " from " UPDATE_REPOSITORY "/releases/tag/" + r.tag );
		return;
	}
	std::string error;
	int staged = 0;
	std::vector<std::string> kept;
	bool ok;
	{
		updateLock_t lock( root );
		if ( !lock.held() ) {
			Update_Set( UPDATE_FAILED, "another JoF window is installing an update; try again once it is done" );
			return;
		}
		// Whatever an earlier download left, including another release's staged files.
		Update_RemoveTree( pending );
		Update_RemoveTree( downloads );
		Update_MakeDirs( downloads );
		Update_MakeDirs( pending );

		const std::wstring archive = downloads + L"\\" + Update_Widen( r.archive );
		ok = Update_DownloadArchive( s, r, archive, &error ) && Update_Unpack( s, r, archive, pending, &staged, &kept, &error );
		Update_RemoveTree( downloads );
		if ( !ok || !staged )
			Update_RemoveTree( pending );
	}

	std::string note;
	for ( const std::string &file : kept )
		note += ( note.empty() ? "" : ", " ) + file;
	if ( !note.empty() )
		note = "JoF " + Update_Display( r.tag ) + " no longer ships " + note + ", but the copy here was changed or is not JoF's, so it stays.";
	if ( !ok )
		Update_Set( UPDATE_FAILED, error );
	else if ( !staged )
		Update_Set( UPDATE_UP_TO_DATE, "Your files already match JoF " + Update_Display( r.tag ) + "." + ( note.empty() ? "" : " " + note ) );
	else
		Update_Set( UPDATE_STAGED, note );
}

/*
===============================================================================

APPLY (main(), before Com_Init: nothing is open yet, and the engine is not up)

===============================================================================
*/

static std::string	s_applyError;		// why the staged update did not go in, for CL_Update_Init

static bool Update_Relaunch( void ) {
	std::wstring program = Update_ProgramPath();
	std::wstring command = GetCommandLineW();
	std::vector<wchar_t> line( command.begin(), command.end() );
	line.push_back( L'\0' );
	STARTUPINFOW startup;
	memset( &startup, 0, sizeof( startup ) );
	startup.cb = sizeof( startup );
	PROCESS_INFORMATION process;
	if ( program.empty() || !CreateProcessW( program.c_str(), line.data(), NULL, NULL, FALSE, 0, NULL, NULL, &startup, &process ) )
		return false;
	CloseHandle( process.hThread );
	CloseHandle( process.hProcess );
	return true;
}

// DeleteFileW, read-only files included. GetLastError says why it failed.
static bool Update_Delete( const std::wstring &path ) {
	SetFileAttributesW( path.c_str(), FILE_ATTRIBUTE_NORMAL );
	return DeleteFileW( path.c_str() ) != 0;
}

// Deletes the <name>.old files the last update left. The client that ran them may
// still be exiting, or another JoF window may still run them, so they can outlast
// a start or two.
static void Update_CleanUp( const std::wstring &dir, const std::wstring &root ) {
	const std::wstring list = root + L"\\cleanup.txt";
	std::string text;
	if ( !Update_ReadFile( list, &text ) )
		return;
	std::string left;
	for ( const std::string &file : Update_Lines( text ) ) {
		if ( !Update_ValidPath( file ) )
			continue;
		std::wstring old = Update_Join( dir, file ) + L".old";
		if ( Update_Exists( old ) && !Update_Delete( old ) )
			left += file + "\n";
	}
	if ( left.empty() )
		DeleteFileW( list.c_str() );
	else
		Update_WriteFile( list, left );
}

// The swap's journal, in applying.txt: "aside <path>" is written, and on the disk,
// before that file is moved aside as <path>.old, so a swap that was stopped (a crash,
// the power) knows which .old files are its own backups and which are older ones.
static std::vector<std::string> Update_JournalAside( const std::wstring &journal ) {
	std::vector<std::string> aside;
	std::string text;
	if ( Update_ReadFile( journal, &text ) ) {
		for ( const std::string &line : Update_Lines( text ) ) {
			if ( !line.compare( 0, 6, "aside " ) )
				aside.push_back( line.substr( 6 ) );
		}
	}
	return aside;
}

static bool Update_JournalAppend( const std::wstring &journal, const std::string &line ) {
	HANDLE file = CreateFileW( journal.c_str(), FILE_APPEND_DATA, FILE_SHARE_READ, NULL, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;
	const std::string text = line + "\n";
	DWORD wrote = 0;
	const bool ok = WriteFile( file, text.data(), (DWORD)text.size(), &wrote, NULL ) && wrote == text.size() && FlushFileBuffers( file );
	const DWORD code = GetLastError();
	CloseHandle( file );
	SetLastError( code );
	return ok;
}

// Puts each staged file in place, keeping the current one as <name>.old, and moves
// each file to remove aside as <name>.old too, noting each in journal first. Waits a
// while for a client that is exiting to close its files; on failure, puts everything
// back. Files no longer staged went in before a crash; files to remove that are gone,
// or changed since they were found to be JoF's, are left as they are.
static bool Update_Swap( const std::wstring &dir, const std::wstring &staging, const std::vector<std::string> &files,
		const std::vector<updateRemoval_t> &remove, const std::wstring &journal, int *moved, std::string *error ) {
	struct swapped_t { std::wstring target, old, staged; bool keptOld; };	// no staged file: a removal
	std::vector<swapped_t> done;
	const DWORD start = GetTickCount();

	// What a swap that was stopped did already goes back too if this one fails, so a
	// failure leaves the old release whole, not half of each. Only a .old the journal
	// names is that swap's backup; any other is older, and never put back.
	const std::vector<std::string> aside = Update_JournalAside( journal );
	auto movedAside = [&aside]( const std::string &path ) {
		return std::find( aside.begin(), aside.end(), path ) != aside.end();
	};
	for ( const std::string &file : files ) {
		swapped_t s = { Update_Join( dir, file ), Update_Join( dir, file ) + L".old", Update_Join( staging, file ), false };
		if ( !Update_Exists( s.staged ) && Update_Exists( s.target ) ) {
			s.keptOld = movedAside( file ) && Update_Exists( s.old );
			done.push_back( s );
		}
	}
	for ( const updateRemoval_t &removal : remove ) {
		swapped_t s = { Update_Join( dir, removal.path ), Update_Join( dir, removal.path ) + L".old", std::wstring(), true };
		if ( movedAside( removal.path ) && !Update_Exists( s.target ) && Update_Exists( s.old ) )
			done.push_back( s );
	}

	// One file in or out: false once it has waited too long, with *error saying why.
	auto swap = [&]( swapped_t &s, const std::string &file ) {
		// Stopped between its two renames: the file is aside as .old, the new one still staged.
		s.keptOld = !Update_Exists( s.target ) && movedAside( file ) && Update_Exists( s.old );
		bool noted = false;
		for ( ;; ) {
			DWORD code = 0;
			bool oldInUse = false;
			if ( Update_Exists( s.target ) ) {
				s.keptOld = false;
				// A .old an earlier update left, which another JoF window may still run.
				if ( Update_Exists( s.old ) && !Update_Delete( s.old ) ) {
					code = GetLastError();
					oldInUse = true;
				} else if ( !noted && !( noted = Update_JournalAppend( journal, "aside " + file ) ) ) {
					code = GetLastError();
				} else if ( !( s.keptOld = MoveFileExW( s.target.c_str(), s.old.c_str(), MOVEFILE_WRITE_THROUGH ) != 0 ) ) {
					code = GetLastError();
				}
			}
			if ( !code ) {
				if ( s.staged.empty() || MoveFileExW( s.staged.c_str(), s.target.c_str(), MOVEFILE_WRITE_THROUGH ) )
					return true;
				code = GetLastError();
				if ( s.keptOld && MoveFileExW( s.old.c_str(), s.target.c_str(), 0 ) )
					s.keptOld = false;
			}
			if ( GetTickCount() - start > APPLY_WAIT_MS ) {
				*error = oldInUse ? file + ".old is in use by another program (another JoF window?)."
					: code == ERROR_SHARING_VIOLATION || code == ERROR_ACCESS_DENIED
					? file + " is in use by another program (another JoF window?)."
					: Update_Format( "%s could not be replaced (Windows error %lu).", file.c_str(), code );
				return false;
			}
			Sleep( 250 );
		}
	};
	auto putBack = [&]() {
		for ( auto it = done.rbegin(); it != done.rend(); ++it ) {
			if ( !it->staged.empty() )
				MoveFileExW( it->target.c_str(), it->staged.c_str(), MOVEFILE_REPLACE_EXISTING );
			if ( it->keptOld )
				MoveFileExW( it->old.c_str(), it->target.c_str(), 0 );
		}
		*moved = 0;
		return false;
	};

	for ( const std::string &file : files ) {
		swapped_t s = { Update_Join( dir, file ), Update_Join( dir, file ) + L".old", Update_Join( staging, file ), false };
		if ( !Update_Exists( s.staged ) )
			continue;
		Update_MakeParentDirs( s.target );
		if ( !swap( s, file ) )
			return putBack();
		done.push_back( s );
		( *moved )++;
	}
	for ( const updateRemoval_t &removal : remove ) {
		swapped_t s = { Update_Join( dir, removal.path ), Update_Join( dir, removal.path ) + L".old", std::wstring(), false };
		uint64_t size, written;
		if ( !Update_Stamp( s.target, &size, &written ) || size != removal.size || written != removal.written )
			continue;
		if ( !swap( s, removal.path ) )
			return putBack();
		done.push_back( s );
		( *moved )++;
	}
	return true;
}

// Whether an update staged as stagedTag still goes in under the build running as
// runningTag. Never back to an older release: one installed by hand meanwhile drops
// it. The same release only to finish a swap that was stopped, by which time the new
// program may be the one running, with the rest of its files still staged.
static bool Update_StillApplies( const char *runningTag, const std::string &stagedTag, bool resuming ) {
	releaseVersion_t running, staged;
	if ( !runningTag[0] || !Update_ParseTag( runningTag, &running ) || !Update_ParseTag( stagedTag.c_str(), &staged ) )
		return true;	// a development build
	if ( Update_IsNewer( staged, running ) )
		return true;
	return resuming && stagedTag == runningTag;
}

void CL_Update_ApplyPending( void ) {
	const std::wstring dir = Update_InstallDir();
	if ( dir.empty() )
		return;
	const std::wstring root = dir + L"\\" UPDATE_DIR, pending = root + L"\\pending";
	const std::wstring ready = pending + L"\\ready.txt", applying = pending + L"\\applying.txt";
	if ( !Update_Exists( root ) )
		return;

	// Cleaning up too: another JoF window may be swapping right now, with fresh .old
	// backups of the very files cleanup.txt names. What this start skips, the next does.
	updateLock_t lock( root );
	if ( !lock.held() )
		return;

	// applying.txt left behind means the last start was stopped mid-swap: finish it.
	const bool resuming = Update_Exists( applying );
	if ( !resuming && !Update_Exists( ready ) ) {
		Update_CleanUp( dir, root );
		return;
	}
	// While an update is pending the .old files stay: its swap deletes stale ones
	// itself, and a swap that was stopped needs the backups it already made.
	if ( !resuming && !MoveFileExW( ready.c_str(), applying.c_str(), 0 ) )
		return;
	updatePending_t p;
	if ( !Update_ReadPending( applying, &p ) ) {
		Update_RemoveTree( pending );
		s_applyError = "the downloaded update was unreadable and has been discarded.";
		return;
	}
	if ( !Update_StillApplies( JOF_RELEASE_TAG, p.tag, resuming ) ) {
		Update_RemoveTree( pending );
		return;
	}

	// Whether this program is already the new one, swapped in before the last start was
	// stopped: then, if the rest cannot go in, the old one goes back in its place.
	const std::wstring program = Update_ProgramPath();
	bool programSwapped = false;
	for ( const std::string &file : p.files ) {
		if ( !_wcsicmp( Update_Join( dir, file ).c_str(), program.c_str() ) && !Update_Exists( Update_Join( pending + L"\\files", file ) ) )
			programSwapped = true;
	}

	int moved = 0;
	std::string error;
	if ( Update_Swap( dir, pending + L"\\files", p.files, p.remove, applying, &moved, &error ) ) {
		std::string cleanup, previous;
		if ( Update_ReadFile( root + L"\\cleanup.txt", &previous ) )
			cleanup = previous;
		for ( const std::string &file : p.files )
			cleanup += file + "\n";
		for ( const updateRemoval_t &removal : p.remove )
			cleanup += removal.path + "\n";
		Update_WriteFile( root + L"\\cleanup.txt", cleanup );
		Update_WriteFile( root + L"\\installed.txt", p.tag + "\n" );
		Update_RemoveTree( pending );
		if ( moved ) {
			// This process started as the old program (or as the new one, finishing a
			// stopped swap): start the new one fresh, which takes the lock to clean up.
			lock.release();
			if ( !Update_Relaunch() )
				MessageBoxW( NULL, L"JoF has been updated. Please start it again.", L"JoF", MB_OK | MB_ICONINFORMATION );
			ExitProcess( 0 );
		}
		return;
	}

	if ( ++p.attempts >= MAX_APPLY_ATTEMPTS ) {
		Update_RemoveTree( pending );
		s_applyError = error + " The update was discarded; type /update to download it again.";
	} else {
		// The journal goes with it: should a file not have gone back, the next try still
		// knows whose backup its .old is.
		p.aside = Update_JournalAside( applying );
		Update_WritePending( ready, p );
		DeleteFileW( applying.c_str() );
		s_applyError = error + " Close every other JoF window, then restart JoF to try again.";
	}
	if ( programSwapped ) {
		// The old release went back under this program, the new one: running on
		// would mix the two, and the console is not up yet to say why.
		const std::wstring text = L"JoF could not finish installing the update: " + Update_Widen( s_applyError );
		MessageBoxW( NULL, text.c_str(), L"JoF", MB_OK | MB_ICONWARNING );
		ExitProcess( 0 );
	}
}

/*
===============================================================================

CLIENT (main thread)

===============================================================================
*/

static cvar_t	*cl_betaUpdates;
static cvar_t	*cl_updateCheck;
static cvar_t	*cl_updateAs;
static cvar_t	*cl_updateURL;

static int			s_seenGeneration;
static bool			s_userAsked;		// /update started this, so even "up to date" is worth saying
static bool			s_installWhenFound;	// /update ran during the check: download what it finds
static bool			s_restart;			// quit for the staged update: start the client again on the way out
static bool			s_promptedInGame;
static int			s_progressQuarter;
static std::string	s_availableTag;
static std::string	s_installedTag;		// what this build counts as installed

// What the worker needs, read from the cvars here; false (and why) when this
// build cannot update itself.
static bool Update_Settings( updateSettings_t *s, std::string *why ) {
	releaseVersion_t v;
	s->installed = cl_updateAs->string[0] ? cl_updateAs->string : JOF_RELEASE_TAG;
	if ( s->installed.empty() || !Update_ParseTag( s->installed.c_str(), &v ) ) {
		*why = "This is a development build without a release version, so it cannot update itself (cl_updateAs <tag> pretends it is one).";
		return false;
	}
	if ( Update_BuildKeys().empty() ) {
		*why = "This build has no update signing key, so it cannot update itself.";
		return false;
	}
	s->beta = cl_betaUpdates->integer != 0;
	s->repository = UPDATE_REPOSITORY;
	s->api = UPDATE_API;
	if ( cl_updateURL->string[0] ) {
		// Testing: a server laid out like GitHub's (docs/updater.md). It still needs
		// the signing key, so it can only serve real releases.
		std::string url = cl_updateURL->string;
		while ( !url.empty() && url.back() == '/' )
			url.pop_back();
		if ( url.compare( 0, 8, "https://" ) && url.compare( 0, 17, "http://127.0.0.1:" ) && url.compare( 0, 17, "http://localhost:" ) ) {
			*why = "cl_updateURL must be an https:// address (or http://127.0.0.1:<port> for testing).";
			return false;
		}
		s->repository = url;
		s->api = url + "/api";
	}
	s->dir = Update_InstallDir();
	if ( s->dir.empty() ) {
		*why = "JoF cannot find its own folder, so it cannot update itself.";
		return false;
	}
	s_installedTag = s->installed;
	return true;
}

static void Update_StartCheck( bool userAsked ) {
	updateSettings_t s;
	std::string why;
	s_userAsked = userAsked;
	{
		// What an earlier check found may not be what this one would (betaupdates changed).
		std::lock_guard<std::mutex> hold( s_shared->lock );
		s_shared->haveRelease = false;
	}
	if ( !Update_Settings( &s, &why ) ) {
		Update_Set( UPDATE_DISABLED, why );
		return;
	}
	Update_Set( UPDATE_CHECKING );
	Update_Start( [s]() { Update_Check( s ); } );
}

static void Update_StartDownload( void ) {
	updateSettings_t s;
	std::string why;
	updateRelease_t release;
	if ( !Update_Settings( &s, &why ) ) {
		Update_Set( UPDATE_DISABLED, why );
		return;
	}
	{
		std::lock_guard<std::mutex> hold( s_shared->lock );
		if ( !s_shared->haveRelease )
			return;
		release = s_shared->release;
		s_shared->done = 0;
		s_shared->total = release.archiveSize;
	}
	s_userAsked = true;
	s_progressQuarter = 0;
	Update_Set( UPDATE_DOWNLOADING );
	Update_Start( [s, release]() { Update_Download( s, release ); } );
}

static void Update_Prompt( void ) {
	Com_Printf( S_COLOR_YELLOW "An update is available (JoF %s). Type " S_COLOR_WHITE "/update" S_COLOR_YELLOW " to update.\n",
		Update_Display( s_availableTag ).c_str() );
}

static void Update_Report( updateState_t state, const std::string &message, const updateRelease_t &release ) {
	switch ( state ) {
	case UPDATE_IDLE:
		break;
	case UPDATE_CHECKING:
		if ( s_userAsked )
			Com_Printf( "Checking for a newer JoF release...\n" );
		break;
	case UPDATE_UP_TO_DATE:
		s_installWhenFound = false;
		if ( !message.empty() )
			Com_Printf( "%s\n", message.c_str() );
		else if ( s_userAsked )
			Com_Printf( "JoF %s is up to date.\n", Update_Display( s_installedTag ).c_str() );
		break;
	case UPDATE_AVAILABLE:
		s_availableTag = release.tag;
		if ( s_installWhenFound ) {
			s_installWhenFound = false;
			Update_StartDownload();
		} else {
			Update_Prompt();
		}
		break;
	case UPDATE_DOWNLOADING:
		Com_Printf( "Downloading JoF %s (%.1f MB)...\n", Update_Display( release.tag ).c_str(), release.archiveSize / ( 1024.0 * 1024.0 ) );
		break;
	case UPDATE_STAGED:
		Com_Printf( S_COLOR_GREEN "JoF %s is downloaded. Type " S_COLOR_WHITE "/update" S_COLOR_GREEN
			" to restart and install it now, or it installs the next time you start JoF.\n", Update_Display( release.tag ).c_str() );
		if ( !message.empty() )
			Com_Printf( S_COLOR_YELLOW "%s\n", message.c_str() );
		break;
	case UPDATE_FAILED:
		s_installWhenFound = false;
		if ( s_userAsked )
			Com_Printf( S_COLOR_RED "Update failed: %s\n", message.c_str() );
		else
			Com_DPrintf( "Update check failed: %s\n", message.c_str() );
		break;
	case UPDATE_DISABLED:
		s_installWhenFound = false;
		if ( s_userAsked )
			Com_Printf( "%s Releases: " UPDATE_REPOSITORY "/releases\n", message.c_str() );
		else
			Com_DPrintf( "Updates: %s\n", message.c_str() );
		break;
	}
}

static void CL_Update_f( void ) {
	updateState_t state;
	bool haveRelease;
	uint64_t done, total;
	{
		std::lock_guard<std::mutex> hold( s_shared->lock );
		state = s_shared->state;
		haveRelease = s_shared->haveRelease;
		done = s_shared->done;
		total = s_shared->total;
	}

	switch ( state ) {
	case UPDATE_CHECKING:
		s_userAsked = true;
		s_installWhenFound = true;
		Com_Printf( "Checking for a newer JoF release...\n" );
		break;
	case UPDATE_AVAILABLE:
		Update_StartDownload();
		break;
	case UPDATE_DOWNLOADING:
		Com_Printf( "Downloading JoF %s: %d%%\n", Update_Display( s_availableTag ).c_str(), total ? (int)( done * 100 / total ) : 0 );
		break;
	case UPDATE_STAGED:
		Com_Printf( "Restarting to install JoF %s...\n", Update_Display( s_availableTag ).c_str() );
		s_restart = true;
		Cbuf_ExecuteText( EXEC_INSERT, "quit\n" );	// next, ahead of whatever a script queued
		break;
	case UPDATE_FAILED:
		if ( haveRelease ) {	// the download failed: try it again
			Update_StartDownload();
			break;
		}
		// fall through: the check failed, check again
	default:
		s_installWhenFound = true;
		Update_StartCheck( true );
		break;
	}
}

void CL_Update_Init( void ) {
	cl_betaUpdates = Cvar_Get( "betaupdates", "0", CVAR_ARCHIVE_ND | CVAR_PROTECTED, "1: /update also offers beta releases, 0: stable releases only" );
	cl_updateCheck = Cvar_Get( "cl_updateCheck", "1", CVAR_ARCHIVE_ND | CVAR_PROTECTED, "Look for a newer JoF release at start-up" );
	cl_updateAs = Cvar_Get( "cl_updateAs", "", CVAR_TEMP | CVAR_PROTECTED, "Testing: look for updates as if this build were that release tag" );
	cl_updateURL = Cvar_Get( "cl_updateURL", "", CVAR_INIT | CVAR_PROTECTED, "Testing: fetch releases from this server instead of GitHub" );
	cl_betaUpdates->modified = qfalse;
	Cmd_AddCommand( "update", CL_Update_f, "Download and install the newest JoF release" );

	if ( !s_shared )
		s_shared = new updateShared_t;

	const std::wstring installed = Update_InstallDir() + L"\\" UPDATE_DIR L"\\installed.txt";
	std::string tag;
	if ( Update_ReadFile( installed, &tag ) ) {
		DeleteFileW( installed.c_str() );
		std::vector<std::string> lines = Update_Lines( tag );
		if ( !lines.empty() )
			Com_Printf( S_COLOR_GREEN "JoF has been updated to %s.\n", Update_Display( lines[0] ).c_str() );
	}
	if ( !s_applyError.empty() )
		Com_Printf( S_COLOR_RED "The downloaded update could not be installed: %s\n", s_applyError.c_str() );

	if ( cl_updateCheck->integer )
		Update_StartCheck( false );
}

void CL_Update_Frame( void ) {
	if ( !s_shared )
		return;

	updateState_t state;
	int generation;
	uint64_t done, total;
	std::string message;
	updateRelease_t release;
	{
		std::lock_guard<std::mutex> hold( s_shared->lock );
		state = s_shared->state;
		generation = s_shared->generation;
		done = s_shared->done;
		total = s_shared->total;
		if ( generation != s_seenGeneration ) {
			message = s_shared->message;
			release = s_shared->release;
		}
	}

	if ( generation != s_seenGeneration ) {
		s_seenGeneration = generation;
		Update_Report( state, message, release );
	}

	if ( state == UPDATE_DOWNLOADING && total ) {
		int quarter = (int)( done * 4 / total );
		if ( quarter > s_progressQuarter && quarter < 4 ) {
			s_progressQuarter = quarter;
			Com_Printf( "Downloading JoF %s: %d%%\n", Update_Display( s_availableTag ).c_str(), quarter * 25 );
		}
	}

	// The start-up prompt went to the console; say it again where a player in a
	// match sees it.
	if ( state == UPDATE_AVAILABLE && !s_promptedInGame && cls.state == CA_ACTIVE ) {
		s_promptedInGame = true;
		Update_Prompt();
	}

	// /betaupdates 1 looks for the newest beta at once (and 0 for the newest stable).
	if ( cl_betaUpdates->modified && !s_busy && state != UPDATE_DOWNLOADING && state != UPDATE_STAGED ) {
		cl_betaUpdates->modified = qfalse;
		Update_StartCheck( true );
	}
}

void CL_Update_Shutdown( void ) {
	if ( !s_shared )
		return;
	Cmd_RemoveCommand( "update" );

	s_cancel = true;
	if ( s_worker ) {
		for ( int i = 0; i < 300 && s_busy; i++ )
			Sleep( 10 );
		if ( s_busy )
			s_worker->detach();		// left to die with the process
		else
			s_worker->join();
		delete s_worker;
		s_worker = NULL;
	}

	// The new client waits in CL_Update_ApplyPending until this one has closed its files.
	if ( s_restart ) {
		s_restart = false;
		Update_Relaunch();
	}
}

#else // !_WIN32

// Only Windows updates itself for now; elsewhere /update points to the releases.
static void CL_Update_f( void ) {
	Com_Printf( "JoF can only update itself on Windows for now. Download the newest release from "
		"https://github.com/JediofFreedom/JoF_EJK/releases\n" );
}

void CL_Update_ApplyPending( void ) {
}

void CL_Update_Init( void ) {
	Cmd_AddCommand( "update", CL_Update_f, "Download and install the newest JoF release" );
}

void CL_Update_Frame( void ) {
}

void CL_Update_Shutdown( void ) {
	Cmd_RemoveCommand( "update" );
}

#endif // _WIN32
