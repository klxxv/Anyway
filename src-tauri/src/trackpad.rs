//! Windows Precision Touchpad complete-frame bridge.
//! Windows 精确式触控板完整帧桥接。

#![cfg(windows)]

use serde::Serialize;
use std::{
    collections::HashMap,
    mem, ptr,
    sync::{
        atomic::{AtomicIsize, AtomicU64, Ordering},
        mpsc::{self, Sender},
        Mutex, OnceLock,
    },
    time::Instant,
};
use tauri::{AppHandle, Emitter, WebviewWindow};
use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings5;
use windows::{
    core::{w, Interface, BOOL, PCSTR},
    Win32::{
        Devices::HumanInterfaceDevice::{
            HidP_GetCaps, HidP_GetUsageValue, HidP_GetValueCaps, HidP_Input, HIDP_CAPS,
            HIDP_STATUS_SUCCESS, HIDP_VALUE_CAPS, PHIDP_PREPARSED_DATA,
        },
        Foundation::{GetLastError, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        System::LibraryLoader::{GetModuleHandleW, GetProcAddress},
        UI::{
            Input::Pointer::{GetPointerDeviceRects, POINTER_TOUCH_INFO},
            Input::{
                GetRawInputData, GetRawInputDeviceInfoW, RegisterRawInputDevices, HRAWINPUT,
                RAWINPUTDEVICE, RAWINPUTHEADER, RIDEV_DEVNOTIFY, RIDEV_INPUTSINK,
                RIDI_PREPARSEDDATA, RID_INPUT, RIM_TYPEHID,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId,
                SetWindowsHookExW, MSG, PT_TOUCHPAD, WH_GETMESSAGE, WM_INPUT,
                WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE,
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

/// Enables only WebView2's physical pinch input path while leaving browser
/// hotkey/page zoom disabled. WRY currently maps both settings to Tauri's one
/// `zoom_hotkeys_enabled` flag, so its default disables pinch before the DOM
/// can receive Chromium's synthetic Ctrl+Wheel events.
///
/// 仅开启 WebView2 的物理捏合输入路径，继续关闭浏览器快捷键/页面缩放。
/// WRY 当前把两项设置合并到一个开关，默认值会在 DOM 收到事件前丢弃捏合。
pub fn enable_webview2_pinch_input(window: &WebviewWindow) -> Result<(), String> {
    window
        .with_webview(|platform_webview| {
            let result = unsafe {
                platform_webview
                    .controller()
                    .CoreWebView2()
                    .and_then(|webview| webview.Settings())
                    .and_then(|settings| settings.cast::<ICoreWebView2Settings5>())
                    .and_then(|settings| settings.SetIsPinchZoomEnabled(true))
            };

            match result {
                Ok(()) => eprintln!(
                    "[trackpad/webview2] pinch input enabled; browser zoom remains disabled"
                ),
                Err(error) => {
                    eprintln!("[trackpad/webview2] could not enable pinch input: {error}")
                }
            }
        })
        .map_err(|error| format!("Could not access the WebView2 controller: {error}"))
}

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
    held_ms: u64,
    held: bool,
}

#[derive(Default)]
struct GestureState {
    active: bool,
    initial_center_x: f64,
    initial_center_y: f64,
    initial_span: f64,
    last_frame_id: u32,
    dwell_center_x: f64,
    dwell_center_y: f64,
    dwell_span: f64,
    dwell_started_at: Option<Instant>,
    held: bool,
}

struct RawTouchpadDescriptor {
    preparsed: Vec<u8>,
    report_id: u8,
    contact_links: Vec<u16>,
    max_x: f64,
    max_y: f64,
}

static EVENT_SENDER: OnceLock<Sender<TrackpadFrameEvent>> = OnceLock::new();
static GESTURE_STATE: OnceLock<Mutex<GestureState>> = OnceLock::new();
static SCALE_FACTOR_BITS: AtomicU64 = AtomicU64::new(1_f64.to_bits());
static ROOT_HWND: AtomicIsize = AtomicIsize::new(0);
static MESSAGE_HOOK: AtomicIsize = AtomicIsize::new(0);
static TOUCHPAD_APIS: OnceLock<TouchpadApis> = OnceLock::new();
static RAW_INPUT_FRAME_COUNT: AtomicU64 = AtomicU64::new(0);
static RAW_DESCRIPTORS: OnceLock<Mutex<HashMap<isize, RawTouchpadDescriptor>>> = OnceLock::new();

fn register_raw_touchpad(hwnd: HWND) -> Result<(), String> {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x0d,
        usUsage: 0x05,
        dwFlags: RIDEV_INPUTSINK | RIDEV_DEVNOTIFY,
        hwndTarget: hwnd,
    };
    unsafe { RegisterRawInputDevices(&[device], mem::size_of::<RAWINPUTDEVICE>() as u32) }
        .map_err(|error| format!("RegisterRawInputDevices failed: {error}"))?;
    #[cfg(debug_assertions)]
    eprintln!("[trackpad/raw] registered HID page=0x0d usage=0x05 sink hwnd={hwnd:?}");
    Ok(())
}

fn build_raw_touchpad_descriptor(
    device: windows::Win32::Foundation::HANDLE,
) -> Option<RawTouchpadDescriptor> {
    let mut preparsed_size = 0_u32;
    let _ = unsafe {
        GetRawInputDeviceInfoW(Some(device), RIDI_PREPARSEDDATA, None, &mut preparsed_size)
    };
    if preparsed_size == 0 || preparsed_size > 64 * 1024 {
        eprintln!("[trackpad/raw] no HID preparsed data for device={device:?}");
        return None;
    }
    let mut preparsed = vec![0_u8; preparsed_size as usize];
    let copied = unsafe {
        GetRawInputDeviceInfoW(
            Some(device),
            RIDI_PREPARSEDDATA,
            Some(preparsed.as_mut_ptr().cast()),
            &mut preparsed_size,
        )
    };
    if copied == u32::MAX {
        eprintln!("[trackpad/raw] HID preparsed data read failed for device={device:?}");
        return None;
    }
    let preparsed_data = PHIDP_PREPARSED_DATA(preparsed.as_mut_ptr() as isize);
    let mut caps = HIDP_CAPS::default();
    if unsafe { HidP_GetCaps(preparsed_data, &mut caps) } != HIDP_STATUS_SUCCESS {
        eprintln!("[trackpad/raw] HidP_GetCaps failed for device={device:?}");
        return None;
    }
    eprintln!(
        "[trackpad/raw] caps usage={:04x}:{:04x} report_bytes={} value_caps={} button_caps={} links={}",
        caps.UsagePage,
        caps.Usage,
        caps.InputReportByteLength,
        caps.NumberInputValueCaps,
        caps.NumberInputButtonCaps,
        caps.NumberLinkCollectionNodes
    );
    let mut value_caps = vec![HIDP_VALUE_CAPS::default(); caps.NumberInputValueCaps as usize];
    let mut value_cap_count = caps.NumberInputValueCaps;
    if unsafe {
        HidP_GetValueCaps(
            HidP_Input,
            value_caps.as_mut_ptr(),
            &mut value_cap_count,
            preparsed_data,
        )
    } != HIDP_STATUS_SUCCESS
    {
        eprintln!("[trackpad/raw] HidP_GetValueCaps failed for device={device:?}");
        return None;
    }
    let mut report_id = 0_u8;
    let mut contact_links = Vec::new();
    let mut max_x = 1_f64;
    let mut max_y = 1_f64;
    for cap in value_caps.into_iter().take(value_cap_count as usize) {
        let (usage_min, usage_max) = unsafe {
            if cap.IsRange {
                (cap.Anonymous.Range.UsageMin, cap.Anonymous.Range.UsageMax)
            } else {
                let usage = cap.Anonymous.NotRange.Usage;
                (usage, usage)
            }
        };
        eprintln!(
            "[trackpad/raw] value report={:02x} page={:04x} usage={:04x}-{:04x} link={} bits={} count={} logical={}..{}",
            cap.ReportID,
            cap.UsagePage,
            usage_min,
            usage_max,
            cap.LinkCollection,
            cap.BitSize,
            cap.ReportCount,
            cap.LogicalMin,
            cap.LogicalMax
        );
        report_id = report_id.max(cap.ReportID);
        if cap.UsagePage == 0x0d && usage_min <= 0x51 && usage_max >= 0x51 {
            contact_links.push(cap.LinkCollection);
        }
        if cap.UsagePage == 0x01 && usage_min <= 0x30 && usage_max >= 0x30 {
            max_x = max_x.max(f64::from(cap.LogicalMax.max(1)));
        }
        if cap.UsagePage == 0x01 && usage_min <= 0x31 && usage_max >= 0x31 {
            max_y = max_y.max(f64::from(cap.LogicalMax.max(1)));
        }
    }
    contact_links.sort_unstable();
    contact_links.dedup();
    if report_id == 0 || contact_links.is_empty() {
        eprintln!("[trackpad/raw] descriptor has no usable touch contacts");
        return None;
    }
    Some(RawTouchpadDescriptor {
        preparsed,
        report_id,
        contact_links,
        max_x,
        max_y,
    })
}

fn hid_usage_value(
    descriptor: &RawTouchpadDescriptor,
    report: &[u8],
    usage_page: u16,
    link: u16,
    usage: u16,
) -> Option<u32> {
    let preparsed = PHIDP_PREPARSED_DATA(descriptor.preparsed.as_ptr() as isize);
    let mut value = 0_u32;
    (unsafe {
        HidP_GetUsageValue(
            HidP_Input,
            usage_page,
            Some(link),
            usage,
            &mut value,
            preparsed,
            report,
        )
    } == HIDP_STATUS_SUCCESS)
        .then_some(value)
}

fn parse_raw_touchpad_report(
    device: windows::Win32::Foundation::HANDLE,
    report: &[u8],
) -> Option<(Vec<TrackpadContact>, (f64, f64), u32)> {
    let descriptors = RAW_DESCRIPTORS.get_or_init(|| Mutex::new(HashMap::new()));
    let mut descriptors = descriptors.lock().ok()?;
    let key = device.0 as isize;
    if !descriptors.contains_key(&key) {
        descriptors.insert(key, build_raw_touchpad_descriptor(device)?);
    }
    let descriptor = descriptors.get(&key)?;
    if report.first().copied()? != descriptor.report_id {
        return None;
    }
    let contact_count = hid_usage_value(descriptor, report, 0x0d, 0, 0x54)? as usize;
    let frame_id = hid_usage_value(descriptor, report, 0x0d, 0, 0x56)
        .unwrap_or_else(|| RAW_INPUT_FRAME_COUNT.load(Ordering::Relaxed) as u32);
    let contacts = descriptor
        .contact_links
        .iter()
        .take(contact_count.min(descriptor.contact_links.len()))
        .filter_map(|link| {
            Some(TrackpadContact {
                id: hid_usage_value(descriptor, report, 0x0d, *link, 0x51)?,
                x: hid_usage_value(descriptor, report, 0x01, *link, 0x30)? as i32,
                y: hid_usage_value(descriptor, report, 0x01, *link, 0x31)? as i32,
            })
        })
        .collect();
    Some((contacts, (descriptor.max_x, descriptor.max_y), frame_id))
}

fn inspect_raw_touchpad_input(handle: HRAWINPUT) {
    let header_size = mem::size_of::<RAWINPUTHEADER>() as u32;
    let mut byte_count = 0_u32;
    let queried = unsafe { GetRawInputData(handle, RID_INPUT, None, &mut byte_count, header_size) };
    if queried != 0 || byte_count < header_size + 8 || byte_count > 64 * 1024 {
        return;
    }
    let mut bytes = vec![0_u8; byte_count as usize];
    let copied = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            Some(bytes.as_mut_ptr().cast()),
            &mut byte_count,
            header_size,
        )
    };
    if copied == u32::MAX || copied < header_size + 8 {
        return;
    }
    let header = unsafe { ptr::read_unaligned(bytes.as_ptr().cast::<RAWINPUTHEADER>()) };
    if header.dwType != RIM_TYPEHID.0 {
        return;
    }
    let payload_offset = header_size as usize;
    let report_size = u32::from_le_bytes(
        bytes[payload_offset..payload_offset + 4]
            .try_into()
            .expect("four-byte raw HID size"),
    );
    let report_count = u32::from_le_bytes(
        bytes[payload_offset + 4..payload_offset + 8]
            .try_into()
            .expect("four-byte raw HID count"),
    );
    let payload = &bytes[payload_offset + 8..copied as usize];
    RAW_INPUT_FRAME_COUNT.fetch_add(1, Ordering::Relaxed);
    let report_size = report_size as usize;
    if report_size == 0 {
        return;
    }
    for report in payload
        .chunks_exact(report_size)
        .take(report_count as usize)
    {
        send_event(raw_frame_event(header.hDevice, report));
    }
}

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
    #[cfg(debug_assertions)]
    eprintln!("[trackpad/native] query pointer={pointer_id} contacts={count}");
    if count == 0 || count > 16 {
        return None;
    }
    let mut frame = vec![POINTER_TOUCH_INFO::default(); count as usize];
    if !unsafe { (apis.get_frame)(pointer_id, &mut count, frame.as_mut_ptr()) }.as_bool() {
        #[cfg(debug_assertions)]
        eprintln!(
            "[trackpad/native] frame read failed pointer={pointer_id} error={}",
            unsafe { GetLastError() }.0
        );
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

fn cursor_client_position() -> Option<(f64, f64)> {
    let hwnd = root_hwnd()?;
    let mut cursor = windows::Win32::Foundation::POINT::default();
    unsafe { GetCursorPos(&mut cursor) }.ok()?;
    let _ = unsafe { ScreenToClient(hwnd, &mut cursor) };
    let window_scale = f64::from_bits(SCALE_FACTOR_BITS.load(Ordering::Relaxed)).max(0.5);
    Some((
        f64::from(cursor.x) / window_scale,
        f64::from(cursor.y) / window_scale,
    ))
}

fn composed_frame_event(
    contacts: Vec<TrackpadContact>,
    surface: (f64, f64),
    frame_id: u32,
    cursor: (f64, f64),
) -> Option<TrackpadFrameEvent> {
    let (center_x, center_y, span) = combined_geometry(&contacts)?;
    let gesture = GESTURE_STATE.get_or_init(|| Mutex::new(GestureState::default()));
    let mut gesture = gesture.lock().ok()?;
    if gesture.active && gesture.last_frame_id == frame_id {
        return None;
    }
    let now = Instant::now();
    let phase = if gesture.active {
        let dwell_distance =
            (center_x - gesture.dwell_center_x).hypot(center_y - gesture.dwell_center_y);
        let dwell_threshold = surface.0.min(surface.1).max(1.0) * 0.012;
        if dwell_distance > dwell_threshold || (span - gesture.dwell_span).abs() > dwell_threshold {
            gesture.dwell_center_x = center_x;
            gesture.dwell_center_y = center_y;
            gesture.dwell_span = span;
            gesture.dwell_started_at = Some(now);
            gesture.held = false;
        }
        "update"
    } else {
        gesture.active = true;
        gesture.initial_center_x = center_x;
        gesture.initial_center_y = center_y;
        gesture.initial_span = span.max(1.0);
        gesture.dwell_center_x = center_x;
        gesture.dwell_center_y = center_y;
        gesture.dwell_span = span;
        gesture.dwell_started_at = Some(now);
        gesture.held = false;
        "start"
    };
    gesture.last_frame_id = frame_id;
    let held_ms = gesture
        .dwell_started_at
        .map(|started| now.saturating_duration_since(started).as_millis() as u64)
        .unwrap_or(0);
    // A half-second dwell keeps the gesture deliberate without making the pie menu feel delayed.
    // 500 ms 停留既能避免误触，也让甩饼呼出更及时。
    gesture.held = held_ms >= 500;
    let (pan_x, pan_y) = normalized_pan(
        (gesture.initial_center_x, gesture.initial_center_y),
        (center_x, center_y),
        surface,
    );

    Some(TrackpadFrameEvent {
        phase,
        frame_id,
        contacts,
        center_x,
        center_y,
        span,
        scale: span / gesture.initial_span,
        pan_x,
        pan_y,
        device_width: surface.0,
        device_height: surface.1,
        cursor_x: cursor.0,
        cursor_y: cursor.1,
        held_ms,
        held: gesture.held,
    })
}

fn raw_frame_event(
    device: windows::Win32::Foundation::HANDLE,
    report: &[u8],
) -> Option<TrackpadFrameEvent> {
    let hwnd = root_hwnd()?;
    if unsafe { GetForegroundWindow() } != hwnd {
        return end_event();
    }
    let (contacts, surface, frame_id) = parse_raw_touchpad_report(device, report)?;
    if contacts.len() != 2 {
        return end_event();
    }
    composed_frame_event(contacts, surface, frame_id, cursor_client_position()?)
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
    let pointer_info = frame.first()?.pointerInfo;
    let surface = device_size(&frame);
    let mut cursor = pointer_info.ptPixelLocation;
    let hwnd = root_hwnd()?;
    let _ = unsafe { ScreenToClient(hwnd, &mut cursor) };
    let window_scale = f64::from_bits(SCALE_FACTOR_BITS.load(Ordering::Relaxed)).max(0.5);
    composed_frame_event(
        contacts,
        surface,
        pointer_info.frameId,
        (
            f64::from(cursor.x) / window_scale,
            f64::from(cursor.y) / window_scale,
        ),
    )
}

fn end_event() -> Option<TrackpadFrameEvent> {
    let gesture = GESTURE_STATE.get_or_init(|| Mutex::new(GestureState::default()));
    let mut gesture = gesture.lock().ok()?;
    if !gesture.active {
        return None;
    }
    gesture.active = false;
    gesture.initial_span = 0.0;
    gesture.dwell_started_at = None;
    gesture.held = false;
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
        held_ms: 0,
        held: false,
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
                #[cfg(debug_assertions)]
                eprintln!(
                    "[trackpad/native] hook message=0x{:04x} hwnd={:?} pointer={}",
                    message.message,
                    message.hwnd,
                    pointer_id(message.wParam)
                );
                send_event(frame_event(pointer_id(message.wParam)));
            }
            WM_POINTERUP | WM_POINTERCAPTURECHANGED => {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[trackpad/native] hook end message=0x{:04x} hwnd={:?}",
                    message.message, message.hwnd
                );
                send_event(end_event());
            }
            WM_INPUT => {
                inspect_raw_touchpad_input(HRAWINPUT(message.lParam.0 as *mut _));
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
    #[cfg(debug_assertions)]
    eprintln!("[trackpad/native] Precision Touchpad ordinals resolved");
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

    register_raw_touchpad(hwnd)?;

    // Register the whole UI thread so WRY/WebView host children created on the
    // thread receive the same raw two-contact frames as the standalone Demo.
    // 注册整个 UI 线程，使 WRY/WebView 宿主子窗口与 Demo 一样接收原始双触点帧。
    if !unsafe { (apis.register_thread)(true.into()) }.as_bool() {
        return Err(format!(
            "RegisterTouchpadCapableThread failed with Windows error {}",
            unsafe { GetLastError() }.0
        ));
    }
    #[cfg(debug_assertions)]
    eprintln!("[trackpad/native] UI thread registered as touchpad-capable");
    if !unsafe { (apis.register_window)(hwnd, true.into()) }.as_bool() {
        return Err(format!(
            "RegisterTouchpadCapableWindow failed with Windows error {}",
            unsafe { GetLastError() }.0
        ));
    }
    #[cfg(debug_assertions)]
    eprintln!("[trackpad/native] root window registered hwnd={hwnd:?}");

    let thread_id = unsafe { GetWindowThreadProcessId(hwnd, None) };
    if thread_id == 0 {
        return Err("Could not resolve the Tauri UI thread".to_string());
    }
    let hook = unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(message_hook), None, thread_id) }
        .map_err(|error| format!("Could not observe the Tauri UI message queue: {error}"))?;
    MESSAGE_HOOK.store(hook.0 as isize, Ordering::Relaxed);
    #[cfg(debug_assertions)]
    eprintln!("[trackpad/native] WH_GETMESSAGE installed thread={thread_id} hook={hook:?}");
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
