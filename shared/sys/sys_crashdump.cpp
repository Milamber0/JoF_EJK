/*
===========================================================================
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

// Catches crashes (unhandled exceptions on Windows, fatal signals on
// Linux/macOS) that would otherwise just take the game down with no trace,
// and writes a crashdump-<timestamp>.log with a stack trace and the recent
// console output next to the other log files in the home path.

#include "qcommon/qcommon.h"
#include "sys_local.h"
#include "sys_public.h"
#include "con_local.h"

#include <cstdio>
#include <cstring>
#include <ctime>

#if defined(_WIN32)
	#include <windows.h>
	#include <dbghelp.h>
#else
	#include <csignal>
	#include <cstdlib>
	#include <execinfo.h>
	#include <fcntl.h>
	#include <unistd.h>
#endif

static volatile qboolean crashHandlerFired = qfalse;

static void Sys_CrashDumpPath( char *out, size_t outSize )
{
	time_t rawtime;
	char timeStr[32] = {};

	time( &rawtime );
	strftime( timeStr, sizeof( timeStr ), "%Y-%m-%d_%H-%M-%S", localtime( &rawtime ) );

	// Sys_DefaultHomePath() returns NULL in portable builds (fs_portable /
	// _PORTABLE_VERSION) - fall back to the EternalJK mod folder next to the
	// binary (<install path>/EternalJK) so we always have somewhere valid,
	// and consistent with the non-portable EternalJK home folder, to write
	// the dump.
	char *base = Sys_DefaultHomePath();
	static char installGameDir[MAX_OSPATH];
	if ( !base || !base[0] )
	{
		const char *gameDir = Cvar_VariableString( "fs_game" );
		if ( !gameDir[0] )
			gameDir = ETERNALJKGAME;

		Com_sprintf( installGameDir, sizeof( installGameDir ), "%s%c%s",
			Sys_DefaultInstallPath(), PATH_SEP, gameDir );
		base = installGameDir;
	}

	Sys_Mkdir( base );

	char crashDir[MAX_OSPATH];
	Com_sprintf( crashDir, sizeof( crashDir ), "%s%ccrashdumps", base, PATH_SEP );
	Sys_Mkdir( crashDir );

	Com_sprintf( out, (int)outSize, "%s%ccrashdump-%s.log",
		crashDir, PATH_SEP, timeStr );
}

#if defined(_WIN32)

static LONG WINAPI Sys_CrashHandler( EXCEPTION_POINTERS *info )
{
	// Don't try to handle a crash that happens while we're already
	// writing the crash dump for a previous one.
	if ( crashHandlerFired )
		return EXCEPTION_EXECUTE_HANDLER;
	crashHandlerFired = qtrue;

	char path[MAX_OSPATH];
	Sys_CrashDumpPath( path, sizeof( path ) );

	FILE *fp = fopen( path, "w" );
	if ( fp )
	{
		fprintf( fp, "JoF EternalJK crash dump\n" );
		fprintf( fp, "Built: %s %s\n", __DATE__, __TIME__ );
		fprintf( fp, "Exception code: 0x%08lX at address %p\n\n",
			info->ExceptionRecord->ExceptionCode,
			info->ExceptionRecord->ExceptionAddress );

		HANDLE process = GetCurrentProcess();
		HANDLE thread = GetCurrentThread();

		SymSetOptions( SYMOPT_LOAD_LINES | SYMOPT_UNDNAME );

		if ( SymInitialize( process, NULL, TRUE ) )
		{
			STACKFRAME64 frame = {};
			CONTEXT context = *info->ContextRecord;

#if defined(_M_X64)
			DWORD machineType = IMAGE_FILE_MACHINE_AMD64;
			frame.AddrPC.Offset = context.Rip;
			frame.AddrFrame.Offset = context.Rbp;
			frame.AddrStack.Offset = context.Rsp;
#else
			DWORD machineType = IMAGE_FILE_MACHINE_I386;
			frame.AddrPC.Offset = context.Eip;
			frame.AddrFrame.Offset = context.Ebp;
			frame.AddrStack.Offset = context.Esp;
#endif
			frame.AddrPC.Mode = AddrModeFlat;
			frame.AddrFrame.Mode = AddrModeFlat;
			frame.AddrStack.Mode = AddrModeFlat;

			fprintf( fp, "Stack trace:\n" );

			char symbolBuffer[sizeof( SYMBOL_INFO ) + MAX_SYM_NAME];
			SYMBOL_INFO *symbol = (SYMBOL_INFO *)symbolBuffer;
			symbol->SizeOfStruct = sizeof( SYMBOL_INFO );
			symbol->MaxNameLen = MAX_SYM_NAME;

			for ( int i = 0; i < 64; i++ )
			{
				if ( !StackWalk64( machineType, process, thread, &frame, &context,
						NULL, SymFunctionTableAccess64, SymGetModuleBase64, NULL ) )
				{
					break;
				}

				if ( frame.AddrPC.Offset == 0 )
					break;

				DWORD64 displacement = 0;
				if ( SymFromAddr( process, frame.AddrPC.Offset, &displacement, symbol ) )
				{
					IMAGEHLP_LINE64 line = {};
					line.SizeOfStruct = sizeof( IMAGEHLP_LINE64 );
					DWORD lineDisplacement = 0;

					if ( SymGetLineFromAddr64( process, frame.AddrPC.Offset, &lineDisplacement, &line ) )
						fprintf( fp, "  %s (%s:%lu)\n", symbol->Name, line.FileName, line.LineNumber );
					else
						fprintf( fp, "  %s + 0x%llx\n", symbol->Name, displacement );
				}
				else
				{
					fprintf( fp, "  0x%016llx\n", frame.AddrPC.Offset );
				}
			}

			SymCleanup( process );
		}
		else
		{
			fprintf( fp, "(Symbol information unavailable, stack trace omitted)\n" );
		}

		fprintf( fp, "\nRecent console output:\n" );
		ConsoleLogWriteOut( fp );

		fclose( fp );

#ifndef DEDICATED
		char message[MAX_OSPATH + 256];
		Com_sprintf( message, sizeof( message ),
			"JoF EternalJK has crashed.\n\nA crash dump was written to:\n%s\n\n"
			"Please attach this file when reporting the issue.", path );
		MessageBoxA( NULL, message, "JoF EternalJK - Crash", MB_OK | MB_ICONERROR );
#endif
	}

	return EXCEPTION_EXECUTE_HANDLER;
}

void Sys_InstallCrashHandler( void )
{
	SetUnhandledExceptionFilter( Sys_CrashHandler );
}

#else // !_WIN32

static const int crashSignals[] = { SIGSEGV, SIGABRT, SIGFPE, SIGILL, SIGBUS };

// Runs on the crashing thread inside the signal handler. Strictly this
// isn't async-signal-safe (Sys_CrashDumpPath formats a timestamp via libc,
// and Com_sprintf/backtrace_symbols_fd may allocate), but it's a best-effort
// dump for a process that's already dying, not a guarantee - and it sticks
// to open/write/backtrace_symbols_fd rather than the malloc-heavy
// ConsoleLogWriteOut path used on Windows to keep that risk as low as
// practical.
static void Sys_CrashHandler( int sig, siginfo_t *info, void *ucontext )
{
	signal( sig, SIG_DFL );

	if ( crashHandlerFired )
	{
		raise( sig );
		return;
	}
	crashHandlerFired = qtrue;

	char path[MAX_OSPATH];
	Sys_CrashDumpPath( path, sizeof( path ) );

	int fd = open( path, O_WRONLY | O_CREAT | O_TRUNC, 0644 );
	if ( fd >= 0 )
	{
		char header[512];
		int len = snprintf( header, sizeof( header ),
			"JoF EternalJK crash dump\nBuilt: %s %s\nSignal: %d (%s)\nFaulting address: %p\n\nStack trace:\n",
			__DATE__, __TIME__, sig, strsignal( sig ), info ? info->si_addr : NULL );
		if ( len > 0 )
			write( fd, header, (size_t)len );

		void *frames[64];
		int frameCount = backtrace( frames, ARRAY_LEN( frames ) );
		backtrace_symbols_fd( frames, frameCount, fd );

		close( fd );
	}

	raise( sig );
}

void Sys_InstallCrashHandler( void )
{
	struct sigaction action = {};
	action.sa_sigaction = Sys_CrashHandler;
	action.sa_flags = SA_SIGINFO;
	sigemptyset( &action.sa_mask );

	for ( size_t i = 0; i < ARRAY_LEN( crashSignals ); i++ )
	{
		sigaction( crashSignals[i], &action, NULL );
	}
}

#endif
