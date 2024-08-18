/*
===========================================================================
Copyright (C) 1999 - 2005, Id Software, Inc.
Copyright (C) 2000 - 2013, Raven Software, Inc.
Copyright (C) 2001 - 2013, Activision, Inc.
Copyright (C) 2005 - 2015, ioquake3 contributors
Copyright (C) 2013 - 2015, OpenJK contributors

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

// console.c

#include "client.h"
#include "cl_cgameapi.h"
#include "qcommon/stringed_ingame.h"
#include "qcommon/game_version.h"



console_t	con;

cvar_t		*con_conspeed;
cvar_t		*con_notifytime;
cvar_t		*con_notifylines;
cvar_t		*con_opacity; // background alpha multiplier
cvar_t		*con_scale;
cvar_t		*con_ratioFix; //for custom console backgrounds
cvar_t		*con_autoclear;
cvar_t		*con_notifywords;
cvar_t		*con_notifyconnect;
cvar_t		*con_notifyvote;

#define	DEFAULT_CONSOLE_WIDTH	78
#define TIMESTAMP_LENGTH 9

vec4_t	console_color = {0.509f, 0.609f, 0.847f, 1.0f};

/*
================
Con_ToggleConsole_f
================
*/
void Con_ToggleConsole_f (void) {
	// closing a full screen console restarts the demo loop
	if ( cls.state == CA_DISCONNECTED && Key_GetCatcher( ) == KEYCATCH_CONSOLE ) {
		CL_StartDemoLoop();
		return;
	}

	if( con_autoclear->integer )
		Field_Clear( &g_consoleField );

	Con_ClearNotify ();
	Key_SetCatcher( Key_GetCatcher( ) ^ KEYCATCH_CONSOLE );
}

/*
===================
Con_ToggleMenu_f
===================
*/
void Con_ToggleMenu_f( void ) {
	CL_KeyEvent( A_ESCAPE, qtrue, Sys_Milliseconds() );
	CL_KeyEvent( A_ESCAPE, qfalse, Sys_Milliseconds() );
}

/*
================
Con_MessageMode_f
================
*/
void Con_MessageMode_f (void) {	//yell
	chat_playerNum = -1;
	chat_team = qfalse;
	Field_Clear( &chatField );
	chatField.widthInChars = SCREEN_WIDTH / (BIGCHAR_WIDTH * cls.widthRatioCoef) - (16 * cls.widthRatioCoef);
	Key_SetCatcher( Key_GetCatcher( ) ^ KEYCATCH_MESSAGE );
}

/*
================
Con_MessageMode2_f
================
*/
void Con_MessageMode2_f (void) {	//team chat
	chat_playerNum = -1;
	chat_team = qtrue;
	Field_Clear( &chatField );
	chatField.widthInChars = SCREEN_WIDTH / (BIGCHAR_WIDTH * cls.widthRatioCoef) - (25 * cls.widthRatioCoef);
	Key_SetCatcher( Key_GetCatcher( ) ^ KEYCATCH_MESSAGE );
}

/*
================
Con_MessageMode3_f
================
*/
void Con_MessageMode3_f (void) {	//target chat
	if (!cls.cgameStarted)
	{
		assert(!"null cgvm");
		return;
	}

	if (cl.snap.ps.pm_flags & PMF_FOLLOW) { //Send to the person we are spectating instead
		chat_playerNum = cl.snap.ps.clientNum;
	}
	else {
		chat_playerNum = CGVM_CrosshairPlayer();
	}

	if ( chat_playerNum < 0 || chat_playerNum >= MAX_CLIENTS ) {
		chat_playerNum = -1;
		return;
	}
	chat_team = qfalse;
	Field_Clear( &chatField );
	chatField.widthInChars = SCREEN_WIDTH / (BIGCHAR_WIDTH * cls.widthRatioCoef) - (24 * cls.widthRatioCoef);
	Key_SetCatcher( Key_GetCatcher( ) ^ KEYCATCH_MESSAGE );
}

/*
================
Con_MessageMode4_f
================
*/
void Con_MessageMode4_f (void)
{	//attacker
	if (!cls.cgameStarted)
	{
		assert(!"null cgvm");
		return;
	}

	chat_playerNum = CGVM_LastAttacker();
	if ( chat_playerNum < 0 || chat_playerNum >= MAX_CLIENTS ) {
		chat_playerNum = -1;
		return;
	}
	chat_team = qfalse;
	Field_Clear( &chatField );
	chatField.widthInChars = SCREEN_WIDTH / (BIGCHAR_WIDTH * cls.widthRatioCoef) - (24 * cls.widthRatioCoef);
	Key_SetCatcher( Key_GetCatcher( ) ^ KEYCATCH_MESSAGE );
}

/*
================
Con_Clear_f
================
*/
void Con_Clear_f (void) {
	int		i;

	for ( i = 0 ; i < CON_TEXTSIZE ; i++ ) {
		con.text[i] = (ColorIndex(COLOR_WHITE)<<8) | ' ';
	}

	Con_Bottom();		// go to end
}

void Con_Copy(void) {
	int l, x, i;
	short *line;
	int bufferlen, savebufferlen;
	char *buffer, *savebuffer;

	// skip empty lines
	for (l = con.current - con.totallines + 1; l <= con.current; l++)
	{
		line = con.text + (l%con.totallines)*con.linewidth;
		for (x = 0; x<con.linewidth; x++)
			if ((line[x] & 0xff) != ' ')
				break;
		if (x != con.linewidth)
			break;
	}

	if (l > con.current) {
		Com_Printf(S_COLOR_YELLOW "Console is empty! Nothing copied.\n");
		return;
	}

#ifdef _WIN32
	bufferlen = con.linewidth + 3;
#else
	bufferlen = con.linewidth + 2;
#endif

	savebufferlen = bufferlen*(con.current - l);
	buffer = (char *)Hunk_AllocateTempMemory(bufferlen);
	savebuffer = (char *)Hunk_AllocateTempMemory(savebufferlen);
	memset(savebuffer, 0, savebufferlen);

	// write the remaining lines
	buffer[bufferlen - 1] = 0;
	for (; l < con.current; l++)
	{
		line = con.text + (l%con.totallines)*con.linewidth;

		buffer[0] = '[';//this was a space
		for (i = 0; i<TIMESTAMP_LENGTH; i++) //Add [ and ] brackets around timestamp.  0 and timestamp_length ?
			buffer[i+1] = (char)(line[i] & 0xff);
		buffer[TIMESTAMP_LENGTH] = ']';//this was a space
		buffer[TIMESTAMP_LENGTH+1] = ' ';//add a new space
		for (i = TIMESTAMP_LENGTH+1; i<con.linewidth; i++) //Add [ and ] brackets around timestamp.  0 and timestamp_length ?
			buffer[i+1] = (char)(line[i-1] & 0xff); //i-1 instead of i, does this fuck up the end?

		for (x = con.linewidth - 1; x >= 0; x--)
		{
			if (buffer[x] == ' ')
				buffer[x] = 0;
			else
				break;
		}
#ifdef _WIN32
		Q_strcat(buffer, bufferlen, "\r\n");
#else
		Q_strcat(buffer, bufferlen, "\n");
#endif
		Q_strcat(savebuffer, savebufferlen, buffer);
	}
	Sys_SetClipboardData(savebuffer);
	Com_Printf("^2Console successfully copied to clipboard!\n");
	Hunk_FreeTempMemory(buffer);
	Hunk_FreeTempMemory(savebuffer);
}

void Con_CopyLink(void) {
	int l, x, i, pointDiff;
	short *line;
	char *buffer, n[] = "\0";
	const char *link = NULL, *point1 = NULL, *point2 = NULL, *point3 = NULL;
	qboolean containsNum = qfalse, containsPoint = qfalse;

	buffer = (char *)Hunk_AllocateTempMemory(con.linewidth);

	for (l = con.current; l >= con.current - 32; l--)
	{
		line = con.text + (l%con.totallines)*con.linewidth;
		for (i = 0; i < con.linewidth; i++) {
			buffer[i] = (char)(line[i] & 0xff);
			if (!containsNum && Q_isanumber(&buffer[i])) containsNum = qtrue;
			if (!containsPoint && buffer[i] == '.') containsPoint = qtrue;
		}
		// Clear spaces at end of buffer
		for (x = con.linewidth - 1; x >= 0; x--) {
			if (buffer[x] == ' ')
				buffer[x] = 0;
			else
				break;
		}
		Q_StripColor(buffer);
		if ((link = Q_stristr(buffer, "://")) || (link = Q_stristr(buffer, "www."))) {
			// Move link ptr back until it hits a space or first char of string
			while (link != &buffer[0] && *(link - 1) != ' ') link--;
			for (i = 0; buffer[i] != 0; i++) {
				buffer[i] = *link++;
				if (*link == ' ' || *link == '"') buffer[i + 1] = 0;
			}
			Sys_SetClipboardData(buffer);
			Com_Printf("^2Link ^7\"%s\" ^2Copied!\n", buffer);
			break;
		}
		if (containsNum && containsPoint) {
			containsNum = qfalse, containsPoint = qfalse;
			if (!(point1 = Q_stristr(buffer, ".")) || // Set address of first point
				// Check if points exist after point1 and set their addresses
				!(point2 = Q_stristr(point1 + 1, ".")) ||
				!(point3 = Q_stristr(point2 + 1, "."))) continue;
			for(i = 0; buffer[i] != 0; i++) {
				if (point1 == &buffer[i]) { // If addresses match, set point1 to next point
					// Check if points exist and set point addresses
					if (
						!(point1 = Q_stristr(&buffer[i + 1], ".")) ||
						!(point2 = Q_stristr(point1 + 1, ".")) ||
						!(point3 = Q_stristr(point2 + 1, "."))
						) break;
				}
				*n = buffer[i]; // Force Q_isanumber to look at a single char
				if (Q_isanumber(n)) {
					// Check if chars exist between points and the amount of chars is > 0 & <=3
					// <xxx>.<xxx>.<xxx>. Can't reliably check for chars after last point
					if ((pointDiff = point1 - &buffer[i]) <= 3 &&
						pointDiff > 0 &&
						(pointDiff = point2 - (point1 + 1)) <= 3 &&
						pointDiff > 0 &&
						(pointDiff = point3 - (point2 + 1)) <= 3 &&
						pointDiff > 0
						) {
						link = &buffer[i];
						break;
					}
				}
			}
			if (link) {
				for (i = 0; buffer[i] != 0; i++) {
					buffer[i] = *link++;
					if (*link == ' ' || *link == '"') buffer[i + 1] = 0;
				}
				Sys_SetClipboardData(buffer);
				Com_Printf("^2IP ^7\"%s\" ^2Copied!\n", buffer);
				break;
			}
		}
	}
	if (!link) {
		Com_Printf("^1No Links or IPs found!\n", buffer);
	}
	Hunk_FreeTempMemory(buffer);
}

/*
================
Con_Dump_f

Save the console contents out to a file
================
*/
void Con_Dump_f (void)
{
	int		l, x, i;
	short	*line;
	fileHandle_t	f;
	int		bufferlen;
	char	*buffer;
	char	filename[MAX_QPATH];

	if (Cmd_Argc() != 2)
	{
		Com_Printf ("%s\n", SE_GetString("CON_TEXT_DUMP_USAGE"));
		return;
	}

	Q_strncpyz( filename, Cmd_Argv( 1 ), sizeof( filename ) );
	COM_DefaultExtension( filename, sizeof( filename ), ".txt" );

	if(!COM_CompareExtension(filename, ".txt"))
	{
		Com_Printf( "Con_Dump_f: Only the \".txt\" extension is supported by this command!\n" );
		return;
	}

	f = FS_FOpenFileWrite( filename );
	if (!f)
	{
		Com_Printf ("ERROR: couldn't open %s.\n", filename);
		return;
	}

	Com_Printf ("Dumped console text to %s.\n", filename );

	// skip empty lines
	for (l = con.current - con.totallines + 1 ; l <= con.current ; l++)
	{
		line = con.text + (l%con.totallines)*con.linewidth;
		for (x=0 ; x<con.linewidth ; x++)
			if ((line[x] & 0xff) != ' ')
				break;
		if (x != con.linewidth)
			break;
	}

#ifdef _WIN32
	bufferlen = con.linewidth + 3 * sizeof ( char );
#else
	bufferlen = con.linewidth + 2 * sizeof ( char );
#endif

	buffer = (char *)Hunk_AllocateTempMemory( bufferlen );

	// write the remaining lines
	buffer[bufferlen-1] = 0;
	for ( ; l <= con.current ; l++)
	{
		line = con.text + (l%con.totallines)*con.linewidth;
		for(i=0; i<con.linewidth; i++)
			buffer[i] = (char) (line[i] & 0xff);
		for (x=con.linewidth-1 ; x>=0 ; x--)
		{
			if (buffer[x] == ' ')
				buffer[x] = 0;
			else
				break;
		}
#ifdef _WIN32
		Q_strcat(buffer, bufferlen, "\r\n");
#else
		Q_strcat(buffer, bufferlen, "\n");
#endif
		FS_Write(buffer, strlen(buffer), f);
	}

	Hunk_FreeTempMemory( buffer );
	FS_FCloseFile( f );
}


/*
================
Con_ClearNotify
================
*/
void Con_ClearNotify( void ) {
	int		i;

	for ( i = 0 ; i < NUM_CON_TIMES ; i++ ) {
		con.times[i] = 0;
	}
}



/*
================
Con_CheckResize

If the line width has changed, reformat the buffer.
================
*/
void Con_CheckResize (void)
{
	int		i, j, width, oldwidth, oldtotallines, numlines, numchars;
	short	tbuf[CON_TEXTSIZE];

	if (cls.glconfig.vidWidth <=0.0f) // video hasn't been initialized yet
	{
		con.xadjust = 1;
		con.yadjust = 1;
		con.charWidth = SMALLCHAR_WIDTH;
		con.charHeight = SMALLCHAR_HEIGHT;
		con.linewidth = DEFAULT_CONSOLE_WIDTH;
		con.totallines = CON_TEXTSIZE / con.linewidth;
		for(i=0; i<CON_TEXTSIZE; i++)
		{
			con.text[i] = (ColorIndex(COLOR_WHITE)<<8) | ' ';
		}
	}
	else
	{
		float scale = (con_scale && con_scale->value > 0.0f) ? con_scale->value : 1.0f;

		width = (cls.glconfig.vidWidth / (scale * SMALLCHAR_WIDTH)) - 2;

		if (width == con.linewidth)
			return;

		con.charWidth = scale * SMALLCHAR_WIDTH;
		con.charHeight = scale * SMALLCHAR_HEIGHT;

		g_consoleField.widthInChars = width;
		for (i = 0; i < COMMAND_HISTORY; i++) {
			historyEditLines[i].widthInChars = width;

		}

		// on wide screens, we will center the text
		con.xadjust = 640.0f / cls.glconfig.vidWidth;
		con.yadjust = 480.0f / cls.glconfig.vidHeight;

		oldwidth = con.linewidth;
		con.linewidth = width;
		oldtotallines = con.totallines;
		con.totallines = CON_TEXTSIZE / con.linewidth;
		numlines = oldtotallines;

		if (con.totallines < numlines)
			numlines = con.totallines;

		numchars = oldwidth;

		if (con.linewidth < numchars)
			numchars = con.linewidth;

		Com_Memcpy (tbuf, con.text, CON_TEXTSIZE * sizeof(short));
		for(i=0; i<CON_TEXTSIZE; i++)

			con.text[i] = (ColorIndex(COLOR_WHITE)<<8) | ' ';


		for (i=0 ; i<numlines ; i++)
		{
			for (j=0 ; j<numchars ; j++)
			{
				con.text[(con.totallines - 1 - i) * con.linewidth + j] =
						tbuf[((con.current - i + oldtotallines) %
							  oldtotallines) * oldwidth + j];
			}
		}

		Con_ClearNotify ();
	}

	con.current = con.totallines - 1;
	con.display = con.current;
}


/*
==================
Cmd_CompleteTxtName
==================
*/
void Cmd_CompleteTxtName( char *args, int argNum ) {
	if ( argNum == 2 )
		Field_CompleteFilename( "", "txt", qfalse, qtrue );
}

/*
================
Con_Init
================
*/
static char version[MAX_STRING_CHARS] = { 0 };
void Con_Init (void) {
	int		i;

	con_notifytime = Cvar_Get ("con_notifytime", "3", 0, "How many seconds notify messages should be shown before they fade away");
	con_notifylines = Cvar_Get("con_notifylines", "3", CVAR_ARCHIVE_ND, "Max number of console lines to print in top left");
	con_conspeed = Cvar_Get ("scr_conspeed", "3", 0, "Console open/close speed");
	Cvar_CheckRange (con_conspeed, 1.0f, 100.0f, qfalse);

	con_scale = Cvar_Get("con_scale", "1.0", CVAR_ARCHIVE_ND, "Console character scale");
	Cvar_CheckRange(con_scale, 0.2, 10.0f, qfalse);

#ifndef TOURNAMENT_CLIENT
	con_opacity = Cvar_Get ("con_opacity", "1.0", CVAR_ARCHIVE_ND, "Opacity of console background");
#else
	con_opacity = Cvar_Get("con_opacity", "0.5", CVAR_ARCHIVE_ND, "Opacity of console background");
#endif
	con_ratioFix = Cvar_Get("con_ratioFix", "1", CVAR_ARCHIVE_ND, "Correct console background height, should probably disable for custom console backgrounds.");
	con_autoclear = Cvar_Get ("con_autoclear", "1", CVAR_ARCHIVE_ND, "Automatically clear console input on close");
	con_notifywords = Cvar_Get("con_notifywords", "0", CVAR_ARCHIVE, "Notifies you when defined words are mentioned");
	con_notifyconnect = Cvar_Get("con_notifyconnect", "0", CVAR_ARCHIVE, "Notifies you when someone connects to the server");
	con_notifyvote = Cvar_Get("con_notifyvote", "1", CVAR_ARCHIVE, "Notifies you when someone calls a vote");

	Field_Clear( &g_consoleField );
	g_consoleField.widthInChars = DEFAULT_CONSOLE_WIDTH;
	for ( i = 0 ; i < COMMAND_HISTORY ; i++ ) {
		Field_Clear( &historyEditLines[i] );
		historyEditLines[i].widthInChars = DEFAULT_CONSOLE_WIDTH;
	}

	Cmd_AddCommand( "toggleconsole", Con_ToggleConsole_f, "Show/hide console" );
	Cmd_AddCommand( "togglemenu", Con_ToggleMenu_f, "Show/hide the menu" );
	Cmd_AddCommand( "messagemode", Con_MessageMode_f, "Global Chat" );
	Cmd_AddCommand( "messagemode2", Con_MessageMode2_f, "Team Chat" );
	Cmd_AddCommand( "messagemode3", Con_MessageMode3_f, "Private Chat with Target Player" );
	Cmd_AddCommand( "messagemode4", Con_MessageMode4_f, "Private Chat with Last Attacker" );
	Cmd_AddCommand( "clear", Con_Clear_f, "Clear console text" );
	Cmd_AddCommand( "condump", Con_Dump_f, "Dump console text to file" );
	Cmd_SetCommandCompletionFunc( "condump", Cmd_CompleteTxtName );

#ifndef _DEBUG
	{//build version string for console
		int day, year;
		char month[4];

		if (sscanf(SOURCE_DATE, "%s %i %i", &month, &day, &year) == 3) {
			int mm = 0;

			//sry..
			if (month[0] == 'J' && month[1] == 'a' && month[2] == 'n')
				mm = 1;
			else if (month[0] == 'F')
				mm = 2;
			else if (month[0] == 'M' && month[1] == 'a' && month[2] == 'r')
				mm = 3;
			else if (month[0] == 'A' && month[1] == 'p')
				mm = 4;
			else if (month[0] == 'M' && month[1] == 'a' && month[2] == 'y')
				mm = 5;
			else if (month[0] == 'J' && month[1] == 'u' && month[2] == 'n')
				mm = 6;
			else if (month[0] == 'J' && month[1] == 'u' && month[2] == 'l')
				mm = 7;
			else if (month[0] == 'A' && month[1] == 'u')
				mm = 8;
			else if (month[0] == 'S')
				mm = 9;
			else if (month[0] == 'O')
				mm = 10;
			else if (month[0] == 'N')
				mm = 11;
			else if (month[0] == 'D')
				mm = 12;

			Com_sprintf(version, sizeof(version), CLIENT_WINDOW_TITLE ": [%02i/%02i/%04i]", day, mm, year);
		}
	}
#else
	Com_sprintf(version, sizeof(version), CLIENT_WINDOW_TITLE "(DEBUG)");
#endif

	if (!version[0])
		Q_strncpyz(version, "EternalJK", sizeof(version));
}

/*
================
Con_Shutdown
================
*/
void Con_Shutdown(void)
{
	Cmd_RemoveCommand("toggleconsole");
	Cmd_RemoveCommand("togglemenu");
	Cmd_RemoveCommand("messagemode");
	Cmd_RemoveCommand("messagemode2");
	Cmd_RemoveCommand("messagemode3");
	Cmd_RemoveCommand("messagemode4");
	Cmd_RemoveCommand("clear");
	Cmd_RemoveCommand("condump");
}

/*
===============
Con_Linefeed
===============
*/
int stampColor = COLOR_GREY;
static void Con_Linefeed (qboolean skipnotify)
{
	int		i;
	char	timetxt[TIMESTAMP_LENGTH];
	qtime_t now;

	// mark time for transparent overlay
	if (con.current >= 0)
	{
		if (skipnotify)
			  con.times[con.current % NUM_CON_TIMES] = 0;
		else
			  con.times[con.current % NUM_CON_TIMES] = cls.realtime;
	}

	Com_RealTime(&now);
	Com_sprintf(timetxt, sizeof(timetxt), "%02d:%02d:%02d", now.tm_hour, now.tm_min, now.tm_sec);
	for (i = 0; i<TIMESTAMP_LENGTH-1; i++)
		con.text[(con.current%con.totallines)*con.linewidth + i] = (ColorIndex(stampColor) << 8) | timetxt[i];

	con.x = TIMESTAMP_LENGTH;
	if (con.display == con.current)
		con.display++;
	con.current++;

	for(i=0; i<con.linewidth; i++)
		con.text[(con.current%con.totallines)*con.linewidth+i] = (ColorIndex(COLOR_WHITE)<<8) | ' '; //Spacing between timestamp and text, and other spaces
}

/*
================
CL_ConsolePrint

Handles cursor positioning, line wrapping, etc
All console printing must go through this in order to be logged to disk
If no console is visible, the text will appear at the top of the game window
================
*/
void CL_ConsolePrint( const char *txt ) {
	int		y;
	int		c, l;
	int		color;
	qboolean skipnotify = qfalse;		// NERVE - SMF
	int prev;							// NERVE - SMF

	// for some demos we don't want to ever show anything on the console
	if (cl_noprint && cl_noprint->integer) {
		return;
	}

	// TTimo - prefix for text that shows up in console but not in notify
	// backported from RTCW
	if ( !Q_strncmp( txt, "[skipnotify]", 12 ) ) {
		skipnotify = qtrue;
		txt += 12;
	}
	if (txt[0] == '*') {
		skipnotify = qtrue;
		txt += 1;
	}


	if (!con.initialized) {
		con.color[0] =
		con.color[1] =
		con.color[2] =
		con.color[3] = 1.0f;
		con.linewidth = -1;
		//con.linecount = 0;
		con.x = TIMESTAMP_LENGTH;
		Con_CheckResize ();
		con.initialized = qtrue;
	}

	color = ColorIndex(COLOR_WHITE);
	l = -1;

	while ( (c = (unsigned char) *txt) != 0 ) {
		if ( Q_IsColorString( (unsigned char*) txt ) ) {
			color = ColorIndex( *(txt+1) );
			txt += 2;
			continue;
		}

		// count word length
		if ((l < 0 ? l = 0, true : false) || (l + 1 == con.linewidth - TIMESTAMP_LENGTH))
			while (l < con.linewidth - TIMESTAMP_LENGTH) {
				if ( txt[l] <= ' ') {
					break;
				}
				l++;
			}

		// word wrap
		if (l != con.linewidth - TIMESTAMP_LENGTH && con.x + l >= con.linewidth) {
			Con_Linefeed(skipnotify);
		}

		txt++;
		l--;

		switch (c)
		{
		case '\n':
			Con_Linefeed (skipnotify);
			//if (con.linecount < con.totallines)
			con.linecount++;
			break;
		case '\r':
			con.x = TIMESTAMP_LENGTH;
			break;
		default:	// display character and advance
			y = con.current % con.totallines;
			con.text[y*con.linewidth+con.x] = (short) ((color << 8) | c);
			con.x++;
			if (con.x >= con.linewidth) {
				Con_Linefeed(skipnotify);
			}
			break;
		}
	}

	// mark time for transparent overlay
	if (con.current >= 0 )
	{
		// NERVE - SMF
		if ( skipnotify ) {
			prev = con.current % NUM_CON_TIMES - 1;
			if ( prev < 0 )
				prev = NUM_CON_TIMES - 1;
			con.times[prev] = 0;
		}
		else
		// -NERVE - SMF
			con.times[con.current % NUM_CON_TIMES] = cls.realtime;
	}

	stampColor = COLOR_GREY;
}


/*
==============================================================================

DRAWING

==============================================================================
*/


/*
================
Con_DrawInput

Draw the editline after a ] prompt
================
*/
void Con_DrawInput (void) {
	int		y, x = 0;
	char ts[TIMESTAMP_LENGTH];
	qtime_t	now;

	if ( cls.state != CA_DISCONNECTED && !(Key_GetCatcher( ) & KEYCATCH_CONSOLE ) ) {
		return;
	}

	y = con.vislines - ( con.charHeight * (re->Language_IsAsian() ? 1.5 : 2) );

	Com_RealTime(&now);
	Com_sprintf(ts, sizeof(ts), "%02d:%02d:%02d", now.tm_hour, now.tm_min, now.tm_sec);

	re->SetColor(g_color_table[ColorIndex(COLOR_GREEN)]);
	for (x = 0; x<TIMESTAMP_LENGTH-1; x++) {
		SCR_DrawSmallChar(con.xadjust + (x + 1) * con.charWidth, y, ts[x]);
	}
	x = TIMESTAMP_LENGTH;

	re->SetColor( con.color );

	SCR_DrawSmallChar( (int)(con.xadjust + (x+1) * con.charWidth), y, CONSOLE_PROMPT_CHAR ); //Add space?

	Field_Draw( &g_consoleField, (int)(con.xadjust + (x+2) * con.charWidth), y,
				SCREEN_WIDTH - 3 * con.charWidth, qtrue, qtrue );
}

static void CL_ClientCleanName(const char* in, char* out, int outSize)
{
	int outpos = 0, colorlessLen = 0;

	// discard leading spaces
	for (; *in == ' '; in++);

	// discard leading asterisk's (fail raven for using * as a skipnotify)
	// apparently .* causes the issue too so... derp

	for (; *in && outpos < outSize - 1; in++)
	{
		out[outpos] = *in;

		if (*in == '^' && *(in + 1) >= '0' && *(in + 1) <= '9') {
			in++; // Skip the digit after ^
			continue; // Skip this iteration, moving to the next character
		}

		if (*(in + 1) && *(in + 1) != '\0' && *(in + 2) && *(in + 2) != '\0')
		{
			if (*in == ' ' && *(in + 1) == ' ' && *(in + 2) == ' ') // don't allow more than 3 consecutive spaces
				continue;

			if (*in == '@' && *(in + 1) == '@' && *(in + 2) == '@') // don't allow too many consecutive @ signs
				continue;
		}

		if ((byte)*in < 0x20)
			continue;

		switch ((byte)*in)
		{
		default:
			break;
		case 0x81:
		case 0x8D:
		case 0x8F:
		case 0x90:
		case 0x9D:
		case 0xA0:
		case 0xAD:
			continue;
			break;
		}

		if (outpos > 0 && out[outpos - 1] == Q_COLOR_ESCAPE)
		{
			if (Q_IsColorStringExt(&out[outpos - 1]))
			{
				colorlessLen--;
			}
			else
			{
				//spaces = ats = 0;
				colorlessLen++;
			}
		}
		else
		{
			//spaces = ats = 0;
			colorlessLen++;
		}
		outpos++;
	}

	out[outpos] = '\0';

	// don't allow empty names
	if (*out == '\0' || colorlessLen == 0)
		Q_strncpyz(out, "Padawan", outSize);
}

float chatColour[4] = { 1.0f, 1.0f, 1.0f, 1.0f }; // For DrawStringExt2
/*
================
Con_DrawNotify

Draws the last few lines of output transparently over the game top
================
*/
void Con_DrawNotify (void)
{
	int		x, v;
	short	*text;
	int		i;
	int		time;
	int		skip;
	int		currentColor;
	const char* chattext;

	currentColor = 7;
	re->SetColor( g_color_table[currentColor] );

	v = 0;
	for (i= con.current-con_notifylines->integer ; i<=con.current ; i++)
	{
		if (i < 0)
			continue;
		time = con.times[i % NUM_CON_TIMES];
		if (time == 0)
			continue;
		time = cls.realtime - time;
		if (time > con_notifytime->value*1000)
			continue;
		text = con.text + (i % con.totallines)*con.linewidth;

		if (cl.snap.ps.pm_type != PM_INTERMISSION && Key_GetCatcher( ) & (KEYCATCH_UI | KEYCATCH_CGAME) ) {
			continue;
		}

		if (!cl_conXOffset)
		{
			cl_conXOffset = Cvar_Get ("cl_conXOffset", "0", 0);
		}

		// asian language needs to use the new font system to print glyphs...
		//
		// (ignore colours since we're going to print the whole thing as one string)
		//
		if (re->Language_IsAsian())
		{
			static int iFontIndex = re->RegisterFont("ocr_a");	// this seems naughty
			const float fFontScale = 0.75f*con.yadjust;
			const int iPixelHeightToAdvance =   2+(1.3/con.yadjust) * re->Font_HeightPixels(iFontIndex, fFontScale);	// for asian spacing, since we don't want glyphs to touch.

			// concat the text to be printed...
			//
			char sTemp[4096]={0};	// ott
			for (x = TIMESTAMP_LENGTH; x < con.linewidth ; x++)
			{
				if ( ( (text[x]>>8)&Q_COLOR_BITS ) != currentColor ) {
					currentColor = (text[x]>>8)&Q_COLOR_BITS;
					strcat(sTemp,va("^%i", (text[x]>>8)&Q_COLOR_BITS) );
				}
				strcat(sTemp,va("%c",text[x] & 0xFF));
			}
			//
			// and print...
			//
			re->Font_DrawString(cl_conXOffset->integer + con.xadjust*(con.xadjust + (1*con.charWidth/*aesthetics*/)), con.yadjust*(v), sTemp, g_color_table[currentColor], iFontIndex, -1, fFontScale);

			v +=  iPixelHeightToAdvance;
		}
		else
		{
			for (x = TIMESTAMP_LENGTH; x < con.linewidth ; x++) {
				if ( ( text[x] & 0xff ) == ' ' ) {
					continue;
				}
				if ( ( (text[x]>>8)&Q_COLOR_BITS ) != currentColor ) {
					currentColor = (text[x]>>8)&Q_COLOR_BITS;
					re->SetColor( g_color_table[currentColor] );
				}
				if (!cl_conXOffset)
				{
					cl_conXOffset = Cvar_Get ("cl_conXOffset", "0", 0);
				}
				SCR_DrawSmallChar((int)(cl_conXOffset->integer + con.xadjust + (x + 1 - TIMESTAMP_LENGTH)*con.charWidth), v, text[x] & 0xff);
			}

			v += con.charHeight;
		}
	}

	re->SetColor( NULL );

	if (Key_GetCatcher( ) & (KEYCATCH_UI | KEYCATCH_CGAME) ) {
		return;
	}

	// draw the chat line
	char base[100] = "Whisper to ";
	char player[100];
	

	if ( Key_GetCatcher( ) & KEYCATCH_MESSAGE )
	{
		if (chat_playerNum != -1) {
			char* s = cl.gameState.stringData + cl.gameState.stringOffsets[CS_PLAYERS+chat_playerNum];		
			char* player = Info_ValueForKey(s, "n");//this contains name
			char sanitized[MAX_NAME_LENGTH];
			char without_color[100];
			CL_ClientCleanName(player, sanitized, MAX_NAME_LENGTH);	//sanitize the name to avoid crashing, saves in sanitized

			strcat(base, sanitized);
			strcat(base, ":");
			chattext = base;

		}
		else if (chat_team)	{
			chattext = SE_GetString("MP_SVGAME", "SAY_TEAM");
		}
		else {
			chattext = SE_GetString("MP_SVGAME", "SAY");
		}

		SCR_DrawStringExt2(8 * cls.widthRatioCoef, v, BIGCHAR_WIDTH*cls.widthRatioCoef, BIGCHAR_HEIGHT, chattext, chatColour, qfalse, qfalse);
		skip = strlen(chattext) + 1;
		Field_BigDraw( &chatField, skip * BIGCHAR_WIDTH, v,
			SCREEN_WIDTH - ( skip + 1 ) * BIGCHAR_WIDTH, qtrue, qtrue );

		v += BIGCHAR_HEIGHT;
	}

}

/*
================
Con_DrawSolidConsole

Draws the console with the solid background
================
*/
void Con_DrawSolidConsole( float frac ) {
	int				i, x, y;
	int				rows;
	short			*text;
	int				row;
	int				lines;
//	qhandle_t		conShader;
	int				currentColor;
	struct tm		*newtime;
	time_t			rawtime;
	qboolean		AM = qtrue;
	char			ts[24];
	const int		padding = (int) (0.5f + (con_scale && con_scale->value > 0.0f) ? 2*con_scale->value : 2.0f);

	lines = (int) (cls.glconfig.vidHeight * frac);
	if (lines <= 0)
		return;

	if (lines > cls.glconfig.vidHeight )
		lines = cls.glconfig.vidHeight;

	// draw the background
	y = (int) (frac * SCREEN_HEIGHT - 2);
	if ( y < 1 ) {
		y = 0;
	}
	else {
		// draw the background at full opacity only if fullscreen
		if (frac < 1.0f)
		{
			vec4_t con_color;
			MAKERGBA(con_color, 1.0f, 1.0f, 1.0f, Com_Clamp(0.0f, 1.0f, con_opacity->value));
			re->SetColor(con_color);
		}
		else
		{
			re->SetColor(NULL);
		}

		//re->DrawStretchPic(0, 0, SCREEN_WIDTH, (float)y, 0, 0 + (cls.widthRatioCoef / 4), 1, 1 - (cls.widthRatioCoef / 4), cls.consoleShader);
		if (con_ratioFix->integer && frac <= 0.5f && cls.widthRatioCoef < 1.0f) // && cls.widthRatioCoef < 1.0f)
			re->DrawStretchPic(0, 0, SCREEN_WIDTH, (float)y, 0, 1 - cls.widthRatioCoef, 1, 0 + cls.widthRatioCoef, cls.consoleShader);
		else
			re->DrawStretchPic(0, 0, SCREEN_WIDTH, (float)y, 0, 0, 1, 1, cls.consoleShader);
	}

	// draw the bottom bar and version number

	re->SetColor( console_color );
	re->DrawStretchPic( 0, y, SCREEN_WIDTH, 2, 0, 0, 0, 0, cls.whiteShader );

#if 0
	i = strlen( JK_VERSION );

	for (x=0 ; x<i ; x++) {
		SCR_DrawSmallChar( cls.glconfig.vidWidth - ( i - x + 1 ) * con.charWidth,
			(lines-(con.charHeight*2+con.charHeight/2)) + padding, JK_VERSION[x] );
	}
#else
	i = strlen(version);

	for (x = 0; x < i; x++) {
		SCR_DrawSmallChar(cls.glconfig.vidWidth - (i - x + 1) * con.charWidth,
			(lines - (con.charHeight * 2 + con.charHeight / 2)) + padding, version[x]);
	}
#endif

	// Draw time and date
	time(&rawtime);
	newtime = localtime(&rawtime);
	if (newtime->tm_hour >= 12) AM = qfalse;
	if (newtime->tm_hour > 12) newtime->tm_hour -= 12;
	if (newtime->tm_hour == 0) newtime->tm_hour = 12;
	Com_sprintf(ts, sizeof(ts), "%.19s %s ", asctime(newtime), AM ? "AM" : "PM" );
	i = strlen(ts);

	for (x = 0; x<i; x++) {
		SCR_DrawSmallChar(cls.glconfig.vidWidth - (i - x) * con.charWidth, lines - (con.charHeight + con.charHeight / 2) + padding, ts[x]);
	}


	// draw the text
	con.vislines = lines;
	rows = (lines-con.charWidth)/con.charWidth;		// rows of text to draw

	y = lines - (con.charHeight*3);

	// draw from the bottom up
	if (con.display != con.current)
	{
	// draw arrows to show the buffer is backscrolled
		re->SetColor( console_color );
		for (x=0 ; x<con.linewidth ; x+=4)
			SCR_DrawSmallChar( (int) (con.xadjust + (x+1)*con.charWidth), y, '^' );
		y -= con.charHeight;
		rows--;
	}

	row = con.display;

	if ( con.x == TIMESTAMP_LENGTH) {
		row--;
	}

	currentColor = 7;
	re->SetColor( g_color_table[currentColor] );

	static int iFontIndexForAsian = 0;
	const float fFontScaleForAsian = 0.75f*con.yadjust;
	int iPixelHeightToAdvance = con.charHeight;
	if (re->Language_IsAsian())
	{
		if (!iFontIndexForAsian)
		{
			iFontIndexForAsian = re->RegisterFont("ocr_a");
		}
		iPixelHeightToAdvance = (1.3/con.yadjust) * re->Font_HeightPixels(iFontIndexForAsian, fFontScaleForAsian);	// for asian spacing, since we don't want glyphs to touch.
	}

	for (i=0 ; i<rows ; i++, y -= iPixelHeightToAdvance, row--)
	{
		if (row < 0)
			break;
		if (con.current - row >= con.totallines) {
			// past scrollback wrap point
			continue;
		}

		text = con.text + (row % con.totallines)*con.linewidth;

		// asian language needs to use the new font system to print glyphs...
		//
		// (ignore colours since we're going to print the whole thing as one string)
		//
		if (re->Language_IsAsian())
		{
			// concat the text to be printed...
			//
			char sTemp[4096]={0};	// ott
			for (x = 0 ; x < con.linewidth ; x++)
			{
				if ( ( (text[x]>>8)&Q_COLOR_BITS ) != currentColor ) {
					currentColor = (text[x]>>8)&Q_COLOR_BITS;
					strcat(sTemp,va("^%i", (text[x]>>8)&Q_COLOR_BITS) );
				}
				strcat(sTemp,va("%c",text[x] & 0xFF));
			}
			//
			// and print...
			//
			re->Font_DrawString(con.xadjust*(con.xadjust + (1*con.charWidth/*(aesthetics)*/)), con.yadjust*(y), sTemp, g_color_table[currentColor], iFontIndexForAsian, -1, fFontScaleForAsian);
		}
		else
		{
			for (x=0 ; x<con.linewidth ; x++) {
				if ( ( text[x] & 0xff ) == ' ' ) {
					continue;
				}

				if ( ( (text[x]>>8)&Q_COLOR_BITS ) != currentColor ) {
					currentColor = (text[x]>>8)&Q_COLOR_BITS;
					re->SetColor( g_color_table[currentColor] );
				}
				SCR_DrawSmallChar(  (int) (con.xadjust + (x+1)*con.charWidth), y, text[x] & 0xff );
			}
		}
	}

	// draw the input prompt, user text, and cursor if desired
	Con_DrawInput ();

	re->SetColor( NULL );
}



/*
==================
Con_DrawConsole
==================
*/
void Con_DrawConsole( void ) {
	// check for console width changes from a vid mode change
	Con_CheckResize ();

	// if disconnected, render console full screen
	if ( cls.state == CA_DISCONNECTED ) {
		if ( !( Key_GetCatcher( ) & (KEYCATCH_UI | KEYCATCH_CGAME)) ) {
			Con_DrawSolidConsole( 1.0 );
			return;
		}
	}

	if ( con.displayFrac ) {
		Con_DrawSolidConsole( con.displayFrac );
	} else {
		// draw notify lines
		if ( cls.state == CA_ACTIVE ) {
			Con_DrawNotify ();
		}
	}
}

//================================================================

/*
==================
Con_RunConsole

Scroll it up or down
==================
*/
void Con_RunConsole (void) {
	// decide on the destination height of the console
	if ( Key_GetCatcher( ) & KEYCATCH_CONSOLE )
		con.finalFrac = con.tempFrac;		// visibility
	else
		con.finalFrac = 0;				// none visible

	// scroll towards the destination height
	if (con.finalFrac < con.displayFrac)
	{
		con.displayFrac -= con_conspeed->value*(float)(cls.realFrametime*0.001);
		if (con.finalFrac > con.displayFrac)
			con.displayFrac = con.finalFrac;

	}
	else if (con.finalFrac > con.displayFrac)
	{
		con.displayFrac += con_conspeed->value*(float)(cls.realFrametime*0.001);
		if (con.finalFrac < con.displayFrac)
			con.displayFrac = con.finalFrac;
	}

}

void Con_SetFrac(const float conFrac) {
	con.tempFrac = conFrac;
}

void Con_PageUp( void ) {
	con.display -= 2;
	if ( con.current - con.display >= con.totallines ) {
		con.display = con.current - con.totallines + 1;
	} //fixme
	/*if ( con.current - con.display >= con.linecount ) {
		con.display = con.current - con.linecount + 2;
	}*/
}

void Con_PageDown( void ) {
	con.display += 2;
	if (con.display > con.current) {
		con.display = con.current;
	}
}

void Con_Top( void ) {
	con.display = con.totallines;
	if ( con.current - con.display >= con.totallines ) {
		con.display = con.current - con.totallines + 1;
	}
}

void Con_Bottom( void ) {
	con.display = con.current;
}


void Con_Close( void ) {
	if ( !com_cl_running->integer ) {
		return;
	}
	Field_Clear( &g_consoleField );
	Con_ClearNotify ();
	Key_SetCatcher( Key_GetCatcher( ) & ~KEYCATCH_CONSOLE );
	con.finalFrac = 0;				// none visible
	con.displayFrac = 0;
}
