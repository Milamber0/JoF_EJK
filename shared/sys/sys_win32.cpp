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

#include "sys_local.h"
#include "sys_loadlib.h"
#include <direct.h>
#include <io.h>
#include <shlobj.h>
#include <windows.h>

#define MEM_THRESHOLD (128*1024*1024)

// Used to determine where to store user-specific files
static char homePath[ MAX_OSPATH ] = { 0 };

static UINT timerResolution = 0;

#ifndef DEDICATED
// Prefer descrete GPU over integrated when both are available.
// http://developer.download.nvidia.com/devzone/devcenter/gamegraphics/files/OptimusRenderingPolicies.pdf
extern "C" __declspec(dllexport) unsigned long NvOptimusEnablement = 0x00000001;
// https://gpuopen.com/amdpowerxpressrequesthighperformance/
extern "C" __declspec(dllexport) int AmdPowerXpressRequestHighPerformance = 1;
#endif

/*
==============
Sys_Basename
==============
*/
const char *Sys_Basename( char *path )
{
	static char base[ MAX_OSPATH ] = { 0 };
	int length;

	length = strlen( path ) - 1;

	// Skip trailing slashes
	while( length > 0 && path[ length ] == '\\' )
		length--;

	while( length > 0 && path[ length - 1 ] != '\\' )
		length--;

	Q_strncpyz( base, &path[ length ], sizeof( base ) );

	length = strlen( base ) - 1;

	// Strip trailing slashes
	while( length > 0 && base[ length ] == '\\' )
    base[ length-- ] = '\0';

	return base;
}

/*
==============
Sys_Dirname
==============
*/
const char *Sys_Dirname( char *path )
{
	static char dir[ MAX_OSPATH ] = { 0 };
	int length;

	Q_strncpyz( dir, path, sizeof( dir ) );
	length = strlen( dir ) - 1;

	while( length > 0 && dir[ length ] != '\\' )
		length--;

	dir[ length ] = '\0';

	return dir;
}

/*
================
Sys_Milliseconds
================
*/
int Sys_Milliseconds (bool baseTime)
{
	static int sys_timeBase = timeGetTime();
	int			sys_curtime;

	sys_curtime = timeGetTime();
	if(!baseTime)
	{
		sys_curtime -= sys_timeBase;
	}

	return sys_curtime;
}

int Sys_Milliseconds2( void )
{
	return Sys_Milliseconds(false);
}

/*
================
Sys_RandomBytes
================
*/
bool Sys_RandomBytes( byte *string, int len )
{
	HCRYPTPROV  prov;

	if( !CryptAcquireContext( &prov, NULL, NULL,
		PROV_RSA_FULL, CRYPT_VERIFYCONTEXT ) )  {

		return false;
	}

	if( !CryptGenRandom( prov, len, (BYTE *)string ) )  {
		CryptReleaseContext( prov, 0 );
		return false;
	}
	CryptReleaseContext( prov, 0 );
	return true;
}

/*
==================
Sys_GetCurrentUser
==================
*/
char *Sys_GetCurrentUser( void )
{
	static char s_userName[1024];
	DWORD size = sizeof( s_userName );

	if ( !GetUserName( s_userName, &size ) )
		strcpy( s_userName, "player" );

	if ( !s_userName[0] )
	{
		strcpy( s_userName, "player" );
	}

	return s_userName;
}

/*
* Builds the path for the user's game directory
*/
extern cvar_t *fs_portable;
char *Sys_DefaultHomePath( void )
{
#if defined(_PORTABLE_VERSION)
	Com_Printf( "Portable install requested, skipping homepath support\n" );
	return NULL;
#else
	if ( fs_portable && fs_portable->integer )
	{
		Com_Printf("fs_portable enabled, skipping fs_homepath support\n");

		if (fs_portable->integer == 2)
			return NULL;

		FILE *ftest = fopen(va("%s/w", Sys_DefaultInstallPath()), "w");

		if (ftest) {
			fclose(ftest);
			remove(va("%s/w", Sys_DefaultInstallPath()));
			return NULL;
		}
		else {
			Com_Printf("fs_portable write test failed, using fs_homepath\n");
		}
	}

	if ( !homePath[0] )
	{
		TCHAR homeDirectory[MAX_PATH];

		if( !SUCCEEDED( SHGetFolderPath( NULL, CSIDL_PERSONAL, NULL, 0, homeDirectory ) ) )
		{
			Com_Printf( "Unable to determine your home directory.\n" );
			return NULL;
		}

		Com_sprintf( homePath, sizeof( homePath ), "%s%cMy Games%c", homeDirectory, PATH_SEP, PATH_SEP );
		if ( com_homepath && com_homepath->string[0] )
			Q_strcat( homePath, sizeof( homePath ), com_homepath->string );
		else
			Q_strcat( homePath, sizeof( homePath ), HOMEPATH_NAME_WIN );
	}

	return homePath;
#endif
}

static const char *GetErrorString( DWORD error ) {
	static char buf[MAX_STRING_CHARS];
	buf[0] = '\0';

	if ( error ) {
		LPVOID lpMsgBuf;
		DWORD bufLen = FormatMessage( FORMAT_MESSAGE_ALLOCATE_BUFFER | FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS,
			NULL, error, MAKELANGID( LANG_NEUTRAL, SUBLANG_DEFAULT ), (LPTSTR)&lpMsgBuf, 0, NULL );
		if ( bufLen ) {
			LPCSTR lpMsgStr = (LPCSTR)lpMsgBuf;
			Q_strncpyz( buf, lpMsgStr, Q_min( (size_t)(lpMsgStr + bufLen), sizeof(buf) ) );
			LocalFree( lpMsgBuf );
		}
	}
	return buf;
}

void Sys_SetProcessorAffinity( void ) {
	DWORD_PTR processMask, processAffinityMask, systemAffinityMask;
	HANDLE handle = GetCurrentProcess();

	if ( !GetProcessAffinityMask( handle, &processAffinityMask, &systemAffinityMask ) )
		return;

	if ( sscanf( com_affinity->string, "%X", &processMask ) != 1 )
		processMask = 1; // set to first core only

	if ( !processMask )
		processMask = systemAffinityMask; // use all the cores available to the system

	if ( processMask == processAffinityMask )
		return; // no change

	if ( !SetProcessAffinityMask( handle, processMask ) )
		Com_DPrintf( "Setting affinity mask failed (%s)\n", GetErrorString( GetLastError() ) );
}

void Sys_SetProcessPriority(void) {
	extern cvar_t *com_priority;
	DWORD_PTR desiredPriorityClass, currentProcessPriorityClass;
	HANDLE handle = GetCurrentProcess();

	if (!com_priority)
		return;

	if (com_priority->integer == -1)
		return;

	currentProcessPriorityClass = GetPriorityClass(handle);

	if (!Q_stricmp(com_priority->string, "normal")) {
		desiredPriorityClass = NORMAL_PRIORITY_CLASS;
	}
	else if (!Q_stricmp(com_priority->string, "high")) {
		desiredPriorityClass = HIGH_PRIORITY_CLASS;
	}
	else if (!Q_stricmp(com_priority->string, "realtime")) {
		desiredPriorityClass = REALTIME_PRIORITY_CLASS;
	}
	else if (!Q_stricmp(com_priority->string, "low")) {
		desiredPriorityClass = BELOW_NORMAL_PRIORITY_CLASS;
	}
	else if (!Q_stricmp(com_priority->string, "idle")) {
		desiredPriorityClass = IDLE_PRIORITY_CLASS;
	}
	else {
		switch (com_priority->integer)
		{
			case 24: //realtime
				desiredPriorityClass = REALTIME_PRIORITY_CLASS;
				break;
			case 13: //high
				desiredPriorityClass = HIGH_PRIORITY_CLASS;
				break;
			case 10: //above normal
				desiredPriorityClass = ABOVE_NORMAL_PRIORITY_CLASS;
				break;
			case 8: //normal/default
			default:
				desiredPriorityClass = NORMAL_PRIORITY_CLASS;
				break;
			case 6: //below normal
				desiredPriorityClass = BELOW_NORMAL_PRIORITY_CLASS;
				break;
			case 4: //background (Low I/O & memory priority)
			case 2: //idle
				desiredPriorityClass = IDLE_PRIORITY_CLASS;
				break;
		}
	}

	if (!desiredPriorityClass)
		return;

	if (desiredPriorityClass != NORMAL_PRIORITY_CLASS && desiredPriorityClass == currentProcessPriorityClass) {
		Com_DPrintf("Desired priority class already set\n");
		return;
	}

	if (!SetPriorityClass(handle, desiredPriorityClass)) {
		Com_Printf("^3failed to set process priority?\n");
		return;
	}

	if (GetPriorityClass(handle) == desiredPriorityClass)
		Com_DPrintf("Set process priority successfully.\n");
		//why does it get here if windows sometimes forces it to background priority????
}

/*
==================
Sys_LowPhysicalMemory()
==================
*/

qboolean Sys_LowPhysicalMemory(void) {
	static MEMORYSTATUSEX stat;
	static qboolean bAsked = qfalse;
	static cvar_t* sys_lowmem = Cvar_Get( "sys_lowmem", "0", 0 );

	if (!bAsked)	// just in case it takes a little time for GlobalMemoryStatusEx() to gather stats on
	{				//	stuff we don't care about such as virtual mem etc.
		bAsked = qtrue;

		stat.dwLength = sizeof (stat);
		GlobalMemoryStatusEx (&stat);
	}
	if (sys_lowmem->integer)
	{
		return qtrue;
	}
	return (stat.ullTotalPhys <= MEM_THRESHOLD) ? qtrue : qfalse;
}

/*
==============
Sys_Mkdir
==============
*/
qboolean Sys_Mkdir( const char *path ) {
	if( !CreateDirectory( path, NULL ) )
	{
		if( GetLastError( ) != ERROR_ALREADY_EXISTS )
			return qfalse;
	}
	return qtrue;
}

/*
==============
Sys_Cwd
==============
*/
char *Sys_Cwd( void ) {
	static char cwd[MAX_OSPATH];

	_getcwd( cwd, sizeof( cwd ) - 1 );
	cwd[MAX_OSPATH-1] = 0;

	return cwd;
}

/* Resolves path names and determines if they are the same */
/* For use with full OS paths not quake paths */
/* Returns true if resulting paths are valid and the same, otherwise false */
bool Sys_PathCmp( const char *path1, const char *path2 ) {
	char *r1, *r2;

	r1 = _fullpath(NULL, path1, MAX_OSPATH);
	r2 = _fullpath(NULL, path2, MAX_OSPATH);

	if(r1 && r2 && !Q_stricmp(r1, r2))
	{
		free(r1);
		free(r2);
		return true;
	}

	free(r1);
	free(r2);
	return false;
}

/*
==============================================================

DIRECTORY SCANNING

==============================================================
*/

#define	MAX_FOUND_FILES	0x1000

void Sys_ListFilteredFiles( const char *basedir, char *subdirs, char *filter, char **psList, int *numfiles ) {
	char		search[MAX_OSPATH], newsubdirs[MAX_OSPATH];
	char		filename[MAX_OSPATH];
	intptr_t	findhandle;
	struct _finddata_t findinfo;

	if ( *numfiles >= MAX_FOUND_FILES - 1 ) {
		return;
	}

	if (strlen(subdirs)) {
		Com_sprintf( search, sizeof(search), "%s\\%s\\*", basedir, subdirs );
	}
	else {
		Com_sprintf( search, sizeof(search), "%s\\*", basedir );
	}

	findhandle = _findfirst (search, &findinfo);
	if (findhandle == -1) {
		return;
	}

	do {
		if (findinfo.attrib & _A_SUBDIR) {
			if (Q_stricmp(findinfo.name, ".") && Q_stricmp(findinfo.name, "..")) {
				if (strlen(subdirs)) {
					Com_sprintf( newsubdirs, sizeof(newsubdirs), "%s\\%s", subdirs, findinfo.name);
				}
				else {
					Com_sprintf( newsubdirs, sizeof(newsubdirs), "%s", findinfo.name);
				}
				Sys_ListFilteredFiles( basedir, newsubdirs, filter, psList, numfiles );
			}
		}
		if ( *numfiles >= MAX_FOUND_FILES - 1 ) {
			break;
		}
		Com_sprintf( filename, sizeof(filename), "%s\\%s", subdirs, findinfo.name );
		if (!Com_FilterPath( filter, filename, qfalse ))
			continue;
		psList[ *numfiles ] = CopyString( filename );
		(*numfiles)++;
	} while ( _findnext (findhandle, &findinfo) != -1 );

	_findclose (findhandle);
}

static qboolean strgtr(const char *s0, const char *s1) {
	int l0, l1, i;

	l0 = strlen(s0);
	l1 = strlen(s1);

	if (l1<l0) {
		l0 = l1;
	}

	for(i=0;i<l0;i++) {
		if (s1[i] > s0[i]) {
			return qtrue;
		}
		if (s1[i] < s0[i]) {
			return qfalse;
		}
	}
	return qfalse;
}

char **Sys_ListFiles( const char *directory, const char *extension, char *filter, int *numfiles, qboolean wantsubs ) {
	char		search[MAX_OSPATH];
	int			nfiles;
	char		**listCopy;
	char		*list[MAX_FOUND_FILES];
	struct _finddata_t findinfo;
	intptr_t	findhandle;
	int			flag;
	int			i;
	int			extLen;

	if (filter) {

		nfiles = 0;
		Sys_ListFilteredFiles( directory, "", filter, list, &nfiles );

		list[ nfiles ] = 0;
		*numfiles = nfiles;

		if (!nfiles)
			return NULL;

		listCopy = (char **)Z_Malloc( ( nfiles + 1 ) * sizeof( *listCopy ), TAG_LISTFILES );
		for ( i = 0 ; i < nfiles ; i++ ) {
			listCopy[i] = list[i];
		}
		listCopy[i] = NULL;

		return listCopy;
	}

	if ( !extension) {
		extension = "";
	}

	// passing a slash as extension will find directories
	if ( extension[0] == '/' && extension[1] == 0 ) {
		extension = "";
		flag = 0;
	} else {
		flag = _A_SUBDIR;
	}

	extLen = strlen( extension );

	Com_sprintf( search, sizeof(search), "%s\\*%s", directory, extension );

	// search
	nfiles = 0;

	findhandle = _findfirst (search, &findinfo);
	if (findhandle == -1) {
		*numfiles = 0;
		return NULL;
	}

	do {
		if ( (!wantsubs && flag ^ ( findinfo.attrib & _A_SUBDIR )) || (wantsubs && findinfo.attrib & _A_SUBDIR) ) {
			if (*extension) {
				if ( strlen( findinfo.name ) < extLen ||
					Q_stricmp(
						findinfo.name + strlen( findinfo.name ) - extLen,
						extension ) ) {
					continue; // didn't match
				}
			}
			if ( nfiles == MAX_FOUND_FILES - 1 ) {
				break;
			}
			list[ nfiles ] = CopyString( findinfo.name );
			nfiles++;
		}
	} while ( _findnext (findhandle, &findinfo) != -1 );

	list[ nfiles ] = 0;

	_findclose (findhandle);

	// return a copy of the list
	*numfiles = nfiles;

	if ( !nfiles ) {
		return NULL;
	}

	listCopy = (char **)Z_Malloc( ( nfiles + 1 ) * sizeof( *listCopy ), TAG_LISTFILES );
	for ( i = 0 ; i < nfiles ; i++ ) {
		listCopy[i] = list[i];
	}
	listCopy[i] = NULL;

	do {
		flag = 0;
		for(i=1; i<nfiles; i++) {
			if (strgtr(listCopy[i-1], listCopy[i])) {
				char *temp = listCopy[i];
				listCopy[i] = listCopy[i-1];
				listCopy[i-1] = temp;
				flag = 1;
			}
		}
	} while(flag);

	return listCopy;
}

void	Sys_FreeFileList( char **psList ) {
	int		i;

	if ( !psList ) {
		return;
	}

	for ( i = 0 ; psList[i] ; i++ ) {
		Z_Free( psList[i] );
	}

	Z_Free( psList );
}

/*
========================================================================

LOAD/UNLOAD DLL

========================================================================
*/
//make sure the dll can be opened by the file system, then write the
//file back out again so it can be loaded is a library. If the read
//fails then the dll is probably not in the pk3 and we are running
//a pure server -rww
extern cvar_t *fs_loadpakdlls;
UnpackDLLResult Sys_UnpackDLL(const char *name)
{
	UnpackDLLResult result = {};
	void *data;
	long len;

#ifdef TOURNAMENT_CLIENT
	if (1)
		len = FS_ReadDLLInPAK(name, &data);
	else
		len = FS_ReadFile(name, &data);
#else
	if (!fs_loadpakdlls || fs_loadpakdlls->integer == 1) {
		len = FS_ReadDLLInPAK(name, &data);
	}
	else {
		len = FS_ReadFile(name, &data);
	}
#endif

	if (len >= 1)
	{
		char *tempFileName;
		if ( FS_WriteToTemporaryFile(data, len, &tempFileName) )
		{
			result.tempDLLPath = tempFileName;
			result.succeeded = true;
		}
	}

	FS_FreeFile(data);

	return result;
}

bool Sys_DLLNeedsUnpacking()
{
#if defined(_JK2EXE)
	return false;
#else
	return Cvar_VariableIntegerValue("sv_pure") != 0;
#endif
}

/*
================
Sys_PlatformInit

Platform-specific initialization
================
*/
void Sys_PlatformInit( void ) {
	TIMECAPS ptc;
	if ( timeGetDevCaps( &ptc, sizeof( ptc ) ) == MMSYSERR_NOERROR )
	{
		timerResolution = ptc.wPeriodMin;

		if ( timerResolution > 1 )
		{
			Com_Printf( "Warning: Minimum supported timer resolution is %ums "
				"on this system, recommended resolution 1ms\n", timerResolution );
		}

		timeBeginPeriod( timerResolution );
	}
	else {
		timerResolution = 0;
	}
}

/*
================
Sys_PlatformExit

Platform-specific exit code
================
*/
void Sys_PlatformExit( void )
{
	if ( timerResolution )
		timeEndPeriod( timerResolution );
}

void Sys_Sleep( int msec )
{
	if ( msec == 0 )
		return;

#ifdef DEDICATED
	if ( msec < 0 )
		WaitForSingleObject( GetStdHandle( STD_INPUT_HANDLE ), INFINITE );
	else
		WaitForSingleObject( GetStdHandle( STD_INPUT_HANDLE ), msec );
#else
	// Client Sys_Sleep doesn't support waiting on stdin
	if ( msec < 0 )
		return;

	Sleep( msec );
#endif
}

#ifndef DEDICATED
void Sys_SetClipboardDataWin32(const char *cbText)
{
	char far	*buffer;
	int			size = 0;
	HGLOBAL		clipbuffer;

	if (!cbText || !cbText[0])
		return;

	size = strlen(cbText);
	if (size < 1)
		return;

	OpenClipboard(NULL);
	EmptyClipboard();

	clipbuffer = GlobalAlloc(GMEM_DDESHARE, size + 1);
	if (!clipbuffer)
		return;

	buffer = (char far *)GlobalLock(clipbuffer);
	if (!buffer)
		return;

	strcpy(buffer, cbText);

	GlobalUnlock(clipbuffer);
	SetClipboardData(CF_TEXT, clipbuffer);
	CloseClipboard();
}
#endif

#ifdef NVML
#include <nvml.h>
#pragma warning(disable : 26812)

static qboolean nvmlSupported = qfalse;
static cvar_t *nvmlDeviceIndex;
static unsigned int NVML_DeviceCount;
static nvmlPciInfo_t nvmlPCI = { 0 };
static nvmlDevice_t currentGPU = { 0 };
static char GPUName[NVML_DEVICE_NAME_BUFFER_SIZE] = { 0 };

void Sys_NVMLShutDown(void) {
	nvmlReturn_t result;

	if (!nvmlSupported)
		return;

	result = nvmlShutdown();
	if (result != NVML_SUCCESS) {
		//Com_Printf(S_COLOR_YELLOW "Sys_NVMLShutDown(): Shutdown failed, error string %s\n", nvmlErrorString(result));
		Com_Error(ERR_FATAL, S_COLOR_YELLOW "Sys_NVMLShutDown(): Shutdown failed, error string %s\n", nvmlErrorString(result));
	}
}


void Sys_NVMLListDevices(void) {
	unsigned int	w;
	nvmlReturn_t	result;

	if (!nvmlSupported)
		return;

	for (w = 0; w < NVML_DeviceCount; w++)
	{
		nvmlDevice_t device;
		char name[NVML_DEVICE_NAME_BUFFER_SIZE];
		nvmlPciInfo_t pci;
		nvmlComputeMode_t compute_mode;

		// Query for device handle to perform operations on a device
		// You can also query device handle by other features like:
		// nvmlDeviceGetHandleBySerial
		// nvmlDeviceGetHandleByPciBusId
		result = nvmlDeviceGetHandleByIndex(w, &device);
		if (NVML_SUCCESS != result)
		{
			Com_Printf(S_COLOR_RED "Failed to get handle for device %u: %s\n", w, nvmlErrorString(result));
			return;
		}

		result = nvmlDeviceGetName(device, name, NVML_DEVICE_NAME_BUFFER_SIZE);
		if (NVML_SUCCESS != result)
		{
			Com_Printf(S_COLOR_YELLOW "Failed to get name of device %u: %s\n", w, nvmlErrorString(result));
			return;
		}

		// pci.busId is very useful to know which device physically you're talking to
		// Using PCI identifier you can also match nvmlDevice handle to CUDA device.
		result = nvmlDeviceGetPciInfo(device, &pci);
		if (NVML_SUCCESS != result)
		{
			Com_Printf(S_COLOR_YELLOW "Failed to get pci info for device %u: %s\n", w, nvmlErrorString(result));
			return;
		}

		Com_Printf(S_COLOR_GREEN "Index: %u. %s [%s]\n", w, name, pci.busId);

		// This is a simple example on how you can modify GPU's state
		result = nvmlDeviceGetComputeMode(device, &compute_mode);
		if (NVML_ERROR_NOT_SUPPORTED == result)
			Com_Printf("\t This is not CUDA capable device\n");
		else if (NVML_SUCCESS != result)
		{
			Com_Printf("Failed to get compute mode for device %u: %s\n", w, nvmlErrorString(result));
			return;
		}
	}
}

void Sys_NVMLSetDeviceHandle(int index) {
	nvmlReturn_t result;

	if (!nvmlSupported)
		return;

	if (index < 0 || index > NVML_DeviceCount) {
		Com_Error(ERR_FATAL, "Invalid NVML Device index (%i)!\n", index);
		return;
	}

	result = nvmlDeviceGetHandleByIndex(nvmlDeviceIndex->integer, &currentGPU);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s\n", nvmlErrorString(result));
		//nvmlSupported = qfalse;
		return;
	}

	result = nvmlDeviceGetPciInfo(currentGPU, &nvmlPCI);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s\n", nvmlErrorString(result));
		//nvmlSupported = qfalse;
		return;
	}

	//result = nvmlDeviceGetName(device, name, NVML_DEVICE_NAME_BUFFER_SIZE);
}

void Sys_NVML_RemoveGPU_f(void) { //probably fucks EVERYTHING up
	int argc = Cmd_Argc();
	nvmlReturn_t result;
	nvmlDetachGpuState_t detachState = NVML_DETACH_GPU_KEEP;
	nvmlPcieLinkState_t pcieLinkState = NVML_PCIE_LINK_KEEP;

	if (argc == 1 && atoi(Cmd_Argv(1))) {
		detachState = NVML_DETACH_GPU_REMOVE;
		pcieLinkState = NVML_PCIE_LINK_SHUT_DOWN;
	}
	else if (argc >= 3) {
		if (atoi(Cmd_Argv(1)))
			detachState = NVML_DETACH_GPU_REMOVE;
		if (atoi(Cmd_Argv(2)))
			pcieLinkState = NVML_PCIE_LINK_SHUT_DOWN;
	}

	if (!nvmlSupported)
		return;

	result = nvmlDeviceRemoveGpu(&nvmlPCI, detachState, pcieLinkState);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
}

/*char NVML_ClockTypeString(nvmlClockType_t clockType) {
	switch (clockType) {
		case NVML_CLOCK_GRAPHICS:
			return "Graphics clock domain";
		case NVML_CLOCK_SM:
			return ""
		default:
			return NULL;
	}
}*/

void Sys_NVMLTemplate(void) {
	nvmlReturn_t result;

	if (!nvmlSupported)
		return;

}

void Sys_NVMLSetPowerLimit_f(void) {
	nvmlReturn_t result;
	nvmlPstates_t pState;
	unsigned int powerLimit, defaultPowerLimit, minPowerLimit, maxPowerLimit, requestedPowerLimit;
	int argc = Cmd_Argc();

	if (!nvmlSupported)
		return;

	result = nvmlDeviceGetPerformanceState(currentGPU, &pState);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Current performance state: %i\n", (int)pState);

	result = nvmlDeviceGetPowerState(currentGPU, &pState);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Current power state: %i\n", (int)pState);

	result = nvmlDeviceGetPowerManagementLimit(currentGPU, &powerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Current power limit: %i\n", powerLimit);

	result = nvmlDeviceGetPowerManagementLimitConstraints(currentGPU, &minPowerLimit, &maxPowerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	result = nvmlDeviceGetPowerManagementDefaultLimit(currentGPU, &defaultPowerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Min Power Limit: %i - Max Power Limit %i - Default Power Limit %i\n", minPowerLimit, maxPowerLimit, defaultPowerLimit);

	if (argc != 2)
		return;

	requestedPowerLimit = atoi(Cmd_Argv(1));
	Com_Printf("Attempting to change power limit from to %i\n", powerLimit, requestedPowerLimit);
	if (requestedPowerLimit < minPowerLimit || requestedPowerLimit > maxPowerLimit)
		Com_Printf(S_COLOR_YELLOW "WARNING: Requested power limit exceeds minimum/maximum (%i/%i)\n", minPowerLimit, maxPowerLimit);

	result = nvmlDeviceSetPowerManagementLimit(currentGPU, requestedPowerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("Failed with error: %s \n", nvmlErrorString(result));
		return;
	}
}

void Sys_NVMLResetClocks_f(void) {
	nvmlReturn_t result;

	if (!nvmlSupported)
		return;

	result = nvmlDeviceResetApplicationsClocks(currentGPU);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("%s clocks reset to default!\n", GPUName);
}

void Sys_NVMLAutoBoostedClock_f(void) {
	nvmlReturn_t result;
	nvmlEnableState_t isEnabled, isEnabledDefault;
	nvmlEnableState_t isRestricted = NVML_FEATURE_DISABLED;
	int argc = Cmd_Argc();
	char *arg2;
	qboolean setDefault;

	if (!nvmlSupported)
		return;

	result = nvmlDeviceSetAPIRestriction(currentGPU, NVML_RESTRICTED_API_SET_APPLICATION_CLOCKS, isRestricted);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	result = nvmlDeviceSetAPIRestriction(currentGPU, NVML_RESTRICTED_API_SET_AUTO_BOOSTED_CLOCKS, isRestricted);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}

	result = nvmlDeviceGetAutoBoostedClocksEnabled(currentGPU, &isEnabled, &isEnabledDefault);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}

	Com_Printf("NVML device autoboosted clocks are %s ", isEnabled ? "enabled" : "disabled");
	if (isEnabledDefault)
		Com_Printf("(default)");
	Com_Printf("\n");

	if (!argc) {
		return;
	}
	else if (argc < 3) {
		Com_Printf("usage: /nvmlAutoBoostClocks <set/setDefault> <enable/disable>\n");
		return;
	}

	setDefault = (qboolean)(!Q_stricmp(Cmd_Argv(1), "setDefault"));
	arg2 = Cmd_Argv(2);

	isEnabled = isEnabledDefault; //assume default setting if code below is invalid..

	if (!Q_stricmp(arg2, "disable") || !Q_stricmp(arg2, "0")) {
		isEnabled = NVML_FEATURE_DISABLED;
	}
	else if (!Q_stricmp(arg2, "enable") || atoi(arg2)) {
		isEnabled = NVML_FEATURE_ENABLED;
	}
	if (setDefault) {
		result = nvmlDeviceSetDefaultAutoBoostedClocksEnabled(currentGPU, isEnabled, 0);
	}
	else {
		result = nvmlDeviceSetAutoBoostedClocksEnabled(currentGPU, isEnabled);
	}

	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
}

void Sys_NVMLGetUtilization(unsigned int *gpuUsage, unsigned int *vramUsage) { //wrote this incase i want to add an on-screen display
	nvmlReturn_t result;
	nvmlUtilization_t util = { 0 };

	if (!nvmlSupported)
		return;

	result = nvmlDeviceGetUtilizationRates(currentGPU, &util);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}

	gpuUsage = &util.gpu;
	vramUsage = &util.memory;
}

void Sys_NVMLStatus_f(void) {
	nvmlReturn_t result;
	nvmlUtilization_t util = { 0 };
	nvmlMemory_t memory = { 0 };
	nvmlPstates_t pState;
	unsigned int gpuUsage, vramUsage, powerUsage;
	unsigned int clock, temp, fanSpeed;
	unsigned int minPowerLimit, maxPowerLimit, defaultPowerLimit, powerLimit;

	if (!nvmlSupported)
		return;

	Com_Printf("%s\n", GPUName);

	//Sys_NVMLGetUtilization(&gpuUsage, &vramUsage);
	//Com_Printf("Utilization: gpu: %i% - memory: %i%\n", gpuUsage, vramUsage);

	result = nvmlDeviceGetUtilizationRates(currentGPU, &util);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	result = nvmlDeviceGetPowerUsage(currentGPU, &powerUsage);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Utilization - gpu: %i % - memory: %i - power %i\n", util.gpu, util.memory, powerUsage);

	result = nvmlDeviceGetTemperature(currentGPU, NVML_TEMPERATURE_GPU, &temp);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Temperature: %iC\n", temp);

	result = nvmlDeviceGetFanSpeed(currentGPU, &fanSpeed);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Fan speed: %i\n", fanSpeed); //is this the percentage ?

	Com_Printf("Temperature thresholds - ");
	result = nvmlDeviceGetTemperatureThreshold(currentGPU, NVML_TEMPERATURE_THRESHOLD_SHUTDOWN, &temp);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		//return;
	}
	else {
		Com_Printf("Shutdown: %iC - ", temp);
	}

	result = nvmlDeviceGetTemperatureThreshold(currentGPU, NVML_TEMPERATURE_THRESHOLD_SLOWDOWN, &temp);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		//return;
	}
	else {
		Com_Printf("Slowdown: %iC - ", temp);
	}

	/*result = nvmlDeviceGetTemperatureThreshold(currentGPU, NVML_TEMPERATURE_THRESHOLD_MEM_MAX, &temp);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		//return;
	}
	else {
		Com_Printf("Mem max: %iC - ", temp);
	}*/

	result = nvmlDeviceGetTemperatureThreshold(currentGPU, NVML_TEMPERATURE_THRESHOLD_GPU_MAX, &temp);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		//return;
	}
	else {
		Com_Printf("GPU Max: %iC\n", temp);
	}

	result = nvmlDeviceGetPerformanceState(currentGPU, &pState);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Current performance state: %i\n", (int)pState);

	result = nvmlDeviceGetPowerState(currentGPU, &pState);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Current power state: %i\n", (int)pState);

	result = nvmlDeviceGetPowerManagementLimit(currentGPU, &powerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Current power limit: %i\n", powerLimit);

	result = nvmlDeviceGetPowerManagementLimitConstraints(currentGPU, &minPowerLimit, &maxPowerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	result = nvmlDeviceGetPowerManagementDefaultLimit(currentGPU, &defaultPowerLimit);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Min Power Limit: %i - Max Power Limit %i - Default Power Limit %i\n", minPowerLimit, maxPowerLimit, defaultPowerLimit);


	result = nvmlDeviceGetMemoryInfo(currentGPU, &memory);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Memory usage - total: %i - free: %i - used: %i\n", memory.total, memory.used, memory.free);
	//Com_Printf("(%i bytes used of %i bytes)\n", memory.used, memory.total);

	Com_Printf("Current clocks - ");
	result = nvmlDeviceGetClockInfo(currentGPU, NVML_CLOCK_GRAPHICS, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	else {
		Com_Printf("graphics: %i - ", clock);
	}

	result = nvmlDeviceGetClockInfo(currentGPU, NVML_CLOCK_SM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	else {
		Com_Printf("SM: %i - ", clock);
	}

	result = nvmlDeviceGetClockInfo(currentGPU, NVML_CLOCK_MEM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	else {
		Com_Printf("memory: %i - ", clock);
	}

	result = nvmlDeviceGetClockInfo(currentGPU, NVML_CLOCK_VIDEO, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	else {
		Com_Printf("video: %i", clock);
	}

	Com_Printf("\n");

	Com_Printf("Maximum customer boost clocks - ");
	result = nvmlDeviceGetMaxCustomerBoostClock(currentGPU, NVML_CLOCK_GRAPHICS, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		clock = 0;
		//return;
	}
	else {
		Com_Printf("graphics: %i - ", clock);
	}

	result = nvmlDeviceGetMaxCustomerBoostClock(currentGPU, NVML_CLOCK_SM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		clock = 0;
		//return;
	}
	else {
		Com_Printf("SM: %i - ", clock);
	}

	result = nvmlDeviceGetMaxCustomerBoostClock(currentGPU, NVML_CLOCK_MEM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		clock = 0;
		//return;
	}
	else {
		Com_Printf("memory: %i - ", clock);
	}

	result = nvmlDeviceGetMaxCustomerBoostClock(currentGPU, NVML_CLOCK_VIDEO, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		clock = 0;
		//return;
	}
	else {
		Com_Printf("video: %i", clock);
	}

	Com_Printf("\n");

	Com_Printf("Maximum clocks - ");
	result = nvmlDeviceGetMaxClockInfo(currentGPU, NVML_CLOCK_GRAPHICS, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("graphics: %i - ", clock);
	}

	result = nvmlDeviceGetMaxClockInfo(currentGPU, NVML_CLOCK_SM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("SM: %i - ", clock);
	}

	result = nvmlDeviceGetMaxClockInfo(currentGPU, NVML_CLOCK_MEM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("memory: %i - ", clock);
	}

	result = nvmlDeviceGetMaxClockInfo(currentGPU, NVML_CLOCK_VIDEO, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("video: %i", clock);
	}

	Com_Printf("\n");

	Com_Printf("Default application clocks - ");
	result = nvmlDeviceGetDefaultApplicationsClock(currentGPU, NVML_CLOCK_GRAPHICS, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("graphics: %i - ", clock);
	}

	result = nvmlDeviceGetDefaultApplicationsClock(currentGPU, NVML_CLOCK_SM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("SM: %i - ", clock);
	}

	result = nvmlDeviceGetDefaultApplicationsClock(currentGPU, NVML_CLOCK_MEM, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("memory: %i - ", clock);
	}

	result = nvmlDeviceGetDefaultApplicationsClock(currentGPU, NVML_CLOCK_VIDEO, &clock);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
	}
	else {
		Com_Printf("video: %i", clock);
	}

	Com_Printf("\n");

	/*result = nvmlDeviceGetMemoryInfo(currentGPU, &memory);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}*/
}

void Sys_NVMLInit(void) {
	nvmlReturn_t result = nvmlInit();

	Com_Printf("^6nvmlInit %s! ", result == NVML_SUCCESS ? "success!" : "failed");

	if (result != NVML_SUCCESS) {
		nvmlSupported = qfalse;
		Com_Printf("nvmlErrorString: %s\n", nvmlErrorString(result));
		return;
	}

	nvmlSupported = qtrue;
	Com_Printf("NVML_API_VERSION %s\n", NVML_API_VERSION_STR);

	result = nvmlDeviceGetCount(&NVML_DeviceCount);

	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}

	Sys_NVMLListDevices();
	//Com_Printf("\n\n");

	if (!nvmlDeviceIndex)
		nvmlDeviceIndex = Cvar_Get("nvmlDeviceIndex", "0", CVAR_TEMP, "Sets index for NVML API to use");

	if (nvmlDeviceIndex->integer < 0 || nvmlDeviceIndex->integer >= NVML_DeviceCount) {
		//Sys_NVMLListDevices(deviceCount);
		return;
	}

	Sys_NVMLSetDeviceHandle(nvmlDeviceIndex->integer);
	//should probably be in the above function but w/e
	result = nvmlDeviceGetName(currentGPU, GPUName, NVML_DEVICE_NAME_BUFFER_SIZE);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		nvmlSupported = qfalse;
		return;
	}
	Com_Printf("Selected NVML device %i: %s\n", nvmlDeviceIndex->integer, GPUName);

	nvmlUtilization_t util = { 0 };
	result = nvmlDeviceGetUtilizationRates(currentGPU, &util);
	if (result != NVML_SUCCESS) {
		Com_Printf("nvmlErrorString: %s \n", nvmlErrorString(result));
		return;
	}
	Com_Printf("Utilization: gpu: %i - memory: %i\n", util.gpu, util.memory);
	Com_Printf(S_COLOR_GREEN "Use command /NVMLStatus for current device information\n");
}
#endif

/*
================
Sys_SteamInit

Steam initialization is done here.
In order for Steam to work, two things are needed:
- steam_api.dll (not included with retail Jedi Academy or Jedi Outcast!)
- steam_appid.txt (likewise)
steam_appid.txt is a text file containing either "6020" or "6030".
These correspond to Jedi Academy and Jedi Outcast, respectively.

Steamworks SDK is required to use the playtime tracking and overlay features
without launching the app manually through Steam.
Unfortunately, the SDK does not play nice with copyleft licenses.
Fortunately! we can invoke the library directly and avoid this entirely,
provided the end-user has the goods.
Unfortunately! this is platform specific and so we have to do it here.
================
*/
#define STEAMCLIENT_INTERFACE_VERSION "SteamClient018"

typedef bool(__stdcall* SteamAPIInit_Type)();
typedef void(__stdcall* SteamAPIShutdown_Type)();
static SteamAPIInit_Type SteamAPI_Init;
static SteamAPIShutdown_Type SteamAPI_Shutdown;

typedef bool(__thiscall *SteamAPISetRichPresence_Type)(void *GetFriends, const char *pchKey, const char *pchValue);
typedef void(__stdcall *SteamAPIClearRichPresence_Type)();

typedef void*(__cdecl *SteamAPISteamClient_Type)();
typedef __int32(__thiscall *SteamAPI_GetHSteamPipe_Type)();
typedef __int32(__thiscall *SteamAPI_GetHSteamUser_Type)();
typedef void*(__thiscall *SteamAPIGetSteamFriends_Type)(void *SteamClient, int hSteamUser, int hSteamPipe, char *InterfaceVersion);

static SteamAPISetRichPresence_Type SteamAPI_SetRichPresence;
static SteamAPIClearRichPresence_Type SteamAPI_ClearRichPresence;

static SteamAPISteamClient_Type SteamAPI_SteamClient;
static SteamAPI_GetHSteamPipe_Type SteamAPI_GetSteamPipe;
static SteamAPI_GetHSteamUser_Type SteamAPI_GetSteamUser;
static SteamAPIGetSteamFriends_Type SteamAPI_GetSteamFriends;

static qboolean SteamRichPresenceSupported = qfalse;
static int steamPipe;
static int steamUser;

static void* gp_steamLibrary = nullptr;

void Sys_SteamInit()
{
	if (!Cvar_VariableIntegerValue("com_steamIntegration"))
	{
		// Don't do anything if com_steamIntegration is disabled
		return;
	}

#ifndef FINAL_BUILD
	return;
#endif

	// Load the library
	gp_steamLibrary = Sys_LoadLibrary("steam_api" DLL_EXT);
	if (!gp_steamLibrary)
	{
		Com_Printf(S_COLOR_RED "Steam integration failed: Couldn't find steam_api" DLL_EXT "\n");
		return;
	}

	// Load the functions
	SteamAPI_Init = (SteamAPIInit_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_Init");
	SteamAPI_Shutdown = (SteamAPIShutdown_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_Shutdown");

	if (!SteamAPI_Shutdown || !SteamAPI_Init)
	{
		Com_Printf(S_COLOR_RED "Steam integration failed: Library invalid\n");
		Sys_UnloadLibrary(gp_steamLibrary);
		gp_steamLibrary = nullptr;
		return;
	}

	// Finally, call the init function in Steam, which should pop up the overlay if everything went correctly
	if (!SteamAPI_Init())
	{
		Com_Printf(S_COLOR_RED "Steam integration failed: Steam init failed. Ensure steam_appid.txt exists and is valid.\n");
		Sys_UnloadLibrary(gp_steamLibrary);
		gp_steamLibrary = nullptr;
		return;
	}

	SteamAPI_SetRichPresence = (SteamAPISetRichPresence_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_ISteamFriends_SetRichPresence");
	SteamAPI_ClearRichPresence = (SteamAPIClearRichPresence_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_ISteamFriends_ClearRichPresence");

	SteamAPI_SteamClient = (SteamAPISteamClient_Type)Sys_LoadFunction(gp_steamLibrary, "SteamClient");
	SteamAPI_GetSteamPipe = (SteamAPI_GetHSteamPipe_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_GetHSteamPipe");
	SteamAPI_GetSteamUser = (SteamAPI_GetHSteamUser_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_GetHSteamUser");
	SteamAPI_GetSteamFriends = (SteamAPIGetSteamFriends_Type)Sys_LoadFunction(gp_steamLibrary, "SteamAPI_ISteamClient_GetISteamFriends");


	if (SteamAPI_SteamClient && SteamAPI_GetSteamPipe && SteamAPI_GetSteamUser && SteamAPI_GetSteamFriends && SteamAPI_SetRichPresence && SteamAPI_ClearRichPresence)
	{
		return;//dunno....
		Com_Printf(S_COLOR_GREY "Steam rich presence integration supported.\n");
		steamPipe = SteamAPI_GetSteamPipe();
		Com_Printf(S_COLOR_GREY "Got Steam communication pipe %i\n", steamPipe);
		steamUser = SteamAPI_GetSteamUser();
		Com_Printf(S_COLOR_GREY "Connected to global Steam user %i\n", steamUser);
		SteamRichPresenceSupported = qtrue;
	}
	else
	{
		Com_Printf(S_COLOR_RED "Steam rich presence integration not supported by loaded steam_api.dll.\n");
	}
}

void Sys_SteamUpdateRichPresence(const char* pchKey, const char* pchValue, qboolean shutdown)
{
	void *SteamClient = NULL;
	void *GetFriends = NULL;
	//void *SteamClient = SteamAPI_SteamClient();
	//void *GetFriends = SteamAPI_GetSteamFriends(SteamClient);

	if (!SteamRichPresenceSupported)
		return;

	SteamClient = SteamAPI_SteamClient();
	SteamClient;
	GetFriends = SteamAPI_GetSteamFriends(SteamClient, steamUser, steamPipe, (char *)STEAMCLIENT_INTERFACE_VERSION);

	if (shutdown) {
		SteamAPI_ClearRichPresence();
		return;
	}

	if (!pchKey || !pchKey[0] || !pchValue || !pchValue[0]) {
		Com_Printf(S_COLOR_RED "Sys_SteamUpdateRichPresence: Tried to update with null perameter!\n");
		return;
	}

	if (!SteamAPI_SetRichPresence(GetFriends, pchKey, pchValue))
	{
		Com_Printf(S_COLOR_RED "SteamAPI_SetRichPresence Failed?\n");
	}
}

/*
================
Sys_SteamShutdown

Platform-specific exit code
================
*/
void Sys_SteamShutdown()
{
	if (!gp_steamLibrary)
	{
		Com_Printf("Skipping Steam integration shutdown...\n");
		return;
	}

	SteamAPI_Shutdown();
	Sys_UnloadLibrary(gp_steamLibrary);
	gp_steamLibrary = nullptr;
}