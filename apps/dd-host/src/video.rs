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
        let bundled = crate::host_environment::bundled_tool("ffmpeg")?;
        let executable = std::env::var_os("DRAWDOCK_FFMPEG").unwrap_or_else(|| {
            if bundled.is_file() {
                return bundled.into_os_string();
            }
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
#[path = "../tests/unit/video.rs"]
mod tests;
