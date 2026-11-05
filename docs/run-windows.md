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

## Native Windows launcher

The same build produces the console host and a native Rust/Win32 launcher:

```powershell
& .\target\debug\drawdock-launcher.exe
```

The launcher has a fixed-size, non-resizable taskbar window and a
notification-area (tray) icon.
All listeners start **off**. Select the output, port, encoder and authorized USB
device before enabling a listener. ADB is discovered from the Android SDK
environment or PATH; **Browse ADB...** can select an explicit executable.
Choose NVENC only with compatible NVIDIA hardware and FFmpeg.

Select a listener row, then **Enable selected** or **Disable selected**.
USB/ADB binds loopback and creates an explicit reverse mapping. Ethernet and
Wi-Fi use selected assigned RFC1918 IPv4 addresses on Private/domain profiles;
they are the same authenticated TLS/TCP transport, not separate protocols.
Multiple endpoints can be enabled simultaneously, with **one active tablet
session** sharing the host's trust store, overlay and scene. Additional tablets
are not supported concurrently and may time out while a session is active.

**Pair tablet / QR** opens the existing native invitation for an enabled,
selected endpoint. Native PC approval and certificate pinning are unchanged.
The Android client still saves one endpoint; switching USB/LAN or changing an
IP requires deliberate re-pairing as described below.

Closing the window or choosing **Hide to tray** leaves its listeners running.
Single-click the tray icon to restore the window; its right-click menu also controls
listeners, Stop all and Quit. Launching the executable again restores the
existing window rather than starting another backend.
If tray registration is lost, the launcher attempts to recreate it; failure
keeps the window visible and reports an error instead of hiding an unreachable UI.
**Stop all** stops the owned host and unlocks configuration, even if its last
listener was already disabled. **Quit** also exits the launcher.
Disabling the endpoint serving a tablet ends that session and clears its
drawings; the launcher warns before intentionally ending an active session.
Disabling an unrelated endpoint does not interrupt the active session.

The log viewer retains the latest 500 lines with timestamps and source/severity,
truncates oversized lines and masks pairing/private-key material.
**Pause logs** pauses display updates, not the host; **Clear view** clears only
the retained log view. Logs are not saved to disk.

Only the launcher's own child host/encoder processes and reverse mappings it
created are stopped/removed. Existing equal mappings are reused and preserved;
conflicting mappings or an existing host's exclusive trust lock produce errors,
not automatic takeover. Abrupt termination or a system crash can leave an ADB
mapping behind; inspect `adb reverse --list` before removing a known stale one.
LAN eligibility is rechecked periodically, not instantaneously; disappearing
addresses or nonprivate profiles disable affected endpoints.

The UI is native Rust/Win32, not WPF or a PowerShell GUI. Read-only built-in
Windows PowerShell queries discover interface addresses/profiles. No elevation,
firewall/profile changes, Windows startup registration, public listener or
certificate bypass is performed. Keep `drawdock-launcher.exe` beside
`dd-host.exe`; FFmpeg and ADB retain their existing prerequisites.

## PowerShell USB launcher

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
| `-Transport USB\|WiFi` | USB by default; WiFi uses no ADB |
| `-ListenAddress` | Required with WiFi: explicit assigned RFC1918 IPv4 on a Private/domain profile |
| `-Output` | Selected Windows output device name |
| `-Port` | Control/video TCP port; default 47831 |
| `-DeviceSerial` / `-AdbPath` | Explicit ADB target and executable |
| `-VideoEncoder software\|nvenc` | Encoder selection; no silent fallback |
| `-Restart` | Stop only hosts using this workspace's debug executable, then restart |
| `-Pair` | Open an invitation for new enrollment even with remembered devices |
| `-WhatIf` | Read-only preflight without process/mapping changes |

Restart ends the active session and its drawings. Use `-Restart -Pair` when
enrolling another tablet or replacing a forgotten/revoked credential. The
script does not change firewall, elevation or PowerShell execution policy.

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

Choose a private IPv4 actually assigned to the host's LAN interface. The PC can
use Ethernet while the tablet uses Wi-Fi on the same LAN. List addresses and
profiles first; avoid virtual adapters, guest networks and client-isolated Wi-Fi:

```powershell
Get-NetIPConfiguration
Get-NetConnectionProfile
# Replace this example address with your assigned private LAN address.
.\Start-DrawDockHost.ps1 -Transport WiFi -ListenAddress 192.168.1.10 `
    -Output '\\.\DISPLAY2' -Pair -VideoEncoder software
```

This mode does not inspect ADB, require a tablet serial or create/remove reverse
mappings. It rejects wildcard/public/loopback/link-local addresses, unassigned
addresses and interfaces without a Private or DomainAuthenticated profile.
`-WhatIf` performs read-only preflight without starting a listener.

Disconnect USB for a pure Wi-Fi test. In tablet Settings choose **Pair another
PC**, scan the new LAN invitation, confirm its IP and approve on the PC.
Afterward **Connect** uses that saved LAN endpoint. Android currently saves one
endpoint, so switching from loopback/USB (or changing the PC's DHCP address)
requires deliberate re-pairing. It does not discover hosts or migrate endpoints
automatically. Consider a router DHCP reservation for a stable PC address.

Control and H.264 video use two authenticated TLS/TCP connections on the same
port. No ADB-over-Wi-Fi, QUIC, public listener or certificate bypass is needed.
Direct CLI also supports `serve --listen <private-IP>`.

Follow network/firewall policy. Neither launcher nor host changes the network
profile, elevates or installs firewall exceptions. If connections time out,
check client isolation, routing and a scoped inbound TCP rule. An administrator
may approve/run this example, restricted to the executable, local IP, TCP port,
Private profile and tablet IP (replace all example values):

```powershell
# Only after explicit approval; do not run automatically.
New-NetFirewallRule -DisplayName 'DrawDock tablet TLS' -Direction Inbound `
    -Action Allow -Program (Resolve-Path .\target\debug\dd-host.exe).Path `
    -Protocol TCP -LocalAddress 192.168.1.10 -LocalPort 47831 `
    -RemoteAddress 192.168.1.20 -Profile Private
```

An existing blocking rule or managed policy can still override an allow rule.
Validate mirrored video, stylus annotation on the physical PC, pointer mode,
Clear, reconnect and stability; local listener checks alone do not prove tablet
Wi-Fi or fullscreen acceptance.

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
