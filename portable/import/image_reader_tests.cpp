#include "image_reader.hpp"
#include <zlib.h>

#include <algorithm>
#include <array>
#include <cstring>
#include <fstream>
#include <iostream>
#include <stdexcept>
#include <string_view>

using clover::importer::Bytes;
using clover::importer::FileMap;
using clover::importer::readImage;
namespace {
const std::filesystem::path output = ".build/image-reader-tests";
void check(bool condition, const std::string& message) {
    if (!condition) throw std::runtime_error(message);
}
void put(Bytes& b, std::size_t offset, std::uint64_t value, unsigned width) {
    for (unsigned i = 0; i < width; ++i) b.at(offset + i) = static_cast<std::uint8_t>(value >> (8 * i));
}
std::uint64_t get(const Bytes& b, std::size_t offset, unsigned width) {
    std::uint64_t value = 0;
    for (unsigned i = 0; i < width; ++i) value |= std::uint64_t(b.at(offset + i)) << (8 * i);
    return value;
}
void append(Bytes& b, const Bytes& value) { b.insert(b.end(), value.begin(), value.end()); }
Bytes text(std::string_view s) { return Bytes(s.begin(), s.end()); }
Bytes compress(const Bytes& bytes, bool gzip = false) {
    z_stream z{};
    check(deflateInit2(&z, 6, Z_DEFLATED, gzip ? 31 : 15, 8, Z_DEFAULT_STRATEGY) == Z_OK, "deflate init");
    Bytes result(compressBound(static_cast<uLong>(bytes.size())) + 32);
    z.next_in = const_cast<Bytef*>(bytes.data());
    z.avail_in = static_cast<uInt>(bytes.size());
    z.next_out = result.data();
    z.avail_out = static_cast<uInt>(result.size());
    check(deflate(&z, Z_FINISH) == Z_STREAM_END, "deflate finish");
    result.resize(z.total_out);
    deflateEnd(&z);
    return result;
}
void write(const std::filesystem::path& path, const Bytes& bytes) {
    std::ofstream stream(path, std::ios::binary);
    stream.write(reinterpret_cast<const char*>(bytes.data()), static_cast<std::streamsize>(bytes.size()));
    check(bool(stream), "write fixture");
}
void rejects(const Bytes& bytes, const std::string& name, std::string_view expected = {}) {
    auto path = output / (name + ".bin");
    write(path, bytes);
    try { (void)readImage(path); }
    catch (const std::runtime_error& e) {
        check(expected.empty() || std::string_view(e.what()).find(expected) != std::string_view::npos,
              name + ": unexpected error: " + e.what());
        return;
    }
    throw std::runtime_error(name + ": should reject");
}
void octal(Bytes& b, std::size_t offset, unsigned width, std::uint64_t value) {
    std::fill(b.begin() + offset, b.begin() + offset + width, '0');
    b[offset + width - 1] = 0;
    for (unsigned i = width - 1; i > 0 && value; --i, value >>= 3) b[offset + i - 1] = '0' + (value & 7);
}
void tarMember(Bytes& tar, std::string_view name, const Bytes& value, char type = '0') {
    check(name.size() <= 100, "fixture name length");
    Bytes h(512);
    std::copy(name.begin(), name.end(), h.begin());
    octal(h, 100, 8, 0644);
    octal(h, 124, 12, value.size());
    h[156] = type;
    std::memcpy(h.data() + 257, "ustar", 5);
    std::fill(h.begin() + 148, h.begin() + 156, ' ');
    std::uint64_t sum = 0;
    for (auto c : h) sum += c;
    octal(h, 148, 8, sum);
    append(tar, h);
    append(tar, value);
    tar.resize((tar.size() + 511) / 512 * 512);
}
Bytes finishTar(Bytes tar) { tar.resize(tar.size() + 1024); return compress(tar, true); }
Bytes metadata(const Bytes& bytes, bool compressed) {
    auto value = compressed ? compress(bytes) : bytes;
    Bytes result(2);
    put(result, 0, value.size() | (compressed ? 0 : 0x8000), 2);
    append(result, value);
    return result;
}
// Independently authored minimal v4 filesystem: root -> usr/share/ui/test.bin.
// Exercises both inode forms, metadata/data compression, fragment and sparse blocks.
Bytes squash(bool compressed, bool extended, bool fragment, bool sparse = false) {
    constexpr unsigned blockSize = 131072;
    Bytes content = fragment ? text("fragment payload") : Bytes(blockSize + 7, sparse ? 0 : 'Q');
    Bytes image(96), data;
    std::uint32_t descriptor = 0;
    if (!sparse) {
        auto block = fragment ? content : Bytes(content.begin(), content.begin() + blockSize);
        data = compressed ? compress(block) : block;
        descriptor = static_cast<std::uint32_t>(data.size()) | (compressed ? 0 : 0x1000000);
        append(image, data);
        if (!fragment) {
            Bytes tail(content.begin() + blockSize, content.end());
            append(image, compressed ? compress(tail) : tail);
        }
    }
    Bytes inodes;
    std::array<std::uint16_t, 5> refs{};
    for (unsigned i = 0; i < 4; ++i) {
        refs[i] = static_cast<std::uint16_t>(inodes.size());
        inodes.resize(inodes.size() + (extended ? 40 : 32));
    }
    refs[4] = static_cast<std::uint16_t>(inodes.size());
    const auto fileStart = inodes.size();
    inodes.resize(fileStart + (extended ? 56 : 32));
    put(inodes, fileStart, extended ? 9 : 2, 2);
    put(inodes, fileStart + 16, 96, extended ? 8 : 4);
    put(inodes, fileStart + (extended ? 24 : 28), content.size(), extended ? 8 : 4);
    put(inodes, fileStart + (extended ? 44 : 20), fragment ? 0 : 0xffffffffU, 4);
    if (!fragment) {
        auto at = inodes.size(); inodes.resize(at + 8);
        put(inodes, at, sparse ? 0 : descriptor, 4);
        auto tail = text("QQQQQQQ");
        put(inodes, at + 4, sparse ? 0 : (compressed ? compress(tail).size() : (0x1000000U | 7)), 4);
    }
    Bytes directories;
    const std::array<std::string, 4> names{"usr", "share", "ui", "test.bin"};
    for (unsigned i = 0; i < 4; ++i) {
        auto at = directories.size();
        directories.resize(at + 20 + names[i].size());
        put(directories, at + 12, refs[i + 1], 2);
        put(directories, at + 16, i == 3 ? 2 : 1, 2);
        put(directories, at + 18, names[i].size() - 1, 2);
        std::copy(names[i].begin(), names[i].end(), directories.begin() + at + 20);
        put(inodes, refs[i], extended ? 8 : 1, 2);
        put(inodes, refs[i] + (extended ? 20 : 24), 23 + names[i].size(), extended ? 4 : 2);
        put(inodes, refs[i] + (extended ? 34 : 26), at, 2);
    }
    put(image, 64, image.size(), 8);
    append(image, metadata(inodes, compressed));
    put(image, 72, image.size(), 8);
    append(image, metadata(directories, compressed));
    if (fragment) {
        const auto position = image.size();
        Bytes record(16);
        put(record, 0, 96, 8); put(record, 8, descriptor, 4);
        append(image, metadata(record, compressed));
        put(image, 80, image.size(), 8);
        const auto at = image.size(); image.resize(at + 8); put(image, at, position, 8);
        put(image, 16, 1, 4);
    }
    put(image, 0, 0x73717368, 4); put(image, 4, 5, 4);
    put(image, 12, blockSize, 4); put(image, 20, 1, 2); put(image, 22, 17, 2);
    put(image, 28, 4, 2); put(image, 40, image.size(), 8);
    return image;
}
void fixtures() {
    std::filesystem::create_directories(output);
    Bytes tar;
    tarMember(tar, "./usr/share/games/A/A.desktop", text("desktop"));
    tarMember(tar, "usr/share/games/A/A.png", text("png"));
    tarMember(tar, "usr/share/games/A/A.sfrom", text("rom"));
    tarMember(tar, "usr/share/ui/font.bin", text("font"));
    tarMember(tar, "etc/clover/VERSION", text("version"));
    tarMember(tar, "etc/clover/version/build", text("build"));
    tarMember(tar, "usr/share/games/A/run.sh", text("never execute"));
    tarMember(tar, "usr/share/ui/link", {}, '2');
    tarMember(tar, "usr/share/ui/device", {}, '3');
    auto valid = finishTar(tar);
    write(output / "valid.tar.gz", valid);
    const auto files = readImage(output / "valid.tar.gz");
    check(files.size() == 6 && files.at("usr/share/ui/font.bin") == text("font"), "tar filtering");
    for (const auto& name : {"../usr/share/ui/a", "/usr/share/ui/a", "usr/share/ui/../../a", "C:/a", "usr\\share\\ui\\a"}) {
        Bytes bad; tarMember(bad, name, text("bad")); rejects(finishTar(bad), "unsafe-path");
    }
    Bytes duplicate;
    tarMember(duplicate, "usr/share/ui/A", text("a"));
    tarMember(duplicate, "./usr/share/ui/a", text("b"));
    rejects(finishTar(duplicate), "case-collision", "colliding");
    Bytes absoluteLink; tarMember(absoluteLink, "/link", {}, '2');
    rejects(finishTar(absoluteLink), "absolute-link");
    auto corruptTar = tar; corruptTar[10] ^= 1;
    rejects(finishTar(corruptTar), "tar-checksum", "checksum");
    auto truncated = valid; truncated.pop_back(); rejects(truncated, "gzip-truncated");
    auto crc = valid; crc[crc.size() - 8] ^= 1; rejects(crc, "gzip-crc");
    auto concat = valid; append(concat, valid); rejects(concat, "concatenated-gzip");
    Bytes pax; tarMember(pax, "pax", text("path=x"), 'x'); rejects(finishTar(pax), "pax");
    Bytes longTar;
    const auto longPath = "usr/share/ui/" + std::string(120, 'a');
    auto longValue = text(longPath); longValue.push_back(0);
    tarMember(longTar, "././@LongLink", longValue, 'L');
    tarMember(longTar, "short", text("long"));
    write(output / "long.tar.gz", finishTar(longTar));
    check(readImage(output / "long.tar.gz").at(longPath) == text("long"), "GNU long path");
    for (bool compressed : {false, true}) for (bool extended : {false, true}) for (bool fragment : {false, true}) {
        const auto bytes = squash(compressed, extended, fragment);
        write(output / "fixture.hsqs", bytes);
        const auto result = readImage(output / "fixture.hsqs");
        check(result.size() == 1 && result.at("usr/share/ui/test.bin") ==
              (fragment ? text("fragment payload") : Bytes(131079, 'Q')), "SquashFS fixture");
    }
    write(output / "sparse.hsqs", squash(false, true, false, true));
    check(readImage(output / "sparse.hsqs").at("usr/share/ui/test.bin") == Bytes(131079, 0), "sparse file");
    auto bad = squash(true, false, true);
    put(bad, 20, 4, 2); rejects(bad, "squash-compressor", "compression");
    bad = squash(true, false, true); put(bad, 40, bad.size() + 1, 8); rejects(bad, "squash-used");
    bad = squash(true, false, true); put(bad, 64, 0xffffffffU, 8); rejects(bad, "squash-offset");
    bad = squash(true, false, true); bad[96] ^= 1; rejects(bad, "squash-data-corrupt");
    bad = squash(false, false, true);
    auto inode = static_cast<std::size_t>(get(bad, 64, 8)) + 2;
    put(bad, inode + 4 * 32 + 28, 64 * 1024 * 1024 + 1, 4);
    rejects(bad, "squash-file-limit", "64 MiB");
    bad = squash(false, false, true);
    inode = static_cast<std::size_t>(get(bad, 64, 8)) + 2;
    put(bad, inode + 4 * 32 + 20, 1, 4);
    rejects(bad, "squash-fragment-index", "fragment index");
    bad = squash(false, false, true);
    auto dir = static_cast<std::size_t>(get(bad, 72, 8)) + 2;
    put(bad, dir + 12, 0, 2);
    rejects(bad, "squash-cycle", "cyclic");
    bad = squash(false, false, true);
    dir = static_cast<std::size_t>(get(bad, 72, 8)) + 2;
    bad[dir + 20] = '/';
    rejects(bad, "squash-path");
    bad = squash(false, false, true);
    put(bad, 32, 0xffffffffffffffffULL, 8);
    rejects(bad, "squash-root-reference");
    Bytes largeHeader; tarMember(largeHeader, "usr/share/ui/large", {});
    octal(largeHeader, 124, 12, 64 * 1024 * 1024 + 1);
    std::fill(largeHeader.begin() + 148, largeHeader.begin() + 156, ' ');
    std::uint64_t sum = 0;
    for (auto c : largeHeader) sum += c;
    octal(largeHeader, 148, 8, sum);
    rejects(finishTar(largeHeader), "tar-file-limit", "64 MiB");
    const auto largePath = output / "oversized.bin";
    {
        std::ofstream stream(largePath, std::ios::binary);
        stream.seekp(1024LL * 1024 * 1024);
        stream.put(0); // Sparse fixture: no 1 GiB allocation or read.
        check(bool(stream), "write oversized sparse archive");
    }
    try { (void)readImage(largePath); throw std::logic_error("archive size limit not enforced"); }
    catch (const std::runtime_error& e) { check(std::string_view(e.what()).find("1 GiB") != std::string_view::npos, "archive size error"); }
    std::filesystem::remove(largePath);
    rejects(text("ANDROID!fake boot"), "android", "matching .hsqs");
    rejects(text("not an image"), "unsupported");
    std::cout << "Self-authored valid/malformed fixtures passed\n";
}
void supplied(const std::filesystem::path& directory) {
    unsigned comparisons = 0;
    for (const auto& entry : std::filesystem::directory_iterator(directory)) {
        const auto path = entry.path();
        if (path.extension() == ".img") {
            try { (void)readImage(path); throw std::logic_error("Android boot image accepted"); }
            catch (const std::runtime_error& e) {
                check(std::string_view(e.what()).find("Android boot") != std::string_view::npos,
                      "unexpected supplied boot image error");
            }
        }
        if (!path.filename().string().ends_with(".tar.gz")) continue;
        const auto stem = path.filename().string().substr(0, path.filename().string().size() - 7);
        auto reference = readImage(path);
        check(!reference.empty(), "empty supplied tar: " + stem);
        for (const auto& suffix : {".hsqs", "_trimmed.hsqs"}) {
            const auto squashPath = directory / (stem + suffix);
            if (!std::filesystem::exists(squashPath)) continue;
            auto actual = readImage(squashPath);
            if (actual != reference) {
                for (const auto& [name, value] : reference) {
                    auto it = actual.find(name);
                    if (it == actual.end() || it->second != value) std::cerr << "Different/missing: " << name << '\n';
                }
                for (const auto& [name, value] : actual) if (!reference.contains(name)) std::cerr << "Extra: " << name << '\n';
                throw std::runtime_error("Supplied archive mismatch: " + squashPath.string());
            }
            std::cout << "Byte-equal: " << squashPath.filename().string() << " (" << actual.size() << " files)\n";
            ++comparisons;
        }
    }
    check(comparisons > 0, "no matching supplied images found");
}
} // namespace
int main(int argc, char** argv) {
    try {
        fixtures();
        if (argc > 1) supplied(argv[1]);
        std::cout << "Image reader tests passed\n";
        return 0;
    } catch (const std::exception& e) {
        std::cerr << e.what() << '\n';
        return 1;
    }
}
