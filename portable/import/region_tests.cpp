#include "image_reader.hpp"
#include "skin.hpp"
#include "hash.hpp"
#include <json.hpp>
#include <chrono>
#include <fstream>
#include <iostream>
#include <set>
#include <sstream>

namespace fs=std::filesystem;
using namespace clover::importer;
using Json=nlohmann::json;
static void check(bool value,const std::string& message) { if(!value) throw std::runtime_error(message); }
static Json json(const fs::path& p) { std::ifstream in(p);return Json::parse(in); }
static std::vector<uint32_t> scalars(const std::string& text) {
  std::vector<uint32_t> out;
  for(size_t i=0;i<text.size();) {
    auto c=uint8_t(text[i++]);unsigned n=c<128 ? 0:c<224 ? 1:c<240 ? 2:3;
    uint32_t code=c & (n==0 ? 127:n==1 ? 31:n==2 ? 15:7);
    while(n--) { check(i<text.size(),"Invalid fixture UTF8");code=(code<<6)|(uint8_t(text[i++])&63); }
    out.push_back(code);
  }
  return out;
}
int main(int argc,char** argv) {
  try {
    if(argc!=2) throw std::runtime_error("Usage: clover_region_tests local-image-directory");
    auto work=fs::current_path();if(fs::is_regular_file(work/"CMakeLists.txt")) work/=".build";
    work/="region-test-"+std::to_string(std::chrono::steady_clock::now().time_since_epoch().count());
    fs::create_directories(work);
    std::map<std::string,std::string> atlases;unsigned tested=0;
    for(auto& entry:fs::directory_iterator(argv[1])) {
      auto name=entry.path().filename().string();if(!name.ends_with(".tar.gz")) continue;
      auto files=readImage(entry.path());std::string ui;
      for(auto id:{"snes-eur","snes-usa","shvc"}) if(files.contains("usr/share/ui/"+std::string(id)+"/dependencies.json")) ui=id;
      check(!ui.empty(),"Missing regional UI");
      auto out=work/name;prepareSkin(files,ui,out);
      auto theme=json(out/"theme.json"),table=json(out/"strings.json");
      check(theme.at("ui")==ui,"Wrong theme identity");
      check(theme.at("colors").at("title")[0]==(ui=="shvc" ? 255:0) && theme.at("colors").at("hud")[0]==(ui=="shvc" ? 0:255),"Wrong regional environment colors");
      check(table.at("locale")== (ui=="shvc" ? "jpn_ja":"usa_en"),"Wrong source locale");
      auto expected=files.at("usr/share/ui/"+ui+"/resources/sprites/packed.png");
      check(fileDigest(out/"packed.png")==digest(expected),"Atlas substituted from another image");
      atlases[ui]=digest(expected);
      std::set<uint32_t> available;
      for(auto font:{"s","m","l","title","copyright"}) {
        auto metrics=json(out/(std::string("font-")+font+".json"));
        for(auto& [code,_]:metrics.at("glyphs").items()) available.insert(uint32_t(std::stoul(code)));
      }
      auto coverage=[&](const std::string& value) {
        for(auto code:scalars(value)) check(code=='\n' || code=='\r' || available.contains(code),"Missing source glyph "+std::to_string(code)+" in "+ui+": "+value);
      };
      unsigned titles=0;
      for(auto& [path,b]:files) if(path.starts_with("usr/share/games/") && path.ends_with(".desktop")) {
        std::istringstream stream(std::string(b.begin(),b.end()));std::string line;
        while(std::getline(stream,line)) if(line.starts_with("Name=")) { coverage(line.substr(5));++titles;break; }
      }
      check(titles==21,"Wrong regional catalog count");
      for(auto key:{"sys_gametitlelist_hud_Menu","sys_gametitlelist_hud_Resume","sys_gametitlelist_hud_Sort","sys_gametitlelist_hud_Decide","sys_option_hud_Return","sys_option_Title","sys_display_Title","sys_language_hud_Title","sys_resume_Title","sys_menubarU_cm_Display","sys_menubarU_cm_Option","sys_menubarU_cm_Language","sys_menubarU_cm_Copyright","sys_menubarU_cm_Manual"}) coverage(table.at("strings").at(key));
      auto layout=json(out/"layout.json");check(layout.at("cardSpacing").get<double>()>0,"Invalid regional layout");
      std::cout<<name<<": original atlas, locale, 21 titles and HUD glyph coverage passed\n";++tested;
    }
    check(tested==6,"Expected the six supplied filesystem archives");
    check(atlases.size()==3 && atlases.at("snes-eur")!=atlases.at("snes-usa") && atlases.at("snes-eur")!=atlases.at("shvc") && atlases.at("snes-usa")!=atlases.at("shvc"),"Regional console artwork was not distinct");
    std::cout<<"All regions/version skins verified. Artifacts: "<<work<<"\n";
  } catch(const std::exception& e) { std::cerr<<e.what()<<"\n";return 1; }
}
