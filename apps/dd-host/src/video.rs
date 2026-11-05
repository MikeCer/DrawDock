use crate::source::{SelectedSource, SharedSource};
use rustls::{ServerConfig, ServerConnection, StreamOwned};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const MAX_ACCESS_UNIT: usize = 8 * 1024 * 1024;
const FPS: u32 = 30;
const FRAME_HANDOFF_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Clone, Copy, Debug)]
pub enum VideoEncoder {
    Software,
    Nvenc,
}

impl VideoEncoder {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "software" => Ok(Self::Software),
            "nvenc" => Ok(Self::Nvenc),
            _ => Err("video encoder must be software or nvenc".into()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Software => "libx264",
            Self::Nvenc => "h264_nvenc",
        }
    }

    fn configure(self, command: &mut Command) {
        command.args([
            "-c:v",
            self.name(),
            "-profile:v",
            "high",
            "-level:v",
            "5.0",
            "-pix_fmt",
            "yuv420p",
            "-colorspace",
            "bt709",
            "-color_primaries",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_range",
            "tv",
            "-maxrate",
            "24M",
            "-bufsize",
            "2M",
            "-g",
            "30",
            "-bf",
            "0",
        ]);
        match self {
            Self::Software => {
                command.args([
                    "-preset",
                    "veryfast",
                    "-tune",
                    "zerolatency",
                    "-crf",
                    "18",
                    "-threads",
                    "4",
                    "-x264-params",
                    "aud=1:repeat-headers=1:keyint=30:min-keyint=30:scenecut=0:bframes=0",
                ]);
            }
            Self::Nvenc => {
                command.args([
                    "-preset",
                    "p5",
                    "-tune",
                    "ull",
                    "-rc",
                    "vbr",
                    "-cq",
                    "18",
                    "-b:v",
                    "12M",
                    "-rc-lookahead",
                    "0",
                    "-zerolatency",
                    "1",
                    "-delay",
                    "0",
                    "-spatial-aq",
                    "1",
                    "-aud",
                    "1",
                    "-forced-idr",
                    "1",
                ]);
            }
        }
    }
}

pub struct VideoService {
    stopped: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl VideoService {
    pub fn start(
        listener: TcpListener,
        config: Arc<ServerConfig>,
        token: String,
        source: SharedSource,
        encoder: VideoEncoder,
    ) -> Result<Self, String> {
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("configure cancellable video listener: {error}"))?;
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let running = Arc::new(AtomicBool::new(true));
        let live = Arc::clone(&running);
        let thread = thread::Builder::new()
            .name("drawdock-video".into())
            .spawn(move || {
                match accept_video(listener, config, &token, source, &stop, encoder) {
                    Err(error) if !stop.load(Ordering::Acquire) => {
                        eprintln!("DrawDock video stopped: {error}")
                    }
                    _ => {}
                }
                live.store(false, Ordering::Release);
            })
            .map_err(|error| format!("start video service: {error}"))?;
        Ok(Self {
            stopped,
            running,
            thread: Some(thread),
        })
    }

    pub fn running(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.running)
    }
}

impl Drop for VideoService {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if self
            .thread
            .take()
            .is_some_and(|thread| thread.join().is_err())
        {
            eprintln!("DrawDock video thread panicked.");
        }
    }
}

fn accept_video(
    listener: TcpListener,
    config: Arc<ServerConfig>,
    token: &str,
    source: SharedSource,
    stopped: &AtomicBool,
    encoder: VideoEncoder,
) -> Result<(), String> {
    let expires = Instant::now() + Duration::from_secs(30);
    let mut attempts = 0;
    while !stopped.load(Ordering::Acquire) && Instant::now() < expires && attempts < 3 {
        let (tcp, _) = match listener.accept() {
            Ok(value) => value,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(25));
                continue;
            }
            Err(error) => return Err(format!("accept video: {error}")),
        };
        attempts += 1;
        tcp.set_nonblocking(false)
            .map_err(|error| error.to_string())?;
        tcp.set_read_timeout(Some(Duration::from_secs(1)))
            .map_err(|error| error.to_string())?;
        tcp.set_write_timeout(Some(Duration::from_millis(500)))
            .map_err(|error| error.to_string())?;
        tcp.set_nodelay(true).map_err(|error| error.to_string())?;
        let tls = ServerConnection::new(Arc::clone(&config)).map_err(|error| error.to_string())?;
        let mut reader = BufReader::new(StreamOwned::new(tls, tcp));
        let request = match crate::session::read_json_line(&mut reader) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Rejected video handshake: {error}");
                continue;
            }
        };
        if request["type"] != "video"
            || !crate::session::constant_time_eq(
                request["token"].as_str().unwrap_or("").as_bytes(),
                token.as_bytes(),
            )
        {
            eprintln!("Rejected unauthorized video connection.");
            continue;
        }
        if request["version"] != 2 {
            return Err("incompatible video protocol; update the DrawDock Android app".into());
        }
        let result = stream_video(reader.get_mut(), source, stopped, encoder);
        if let Err(error) = &result {
            let _ = write_packet(reader.get_mut(), 3, 0, error.as_bytes());
        }
        return result;
    }
    if stopped.load(Ordering::Acquire) {
        Ok(())
    } else {
        Err("video connection expired or attempt limit reached".into())
    }
}

pub fn encoded_size(width: i32, height: i32) -> (u32, u32) {
    let scale = (2560.0 / f64::from(width))
        .min(1440.0 / f64::from(height))
        .min(1.0);
    (
        ((f64::from(width) * scale) as u32 & !1).max(2),
        ((f64::from(height) * scale) as u32 & !1).max(2),
    )
}

pub fn probe(
    output: crate::windows_overlay::Output,
    frames: usize,
    encoder: VideoEncoder,
) -> Result<(), String> {
    let selected = SelectedSource::new(output);
    let start = Instant::now();
    let capture = Capture::start(&selected, encoder)?;
    let mut total = 0;
    for _ in 0..frames {
        let frame = capture
            .frames
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| format!("capture probe: {error}"))??;
        total += frame.bytes.len();
    }
    let (width, height) = encoded_size(selected.output.width(), selected.output.height());
    println!(
        "Captured {frames} H.264 access units ({total} bytes), {width}x{height}, {}, {:.2}s including startup, from {}. No recording saved.",
        encoder.name(),
        start.elapsed().as_secs_f64(),
        selected.output.name
    );
    Ok(())
}

struct Capture {
    child: Child,
    stopped: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
    frames: mpsc::Receiver<Result<EncodedFrame, String>>,
}

struct EncodedFrame {
    capture_us: u64,
    bytes: Vec<u8>,
}

impl Capture {
    fn start(source: &SelectedSource, encoder: VideoEncoder) -> Result<Self, String> {
        let output = &source.output;
        let (width, height) = encoded_size(output.width(), output.height());
        let executable = std::env::var_os("DRAWDOCK_FFMPEG").unwrap_or_else(|| {
            let installed = r"C:\ProgramData\chocolatey\lib\ffmpeg\tools\ffmpeg\bin\ffmpeg.exe";
            if std::path::Path::new(installed).is_file() {
                installed.into()
            } else {
                "ffmpeg".into()
            }
        });
        let mut command = Command::new(executable);
        command.args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-copyts",
            "-f",
            "gdigrab",
            "-draw_mouse",
            "1",
            "-framerate",
            "30",
            "-offset_x",
            &output.left.to_string(),
            "-offset_y",
            &output.top.to_string(),
            "-video_size",
            &format!("{}x{}", output.width(), output.height()),
            "-i",
            "desktop",
            "-an",
            "-map",
            "0:v",
            "-vf",
            &format!("scale={width}:{height}:flags=lanczos:out_color_matrix=bt709:out_range=tv"),
        ]);
        encoder.configure(&mut command);
        let mut child = command
            .args([
                "-fps_mode",
                "passthrough",
                "-enc_time_base",
                "1:1000000",
                "-f",
                "tee",
                "[f=framecrc:flush_packets=1]pipe:2|[f=h264:flush_packets=1]pipe:1",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x0800_0000)
            .spawn()
            .map_err(|error| {
                format!(
                    "start native FFmpeg gdigrab/{} (or set DRAWDOCK_FFMPEG): {error}",
                    encoder.name()
                )
            })?;
        let stdout = child.stdout.take().ok_or("FFmpeg stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .ok_or("FFmpeg packet metadata is unavailable")?;
        if std::env::var("DRAWDOCK_PERF_TRACE").as_deref() == Ok("1") {
            println!("PERF capture_started epoch={}", source.epoch);
        }
        let (tx, frames) = mpsc::sync_channel(1);
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let reader = match thread::Builder::new()
            .name("drawdock-h264-reader".into())
            .spawn(move || {
                let mut reader = BufReader::new(stdout);
                let mut metadata = BufReader::new(stderr);
                let result = (|| -> Result<(), String> {
                    let mut microsecond_timebase = false;
                    let mut last_pts = 0;
                    loop {
                        let mut line = Vec::new();
                        let count = metadata
                            .by_ref()
                            .take(1025)
                            .read_until(b'\n', &mut line)
                            .map_err(|error| format!("read FFmpeg packet metadata: {error}"))?;
                        if count == 0 {
                            return Err("native capture/encoder exited".into());
                        }
                        if count > 1024 || !line.ends_with(b"\n") {
                            return Err(
                                "FFmpeg packet metadata line is too long or incomplete".into()
                            );
                        }
                        let line = std::str::from_utf8(&line)
                            .map_err(|_| "invalid FFmpeg metadata encoding")?
                            .trim();
                        if line.starts_with('#') {
                            if line.starts_with("#tb ") {
                                if line != "#tb 0: 1/1000000" {
                                    return Err(
                                        "FFmpeg did not preserve microsecond capture timestamps"
                                            .into(),
                                    );
                                }
                                microsecond_timebase = true;
                            }
                            continue;
                        }
                        if !microsecond_timebase {
                            return Err(format!("FFmpeg failed before packet timebase: {line}"));
                        }
                        let (capture_us, size) = parse_packet_metadata(line)?;
                        if capture_us <= last_pts {
                            return Err("FFmpeg capture timestamps did not increase".into());
                        }
                        last_pts = capture_us;
                        let mut bytes = vec![0; size];
                        reader
                            .read_exact(&mut bytes)
                            .map_err(|error| format!("read timestamped H.264 packet: {error}"))?;
                        if !bytes.starts_with(&[0, 0, 0, 1]) && !bytes.starts_with(&[0, 0, 1]) {
                            return Err("FFmpeg packet is not Annex-B H.264".into());
                        }
                        send_frame(&tx, EncodedFrame { capture_us, bytes }, &stop)?;
                    }
                })();
                if let Err(error) = result {
                    if !stop.load(Ordering::Acquire) {
                        eprintln!("Capture reader stopped: {error}");
                    }
                    let _ = tx.try_send(Err(error));
                }
            }) {
            Ok(reader) => reader,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("start capture reader: {error}"));
            }
        };
        Ok(Self {
            child,
            stopped,
            reader: Some(reader),
            frames,
        })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        if let Err(error) = self.child.kill() {
            eprintln!("Stop capture process: {error}");
        }
        if let Err(error) = self.child.wait() {
            eprintln!("Wait for capture process: {error}");
        }
        if self
            .reader
            .take()
            .is_some_and(|reader| reader.join().is_err())
        {
            eprintln!("Capture reader thread panicked.");
        }
    }
}

fn send_frame<T>(
    sender: &mpsc::SyncSender<Result<T, String>>,
    frame: T,
    stopped: &AtomicBool,
) -> Result<(), String> {
    let deadline = Instant::now() + FRAME_HANDOFF_TIMEOUT;
    let mut pending = Ok(frame);
    loop {
        if stopped.load(Ordering::Acquire) {
            return Err("capture stopped".into());
        }
        match sender.try_send(pending) {
            Ok(()) => return Ok(()),
            Err(mpsc::TrySendError::Disconnected(_)) => return Err("video receiver closed".into()),
            Err(mpsc::TrySendError::Full(frame)) => pending = frame,
        }
        if Instant::now() >= deadline {
            return Err("encoded video handoff exceeded 500 ms; reconnect from a keyframe".into());
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn stream_video(
    stream: &mut StreamOwned<ServerConnection, TcpStream>,
    source: SharedSource,
    stopped: &AtomicBool,
    encoder: VideoEncoder,
) -> Result<(), String> {
    let mut frame_id = 0u64;
    source
        .lock()
        .map_err(|_| "source lock poisoned")?
        .restart_video_timeline();
    while !stopped.load(Ordering::Acquire) {
        let selected = source.lock().map_err(|_| "source lock poisoned")?.clone();
        let (width, height) = encoded_size(selected.output.width(), selected.output.height());
        let capture = Capture::start(&selected, encoder)?;
        let config = serde_json::to_vec(&json!({
            "codec": "video/avc", "width": width, "height": height, "fps": FPS,
            "source": selected.output.name, "annotation_capture_mode": "included",
            "encoder": encoder.name(), "color_space": "bt709",
            "frame_metadata": 1,
        }))
        .map_err(|error| error.to_string())?;
        write_packet(stream, 1, selected.epoch, &config)?;
        loop {
            if stopped.load(Ordering::Acquire) {
                return Ok(());
            }
            if source.lock().map_err(|_| "source lock poisoned")?.output != selected.output {
                break;
            }
            match capture.frames.recv_timeout(Duration::from_millis(100)) {
                Ok(frame) => {
                    let frame = frame?;
                    let source = source.lock().map_err(|_| "source lock poisoned")?;
                    if source.output != selected.output {
                        break;
                    }
                    let presentation = source.captured_presentation(frame.capture_us)?.cloned();
                    drop(source);
                    frame_id = frame_id
                        .checked_add(1)
                        .filter(|id| *id <= i64::MAX as u64 / 1_000_000)
                        .ok_or("video frame sequence exhausted")?;
                    let epoch = presentation.as_ref().map_or(0, |state| state.epoch);
                    let metadata = serde_json::to_vec(&json!({
                        "frame_id": frame_id, "presentation": presentation,
                    }))
                    .map_err(|error| error.to_string())?;
                    write_packet(stream, 4, epoch, &metadata)?;
                    write_packet(stream, 2, epoch, &frame.bytes)?;
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(
                        "capture ended or video queue overflowed; reconnect required".into(),
                    );
                }
            }
        }
    }
    Ok(())
}

fn write_packet<W: Write>(
    stream: &mut W,
    kind: u8,
    epoch: u64,
    bytes: &[u8],
) -> Result<(), String> {
    if bytes.is_empty() || bytes.len() > MAX_ACCESS_UNIT {
        return Err("invalid video packet length".into());
    }
    stream
        .write_all(&[kind])
        .and_then(|_| stream.write_all(&epoch.to_be_bytes()))
        .and_then(|_| stream.write_all(&(bytes.len() as u32).to_be_bytes()))
        .and_then(|_| stream.write_all(bytes))
        .and_then(|_| stream.flush())
        .map_err(|error| format!("write bounded video packet: {error}"))
}

fn parse_packet_metadata(line: &str) -> Result<(u64, usize), String> {
    let fields: Vec<_> = line.split(',').map(str::trim).collect();
    // Optional packet flags and encoder side-data follow the six fixed fields.
    if fields.len() < 6 || fields[0] != "0" || fields[1] != fields[2] {
        return Err(format!("invalid FFmpeg packet metadata: {line}"));
    }
    let pts = fields[2]
        .parse::<u64>()
        .map_err(|_| "invalid FFmpeg capture timestamp")?;
    let size = fields[4]
        .parse::<usize>()
        .ok()
        .filter(|size| (1..=MAX_ACCESS_UNIT).contains(size))
        .ok_or("FFmpeg packet size exceeds the H.264 limit")?;
    if pts == 0 {
        return Err("missing FFmpeg wall-clock capture timestamp".into());
    }
    Ok((pts, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encoded_pipe_bursts_preserve_frame_order_with_one_pending_slot() {
        let (sender, receiver) = mpsc::sync_channel(1);
        sender.try_send(Ok(vec![1])).unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let stop = Arc::clone(&stopped);
        let worker = thread::spawn(move || {
            send_frame(&sender, vec![2], &stop).unwrap();
            send_frame(&sender, vec![3], &stop).unwrap();
        });
        thread::sleep(Duration::from_millis(150));
        for expected in [1, 2, 3] {
            assert_eq!(
                receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap()
                    .unwrap(),
                vec![expected]
            );
        }
        worker.join().unwrap();
    }

    #[test]
    fn stalled_video_handoff_times_out_and_stop_never_waits_for_a_consumer() {
        let (sender, _receiver) = mpsc::sync_channel(1);
        sender.try_send(Ok(vec![1])).unwrap();
        assert!(
            send_frame(&sender, vec![2], &AtomicBool::new(false))
                .unwrap_err()
                .contains("500 ms")
        );
        assert!(
            send_frame(&sender, vec![2], &AtomicBool::new(true))
                .unwrap_err()
                .contains("stopped")
        );
    }

    #[test]
    #[ignore = "Captures two Windows displays with x264 and NVIDIA NVENC; run explicitly on this desktop"]
    fn native_tls_video_authenticates_and_switches_monitors() {
        use rustls::{
            ClientConfig, ClientConnection, RootCertStore,
            pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer, ServerName},
        };
        crate::windows_overlay::initialize_dpi().unwrap();
        let outputs = crate::windows_overlay::Output::enumerate().unwrap();
        assert!(
            outputs.len() >= 2,
            "Two real outputs are needed for this test"
        );
        let certificate = rcgen::generate_simple_self_signed(vec!["drawdock".into()]).unwrap();
        let server_config = Arc::new(
            ServerConfig::builder()
                .with_no_client_auth()
                .with_single_cert(
                    vec![certificate.cert.der().clone()],
                    PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                        certificate.key_pair.serialize_der(),
                    )),
                )
                .unwrap(),
        );
        let mut roots = RootCertStore::empty();
        roots.add(certificate.cert.der().clone()).unwrap();
        let client_config = Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        );
        for encoder in [VideoEncoder::Software, VideoEncoder::Nvenc] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let address = listener.local_addr().unwrap();
            let source = Arc::new(std::sync::Mutex::new(SelectedSource::new(
                outputs[0].clone(),
            )));
            let service = VideoService::start(
                listener,
                Arc::clone(&server_config),
                "test-only-video-token".into(),
                Arc::clone(&source),
                encoder,
            )
            .unwrap();
            let connect = |token: &str| {
                let tcp = TcpStream::connect(address).unwrap();
                tcp.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
                let tls = ClientConnection::new(
                    Arc::clone(&client_config),
                    ServerName::try_from("drawdock").unwrap(),
                )
                .unwrap();
                let mut stream = StreamOwned::new(tls, tcp);
                writeln!(
                    stream,
                    "{}",
                    json!({"type": "video", "version": 2, "token": token})
                )
                .unwrap();
                stream.flush().unwrap();
                stream
            };
            let mut denied = connect("wrong-token");
            match denied.read(&mut [0]) {
                Ok(0) => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::UnexpectedEof | std::io::ErrorKind::ConnectionReset
                    ) => {}
                other => panic!("Unauthorized video was not rejected: {other:?}"),
            }
            drop(denied);
            let mut stream = connect("test-only-video-token");
            let packet = |stream: &mut StreamOwned<ClientConnection, TcpStream>| {
                let mut header = [0; 13];
                stream.read_exact(&mut header).unwrap();
                let epoch = u64::from_be_bytes(header[1..9].try_into().unwrap());
                let size = u32::from_be_bytes(header[9..13].try_into().unwrap()) as usize;
                assert!((1..=MAX_ACCESS_UNIT).contains(&size));
                let mut payload = vec![0; size];
                stream.read_exact(&mut payload).unwrap();
                assert_ne!(
                    header[0],
                    3,
                    "Video error: {}",
                    String::from_utf8_lossy(&payload)
                );
                (header[0], epoch, payload)
            };
            let config = packet(&mut stream);
            assert_eq!((config.0, config.1), (1, 1));
            let metadata: serde_json::Value = serde_json::from_slice(&config.2).unwrap();
            assert_eq!(metadata["encoder"], encoder.name());
            let expected = encoded_size(outputs[0].width(), outputs[0].height());
            assert_eq!(metadata["width"], expected.0);
            assert_eq!(metadata["height"], expected.1);
            for index in 0..30 {
                assert_eq!(packet(&mut stream).0, 4);
                let frame = packet(&mut stream);
                assert_eq!(frame.0, 2);
                if index == 0 {
                    for nal_type in [7, 8, 5] {
                        assert!(
                            frame
                                .2
                                .windows(4)
                                .any(|bytes| bytes[..3] == [0, 0, 1] && bytes[3] & 31 == nal_type),
                            "First H.264 frame needs SPS, PPS and IDR for {encoder:?}"
                        );
                    }
                }
            }
            {
                let mut selected = source.lock().unwrap();
                selected.epoch = 2;
            }
            for _ in 0..15 {
                assert_eq!(
                    packet(&mut stream).0,
                    4,
                    "Zoom epoch must not restart capture"
                );
                assert_eq!(packet(&mut stream).0, 2);
            }
            {
                let mut selected = source.lock().unwrap();
                selected.output = outputs[1].clone();
                selected.epoch = 3;
            }
            let mut switched = false;
            for _ in 0..90 {
                let frame = packet(&mut stream);
                if frame.0 == 1 && frame.1 == 3 {
                    switched = true;
                    break;
                }
            }
            assert!(switched, "No new source configuration arrived");
            for _ in 0..30 {
                assert_eq!(packet(&mut stream).0, 4);
                let frame = packet(&mut stream);
                assert_eq!(frame.0, 2);
            }
            drop(service);
            println!(
                "Authenticated 1440p TLS H.264 ({encoder:?}) received from two real monitors; wrong token rejected."
            );
        }
    }

    #[test]
    fn bounded_packets_include_kind_epoch_and_network_order_length() {
        let mut bytes = Vec::new();
        write_packet(&mut bytes, 2, 7, &[1, 2, 3]).unwrap();
        assert_eq!(
            bytes,
            [
                vec![2],
                7u64.to_be_bytes().to_vec(),
                3u32.to_be_bytes().to_vec(),
                vec![1, 2, 3]
            ]
            .concat()
        );
        assert!(write_packet(&mut bytes, 2, 7, &[]).is_err());
        assert!(write_packet(&mut bytes, 2, 7, &vec![0; MAX_ACCESS_UNIT + 1]).is_err());
    }
    #[test]
    fn timestamped_packets_reject_reordering_invalid_sizes_and_encoder_errors() {
        assert_eq!(
            parse_packet_metadata("0, 1770000000000000, 1770000000000000, 33333, 2413, 0x0")
                .unwrap(),
            (1770000000000000, 2413)
        );
        assert_eq!(
            parse_packet_metadata(
                "0, 1770000000000000, 1770000000000000, 33333, 2413, 0x0, F=0x0, S=1, 8"
            )
            .unwrap(),
            (1770000000000000, 2413)
        );
        for line in [
            "0, 1, 2, 33333, 10, 0x0",
            "0, 0, 0, 33333, 10, 0x0",
            "0, -1, -1, 33333, 10, 0x0",
            "0, 1, 1, 33333, 0, 0x0",
            "0, 1, 1, 33333, 8388609, 0x0",
            "encoder failed",
        ] {
            assert!(parse_packet_metadata(line).is_err(), "{line}");
        }
    }
    #[test]
    fn video_size_preserves_landscape_portrait_and_even_dimensions() {
        assert_eq!(encoded_size(2560, 1440), (2560, 1440));
        assert_eq!(encoded_size(3840, 2160), (2560, 1440));
        assert_eq!(encoded_size(1080, 1920), (810, 1440));
        assert_eq!(encoded_size(640, 480), (640, 480));
    }
}
