use crate::{
    pairing_window::{self, PairingWindow},
    windows_input::{Mode, OutputBounds, WindowsInput},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use dd_scene::{InkObject, Scene};
use rcgen::generate_simple_self_signed;
use rustls::{
    ServerConfig, ServerConnection, StreamOwned,
    pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::{self, BufRead, BufReader, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    path::PathBuf,
    sync::{Arc, Mutex},
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
}

pub fn run_server(
    address: IpAddr,
    port: u16,
    scene: Arc<Mutex<Scene>>,
    output: OutputBounds,
    pairing_info: Option<PathBuf>,
) -> Result<(), String> {
    if !is_allowed_bind(address) {
        return Err(format!(
            "refusing non-loopback, non-private listener address {address}"
        ));
    }

    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| "a Rustls crypto provider is already installed".to_owned())?;
    let certified = generate_simple_self_signed(vec!["drawdock".to_owned()])
        .map_err(|error| format!("create temporary host TLS identity: {error}"))?;
    let fingerprint = hex(&Sha256::digest(certified.cert.der().as_ref()));
    let mut token_bytes = [0u8; 16];
    getrandom::fill(&mut token_bytes)
        .map_err(|error| format!("generate invitation token: {error}"))?;
    let token = hex(&token_bytes);
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
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(certified.key_pair.serialize_der()));
    let config = Arc::new(
        ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![certified.cert.der().clone()], key)
            .map_err(|error| format!("configure host TLS identity: {error}"))?,
    );
    let mut input = WindowsInput::new(output)?;
    let listener = TcpListener::bind(SocketAddr::new(address, port))
        .map_err(|error| format!("bind TLS listener {address}:{port}: {error}"))?;
    listener
        .set_nonblocking(true)
        .map_err(|error| format!("configure TLS listener: {error}"))?;

    let expires = Instant::now() + TOKEN_LIFETIME;
    let invitation = invitation_uri(address, port, &fingerprint, &token);
    let pairing = PairingWindow::open(&invitation, &format!("{address}:{port}"), TOKEN_LIFETIME)?;
    println!("Authenticated TLS/TCP listener: {address}:{port}");
    println!("Scan the QR invitation with DrawDock, then approve your tablet in the PC dialog.");
    println!("Invitation expires in 5 minutes. Closing the QR window cancels pairing.");
    if !address.is_loopback() {
        println!(
            "WARNING: listening on the explicitly selected private interface. \
             No firewall rule was changed."
        );
    }
    let mut attempts = 0;
    while Instant::now() < expires && attempts < MAX_AUTH_ATTEMPTS && pairing.is_open() {
        let (tcp, peer) = match listener.accept() {
            Ok(connection) => connection,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(50));
                continue;
            }
            Err(error) => return Err(format!("accept TLS connection: {error}")),
        };
        tcp.set_nonblocking(false)
            .map_err(|error| format!("configure accepted connection from {peer}: {error}"))?;
        tcp.set_nodelay(true)
            .map_err(|error| format!("set TCP_NODELAY for {peer}: {error}"))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| format!("set pairing read timeout: {error}"))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|error| format!("set pairing write timeout: {error}"))?;
        let tls = match ServerConnection::new(Arc::clone(&config)) {
            Ok(connection) => StreamOwned::new(connection, tcp),
            Err(error) => {
                attempts += 1;
                eprintln!("Rejected TLS session from {peer}: {error}");
                continue;
            }
        };
        let mut reader = BufReader::new(tls);
        let invitation = match read_json_line(&mut reader) {
            Ok(message) => message,
            Err(error) => {
                attempts += 1;
                eprintln!("Rejected invalid pairing request from {peer}: {error}");
                continue;
            }
        };
        if !valid_invitation(&invitation, &token, expires) {
            attempts += 1;
            eprintln!("Rejected unauthorized pairing request from {peer}");
            continue;
        }

        let client_name = invitation
            .get("client")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 64)
            .unwrap_or("Android tablet")
            .chars()
            .filter(|character| !character.is_control())
            .collect::<String>();
        if Instant::now() >= expires || !pairing.is_open() {
            return Err("pairing invitation expired or was cancelled".into());
        }
        println!("Pair request from {client_name} at {peer}.");
        if !pairing_window::approve(&client_name, &peer.to_string())? {
            eprintln!("Pairing denied by local user.");
            break;
        }
        if Instant::now() >= expires || !pairing.is_open() {
            return Err("pairing invitation expired or was cancelled before local approval".into());
        }

        drop(pairing);
        let stream = reader.get_mut();
        write_json_line(stream, &json!({"type": "paired", "version": 1}))?;
        stream
            .get_ref()
            .set_read_timeout(Some(READ_LEASE))
            .map_err(|error| format!("set session lease timeout: {error}"))?;
        println!("Tablet paired. Disconnect or press Ctrl+C to stop the host.");
        let result = serve_authenticated(&mut reader, Arc::clone(&scene), &mut input);
        let release_result = input.release_all();
        let clear_result = scene
            .lock()
            .map_err(|_| "canonical scene lock is poisoned".to_owned())
            .and_then(|mut state| state.clear());
        println!("Remote session ended; host annotations were cleared.");
        release_result?;
        clear_result?;
        return result;
    }
    Err("pairing cancelled, expired or the attempt limit was reached".into())
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
) -> Result<(), String> {
    let mut window_start = Instant::now();
    let mut message_count = 0;
    let mut transaction: Option<CommitTransaction> = None;
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
        let response = process_command(&message, &scene, input, &mut transaction);
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                write_json_line(
                    reader.get_mut(),
                    &json!({"type": "error", "message": error}),
                )?;
                continue;
            }
        };
        if let Some(response) = response {
            write_json_line(reader.get_mut(), &response)?;
        }
    }
}

fn process_command(
    message: &Value,
    scene: &Arc<Mutex<Scene>>,
    input: &mut WindowsInput,
    transaction: &mut Option<CommitTransaction>,
) -> Result<Option<Value>, String> {
    let command = message.get("type").and_then(Value::as_str).unwrap_or("");
    match command {
        "commit" => {
            let object: InkObject = serde_json::from_value(
                message
                    .get("object")
                    .cloned()
                    .ok_or("commit is missing its object")?,
            )
            .map_err(|error| format!("invalid annotation object: {error}"))?;
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .commit(object)?;
            Ok(Some(json!({"type": "ack"})))
        }
        "begin_commit" => {
            if transaction.is_some() {
                return Err("another commit transaction is already active".into());
            }
            let object_id = message
                .get("object_id")
                .and_then(Value::as_u64)
                .ok_or("commit transaction has an invalid object id")?;
            let expected_bytes = message
                .get("total_bytes")
                .and_then(Value::as_u64)
                .and_then(|length| usize::try_from(length).ok())
                .filter(|length| (1..=MAX_COMMIT_BYTES).contains(length))
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
                expires: Instant::now() + COMMIT_LIFETIME,
            });
            Ok(Some(json!({"type": "ack"})))
        }
        "commit_chunk" => {
            let active = transaction
                .as_mut()
                .ok_or("commit chunk has no active transaction")?;
            let object_id = message
                .get("object_id")
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
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .commit(object)?;
            Ok(Some(json!({"type": "ack"})))
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
            let request_id = message
                .get("request_id")
                .and_then(Value::as_u64)
                .ok_or("mode change has an invalid request id")?;
            input.set_mode(mode, request_id)?;
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
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .undo();
            Ok(Some(json!({"type": "ack"})))
        }
        "redo" => {
            scene
                .lock()
                .map_err(|_| "canonical scene lock is poisoned".to_owned())?
                .redo();
            Ok(Some(json!({"type": "ack"})))
        }
        "clear" => {
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
    }
}

fn read_json_line<R: BufRead>(reader: &mut R) -> io::Result<Value> {
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

fn write_json_line<W: Write>(writer: &mut W, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value)
        .map_err(|error| format!("serialize session response: {error}"))?;
    writer
        .write_all(b"\n")
        .and_then(|()| writer.flush())
        .map_err(|error| format!("write session response: {error}"))
}

fn is_allowed_bind(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => address.is_loopback() || address.is_private(),
        IpAddr::V6(address) => {
            address.is_loopback() || address.is_unique_local() || address.is_unicast_link_local()
        }
    }
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
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
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn only_loopback_and_private_addresses_are_allowed() {
        assert!(is_allowed_bind(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(is_allowed_bind(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 4))));
        assert!(!is_allowed_bind(IpAddr::V4(Ipv4Addr::new(8, 8, 8, 8))));
        assert!(!is_allowed_bind(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
        assert!(is_allowed_bind(IpAddr::V6(Ipv6Addr::LOCALHOST)));
        assert!(!is_allowed_bind(IpAddr::V6(Ipv6Addr::UNSPECIFIED)));
    }

    #[test]
    fn pairing_tokens_are_compared_without_early_exit() {
        assert!(constant_time_eq(b"same", b"same"));
        assert!(!constant_time_eq(b"same", b"diff"));
        assert!(!constant_time_eq(b"short", b"longer"));
    }

    #[test]
    fn pairing_requires_correct_unexpired_invitation_before_local_approval() {
        let token = "abcdef0123456789abcdef0123456789";
        let expires = Instant::now() + TOKEN_LIFETIME;
        assert!(valid_invitation(
            &json!({"type": "pair", "token": token}),
            token,
            expires,
        ));
        for request in [
            json!({"type": "pair", "token": "123456"}),
            json!({"type": "pair"}),
            json!({"type": "commit", "token": token}),
        ] {
            assert!(!valid_invitation(&request, token, expires));
        }
        assert!(!valid_invitation(
            &json!({"type": "pair", "token": token}),
            token,
            Instant::now() - Duration::from_secs(1),
        ));
    }

    #[test]
    fn oversized_json_line_is_rejected_before_decode() {
        let mut reader = io::Cursor::new(vec![b'x'; MAX_FRAME_BYTES + 1]);
        let error = read_json_line(&mut reader).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn clear_display_command_clears_the_shared_scene_and_acknowledges() {
        let scene = Arc::new(Mutex::new(Scene::demo()));
        let mut input = WindowsInput::new(OutputBounds {
            left: 0,
            top: 0,
            width: 100,
            height: 100,
        })
        .unwrap();
        let mut transaction = None;
        let response = process_command(
            &json!({"type": "clear_display", "request_id": 7}),
            &scene,
            &mut input,
            &mut transaction,
        )
        .unwrap()
        .unwrap();

        assert!(scene.lock().unwrap().objects().is_empty());
        assert_eq!(response["type"], "clear_ack");
        assert_eq!(response["request_id"], 7);
    }
}
