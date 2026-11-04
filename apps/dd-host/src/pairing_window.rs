use qrcode::{Color, QrCode};
use std::{
    io,
    mem::zeroed,
    ptr::{null, null_mut},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
    Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BeginPaint, DIB_RGB_COLORS, DT_CENTER, DT_WORDBREAK, DrawTextW,
        EndPaint, FillRect, GetStockObject, PAINTSTRUCT, SRCCOPY, StretchDIBits, WHITE_BRUSH,
    },
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

pub struct PairingWindow {
    stopped: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl PairingWindow {
    pub fn open(invitation: &str, endpoint: &str, lifetime: Duration) -> Result<Self, String> {
        let (size, pixels) = qr_pixels(invitation)?;
        let state = WindowState {
            size,
            pixels,
            caption: wide(&format!(
                "Open DrawDock on the tablet and tap Scan QR.\nEndpoint: {endpoint}"
            )),
            footer: wide(
                "Approve only your tablet in the PC prompt after scanning.\n\
                 Expires in 5 minutes. Closing this window cancels pairing.\n\
                 USB requires ADB reverse; Wi-Fi requires a private IP.",
            ),
        };
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("drawdock-pairing-window".into())
            .spawn(move || {
                if let Err(error) = show(state, &stop, lifetime, &ready_tx) {
                    let _ = ready_tx.send(Err(error.clone()));
                    eprintln!("DrawDock pairing window failed: {error}");
                }
                stop.store(true, Ordering::Release);
            })
            .map_err(|error| format!("start QR window: {error}"))?;
        match ready_rx.recv().map_err(|error| error.to_string())? {
            Ok(()) => Ok(Self {
                stopped,
                thread: Some(thread),
            }),
            Err(error) => {
                let _ = thread.join();
                Err(error)
            }
        }
    }

    pub fn is_open(&self) -> bool {
        !self.stopped.load(Ordering::Acquire)
    }
}

impl Drop for PairingWindow {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if self
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err())
        {
            eprintln!("DrawDock pairing window thread panicked.");
        }
    }
}

pub fn approve(client: &str, peer: &str) -> Result<bool, String> {
    let message = wide(&format!(
        "Remember tablet \"{client}\" at {peer} and allow it to view PC screens, draw annotations and request pointer control?\n\n\
         This tablet will reconnect without asking again, including after a host restart, until you revoke it.\n\n\
         Choose Yes only if you just scanned this PC's QR invitation or used its trusted \
         USB invitation. Otherwise choose No."
    ));
    // SAFETY: Both strings are NUL-terminated and remain alive for the modal call.
    let result = unsafe {
        MessageBoxW(
            null_mut(),
            message.as_ptr(),
            wide("DrawDock - approve tablet").as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
        )
    };
    if result == 0 {
        Err(last_error("MessageBoxW(pairing approval)"))
    } else {
        Ok(result == IDYES)
    }
}

struct WindowState {
    size: i32,
    pixels: Vec<u32>,
    caption: Vec<u16>,
    footer: Vec<u16>,
}

fn qr_pixels(invitation: &str) -> Result<(i32, Vec<u32>), String> {
    let code = QrCode::new(invitation.as_bytes())
        .map_err(|error| format!("encode pairing QR: {error}"))?;
    let modules = code.width();
    let scale = 5;
    let border = 4;
    let size = (modules + border * 2) * scale;
    let mut pixels = vec![0x00ff_ffff; size * size];
    for y in 0..modules {
        for x in 0..modules {
            if code[(x, y)] == Color::Dark {
                for dy in 0..scale {
                    for dx in 0..scale {
                        pixels[((y + border) * scale + dy) * size + (x + border) * scale + dx] = 0;
                    }
                }
            }
        }
    }
    Ok((size as i32, pixels))
}

fn show(
    mut state: WindowState,
    stopped: &AtomicBool,
    lifetime: Duration,
    ready: &mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    let class_name = wide("DrawDockPairing");
    let title = wide("DrawDock - scan to pair");
    let style = WS_OVERLAPPEDWINDOW & !WS_MAXIMIZEBOX & !WS_THICKFRAME;
    let mut bounds = RECT {
        left: 0,
        top: 0,
        right: state.size.max(440) + 32,
        bottom: state.size + 180,
    };
    // SAFETY: This thread owns the window and its message queue. state remains at
    // a stable address until after DestroyWindow; all GDI resources are borrowed.
    unsafe {
        let instance = GetModuleHandleW(null());
        if instance.is_null() {
            return Err(last_error("GetModuleHandleW(pairing)"));
        }
        let mut class: WNDCLASSW = zeroed();
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.lpszClassName = class_name.as_ptr();
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hbrBackground = GetStockObject(WHITE_BRUSH);
        if RegisterClassW(&class) == 0 {
            return Err(last_error("RegisterClassW(pairing)"));
        }
        if AdjustWindowRect(&mut bounds, style, 0) == 0 {
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err(last_error("AdjustWindowRect(pairing)"));
        }
        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            title.as_ptr(),
            style,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            bounds.right - bounds.left,
            bounds.bottom - bounds.top,
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if hwnd.is_null() {
            UnregisterClassW(class_name.as_ptr(), instance);
            return Err(last_error("CreateWindowExW(pairing)"));
        }
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut state as *mut WindowState as isize);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = ready.send(Ok(()));
        let expires = Instant::now() + lifetime;
        let mut message: MSG = zeroed();
        while !stopped.load(Ordering::Acquire) && Instant::now() < expires {
            if PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if message.message == WM_QUIT {
                    break;
                }
                TranslateMessage(&message);
                DispatchMessageW(&message);
            } else {
                thread::sleep(Duration::from_millis(25));
            }
        }
        if IsWindow(hwnd) != 0 {
            DestroyWindow(hwnd);
        }
        UnregisterClassW(class_name.as_ptr(), instance);
    }
    Ok(())
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: GWLP_USERDATA is assigned before showing the window and points to
    // the owning thread's state until after window destruction.
    unsafe {
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
        if message == WM_PAINT && !state.is_null() {
            let state = &*state;
            let mut paint: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            let mut client: RECT = zeroed();
            GetClientRect(hwnd, &mut client);
            FillRect(dc, &client, GetStockObject(WHITE_BRUSH));
            let mut caption = RECT {
                left: 12,
                top: 12,
                right: client.right - 12,
                bottom: 76,
            };
            DrawTextW(
                dc,
                state.caption.as_ptr(),
                -1,
                &mut caption,
                DT_CENTER | DT_WORDBREAK,
            );
            let mut info: BITMAPINFO = zeroed();
            info.bmiHeader.biSize = size_of_val(&info.bmiHeader) as u32;
            info.bmiHeader.biWidth = state.size;
            info.bmiHeader.biHeight = -state.size;
            info.bmiHeader.biPlanes = 1;
            info.bmiHeader.biBitCount = 32;
            info.bmiHeader.biCompression = BI_RGB;
            let result = StretchDIBits(
                dc,
                (client.right - state.size) / 2,
                80,
                state.size,
                state.size,
                0,
                0,
                state.size,
                state.size,
                state.pixels.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
                SRCCOPY,
            );
            let mut footer = RECT {
                left: 12,
                top: state.size + 88,
                right: client.right - 12,
                bottom: client.bottom - 8,
            };
            DrawTextW(
                dc,
                state.footer.as_ptr(),
                -1,
                &mut footer,
                DT_CENTER | DT_WORDBREAK,
            );
            EndPaint(hwnd, &paint);
            if result <= 0 {
                eprintln!("{}", last_error("StretchDIBits(pairing QR)"));
                DestroyWindow(hwnd);
            }
            return 0;
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn last_error(operation: &str) -> String {
    format!("{operation}: {}", io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qr_bitmap_preserves_modules_and_four_module_quiet_zone() {
        let payload = format!(
            "drawdock://pair?v=1&host=127.0.0.1&port=47831&fingerprint={}&token={}",
            "0123456789abcdef".repeat(4),
            "abcdef0123456789".repeat(2),
        );
        let code = QrCode::new(payload.as_bytes()).unwrap();
        let (size, pixels) = qr_pixels(&payload).unwrap();
        let size = size as usize;
        for y in 0..size {
            for x in 0..size {
                let expected = if x < 20 || y < 20 || x >= size - 20 || y >= size - 20 {
                    0x00ff_ffff
                } else if code[(x / 5 - 4, y / 5 - 4)] == Color::Dark {
                    0
                } else {
                    0x00ff_ffff
                };
                assert_eq!(pixels[y * size + x], expected);
            }
        }
    }
}
