// Stand-in for the generated jofupdatekey.h: the check sets the trusted keys at
// run time, from a key it makes itself.
#pragma once

extern char g_updateTestKeys[];
#define JOF_UPDATE_PUBKEYS g_updateTestKeys
