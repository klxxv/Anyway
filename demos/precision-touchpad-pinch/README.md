# Precision Touchpad Pinch Demo / 精确式触控板捏合 Demo

这个独立程序不使用 React、WebView、ReactFlow 或应用插件系统。它通过 Rust `windows`
crate 调用 Windows 11 Precision Touchpad API，并在每次 `WM_POINTER` 消息中使用
`GetPointerFrameTouchpadInfo` 一次读取完整触点帧。

The demo deliberately avoids React, WebView, ReactFlow, and the application plugin system.
It uses the Rust `windows` crate and reads each complete Precision Touchpad frame atomically.

## 运行 / Run

```powershell
cargo run --manifest-path demos/precision-touchpad-pinch/Cargo.toml
```

将鼠标放在 Demo 窗口内，然后用双指捏合。窗口应同时显示：

- 两个蓝色物理触点；
- 同一帧内合成的中心点；
- 双指间距、相对缩放值和帧编号。

Place the mouse inside the demo window and pinch with two fingers. The window should show two
physical contacts, their combined center, the distance/scale, and a monotonically increasing
frame number.

## 为什么不使用手势库 / Why no gesture wrapper

- `winit` 当前明确说明 Windows 不支持其 `PinchGesture` 事件。
- 浏览器手势库（包括 `@use-gesture`）在 Chromium/Windows 上仍依赖 `Ctrl+Wheel`，
  无法修复 WebView 没有生成该事件的情况。
- 因此最小可验证路径是 `windows` crate + 原生完整帧 API。只有这条路径失败时，
  才值得增加 C++/Rust FFI 层。

References:

- <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/getpointertouchpadinfo>
- <https://learn.microsoft.com/en-us/windows/win32/input-precisiontouchpad/registertouchpadcapable>
- <https://docs.rs/winit/latest/winit/event/enum.WindowEvent.html#variant.PinchGesture>
