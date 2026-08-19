# Anyway 插件系统 v2 — 清单格式 + Host Bus 架构

> 目标：① 插件清单全面 JSON 化，采用 VSCode(`package.json` contributes) + Cordis(services/events/lifecycle) 风格；② 补齐 8 个 host bus API，文件解耦 + 中间件架构。

## 1. 新清单格式（plugin.json）

从 `{ apiVersion, kind, metadata{...}, spec{...} }` 扁平化为 VSCode `package.json` 风格，并叠加 Cordis 的服务/事件/生命周期语义。

```jsonc
{
  // ── VSCode package.json 顶层字段（扁平）──
  "name": "myc.folder-workspaces",          // 唯一 id（原 metadata.id）
  "displayName": "Folder Workspaces",       // 原 metadata.name
  "version": "1.0.0",
  "publisher": "Research Canvas",
  "description": "...",
  "license": "MIT",
  "homepage": "...",
  "categories": ["Workspace"],              // 原 kind（WorkspacePlugin/ThemePlugin/AgentPlugin/...）
  "engines": { "anyway": ">=0.3.0" },
  "main": "workspace-plugin.json",          // 原 spec.entry
  "activationEvents": ["onCommand:open-folder-workspace"],

  // ── VSCode contributes ──
  "contributes": {
    "commands": [ { "id", "label", "description", "category", "capability", "formats" } ],
    "menus":     [ { "id", "scope": "node|edge|canvas", "label", "icon", "command" } ],
    "configuration": { "title", "properties": { ... } },   // 原 spec.settings（扁平化）
    "viewsContainers": { "activitybar": [ { "id", "title", "icon" } ] },
    "views": { "activity-sidebar": [ { "id", "name", "when" } ] },
    "uiIr":      [ { "slotId", "ir" } ],
    "locales":   [ { "locale", "name", "path" } ]
  },

  // ── Cordis 服务/事件/生命周期语义 ──
  "provides":  { "services": ["git.repository"], "events": ["git.commit"] },  // 插件对外提供
  "inject":    { "services": ["project.store"], "events": ["project.saved"] }, // 插件依赖
  "lifecycle": { "start": "entry.start", "stop": "entry.stop", "reusable": false },

  // ── 权限 / 载荷 / 签名（沿用现有语义）──
  "capabilities": ["project.folder"],
  "permissions": [],
  "payloads": { "README.md": "<sha256 hex>", ... },
  "signature": "<base64 Ed25519>"
}
```

### 兼容性

- 保留 `developer`/`developerId` 语义，归一为 `publisher` + `developerId`。
- 旧 `spec.settings` 迁移到 `contributes.configuration.properties`（`PluginSettingDefinition` 结构不变）。
- 旧 `spec.connections` 迁移到 `contributes.configuration.connections`（`PluginConnectionDefinition` 结构不变）。
- 旧 `spec.contributes.*` 迁移到 `contributes.*`。
- `kind` 从字符串枚举变成 `categories`（可多值），但内核仍按「是否声明某类 capability/entry」判定可执行类型。

### Rust 解析重写

- 新 `ManifestV2` 结构体（`serde(rename_all = "camelCase")`，纯 `serde_json`，删除 `serde_yaml` 依赖）。
- `ManifestV2::try_from(Value) -> (Self, ManifestKind)` 校验。
- 迁移适配器 `ManifestV1`（旧 `MycPluginManifest`）→ `ManifestV2`，安装旧 `.myc` 时自动归一。

## 2. 密钥打包签名

沿用 Ed25519，但签名对象从「manifest JSON 不含 signature」升级为：

```
signed_bytes = canonical_json(manifest_without_signature + payloads)
signature    = Ed25519_sign(publisher_key, SHA256(signed_bytes))
```

- 信任链：`plugins/packages/*.myc` 由 `scripts/pack-plugin.mjs` 用离线 `id_ed25519` 生成；`src-tauri/src/signing.rs` 只验证。
- 清单文件名统一 `plugin.json`；`payloads` 不含 `plugin.json` 自身。

## 3. 8 个 Host Bus API（文件解耦 + 中间件）

新增目录 `src-tauri/src/host_bus/`（每域一文件）：

| # | operation | 文件 | 能力 | 生命周期 |
|---|---|---|---|---|
| 1 | `graph.storage.put` / `graph.storage.query.*` | `storage.rs` | `graph.storage.write/read` | 请求级（幂等 put，query 只读） |
| 2 | `lease.renew` | `lease.rs` | `host-bus.lease` | 租约级（续期/过期/吊销） |
| 3 | `event.subscribe` / `event.publish` | `events.rs` | `host-bus.event` | 订阅级（ack/replay/关闭） |
| 4 | `worker.spawn` / `worker.stop` | `workers.rs` | `host-bus.worker` | worker 级（spawn/lease/stop） |
| 5 | `service.list` / `service.unregister` | `services.rs` | `host-bus.service` | 注册级 |
| 6 | `audit.read` / `audit.query` | `audit.rs` | `audit.read` | 请求级（只读） |
| 7 | `graph.ir.compile` / `graph.ir.query` | `ir.rs` | `graph.ir` | 请求级（调用 schema-v4 编译器） |
| 8 | `blob.list` / `blob.release` | `blob.rs` | `blob.manage` | blob 级（retention） |

### 中间件架构

```text
HostCallRequest
   │
   ▼
kernel_host_call  (唯一入口)
   ├─[1] transport_auth      webview 主体校验（MAIN_WEBVIEW）
   ├─[2] schema_validate     请求信封校验（apiVersion/requestId/deadline）
   ├─[3] principal_bind      原生 UI principal
   ├─[4] lease_resolve       解析 capabilityLeaseIds
   ├─[5] capability_authorize 能力授权（policy.rs 现有 authorize_for_bus）
   ├─[6] admission_gate      并发/配额/死线（AdmissionRequest）
   ├─[7] dispatch            分域 handler（host_bus/*.rs）
   └─[8] audit               record_audit（成功/失败/拒绝）
   ▼
HostCallResponse
```

- 每层是独立函数（中间件），`host_bus/mod.rs` 提供 `pub fn chain(...)` 组合，不互相 import 具体 handler。
- 状态经 `KernelState`（audit/blobs/services/packages）+ 新增 `HostBusState`（leases/events/workers）注入。
- 信息流：request → 中间件链 → handler → response；大 payload 走 `blob.*` 多分块，handler 只拿 `BlobRef`。

## 4. 实施顺序（每步一 commit）

1. 新清单 schema（`ManifestV2`）+ 迁移适配器 + 测试夹具 JSON 化
2. 签名/打包脚本 `scripts/pack-plugin.mjs`
3. `host_bus/` 骨架 + 中间件链 + `HostBusState`
4. `service.list/unregister` + `lease.renew`
5. `event.subscribe/publish`
6. `worker.spawn/stop`
7. `graph.storage.*`（接 schema-v4 `Storage` trait）
8. `audit.read` + `blob.list/release`
9. `graph.ir.compile/query`
10. 全量测试 + 迁移 `.myc` 包
