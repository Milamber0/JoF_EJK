/* Native client presentation for server-authoritative dialogue graphs. */

#include "cg_local.h"
#include "cg_dialogue.h"
#include "ui/keycodes.h"
#include "ui/menudef.h"

#define CG_DLG_MAX_CHOICES 12
#define CG_DLG_VISIBLE_CHOICES 5
#define CG_DLG_SPEAKER_SIZE 64
#define CG_DLG_TEXT_SIZE 512
#define CG_DLG_CHOICE_SIZE 160
#define CG_DLG_BODY_SCALE 0.60f
#define CG_DLG_CHOICE_SCALE 0.55f
#define CG_DLG_SPEAKER_SCALE 0.60f

typedef struct {
	qboolean active;
	unsigned int serial;
	char speaker[CG_DLG_SPEAKER_SIZE];
	char text[CG_DLG_TEXT_SIZE];
	int numChoices;
	char choices[CG_DLG_MAX_CHOICES][CG_DLG_CHOICE_SIZE];
	int selected;
	qboolean awaitingResponse;
	int responseTime;
	int openTime;
	float choiceY[CG_DLG_MAX_CHOICES];
	float choiceH[CG_DLG_MAX_CHOICES];
} cgDialogueState_t;

static cgDialogueState_t s_dialogue;

static void CG_DialogueDecodeText( const char *in, char *out, size_t outSize ) {
	size_t used = 0;
	while ( *in && used + 1 < outSize ) {
		if ( in[0] == '\\' && in[1] == 'n' ) {
			out[used++] = '\n';
			in += 2;
		} else {
			out[used++] = *in++;
		}
	}
	out[used] = '\0';
}

static void CG_DialogueCloseLocal( void ) {
	memset( &s_dialogue, 0, sizeof( s_dialogue ) );
	trap->Key_SetCatcher( trap->Key_GetCatcher() & ~KEYCATCH_CGAME );
	if ( cgs.eventHandling == CGAME_EVENT_DIALOGUE ) CG_EventHandling( CGAME_EVENT_NONE );
}

void CG_DialogueReset( void ) {
	memset( &s_dialogue, 0, sizeof( s_dialogue ) );
}

qboolean CG_DialogueIsActive( void ) {
	return s_dialogue.active;
}

void CG_DialogueServerCommand( void ) {
	char action[16];
	unsigned int serial;

	// CG_Argv returns a pointer to one shared scratch buffer.  Preserve the
	// action before reading any other argument or the next call overwrites it.
	Q_strncpyz( action, CG_Argv( 1 ), sizeof( action ) );
	serial = (unsigned int)strtoul( CG_Argv( 2 ), NULL, 10 );

	if ( !Q_stricmp( action, "begin" ) ) {
		memset( &s_dialogue, 0, sizeof( s_dialogue ) );
		s_dialogue.serial = serial;
		CG_DialogueDecodeText( CG_Argv( 3 ), s_dialogue.speaker, sizeof( s_dialogue.speaker ) );
		CG_DialogueDecodeText( CG_Argv( 4 ), s_dialogue.text, sizeof( s_dialogue.text ) );
	} else if ( !Q_stricmp( action, "choice" ) ) {
		int index;
		if ( serial != s_dialogue.serial ) return;
		index = atoi( CG_Argv( 3 ) );
		if ( index < 0 || index >= CG_DLG_MAX_CHOICES ) return;
		CG_DialogueDecodeText( CG_Argv( 4 ), s_dialogue.choices[index], sizeof( s_dialogue.choices[index] ) );
		if ( index >= s_dialogue.numChoices ) s_dialogue.numChoices = index + 1;
	} else if ( !Q_stricmp( action, "show" ) ) {
		if ( serial != s_dialogue.serial || !s_dialogue.numChoices ) return;
		s_dialogue.active = qtrue;
		s_dialogue.selected = 0;
		s_dialogue.openTime = cg.time;
		cgs.cursorX = SCREEN_WIDTH * 0.5f;
		cgs.cursorY = SCREEN_HEIGHT - ( s_dialogue.numChoices * 25.0f + 22.0f ) + 34.0f;
		CG_EventHandling( CGAME_EVENT_DIALOGUE );
		trap->Key_SetCatcher( trap->Key_GetCatcher() | KEYCATCH_CGAME );
	} else if ( !Q_stricmp( action, "stop" ) ) {
		if ( serial == s_dialogue.serial ) CG_DialogueCloseLocal();
	}
}

static void CG_DialogueSubmit( int selection ) {
	if ( !s_dialogue.active ) return;
	if ( s_dialogue.awaitingResponse && cg.time - s_dialogue.responseTime < 1000 ) return;
	s_dialogue.awaitingResponse = qtrue;
	s_dialogue.responseTime = cg.time;
	trap->SendClientCommand( va( "dialogueresponse %u %d", s_dialogue.serial, selection ) );
}

void CG_DialogueCancel( void ) {
	if ( !s_dialogue.active ) return;
	trap->SendClientCommand( va( "dialogueresponse %u -1", s_dialogue.serial ) );
	CG_DialogueCloseLocal();
}

qboolean CG_DialogueKeyEvent( int key ) {
	if ( !s_dialogue.active ) return qfalse;

	if ( key == A_ESCAPE ) {
		CG_DialogueCancel();
		return qtrue;
	}
	if ( key >= A_1 && key <= A_9 ) {
		int choice = key - A_1;
		if ( choice < s_dialogue.numChoices ) CG_DialogueSubmit( choice );
		return qtrue;
	}
	if ( key == A_0 ) {
		if ( s_dialogue.numChoices > 9 ) CG_DialogueSubmit( 9 );
		return qtrue;
	}
	if ( key == A_CURSOR_UP || key == A_KP_8 || key == A_MWHEELUP ) {
		if ( s_dialogue.selected > 0 ) --s_dialogue.selected;
		return qtrue;
	}
	if ( key == A_CURSOR_DOWN || key == A_KP_2 || key == A_MWHEELDOWN ) {
		if ( s_dialogue.selected + 1 < s_dialogue.numChoices ) ++s_dialogue.selected;
		return qtrue;
	}
	if ( key == A_ENTER || key == A_KP_ENTER || key == A_SPACE ) {
		CG_DialogueSubmit( s_dialogue.selected );
		return qtrue;
	}
	if ( key == A_MOUSE1 ) {
		int i;
		for ( i = 0; i < s_dialogue.numChoices; ++i ) {
			if ( cgs.cursorX >= 24.0f && cgs.cursorX <= 616.0f &&
				 cgs.cursorY >= s_dialogue.choiceY[i] &&
				 cgs.cursorY <= s_dialogue.choiceY[i] + s_dialogue.choiceH[i] ) {
				CG_DialogueSubmit( i );
				break;
			}
		}
		return qtrue;
	}
	return qtrue;
}

void CG_DialogueMouseMove( void ) {
	int i;
	if ( !s_dialogue.active ) return;
	for ( i = 0; i < s_dialogue.numChoices; ++i ) {
		if ( cgs.cursorX >= 24.0f && cgs.cursorX <= 616.0f &&
			 cgs.cursorY >= s_dialogue.choiceY[i] &&
			 cgs.cursorY <= s_dialogue.choiceY[i] + s_dialogue.choiceH[i] ) {
			s_dialogue.selected = i;
			break;
		}
	}
}

static int CG_DialoguePaintCenteredWrapped( const char *text, float centerX, float y, float maxWidth,
	float scale, vec4_t color, int maxLines ) {
	char source[CG_DLG_TEXT_SIZE];
	char line[CG_DLG_TEXT_SIZE];
	char candidate[CG_DLG_TEXT_SIZE];
	char *p, *word;
	int lines = 0;
	float lineHeight = CG_Text_Height( "Ag", scale, FONT_MEDIUM ) + 3.0f;
	float width;

	Q_strncpyz( source, text, sizeof( source ) );
	line[0] = '\0';
	p = source;
	while ( *p && lines < maxLines ) {
		char delimiter;
		while ( *p == ' ' ) ++p;
		if ( *p == '\n' ) {
			if ( line[0] ) {
				width = CG_Text_Width( line, scale, FONT_MEDIUM );
				CG_Text_Paint( centerX - width * 0.5f, y + lines++ * lineHeight, scale, color,
					line, 0, 0, ITEM_TEXTSTYLE_SHADOWED, FONT_MEDIUM );
			}
			line[0] = '\0';
			++p;
			continue;
		}
		if ( !*p ) break;
		word = p;
		while ( *p && *p != ' ' && *p != '\n' ) ++p;
		delimiter = *p;
		if ( *p ) *p++ = '\0';
		Com_sprintf( candidate, sizeof( candidate ), "%s%s%s", line, line[0] ? " " : "", word );
		if ( line[0] && CG_Text_Width( candidate, scale, FONT_MEDIUM ) > maxWidth ) {
			width = CG_Text_Width( line, scale, FONT_MEDIUM );
			CG_Text_Paint( centerX - width * 0.5f, y + lines++ * lineHeight, scale, color,
				line, 0, 0, ITEM_TEXTSTYLE_SHADOWED, FONT_MEDIUM );
			Q_strncpyz( line, word, sizeof( line ) );
		} else {
			Q_strncpyz( line, candidate, sizeof( line ) );
		}
		if ( delimiter == '\n' && line[0] && lines < maxLines ) {
			width = CG_Text_Width( line, scale, FONT_MEDIUM );
			CG_Text_Paint( centerX - width * 0.5f, y + lines++ * lineHeight, scale, color,
				line, 0, 0, ITEM_TEXTSTYLE_SHADOWED, FONT_MEDIUM );
			line[0] = '\0';
		}
	}
	if ( line[0] && lines < maxLines ) {
		width = CG_Text_Width( line, scale, FONT_MEDIUM );
		CG_Text_Paint( centerX - width * 0.5f, y + lines++ * lineHeight, scale, color,
			line, 0, 0, ITEM_TEXTSTYLE_SHADOWED, FONT_MEDIUM );
	}
	return lines;
}

static void CG_DialogueFitText( const char *text, char *out, size_t outSize, float maxWidth, float scale ) {
	size_t length;
	Q_strncpyz( out, text, (int)outSize );
	if ( CG_Text_Width( out, scale, FONT_MEDIUM ) <= maxWidth ) return;
	length = strlen( out );
	while ( length > 4 && CG_Text_Width( out, scale, FONT_MEDIUM ) > maxWidth ) out[--length] = '\0';
	if ( length > 3 ) {
		out[length - 3] = '.'; out[length - 2] = '.'; out[length - 1] = '.';
	}
}

void CG_DialogueDraw( void ) {
	vec4_t backdrop = { 0.004f, 0.007f, 0.012f, 0.94f };
	vec4_t divider = { 0.06f, 0.30f, 0.43f, 0.88f };
	vec4_t dividerGlow = { 0.12f, 0.55f, 0.75f, 0.38f };
	vec4_t speakerColor = { 0.92f, 0.67f, 0.16f, 1.0f };
	vec4_t dialogueColor = { 0.12f, 0.68f, 0.98f, 1.0f };
	vec4_t normalColor = { 0.10f, 0.64f, 0.94f, 1.0f };
	vec4_t selectedText = { 1.0f, 0.78f, 0.12f, 1.0f };
	vec4_t selectedAccent = { 1.0f, 0.63f, 0.08f, 0.90f };
	vec4_t selectedBack = { 0.20f, 0.10f, 0.01f, 0.42f };
	char speakerLabel[CG_DLG_SPEAKER_SIZE + 8];
	float reveal;
	float topBarHeight;
	float bottomY;
	float bottomHeight;
	float bodyY;
	float bodyLineHeight;
	float speakerY;
	float speakerHeight;
	float speakerWidth;
	float railY;
	float choiceTextHeight;
	float choiceFrameHeight;
	float rowHeight;
	float y;
	int i, visibleRows, firstChoice, lastChoice;

	if ( !s_dialogue.active ) return;
	visibleRows = Q_min( s_dialogue.numChoices, CG_DLG_VISIBLE_CHOICES );
	firstChoice = 0;
	if ( s_dialogue.selected >= visibleRows ) firstChoice = s_dialogue.selected - visibleRows + 1;
	if ( firstChoice > s_dialogue.numChoices - visibleRows ) firstChoice = s_dialogue.numChoices - visibleRows;
	lastChoice = firstChoice + visibleRows;

	speakerY = 8.0f;
	speakerHeight = CG_Text_Height( "Ag", CG_DLG_SPEAKER_SCALE, FONT_SMALL );
	railY = speakerY + speakerHeight * 0.5f;
	bodyY = speakerY + speakerHeight + 8.0f;
	bodyLineHeight = CG_Text_Height( "Ag", CG_DLG_BODY_SCALE, FONT_MEDIUM ) + 3.0f;
	topBarHeight = bodyY + bodyLineHeight * 3.0f + 7.0f;
	choiceTextHeight = CG_Text_Height( "Ag", CG_DLG_CHOICE_SCALE, FONT_MEDIUM );
	choiceFrameHeight = choiceTextHeight + 8.0f;
	rowHeight = choiceFrameHeight + 3.0f;
	bottomHeight = 12.0f + visibleRows * rowHeight;
	bottomY = SCREEN_HEIGHT - bottomHeight;

	reveal = Com_Clamp( 0.0f, 1.0f, ( cg.time - s_dialogue.openTime ) / 180.0f );
	backdrop[3] *= reveal;
	divider[3] *= reveal;
	dividerGlow[3] *= reveal;
	speakerColor[3] *= reveal;
	dialogueColor[3] *= reveal;
	normalColor[3] *= reveal;
	selectedText[3] *= reveal;
	selectedAccent[3] *= reveal;
	selectedBack[3] *= reveal;

	/* Cinematic letterbox: keep the world visible between the two bands. */
	CG_FillRect( 0, 0, SCREEN_WIDTH, topBarHeight, backdrop );
	CG_FillRect( 0, bottomY, SCREEN_WIDTH, SCREEN_HEIGHT - bottomY, backdrop );

	/* Broken luminous rails and asymmetric ticks evoke the classic SW targeting HUD. */
	CG_FillRect( 0, topBarHeight - 3, 118, 2, divider );
	CG_FillRect( 126, topBarHeight - 2, 388, 1, dividerGlow );
	CG_FillRect( 522, topBarHeight - 3, 118, 2, divider );
	CG_FillRect( 0, bottomY, 82, 2, selectedAccent );
	CG_FillRect( 90, bottomY, SCREEN_WIDTH - 90, 1, divider );
	CG_FillRect( 90, bottomY + 2, SCREEN_WIDTH - 90, 1, dividerGlow );

	if ( s_dialogue.speaker[0] ) {
		Q_strncpyz( speakerLabel, s_dialogue.speaker, sizeof( speakerLabel ) );
		Q_strupr( speakerLabel );
		speakerWidth = CG_Text_Width( speakerLabel, CG_DLG_SPEAKER_SCALE, FONT_SMALL );
		if ( speakerWidth < 500.0f ) {
			CG_FillRect( 28, railY, 320.0f - speakerWidth * 0.5f - 42, 1, dividerGlow );
			CG_FillRect( 320.0f + speakerWidth * 0.5f + 14, railY,
				SCREEN_WIDTH - ( 320.0f + speakerWidth * 0.5f + 42 ), 1, dividerGlow );
		}
		CG_Text_Paint( 320.0f - speakerWidth * 0.5f, speakerY, CG_DLG_SPEAKER_SCALE, speakerColor,
			speakerLabel, 0, 0, ITEM_TEXTSTYLE_OUTLINED, FONT_SMALL );
	}
	CG_DialoguePaintCenteredWrapped( s_dialogue.text, 320.0f, bodyY, 604, CG_DLG_BODY_SCALE,
		dialogueColor, 3 );

	y = bottomY + 12.0f;
	for ( i = 0; i < s_dialogue.numChoices; ++i ) {
		s_dialogue.choiceY[i] = -1.0f;
		s_dialogue.choiceH[i] = 0.0f;
	}

	for ( i = firstChoice; i < lastChoice; ++i ) {
		char label[CG_DLG_CHOICE_SIZE + 12];
		char fitted[CG_DLG_CHOICE_SIZE + 12];
		Com_sprintf( label, sizeof( label ), "%s%d: %s",
			i == s_dialogue.selected ? "> " : "  ", i + 1, s_dialogue.choices[i] );
		CG_DialogueFitText( label, fitted, sizeof( fitted ), 566.0f, CG_DLG_CHOICE_SCALE );
		s_dialogue.choiceY[i] = y - 4.0f;
		s_dialogue.choiceH[i] = choiceFrameHeight;
		if ( i == s_dialogue.selected ) {
			CG_FillRect( 24, s_dialogue.choiceY[i], 592, s_dialogue.choiceH[i], selectedBack );
			CG_FillRect( 24, s_dialogue.choiceY[i], 3, s_dialogue.choiceH[i], selectedAccent );
			CG_FillRect( 24, s_dialogue.choiceY[i], 12, 1, selectedAccent );
			CG_FillRect( 24, s_dialogue.choiceY[i] + s_dialogue.choiceH[i] - 1, 12, 1, selectedAccent );
			CG_FillRect( 604, s_dialogue.choiceY[i], 12, 1, selectedAccent );
			CG_FillRect( 604, s_dialogue.choiceY[i] + s_dialogue.choiceH[i] - 1, 12, 1, selectedAccent );
		}
		CG_Text_Paint( 37, y - 2.0f, CG_DLG_CHOICE_SCALE,
			i == s_dialogue.selected ? selectedText : normalColor,
			fitted, 0, 0, ITEM_TEXTSTYLE_OUTLINESHADOWED, FONT_MEDIUM );
		y += rowHeight;
	}

	if ( cgs.media.selectCursor ) {
		trap->R_SetColor( NULL );
		CG_DrawPic( cgs.cursorX - 8, cgs.cursorY - 8, 16, 16, cgs.media.selectCursor );
	}
}
