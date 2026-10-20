#include "importer.hpp"
#include "hash.hpp"
#include <chrono>
#include <iostream>

namespace fs=std::filesystem;
using namespace clover::importer;
static void check(bool value,const char* error) { if(!value) throw std::runtime_error(error); }
int main(int argc,char** argv) {
  try {
    check(digest({})=="e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","Empty SHA256");
    std::vector<uint8_t> abc{'a','b','c'};
    check(digest(abc)=="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","abc SHA256");
    std::vector<uint8_t> million(1000000,'a');
    check(digest(million)=="cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0","Multiblock SHA256");
    auto base=fs::current_path();if(fs::is_regular_file(base/"CMakeLists.txt")) base/=".build";
    auto work=base/("import-test-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count()));
    fs::create_directories(work);
    auto boot=work/"kernel-example.img";{std::ofstream out(boot,std::ios::binary);out<<"ANDROID!";}
    bool rejected=false;try{resolveFilesystem(boot);}catch(...){rejected=true;}check(rejected,"Boot without companion accepted");
    {std::ofstream out(work/"example.hsqs");out<<"invalid fixture";}
    check(resolveFilesystem(boot)==fs::absolute(work/"example.hsqs"),"Boot companion not resolved");
    rejected=false;try{importImage(boot,work/"bad-cache");}catch(...){rejected=true;}check(rejected,"Malformed filesystem accepted");
    check(listProfiles(work/"bad-cache").empty(),"Incomplete import published");
    if(argc>1) {
      auto profile=importImage(argv[1],work/"profiles");
      check(profile.manifest.at("games")==21,"Stock catalog incomplete");
      check(profile.manifest.at("playableGames")==21,"Stock ROM conversion incomplete");
      auto before=fs::last_write_time(profile.directory/"profile.json");
      auto again=importImage(argv[1],work/"profiles");
      check(again.directory==profile.directory && before==fs::last_write_time(profile.directory/"profile.json"),"Cache was not reused");
      check(listProfiles(work/"profiles").size()==1,"Profile not listed");
      auto old=profile.directory;old.replace_filename(profile.manifest.at("sourceSHA256").get<std::string>()+"-v2");
      fs::rename(profile.directory,old);
      {auto previous=profile.manifest;previous["importerVersion"]=2;std::ofstream out(old/"profile.json");out<<previous.dump();}
      fs::create_directories(old/"data/saves");
      {std::ofstream out(old/"data/saves/migration.fixture");out<<"preserve user saves";}
      {std::ofstream out(old/"data/preferences.json");out<<nlohmann::json{{"selected","CLV-P-SADGE"},{"sound",true},{"firmware",(old/"assets/firmware").string()}}.dump();}
      profile=importImage(argv[1],work/"profiles");
      check(fs::is_regular_file(old/"data/saves/migration.fixture") && fs::is_regular_file(profile.data()/"saves/migration.fixture"),"Save migration lost data");
      {std::ifstream in(profile.data()/"preferences.json");auto prefs=nlohmann::json::parse(in);check(prefs.at("selected")=="CLV-P-SADGE" && prefs.at("sound")==true,"Settings migration failed");check(prefs.at("firmware")== (profile.assets()/"firmware").string(),"Default firmware path was not migrated");}
      check(listProfiles(work/"profiles").size()==1,"Previous importer revision was exposed as current");
      {std::ofstream out(profile.assets()/"skin/wall.png",std::ios::app);out<<"corruption fixture";}
      check(listProfiles(work/"profiles").empty(),"Corrupt cache listed");
      rejected=false;try{importImage(argv[1],work/"profiles");}catch(...){rejected=true;}check(rejected,"Corrupt cache reused");
      std::cout<<"Stock import, cache reuse, and tamper detection passed\n";
    }
    std::cout<<"Hash, companion resolution, malformed input, incomplete profile tests passed\n";
    return 0;
  } catch(const std::exception& e) { std::cerr<<e.what()<<"\n"; return 1; }
}
