#pragma once

#include <cstddef>
#include <cstdint>

#if defined(__wasm__)
#define MYC_EXPORT extern "C" __attribute__((visibility("default")))
#else
#define MYC_EXPORT extern "C"
#endif

enum class PluginSettingType {
  Boolean,
  Number,
  Text,
  Select,
};

enum class PluginApiFormat {
  OpenAi,
  Anthropic,
};

enum class PluginApiKeySource {
  HostSecret,
  Environment,
};

// Declarative connection metadata is rendered and tested by the native host.
struct PluginConnectionDefinition {
  const char* id = nullptr;
  const char* label = nullptr;
  const char* urlSettingId = nullptr;
  const char* formatSettingId = nullptr;
  const char* modelSettingId = nullptr;
  const char* credentialSourceSettingId = nullptr;
  const char* credentialEnvVarSettingId = nullptr;
  PluginApiKeySource apiKeySource = PluginApiKeySource::Environment;
  const char* credentialEnvVar = nullptr;
  const char* fallbackSettingId = nullptr;
  const char* testActionId = nullptr;
};

// Stable plugin identity metadata. developerId is optional for legacy
// manifests and should contain a UUID when present.
struct PluginIdentity {
  const char* id = nullptr;
  const char* name = nullptr;
  const char* version = nullptr;
  const char* developer = nullptr;
  bool hasDeveloperId = false;
  const char* developerId = nullptr;
};

struct PluginSettingOption {
  const char* value = nullptr;
  const char* label = nullptr;
};

enum class PluginSettingDefaultType {
  None,
  Boolean,
  Number,
  Text,
};

struct PluginSettingDefault {
  PluginSettingDefaultType type = PluginSettingDefaultType::None;
  bool booleanValue = false;
  double numberValue = 0.0;
  const char* textValue = nullptr;
};

// Host-rendered declaration metadata. Secret settings are write-only host
// credentials and are not returned by SettingsReader.
struct PluginSettingDefinition {
  const char* id = nullptr;
  const char* label = nullptr;
  PluginSettingType type;
  bool secret = false;
  bool required = false;
  const char* description = nullptr;
  const char* placeholder = nullptr;
  const char* group = nullptr;
  PluginSettingDefault defaultValue;
  bool hasMin = false;
  double min = 0.0;
  bool hasMax = false;
  double max = 0.0;
  bool hasStep = false;
  double step = 0.0;
  const PluginSettingOption* options = nullptr;
  std::size_t optionCount = 0;
};

// The host supplies effective, validated non-secret values. There is no
// getSecret method: API keys remain in the host model gateway/request proxy.
class SettingsReader {
 public:
  virtual ~SettingsReader() = default;
  virtual bool has(const char* id) const = 0;
  virtual bool getBoolean(const char* id, bool& value) const = 0;
  virtual bool getNumber(const char* id, double& value) const = 0;
  virtual bool getText(const char* id, const char*& value) const = 0;
};

// The host writes UTF-8 JSON into the returned guest-memory range.
// 主机把 UTF-8 JSON 写入返回的访客内存区间。
MYC_EXPORT std::int32_t myc_alloc(std::int32_t size);

// Free memory previously returned by `myc_alloc`; `size` must match the allocation.
// 释放 `myc_alloc` 返回的内存；`size` 必须与分配时一致。
MYC_EXPORT void myc_free(std::int32_t pointer, std::int32_t size);

// Return `(output_pointer << 32) | output_length`; output must be UTF-8 JSON.
// 返回 `(输出指针 << 32) | 输出长度`；输出必须是 UTF-8 JSON。
MYC_EXPORT std::uint64_t myc_run(std::int32_t input_pointer, std::int32_t input_length);
