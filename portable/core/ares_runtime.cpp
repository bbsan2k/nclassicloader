// Minimal ares runtime composition. Upstream implementation files remain under
// Copyright (c) 2004-2025 ares team, Near et al; ISC license (ares LICENSE).
#include <ares/ares.hpp>
#include <ares/debug/debug.cpp>
#include <nall/gdb/server.cpp>
#include <ares/node/node.cpp>

namespace ares {
Platform* platform = nullptr;
atomic<bool> _runAhead = false;
const string Name = "ares";
const string Version = CLOVER_ARES_REVISION;
const string Copyright = "Copyright (c) 2004-2025 ares team, Near et al";
const string License = "ISC";
const string LicenseURI = "https://opensource.org/licenses/ISC";
const string Website = "ares-emu.net";
const string WebsiteURI = "https://ares-emu.net/";
const u32 SerializerSignature = 0x31545342;
}
