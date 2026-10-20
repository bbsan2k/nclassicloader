#pragma once
#include <SDL3/SDL_rect.h>
#include <algorithm>

namespace clover::ui {
inline SDL_FRect aspectFit(float width,float height,SDL_FRect box) {
  if(width<=0 || height<=0 || box.w<=0 || box.h<=0) return {box.x,box.y,0,0};
  float scale=std::min(box.w/width,box.h/height);
  float w=width*scale,h=height*scale;
  return {box.x+(box.w-w)/2,box.y+(box.h-h)/2,w,h};
}
}
