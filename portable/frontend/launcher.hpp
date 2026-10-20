#pragma once
#include "assets.hpp"
#include "importer.hpp"
#include <future>
#include <mutex>
#include <optional>
#include <stb_image_write.h>
#include "native_menu.hpp"
#include "hash.hpp"

namespace clover::ui {
inline std::vector<fs::path> imageSearchDirectories(const fs::path& base) {
  std::vector<fs::path> places{base,base/"../../../"};
  auto cwd=fs::current_path();
  if(cwd!=cwd.root_path()) { places.push_back(cwd);places.push_back(cwd/"SNES-Mini-Kernels"); }
  // Optional checkout discovery for this app's dist/portable layout. Not a
  // runtime dependency; installed/relocated apps use adjacent images or Browse.
  for(auto root:{base/"../../../../../",base/"../.."})
    if(fs::is_regular_file(root/"dependencies.lock.json")) { places.push_back(root);places.push_back(root/"SNES-Mini-Kernels"); }
  return places;
}
inline void SDLCALL imageDialog(void* userdata,const char* const* files,int) {
  auto event=Uint32(reinterpret_cast<uintptr_t>(userdata));
  if(!files || !files[0]) return;
  auto* path=new std::string(files[0]); SDL_Event message{}; message.type=event; message.user.data1=path;
  if(!SDL_PushEvent(&message)) delete path;
}
// This screen deliberately needs no Nintendo assets or system fonts.
inline std::optional<importer::Profile> launchProfiles(const fs::path& home,const std::vector<fs::path>& directories,bool force=false,const fs::path& preview={},bool browseOnOpen=false) {
  auto profiles=importer::listProfiles(home/"profiles");
  auto last=home/"last-profile.json";
  std::optional<fs::path> upgrade;
  if(!force && preview.empty() && fs::is_regular_file(last)) {
    try {
      auto id=readJson(last).at("directory").get<std::string>();
      for(auto& p:profiles) if(p.directory.filename().string()==id) return p;
      bool oldID=id.size()==67 && (id.ends_with("-v1") || id.ends_with("-v2")) && std::all_of(id.begin(),id.begin()+64,[](unsigned char c){return (c>='0' && c<='9') || (c>='a' && c<='f');});
      auto old=home/"profiles"/id;
      if(oldID && !fs::is_symlink(old) && !fs::is_symlink(old/"profile.json")) {
        auto manifest=readJson(old/"profile.json");
        auto sha=manifest.at("sourceSHA256").get<std::string>();
        for(auto& p:profiles) if(p.manifest.value("sourceSHA256","")==sha) return p;
        auto source=pathFromUTF8(manifest.at("source").get<std::string>());
        if(fs::is_regular_file(source) && !fs::is_symlink(source) && importer::fileDigest(source)==sha) upgrade=source;
      }
    } catch(...) {}
  }
  auto images=importer::discoverImages(directories);
  std::stable_sort(images.begin(),images.end(),[](const auto& a,const auto& b) {
    bool ae=utf8(a.filename()).find("sneseur")!=std::string::npos,be=utf8(b.filename()).find("sneseur")!=std::string::npos;
    return ae && !be;
  });
  auto* window=SDL_CreateWindow("Classic Menu - Image Library",1000,650,SDL_WINDOW_RESIZABLE|SDL_WINDOW_HIGH_PIXEL_DENSITY|(preview.empty() ? 0:SDL_WINDOW_HIDDEN));
  if(!window) throw std::runtime_error(SDL_GetError());
  auto* renderer=SDL_CreateRenderer(window,nullptr);
  if(!renderer) { SDL_DestroyWindow(window); throw std::runtime_error(SDL_GetError()); }
  struct Cleanup { SDL_Renderer* renderer; SDL_Window* window; ~Cleanup(){SDL_DestroyRenderer(renderer);SDL_DestroyWindow(window);} } cleanup{renderer,window};
  SDL_SetRenderLogicalPresentation(renderer,800,520,SDL_LOGICAL_PRESENTATION_LETTERBOX);
  installNativeMenu(true);
  auto dialogEvent=SDL_RegisterEvents(1); if(dialogEvent==Uint32(-1)) throw std::runtime_error(SDL_GetError());
  int selection=0;
  std::string status="Choose any supplied region/version. Artwork, fonts and labels follow the image.";
  std::mutex mutex;
  std::future<importer::Profile> pending;
  bool quit=false, done=false;
  std::optional<importer::Profile> result;
  auto import=[&](fs::path source) {
    if(pending.valid()) return;
    status="Starting import...";
    pending=std::async(std::launch::async,[&,source] {
      return importer::importImage(source,home/"profiles",[&](const std::string& value){std::lock_guard lock(mutex);status=value;});
    });
  };
  auto browse=[&] {
    static const SDL_DialogFileFilter filter[]{ {"SNES Classic image / filesystem","img;hsqs;gz"} };
    SDL_ShowOpenFileDialog(imageDialog,reinterpret_cast<void*>(uintptr_t(dialogEvent)),window,filter,1,nullptr,false);
  };
  auto choose=[&] {
    if(pending.valid()) return;
    if(selection<int(profiles.size())) { result=profiles[selection]; done=true; }
    else if(selection<int(profiles.size()+images.size())) import(images[selection-profiles.size()]);
    else browse();
  };
  if(upgrade) import(*upgrade);
  else if(browseOnOpen && preview.empty()) browse();
  while(!done) {
    SDL_Event event{};
    while(SDL_PollEvent(&event)) {
      if(event.type==SDL_EVENT_QUIT) quit=true;
      else if(event.type==nativeMenuEvent()) {
        auto command=MenuCommand(event.user.code);
        if(command==MenuCommand::quit) quit=true;
        else if(command==MenuCommand::importImage && !pending.valid()) browse();
      }
      else if(event.type==dialogEvent) {
        std::unique_ptr<std::string> path(static_cast<std::string*>(event.user.data1)); import(pathFromUTF8(*path));
      } else if(event.type==SDL_EVENT_DROP_FILE) import(pathFromUTF8(event.drop.data));
      else if(event.type==SDL_EVENT_KEY_DOWN && !pending.valid()) {
        auto key=event.key.scancode; int count=int(profiles.size()+images.size()+1);
        if(key==SDL_SCANCODE_ESCAPE) quit=true;
        else if(key==SDL_SCANCODE_DOWN) selection=(selection+1)%count;
        else if(key==SDL_SCANCODE_UP) selection=(selection+count-1)%count;
        else if(key==SDL_SCANCODE_RETURN) choose();
        else if(key==SDL_SCANCODE_I) browse();
      } else if(event.type==SDL_EVENT_MOUSE_BUTTON_DOWN && event.button.button==SDL_BUTTON_LEFT && !pending.valid()) {
        SDL_ConvertEventToRenderCoordinates(renderer,&event);
        int first=std::max(0,selection-6), row=int((event.button.y-132)/30);
        if(event.button.y>=132 && row>=0 && row<8) {
          int item=first+row;
          if(item<int(profiles.size()+images.size()+1)) { selection=item; if(event.button.clicks==2) choose(); }
        }
        if(event.button.y>=430 && event.button.y<465) choose();
      }
    }
    if(pending.valid() && pending.wait_for(std::chrono::milliseconds(0))==std::future_status::ready) {
      try { result=pending.get(); done=true; } catch(const std::exception& e) { std::lock_guard lock(mutex);status=e.what(); }
    }
    if(quit && !pending.valid()) return {};
    SDL_SetRenderDrawColor(renderer,15,18,30,255); SDL_RenderClear(renderer);
    auto text=[&](float x,float y,const std::string& s) { SDL_SetRenderDrawColor(renderer,235,240,255,255); SDL_RenderDebugText(renderer,x,y,s.c_str()); };
    text(28,24,"CLASSIC MENU / IMAGE LIBRARY");
    text(28,52,"Import local SNES Classic games and Clover artwork. No downloads.");
    text(28,70,"Boot .img files need their matching .hsqs or .tar.gz alongside them.");
    text(28,88,"Sources stay unchanged. Saves and suspend points are profile-specific.");
    int first=std::max(0,selection-6), count=int(profiles.size()+images.size()+1);
    for(int row=0;row<8 && first+row<count;++row) {
      int i=first+row; float y=132+row*30;
      if(i==selection) { SDL_SetRenderDrawColor(renderer,35,80,130,255);SDL_FRect r{20,y-6,760,26};SDL_RenderFillRect(renderer,&r); }
      std::string label;
      if(i<int(profiles.size())) label="[Cached] "+profiles[i].label();
      else if(i<int(profiles.size()+images.size())) {
        auto name=utf8(images[i-profiles.size()].filename());
        label="[Image] "+name;
      }
      else label="[Browse] Choose another local image...";
      if(label.size()>91) label=label.substr(0,88)+"...";
      text(28,y,label);
    }
    std::string message;{std::lock_guard lock(mutex);message=status;}
    // Debug font is ASCII. Long errors wrap instead of running off-screen.
    for(int row=0;row<4 && size_t(row*92)<message.size();++row) text(28,375+row*12,message.substr(row*92,92));
    SDL_SetRenderDrawColor(renderer,35,80,130,255); SDL_FRect button{20,430,760,35};SDL_RenderFillRect(renderer,&button);
    text(28,443,pending.valid() ? "Importing... please wait" : "Open / Import selected");
    text(28,485,"Up/Down: select   Enter: open   I: browse   Escape: close   Double-click: open");
    if(!preview.empty()) {
      auto* surface=SDL_RenderReadPixels(renderer,nullptr);
      auto* rgba=surface ? SDL_ConvertSurface(surface,SDL_PIXELFORMAT_RGBA32):nullptr;
      bool saved=rgba && stbi_write_png(utf8(preview).c_str(),rgba->w,rgba->h,4,rgba->pixels,rgba->pitch);
      SDL_DestroySurface(rgba);SDL_DestroySurface(surface);
      if(!saved) throw std::runtime_error("Cannot save launcher preview");
      return {};
    }
    SDL_RenderPresent(renderer);SDL_Delay(16);
    if(quit && done) return {};
  }
  return result;
}
inline void rememberProfile(const fs::path& home,const importer::Profile& profile) {
  fs::create_directories(home); auto temp=home/"last-profile.json.tmp";
  { std::ofstream out(temp); out<<Json{{"directory",profile.directory.filename().string()}}.dump(); if(!out) throw std::runtime_error("Cannot remember profile"); }
  std::error_code ec; fs::rename(temp,home/"last-profile.json",ec);
  if(ec) { fs::remove(home/"last-profile.json");fs::rename(temp,home/"last-profile.json"); }
}
}
