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
// console output next to the other log files in the home path. On Windows it
// also writes a crashdump-<timestamp>.dmp minidump with the same name.

#include "qcommon/qcommon.h"
#include "sys_local.h"
#include "sys_public.h"
#include "con_local.h"

#include <atomic>
#include <cstdio>
#include <cstring>
#include <csignal>
#include <ctime>

#if defined(_WIN32)
	#include <windows.h>
	#include <tlhelp32.h>
	#include <dbghelp.h>
#else
	#include <cstdlib>
	#include <execinfo.h>
	#include <fcntl.h>
	#include <unistd.h>
	#if defined(__APPLE__)
		#include <mach-o/dyld.h>
	#endif
#endif

// Guards against two threads crashing at once, not just one thread
// recursing into its own handler: a plain flag lets both threads read 0
// before either writes 1, so both would proceed into dbghelp (which isn't
// thread-safe) or race on the same fd/FILE*. A lock-free atomic CAS is the
// cheapest way to guarantee only one thread ever wins, and lock-free atomic
// ops on a type this size are async-signal-safe, so it's fine to touch from
// a POSIX signal handler as well as a Windows SEH filter.
static std::atomic<int> crashHandlerFired{ 0 };

static bool Sys_ClaimCrashHandler( void )
{
	int expected = 0;
	return crashHandlerFired.compare_exchange_strong( expected, 1 );
}

// Fills out with "<crash dir>/crashdump-<timestamp>" - no extension, so the
// .dmp and .log from one crash share a name.
static void Sys_CrashDumpBasePath( char *out, size_t outSize )
{
	time_t rawtime;
	char timeStr[32] = {};

	time( &rawtime );
	strftime( timeStr, sizeof( timeStr ), "%Y-%m-%d_%H-%M-%S", localtime( &rawtime ) );

	// Sys_DefaultHomePath() returns NULL in portable builds (fs_portable /
	// _PORTABLE_VERSION) - fall back to the EternalJK mod folder next to the
	// binary (<install path>/EternalJK) so we always have somewhere valid,
	// and consistent with the non-portable EternalJK home folder, to write
	// the dump. Always EternalJK, not fs_game: crash dumps aren't
	// mod-specific, and fs_game is a server-controlled value we'd rather
	// not fold into a filesystem path we're about to write to.
	char *base = Sys_DefaultHomePath();
	static char installGameDir[MAX_OSPATH];
	if ( !base || !base[0] )
	{
		Com_sprintf( installGameDir, sizeof( installGameDir ), "%s%c%s",
			Sys_DefaultInstallPath(), PATH_SEP, ETERNALJKGAME );
		base = installGameDir;
	}

	Sys_Mkdir( base );

	char crashDir[MAX_OSPATH];
	Com_sprintf( crashDir, sizeof( crashDir ), "%s%ccrashdumps", base, PATH_SEP );
	Sys_Mkdir( crashDir );

	Com_sprintf( out, (int)outSize, "%s%ccrashdump-%s",
		crashDir, PATH_SEP, timeStr );
}

#if defined(_WIN32)

// Resolved dynamically (see Sys_LoadDbgHelp) rather than linked at build
// time, so dbghelp isn't a startup dependency and can't be shadowed by a
// same-named DLL sitting in the game directory.
struct DbgHelpApi
{
	decltype( &SymInitialize ) SymInitialize;
	decltype( &SymCleanup ) SymCleanup;
	decltype( &SymSetOptions ) SymSetOptions;
	decltype( &StackWalk64 ) StackWalk64;
	decltype( &SymFunctionTableAccess64 ) SymFunctionTableAccess64;
	decltype( &SymGetModuleBase64 ) SymGetModuleBase64;
	decltype( &SymFromAddr ) SymFromAddr;
	decltype( &SymGetLineFromAddr64 ) SymGetLineFromAddr64;
	decltype( &SymGetModuleInfo64 ) SymGetModuleInfo64;
	decltype( &MiniDumpWriteDump ) MiniDumpWriteDump;
};

static bool Sys_LoadDbgHelp( DbgHelpApi *api )
{
	// LOAD_LIBRARY_SEARCH_SYSTEM32 pins this to the trusted system copy -
	// it won't resolve to a dbghelp.dll planted next to the game binary.
	HMODULE module = LoadLibraryExW( L"dbghelp.dll", NULL, LOAD_LIBRARY_SEARCH_SYSTEM32 );
	if ( !module )
		return false;

	api->SymInitialize = (decltype( api->SymInitialize ))GetProcAddress( module, "SymInitialize" );
	api->SymCleanup = (decltype( api->SymCleanup ))GetProcAddress( module, "SymCleanup" );
	api->SymSetOptions = (decltype( api->SymSetOptions ))GetProcAddress( module, "SymSetOptions" );
	api->StackWalk64 = (decltype( api->StackWalk64 ))GetProcAddress( module, "StackWalk64" );
	api->SymFunctionTableAccess64 = (decltype( api->SymFunctionTableAccess64 ))GetProcAddress( module, "SymFunctionTableAccess64" );
	api->SymGetModuleBase64 = (decltype( api->SymGetModuleBase64 ))GetProcAddress( module, "SymGetModuleBase64" );
	api->SymFromAddr = (decltype( api->SymFromAddr ))GetProcAddress( module, "SymFromAddr" );
	api->SymGetLineFromAddr64 = (decltype( api->SymGetLineFromAddr64 ))GetProcAddress( module, "SymGetLineFromAddr64" );
	api->SymGetModuleInfo64 = (decltype( api->SymGetModuleInfo64 ))GetProcAddress( module, "SymGetModuleInfo64" );
	api->MiniDumpWriteDump = (decltype( api->MiniDumpWriteDump ))GetProcAddress( module, "MiniDumpWriteDump" );

	return api->SymInitialize && api->SymCleanup && api->SymSetOptions && api->StackWalk64 &&
		api->SymFunctionTableAccess64 && api->SymGetModuleBase64 && api->SymFromAddr &&
		api->SymGetLineFromAddr64 && api->SymGetModuleInfo64 && api->MiniDumpWriteDump;
}

// Writes a minidump (.dmp) next to the text log - registers, stack memory,
// thread and module info, everything WinDbg/Visual Studio need to open this
// like a live debugging session frozen at the moment of the crash, rather
// than the hand-rolled stack trace we write to the text log (which only
// ever gets you function names/offsets, never variable values).
// MiniDumpWithIndirectlyReferencedMemory pulls in whatever the stack and
// registers point to (so locals/arguments are inspectable) without going as
// far as a full process memory dump. MiniDumpWithUnloadedModules keeps DLLs
// that were loaded and then unloaded before the crash (e.g. a renderer after
// vid_restart) in the module list, so those frames can still be resolved.
// Deliberately NOT MiniDumpWithDataSegs - that pulls in the full .data
// section of every one of the 30-odd loaded modules (game + every system
// DLL), which measured ~60MB on a real crash here, several times over what's
// practical to attach to a bug report; the crashing module's globals are
// rarely what's needed to read a stack trace.
static bool Sys_WriteMiniDump( DbgHelpApi *dbghelp, EXCEPTION_POINTERS *info, const char *dumpPath )
{
	HANDLE file = CreateFileA( dumpPath, GENERIC_WRITE, 0, NULL, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL );
	if ( file == INVALID_HANDLE_VALUE )
		return false;

	MINIDUMP_EXCEPTION_INFORMATION exceptionInfo = {};
	exceptionInfo.ThreadId = GetCurrentThreadId();
	exceptionInfo.ExceptionPointers = info;
	exceptionInfo.ClientPointers = FALSE;

	const MINIDUMP_TYPE dumpType = (MINIDUMP_TYPE)(
		MiniDumpNormal |
		MiniDumpWithIndirectlyReferencedMemory |
		MiniDumpWithThreadInfo |
		MiniDumpWithUnloadedModules );

	BOOL ok = dbghelp->MiniDumpWriteDump( GetCurrentProcess(), GetCurrentProcessId(),
		file, dumpType, &exceptionInfo, NULL, NULL );

	CloseHandle( file );
	return ok != FALSE;
}

// Logs each loaded module's name and base address, so frames that can only
// be resolved to "module+offset" (no matching PDB) can still be rebased
// against the right build's binaries afterwards.
static void Sys_CrashDumpModules( FILE *fp )
{
	fprintf( fp, "Modules:\n" );

	HANDLE snapshot = CreateToolhelp32Snapshot( TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, GetCurrentProcessId() );
	if ( snapshot == INVALID_HANDLE_VALUE )
		return;

	MODULEENTRY32 module = {};
	module.dwSize = sizeof( module );

	if ( Module32First( snapshot, &module ) )
	{
		do
		{
			fprintf( fp, "  %p %s\n", module.modBaseAddr, module.szModule );
		} while ( Module32Next( snapshot, &module ) );
	}

	CloseHandle( snapshot );
}

// Writes a text stack trace of the crashing thread. Frames resolve to
// function (file:line) when a matching .pdb is available, and otherwise to
// module+offset (e.g. eternaljk.x86_64.exe+0x3f21a8).
static void Sys_CrashDumpStackTrace( FILE *fp, DbgHelpApi *dbghelp, EXCEPTION_POINTERS *info )
{
	HANDLE process = GetCurrentProcess();
	HANDLE thread = GetCurrentThread();

	dbghelp->SymSetOptions( SYMOPT_LOAD_LINES | SYMOPT_UNDNAME );

	if ( !dbghelp->SymInitialize( process, NULL, TRUE ) )
	{
		fprintf( fp, "(Symbol information unavailable, stack trace omitted)\n" );
		return;
	}

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

	// SYMBOL_INFO must be aligned to ULONG64 (the trailing
	// Name[] array is accessed through DWORD64-sized fields) -
	// a char[] buffer only guarantees 1-byte alignment, so
	// allocate through a ULONG64 array per the documented
	// dbghelp idiom instead.
	ULONG64 symbolBuffer[(sizeof( SYMBOL_INFO ) + MAX_SYM_NAME * sizeof( char ) + sizeof( ULONG64 ) - 1) / sizeof( ULONG64 )];
	SYMBOL_INFO *symbol = (SYMBOL_INFO *)symbolBuffer;
	symbol->SizeOfStruct = sizeof( SYMBOL_INFO );
	symbol->MaxNameLen = MAX_SYM_NAME;

	for ( int i = 0; i < 64; i++ )
	{
		if ( !dbghelp->StackWalk64( machineType, process, thread, &frame, &context,
				NULL, dbghelp->SymFunctionTableAccess64, dbghelp->SymGetModuleBase64, NULL ) )
		{
			break;
		}

		if ( frame.AddrPC.Offset == 0 )
			break;

		DWORD64 displacement = 0;
		if ( dbghelp->SymFromAddr( process, frame.AddrPC.Offset, &displacement, symbol ) )
		{
			IMAGEHLP_LINE64 line = {};
			line.SizeOfStruct = sizeof( IMAGEHLP_LINE64 );
			DWORD lineDisplacement = 0;

			if ( dbghelp->SymGetLineFromAddr64( process, frame.AddrPC.Offset, &lineDisplacement, &line ) )
				fprintf( fp, "  %s (%s:%lu)\n", symbol->Name, line.FileName, line.LineNumber );
			else
				fprintf( fp, "  %s + 0x%llx\n", symbol->Name, displacement );
		}
		else
		{
			// Release builds ship without a PDB, so SymFromAddr
			// fails here on every frame - fall back to
			// module+offset (e.g. eternaljk.x86_64.exe+0x3f21a8)
			// so the address is still something that can be
			// rebased and looked up against the matching build.
			IMAGEHLP_MODULE64 moduleInfo = {};
			moduleInfo.SizeOfStruct = sizeof( IMAGEHLP_MODULE64 );
			if ( dbghelp->SymGetModuleInfo64( process, frame.AddrPC.Offset, &moduleInfo ) )
				fprintf( fp, "  %s+0x%llx\n", moduleInfo.ModuleName,
					frame.AddrPC.Offset - moduleInfo.BaseOfImage );
			else
				fprintf( fp, "  0x%016llx\n", frame.AddrPC.Offset );
		}
	}

	dbghelp->SymCleanup( process );
}

static LONG WINAPI Sys_CrashHandler( EXCEPTION_POINTERS *info )
{
	// Don't try to handle a crash that happens while we're already
	// writing the crash files for a previous one (or a different thread
	// crashing at the same instant - see Sys_ClaimCrashHandler).
	if ( !Sys_ClaimCrashHandler() )
		return EXCEPTION_EXECUTE_HANDLER;

	char basePath[MAX_OSPATH];
	Sys_CrashDumpBasePath( basePath, sizeof( basePath ) );

	char logPath[MAX_OSPATH];
	char dumpPath[MAX_OSPATH];
	Com_sprintf( logPath, sizeof( logPath ), "%s.log", basePath );
	Com_sprintf( dumpPath, sizeof( dumpPath ), "%s.dmp", basePath );

	// Loaded once, used for both the minidump and the text stack trace.
	DbgHelpApi dbghelp = {};
	bool haveDbgHelp = Sys_LoadDbgHelp( &dbghelp );

	// The minidump goes first, before anything heavy: SymInitialize() in the
	// stack trace below allocates a lot and can re-fault if the original
	// crash was heap corruption. By then the .dmp - the file we actually
	// debug with - is already safely on disk. It doesn't depend on the .log.
	bool wroteMiniDump = haveDbgHelp && Sys_WriteMiniDump( &dbghelp, info, dumpPath );

	bool wroteLog = false;

	FILE *fp = fopen( logPath, "w" );
	if ( fp )
	{
		// Unbuffered for the same reason: if the stack trace re-faults we
		// never reach fclose(), and a buffered file would be left empty.
		setvbuf( fp, NULL, _IONBF, 0 );

		fprintf( fp, "JoF EternalJK crash dump\n" );
		fprintf( fp, "Version: %s (%s@%s)\n", JOFVERSION, JOF_GIT_BRANCH, JOF_COMMIT_SHA_SHORT );
		fprintf( fp, "Built: %s %s\n", __DATE__, __TIME__ );
		fprintf( fp, "Exception code: 0x%08lX at address %p\n",
			info->ExceptionRecord->ExceptionCode,
			info->ExceptionRecord->ExceptionAddress );
		fprintf( fp, "Minidump: %s\n\n", wroteMiniDump ? dumpPath : "(could not be written)" );

		Sys_CrashDumpModules( fp );
		fprintf( fp, "\n" );

		if ( haveDbgHelp )
			Sys_CrashDumpStackTrace( fp, &dbghelp, info );
		else
			fprintf( fp, "(dbghelp.dll unavailable, stack trace omitted)\n" );

		fprintf( fp, "\nRecent console output:\n" );
		ConsoleLogWriteOut( fp );

		fclose( fp );
		wroteLog = true;
	}

#ifndef DEDICATED
	// Shown whichever files were written (mirrors Sys_ErrorDialog's own
	// fopen-failed path) - a failed write should still tell the player
	// something happened, rather than have the game silently vanish.
	char message[2 * MAX_OSPATH + 256];
	if ( wroteMiniDump && wroteLog )
	{
		Com_sprintf( message, sizeof( message ),
			"JoF EternalJK has crashed.\n\nCrash files were written to:\n%s\n%s\n\n"
			"Please send both files when reporting the issue.", dumpPath, logPath );
	}
	else if ( wroteMiniDump || wroteLog )
	{
		Com_sprintf( message, sizeof( message ),
			"JoF EternalJK has crashed.\n\nA crash file was written to:\n%s\n\n"
			"Please send this file when reporting the issue.", wroteMiniDump ? dumpPath : logPath );
	}
	else
	{
		Com_sprintf( message, sizeof( message ),
			"JoF EternalJK has crashed, and the crash files could not be written to:\n%s\n\n"
			"Please report the issue and mention this.", basePath );
	}
	MessageBoxA( NULL, message, "JoF EternalJK - Crash", MB_OK | MB_ICONERROR );
#endif

	return EXCEPTION_EXECUTE_HANDLER;
}

void Sys_InstallCrashHandler( void )
{
	SetUnhandledExceptionFilter( Sys_CrashHandler );
}

#else // !_WIN32

static const int crashSignals[] = { SIGSEGV, SIGABRT, SIGFPE, SIGILL, SIGBUS };

// Logs loaded modules so frames backtrace_symbols_fd can only resolve to
// "binary(function+offset)" (no matching debug info) can still be rebased
// against the right build's binary/debug file afterwards - same purpose as
// the Modules: section on the Windows side.
static void Sys_CrashDumpModules( int fd )
{
	const char header[] = "Modules:\n";
	write( fd, header, sizeof( header ) - 1 );

#if defined(__APPLE__)
	uint32_t count = _dyld_image_count();
	for ( uint32_t i = 0; i < count; i++ )
	{
		const char *name = _dyld_get_image_name( i );
		const struct mach_header *mh = _dyld_get_image_header( i );
		if ( !name || !mh )
			continue;

		char line[512];
		int len = snprintf( line, sizeof( line ), "  %p %s\n", (const void *)mh, name );
		if ( len > 0 )
			write( fd, line, (size_t)len );
	}
#else
	// Linux: /proc/self/maps already has exactly what we need (address
	// ranges + module paths per mapped region) - copy it through as-is
	// rather than parsing it ourselves.
	int mapsFd = open( "/proc/self/maps", O_RDONLY );
	if ( mapsFd >= 0 )
	{
		char buf[4096];
		ssize_t n;
		while ( ( n = read( mapsFd, buf, sizeof( buf ) ) ) > 0 )
			write( fd, buf, (size_t)n );
		close( mapsFd );
	}
#endif

	write( fd, "\n", 1 );
}

// Runs on the crashing thread inside the signal handler. Strictly this
// isn't async-signal-safe (Sys_CrashDumpBasePath formats a timestamp via libc,
// and Com_sprintf/backtrace_symbols_fd may allocate), but it's a best-effort
// dump for a process that's already dying, not a guarantee - and it sticks
// to open/write/backtrace_symbols_fd rather than the malloc-heavy
// ConsoleLogWriteOut path used on Windows to keep that risk as low as
// practical.
static void Sys_CrashHandler( int sig, siginfo_t *info, void *ucontext )
{
	signal( sig, SIG_DFL );

	// See Sys_ClaimCrashHandler: guards against another thread crashing at
	// the same instant, not just this handler recursing into itself.
	if ( !Sys_ClaimCrashHandler() )
	{
		raise( sig );
		return;
	}

	char basePath[MAX_OSPATH];
	Sys_CrashDumpBasePath( basePath, sizeof( basePath ) );

	char path[MAX_OSPATH];
	Com_sprintf( path, sizeof( path ), "%s.log", basePath );

	int fd = open( path, O_WRONLY | O_CREAT | O_TRUNC, 0644 );
	if ( fd >= 0 )
	{
		char header[512];
		int len = snprintf( header, sizeof( header ),
			"JoF EternalJK crash dump\nVersion: %s (%s@%s)\nBuilt: %s %s\nSignal: %d (%s)\nFaulting address: %p\n\n",
			JOFVERSION, JOF_GIT_BRANCH, JOF_COMMIT_SHA_SHORT,
			__DATE__, __TIME__, sig, strsignal( sig ), info ? info->si_addr : NULL );
		if ( len > 0 )
			write( fd, header, (size_t)len );

		Sys_CrashDumpModules( fd );

		const char stackHeader[] = "Stack trace:\n";
		write( fd, stackHeader, sizeof( stackHeader ) - 1 );

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
