use dd_scene::Scene;
#[path = "raster.rs"]
mod raster;
use raster::render;
use std::{
    ffi::c_void,
    io,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    thread,
    time::Duration,
};

type Bool = i32;
type Handle = *mut c_void;
type Hwnd = Handle;
type Hmonitor = Handle;
type Hdc = Handle;
type Hbitmap = Handle;
type Hgdiobj = Handle;
type Hinstance = Handle;
type Wparam = usize;
type Lparam = isize;
type Lresult = isize;

const TRUE: Bool = 1;
const MONITORINFOF_PRIMARY: u32 = 1;
const WS_POPUP: u32 = 0x8000_0000;
const WS_EX_TOPMOST: u32 = 0x0000_0008;
const WS_EX_TRANSPARENT: u32 = 0x0000_0020;
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
const WS_EX_LAYERED: u32 = 0x0008_0000;
const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
const SW_SHOWNOACTIVATE: i32 = 4;
const SWP_NOACTIVATE: u32 = 0x0010;
const SWP_SHOWWINDOW: u32 = 0x0040;
const HWND_TOPMOST: Hwnd = -1isize as Hwnd;
const ULW_ALPHA: u32 = 2;
const AC_SRC_OVER: u8 = 0;
const AC_SRC_ALPHA: u8 = 1;
const BI_RGB: u32 = 0;
const DIB_RGB_COLORS: u32 = 0;
const WM_DESTROY: u32 = 0x0002;
const WM_QUIT: u32 = 0x0012;
const CTRL_C_EVENT: u32 = 0;
const CTRL_BREAK_EVENT: u32 = 1;
const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: Handle = -4isize as Handle;
const CLASS_NAME: &[u16] = &[
    68, 114, 97, 119, 68, 111, 99, 107, 79, 118, 101, 114, 108, 97, 121, 0,
];

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MonitorInfoEx {
    cb_size: u32,
    monitor: Rect,
    work: Rect,
    flags: u32,
    device: [u16; 32],
}

#[repr(C)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
struct Size {
    cx: i32,
    cy: i32,
}

#[repr(C)]
struct BlendFunction {
    op: u8,
    flags: u8,
    source_constant_alpha: u8,
    alpha_format: u8,
}

#[repr(C)]
struct BitmapInfoHeader {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bit_count: u16,
    compression: u32,
    size_image: u32,
    x_pels_per_meter: i32,
    y_pels_per_meter: i32,
    colors_used: u32,
    colors_important: u32,
}

#[repr(C)]
struct RgbQuad {
    blue: u8,
    green: u8,
    red: u8,
    reserved: u8,
}

#[repr(C)]
struct BitmapInfo {
    header: BitmapInfoHeader,
    colors: [RgbQuad; 1],
}

#[repr(C)]
struct WndClassExW {
    size: u32,
    style: u32,
    wnd_proc: Option<unsafe extern "system" fn(Hwnd, u32, Wparam, Lparam) -> Lresult>,
    class_extra: i32,
    window_extra: i32,
    instance: Hinstance,
    icon: Handle,
    cursor: Handle,
    background: Handle,
    menu_name: *const u16,
    class_name: *const u16,
    small_icon: Handle,
}

#[repr(C)]
struct Msg {
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
    time: u32,
    point: Point,
    private: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct Output {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
    pub primary: bool,
}

impl Output {
    pub fn enumerate() -> Result<Vec<Self>, String> {
        let mut outputs = Vec::new();
        let context = &mut outputs as *mut Vec<Self> as isize;
        // SAFETY: EnumDisplayMonitors invokes this callback synchronously and the context
        // remains a valid mutable Vec for the duration of the call.
        let result =
            unsafe { EnumDisplayMonitors(null_mut(), null(), Some(collect_monitor), context) };
        if result == 0 {
            return Err(last_error("EnumDisplayMonitors"));
        }
        Ok(outputs)
    }

    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

unsafe extern "system" fn collect_monitor(
    monitor: Hmonitor,
    _dc: Hdc,
    _rect: *mut Rect,
    context: isize,
) -> Bool {
    let outputs = unsafe { &mut *(context as *mut Vec<Output>) };
    // SAFETY: The zeroed structure is initialized with its required size before the API call.
    let mut info: MonitorInfoEx = unsafe { zeroed() };
    info.cb_size = size_of::<MonitorInfoEx>() as u32;
    // SAFETY: monitor is supplied by EnumDisplayMonitors and info is writable.
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return TRUE;
    }
    let len = info
        .device
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(info.device.len());
    outputs.push(Output {
        name: String::from_utf16_lossy(&info.device[..len]),
        left: info.monitor.left,
        top: info.monitor.top,
        right: info.monitor.right,
        bottom: info.monitor.bottom,
        primary: info.flags & MONITORINFOF_PRIMARY != 0,
    });
    TRUE
}

static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

pub fn request_stop() {
    STOP_REQUESTED.store(true, Ordering::Release);
}

unsafe extern "system" fn console_control(code: u32) -> Bool {
    if matches!(code, CTRL_C_EVENT | CTRL_BREAK_EVENT) {
        let thread_id = THREAD_ID.load(Ordering::Relaxed);
        if thread_id != 0 {
            // SAFETY: The UI thread created this message queue before registering the handler.
            unsafe { PostThreadMessageW(thread_id, WM_QUIT, 0, 0) };
        }
        return TRUE;
    }
    0
}

unsafe extern "system" fn window_proc(
    hwnd: Hwnd,
    message: u32,
    wparam: Wparam,
    lparam: Lparam,
) -> Lresult {
    if message == WM_DESTROY {
        return 0;
    }
    // SAFETY: Delegate all unhandled messages to the system window procedure.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub fn run(output: Output, scene: Arc<Mutex<Scene>>) -> Result<(), String> {
    run_selected(output, scene, None)
}

pub fn initialize_dpi() -> Result<(), String> {
    // SAFETY: Called at startup, before monitor enumeration or window creation.
    if unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) } == 0 {
        return Err(last_error(
            "SetProcessDpiAwarenessContext(PER_MONITOR_AWARE_V2)",
        ));
    }
    Ok(())
}

pub fn run_selected(
    mut output: Output,
    scene: Arc<Mutex<Scene>>,
    source: Option<crate::source::SharedSource>,
) -> Result<(), String> {
    loop {
        let width = output.width();
        let height = output.height();
        let pixel_count = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
            .ok_or("monitor dimensions overflow")?;
        let byte_count = pixel_count
            .checked_mul(4)
            .filter(|bytes| *bytes <= 512 * 1024 * 1024)
            .ok_or("overlay surface exceeds the 512 MiB safety limit")?;
        if width <= 0 || height <= 0 {
            return Err("selected monitor has invalid dimensions".into());
        }

        let mut pixels = vec![0u8; byte_count];

        // SAFETY: Win32 handles and buffers are checked and released on every normal return path.
        let switched = unsafe {
            show_layered_window(&output, width, height, &scene, &mut pixels, source.as_ref())
        }?;
        if !switched {
            return Ok(());
        }
        output = source
            .as_ref()
            .ok_or("source switch without source state")?
            .lock()
            .map_err(|_| "source lock is poisoned")?
            .output
            .clone();
    }
}

unsafe fn show_layered_window(
    output: &Output,
    width: i32,
    height: i32,
    scene: &Arc<Mutex<Scene>>,
    pixels: &mut [u8],
    selected_source: Option<&crate::source::SharedSource>,
) -> Result<bool, String> {
    let mut current_scene = scene
        .lock()
        .map_err(|_| "canonical scene lock is poisoned".to_owned())?
        .render_snapshot();
    render(&current_scene, width as usize, height as usize, pixels)?;
    // SAFETY: GetModuleHandleW(null) retrieves this process module handle.
    let instance = unsafe { GetModuleHandleW(null()) };
    if instance.is_null() {
        return Err(last_error("GetModuleHandleW"));
    }
    let class = WndClassExW {
        size: size_of::<WndClassExW>() as u32,
        style: 0,
        wnd_proc: Some(window_proc),
        class_extra: 0,
        window_extra: 0,
        instance,
        icon: null_mut(),
        cursor: null_mut(),
        background: null_mut(),
        menu_name: null(),
        class_name: CLASS_NAME.as_ptr(),
        small_icon: null_mut(),
    };
    // SAFETY: class contains valid pointers for the duration of registration.
    if unsafe { RegisterClassExW(&class) } == 0
        && io::Error::last_os_error().raw_os_error() != Some(1410)
    {
        return Err(last_error("RegisterClassExW"));
    }
    let title: Vec<u16> = "DrawDock Windows Overlay\0".encode_utf16().collect();
    // SAFETY: Window class and dimensions have been validated above.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            CLASS_NAME.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            output.left,
            output.top,
            width,
            height,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        )
    };
    if hwnd.is_null() {
        return Err(last_error("CreateWindowExW"));
    }

    // SAFETY: A memory DC with a top-down 32-bit DIB is the documented UpdateLayeredWindow
    // per-pixel-alpha path. The selected bitmap remains alive through the message loop.
    let dc = unsafe { CreateCompatibleDC(null_mut()) };
    if dc.is_null() {
        unsafe { DestroyWindow(hwnd) };
        return Err(last_error("CreateCompatibleDC"));
    }
    let mut bitmap_info = BitmapInfo {
        header: BitmapInfoHeader {
            size: size_of::<BitmapInfoHeader>() as u32,
            width,
            height: -height,
            planes: 1,
            bit_count: 32,
            compression: BI_RGB,
            size_image: pixels.len() as u32,
            x_pels_per_meter: 0,
            y_pels_per_meter: 0,
            colors_used: 0,
            colors_important: 0,
        },
        colors: [RgbQuad {
            blue: 0,
            green: 0,
            red: 0,
            reserved: 0,
        }],
    };
    let mut bitmap_bits = null_mut();
    // SAFETY: bitmap_info and bitmap_bits are valid out parameters; dimensions are bounded.
    let bitmap: Hbitmap = unsafe {
        CreateDIBSection(
            dc,
            &mut bitmap_info,
            DIB_RGB_COLORS,
            &mut bitmap_bits,
            null_mut(),
            0,
        )
    };
    if bitmap.is_null() || bitmap_bits.is_null() {
        unsafe {
            DeleteDC(dc);
            DestroyWindow(hwnd);
        }
        return Err(last_error("CreateDIBSection"));
    }
    // SAFETY: The DIB section allocation is at least pixels.len() bytes.
    unsafe {
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bitmap_bits.cast::<u8>(), pixels.len());
    }
    // SAFETY: bitmap is a GDI bitmap handle and dc is a compatible memory DC.
    let old_bitmap = unsafe { SelectObject(dc, bitmap) };
    if old_bitmap.is_null() {
        unsafe {
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(hwnd);
        }
        return Err(last_error("SelectObject"));
    }

    let destination = Point {
        x: output.left,
        y: output.top,
    };
    let source = Point { x: 0, y: 0 };
    let size = Size {
        cx: width,
        cy: height,
    };
    let blend = BlendFunction {
        op: AC_SRC_OVER,
        flags: 0,
        source_constant_alpha: 255,
        alpha_format: AC_SRC_ALPHA,
    };
    // SAFETY: All pointers refer to stack values valid for the duration of this call.
    if unsafe {
        UpdateLayeredWindow(
            hwnd,
            null_mut(),
            &destination,
            &size,
            dc,
            &source,
            0,
            &blend,
            ULW_ALPHA,
        )
    } == 0
    {
        unsafe {
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(hwnd);
        }
        return Err(last_error("UpdateLayeredWindow"));
    }
    // SAFETY: A layered window is shown without activating or taking keyboard focus.
    if unsafe {
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            output.left,
            output.top,
            width,
            height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW,
        )
    } == 0
    {
        unsafe {
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(hwnd);
        }
        return Err(last_error("SetWindowPos"));
    }
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
    }

    // Ensure the message queue exists before the control handler can post WM_QUIT.
    let mut message: Msg = unsafe { zeroed() };
    // SAFETY: PeekMessageW initializes this thread's message queue.
    unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, 0) };
    let thread_id = unsafe { GetCurrentThreadId() };
    THREAD_ID.store(thread_id, Ordering::Relaxed);
    // SAFETY: Handler has static lifetime and only posts a quit message to the owning thread.
    if !unsafe { GetConsoleWindow() }.is_null()
        && unsafe { SetConsoleCtrlHandler(Some(console_control), TRUE) } == 0
    {
        unsafe {
            SelectObject(dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(dc);
            DestroyWindow(hwnd);
        }
        return Err(last_error("SetConsoleCtrlHandler"));
    }
    println!("Overlay visible.");
    let mut switched = false;
    let mut magnifier: Option<crate::windows_zoom::Magnifier> = None;
    loop {
        if STOP_REQUESTED.load(Ordering::Acquire) {
            break;
        }
        if let Some(source) = selected_source {
            let mut selected = source.lock().map_err(|_| "source lock is poisoned")?;
            if selected.output.name != output.name {
                switched = true;
                break;
            }
            if let Some(request) = selected.pending_zoom.take() {
                let view = request.view.for_pixels(width, height);
                let result = if view.scale == 1.0 {
                    magnifier = None;
                    Ok(view)
                } else {
                    match &mut magnifier {
                        Some(mag) => mag.set_view(view).map(|()| view),
                        None => crate::windows_zoom::Magnifier::create(output, view).map(|mag| {
                            magnifier = Some(mag);
                            view
                        }),
                    }
                };
                if let Ok(view) = result.as_ref() {
                    selected.zoom = *view;
                } else {
                    // Fail closed to an unmagnified desktop, never retain a partial transform.
                    magnifier = None;
                    selected.zoom = crate::zoom::Viewport::default();
                }
                if request.reply.send(result).is_err() {
                    magnifier = None;
                    selected.zoom = crate::zoom::Viewport::default();
                    eprintln!("Zoom request expired before the Windows presenter replied.");
                }
            }
        }
        // SAFETY: message is a writable MSG and null hwnd receives all thread messages.
        if unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, 1) } != 0 {
            if message.message == WM_QUIT {
                break;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        } else {
            thread::sleep(Duration::from_millis(16));
        }

        let updated_scene = {
            let mut scene = scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned")?;
            scene.tick();
            (scene.revision() != current_scene.revision()).then(|| scene.render_snapshot())
        };
        if let Some(updated_scene) = updated_scene {
            current_scene = updated_scene;
            if let Err(error) = render(&current_scene, width as usize, height as usize, pixels) {
                unsafe {
                    SetConsoleCtrlHandler(Some(console_control), 0);
                    SelectObject(dc, old_bitmap);
                    DeleteObject(bitmap);
                    DeleteDC(dc);
                    DestroyWindow(hwnd);
                }
                return Err(error);
            }
            unsafe {
                std::ptr::copy_nonoverlapping(
                    pixels.as_ptr(),
                    bitmap_bits.cast::<u8>(),
                    pixels.len(),
                );
            }
            // SAFETY: The DIB and per-call stack data remain valid for this presentation.
            if unsafe {
                UpdateLayeredWindow(
                    hwnd,
                    null_mut(),
                    &destination,
                    &size,
                    dc,
                    &source,
                    0,
                    &blend,
                    ULW_ALPHA,
                )
            } == 0
            {
                unsafe {
                    SetConsoleCtrlHandler(Some(console_control), 0);
                    SelectObject(dc, old_bitmap);
                    DeleteObject(bitmap);
                    DeleteDC(dc);
                    DestroyWindow(hwnd);
                }
                return Err(last_error("UpdateLayeredWindow(scene update)"));
            }
        }
        if let Some(mag) = &magnifier {
            mag.refresh()?;
        }
    }
    drop(magnifier);
    THREAD_ID.store(0, Ordering::Relaxed);
    unsafe {
        SetConsoleCtrlHandler(Some(console_control), 0);
        SelectObject(dc, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(dc);
        DestroyWindow(hwnd);
    }
    Ok(switched)
}

fn last_error(operation: &str) -> String {
    let error = io::Error::last_os_error();
    format!("{operation} failed: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use dd_scene::{InkObject, InkStyle, Point as ScenePoint, Tool, Transform};

    #[test]
    fn rasterizes_scene_into_premultiplied_bgra_pixels() {
        let mut scene = Scene::default();
        scene
            .commit(InkObject {
                id: 1,
                tool: Tool::Freehand,
                style: InkStyle {
                    rgba: [255, 0, 0, 128],
                    width: 0.05,
                },
                points: vec![ScenePoint { x: 0.5, y: 0.5 }],
                text: None,
                image_data: None,
                transform: Transform::default(),
            })
            .unwrap();
        let mut pixels = vec![0; 20 * 20 * 4];
        render(&scene, 20, 20, &mut pixels).unwrap();
        let painted = &pixels[(9 * 20 + 9) * 4..(9 * 20 + 9) * 4 + 4];
        assert!(painted[2] > 0 && painted[2] == painted[3] && painted[0] == 0 && painted[1] == 0);
        let first_frame = pixels.clone();
        render(&scene, 20, 20, &mut pixels).unwrap();
        assert_eq!(pixels, first_frame);

        scene.clear().unwrap();
        render(&scene, 20, 20, &mut pixels).unwrap();
        assert!(pixels.iter().all(|pixel| *pixel == 0));
        scene.undo();
        render(&scene, 20, 20, &mut pixels).unwrap();
        assert_eq!(pixels, first_frame);
    }
}

#[link(name = "user32")]
unsafe extern "system" {
    fn EnumDisplayMonitors(
        dc: Hdc,
        clip: *const Rect,
        callback: Option<unsafe extern "system" fn(Hmonitor, Hdc, *mut Rect, isize) -> Bool>,
        context: isize,
    ) -> Bool;
    fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfoEx) -> Bool;
    fn SetProcessDpiAwarenessContext(context: Handle) -> Bool;
    fn GetModuleHandleW(module: *const u16) -> Hinstance;
    fn RegisterClassExW(class: *const WndClassExW) -> u16;
    fn CreateWindowExW(
        ex_style: u32,
        class_name: *const u16,
        window_name: *const u16,
        style: u32,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        parent: Hwnd,
        menu: Handle,
        instance: Hinstance,
        param: *mut c_void,
    ) -> Hwnd;
    fn SetWindowPos(
        hwnd: Hwnd,
        insert_after: Hwnd,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> Bool;
    fn ShowWindow(hwnd: Hwnd, command: i32) -> Bool;
    fn DestroyWindow(hwnd: Hwnd) -> Bool;
    fn DefWindowProcW(hwnd: Hwnd, message: u32, wparam: Wparam, lparam: Lparam) -> Lresult;
    fn PostThreadMessageW(thread_id: u32, message: u32, wparam: Wparam, lparam: Lparam) -> Bool;
    fn GetCurrentThreadId() -> u32;
    fn SetConsoleCtrlHandler(
        handler: Option<unsafe extern "system" fn(u32) -> Bool>,
        add: Bool,
    ) -> Bool;
    fn GetConsoleWindow() -> Hwnd;
    fn PeekMessageW(message: *mut Msg, hwnd: Hwnd, min: u32, max: u32, remove: u32) -> Bool;
    fn TranslateMessage(message: *const Msg) -> Bool;
    fn DispatchMessageW(message: *const Msg) -> Lresult;
    fn UpdateLayeredWindow(
        hwnd: Hwnd,
        destination_dc: Hdc,
        destination: *const Point,
        size: *const Size,
        source_dc: Hdc,
        source: *const Point,
        color_key: u32,
        blend: *const BlendFunction,
        flags: u32,
    ) -> Bool;
}

#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: Hdc) -> Hdc;
    fn DeleteDC(dc: Hdc) -> Bool;
    fn CreateDIBSection(
        dc: Hdc,
        info: *mut BitmapInfo,
        usage: u32,
        bits: *mut *mut c_void,
        section: Handle,
        offset: u32,
    ) -> Hbitmap;
    fn SelectObject(dc: Hdc, object: Hgdiobj) -> Hgdiobj;
    fn DeleteObject(object: Hgdiobj) -> Bool;
}
