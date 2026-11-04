# Windows host

Run commands from the repository root in PowerShell. Complete the
[development setup](development.md) first.

## Build and choose an output

```powershell
cargo build -p dd-host
& .\target\debug\dd-host.exe outputs
```

Select an exact device name printed by `outputs`; examples below use
`\\.\DISPLAY2`. Enumerate again after changing monitors, display modes or DPI
configuration rather than copying another machine's geometry.

## USB launcher

Connect and authorize the tablet in ADB. Set your paths/device explicitly:

```powershell
$adb = 'C:\Path\To\Android\Sdk\platform-tools\adb.exe'
& $adb devices -l
$serial = '<authorized-device-serial>'
$output = '\\.\DISPLAY2'

.\Start-DrawDockHost.ps1 -Output $output -DeviceSerial $serial `
    -AdbPath $adb -VideoEncoder software
```

The script validates the output/device, checks existing mappings and starts the
debug host on loopback. It refuses to overwrite a conflicting reverse mapping.
Ctrl+C stops the foreground host; cleanup removes only forwarding created by
that invocation.

The script contains machine-specific defaults for output, serial and ADB path,
and defaults to NVENC. Override them for your environment. Use
`-VideoEncoder nvenc` only with a compatible encoder.

| Option | Behavior |
|---|---|
| `-Output` | Selected Windows output device name |
| `-Port` | Control/video TCP port; default 47831 |
| `-DeviceSerial` / `-AdbPath` | Explicit ADB target and executable |
| `-VideoEncoder software\|nvenc` | Encoder selection; no silent fallback |
| `-Restart` | Stop only hosts using this workspace's debug executable, then restart |
| `-Pair` | Open an invitation for new enrollment even with remembered devices |
| `-WhatIf` | Read-only preflight without process/mapping changes |

Restart ends the active session and its drawings. Use `-Restart -Pair` when
enrolling another tablet or replacing a forgotten/revoked credential. The
launcher does not change firewall, elevation or PowerShell execution policy.

## Direct CLI

For manual transport setup, manage the ADB reverse mapping as described in
[Android and ADB](run-android-and-adb.md#usb-forwarding).
Direct CLI uses software encoding unless explicitly overridden:

```powershell
& .\target\debug\dd-host.exe serve --output '\\.\DISPLAY2' `
    --listen 127.0.0.1 --port 47831 --video-encoder software
```

| Command | Purpose |
|---|---|
| `outputs` | Enumerate output names, physical dimensions and origins |
| `doctor [--output <path>]` | Print/write a diagnostic JSON snapshot |
| `overlay-demo --output <name>` | Display the built-in annotation scene without a network session |
| `replay <fixture> --output <name>` | Render a validated JSON scene |
| `video-probe --output <name> [--video-encoder software\|nvenc]` | Capture/encode a bounded frame probe without saving a recording |
| `serve --output <name>` | Start overlay, pairing/control and video services |
| `devices list` / `devices revoke <id>` | Manage remembered devices while the host is stopped |

`serve` also accepts `--listen`, `--port`, `--pair`, `--pairing-info <new-path>`
and `--video-encoder`. Public/wildcard addresses and port zero are rejected.

Replay and diagnostics:

```powershell
& .\target\debug\dd-host.exe replay .\tests\fixtures\basic-scene.json `
    --output '\\.\DISPLAY2'
# Stop the replay with Ctrl+C before the next command.
& .\target\debug\dd-host.exe doctor --output .\project-status\doctor.json
```

Replay uses the same native presenter, without opening a network listener.
Diagnostic output is a machine snapshot, not an interactive test result.

## Private-network connections

Choose a private IP actually assigned to the host interface. The USB launcher
always uses loopback; use direct CLI for a private-network listener:

```powershell
# Replace this example address with your assigned private address.
& .\target\debug\dd-host.exe serve --output '\\.\DISPLAY2' `
    --listen 192.168.1.10 --port 47831 --pair --video-encoder software
```

Pair using the invitation for that endpoint. ADB reverse is not used for this
connection. Android saves one endpoint, so switching from loopback/USB requires
deliberate re-pairing. Follow your network/firewall policy; the application does
not create an exception automatically.

## Remembered devices

The registry and TLS identity are stored at
`%LOCALAPPDATA%\DrawDock\trust.dpapi`. Do not delete it to fix a connection:
replacing identity invalidates saved pins. Corrupt/inaccessible state is reported
explicitly. Stop the host before administration because it holds an exclusive
trust-store lock.

```powershell
& .\target\debug\dd-host.exe devices list
& .\target\debug\dd-host.exe devices revoke '<device-id-from-list>'
```

Listed IDs are not authentication secrets. Revocation rejects future resumes
for that credential; it is not a request to interrupt a running session.
Android Forget PC removes only its local saved copy.

## Video configuration and troubleshooting

Set a direct FFmpeg executable path if needed:

```powershell
$env:DRAWDOCK_FFMPEG = 'C:\Path\To\ffmpeg.exe'
& .\target\debug\dd-host.exe video-probe --output '\\.\DISPLAY2' `
    --video-encoder software
```

See [video streaming](video-streaming.md) for codec support, defaults, queue
limits and lifecycle, and [development](development.md#native-integration-checks)
for native integration commands.

If the output disappears or changes geometry, reconnect after re-enumerating.
For NVENC failure, explicitly choose software or correct the encoder setup.
For trust-lock errors, stop the other host using that registry. Do not recover
from a pin mismatch by disabling validation.
