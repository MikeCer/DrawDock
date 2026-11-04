use rcgen::generate_simple_self_signed;
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    os::windows::{ffi::OsStrExt, fs::OpenOptionsExt},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
    },
    Storage::FileSystem::{MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW},
};

const MAX_DEVICES: usize = 32;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrustedDevice {
    pub id: String,
    pub name: String,
    token_hash: String,
}

#[derive(Serialize)]
pub struct DeviceCredential {
    pub id: String,
    pub token: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct State {
    version: u32,
    certificate: Vec<u8>,
    private_key: Vec<u8>,
    devices: Vec<TrustedDevice>,
}

pub struct TrustStore {
    path: PathBuf,
    state: State,
    _lock: File,
}

impl TrustStore {
    pub fn open_default() -> Result<Self, String> {
        let directory = std::env::var_os("LOCALAPPDATA")
            .ok_or("LOCALAPPDATA is unavailable; cannot protect host identity")?;
        Self::open(Path::new(&directory).join("DrawDock"))
    }

    fn open(directory: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&directory)
            .map_err(|error| format!("create trust directory: {error}"))?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .share_mode(0)
            .open(directory.join("trust.lock"))
            .map_err(|error| {
                format!("lock trusted devices (stop other DrawDock hosts before managing trust): {error}")
            })?;
        let path = directory.join("trust.dpapi");
        let state = match fs::metadata(&path) {
            Ok(metadata) => {
                if metadata.len() > 1024 * 1024 {
                    return Err("encrypted trust store is too large".into());
                }
                let ciphertext =
                    fs::read(&path).map_err(|error| format!("read trust store: {error}"))?;
                let plaintext = protect(&ciphertext, false)?;
                let state: State = serde_json::from_slice(&plaintext)
                    .map_err(|_| "invalid decrypted trust store; refusing to replace identity")?;
                if state.version != 1
                    || state.devices.len() > MAX_DEVICES
                    || !(1..=16_384).contains(&state.certificate.len())
                    || !(1..=16_384).contains(&state.private_key.len())
                    || state.devices.iter().any(|device| {
                        !is_hex(&device.id, 32)
                            || !is_hex(&device.token_hash, 64)
                            || device.name.len() > 64
                            || device.name.chars().any(char::is_control)
                    })
                {
                    return Err("invalid trust store; refusing to replace identity".into());
                }
                state
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let identity = generate_simple_self_signed(vec!["drawdock".to_owned()])
                    .map_err(|error| format!("create persistent host TLS identity: {error}"))?;
                State {
                    version: 1,
                    certificate: identity.cert.der().to_vec(),
                    private_key: identity.key_pair.serialize_der(),
                    devices: Vec::new(),
                }
            }
            Err(error) => return Err(format!("inspect trust store: {error}")),
        };
        let store = Self {
            path,
            state,
            _lock: lock,
        };
        store.tls_config()?;
        if !store.path.exists() {
            store.save()?;
        }
        Ok(store)
    }

    pub fn fingerprint(&self) -> String {
        hex(&Sha256::digest(&self.state.certificate))
    }

    pub fn tls_config(&self) -> Result<ServerConfig, String> {
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.state.certificate.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.state.private_key.clone())),
            )
            .map_err(|error| format!("configure persistent host TLS identity: {error}"))
    }

    pub fn devices(&self) -> &[TrustedDevice] {
        &self.state.devices
    }

    pub fn remember(&mut self, name: String) -> Result<DeviceCredential, String> {
        if self.state.devices.len() >= MAX_DEVICES {
            return Err("trusted-device limit reached; revoke an unused device first".into());
        }
        if name.len() > 64 || name.chars().any(char::is_control) {
            return Err("invalid trusted-device name".into());
        }
        let credential = DeviceCredential {
            id: random_hex::<16>()?,
            token: random_hex::<32>()?,
        };
        self.state.devices.push(TrustedDevice {
            id: credential.id.clone(),
            name,
            token_hash: hex(&Sha256::digest(credential.token.as_bytes())),
        });
        if let Err(error) = self.save() {
            self.state.devices.pop();
            return Err(error);
        }
        Ok(credential)
    }

    pub fn authenticate(&self, id: &str, token: &str) -> Option<&TrustedDevice> {
        if !is_hex(id, 32) || !is_hex(token, 64) {
            return None;
        }
        let digest = hex(&Sha256::digest(token.as_bytes()));
        self.state.devices.iter().find(|device| {
            device.id == id
                && crate::session::constant_time_eq(digest.as_bytes(), device.token_hash.as_bytes())
        })
    }

    pub fn revoke(&mut self, id: &str) -> Result<(), String> {
        let index = self
            .state
            .devices
            .iter()
            .position(|device| device.id == id)
            .ok_or("unknown remembered-device ID")?;
        let device = self.state.devices.remove(index);
        if let Err(error) = self.save() {
            self.state.devices.insert(index, device);
            return Err(error);
        }
        Ok(())
    }

    fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec(&self.state)
            .map_err(|error| format!("serialize protected trust store: {error}"))?;
        let encrypted = protect(&bytes, true)?;
        let temporary = self.path.with_extension("dpapi.new");
        let mut file = File::create(&temporary)
            .map_err(|error| format!("create encrypted trust update: {error}"))?;
        file.write_all(&encrypted)
            .and_then(|_| file.sync_all())
            .map_err(|error| format!("write encrypted trust update: {error}"))?;
        drop(file);
        let from: Vec<_> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = self.path.as_os_str().encode_wide().chain(Some(0)).collect();
        // Both paths are stable, NUL-terminated buffers; the exclusive lock covers replacement.
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(format!(
                "replace encrypted trust store: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(())
    }
}

fn protect(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(bytes.len()).map_err(|_| "trust data is too large")?,
        pbData: bytes.as_ptr().cast_mut(),
    };
    let purpose = b"DrawDock trusted devices v1";
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: purpose.len() as u32,
        pbData: purpose.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    // DPAPI reads these buffers synchronously; its output must be released with LocalFree.
    let ok = unsafe {
        if encrypt {
            CryptProtectData(
                &input,
                null(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                null_mut(),
                &entropy,
                null(),
                null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
    };
    if ok == 0 {
        return Err(format!(
            "{} current-user DPAPI trust store: {}",
            if encrypt { "protect" } else { "unlock" },
            io::Error::last_os_error()
        ));
    }
    if output.pbData.is_null() || output.cbData == 0 {
        unsafe {
            LocalFree(output.pbData.cast());
        }
        return Err("DPAPI returned an empty trust store".into());
    }
    // A successful DPAPI call owns an initialized buffer of cbData bytes.
    let plaintext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    Ok(plaintext)
}

pub fn random_hex<const N: usize>() -> Result<String, String> {
    let mut bytes = [0; N];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("generate authentication credential: {error}"))?;
    Ok(hex(&bytes))
}

fn is_hex(value: &str, length: usize) -> bool {
    value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "Runs the built native host with an isolated trust store; requires an interactive desktop"]
    fn native_host_resumes_repeatedly_after_restart_and_rejects_revocation() {
        use crate::windows_overlay::Output;
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        use rustls::{
            ClientConfig, ClientConnection, RootCertStore, StreamOwned, pki_types::ServerName,
        };
        use serde_json::{Value, json};
        use std::{
            io::{BufRead, BufReader},
            net::{TcpListener, TcpStream},
            process::{Child, Command, Stdio},
            sync::{Arc, mpsc},
            thread,
            time::{Duration, Instant},
        };
        #[link(name = "user32")]
        unsafe extern "system" {
            fn SetThreadDpiAwarenessContext(
                context: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
        }
        struct ThreadDpi(*mut std::ffi::c_void);
        impl Drop for ThreadDpi {
            fn drop(&mut self) {
                // SAFETY: Restore this test thread's previous valid DPI-awareness context.
                unsafe {
                    SetThreadDpiAwarenessContext(self.0);
                }
            }
        }
        // SAFETY: Only this test thread changes context; the parallel video test owns process DPI.
        let previous = unsafe { SetThreadDpiAwarenessContext(-4isize as *mut std::ffi::c_void) };
        assert!(!previous.is_null(), "set physical-pixel test thread DPI");
        let _dpi = ThreadDpi(previous);
        type Wire = BufReader<StreamOwned<ClientConnection, TcpStream>>;
        fn exchange(stream: &mut Wire, command: Value) -> Value {
            crate::session::write_json_line(stream.get_mut(), &command).unwrap();
            crate::session::read_json_line(stream).unwrap()
        }
        fn assert_editor(response: &Value, request_id: u64, epoch: u64) {
            assert_eq!(response["type"], "editor_ack", "{response}");
            assert_eq!(response["request_id"], request_id);
            assert_eq!(response["source_epoch"], epoch);
            assert!(response["canvas"]["blackboard"].is_boolean());
            assert!(response["canvas"]["grid"].is_boolean());
            assert_eq!(response["mode"], "draw");
        }
        fn assert_error(response: &Value, request_id: u64, epoch: u64) {
            assert_eq!(response["type"], "error", "{response}");
            assert_eq!(response["request_id"], request_id);
            assert_eq!(response["source_epoch"], epoch);
            assert!(response["message"].as_str().is_some_and(|s| !s.is_empty()));
        }
        fn wait_pixel(stream: &mut Wire, bounds: [i32; 4], x: i32, y: i32, rgb: [u8; 3]) {
            use windows_sys::Win32::Graphics::Gdi::{
                BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, CAPTUREBLT, CreateCompatibleDC,
                CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC,
                ReleaseDC, SRCCOPY, SelectObject,
            };
            let expected = u32::from(rgb[0]) | (u32::from(rgb[1]) << 8) | (u32::from(rgb[2]) << 16);
            let [left, top, width, height] = bounds;
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                crate::session::write_json_line(stream.get_mut(), &json!({"type":"heartbeat"}))
                    .unwrap();
                // SAFETY: Capture the physical monitor including layered windows into an owned bitmap,
                // restore the selected object and release all GDI resources on every normal path.
                let actual = unsafe {
                    let dc = GetDC(std::ptr::null_mut());
                    assert!(!dc.is_null(), "acquire test screen DC");
                    let memory = CreateCompatibleDC(dc);
                    let mut info: BITMAPINFO = std::mem::zeroed();
                    info.bmiHeader = BITMAPINFOHEADER {
                        biSize: size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: width,
                        biHeight: -height,
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB,
                        ..std::mem::zeroed()
                    };
                    let mut bits = std::ptr::null_mut();
                    let bitmap = CreateDIBSection(
                        dc,
                        &info,
                        DIB_RGB_COLORS,
                        &mut bits,
                        std::ptr::null_mut(),
                        0,
                    );
                    assert!(
                        !memory.is_null() && !bitmap.is_null(),
                        "allocate test pixel bitmap"
                    );
                    let previous = SelectObject(memory, bitmap);
                    let copied = BitBlt(
                        memory,
                        0,
                        0,
                        width,
                        height,
                        dc,
                        left,
                        top,
                        SRCCOPY | CAPTUREBLT,
                    );
                    GdiFlush();
                    let frame = std::slice::from_raw_parts(
                        bits.cast::<u8>(),
                        width as usize * height as usize * 4,
                    );
                    let offset = ((y - top) * width + (x - left)) as usize * 4;
                    let bgra = &frame[offset..offset + 4];
                    let color =
                        u32::from(bgra[2]) | (u32::from(bgra[1]) << 8) | (u32::from(bgra[0]) << 16);
                    SelectObject(memory, previous);
                    DeleteObject(bitmap);
                    DeleteDC(memory);
                    ReleaseDC(std::ptr::null_mut(), dc);
                    assert_ne!(copied, 0, "capture native layered overlay pixel");
                    color
                };
                if actual == expected {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "native overlay pixel ({x},{y}) expected {expected:#08x}, got {actual:#08x}"
                );
                thread::sleep(Duration::from_millis(100));
            }
        }
        struct TestDirectory(PathBuf);
        impl Drop for TestDirectory {
            fn drop(&mut self) {
                if let Err(error) = fs::remove_dir_all(&self.0) {
                    eprintln!(
                        "Remove isolated native test store {}: {error}",
                        self.0.display()
                    );
                }
            }
        }
        struct Host {
            child: Child,
            events: mpsc::Receiver<String>,
            logger: Option<thread::JoinHandle<()>>,
        }
        impl Host {
            fn wait(&self, marker: &str) {
                self.wait_all(&[marker]);
            }
            fn wait_all(&self, markers: &[&str]) {
                let mut seen = vec![false; markers.len()];
                loop {
                    let line = self
                        .events
                        .recv_timeout(Duration::from_secs(15))
                        .expect("Host did not become ready; inspect its stderr");
                    eprintln!("Test host: {line}");
                    for (marker, seen) in markers.iter().zip(&mut seen) {
                        *seen |= line.contains(marker);
                    }
                    if seen.iter().all(|seen| *seen) {
                        return;
                    }
                }
            }
        }
        impl Drop for Host {
            fn drop(&mut self) {
                if self.child.try_wait().ok().flatten().is_none() {
                    self.child.kill().expect("stop owned test host");
                }
                self.child.wait().expect("wait for owned test host");
                self.logger.take().unwrap().join().unwrap();
            }
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "drawdock-resume-test-{}",
                random_hex::<8>().unwrap()
            ));
        let _directory_cleanup = TestDirectory(directory.clone());
        let mut trust = TrustStore::open(directory.join("DrawDock")).unwrap();
        let credential = trust.remember("TLS reconnect fixture".into()).unwrap();
        trust
            .remember("Independent retained device".into())
            .unwrap();
        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from(trust.state.certificate.clone()))
            .unwrap();
        let config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        drop(trust);
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("dd-host.exe");
        assert!(
            executable.is_file(),
            "Run cargo build -p dd-host before this test"
        );
        let output = Output::enumerate()
            .unwrap()
            .into_iter()
            .find(|output| output.primary)
            .unwrap();
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let start = || {
            let mut child = Command::new(&executable)
                .args([
                    "serve",
                    "--output",
                    &output.name,
                    "--port",
                    &port.to_string(),
                ])
                .env("LOCALAPPDATA", &directory)
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .unwrap();
            let stdout = child.stdout.take().unwrap();
            let (sender, events) = mpsc::channel();
            let logger = thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let line = line.expect("read native host output");
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            });
            let host = Host {
                child,
                events,
                logger: Some(logger),
            };
            host.wait_all(&["Authenticated TLS/TCP listener", "Overlay visible"]);
            host
        };
        let resume = |token: &str| {
            let tcp = TcpStream::connect(("127.0.0.1", port)).unwrap();
            tcp.set_read_timeout(Some(Duration::from_secs(15))).unwrap();
            let tls = ClientConnection::new(
                Arc::clone(&config),
                ServerName::try_from("drawdock").unwrap(),
            )
            .unwrap();
            let mut stream = BufReader::new(StreamOwned::new(tls, tcp));
            crate::session::write_json_line(
                stream.get_mut(),
                &serde_json::json!({
                    "type": "resume", "device_id": credential.id, "token": token,
                }),
            )
            .unwrap();
            let response = crate::session::read_json_line(&mut stream).unwrap();
            (stream, response)
        };
        let host = start();
        let (bad, rejection) = resume(&"0".repeat(64));
        assert_eq!(rejection["type"], "authentication_failed");
        drop(bad);
        let (mut stream, paired) = resume(&credential.token);
        assert_eq!(paired["type"], "paired");
        assert!(paired["remembered_device"].is_null());
        let first_video_token = paired["video_token"].clone();
        let epoch = paired["selected"]["source_epoch"].as_u64().unwrap();
        let send = |stream: &mut Wire, mut command: Value| {
            command["source_epoch"] = json!(epoch);
            exchange(stream, command)
        };
        let blocked = send(
            &mut stream,
            json!({"type":"select_object","x":0.7,"y":0.25,"request_id":100}),
        );
        assert_error(&blocked, 100, epoch);
        let ready = send(&mut stream, json!({"type":"source_ready"}));
        assert_eq!(
            ready,
            json!({"type":"source_ready_ack","source_epoch":epoch,"canvas":{"blackboard":false,"grid":false},"mode":"draw"})
        );
        let stale = exchange(
            &mut stream,
            json!({"type":"select_object","x":0.7,"y":0.25,"request_id":101,"source_epoch":epoch+1}),
        );
        assert_error(&stale, 101, epoch + 1);

        let mut png = resvg::tiny_skia::Pixmap::new(1, 1).unwrap();
        png.fill(resvg::tiny_skia::Color::from_rgba8(12, 34, 56, 255));
        let image_data = STANDARD.encode(png.encode_png().unwrap());
        for (index, tool) in [
            "line",
            "circle",
            "polygon",
            "polyline",
            "highlighter",
            "text",
            "image",
        ]
        .into_iter()
        .enumerate()
        {
            let points = match tool {
                "line" => json!([{"x":0.6,"y":0.2},{"x":0.8,"y":0.3}]),
                "polygon" | "polyline" | "highlighter" => {
                    json!([{"x":0.5,"y":0.6},{"x":0.6,"y":0.7},{"x":0.7,"y":0.6}])
                }
                "text" => json!([{"x":0.05,"y":0.55},{"x":0.45,"y":0.65}]),
                "image" => json!([{"x":0.2,"y":0.2},{"x":0.3,"y":0.3}]),
                _ => json!([{"x":0.7,"y":0.7},{"x":0.8,"y":0.8}]),
            };
            let mut object = json!({"id":200+index,"tool":tool,"style":{"rgba":[255,0,0,if tool=="highlighter" {80} else {255}],"width":if tool=="text" {0.022} else {0.008}},"points":points});
            if tool == "text" {
                object["text"] = json!("Native TLS & <text>");
            }
            if tool == "image" {
                object["image_data"] = json!(image_data);
            }
            let ack = send(&mut stream, json!({"type":"commit","object":object}));
            assert_eq!(ack["type"], "ack", "{tool}: {ack}");
        }
        let selected = send(
            &mut stream,
            json!({"type":"select_object","x":0.7,"y":0.25,"request_id":110}),
        );
        assert_editor(&selected, 110, epoch);
        assert_eq!(selected["selection"]["id"], 200);
        assert_eq!(
            selected["selection"]["local_bounds"],
            selected["selection"]["bounds"]
        );
        assert_eq!(
            selected["selection"]["bounds"].as_object().unwrap().len(),
            4
        );
        for (field, expected) in [("left", 0.6), ("top", 0.2), ("right", 0.8), ("bottom", 0.3)] {
            assert!(
                (selected["selection"]["bounds"][field].as_f64().unwrap() - expected).abs()
                    < 0.00001
            );
        }
        let transform = json!({"a":1.0,"b":0.0,"c":0.0,"d":1.0,"tx":0.05,"ty":0.1});
        let transformed = send(
            &mut stream,
            json!({"type":"transform_object","object_id":200,"transform":transform,"request_id":111}),
        );
        assert_editor(&transformed, 111, epoch);
        assert_eq!(transformed["selection"]["id"], 200);
        assert_eq!(
            transformed["selection"]["local_bounds"],
            selected["selection"]["local_bounds"]
        );
        assert_eq!(
            transformed["selection"]["bounds"]
                .as_object()
                .unwrap()
                .len(),
            4
        );
        for (field, expected) in [
            ("left", 0.65),
            ("top", 0.3),
            ("right", 0.85),
            ("bottom", 0.4),
        ] {
            assert!(
                (transformed["selection"]["bounds"][field].as_f64().unwrap() - expected).abs()
                    < 0.00001
            );
        }
        for (field, expected) in [
            ("a", 1.0),
            ("b", 0.0),
            ("c", 0.0),
            ("d", 1.0),
            ("tx", 0.05),
            ("ty", 0.1),
        ] {
            assert!(
                (transformed["selection"]["transform"][field]
                    .as_f64()
                    .unwrap()
                    - expected)
                    .abs()
                    < 0.00001
            );
        }
        let invalid = send(
            &mut stream,
            json!({"type":"transform_object","object_id":200,"transform":{"a":0,"b":0,"c":0,"d":0,"tx":0,"ty":0},"request_id":112}),
        );
        assert_error(&invalid, 112, epoch);
        let selected = send(
            &mut stream,
            json!({"type":"select_object","x":0.75,"y":0.35,"request_id":113}),
        );
        assert_eq!(selected["selection"], transformed["selection"]);

        let board = send(
            &mut stream,
            json!({"type":"set_canvas","blackboard":true,"grid":true,"request_id":120}),
        );
        assert_editor(&board, 120, epoch);
        assert!(board["selection"].is_null());
        assert_eq!(board["canvas"], json!({"blackboard":true,"grid":true}));
        let ready = send(&mut stream, json!({"type":"source_ready"}));
        assert_eq!(ready["canvas"], board["canvas"]);
        assert_eq!(ready["mode"], "draw");
        let denied = send(
            &mut stream,
            json!({"type":"set_mode","mode":"pointer","request_id":121}),
        );
        assert_error(&denied, 121, epoch);
        assert!(denied["message"].as_str().unwrap().contains("blackboard"));

        let physical = &paired["selected"]["source"];
        let left = physical["left"].as_i64().unwrap() as i32;
        let top = physical["top"].as_i64().unwrap() as i32;
        let width = physical["right"].as_i64().unwrap() as i32 - left;
        let height = physical["bottom"].as_i64().unwrap() as i32 - top;
        wait_pixel(
            &mut stream,
            [left, top, width, height],
            left + width / 4,
            top + height / 4,
            [12, 34, 56],
        );
        wait_pixel(
            &mut stream,
            [left, top, width, height],
            left + (width / 32 - 2) * 32 + 16,
            top + (height / 32 - 2) * 32 + 16,
            [0, 0, 0],
        );
        let clear = send(
            &mut stream,
            json!({"type":"clear_display","request_id":122}),
        );
        assert_eq!(clear["type"], "clear_ack");
        assert_eq!(clear["request_id"], 122);
        assert_eq!(clear["source_epoch"], epoch);
        assert_eq!(clear["canvas"], json!({"blackboard":false,"grid":false}));
        let pointer = send(
            &mut stream,
            json!({"type":"set_mode","mode":"pointer","request_id":123}),
        );
        assert_eq!(pointer["type"], "mode_ack");
        assert_eq!(pointer["mode"], "pointer");
        assert_eq!(pointer["request_id"], 123);
        let pointer_ready = send(&mut stream, json!({"type":"source_ready"}));
        assert_eq!(pointer_ready["mode"], "pointer");
        assert_eq!(pointer_ready["canvas"], clear["canvas"]);
        let undo = send(&mut stream, json!({"type":"undo","request_id":124}));
        assert_eq!(undo["type"], "ack");
        assert_eq!(undo["request_id"], 124);
        assert_eq!(undo["source_epoch"], epoch);
        assert_eq!(undo["mode"], "draw");
        assert_eq!(undo["canvas"], board["canvas"]);
        wait_pixel(
            &mut stream,
            [left, top, width, height],
            left + width / 4,
            top + height / 4,
            [12, 34, 56],
        );
        let redo = send(&mut stream, json!({"type":"redo","request_id":125}));
        assert_eq!(redo["canvas"], clear["canvas"]);
        assert_eq!(redo["mode"], "draw");
        let undo = send(&mut stream, json!({"type":"undo","request_id":126}));
        assert_eq!(undo["canvas"], board["canvas"]);
        let clear = send(&mut stream, json!({"type":"clear","request_id":127}));
        assert_eq!(clear["canvas"], json!({"blackboard":false,"grid":false}));
        assert_eq!(clear["request_id"], 127);
        let selected = send(
            &mut stream,
            json!({"type":"select_object","x":0.75,"y":0.35,"request_id":128}),
        );
        assert!(selected["selection"].is_null());
        let image = send(
            &mut stream,
            json!({"type":"commit","object":{"id":300,"tool":"image","style":{"rgba":[255,0,0,255],"width":0.008},
                "points":[{"x":0.2,"y":0.2},{"x":0.3,"y":0.3}],"image_data":image_data}}),
        );
        assert_eq!(image["type"], "ack");
        let aspect = width as f64 / height as f64;
        let a = std::f64::consts::FRAC_1_SQRT_2;
        let b = a * aspect;
        let c = -a / aspect;
        let tx = 0.25 - a * 0.25 - c * 0.25;
        let ty = 0.25 - b * 0.25 - a * 0.25;
        let rotated = send(
            &mut stream,
            json!({"type":"transform_object","object_id":300,"request_id":129,
                "transform":{"a":a,"b":b,"c":c,"d":a,"tx":tx,"ty":ty}}),
        );
        assert_editor(&rotated, 129, epoch);
        for (field, expected) in [("left", 0.2), ("top", 0.2), ("right", 0.3), ("bottom", 0.3)] {
            assert!(
                (rotated["selection"]["local_bounds"][field]
                    .as_f64()
                    .unwrap()
                    - expected)
                    .abs()
                    < 0.00001
            );
        }
        let selected = send(
            &mut stream,
            json!({"type":"select_object","x":0.25,"y":0.25,"request_id":130}),
        );
        assert_eq!(selected["selection"], rotated["selection"]);
        wait_pixel(
            &mut stream,
            [left, top, width, height],
            left + ((a * 0.205 + c * 0.205 + tx) * width as f64) as i32,
            top + ((b * 0.205 + a * 0.205 + ty) * height as f64) as i32,
            [12, 34, 56],
        );
        println!(
            "Native TLS editor commits, oriented image selection/reselection, correlated errors, board policy/history, canvas synchronization, and physical rotated PNG/blackboard pixels verified."
        );
        drop(stream);
        host.wait("Waiting for a remembered tablet to reconnect");
        let (mut stream, paired) = resume(&credential.token);
        assert_eq!(paired["type"], "paired");
        assert!(
            paired["video_token"] != first_video_token,
            "Each session needs a fresh video credential"
        );
        let epoch = paired["selected"]["source_epoch"].as_u64().unwrap();
        let ready = exchange(
            &mut stream,
            json!({"type":"source_ready","source_epoch":epoch}),
        );
        assert_eq!(ready["canvas"], json!({"blackboard":false,"grid":false}));
        assert_eq!(ready["mode"], "draw");
        drop(stream);
        host.wait("Waiting for a remembered tablet to reconnect");
        drop(host);
        let host = start();
        let (stream, paired) = resume(&credential.token);
        assert_eq!(
            paired["type"], "paired",
            "Pinned TLS and trust must survive process restart"
        );
        drop(stream);
        host.wait("Waiting for a remembered tablet to reconnect");
        drop(host);
        let mut trust = TrustStore::open(directory.join("DrawDock")).unwrap();
        trust.revoke(&credential.id).unwrap();
        drop(trust);
        let host = start();
        let (stream, response) = resume(&credential.token);
        assert_eq!(response["type"], "authentication_failed");
        drop(stream);
        drop(host);
        println!(
            "Native host reused pinned TLS trust across repeated sessions and process restart; wrong and revoked credentials rejected."
        );
    }

    #[test]
    fn dpapi_identity_credentials_and_revocation_survive_restart() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = std::env::temp_dir().join(format!(
            "drawdock-trust-test-{}",
            random_hex::<8>().unwrap()
        ));
        let mut store = TrustStore::open(directory.clone()).unwrap();
        let fingerprint = store.fingerprint();
        let credential = store.remember("Test tablet".into()).unwrap();
        assert!(
            TrustStore::open(directory.clone()).is_err(),
            "Concurrent writers must be excluded"
        );
        assert!(
            store
                .authenticate(&credential.id, &"0".repeat(64))
                .is_none()
        );
        assert!(store.authenticate("unknown", &credential.token).is_none());
        let encrypted = fs::read(&store.path).unwrap();
        assert!(
            !encrypted
                .windows(credential.token.len())
                .any(|part| part == credential.token.as_bytes())
        );
        drop(store);
        let mut store = TrustStore::open(directory.clone()).unwrap();
        assert_eq!(store.fingerprint(), fingerprint);
        assert!(
            store
                .authenticate(&credential.id, &credential.token)
                .is_some()
        );
        store.revoke(&credential.id).unwrap();
        assert!(
            store
                .authenticate(&credential.id, &credential.token)
                .is_none()
        );
        drop(store);
        let store = TrustStore::open(directory.clone()).unwrap();
        assert_eq!(store.fingerprint(), fingerprint);
        assert!(
            store
                .authenticate(&credential.id, &credential.token)
                .is_none()
        );
        drop(store);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn corrupt_state_is_not_silently_replaced_with_a_new_identity() {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let directory = std::env::temp_dir().join(format!(
            "drawdock-trust-test-{}",
            random_hex::<8>().unwrap()
        ));
        let store = TrustStore::open(directory.clone()).unwrap();
        let path = store.path.clone();
        drop(store);
        fs::write(&path, b"invalid protected state").unwrap();
        assert!(TrustStore::open(directory.clone()).is_err());
        assert_eq!(fs::read(path).unwrap(), b"invalid protected state");
        fs::remove_dir_all(directory).unwrap();
    }
}
