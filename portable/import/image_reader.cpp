#include "image_reader.hpp"

#include <zlib.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <fstream>
#include <limits>
#include <set>
#include <stdexcept>
#include <string_view>
#include <utility>

namespace clover::importer {
namespace {
constexpr std::uint64_t MiB = 1024 * 1024;
constexpr std::uint64_t archiveLimit = 1024 * MiB;
constexpr std::uint64_t expandedLimit = 512 * MiB;
constexpr std::uint64_t fileLimit = 64 * MiB;
constexpr std::size_t memberLimit = 100000;
constexpr std::size_t pathLimit = 4096;
[[noreturn]] void fail(const std::string& text) {
    throw std::runtime_error("Image reader: " + text);
}
void require(bool condition, const std::string& text) {
    if (!condition) fail(text);
}
std::uint16_t u16(const std::uint8_t* p) {
    return std::uint16_t(p[0]) | (std::uint16_t(p[1]) << 8);
}
std::uint32_t u32(const std::uint8_t* p) {
    return std::uint32_t(u16(p)) | (std::uint32_t(u16(p + 2)) << 16);
}
std::uint64_t u64(const std::uint8_t* p) {
    return std::uint64_t(u32(p)) | (std::uint64_t(u32(p + 4)) << 32);
}
class Source {
    std::ifstream stream;
public:
    std::uint64_t size;
    explicit Source(const std::filesystem::path& path) : stream(path, std::ios::binary) {
        require(bool(stream), "cannot open " + path.string());
        stream.seekg(0, std::ios::end);
        const auto end = stream.tellg();
        require(end >= 0, "cannot determine image size");
        size = static_cast<std::uint64_t>(end);
        require(size <= archiveLimit, "archive exceeds 1 GiB limit");
    }
    Bytes read(std::uint64_t offset, std::uint64_t length) {
        require(offset <= size && length <= size - offset, "read outside image");
        Bytes data(static_cast<std::size_t>(length));
        stream.clear();
        stream.seekg(static_cast<std::streamoff>(offset));
        if (length) stream.read(reinterpret_cast<char*>(data.data()), static_cast<std::streamsize>(length));
        require(bool(stream), "truncated or unreadable image");
        return data;
    }
};

std::string normalize(std::string path) {
    require(path.size() <= pathLimit, "member path too long");
    require(path.empty() || path.front() != '/', "absolute archive member path");
    std::string result;
    std::size_t start = 0;
    while (start < path.size()) {
        auto end = path.find('/', start);
        if (end == std::string::npos) end = path.size();
        auto part = path.substr(start, end - start);
        require(part != "..", "parent traversal in archive member path");
        for (unsigned char c : part)
            require(c >= 32 && c != 127 && c != '\\' && c != ':', "unsafe archive member path");
        if (!part.empty() && part != ".") {
            if (!result.empty()) result += '/';
            result += part;
        }
        start = end + 1;
    }
    return result;
}
bool selected(const std::string& path) {
    // Read only as a byte carrier for the verified 64-byte SMP IPL; the
    // importer never publishes or executes this ARM/Linux executable.
    if (path == "usr/bin/canoe-shvc") return true;
    if (path == "etc/clover/VERSION" || path.starts_with("etc/clover/version/")) return true;
    if (path.starts_with("usr/share/ui/")) return true;
    if (!path.starts_with("usr/share/games/")) return false;
    return path.ends_with(".desktop") || path.ends_with(".png") || path.ends_with(".sfrom");
}
class Collector {
    std::set<std::string> folded;
    std::uint64_t total = 0;
public:
    FileMap files;
    std::size_t members = 0;
    void member() { require(++members <= memberLimit, "too many archive members"); }
    void account(std::uint64_t size) {
        require(size <= fileLimit, "regular file exceeds 64 MiB limit");
        require(size <= expandedLimit - total, "expanded regular files exceed 512 MiB limit");
        total += size;
    }
    void add(const std::string& path, Bytes data) {
        std::string key = path;
        for (char& c : key) if (c >= 'A' && c <= 'Z') c += 'a' - 'A';
        require(folded.insert(key).second, "duplicate or case-colliding selected file: " + path);
        files.emplace(path, std::move(data));
    }
};

Bytes inflateBlock(const Bytes& input, std::size_t maximum) {
    Bytes result(maximum);
    z_stream z{};
    require(inflateInit(&z) == Z_OK, "cannot initialize zlib");
    z.next_in = const_cast<Bytef*>(input.data());
    z.avail_in = static_cast<uInt>(input.size());
    z.next_out = result.data();
    z.avail_out = static_cast<uInt>(result.size());
    const int status = inflate(&z, Z_FINISH);
    const auto length = z.total_out;
    const bool valid = status == Z_STREAM_END && z.avail_in == 0;
    inflateEnd(&z);
    require(valid, "invalid or oversized SquashFS zlib block");
    result.resize(length);
    return result;
}

class Squash {
    Source& source;
    Collector out;
    std::uint64_t used = 0, inodeTable = 0, directoryTable = 0, fragmentTable = 0;
    std::uint32_t blockSize = 0, fragmentCount = 0;
    struct Metadata { Bytes bytes; std::uint64_t next; };
    std::map<std::uint64_t, Metadata> metadata;
    std::size_t metadataSize = 0;
    std::set<std::uint64_t> visitedDirectories;

    Bytes read(std::uint64_t offset, std::uint64_t length) {
        require(offset <= used && length <= used - offset, "SquashFS offset outside bytes_used");
        return source.read(offset, length);
    }
    const Metadata& meta(std::uint64_t offset) {
        if (auto it = metadata.find(offset); it != metadata.end()) return it->second;
        auto header = read(offset, 2);
        const auto value = u16(header.data());
        const auto length = value & 0x7fff;
        require(length > 0 && length <= 8192, "invalid SquashFS metadata length");
        auto bytes = read(offset + 2, length);
        if (!(value & 0x8000)) bytes = inflateBlock(bytes, 8192);
        require(!bytes.empty(), "empty SquashFS metadata block");
        metadataSize += bytes.size();
        require(metadataSize <= 64 * MiB, "SquashFS metadata exceeds 64 MiB limit");
        return metadata.emplace(offset, Metadata{std::move(bytes), offset + 2 + length}).first->second;
    }
    struct Cursor {
        Squash& fs;
        std::uint64_t block;
        std::size_t offset;
        Bytes take(std::size_t count) {
            Bytes result;
            result.reserve(count);
            while (count) {
                const auto& m = fs.meta(block);
                require(offset <= m.bytes.size(), "invalid metadata cursor");
                if (offset == m.bytes.size()) { block = m.next; offset = 0; continue; }
                const auto n = std::min(count, m.bytes.size() - offset);
                result.insert(result.end(), m.bytes.begin() + offset, m.bytes.begin() + offset + n);
                count -= n;
                offset += n;
            }
            return result;
        }
    };
    Cursor cursor(std::uint64_t table, std::uint64_t reference) {
        require((reference >> 16) <= used && table <= used - (reference >> 16), "invalid inode reference");
        return Cursor{*this, table + (reference >> 16), static_cast<std::size_t>(reference & 0xffff)};
    }
    Bytes dataBlock(std::uint64_t start, std::uint32_t descriptor) {
        require((descriptor & 0xfe000000U) == 0, "invalid SquashFS data block flags");
        auto size = descriptor & 0xffffffU;
        require(size > 0 && size <= blockSize, "invalid SquashFS data block size");
        auto data = read(start, size);
        if (!(descriptor & 0x1000000U)) data = inflateBlock(data, blockSize);
        return data;
    }
    Bytes file(Cursor& cursor, std::uint64_t start, std::uint64_t size,
               std::uint32_t fragment, std::uint32_t fragmentOffset) {
        Bytes result;
        result.reserve(static_cast<std::size_t>(size));
        const auto count = size / blockSize + ((fragment == 0xffffffffU && size % blockSize) ? 1 : 0);
        for (std::uint64_t i = 0; i < count; ++i) {
            auto word = cursor.take(4);
            const auto descriptor = u32(word.data());
            const auto expected = static_cast<std::size_t>(std::min<std::uint64_t>(blockSize, size - result.size()));
            if (descriptor == 0) result.resize(result.size() + expected, 0);
            else {
                auto bytes = dataBlock(start, descriptor);
                require(bytes.size() == expected, "wrong SquashFS data block expansion size");
                result.insert(result.end(), bytes.begin(), bytes.end());
                start += descriptor & 0xffffffU;
            }
        }
        if (fragment != 0xffffffffU && size % blockSize) {
            require(fragment < fragmentCount, "invalid SquashFS fragment index");
            const auto index = std::uint64_t(fragment) * 16;
            auto pointer = read(fragmentTable + (index / 8192) * 8, 8);
            Cursor f{*this, u64(pointer.data()), static_cast<std::size_t>(index % 8192)};
            auto record = f.take(16);
            auto bytes = dataBlock(u64(record.data()), u32(record.data() + 8));
            const auto tail = static_cast<std::size_t>(size % blockSize);
            require(fragmentOffset <= bytes.size() && tail <= bytes.size() - fragmentOffset,
                    "SquashFS fragment outside block");
            result.insert(result.end(), bytes.begin() + fragmentOffset, bytes.begin() + fragmentOffset + tail);
        }
        require(result.size() == size, "wrong SquashFS file size");
        return result;
    }
    void visit(std::uint64_t reference, const std::string& path, unsigned depth) {
        require(depth <= 128, "SquashFS directory nesting exceeds limit");
        out.member();
        auto c = cursor(inodeTable, reference);
        auto base = c.take(16);
        const auto type = u16(base.data());
        require(type >= 1 && type <= 14, "unknown SquashFS inode type");
        if (type == 1 || type == 8) {
            require(visitedDirectories.insert(reference).second, "cyclic or aliased SquashFS directory");
            auto inode = c.take(type == 1 ? 16 : 24);
            const auto start = u32(inode.data() + (type == 1 ? 0 : 8));
            const auto size = type == 1 ? u16(inode.data() + 8) : u32(inode.data() + 4);
            const auto offset = u16(inode.data() + (type == 1 ? 10 : 18));
            require(size >= 3 && size <= fileLimit, "invalid SquashFS directory size");
            Cursor d{*this, directoryTable + start, offset};
            auto bytes = d.take(size - 3);
            std::size_t pos = 0;
            std::set<std::string> names;
            while (pos < bytes.size()) {
                require(bytes.size() - pos >= 12, "truncated SquashFS directory header");
                const auto count = std::uint64_t(u32(bytes.data() + pos)) + 1;
                const auto block = u32(bytes.data() + pos + 4);
                require(count <= 256, "invalid SquashFS directory entry count");
                pos += 12;
                for (std::uint64_t i = 0; i < count; ++i) {
                    require(bytes.size() - pos >= 8, "truncated SquashFS directory entry");
                    auto entry = bytes.data() + pos;
                    const auto inodeOffset = u16(entry);
                    const auto length = std::size_t(u16(entry + 6)) + 1;
                    pos += 8;
                    require(length <= 256 && length <= bytes.size() - pos, "invalid SquashFS name size");
                    std::string name(reinterpret_cast<const char*>(bytes.data() + pos), length);
                    pos += length;
                    require(name != "." && !name.empty() && name.find('/') == std::string::npos,
                            "invalid SquashFS directory name");
                    require(names.insert(name).second, "duplicate SquashFS directory entry");
                    const auto child = normalize(path.empty() ? name : path + "/" + name);
                    visit((std::uint64_t(block) << 16) | inodeOffset, child, depth + 1);
                }
            }
        } else if (type == 2 || type == 9) {
            auto inode = c.take(type == 2 ? 16 : 40);
            const auto start = type == 2 ? u32(inode.data()) : u64(inode.data());
            const auto size = type == 2 ? u32(inode.data() + 12) : u64(inode.data() + 8);
            const auto fragment = u32(inode.data() + (type == 2 ? 4 : 28));
            const auto offset = u32(inode.data() + (type == 2 ? 8 : 32));
            out.account(size);
            if (selected(path)) out.add(path, file(c, start, size, fragment, offset));
        }
        // Other inode types (symlinks, devices, FIFOs, sockets) are never followed or retained.
    }
public:
    explicit Squash(Source& s) : source(s) {}
    FileMap run() {
        auto sb = source.read(0, 96);
        require(u32(sb.data()) == 0x73717368U, "not little-endian SquashFS");
        require(u16(sb.data() + 28) == 4 && u16(sb.data() + 30) == 0, "only SquashFS v4.0 is supported");
        require(u16(sb.data() + 20) == 1, "SquashFS compression must be gzip/zlib (1)");
        blockSize = u32(sb.data() + 12);
        const auto log = u16(sb.data() + 22);
        require(blockSize >= 4096 && blockSize <= MiB && log < 32 && (std::uint32_t(1) << log) == blockSize,
                "invalid SquashFS block size");
        used = u64(sb.data() + 40);
        require(used >= 96 && used <= source.size, "invalid SquashFS bytes_used");
        inodeTable = u64(sb.data() + 64);
        directoryTable = u64(sb.data() + 72);
        fragmentTable = u64(sb.data() + 80);
        fragmentCount = u32(sb.data() + 16);
        require(inodeTable >= 96 && inodeTable < used && directoryTable >= 96 && directoryTable < used,
                "invalid SquashFS table offset");
        if (fragmentCount) {
            const auto pointers = (std::uint64_t(fragmentCount) * 16 + 8191) / 8192;
            require(fragmentTable <= used && pointers * 8 <= used - fragmentTable, "invalid fragment table");
        }
        visit(u64(sb.data() + 32), "", 0);
        return std::move(out.files);
    }
};

class Gzip {
    Source& source;
    z_stream z{};
    Bytes input;
    std::uint64_t position = 0, expanded = 0;
    bool ended = false;
public:
    explicit Gzip(Source& s) : source(s) {
        require(inflateInit2(&z, 15 + 16) == Z_OK, "cannot initialize gzip decoder");
    }
    ~Gzip() { inflateEnd(&z); }
    std::size_t read(std::uint8_t* destination, std::size_t length) {
        z.next_out = destination;
        z.avail_out = static_cast<uInt>(length);
        while (z.avail_out && !ended) {
            if (!z.avail_in && position < source.size) {
                input = source.read(position, std::min<std::uint64_t>(65536, source.size - position));
                position += input.size();
                z.next_in = input.data();
                z.avail_in = static_cast<uInt>(input.size());
            }
            const auto before = z.avail_out;
            const int status = inflate(&z, Z_NO_FLUSH);
            expanded += before - z.avail_out;
            require(expanded <= expandedLimit, "gzip expansion exceeds 512 MiB limit");
            if (status == Z_STREAM_END) {
                ended = true;
                require(z.avail_in == 0 && position == source.size,
                        "trailing data or concatenated gzip streams are unsupported");
            } else {
                require(status == Z_OK, "invalid or truncated gzip stream");
                require(z.avail_out != before || z.avail_in || position < source.size,
                        "truncated gzip stream");
            }
        }
        return length - z.avail_out;
    }
    Bytes take(std::size_t length) {
        Bytes bytes(length);
        require(read(bytes.data(), length) == length, "truncated tar archive");
        return bytes;
    }
    void skip(std::uint64_t length) {
        std::array<std::uint8_t, 65536> buffer{};
        while (length) {
            auto n = static_cast<std::size_t>(std::min<std::uint64_t>(length, buffer.size()));
            require(read(buffer.data(), n) == n, "truncated tar member");
            length -= n;
        }
    }
    void finish() {
        std::array<std::uint8_t, 65536> buffer{};
        for (;;) {
            const auto n = read(buffer.data(), buffer.size());
            if (!n) break;
            require(std::all_of(buffer.begin(), buffer.begin() + n, [](auto b) { return b == 0; }),
                    "nonzero data after tar end markers");
        }
    }
};
std::uint64_t octal(const std::uint8_t* p, std::size_t length) {
    std::uint64_t result = 0;
    bool trailing = false, digit = false;
    for (std::size_t i = 0; i < length; ++i) {
        const auto c = p[i];
        if (c == 0 || c == ' ') { if (digit || c == 0) trailing = true; continue; }
        require(!trailing && c >= '0' && c <= '7', "invalid tar octal field (base-256 unsupported)");
        require(result <= (std::numeric_limits<std::uint64_t>::max() - 7) / 8, "tar numeric overflow");
        result = result * 8 + c - '0';
        digit = true;
    }
    return result;
}
std::string field(const std::uint8_t* p, std::size_t length) {
    const auto end = std::find(p, p + length, 0);
    return {reinterpret_cast<const char*>(p), static_cast<std::size_t>(end - p)};
}
FileMap tar(Source& source) {
    Gzip gz(source);
    Collector out;
    std::string longName;
    for (;;) {
        auto h = gz.take(512);
        if (std::all_of(h.begin(), h.end(), [](auto b) { return b == 0; })) {
            auto second = gz.take(512);
            require(std::all_of(second.begin(), second.end(), [](auto b) { return b == 0; }),
                    "missing second tar end marker");
            require(longName.empty(), "orphan tar long-name record");
            gz.finish();
            return std::move(out.files);
        }
        out.member();
        std::uint64_t sum = 0;
        for (std::size_t i = 0; i < h.size(); ++i) sum += (i >= 148 && i < 156) ? ' ' : h[i];
        require(sum == octal(h.data() + 148, 8), "invalid tar header checksum");
        const auto size = octal(h.data() + 124, 12);
        require(size <= fileLimit, "tar member exceeds 64 MiB limit");
        const auto type = h[156];
        std::string path = field(h.data(), 100);
        if (std::memcmp(h.data() + 257, "ustar", 5) == 0 && h[262] == 0) {
            auto prefix = field(h.data() + 345, 155);
            if (!prefix.empty()) path = prefix + "/" + path;
        }
        if (type == 'L') {
            require(longName.empty() && size > 0 && size <= pathLimit + 1, "invalid GNU tar long name");
            auto value = gz.take(static_cast<std::size_t>(size));
            require(value.back() == 0, "unterminated GNU tar long name");
            longName = field(value.data(), value.size());
            require(longName.size() + 1 == value.size(), "embedded NUL in GNU tar long name");
            normalize(longName);
        } else {
            if (!longName.empty()) { path = std::move(longName); longName.clear(); }
            path = normalize(path);
            require(type != 'x' && type != 'g' && type != 'S', "PAX and sparse tar extensions are unsupported; use a standard GNU/ustar archive");
            if (type == 0 || type == '0') {
                require(!path.empty(), "empty tar regular-file path");
                out.account(size);
                if (selected(path)) out.add(path, gz.take(static_cast<std::size_t>(size)));
                else gz.skip(size);
            } else {
                require(type == '1' || type == '2' || type == '3' || type == '4' || type == '5' || type == '6' || type == 'K',
                        "unsupported tar member type");
                // Do not follow links or interpret device nodes. GNU link targets are discarded.
                gz.skip(size);
            }
        }
        gz.skip((512 - size % 512) % 512);
    }
}
} // namespace

FileMap readImage(const std::filesystem::path& source) {
    Source input(source);
    require(input.size >= 8, "image is too short");
    auto magic = input.read(0, 8);
    if (std::memcmp(magic.data(), "ANDROID!", 8) == 0)
        fail("Android boot .img contains the kernel/boot ramdisk, not the game root filesystem. "
             "Select the matching .hsqs or .tar.gz rootfs (or place it beside this .img for the importer to resolve).");
    if (u32(magic.data()) == 0x73717368U) return Squash(input).run();
    if (magic[0] == 0x1f && magic[1] == 0x8b) return tar(input);
    fail("unsupported image; select a SquashFS .hsqs or gzip .tar.gz root filesystem");
}
} // namespace clover::importer
