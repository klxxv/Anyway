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
  "engines": { "engine": "host-mediated" }, // 原 spec.engine
  "frontend": {
    "mode": "trusted-module",
    "entry": "dist/frontend.mjs",
    "framework": "vue3",
    "apiVersion": "1"
  },
  "main": "workspace-plugin.json",          // 原 spec.entry
  "language": "rust",                       // 可选：AnalysisPlugin 载荷语言（原 spec.language）
  "activationEvents": ["onCommand:open-folder-workspace"],

  // ── VSCode contributes ──
  "contributes": {
    "commands": [ { "id", "label", "description", "category", "capability", "formats" } ],
    "menus":     [ { "id", "scope": "node|edge|canvas", "label", "icon", "command" } ],
    "configuration": { "title", "settings": [ ... ], "connections": [ ... ] },   // 原 spec.settings/connections
    "viewsContainers": { "activitybar": [ { "id", "title", "icon" } ] },
    "views": { "activity-sidebar": [ { "id", "name", "when" } ] },
    "ui":        [ { "id", "slotId", "export", "order?", "when?" } ],
    "uiIr":      [ { "slotId", "ir" } ], // 兼容轨道：仅用于不可信声明式 UI，不是 anPdfsolver 当前路径
    "locales":   [ { "locale", "name", "path" } ]
  },

  "workers": [
    {
      "id": "worker",
      "language": "python",
      "entrypoint": "workers/python/main.py",
      "transport": "stdio-framed-json-v1"
    }
  ],
  "network": { "mode": "direct" },

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
- 旧 `spec.settings` 迁移到 `contributes.configuration.settings`（`PluginSettingDefinition` 结构不变）。
- 旧 `spec.connections` 迁移到 `contributes.configuration.connections`（`PluginConnectionDefinition` 结构不变）。
- 旧 `spec.language` 迁移到顶层 `language`（仅 `AnalysisPlugin` 使用，供 SDK 声明与校验参考）。
- 旧 `spec.contributes.*` 迁移到 `contributes.*`（`contextMenus` → `menus`，`locales`/`commands` 同名）。
- `kind` 从字符串枚举变成 `categories`（可多值），但内核仍按「是否声明某类 capability/entry」判定可执行类型。
- 迁移后内部 `api_version` 归一为 `researchcanvas.dev/v2`；`validate_manifest` 同时接受 v1alpha1 与 v2。

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

- 信任链：官方 `.myc` 包由 `scripts/pack-plugin.mjs` 生成，并由 `config/plugin-loading.json` 的显式清单选择进入 dev/release staging；`src-tauri/src/signing.rs` 只验证。当前仓库内跟踪包不包含签名（离线私钥不在仓库，`.gitignore` 排除 `id_ed25519*`）；拿到密钥后带 `--key` 重新打包即完成签发。
- 清单文件名统一 `plugin.json`；`payloads` 不含 `plugin.json` 自身。

## 3. 8 个 Host Bus API（文件解耦 + 中间件）

新增目录 `src-tauri/src/host_bus/`（每域一文件）：

| # | operation | 文件 | 能力 | 生命周期 |
|---|---|---|---|---|
| 1 | `graph.patch.propose` / `graph.patch.review` / `graph.patch.commit` / `graph.project.get` | `graph_patch.rs` | `graph.patch.propose/review/commit` | proposal/session 级；commit 由 Rust canonical registry 原子执行 |
| 2 | `lease.renew` | `lease.rs` | `host-bus.lease` | 租约级（续期/过期/吊销） |
| 3 | `event.subscribe` / `event.publish` | `events.rs` | `host-bus.event` | 订阅级（ack/replay/关闭） |
| 4 | `plugin.worker.open` / `plugin.worker.call` / `plugin.worker.cancel` / `plugin.worker.close` | `workers.rs` | `host-bus.worker` | worker 级（Rust 启动、身份绑定、取消和回收） |
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
- 信息流：request → 中间件链 → handler → response；大 payload 走 `blob.*` 多分块，handler 只拿 `BlobRef`。插件不能直写 graph storage；GraphPatch 只能进入 Rust canonical proposal/review/commit 流程，`GraphProjectRegistry` 完成原子 commit 后通过 `graph.project.get` 和 canonical `ProjectState` event 让 Host/Vue 更新。

### anPdfsolver 数据流

```text
installed verified frontend
  -> shared Vue singleton + PluginContext
  -> plugin SFC from dist/frontend.mjs
  -> files API returns BlobRef
  -> Rust Worker Manager opens Python worker through stdio-framed-json-v1
  -> plugin frontend/worker talks directly to provider when network.mode=direct
  -> provider streams small typed NDJSON frames
  -> plugin deterministically builds GraphPatch proposal
  -> Rust canonical registry reviews and commits atomically
  -> canonical ProjectState event + graph.project.get update Host
```

Host 只枚举物理 slot，例如 `workspace.toolbar.actions`、`workspace.dialogs`、`workspace.status`，再按已安装/启用插件的 `contributes.ui` 渲染 `PluginContributionSlot`。插件组件内部自己的 Vue slots 只是组件实现细节，不是 Host Slot Catalog。

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

> 状态：全部完成。官方 `plugins/packages/<id>@<version>.myc` 已用 `pack-plugin.mjs` 重建为 v2 JSON 清单，并通过显式 staging 清单进入运行时；
> 官方 `myc.pdf-canvas-agent@0.5.0` 已成为可信动态插件消费者：Host 加载
> `dist/frontend.mjs`，提供共享 Vue 单例和 `PluginContext`；插件拥有上传、批量分析、
> response SSE、错误和 review UI，并可在 `network.mode=direct` 下直接连接 provider。
> GraphPatch 只走 Rust canonical proposal/review/commit，`GraphProjectRegistry`
> 原子提交后发布 canonical `ProjectState` event；
> `plugins::tests::tracked_packages_pass_the_full_v2_install_pipeline` 对每个跟踪包走完整安装管线回归。

## 5. 目录与加载策略

插件目录分成三层，不允许互相替代：

| 层级 | 路径 | 说明 |
|---|---|---|
| 开发源码 | `my-plugins/<plugin-folder>/` | 官方和本地开发源码，如 `anPdfsolver`、`ancordis`、`anmarket`。Host 不直接从这里运行插件。 |
| 第三方缓存 | `my-third-plugins/` | 本地第三方包、导入包、日语包、One Dark Pro 等外部内容；该目录被 Git 忽略，并且 Desktop dev 默认不扫描。 |
| 调试运行时 | `.plugin-runtime/dev/` | `scripts/stage-plugin-runtime.mjs dev` 生成的运行时目录，包含 `packages/`、`installed/`、`quarantine/` 和 `dev-manifest.json`。 |
| 正式安装 | Desktop 管理的 app data | 安装器校验后展开的用户安装状态，不在仓库内维护。 |

`config/plugin-loading.json` 是 staging 的唯一输入。Desktop dev 默认策略是
`official-bundled-only`：只加载 `desktopDev.packageFiles` 中显式列出的官方内置包；
`my-plugins/` 里的开发插件必须通过 `desktopDev.enabledDevelopmentPluginIds` 或
`--with-dev-plugin <pluginId>` 显式选择后才会被打包到 `.plugin-runtime/dev`。

`scripts/stage-plugin-runtime.mjs` 只复制显式 package 文件或调用
`scripts/pack-plugin.mjs` 将显式 source 打包进 runtime。它不会从源码目录直接运行，
不会扫描 `my-third-plugins/`，也不会自动恢复日语包或 One Dark Pro。单个版本清理只接受
精确的 `pluginId@version`，并限制在 `.plugin-runtime/*` 内部。

`scripts/pack-plugin.mjs` 在写出 `.myc` 前校验 manifest 引用的 `frontend.entry`、
`workers[*].entrypoint` 和 `artifacts` 路径：这些路径必须是包内相对 POSIX 路径，
不能包含绝对路径、反斜杠或 `..`，并且必须已经存在于包内容中。旧
`contributes.uiIr` 的 Vue SFC 编译路径继续作为不可信声明式 UI 兼容轨道，但不是
`anPdfsolver` 当前路径。

目录、脚本、`package.json` 命令、Tauri resource wiring 和 Rust loader 已统一到
`.plugin-runtime/dev` / `.plugin-runtime/release-staging` 的 staging 边界。
