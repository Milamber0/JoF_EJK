// Checks SJK chat (codemp/client/cl_sjk.cpp) without the engine or a hub: the
// signature against the hub protocol's test vector, SJK's key file, base64url, the
// text the hub takes, the game's charset both ways, hub addresses, the User-Agent, how
// a message of the feed reads, GIPHY links, the SJK chat menu's history and the
// server's copy of a line.
//
//   sjk_chat_check

#include "cl_sjk.cpp"	// the copy beside this test's client.h

#include <string>

static int checks;

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
const char *Cvar_VariableString( const char * ) { return ""; }
cvar_t *Cvar_Set2( const char *, const char *value, uint32_t, qboolean ) { return Cvar_Get( "", value, 0 ); }
void Cmd_AddCommand( const char *, xcommand_t, const char * ) {}
void Cmd_RemoveCommand( const char * ) {}
int Cmd_Argc( void ) { return 0; }
char *Cmd_ArgsFrom( int ) { return (char *)""; }
void Com_Printf( const char *fmt, ... ) {
	va_list args;
	va_start( args, fmt );
	vprintf( fmt, args );
	va_end( args );
}
qboolean CL_SJK_ToChatBox( const sjkChatLine_t * ) { return qfalse; }
qboolean CL_SJK_ServerSession( char *, int, char *, int ) { return qfalse; }
void CL_SayToServer( const char * ) {}
int Sys_Milliseconds( bool ) { return 0; }
void Q_StripColor( char *text ) {	// as shared/qcommon/q_string.c does it
	char *write = text;
	for ( char *read = text; *read; read++ ) {
		if ( read[0] == '^' && read[1] >= '0' && read[1] <= '9' ) {
			read++;
			continue;
		}
		*write++ = *read;
	}
	*write = 0;
}

static void CheckSignature( void ) {
	// The hub protocol's test vector, which the hub and SJK test against too.
	uint8_t seed[32], nonce[12];
	memset( seed, 0x07, sizeof( seed ) );
	memset( nonce, 0x01, sizeof( nonce ) );
	sjkIdentity_t id;
	require( SJK_IdentityFromSeed( seed, &id ), "a key from a seed" );
	require( id.key == "6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw", "the public key as the hub writes it: " + id.key );
	require( id.keyId == "fe812c12f3ab4ce6", "the key id: " + id.keyId );
	const std::string header = SJK_Authorization( id, "POST", "/v1/claim", "{\"server\":\"1.2.3.4:29070\",\"slot\":3,\"name\":\"Sol\"}", 1000000, nonce );
	require( header == "SJK-Sig key=6kpsY-KcUgq-9VB7Ey7F-ZVHdq6-vnuSQh7qaRRG0iw, ts=1000000, nonce=AQEBAQEBAQEBAQEB, "
		"sig=IKLlpqaSWIXqXK7Q77SsGJiE4BIdUTjkSWv2zyr0951WKByhEShSL-Ow70Rgc3znokl0Sf4rQK2VjNkPEm9uCQ", "the test vector's signature: " + header );
}

static void CheckKeyFile( void ) {
	uint8_t seed[32], again[32];
	memset( seed, 0x07, sizeof( seed ) );
	const std::string text = std::string( SJK_KEY_MAGIC ) + "\n" + SJK_Base64( seed, 32 ) + "\n";
	require( SJK_ParseKeyFile( text, again ) && !memcmp( seed, again, 32 ), "SJK's key file reads" );
	require( SJK_ParseKeyFile( "\r\n  " SJK_KEY_MAGIC " \r\n\r\n" + SJK_Base64( seed, 32 ) + "\r\n\r\n", again ) && !memcmp( seed, again, 32 ),
		"with Windows line ends, spaces and blank lines" );
	for ( const std::string &bad : { std::string( "SJK-IDENTITY-2\n" ) + SJK_Base64( seed, 32 ) + "\n", text + "more\n",
			std::string( SJK_KEY_MAGIC ) + "\n" + SJK_Base64( seed, 31 ) + "\n", std::string( SJK_KEY_MAGIC ) + "\n", std::string(),
			std::string( SJK_KEY_MAGIC ) + "\n" + SJK_Base64( seed, 32 ).substr( 1 ) + "!\n" } )
		require( !SJK_ParseKeyFile( bad, again ), "not a key file: " + bad );
}

static void CheckBase64( void ) {
	const uint8_t bytes[] = { 0xfb, 0xff, 0xbf, 0x00, 0x10 };
	for ( size_t n = 0; n <= sizeof( bytes ); n++ ) {
		std::vector<uint8_t> back;
		const std::string text = SJK_Base64( bytes, n );
		require( text.find_first_of( "+/=" ) == std::string::npos, "base64url, unpadded" );
		require( SJK_Unbase64( text, &back ) && back.size() == n && !memcmp( back.data(), bytes, n ), "base64url round trip" );
	}
	std::vector<uint8_t> back;
	require( !SJK_Unbase64( "ab+c", &back ) && !SJK_Unbase64( "ab c", &back ), "not base64url" );
}

static void CheckText( void ) {
	std::string out, why;
	require( SJK_PrepareText( "  hi \t  there  ", &out, &why ) && out == "hi there", "runs of spaces become one, ends trimmed: " + out );
	require( !SJK_PrepareText( "   ", &out, &why ), "nothing to say" );
	require( !SJK_PrepareText( "a\nb", &out, &why ), "one line" );
	require( SJK_PrepareText( std::string( 300, 'x' ), &out, &why ), "300 characters" );
	require( !SJK_PrepareText( std::string( 301, 'x' ), &out, &why ), "not 301" );
	std::string accents;
	for ( int i = 0; i < 300; i++ )
		accents += "\xC3\xA9";
	require( SJK_PrepareText( accents, &out, &why ), "300 characters, counted as characters, not bytes" );

	// The game's charset to UTF-8 and back, as the language picks.
	require( SJK_ToUTF8( "caf\xE9", "english" ) == "caf\xC3\xA9", "Latin-1 to UTF-8" );
	const char cyrillicA = (char)ConvertUTF32ToExpectedCharset( 0x0410 );
	require( SJK_ToUTF8( std::string( 1, cyrillicA ).c_str(), "russian" ) == "\xD0\x90", "Cyrillic in Russian" );
	const char centralS = (char)ConvertUTF32ToExpectedCharset( 0x0160 );
	require( SJK_ToUTF8( std::string( 1, centralS ).c_str(), "english" ) == "\xC5\xA0", "a byte Latin-1 has no letter for is the central one" );
	require( SJK_FromUTF8( "caf\xC3\xA9" ) == "caf\xE9", "UTF-8 to Latin-1" );
	require( SJK_FromUTF8( "\xD0\x90" ) == std::string( 1, cyrillicA ), "UTF-8 Cyrillic as the game keeps it" );
	require( SJK_FromUTF8( "hi \xF0\x9F\xA6\x8A!" ) == "hi ?!", "what the game cannot show is '?'" );
	require( SJK_FromUTF8( std::string( "a\x01" "b\x7f" "c\xFF", 6 ) ) == "abc?", "control characters and broken UTF-8" );
}

static void CheckHubAddress( void ) {
	std::string hub, why;
	require( SJK_HubAddress( "https://sjk.dfox.app", &hub, &why ) && hub == "https://sjk.dfox.app", "SJK's hub" );
	require( SJK_HubAddress( "https://hub.example.com:8443/", &hub, &why ) && hub == "https://hub.example.com:8443", "a port, a trailing slash dropped" );
	require( SJK_HubAddress( "http://127.0.0.1:8787", &hub, &why ), "a local test hub" );
	for ( const char *bad : { "http://sjk.dfox.app", "https://sjk.dfox.app/v1", "ftp://x", "https://", "http://127.0.0.1", "sjk.dfox.app" } )
		require( !SJK_HubAddress( bad, &hub, &why ), std::string( "not a hub address: " ) + bad );
}

static void CheckUserAgent( void ) {
	require( SJK_UserAgent( "1.0.1-beta.2", "1.0.0", "0123abc" ) == "EJK/1.0.1-beta.2 (0123abc)", "a release: its tag and commit" );
	require( SJK_UserAgent( "1.0.1", "1.0.1", "0123ABC" ) == "EJK/1.0.1 (0123ABC)", "a stable release" );
	require( SJK_UserAgent( "", "1.0.0", "local" ) == "EJK/1.0.0-dev", "a build from source, its commit unknown" );
	require( SJK_UserAgent( "", "dev", "local" ) == "EJK/dev", "a build from source with no release tag at all" );
	require( SJK_UserAgent( "", "1.0.0", "0123abc" ) == "EJK/1.0.0-dev (0123abc)", "a build from source that knows its commit" );
	require( SJK_UserAgent( "1.0.1", "1.0.1", "0123" ) == "EJK/1.0.1", "no commit that is not 7 to 40 hex digits" );
	require( SJK_UserAgent( JOF_RELEASE_TAG, JOF_VERSION_STRING, JOF_COMMIT_SHA_SHORT ).size() <= 120, "short enough for the hub to keep whole" );
}

static void CheckMessageLine( void ) {
	cJSON *message = cJSON_Parse( "{\"id\":12,\"at\":1,\"key_id\":\"ab\",\"name\":\"^2Sol\",\"verified\":true,\"text\":\"hello ^1there \xC3\xA9\"}" );
	sjkLine_t line = SJK_MessageParts( message );
	require( line.who == "^2Sol^2+" && line.text == "hello there \xE9", "a feed message, its text's colours left out: " + line.who + " / " + line.text );
	require( line.nameWords == "sol" && line.words == "hellothere", "what tells its copy on the server: " + line.nameWords + " / " + line.words );
	require( SJK_ConsoleLine( line ) == "^2Sol^2+^7 ^8SJK^7: ^3hello there \xE9", "as the console shows it: " + SJK_ConsoleLine( line ) );
	line.inAll = true;
	require( SJK_ConsoleLine( line ) == "^2Sol^2+^7 ^8ALL/SJK^7: ^3hello there \xE9", "said in All chat too: " + SJK_ConsoleLine( line ) );
	cJSON_Delete( message );
	message = cJSON_Parse( "{\"id\":13,\"name\":\"Fox\",\"verified\":false,\"text\":\"hi\"}" );
	require( SJK_MessageParts( message ).who == "Fox", "unverified: no mark" );
	cJSON_Delete( message );
	message = cJSON_Parse( "{\"id\":14,\"name\":\"Fox\"}" );
	require( SJK_MessageParts( message ).text.empty(), "no text, no line" );
	cJSON_Delete( message );
	message = cJSON_Parse( "{\"id\":15,\"name\":\"Fox\",\"text\":\"look https://giphy.com/gifs/3oEhmHmWP3Y9wQxoli!\"}" );
	line = SJK_MessageParts( message );
	require( line.text == "look <GIF>!", "a GIF shows as <GIF>: " + line.text );
	require( line.words == "lookhttpsgiphycomgifs3oehmhmwp3y9wqxoli", "its words are what was said, the link too: " + line.words );
	cJSON_Delete( message );
	const sjkLine_t notice = { S_COLOR_YELLOW "SJK chat: not sent" };
	require( SJK_ConsoleLine( notice ) == notice.text, "a notice as it is" );

	// The client the hub says a message came from, for its icon: only the two it names.
	for ( const auto &item : { std::make_pair( "\"client\":\"sjk\",", "sjk" ), std::make_pair( "\"client\":\"ejk\",", "ejk" ),
			std::make_pair( "\"client\":\"TaystJK\",", "" ), std::make_pair( "\"client\":\"SJK\",", "" ), std::make_pair( "\"client\":3,", "" ),
			std::make_pair( "", "" ) } ) {
		message = cJSON_Parse( ( std::string( "{\"id\":15," ) + item.first + "\"name\":\"Fox\",\"text\":\"hi\"}" ).c_str() );
		require( SJK_MessageClient( message ) == item.second, std::string( "the client of " ) + item.first );
		cJSON_Delete( message );
	}
}

// GIPHY's links as SJK reads them (chat_gifs/link.rs): each becomes <GIF>, nothing else.
static void CheckGifs( void ) {
	for ( const char *link : { "https://giphy.com/gifs/happy-birthday-cake-3oEhmHmWP3Y9wQxoli",
			"https://www.giphy.com/gifs/3oEhmHmWP3Y9wQxoli", "http://giphy.com/stickers/cat-dance-W3QKEujo8vztC",
			"https://giphy.com/embed/3oEhmHmWP3Y9wQxoli", "https://giphy.com/gifs/3oEhmHmWP3Y9wQxoli/",
			"https://media.giphy.com/media/3oEhmHmWP3Y9wQxoli/giphy.gif",
			"https://media2.giphy.com/media/v1.Y2lkPTc5MGI3NjEx/3oEhmHmWP3Y9wQxoli/200w.webp",
			"https://i.giphy.com/3oEhmHmWP3Y9wQxoli.gif", "https://i.giphy.com/3oEhmHmWP3Y9wQxoli.webp",
			"https://i.giphy.com/media/3oEhmHmWP3Y9wQxoli/giphy-downsized.gif",
			"HTTPS://Media.Giphy.COM/media/3oEhmHmWP3Y9wQxoli/giphy.gif",
			"https://media.giphy.com/media/3oEhmHmWP3Y9wQxoli/giphy.gif?cid=ab12&ct=g#top" } )
		require( SJK_GifText( link ) == "<GIF>", std::string( "a GIPHY link: " ) + link );
	for ( const char *text : { "https://giphy.com/gifs/happy-birthday", "https://giphy.com/search/cats",
			"https://giphy.com.example.org/gifs/3oEhmHmWP3Y9wQxoli", "https://fox@giphy.com/gifs/3oEhmHmWP3Y9wQxoli",
			"https://giphy.com:443/gifs/3oEhmHmWP3Y9wQxoli", "https://media5.giphy.com/media/3oEhmHmWP3Y9wQxoli/giphy.gif",
			"https://media.giphy.com/media/3oEhmHmWP3Y9wQxoli/giphy", "https://giphy.com//gifs/3oEhmHmWP3Y9wQxoli",
			"https://giphy.com/gifs/3oEhmHmWP3Y9wQxoli//", "ftp://giphy.com/gifs/3oEhmHmWP3Y9wQxoli",
			"giphy.com/gifs/3oEhmHmWP3Y9wQxoli", "https://i.giphy.com/abc12.gif", "https://tenor.com/view/cat-dance-12345678",
			"(https://giphy.com/gifs/3oEhmHmWP3Y9wQxoli)", "https://giphy.com", "" } )
		require( SJK_GifText( text ) == text, std::string( "not a GIPHY GIF: " ) + text );
	require( SJK_GifText( "look https://giphy.com/gifs/3oEhmHmWP3Y9wQxoli!! and https://giphy.com/embed/W3QKEujo8vztC." )
		== "look <GIF>!! and <GIF>.", "links in a line, the punctuation after each kept" );
	require( SJK_GifText( " a  b " ) == " a  b ", "spaces as they were" );
}

// What of a line survives a game server's old code pages, to know its copy there.
static void CheckWords( void ) {
	require( SJK_Words( "Hello, ^1World 42! \xC3\xA9t\xC3\xA9" ) == "helloworld42t", "letters and digits, lower case, no colour codes or other scripts" );
	require( SJK_Words( "\xF0\x9F\xA6\x8A ?! ^1" ).empty(), "none in an emoji and symbols" );
	require( SJK_Words( "^^1a^" ) == "a", "a ^ that is no colour code is no letter either" );
}

// Only a feed answer moves the reading on: anything else counts as a failed read.
static void CheckFeedAnswer( void ) {
	for ( const char *good : { "{\"next\":14,\"chat\":[],\"online\":3}", "{\"next\":0,\"chat\":[{\"id\":1,\"text\":\"hi\"}]}" } ) {
		cJSON *root = cJSON_Parse( good );
		require( SJK_FeedAnswer( root ), std::string( "a feed answer: " ) + good );
		cJSON_Delete( root );
	}
	for ( const char *bad : { "", "<html>ok</html>", "null", "[]", "{}", "{\"chat\":[]}", "{\"next\":14}",
			"{\"next\":\"14\",\"chat\":[]}", "{\"next\":14,\"chat\":{}}" } ) {
		cJSON *root = cJSON_Parse( bad );
		require( !SJK_FeedAnswer( root ), std::string( "not a feed answer: " ) + bad );
		cJSON_Delete( root );
	}
}

// A new session's backlog adds to the SJK chat menu's history only what it lacks.
static void CheckHistory( void ) {
	sjkLine_t line;
	line.who = "Fox";
	line.text = "hi";
	line.id = "41";
	require( !SJK_Remembered( line ), "a message not in the history yet" );
	SJK_Remember( line );
	require( SJK_Remembered( line ), "the same message again" );
	sjkLine_t other = line;
	other.text = "hi there";
	require( !SJK_Remembered( other ), "the same id with other words, as another hub's" );
	other = line;
	other.who = "Sol";
	require( !SJK_Remembered( other ), "the same id from someone else" );
	other = line;
	other.id = "42";
	require( !SJK_Remembered( other ), "the same words with another id, said again" );
	other = line;
	other.id.clear();
	require( !SJK_Remembered( other ), "nothing to tell a message without an id by" );
}

static void CheckServerCopy( void ) {
	// The id the hub gave a message, from the feed and from the hub's answer.
	for ( const auto &item : { std::make_pair( "{\"id\":12,\"text\":\"hi\"}", "12" ), std::make_pair( "{\"id\":4294967296}", "4294967296" ),
			std::make_pair( "{\"text\":\"hi\"}", "" ), std::make_pair( "{\"id\":\"12\"}", "" ), std::make_pair( "{\"id\":0}", "" ) } ) {
		cJSON *message = cJSON_Parse( item.first );
		require( SJK_MessageId( message ) == item.second, std::string( "the id of " ) + item.first );
		cJSON_Delete( message );
	}
	require( SJK_MessageId( NULL ).empty(), "no answer, no id" );

	// The end of the server's copy: the id in colour codes, which no client draws.
	require( SJK_ServerMarker( "12" ) == "^9^2^1^2", "the marker of id 12: " + SJK_ServerMarker( "12" ) );
	require( SJK_ServerMarker( "7" ) == "^9^1^7" && SJK_ServerMarker( "123456789" ) == "^9^9^1^2^3^4^5^6^7^8^9", "1 to 9 digits" );
	require( SJK_ServerMarker( "" ).empty() && SJK_ServerMarker( "1234567890" ).empty() && SJK_ServerMarker( "1a" ).empty(),
		"none for no id, 10 digits or not a number" );
	require( SJK_JSONBody( "hi", "Sol" ) == "{\"text\":\"hi\",\"name\":\"Sol\"}", "the chat body is only the text and the name" );

	// The pieces the server's chat gets: what a server keeps of a say, quoted.
	std::vector<std::string> pieces = SJK_ServerPieces( "  hello there  " );
	require( pieces.size() == 1 && pieces[0] == "hello there", "a short line, trimmed" );
	require( SJK_ServerPieces( "   " ).empty() && SJK_ServerPieces( "" ).empty(), "nothing to say" );
	std::string words;
	while ( words.size() < 300 )
		words += "word ";
	pieces = SJK_ServerPieces( words );
	size_t total = 0;
	for ( const std::string &piece : pieces ) {
		require( piece.size() <= SJK_SERVER_SAY_MAX && !piece.empty() && piece.front() != ' ' && piece.back() != ' ', "a piece fits: " + piece );
		require( piece.size() % 5 == 4, "cut at a space, never inside a word: " + piece );
		total += piece.size() + 1;
	}
	require( pieces.size() == 3 && total == words.size(), "300 characters in three pieces, nothing lost" );
	pieces = SJK_ServerPieces( std::string( 200, '%' ) );
	require( pieces.size() == 5 && pieces[0].size() == SJK_SERVER_SAY_MAX / 3, "a % takes three of what the server keeps" );
	const std::string marker = SJK_ServerMarker( "12345" );
	for ( const std::string &piece : SJK_ServerPieces( words, SJK_SERVER_SAY_MAX - marker.size() ) )
		require( piece.size() + marker.size() <= SJK_SERVER_SAY_MAX, "a piece and its marker fit: " + piece );
	pieces = SJK_ServerPieces( std::string( 148, 'x' ) + "^1red" );
	require( pieces.size() == 2 && pieces[0] == std::string( 148, 'x' ) && pieces[1] == "^1red", "never cut inside a colour code" );

	// The says of the server's copy: tagged SJK and marked when SJK chat took it, else plain.
	std::vector<std::string> says = SJK_ServerCopyLines( "hello there", "12" );
	require( says.size() == 1 && says[0] == "^3SJK^2 hello there^9^2^1^2", "tagged and marked: " + ( says.empty() ? std::string() : says[0] ) );
	require( SJK_ServerCopyLines( "hello there", "" ).empty() && SJK_ServerCopyLines( "hello there", "1234567890" ).empty(),
		"no copy without the hub's id: a line SJK chat did not take goes nowhere" );
	says = SJK_ServerCopyLines( words, "12345" );
	require( says.size() == 3, "300 characters still in three says" );
	for ( const std::string &say : says ) {
		require( say.size() <= SJK_SERVER_SAY_MAX, "a say fits what a server keeps: " + say );
		require( say.compare( 0, 8, "^3SJK^2 " ) == 0 && say.size() > 8 && say[8] != ' ', "every piece tagged: " + say );
		require( say.size() >= marker.size() && say.compare( say.size() - marker.size(), marker.size(), marker ) == 0, "every piece marked: " + say );
	}
}

int main( void ) {
	CheckSignature();
	CheckKeyFile();
	CheckBase64();
	CheckText();
	CheckHubAddress();
	CheckUserAgent();
	CheckMessageLine();
	CheckGifs();
	CheckWords();
	CheckFeedAnswer();
	CheckHistory();
	CheckServerCopy();
	printf( "sjk chat: %d checks passed\n", checks );
	return 0;
}
