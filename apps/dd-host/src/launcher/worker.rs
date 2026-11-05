use crate::host_environment::{self, NO_WINDOW, Network, checked_output, command_output};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::SocketAddr,
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::mpsc::{self, Receiver, SyncSender},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    },
};

const EVENT_PREFIX: &str = "DRAWDock_EVENT ";

#[derive(Clone, Debug)]
pub struct Device {
    pub serial: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Output {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone)]
pub struct Config {
    pub output: String,
    pub encoder: String,
}

pub enum Request {
    Discover(String),
    Enable {
        endpoint: SocketAddr,
        config: Config,
        usb: Option<(String, String)>,
    },
    Disable(SocketAddr),
    Pair(SocketAddr),
    Stop,
    Quit,
}

pub enum Event {
    Log(&'static str, String),
    Discovery {
        outputs: Vec<Output>,
        networks: Vec<Network>,
        devices: Vec<Device>,
    },
    Host(bool),
    State(Value),
    Complete,
    Exited,
}

enum Message {
    Request(Request),
    Line(u64, bool, String),
}

pub struct Worker {
    sender: SyncSender<Message>,
    pub events: Receiver<Event>,
}

impl Worker {
    pub fn start() -> Result<Self, String> {
        let (sender, commands) = mpsc::sync_channel(512);
        let (events, receiver) = mpsc::sync_channel(512);
        let reader_sender = sender.clone();
        thread::Builder::new()
            .name("drawdock-launcher".into())
            .spawn(move || {
                run(commands, reader_sender, &events);
            })
            .map_err(|e| format!("start launcher worker: {e}"))?;
        Ok(Self {
            sender,
            events: receiver,
        })
    }

    pub fn send(&self, command: Request) -> Result<(), String> {
        self.sender
            .try_send(Message::Request(command))
            .map_err(|e| format!("launcher command was not queued: {e}"))
    }
}

struct Job(HANDLE);
impl Job {
    fn attach(child: &mut Child) -> Result<Self, String> {
        // SAFETY: The job owns only the child created by this launcher and its descendants.
        unsafe {
            let job = Self(CreateJobObjectW(std::ptr::null(), std::ptr::null()));
            if job.0.is_null() {
                return Err(format!(
                    "create host job: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
                || AssignProcessToJobObject(job.0, child.as_raw_handle()) == 0
            {
                return Err(format!(
                    "contain owned host process: {}",
                    std::io::Error::last_os_error()
                ));
            }
            Ok(job)
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: This handle is exclusively owned by Job.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

struct Host {
    child: Child,
    _job: Job,
    generation: u64,
}

fn host_exe() -> Result<PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|e| format!("locate launcher: {e}"))?
        .with_file_name("dd-host.exe"))
}

fn spawn_host(
    config: &Config,
    generation: u64,
    sender: &SyncSender<Message>,
) -> Result<Host, String> {
    let mut child = Command::new(host_exe()?)
        .args([
            "serve-managed",
            "--output",
            &config.output,
            "--video-encoder",
            &config.encoder,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(NO_WINDOW)
        .spawn()
        .map_err(|e| format!("start owned host: {e}"))?;
    let job = match Job::attach(&mut child) {
        Ok(job) => job,
        Err(error) => {
            let cleanup = child.kill().and_then(|()| child.wait().map(|_| ()));
            return Err(format!("{error}; child cleanup: {cleanup:?}"));
        }
    };
    let stdout = child.stdout.take().ok_or("missing host stdout")?;
    let stderr = child.stderr.take().ok_or("missing host stderr")?;
    for (error, pipe) in [
        (false, Box::new(stdout) as Box<dyn std::io::Read + Send>),
        (true, Box::new(stderr) as Box<dyn std::io::Read + Send>),
    ] {
        let sender = sender.clone();
        thread::Builder::new()
            .name("drawdock-host-log".into())
            .spawn(move || {
                for line in BufReader::new(pipe).lines() {
                    let line = match line {
                        Ok(line) => line,
                        Err(error) => {
                            let _ = sender.send(Message::Line(
                                generation,
                                true,
                                format!("Host log pipe failed: {error}"),
                            ));
                            break;
                        }
                    };
                    if sender.send(Message::Line(generation, error, line)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|e| format!("start host log reader: {e}"))?;
    }
    Ok(Host {
        child,
        _job: job,
        generation,
    })
}

fn command(host: &mut Host, value: Value) -> Result<(), String> {
    let stdin = host
        .child
        .stdin
        .as_mut()
        .ok_or("host control pipe unavailable")?;
    writeln!(stdin, "{value}")
        .and_then(|()| stdin.flush())
        .map_err(|e| format!("write host control: {e}"))
}

fn adb(path: &str, args: &[&str]) -> Result<String, String> {
    let output = command_output(Command::new(path).args(args), "run ADB")?;
    checked_output(output, "ADB")
}

pub fn parse_devices(text: &str) -> Vec<Device> {
    text.lines()
        .filter_map(|line| {
            let mut words = line.split_whitespace();
            let serial = words.next()?;
            let state = words.next()?;
            if line.starts_with("List ") || line.starts_with('*') || serial.contains(':') {
                return None;
            }
            Some(Device {
                serial: serial.into(),
                state: state.into(),
            })
        })
        .collect()
}

#[derive(Debug)]
struct Reverse {
    adb: String,
    serial: String,
    port: u16,
    created: bool,
}

fn reverse_exists(text: &str, port: u16) -> Result<bool, String> {
    let expected = format!("tcp:{port}");
    for line in text.lines() {
        let parts: Vec<_> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[1] == expected {
            return if parts[2] == expected {
                Ok(true)
            } else {
                Err(format!(
                    "Tablet port {port} already has another mapping; refusing to replace it"
                ))
            };
        }
    }
    Ok(false)
}

impl Reverse {
    fn enable(path: String, serial: String, port: u16) -> Result<Self, String> {
        if serial.contains(':') || serial.is_empty() {
            return Err("Select an authorized USB device, not ADB-over-Wi-Fi".into());
        }
        if adb(&path, &["-s", &serial, "get-state"])?.trim() != "device" {
            return Err("Selected USB device is not authorized/connected".into());
        }
        let exists = reverse_exists(&adb(&path, &["-s", &serial, "reverse", "--list"])?, port)?;
        if !exists {
            let endpoint = format!("tcp:{port}");
            adb(
                &path,
                &[
                    "-s",
                    &serial,
                    "reverse",
                    "--no-rebind",
                    &endpoint,
                    &endpoint,
                ],
            )?;
        }
        Ok(Self {
            adb: path,
            serial,
            port,
            created: !exists,
        })
    }

    fn remove(&self) -> Result<(), String> {
        if self.created {
            let listing = adb(&self.adb, &["-s", &self.serial, "reverse", "--list"])?;
            if !reverse_exists(&listing, self.port)? {
                return Ok(());
            }
            adb(
                &self.adb,
                &[
                    "-s",
                    &self.serial,
                    "reverse",
                    "--remove",
                    &format!("tcp:{}", self.port),
                ],
            )?;
        }
        Ok(())
    }
}

fn report(events: &SyncSender<Event>, event: Event) {
    let _ = events.send(event);
}
fn log(events: &SyncSender<Event>, level: &'static str, text: impl Into<String>) {
    report(events, Event::Log(level, text.into()));
}

fn cleanup_reverse(reverse: &mut Option<Reverse>, events: &SyncSender<Event>) {
    if let Some(mapping) = reverse.take()
        && let Err(error) = mapping.remove()
    {
        log(events, "ERROR", format!("USB cleanup incomplete: {error}"));
        *reverse = Some(mapping);
    }
}

fn discover(path: &str, events: &SyncSender<Event>) -> Result<(), String> {
    let output = command_output(Command::new(host_exe()?).arg("doctor"), "enumerate outputs")?;
    let value: Value = serde_json::from_str(&checked_output(output, "host doctor")?)
        .map_err(|e| format!("read host outputs: {e}"))?;
    let outputs = serde_json::from_value(value["outputs"].clone())
        .map_err(|e| format!("parse outputs: {e}"))?;
    let networks = host_environment::networks()?;
    let devices = match adb(path, &["devices", "-l"]) {
        Ok(text) => parse_devices(&text),
        Err(error) => {
            log(events, "ERROR", error);
            Vec::new()
        }
    };
    report(
        events,
        Event::Discovery {
            outputs,
            networks,
            devices,
        },
    );
    Ok(())
}

fn run(receiver: Receiver<Message>, sender: SyncSender<Message>, events: &SyncSender<Event>) {
    let mut host: Option<Host> = None;
    let mut reverse = None;
    let mut pending_usb = BTreeMap::new();
    let mut next_id = 0u64;
    let mut generation = 0u64;
    let mut stopping = None;
    let mut quit = false;
    loop {
        if let Some(active) = &mut host {
            match active.child.try_wait() {
                Ok(Some(status)) => {
                    log(
                        events,
                        if stopping.is_some() { "INFO" } else { "ERROR" },
                        format!("Owned host exited: {status}"),
                    );
                    host = None;
                    stopping = None;
                    pending_usb.clear();
                    cleanup_reverse(&mut reverse, events);
                    report(events, Event::Host(false));
                    report(events, Event::Complete);
                }
                Err(error) => log(events, "ERROR", format!("Inspect owned host: {error}")),
                Ok(None) => {}
            }
        }
        if stopping.is_some_and(|deadline| Instant::now() >= deadline) {
            if let Some(active) = host.take() {
                log(
                    events,
                    "ERROR",
                    "Graceful host stop timed out; closing only the launcher's owned process job.",
                );
                drop(active);
            }
            cleanup_reverse(&mut reverse, events);
            stopping = None;
            report(events, Event::Host(false));
            report(events, Event::Complete);
        }
        if quit && host.is_none() {
            cleanup_reverse(&mut reverse, events);
            report(events, Event::Exited);
            return;
        }
        let message = match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(value) => value,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(_) => {
                drop(host.take());
                cleanup_reverse(&mut reverse, events);
                return;
            }
        };
        let result: Result<(), String> = (|| {
            match message {
                Message::Line(id, error, text) => {
                    if let Some(json) = text.strip_prefix(EVENT_PREFIX) {
                        if host.as_ref().is_some_and(|host| host.generation == id) {
                            let value: Value = serde_json::from_str(json)
                                .map_err(|e| format!("invalid host status: {e}"))?;
                            if let Some(request_id) = value["request_id"].as_u64()
                                && pending_usb.remove(&request_id).is_some()
                                && value["event"] == "error"
                            {
                                cleanup_reverse(&mut reverse, events);
                            }
                            if value["event"] == "error" {
                                log(
                                    events,
                                    "ERROR",
                                    value["message"].as_str().unwrap_or("Host command failed"),
                                );
                            }
                            report(events, Event::State(value));
                        }
                    } else {
                        log(events, if error { "ERROR" } else { "HOST" }, text);
                    }
                }
                Message::Request(request) => {
                    if stopping.is_some() && !matches!(request, Request::Quit) {
                        return Err("Wait for the owned host to finish stopping".into());
                    }
                    next_id += 1;
                    match request {
                        Request::Discover(path) => {
                            discover(&path, events)?;
                            report(events, Event::Complete);
                        }
                        Request::Enable {
                            endpoint,
                            config,
                            usb,
                        } => {
                            if endpoint.ip().is_loopback() {
                                if reverse.is_some() {
                                    return Err("USB mapping already owned; disable USB or finish cleanup first".into());
                                }
                                let (path, serial) =
                                    usb.ok_or("USB device configuration is missing")?;
                                reverse = Some(Reverse::enable(path, serial, endpoint.port())?);
                                pending_usb.insert(next_id, ());
                            }
                            if host.is_none() {
                                generation += 1;
                                match spawn_host(&config, generation, &sender) {
                                    Ok(value) => host = Some(value),
                                    Err(error) => {
                                        cleanup_reverse(&mut reverse, events);
                                        return Err(error);
                                    }
                                }
                                report(events, Event::Host(true));
                            }
                            command(
                                host.as_mut().ok_or("host unavailable")?,
                                json!({"action":"enable","endpoint":endpoint,"request_id":next_id}),
                            )?;
                        }
                        Request::Disable(endpoint) => {
                            if endpoint.ip().is_loopback() {
                                cleanup_reverse(&mut reverse, events);
                            }
                            command(
                                host.as_mut().ok_or("host is stopped")?,
                                json!({"action":"disable","endpoint":endpoint,"request_id":next_id}),
                            )?;
                        }
                        Request::Pair(endpoint) => command(
                            host.as_mut().ok_or("Enable a listener first")?,
                            json!({"action":"pair","endpoint":endpoint,"request_id":next_id}),
                        )?,
                        Request::Stop | Request::Quit => {
                            quit |= matches!(request, Request::Quit);
                            if let Some(host) = &mut host {
                                command(host, json!({"action":"stop","request_id":next_id}))?;
                                stopping = Some(Instant::now() + Duration::from_secs(8));
                            } else {
                                cleanup_reverse(&mut reverse, events);
                                report(events, Event::Complete);
                            }
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            log(events, "ERROR", error);
            report(events, Event::Complete);
            if quit {
                drop(host.take());
            }
        }
    }
}

pub fn default_adb() -> String {
    for variable in ["ANDROID_HOME", "ANDROID_SDK_ROOT"] {
        if let Some(root) = std::env::var_os(variable) {
            let path = Path::new(&root).join("platform-tools").join("adb.exe");
            if path.is_file() {
                return path.to_string_lossy().into();
            }
        }
    }
    if let Some(paths) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&paths) {
            let path = directory.join("adb.exe");
            if path.is_file() {
                return path.to_string_lossy().into();
            }
        }
    }
    "adb.exe".into()
}

#[cfg(test)]
#[path = "../../tests/unit/launcher_worker.rs"]
mod tests;
