// Checks the in-client updater (codemp/client/cl_update.cpp) without the engine:
// release ordering, the paths it may write, manifests and their signatures, its
// zip reader against what make_update_package.py writes, and the start-up swap.
//
//   updater_check <repo> <python> <work dir>

#include "cl_update.cpp"	// the copy beside this test's client.h

#include <map>

char			g_updateTestKeys[64 * 4 + 1];
clientStatic_t	cls;

static int		checks;
static std::wstring	work;

static void require( bool condition, const std::string &what ) {
	if ( !condition ) {
		fprintf( stderr, "FAIL: %s\n", what.c_str() );
		exit( 1 );
	}
	checks++;
}

cvar_t *Cvar_Get( const char *, const char *value, uint32_t, const char * ) {
	cvar_t *cvar = new cvar_t();
	cvar->string = _strdup( value );
	cvar->integer = atoi( value );
	return cvar;
}
void Cmd_AddCommand( const char *, xcommand_t, const char * ) {}
void Cmd_RemoveCommand( const char * ) {}
void Cbuf_ExecuteText( int, const char * ) {}
void Com_Printf( const char *fmt, ... ) {
	va_list args;
	va_start( args, fmt );
	vprintf( fmt, args );
	va_end( args );
}
void Com_DPrintf( const char *fmt, ... ) {}

static std::string Hex( const uint8_t *bytes, size_t size ) {
	std::string hex;
	for ( size_t i = 0; i < size; i++ )
		hex += Update_Format( "%02x", bytes[i] );
	return hex;
}

static std::string ReadAll( const std::wstring &path ) {
	std::string text;
	return Update_ReadFile( path, &text ) ? text : "<missing>";
}

static void WriteAll( const std::wstring &path, const std::string &data ) {
	Update_MakeParentDirs( path );
	require( Update_WriteFile( path, data ), "write " + Update_Narrow( path ) );
}

static void CheckVersions( void ) {
	releaseVersion_t v;
	require( Update_ParseTag( "1.6.2", &v ) && v.kind == RELEASE_STABLE && v.major == 1 && v.minor == 6 && v.patch == 2, "1.6.2 is stable" );
	require( Update_ParseTag( "1.6.2-beta.4", &v ) && v.kind == RELEASE_BETA && v.number == 4, "1.6.2-beta.4 is beta 4" );
	require( Update_ParseTag( "1.6.2-alpha.9", &v ) && v.kind == RELEASE_ALPHA, "1.6.2-alpha.9 is an alpha" );
	for ( const char *bad : { "", "1.6", "1.6.2.1", "v1.6.2", "1.6.2-beta", "1.6.2-beta.", "1.6.2-rc.1", "1.6.2 ", "latest-beta", "reserve/1.6.2-beta.1" } )
		require( !Update_ParseTag( bad, &v ), std::string( "not a release tag: '" ) + bad + "'" );
	require( Update_ParseTag( "1.6.2-beta.1000000000", &v ) && v.number == 1000000000, "beta numbers go up to a billion" );
	require( !Update_ParseTag( "1.6.2-beta.1000000001", &v ) && !Update_ParseTag( "1.6.2-beta.99999999999999999999", &v ), "and no further, without overflowing" );

	auto newer = []( const char *candidate, const char *installed ) {
		releaseVersion_t c, i;
		require( Update_ParseTag( candidate, &c ) && Update_ParseTag( installed, &i ), "tags parse" );
		return Update_IsNewer( c, i );
	};
	require( newer( "1.6.2-beta.4", "1.6.2" ), "a beta is newer than the stable release it is built on" );
	require( !newer( "1.6.2", "1.6.2-beta.4" ), "that stable release is not newer than its beta" );
	require( newer( "1.6.3", "1.6.2-beta.400" ), "the next stable release is newer than any beta before it" );
	require( newer( "1.6.2-beta.10", "1.6.2-beta.9" ), "betas count up as numbers, not text" );
	require( !newer( "1.6.2-beta.4", "1.6.2-beta.4" ), "the same release is not newer" );
	require( newer( "1.7.0", "1.6.99" ) && newer( "2.0.0", "1.99.99" ), "minor and major releases" );
	require( !newer( "1.6.2-alpha.5", "1.6.2" ) && !newer( "1.6.2-beta.1", "1.6.2-alpha.5" ), "alpha numbers are not comparable" );
	require( Update_Display( "1.6.2-beta.4" ) == "1.6.2 beta 4" && Update_Display( "1.6.2" ) == "1.6.2", "tags as players see them" );
}

static void CheckPaths( void ) {
	for ( const char *good : { "eternaljk.x86_64.exe", "SDL2.x86_64.dll", "rd-vulkan_x86.dll", "base/JoF_Animations.pk3", "EternalJK/jofclient-win-x86_64.pk3" } )
		require( Update_ValidPath( good ), std::string( "may install " ) + good );
	for ( const char *bad : { "", "../eternaljk.x86_64.exe", "base/../x.dll", "base/autoexec.cfg", "base/x.dll", "EternalJK/eternaljk.cfg",
			"japlus/x.pk3", "base/sub/x.pk3", "x.pk3", ".x.dll", "readme.txt", "C:x.dll", "a\\b.dll", "/x.dll", "base/", "x.exe:stream" } )
		require( !Update_ValidPath( bad ), std::string( "may not install '" ) + bad + "'" );
}

static void CheckManifests( const std::string &repo, const std::string &python ) {
	// A GameData folder, packaged by the release script.
	const std::wstring gamedata = work + L"\\GameData", out = work + L"\\out";
	std::string noise;	// does not compress: several reads of input
	uint32_t seedValue = 1;
	for ( int i = 0; i < 300000; i++ ) {
		seedValue = seedValue * 1103515245u + 12345u;
		noise += (char)( seedValue >> 16 );
	}
	const std::string emptyZip = std::string( "PK\x05\x06", 4 ) + std::string( 18, '\0' );	// a valid pk3 with nothing in it
	std::map<std::string, std::string> files = {
		{ "eternaljk.x86_64.exe", noise },
		{ "empty.dll", "" },
		{ "rd-test_x86_64.dll", std::string( 900000, 'j' ) },	// several output buffers (ReadAll stops at 1 MB)
		{ "base/JoF_Test.pk3", emptyZip },
		{ "EternalJK/jofclient-win-x86_64.pk3", emptyZip },
	};
	for ( const auto &file : files )
		WriteAll( Update_Join( gamedata, file.first ), file.second );
	auto package = [&]( const std::wstring &folder ) {
		std::string command = "\"\"" + python + "\" -I \"" + repo + "/scripts/update/make_update_package.py\" --gamedata \"" + Update_Narrow( folder )
			+ "\" --platform " UPDATE_PLATFORM " --tag 1.6.2-beta.4 --channel beta --output \"" + Update_Narrow( out ) + "\"\"";
		return system( command.c_str() );
	};
	WriteAll( work + L"\\Broken\\base\\JoF_Test.pk3", "<html>502 Bad Gateway</html>" );
	require( package( work + L"\\Broken" ) != 0, "make_update_package.py refuses a pk3 that is not a zip (a failed download)" );
	require( package( gamedata ) == 0, "make_update_package.py packages the folder" );
	const std::string manifest = ReadAll( out + L"\\" + Update_Widen( UPDATE_MANIFEST ) );

	// Signed with a key the build lists (after a no-key placeholder), not with another.
	uint8_t seed[32], secret[64], key[32], signature[64];
	memset( seed, 7, sizeof( seed ) );
	crypto_ed25519_key_pair( secret, key, seed );
	strcpy( g_updateTestKeys, ( std::string( 64, '0' ) + Hex( key, sizeof( key ) ) ).c_str() );
	crypto_ed25519_sign( signature, secret, (const uint8_t *)manifest.data(), manifest.size() );
	const std::string signed_( (const char *)signature, sizeof( signature ) );
	require( Update_Signed( signed_, manifest ), "a manifest signed with a listed key passes" );
	std::string tampered = manifest;
	tampered[tampered.find( "1.6.2-beta.4" ) + 4] = '3';
	require( !Update_Signed( signed_, tampered ), "a manifest changed after signing fails" );
	memset( seed, 9, sizeof( seed ) );
	crypto_ed25519_key_pair( secret, key, seed );
	crypto_ed25519_sign( signature, secret, (const uint8_t *)manifest.data(), manifest.size() );
	require( !Update_Signed( std::string( (const char *)signature, sizeof( signature ) ), manifest ), "a key the build does not list fails" );
	strcpy( g_updateTestKeys, std::string( 64, '0' ).c_str() );
	require( Update_BuildKeys().empty(), "the all-zero placeholder is no key" );

	updateRelease_t release;
	std::string error;
	require( Update_ParseManifest( manifest, &release, &error ), "the release script's manifest parses: " + error );
	require( release.tag == "1.6.2-beta.4" && release.version.kind == RELEASE_BETA && release.files.size() == files.size(), "it lists every file" );

	auto refuses = [&manifest]( const std::string &from, const std::string &to, const std::string &what ) {
		std::string changed = manifest;
		size_t at = changed.find( from );
		require( at != std::string::npos, "the manifest holds " + from );
		changed.replace( at, from.size(), to );
		updateRelease_t r;
		std::string e;
		require( !Update_ParseManifest( changed, &r, &e ), what );
	};
	refuses( "\"tag\": \"1.6.2-beta.4\"", "\"tag\": \"1.6.2-alpha.4\"", "an alpha release is refused" );
	refuses( "\"channel\": \"beta\"", "\"channel\": \"stable\"", "a channel that does not match the tag is refused" );
	refuses( "\"platform\": \"" UPDATE_PLATFORM "\"", "\"platform\": \"linux-x86_64\"", "another platform's update is refused" );
	refuses( "\"path\": \"base/JoF_Test.pk3\"", "\"path\": \"base/autoexec.cfg\"", "a config file is refused" );
	refuses( "\"path\": \"base/JoF_Test.pk3\"", "\"path\": \"../JoF_Test.pk3\"", "a path out of GameData is refused" );
	refuses( "\"path\": \"base/JoF_Test.pk3\"", "\"path\": \"empty.dll\"", "a file listed twice is refused" );
	refuses( "\"format\": 1", "\"format\": 2", "a newer manifest format is refused" );
	refuses( "\"name\": \"JoF-update-", "\"name\": \"../JoF-update-", "an archive name with a path is refused" );

	// The release script's zip, through the client's reader.
	const std::wstring archive = out + L"\\" + Update_Widen( release.archive );
	uint64_t size;
	uint8_t hash[64];
	require( Update_HashFile( archive, &size, hash ) && size == release.archiveSize && !memcmp( hash, release.archiveSha512, 64 ), "the archive matches its manifest" );
	HANDLE zip = CreateFileW( archive.c_str(), GENERIC_READ, FILE_SHARE_READ, NULL, OPEN_EXISTING, 0, NULL );
	std::vector<zipEntry_t> entries;
	require( zip != INVALID_HANDLE_VALUE && Update_ZipDirectory( zip, &entries ) && entries.size() == files.size(), "the zip's directory reads" );
	for ( const updateFile_t &file : release.files ) {
		const zipEntry_t *entry = NULL;
		for ( const zipEntry_t &e : entries ) {
			if ( e.name == file.path )
				entry = &e;
		}
		std::wstring unpacked = work + L"\\unpacked-" + std::to_wstring( checks );
		require( entry && Update_ZipExtract( zip, *entry, unpacked, hash ) && !memcmp( hash, file.sha512, 64 ), file.path + " unpacks to its manifest hash" );
		require( ReadAll( unpacked ) == files[file.path], file.path + " unpacks to its own bytes" );
	}
	CloseHandle( zip );
}

static void CheckSwap( void ) {
	const std::wstring dir = work + L"\\install", staging = work + L"\\staging";
	const std::vector<std::string> list = { "a.dll", "base/b.pk3", "EternalJK/c.pk3" };
	WriteAll( dir + L"\\a.dll", "old a" );
	WriteAll( dir + L"\\base\\b.pk3", "old b" );
	WriteAll( staging + L"\\a.dll", "new a" );
	WriteAll( staging + L"\\base\\b.pk3", "new b" );
	WriteAll( staging + L"\\EternalJK\\c.pk3", "new c" );

	int moved = 0;
	std::string error;
	require( Update_Swap( dir, staging, list, &moved, &error ) && moved == 3, "the staged files go in: " + error );
	require( ReadAll( dir + L"\\a.dll" ) == "new a" && ReadAll( dir + L"\\base\\b.pk3" ) == "new b" && ReadAll( dir + L"\\EternalJK\\c.pk3" ) == "new c",
		"each file holds its new bytes" );
	require( ReadAll( dir + L"\\a.dll.old" ) == "old a" && !Update_Exists( dir + L"\\EternalJK\\c.pk3.old" ) && !Update_Exists( staging + L"\\a.dll" ),
		"the old ones are kept as .old, and staging is empty" );
	moved = 0;
	require( Update_Swap( dir, staging, list, &moved, &error ) && moved == 0, "files no longer staged went in already (a crash mid-swap resumes)" );

	// A read-only file, and the read-only .old it leaves, do not stop the next update.
	SetFileAttributesW( ( dir + L"\\a.dll" ).c_str(), FILE_ATTRIBUTE_READONLY );
	WriteAll( staging + L"\\a.dll", "next a" );
	require( Update_Swap( dir, staging, { "a.dll" }, &moved, &error ) && moved == 1, "a read-only file is replaced: " + error );
	require( SetFileAttributesW( ( dir + L"\\a.dll.old" ).c_str(), FILE_ATTRIBUTE_READONLY ) != 0, "make the .old read-only" );
	WriteAll( staging + L"\\a.dll", "new a" );
	require( Update_Swap( dir, staging, { "a.dll" }, &moved, &error ) && ReadAll( dir + L"\\a.dll" ) == "new a", "a read-only .old is replaced: " + error );
	WriteAll( work + L"\\jofupdate\\cleanup.txt", "a.dll\n" );
	SetFileAttributesW( ( dir + L"\\a.dll.old" ).c_str(), FILE_ATTRIBUTE_READONLY );
	Update_CleanUp( dir, work + L"\\jofupdate" );
	require( !Update_Exists( dir + L"\\a.dll.old" ) && !Update_Exists( work + L"\\jofupdate\\cleanup.txt" ), "clean-up deletes a read-only .old" );

	// One JoF window at a time stages or swaps.
	{
		updateLock_t first( work );
		updateLock_t second( work );
		require( first.held() && !second.held(), "a second window cannot take the update lock" );
	}
	{
		updateLock_t again( work );
		require( again.held(), "the lock is free again once let go of" );
	}

	// A pk3 another client has open (fopen shares neither delete nor rename):
	// nothing goes in, and everything is put back.
	WriteAll( staging + L"\\a.dll", "newer a" );
	WriteAll( staging + L"\\base\\b.pk3", "newer b" );
	HANDLE held = CreateFileW( ( dir + L"\\base\\b.pk3" ).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL );
	require( held != INVALID_HANDLE_VALUE, "hold b.pk3 open" );
	printf( "(waiting %lu s on a file in use)\n", APPLY_WAIT_MS / 1000 );
	require( !Update_Swap( dir, staging, list, &moved, &error ) && moved == 0, "a file in use stops the swap" );
	require( error.find( "base/b.pk3 is in use" ) == 0, "and says which file: " + error );
	CloseHandle( held );
	require( ReadAll( dir + L"\\a.dll" ) == "new a" && ReadAll( dir + L"\\base\\b.pk3" ) == "new b", "the installed files are back as they were" );
	require( ReadAll( staging + L"\\a.dll" ) == "newer a" && ReadAll( staging + L"\\base\\b.pk3" ) == "newer b", "and the update is staged again for the next try" );
}

static void CheckPending( void ) {
	const std::wstring path = work + L"\\ready.txt";
	updatePending_t p;
	p.tag = "1.6.2";
	p.attempts = 2;
	p.files = { "eternaljk.x86_64.exe", "base/JoF_Test.pk3" };
	require( Update_WritePending( path, p ), "ready.txt writes" );
	updatePending_t read;
	require( Update_ReadPending( path, &read ) && read.tag == "1.6.2" && read.attempts == 2 && read.files == p.files, "ready.txt reads back" );
	WriteAll( path, "jofupdate 1\ntag 1.6.2\nfile base/autoexec.cfg\n" );
	updatePending_t refused;
	require( !Update_ReadPending( path, &refused ), "ready.txt cannot name a file the updater may not write" );
}

int main( int argc, char **argv ) {
	if ( argc != 4 ) {
		fprintf( stderr, "usage: updater_check <repo> <python> <work dir>\n" );
		return 2;
	}
	work = Update_Widen( argv[3] );
	std::replace( work.begin(), work.end(), L'/', L'\\' );
	Update_RemoveTree( work );
	require( Update_MakeDirs( work ), "make the work folder" );

	CheckVersions();
	CheckPaths();
	CheckManifests( argv[1], argv[2] );
	CheckPending();
	CheckSwap();

	Update_RemoveTree( work );
	printf( "updater: %d checks passed\n", checks );
	return 0;
}
