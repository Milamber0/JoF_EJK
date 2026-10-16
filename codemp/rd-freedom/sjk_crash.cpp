/*
rd-freedom crash log: a fault while the renderer runs is written, with the addresses
in this DLL found on the stack, to rd-freedom-crash.log beside the renderer's settings.
The engine's console never sees the lines of a frame that does not return.
*/

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>

static char crashPath[MAX_PATH];
static HMODULE self;
static uintptr_t selfBase, selfEnd;
static PVOID handler;

static int SJK_Fatal( DWORD code ) {
	switch ( code ) {
	case EXCEPTION_ACCESS_VIOLATION:
	case EXCEPTION_STACK_OVERFLOW:
	case EXCEPTION_ILLEGAL_INSTRUCTION:
	case EXCEPTION_INT_DIVIDE_BY_ZERO:
	case EXCEPTION_PRIV_INSTRUCTION:
	case EXCEPTION_ARRAY_BOUNDS_EXCEEDED:
	case EXCEPTION_DATATYPE_MISALIGNMENT:
	case EXCEPTION_IN_PAGE_ERROR:
		return 1;
	default:
		return 0;
	}
}

static LONG CALLBACK SJK_CrashHandler( PEXCEPTION_POINTERS info ) {
	const DWORD code = info->ExceptionRecord->ExceptionCode;
	if ( !SJK_Fatal( code ) || !crashPath[0] ) {
		return EXCEPTION_CONTINUE_SEARCH;
	}
	FILE *f = fopen( crashPath, "a" );
	if ( f ) {
		const uintptr_t pc = (uintptr_t)info->ExceptionRecord->ExceptionAddress;
		fprintf( f, "exception %08lx at %p", code, (void *)pc );
		if ( pc >= selfBase && pc < selfEnd ) {
			fprintf( f, " (rd-freedom+0x%llx)", (unsigned long long)( pc - selfBase ) );
		}
		if ( code == EXCEPTION_ACCESS_VIOLATION && info->ExceptionRecord->NumberParameters >= 2 ) {
			fprintf( f, ", %s %p", info->ExceptionRecord->ExceptionInformation[0] ? "writing" : "reading",
				(void *)info->ExceptionRecord->ExceptionInformation[1] );
		}
		fprintf( f, ", thread %lu\n", GetCurrentThreadId() );
#ifdef _WIN64
		const uintptr_t *sp = (const uintptr_t *)info->ContextRecord->Rsp;
		MEMORY_BASIC_INFORMATION region;
		if ( VirtualQuery( sp, &region, sizeof( region ) ) ) {
			const uintptr_t *end = (const uintptr_t *)( (const char *)region.BaseAddress + region.RegionSize );
			int found = 0;
			for ( const uintptr_t *p = sp; p < end && found < 48; p++ ) {
				if ( *p >= selfBase && *p < selfEnd ) {
					fprintf( f, "  stack rd-freedom+0x%llx\n", (unsigned long long)( *p - selfBase ) );
					found++;
				}
			}
		}
#endif
		fclose( f );
	}
	return EXCEPTION_CONTINUE_SEARCH;
}

void SJK_InstallCrashLog( const char *path ) {
	strncpy( crashPath, path, sizeof( crashPath ) - 1 );
	if ( handler ) {
		return;
	}
	GetModuleHandleExA( GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
		(LPCSTR)&SJK_InstallCrashLog, &self );
	if ( self ) {
		const IMAGE_DOS_HEADER *dos = (const IMAGE_DOS_HEADER *)self;
		const IMAGE_NT_HEADERS *nt = (const IMAGE_NT_HEADERS *)( (const char *)self + dos->e_lfanew );
		selfBase = (uintptr_t)self;
		selfEnd = selfBase + nt->OptionalHeader.SizeOfImage;
	}
	handler = AddVectoredExceptionHandler( 1, SJK_CrashHandler );
}

// One line into the crash log, written through at once.
void SJK_Trace( const char *line ) {
	if ( !crashPath[0] ) {
		return;
	}
	FILE *f = fopen( crashPath, "a" );
	if ( f ) {
		fprintf( f, "%s\n", line );
		fclose( f );
	}
}

// Before the DLL unloads: the handler is code in it.
void SJK_RemoveCrashLog( void ) {
	if ( handler ) {
		RemoveVectoredExceptionHandler( handler );
		handler = NULL;
	}
}
extern "C" BOOL WINAPI DllMain( HINSTANCE instance, DWORD reason, LPVOID reserved ) {
	if ( reason == DLL_PROCESS_DETACH ) {
		SJK_Trace( reserved ? "unload at exit" : "unload" );
	}
	return TRUE;
}
#else
void SJK_InstallCrashLog( const char *path ) {}
void SJK_RemoveCrashLog( void ) {}
void SJK_Trace( const char *line ) {}
#endif
