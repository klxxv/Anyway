#pragma once

#include <cstddef>
#include <cstdint>

#if defined(__wasm__)
#define MYC_EXPORT extern "C" __attribute__((visibility("default")))
#else
#define MYC_EXPORT extern "C"
#endif

// The host writes UTF-8 JSON into the returned guest-memory range.
// 主机把 UTF-8 JSON 写入返回的访客内存区间。
MYC_EXPORT std::int32_t myc_alloc(std::int32_t size);

// Return `(output_pointer << 32) | output_length`; output must be UTF-8 JSON.
// 返回 `(输出指针 << 32) | 输出长度`；输出必须是 UTF-8 JSON。
MYC_EXPORT std::uint64_t myc_run(std::int32_t input_pointer, std::int32_t input_length);
