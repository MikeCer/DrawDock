#[cfg(not(windows))]
compile_error!("dd-host currently implements only the native Windows presenter");

mod pairing_window;
mod session;
mod windows_input;
mod windows_overlay;

use dd_scene::{Scene, SceneFixture};
use serde_json::json;
use std::{
    env, fs,
    net::IpAddr,
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
                "tablet_connectivity": "not probed; verify with adb devices -l",
                "wifi_listener": "disabled",
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
        Some("serve") => {
            let output = selected_output(&args)?;
            let listen = selected_listen_address(&args)?;
            let port = selected_port(&args)?;
            let pairing_info = selected_pairing_info(&args)?;
            let scene = Arc::new(Mutex::new(Scene::default()));
            let server_scene = Arc::clone(&scene);
            let bounds = windows_input::OutputBounds {
                left: output.left,
                top: output.top,
                width: output.width(),
                height: output.height(),
            };
            thread::Builder::new()
                .name("drawdock-tls-session".into())
                .spawn(move || {
                    if let Err(error) =
                        session::run_server(listen, port, server_scene, bounds, pairing_info)
                    {
                        eprintln!("DrawDock session stopped: {error}");
                    }
                })
                .map_err(|error| format!("start session thread: {error}"))?;
            println!(
                "Starting host overlay on {}; Ctrl+C removes the overlay and stops this process.",
                output.name
            );
            windows_overlay::run(output, scene)
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
                   dd-host overlay-demo --output \\\\.\\DISPLAY2\n\
                   dd-host replay tests\\fixtures\\basic-scene.json --output \\\\.\\DISPLAY2\n\
                   dd-host serve --output \\\\.\\DISPLAY2 [--listen 127.0.0.1] [--port 47831] [--pairing-info <path>]"
            );
            Ok(())
        }
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
mod tests {
    use super::*;

    #[test]
    fn pairing_info_output_is_opt_in_and_requires_a_path() {
        assert!(selected_pairing_info(&[]).unwrap().is_none());
        assert!(selected_pairing_info(&["serve".into(), "--pairing-info".into()]).is_err());
        assert_eq!(
            selected_pairing_info(&["serve".into(), "--pairing-info".into(), "pair.json".into()])
                .unwrap(),
            Some(PathBuf::from("pair.json"))
        );
    }
}
