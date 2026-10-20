#include "importer.hpp"
#include "image_reader.hpp"
#include "sfrom.hpp"
#include "skin.hpp"
#include "hash.hpp"
#include <algorithm>
#include <chrono>
#include <fstream>
#include <set>
#include <sstream>
#include <atomic>

namespace clover::importer {
namespace fs=std::filesystem;
using Json=nlohmann::json;
static constexpr int importerVersion=3;
static std::string pathString(const fs::path& path) { auto s=path.u8string(); return {reinterpret_cast<const char*>(s.data()),s.size()}; }
static void write(const fs::path& path,const Bytes& b) {
  fs::create_directories(path.parent_path()); std::ofstream out(path,std::ios::binary);
  out.write(reinterpret_cast<const char*>(b.data()),b.size()); if(!out) throw std::runtime_error("Cannot write imported profile");
}
static void writeJSON(const fs::path& path,const Json& j) { auto s=j.dump(2); write(path,Bytes(s.begin(),s.end())); }
static Json readJSON(const fs::path& path) {
  if(fs::is_symlink(path) || fs::file_size(path)>32*1024*1024) throw std::runtime_error("Invalid profile manifest");
  std::ifstream in(path,std::ios::binary); return Json::parse(in);
}
static bool idSafe(const std::string& s) {
  return s.size()>0 && s.size()<100 && std::all_of(s.begin(),s.end(),[](unsigned char c){return (c>='A' && c<='Z') || (c>='0' && c<='9') || c=='-';});
}
static std::string trim(std::string s) {
  auto a=s.find_first_not_of(" \t\r\n"), b=s.find_last_not_of(" \t\r\n"); return a==s.npos ? "":s.substr(a,b-a+1);
}
static std::map<std::string,std::map<std::string,std::string>> desktop(const Bytes& b) {
  std::istringstream in(std::string(b.begin(),b.end())); std::string line,section;
  std::map<std::string,std::map<std::string,std::string>> result;
  while(std::getline(in,line)) {
    line=trim(line); if(line.empty() || line[0]=='#') continue;
    if(line[0]=='[' && line.back()==']') { section=line.substr(1,line.size()-2); continue; }
    auto pos=line.find('='); if(pos==line.npos) continue;
    auto key=trim(line.substr(0,pos));
    if(result[section].contains(key)) throw std::runtime_error("Duplicate desktop metadata key");
    result[section][key]=trim(line.substr(pos+1));
  }
  return result;
}
static bool validProfile(const Profile& profile) {
  try {
    auto& m=profile.manifest;
    if(m.at("importerVersion")!=importerVersion || !fs::is_directory(profile.directory) || fs::is_symlink(profile.directory) || m.at("files").empty()) return false;
    if(profile.directory.filename().string()!=m.at("sourceSHA256").get<std::string>()+"-v"+std::to_string(importerVersion)) return false;
    for(auto& [name,hash]:m.at("files").items()) {
      fs::path relative(name); if(relative.is_absolute() || name.find('\\')!=name.npos || !name.starts_with("assets/")) return false;
      fs::path path=profile.directory;
      for(const auto& part:relative) { if(part==".." || part==".") return false; path/=part; if(fs::is_symlink(path)) return false; }
      if(!fs::is_regular_file(path) || fileDigest(path)!=hash.get<std::string>()) return false;
    }
    return m.at("files").contains("assets/catalog.json") && m.at("files").contains("assets/skin/layout.json");
  } catch(...) { return false; }
}
std::string Profile::label() const {
  auto label=manifest.value("region","Unknown")+" / "+manifest.value("version","Unknown")+" ("+std::to_string(manifest.value("playableGames",0))+" converted";
  int firmware=manifest.value("externalFirmwareGames",0);
  if(firmware) label+="; "+std::to_string(firmware)+" needs firmware";
  return label+")";
}
fs::path resolveFilesystem(const fs::path& source) {
  if(!fs::is_regular_file(source) || fs::is_symlink(source)) throw std::runtime_error("Choose a regular local firmware image or filesystem archive");
  std::ifstream in(source,std::ios::binary); char magic[8]{}; in.read(magic,8);
  if(std::string(magic,8)!="ANDROID!") return fs::absolute(source);
  auto name=source.filename().string();
  if(name.starts_with("kernel-") && name.ends_with(".img")) {
    auto stem=name.substr(7,name.size()-7-4);
    for(auto directory:{source.parent_path(),source.parent_path()/"SNES-Mini-Kernels"})
      for(auto suffix:{".hsqs","_trimmed.hsqs",".tar.gz"}) {
        auto sibling=directory/(stem+suffix);
        if(fs::is_regular_file(sibling) && !fs::is_symlink(sibling)) return fs::absolute(sibling);
      }
  }
  throw std::runtime_error("This Android boot image contains no game filesystem. Put its matching .hsqs or .tar.gz beside it, or choose the filesystem directly.");
}
Profile importImage(const fs::path& source,const fs::path& cache,const Progress& progress) {
  auto report=[&](const std::string& s){if(progress) progress(s);};
  auto filesystem=resolveFilesystem(source);
  if(fs::file_size(filesystem)>1024ull*1024*1024) throw std::runtime_error("Image exceeds 1 GiB import limit");
  report("Hashing filesystem image..."); auto sha=fileDigest(filesystem);
  fs::create_directories(cache);
  if(fs::is_symlink(cache)) throw std::runtime_error("Profile cache must not be a symbolic link");
  auto destination=cache/(sha+"-v"+std::to_string(importerVersion));
  if(fs::exists(destination)) {
    Profile existing{destination,readJSON(destination/"profile.json")};
    report("Checking cached profile...");
    if(!validProfile(existing)) throw std::runtime_error("Cached profile failed integrity checks. Preserve/rename that profile folder, then import again: "+pathString(destination));
    report("Using verified cache"); return existing;
  }
  report("Reading image (no extracted code is executed)...");
  auto files=readImage(filesystem);
  // Rehash after reading to reject images changed while importing.
  if(fileDigest(filesystem)!=sha) throw std::runtime_error("Source image changed during import");
  std::string ui;
  for(auto candidate:{"snes-eur","snes-usa","shvc"}) if(files.contains("usr/share/ui/"+std::string(candidate)+"/dependencies.json")) {
    if(!ui.empty()) throw std::runtime_error("Ambiguous image: multiple stock UI roots"); ui=candidate;
  }
  if(ui.empty()) throw std::runtime_error("Unsupported image: no recognized SNES Classic UI");
  std::string region=ui=="shvc" ? "Japan":ui=="snes-usa" ? "USA":"Europe";
  std::string version="unknown";
  if(auto i=files.find("etc/clover/VERSION");i!=files.end()) version=trim(std::string(i->second.begin(),i->second.end()));
  if(version.empty() || version.size()>200) throw std::runtime_error("Invalid image version metadata");
  static std::atomic<unsigned> serial=0;
  auto stage=cache/(".import-"+sha.substr(0,12)+"-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count())+"-"+std::to_string(serial++));
  if(!fs::create_directory(stage)) throw std::runtime_error("Cannot create staging profile");
  report("Decoding Clover artwork and fonts..."); prepareSkin(files,ui,stage/"assets/skin");
  // Identify the standard SMP boot firmware by its full digest, not by a
  // guessed executable offset. Only these user-supplied 64 bytes are retained.
  report("Locating verified SMP boot firmware in the supplied image...");
  auto carrier=files.find("usr/bin/canoe-shvc");
  bool foundIPL=false;
  constexpr auto iplSHA="c95f88b299030d5afa55b1031e2b5ef2dff650c4b4e6bb6f8b1359436521278f";
  if(carrier!=files.end()) {
    auto& b=carrier->second;
    for(size_t offset=0;b.size()>=64 && offset<=b.size()-64;++offset) {
      // Prefix is only a search accelerator; all 64 bytes must match SHA-256.
      if(b[offset]!=0xcd || b[offset+1]!=0xef || b[offset+2]!=0xbd || b[offset+3]!=0xe8) continue;
      auto candidate=std::span<const uint8_t>(b).subspan(offset,64);
      if(digest(candidate)==iplSHA) {
        write(stage/"assets/firmware/ipl.rom",Bytes(candidate.begin(),candidate.end()));foundIPL=true;break;
      }
    }
  }
  if(!foundIPL) throw std::runtime_error("Image does not contain the verified SMP IPL firmware required by ares");
  Json games=Json::array(), conversions=Json::array(); unsigned playable=0,externalFirmwareGames=0; std::set<std::string> codes;
  for(auto& [path,b]:files) {
    if(!path.starts_with("usr/share/games/") || !path.ends_with(".desktop")) continue;
    auto sections=desktop(b); auto& entry=sections["Desktop Entry"]; auto& meta=sections["X-CLOVER Game"];
    auto code=meta["Code"];
    if(!idSafe(code) || path!="usr/share/games/"+code+"/"+code+".desktop" || !codes.insert(code).second) throw std::runtime_error("Invalid or duplicate catalog code");
    if(entry["Name"].empty()) throw std::runtime_error("Missing catalog title");
    int players=1; if(!meta["Players"].empty()) players=std::stoi(meta["Players"]);
    if(players<1 || players>8) throw std::runtime_error("Invalid player count");
    Json game={{"id",code},{"title",entry["Name"]},{"sortTitle",meta["SortRawTitle"]},{"publisher",meta["SortRawPublisher"]},
      {"releaseDate",meta["ReleaseDate"]},{"players",players},{"simultaneous",meta["Simultaneous"]=="1"},{"copyright",meta["Copyright"]}};
    auto root="usr/share/games/"+code+"/";
    for(auto suffix:{".png","_small.png"}) {
      auto it=files.find(root+code+suffix); if(it==files.end()) throw std::runtime_error("Catalog artwork is missing: "+code);
      write(stage/"assets/games"/code/(code+suffix),it->second);
    }
    report("Preparing "+entry["Name"]+"...");
    Json conversion={{"id",code},{"title",entry["Name"]}};
    auto rom=files.find(root+code+".sfrom");
    try {
      if(rom==files.end()) throw std::runtime_error("Missing SFROM member");
      conversion["sourceSHA256"]=digest(rom->second);
      auto converted=convertSFROM(rom->second);
      if(converted.rom.empty()) throw std::runtime_error("Converter produced no ROM");
      auto relative="roms/"+code+".sfc"; write(stage/"assets"/relative,converted.rom);
      game["rom"]=relative; game["importStatus"]=converted.status;
      // Both regional Mario Kart IDs use the DSP1B board selected by ares's
      // analyzer. The supplied container is not a dump of that coprocessor ROM.
      if(code=="CLV-P-SAAFE" || code=="CLV-P-VAAFJ") {
        game["firmwareRequired"]=Json::array({"dsp1b.upd7725.program.rom","dsp1b.upd7725.data.rom"});
        conversion["firmwareRequired"]=game["firmwareRequired"];++externalFirmwareGames;
      }
      conversion["status"]=converted.status; conversion["metadata"]=converted.metadata; conversion["romSHA256"]=digest(converted.rom); ++playable;
    } catch(const std::exception& e) { game["importError"]=e.what(); conversion["error"]=e.what(); }
    games.push_back(game); conversions.push_back(conversion);
  }
  if(games.size()!=21) throw std::runtime_error("Unsupported stock catalog: expected 21 entries");
  writeJSON(stage/"assets/catalog.json",{{"schema",1},{"region",region},{"ui",ui},{"locale",ui=="shvc" ? "jpn_ja":"usa_en"},{"version",version},{"games",games}});
  Json manifest={{"schema",1},{"importerVersion",importerVersion},{"sourceSHA256",sha},{"source",pathString(filesystem)},
    {"selectedSource",pathString(fs::absolute(source))},{"selectedSourceSHA256",fileDigest(source)},{"region",region},{"ui",ui},{"locale",ui=="shvc" ? "jpn_ja":"usa_en"},{"version",version},
    {"games",games.size()},{"playableGames",playable},{"externalFirmwareGames",externalFirmwareGames},{"conversions",conversions},{"smpIPL",{{"source","usr/bin/canoe-shvc"},{"sha256",iplSHA}}},{"files",Json::object()}};
  // Never replace the old cache. Copy its user data into the new revision so
  // existing settings, mappings and the three unchanged ROMs' saves survive.
  for(int previousVersion=importerVersion-1;previousVersion>=1;--previousVersion) {
    auto old=cache/(sha+"-v"+std::to_string(previousVersion));
    if(!fs::is_directory(old/"data") || fs::is_symlink(old) || fs::is_symlink(old/"data")) continue;
    auto oldManifest=readJSON(old/"profile.json");
    if(oldManifest.value("sourceSHA256","")!=sha || oldManifest.value("importerVersion",0)!=previousVersion) continue;
    report("Preserving settings and saves from the previous importer...");
    uint64_t total=0;unsigned count=0;
    for(auto& item:fs::recursive_directory_iterator(old/"data")) {
      if(++count>20000 || item.is_symlink()) throw std::runtime_error("Previous profile data exceeds migration limits or contains a symlink; the old profile is unchanged");
      auto target=stage/"data"/item.path().lexically_relative(old/"data");
      if(item.is_directory()) fs::create_directories(target);
      else if(item.is_regular_file()) {
        auto size=item.file_size();
        if(size>1024ull*1024*1024-total) throw std::runtime_error("Previous saves exceed the 1 GiB migration limit; old profile is unchanged");
        total+=size;
        fs::create_directories(target.parent_path());fs::copy_file(item.path(),target);
      }
    }
    auto preferences=stage/"data/preferences.json";
    if(fs::is_regular_file(preferences)) {
      auto values=readJSON(preferences);
      if(values.value("firmware","")==pathString(old/"assets/firmware")) {
        values["firmware"]=pathString(destination/"assets/firmware");writeJSON(preferences,values);
      }
    }
    manifest["migratedFrom"]=pathString(old);break;
  }
  for(auto& item:fs::recursive_directory_iterator(stage/"assets")) if(item.is_regular_file()) manifest["files"][item.path().lexically_relative(stage).generic_string()]=fileDigest(item.path());
  writeJSON(stage/"profile.json",manifest);
  fs::rename(stage,destination); report("Profile ready"); return {destination,manifest};
}
std::vector<Profile> listProfiles(const fs::path& cache) {
  std::vector<Profile> result; if(!fs::is_directory(cache)) return result;
  for(auto& dir:fs::directory_iterator(cache)) {
    auto name=dir.path().filename().string(); if(!dir.is_directory() || dir.is_symlink() || name.starts_with(".")) continue;
    try { Profile p{dir.path(),readJSON(dir.path()/"profile.json")}; if(validProfile(p)) result.push_back(std::move(p)); } catch(...) {}
  }
  std::sort(result.begin(),result.end(),[](auto& a,auto& b){return a.label()<b.label();}); return result;
}
std::vector<fs::path> discoverImages(const std::vector<fs::path>& directories) {
  std::set<fs::path> unique;
  for(auto& directory:directories) {
    std::error_code ec; if(!fs::is_directory(directory,ec)) continue;
    unsigned count=0;
    for(auto& item:fs::directory_iterator(directory,ec)) {
      if(++count>2000) break;
      if(!item.is_regular_file(ec) || item.is_symlink(ec)) continue;
      auto name=item.path().filename().string();
      if(!(name.ends_with(".img") || name.ends_with(".hsqs") || name.ends_with(".tar.gz"))) continue;
      try { unique.insert(fs::weakly_canonical(resolveFilesystem(item.path()))); } catch(...) {}
    }
  }
  // Hide redundant tar/trimmed companions when the original SquashFS is present.
  std::vector<fs::path> result;
  for(auto& path:unique) {
    auto s=path.string(); auto original=s;
    if(s.ends_with("_trimmed.hsqs")) original=s.substr(0,s.size()-13)+".hsqs";
    else if(s.ends_with(".tar.gz")) original=s.substr(0,s.size()-7)+".hsqs";
    if(s!=original && unique.contains(fs::path(original))) continue;
    result.push_back(path);
  }
  return result;
}
}
