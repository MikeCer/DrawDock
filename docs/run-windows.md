# Build and run the Windows host

For the already-built host and connected USB tablet, run the launcher from
PowerShell:

```powershell
E:\dev\DrawDock\Start-DrawDockHost.ps1 -Restart
```

It validates the selected display and authorized tablet, stops only this
workspace's existing host when `-Restart` is supplied, configures ADB reverse
and starts a fresh loopback-only host with a QR invitation. On the tablet,
choose **Connect > Scan QR**, then approve in the PC dialog. Press **Ctrl+C**
in the script's console to stop. Only USB forwarding created by the script is
removed on exit; pre-existing mappings are preserved. No firewall, elevation
or execution-policy changes are made.

The defaults match the current system (`\\.\DISPLAY2`, port 47831, tablet
`cfa3e5dce217`). Override them with `-Output`, `-Port`, `-DeviceSerial` or
`-AdbPath`. Use `-WhatIf -Restart` for a read-only preflight. If PowerShell blocks
the script under your execution policy, review your organization's approved
script-running procedure; the launcher does not bypass that policy.

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
