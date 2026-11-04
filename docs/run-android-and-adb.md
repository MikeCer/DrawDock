# Android client and ADB

The connected target discovered for this PoC is Xiaomi `25099RP13G`, API 36,
ADB serial `cfa3e5dce217`. Its Android input inventory exposes an
`NVTCapacitivePen` stylus source, pressure and tilt ranges. Recheck before each
run:

```powershell
$adb = 'D:\Tools\android-dev\platform-tools\adb.exe'
$serial = 'cfa3e5dce217'
& $adb devices -l
```

Build and run the Android unit tests with the cached Gradle distribution and the
installed Temurin 17 JDK. No global PATH or system setting is changed:

```powershell
$env:JAVA_HOME = 'C:\Program Files\Eclipse Adoptium\jdk-17.0.18.8-hotspot'
$env:ANDROID_HOME = 'D:\Tools\android-dev'
$gradle = Get-ChildItem "$env:USERPROFILE\.gradle\wrapper\dists\gradle-9.4.1-bin" -Filter gradle.bat -Recurse -File | Select-Object -First 1 -ExpandProperty FullName
& $gradle -p E:\dev\DrawDock\android testDebugUnitTest assembleDebug
if ($LASTEXITCODE -ne 0) { throw 'Android build or unit tests failed' }
```

Install only the resulting DrawDock debug APK to the explicitly selected tablet:

```powershell
& $adb -s $serial install -r 'E:\dev\DrawDock\android\app\build\outputs\apk\debug\app-debug.apk'
```

If Android cancels installation or requests device-side confirmation, do not
bypass that control. The app's tablet preview is not host annotation.

For USB-only host connection, map the single implemented TLS/TCP port to the
host's loopback listener. ADB is transport only; the app still verifies the
scanned TLS certificate fingerprint and single-use pairing token, and the host
requires explicit approval in a native Windows dialog on first pairing:

```powershell
& $adb -s $serial reverse tcp:47831 tcp:47831
& $adb -s $serial reverse --list
```

Start `dd-host serve` on the selected output with its default `127.0.0.1`
listener:

```powershell
cargo build -p dd-host
& 'E:\dev\DrawDock\target\debug\dd-host.exe' serve --output '\\.\DISPLAY2'
```

In the tablet app choose **Connect > Scan QR**, allow camera permission if
prompted, and point the camera at the native DrawDock QR window on the PC.
Confirm the endpoint on the tablet, then choose **Yes** in the PC's
**DrawDock - approve tablet** dialog for your device. No fingerprint or token
typing is needed. The QR expires after five minutes and closes after success;
closing it cancels new enrollment. After this update, pair once to create the
remembered credential; thereafter tap **Connect** without scanning or another
PC approval, even after restarting either app. The host stays available after
Disconnect. For another tablet or a lost/revoked credential, restart using
`Start-DrawDockHost.ps1 -Restart -Pair` and scan a fresh invitation.
A loopback QR works through the ADB reverse
mapping above, not over Wi-Fi. Private-IP Wi-Fi still requires separate acceptance.

**Pair another PC** opens enrollment without reusing the saved credential.
**Forget PC** removes the tablet's saved endpoint/pin and Keystore-protected
credential. It does not revoke a copied credential on the host: use the host's
`devices revoke` command for that. This initial client remembers its last PC
endpoint; changing transport/IP currently requires deliberate re-pairing.

For optional ADB-assisted launch (without using the camera), pass
`--pairing-info <new private local path>` to `serve`. The host
creates that file only if it does not already exist. Read it into a PowerShell
object and launch the app with its one-time extras:

```powershell
$pairingInfoPath = Join-Path $env:LOCALAPPDATA ('DrawDock-pairing-' + [guid]::NewGuid().ToString('N') + '.json')
# Include --pairing-info $pairingInfoPath in the dd-host serve arguments.
$pair = Get-Content -LiteralPath $pairingInfoPath -Raw | ConvertFrom-Json
& $adb -s $serial shell am start -n 'com.drawdock/.MainActivity' `
    --es 'com.drawdock.extra.HOST' ([string]$pair.host) `
    --ei 'com.drawdock.extra.PORT' ([int]$pair.port) `
    --es 'com.drawdock.extra.FINGERPRINT' ([string]$pair.fingerprint) `
    --es 'com.drawdock.extra.TOKEN' ([string]$pair.token)
Remove-Item -LiteralPath $pairingInfoPath
```

The intent delivers the same fingerprint-pinned, single-use token; it does not
bypass TLS or host approval. Approve only your tablet in the native PC dialog.
Confirm tablet Wi-Fi is disabled during USB acceptance. Remove
only the mapping created for DrawDock after the test:

```powershell
& $adb -s $serial reverse --remove tcp:47831
```

The authenticated annotation/control channel and initial monitor video pipeline
are implemented. Tablet playback, Wi-Fi acceptance, and tablet-to-host pen
behavior still require physical validation; a successful APK build is not
evidence of those behaviors.

The single **Clear** control clears tablet and PC annotations in a paired
session. It waits for the host acknowledgement before clearing the tablet;
timeouts are reported explicitly. Disconnected clearing affects only the tablet
and reports that no PC is connected. Both the APK and host must be rebuilt and
restarted for the clear/redraw and shape-endpoint fixes; restarting ends the
current session. Manually verify freehand, arrow, rectangle and ellipse on the
physical PC display, then Clear and verify both surfaces are blank.

## Live monitor workspace

Rebuild both host and APK for protocol version 2, install the APK, and run
`E:\dev\DrawDock\Start-DrawDockHost.ps1 -Restart`. Pair and approve viewing,
annotation and pointer access. The monitor dropdown at the top lists the PC
outputs. **Tools** reopens the floating controls; **Hide tools** closes them.
Controls overlay the image rather than reserving drawing space. The video fills
the largest available aspect-fit rectangle; letterbox margins are not ink
targets. Changing the monitor cancels active pen contact and waits for video
geometry acknowledgement before allowing new input.

The installed FFmpeg 7.0 native Windows executable supplies gdigrab, libx264
and NVIDIA NVENC. It is not downloaded or bundled. `DRAWDOCK_FFMPEG` can specify a direct
executable path on another setup. The stream now uses a 2560x1440 bounding box
at 30 fps, H.264 High/low-latency encoding and Android MediaCodec decoding.
Update both applications: old APKs reject the larger format. The app selects
and checks a decoder that supports the new geometry/rate before enabling input.
It uses a second pinned-TLS connection on port 47831;
no additional ADB port or firewall change is needed.

Test every source, its corners/center and pen alignment on the physical PC
display. The live video includes host ink; tablet provisional ink currently
hands off on pen-up and can momentarily disappear before the captured ink
arrives. Reconciliation latency/watermarks and fullscreen acceptance are not
passed. If video fails or its bounded queue overflows, drawing is suspended
and the app reports that reconnection is required.
