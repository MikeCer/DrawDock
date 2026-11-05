use crate::{host_environment, session};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    io::{self, Write},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

pub const EVENT_PREFIX: &str = "DRAWDock_EVENT ";

#[derive(Default)]
struct State {
    listeners: BTreeMap<SocketAddr, TcpListener>,
    active: Option<(SocketAddr, TcpStream)>,
    invitations: VecDeque<SocketAddr>,
}

pub struct Listeners {
    state: Mutex<State>,
    stopped: AtomicBool,
    pub managed: bool,
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    Enable {
        endpoint: SocketAddr,
        request_id: u64,
    },
    Disable {
        endpoint: SocketAddr,
        request_id: u64,
    },
    Pair {
        endpoint: SocketAddr,
        request_id: u64,
    },
    Stop {
        request_id: u64,
    },
}

pub fn event(value: serde_json::Value) {
    let mut stdout = io::stdout().lock();
    if let Err(error) = writeln!(stdout, "{EVENT_PREFIX}{value}") {
        eprintln!("Could not report launcher state: {error}");
    }
}

impl Listeners {
    pub fn single(endpoint: SocketAddr) -> Result<Arc<Self>, String> {
        let listeners = Self::new(false);
        listeners.enable(endpoint)?;
        Ok(listeners)
    }

    fn watch_network(self: &Arc<Self>) -> Result<(), String> {
        let watch = Arc::clone(self);
        thread::Builder::new().name("drawdock-network-policy".into()).spawn(move || {
            while !watch.stopped() {
                for _ in 0..50 {
                    if watch.stopped() { return; }
                    thread::sleep(std::time::Duration::from_millis(100));
                }
                let endpoints = match watch.endpoints() {
                    Ok(items) => items.into_iter().filter(|endpoint| !endpoint.ip().is_loopback()).collect::<Vec<_>>(),
                    Err(error) => { eprintln!("Network policy monitor: {error}"); watch.stop(); return; }
                };
                if endpoints.is_empty() { continue; }
                let discovered = host_environment::networks();
                for endpoint in endpoints {
                    let eligible = discovered.as_ref().is_ok_and(|items|
                        items.iter().any(|item| std::net::IpAddr::V4(item.address) == endpoint.ip()));
                    if !eligible {
                        match watch.enabled(endpoint).and_then(|enabled| if enabled { watch.disable(endpoint) } else { Ok(()) }) {
                            Ok(()) => watch.report(json!({"event":"error","message":format!(
                                "Disabled {endpoint}: interface/profile is no longer eligible or discovery failed: {:?}",
                                discovered.as_ref().err())})),
                            Err(error) => { eprintln!("Disable ineligible LAN listener: {error}"); watch.stop(); return; }
                        }
                    }
                }
            }
        }).map_err(|e| format!("start network policy monitor: {e}"))?;
        Ok(())
    }

    fn new(managed: bool) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State::default()),
            stopped: AtomicBool::new(false),
            managed,
        })
    }

    pub fn managed() -> Result<Arc<Self>, String> {
        let listeners = Self::new(true);
        let control = Arc::clone(&listeners);
        thread::Builder::new().name("drawdock-launcher-control".into()).spawn(move || {
            let stdin = io::stdin();
            let mut reader = stdin.lock();
            loop {
                match session::read_json_line(&mut reader) {
                    Ok(value) => {
                        let result = serde_json::from_value::<Command>(value)
                            .map_err(|e| format!("invalid launcher command: {e}"));
                        if let Err(error) = result.and_then(|command| control.command(command)) {
                            event(json!({"event":"error","message":error}));
                        }
                        if control.stopped() { break; }
                    }
                    Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                        control.stop();
                        break;
                    }
                    Err(error) => {
                        event(json!({"event":"error","message":format!("launcher control closed: {error}")}));
                        control.stop();
                        break;
                    }
                }
            }
        }).map_err(|e| format!("start launcher control: {e}"))?;
        listeners.watch_network()?;
        Ok(listeners)
    }

    fn command(&self, command: Command) -> Result<(), String> {
        let (id, result) = match command {
            Command::Enable {
                endpoint,
                request_id,
            } => {
                let result = if endpoint.ip().is_loopback() {
                    self.enable(endpoint)
                } else {
                    host_environment::networks().and_then(|items| {
                        if items.iter().any(|item| std::net::IpAddr::V4(item.address) == endpoint.ip()) {
                            self.enable(endpoint)
                        } else {
                            Err("LAN address must be assigned to a Private/domain Windows interface; no settings changed".into())
                        }
                    })
                };
                (request_id, result)
            }
            Command::Disable {
                endpoint,
                request_id,
            } => (request_id, self.disable(endpoint)),
            Command::Pair {
                endpoint,
                request_id,
            } => {
                let mut state = self.state.lock().map_err(|_| "listener lock poisoned")?;
                let result = if state.active.is_some() {
                    Err("Disconnect the active tablet before opening a new invitation".into())
                } else if !state.listeners.contains_key(&endpoint) {
                    Err("Enable the selected listener before pairing".into())
                } else if !state.invitations.is_empty() {
                    Err("A pairing request is already pending".into())
                } else {
                    state.invitations.push_back(endpoint);
                    Ok(())
                };
                (request_id, result)
            }
            Command::Stop { request_id } => {
                self.stop();
                (request_id, Ok(()))
            }
        };
        match result {
            Ok(()) => self.report(json!({"event":"listeners","request_id":id})),
            Err(message) => event(json!({"event":"error","request_id":id,"message":message})),
        }
        Ok(())
    }

    pub fn enable(&self, endpoint: SocketAddr) -> Result<(), String> {
        if endpoint.port() == 0 || !session::is_allowed_bind(endpoint.ip()) {
            return Err(
                "Only explicit loopback/private addresses and nonzero ports are permitted".into(),
            );
        }
        let mut state = self.state.lock().map_err(|_| "listener lock poisoned")?;
        if state.listeners.contains_key(&endpoint) {
            return Err(format!("Listener {endpoint} is already enabled"));
        }
        let listener = TcpListener::bind(endpoint).map_err(|e| format!("bind {endpoint}: {e}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|e| format!("configure {endpoint}: {e}"))?;
        state.listeners.insert(endpoint, listener);
        Ok(())
    }

    fn disable(&self, endpoint: SocketAddr) -> Result<(), String> {
        let mut state = self.state.lock().map_err(|_| "listener lock poisoned")?;
        if state.listeners.remove(&endpoint).is_none() {
            return Err(format!("Listener {endpoint} is not enabled"));
        }
        state.invitations.retain(|candidate| *candidate != endpoint);
        if let Some((active, tcp)) = &state.active
            && *active == endpoint
        {
            tcp.shutdown(Shutdown::Both)
                .map_err(|e| format!("stop active connection: {e}"))?;
        }
        Ok(())
    }

    pub fn stop(&self) {
        self.stopped.store(true, Ordering::Release);
        match self.state.lock() {
            Ok(mut state) => {
                state.listeners.clear();
                state.invitations.clear();
                if let Some((_, tcp)) = &state.active
                    && let Err(error) = tcp.shutdown(Shutdown::Both)
                {
                    eprintln!("Stop active tablet: {error}");
                }
            }
            Err(_) => eprintln!("Cannot stop listeners: listener lock poisoned"),
        }
    }

    pub fn stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    pub fn enabled(&self, endpoint: SocketAddr) -> Result<bool, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "listener lock poisoned")?
            .listeners
            .contains_key(&endpoint))
    }

    pub fn endpoints(&self) -> Result<Vec<SocketAddr>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "listener lock poisoned")?
            .listeners
            .keys()
            .copied()
            .collect())
    }

    pub fn invitation(&self) -> Result<Option<SocketAddr>, String> {
        Ok(self
            .state
            .lock()
            .map_err(|_| "listener lock poisoned")?
            .invitations
            .pop_front())
    }

    pub fn accept(&self) -> Result<Option<(TcpStream, SocketAddr, SocketAddr)>, String> {
        let state = self.state.lock().map_err(|_| "listener lock poisoned")?;
        for (endpoint, listener) in &state.listeners {
            match listener.accept() {
                Ok((tcp, peer)) => return Ok(Some((tcp, peer, *endpoint))),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(format!("accept on {endpoint}: {error}")),
            }
        }
        Ok(None)
    }

    pub fn video_listener(&self, endpoint: SocketAddr) -> Result<TcpListener, String> {
        self.state
            .lock()
            .map_err(|_| "listener lock poisoned")?
            .listeners
            .get(&endpoint)
            .ok_or("Listener was disabled".to_owned())?
            .try_clone()
            .map_err(|e| format!("clone video listener: {e}"))
    }

    pub fn connection(
        self: &Arc<Self>,
        endpoint: SocketAddr,
        tcp: &TcpStream,
    ) -> Result<Connection, String> {
        let mut state = self.state.lock().map_err(|_| "listener lock poisoned")?;
        if !state.listeners.contains_key(&endpoint) {
            return Err("Listener was disabled".into());
        }
        state.active = Some((
            endpoint,
            tcp.try_clone()
                .map_err(|e| format!("track connection: {e}"))?,
        ));
        Ok(Connection(Arc::clone(self)))
    }

    pub fn report(&self, mut value: serde_json::Value) {
        if !self.managed {
            return;
        }
        match self.state.lock() {
            Ok(state) => {
                value["endpoints"] = json!(
                    state
                        .listeners
                        .keys()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                );
                event(value);
            }
            Err(_) => eprintln!("Cannot report listeners: listener lock poisoned"),
        }
    }
}

pub struct Connection(Arc<Listeners>);
impl Drop for Connection {
    fn drop(&mut self) {
        match self.0.state.lock() {
            Ok(mut state) => {
                state.active = None;
            }
            Err(_) => eprintln!("Cannot release connection tracking: listener lock poisoned"),
        }
        self.0.report(json!({"event":"session","active":false}));
    }
}

#[cfg(test)]
#[path = "../tests/unit/listener_control.rs"]
mod tests;
