#[cfg(not(windows))]
compile_error!("dd-host currently implements only the native Windows presenter");

mod host_environment;
mod listener_control;
mod pairing_window;
mod session;
mod source;
mod trust;
mod video;
mod windows_input;
mod windows_overlay;
mod windows_zoom;
mod zoom;

use dd_scene::{Scene, SceneFixture};
use serde_json::json;
use std::{
    env, fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    process::ExitCode,
    str::FromStr,
    sync::{Arc, Mutex},
    thread,
};
use windows_overlay::Output;

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("DrawDock: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    windows_overlay::initialize_dpi()?;
    match args.first().map(String::as_str) {
        Some("outputs") => {
            for output in Output::enumerate()? {
                println!(
                    "{}  {}x{} at ({}, {}){}",
                    output.name,
                    output.width(),
                    output.height(),
                    output.left,
                    output.top,
                    if output.primary { "  primary" } else { "" }
                );
            }
            Ok(())
        }
        Some("doctor") => {
            let outputs = Output::enumerate()?;
            let report = json!({
                "os": "Windows",
                "native_overlay": "implemented; interactive fullscreen behavior not yet verified",
                "output_count": outputs.len(),
                "outputs": outputs,
                "tablet_connectivity": "not probed; USB uses ADB, Wi-Fi uses the selected private LAN IP",
                "wifi_listener": "supported with explicit private --listen IP; not started by doctor",
                "linux": "DEFERRED",
                "quic": "DEFERRED"
            });
            let pretty = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
            if let Some(index) = args.iter().position(|arg| arg == "--output") {
                let path = args.get(index + 1).ok_or("--output requires a path")?;
                fs::write(path, &pretty).map_err(|e| format!("write {path}: {e}"))?;
                println!("Doctor report written to {path}");
            } else {
                println!("{pretty}");
            }
            Ok(())
        }
        Some("video-probe") => {
            video::probe(selected_output(&args)?, 60, selected_video_encoder(&args)?)
        }
        Some("overlay-demo") => {
            let output = selected_output(&args)?;
            println!(
                "DrawDock overlay on {} ({}x{}). Press Ctrl+C in this console to stop.",
                output.name,
                output.width(),
                output.height()
            );
            println!("The overlay is click-through and does not activate.");
            windows_overlay::run(output, Arc::new(Mutex::new(Scene::demo())))
        }
        Some("serve" | "serve-managed") => {
            let output = selected_output(&args)?;
            let listen = selected_listen_address(&args)?;
            let port = selected_port(&args)?;
            let pairing_info = selected_pairing_info(&args)?;
            let allow_pairing = args.iter().any(|arg| arg == "--pair");
            let encoder = selected_video_encoder(&args)?;
            let trust = trust::TrustStore::open_default()?;
            let scene = Arc::new(Mutex::new(Scene::default()));
            let source = Arc::new(Mutex::new(source::SelectedSource::new(output.clone())));
            let managed = args[0] == "serve-managed";
            if managed
                && (allow_pairing
                    || pairing_info.is_some()
                    || args
                        .iter()
                        .any(|arg| matches!(arg.as_str(), "--listen" | "--port")))
            {
                return Err("serve-managed uses launcher commands for endpoints/pairing; use serve for direct CLI options".into());
            }
            let listeners = if managed {
                listener_control::Listeners::managed()?
            } else {
                listener_control::Listeners::single(SocketAddr::new(listen, port))?
            };
            let server_listeners = Arc::clone(&listeners);
            let server_source = Arc::clone(&source);
            let server_scene = Arc::clone(&scene);
            let server = thread::Builder::new()
                .name("drawdock-tls-session".into())
                .spawn(move || {
                    let result = session::run_server(
                        server_listeners,
                        server_scene,
                        server_source,
                        pairing_info,
                        allow_pairing,
                        trust,
                        encoder,
                    );
                    if let Err(error) = &result {
                        eprintln!("DrawDock session stopped: {error}");
                    }
                    if managed {
                        windows_overlay::request_stop();
                    }
                    result
                })
                .map_err(|error| format!("start session thread: {error}"))?;
            if managed {
                println!(
                    "Starting host overlay on {}; controlled by the Windows launcher.",
                    output.name
                );
            } else {
                println!(
                    "Starting host overlay on {}; Ctrl+C removes the overlay and stops this process.",
                    output.name
                );
            }
            let result = windows_overlay::run_selected(output, scene, Some(source));
            if managed {
                listeners.stop();
                server
                    .join()
                    .map_err(|_| "host session thread panicked")??;
            }
            result
        }
        Some("devices") => {
            let mut trust = trust::TrustStore::open_default()?;
            match args.get(1).map(String::as_str) {
                Some("list") if args.len() == 2 => {
                    if trust.devices().is_empty() {
                        println!("No remembered devices.");
                    }
                    for device in trust.devices() {
                        println!("{}  {}", device.id, device.name);
                    }
                }
                Some("revoke") if args.len() == 3 => {
                    trust.revoke(&args[2])?;
                    println!("Device revoked. Its saved credential will no longer reconnect.");
                }
                _ => return Err("use `dd-host devices list` or `dd-host devices revoke <id>`; stop the host first".into()),
            }
            Ok(())
        }
        Some("replay") => {
            let path = args.get(1).ok_or("replay requires a fixture path")?;
            let output = selected_output(&args)?;
            let bytes = fs::read(path).map_err(|e| format!("read {path}: {e}"))?;
            let fixture: SceneFixture =
                serde_json::from_slice(&bytes).map_err(|e| format!("parse {path}: {e}"))?;
            let scene = Scene::from_fixture(fixture)?;
            println!(
                "Replaying {} objects on {} ({}x{}). Press Ctrl+C in this console to stop.",
                scene.objects().len(),
                output.name,
                output.width(),
                output.height()
            );
            windows_overlay::run(output, Arc::new(Mutex::new(scene)))
        }
        _ => {
            eprintln!(
                "DrawDock Windows host\n\
                 Usage:\n\
                   dd-host outputs\n\
                   dd-host doctor [--output reports\\doctor.json]\n\
                   dd-host video-probe --output \\\\.\\DISPLAY2 [--video-encoder software|nvenc]\n\
                   dd-host overlay-demo --output \\\\.\\DISPLAY2\n\
                   dd-host replay tests\\fixtures\\basic-scene.json --output \\\\.\\DISPLAY2\n\
                   dd-host serve --output \\\\.\\DISPLAY2 [--listen 127.0.0.1] [--port 47831] [--pair] [--pairing-info <path>] [--video-encoder software|nvenc]\n\
                   dd-host devices list\n\
                   dd-host devices revoke <id>"
            );
            Ok(())
        }
    }
}

fn selected_video_encoder(args: &[String]) -> Result<video::VideoEncoder, String> {
    match args.iter().position(|arg| arg == "--video-encoder") {
        Some(index) => video::VideoEncoder::parse(
            args.get(index + 1)
                .ok_or("--video-encoder requires software or nvenc")?,
        ),
        None => Ok(video::VideoEncoder::Software),
    }
}

fn selected_pairing_info(args: &[String]) -> Result<Option<PathBuf>, String> {
    let Some(index) = args.iter().position(|arg| arg == "--pairing-info") else {
        return Ok(None);
    };
    let path = args
        .get(index + 1)
        .ok_or("--pairing-info requires a path")?;
    Ok(Some(PathBuf::from(path)))
}

fn selected_listen_address(args: &[String]) -> Result<IpAddr, String> {
    let address = args
        .iter()
        .position(|arg| arg == "--listen")
        .and_then(|index| args.get(index + 1))
        .map(String::as_str)
        .unwrap_or("127.0.0.1");
    IpAddr::from_str(address).map_err(|_| format!("invalid listen IP address {address:?}"))
}

fn selected_port(args: &[String]) -> Result<u16, String> {
    let Some(index) = args.iter().position(|arg| arg == "--port") else {
        return Ok(47831);
    };
    args.get(index + 1)
        .ok_or("--port requires a number")?
        .parse::<u16>()
        .map_err(|_| "--port must be a nonzero TCP port".into())
        .and_then(|port| {
            if port == 0 {
                Err("--port must be a nonzero TCP port".into())
            } else {
                Ok(port)
            }
        })
}

fn selected_output(args: &[String]) -> Result<Output, String> {
    let index = args
        .iter()
        .position(|arg| arg == "--output")
        .ok_or("select an output explicitly with --output <device-name>")?;
    let name = args
        .get(index + 1)
        .ok_or("--output requires a device name")?;
    Output::enumerate()?
        .into_iter()
        .find(|output| output.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("output {name:?} was not found; run `dd-host outputs`"))
}

#[cfg(test)]
#[path = "../tests/unit/host_cli.rs"]
mod tests;
