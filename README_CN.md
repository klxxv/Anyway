# Anyway

人类主导的研究画布。在交互式图谱上组织变量、方法、假设、证据和结果——以确定性算法提供导航与结构分析。

## 为什么叫 Anyway

研究本身就是混乱的。你跑实验、读论文、提出假设、改变想法。**Anyway** 给你一个持久化的、本地优先的工作空间，在这里每个想法、变量和结果都是一个可以连接、质疑、回溯的节点。没有云锁定，没有 AI 黑箱。你的图谱，你的规则。

## 核心概念

- **语义图谱** — 类型化节点（问题、假设、方法、证据、结果…）和有向边（支持、矛盾、导致、中介…）。图谱即是研究记录。
- **确定性算法** — BFS/DFS 遍历、环检测、最短路径、逻辑链、矛盾链、场景可达性、BP 风格影响传播。全部硬计算，绝不依赖 LLM。
- **六种布局投影** — 证据链、反驳链、树形、前缀 Huffman、表格、神经网络视图。一键切换。
- **消融场景** — 非破坏性叠加层，可禁用节点/边。比较「如果去掉这个变量会怎样」而不改变基础图谱。
- **插件系统（.myc）** — 带版本号、Ed25519 签名的 ZIP 包。支持主题插件、边样式插件、分析插件（WebAssembly）、工作空间插件和语言包插件。通过导入他人的 `.myc` 包，可以引用其他研究者的实验模块。
- **本地优先** — 所有数据存储在本地。可选 Git 集成用于版本控制与协作。

## 快速开始

```bash
npm install
npm run dev:desktop-web    # 启动 Vite 开发服务器
npm run desktop:dev        # 启动 Tauri 桌面应用
```

浏览器打开 `http://localhost:3000`。

## 脚本

| 命令 | 说明 |
|---|---|
| `npm run dev:desktop-web` | Vite 开发服务器 |
| `npm run build:desktop` | Tauri 生产构建 |
| `npm run desktop:dev` | Tauri 桌面开发模式 |
| `npm test` | 完整测试套件 |
| `npm run lint` | ESLint 检查 |
| `npx tsc --noEmit` | TypeScript 类型检查 |
| `cargo test --manifest-path src-tauri/Cargo.toml` | Rust 测试 |

## `.myc` 插件格式

`.myc` 是一个以 `plugin.json` 为根清单的 ZIP 归档文件。插件在安装时经过多层验证：

- **清单校验** — API 版本、插件类型、能力声明
- **签名验证** — 对清单内容的 Ed25519 签名进行验证，与受信任的发布者公钥比对
- **归档安全** — 路径穿越防护、大小限制（压缩包 16 MB、解压后 32 MB）、文件数上限
- **原子安装** — 暂存区解压 → 身份校验 → 原子性重命名到 `installed/`

构建插件：

```bash
node scripts/pack-plugin.mjs \
  plugins/sources/myc.onedarkpro \
  plugins/packages/myc.onedarkpro@1.3.0.myc
```

插件类型：

| 类型 | 引擎 | 用途 |
|---|---|---|
| `ThemePlugin` | declarative | 色彩主题与视觉令牌 |
| `EdgeStylePlugin` | declarative | 连线路由、线型、箭头标记 |
| `AnalysisPlugin` | wasm32-myc | WebAssembly 计算内核 |
| `WorkspacePlugin` | host-mediated | 导出、文件夹扫描、Git 操作 |
| `LocalePlugin` | declarative | 社区语言包 |

## 架构

```
src/                  Vue 3 渲染层（Vite + Vue Flow + Pinia）
  vue/                工作区组件、组合式函数与状态仓库
app/                  与渲染层无关的 TypeScript 领域与平台层
  lib/
    graph/            图算法（遍历、环、路径、可达性）
    layout/           确定性布局投影
    analysis/         逻辑链、影响传播
    project/          状态管理、导出、场景
src-tauri/            Rust 桌面后端
  src/
    graph_compiler/   规范化、哈希、不变式、布局、算法
    plugins.rs        .myc 安装器与校验器
    signing.rs        Ed25519 签名验证
    plugin_vm.rs      WebAssembly 沙箱（wasmi）
    workspace_host.rs 原生工作空间操作
plugins/              .myc 插件源码、分发包与已安装状态
tests/                测试套件（core、platform、workspace、SDK、编译器比对）
```

TypeScript 和 Rust 两端图算法实现通过编译器逐位比对测试（`tests/compiler-parity.test.ts`）保持严格一致。两组实现处理相同的数据夹具，对每个算法断言完全相同的输出。

## 技术栈

| 层 | 技术 |
|---|---|
| 前端 | Vue 3、Vite、Vue Flow、Pinia、Tailwind CSS |
| 桌面 | Tauri 2、WebView2（Windows） |
| 图算法（TS） | TypeScript，纯函数 |
| 图算法（Rust） | Rust、serde、sha2 |
| 插件运行时 | WebAssembly（wasmi） |
| 密码学 | Ed25519（ed25519-dalek） |
| CI/CD | GitHub Actions |

## 许可证

详见 [LICENSE](LICENSE)。
