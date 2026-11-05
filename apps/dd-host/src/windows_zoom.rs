use crate::{windows_overlay::Output, zoom::Viewport};
use std::{io, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::{HWND, RECT},
    Graphics::Gdi::{InvalidateRect, UpdateWindow},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::*,
};

#[link(name = "Magnification")]
unsafe extern "system" {
    fn MagInitialize() -> i32;
    fn MagUninitialize() -> i32;
    fn MagSetWindowTransform(hwnd: HWND, matrix: *const [f32; 9]) -> i32;
    fn MagSetWindowSource(hwnd: HWND, source: RECT) -> i32;
    fn MagSetWindowFilterList(hwnd: HWND, mode: u32, count: i32, windows: *const HWND) -> i32;
}

// Owned by the presenter thread. Only this monitor-scoped window is magnified;
// no global accessibility transform, cursor setting or UIAccess is required.
pub struct Magnifier {
    host: HWND,
    control: HWND,
    output: Output,
    view: Viewport,
}

impl Magnifier {
    pub fn create(output: &Output, view: Viewport) -> Result<Self, String> {
        // SAFETY: All magnifier/window calls run on the owning presenter thread.
        unsafe {
            check(MagInitialize(), "MagInitialize")?;
            let mut mag = Self {
                host: null_mut(),
                control: null_mut(),
                output: output.clone(),
                view,
            };
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let title: Vec<u16> = "DrawDock Shared Zoom\0".encode_utf16().collect();
            let instance = GetModuleHandleW(null_mut());
            mag.host = CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                class.as_ptr(),
                title.as_ptr(),
                WS_POPUP,
                output.left,
                output.top,
                output.width(),
                output.height(),
                null_mut(),
                null_mut(),
                instance,
                null_mut(),
            );
            if mag.host.is_null() {
                return Err(error("CreateWindowExW(zoom host)"));
            }
            check(
                SetLayeredWindowAttributes(mag.host, 0, 255, LWA_ALPHA),
                "zoom opacity",
            )?;
            let class: Vec<u16> = "Magnifier\0".encode_utf16().collect();
            mag.control = CreateWindowExW(
                0,
                class.as_ptr(),
                null_mut(),
                WS_CHILD | WS_VISIBLE | 1,
                0,
                0,
                output.width(),
                output.height(),
                mag.host,
                null_mut(),
                instance,
                null_mut(),
            );
            if mag.control.is_null() {
                return Err(error("CreateWindowExW(Magnifier)"));
            }
            check(
                MagSetWindowFilterList(mag.control, 0, 1, &mag.host),
                "exclude zoom host",
            )?;
            mag.set_view(view)?;
            check(
                SetWindowPos(
                    mag.host,
                    HWND_TOPMOST,
                    output.left,
                    output.top,
                    output.width(),
                    output.height(),
                    SWP_NOACTIVATE | if view.scale > 1.0 { SWP_SHOWWINDOW } else { 0 },
                ),
                "show zoom host",
            )?;
            Ok(mag)
        }
    }

    pub fn set_view(&mut self, view: Viewport) -> Result<(), String> {
        if view.scale == 1.0 {
            self.view = view;
            // SAFETY: Keep the native control warm without magnifying the unzoomed desktop.
            unsafe {
                ShowWindow(self.host, SW_HIDE);
            }
            return Ok(());
        }
        let matrix = [view.scale, 0.0, 0.0, 0.0, view.scale, 0.0, 0.0, 0.0, 1.0];
        // SAFETY: control belongs to this thread and matrix is valid during the call.
        unsafe {
            check(
                MagSetWindowTransform(self.control, &matrix),
                "zoom transform",
            )?
        };
        self.view = view;
        self.refresh()?;
        // SAFETY: The control stays above the annotation and laser source windows without activation.
        unsafe {
            check(
                SetWindowPos(
                    self.host,
                    HWND_TOPMOST,
                    self.output.left,
                    self.output.top,
                    self.output.width(),
                    self.output.height(),
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ),
                "show zoom host",
            )
        }
    }

    pub fn refresh(&self) -> Result<(), String> {
        if self.view.scale == 1.0 {
            return Ok(());
        }
        let left = self.output.left + (self.view.left * self.output.width() as f32).round() as i32;
        let top = self.output.top + (self.view.top * self.output.height() as f32).round() as i32;
        let rect = RECT {
            left,
            top,
            right: left + (self.output.width() as f32 / self.view.scale).ceil() as i32,
            bottom: top + (self.output.height() as f32 / self.view.scale).ceil() as i32,
        };
        // SAFETY: The source rectangle is bounded to the selected output. The host
        // is filtered out, while the original annotation overlay remains included.
        unsafe {
            check(MagSetWindowSource(self.control, rect), "zoom source")?;
            check(
                InvalidateRect(self.control, null_mut(), 0),
                "refresh zoom control",
            )?;
            check(UpdateWindow(self.control), "paint zoom control")
        }
    }
}

impl Drop for Magnifier {
    fn drop(&mut self) {
        // SAFETY: Destroying the host destroys its child control on the owner thread.
        unsafe {
            if !self.host.is_null() && DestroyWindow(self.host) == 0 {
                eprintln!("{}", error("destroy zoom host"));
            }
            if MagUninitialize() == 0 {
                eprintln!("{}", error("MagUninitialize"));
            }
        }
    }
}

fn check(result: i32, operation: &str) -> Result<(), String> {
    if result == 0 {
        Err(error(operation))
    } else {
        Ok(())
    }
}

fn error(operation: &str) -> String {
    format!("{operation} failed: {}", io::Error::last_os_error())
}
