//! Windows 精确式触控板缩放桥；仅转发实现捏合缩放所需的接触点。
//! Precision Touchpad bridge that forwards only contacts needed for pinch zoom.

#![cfg(windows)]

use serde::Serialize;
use std::{
    collections::HashSet,
    mem,
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
            Input::Pointer::{
                GetPointerInfo, POINTER_FLAG_CANCELED, POINTER_INFO, POINTER_TOUCH_INFO,
            },
            WindowsAndMessaging::{
                CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, PT_TOUCHPAD,
                WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE, WNDPROC,
            },
        },
    },
};

const EVENT_NAME: &str = "research-canvas://trackpad-contact";
// Windows 11 currently publishes these Precision Touchpad entry points by ordinal only.
// Windows 11 当前仅按序号导出这些 Precision Touchpad 入口，因此必须运行时解析。
const REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL: u16 = 2689;
const GET_POINTER_TOUCHPAD_INFO_ORDINAL: u16 = 2691;

type RegisterTouchpadCapableWindowFn = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
type GetPointerTouchpadInfoFn = unsafe extern "system" fn(u32, *mut POINTER_TOUCH_INFO) -> BOOL;

#[derive(Clone, Copy)]
struct TouchpadApis {
    register_window: RegisterTouchpadCapableWindowFn,
    get_pointer_info: GetPointerTouchpadInfoFn,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackpadContactEvent {
    phase: &'static str,
    pointer_id: u32,
    contact_count: usize,
    x: f64,
    y: f64,
    physical_x: i32,
    physical_y: i32,
    timestamp_ms: u32,
}

static ORIGINAL_WNDPROC: OnceLock<isize> = OnceLock::new();
static EVENT_SENDER: OnceLock<Sender<TrackpadContactEvent>> = OnceLock::new();
static ACTIVE_CONTACTS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
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
    let get_pointer_info = unsafe {
        GetProcAddress(
            user32,
            PCSTR(GET_POINTER_TOUCHPAD_INFO_ORDINAL as usize as *const u8),
        )
    }
    .ok_or_else(|| {
        "this Windows build does not provide GetPointerTouchpadInfo (ordinal 2691)".to_string()
    })?;

    Ok(TouchpadApis {
        // The resolved ordinals have the signatures documented in Winuser.h.
        // 这些序号的签名来自 Winuser.h；动态解析可兼容尚未更新的 Rust SDK 元数据。
        register_window: unsafe { mem::transmute(register) },
        get_pointer_info: unsafe { mem::transmute(get_pointer_info) },
    })
}

fn touchpad_info_for_pointer(pointer_id: u32) -> Option<POINTER_TOUCH_INFO> {
    if let Some(apis) = TOUCHPAD_APIS.get() {
        let mut touchpad_info = POINTER_TOUCH_INFO::default();
        if unsafe { (apis.get_pointer_info)(pointer_id, &mut touchpad_info) }.as_bool() {
            return Some(touchpad_info);
        }
    }

    // Keep GetPointerInfo as a compatibility fallback for early preview builds.
    // 为早期预览版保留 GetPointerInfo 兼容回退。
    let mut pointer_info = POINTER_INFO::default();
    if unsafe { GetPointerInfo(pointer_id, &mut pointer_info) }.is_err() {
        return None;
    }
    Some(POINTER_TOUCH_INFO {
        pointerInfo: pointer_info,
        ..Default::default()
    })
}

unsafe extern "system" fn trackpad_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if matches!(message, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP) {
        let pointer_id = (wparam.0 as u32) & 0xffff;
        if let Some(touchpad_info) = touchpad_info_for_pointer(pointer_id)
            .filter(|touchpad_info| touchpad_info.pointerInfo.pointerType == PT_TOUCHPAD)
        {
            let pointer_info = touchpad_info.pointerInfo;
            let cancelled =
                (pointer_info.pointerFlags & POINTER_FLAG_CANCELED) != Default::default();
            let phase = if cancelled {
                "cancel"
            } else {
                match message {
                    WM_POINTERDOWN => "down",
                    WM_POINTERUP => "up",
                    _ => "move",
                }
            };
            let mut point = pointer_info.ptPixelLocation;
            let _ = unsafe { ScreenToClient(hwnd, &mut point) };
            let contacts = ACTIVE_CONTACTS.get_or_init(|| Mutex::new(HashSet::new()));
            if let Ok(mut active) = contacts.lock() {
                if message == WM_POINTERUP || cancelled {
                    active.remove(&pointer_id);
                } else {
                    active.insert(pointer_id);
                }
                let scale = f64::from_bits(SCALE_FACTOR_BITS.load(Ordering::Relaxed)).max(0.5);
                if let Some(sender) = EVENT_SENDER.get() {
                    let _ = sender.send(TrackpadContactEvent {
                        phase,
                        pointer_id,
                        contact_count: active.len(),
                        x: f64::from(point.x) / scale,
                        y: f64::from(point.y) / scale,
                        // Device-relative HIMETRIC coordinates expose the distance
                        // between fingers; pixel coordinates intentionally stay at
                        // the mouse cursor for the whole touchpad gesture.
                        // HIMETRIC 是触控板设备坐标，可用于计算双指间距；像素坐标则
                        // 在整个手势期间固定为鼠标位置，不能用于捏合比例。
                        physical_x: pointer_info.ptHimetricLocation.x,
                        physical_y: pointer_info.ptHimetricLocation.y,
                        timestamp_ms: pointer_info.dwTime,
                    });
                }
            }
        }
    } else if message == WM_POINTERCAPTURECHANGED {
        // 驱动或窗口焦点中断时清理全部接触点，避免下一次手势继承陈旧状态。
        // Clear stale contacts after capture loss so the next gesture starts cleanly.
        let contacts = ACTIVE_CONTACTS.get_or_init(|| Mutex::new(HashSet::new()));
        if let Ok(mut active) = contacts.lock() {
            active.clear();
        }
        if let Some(sender) = EVENT_SENDER.get() {
            let _ = sender.send(TrackpadContactEvent {
                phase: "cancel",
                pointer_id: (wparam.0 as u32) & 0xffff,
                contact_count: 0,
                x: 0.0,
                y: 0.0,
                physical_x: 0,
                physical_y: 0,
                timestamp_ms: 0,
            });
        }
    }

    let original = ORIGINAL_WNDPROC.get().copied().unwrap_or_default();
    if original == 0 {
        return LRESULT(0);
    }
    let previous: WNDPROC = Some(unsafe { mem::transmute(original) });
    unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) }
}

/// 注册观察型触控板输入；失败时 WebView 的标准缩放与触屏 PointerEvent 仍然可用。
/// Registers observation-only touchpad input with safe web fallbacks on failure.
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

    let (sender, receiver) = mpsc::channel::<TrackpadContactEvent>();
    EVENT_SENDER
        .set(sender)
        .map_err(|_| "Trackpad event channel is already registered".to_string())?;
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

    // This is the supported Windows 11 opt-in for touchpad pan/zoom WM_POINTER input.
    // 这是 Windows 11 为触控板平移/缩放 WM_POINTER 提供的受支持入口。
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
