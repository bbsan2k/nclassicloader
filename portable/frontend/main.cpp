#define STB_IMAGE_IMPLEMENTATION
#define STBI_ONLY_PNG
#define STBI_MAX_DIMENSIONS 8192
#define STBI_WINDOWS_UTF8
#define STB_IMAGE_WRITE_IMPLEMENTATION
#define STBIW_WINDOWS_UTF8
#include <stb_image_write.h>
#undef STB_IMAGE_WRITE_IMPLEMENTATION
#include "assets.hpp"
#include "launcher.hpp"
#include "native_menu.hpp"
#include "ares_core.h"
#include <SDL3/SDL_main.h>
#include <chrono>
#include <iostream>
#include <optional>
#include <set>
#include <sstream>
#include <tuple>
#include <array>

namespace clover::ui {
struct Game {
  std::string id, title, sortTitle, publisher, release, copyright, bundledROM, importError;
  int players = 1;
  bool simultaneous = false;
};
struct Hit { std::string action; SDL_FRect rect; };
struct DialogResult { std::string game, path; bool firmware = false, start = false; };
struct DialogRequest { Uint32 event; std::string game; bool firmware, start; };
static void SDLCALL fileDialog(void* userdata, const char* const* files, int) {
  std::unique_ptr<DialogRequest> request(static_cast<DialogRequest*>(userdata));
  if(!files || !files[0]) return;
  auto* result = new DialogResult{request->game, files[0], request->firmware, request->start};
  SDL_Event event{}; event.type = request->event; event.user.data1 = result;
  if(!SDL_PushEvent(&event)) delete result;
}
class Audio {
  SDL_AudioStream* stream_ = nullptr;
  std::vector<uint8_t> samples_;
  std::string error_;
public:
  Audio() = default;
  ~Audio() { if(stream_) SDL_DestroyAudioStream(stream_); }
  Audio(const Audio&) = delete;
  void openCore(unsigned rate) {
    SDL_AudioSpec spec{SDL_AUDIO_F32, 2, static_cast<int>(rate)};
    stream_ = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,&spec,nullptr,nullptr);
    if(stream_) { if(!SDL_ResumeAudioStreamDevice(stream_)) error_=SDL_GetError(); }
    else error_=SDL_GetError();
  }
  void openWAV(const fs::path& path) {
    SDL_AudioSpec spec{}; Uint8* data = nullptr; Uint32 size = 0;
    if(!SDL_LoadWAV(utf8(path).c_str(),&spec,&data,&size)) { error_=SDL_GetError();return; }
    samples_.assign(data,data+size); SDL_free(data);
    stream_ = SDL_OpenAudioDeviceStream(SDL_AUDIO_DEVICE_DEFAULT_PLAYBACK,&spec,nullptr,nullptr);
    if(stream_) { if(!SDL_ResumeAudioStreamDevice(stream_)) error_=SDL_GetError(); }
    else error_=SDL_GetError();
  }
  bool available() const { return stream_ != nullptr; }
  const std::string& error() const { return error_; }
  int queued() const { return stream_ ? SDL_GetAudioStreamQueued(stream_) : 0; }
  void clear() { if(stream_) SDL_ClearAudioStream(stream_); }
  void play(bool loop = false) {
    if(!stream_ || samples_.empty()) return;
    // Refill before the current clip drains; waiting for zero introduced a
    // frame-sized hole at every music loop boundary.
    if(loop && queued() > int(samples_.size()/2)) return;
    if(!loop) clear();
    if(!SDL_PutAudioStreamData(stream_,samples_.data(),static_cast<int>(samples_.size()))) error_=SDL_GetError();
  }
  void put(const std::vector<float>& samples) {
    if(stream_ && !samples.empty() && !SDL_PutAudioStreamData(stream_,samples.data(),static_cast<int>(samples.size()*sizeof(float)))) error_=SDL_GetError();
  }
};
static void writeJSON(const fs::path& path, const Json& data) {
  fs::create_directories(path.parent_path());
  auto temporary = path; temporary += ".tmp";
  { std::ofstream stream(temporary,std::ios::binary); stream << data.dump(2); stream.flush(); if(!stream) throw std::runtime_error("Cannot write settings"); }
  std::error_code error; fs::rename(temporary,path,error);
  if(error) {
    auto backup = path; backup += ".previous";
    fs::remove(backup,error); error.clear();
    if(fs::exists(path)) fs::rename(path,backup);
    fs::rename(temporary,path);
  }
}
static bool safeID(const std::string& value) {
  return !value.empty() && value.size()<100 && std::all_of(value.begin(),value.end(),[](unsigned char c) { return (c>='A' && c<='Z') || (c>='a' && c<='z') || (c>='0' && c<='9') || c=='-'; });
}
class App {
  SDL_Window* window_ = nullptr;
  SDL_Renderer* renderer_ = nullptr;
  SDL_Texture* frameTexture_ = nullptr;
  SDL_Texture* stageTexture_ = nullptr;
  SDL_Gamepad* gamepad_ = nullptr;
  std::unique_ptr<Assets> art_;
  std::unique_ptr<Audio> coreAudio_, music_, click_;
  AresCore core_;
  fs::path assets_, data_, firmware_;
  Json preferences_ = Json::object();
  std::vector<Game> games_;
  std::vector<Hit> hits_;
  std::string selected_, panel_, hover_, message_, runningGame_;
  fs::path runningROM_;
  Uint32 dialogEvent_ = 0;
  int sort_ = 0, slot_ = 0, focus_ = 0;
  bool running_ = true, playing_ = false, sound_ = false, motion_ = true, hidden_ = false, switchProfile_ = false, browseImage_ = false, vsync_ = false, scrollTest_ = false;
  std::map<std::string,std::array<bool,4>> savedSlots_;
  double audioPeak_=0;
  uint64_t audioSamples_=0;
  double start_ = 0, scrollStarted_ = 0, scrollFrom_ = 0, scrollTo_ = 0, scrollCurrent_ = 0, duration_ = .24, noticeUntil_ = 0;
  unsigned textureWidth_ = 0, textureHeight_ = 0;
  std::vector<std::string> actions_;
  inline static const std::vector<std::string> sorts_{"Title","Players","Recently played","Play count","Release date","Publisher"};
  std::string tr(const std::string& key,const std::string& fallback) const { return art_->tr(key,fallback); }
  std::string sortName() const {
    static const std::array<std::string,6> keys{"TitleName","Players","RecentPlay","PlayCount","ReleaseDate","Manufacturer"};
    return tr("sys_sort_order_"+keys[sort_],sorts_[sort_]);
  }

  static double now() { return static_cast<double>(SDL_GetTicks())/1000; }
  static int wrap(int i,int count) { return count ? (i%count+count)%count : 0; }
  Game& selected() {
    auto i=std::find_if(games_.begin(),games_.end(),[&](auto& g){return g.id==selected_;});
    return i==games_.end() ? games_.front() : *i;
  }
  int index() const {
    for(size_t i=0;i<games_.size();++i) if(games_[i].id==selected_) return static_cast<int>(i);
    return 0;
  }
  Json record(const Game& game) const {
    if(preferences_.contains("history") && preferences_["history"].contains(game.id)) return preferences_["history"][game.id];
    return Json::object();
  }
  void sortGames() {
    std::sort(games_.begin(),games_.end(),[&](const Game& a,const Game& b) {
      if(sort_==1 && a.players!=b.players) return a.players>b.players;
      if(sort_==2 || sort_==3) {
        auto key=sort_==2 ? "recent" : "count";
        auto av=record(a).value(key,int64_t(0)), bv=record(b).value(key,int64_t(0));
        if(av!=bv) return av>bv;
      }
      if(sort_==4 && a.release!=b.release) return a.release<b.release;
      if(sort_==5 && a.publisher!=b.publisher) return a.publisher<b.publisher;
      return a.sortTitle!=b.sortTitle ? a.sortTitle<b.sortTitle : a.id<b.id;
    });
    scrollCurrent_=scrollFrom_=scrollTo_=index();
  }
  void saveSettings() {
    preferences_["schema"]=1; preferences_["selected"]=selected_; preferences_["sort"]=sort_;
    preferences_["slot"]=slot_; preferences_["sound"]=sound_; preferences_["motion"]=motion_; preferences_["firmware"]=utf8(firmware_);
    writeJSON(data_/"preferences.json",preferences_);
  }
  void alert(std::string message) { message_=std::move(message); panel_="message"; focus_=0; hover_.clear(); }
  fs::path saveFolder(const Game& game) const { return data_/"saves"/game.id; }
  fs::path statePath(const Game& game,int slot) const { return saveFolder(game)/("slot-"+std::to_string(slot+1)+".state"); }
  void refreshSlots() {
    for(auto& game:games_) for(int slot=0;slot<4;++slot) savedSlots_[game.id][slot]=fs::exists(statePath(game,slot));
  }
  fs::path resolveROM(const Game& game) const {
    if(preferences_.contains("roms") && preferences_["roms"].contains(game.id)) {
      auto path=pathFromUTF8(preferences_["roms"][game.id].get<std::string>());
      if(fs::is_regular_file(path)) return path;
    }
    if(!game.bundledROM.empty()) {
      auto relative=pathFromUTF8(game.bundledROM);
      bool safe=!relative.is_absolute(); for(const auto& part:relative) if(part=="..") safe=false;
      if(safe && fs::is_regular_file(assets_/relative)) return assets_/relative;
    }
    return {};
  }
  void chooseROM(bool start) {
    static const SDL_DialogFileFilter filters[]{{"Super Nintendo ROM","sfc;smc"}};
    auto* request=new DialogRequest{dialogEvent_,selected_,false,start};
    SDL_ShowOpenFileDialog(fileDialog,request,window_,filters,1,nullptr,false);
  }
  void chooseFirmware() {
    auto* request=new DialogRequest{dialogEvent_,selected_,true,false};
    SDL_ShowOpenFolderDialog(fileDialog,request,window_,nullptr,false);
  }
  void beginGame(bool resume=false) {
    auto& game=selected(); auto rom=resolveROM(game);
    if(rom.empty()) {
      if(!game.importError.empty()) alert("Image import: "+game.importError+" You can choose a compatible ROM in Settings.");
      else if(resume) alert("Choose this game's compatible ROM in Settings before loading a suspend point.");
      else chooseROM(true);
      return;
    }
    if(!core_.loaded() || runningGame_!=game.id || runningROM_!=rom) {
      coreAudio_.reset();
      auto systemFirmware=fs::is_regular_file(assets_/"firmware/ipl.rom") ? assets_/"firmware":firmware_;
      if(!core_.load(rom,saveFolder(game),firmware_,systemFirmware)) { alert(core_.error()); return; }
      runningGame_=game.id; runningROM_=rom;
      coreAudio_=std::make_unique<Audio>(); coreAudio_->openCore(core_.audioRate());
      if(!coreAudio_->error().empty()) std::cerr<<"Audio output: "<<coreAudio_->error()<<"\n";
    }
    if(resume && !core_.loadState(statePath(game,slot_))) { alert(core_.error()); return; }
    if(coreAudio_) coreAudio_->clear();
    music_->clear(); panel_.clear(); playing_=true;
    preferences_["history"][game.id]["count"]=record(game).value("count",int64_t(0))+1;
    preferences_["history"][game.id]["recent"]=static_cast<int64_t>(std::chrono::system_clock::to_time_t(std::chrono::system_clock::now()));
    saveSettings();
  }
  void saveSlot() {
    if(!core_.loaded() || runningGame_!=selected_) { alert("Start the selected game before saving a suspend point."); return; }
    auto path=statePath(selected(),slot_); fs::create_directories(path.parent_path());
    if(!core_.saveState(path)) { playing_=false; if(coreAudio_) coreAudio_->clear(); alert(core_.error()); return; }
    savedSlots_[selected_][slot_]=true;
    const auto& frame=core_.frame();
    if(frame.width && frame.height && frame.pixels.size()>=size_t(frame.width)*frame.height) {
      std::vector<uint8_t> rgba(size_t(frame.width)*frame.height*4);
      for(size_t i=0;i<size_t(frame.width)*frame.height;++i) {
        auto value=frame.pixels[i]; rgba[i*4]=value>>16; rgba[i*4+1]=value>>8; rgba[i*4+2]=value; rgba[i*4+3]=255;
      }
      auto image=path; image.replace_extension("png");
      stbi_write_png(utf8(image).c_str(),frame.width,frame.height,4,rgba.data(),frame.width*4);
    }
    noticeUntil_=now()+1.3;
  }
  void toMenu() {
    if(playing_) saveSlot();
    playing_=false; if(coreAudio_) coreAudio_->clear();
    sortGames();
  }
  uint16_t buttons() const {
    auto* keys=SDL_GetKeyboardState(nullptr); uint16_t mask=0;
    const SDL_Scancode keyMap[]{SDL_SCANCODE_Z,SDL_SCANCODE_A,SDL_SCANCODE_BACKSPACE,SDL_SCANCODE_RETURN,SDL_SCANCODE_UP,SDL_SCANCODE_DOWN,SDL_SCANCODE_LEFT,SDL_SCANCODE_RIGHT,SDL_SCANCODE_X,SDL_SCANCODE_S,SDL_SCANCODE_Q,SDL_SCANCODE_W};
    const SDL_GamepadButton padMap[]{SDL_GAMEPAD_BUTTON_SOUTH,SDL_GAMEPAD_BUTTON_WEST,SDL_GAMEPAD_BUTTON_BACK,SDL_GAMEPAD_BUTTON_START,SDL_GAMEPAD_BUTTON_DPAD_UP,SDL_GAMEPAD_BUTTON_DPAD_DOWN,SDL_GAMEPAD_BUTTON_DPAD_LEFT,SDL_GAMEPAD_BUTTON_DPAD_RIGHT,SDL_GAMEPAD_BUTTON_EAST,SDL_GAMEPAD_BUTTON_NORTH,SDL_GAMEPAD_BUTTON_LEFT_SHOULDER,SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER};
    for(int i=0;i<12;++i) if(keys[keyMap[i]] || (gamepad_ && SDL_GetGamepadButton(gamepad_,padMap[i]))) mask|=uint16_t(1u<<i);
    return mask;
  }
  void move(int amount,bool repeat=false) {
    auto old=index(); selected_=games_[wrap(old+amount,int(games_.size()))].id;
    double target=index(), n=games_.size(); target+=std::round((scrollCurrent_-target)/n)*n;
    scrollFrom_=scrollCurrent_; scrollTo_=target; scrollStarted_=now(); duration_=(repeat ? .09 : .24)*std::min(1.0,std::abs(target-scrollCurrent_));
    if(sound_) click_->play();
  }
  void hit(std::string action,SDL_FRect rect) { hits_.push_back({std::move(action),rect}); }
  void sprite(const std::string& name,float x,float y,float w,float h) { art_->sprite(name,x,y,w,h); }
  void text(const std::string& value,float x,float y,std::string font="m",float width=1200,SDL_Color color={255,255,255,255},bool line=false) { art_->text(value,x,y,font,color,width,line); }
  void dialog(float x,float y,float w,float h) {
    SDL_SetRenderDrawColor(renderer_,0,0,0,255); SDL_FRect rect{x,y,w,h}; SDL_RenderFillRect(renderer_,&rect);
    SDL_SetRenderDrawColor(renderer_,255,255,255,255);
    for(int i=0;i<3;++i) { SDL_FRect border{x+float(i),y+float(i),w-2*i,h-2*i}; SDL_RenderRect(renderer_,&border); }
  }
  void drawGame() {
    const auto& frame=core_.frame();
    if(!frame.width || !frame.height || frame.pixels.empty()) return;
    if(!frameTexture_ || textureWidth_!=frame.width || textureHeight_!=frame.height) {
      if(frameTexture_) SDL_DestroyTexture(frameTexture_);
      frameTexture_=SDL_CreateTexture(renderer_,SDL_PIXELFORMAT_ARGB8888,SDL_TEXTUREACCESS_STREAMING,frame.width,frame.height);
      textureWidth_=frame.width; textureHeight_=frame.height;
      if(!frameTexture_) throw std::runtime_error(SDL_GetError());
    }
    SDL_UpdateTexture(frameTexture_,nullptr,frame.pixels.data(),frame.width*sizeof(uint32_t));
    SDL_SetTextureScaleMode(frameTexture_,SDL_SCALEMODE_NEAREST);
    SDL_FRect destination{160,0,960,720}; SDL_RenderTexture(renderer_,frameTexture_,nullptr,&destination);
  }
  void drawPanel() {
    SDL_SetRenderDrawBlendMode(renderer_,SDL_BLENDMODE_BLEND); SDL_SetRenderDrawColor(renderer_,0,0,0,190);
    SDL_FRect dim{0,96,1280,550}; SDL_RenderFillRect(renderer_,&dim);
    dialog(200,128,880,478); hits_.clear(); actions_.clear();
    std::string heading; std::vector<std::string> lines; std::vector<std::pair<std::string,std::string>> entries;
    if(panel_=="message") { heading="Classic Menu"; lines={message_}; }
    else if(panel_=="toolbar:0") { heading=tr("sys_display_Title","Display"); lines={"Original 1280 x 720 Clover stage","SNES games use a 4:3 display area."}; entries={{"fullscreen","Toggle full screen"},{"motion",motion_ ? "Animation: On":"Animation: Off"}}; }
    else if(panel_=="toolbar:1") { heading=tr("sys_option_Title","Settings"); entries={{"rom","Choose ROM for selected game"},{"firmware","Choose coprocessor firmware folder"},{"sound",sound_ ? "Menu sound: On":"Menu sound: Off"},{"slot","Suspend slot: "+std::to_string(slot_+1)},{"profiles","Image library / switch profile (F2)"}}; }
    else if(panel_=="toolbar:2") { heading=tr("sys_language_hud_Title","Language"); lines={art_->region+" / "+art_->locale,"Labels, artwork and bitmap fonts come from this image.","Custom emulator/image controls remain English."}; }
    else if(panel_=="toolbar:3") { heading=selected().title; lines={selected().publisher+"    "+selected().release,selected().copyright,"Core: ares (embedded, no external emulator)"}; }
    else if(panel_=="resume") {
      heading=tr("sys_resume_Title","Suspend points");
      lines={"Slot "+std::to_string(slot_+1)+(fs::exists(statePath(selected(),slot_)) ? " - Saved":" - Empty"),"These are ares states, not Canoe states."};
      entries={{"slot","Choose next slot"},{"resume","Resume selected slot"},{"save","Save current session to this slot"}};
    } else { heading="Classic Menu"; lines={"Arrows: select    Return: start    Tab: sort","Up: settings    Down: suspend points","Game: Z B, X A, A Y, S X, Q L, W R","Return: Start    Backspace: Select","Escape: save and return to menu","F5: save slot    F8: load slot"}; }
    text(heading,640,174,"l",790);
    float y=227;
    std::string bodyFont=panel_=="message" ? "s":"m";
    float lineStep=panel_=="message" ? 28:35;
    for(auto& line:lines) {
      // Wrap messages by measured word widths instead of shrinking whole paragraphs.
      std::string part,word; std::istringstream words(line);
      while(words>>word) {
        auto candidate=part.empty() ? word : part+" "+word;
        if(art_->measure(candidate,bodyFont).advance>770 && !part.empty()) { text(part,640,y,bodyFont,790); y+=lineStep; part=word; } else part=candidate;
      }
      if(!part.empty()) { text(part,640,y,bodyFont,790); y+=lineStep+4; }
    }
    focus_=std::clamp(focus_,0,int(entries.size()));
    for(size_t i=0;i<entries.size();++i) {
      auto [action,label]=entries[i];
      SDL_FRect rect{365,y-14,550,43};
      if(hover_==action || (hover_.empty() && focus_==int(i))) sprite("btn_init_on",640,rect.y+rect.h/2,570,42);
      text(label,640,rect.y+rect.h/2,"m",520); hit(action,rect); actions_.push_back(action); y+=49;
    }
    auto back=tr("sys_option_hud_Return","Back");
    auto labelWidth=art_->measure(back,"s").advance; float total=36+4+labelWidth;
    sprite("icon_B_light",640-total/2+18,567,36,36); text(back,640-total/2+40+labelWidth/2,567,"s");
    hit("close",{640-total/2-12,543,total+24,48}); actions_.push_back("close");
  }
  void drawMenu() {
    auto time=now()-start_;
    double progress=duration_>0 ? std::clamp((now()-scrollStarted_)/duration_,0.0,1.0) : 1;
    scrollCurrent_=motion_ ? scrollFrom_+(scrollTo_-scrollFrom_)*progress : scrollTo_;
    float wallpaperDuration=art_->layout.at("wallpaperDuration");
    float offset=motion_ ? float(std::fmod(time,wallpaperDuration)/wallpaperDuration*1536) : 0;
    auto* wall=art_->image(assets_/"skin/wall.png");
    for(int i=-1;i<=1;++i) art_->draw(wall,{-128-offset+i*1536,87,1536,552},nullptr,{255,255,255,255},true);
    sprite("caption_title",640,178,1044,66); text(selected().title,640,176,"title",990,art_->titleColor,true);
    float gap=art_->layout.at("cardSpacing");
    for(int i=int(std::floor(scrollCurrent_))-3;i<=int(std::floor(scrollCurrent_))+3;++i) {
      auto& game=games_[wrap(i,int(games_.size()))]; float x=std::round(640+float(i-scrollCurrent_)*gap);
      if(x< -150 || x>1430) continue;
      bool active=i==int(scrollTo_);
      sprite(active ? "game_card_active":"game_card",x,360,252,276);
      auto* cover=art_->image(assets_/"games"/game.id/(game.id+".png")); art_->fitImage(cover,{x-114,234,228,204});
      sprite(game.players>1 ? (game.simultaneous ? "icon_2P_simultaneous":"icon_1P-2P") : "icon_1P",x+81,462,66,42);
      for(int n=0;n<4;++n) {
        bool saved=savedSlots_[game.id][n];
        sprite(std::string(saved ? "icon_Resume_on":"icon_Resume_off")+(active ? "_active":""),x-96+n*24,468,18,18);
      }
      hit("game:"+game.id,{x-126,222,252,276});
    }
    art_->selection(640+float(scrollTo_-scrollCurrent_)*gap,360,278,302,motion_ ? float(time) : .5f);
    sprite("arrow_left",38,360,32,44); sprite("arrow_right",1242,360,32,44);
    hit("previous",{0,240,65,255}); hit("next",{1215,240,65,255});
    float step=std::min(42.0f,1080.0f/float(games_.size())), left=640-float(games_.size()-1)*step/2;
    for(size_t i=0;i<games_.size();++i) {
      auto& game=games_[i]; float x=left+i*step;
      auto* cover=art_->image(assets_/"games"/game.id/(game.id+"_small.png"));
      art_->fitImage(cover,{x-16,535,32,32},{255,255,255,uint8_t(game.id==selected_ ? 255:190)});
      if(game.id==selected_) sprite("curor_thumbnail_1",x,522,24,16);
      hit("game:"+game.id,{x-step/2,526,step,55});
    }
    const std::vector<std::tuple<std::string,std::string,float,std::string>> prompts{
      {"icon_direction_up_light",tr("sys_gametitlelist_hud_Menu","Menu"),36,"toolbar:1"},{"icon_direction_down_light",tr("sys_gametitlelist_hud_Resume","Resume"),36,"panel:resume"},
      {"icon_select_light",tr("sys_gametitlelist_hud_Sort","Sort"),96,"sort"},{"icon_start_light",tr("sys_gametitlelist_hud_Decide","Start"),84,"start"}};
    float itemGap=art_->layout.at("hudItemGap"), groupGap=art_->layout.at("hudGroupGap"), width=0;
    for(auto& [_,label,w,__]:prompts) width+=w+itemGap+art_->measure(label,"s").advance;
    width+=groupGap*3; float x=640-width/2;
    for(auto& [icon,label,w,action]:prompts) {
      auto size=art_->measure(label,"s").advance;
      sprite(icon,x+w/2,609,w,w==36 ? 36:30); text(label,x+w+itemGap+size/2,609,"s",1200,art_->hudColor,true);
      hit(action,{x-5,585,w+itemGap+size+10,48}); x+=w+itemGap+size+groupGap;
    }
    sprite("menubar_U",640,36,1296,120); sprite("menubar_L",640,690,1296,120);
    const std::vector<std::string> icons{"btn_display","btn_option","btn_language","btn_copyright","manual"};
    for(int i=0;i<5;++i) {
      float bx=640+(i-2)*96; auto action="toolbar:"+std::to_string(i);
      if(hover_==action || panel_==action) sprite("btn_menubar_active",bx,62,96,70);
      sprite(icons[i],bx,62,72,72); hit(action,{bx-46,27,92,70});
      if(hover_==action && panel_.empty()) {
        const std::vector<std::string> labels{tr("sys_menubarU_cm_Display","Display"),tr("sys_menubarU_cm_Option","Settings"),tr("sys_menubarU_cm_Language","Language"),tr("sys_menubarU_cm_Copyright","Game info"),tr("sys_menubarU_cm_Manual","Help")};
        auto& style=art_->layout.at("tooltip"); float bodyY=style.at("bodyY"), height=style.at("height"), edge=style.at("edgeWidth");
        float body=std::max(float(style.at("minimumWidth")),art_->measure(labels[i],"s").advance);
        sprite("caption",bx,bodyY,body,height);
        art_->sprite("caption_edge",bx-body/2-edge/2,bodyY,edge,height,{255,255,255,255},true);
        sprite("caption_edge",bx+body/2+edge/2,bodyY,edge,height);
        sprite("caption_arrow",bx,bodyY-height/2+9,24,24);
        text(labels[i],bx,art_->layout.at("labels").at("tooltip").at("position")[1],"s",1200,{255,255,255,255},true);
      }
    }
    if(now()<noticeUntil_ && panel_.empty()) { dialog(304,312,672,96); text(sortName(),640,356,"l",610); }
    if(!panel_.empty()) drawPanel();
  }
  void activate(const std::string& action) {
    if(sound_) click_->play();
    if(action.starts_with("game:")) {
      auto id=action.substr(5); if(safeID(id)) { selected_=id; scrollTo_=scrollFrom_=scrollCurrent_=index(); }
    } else if(action.starts_with("toolbar:")) { panel_=action; focus_=0; hover_.clear(); }
    else if(action=="panel:resume") { panel_="resume"; focus_=0; hover_.clear(); }
    else if(action=="close") { panel_.clear(); hover_.clear(); }
    else if(action=="previous") move(-1);
    else if(action=="next") move(1);
    else if(action=="start") beginGame();
    else if(action=="rom") chooseROM(false);
    else if(action=="firmware") chooseFirmware();
    else if(action=="profiles") { switchProfile_=true; running_=false; }
    else if(action=="resume") beginGame(true);
    else if(action=="save") saveSlot();
    else if(action=="slot") { slot_=(slot_+1)%4; saveSettings(); }
    else if(action=="sound") { sound_=!sound_; music_->clear(); saveSettings(); }
    else if(action=="motion") { motion_=!motion_; saveSettings(); }
    else if(action=="fullscreen") SDL_SetWindowFullscreen(window_,!(SDL_GetWindowFlags(window_)&SDL_WINDOW_FULLSCREEN));
    else if(action=="sort") { sort_=(sort_+1)%int(sorts_.size()); sortGames(); noticeUntil_=now()+1.3; saveSettings(); }
  }
  void key(SDL_Scancode code,bool repeat) {
    if(code==SDL_SCANCODE_F2 && !repeat && !playing_) { switchProfile_=true; running_=false; return; }
    if(playing_) {
      if(code==SDL_SCANCODE_ESCAPE) toMenu();
      else if(code==SDL_SCANCODE_F5 && !repeat) saveSlot();
      else if(code==SDL_SCANCODE_F8 && !repeat) {
        if(!core_.loadState(statePath(selected(),slot_))) { playing_=false; alert(core_.error()); }
        if(coreAudio_) coreAudio_->clear();
      }
      return;
    }
    if(code==SDL_SCANCODE_ESCAPE) { panel_.clear(); return; }
    if(!panel_.empty()) {
      if(code==SDL_SCANCODE_RETURN && !actions_.empty()) activate(actions_[std::clamp(focus_,0,int(actions_.size()-1))]);
      else if(code==SDL_SCANCODE_DOWN || code==SDL_SCANCODE_UP || code==SDL_SCANCODE_TAB) { focus_=wrap(focus_+(code==SDL_SCANCODE_UP ? -1:1),int(actions_.size())); hover_.clear(); }
      return;
    }
    if(code==SDL_SCANCODE_LEFT) move(-1,repeat);
    else if(code==SDL_SCANCODE_RIGHT) move(1,repeat);
    else if(code==SDL_SCANCODE_RETURN) beginGame();
    else if(code==SDL_SCANCODE_UP) activate("toolbar:1");
    else if(code==SDL_SCANCODE_DOWN) activate("panel:resume");
    else if(code==SDL_SCANCODE_TAB) activate("sort");
    else if(code==SDL_SCANCODE_M && !repeat) activate("sound");
  }
  void events() {
    SDL_Event event;
    while(SDL_PollEvent(&event)) {
      if(event.type==SDL_EVENT_QUIT) running_=false;
      else if(event.type==nativeMenuEvent()) {
        auto command=MenuCommand(event.user.code);
        if(command==MenuCommand::quit) { running_=false;continue; }
        if(playing_) toMenu();
        switch(command) {
          case MenuCommand::library: activate("profiles");break;
          case MenuCommand::importImage: browseImage_=true;activate("profiles");break;
          case MenuCommand::settings: activate("toolbar:1");break;
          case MenuCommand::chooseROM: chooseROM(false);break;
          case MenuCommand::firmware: chooseFirmware();break;
          case MenuCommand::sort: activate("sort");break;
          case MenuCommand::sound: activate("sound");break;
          case MenuCommand::fullscreen: activate("fullscreen");break;
          case MenuCommand::help: activate("toolbar:4");break;
          default: break;
        }
      }
      else if(event.type==dialogEvent_) {
        std::unique_ptr<DialogResult> result(static_cast<DialogResult*>(event.user.data1));
        if(result->firmware) firmware_=pathFromUTF8(result->path);
        else {
          preferences_["roms"][result->game]=result->path;
          selected_=result->game; scrollTo_=scrollFrom_=scrollCurrent_=index();
        }
        saveSettings();
        if(result->start) beginGame();
      } else if(event.type==SDL_EVENT_KEY_DOWN) key(event.key.scancode,event.key.repeat);
      else if(event.type==SDL_EVENT_DROP_FILE) {
        auto path=pathFromUTF8(event.drop.data); auto extension=utf8(path.extension());
        if(extension==".sfc" || extension==".smc") { preferences_["roms"][selected_]=utf8(path); saveSettings(); beginGame(); }
        else alert("Choose a compatible .sfc or .smc ROM. SFROM conversion is not implemented.");
      } else if(event.type==SDL_EVENT_GAMEPAD_ADDED && !gamepad_) gamepad_=SDL_OpenGamepad(event.gdevice.which);
      else if(event.type==SDL_EVENT_GAMEPAD_REMOVED && gamepad_ && event.gdevice.which==SDL_GetGamepadID(gamepad_)) { SDL_CloseGamepad(gamepad_); gamepad_=nullptr; }
      else if(event.type==SDL_EVENT_GAMEPAD_BUTTON_DOWN) {
        auto button=event.gbutton.button;
        if(playing_) { if(button==SDL_GAMEPAD_BUTTON_GUIDE) toMenu(); }
        else if(button==SDL_GAMEPAD_BUTTON_DPAD_LEFT) key(SDL_SCANCODE_LEFT,false);
        else if(button==SDL_GAMEPAD_BUTTON_DPAD_RIGHT) key(SDL_SCANCODE_RIGHT,false);
        else if(button==SDL_GAMEPAD_BUTTON_DPAD_UP) key(SDL_SCANCODE_UP,false);
        else if(button==SDL_GAMEPAD_BUTTON_DPAD_DOWN) key(SDL_SCANCODE_DOWN,false);
        else if(button==SDL_GAMEPAD_BUTTON_SOUTH || button==SDL_GAMEPAD_BUTTON_START) key(SDL_SCANCODE_RETURN,false);
        else if(button==SDL_GAMEPAD_BUTTON_EAST) key(SDL_SCANCODE_ESCAPE,false);
      } else if(!playing_ && (event.type==SDL_EVENT_MOUSE_MOTION || event.type==SDL_EVENT_MOUSE_BUTTON_DOWN)) {
        SDL_ConvertEventToRenderCoordinates(renderer_,&event);
        SDL_FPoint point{event.type==SDL_EVENT_MOUSE_MOTION ? event.motion.x:event.button.x,event.type==SDL_EVENT_MOUSE_MOTION ? event.motion.y:event.button.y};
        std::string action;
        for(auto i=hits_.rbegin();i!=hits_.rend();++i) if(SDL_PointInRectFloat(&point,&i->rect)) { action=i->action; break; }
        hover_=action;
        if(event.type==SDL_EVENT_MOUSE_BUTTON_DOWN && event.button.button==SDL_BUTTON_LEFT && !action.empty()) {
          activate(action);
          if(event.button.clicks==2 && action.starts_with("game:")) beginGame();
        }
      }
    }
  }
public:
  App(fs::path assets,fs::path data,bool hidden,std::string profileLabel=""):assets_(std::move(assets)),data_(std::move(data)),hidden_(hidden) {
    auto title=profileLabel.empty() ? "Classic Menu - ares" : "Classic Menu - "+profileLabel;
    window_=SDL_CreateWindow(title.c_str(),1280,720,SDL_WINDOW_RESIZABLE|SDL_WINDOW_HIGH_PIXEL_DENSITY|(hidden ? SDL_WINDOW_HIDDEN:0));
    if(!window_) throw std::runtime_error(SDL_GetError());
    const char* backend=hidden ? "software":nullptr;
#ifdef __APPLE__
    if(!hidden) backend="metal";
#endif
    renderer_=SDL_CreateRenderer(window_,backend);
    if(!renderer_ && !hidden) renderer_=SDL_CreateRenderer(window_,nullptr);
    if(!renderer_) throw std::runtime_error(SDL_GetError());
    SDL_SetRenderLogicalPresentation(renderer_,1280,720,SDL_LOGICAL_PRESENTATION_LETTERBOX);
    vsync_=!hidden && SDL_SetRenderVSync(renderer_,1);
    stageTexture_=SDL_CreateTexture(renderer_,SDL_PIXELFORMAT_RGBA8888,SDL_TEXTUREACCESS_TARGET,1280,720);
    if(!stageTexture_) throw std::runtime_error(SDL_GetError());
    SDL_SetTextureScaleMode(stageTexture_,SDL_SCALEMODE_LINEAR);
    SDL_SetTextureBlendMode(stageTexture_,SDL_BLENDMODE_NONE);
    installNativeMenu();
    SDL_SetRenderDrawBlendMode(renderer_,SDL_BLENDMODE_BLEND);
    art_=std::make_unique<Assets>(renderer_,assets_);
    auto catalog=readJson(assets_/"catalog.json");
    if(catalog.value("schema",0)!=1) throw std::runtime_error("Unsupported catalog schema");
    std::set<std::string> codes;
    for(auto& value:catalog.at("games")) {
      Game game; game.id=value.at("id"); game.title=value.at("title"); game.sortTitle=value.value("sortTitle",game.title);
      game.publisher=value.value("publisher",""); game.release=value.value("releaseDate",""); game.copyright=value.value("copyright","");
      game.players=value.value("players",1); game.simultaneous=value.value("simultaneous",false); game.bundledROM=value.value("rom","");
      game.importError=value.value("importError","");
      if(!safeID(game.id) || !codes.insert(game.id).second) throw std::runtime_error("Invalid or duplicate game ID");
      games_.push_back(std::move(game));
    }
    if(games_.empty()) throw std::runtime_error("Empty game catalog");
    // Upload all art outside the animation loop, avoiding first-scroll stalls.
    for(auto& game:games_) {
      art_->image(assets_/"games"/game.id/(game.id+".png"));
      art_->image(assets_/"games"/game.id/(game.id+"_small.png"));
    }
    refreshSlots();
    fs::create_directories(data_);
    if(fs::exists(data_/"preferences.json")) {
      try { preferences_=readJson(data_/"preferences.json"); }
      catch(...) { throw std::runtime_error("Invalid preferences.json; preserve or repair it before starting"); }
    }
    selected_=preferences_.value("selected",games_.front().id);
    if(!codes.contains(selected_)) selected_=games_.front().id;
    sort_=std::clamp(preferences_.value("sort",0),0,5); slot_=std::clamp(preferences_.value("slot",0),0,3);
    sound_=preferences_.value("sound",false); motion_=preferences_.value("motion",true);
    auto defaultFirmware=fs::is_regular_file(assets_/"firmware/ipl.rom") ? assets_/"firmware":data_/"firmware";
    firmware_=pathFromUTF8(preferences_.value("firmware",utf8(defaultFirmware)));
    sortGames();
    music_=std::make_unique<Audio>(); music_->openWAV(assets_/"skin/bgm_home.wav");
    click_=std::make_unique<Audio>(); click_->openWAV(assets_/"skin/se_sys_cursor.wav");
    dialogEvent_=SDL_RegisterEvents(1); if(dialogEvent_==Uint32(-1)) throw std::runtime_error(SDL_GetError());
    int count=0; auto* pads=SDL_GetGamepads(&count);
    if(count) gamepad_=SDL_OpenGamepad(pads[0]); SDL_free(pads);
    start_=now();
  }
  ~App() {
    core_.unload(); coreAudio_.reset(); music_.reset(); click_.reset(); art_.reset();
    if(gamepad_) SDL_CloseGamepad(gamepad_);
    if(frameTexture_) SDL_DestroyTexture(frameTexture_);
    if(stageTexture_) SDL_DestroyTexture(stageTexture_);
    if(renderer_) SDL_DestroyRenderer(renderer_);
    if(window_) SDL_DestroyWindow(window_);
  }
  void openROM(const fs::path& path) { preferences_["roms"][selected_]=utf8(fs::absolute(path)); beginGame(); if(!playing_) throw std::runtime_error(message_); }
  void setScrollTest(bool enabled) { scrollTest_=enabled; }
  void selectGame(const std::string& code,bool start) {
    if(std::none_of(games_.begin(),games_.end(),[&](const auto& game){return game.id==code;})) throw std::runtime_error("Game code not in profile: "+code);
    selected_=code; scrollTo_=scrollFrom_=scrollCurrent_=index();
    if(start) { beginGame(); if(!playing_) throw std::runtime_error(message_); }
  }
  void showImportSummary(const importer::Profile& profile) {
    auto identity=profile.manifest.at("sourceSHA256").get<std::string>()+"-v"+std::to_string(profile.manifest.at("importerVersion").get<int>());
    if(preferences_.value("importSummarySeen","")==identity) return;
    int total=profile.manifest.at("games"), ready=profile.manifest.at("playableGames");
    std::string message=profile.manifest.value("region","")+" / "+profile.manifest.value("version","")+": imported "+std::to_string(total)+" games' artwork, fonts and labels. "+std::to_string(ready)+" ROMs converted. ";
    if(ready<total) message+=std::to_string(total-ready)+" conversions remain unsupported; their errors are shown when selected. ";
    if(profile.manifest.value("externalFirmwareGames",0)) message+="Super Mario Kart additionally needs user-supplied DSP1B firmware. ";
    message+="F2 opens the image library. Restored audio is signal-verified against the supplied PCM and recorded loops, not verified as retail-ROM byte-identical. Custom emulator/image controls remain English.";
    alert(message);
    preferences_["importSummarySeen"]=identity; saveSettings();
  }
  int run(int frames,const fs::path& screenshot) {
    int rendered=0; double lastFrame=now();
    while(running_) {
      events(); hits_.clear();
      if(scrollTest_ && !playing_ && panel_.empty() && rendered%12==0) move(1);
      if(playing_) {
        if(coreAudio_ && coreAudio_->available() && coreAudio_->queued()>int(core_.audioRate()*sizeof(float)*2*.04)) { SDL_Delay(1); continue; }
        if(!core_.runFrame(buttons())) { playing_=false; alert(core_.error()); if(hidden_) return 2; }
        else {
          for(float value:core_.audio()) { audioPeak_=std::max(audioPeak_,std::abs(double(value)));++audioSamples_; }
          if(coreAudio_) coreAudio_->put(core_.audio());
        }
      } else if(sound_) music_->play(true);
      SDL_SetRenderTarget(renderer_,stageTexture_);
      SDL_SetRenderDrawColor(renderer_,0,0,0,255); SDL_RenderClear(renderer_);
      if(playing_) drawGame(); else drawMenu();
      SDL_SetRenderTarget(renderer_,nullptr);
      SDL_SetRenderDrawColor(renderer_,0,0,0,255);SDL_RenderClear(renderer_);
      SDL_FRect stage{0,0,1280,720};SDL_RenderTexture(renderer_,stageTexture_,nullptr,&stage);
      ++rendered;
      if(frames>0 && rendered>=frames) {
        if(!screenshot.empty()) {
          auto* surface=SDL_RenderReadPixels(renderer_,nullptr);
          auto* rgba=surface ? SDL_ConvertSurface(surface,SDL_PIXELFORMAT_RGBA32) : nullptr;
          if(!rgba || !stbi_write_png(utf8(screenshot).c_str(),rgba->w,rgba->h,4,rgba->pixels,rgba->pitch)) throw std::runtime_error("Cannot write screenshot");
          SDL_DestroySurface(rgba); SDL_DestroySurface(surface);
        }
        SDL_RenderPresent(renderer_);
        std::cout<<"Rendered "<<rendered<<" frames; embedded ares="<<(core_.loaded() ? "running":"ready")<<"; catalog="<<games_.size()<<"; renderer="<<SDL_GetRendererName(renderer_)<<"; vsync="<<vsync_<<"; stage=1280x720; audio_samples="<<audioSamples_<<"; audio_peak="<<audioPeak_<<"; audio_output="<<(coreAudio_ && coreAudio_->available())<<"\n";
        if(coreAudio_ && !coreAudio_->error().empty()) std::cerr<<"Audio output: "<<coreAudio_->error()<<"\n";
        return 0;
      }
      SDL_RenderPresent(renderer_);
      if(!vsync_ && (!playing_ || !coreAudio_ || !coreAudio_->available())) {
        auto delay=1.0/60-(now()-lastFrame); if(delay>0) SDL_Delay(Uint32(delay*1000)); lastFrame=now();
      }
    }
    saveSettings();
    if(core_.loaded()) {
      core_.unload();
      if(!core_.error().empty()) { std::cerr<<"Save memory flush failed: "<<core_.error()<<"\n"; return 2; }
    }
    return switchProfile_ ? (browseImage_ ? 4:3):0;
  }
};
}

int main(int argc,char** argv) {
  using namespace clover::ui;
  namespace importer=clover::importer;
  if(!SDL_Init(SDL_INIT_VIDEO|SDL_INIT_GAMEPAD)) { std::cerr<<SDL_GetError()<<"\n"; return 1; }
  SDL_InitSubSystem(SDL_INIT_AUDIO);
  int result=1;
  try {
    fs::path base=pathFromUTF8(SDL_GetBasePath() ? SDL_GetBasePath():".");
    fs::path assets=base/"assets", data, rom, screenshot, image,launcherPreview;
    if(!fs::exists(assets/"catalog.json") && fs::exists(base/"../Resources/assets/catalog.json")) assets=base/"../Resources/assets";
    char* prefs=SDL_GetPrefPath("ClassicMenu","ClassicMenu"); if(!prefs) throw std::runtime_error(SDL_GetError()); data=pathFromUTF8(prefs); SDL_free(prefs);
    bool smoke=false,explicitAssets=false,forceLauncher=false,importOnly=false,listOnly=false,startGame=false,browseLauncher=false,scrollTest=false; int frames=0;
    std::string gameCode;
    for(int i=1;i<argc;++i) {
      std::string option=argv[i];
      auto value=[&]() -> const char* { if(i+1>=argc) throw std::runtime_error("Missing argument for "+option); return argv[++i]; };
      if(option=="--assets") { assets=pathFromUTF8(value()); explicitAssets=true; }
      else if(option=="--data-dir") data=pathFromUTF8(value());
      else if(option=="--rom") rom=pathFromUTF8(value());
      else if(option=="--game") gameCode=value();
      else if(option=="--start") startGame=true;
      else if(option=="--screenshot") screenshot=pathFromUTF8(value());
      else if(option=="--frames") frames=std::stoi(value());
      else if(option=="--smoke-test") smoke=true;
      else if(option=="--scroll-test") scrollTest=true;
      else if(option=="--image") image=pathFromUTF8(value());
      else if(option=="--import-image") { image=pathFromUTF8(value()); importOnly=true; }
      else if(option=="--launcher") forceLauncher=true;
      else if(option=="--render-launcher") launcherPreview=pathFromUTF8(value());
      else if(option=="--list-profiles") listOnly=true;
      else if(option=="--help") { std::cout<<"ClassicMenu [--image image.hsqs | --import-image image.hsqs | --assets folder] [--launcher] [--list-profiles] [--data-dir folder] [--game CLV-P-XXXXX --start] [--rom game.sfc] [--frames N] [--screenshot output.png] [--smoke-test]\n"; SDL_Quit(); return 0; }
      else throw std::runtime_error("Unknown option: "+option);
    }
    if(smoke && frames<=0) frames=8;
    data=fs::absolute(data);assets=fs::absolute(assets);
    if(!image.empty() && explicitAssets) throw std::runtime_error("Choose --image or --assets, not both");
    if(startGame && gameCode.empty()) throw std::runtime_error("--start requires --game");
    if(!launcherPreview.empty()) {
      launchProfiles(data,imageSearchDirectories(base),true,launcherPreview);result=0;
    } else if(listOnly) {
      Json list=Json::array();for(auto& p:importer::listProfiles(data/"profiles")) list.push_back(p.manifest);
      std::cout<<list.dump(2)<<"\n"; result=0;
    } else {
      std::optional<importer::Profile> profile;
      if(!image.empty()) {
        profile=importer::importImage(image,data/"profiles",[](const std::string& status){std::cout<<status<<"\n";});
        rememberProfile(data,*profile);
      }
      if(importOnly) { std::cout<<profile->manifest.dump(2)<<"\n";result=0; }
      else {
        do {
          if(!explicitAssets && !profile) {
            if(smoke) {
              if(fs::is_regular_file(data/"last-profile.json")) {
                auto id=readJson(data/"last-profile.json").at("directory").get<std::string>();
                for(auto& cached:importer::listProfiles(data/"profiles")) if(cached.directory.filename().string()==id) profile=cached;
              }
              if(!profile) throw std::runtime_error("Headless mode needs --assets, --image, or a valid last-used profile");
            } else {
              profile=launchProfiles(data,imageSearchDirectories(base),forceLauncher,{},browseLauncher);
            }
            if(!profile) { result=0;break; }
            rememberProfile(data,*profile);
          }
          auto assetPath=profile ? profile->assets():assets;
          auto dataPath=profile ? profile->data():data;
          {
            App app(assetPath,dataPath,smoke,profile ? profile->label():"");
            app.setScrollTest(scrollTest);
            if(profile && !smoke) app.showImportSummary(*profile);
            if(!gameCode.empty()) app.selectGame(gameCode,startGame);
            if(!rom.empty()) app.openROM(rom);
            result=app.run(frames,screenshot);
          }
          if(result==3 || result==4) { profile.reset();explicitAssets=false;forceLauncher=true;browseLauncher=result==4; }
        } while(result==3 || result==4);
      }
    }
  } catch(const std::exception& error) {
    std::cerr<<"Classic Menu: "<<error.what()<<"\n";
    if(!SDL_GetCurrentVideoDriver() || std::string(SDL_GetCurrentVideoDriver())!="dummy") SDL_ShowSimpleMessageBox(SDL_MESSAGEBOX_ERROR,"Classic Menu",error.what(),nullptr);
  }
  SDL_Quit(); return result;
}
