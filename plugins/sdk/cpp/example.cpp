#include "research_canvas_plugin.h"

#include <cstdint>

extern "C" unsigned char __heap_base;
static std::uintptr_t heap = reinterpret_cast<std::uintptr_t>(&__heap_base);
static constexpr char RESPONSE[] = R"({"runtime":"cpp","status":"ok"})";

MYC_EXPORT std::int32_t myc_alloc(std::int32_t size) {
  const auto pointer = heap;
  heap += static_cast<std::uintptr_t>(size + 7) & ~std::uintptr_t{7};
  return static_cast<std::int32_t>(pointer);
}

MYC_EXPORT std::uint64_t myc_run(std::int32_t, std::int32_t) {
  const auto pointer = reinterpret_cast<std::uintptr_t>(RESPONSE);
  constexpr std::uint64_t length = sizeof(RESPONSE) - 1;
  return (static_cast<std::uint64_t>(pointer) << 32) | length;
}
