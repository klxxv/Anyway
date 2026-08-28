# AnCordis system extension

_Anyway 的官方 Cordis Extension Host 最小骨架；协议草案基于基线 `ae47fe6`，当前不接入现有插件加载器。_

---

## 📋 定位

AnCordis 是运行在 **非 Kernel 进程池**中的官方系统插件。它承载 Cordis 的服务注册、依赖解析、事件订阅和可逆 Effect，并通过 Anyway Kernel Host Bus 调度并行 worker。

AnCordis **不是安全边界**。它不能替代 Kernel 的身份验证、Capability 授权、Blob 访问控制、进程监督或审计。任何第三方代码都必须经过独立进程和 Host RPC；官方可信插件才可以与 AnCordis 共进程运行。

## 🏗️ 目录

| 文件 | 作用 |
| --- | --- |
| `manifest.json` | 官方系统插件的提议清单，声明双运行模式和 Principal 规则 |
| `protocol.ts` | 无依赖的可序列化 Host Bus、Cordis、Blob、worker 和 Vue IR 类型 |
| `README.md` | 当前骨架边界和接入约束 |

## 🔐 信任与运行模式

| 插件类别 | 运行位置 | 安全含义 |
| --- | --- | --- |
| Anyway 官方可信插件 | AnCordis 同一进程 | 享受完整 Cordis 语义，但不形成 OS 安全边界 |
| 第三方插件 | 独立 worker 进程 | 只能使用 Kernel 重新授权后的 Host RPC |
| Kernel | Kernel 进程 | 唯一安全边界和最终授权者 |

`SecurityContext.originalPrincipal` 必须由 Kernel 创建。AnCordis 转发调用时只能设置 `immediateCaller` 并追加 `delegationChain`，不能冒充原始调用者或扩大 Capability。每个子插件都必须拥有自己的 `PrincipalRef`。

## 📡 协议约束

`protocol.ts` 只允许可序列化数据：

- 服务用 `ServiceDescriptor` 描述方法、JSON Schema 和 Capability，不传函数引用
- 注册、订阅和 worker 使用 `RegistrationLease`，租约过期时由 Kernel 撤销
- 事件通过 `EventSubscriptionRequest` 声明 `serial`、`parallel` 或 `latest` 投递策略
- 大数据通过 `BlobRef` 传递，RPC 只传小型元数据和引用
- worker 通过 `WorkerSpawnRequest`/`WorkerStopRequest` 管理，语言不限制为 TypeScript
- Vue UI 通过 `VueIrNode` 表达，不包含任意 HTML、JavaScript 表达式或 Vue 回调

## 🧭 后续接入顺序

1. 在 Kernel 中实现 Host Bus 的消息验证、Principal 绑定、Capability 重授权和租约回收
2. 将现有串行 Agent 执行器拆出受配额控制的 worker supervisor
3. 实现 AnCordis 的 Cordis adapter，并把 Effect 清理绑定到 registration lease
4. 增加 AnMarket 作为独立系统插件，使用供应链扫描和安装服务，但不能修改 Kernel 信任根
5. 最后再把正式 SDK、插件清单和前端 Vue IR 渲染器接入本协议

> ⚠️ **当前状态：** 这些文件是设计和协议骨架。它们不会自动改变现有插件 SDK、Cargo 构建、前端渲染或插件清单行为。
