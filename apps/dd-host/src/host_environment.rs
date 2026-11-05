use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    net::Ipv4Addr,
    os::windows::process::CommandExt,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

pub const NO_WINDOW: u32 = 0x0800_0000;

pub fn bundled_tool(name: &str) -> Result<std::path::PathBuf, String> {
    Ok(std::env::current_exe()
        .map_err(|error| format!("locate bundled {name}: {error}"))?
        .with_file_name("runtime")
        .join(name)
        .join(format!("{name}.exe")))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Network {
    pub name: String,
    pub address: Ipv4Addr,
}

pub fn networks() -> Result<Vec<Network>, String> {
    let script = r#"
$ErrorActionPreference='Stop'
$OutputEncoding=[Console]::OutputEncoding=[System.Text.UTF8Encoding]::new($false)
$profiles=@(Get-NetConnectionProfile)
$items=@(Get-NetIPAddress -AddressFamily IPv4 | Where-Object AddressState -eq Preferred | ForEach-Object {
 $ip=[System.Net.IPAddress]::Parse($_.IPAddress); $b=$ip.GetAddressBytes()
 $private=$b[0] -eq 10 -or ($b[0] -eq 172 -and $b[1] -ge 16 -and $b[1] -le 31) -or ($b[0] -eq 192 -and $b[1] -eq 168)
 $p=@($profiles | Where-Object InterfaceIndex -eq $_.InterfaceIndex)
 if($private -and $p.Count -eq 1 -and $p[0].NetworkCategory -in @('Private','DomainAuthenticated')){
  [pscustomobject]@{name=$_.InterfaceAlias;address=$_.IPAddress}
 }
})
ConvertTo-Json -InputObject $items -Compress
"#;
    let output = command_output(
        Command::new("powershell.exe").args(["-NoProfile", "-NonInteractive", "-Command", script]),
        "enumerate private Windows interfaces",
    )?;
    let text = checked_output(output, "enumerate private Windows interfaces")?;
    serde_json::from_str(text.trim_start_matches('\u{feff}').trim())
        .map_err(|e| format!("parse Windows interface discovery: {e}"))
}

pub fn command_output(command: &mut Command, action: &str) -> Result<Output, String> {
    let mut child = command
        .creation_flags(NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{action}: {e}"))?;
    let stdout = child.stdout.take().ok_or("command stdout unavailable")?;
    let stderr = child.stderr.take().ok_or("command stderr unavailable")?;
    let read = |pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            pipe.take(1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    };
    let stdout = read(Box::new(stdout));
    let stderr = read(Box::new(stderr));
    let deadline = Instant::now() + Duration::from_secs(10);
    let result = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => break Err(format!("{action} exceeded 10 seconds")),
            Err(error) => break Err(format!("{action} process status: {error}")),
        }
    };
    if result.is_err() {
        child
            .kill()
            .map_err(|e| format!("{action} timeout cleanup: {e}"))?;
        child
            .wait()
            .map_err(|e| format!("{action} wait after cleanup: {e}"))?;
    }
    let stdout = stdout
        .join()
        .map_err(|_| "command stdout reader panicked")?
        .map_err(|e| format!("{action} stdout: {e}"))?;
    let stderr = stderr
        .join()
        .map_err(|_| "command stderr reader panicked")?
        .map_err(|e| format!("{action} stderr: {e}"))?;
    if stdout.len() > 1024 * 1024 || stderr.len() > 1024 * 1024 {
        return Err(format!("{action} output exceeded its 1 MiB bound"));
    }
    Ok(Output {
        status: result?,
        stdout,
        stderr,
    })
}

pub fn checked_output(output: Output, action: &str) -> Result<String, String> {
    if !output.status.success() {
        return Err(format!(
            "{action} failed ({}): {} {}",
            output.status,
            String::from_utf8_lossy(&output.stdout).trim(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout).map_err(|e| format!("{action} returned invalid UTF-8: {e}"))
}
