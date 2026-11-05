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
    time::{Duration, Instant},
};

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmFlush() -> i32;
}

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

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
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
    current_scene.clear_laser();
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
    let mut laser_window = LaserWindow::create(instance)?;
    let mut switched = false;
    let mut magnifier = if selected_source.is_some() {
        Some(crate::windows_zoom::Magnifier::create(
            output,
            crate::zoom::Viewport::default(),
        )?)
    } else {
        None
    };
    'presenter: loop {
        let frame_started = Instant::now();
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
                selected.begin_presentation_change()?;
                let view = request.view.for_pixels(width, height);
                let result = match &mut magnifier {
                    Some(mag) => mag.set_view(view).map(|()| view),
                    None => crate::windows_zoom::Magnifier::create(output, view).map(|mag| {
                        magnifier = Some(mag);
                        view
                    }),
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
        for _ in 0..64 {
            if unsafe { PeekMessageW(&mut message, null_mut(), 0, 0, 1) } == 0 {
                break;
            }
            if message.message == WM_QUIT {
                break 'presenter;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }

        let (updated_scene, laser) = {
            let mut scene = scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned")?;
            scene.tick();
            let snapshot = (scene.revision() != current_scene.revision()).then(|| {
                let mut snapshot = scene.render_snapshot();
                snapshot.clear_laser();
                snapshot
            });
            (snapshot, scene.laser().cloned())
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
            if let Some(source) = selected_source {
                source
                    .lock()
                    .map_err(|_| "source lock poisoned")?
                    .begin_presentation_change()?;
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
        laser_window.update(laser.as_ref(), output)?;
        if let Some(mag) = &magnifier {
            mag.refresh()?;
        }
        if let Some(source) = selected_source {
            let mut selected = source.lock().map_err(|_| "source lock poisoned")?;
            if selected.output == *output && selected.needs_presentation(current_scene.revision()) {
                // A watermark is published only after the actual composition barrier.
                if unsafe { DwmFlush() } < 0 {
                    return Err("DwmFlush failed; cannot attest the captured scene".into());
                }
                selected.record_presentation(current_scene.revision())?;
            }
        }
        thread::sleep(Duration::from_micros(16_667).saturating_sub(frame_started.elapsed()));
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

struct LaserMotion {
    from: dd_scene::Point,
    to: dd_scene::Point,
    started: Instant,
}

impl LaserMotion {
    fn at(&self, now: Instant) -> dd_scene::Point {
        let t = (now.saturating_duration_since(self.started).as_secs_f32() * 60.0).min(1.0);
        dd_scene::Point {
            x: self.from.x + (self.to.x - self.from.x) * t,
            y: self.from.y + (self.to.y - self.from.y) * t,
        }
    }
}

struct LaserWindow {
    hwnd: Hwnd,
    dc: Hdc,
    bitmap: Hbitmap,
    old_bitmap: Hgdiobj,
    bits: *mut u8,
    style: Option<([u8; 4], [i32; 4])>,
    position: Option<(i32, i32)>,
    motion: Option<LaserMotion>,
    timings: Vec<f64>,
    trace: bool,
}

impl LaserWindow {
    fn create(instance: Hinstance) -> Result<Self, String> {
        // SAFETY: Uses the already registered click-through overlay class on its owner thread.
        unsafe {
            let title: Vec<u16> = "DrawDock Laser\0".encode_utf16().collect();
            let mut sprite = Self {
                hwnd: CreateWindowExW(
                    WS_EX_LAYERED
                        | WS_EX_TRANSPARENT
                        | WS_EX_NOACTIVATE
                        | WS_EX_TOOLWINDOW
                        | WS_EX_TOPMOST,
                    CLASS_NAME.as_ptr(),
                    title.as_ptr(),
                    WS_POPUP,
                    0,
                    0,
                    16,
                    16,
                    null_mut(),
                    null_mut(),
                    instance,
                    null_mut(),
                ),
                dc: null_mut(),
                bitmap: null_mut(),
                old_bitmap: null_mut(),
                bits: null_mut(),
                style: None,
                position: None,
                motion: None,
                timings: Vec::new(),
                trace: std::env::var("DRAWDOCK_PERF_TRACE").as_deref() == Ok("1"),
            };
            if sprite.hwnd.is_null() {
                return Err(last_error("create laser window"));
            }
            sprite.dc = CreateCompatibleDC(null_mut());
            if sprite.dc.is_null() {
                return Err(last_error("create laser DC"));
            }
            let mut info: BitmapInfo = zeroed();
            info.header.size = size_of::<BitmapInfoHeader>() as u32;
            info.header.width = 16;
            info.header.height = -16;
            info.header.planes = 1;
            info.header.bit_count = 32;
            let mut bits = null_mut();
            sprite.bitmap = CreateDIBSection(
                sprite.dc,
                &mut info,
                DIB_RGB_COLORS,
                &mut bits,
                null_mut(),
                0,
            );
            sprite.bits = bits.cast();
            if sprite.bitmap.is_null() || sprite.bits.is_null() {
                return Err(last_error("create laser DIB"));
            }
            sprite.old_bitmap = SelectObject(sprite.dc, sprite.bitmap);
            if sprite.old_bitmap.is_null() {
                return Err(last_error("select laser DIB"));
            }
            Ok(sprite)
        }
    }

    fn update(&mut self, laser: Option<&dd_scene::Laser>, output: &Output) -> Result<(), String> {
        let started = Instant::now();
        let Some(laser) = laser else {
            if self.position.take().is_some() {
                // SAFETY: Hides only our transient, nonactivating window.
                unsafe {
                    ShowWindow(self.hwnd, 0);
                }
                self.report_timings();
            }
            self.motion = None;
            return Ok(());
        };
        if self
            .motion
            .as_ref()
            .is_none_or(|motion| motion.to != laser.point)
        {
            let from = self
                .motion
                .as_ref()
                .map_or(laser.point, |motion| motion.at(started));
            self.motion = Some(LaserMotion {
                from,
                to: laser.point,
                started,
            });
        }
        let point = self
            .motion
            .as_ref()
            .ok_or("laser motion missing")?
            .at(started);
        let x = (point.x * output.width() as f32).round() as i32 - 8;
        let y = (point.y * output.height() as f32).round() as i32 - 8;
        let clip = [
            (-x).max(0),
            (-y).max(0),
            (output.width() - x).min(16),
            (output.height() - y).min(16),
        ];
        let style = (laser.rgba, clip);
        let destination = Point {
            x: output.left + x,
            y: output.top + y,
        };
        // SAFETY: The 16x16 DIB remains selected and alive; moves never raise the laser above Magnifier.
        unsafe {
            if self.style != Some(style) {
                let mut pixels = raster::laser_sprite(laser.rgba)?;
                for (index, pixel) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let (px, py) = ((index % 16) as i32, (index / 16) as i32);
                    if px < clip[0] || py < clip[1] || px >= clip[2] || py >= clip[3] {
                        pixel.fill(0);
                    }
                }
                std::ptr::copy_nonoverlapping(pixels.as_ptr(), self.bits, pixels.len());
                let blend = BlendFunction {
                    op: AC_SRC_OVER,
                    flags: 0,
                    source_constant_alpha: 255,
                    alpha_format: AC_SRC_ALPHA,
                };
                if UpdateLayeredWindow(
                    self.hwnd,
                    null_mut(),
                    &destination,
                    &Size { cx: 16, cy: 16 },
                    self.dc,
                    &Point { x: 0, y: 0 },
                    0,
                    &blend,
                    ULW_ALPHA,
                ) == 0
                {
                    return Err(last_error("present laser sprite"));
                }
                self.style = Some(style);
            } else if self.position == Some((x, y)) {
                return Ok(());
            }
            if SetWindowPos(
                self.hwnd,
                null_mut(),
                destination.x,
                destination.y,
                16,
                16,
                SWP_NOACTIVATE | SWP_SHOWWINDOW | 0x0004,
            ) == 0
            {
                return Err(last_error("move laser sprite"));
            }
        }
        self.position = Some((x, y));
        if self.trace && self.timings.len() < 4096 {
            self.timings.push(started.elapsed().as_secs_f64() * 1000.0);
        }
        Ok(())
    }

    fn report_timings(&mut self) {
        if self.timings.is_empty() {
            return;
        }
        self.timings.sort_by(f64::total_cmp);
        let index = (self.timings.len() * 95).div_ceil(100).saturating_sub(1);
        println!(
            "PERF laser samples={} p95_ms={:.3}",
            self.timings.len(),
            self.timings[index]
        );
        self.timings.clear();
    }
}

impl Drop for LaserWindow {
    fn drop(&mut self) {
        self.report_timings();
        // SAFETY: All resources belong to this presenter thread; restore the selected bitmap first.
        unsafe {
            if !self.old_bitmap.is_null() {
                SelectObject(self.dc, self.old_bitmap);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.dc.is_null() {
                DeleteDC(self.dc);
            }
            if !self.hwnd.is_null() {
                DestroyWindow(self.hwnd);
            }
        }
    }
}

fn last_error(operation: &str) -> String {
    let error = io::Error::last_os_error();
    format!("{operation} failed: {error}")
}

#[cfg(test)]
#[path = "../tests/unit/windows_overlay.rs"]
mod tests;

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
