//! Windows 精确式触控板观察桥；不接管系统手势，只转发原始接触点。 / Observes Precision Touchpad contacts without taking over system gestures.

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
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, WPARAM},
    Graphics::Gdi::ScreenToClient,
    UI::{
        Accessibility::RegisterPointerInputTargetEx,
        Input::Pointer::{GetPointerInfo, POINTER_INFO},
        WindowsAndMessaging::{
            CallWindowProcW, SetWindowLongPtrW, GWLP_WNDPROC, PT_TOUCHPAD, WM_POINTERDOWN,
            WM_POINTERUP, WM_POINTERUPDATE, WNDPROC,
        },
    },
};

const EVENT_NAME: &str = "research-canvas://trackpad-contact";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackpadContactEvent {
    phase: &'static str,
    pointer_id: u32,
    contact_count: usize,
    x: f64,
    y: f64,
    timestamp_ms: u32,
}

static ORIGINAL_WNDPROC: OnceLock<isize> = OnceLock::new();
static EVENT_SENDER: OnceLock<Sender<TrackpadContactEvent>> = OnceLock::new();
static ACTIVE_CONTACTS: OnceLock<Mutex<HashSet<u32>>> = OnceLock::new();
static SCALE_FACTOR_BITS: AtomicU64 = AtomicU64::new(1_f64.to_bits());

unsafe extern "system" fn trackpad_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if matches!(message, WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP) {
        let pointer_id = (wparam.0 as u32) & 0xffff;
        let mut pointer_info = POINTER_INFO::default();
        if unsafe { GetPointerInfo(pointer_id, &mut pointer_info) }.is_ok()
            && pointer_info.pointerType == PT_TOUCHPAD
        {
            let phase = match message {
                WM_POINTERDOWN => "down",
                WM_POINTERUP => "up",
                _ => "move",
            };
            let mut point = pointer_info.ptPixelLocation;
            let _ = unsafe { ScreenToClient(hwnd, &mut point) };
            let contacts = ACTIVE_CONTACTS.get_or_init(|| Mutex::new(HashSet::new()));
            if let Ok(mut active) = contacts.lock() {
                if message == WM_POINTERUP {
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
                        timestamp_ms: pointer_info.dwTime,
                    });
                }
            }
        }
    }

    let original = ORIGINAL_WNDPROC.get().copied().unwrap_or_default();
    if original == 0 {
        return LRESULT(0);
    }
    let previous: WNDPROC = Some(unsafe { mem::transmute(original) });
    unsafe { CallWindowProcW(previous, hwnd, message, wparam, lparam) }
}

/// 注册观察型触控板输入；失败时 WebView 的标准缩放与触屏 PointerEvent 仍然可用。 / Registers observation-only touchpad input with safe web fallbacks on failure.
pub fn install(window: &WebviewWindow, app: AppHandle) -> Result<bool, String> {
    if ORIGINAL_WNDPROC.get().is_some() {
        return Ok(true);
    }
    let hwnd = window.hwnd().map_err(|error| error.to_string())?;
    let scale = window.scale_factor().unwrap_or(1.0);
    SCALE_FACTOR_BITS.store(scale.to_bits(), Ordering::Relaxed);

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

    let registered = unsafe { RegisterPointerInputTargetEx(hwnd, PT_TOUCHPAD, true) }.as_bool();
    if !registered {
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_WNDPROC, previous);
        }
        return Err("Windows did not expose Precision Touchpad pointer input".to_string());
    }
    Ok(true)
}
