//! Read only the selected Clover data from supported stock root filesystems.
//!
//! Archives are never extracted to disk. Tar links are ignored, SquashFS
//! symlinks/devices are not followed, and all paths, members, and expansion are
//! bounded before allocation. Boot images are recognized but never unpacked.

use flate2::bufread::{GzDecoder, ZlibDecoder};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom},
    path::Path,
};

const MIB: u64 = 1024 * 1024;
const ARCHIVE_LIMIT: u64 = 1024 * MIB;
const EXPANDED_LIMIT: u64 = 512 * MIB;
const FILE_LIMIT: u64 = 64 * MIB;
const MEMBER_LIMIT: usize = 100_000;
const PATH_LIMIT: usize = 4096;

pub type Files = BTreeMap<String, Vec<u8>>;
type ReadResult<T> = Result<T, String>;

fn fail(message: impl Into<String>) -> String {
    format!("Image reader: {}", message.into())
}

fn require(test: bool, message: impl Into<String>) -> ReadResult<()> {
    if test { Ok(()) } else { Err(fail(message)) }
}

fn u16le(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}

fn u32le(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes(b[p..p + 4].try_into().expect("validated u32"))
}

fn u64le(b: &[u8], p: usize) -> u64 {
    u64::from_le_bytes(b[p..p + 8].try_into().expect("validated u64"))
}

struct Source {
    file: File,
    size: u64,
}

impl Source {
    fn open(path: &Path) -> ReadResult<Self> {
        let file =
            File::open(path).map_err(|e| fail(format!("cannot open {}: {e}", path.display())))?;
        let size = file
            .metadata()
            .map_err(|e| fail(format!("cannot determine image size: {e}")))?
            .len();
        require(size <= ARCHIVE_LIMIT, "archive exceeds 1 GiB limit")?;
        Ok(Self { file, size })
    }

    fn read(&mut self, offset: u64, length: usize) -> ReadResult<Vec<u8>> {
        require(
            offset <= self.size && (length as u64) <= self.size - offset,
            "read outside image",
        )?;
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| fail(format!("image seek failed: {e}")))?;
        let mut bytes = vec![0; length];
        self.file
            .read_exact(&mut bytes)
            .map_err(|e| fail(format!("truncated or unreadable image: {e}")))?;
        Ok(bytes)
    }
}

fn normalize(path: &str) -> ReadResult<String> {
    require(path.len() <= PATH_LIMIT, "member path too long")?;
    require(!path.starts_with('/'), "absolute archive member path")?;
    let mut parts = Vec::new();
    for part in path.split('/') {
        require(part != "..", "parent traversal in archive member path")?;
        require(
            !part
                .chars()
                .any(|c| c.is_control() || c == '\\' || c == ':'),
            "unsafe archive member path",
        )?;
        if !part.is_empty() && part != "." {
            parts.push(part);
        }
    }
    Ok(parts.join("/"))
}

fn selected(path: &str) -> bool {
    if path == "usr/bin/canoe-shvc"
        || path == "etc/clover/VERSION"
        || path.starts_with("etc/clover/version/")
    {
        return true;
    }
    if path.starts_with("usr/share/ui/") || path.starts_with("usr/share/clover-ui/") {
        return true;
    }
    path.starts_with("usr/share/games/")
        && [".desktop", ".png", ".sfrom", ".nes", ".qd", ".fnt"]
            .iter()
            .any(|suffix| path.ends_with(suffix))
}

#[derive(Default)]
struct Collector {
    files: Files,
    folded: BTreeSet<String>,
    total: u64,
    members: usize,
}

impl Collector {
    fn member(&mut self) -> ReadResult<()> {
        self.members += 1;
        require(self.members <= MEMBER_LIMIT, "too many archive members")
    }

    fn account(&mut self, size: u64) -> ReadResult<()> {
        require(size <= FILE_LIMIT, "regular file exceeds 64 MiB limit")?;
        require(
            size <= EXPANDED_LIMIT - self.total,
            "expanded regular files exceed 512 MiB limit",
        )?;
        self.total += size;
        Ok(())
    }

    fn add(&mut self, path: String, bytes: Vec<u8>) -> ReadResult<()> {
        let key = path.to_ascii_lowercase();
        require(
            self.folded.insert(key),
            format!("duplicate or case-colliding selected file: {path}"),
        )?;
        self.files.insert(path, bytes);
        Ok(())
    }
}

fn inflate_block(input: &[u8], maximum: usize) -> ReadResult<Vec<u8>> {
    let mut decoder = ZlibDecoder::new(Cursor::new(input));
    let mut result = Vec::with_capacity(maximum.min(8192));
    let mut buffer = [0; 4096];
    loop {
        let n = decoder
            .read(&mut buffer)
            .map_err(|e| fail(format!("invalid zlib block: {e}")))?;
        if n == 0 {
            break;
        }
        require(
            result.len().saturating_add(n) <= maximum,
            "oversized SquashFS zlib block",
        )?;
        result.extend_from_slice(&buffer[..n]);
    }
    require(result.len() <= maximum, "oversized SquashFS zlib block")?;
    let input = decoder.into_inner();
    require(
        input.position() == input.get_ref().len() as u64,
        "trailing data in SquashFS zlib block",
    )?;
    Ok(result)
}

fn decode_block(input: &[u8], maximum: usize, compression: u16) -> ReadResult<Vec<u8>> {
    if compression == 1 {
        return inflate_block(input, maximum);
    }
    decode_xz_block(input, maximum)
}

#[cfg(not(feature = "xz"))]
fn decode_xz_block(_input: &[u8], _maximum: usize) -> ReadResult<Vec<u8>> {
    Err(fail(
        "XZ-compressed filesystems require the xz feature (enabled by default in desktop builds)",
    ))
}

#[cfg(feature = "xz")]
fn decode_xz_block(input: &[u8], maximum: usize) -> ReadResult<Vec<u8>> {
    // Bound both liblzma's dictionary allocation and decompressed output. A
    // single SquashFS block must contain exactly one complete XZ stream.
    let stream = xz2::stream::Stream::new_stream_decoder(64 * 1024 * 1024, 0)
        .map_err(|e| fail(format!("Cannot initialize XZ decoder: {e}")))?;
    let mut decoder = xz2::bufread::XzDecoder::new_stream(Cursor::new(input), stream);
    let mut output = Vec::new();
    (&mut decoder)
        .take(maximum as u64 + 1)
        .read_to_end(&mut output)
        .map_err(|e| fail(format!("Invalid SquashFS XZ block: {e}")))?;
    require(output.len() <= maximum, "oversized SquashFS XZ block")?;
    require(
        decoder.total_in() == input.len() as u64,
        "trailing data in SquashFS XZ block",
    )?;
    Ok(output)
}

#[derive(Clone)]
struct Metadata {
    bytes: Vec<u8>,
    next: u64,
}

#[derive(Clone, Copy)]
struct CursorRef {
    block: u64,
    offset: usize,
}

struct Squash<'a> {
    source: &'a mut Source,
    out: Collector,
    used: u64,
    inode_table: u64,
    directory_table: u64,
    fragment_table: u64,
    block_size: usize,
    fragment_count: u32,
    compression: u16,
    metadata: BTreeMap<u64, Metadata>,
    metadata_size: usize,
    visited_directories: BTreeSet<u64>,
}

impl<'a> Squash<'a> {
    fn read(&mut self, offset: u64, length: usize) -> ReadResult<Vec<u8>> {
        require(
            offset <= self.used && (length as u64) <= self.used - offset,
            "SquashFS offset outside bytes_used",
        )?;
        self.source.read(offset, length)
    }

    fn meta(&mut self, offset: u64) -> ReadResult<Metadata> {
        if let Some(value) = self.metadata.get(&offset) {
            return Ok(value.clone());
        }
        let header = self.read(offset, 2)?;
        let value = u16le(&header, 0);
        let length = usize::from(value & 0x7fff);
        require(
            (1..=8192).contains(&length),
            "invalid SquashFS metadata length",
        )?;
        let raw = self.read(offset + 2, length)?;
        let bytes = if value & 0x8000 == 0 {
            decode_block(&raw, 8192, self.compression)?
        } else {
            raw
        };
        require(!bytes.is_empty(), "empty SquashFS metadata block")?;
        self.metadata_size = self.metadata_size.saturating_add(bytes.len());
        require(
            self.metadata_size <= 64 * 1024 * 1024,
            "SquashFS metadata exceeds 64 MiB limit",
        )?;
        let meta = Metadata {
            bytes,
            next: offset + 2 + length as u64,
        };
        self.metadata.insert(offset, meta.clone());
        Ok(meta)
    }

    fn cursor(table: u64, reference: u64) -> ReadResult<CursorRef> {
        let block = reference >> 16;
        require(
            table.checked_add(block).is_some(),
            "SquashFS inode reference overflow",
        )?;
        Ok(CursorRef {
            block: table + block,
            offset: (reference & 0xffff) as usize,
        })
    }

    fn take(&mut self, cursor: &mut CursorRef, mut count: usize) -> ReadResult<Vec<u8>> {
        let mut result = Vec::with_capacity(count);
        while count != 0 {
            let meta = self.meta(cursor.block)?;
            require(cursor.offset <= meta.bytes.len(), "invalid metadata cursor")?;
            if cursor.offset == meta.bytes.len() {
                require(meta.next > cursor.block, "metadata cursor did not advance")?;
                cursor.block = meta.next;
                cursor.offset = 0;
                continue;
            }
            let n = count.min(meta.bytes.len() - cursor.offset);
            result.extend_from_slice(&meta.bytes[cursor.offset..cursor.offset + n]);
            cursor.offset += n;
            count -= n;
        }
        Ok(result)
    }

    fn data_block(&mut self, start: u64, descriptor: u32) -> ReadResult<Vec<u8>> {
        require(
            descriptor & 0xfe00_0000 == 0,
            "invalid SquashFS data block flags",
        )?;
        let size = (descriptor & 0x00ff_ffff) as usize;
        require(
            size > 0 && size <= self.block_size,
            "invalid SquashFS data block size",
        )?;
        let data = self.read(start, size)?;
        if descriptor & 0x0100_0000 == 0 {
            decode_block(&data, self.block_size, self.compression)
        } else {
            Ok(data)
        }
    }

    fn file(
        &mut self,
        cursor: &mut CursorRef,
        mut start: u64,
        size: u64,
        fragment: u32,
        fragment_offset: u32,
    ) -> ReadResult<Vec<u8>> {
        require(size <= FILE_LIMIT, "selected file exceeds 64 MiB limit")?;
        let size_usize = usize::try_from(size)
            .map_err(|_| fail("file size does not fit memory address space"))?;
        let mut result = Vec::with_capacity(size_usize);
        let count = size / self.block_size as u64
            + u64::from(fragment == u32::MAX && !size.is_multiple_of(self.block_size as u64));
        for _ in 0..count {
            let word = self.take(cursor, 4)?;
            let descriptor = u32le(&word, 0);
            let expected = self.block_size.min(size_usize - result.len());
            if descriptor == 0 {
                result.resize(result.len() + expected, 0);
            } else {
                let block = self.data_block(start, descriptor)?;
                require(
                    block.len() == expected,
                    "wrong SquashFS data block expansion size",
                )?;
                result.extend_from_slice(&block);
                start = start
                    .checked_add(u64::from(descriptor & 0x00ff_ffff))
                    .ok_or_else(|| fail("SquashFS file offset overflow"))?;
            }
        }
        if fragment != u32::MAX && !size.is_multiple_of(self.block_size as u64) {
            require(
                fragment < self.fragment_count,
                "invalid SquashFS fragment index",
            )?;
            let index = u64::from(fragment) * 16;
            let pointer = self.read(self.fragment_table + (index / 8192) * 8, 8)?;
            let mut f = CursorRef {
                block: u64le(&pointer, 0),
                offset: (index % 8192) as usize,
            };
            let record = self.take(&mut f, 16)?;
            let block = self.data_block(u64le(&record, 0), u32le(&record, 8))?;
            let tail = (size % self.block_size as u64) as usize;
            let at = fragment_offset as usize;
            require(
                at <= block.len() && tail <= block.len() - at,
                "SquashFS fragment outside block",
            )?;
            result.extend_from_slice(&block[at..at + tail]);
        }
        require(result.len() == size_usize, "wrong SquashFS file size")?;
        Ok(result)
    }

    fn visit(&mut self, reference: u64, path: &str, depth: usize) -> ReadResult<()> {
        require(depth <= 128, "SquashFS directory nesting exceeds limit")?;
        self.out.member()?;
        let mut cursor = Self::cursor(self.inode_table, reference)?;
        let base = self.take(&mut cursor, 16)?;
        let kind = u16le(&base, 0);
        require((1..=14).contains(&kind), "unknown SquashFS inode type")?;
        if kind == 1 || kind == 8 {
            require(
                self.visited_directories.insert(reference),
                "cyclic or aliased SquashFS directory",
            )?;
            let inode = self.take(&mut cursor, if kind == 1 { 16 } else { 24 })?;
            let start = u64::from(u32le(&inode, if kind == 1 { 0 } else { 8 }));
            let size = if kind == 1 {
                u64::from(u16le(&inode, 8))
            } else {
                u64::from(u32le(&inode, 4))
            };
            let offset = usize::from(u16le(&inode, if kind == 1 { 10 } else { 18 }));
            require(
                (3..=FILE_LIMIT).contains(&size),
                "invalid SquashFS directory size",
            )?;
            let mut directory_cursor = CursorRef {
                block: self.directory_table + start,
                offset,
            };
            let bytes = self.take(&mut directory_cursor, size as usize - 3)?;
            let mut pos = 0;
            let mut names = BTreeSet::new();
            while pos < bytes.len() {
                require(
                    bytes.len() - pos >= 12,
                    "truncated SquashFS directory header",
                )?;
                let count = u64::from(u32le(&bytes, pos)) + 1;
                let block = u32le(&bytes, pos + 4);
                require(count <= 256, "invalid SquashFS directory entry count")?;
                pos += 12;
                for _ in 0..count {
                    require(bytes.len() - pos >= 8, "truncated SquashFS directory entry")?;
                    let inode_offset = u16le(&bytes, pos);
                    let name_length = usize::from(u16le(&bytes, pos + 6)) + 1;
                    pos += 8;
                    require(
                        name_length <= 256 && name_length <= bytes.len() - pos,
                        "invalid SquashFS name size",
                    )?;
                    let name = std::str::from_utf8(&bytes[pos..pos + name_length])
                        .map_err(|_| fail("non-UTF8 SquashFS name"))?
                        .to_owned();
                    pos += name_length;
                    require(
                        !name.is_empty() && name != "." && !name.contains('/'),
                        "invalid SquashFS directory name",
                    )?;
                    require(
                        names.insert(name.clone()),
                        "duplicate SquashFS directory entry",
                    )?;
                    let child_path = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{path}/{name}")
                    };
                    let child = normalize(&child_path)?;
                    self.visit(
                        (u64::from(block) << 16) | u64::from(inode_offset),
                        &child,
                        depth + 1,
                    )?;
                }
            }
        } else if kind == 2 || kind == 9 {
            let inode = self.take(&mut cursor, if kind == 2 { 16 } else { 40 })?;
            let start = if kind == 2 {
                u64::from(u32le(&inode, 0))
            } else {
                u64le(&inode, 0)
            };
            let size = if kind == 2 {
                u64::from(u32le(&inode, 12))
            } else {
                u64le(&inode, 8)
            };
            let fragment = u32le(&inode, if kind == 2 { 4 } else { 28 });
            let fragment_offset = u32le(&inode, if kind == 2 { 8 } else { 32 });
            self.out.account(size)?;
            if selected(path) {
                let bytes = self.file(&mut cursor, start, size, fragment, fragment_offset)?;
                self.out.add(path.to_owned(), bytes)?;
            }
        }
        // Symlinks, devices, FIFOs, sockets, and unknown non-file inodes are not followed.
        Ok(())
    }

    fn run(mut self, superblock: &[u8]) -> ReadResult<Files> {
        require(
            u32le(superblock, 0) == 0x7371_7368,
            "not little-endian SquashFS",
        )?;
        require(
            u16le(superblock, 28) == 4 && u16le(superblock, 30) == 0,
            "only SquashFS v4.0 is supported",
        )?;
        require(
            matches!(u16le(superblock, 20), 1 | 4),
            "SquashFS compression must be gzip/zlib (1) or XZ (4)",
        )?;
        self.compression = u16le(superblock, 20);
        let block_size = u32le(superblock, 12);
        let log = u16le(superblock, 22);
        require(
            (4096..=1_048_576).contains(&block_size) && log < 32 && (1u32 << log) == block_size,
            "invalid SquashFS block size",
        )?;
        self.block_size = block_size as usize;
        self.used = u64le(superblock, 40);
        require(
            self.used >= 96 && self.used <= self.source.size,
            "invalid SquashFS bytes_used",
        )?;
        self.inode_table = u64le(superblock, 64);
        self.directory_table = u64le(superblock, 72);
        self.fragment_table = u64le(superblock, 80);
        self.fragment_count = u32le(superblock, 16);
        require(
            self.inode_table >= 96
                && self.inode_table < self.used
                && self.directory_table >= 96
                && self.directory_table < self.used,
            "invalid SquashFS table offset",
        )?;
        if self.fragment_count != 0 {
            let pointers = (u64::from(self.fragment_count) * 16).div_ceil(8192);
            require(
                self.fragment_table <= self.used && pointers * 8 <= self.used - self.fragment_table,
                "invalid fragment table",
            )?;
        }
        self.visit(u64le(superblock, 32), "", 0)?;
        Ok(self.out.files)
    }
}

struct Gzip<R: BufRead> {
    decoder: GzDecoder<R>,
    expanded: u64,
}

impl<R: BufRead> Gzip<R> {
    fn read(&mut self, bytes: &mut [u8]) -> ReadResult<usize> {
        let n = self
            .decoder
            .read(bytes)
            .map_err(|e| fail(format!("invalid or truncated gzip stream: {e}")))?;
        self.expanded = self
            .expanded
            .checked_add(n as u64)
            .ok_or_else(|| fail("gzip expansion overflow"))?;
        require(
            self.expanded <= EXPANDED_LIMIT,
            "gzip expansion exceeds 512 MiB limit",
        )?;
        Ok(n)
    }

    fn exact(&mut self, bytes: &mut [u8], message: &str) -> ReadResult<()> {
        let mut at = 0;
        while at < bytes.len() {
            let n = self.read(&mut bytes[at..])?;
            require(n != 0, message)?;
            at += n;
        }
        Ok(())
    }

    fn take(&mut self, length: usize) -> ReadResult<Vec<u8>> {
        let mut bytes = vec![0; length];
        self.exact(&mut bytes, "truncated tar archive")?;
        Ok(bytes)
    }

    fn skip(&mut self, mut length: u64, message: &str) -> ReadResult<()> {
        let mut buffer = [0; 64 * 1024];
        while length != 0 {
            let n = (length as usize).min(buffer.len());
            self.exact(&mut buffer[..n], message)?;
            length -= n as u64;
        }
        Ok(())
    }

    fn finish_zeroes(mut self) -> ReadResult<()> {
        let mut buffer = [0; 64 * 1024];
        loop {
            let n = self.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            require(
                buffer[..n].iter().all(|&b| b == 0),
                "nonzero data after tar end markers",
            )?;
        }
        let mut input = self.decoder.into_inner();
        require(
            input
                .fill_buf()
                .map_err(|e| fail(format!("gzip input read failed: {e}")))?
                .is_empty(),
            "trailing data or concatenated gzip streams are unsupported",
        )
    }
}

fn octal(bytes: &[u8]) -> ReadResult<u64> {
    let mut result = 0u64;
    let mut trailing = false;
    let mut digit = false;
    for &c in bytes {
        if c == 0 || c == b' ' {
            if digit || c == 0 {
                trailing = true;
            }
            continue;
        }
        require(
            !trailing && (b'0'..=b'7').contains(&c),
            "invalid tar octal field (base-256 unsupported)",
        )?;
        result = result
            .checked_mul(8)
            .and_then(|v| v.checked_add(u64::from(c - b'0')))
            .ok_or_else(|| fail("tar numeric overflow"))?;
        digit = true;
    }
    Ok(result)
}

fn field(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

fn tar(mut source: Source) -> ReadResult<Files> {
    source
        .file
        .seek(SeekFrom::Start(0))
        .map_err(|e| fail(format!("image seek failed: {e}")))?;
    let reader = BufReader::new(source.file);
    let mut gz = Gzip {
        decoder: GzDecoder::new(reader),
        expanded: 0,
    };
    let mut out = Collector::default();
    let mut long_name = String::new();
    loop {
        let header = gz.take(512)?;
        if header.iter().all(|&b| b == 0) {
            let second = gz.take(512)?;
            require(
                second.iter().all(|&b| b == 0),
                "missing second tar end marker",
            )?;
            require(long_name.is_empty(), "orphan tar long-name record")?;
            gz.finish_zeroes()?;
            return Ok(out.files);
        }
        out.member()?;
        let sum: u64 = header
            .iter()
            .enumerate()
            .map(|(i, &b)| u64::from(if (148..156).contains(&i) { b' ' } else { b }))
            .sum();
        require(
            sum == octal(&header[148..156])?,
            "invalid tar header checksum",
        )?;
        let size = octal(&header[124..136])?;
        require(size <= FILE_LIMIT, "tar member exceeds 64 MiB limit")?;
        let kind = header[156];
        let mut path = field(&header[0..100]);
        if &header[257..262] == b"ustar" && header[262] == 0 {
            let prefix = field(&header[345..500]);
            if !prefix.is_empty() {
                path = format!("{prefix}/{path}");
            }
        }
        if kind == b'L' {
            require(
                long_name.is_empty() && size > 0 && size <= (PATH_LIMIT + 1) as u64,
                "invalid GNU tar long name",
            )?;
            let value = gz.take(size as usize)?;
            require(value.last() == Some(&0), "unterminated GNU tar long name")?;
            long_name = field(&value);
            require(
                long_name.len() + 1 == value.len(),
                "embedded NUL in GNU tar long name",
            )?;
            normalize(&long_name)?;
        } else {
            if !long_name.is_empty() {
                path = std::mem::take(&mut long_name);
            }
            path = normalize(&path)?;
            require(
                !b"xgS".contains(&kind),
                "PAX and sparse tar extensions are unsupported; use a standard GNU/ustar archive",
            )?;
            if kind == 0 || kind == b'0' {
                require(!path.is_empty(), "empty tar regular-file path")?;
                out.account(size)?;
                if selected(&path) {
                    out.add(path, gz.take(size as usize)?)?;
                } else {
                    gz.skip(size, "truncated tar member")?;
                }
            } else {
                require(b"123456K".contains(&kind), "unsupported tar member type")?;
                gz.skip(size, "truncated tar member")?;
            }
        }
        gz.skip((512 - size % 512) % 512, "truncated tar member")?;
    }
}

fn squash(source: &mut Source) -> ReadResult<Files> {
    let superblock = source.read(0, 96)?;
    let fs = Squash {
        source,
        out: Collector::default(),
        used: 0,
        inode_table: 0,
        directory_table: 0,
        fragment_table: 0,
        block_size: 0,
        fragment_count: 0,
        compression: 1,
        metadata: BTreeMap::new(),
        metadata_size: 0,
        visited_directories: BTreeSet::new(),
    };
    fs.run(&superblock)
}

/// Read selected files from a stock SquashFS v4 or gzip/ustar image.
pub fn read_image(path: &Path) -> ReadResult<Files> {
    let mut source = Source::open(path)?;
    require(source.size >= 8, "image is too short")?;
    let magic = source.read(0, 8)?;
    if &magic == b"ANDROID!" {
        return Err(fail(
            "Android boot .img contains the kernel/boot ramdisk, not the game root filesystem. Select the matching .hsqs or .tar.gz rootfs (or place it beside this .img for the importer to resolve).",
        ));
    }
    if u32le(&magic, 0) == 0x7371_7368 {
        return squash(&mut source);
    }
    if magic[0] == 0x1f && magic[1] == 0x8b {
        return tar(source);
    }
    Err(fail(
        "unsupported image; select a SquashFS .hsqs or gzip .tar.gz root filesystem",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "xz")]
    #[test]
    fn xz_blocks_are_bounded_and_require_complete_single_streams() {
        use std::io::Write;
        let mut encoder = xz2::write::XzEncoder::new(Vec::new(), 1);
        encoder.write_all(&vec![42; 16384]).unwrap();
        let bytes = encoder.finish().unwrap();
        assert_eq!(decode_block(&bytes, 16384, 4).unwrap(), vec![42; 16384]);
        assert!(decode_block(&bytes, 1024, 4).is_err());
        assert!(decode_block(&bytes[..bytes.len() - 1], 16384, 4).is_err());
        let mut trailing = bytes.clone();
        trailing.extend_from_slice(&bytes);
        assert!(decode_block(&trailing, 16384, 4).is_err());
    }
    use flate2::{Compression, write::GzEncoder};
    use std::{
        fs,
        io::Write,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temporary(name: &str) -> std::path::PathBuf {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("clover-{name}-{now}"))
    }

    fn tar_header(name: &str, bytes: &[u8]) -> [u8; 512] {
        let mut h = [0u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..108].copy_from_slice(b"0000644\0");
        h[108..116].copy_from_slice(b"0000000\0");
        h[116..124].copy_from_slice(b"0000000\0");
        let size = format!("{:011o}\0", bytes.len());
        h[124..136].copy_from_slice(size.as_bytes());
        h[136..148].copy_from_slice(b"00000000000\0");
        h[148..156].fill(b' ');
        h[156] = b'0';
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let checksum: u64 = h.iter().map(|&b| u64::from(b)).sum();
        let check = format!("{:06o}\0 ", checksum);
        h[148..156].copy_from_slice(check.as_bytes());
        h
    }

    fn tar_gz(entries: &[(&str, &[u8])], trailing: &[u8]) -> Vec<u8> {
        let mut raw = Vec::new();
        for (name, contents) in entries {
            raw.extend_from_slice(&tar_header(name, contents));
            raw.extend_from_slice(contents);
            raw.resize(raw.len() + (512 - contents.len() % 512) % 512, 0);
        }
        raw.extend([0; 1024]);
        raw.extend_from_slice(trailing);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&raw).unwrap();
        encoder.finish().unwrap()
    }

    fn write_image(contents: &[u8]) -> std::path::PathBuf {
        let path = temporary("image");
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn selects_only_clover_files_without_extracting() {
        let target = b"desktop metadata";
        let image = tar_gz(
            &[
                ("usr/share/games/CLV-P-TEST/CLV-P-TEST.desktop", target),
                ("usr/share/games/CLV-P-TEST/CLV-P-TEST.png", b"art"),
                ("usr/bin/ignored", b"do not retain"),
            ],
            &[],
        );
        let path = write_image(&image);
        let files = read_image(&path).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(
            files["usr/share/games/CLV-P-TEST/CLV-P-TEST.desktop"],
            target
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_path_traversal_and_nonzero_trailing_tar_data() {
        let traversal = write_image(&tar_gz(&[("../escape", b"bad")], &[]));
        assert!(
            read_image(&traversal)
                .unwrap_err()
                .contains("parent traversal")
        );
        fs::remove_file(traversal).unwrap();

        let trailing = write_image(&tar_gz(&[], &[1]));
        assert!(
            read_image(&trailing)
                .unwrap_err()
                .contains("nonzero data after tar end markers")
        );
        fs::remove_file(trailing).unwrap();
    }

    #[test]
    fn rejects_concatenated_gzip_streams() {
        let mut image = tar_gz(&[], &[]);
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(b"second member").unwrap();
        image.extend(encoder.finish().unwrap());
        let path = write_image(&image);
        assert!(
            read_image(&path)
                .unwrap_err()
                .contains("trailing data or concatenated gzip streams")
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn identifies_but_does_not_extract_boot_images() {
        let path = write_image(b"ANDROID!payload");
        assert!(
            read_image(&path)
                .unwrap_err()
                .contains("contains the kernel/boot ramdisk")
        );
        fs::remove_file(path).unwrap();
    }
}
