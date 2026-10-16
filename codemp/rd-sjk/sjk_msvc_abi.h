/*
rd-sjk: calls into C++ objects the engine made.

The engine (eternaljk.x86_64.exe) is built with MSVC and rd-sjk with MinGW. Both use
the same x64 calling convention, but their vtables differ when a class has a virtual
destructor: MSVC gives it one slot (the scalar deleting destructor), the Itanium ABI
two. IHeapAllocator (qcommon/MiniHeap.h) is such a class, and the G2 vertex heaps
(G2VertSpaceServer, G2VertSpaceClient) live in the engine, so their methods are
called here through MSVC's layout: [0] destructor, [1] ResetHeap, [2] MiniHeapAlloc.
*/
#pragma once

#include "qcommon/MiniHeap.h"

#if defined( __GNUC__ ) && defined( _WIN32 )
static inline void SJK_HeapReset( IHeapAllocator *heap ) {
	typedef void ( *ResetFn )( IHeapAllocator * );
	( *reinterpret_cast<ResetFn **>( heap ) )[1]( heap );
}
static inline char *SJK_HeapAlloc( IHeapAllocator *heap, int size ) {
	typedef char *( *AllocFn )( IHeapAllocator *, int );
	return ( *reinterpret_cast<AllocFn **>( heap ) )[2]( heap, size );
}
#else
static inline void SJK_HeapReset( IHeapAllocator *heap ) { heap->ResetHeap(); }
static inline char *SJK_HeapAlloc( IHeapAllocator *heap, int size ) { return heap->MiniHeapAlloc( size ); }
#endif
