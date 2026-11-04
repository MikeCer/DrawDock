# Build and run the Windows host

For the already-built host and connected USB tablet, run the launcher from
PowerShell:

```powershell
E:\dev\DrawDock\Start-DrawDockHost.ps1 -Restart
```

It validates the selected display and authorized tablet, stops only this
workspace's existing host when `-Restart` is supplied, configures ADB reverse
and starts a fresh loopback-only host. For first pairing, choose
**Connect > Scan QR**, then approve remembering the tablet in the PC dialog.
Subsequent **Connect** actions reuse saved trust without another QR/approval.
Press **Ctrl+C**
in the script's console to stop. Only USB forwarding created by the script is
removed on exit; pre-existing mappings are preserved. No firewall, elevation
or execution-policy changes are made.

The defaults match the current system (`\\.\DISPLAY2`, port 47831, tablet
`cfa3e5dce217`). Override them with `-Output`, `-Port`, `-DeviceSerial` or
`-AdbPath`. Use `-WhatIf -Restart` for a read-only preflight. If PowerShell blocks
the script under your execution policy, review your organization's approved
script-running procedure; the launcher does not bypass that policy.

Video now uses up to 2560x1440 at 30 fps. This machine's launcher defaults to
NVIDIA NVENC, verified on its RTX 3080 Ti; no SDK or driver was installed.
For another GPU or an unavailable NVENC session, explicitly select software:

```powershell
E:\dev\DrawDock\Start-DrawDockHost.ps1 -Restart -VideoEncoder software
& .\target\debug\dd-host.exe video-probe --output '\\.\DISPLAY2' --video-encoder nvenc
```

Direct `dd-host serve` defaults to software; add `--video-encoder nvenc` for
hardware encoding. There is no silent runtime fallback. Rebuild/update the
Android APK as well: the older decoder deliberately rejects video above 720p.
Saved pairing credentials are unaffected. High-quality video may use up to
24 Mbit/s; Wi-Fi/thermal/long-running acceptance is still outstanding.

Run these commands from PowerShell in `E:\dev\DrawDock`. They load the existing
Visual Studio 2026 x64 environment only in the current child process; they do not
modify system settings or install software.

```powershell
$vcvars = 'D:\Program Files\Microsoft Visual Studio\18\Enterprise\VC\Auxiliary\Build\vcvars64.bat'
$env:PATH += ';C:\Program Files (x86)\Microsoft Visual Studio\Installer'
$cmd = "call `"$vcvars`" >nul && cd /d E:\dev\DrawDock && cargo fmt --all -- --check && cargo test --workspace"
& $env:ComSpec /c $cmd
```

Enumerate the current outputs and generate a local doctor report:

```powershell
$cmd = "call `"$vcvars`" >nul && cd /d E:\dev\DrawDock && cargo run -p dd-host -- outputs"
& $env:ComSpec /c $cmd
$cmd = "call `"$vcvars`" >nul && cd /d E:\dev\DrawDock && cargo run -p dd-host -- doctor --output reports\doctor.json"
& $env:ComSpec /c $cmd
```

Choose the exact device name printed by `outputs`; the default demo deliberately
requires explicit output selection:

```powershell
$cmd = "call `"$vcvars`" >nul && cd /d E:\dev\DrawDock && cargo run -p dd-host -- replay tests\fixtures\basic-scene.json --output '\\.\DISPLAY2'"
& $env:ComSpec /c $cmd
```

Replace `\\.\DISPLAY2` with the selected output from the first command. The demo
is a click-through overlay with no controls; press **Ctrl+C** in that same host
console to remove it. Manually verify that the underlying application continues
to animate, receives clicks through painted ink, retains focus/keyboard input,
and shows all four shapes. Separately test an actual fullscreen slideshow. These
manual results must be recorded; build or unit-test success is not a substitute.

The initial `doctor` output records implementation status, not a PASS for
desktop interaction.

The mirroring backend uses the already-installed native FFmpeg 7.0
gdigrab/libx264/NVENC executable. Rust controls its lifecycle; no Linux, Docker or
system installation is needed. `DRAWDOCK_FFMPEG` can override its path. This
installed build is GPL-enabled and is not redistributed with DrawDock; binary
distribution/licensing review is still required before packaging dependencies.

Capture/stream checks (no recording is saved):

```powershell
& .\target\debug\dd-host.exe video-probe --output '\\.\DISPLAY2'
cargo test -p dd-host native_tls_video_authenticates_and_switches_monitors -- --ignored --nocapture
```

The second command captures two real displays over locally authenticated TLS,
with both x264 and NVENC, checking SPS/PPS/IDR startup and source switching;
it requires two available outputs and NVIDIA NVENC and is excluded from ordinary
unit-test runs. It is not a physical tablet playback or fullscreen test.

## Remembered tablets

The host retains its identity and trusted tablets in current-user DPAPI-encrypted
`%LOCALAPPDATA%\DrawDock\trust.dpapi`. Do not delete this file to troubleshoot a
connection: changing identity invalidates the tablet's saved pin. Corrupt or
inaccessible state is reported explicitly, without silently replacing trust.
No administrator access, firewall rule or system setting is involved.

After upgrading both applications, pair once more to enroll a remembered
credential. Afterwards the host stays listening across disconnects, and the
tablet's **Connect** reuses its credential after app or host restart.
To enroll another tablet (or re-pair after Forget PC):

```powershell
E:\dev\DrawDock\Start-DrawDockHost.ps1 -Restart -Pair
```

Stop the host before inspecting or revoking trust; the encrypted registry has an
exclusive process lock. Listed device IDs are not authentication secrets:

```powershell
& .\target\debug\dd-host.exe devices list
& .\target\debug\dd-host.exe devices revoke '<device-id-from-list>'
```

Revocation rejects that device's subsequent reconnects while retaining the host
certificate and other devices. Tablet **Forget PC** removes only the tablet copy.
The host accepts one active tablet at a time.

Actual native host reconnect/restart/revocation regression (isolated encrypted
test store; no production enrollment or saved credentials are used):

```powershell
cargo build -p dd-host
cargo test -p dd-host native_host_resumes_repeatedly_after_restart_and_rejects_revocation -- --ignored --nocapture
```
