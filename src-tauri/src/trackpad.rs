//! Windows 精确式触控板完整帧桥；每条事件同时包含两个触点及合成缩放数据。
//! Complete-frame Precision Touchpad bridge with two contacts and composed pinch data.

#![cfg(windows)]

use serde::Serialize;
use std::{
    mem, ptr,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Sender},
        Mutex, OnceLock,
    },
};
use tauri::{AppHandle, Emitter, WebviewWindow};
use windows::{
    core::{w, BOOL, PCSTR},
    Win32::{
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::{
            Input::Pointer::POINTER_TOUCH_INFO,
            WindowsAndMessaging::{
                CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, PT_TOUCHPAD,
                WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE, WNDPROC,
            },
        },
    },
};

const EVENT_NAME: &str = "research-canvas://trackpad-frame";
// Windows 11 currently publishes these Precision Touchpad entry points by ordinal only.
// Windows 11 当前仅按序号导出这些 Precision Touchpad 入口，因此必须运行时解析。
const REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL: u16 = 2689;
const GET_POINTER_FRAME_TOUCHPAD_INFO_ORDINAL: u16 = 2693;

type RegisterTouchpadCapableWindowFn = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
type GetPointerFrameTouchpadInfoFn =
    unsafe extern "system" fn(u32, *mut u32, *mut POINTER_TOUCH_INFO) -> BOOL;

#[derive(Clone, Copy)]
struct TouchpadApis {
    register_window: RegisterTouchpadCapableWindowFn,
    get_frame: GetPointerFrameTouchpadInfoFn,
}

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackpadContact {
    id: u32,
    x: i32,
    y: i32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackpadFrameEvent {
    phase: &'static str,
    frame_id: u32,
    contacts: Vec<TrackpadContact>,
    center_x: f64,
    center_y: f64,
    span: f64,
    scale: f64,
    cursor_x: f64,
    cursor_y: f64,
}

#[derive(Default)]
struct PinchState {
    active: bool,
    initial_span: f64,
    last_frame_id: u32,
}

static ORIGINAL_WNDPROC: OnceLock<isize> = OnceLock::new();
static EVENT_SENDER: OnceLock<Sender<TrackpadFrameEvent>> = OnceLock::new();
static PINCH_STATE: OnceLock<Mutex<PinchState>> = OnceLock::new();
static SCALE_FACTOR_BITS: AtomicU64 = AtomicU64::new(1_f64.to_bits());
static TOUCHPAD_APIS: OnceLock<TouchpadApis> = OnceLock::new();

fn resolve_touchpad_apis() -> Result<TouchpadApis, String> {
    let user32 = unsafe { GetModuleHandleW(w!("user32.dll")) }
        .map_err(|error| format!("could not load user32.dll: {error}"))?;
    let register = unsafe {
        GetProcAddress(
            user32,
            PCSTR(REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL as usize as *const u8),
        )
    }
    .ok_or_else(|| {
        "this Windows build does not provide RegisterTouchpadCapableWindow (ordinal 2689)"
            .to_string()
    })?;
    let get_frame = unsafe {
        GetProcAddress(
            user32,
            PCSTR(GET_POINTER_FRAME_TOUCHPAD_INFO_ORDINAL as usize as *const u8),
        )
    }
    .ok_or_else(|| {
        "this Windows build does not provide GetPointerFrameTouchpadInfo (ordinal 2693)".to_string()
    })?;

    Ok(TouchpadApis {
        // The resolved ordinals have the signatures documented in Winuser.h.
        // 这些序号的签名来自 Winuser.h；动态解析可兼容尚未更新的 Rust SDK 元数据。
        register_window: unsafe { mem::transmute(register) },
        get_frame: unsafe { mem::transmute(get_frame) },
    })
}

fn pointer_id(wparam: WPARAM) -> u32 {
    (wparam.0 as u32) & 0xffff
}

fn read_complete_frame(pointer_id: u32) -> Option<Vec<POINTER_TOUCH_INFO>> {
    let apis = TOUCHPAD_APIS.get()?;
    let mut count = 0_u32;
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

fn frame_event(hwnd: HWND, pointer_id: u32) -> Option<TrackpadFrameEvent> {
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
    let mut cursor = pointer_info.ptPixelLocation;
    let _ = unsafe { ScreenToClient(hwnd, &mut cursor) };
    let window_scale = f64::from_bits(SCALE_FACTOR_BITS.load(Ordering::Relaxed)).max(0.5);
    let pinch = PINCH_STATE.get_or_init(|| Mutex::new(PinchState::default()));
    let mut pinch = pinch.lock().ok()?;
    let phase = if pinch.active {
        "update"
    } else {
        pinch.active = true;
        pinch.initial_span = span.max(1.0);
        "start"
    };
    pinch.last_frame_id = pointer_info.frameId;

    Some(TrackpadFrameEvent {
        phase,
        frame_id: pointer_info.frameId,
        contacts,
        center_x,
        center_y,
        span,
        scale: span / pinch.initial_span,
        cursor_x: f64::from(cursor.x) / window_scale,
        cursor_y: f64::from(cursor.y) / window_scale,
    })
}

fn end_event() -> Option<TrackpadFrameEvent> {
    let pinch = PINCH_STATE.get_or_init(|| Mutex::new(PinchState::default()));
    let mut pinch = pinch.lock().ok()?;
    if !pinch.active {
        return None;
    }
    pinch.active = false;
    pinch.initial_span = 0.0;
    Some(TrackpadFrameEvent {
        phase: "end",
        frame_id: pinch.last_frame_id,
        contacts: Vec::new(),
        center_x: 0.0,
        center_y: 0.0,
        span: 0.0,
        scale: 1.0,
        cursor_x: 0.0,
        cursor_y: 0.0,
    })
}

fn send_event(event: Option<TrackpadFrameEvent>) {
    if let (Some(event), Some(sender)) = (event, EVENT_SENDER.get()) {
        let _ = sender.send(event);
    }
}

unsafe extern "system" fn trackpad_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_POINTERDOWN | WM_POINTERUPDATE => {
            send_event(frame_event(hwnd, pointer_id(wparam)));
        }
        WM_POINTERUP | WM_POINTERCAPTURECHANGED => {
            send_event(end_event());
        }
        _ => {}
    }

    let original = ORIGINAL_WNDPROC.get().copied().unwrap_or_default();
    if original == 0 {
        return LRESULT(0);
    }
    let previous: WNDPROC = Some(unsafe { mem::transmute(original) });
    unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) }
}

/// 注册完整帧触控板输入；失败时 WebView 的标准缩放回退仍然可用。
/// Registers complete-frame touchpad input with the WebView wheel fallback on failure.
pub fn install(window: &WebviewWindow, app: AppHandle) -> Result<bool, String> {
    if ORIGINAL_WNDPROC.get().is_some() {
        return Ok(true);
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    SCALE_FACTOR_BITS.store(scale.to_bits(), Ordering::Relaxed);
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

    let previous =
        unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, trackpad_wnd_proc as *const () as isize) };
    if previous == 0 {
        return Err("Could not observe the Research Canvas window procedure".to_string());
    }
    ORIGINAL_WNDPROC
        .set(previous)
        .map_err(|_| "Trackpad window procedure is already registered".to_string())?;

    let registered = unsafe { (apis.register_window)(hwnd, true.into()) }.as_bool();
    if !registered {
        let error_code = unsafe { GetLastError() }.0;
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, previous);
        }
        return Err(format!(
            "RegisterTouchpadCapableWindow failed with Windows error {error_code}"
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::{combined_geometry, TrackpadContact};

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
}
