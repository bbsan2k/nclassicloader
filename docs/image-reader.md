# Native image reader

`portable/import/image_reader.hpp` exposes:

```cpp
namespace clover::importer {
using Bytes = std::vector<std::uint8_t>;
using FileMap = std::map<std::string, Bytes>;
FileMap readImage(const std::filesystem::path& source);
}
```

The implementation is C++20 plus zlib, with no subprocesses, extraction directory,
runtime Python, filesystem writes, or execution of image contents. It opens the
source read-only and returns owned bytes keyed by normalized POSIX member paths.
Failures throw `std::runtime_error` with an `Image reader:` prefix. The caller
should surface the error and must not treat a failed import as partial success.

## Build integration

Compile `portable/import/image_reader.cpp` into the importer target. Add the
vendored zlib include directories and link its static target, typically
`zlibstatic` (or the target name provided by the parent build). Nothing beyond
the public `zlib.h` API is required. Do not compile `image_reader_tests.cpp` into
the application; it has its own `main`.

Example standalone verification using the system development zlib:

```sh
mkdir -p .build/image-reader-tests
c++ -std=c++20 -O2 -Wall -Wextra -Wpedantic \
  portable/import/image_reader.cpp portable/import/image_reader_tests.cpp \
  -lz -o .build/image-reader-tests/image_reader_tests
.build/image-reader-tests/image_reader_tests SNES-Mini-Kernels
```

Run from the workspace root. Without the directory argument, only self-authored
fixtures run. Tests write exclusively below `.build/image-reader-tests`; supplied
images are never changed. No Nintendo runtime is launched.

## Formats and selection

Detection uses file signatures, not filename extensions:

* Little-endian SquashFS v4.0, compression ID 1 (zlib). Supports basic/extended
  directory and regular-file inodes, compressed/uncompressed metadata and data,
  sparse blocks, multi-block files, and fragment tails. The supplied 128 KiB
  block images are supported; validated power-of-two blocks from 4 KiB through
  1 MiB are accepted. Full images may have padding beyond `bytes_used`.
* A single gzip stream containing standard tar, including ustar prefixes and
  GNU long-name records. Gzip integrity, tar header checksums, member sizes,
  and two zero end-of-archive blocks are checked. Zero tar padding is accepted.
* `ANDROID!` boot images are deliberately rejected with an actionable message:
  they contain a kernel/boot ramdisk rather than the game root filesystem.
  Matching sibling `.hsqs`/`.tar.gz` resolution belongs to the parent importer.

Only regular files in these locations are retained:

* `usr/share/games/**`: `.desktop`, `.png`, `.sfrom` (case-sensitive suffixes).
* `usr/share/ui/**`: all regular resource files, regardless of extension.
* `etc/clover/VERSION` and `etc/clover/version/**`.
* `usr/bin/canoe-shvc`, as a read-only byte carrier for verified SMP IPL discovery.
  The parent retains only the matched 64-byte firmware, never the executable.

Links, devices, sockets, FIFOs, and directories are not returned. No link target
is followed, including tar hardlinks. Files under UI/version are inert bytes,
even if their contents happen to be scripts. Consumers must not execute them.

## Safety limits and boundaries

* Source archive: at most 1 GiB; each regular file: at most 64 MiB.
* Sum of visited regular-file lengths: at most 512 MiB. Additionally the **entire**
  decompressed tar stream (including headers, skipped members, and padding) is
  limited to 512 MiB. SquashFS does not inflate unselected regular-file data.
* At most 100,000 visited members; paths at most 4096 bytes; SquashFS directory
  depth at most 128, directory data at most 64 MiB, cached metadata at most 64 MiB.
* Every source read is range-checked; SquashFS reads must also fit `bytes_used`.
  Metadata expansion is limited to 8192 bytes per block and data to block size.
* Absolute member paths and `..` components are rejected, not stripped. Leading
  `./`, internal `.` and redundant separators are normalized. Control bytes,
  backslashes, and colons are rejected to avoid cross-platform path confusion.
* Duplicate selected paths and ASCII-case-insensitive selected path collisions
  fail, rather than overwriting bytes. SquashFS directory aliases/cycles and
  duplicate directory names fail. `VERSION` and the `version` directory coexist
  because only regular-file paths are checked for selected-file collisions.

This is a focused importer, not a general archive recovery tool or filesystem
validator. Unsupported: big-endian/older SquashFS, other compressors, PAX tar,
GNU sparse/base-256 tar, concatenated gzip streams, nonzero trailing tar data,
and hardlink reconstruction. SquashFS ownership, permissions, xattrs, inode
numbers, export tables, and unselected file payloads are not interpreted or
fully validated. Unicode normalization/case folding and host filename rules
are not implemented; returned names are archive keys, not host output paths.
Consumers that later write files must separately enforce safe destination paths
and host-specific filename/collision policy. The source should not be modified
concurrently with an import. Returned payloads remain untrusted data.

## Verification performed

Self-authored tests cover gzip/tar filtering, GNU long names, checksums, gzip CRC
and truncation, traversal/absolute/cross-platform paths, case collisions,
unsupported PAX, basic/extended SquashFS inodes, compressed/raw metadata and data,
fragment tails, sparse files, invalid offsets, corrupt data, directory cycles,
file/archive limits, and actionable boot-image rejection.

All six supplied `.tar.gz` images were compared by exact key and byte equality
against all eleven matching full/trimmed `.hsqs` images. Each comparison returned
375 identical selected files after adding the IPL carrier. All six supplied Android boot images were checked
for actionable rejection. The complete suite also passed with AddressSanitizer
and UndefinedBehaviorSanitizer enabled. This is ordinary native parser/test observation,
not REA evidence; no firmware code or Nintendo runtime was executed.
