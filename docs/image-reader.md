# Rust image reader

`rust/src/image_reader.rs` exposes `read_image(&Path) -> Result<Files, String>`.
It reads selected image members into owned byte buffers without extracting the
archive to disk. Inputs are opened read-only. Links/devices are not followed,
paths and decompression are bounded, and console binaries/scripts remain inert
data. The importer separately hashes the source before and after processing.

## Supported containers

- Little-endian SquashFS v4 with zlib or XZ compression. Supports basic/extended
  directory and regular-file inodes, compressed/raw metadata and data, sparse
  files, multi-block files, and fragment tails. XZ decoder memory is capped at
  64 MiB and output is bounded by the SquashFS block size.
- A single gzip stream containing standard tar, including ustar prefixes and
  GNU long-name records. Header checksums, member sizes, gzip integrity, and
  both zero end markers are validated.
- Android `ANDROID!` boot images are rejected with guidance to select a matching
  `.hsqs` or `.tar.gz` filesystem. The importer resolves local companions; it
  does not execute the boot image.

Selected files are limited to Clover game metadata/art/ROM containers, UI
resources, version metadata, and the exact `usr/bin/canoe-shvc` byte carrier
used to locate the verified 64-byte SMP IPL. The executable itself is never
retained or run. NES `.nes`/`.qd` and legacy font containers are accepted as
selected data where the profile importer recognizes them.

## Safety limits and unsupported inputs

- Source image: 1 GiB maximum; each regular file: 64 MiB maximum; selected
  expanded regular files: 512 MiB maximum; 100,000 members maximum.
- Paths are capped at 4096 bytes. Absolute paths, `..`, control bytes, backslash,
  colon, duplicate paths, and ASCII case-collisions are rejected.
- Concatenated gzip streams, nonzero trailing tar data, PAX/sparse/base-256 tar,
  big-endian or older SquashFS, and unrecognized SquashFS compressors are not
  supported. Unsupported archive metadata is never interpreted as executable
  content.

The reader is intentionally a focused importer, not a general archive recovery
tool or a complete filesystem validator. Rust unit tests cover hostile paths,
limits, tar checksums/long names, gzip truncation/concatenation, XZ stream
boundaries, and SquashFS structures. Image-backed import checks are described
in `docs/rust-migration.md`.
