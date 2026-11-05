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
        fn SetThreadDpiAwarenessContext(context: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
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
            CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, ReleaseDC,
            SRCCOPY, SelectObject,
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
            (selected["selection"]["bounds"][field].as_f64().unwrap() - expected).abs() < 0.00001
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
