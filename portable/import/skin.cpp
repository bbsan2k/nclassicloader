#include "skin.hpp"
#include "hash.hpp"
#include <array>
#include <fstream>
#include <functional>
#include <cmath>
#include <set>

namespace clover::importer {
using Json=nlohmann::json;
static void require(bool test,const char* error) { if(!test) throw std::runtime_error(error); }
static uint32_t le(const Bytes& b,size_t p,unsigned n) {
  require(p<=b.size() && n<=b.size()-p,"Truncated font metrics");
  uint32_t v=0; for(unsigned i=0;i<n;++i) v|=uint32_t(b[p+i])<<(8*i); return v;
}
static void write(const std::filesystem::path& path,const Bytes& bytes) {
  std::filesystem::create_directories(path.parent_path());
  std::ofstream out(path,std::ios::binary); out.write(reinterpret_cast<const char*>(bytes.data()),bytes.size());
  if(!out) throw std::runtime_error("Cannot write decoded skin");
}
static void writeJSON(const std::filesystem::path& path,const Json& value) {
  auto s=value.dump(2); write(path,Bytes(s.begin(),s.end()));
}
static void font(const Bytes& b,const std::filesystem::path& out,const std::string& name) {
  require(b.size()>=9 && b[0]==1,"Unsupported Reed font container");
  auto metrics=le(b,1,4), image=le(b,5,4);
  require(uint64_t(9)+metrics+image==b.size() && metrics>=4 && image>=8,"Invalid Reed font section lengths");
  require(b[9]=='B' && b[10]=='M' && b[11]=='F' && b[12]==3,"Unsupported BMFont version");
  size_t end=9+metrics; const uint8_t png[]{137,80,78,71,13,10,26,10};
  require(std::equal(std::begin(png),std::end(png),b.begin()+end),"Invalid font image");
  Json result={{"glyphs",Json::object()},{"kernings",Json::object()}};
  std::set<unsigned> seen;
  for(size_t p=13;p<end;) {
    require(end-p>=5,"Truncated BMFont block"); unsigned kind=b[p]; auto size=le(b,p+1,4); p+=5;
    require(size<=end-p && seen.insert(kind).second,"Invalid/duplicate BMFont block");
    if(kind==1) { require(size>=2,"Invalid font info"); result["fontSize"]=std::abs(int(int16_t(le(b,p,2)))); }
    if(kind==2) {
      require(size==15 && le(b,p+8,2)==1,"Unsupported font page count");
      result["lineHeight"]=le(b,p,2); result["baseline"]=le(b,p+2,2); result["width"]=le(b,p+4,2); result["height"]=le(b,p+6,2);
    }
    if(kind==4) {
      require(size%20==0,"Invalid font glyph block");
      for(size_t j=p;j<p+size;j+=20) {
        require(b[j+18]==0,"Unsupported font glyph page");
        auto code=std::to_string(le(b,j,4)); require(!result["glyphs"].contains(code),"Duplicate font glyph");
        result["glyphs"][code]={{"x",le(b,j+4,2)},{"y",le(b,j+6,2)},{"width",le(b,j+8,2)},{"height",le(b,j+10,2)},
          {"xOffset",int16_t(le(b,j+12,2))},{"yOffset",int16_t(le(b,j+14,2))},{"advance",int16_t(le(b,j+16,2))}};
      }
    }
    if(kind==5) {
      require(size%10==0,"Invalid font kerning block");
      for(size_t j=p;j<p+size;j+=10) result["kernings"][std::to_string(le(b,j,4))+":"+std::to_string(le(b,j+4,4))]=int16_t(le(b,j+8,2));
    }
    p+=size;
  }
  require(result.value("fontSize",0)>0 && result.value("lineHeight",0)>0 && !result["glyphs"].empty(),"Missing font metrics");
  for(auto& g:result["glyphs"]) require(g["x"].get<unsigned>()+g["width"].get<unsigned>()<=result["width"].get<unsigned>() && g["y"].get<unsigned>()+g["height"].get<unsigned>()<=result["height"].get<unsigned>(),"Glyph outside atlas");
  writeJSON(out/("font-"+name+".json"),result); write(out/("font-"+name+".png"),Bytes(b.begin()+end,b.end()));
}
void prepareSkin(const FileMap& files,const std::string& ui,const std::filesystem::path& out) {
  auto root="usr/share/ui/"+ui+"/resources/";
  auto read=[&](const std::string& relative)->const Bytes& {
    auto i=files.find(root+relative); if(i==files.end()) throw std::runtime_error("Image missing skin member: "+relative); return i->second;
  };
  auto json=[&](const std::string& relative) { auto& b=read(relative); return Json::parse(b.begin(),b.end()); };
  for(auto name:{"packed.png","packed.json"}) write(out/name,read(std::string("sprites/")+name));
  write(out/"wall.png",read("layout/asset/sprite/wallpaper/wall.png"));
  write(out/"cursor-thumbnails.json",read("animation/cursor_thumbnails.spriteanim"));
  for(auto name:{"bgm_home","se_sys_cursor","se_sys_click"}) write(out/(std::string(name)+".wav"),read(std::string("sounds/hvc/")+name+".wav"));
  for(auto name:{"title","m","s","l","copyright"}) {
    auto relative=std::string("fonts/")+name+".font";
    if(!files.contains(root+relative)) relative=std::string("fonts/")+name+".fnt";
    font(read(relative),out,name);
  }
  // Stock strings are inert JSON, not Lua. Keep each image's own locale and
  // font resources together; Japanese bitmap fonts use .fnt Reed containers.
  std::string locale=ui=="shvc" ? "jpn_ja":"usa_en";
  auto strings=json("strings/"+locale+"/strings.lng").at("strings");
  require(strings.is_object() && strings.size()<5000,"Invalid localized string table");
  for(auto& value:strings) require(value.is_string(),"Invalid localized string value");
  writeJSON(out/"strings.json",{{"locale",locale},{"strings",strings},{"source","usr/share/ui/"+ui+"/resources/strings/"+locale+"/strings.lng"}});
  auto palette=ui=="shvc" ? "jpn":ui=="snes-usa" ? "usa":"eur";
  auto palettePath=std::string("scripts/env/colors_")+palette+".lua";
  auto paletteHash=digest(read(palettePath));
  auto expectedPalette=ui=="shvc" ? "8a53966a583c45c115fe4d55aff76a56bb63b032512d1570f3879ba09c24a914":ui=="snes-usa" ? "1181691260ecb050374b10676c4d4c657dafbd143e886385853607949dd4951d":"cd85ce84d70c31f83e2a17739e3869eae7790e973851fd4e8c03977ff609073a";
  require(paletteHash==expectedPalette,"Unverified regional palette revision");
  // Values statically decoded from the hash-identified tables; target Lua is
  // never executed. Japanese title/HUD colors are the reverse of EUR/USA.
  writeJSON(out/"theme.json",{{"ui",ui},{"locale",locale},{"region",ui=="shvc" ? "Japan":ui=="snes-usa" ? "USA":"Europe"},
    {"colors",{{"title",ui=="shvc" ? Json::array({255,255,255,255}):Json::array({0,0,0,255})},{"hud",ui=="shvc" ? Json::array({0,0,0,255}):Json::array({255,255,255,255})}}},
    {"paletteSource",root+palettePath},{"paletteSHA256",paletteHash}});
  auto scene=json("defaultscene.scn");
  std::map<std::string,Json> nodes;
  std::map<std::string,std::array<double,6>> world;
  using Matrix=std::array<double,6>;
  std::function<void(const Json&,std::string,Matrix,unsigned)> visit=[&](const Json& node,std::string path,Matrix m,unsigned depth) {
    require(depth<100 && nodes.size()<30000,"Scene exceeds import limits");
    path+="/"+node.at("name").get<std::string>(); nodes[path]=node;
    auto t=node.at("transform").get<Matrix>(); auto [a,b,c,d,e,f]=m; auto [A,B,C,D,E,F]=t;
    Matrix w{a*A+b*D,a*B+b*E,a*C+b*F+c,d*A+e*D,d*B+e*E,d*C+e*F+f}; world[path]=w;
    if(node.contains("children")) for(auto& child:node["children"]) visit(child,path,w,depth+1);
  };
  visit(scene.at("rootWorldNode"),"",{1,0,0,0,1,0},0);
  auto position=[&](const std::string& path) { auto w=world.at(path); return Json::array({640+w[2],360-w[5]}); };
  auto component=[&](const std::string& path,const std::string& kind) {
    for(auto& c:nodes.at(path).at("components")) if(c.at("type")==kind) return c;
    throw std::runtime_error("Missing scene component: "+path);
  };
  auto home=std::string("/Main/homemenu"), hud=home+"/hud/hud_gametitle", caption=home+"/elements/menubar_upper/elements/sys_menubar_cm5/caption_down";
  Json positions,labels;
  for(auto [key,path]:std::map<std::string,std::string>{{"title",home+"/gametitle"},{"thumbnails",home+"/thumbnails"},{"hud",home+"/hud"},{"bottomBar",home+"/menubar"},{"topBar",home+"/elements/menubar_upper/menubar"},{"topButtons",home+"/elements/menubar_upper/elements"}}) positions[key]=position(path);
  auto depsIt=files.find("usr/share/ui/"+ui+"/dependencies.json"); require(depsIt!=files.end(),"Missing scene dependencies");
  auto dependencies=Json::parse(depsIt->second.begin(),depsIt->second.end());
  std::map<std::string,std::string> fonts;
  for(auto& r:dependencies.at("resources")) if(r.at("type")=="FontResource") fonts[r.at("id").get<std::string>()]=std::filesystem::path(r.at("path").get<std::string>()).stem().string();
  for(auto [key,path]:std::map<std::string,std::string>{{"title",home+"/gametitle/gametitle_label"},{"hud",hud+"/hud_item1_menuU/hud_2_label"},{"tooltip",caption+"/Label"}}) {
    auto c=component(path,"LabelComponent"); std::string name=fonts.at(c.at("fontResource").get<std::string>());
    if(c.contains("properties")) name=c["properties"].value("fontName",name);
    labels[key]={{"position",position(path)},{"font",name},{"horizontalAnchor",c.at("horizontalAnchor")},{"verticalAnchor",c.at("verticalAnchor")}};
  }
  auto cap=component(caption+"/caption","SpriteComponent"), edge=component(caption+"/caption_r","SpriteComponent");
  auto list=nodes.at(home+"/elements/cardlist");
  auto marker=[&](const std::string& key) {
    auto id=list.at("properties").at(key).at("linkId");
    for(auto& [_,n]:nodes) if(n.at("id")==id) return n.at("transform")[2].get<double>();
    throw std::runtime_error("Missing card scene marker");
  };
  auto card=json("prefab/sys_game_card.scn").at("rootWorldNode"); Json cardSize;
  for(auto& n:card.at("children")) if(n.at("name")=="non_active") for(auto& child:n.at("children")) if(child.at("name")=="card") cardSize=child.at("components")[0].at("size");
  require(!cardSize.is_null(),"Missing card size");
  Json layout={{"width",1280},{"height",720},{"positions",positions},{"labels",labels},
    {"hudItemGap",nodes.at(hud+"/hud_item1_menuU").at("properties").at("VGap")},{"hudGroupGap",nodes.at(hud).at("properties").at("VGap")},
    {"tooltip",{{"bodyY",position(caption+"/caption")[1]},{"minimumWidth",cap.at("size")[0]},{"height",cap.at("size")[1]},{"edgeWidth",edge.at("size")[0]}}},
    {"cardSize",cardSize},{"cardSpacing",marker("narabe2")-marker("narabe1")},{"wallpaperDuration",json("layout/lyt_wallpaper_default.sceneanimation").at("duration")}};
  require(layout["cardSpacing"].get<double>()>0 && layout["wallpaperDuration"].get<double>()>0,"Invalid scene timing/spacing");
  writeJSON(out/"layout.json",layout);
}
}
