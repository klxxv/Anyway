//! Windows Precision Touchpad complete-frame bridge.
//! Windows 精确式触控板完整帧桥接。

#![cfg(windows)]

use serde::Serialize;
use std::{
    mem, ptr,
    sync::{
        atomic::{AtomicIsize, AtomicU64, Ordering},
        mpsc::{self, Sender},
        Mutex, OnceLock,
    },
};
use tauri::{AppHandle, Emitter, WebviewWindow};
use windows::{
    core::{w, BOOL, PCSTR},
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::{
            Input::Pointer::{GetPointerDeviceRects, POINTER_TOUCH_INFO},
            WindowsAndMessaging::{
                CallNextHookEx, GetWindowThreadProcessId, SetWindowsHookExW, MSG, PT_TOUCHPAD,
                WH_GETMESSAGE, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERUP,
                WM_POINTERUPDATE,
            },
        },
    },
};

const EVENT_NAME: &str = "research-canvas://trackpad-frame";
// Windows 11 publishes the new Precision Touchpad entry points by ordinal.
// Windows 11 当前通过序号导出新的 Precision Touchpad 入口。
const REGISTER_TOUCHPAD_CAPABLE_THREAD_ORDINAL: u16 = 2688;
const REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL: u16 = 2689;
const GET_POINTER_FRAME_TOUCHPAD_INFO_ORDINAL: u16 = 2693;

type RegisterTouchpadCapableThreadFn = unsafe extern "system" fn(BOOL) -> BOOL;
type RegisterTouchpadCapableWindowFn = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
type GetPointerFrameTouchpadInfoFn =
    unsafe extern "system" fn(u32, *mut u32, *mut POINTER_TOUCH_INFO) -> BOOL;

#[derive(Clone, Copy)]
struct TouchpadApis {
    register_thread: RegisterTouchpadCapableThreadFn,
    register_window: RegisterTouchpadCapableWindowFn,
    get_frame: GetPointerFrameTouchpadInfoFn,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackpadContact {
    id: u32,
    x: i32,
    y: i32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackpadFrameEvent {
    phase: &'static str,
    frame_id: u32,
    contacts: Vec<TrackpadContact>,
    center_x: f64,
    center_y: f64,
    span: f64,
    scale: f64,
    pan_x: f64,
    pan_y: f64,
    device_width: f64,
    device_height: f64,
    cursor_x: f64,
    cursor_y: f64,
}

#[derive(Default)]
struct GestureState {
    active: bool,
    initial_center_x: f64,
    initial_center_y: f64,
    initial_span: f64,
    last_frame_id: u32,
}

static EVENT_SENDER: OnceLock<Sender<TrackpadFrameEvent>> = OnceLock::new();
static GESTURE_STATE: OnceLock<Mutex<GestureState>> = OnceLock::new();
static SCALE_FACTOR_BITS: AtomicU64 = AtomicU64::new(1_f64.to_bits());
static ROOT_HWND: AtomicIsize = AtomicIsize::new(0);
static MESSAGE_HOOK: AtomicIsize = AtomicIsize::new(0);
static TOUCHPAD_APIS: OnceLock<TouchpadApis> = OnceLock::new();

fn resolve_ordinal<T: Copy>(ordinal: u16, name: &str) -> Result<T, String> {
    let user32 = unsafe { GetModuleHandleW(w!("user32.dll")) }
        .map_err(|error| format!("could not load user32.dll: {error}"))?;
    let address = unsafe { GetProcAddress(user32, PCSTR(ordinal as usize as *const u8)) }
        .ok_or_else(|| format!("this Windows build does not provide {name} (ordinal {ordinal})"))?;
    // The signatures match Microsoft's Winuser.h declarations.
    // 函数签名与微软 Winuser.h 声明一致。
    Ok(unsafe { mem::transmute_copy(&address) })
}

fn resolve_touchpad_apis() -> Result<TouchpadApis, String> {
    Ok(TouchpadApis {
        register_thread: resolve_ordinal(
            REGISTER_TOUCHPAD_CAPABLE_THREAD_ORDINAL,
            "RegisterTouchpadCapableThread",
        )?,
        register_window: resolve_ordinal(
            REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL,
            "RegisterTouchpadCapableWindow",
        )?,
        get_frame: resolve_ordinal(
            GET_POINTER_FRAME_TOUCHPAD_INFO_ORDINAL,
            "GetPointerFrameTouchpadInfo",
        )?,
    })
}

fn pointer_id(wparam: WPARAM) -> u32 {
    (wparam.0 as u32) & 0xffff
}

fn read_complete_frame(pointer_id: u32) -> Option<Vec<POINTER_TOUCH_INFO>> {
    let apis = TOUCHPAD_APIS.get()?;
    let mut count = 0_u32;
    // A null buffer queries the contact count for this exact native frame.
    // 空缓冲区先查询这一个原生帧的触点数量。
    let _ = unsafe { (apis.get_frame)(pointer_id, &mut count, ptr::null_mut()) };
    if count == 0 || count > 16 {
        return None;
    }
    let mut frame = vec![POINTER_TOUCH_INFO::default(); count as usize];
    if !unsafe { (apis.get_frame)(pointer_id, &mut count, frame.as_mut_ptr()) }.as_bool() {
        return None;
    }
    frame.truncate(count as usize);
    Some(frame)
}

fn combined_geometry(contacts: &[TrackpadContact]) -> Option<(f64, f64, f64)> {
    if contacts.len() != 2 {
        return None;
    }
    let first = contacts[0];
    let second = contacts[1];
    Some((
        f64::from(first.x + second.x) / 2.0,
        f64::from(first.y + second.y) / 2.0,
        f64::from(second.x - first.x).hypot(f64::from(second.y - first.y)),
    ))
}

fn normalized_pan(
    initial_center: (f64, f64),
    current_center: (f64, f64),
    device_size: (f64, f64),
) -> (f64, f64) {
    let width = device_size.0.max(1.0);
    let height = device_size.1.max(1.0);
    (
        (current_center.0 - initial_center.0) / width,
        (current_center.1 - initial_center.1) / height,
    )
}

fn device_size(frame: &[POINTER_TOUCH_INFO]) -> (f64, f64) {
    let mut device = RECT::default();
    let mut display = RECT::default();
    if let Some(info) = frame.first() {
        if unsafe {
            GetPointerDeviceRects(info.pointerInfo.sourceDevice, &mut device, &mut display)
        }
        .is_ok()
        {
            return (
                f64::from((device.right - device.left).abs()).max(1.0),
                f64::from((device.bottom - device.top).abs()).max(1.0),
            );
        }
    }
    // The Windows injection sample uses a 10000 x 6000 himetric touchpad.
    // 与 Windows 官方注入示例保持同一个安全回退尺寸。
    (10_000.0, 6_000.0)
}

fn root_hwnd() -> Option<HWND> {
    let raw = ROOT_HWND.load(Ordering::Relaxed);
    (raw != 0).then(|| HWND(raw as *mut _))
}

fn frame_event(pointer_id: u32) -> Option<TrackpadFrameEvent> {
    let frame = read_complete_frame(pointer_id)?;
    let contacts: Vec<TrackpadContact> = frame
        .iter()
        .filter(|info| info.pointerInfo.pointerType == PT_TOUCHPAD)
        .map(|info| TrackpadContact {
            id: info.pointerInfo.pointerId,
            x: info.pointerInfo.ptHimetricLocation.x,
            y: info.pointerInfo.ptHimetricLocation.y,
        })
        .collect();
    let (center_x, center_y, span) = combined_geometry(&contacts)?;
    let pointer_info = frame.first()?.pointerInfo;
    let surface = device_size(&frame);
    let gesture = GESTURE_STATE.get_or_init(|| Mutex::new(GestureState::default()));
    let mut gesture = gesture.lock().ok()?;
    if gesture.active && gesture.last_frame_id == pointer_info.frameId {
        return None;
    }
    let phase = if gesture.active {
        "update"
    } else {
        gesture.active = true;
        gesture.initial_center_x = center_x;
        gesture.initial_center_y = center_y;
        gesture.initial_span = span.max(1.0);
        "start"
    };
    gesture.last_frame_id = pointer_info.frameId;
    let (pan_x, pan_y) = normalized_pan(
        (gesture.initial_center_x, gesture.initial_center_y),
        (center_x, center_y),
        surface,
    );

    let mut cursor = pointer_info.ptPixelLocation;
    let hwnd = root_hwnd()?;
    let _ = unsafe { ScreenToClient(hwnd, &mut cursor) };
    let window_scale = f64::from_bits(SCALE_FACTOR_BITS.load(Ordering::Relaxed)).max(0.5);

    Some(TrackpadFrameEvent {
        phase,
        frame_id: pointer_info.frameId,
        contacts,
        center_x,
        center_y,
        span,
        scale: span / gesture.initial_span,
        pan_x,
        pan_y,
        device_width: surface.0,
        device_height: surface.1,
        cursor_x: f64::from(cursor.x) / window_scale,
        cursor_y: f64::from(cursor.y) / window_scale,
    })
}

fn end_event() -> Option<TrackpadFrameEvent> {
    let gesture = GESTURE_STATE.get_or_init(|| Mutex::new(GestureState::default()));
    let mut gesture = gesture.lock().ok()?;
    if !gesture.active {
        return None;
    }
    gesture.active = false;
    gesture.initial_span = 0.0;
    Some(TrackpadFrameEvent {
        phase: "end",
        frame_id: gesture.last_frame_id,
        contacts: Vec::new(),
        center_x: 0.0,
        center_y: 0.0,
        span: 0.0,
        scale: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
        device_width: 0.0,
        device_height: 0.0,
        cursor_x: 0.0,
        cursor_y: 0.0,
    })
}

fn send_event(event: Option<TrackpadFrameEvent>) {
    if let (Some(event), Some(sender)) = (event, EVENT_SENDER.get()) {
        let _ = sender.send(event);
    }
}

/// Observes every message dequeued by the Tauri UI thread, regardless of which
/// same-thread WebView host child owns the pointer target.
/// 观察 Tauri UI 线程取出的每条消息，不再依赖某个 WebView 子窗口句柄。
unsafe extern "system" fn message_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && lparam.0 != 0 {
        let message = unsafe { &*(lparam.0 as *const MSG) };
        match message.message {
            WM_POINTERDOWN | WM_POINTERUPDATE => {
                send_event(frame_event(pointer_id(message.wParam)));
            }
            WM_POINTERUP | WM_POINTERCAPTURECHANGED => {
                send_event(end_event());
            }
            _ => {}
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Installs the Demo-equivalent complete-frame reader on the Tauri UI thread.
/// 在 Tauri UI 线程安装与 Demo 等价的完整帧读取器。
pub fn install(window: &WebviewWindow, app: AppHandle) -> Result<bool, String> {
    if MESSAGE_HOOK.load(Ordering::Relaxed) != 0 {
        return Ok(true);
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    ROOT_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
    SCALE_FACTOR_BITS.store(
        window.scale_factor().unwrap_or(1.0).to_bits(),
        Ordering::Relaxed,
    );

    let apis = resolve_touchpad_apis()?;
    TOUCHPAD_APIS
        .set(apis)
        .map_err(|_| "Precision Touchpad APIs are already registered".to_string())?;
    let (sender, receiver) = mpsc::channel::<TrackpadFrameEvent>();
    EVENT_SENDER
        .set(sender)
        .map_err(|_| "Trackpad frame channel is already registered".to_string())?;
    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            let _ = app.emit(EVENT_NAME, event);
        }
    });

    // Register the whole UI thread so WRY/WebView host children created on the
    // thread receive the same raw two-contact frames as the standalone Demo.
    // 注册整个 UI 线程，使 WRY/WebView 宿主子窗口与 Demo 一样接收原始双触点帧。
    if !unsafe { (apis.register_thread)(true.into()) }.as_bool() {
        return Err(format!(
            "RegisterTouchpadCapableThread failed with Windows error {}",
            unsafe { GetLastError() }.0
        ));
    }
    if !unsafe { (apis.register_window)(hwnd, true.into()) }.as_bool() {
        return Err(format!(
            "RegisterTouchpadCapableWindow failed with Windows error {}",
            unsafe { GetLastError() }.0
        ));
    }

    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if thread_id == 0 {
        return Err("Could not resolve the Tauri UI thread".to_string());
    }
    let hook = unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(message_hook), None, thread_id) }
        .map_err(|error| format!("Could not observe the Tauri UI message queue: {error}"))?;
    MESSAGE_HOOK.store(hook.0 as isize, Ordering::Relaxed);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{combined_geometry, normalized_pan, TrackpadContact};

    #[test]
    fn combines_both_axes_and_span_from_one_native_frame() {
        let contacts = [
            TrackpadContact {
                id: 1,
                x: 10,
                y: 20,
            },
            TrackpadContact {
                id: 2,
                x: 40,
                y: 60,
            },
        ];
        assert_eq!(combined_geometry(&contacts), Some((25.0, 40.0, 50.0)));
        assert_eq!(combined_geometry(&contacts[..1]), None);
    }

    #[test]
    fn normalizes_two_axis_pan_against_the_touchpad_surface() {
        assert_eq!(
            normalized_pan((5000.0, 3000.0), (5500.0, 2400.0), (10_000.0, 6000.0)),
            (0.05, -0.1)
        );
    }
}
