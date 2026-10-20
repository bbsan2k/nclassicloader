#pragma once
#include <SDL3/SDL.h>
#include <json.hpp>
#include <stb_image.h>
#include <filesystem>
#include <fstream>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <vector>
#include <algorithm>
#include <cmath>
#include "geometry.hpp"

namespace clover::ui {
namespace fs = std::filesystem;
using Json = nlohmann::json;
inline std::string utf8(const fs::path& path) {
  auto value = path.u8string();
  return {reinterpret_cast<const char*>(value.data()), value.size()};
}
inline fs::path pathFromUTF8(std::string_view value) {
  return fs::path(std::u8string(reinterpret_cast<const char8_t*>(value.data()),value.size()));
}
inline Json readJson(const fs::path& path) {
  if(fs::file_size(path) > 32 * 1024 * 1024) throw std::runtime_error("JSON file too large: " + utf8(path));
  std::ifstream stream(path, std::ios::binary);
  if(!stream) throw std::runtime_error("Cannot open " + utf8(path));
  return Json::parse(stream);
}
inline std::vector<uint32_t> scalars(const std::string& text) {
  std::vector<uint32_t> result;
  for(size_t i = 0; i < text.size() && result.size() < 4096;) {
    auto c = static_cast<unsigned char>(text[i++]);
    if(c < 128) { result.push_back(c); continue; }
    unsigned count = c < 0xe0 ? 1 : c < 0xf0 ? 2 : 3;
    uint32_t code = c & (count == 1 ? 31 : count == 2 ? 15 : 7);
    bool valid = c >= 0xc2 && c <= 0xf4;
    for(unsigned j = 0; j < count; ++j) {
      if(i >= text.size() || (static_cast<unsigned char>(text[i]) & 0xc0) != 0x80) { valid = false; break; }
      code = (code << 6) | (static_cast<unsigned char>(text[i++]) & 63);
    }
    if(code > 0x10ffff || (code >= 0xd800 && code <= 0xdfff)) valid = false;
    result.push_back(valid ? code : '?');
  }
  return result;
}
struct Texture {
  SDL_Texture* value = nullptr;
  int width = 0, height = 0;
  ~Texture() { if(value) SDL_DestroyTexture(value); }
};
struct Glyph { SDL_FRect source{}; float x = 0, y = 0, advance = 0; };
struct Font { std::map<uint32_t, Glyph> glyphs; std::map<std::string, float> kerning; float size = 1, baseline = 0, line = 0; Texture* texture = nullptr; };
struct Placed { Texture* texture; SDL_FRect source, destination; };
struct Text {
  std::vector<Placed> glyphs;
  float advance = 0, line = 0;
  float left = 0, top = 0, right = 0, bottom = 0;
};
class Assets {
  SDL_Renderer* renderer_;
  std::map<std::string, std::unique_ptr<Texture>> images_;
  std::map<std::string, Font> fonts_;
  Texture* atlas_ = nullptr;
  Json sprites_;
public:
  fs::path root;
  Json layout;
  Json strings=Json::object();
  std::string locale="usa_en",region="Europe";
  SDL_Color titleColor{0,0,0,255},hudColor{255,255,255,255};
  Assets(SDL_Renderer* renderer, fs::path directory): renderer_(renderer), root(std::move(directory)) {
    layout = readJson(root / "skin/layout.json");
    if(fs::is_regular_file(root/"skin/strings.json")) { auto table=readJson(root/"skin/strings.json");strings=table.at("strings");locale=table.at("locale"); }
    if(fs::is_regular_file(root/"skin/theme.json")) {
      auto theme=readJson(root/"skin/theme.json");region=theme.value("region","Europe");
      if(theme.contains("colors")) {
        auto color=[&](const char* key) { auto c=theme.at("colors").at(key);return SDL_Color{c[0],c[1],c[2],c[3]}; };
        titleColor=color("title");hudColor=color("hud");
      }
    }
    sprites_ = readJson(root / "skin/packed.json").at("sprites");
    atlas_ = image(root / "skin/packed.png");
    if(!atlas_) throw std::runtime_error("Missing Clover sprite atlas");
    for(auto name : {"s", "m", "l", "title", "copyright"}) {
      if(std::string(name)=="copyright" && !fs::is_regular_file(root/"skin/font-copyright.json")) continue;
      auto data = readJson(root / (std::string("skin/font-") + name + ".json"));
      Font font;
      font.size = data.at("fontSize"); font.line = data.at("lineHeight"); font.baseline = data.at("baseline");
      font.texture = image(root / (std::string("skin/font-") + name + ".png"));
      if(!font.texture) throw std::runtime_error("Missing Clover bitmap font");
      for(auto& [key, item] : data.at("glyphs").items()) {
        font.glyphs[static_cast<uint32_t>(std::stoul(key))] = {
          {item.at("x"), item.at("y"), item.at("width"), item.at("height")},
          item.at("xOffset"), item.at("yOffset"), item.at("advance")};
      }
      for(auto& [key, value] : data.at("kernings").items()) font.kerning[key] = value;
      fonts_[name] = std::move(font);
    }
  }
  std::string tr(const std::string& key,const std::string& fallback) const {
    if(strings.contains(key) && strings[key].is_string()) return strings[key].get<std::string>();
    return fallback;
  }
  Texture* image(const fs::path& path) {
    auto key = utf8(path);
    if(auto i = images_.find(key); i != images_.end()) return i->second.get();
    auto result = std::make_unique<Texture>();
    auto* pixels = stbi_load(key.c_str(), &result->width, &result->height, nullptr, 4);
    if(pixels) {
      auto* surface = SDL_CreateSurfaceFrom(result->width, result->height, SDL_PIXELFORMAT_RGBA32, pixels, result->width * 4);
      if(surface) { result->value = SDL_CreateTextureFromSurface(renderer_, surface); SDL_DestroySurface(surface); }
      stbi_image_free(pixels);
    }
    if(result->value) SDL_SetTextureBlendMode(result->value, SDL_BLENDMODE_BLEND);
    auto* pointer = result.get(); images_[key] = std::move(result);
    return pointer->value ? pointer : nullptr;
  }
  void draw(Texture* texture, SDL_FRect destination, const SDL_FRect* source = nullptr, SDL_Color color = {255,255,255,255}, bool smooth = false, bool flip = false) {
    if(!texture || !texture->value) return;
    SDL_SetTextureColorMod(texture->value, color.r, color.g, color.b);
    SDL_SetTextureAlphaMod(texture->value, color.a);
    SDL_SetTextureScaleMode(texture->value, smooth ? SDL_SCALEMODE_LINEAR : SDL_SCALEMODE_NEAREST);
    if(flip) SDL_RenderTextureRotated(renderer_,texture->value,source,&destination,0,nullptr,SDL_FLIP_HORIZONTAL);
    else SDL_RenderTexture(renderer_, texture->value, source, &destination);
  }
  void sprite(std::string name, float x, float y, float w, float h, SDL_Color color = {255,255,255,255}, bool flip = false) {
    if(!name.ends_with(".png")) name += ".png";
    if(!sprites_.contains(name)) return;
    auto r = sprites_[name]["textureFrame"];
    SDL_FRect source{r[0],r[1],r[2],r[3]};
    // Resolve nearest-filtered atlas sprites on the fixed stage's pixel grid.
    draw(atlas_, {std::round(x-w/2),std::round(y-h/2),w,h}, &source, color, false, flip);
  }
  void fitImage(Texture* texture,SDL_FRect box,SDL_Color color={255,255,255,255}) {
    if(texture) draw(texture,aspectFit(texture->width,texture->height,box),nullptr,color,true);
  }
  Text measure(const std::string& value, const std::string& name) {
    auto& primary = fonts_.at(name);
    Text text; text.line = primary.line;
    bool first = true;
    const Font* previous = nullptr; uint32_t previousCode = 0;
    for(auto code : scalars(value)) {
      Font* chosen = nullptr;
      if(primary.glyphs.contains(code)) chosen = &primary;
      else for(auto& [_, font] : fonts_) if(font.glyphs.contains(code)) { chosen = &font; break; }
      if(!chosen) {
        code='?';
        for(auto& [_,font]:fonts_) if(font.glyphs.contains(code)) { chosen=&font;break; }
        // A visible replacement is preferable to silently deleting unknown text.
        if(!chosen) { code=0x25a1;for(auto& [_,font]:fonts_) if(font.glyphs.contains(code)) { chosen=&font;break; } }
        if(!chosen) { code='-';chosen=fonts_.contains("copyright") ? &fonts_.at("copyright"):&primary; }
      }
      if(!chosen->glyphs.contains(code)) continue;
      auto& g = chosen->glyphs.at(code);
      float scale = primary.size / chosen->size;
      if(previous == chosen) {
        auto key = std::to_string(previousCode) + ":" + std::to_string(code);
        if(chosen->kerning.contains(key)) text.advance += chosen->kerning.at(key) * scale;
      }
      SDL_FRect rect{text.advance + g.x*scale, primary.baseline - chosen->baseline*scale + g.y*scale, g.source.w*scale, g.source.h*scale};
      text.glyphs.push_back({chosen->texture,g.source,rect});
      if(code != ' ' && rect.w > 0 && rect.h > 0) {
        text.left = first ? rect.x : std::min(text.left,rect.x); text.top = first ? rect.y : std::min(text.top,rect.y);
        text.right = first ? rect.x+rect.w : std::max(text.right,rect.x+rect.w); text.bottom = first ? rect.y+rect.h : std::max(text.bottom,rect.y+rect.h);
        first = false;
      }
      text.advance += g.advance*scale; previous = chosen; previousCode = code;
    }
    return text;
  }
  void text(const std::string& value, float x, float y, std::string font = "m", SDL_Color color = {255,255,255,255}, float maxWidth = 1200, bool lineAnchor = false) {
    auto text = measure(value,font);
    float extent = lineAnchor ? std::max(text.advance,text.right) : text.right-text.left;
    float scale = std::min(1.0f,maxWidth/std::max(1.0f,extent));
    float left = x - (lineAnchor ? text.advance/2 : (text.left+text.right)/2)*scale;
    float top = y - (lineAnchor ? text.line/2 : (text.top+text.bottom)/2)*scale;
    for(auto& glyph : text.glyphs) {
      auto r = glyph.destination;
      draw(glyph.texture,{left+r.x*scale,top+r.y*scale,r.w*scale,r.h*scale},&glyph.source,color,true);
    }
  }
  void selection(float x, float y, float w, float h, float time) {
    float phase = std::fmod(time,2.25f), tint = phase < .5f ? .75f+.5f*phase : phase < 1.25f ? 1 : phase < 2 ? 1-(phase-1.25f)/3 : .75f;
    SDL_Color color{uint8_t(255*tint),uint8_t(255*tint),255,255};
    float widths[3]{32,w-64,32}, heights[3]{32,h-64,32};
    float left = x-w/2, top = y-h/2;
    for(int row=0;row<3;row++) for(int col=0;col<3;col++) {
      if(row==1 && col==1) continue;
      int tile = row*3+col+1;
      auto name = ((row!=1 && col!=1) ? "cursor_" : "cursor_game_") + std::to_string(tile);
      float cx=left+(col==0 ? 0 : col==1 ? 32 : w-32)+widths[col]/2;
      float cy=top+(row==0 ? 0 : row==1 ? 32 : h-32)+heights[row]/2;
      sprite(name,cx,cy,widths[col],heights[row],color);
    }
  }
};
}
