# 触控板输入审计 / Trackpad input audit

## 审计结论 / Findings

| 层级 | 旧实现 | 阻塞与竞争 | 新实现 |
| --- | --- | --- | --- |
| Windows | 只替换 Tauri 顶层 `WndProc` | WebView2 子窗口可以成为真实消息目标 | 注册整个 Tauri UI 线程，用 `WH_GETMESSAGE` 观察同线程所有窗口消息 |
| 原生帧 | 仅使用双指间距 | 丢弃双指中心的 X/Y 合成移动 | 每条 `WM_POINTER` 通过 `GetPointerFrameTouchpadInfo` 一次读取双触点完整帧 |
| Rust 手势 | 只发送 `scale` | 主应用不等价于已验证 Demo | 同帧计算中心、跨度、归一化 `panX/panY` 和 `scale` |
| WebView | 原生与 `Ctrl+Wheel` 两套缩放逻辑 | 可能重复处理，也可能同时被关闭 | 桌面端仅保留原生完整帧状态机，派生滚轮只被消费 |
| React Flow | `zoomOnPinch`/`panOnScroll` 与自定义逻辑并行 | 多个处理器争抢视口 | Tauri 中关闭内置 pinch/scroll，每个 `requestAnimationFrame` 只提交一次合成视口 |

## 唯一数据流 / Single data flow

```text
Windows UI thread WM_POINTER frame
  -> GetPointerFrameTouchpadInfo(two contacts atomically)
  -> Rust { center, span, panX, panY, scale }
  -> one Tauri frame event
  -> latest frame per requestAnimationFrame
  -> one composed React Flow setViewport
```

## 平移与缩放合成 / Composed pan and zoom

手势开始时记录视口与光标锚点。当前双指中心相对起点的移动，按触控板物理尺寸归一化后映射到画布；同一帧的双指跨度比例决定缩放。最终视口由一个公式同时计算 X、Y 和 zoom，不存在轴间时序差。

## 验证边界 / Verification boundary

自动化测试覆盖完整帧几何、物理表面归一化、平移+缩放合成和缩放边界。真实 Precision Touchpad 手势仍需在安装版中进行最终物理验证，因为软件测试无法伪造用户的实际驱动输入。
