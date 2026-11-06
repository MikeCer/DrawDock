use crate::{
    pairing_window::{self, PairingWindow},
    source::SharedSource,
    trust::TrustStore,
    video::{VideoEncoder, VideoService},
    windows_input::{Mode, OutputBounds, WindowsInput},
    windows_overlay::Output,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use dd_scene::{Canvas, InkObject, Point, Scene, Transform};
use rustls::{ServerConnection, StreamOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    fs::OpenOptions,
    io::{self, BufRead, BufReader, Write},
    net::{IpAddr, TcpStream},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const MAX_FRAME_BYTES: usize = 64 * 1024;
const MAX_AUTH_ATTEMPTS: usize = 3;
const TOKEN_LIFETIME: Duration = Duration::from_secs(5 * 60);
const READ_LEASE: Duration = Duration::from_millis(1500);
const MAX_MESSAGES_PER_SECOND: u32 = 120;
const MAX_COMMIT_BYTES: usize = 512 * 1024;
const MAX_COMMIT_CHUNK_BYTES: usize = 48 * 1024;
const COMMIT_LIFETIME: Duration = Duration::from_secs(3);

struct CommitTransaction {
    object_id: u64,
    digest: String,
    bytes: Vec<u8>,
    expected_bytes: usize,
    expires: Instant,
    restoring: bool,
}

pub fn run_server(
    listeners: Arc<crate::listener_control::Listeners>,
    scene: Arc<Mutex<Scene>>,
    source: SharedSource,
    pairing_info: Option<PathBuf>,
    allow_pairing: bool,
    mut trust: TrustStore,
    encoder: VideoEncoder,
) -> Result<(), String> {
    let initial_endpoint = if listeners.managed {
        None
    } else {
        // The CLI has exactly one already-bound endpoint.
        listeners.endpoints()?.first().copied()
    };
    let address = initial_endpoint
        .map(|endpoint| endpoint.ip())
        .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST));
    let port = initial_endpoint
        .map(|endpoint| endpoint.port())
        .unwrap_or(47831);
    let fingerprint = trust.fingerprint();
    let enroll = !listeners.managed
        && (allow_pairing || pairing_info.is_some() || trust.devices().is_empty());
    let mut token_bytes = [0u8; 16];
    getrandom::fill(&mut token_bytes)
        .map_err(|error| format!("generate invitation token: {error}"))?;
    let mut token = hex(&token_bytes);
    if let Some(path) = pairing_info {
        let info = serde_json::to_vec_pretty(&json!({
            "host": address.to_string(),
            "port": port,
            "fingerprint": fingerprint,
            "token": token
        }))
        .map_err(|error| format!("serialize one-time pairing info: {error}"))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| format!("create one-time pairing info {}: {error}", path.display()))?;
        file.write_all(&info)
            .map_err(|error| format!("write one-time pairing info {}: {error}", path.display()))?;
        println!("One-time pairing info written to {}.", path.display());
    }
    let config = Arc::new(trust.tls_config()?);
    let mut expires = Instant::now() + TOKEN_LIFETIME;
    let mut pairing_endpoint = initial_endpoint;
    let invitation = invitation_uri(address, port, &fingerprint, &token);
    let mut pairing = if enroll {
        Some(PairingWindow::open(
            &invitation,
            &format!("{address}:{port}"),
            TOKEN_LIFETIME,
        )?)
    } else {
        None
    };
    if !listeners.managed {
        println!("Authenticated TLS/TCP listener: {address}:{port}");
    }
    listeners.report(json!({"event":"ready"}));
    println!("Remembered tablets can reconnect without scanning or another approval.");
    if enroll {
        println!("For a new tablet, scan the QR and approve remembering it in the PC dialog.");
        println!("Invitation expires in 5 minutes; closing it cancels new enrollment only.");
    } else if listeners.managed {
        println!(
            "For a new tablet, select an enabled listener and choose Pair tablet / QR in the launcher."
        );
    } else {
        println!("To enroll a new tablet, restart with --pair (launcher: -Restart -Pair).");
    }
    if !address.is_loopback() {
        println!(
            "WARNING: listening on the explicitly selected private interface. \
             No firewall rule was changed."
        );
    }
    let mut attempts = 0;
    let mut auth_window = Instant::now();
    let mut auth_failures = 0;
    loop {
        if listeners.stopped() {
            return Ok(());
        }
        if let Some(endpoint) = listeners.invitation()? {
            getrandom::fill(&mut token_bytes)
                .map_err(|error| format!("generate invitation: {error}"))?;
            token = hex(&token_bytes);
            expires = Instant::now() + TOKEN_LIFETIME;
            drop(pairing.take());
            pairing = Some(PairingWindow::open(
                &invitation_uri(endpoint.ip(), endpoint.port(), &fingerprint, &token),
                &endpoint.to_string(),
                TOKEN_LIFETIME,
            )?);
            pairing_endpoint = Some(endpoint);
            attempts = 0;
            println!("New-device invitation opened for {endpoint}; expires in five minutes.");
        }
        let endpoint_closed = match pairing_endpoint {
            Some(endpoint) => !listeners.enabled(endpoint)?,
            None => false,
        };
        if pairing.as_ref().is_some_and(|window| {
            !window.is_open() || Instant::now() >= expires || attempts >= MAX_AUTH_ATTEMPTS
        }) || endpoint_closed
        {
            drop(pairing.take());
            pairing_endpoint = None;
            println!("New-device invitation closed; remembered devices may still reconnect.");
        }
        if auth_window.elapsed() >= Duration::from_secs(10) {
            auth_window = Instant::now();
            auth_failures = 0;
        }
        if auth_failures >= MAX_AUTH_ATTEMPTS {
            thread::sleep(Duration::from_millis(50));
            continue;
        }
        let (tcp, peer, endpoint) = match listeners.accept()? {
            Some(connection) => connection,
            None => {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
        };
        let _connection = match listeners.connection(endpoint, &tcp) {
            Ok(connection) => connection,
            Err(error) => {
                eprintln!("Connection no longer eligible: {error}");
                continue;
            }
        };
        tcp.set_nonblocking(false)
            .map_err(|error| format!("configure accepted connection from {peer}: {error}"))?;
        tcp.set_nodelay(true)
            .map_err(|error| format!("set TCP_NODELAY for {peer}: {error}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|error| format!("set pairing read timeout: {error}"))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| format!("set pairing write timeout: {error}"))?;
        let tls = match ServerConnection::new(Arc::clone(&config)) {
            Ok(connection) => StreamOwned::new(connection, tcp),
            Err(error) => {
                attempts += 1;
                auth_failures += 1;
                eprintln!("Rejected TLS session from {peer}: {error}");
                continue;
            }
        };
        let mut reader = BufReader::new(tls);
        let invitation = match read_json_line(&mut reader) {
            Ok(message) => message,
            Err(error) => {
                attempts += 1;
                auth_failures += 1;
                eprintln!("Rejected invalid pairing request from {peer}: {error}");
                continue;
            }
        };
        let resumed = invitation["type"] == "resume";
        let remembered = if resumed {
            trust.authenticate(
                invitation["device_id"].as_str().unwrap_or(""),
                invitation["token"].as_str().unwrap_or(""),
            )
        } else {
            None
        };
        let new_pairing = !resumed
            && pairing_endpoint == Some(endpoint)
            && pairing.as_ref().is_some_and(PairingWindow::is_open)
            && valid_invitation(&invitation, &token, expires);
        if remembered.is_none() && !new_pairing {
            attempts += 1;
            auth_failures += 1;
            eprintln!("Rejected unauthorized pairing request from {peer}");
            if let Err(error) = write_json_line(
                reader.get_mut(),
                &json!({
                    "type": "authentication_failed",
                    "message": "Credential not recognized or invitation closed; pair again using a new QR."
                }),
            ) {
                eprintln!("Could not deliver authentication rejection to {peer}: {error}");
            }
            continue;
        }

        let client_name = remembered
            .map(|device| device.name.clone())
            .unwrap_or_else(|| {
                invitation
                    .get("client")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty() && value.len() <= 64)
                    .unwrap_or("Android tablet")
                    .chars()
                    .filter(|character| !character.is_control())
                    .collect::<String>()
            });
        let credential = if resumed {
            println!("Remembered tablet reconnected: {client_name} at {peer}.");
            None
        } else {
            println!("Pair request from {client_name} at {peer}.");
            if !pairing_window::approve(&client_name, &peer.to_string())? {
                eprintln!("Pairing denied by local user.");
                drop(pairing.take());
                continue;
            }
            if Instant::now() >= expires || !pairing.as_ref().is_some_and(PairingWindow::is_open) {
                eprintln!("Pairing expired or cancelled before approval; device not remembered.");
                drop(pairing.take());
                continue;
            }
            if !listeners.enabled(endpoint)? {
                eprintln!("Listener disabled before approval completed; device not remembered.");
                continue;
            }
            Some(trust.remember(client_name)?)
        };

        drop(pairing.take());
        auth_failures = 0;
        let output = source
            .lock()
            .map_err(|_| "source lock poisoned")?
            .output
            .clone();
        let mut input = WindowsInput::new(OutputBounds {
            left: output.left,
            top: output.top,
            width: output.width(),
            height: output.height(),
        })?;
        let result: Result<(), String> = (|| {
            let mut video_token = [0; 16];
            getrandom::fill(&mut video_token)
                .map_err(|error| format!("generate video credential: {error}"))?;
            let video_token = hex(&video_token);
            let video = VideoService::start(
                listeners.video_listener(endpoint)?,
                Arc::clone(&config),
                video_token.clone(),
                Arc::clone(&source),
                encoder,
            )?;
            let selected = source
                .lock()
                .map_err(|_| "source lock poisoned")?
                .metadata();
            let stream = reader.get_mut();
            write_json_line(
                stream,
                &json!({
                    "type": "paired", "version": 2, "sources": Output::enumerate()?,
                    "selected": selected, "video_token": video_token,
                    "remembered_device": credential,
                    "shared_zoom": true,
                    "drawing_recovery": true,
                    "recovery_object_bytes": size_of::<InkObject>(),
                }),
            )?;
            stream
                .get_ref()
                .set_read_timeout(Some(READ_LEASE))
                .map_err(|error| format!("set session lease timeout: {error}"))?;
            println!("Tablet authenticated. Disconnect ends the session.");
            listeners
                .report(json!({"event":"session","active":true,"endpoint":endpoint.to_string()}));
            let result = serve_authenticated(
                &mut reader,
                Arc::clone(&scene),
                &mut input,
                Arc::clone(&source),
                video.running(),
            );
            let reset_zoom = crate::source::apply_zoom(&source, crate::zoom::Viewport::default());
            drop(video);
            reset_zoom?;
            result
        })();
        let release_result = input.release_all();
        let clear_result = scene
            .lock()
            .map_err(|_| "canonical scene lock is poisoned".to_owned())
            .map(|mut state| state.reset_session());
        println!("Remote session ended; host annotations were cleared.");
        release_result?;
        clear_result?;
        if let Err(error) = result {
            eprintln!("DrawDock session ended: {error}");
        }
        println!("Waiting for a remembered tablet to reconnect.");
    }
}

fn invitation_uri(address: IpAddr, port: u16, fingerprint: &str, token: &str) -> String {
    format!(
        "drawdock://pair?v=1&host={address}&port={port}&fingerprint={fingerprint}&token={token}"
    )
}

fn valid_invitation(message: &Value, token: &str, expires: Instant) -> bool {
    Instant::now() < expires
        && message.get("type").and_then(Value::as_str) == Some("pair")
        && constant_time_eq(
            message
                .get("token")
                .and_then(Value::as_str)
                .unwrap_or("")
                .as_bytes(),
            token.as_bytes(),
        )
}

fn serve_authenticated(
    reader: &mut BufReader<StreamOwned<ServerConnection, TcpStream>>,
    scene: Arc<Mutex<Scene>>,
    input: &mut WindowsInput,
    source: SharedSource,
    video_running: Arc<AtomicBool>,
) -> Result<(), String> {
    let mut window_start = Instant::now();
    let mut message_count = 0;
    let mut transaction: Option<CommitTransaction> = None;
    let mut source_ready = false;
    let mut source_scenes: HashMap<String, Scene> = HashMap::new();
    let trace_performance = std::env::var("DRAWDOCK_PERF_TRACE").as_deref() == Ok("1");
    let mut zoom_timing: Option<(u64, Instant)> = None;
    loop {
        let message = match read_json_line(reader) {
            Ok(message) => message,
            Err(error)
                if error.kind() == io::ErrorKind::WouldBlock
                    || error.kind() == io::ErrorKind::TimedOut =>
            {
                return Err("peer lease expired; no input is retained".into());
            }
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err("tablet disconnected".into());
            }
            Err(error) => return Err(format!("read tablet command: {error}")),
        };

        if window_start.elapsed() >= Duration::from_secs(1) {
            let selected = source.lock().map_err(|_| "source lock poisoned")?.clone();
            let still_present = Output::enumerate()?.iter().any(|output| {
                output.name == selected.output.name
                    && output.left == selected.output.left
                    && output.top == selected.output.top
                    && output.right == selected.output.right
                    && output.bottom == selected.output.bottom
            });
            if !still_present {
                return Err(
                    "selected monitor was removed or its geometry changed; reconnect required"
                        .into(),
                );
            }
            window_start = Instant::now();
            message_count = 0;
        }
        message_count += 1;
        if message_count > MAX_MESSAGES_PER_SECOND {
            return Err("peer exceeded the control message rate limit".into());
        }

        if transaction
            .as_ref()
            .is_some_and(|active| Instant::now() >= active.expires)
        {
            transaction = None;
        }
        let response = (|| {
            let command = message["type"].as_str().unwrap_or("");
            if !video_running.load(Ordering::Acquire) {
                if source_ready {
                    input.release_all()?;
                    source_ready = false;
                }
                if command == "source_ready" {
                    return Err("video unavailable; reconnect required".to_owned());
                }
            }
            if command != "heartbeat" {
                source
                    .lock()
                    .map_err(|_| "source lock poisoned")?
                    .validate_epoch(&message)?;
            }
            if command == "source_ready" {
                if let Some((epoch, started)) = zoom_timing.take()
                    && trace_performance
                    && message["source_epoch"].as_u64() == Some(epoch)
                {
                    println!(
                        "PERF zoom_ready epoch={epoch} elapsed_ms={:.3}",
                        started.elapsed().as_secs_f64() * 1000.0
                    );
                }
                source_ready = true;
                let canvas = scene.lock().map_err(|_| "scene lock poisoned")?.canvas();
                return Ok(Some(source_ready_ack(&message, canvas, input.mode())));
            }
            if command == "select_source" {
                zoom_timing = None;
                let name = message["source"].as_str().ok_or("source name missing")?;
                let request_id = message["request_id"]
                    .as_u64()
                    .ok_or("source request id missing")?;
                let output = Output::enumerate()?
                    .into_iter()
                    .find(|output| output.name == name)
                    .ok_or("selected monitor is no longer available")?;
                crate::source::apply_zoom(&source, crate::zoom::Viewport::default())?;
                input.select_output(OutputBounds {
                    left: output.left,
                    top: output.top,
                    width: output.width(),
                    height: output.height(),
                })?;
                source_ready = false;
                transaction = None;
                let mut selected = source.lock().map_err(|_| "source lock poisoned")?;
                let mut scene = scene.lock().map_err(|_| "scene lock poisoned")?;
                scene.clear_laser();
                source_scenes.insert(selected.output.name.clone(), scene.clone());
                *scene = source_scenes.remove(name).unwrap_or_default();
                if scene.canvas().blackboard {
                    input.force_draw()?;
                }
                selected.output = output;
                selected.reset_presentations();
                selected.epoch = selected
                    .epoch
                    .checked_add(1)
                    .ok_or("source epoch exhausted")?;
                return Ok(Some(
                    json!({"type": "source_selected", "request_id": request_id, "selected": selected.metadata(), "canvas": scene.canvas(), "mode":input.mode().as_str()}),
                ));
            }
            require_ready(command, source_ready)?;
            if transaction.as_ref().is_some_and(|active| active.restoring)
                && !matches!(command, "heartbeat" | "restore_chunk" | "finish_restore")
            {
                return Err("drawing restoration is in progress".into());
            }
            if command == "finish_restore" {
                let active = transaction
                    .take()
                    .ok_or("restore has no active transaction")?;
                if !active.restoring
                    || message["request_id"].as_u64() != Some(active.object_id)
                    || active.bytes.len() != active.expected_bytes
                    || hex(&Sha256::digest(&active.bytes)) != active.digest
                {
                    return Err("drawing restore length, identity or digest mismatch".into());
                }
                let document: Value = serde_json::from_slice(&active.bytes)
                    .map_err(|error| format!("invalid recovery document: {error}"))?;
                let objects: Vec<InkObject> =
                    serde_json::from_value(document["objects"].clone())
                        .map_err(|error| format!("invalid recovery objects: {error}"))?;
                let canvas: Canvas = serde_json::from_value(document["canvas"].clone())
                    .map_err(|error| format!("invalid recovery canvas: {error}"))?;
                let mut restored = scene.lock().map_err(|_| "scene lock poisoned")?.clone();
                restored.restore(objects, canvas)?;
                input.force_draw()?;
                crate::source::apply_zoom(&source, crate::zoom::Viewport::default())?;
                let mut selected = source.lock().map_err(|_| "source lock poisoned")?;
                selected.epoch = selected
                    .epoch
                    .checked_add(1)
                    .ok_or("source epoch exhausted")?;
                *scene.lock().map_err(|_| "scene lock poisoned")? = restored;
                source_ready = false;
                return Ok(Some(json!({
                    "type":"restore_ack", "request_id": active.object_id,
                    "selected": selected.metadata()
                })));
            }
            if command == "set_zoom" {
                let zoom_started = Instant::now();
                let request_id = message["request_id"]
                    .as_u64()
                    .ok_or("zoom request id missing")?;
                let view: crate::zoom::Viewport =
                    serde_json::from_value(message["zoom"].clone())
                        .map_err(|e| format!("invalid zoom viewport: {e}"))?;
                view.validate()?;
                input.release_all()?;
                scene
                    .lock()
                    .map_err(|_| "scene lock poisoned")?
                    .clear_laser();
                transaction = None;
                crate::source::apply_zoom(&source, view)?;
                let mut selected = source.lock().map_err(|_| "source lock poisoned")?;
                selected.epoch = selected
                    .epoch
                    .checked_add(1)
                    .ok_or("source epoch exhausted")?;
                zoom_timing = Some((selected.epoch, zoom_started));
                source_ready = false;
                return Ok(Some(json!({
                    "type": "zoom_selected", "request_id": request_id,
                    "selected": selected.metadata()
                })));
            }
            process_command(&message, &scene, input, &mut transaction)
        })();
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                write_json_line(reader.get_mut(), &command_error(&message, &error))?;
                continue;
            }
        };
        if let Some(response) = response {
            write_json_line(reader.get_mut(), &response)?;
        }
    }
}

fn require_ready(command: &str, ready: bool) -> Result<(), String> {
    if !ready
        && matches!(
            command,
            "pointer"
                | "commit"
                | "begin_commit"
                | "commit_chunk"
                | "finish_commit"
                | "select_object"
                | "transform_object"
                | "set_canvas"
                | "laser"
                | "undo"
                | "redo"
                | "clear"
                | "clear_display"
                | "set_mode"
                | "set_zoom"
                | "begin_restore"
                | "restore_chunk"
                | "finish_restore"
        )
    {
        return Err("input suspended until the selected video geometry is acknowledged".into());
    }
    Ok(())
}

fn command_error(message: &Value, error: &str) -> Value {
    let mut response = json!({"type":"error","message":error,"request_id":message["request_id"],"source_epoch":message["source_epoch"]});
    if let Some(id) = message["object_id"]
        .as_u64()
        .or_else(|| message["object"]["id"].as_u64())
    {
        response["object_id"] = json!(id);
    }
    response
}

fn source_ready_ack(message: &Value, canvas: Canvas, mode: Mode) -> Value {
    json!({"type":"source_ready_ack","source_epoch":message["source_epoch"],"canvas":canvas,"mode":mode.as_str()})
}

fn process_command(
    message: &Value,
    scene: &Arc<Mutex<Scene>>,
    input: &mut WindowsInput,
    transaction: &mut Option<CommitTransaction>,
) -> Result<Option<Value>, String> {
    let command = message.get("type").and_then(Value::as_str).unwrap_or("");
    if matches!(
        command,
        "commit"
            | "begin_commit"
            | "commit_chunk"
            | "finish_commit"
            | "select_object"
            | "transform_object"
            | "laser"
    ) && input.mode() != Mode::Draw
    {
        return Err("editing and laser require Draw mode".into());
    }
    let response = match command {
        "select_object" | "transform_object" | "set_canvas" => {
            let request_id = message["request_id"]
                .as_u64()
                .ok_or("editor request id missing")?;
            let mut scene = scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned")?;
            let selection = match command {
                "select_object" => scene.select(command_point(message)?, input.aspect())?,
                "transform_object" => {
                    let id = message["object_id"].as_u64().ok_or("object id missing")?;
                    let transform: Transform = serde_json::from_value(message["transform"].clone())
                        .map_err(|e| format!("invalid transform: {e}"))?;
                    Some(scene.transform_object(id, transform, input.aspect())?)
                }
                _ => {
                    let canvas = Canvas {
                        blackboard: message["blackboard"]
                            .as_bool()
                            .ok_or("blackboard flag missing")?,
                        grid: message["grid"].as_bool().ok_or("grid flag missing")?,
                    };
                    input.release_all()?;
                    if canvas.blackboard {
                        input.force_draw()?;
                    }
                    scene.set_canvas(canvas)?;
                    None
                }
            };
            Ok(Some(
                json!({"type":"editor_ack","request_id":request_id,"source_epoch":message["source_epoch"],"selection":selection,"canvas":scene.canvas(),"mode":input.mode().as_str()}),
            ))
        }
        "laser" => {
            let active = message["active"]
                .as_bool()
                .ok_or("laser active flag missing")?;
            let mut scene = scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned")?;
            if active {
                let rgba = if message["rgba"].is_null() {
                    [255, 0, 0, 255]
                } else {
                    serde_json::from_value(message["rgba"].clone())
                        .map_err(|e| format!("invalid laser color: {e}"))?
                };
                scene.set_laser(true, command_point(message)?, rgba)?;
            } else {
                scene.clear_laser();
            }
            Ok(None)
        }
        "commit" => {
            let object: InkObject = serde_json::from_value(
                message
                    .get("object")
                    .cloned()
                    .ok_or("commit is missing its object")?,
            )
            .map_err(|error| format!("invalid annotation object: {error}"))?;
            let id = object.id;
            let mut scene = scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned")?;
            scene.commit(object)?;
            Ok(Some(json!({"type": "commit_ack", "object_id": id,
                "source_epoch": message["source_epoch"], "revision": scene.revision()})))
        }
        "begin_commit" | "begin_restore" => {
            if transaction.is_some() {
                return Err("another commit transaction is already active".into());
            }
            let restoring = command == "begin_restore";
            let object_id = message
                .get(if restoring { "request_id" } else { "object_id" })
                .and_then(Value::as_u64)
                .ok_or("commit transaction has an invalid object id")?;
            let expected_bytes = message
                .get("total_bytes")
                .and_then(Value::as_u64)
                .and_then(|length| usize::try_from(length).ok())
                .filter(|length| {
                    (1..=if restoring {
                        16 * 1024 * 1024
                    } else {
                        MAX_COMMIT_BYTES
                    })
                        .contains(length)
                })
                .ok_or("commit transaction size is out of range")?;
            let digest = message
                .get("digest")
                .and_then(Value::as_str)
                .filter(|value| {
                    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                })
                .ok_or("commit transaction has an invalid SHA-256 digest")?
                .to_ascii_lowercase();
            *transaction = Some(CommitTransaction {
                object_id,
                digest,
                bytes: Vec::with_capacity(expected_bytes),
                expected_bytes,
                expires: Instant::now()
                    + if restoring {
                        Duration::from_secs(120)
                    } else {
                        COMMIT_LIFETIME
                    },
                restoring,
            });
            Ok(Some(json!({"type": "ack"})))
        }
        "commit_chunk" | "restore_chunk" => {
            let active = transaction
                .as_mut()
                .ok_or("commit chunk has no active transaction")?;
            let object_id = message
                .get(if command == "restore_chunk" {
                    "request_id"
                } else {
                    "object_id"
                })
                .and_then(Value::as_u64)
                .ok_or("commit chunk has an invalid object id")?;
            let offset = message
                .get("offset")
                .and_then(Value::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or("commit chunk has an invalid byte offset")?;
            let encoded = message
                .get("data")
                .and_then(Value::as_str)
                .ok_or("commit chunk is missing data")?;
            let chunk = STANDARD
                .decode(encoded)
                .map_err(|error| format!("invalid base64 commit chunk: {error}"))?;
            let invalid = active.object_id != object_id
                || active.restoring != (command == "restore_chunk")
                || active.bytes.len() != offset
                || chunk.is_empty()
                || chunk.len() > MAX_COMMIT_CHUNK_BYTES
                || active.bytes.len().saturating_add(chunk.len()) > active.expected_bytes;
            if invalid {
                *transaction = None;
                return Err("commit chunk is out of order or exceeds its bounds".into());
            }
            active.bytes.extend_from_slice(&chunk);
            Ok(Some(json!({"type": "ack"})))
        }
        "finish_commit" => {
            let active = transaction
                .take()
                .ok_or("commit finish has no active transaction")?;
            let object_id = message
                .get("object_id")
                .and_then(Value::as_u64)
                .ok_or("commit finish has an invalid object id")?;
            if active.object_id != object_id
                || active.restoring
                || active.bytes.len() != active.expected_bytes
                || hex(&Sha256::digest(&active.bytes)) != active.digest
            {
                return Err("commit transaction length or digest did not match".into());
            }
            let object: InkObject = serde_json::from_slice(&active.bytes)
                .map_err(|error| format!("invalid committed object: {error}"))?;
            if object.id != object_id {
                return Err("commit transaction object id did not match".into());
            }
            let mut scene = scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned")?;
            scene.commit(object)?;
            Ok(Some(json!({"type": "commit_ack", "object_id": object_id,
                "source_epoch": message["source_epoch"], "revision": scene.revision()})))
        }
        "cancel_commit" => {
            *transaction = None;
            Ok(Some(json!({"type": "ack"})))
        }
        "set_mode" => {
            let mode = message
                .get("mode")
                .and_then(Value::as_str)
                .ok_or("mode change is missing its mode")?;
            let mode = Mode::parse(mode)?;
            if mode == Mode::Pointer
                && scene
                    .lock()
                    .map_err(|_| "scene lock poisoned")?
                    .canvas()
                    .blackboard
            {
                input.force_draw()?;
                return Err("Pointer mode is unavailable on the blackboard".into());
            }
            let request_id = message
                .get("request_id")
                .and_then(Value::as_u64)
                .ok_or("mode change has an invalid request id")?;
            input.set_mode(mode, request_id)?;
            scene
                .lock()
                .map_err(|_| "scene lock poisoned")?
                .clear_laser();
            Ok(Some(json!({
                "type": "mode_ack",
                "mode": mode.as_str(),
                "request_id": request_id
            })))
        }
        "pointer" => {
            let action = message
                .get("action")
                .and_then(Value::as_str)
                .ok_or("pointer command is missing its action")?;
            let generation = message
                .get("generation")
                .and_then(Value::as_u64)
                .ok_or("pointer command has an invalid generation")?;
            let x = message
                .get("x")
                .and_then(Value::as_f64)
                .ok_or("pointer command has an invalid x coordinate")? as f32;
            let y = message
                .get("y")
                .and_then(Value::as_f64)
                .ok_or("pointer command has an invalid y coordinate")? as f32;
            let button = message.get("button").and_then(Value::as_str).unwrap_or("");
            let ack = input.pointer(action, generation, x, y, button)?;
            Ok(ack.then(|| json!({"type": "ack"})))
        }
        "undo" => {
            input.release_all()?;
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .undo();
            Ok(Some(json!({"type": "ack"})))
        }
        "redo" => {
            input.release_all()?;
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .redo();
            Ok(Some(json!({"type": "ack"})))
        }
        "clear" => {
            input.release_all()?;
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .clear()?;
            Ok(Some(json!({"type": "ack"})))
        }
        "clear_display" => {
            let request_id = message
                .get("request_id")
                .and_then(Value::as_u64)
                .filter(|request_id| *request_id > 0)
                .ok_or("display clear has an invalid request id")?;
            input.release_all()?;
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .clear()?;
            Ok(Some(json!({
                "type": "clear_ack",
                "request_id": request_id
            })))
        }
        "heartbeat" => Ok(None),
        _ => Err(format!("unknown command type {command:?}")),
    }?;
    if matches!(command, "undo" | "redo" | "clear" | "clear_display") {
        let scene = scene
            .lock()
            .map_err(|_| "canonical scene lock is poisoned")?;
        if scene.canvas().blackboard {
            input.force_draw()?;
        }
        return Ok(response.map(|mut response| {
            response["request_id"] = message["request_id"].clone();
            response["source_epoch"] = message["source_epoch"].clone();
            response["canvas"] = json!(scene.canvas());
            response["mode"] = json!(input.mode().as_str());
            response
        }));
    }
    Ok(response)
}

fn command_point(message: &Value) -> Result<Point, String> {
    Ok(Point {
        x: message["x"].as_f64().ok_or("x coordinate missing")? as f32,
        y: message["y"].as_f64().ok_or("y coordinate missing")? as f32,
    })
}
pub(crate) fn read_json_line<R: BufRead>(reader: &mut R) -> io::Result<Value> {
    let mut bytes = Vec::with_capacity(256);
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            if bytes.is_empty() {
                return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
            }
            break;
        }
        let count = available
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(available.len(), |position| position + 1);
        if bytes.len().saturating_add(count) > MAX_FRAME_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message exceeds 64 KiB",
            ));
        }
        let complete = available[count - 1] == b'\n';
        bytes.extend_from_slice(&available[..count]);
        reader.consume(count);
        if complete {
            break;
        }
    }
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
    }
    if bytes.last() == Some(&b'\r') {
        bytes.pop();
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

pub(crate) fn write_json_line<W: Write>(writer: &mut W, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value)
        .map_err(|error| format!("serialize session response: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("write session response: {error}"))
}

pub(crate) fn is_allowed_bind(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_loopback() || address.is_private(),
        IpAddr::V6(address) => {
            address.is_loopback() || address.is_unique_local() || address.is_unicast_link_local()
        }
    }
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
#[path = "../tests/unit/session.rs"]
mod tests;
