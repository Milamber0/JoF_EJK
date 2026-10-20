// Checks the in-client updater (codemp/client/cl_update.cpp) without the engine:
// release ordering, the paths it may write, manifests and their signatures, its
// zip reader against what make_update_package.py writes, the remove lists the
// script builds from earlier manifests, and the start-up swap.
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

// A swap's journal (the client's applying.txt), new for each update a check makes,
// holding what a swap that was stopped had noted.
static std::wstring Journal( const wchar_t *name, const std::string &noted = std::string() ) {
	const std::wstring path = work + L"\\journal-" + name + L".txt";
	DeleteFileW( path.c_str() );
	if ( !noted.empty() )
		WriteAll( path, noted );
	return path;
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

	// Whether a staged update still goes in under the running build.
	require( Update_StillApplies( "1.6.1-beta.119", "1.6.1-beta.122", false ), "a newer staged release goes in" );
	require( !Update_StillApplies( "1.6.1-beta.122", "1.6.1-beta.122", false ), "the running release is not installed again" );
	require( Update_StillApplies( "1.6.1-beta.122", "1.6.1-beta.122", true ),
		"but a stopped swap of it resumes, once its new program is the one running" );
	require( !Update_StillApplies( "1.6.1-beta.123", "1.6.1-beta.122", true ) && !Update_StillApplies( "1.6.2", "1.6.1-beta.122", false ),
		"an older staged release never goes in, resumed or not" );
	require( Update_StillApplies( "", "1.6.1-beta.122", false ), "a development build takes what it staged" );
}

static void CheckPaths( void ) {
	for ( const char *good : { "eternaljk.x86_64.exe", "SDL2.x86_64.dll", "rd-vulkan_x86.dll", "base/JoF_Animations.pk3", "EternalJK/jofclient-win-x86_64.pk3" } )
		require( Update_ValidPath( good ), std::string( "may install " ) + good );
	for ( const char *bad : { "", "../eternaljk.x86_64.exe", "base/../x.dll", "base/autoexec.cfg", "base/x.dll", "EternalJK/eternaljk.cfg",
			"japlus/x.pk3", "base/sub/x.pk3", "x.pk3", ".x.dll", "readme.txt", "C:x.dll", "a\\b.dll", "/x.dll", "base/", "x.exe:stream" } )
		require( !Update_ValidPath( bad ), std::string( "may not install '" ) + bad + "'" );
	for ( const char *retail : { "base/assets0.pk3", "BASE/Assets3.PK3", "jamp.exe", "JaSP.exe" } )
		require( Update_RetailFile( retail ), std::string( "never removes the retail " ) + retail );
	require( !Update_RetailFile( "base/JoF_Animations.pk3" ) && !Update_RetailFile( "base/assets4.pk3" ), "JoF's own pk3s are not retail" );
}

static updateVersion_t Version( const std::string &bytes ) {
	updateVersion_t v;
	v.size = bytes.size();
	crypto_sha512( v.sha512, (const uint8_t *)bytes.data(), bytes.size() );
	return v;
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
	auto packageAs = [&]( const std::wstring &folder, const std::string &tag, const std::wstring &output, const std::string &extra ) {
		std::string command = "\"\"" + python + "\" -I \"" + repo + "/scripts/update/make_update_package.py\" --gamedata \"" + Update_Narrow( folder )
			+ "\" --platform " UPDATE_PLATFORM " --tag " + tag + " --channel beta --output \"" + Update_Narrow( output ) + "\" " + extra + "\"";
		return system( command.c_str() );
	};
	auto package = [&]( const std::wstring &folder ) {
		return packageAs( folder, "1.6.2-beta.4", out, "" );
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

	auto refusesIn = []( const std::string &text, const std::string &from, const std::string &to, const std::string &what ) {
		std::string changed = text;
		size_t at = changed.find( from );
		require( at != std::string::npos, "the manifest holds " + from );
		changed.replace( at, from.size(), to );
		updateRelease_t r;
		std::string e;
		require( !Update_ParseManifest( changed, &r, &e ), what );
	};
	auto refuses = [&]( const std::string &from, const std::string &to, const std::string &what ) {
		refusesIn( manifest, from, to, what );
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

	// A release that drops nothing has an empty remove list, and one without any parses.
	std::string bare = manifest;
	const std::string emptyRemove = ",\n  \"remove\": []";
	require( bare.find( emptyRemove ) != std::string::npos, "a release that drops nothing has an empty remove list" );
	bare.erase( bare.find( emptyRemove ), emptyRemove.size() );
	require( Update_ParseManifest( bare, &release, &error ) && release.remove.empty(), "a manifest without a remove list parses: " + error );

	// The next release no longer ships the EternalJK pk3, and a pk3 from before
	// manifests is retired: its manifest removes both, with the versions shipped.
	const std::wstring previous = work + L"\\previous", next = work + L"\\Next", nextOut = work + L"\\out-next";
	const std::string pk3 = "EternalJK/jofclient-win-x86_64.pk3";
	WriteAll( previous + L"\\1.6.2-beta.4\\" + Update_Widen( UPDATE_MANIFEST ), manifest );
	for ( const auto &file : files ) {
		if ( file.first != pk3 )
			WriteAll( Update_Join( next, file.first ), file.second );
	}
	auto retiredJSON = []( const std::string &path, const updateVersion_t &v ) {
		return "{\"files\": [{\"path\": \"" + path + "\", \"versions\": [{\"size\": " + std::to_string( v.size ) + ", \"sha512\": \"" + Hex( v.sha512, 64 ) + "\"}]}]}";
	};
	const updateVersion_t shipped = Version( emptyZip ), retired = Version( "an old pk3" );
	WriteAll( work + L"\\retired.json", retiredJSON( "base/JoF_Old.pk3", retired ) );
	require( packageAs( next, "1.6.2-beta.5", nextOut, "--previous \"" + Update_Narrow( previous ) + "\" --retired \"" + Update_Narrow( work + L"\\retired.json" ) + "\"" ) == 0,
		"make_update_package.py packages the next release" );
	const std::string nextManifest = ReadAll( nextOut + L"\\" + Update_Widen( UPDATE_MANIFEST ) );
	updateRelease_t nextRelease;
	require( Update_ParseManifest( nextManifest, &nextRelease, &error ), "its manifest parses: " + error );
	auto removes = []( const updateRelease_t &r, const std::string &path, const updateVersion_t *v ) {
		for ( const updateRemove_t &old : r.remove ) {
			if ( old.path == path )
				return !v || ( old.versions.size() == 1 && old.versions[0].size == v->size && !memcmp( old.versions[0].sha512, v->sha512, 64 ) );
		}
		return false;
	};
	require( nextRelease.remove.size() == 2 && removes( nextRelease, pk3, &shipped ) && removes( nextRelease, "base/JoF_Old.pk3", &retired ),
		"it removes the dropped pk3 and the retired one, each with the version shipped, and nothing it still ships" );

	// Later releases keep removing them from the remove list alone, for a player
	// who skipped beta 5, until a release ships the file again.
	const std::wstring onlyNext = work + L"\\previous-next", laterOut = work + L"\\out-later";
	WriteAll( onlyNext + L"\\1.6.2-beta.5\\" + Update_Widen( UPDATE_MANIFEST ), nextManifest );
	updateRelease_t later;
	require( packageAs( next, "1.6.2-beta.6", laterOut, "--previous \"" + Update_Narrow( onlyNext ) + "\"" ) == 0
		&& Update_ParseManifest( ReadAll( laterOut + L"\\" + Update_Widen( UPDATE_MANIFEST ) ), &later, &error )
		&& later.remove.size() == 2 && removes( later, pk3, &shipped ), "an earlier release's remove list carries over: " + error );
	require( packageAs( gamedata, "1.6.2-beta.7", laterOut, "--previous \"" + Update_Narrow( onlyNext ) + "\"" ) == 0
		&& Update_ParseManifest( ReadAll( laterOut + L"\\" + Update_Widen( UPDATE_MANIFEST ) ), &later, &error )
		&& later.remove.size() == 1 && !removes( later, pk3, NULL ), "a file shipped again is not removed: " + error );
	WriteAll( work + L"\\retired-retail.json", retiredJSON( "base/assets0.pk3", retired ) );
	require( packageAs( next, "1.6.2-beta.5", work + L"\\out-retail", "--retired \"" + Update_Narrow( work + L"\\retired-retail.json" ) + "\"" ) != 0,
		"make_update_package.py refuses to remove a retail file" );

	const std::string removed = "\"path\": \"" + pk3 + "\"";
	refusesIn( nextManifest, removed, "\"path\": \"base/JoF_Test.pk3\"", "a manifest that installs and removes the same file is refused" );
	refusesIn( nextManifest, removed, "\"path\": \"base/assets0.pk3\"", "removing a retail file is refused" );
	refusesIn( nextManifest, removed, "\"path\": \"base/autoexec.cfg\"", "removing a config is refused" );
	refusesIn( nextManifest, removed, "\"path\": \"../x.pk3\"", "removing outside GameData is refused" );
	refusesIn( nextManifest, "\"remove\": [", "\"remove\": [{\"path\": \"base/x.pk3\", \"versions\": []}, ", "a file to remove with no versions is refused" );
	refusesIn( nextManifest, "\"remove\": [", "\"remove\": [{\"path\": \"base/JoF_Old.pk3\", \"versions\": [{\"size\": 1, \"sha512\": \"" + std::string( 128, 'a' ) + "\"}]}, ",
		"a file to remove listed twice is refused" );
	refusesIn( nextManifest, "\"remove\": [", "\"remove\": 5, \"rest\": [", "a remove list that is not a list is refused" );
}

static void CheckRemovals( void ) {
	// Only a file still as JoF shipped it goes: not one the player changed, not
	// even to a shipped size, and nothing for one that is not there.
	const std::wstring dir = work + L"\\removals";
	WriteAll( dir + L"\\base\\shipped.pk3", "shipped v2" );
	WriteAll( dir + L"\\base\\changed.pk3", "changed by the player" );
	WriteAll( dir + L"\\EternalJK\\samesize.pk3", "shipped vX" );
	updateRelease_t r;
	r.remove = {
		{ "base/shipped.pk3", { Version( "shipped v1" ), Version( "shipped v2" ) } },
		{ "base/changed.pk3", { Version( "shipped" ) } },
		{ "EternalJK/samesize.pk3", { Version( "shipped v1" ) } },
		{ "base/missing.pk3", { Version( "anything" ) } },
	};
	std::vector<updateRemoval_t> remove;
	std::vector<std::string> kept;
	Update_PlanRemovals( dir, r, &remove, &kept );
	require( remove.size() == 1 && remove[0].path == "base/shipped.pk3" && remove[0].size == 10, "only the file still as JoF shipped it is removed" );
	require( kept == std::vector<std::string>( { "base/changed.pk3", "EternalJK/samesize.pk3" } ), "the changed ones are kept and named, the missing one neither" );

	// The running program is never removed, even listed with its own bytes.
	const std::wstring program = Update_ProgramPath();
	const size_t slash = program.find_last_of( L'\\' );
	updateVersion_t own;
	require( slash != std::wstring::npos && Update_HashFile( program, &own.size, own.sha512 ), "hash this program" );
	updateRelease_t self;
	self.remove = { { Update_Narrow( program.substr( slash + 1 ) ), { own } } };
	std::vector<updateRemoval_t> none;
	std::vector<std::string> keptSelf;
	Update_PlanRemovals( program.substr( 0, slash ), self, &none, &keptSelf );
	require( none.empty() && keptSelf.size() == 1, "the running program is kept" );

	// The swap moves it aside as .old, like a replaced file, and a resumed swap skips it.
	const std::wstring noStaging = work + L"\\no-staging";
	int moved = 0;
	std::string error;
	std::wstring journal = Journal( L"removal" );
	require( Update_Swap( dir, noStaging, {}, remove, journal, &moved, &error ) && moved == 1, "the removal goes through: " + error );
	require( !Update_Exists( dir + L"\\base\\shipped.pk3" ) && ReadAll( dir + L"\\base\\shipped.pk3.old" ) == "shipped v2",
		"the file is gone, kept as .old until the next start" );
	require( Update_JournalAside( journal ) == std::vector<std::string>( { "base/shipped.pk3" } ), "noted in the journal before it went aside" );
	moved = 0;
	require( Update_Swap( dir, noStaging, {}, remove, journal, &moved, &error ) && moved == 0, "a removal done before a crash is skipped" );

	// A file changed between the check and the swap stays, even at the same size.
	WriteAll( dir + L"\\base\\shipped.pk3", "shipped v2" );
	remove.clear();
	kept.clear();
	Update_PlanRemovals( dir, r, &remove, &kept );
	require( remove.size() == 1, "the file is found to be JoF's again" );
	Sleep( 50 );
	WriteAll( dir + L"\\base\\shipped.pk3", "edited! v2" );
	require( Update_Swap( dir, noStaging, {}, remove, Journal( L"changed" ), &moved, &error ) && moved == 0
		&& ReadAll( dir + L"\\base\\shipped.pk3" ) == "edited! v2", "a file changed since the check is left alone" );
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
	const std::wstring journal = Journal( L"first" );
	require( Update_Swap( dir, staging, list, {}, journal, &moved, &error ) && moved == 3, "the staged files go in: " + error );
	require( ReadAll( dir + L"\\a.dll" ) == "new a" && ReadAll( dir + L"\\base\\b.pk3" ) == "new b" && ReadAll( dir + L"\\EternalJK\\c.pk3" ) == "new c",
		"each file holds its new bytes" );
	require( ReadAll( dir + L"\\a.dll.old" ) == "old a" && !Update_Exists( dir + L"\\EternalJK\\c.pk3.old" ) && !Update_Exists( staging + L"\\a.dll" ),
		"the old ones are kept as .old, and staging is empty" );
	require( Update_JournalAside( journal ) == std::vector<std::string>( { "a.dll", "base/b.pk3" } ), "the journal names each file moved aside, the new one not" );
	moved = 0;
	require( Update_Swap( dir, staging, list, {}, journal, &moved, &error ) && moved == 0, "files no longer staged went in already (a crash mid-swap resumes)" );

	// A read-only file, and the read-only .old it leaves, do not stop the next update.
	SetFileAttributesW( ( dir + L"\\a.dll" ).c_str(), FILE_ATTRIBUTE_READONLY );
	WriteAll( staging + L"\\a.dll", "next a" );
	require( Update_Swap( dir, staging, { "a.dll" }, {}, Journal( L"read-only" ), &moved, &error ) && moved == 1, "a read-only file is replaced: " + error );
	require( SetFileAttributesW( ( dir + L"\\a.dll.old" ).c_str(), FILE_ATTRIBUTE_READONLY ) != 0, "make the .old read-only" );
	WriteAll( staging + L"\\a.dll", "new a" );
	require( Update_Swap( dir, staging, { "a.dll" }, {}, Journal( L"read-only-old" ), &moved, &error ) && ReadAll( dir + L"\\a.dll" ) == "new a",
		"a read-only .old is replaced: " + error );
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
	require( !Update_Swap( dir, staging, list, {}, Journal( L"held" ), &moved, &error ) && moved == 0, "a file in use stops the swap" );
	require( error.find( "base/b.pk3 is in use" ) == 0, "and says which file: " + error );
	CloseHandle( held );
	require( ReadAll( dir + L"\\a.dll" ) == "new a" && ReadAll( dir + L"\\base\\b.pk3" ) == "new b", "the installed files are back as they were" );
	require( ReadAll( staging + L"\\a.dll" ) == "newer a" && ReadAll( staging + L"\\base\\b.pk3" ) == "newer b", "and the update is staged again for the next try" );

	// A file to remove that another client has open stops the swap the same way,
	// and what already went in and what already went out are put back.
	WriteAll( dir + L"\\EternalJK\\gone.pk3", "gone" );
	std::vector<updateRemoval_t> out( 2 );
	out[0].path = "EternalJK/gone.pk3";
	out[1].path = "base/b.pk3";
	require( Update_Stamp( dir + L"\\EternalJK\\gone.pk3", &out[0].size, &out[0].written ) && Update_Stamp( dir + L"\\base\\b.pk3", &out[1].size, &out[1].written ),
		"stamp the files to remove" );
	held = CreateFileW( ( dir + L"\\base\\b.pk3" ).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL );
	require( held != INVALID_HANDLE_VALUE, "hold b.pk3 open again" );
	printf( "(waiting %lu s on a file to remove in use)\n", APPLY_WAIT_MS / 1000 );
	require( !Update_Swap( dir, staging, { "a.dll" }, out, Journal( L"held-removal" ), &moved, &error ) && moved == 0, "a file to remove in use stops the swap" );
	require( error.find( "base/b.pk3 is in use" ) == 0, "and says which file: " + error );
	CloseHandle( held );
	require( ReadAll( dir + L"\\a.dll" ) == "new a" && ReadAll( dir + L"\\EternalJK\\gone.pk3" ) == "gone" && ReadAll( dir + L"\\base\\b.pk3" ) == "new b",
		"what went in and what went out are back" );
	require( ReadAll( staging + L"\\a.dll" ) == "newer a", "and the update is staged again" );

	// A swap stopped (a crash) after x.dll went in and z.pk3 went out: if the resumed
	// swap then fails, those go back too, so the old release is whole again.
	const std::wstring dir2 = work + L"\\stopped", staging2 = work + L"\\stopped-staging";
	WriteAll( dir2 + L"\\x.dll", "old x" );
	WriteAll( dir2 + L"\\base\\y.pk3", "old y" );
	WriteAll( dir2 + L"\\base\\z.pk3", "shipped z" );
	WriteAll( staging2 + L"\\x.dll", "new x" );
	WriteAll( staging2 + L"\\base\\y.pk3", "new y" );
	std::vector<updateRemoval_t> zOut( 1 );
	zOut[0].path = "base/z.pk3";
	require( Update_Stamp( dir2 + L"\\base\\z.pk3", &zOut[0].size, &zOut[0].written ), "stamp z.pk3" );
	require( MoveFileExW( ( dir2 + L"\\x.dll" ).c_str(), ( dir2 + L"\\x.dll.old" ).c_str(), 0 )
		&& MoveFileExW( ( staging2 + L"\\x.dll" ).c_str(), ( dir2 + L"\\x.dll" ).c_str(), 0 )
		&& MoveFileExW( ( dir2 + L"\\base\\z.pk3" ).c_str(), ( dir2 + L"\\base\\z.pk3.old" ).c_str(), 0 ), "stop a swap halfway" );
	const std::wstring journal2 = Journal( L"stopped", "aside x.dll\naside base/z.pk3\n" );
	held = CreateFileW( ( dir2 + L"\\base\\y.pk3" ).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL );
	require( held != INVALID_HANDLE_VALUE, "hold y.pk3 open" );
	printf( "(waiting %lu s on a file in use, resuming a stopped swap)\n", APPLY_WAIT_MS / 1000 );
	require( !Update_Swap( dir2, staging2, { "x.dll", "base/y.pk3" }, zOut, journal2, &moved, &error ) && moved == 0, "a file in use stops the resumed swap" );
	CloseHandle( held );
	require( ReadAll( dir2 + L"\\x.dll" ) == "old x" && ReadAll( dir2 + L"\\base\\y.pk3" ) == "old y" && ReadAll( dir2 + L"\\base\\z.pk3" ) == "shipped z",
		"what the stopped swap had moved is back too" );
	require( ReadAll( staging2 + L"\\x.dll" ) == "new x" && ReadAll( staging2 + L"\\base\\y.pk3" ) == "new y" && !Update_Exists( dir2 + L"\\x.dll.old" ),
		"and the whole update is staged again" );
	require( Update_JournalAside( journal2 ) == std::vector<std::string>( { "x.dll", "base/z.pk3", "base/y.pk3" } ),
		"the journal notes a file once a try, not once a retry" );
	require( Update_Swap( dir2, staging2, { "x.dll", "base/y.pk3" }, zOut, journal2, &moved, &error ) && moved == 3
		&& ReadAll( dir2 + L"\\x.dll" ) == "new x" && ReadAll( dir2 + L"\\base\\y.pk3" ) == "new y" && !Update_Exists( dir2 + L"\\base\\z.pk3" ),
		"and then goes in whole: " + error );

	// A swap stopped between p.dll's two renames: p.dll is aside as .old, its new copy
	// still staged. A stale n.pk3.old, from an update before, belongs to no swap. When
	// the resumed swap then fails on q.pk3, p.dll's backup goes back, the stale .old
	// does not, and the new n.pk3 goes back to staging.
	const std::wstring dir3 = work + L"\\between", staging3 = work + L"\\between-staging";
	const std::vector<std::string> list3 = { "p.dll", "EternalJK/n.pk3", "base/q.pk3" };
	WriteAll( dir3 + L"\\p.dll", "old p" );
	WriteAll( dir3 + L"\\EternalJK\\n.pk3.old", "stale n" );
	WriteAll( dir3 + L"\\base\\q.pk3", "old q" );
	WriteAll( staging3 + L"\\p.dll", "new p" );
	WriteAll( staging3 + L"\\EternalJK\\n.pk3", "new n" );
	WriteAll( staging3 + L"\\base\\q.pk3", "new q" );
	require( MoveFileExW( ( dir3 + L"\\p.dll" ).c_str(), ( dir3 + L"\\p.dll.old" ).c_str(), 0 ), "stop a swap between p.dll's renames" );
	const std::wstring journal3 = Journal( L"between", "aside p.dll\n" );
	held = CreateFileW( ( dir3 + L"\\base\\q.pk3" ).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL );
	require( held != INVALID_HANDLE_VALUE, "hold q.pk3 open" );
	printf( "(waiting %lu s on a file in use, resuming a swap stopped between two renames)\n", APPLY_WAIT_MS / 1000 );
	require( !Update_Swap( dir3, staging3, list3, {}, journal3, &moved, &error ) && moved == 0, "a file in use stops it" );
	CloseHandle( held );
	require( ReadAll( dir3 + L"\\p.dll" ) == "old p" && !Update_Exists( dir3 + L"\\p.dll.old" ) && ReadAll( staging3 + L"\\p.dll" ) == "new p",
		"the file stopped between its renames is back from its .old, its new copy staged again" );
	require( !Update_Exists( dir3 + L"\\EternalJK\\n.pk3" ) && ReadAll( dir3 + L"\\EternalJK\\n.pk3.old" ) == "stale n"
		&& ReadAll( staging3 + L"\\EternalJK\\n.pk3" ) == "new n", "a .old the journal does not name is never put back" );
	require( ReadAll( dir3 + L"\\base\\q.pk3" ) == "old q" && ReadAll( staging3 + L"\\base\\q.pk3" ) == "new q", "the file in use is untouched" );
	require( Update_Swap( dir3, staging3, list3, {}, journal3, &moved, &error ) && moved == 3 && ReadAll( dir3 + L"\\p.dll" ) == "new p"
		&& ReadAll( dir3 + L"\\p.dll.old" ) == "old p" && ReadAll( dir3 + L"\\EternalJK\\n.pk3" ) == "new n" && ReadAll( dir3 + L"\\base\\q.pk3" ) == "new q",
		"and then it goes in whole: " + error );

	// A swap that had put s.dll in, then lost power while noting t.pk3: the journal
	// ends in a torn record. The update still reads; t.pk3 was not moved aside yet; a
	// failed resume still puts s.dll's backup back; then it goes in whole.
	const std::wstring dir4 = work + L"\\torn", staging4 = work + L"\\torn-staging", applying = work + L"\\torn-applying.txt";
	WriteAll( dir4 + L"\\s.dll.old", "old s" );
	WriteAll( dir4 + L"\\s.dll", "new s" );
	WriteAll( dir4 + L"\\base\\t.pk3", "old t" );
	WriteAll( staging4 + L"\\base\\t.pk3", "new t" );
	WriteAll( applying, "jofupdate 1\ntag 1.6.2\nattempts 0\nfile s.dll\nfile base/t.pk3\n\naside s.dll\n\naside base/t." );
	updatePending_t p;
	require( Update_ReadPending( applying, &p ) && p.files.size() == 2 && p.aside == std::vector<std::string>( { "s.dll" } ),
		"an update whose journal ends in a torn record still reads" );
	held = CreateFileW( ( dir4 + L"\\base\\t.pk3" ).c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_EXISTING, 0, NULL );
	require( held != INVALID_HANDLE_VALUE, "hold t.pk3 open" );
	printf( "(waiting %lu s on a file in use, resuming after a torn journal record)\n", APPLY_WAIT_MS / 1000 );
	require( !Update_Swap( dir4, staging4, p.files, p.remove, applying, &moved, &error ) && moved == 0, "a file in use stops the resumed swap" );
	CloseHandle( held );
	require( ReadAll( dir4 + L"\\s.dll" ) == "old s" && ReadAll( staging4 + L"\\s.dll" ) == "new s" && ReadAll( dir4 + L"\\base\\t.pk3" ) == "old t",
		"what went in before the power cut goes back, as its intact record says" );
	require( Update_Swap( dir4, staging4, p.files, p.remove, applying, &moved, &error ) && moved == 2
		&& ReadAll( dir4 + L"\\s.dll" ) == "new s" && ReadAll( dir4 + L"\\base\\t.pk3" ) == "new t", "and then it goes in whole: " + error );
	updatePending_t after;
	require( Update_ReadPending( applying, &after ), "the journal still reads after more records went on after the torn one" );
}

static void CheckPending( void ) {
	const std::wstring path = work + L"\\ready.txt";
	updatePending_t p;
	p.tag = "1.6.2";
	p.attempts = 2;
	p.files = { "eternaljk.x86_64.exe", "base/JoF_Test.pk3" };
	p.remove = { { "EternalJK/old.pk3", 123456, 133420000000000000ull } };
	p.aside = { "eternaljk.x86_64.exe" };
	require( Update_WritePending( path, p ), "ready.txt writes" );
	updatePending_t read;
	require( Update_ReadPending( path, &read ) && read.tag == "1.6.2" && read.attempts == 2 && read.files == p.files, "ready.txt reads back" );
	require( read.remove.size() == 1 && read.remove[0].path == "EternalJK/old.pk3" && read.remove[0].size == 123456 && read.remove[0].written == 133420000000000000ull,
		"with the file to remove, its size and time" );
	require( read.aside == p.aside && Update_JournalAside( path ) == p.aside, "and the swap's journal, kept for the next try" );
	WriteAll( path, "jofupdate 1\ntag 1.6.2\nremove 5 6 base/old.pk3\n" );
	updatePending_t onlyRemove;
	require( Update_ReadPending( path, &onlyRemove ) && onlyRemove.files.empty() && onlyRemove.remove.size() == 1, "an update may only remove files" );
	// A journal record a power cut tore as it was appended is passed over, and the
	// intact records stay: anything of it, garbage, or a path the updater may not write.
	const std::string list = "jofupdate 1\ntag 1.6.2\nattempts 0\nfile x.dll\nfile base/y.pk3\n";
	const std::vector<std::string> justX = { "x.dll" };
	for ( const std::string &tail : { std::string( "\naside x.dll\n\naside base/y" ), "\naside x.dll\n" + std::string( 9, '\0' ),
			std::string( "\naside x.dll\n\nasi" ), std::string( "\naside x.dll\n\naside ../x.dll\n" ) } ) {
		WriteAll( path, list + tail );
		updatePending_t torn;
		require( Update_ReadPending( path, &torn ) && torn.files.size() == 2 && torn.aside == justX && Update_JournalAside( path ) == justX,
			"a torn journal record is passed over, and the intact ones kept" );
	}
	WriteAll( path, list + "\naside ba" );
	updatePending_t tornFirst;
	require( Update_ReadPending( path, &tornFirst ) && tornFirst.aside.empty(), "so is the first record, torn" );
	// The next record goes on a line of its own, not onto the torn one.
	WriteAll( path, list + "\naside x.dll\n\naside base/y" );
	require( Update_JournalAppend( path, "aside base/y.pk3" ) && Update_JournalAside( path ) == std::vector<std::string>( { "x.dll", "base/y.pk3" } ),
		"a record appended after a torn one is read" );
	// But the list after a torn record is no journal's tail.
	WriteAll( path, list + "garbage\nfile z.dll\n" );
	updatePending_t notTail;
	require( !Update_ReadPending( path, &notTail ), "a list line after a torn record makes the list unreadable" );

	for ( const char *bad : { "file base/autoexec.cfg", "remove 5 6 base/autoexec.cfg", "remove 5 6 base/assets0.pk3", "remove x 6 base/old.pk3",
			"remove 5 base/old.pk3", "remove 5 6", "remove -5 6 base/old.pk3" } ) {
		WriteAll( path, std::string( "jofupdate 1\ntag 1.6.2\n" ) + bad + "\n" );
		updatePending_t refused;
		require( !Update_ReadPending( path, &refused ), std::string( "ready.txt cannot say '" ) + bad + "'" );
	}
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
	CheckRemovals();
	CheckPending();
	CheckSwap();

	Update_RemoveTree( work );
	printf( "updater: %d checks passed\n", checks );
	return 0;
}
