//! Minimal Windows 11 Precision Touchpad pinch visualizer.
//! Windows 11 精确式触控板完整帧捏合可视化 Demo。

#![cfg(windows)]

use std::{
    mem, ptr,
    sync::{Mutex, OnceLock},
};
use windows::{
    core::{w, BOOL, PCSTR},
    Win32::{
        Foundation::{GetLastError, COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{
            BeginPaint, CreateSolidBrush, DeleteObject, Ellipse, EndPaint, FillRect,
            InvalidateRect, LineTo, MoveToEx, SelectObject, SetBkMode, SetTextColor, TextOutW,
            HBRUSH, PAINTSTRUCT, TRANSPARENT,
        },
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::{
            Input::Pointer::{GetPointerDeviceRects, POINTER_TOUCH_INFO},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW,
                LoadCursorW, PostQuitMessage, RegisterClassW, ShowWindow, TranslateMessage,
                CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, IDC_ARROW, MSG, PT_TOUCHPAD, SW_SHOW,
                WINDOW_EX_STYLE, WM_DESTROY, WM_PAINT, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN,
                WM_POINTERUP, WM_POINTERUPDATE, WNDCLASSW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
            },
        },
    },
};

const REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL: u16 = 2689;
const GET_POINTER_FRAME_TOUCHPAD_INFO_ORDINAL: u16 = 2693;

type RegisterTouchpadCapableWindowFn = unsafe extern "system" fn(HWND, BOOL) -> BOOL;
type GetPointerFrameTouchpadInfoFn =
    unsafe extern "system" fn(u32, *mut u32, *mut POINTER_TOUCH_INFO) -> BOOL;

#[derive(Clone, Copy, Debug)]
struct TouchpadApis {
    register_window: RegisterTouchpadCapableWindowFn,
    get_frame: GetPointerFrameTouchpadInfoFn,
}

#[derive(Clone, Copy, Default)]
struct Contact {
    id: u32,
    x: i32,
    y: i32,
}

#[derive(Default)]
struct DemoState {
    registration: String,
    contacts: Vec<Contact>,
    device_rect: RECT,
    initial_span: Option<f64>,
    span: f64,
    scale: f64,
    center_x: f64,
    center_y: f64,
    frame_number: u64,
}

static APIS: OnceLock<TouchpadApis> = OnceLock::new();
static STATE: OnceLock<Mutex<DemoState>> = OnceLock::new();

fn state() -> &'static Mutex<DemoState> {
    STATE.get_or_init(|| {
        Mutex::new(DemoState {
            scale: 1.0,
            ..Default::default()
        })
    })
}

fn resolve_apis() -> Result<TouchpadApis, String> {
    let user32 = unsafe { GetModuleHandleW(w!("user32.dll")) }
        .map_err(|error| format!("GetModuleHandleW failed: {error}"))?;
    let register = unsafe {
        GetProcAddress(
            user32,
            PCSTR(REGISTER_TOUCHPAD_CAPABLE_WINDOW_ORDINAL as usize as *const u8),
        )
    }
    .ok_or_else(|| "RegisterTouchpadCapableWindow (ordinal 2689) is unavailable".to_string())?;
    let get_frame = unsafe {
        GetProcAddress(
            user32,
            PCSTR(GET_POINTER_FRAME_TOUCHPAD_INFO_ORDINAL as usize as *const u8),
        )
    }
    .ok_or_else(|| "GetPointerFrameTouchpadInfo (ordinal 2693) is unavailable".to_string())?;

    Ok(TouchpadApis {
        // Signatures are the Winuser.h declarations documented by Microsoft.
        // 函数签名与微软 Winuser.h 文档一致。
        register_window: unsafe { mem::transmute(register) },
        get_frame: unsafe { mem::transmute(get_frame) },
    })
}

fn pointer_id(wparam: WPARAM) -> u32 {
    (wparam.0 as u32) & 0xffff
}

fn read_complete_frame(pointer_id: u32) -> Option<Vec<POINTER_TOUCH_INFO>> {
    let apis = APIS.get()?;
    let mut count = 0_u32;
    // A null buffer asks Windows for the number of contacts in this exact frame.
    // 空缓冲区仅查询同一帧中的触点数量。
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

fn combined_geometry(contacts: &[Contact]) -> Option<(f64, f64, f64)> {
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

fn update_frame(frame: &[POINTER_TOUCH_INFO]) {
    let contacts: Vec<Contact> = frame
        .iter()
        .filter(|info| info.pointerInfo.pointerType == PT_TOUCHPAD)
        .map(|info| Contact {
            id: info.pointerInfo.pointerId,
            x: info.pointerInfo.ptHimetricLocation.x,
            y: info.pointerInfo.ptHimetricLocation.y,
        })
        .collect();

    let mut demo = state().lock().expect("demo state lock");
    demo.frame_number += 1;
    demo.contacts = contacts;
    if let Some(info) = frame.first() {
        let mut device_rect = RECT::default();
        let mut display_rect = RECT::default();
        if unsafe {
            GetPointerDeviceRects(
                info.pointerInfo.sourceDevice,
                &mut device_rect,
                &mut display_rect,
            )
        }
        .is_ok()
        {
            demo.device_rect = device_rect;
        }
    }

    if let Some((center_x, center_y, span)) = combined_geometry(&demo.contacts) {
        demo.center_x = center_x;
        demo.center_y = center_y;
        demo.span = span;
        let current_span = demo.span.max(1.0);
        let initial = *demo.initial_span.get_or_insert(current_span);
        demo.scale = demo.span / initial;
    } else {
        demo.initial_span = None;
        demo.span = 0.0;
        demo.scale = 1.0;
    }
}

fn reset_frame() {
    let mut demo = state().lock().expect("demo state lock");
    demo.contacts.clear();
    demo.initial_span = None;
    demo.span = 0.0;
    demo.scale = 1.0;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

unsafe fn draw_text(hdc: windows::Win32::Graphics::Gdi::HDC, x: i32, y: i32, text: &str) {
    let text = wide(text);
    let _ = unsafe { TextOutW(hdc, x, y, &text) };
}

unsafe fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    let hdc = unsafe { BeginPaint(hwnd, &mut paint) };
    let mut client = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut client) };
    let white = unsafe { CreateSolidBrush(COLORREF(0x00ff_ffff)) };
    unsafe { FillRect(hdc, &client, white) };
    let _ = unsafe { DeleteObject(white.into()) };
    let _ = unsafe { SetBkMode(hdc, TRANSPARENT) };
    let _ = unsafe { SetTextColor(hdc, COLORREF(0x002b_2927)) };

    let demo = state().lock().expect("demo state lock");
    unsafe {
        draw_text(
            hdc,
            28,
            24,
            "Precision Touchpad · complete-frame pinch demo",
        )
    };
    unsafe { draw_text(hdc, 28, 52, &demo.registration) };
    unsafe {
        draw_text(
            hdc,
            28,
            82,
            &format!(
                "frame {:06}    contacts {}    span {:.1}    scale {:.4}",
                demo.frame_number,
                demo.contacts.len(),
                demo.span,
                demo.scale,
            ),
        )
    };
    unsafe {
        draw_text(
            hdc,
            28,
            108,
            &format!(
                "combined center: ({:.1}, {:.1})",
                demo.center_x, demo.center_y
            ),
        )
    };

    let area = RECT {
        left: 28,
        top: 150,
        right: client.right - 28,
        bottom: client.bottom - 28,
    };
    let surface = unsafe { CreateSolidBrush(COLORREF(0x00f8_f8f8)) };
    unsafe { FillRect(hdc, &area, surface) };
    let _ = unsafe { DeleteObject(surface.into()) };

    let width = (demo.device_rect.right - demo.device_rect.left).max(1);
    let height = (demo.device_rect.bottom - demo.device_rect.top).max(1);
    let draw_width = (area.right - area.left).max(1);
    let draw_height = (area.bottom - area.top).max(1);
    let positions: Vec<(i32, i32)> = demo
        .contacts
        .iter()
        .map(|contact| {
            (
                area.left + ((contact.x - demo.device_rect.left) * draw_width / width),
                area.top + ((contact.y - demo.device_rect.top) * draw_height / height),
            )
        })
        .collect();
    if positions.len() == 2 {
        let _ = unsafe { MoveToEx(hdc, positions[0].0, positions[0].1, None) };
        let _ = unsafe { LineTo(hdc, positions[1].0, positions[1].1) };
        let center_x = (positions[0].0 + positions[1].0) / 2;
        let center_y = (positions[0].1 + positions[1].1) / 2;
        let center_brush = unsafe { CreateSolidBrush(COLORREF(0x006f_6b67)) };
        let previous = unsafe { SelectObject(hdc, center_brush.into()) };
        let _ = unsafe { Ellipse(hdc, center_x - 6, center_y - 6, center_x + 6, center_y + 6) };
        let _ = unsafe { SelectObject(hdc, previous) };
        let _ = unsafe { DeleteObject(center_brush.into()) };
    }
    let blue = unsafe { CreateSolidBrush(COLORREF(0x00d6_5724)) };
    let old_brush = unsafe { SelectObject(hdc, blue.into()) };
    for (contact, (x, y)) in demo.contacts.iter().zip(positions) {
        let _ = unsafe { Ellipse(hdc, x - 15, y - 15, x + 15, y + 15) };
        unsafe {
            draw_text(
                hdc,
                x + 20,
                y - 8,
                &format!("#{} ({}, {})", contact.id, contact.x, contact.y),
            )
        };
    }
    let _ = unsafe { SelectObject(hdc, old_brush) };
    let _ = unsafe { DeleteObject(blue.into()) };
    let _ = unsafe { EndPaint(hwnd, &paint) };
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_POINTERDOWN | WM_POINTERUPDATE | WM_POINTERUP => {
            if let Some(frame) = read_complete_frame(pointer_id(wparam)) {
                update_frame(&frame);
            }
            if message == WM_POINTERUP {
                reset_frame();
            }
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_POINTERCAPTURECHANGED => {
            reset_frame();
            let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
            LRESULT(0)
        }
        WM_PAINT => {
            unsafe { paint(hwnd) };
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn main() -> windows::core::Result<()> {
    let apis = resolve_apis().map_err(|message| {
        windows::core::Error::new(windows::core::HRESULT(0x80004005_u32 as i32), message)
    })?;
    APIS.set(apis).expect("touchpad APIs initialized once");
    let instance = unsafe { GetModuleHandleW(None) }?;
    let class_name = w!("ResearchCanvasPrecisionTouchpadDemo");
    let cursor = unsafe { LoadCursorW(None, IDC_ARROW) }?;
    let background = HBRUSH((windows::Win32::Graphics::Gdi::COLOR_WINDOW.0 + 1) as *mut _);
    let class = WNDCLASSW {
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance.into(),
        hCursor: cursor,
        hbrBackground: background,
        lpszClassName: class_name,
        ..Default::default()
    };
    if unsafe { RegisterClassW(&class) } == 0 {
        return Err(windows::core::Error::from_win32());
    }
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Precision Touchpad Pinch Demo"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            980,
            680,
            None,
            None,
            Some(instance.into()),
            None,
        )
    }?;

    let registered = unsafe { (apis.register_window)(hwnd, true.into()) }.as_bool();
    let registration = if registered {
        "READY · cursor inside this window, then pinch with two fingers".to_string()
    } else {
        format!(
            "FAILED · RegisterTouchpadCapableWindow error {}",
            unsafe { GetLastError() }.0
        )
    };
    state().lock().expect("demo state lock").registration = registration;
    let _ = unsafe { ShowWindow(hwnd, SW_SHOW) };

    let mut message = MSG::default();
    while unsafe { GetMessageW(&mut message, None, 0, 0) }.as_bool() {
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{combined_geometry, Contact};

    #[test]
    fn combines_x_y_center_and_span_from_one_two_contact_frame() {
        let contacts = [
            Contact {
                id: 1,
                x: 10,
                y: 20,
            },
            Contact {
                id: 2,
                x: 40,
                y: 60,
            },
        ];
        assert_eq!(combined_geometry(&contacts), Some((25.0, 40.0, 50.0)));
        assert_eq!(combined_geometry(&contacts[..1]), None);
    }
}
