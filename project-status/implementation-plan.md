# Windows-first implementation status

Historical milestone record. The implementation is described in the
[architecture guide](../docs/architecture.md).

The Windows PoC profile controls scope and order. The host-visible annotation
requirement remains mandatory; compiling the native presenter is not proof of
fullscreen or physical-display behavior.

| Milestone | Work | Evidence/status |
|---|---|---|
| W0 | Inspect Windows, Rust/MSVC, Android SDK/ADB, displays and device input capabilities; create runnable workspace/docs | **PASS** for discovery: environment is recorded in `environment.md`; Rust and Android test/build commands run successfully. |
| W1 | Native click-through layered presenter, shared scene/history, deterministic fixture replay | **PARTIAL**: 12 Rust unit tests pass; fixture replay ran on `\\.\DISPLAY2` and host reported its overlay visible. Physical visual confirmation, pass-through, focus and fullscreen checks remain blocked/not run. |
| W2 | Kotlin stylus client, bounded reducer, authenticated TLS/TCP and USB/ADB | **PARTIAL**: QR invitation/scanner and native PC approval added; 16 Rust tests and 7 Android tests pass, Android build/lint pass. APK installed and USB reverse configured; physical scan, approved pairing and pen-to-host annotation remain unverified. |
| W3 | Windows mouse injection and selected-monitor capture/video | **PARTIAL/BLOCKED**: bounded native FFmpeg H.264 capture and separate pinned-TLS video implemented; real two-monitor TLS switching test passes. Android has MediaCodec, a top source dropdown, overlay tools and fitted ink geometry. Physical tablet playback/alignment and revision-watermark reconciliation remain unverified/blocked. |
| W4 | Explicit private-interface TLS/TCP and real Wi-Fi acceptance | **BLOCKED**: address validation allows only loopback/private ranges and no firewall rule was changed, but no Wi-Fi session was started or validated. |

Linux/X11/Wayland/GNOME and QUIC are **DEFERRED**. This does not reduce the
Windows acceptance requirements.

## W1 safety boundary

The presenter is an ordinary-user topmost layered window using per-pixel alpha,
no-activate and click-through extended styles. It does not elevate, capture a
screen, bind a socket or alter the firewall. The fixture replay on
`\\.\DISPLAY2` exited and removed its window; physical display and live-app
behavior remain unverified.

## W2 security boundary

The host retains a current-user DPAPI-protected TLS identity and creates a single-use 128-bit invitation,
then displays the endpoint, certificate fingerprint and token in a native QR
window. Android scans and pins that fingerprint before transmitting the token.
The PC requires explicit Yes/No approval; no terminal typing is needed. The
five-minute invitation expiry and attempt bound remain in place. At the owner's
request, first approval now enrolls a persistent per-device reconnect credential.
The host stores its digest with the protected identity; Android saves the
credential/pin using Keystore AES-GCM. **Connect** resumes without another QR
or approval, across process restarts. Stop the host to list/revoke devices;
use `--pair` / launcher `-Pair` to explicitly enroll additional tablets.
The USB test uses `\\.\DISPLAY2`, a loopback-only listener at
`127.0.0.1:47831` and an explicit ADB reverse mapping. Physical scanning,
approved pairing and pen input are not yet confirmed. Remove the mapping and
stop the host after testing. No Wi-Fi listener or firewall change was made.

## Exit criteria

The Windows-only verdict is PASS only after every mandatory Windows gate has
physical evidence. Until a gate is performed, record it as PARTIAL or BLOCKED;
never infer desktop, tablet, fullscreen, mirroring or latency success from a
build.
