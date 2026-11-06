# Android client and ADB

## Build and install

Use the JDK/SDK/Gradle configuration in [Development](development.md).
From the repository root:

```powershell
gradle -p .\android testDebugUnitTest assembleDebug lintDebug
if ($LASTEXITCODE -ne 0) { throw 'Android checks failed' }

$adb = 'C:\Path\To\Android\Sdk\platform-tools\adb.exe'
& $adb devices -l
$serial = '<authorized-device-serial>'
& $adb -s $serial install -r .\android\app\build\outputs\apk\debug\app-debug.apk
if ($LASTEXITCODE -ne 0) { throw 'APK installation failed' }
& $adb -s $serial shell am start -n 'com.drawdock/.MainActivity'
```

Confirm USB debugging authorization and any installation prompt on the selected
device. Do not target an unspecified tablet when multiple devices are attached.
The app requires Android API 30 or later; a real stylus is needed for drawing.
Camera hardware is optional because manual invitation entry is available.

## USB forwarding

The [Windows launcher](run-windows.md#native-windows-launcher) manages forwarding safely.
For direct CLI use, inspect existing mappings first:

```powershell
& $adb -s $serial reverse --list
```

If the tablet port is unused, create the mapping without replacing another:

```powershell
& $adb -s $serial reverse --no-rebind tcp:47831 tcp:47831
```

If the desired mapping already exists, leave it in place. If it points somewhere
else, choose another port or explicitly resolve ownership rather than overwriting
it. Control and video both use this single port. Start the loopback host in a
separate PowerShell:

```powershell
& .\target\debug\dd-host.exe outputs
$output = Read-Host 'Paste the exact output name printed above'
& .\target\debug\dd-host.exe serve --output $output `
    --listen 127.0.0.1 --port 47831 --video-encoder software
```

The tablet endpoint is `127.0.0.1:47831` through ADB. Loopback invitations do not
become private-network invitations when Wi-Fi is enabled.

When finished, remove only a mapping you created and no longer need:

```powershell
& $adb -s $serial reverse --remove tcp:47831
```

## Pair and reconnect

1. Choose Connect > Scan QR in the app.
2. Grant camera access if desired and scan the invitation displayed by your PC.
3. Confirm the endpoint on the tablet.
4. Approve only that tablet in the native PC approval dialog.

The invitation expires after five minutes. Closing it cancels enrollment.
Manual invitation entry is under the advanced pairing option and requires
the full certificate fingerprint and invitation token.

First approval saves a Keystore-encrypted reconnect credential and certificate
pin. Thereafter Connect reuses the saved PC endpoint without another QR/approval,
including after restarting either app. The host keeps listening after disconnects.

Pair another PC opens a new enrollment flow. Forget PC removes the tablet's
saved record but does not revoke a copied credential on the host. For a new
tablet or re-enrollment, open an invitation with host `--pair` or launcher
`-Restart -Pair`. Changing IP/transport requires deliberate pairing for the
new endpoint. See [pairing and threat model](pairing-and-threat-model.md).

## Wi-Fi without USB or ADB

Start the [private-network launcher](run-windows.md#private-network-connections)
with `-Transport WiFi -ListenAddress <PC-LAN-IP> -Pair`. Connect the tablet to
the same trusted LAN and unplug USB to exclude ADB forwarding from the test.
In Settings choose **Pair another PC**, scan the LAN QR, confirm its private IP
and approve the tablet on the PC. **Connect** afterward reuses the saved LAN
endpoint with the same certificate pinning and protected reconnect credential.

The app already supports private-IP control and video sockets using its
`INTERNET` permission; normal IPv4 LAN operation does not require an Android
permission or wireless-debugging setup. It stores one endpoint, not a list of
USB/Wi-Fi alternatives. Re-pair deliberately when switching transport or when
the PC IP changes. Guest/client-isolated Wi-Fi or a PC firewall can block TCP
even when both devices have internet access.

## Workspace

The top dropdown selects the PC monitor. Video and ink share an aspect-fit
rectangle; letterbox space is not a drawing target. Changing monitor cancels
contact and waits for matching video/readiness before enabling new input.

Hold the pen still for about 550 ms to open the radial tools. Use Settings for
connection controls or Pen tools as a menu fallback. See [Features](features.md)
for shape completion, text, transforms, laser/highlighter, blackboard/grid,
clipboard image paste and Clear.

Completed drawings are saved privately on the tablet before sending them.
Backgrounding ends the connection and clears the PC overlay, not the saved
document; retries resume when you return. Transport/video failures show the
saved drawing read-only and retry the same pinned endpoint automatically.
Explicit Disconnect/Stop retrying stops retries while retaining the document.
The Windows launcher restores its selected USB tablet's ADB tunnel after cable
reconnect. With a command-line host, rerun your explicit `adb reverse` command
if the tunnel disappeared; tablet retries cannot create a Windows ADB mapping.
After restarting either app, start the host and tap Connect on the tablet.
Recovery waits up to 120 seconds for the screen and saved document to become
ready; the monitor selector and editing remain disabled until matching video
arrives. See [recovery and its limits](features.md#clear-history-and-lifecycle).

## Optional ADB invitation delivery

Prefer QR for normal pairing. For explicitly chosen USB automation, the host
can create a new invitation file. In the host console:

```powershell
& .\target\debug\dd-host.exe outputs
$output = Read-Host 'Paste the exact output name printed above'
$pairingInfoPath = Join-Path $env:LOCALAPPDATA `
    ('DrawDock-invitation-' + [guid]::NewGuid().ToString('N') + '.json')
$pairingInfoPath
& .\target\debug\dd-host.exe serve --output $output `
    --pairing-info $pairingInfoPath --video-encoder software
```

Keep that host running. In a second console, set `$adb`/`$serial` as above and
use the exact private path printed by the host console:

```powershell
$pairingInfoPath = '<private-path-printed-by-host-console>'
$pair = Get-Content -LiteralPath $pairingInfoPath -Raw | ConvertFrom-Json
try {
    & $adb -s $serial shell am start -n 'com.drawdock/.MainActivity' `
        --es 'com.drawdock.extra.HOST' ([string]$pair.host) `
        --ei 'com.drawdock.extra.PORT' ([int]$pair.port) `
        --es 'com.drawdock.extra.FINGERPRINT' ([string]$pair.fingerprint) `
        --es 'com.drawdock.extra.TOKEN' ([string]$pair.token)
} finally {
    Remove-Item -LiteralPath $pairingInfoPath
}
```

This still requires the pin and native PC approval; it is not an authentication
bypass. The invitation is plaintext and enters the local ADB process arguments:
use a private file, avoid logs/recorded command lines and remove it after
delivery. Do not publish its contents.

## Troubleshooting

| Symptom | Check |
|---|---|
| ADB device missing/unauthorized | USB debugging, cable, selected serial and device-side authorization |
| Connect fails on loopback | Host process, matching port and tablet reverse mapping |
| LAN connect times out | Correct PC private IP/profile, same LAN, client isolation and scoped firewall rule |
| Connect still targets USB after switching to Wi-Fi | Pair another PC with the new LAN invitation |
| Host requests new pairing | Invitation/revocation state and saved endpoint; reopen enrollment deliberately |
| Pin mismatch or protected-store error | Correct PC identity/permissions; never disable pinning or silently reset trust |
| Video unavailable/input suspended | FFmpeg/encoder output, decoder capabilities and source geometry; reconnect |
| App disconnects while copying an image | Backgrounding pauses the connection; return to resume authenticated recovery of completed objects |
| Clipboard paste rejects content | Copy an image URI, not a link/text; check provider access and image limits |
| Menu hold ignored | Use stylus contact inside the image, avoid moving beyond slop and unlock the device |

Rebuild both applications when changing the protocol, geometry or negotiated
video format. Unit tests/builds do not exercise actual device input/decoder
behavior; use the [device testing guidance](development.md#device-and-visual-checks).
