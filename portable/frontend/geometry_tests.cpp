#include "geometry.hpp"
#include <cmath>
#include <stdexcept>
int main() {
  using clover::ui::aspectFit;
  auto check=[](bool v){if(!v)throw std::runtime_error("Artwork aspect fit regression");};
  auto cover=aspectFit(228,160,{526,234,228,204});
  check(cover.x==526 && cover.y==256 && cover.w==228 && cover.h==160);
  auto thumbnail=aspectFit(40,28,{0,0,32,32});
  check(std::abs(thumbnail.w/thumbnail.h-40.f/28)<.0001f && thumbnail.w==32 && thumbnail.y>0);
  auto portrait=aspectFit(160,228,{0,0,228,204});
  check(portrait.h==204 && portrait.w<228 && portrait.x>0);
  check(aspectFit(0,0,{0,0,32,32}).w==0);
}
