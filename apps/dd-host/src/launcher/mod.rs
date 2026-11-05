mod worker;

use crate::host_environment::Network;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    mem::{size_of, zeroed},
    net::{IpAddr, Ipv4Addr, SocketAddr},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, SystemInformation::GetLocalTime},
    UI::{
        Controls::Dialogs::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*, Shell::*,
        WindowsAndMessaging::*,
    },
};
use worker::{Config, Device, Event, Output, Request, Worker};

const TITLE: &str = "DrawDock - Windows host";
const CLASS: &str = "DrawDockHostLauncher";
const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_COMMAND: u32 = WM_APP + 2;
const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
const WINDOW_WIDTH: i32 = 1100;
const WINDOW_HEIGHT: i32 = 900;
const NAVY: u32 = 0x003a_1b0b;
const SURFACE: u32 = 0x004a_2812;
const CYAN: u32 = 0x00ee_d322;
const TEXT: u32 = 0x00ff_f0e6;
const MUTED: u32 = 0x00cd_b7a1;
const MAX_LOGS: usize = 500;
const SS_ICON: u32 = 3;
const OUTPUT: i32 = 101;
const PORT: i32 = 102;
const ENCODER: i32 = 103;
const ADB: i32 = 104;
const DEVICE: i32 = 105;
const LIST: i32 = 106;
const ENABLE: i32 = 107;
const DISABLE: i32 = 108;
const PAIR: i32 = 109;
const STOP: i32 = 110;
const REFRESH: i32 = 111;
const BROWSE: i32 = 112;
const LOG: i32 = 113;
const PAUSE: i32 = 114;
const CLEAR: i32 = 115;
const STATUS: i32 = 116;
const HIDE: i32 = 117;
const QUIT: i32 = 118;
const TRAY_LISTENER: u32 = 2000;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
fn error(action: &str) -> String {
    format!("{action}: {}", std::io::Error::last_os_error())
}

#[derive(Debug, PartialEq)]
enum TrayEvent {
    Open,
    Menu,
}

fn tray_event(lparam: LPARAM) -> Option<TrayEvent> {
    // Version 4 packs the icon ID into the high word of lParam.
    match (lparam as u32) & 0xffff {
        WM_LBUTTONUP | WM_LBUTTONDBLCLK | NIN_SELECT | NIN_KEYSELECT => Some(TrayEvent::Open),
        WM_RBUTTONUP | WM_CONTEXTMENU => Some(TrayEvent::Menu),
        _ => None,
    }
}

fn show_window(window: HWND) {
    // SAFETY: Callers supply the live launcher window, without borrowing UI state.
    unsafe {
        ShowWindow(
            window,
            if IsIconic(window) != 0 {
                SW_RESTORE
            } else {
                SW_SHOW
            },
        );
        SetForegroundWindow(window);
    }
}

fn append_logs(logs: &mut VecDeque<String>, prefix: &str, message: &str) {
    let message = if message.contains("drawdock://")
        || message.contains("\"token\"")
        || message.contains("PRIVATE KEY")
    {
        "[Sensitive pairing/credential content omitted from logs]"
    } else {
        message
    };
    for line in message.lines() {
        let mut text: String = line.chars().take(4096).collect();
        if line.chars().count() > 4096 {
            text.push_str(" [truncated]");
        }
        logs.push_back(format!("{prefix} {text}"));
        while logs.len() > MAX_LOGS {
            logs.pop_front();
        }
    }
}

pub fn show_error(message: &str) {
    // SAFETY: NUL-terminated strings outlive the synchronous dialog.
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            wide(TITLE).as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

struct Ui {
    window: HWND,
    controls: BTreeMap<i32, HWND>,
    worker: Worker,
    outputs: Vec<Output>,
    networks: Vec<Network>,
    devices: Vec<Device>,
    endpoints: BTreeSet<SocketAddr>,
    active: Option<String>,
    host: bool,
    busy: bool,
    quitting: bool,
    paused: bool,
    logs: VecDeque<String>,
    brush: HBRUSH,
    font: HFONT,
    title_font: HFONT,
    icon: HICON,
    tray: bool,
    taskbar_created: u32,
    dpi: i32,
}

impl Drop for Ui {
    fn drop(&mut self) {
        // SAFETY: All owned windows have been destroyed before these GDI resources.
        unsafe {
            if self.tray {
                let data = self.tray_data();
                Shell_NotifyIconW(NIM_DELETE, &data);
            }
            for object in [self.brush, self.font, self.title_font] {
                if !object.is_null() {
                    DeleteObject(object);
                }
            }
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
        }
    }
}

impl Ui {
    fn dip(&self, value: i32) -> i32 {
        value * self.dpi / 96
    }
    fn control(&self, id: i32) -> HWND {
        self.controls[&id]
    }

    fn tray_data(&self) -> NOTIFYICONDATAW {
        // SAFETY: A zeroed NOTIFYICONDATAW is initialized with its size and required fields.
        let mut data: NOTIFYICONDATAW = unsafe { zeroed() };
        data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        data.hWnd = self.window;
        data.uID = 1;
        data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        data.uCallbackMessage = TRAY_MESSAGE;
        data.hIcon = self.icon;
        let tip = format!(
            "DrawDock: {} listener(s){}",
            self.endpoints.len(),
            if self.active.is_some() {
                ", tablet connected"
            } else {
                ""
            }
        );
        let tip = wide(&tip);
        data.szTip[..tip.len()].copy_from_slice(&tip);
        data
    }

    fn update_tray(&mut self) -> Result<(), String> {
        // SAFETY: The icon and window remain owned by this UI.
        unsafe {
            let mut data = self.tray_data();
            if Shell_NotifyIconW(NIM_MODIFY, &data) != 0 {
                self.tray = true;
                return Ok(());
            }
            self.tray = false;
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                ShowWindow(self.window, SW_SHOW);
                return Err(error("create or recover tray icon; launcher kept visible"));
            }
            self.tray = true;
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            if Shell_NotifyIconW(NIM_SETVERSION, &data) == 0 {
                ShowWindow(self.window, SW_SHOW);
                return Err(error("configure tray notifications; launcher kept visible"));
            }
        }
        Ok(())
    }

    fn add(&mut self, id: i32, class: &str, text: &str, style: u32) -> Result<(), String> {
        // SAFETY: Child controls and their handles are owned by this UI thread/window.
        unsafe {
            let handle = CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                10,
                10,
                self.window,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            );
            if handle.is_null() {
                return Err(error("create launcher control"));
            }
            SendMessageW(handle, WM_SETFONT, self.font as usize, 1);
            self.controls.insert(id, handle);
        }
        Ok(())
    }

    fn fonts(&mut self) -> Result<(), String> {
        // SAFETY: Fonts are owned and replaced only on the UI thread.
        unsafe {
            let normal = CreateFontW(
                -self.dip(14),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                wide("Segoe UI").as_ptr(),
            );
            let title = CreateFontW(
                -self.dip(25),
                0,
                0,
                0,
                600,
                0,
                0,
                0,
                1,
                0,
                0,
                5,
                0,
                wide("Segoe UI").as_ptr(),
            );
            if normal.is_null() || title.is_null() {
                if !normal.is_null() {
                    DeleteObject(normal);
                }
                if !title.is_null() {
                    DeleteObject(title);
                }
                return Err(error("create launcher fonts"));
            }
            for (&id, &handle) in &self.controls {
                SendMessageW(
                    handle,
                    WM_SETFONT,
                    if id == 1001 { title } else { normal } as usize,
                    1,
                );
            }
            if !self.font.is_null() {
                DeleteObject(self.font);
            }
            if !self.title_font.is_null() {
                DeleteObject(self.title_font);
            }
            self.font = normal;
            self.title_font = title;
        }
        Ok(())
    }

    fn create(&mut self) -> Result<(), String> {
        // SAFETY: The newly-created window is valid on this thread.
        self.dpi = unsafe { GetDpiForWindow(self.window) } as i32;
        self.fonts()?;
        self.add(1000, "STATIC", "", SS_ICON)?;
        self.add(1001, "STATIC", "DrawDock", 0)?;
        self.add(
            1002,
            "STATIC",
            "Windows host / authenticated USB and private LAN",
            0,
        )?;
        self.add(1003, "STATIC", "Monitor", 0)?;
        self.add(
            OUTPUT,
            "COMBOBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
        )?;
        self.add(1004, "STATIC", "Port", 0)?;
        self.add(
            PORT,
            "EDIT",
            "47831",
            WS_TABSTOP | WS_BORDER | ES_NUMBER as u32,
        )?;
        self.add(1005, "STATIC", "Video encoder", 0)?;
        self.add(
            ENCODER,
            "COMBOBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
        )?;
        self.add(1006, "STATIC", "ADB executable", 0)?;
        self.add(
            ADB,
            "EDIT",
            &worker::default_adb(),
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL as u32,
        )?;
        self.add(1007, "STATIC", "USB device (authorization required)", 0)?;
        self.add(
            DEVICE,
            "COMBOBOX",
            "",
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST as u32,
        )?;
        self.add(
            1008,
            "STATIC",
            "LISTENERS - select a row, then enable or disable",
            0,
        )?;
        self.add(
            LIST,
            "LISTBOX",
            "",
            WS_TABSTOP | WS_BORDER | WS_VSCROLL | LBS_NOTIFY as u32,
        )?;
        self.add(
            1009,
            "STATIC",
            "LIVE LOGS - bounded to the latest 500 lines",
            0,
        )?;
        self.add(
            LOG,
            "EDIT",
            "",
            WS_TABSTOP
                | WS_BORDER
                | WS_VSCROLL
                | WS_HSCROLL
                | ES_MULTILINE as u32
                | ES_READONLY as u32
                | ES_AUTOVSCROLL as u32
                | ES_AUTOHSCROLL as u32,
        )?;
        self.add(
            STATUS,
            "STATIC",
            "All listeners are off. Refreshing devices and eligible private interfaces...",
            0,
        )?;
        for (id, text) in [
            (ENABLE, "Enable selected"),
            (DISABLE, "Disable selected"),
            (PAIR, "Pair tablet / QR"),
            (STOP, "Stop all"),
            (REFRESH, "Refresh"),
            (BROWSE, "Browse ADB..."),
            (PAUSE, "Pause logs"),
            (CLEAR, "Clear view"),
            (HIDE, "Hide to tray"),
            (QUIT, "Quit"),
        ] {
            self.add(id, "BUTTON", text, WS_TABSTOP | BS_OWNERDRAW as u32)?;
        }
        self.add(
            1010,
            "STATIC",
            "Close hides to tray. Quit stops owned listeners. No firewall or startup changes.",
            0,
        )?;
        // SAFETY: Control messages use copied strings or borrowed icon/font handles.
        unsafe {
            SendMessageW(self.control(PORT), EM_SETLIMITTEXT, 5, 0);
            SendMessageW(self.control(LOG), EM_SETLIMITTEXT, 3_000_000, 0);
            SendMessageW(self.control(1000), STM_SETICON, self.icon as usize, 0);
            SendMessageW(self.control(1001), WM_SETFONT, self.title_font as usize, 1);
            for encoder in ["Software (x264)", "NVIDIA NVENC"] {
                SendMessageW(
                    self.control(ENCODER),
                    CB_ADDSTRING,
                    0,
                    wide(encoder).as_ptr() as isize,
                );
            }
            SendMessageW(self.control(ENCODER), CB_SETCURSEL, 0, 0);
            SendMessageW(
                self.window,
                WM_SETICON,
                ICON_BIG as usize,
                self.icon as isize,
            );
            SendMessageW(
                self.window,
                WM_SETICON,
                ICON_SMALL as usize,
                self.icon as isize,
            );
            self.update_tray()?;
            if SetTimer(self.window, 1, 100, None) == 0 {
                return Err(error("start launcher timer"));
            }
        }
        self.layout();
        self.update_controls();
        self.submit(Request::Discover(self.text(ADB)), true)?;
        self.log("INFO", "Ready. Enable only the listeners you intend to use; existing CLI hosts are never stopped automatically.");
        Ok(())
    }

    fn text(&self, id: i32) -> String {
        // SAFETY: Buffer is sized for the complete control text and terminator.
        unsafe {
            let handle = self.control(id);
            let mut text = vec![0u16; GetWindowTextLengthW(handle) as usize + 1];
            let count = GetWindowTextW(handle, text.as_mut_ptr(), text.len() as i32);
            String::from_utf16_lossy(&text[..count as usize])
        }
    }

    fn set(&self, id: i32, text: &str) {
        // SAFETY: SetWindowTextW copies this NUL-terminated string synchronously.
        unsafe {
            SetWindowTextW(self.control(id), wide(text).as_ptr());
        }
    }

    fn layout(&self) {
        // SAFETY: Valid children are moved within the owning window's client area.
        unsafe {
            let mut bounds: RECT = zeroed();
            GetClientRect(self.window, &mut bounds);
            let width = bounds.right * 96 / self.dpi;
            let height = bounds.bottom * 96 / self.dpi;
            let positions = [
                (1000, 24, 16, 48, 48),
                (1001, 86, 13, 340, 36),
                (1002, 86, 52, 600, 24),
                (HIDE, width - 252, 24, 132, 34),
                (QUIT, width - 104, 24, 80, 34),
                (1003, 24, 94, 300, 22),
                (OUTPUT, 24, 119, 330, 300),
                (1004, 374, 94, 72, 22),
                (PORT, 374, 119, 72, 32),
                (1005, 466, 94, 180, 22),
                (ENCODER, 466, 119, 195, 150),
                (REFRESH, width - 164, 119, 140, 32),
                (1006, 24, 165, 600, 22),
                (ADB, 24, 190, width - 212, 32),
                (BROWSE, width - 164, 190, 140, 32),
                (1007, 24, 234, 650, 22),
                (DEVICE, 24, 259, width - 48, 250),
                (1008, 24, 306, width - 48, 22),
                (LIST, 24, 334, width - 264, 146),
                (ENABLE, width - 216, 334, 192, 32),
                (DISABLE, width - 216, 372, 192, 32),
                (PAIR, width - 216, 410, 192, 32),
                (STOP, width - 216, 448, 192, 32),
                (STATUS, 24, 493, width - 48, 36),
                (1009, 24, 540, width - 400, 22),
                (PAUSE, width - 274, 533, 120, 32),
                (CLEAR, width - 144, 533, 120, 32),
                (LOG, 24, 575, width - 48, (height - 617).max(70)),
                (1010, 24, height - 30, width - 48, 22),
            ];
            for (id, x, y, w, h) in positions {
                MoveWindow(
                    self.control(id),
                    self.dip(x),
                    self.dip(y),
                    self.dip(w),
                    self.dip(h),
                    1,
                );
            }
        }
    }

    fn port(&self) -> Result<u16, String> {
        self.text(PORT)
            .parse::<u16>()
            .ok()
            .filter(|port| *port > 0)
            .ok_or("Port must be between 1 and 65535".into())
    }

    fn endpoint(&self, index: usize) -> Result<SocketAddr, String> {
        let address = if index == 0 {
            Ipv4Addr::LOCALHOST
        } else {
            self.networks
                .get(index - 1)
                .ok_or("Select an available listener")?
                .address
        };
        Ok(SocketAddr::new(IpAddr::V4(address), self.port()?))
    }

    fn selected(&self, id: i32, combo: bool) -> Result<usize, String> {
        // SAFETY: These list/combobox messages return a selected integer index.
        let index = unsafe {
            SendMessageW(
                self.control(id),
                if combo { CB_GETCURSEL } else { LB_GETCURSEL },
                0,
                0,
            )
        };
        if index < 0 {
            Err("Select an available device, monitor or listener first".into())
        } else {
            Ok(index as usize)
        }
    }

    fn fill_listeners(&self) {
        // SAFETY: Strings are copied into the listbox.
        unsafe {
            let selected = SendMessageW(self.control(LIST), LB_GETCURSEL, 0, 0).max(0);
            SendMessageW(self.control(LIST), LB_RESETCONTENT, 0, 0);
            for index in 0..=self.networks.len() {
                let name = if index == 0 {
                    "USB / ADB".into()
                } else {
                    self.networks[index - 1].name.clone()
                };
                let value = match self.endpoint(index) {
                    Ok(endpoint) => format!(
                        "[{}] {name}   {endpoint}",
                        if self.endpoints.contains(&endpoint) {
                            "ON"
                        } else {
                            "OFF"
                        }
                    ),
                    Err(_) => format!("[OFF] {name} - enter a valid port"),
                };
                SendMessageW(
                    self.control(LIST),
                    LB_ADDSTRING,
                    0,
                    wide(&value).as_ptr() as isize,
                );
            }
            SendMessageW(
                self.control(LIST),
                LB_SETCURSEL,
                selected.min(self.networks.len() as isize) as usize,
                0,
            );
        }
    }

    fn update_controls(&self) {
        // SAFETY: All controls are owned by the UI. Configuration locks while a host owns it.
        unsafe {
            for id in [OUTPUT, PORT, ENCODER, ADB, DEVICE, BROWSE] {
                EnableWindow(
                    self.control(id),
                    (!self.host && !self.busy && !self.quitting) as i32,
                );
            }
            for id in [ENABLE, DISABLE, PAIR, STOP, REFRESH] {
                EnableWindow(self.control(id), (!self.busy && !self.quitting) as i32);
            }
            EnableWindow(self.control(QUIT), (!self.quitting) as i32);
        }
    }

    fn submit(&mut self, command: Request, busy: bool) -> Result<(), String> {
        self.worker.send(command)?;
        self.busy |= busy;
        self.update_controls();
        Ok(())
    }

    fn confirm_stop(&self, message: &str) -> bool {
        if self.active.is_none() {
            return true;
        }
        // SAFETY: This dialog belongs to the launcher and defaults to No.
        unsafe {
            MessageBoxW(
                self.window,
                wide(message).as_ptr(),
                wide(TITLE).as_ptr(),
                MB_YESNO | MB_ICONWARNING | MB_DEFBUTTON2,
            ) == IDYES
        }
    }

    fn toggle(&mut self, index: usize, enable: bool) -> Result<(), String> {
        if self.busy || self.quitting {
            return Err("Wait for the current host operation".into());
        }
        let endpoint = self.endpoint(index)?;
        if enable {
            if self.endpoints.contains(&endpoint) {
                return Err("Selected listener is already enabled".into());
            }
            let output = self
                .outputs
                .get(self.selected(OUTPUT, true)?)
                .ok_or("Monitor is unavailable")?;
            let config = Config {
                output: output.name.clone(),
                encoder: if self.selected(ENCODER, true)? == 0 {
                    "software"
                } else {
                    "nvenc"
                }
                .into(),
            };
            let usb = if index == 0 {
                let device = self
                    .devices
                    .get(self.selected(DEVICE, true)?)
                    .ok_or("USB device is unavailable")?;
                if device.state != "device" {
                    return Err("Authorize/unlock the USB device in ADB, then Refresh".into());
                }
                Some((self.text(ADB), device.serial.clone()))
            } else {
                None
            };
            self.set(STATUS, &format!("Enabling {endpoint}..."));
            self.submit(
                Request::Enable {
                    endpoint,
                    config,
                    usb,
                },
                true,
            )
        } else {
            if !self.endpoints.contains(&endpoint) {
                return Err("Selected listener is not enabled".into());
            }
            if self.active.as_deref() == Some(&endpoint.to_string())
                && !self.confirm_stop("Disabling this listener disconnects the tablet and clears its current session drawings. Continue?")
            { return Ok(()); }
            self.set(STATUS, &format!("Disabling {endpoint}..."));
            self.submit(Request::Disable(endpoint), true)
        }
    }

    fn action(&mut self, id: i32) -> Result<(), String> {
        match id {
            HIDE => {
                self.update_tray()?;
                unsafe { ShowWindow(self.window, SW_HIDE); }
            }
            QUIT => {
                if self.confirm_stop("Quit DrawDock? This stops owned listeners, disconnects the tablet and clears its session drawings.") {
                    self.submit(Request::Quit, true)?;
                    self.quitting = true;
                    self.set(STATUS, "Stopping owned host and cleaning up USB forwarding...");
                    self.update_controls();
                }
            }
            REFRESH => self.submit(Request::Discover(self.text(ADB)), true)?,
            ENABLE | DISABLE => self.toggle(self.selected(LIST, false)?, id == ENABLE)?,
            PAIR => {
                let endpoint = self.endpoint(self.selected(LIST, false)?)?;
                self.submit(Request::Pair(endpoint), true)?;
            }
            STOP => {
                if self.confirm_stop("Stop all owned listeners? The tablet disconnects and current session drawings are cleared.") {
                    self.submit(Request::Stop, true)?;
                    self.set(STATUS, "Stopping all owned listeners...");
                }
            }
            CLEAR => { self.logs.clear(); self.set(LOG, ""); }
            PAUSE => {
                self.paused = !self.paused;
                self.set(PAUSE, if self.paused { "Resume logs" } else { "Pause logs" });
                if !self.paused { self.render_logs(); }
            }
            BROWSE => self.browse_adb()?,
            _ => {}
        }
        Ok(())
    }

    fn browse_adb(&mut self) -> Result<(), String> {
        // SAFETY: Dialog buffers and filter remain live for the modal call.
        unsafe {
            let mut file = vec![0u16; 32768];
            let filter = wide("Android Debug Bridge\0adb.exe\0Executables\0*.exe\0");
            let mut dialog: OPENFILENAMEW = zeroed();
            dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
            dialog.hwndOwner = self.window;
            dialog.lpstrFile = file.as_mut_ptr();
            dialog.nMaxFile = file.len() as u32;
            dialog.lpstrFilter = filter.as_ptr();
            dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
            if GetOpenFileNameW(&mut dialog) != 0 {
                let end = file
                    .iter()
                    .position(|ch| *ch == 0)
                    .ok_or("Invalid executable path")?;
                self.set(ADB, &String::from_utf16_lossy(&file[..end]));
            } else {
                let code = CommDlgExtendedError();
                if code != 0 {
                    return Err(format!("ADB file picker failed: {code}"));
                }
            }
        }
        Ok(())
    }

    fn log(&mut self, level: &str, message: &str) {
        // SAFETY: GetLocalTime fills the supplied SYSTEMTIME.
        let mut now: SYSTEMTIME = unsafe { zeroed() };
        unsafe {
            GetLocalTime(&mut now);
        }
        append_logs(
            &mut self.logs,
            &format!(
                "{:02}:{:02}:{:02} [{level}]",
                now.wHour, now.wMinute, now.wSecond
            ),
            message,
        );
        if !self.paused {
            self.render_logs();
        }
    }

    fn render_logs(&self) {
        self.set(
            LOG,
            &self
                .logs
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join("\r\n"),
        );
        // SAFETY: Put the edit caret at the end and scroll to it only in live mode.
        unsafe {
            let end = GetWindowTextLengthW(self.control(LOG));
            SendMessageW(self.control(LOG), EM_SETSEL, end as usize, end as isize);
            SendMessageW(self.control(LOG), EM_SCROLLCARET, 0, 0);
        }
    }

    fn poll(&mut self) -> Result<(), String> {
        let mut changed = false;
        for _ in 0..100 {
            let event = match self.worker.events.try_recv() {
                Ok(event) => {
                    changed = true;
                    event
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                Err(_) if self.quitting => {
                    unsafe {
                        DestroyWindow(self.window);
                    }
                    return Ok(());
                }
                Err(_) => return Err(
                    "Launcher worker stopped unexpectedly; quit to release its owned process job"
                        .into(),
                ),
            };
            match event {
                Event::Log(level, text) => {
                    self.log(level, &text);
                    if level == "ERROR" {
                        self.set(STATUS, &format!("Error: {text}"));
                    }
                }
                Event::Complete => self.busy = false,
                Event::Host(running) => {
                    self.host = running;
                    if !running {
                        self.endpoints.clear();
                        self.active = None;
                        self.set(STATUS, "Host stopped. All listeners are off.");
                        self.fill_listeners();
                    }
                }
                Event::State(value) => {
                    if let Some(items) = value["endpoints"].as_array() {
                        let endpoints: BTreeSet<SocketAddr> = items
                            .iter()
                            .map(|value| {
                                value
                                    .as_str()
                                    .ok_or("Invalid listener status")?
                                    .parse::<SocketAddr>()
                                    .map_err(|_| "Invalid listener address")
                            })
                            .collect::<Result<_, _>>()?;
                        let changes: Vec<_> = endpoints
                            .difference(&self.endpoints)
                            .map(|endpoint| format!("Enabled TLS listener {endpoint}"))
                            .chain(
                                self.endpoints
                                    .difference(&endpoints)
                                    .map(|endpoint| format!("Disabled TLS listener {endpoint}")),
                            )
                            .collect();
                        for change in changes {
                            self.log("INFO", &change);
                        }
                        self.endpoints = endpoints;
                        self.fill_listeners();
                    }
                    if value["event"] == "session" {
                        self.active = if value["active"] == true {
                            Some(
                                value["endpoint"]
                                    .as_str()
                                    .ok_or("Missing active endpoint")?
                                    .into(),
                            )
                        } else {
                            None
                        };
                    }
                    if value["request_id"].is_u64() || value["event"] == "error" {
                        self.busy = false;
                    }
                    if value["event"] != "error" {
                        let text = if let Some(endpoint) = &self.active {
                            format!(
                                "Tablet connected on {endpoint}. One active annotation session."
                            )
                        } else {
                            format!(
                                "{} listener(s) enabled. {}",
                                self.endpoints.len(),
                                if self.endpoints.is_empty() {
                                    "Host idle; Stop all releases configuration and trust ownership."
                                } else {
                                    "Remembered tablets can connect; use Pair tablet for a new endpoint/device."
                                }
                            )
                        };
                        self.set(STATUS, &text);
                    }
                }
                Event::Discovery {
                    outputs,
                    networks,
                    devices,
                } => {
                    let old_output = self.text(OUTPUT);
                    let old_device = self.text(DEVICE);
                    self.outputs = outputs;
                    self.networks = networks;
                    self.networks.sort_by_key(|item| item.address);
                    self.networks.dedup_by_key(|item| item.address);
                    self.devices = devices;
                    // SAFETY: Combo boxes copy each label; selections are restored by label.
                    unsafe {
                        for (id, labels, old) in [
                            (
                                OUTPUT,
                                self.outputs
                                    .iter()
                                    .map(|item| {
                                        format!(
                                            "{} ({}x{})",
                                            item.name,
                                            item.right - item.left,
                                            item.bottom - item.top
                                        )
                                    })
                                    .collect::<Vec<_>>(),
                                old_output,
                            ),
                            (
                                DEVICE,
                                self.devices
                                    .iter()
                                    .map(|item| format!("{} [{}]", item.serial, item.state))
                                    .collect(),
                                old_device,
                            ),
                        ] {
                            SendMessageW(self.control(id), CB_RESETCONTENT, 0, 0);
                            let selected =
                                labels.iter().position(|label| *label == old).unwrap_or(0);
                            for label in &labels {
                                SendMessageW(
                                    self.control(id),
                                    CB_ADDSTRING,
                                    0,
                                    wide(label).as_ptr() as isize,
                                );
                            }
                            SendMessageW(self.control(id), CB_SETCURSEL, selected, 0);
                        }
                    }
                    self.fill_listeners();
                    self.log("INFO", &format!("Found {} monitor(s), {} USB device(s), {} eligible private interface(s).",
                        self.outputs.len(),self.devices.len(),self.networks.len()));
                    if !self.host {
                        self.set(
                            STATUS,
                            "All listeners are off. Select a listener and choose Enable selected.",
                        );
                    }
                }
                Event::Exited => {
                    unsafe {
                        DestroyWindow(self.window);
                    }
                    return Ok(());
                }
            }
        }
        if changed {
            self.update_controls();
            self.update_tray()?;
        }
        Ok(())
    }

    fn show(&self) {
        show_window(self.window);
    }

    fn tray_menu(&self) -> Result<HMENU, String> {
        // SAFETY: The caller owns the temporary menu and destroys it after selection.
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return Err(error("create tray menu"));
            }
            AppendMenuW(
                menu,
                MF_STRING,
                HIDE as usize,
                wide("Open DrawDock / logs").as_ptr(),
            );
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            for index in 0..=self.networks.len() {
                let endpoint = match self.endpoint(index) {
                    Ok(endpoint) => endpoint,
                    Err(_) => {
                        AppendMenuW(
                            menu,
                            MF_STRING | MF_GRAYED,
                            0,
                            wide("Set a valid port in the launcher to enable listeners").as_ptr(),
                        );
                        break;
                    }
                };
                let label = if index == 0 {
                    "USB / ADB".into()
                } else {
                    format!("{} ({})", self.networks[index - 1].name, endpoint.ip())
                };
                AppendMenuW(
                    menu,
                    MF_STRING
                        | if self.endpoints.contains(&endpoint) {
                            MF_CHECKED
                        } else {
                            0
                        }
                        | if self.busy { MF_GRAYED } else { 0 },
                    TRAY_LISTENER as usize + index,
                    wide(&label).as_ptr(),
                );
            }
            AppendMenuW(menu, MF_SEPARATOR, 0, null());
            AppendMenuW(
                menu,
                MF_STRING,
                STOP as usize,
                wide("Stop all listeners").as_ptr(),
            );
            AppendMenuW(
                menu,
                MF_STRING,
                QUIT as usize,
                wide("Quit DrawDock").as_ptr(),
            );
            Ok(menu)
        }
    }

    fn tray_action(&mut self, selected: u32) -> Result<(), String> {
        if selected == HIDE as u32 {
            self.show();
        } else if selected >= TRAY_LISTENER
            && selected <= TRAY_LISTENER + self.networks.len() as u32
        {
            let index = (selected - TRAY_LISTENER) as usize;
            self.toggle(index, !self.endpoints.contains(&self.endpoint(index)?))?;
        } else if selected != 0 {
            self.action(selected as i32)?;
        }
        Ok(())
    }
}

fn show_tray_menu(window: HWND, menu: HMENU) {
    // SAFETY: No RefCell borrow is held while the native menu pumps window messages.
    unsafe {
        let mut point: POINT = zeroed();
        GetCursorPos(&mut point);
        SetForegroundWindow(window);
        let selected = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            window,
            null(),
        );
        DestroyMenu(menu);
        PostMessageW(window, WM_NULL, 0, 0);
        if selected != 0 {
            SendMessageW(window, TRAY_COMMAND, selected as usize, 0);
        }
    }
}

unsafe extern "system" fn window_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    // SAFETY: The pointer is installed at WM_NCCREATE and remains valid through message-loop exit.
    unsafe {
        if message == WM_DESTROY {
            KillTimer(window, 1);
            SetWindowLongPtrW(window, GWLP_USERDATA, 0);
            PostQuitMessage(0);
            return 0;
        }
        if message == WM_CLOSE {
            if PostMessageW(window, WM_COMMAND, HIDE as usize, 0) == 0 {
                show_error(&error("queue hide-to-tray; launcher kept visible"));
            }
            return 0;
        }
        if message == TRAY_MESSAGE && tray_event(lparam) == Some(TrayEvent::Open) {
            show_window(window);
            return 0;
        }
        // Control painting reenters during updates; do not borrow mutable UI state here.
        if matches!(
            message,
            WM_CTLCOLORSTATIC | WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX
        ) {
            let dc = wparam as HDC;
            SetTextColor(
                dc,
                if IsWindowEnabled(lparam as HWND) != 0 {
                    TEXT
                } else {
                    MUTED
                },
            );
            SetBkColor(dc, NAVY);
            return GetClassLongPtrW(window, GCLP_HBRBACKGROUND) as isize;
        }
        if message == WM_DRAWITEM {
            let item = &*(lparam as *const DRAWITEMSTRUCT);
            SetDCBrushColor(
                item.hDC,
                if item.itemState & ODS_SELECTED != 0 {
                    NAVY
                } else {
                    SURFACE
                },
            );
            FillRect(item.hDC, &item.rcItem, GetStockObject(DC_BRUSH));
            SetBkMode(item.hDC, TRANSPARENT as i32);
            SetTextColor(
                item.hDC,
                if item.itemState & ODS_DISABLED != 0 {
                    MUTED
                } else if item.CtlID == ENABLE as u32 {
                    CYAN
                } else {
                    TEXT
                },
            );
            let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
            if !font.is_null() {
                SelectObject(item.hDC, font);
            }
            let mut label = vec![0u16; GetWindowTextLengthW(item.hwndItem) as usize + 1];
            GetWindowTextW(item.hwndItem, label.as_mut_ptr(), label.len() as i32);
            let mut rect = item.rcItem;
            DrawTextW(
                item.hDC,
                label.as_ptr(),
                -1,
                &mut rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE,
            );
            if item.itemState & ODS_FOCUS != 0 {
                DrawFocusRect(item.hDC, &item.rcItem);
            }
            return 1;
        }
        if message == WM_NCCREATE {
            let create = &*(lparam as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *const RefCell<Ui>;
        if pointer.is_null() {
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let Ok(mut state) = (*pointer).try_borrow_mut() else {
            return DefWindowProcW(window, message, wparam, lparam);
        };
        // Default processing can synchronously send layout and close messages.
        if !matches!(
            message,
            WM_CREATE
                | WM_SIZE
                | WM_DPICHANGED
                | WM_GETMINMAXINFO
                | WM_COMMAND
                | WM_TIMER
                | TRAY_MESSAGE
                | TRAY_COMMAND
        ) && message != state.taskbar_created
        {
            drop(state);
            return DefWindowProcW(window, message, wparam, lparam);
        }
        let mut popup = None;
        let result: Result<LRESULT, String> = (|| match message {
            WM_CREATE => {
                state.window = window;
                state.create()?;
                Ok(0)
            }
            WM_SIZE => {
                if !state.controls.is_empty() {
                    state.layout();
                }
                Ok(0)
            }
            WM_DPICHANGED => {
                state.dpi = (wparam & 0xffff) as i32;
                state.fonts()?;
                let rect = &*(lparam as *const RECT);
                SetWindowPos(
                    window,
                    null_mut(),
                    rect.left,
                    rect.top,
                    rect.right - rect.left,
                    rect.bottom - rect.top,
                    SWP_NOACTIVATE | SWP_NOZORDER,
                );
                state.layout();
                Ok(0)
            }
            WM_GETMINMAXINFO => {
                let limits = &mut *(lparam as *mut MINMAXINFO);
                limits.ptMinTrackSize = POINT {
                    x: state.dip(WINDOW_WIDTH),
                    y: state.dip(WINDOW_HEIGHT),
                };
                limits.ptMaxTrackSize = limits.ptMinTrackSize;
                Ok(0)
            }
            WM_COMMAND => {
                let id = (wparam & 0xffff) as i32;
                let notification = (wparam >> 16) as u16;
                if notification == BN_CLICKED as u16 && (ENABLE..=QUIT).contains(&id) {
                    state.action(id)?;
                }
                if id == PORT && notification == EN_CHANGE as u16 {
                    state.fill_listeners();
                }
                Ok(0)
            }
            WM_TIMER => {
                state.poll()?;
                Ok(0)
            }
            TRAY_MESSAGE => {
                if tray_event(lparam) == Some(TrayEvent::Menu) {
                    popup = Some(state.tray_menu()?);
                }
                Ok(0)
            }
            TRAY_COMMAND => {
                state.tray_action(wparam as u32)?;
                Ok(0)
            }
            _ if message == state.taskbar_created => {
                state.update_tray()?;
                Ok(0)
            }
            _ => Ok(DefWindowProcW(window, message, wparam, lparam)),
        })();
        if let Some(menu) = popup {
            drop(state);
            show_tray_menu(window, menu);
            return 0;
        }
        match result {
            Ok(value) => value,
            Err(text) => {
                if message != WM_CREATE && state.controls.contains_key(&LOG) {
                    state.log("ERROR", &text);
                    state.set(STATUS, &format!("Error: {text}"));
                    state.busy = false;
                    state.update_controls();
                }
                MessageBoxW(
                    window,
                    wide(&text).as_ptr(),
                    wide(TITLE).as_ptr(),
                    MB_OK | MB_ICONERROR,
                );
                if message == WM_CREATE { -1 } else { 0 }
            }
        }
    }
}

fn icon() -> Result<HICON, String> {
    let bytes = include_bytes!("..\\..\\assets\\drawdock.ico");
    let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
    for index in 0..count {
        let entry = 6 + 16 * index;
        if bytes[entry] != 48 {
            continue;
        }
        let size = u32::from_le_bytes(
            bytes[entry + 8..entry + 12]
                .try_into()
                .map_err(|_| "Invalid icon size")?,
        ) as usize;
        let offset = u32::from_le_bytes(
            bytes[entry + 12..entry + 16]
                .try_into()
                .map_err(|_| "Invalid icon offset")?,
        ) as usize;
        let image = bytes
            .get(offset..offset + size)
            .ok_or("Invalid embedded icon bounds")?;
        // SAFETY: The embedded ICO image is complete and bounds-checked.
        let handle = unsafe {
            CreateIconFromResourceEx(image.as_ptr(), image.len() as u32, 1, 0x00030000, 48, 48, 0)
        };
        if handle.is_null() {
            return Err(error("load embedded DrawDock icon"));
        }
        return Ok(handle);
    }
    Err("Embedded DrawDock icon has no 48px image".into())
}

pub fn run() -> Result<(), String> {
    // SAFETY: Initialize per-process DPI before creating the first window.
    unsafe {
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) == 0 {
            return Err(error("initialize launcher DPI"));
        }
        let app_id =
            SetCurrentProcessExplicitAppUserModelID(wide("DrawDock.Host.Launcher").as_ptr());
        if app_id < 0 {
            return Err(format!("set launcher app identity: HRESULT {app_id:#x}"));
        }
        let existing = FindWindowW(wide(CLASS).as_ptr(), null());
        if !existing.is_null() {
            show_window(existing);
            return Ok(());
        }
        let worker = Worker::start()?;
        let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
        if taskbar_created == 0 {
            return Err(error("register tray recovery message"));
        }
        let state = RefCell::new(Ui {
            window: null_mut(),
            controls: BTreeMap::new(),
            worker,
            outputs: Vec::new(),
            networks: Vec::new(),
            devices: Vec::new(),
            endpoints: BTreeSet::new(),
            active: None,
            host: false,
            busy: false,
            quitting: false,
            paused: false,
            logs: VecDeque::new(),
            brush: CreateSolidBrush(NAVY),
            font: null_mut(),
            title_font: null_mut(),
            icon: icon()?,
            tray: false,
            taskbar_created,
            dpi: 96,
        });
        let instance = GetModuleHandleW(null());
        let mut class: WNDCLASSW = zeroed();
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        let class_name = wide(CLASS);
        class.lpszClassName = class_name.as_ptr();
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hbrBackground = state.borrow().brush;
        class.hIcon = state.borrow().icon;
        if RegisterClassW(&class) == 0 {
            return Err(error("register launcher window"));
        }
        let window = CreateWindowExW(
            0,
            class_name.as_ptr(),
            wide(TITLE).as_ptr(),
            WS_OVERLAPPEDWINDOW & !(WS_THICKFRAME | WS_MAXIMIZEBOX),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            null_mut(),
            null_mut(),
            instance,
            (&state as *const RefCell<Ui>).cast(),
        );
        if window.is_null() {
            return Err(error("create launcher window"));
        }
        ShowWindow(window, SW_SHOW);
        let mut message: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == -1 {
                return Err(error("read launcher messages"));
            }
            if result == 0 {
                break;
            }
            if !(WM_KEYFIRST..=WM_KEYLAST).contains(&message.message)
                || IsDialogMessageW(window, &message) == 0
            {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/launcher.rs"]
mod tests;
